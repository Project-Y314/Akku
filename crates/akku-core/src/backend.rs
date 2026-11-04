//! Backend plugin trait (`DeviceBackend`), capability-enforcing decorator
//! (`CapabilityGatedBackend`), single-retry-on-busy policy, and `NotImplementedBackend`.

use std::thread;
use std::time::Duration;

use crate::capabilities::Capabilities;
use crate::error::{DeckError, Result};
use crate::model::{Axis, DeviceDescriptor, DeviceId, DeviceState, SettingKey, SettingValue};

/// Non-negotiable backend plugin trait implemented by every hardware or SDK driver.
///
/// Backends must open HID handles on demand, close them immediately on completion,
/// and never hold a handle across idle intervals that could block vendor software.
pub trait DeviceBackend: Send + Sync {
    /// Unique static identifier (e.g. `"logitech-hidpp"`, `"generic-hid-battery"`).
    fn id(&self) -> &'static str;

    /// Discovers all currently attached or paired peripherals managed by this backend.
    fn enumerate(&self) -> Result<Vec<DeviceDescriptor>>;

    /// Returns the bitflags of capabilities supported by `dev`.
    fn capabilities(&self, dev: &DeviceId) -> Capabilities;

    /// Reads the current telemetry and settings state for `dev`.
    fn read_state(&self, dev: &DeviceId) -> Result<DeviceState>;

    /// Writes a new CPI / DPI value to `dev` on the specified `axis`.
    fn write_dpi(&self, dev: &DeviceId, dpi: u16, axis: Axis) -> Result<()>;

    /// Writes a non-DPI setting (`PollingRateHz`, `Lighting`, `LiftoffDistanceMm`, etc.) to `dev`.
    fn write_setting(&self, dev: &DeviceId, key: SettingKey, val: SettingValue) -> Result<()>;
}

/// Middleware wrapper around any `DeviceBackend` that enforces:
/// 1. Strict capability gating on `read_state`, `write_dpi`, and `write_setting`.
/// 2. Key/value variant consistency checks before touching hardware.
/// 3. Single automatic retry on `DeckError::DeviceBusy` (Section D behavioral requirement).
pub struct CapabilityGatedBackend<B: DeviceBackend> {
    inner: B,
    busy_retry_delay: Duration,
}

impl<B: DeviceBackend> CapabilityGatedBackend<B> {
    pub fn new(inner: B) -> Self {
        Self {
            inner,
            busy_retry_delay: Duration::from_millis(25),
        }
    }

    pub fn with_busy_retry_delay(mut self, delay: Duration) -> Self {
        self.busy_retry_delay = delay;
        self
    }

    pub fn inner(&self) -> &B {
        &self.inner
    }

    /// Executes `op` and retries exactly once if it fails with `DeckError::DeviceBusy`.
    fn retry_once_on_busy<T, F>(&self, dev: &DeviceId, mut op: F) -> Result<T>
    where
        F: FnMut() -> Result<T>,
    {
        match op() {
            Ok(val) => Ok(val),
            Err(DeckError::DeviceBusy { .. }) => {
                if !self.busy_retry_delay.is_zero() {
                    thread::sleep(self.busy_retry_delay);
                }
                match op() {
                    Ok(val) => Ok(val),
                    Err(DeckError::DeviceBusy { .. }) => Err(DeckError::DeviceBusy {
                        device_id: dev.canonical_key(),
                        retries_attempted: 1,
                    }),
                    Err(other) => Err(other),
                }
            }
            Err(other) => Err(other),
        }
    }
}

impl<B: DeviceBackend> DeviceBackend for CapabilityGatedBackend<B> {
    fn id(&self) -> &'static str {
        self.inner.id()
    }

    fn enumerate(&self) -> Result<Vec<DeviceDescriptor>> {
        self.inner.enumerate()
    }

    fn capabilities(&self, dev: &DeviceId) -> Capabilities {
        self.inner.capabilities(dev)
    }

    fn read_state(&self, dev: &DeviceId) -> Result<DeviceState> {
        let caps = self.inner.capabilities(dev);
        let raw_state = self.retry_once_on_busy(dev, || self.inner.read_state(dev))?;
        Ok(raw_state.mask_to_capabilities(caps))
    }

    fn write_dpi(&self, dev: &DeviceId, dpi: u16, axis: Axis) -> Result<()> {
        let caps = self.inner.capabilities(dev);
        if !caps.contains(Capabilities::DPI) {
            return Err(DeckError::Unsupported {
                capability: "DPI".to_string(),
                backend_id: self.inner.id().to_string(),
            });
        }
        if dpi == 0 {
            return Err(DeckError::InvalidArgument {
                field: "dpi".to_string(),
                reason: "DPI must be greater than zero".to_string(),
            });
        }
        self.retry_once_on_busy(dev, || self.inner.write_dpi(dev, dpi, axis))
    }

    fn write_setting(&self, dev: &DeviceId, key: SettingKey, val: SettingValue) -> Result<()> {
        let caps = self.inner.capabilities(dev);
        let required = key.required_capability();
        if !caps.contains(required) {
            return Err(DeckError::Unsupported {
                capability: key.capability_name().as_str().to_string(),
                backend_id: self.inner.id().to_string(),
            });
        }
        if !val.matches_key(key) {
            return Err(DeckError::InvalidArgument {
                field: format!("{key:?}"),
                reason: "SettingValue variant does not match SettingKey".to_string(),
            });
        }
        self.retry_once_on_busy(dev, || self.inner.write_setting(dev, key, val.clone()))
    }
}

/// Explicit fallback backend for unverified or platform-gated vendor protocols.
///
/// Section F requirement: never invent vendor protocol bytes; emit `NotImplemented`
/// linking the authoritative upstream protocol reference.
pub struct NotImplementedBackend {
    backend_id: &'static str,
    reference_url: &'static str,
}

impl NotImplementedBackend {
    pub const fn new(backend_id: &'static str, reference_url: &'static str) -> Self {
        Self {
            backend_id,
            reference_url,
        }
    }
}

impl DeviceBackend for NotImplementedBackend {
    fn id(&self) -> &'static str {
        self.backend_id
    }

    fn enumerate(&self) -> Result<Vec<DeviceDescriptor>> {
        Ok(Vec::new())
    }

    fn capabilities(&self, _dev: &DeviceId) -> Capabilities {
        Capabilities::empty()
    }

    fn read_state(&self, _dev: &DeviceId) -> Result<DeviceState> {
        Err(DeckError::NotImplemented {
            backend_id: self.backend_id.to_string(),
            feature: "read_state".to_string(),
            reference_url: self.reference_url.to_string(),
        })
    }

    fn write_dpi(&self, _dev: &DeviceId, _dpi: u16, _axis: Axis) -> Result<()> {
        Err(DeckError::NotImplemented {
            backend_id: self.backend_id.to_string(),
            feature: "write_dpi".to_string(),
            reference_url: self.reference_url.to_string(),
        })
    }

    fn write_setting(&self, _dev: &DeviceId, key: SettingKey, _val: SettingValue) -> Result<()> {
        Err(DeckError::NotImplemented {
            backend_id: self.backend_id.to_string(),
            feature: format!("write_setting({key:?})"),
            reference_url: self.reference_url.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU8, Ordering};

    use super::*;
    use crate::model::{BatteryState, DpiState, FieldStatus, LightingEffect, LightingState};

    struct DummyBackend {
        caps: Capabilities,
        busy_failures_remaining: AtomicU8,
    }

    impl DeviceBackend for DummyBackend {
        fn id(&self) -> &'static str {
            "dummy-test"
        }

        fn enumerate(&self) -> Result<Vec<DeviceDescriptor>> {
            Ok(Vec::new())
        }

        fn capabilities(&self, _dev: &DeviceId) -> Capabilities {
            self.caps
        }

        fn read_state(&self, dev: &DeviceId) -> Result<DeviceState> {
            let mut state = DeviceState::empty_for(dev.clone(), 1_700_000_000_000);
            state.battery = FieldStatus::available(BatteryState {
                percent: 82,
                charging: false,
                voltage_mv: Some(3950),
            });
            // Intentionally populate lighting even when capability bit may be off
            state.lighting = FieldStatus::available(LightingState {
                effect: LightingEffect::Off,
                zone_count: 1,
            });
            state.dpi = FieldStatus::available(DpiState::uniform(1600));
            Ok(state)
        }

        fn write_dpi(&self, dev: &DeviceId, _dpi: u16, _axis: Axis) -> Result<()> {
            let rem = self.busy_failures_remaining.load(Ordering::SeqCst);
            if rem > 0 {
                self.busy_failures_remaining
                    .store(rem - 1, Ordering::SeqCst);
                return Err(DeckError::DeviceBusy {
                    device_id: dev.canonical_key(),
                    retries_attempted: 0,
                });
            }
            Ok(())
        }

        fn write_setting(
            &self,
            _dev: &DeviceId,
            _key: SettingKey,
            _val: SettingValue,
        ) -> Result<()> {
            Ok(())
        }
    }

    #[test]
    fn masks_unadvertised_capabilities_on_read_and_blocks_writes() {
        let dev = DeviceId::new("dummy-test", 0x046D, 0xC548, 0xFF00, 0x0001).with_receiver_slot(1);
        let backend = CapabilityGatedBackend::new(DummyBackend {
            caps: Capabilities::BATTERY,
            busy_failures_remaining: AtomicU8::new(0),
        })
        .with_busy_retry_delay(Duration::ZERO);

        let state = backend.read_state(&dev).expect("read state");
        assert!(matches!(state.battery, FieldStatus::Available { .. }));
        assert_eq!(state.dpi, FieldStatus::Unsupported);
        assert_eq!(state.lighting, FieldStatus::Unsupported);

        let err = backend
            .write_dpi(&dev, 1600, Axis::Uniform)
            .expect_err("DPI write should be blocked when capability bit is absent");
        assert!(matches!(err, DeckError::Unsupported { .. }));
    }

    #[test]
    fn retries_exactly_once_on_device_busy() {
        let dev = DeviceId::new("dummy-test", 0x046D, 0xC548, 0xFF00, 0x0001);

        // 1 transient DeviceBusy error -> succeeds on retry #1
        let ok_backend = CapabilityGatedBackend::new(DummyBackend {
            caps: Capabilities::DPI,
            busy_failures_remaining: AtomicU8::new(1),
        })
        .with_busy_retry_delay(Duration::ZERO);
        assert!(ok_backend.write_dpi(&dev, 800, Axis::Uniform).is_ok());

        // 2 consecutive DeviceBusy errors -> fails after 1 retry
        let fail_backend = CapabilityGatedBackend::new(DummyBackend {
            caps: Capabilities::DPI,
            busy_failures_remaining: AtomicU8::new(2),
        })
        .with_busy_retry_delay(Duration::ZERO);
        let err = fail_backend
            .write_dpi(&dev, 800, Axis::Uniform)
            .expect_err("should fail after 1 retry");
        assert_eq!(
            err,
            DeckError::DeviceBusy {
                device_id: dev.canonical_key(),
                retries_attempted: 1,
            }
        );
    }
}
