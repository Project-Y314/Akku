//! Hardware backends implementing `DeviceBackend` for various protocols.
//!
//! Implements backends in the required order:
//! 1. generic-hid-battery
//! 2. logitech-hidpp
//! 3. openrgb-bridge
//! 4. ratbag-bridge (Linux only)
//! 5. vendor-sdk stubs (Windows only, feature-gated)

#![allow(
    unused_imports,
    dead_code,
    unreachable_patterns,
    unused_variables,
    clippy::unnecessary_map_or
)]

use std::collections::HashMap;
use std::sync::Arc;

use hidapi::HidApi;

use crate::decode::{
    decode_actuation, decode_battery, decode_dpi, decode_polling, encode_actuation, encode_dpi,
    encode_polling,
};
use akku_core::manifest::{BatteryDescriptor, DpiDescriptor, Manifest, PollingDescriptor};
use akku_core::model::{ConnectionKind as ModelConnectionKind, DeviceKind as ModelDeviceKind};
use akku_core::{
    ActuationSpec, Axis, BatteryReport, BatteryState, Capabilities, DeckError, DeviceBackend,
    DeviceDescriptor, DeviceId, DeviceKind, DeviceState, DpiSpec, DpiState, FieldStatus,
    LiftoffSpec, PollingSpec, ProfileId, Result, SettingKey, SettingValue,
};

/// Trait for HID transport operations. Allows mocking in tests.
pub trait HidTransport: Send + Sync {
    fn open_device(&self, vid: u16, pid: u16) -> Result<hidapi::HidDevice>;
    fn read_feature_report(
        &self,
        device: &hidapi::HidDevice,
        report_id: u8,
        buf: &mut [u8],
    ) -> Result<usize>;
    fn write_feature_report(&self, device: &hidapi::HidDevice, buf: &[u8]) -> Result<()>;
    fn write_output_report(&self, device: &hidapi::HidDevice, buf: &[u8]) -> Result<()>;
    fn read_input_report_timeout(
        &self,
        device: &hidapi::HidDevice,
        buf: &mut [u8],
        timeout_ms: i32,
    ) -> Result<usize>;
}

/// Real implementation using hidapi
pub struct RealHidTransport {
    api: HidApi,
}

impl RealHidTransport {
    pub fn new() -> Result<Self> {
        let api = HidApi::new().map_err(|e| DeckError::Transport {
            message: e.to_string(),
        })?;
        Ok(Self { api })
    }
}

impl HidTransport for RealHidTransport {
    fn open_device(&self, vid: u16, pid: u16) -> Result<hidapi::HidDevice> {
        self.api.open(vid, pid).map_err(|e| {
            DeckError::from_hid_error(format!("{:04X}:{:04X}", vid, pid), e.to_string())
        })
    }

    fn read_feature_report(
        &self,
        device: &hidapi::HidDevice,
        report_id: u8,
        buf: &mut [u8],
    ) -> Result<usize> {
        let mut report = vec![0u8; buf.len() + 1];
        report[0] = report_id;
        let len = device
            .get_feature_report(&mut report)
            .map_err(|e| DeckError::from_hid_error("read_feature", e.to_string()))?;
        if len > 0 {
            buf.copy_from_slice(&report[1..len.min(buf.len() + 1)]);
        }
        Ok(len.saturating_sub(1))
    }

    fn write_feature_report(&self, device: &hidapi::HidDevice, buf: &[u8]) -> Result<()> {
        device
            .send_feature_report(buf)
            .map_err(|e| DeckError::from_hid_error("write_feature", e.to_string()))
    }

    fn write_output_report(&self, device: &hidapi::HidDevice, buf: &[u8]) -> Result<()> {
        let _ = device
            .write(buf)
            .map_err(|e| DeckError::from_hid_error("write_output", e.to_string()))?;
        Ok(())
    }

    fn read_input_report_timeout(
        &self,
        device: &hidapi::HidDevice,
        buf: &mut [u8],
        timeout_ms: i32,
    ) -> Result<usize> {
        device
            .read_timeout(buf, timeout_ms)
            .map_err(|e| DeckError::from_hid_error("read_input", e.to_string()))
    }
}

/// Deterministic mock transport for unit tests
#[derive(Default)]
pub struct MockHidTransport {
    pub feature_reports: HashMap<(u16, u16, u8), Vec<u8>>,
    pub output_reports: HashMap<(u16, u16), Vec<Vec<u8>>>,
    pub input_reports: HashMap<(u16, u16), Vec<Vec<u8>>>,
    pub should_fail: bool,
}

impl MockHidTransport {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_feature_report(mut self, vid: u16, pid: u16, report_id: u8, data: Vec<u8>) -> Self {
        self.feature_reports.insert((vid, pid, report_id), data);
        self
    }
}

impl HidTransport for MockHidTransport {
    fn open_device(&self, _vid: u16, _pid: u16) -> Result<hidapi::HidDevice> {
        if self.should_fail {
            return Err(DeckError::Transport {
                message: "mock failure".into(),
            });
        }
        Err(DeckError::Transport {
            message: "mock transport does not create real devices".into(),
        })
    }

    fn read_feature_report(
        &self,
        _device: &hidapi::HidDevice,
        _report_id: u8,
        _buf: &mut [u8],
    ) -> Result<usize> {
        Err(DeckError::Transport {
            message: "use MockDriver for tests".into(),
        })
    }

    fn write_feature_report(&self, _device: &hidapi::HidDevice, _buf: &[u8]) -> Result<()> {
        Ok(())
    }

    fn write_output_report(&self, _device: &hidapi::HidDevice, _buf: &[u8]) -> Result<()> {
        Ok(())
    }

    fn read_input_report_timeout(
        &self,
        _device: &hidapi::HidDevice,
        _buf: &mut [u8],
        _timeout_ms: i32,
    ) -> Result<usize> {
        Err(DeckError::Transport {
            message: "use MockDriver for tests".into(),
        })
    }
}

/// Generic HID battery backend - matches manifests from devices/*.toml
pub struct GenericHidBatteryBackend {
    manifests: Vec<Manifest>,
    transport: Arc<dyn HidTransport>,
}

impl GenericHidBatteryBackend {
    pub fn new(manifests: Vec<Manifest>) -> Result<Self> {
        let transport = Arc::new(RealHidTransport::new()?);
        Ok(Self {
            manifests,
            transport,
        })
    }

    pub fn with_transport(manifests: Vec<Manifest>, transport: Arc<dyn HidTransport>) -> Self {
        Self {
            manifests,
            transport,
        }
    }

    fn read_battery_report(
        &self,
        manifest: &Manifest,
        device: &hidapi::HidDevice,
    ) -> Result<BatteryState> {
        let Some(battery) = &manifest.battery else {
            return Err(DeckError::Unsupported {
                capability: "battery".into(),
                backend_id: self.id().into(),
            });
        };
        let mut buf = vec![0u8; battery.report_len];
        let len = self
            .transport
            .read_feature_report(device, battery.report_id, &mut buf)?;
        if len < battery.offset + battery.width {
            return Err(DeckError::Protocol {
                backend_id: self.id().into(),
                message: "battery report too short".into(),
                reference_url: None,
            });
        }
        let percent = decode_battery(&buf, battery)?;
        let charging = battery.charging_offset.map_or(false, |offset| {
            if offset < len {
                buf[offset] & 0x01 != 0
            } else {
                false
            }
        });
        let percent_u8 = percent.percent.unwrap_or(0);
        Ok(BatteryState {
            percent: percent_u8,
            charging,
            voltage_mv: None,
        })
    }

    fn read_dpi(
        &self,
        manifest: &Manifest,
        device: &hidapi::HidDevice,
    ) -> Result<Option<DpiState>> {
        let Some(dpi) = &manifest.dpi else {
            return Ok(None);
        };
        let mut buf = vec![0u8; dpi.report_len];
        let len = self
            .transport
            .read_feature_report(device, dpi.report_id, &mut buf)?;
        if len < dpi.offset + 2 {
            return Err(DeckError::Protocol {
                backend_id: self.id().into(),
                message: "dpi report too short".into(),
                reference_url: None,
            });
        }
        let dpi_value = decode_dpi(&buf, dpi)?;
        Ok(Some(DpiState::uniform(dpi_value)))
    }

    fn read_polling(&self, manifest: &Manifest, device: &hidapi::HidDevice) -> Result<Option<u16>> {
        let Some(polling) = &manifest.polling else {
            return Ok(None);
        };
        let mut buf = vec![0u8; polling.report_len];
        let len = self
            .transport
            .read_feature_report(device, polling.report_id, &mut buf)?;
        if len < polling.offset + 2 {
            return Err(DeckError::Protocol {
                backend_id: self.id().into(),
                message: "polling report too short".into(),
                reference_url: None,
            });
        }
        let rate = decode_polling(&buf, polling)?;
        Ok(Some(rate))
    }

    fn read_actuation(
        &self,
        manifest: &Manifest,
        device: &hidapi::HidDevice,
    ) -> Result<Option<f32>> {
        let Some(actuation) = &manifest.actuation else {
            return Ok(None);
        };
        let mut buf = vec![0u8; actuation.report_len];
        let len = self
            .transport
            .read_feature_report(device, actuation.report_id, &mut buf)?;
        if len < actuation.offset + 2 {
            return Err(DeckError::Protocol {
                backend_id: self.id().into(),
                message: "actuation report too short".into(),
                reference_url: None,
            });
        }
        let value = decode_actuation(&buf, actuation)?;
        Ok(Some(value))
    }
}

impl akku_core::DeviceBackend for GenericHidBatteryBackend {
    fn id(&self) -> &'static str {
        "generic-hid-battery"
    }

    fn enumerate(&self) -> Result<Vec<DeviceDescriptor>> {
        let mut descriptors = Vec::new();
        for manifest in &self.manifests {
            let device = self
                .transport
                .open_device(manifest.matches[0].vid, manifest.matches[0].pid)?;
            let device_id = DeviceId::new(
                self.id(),
                manifest.matches[0].vid,
                manifest.matches[0].pid,
                manifest.matches[0].usage_page,
                manifest.matches[0].usage,
            );
            let mut caps = Capabilities::BATTERY;
            if manifest.dpi.is_some() {
                caps.insert(Capabilities::DPI);
            }
            if manifest.polling.is_some() {
                caps.insert(Capabilities::POLLING_RATE);
            }
            if manifest.actuation.is_some() {
                caps.insert(Capabilities::ACTUATION);
            }

            descriptors.push(DeviceDescriptor {
                id: device_id,
                name: manifest.name.clone(),
                vendor_name: manifest.vendor.clone(),
                kind: match manifest.kind {
                    akku_core::manifest::DeviceKind::Mouse => ModelDeviceKind::Mouse,
                    akku_core::manifest::DeviceKind::Keyboard => ModelDeviceKind::Keyboard,
                    _ => ModelDeviceKind::Other,
                },
                connection: match manifest.connection {
                    akku_core::manifest::ConnectionKind::UsbReceiver => {
                        ModelConnectionKind::UsbReceiver
                    }
                    akku_core::manifest::ConnectionKind::Wired => ModelConnectionKind::Wired,
                    akku_core::manifest::ConnectionKind::BluetoothLe => {
                        ModelConnectionKind::BluetoothLe
                    }
                },
                capabilities: caps,
                dpi_spec: manifest.dpi.as_ref().map(|d| DpiSpec {
                    min: d.values.first().copied().unwrap_or(400),
                    max: d.values.last().copied().unwrap_or(3200),
                    step: 0,
                    presets: d.values.clone(),
                    independent_xy: false,
                }),
                polling_spec: manifest.polling.as_ref().map(|p| PollingSpec {
                    rates_hz: p.rates.clone(),
                }),
                liftoff_spec: None,
                debounce_spec: None,
                actuation_spec: manifest.actuation.as_ref().map(|a| ActuationSpec {
                    min_mm: a.min_mm,
                    max_mm: a.max_mm,
                    step_mm: a.step_mm,
                }),
            });
        }
        Ok(descriptors)
    }

    fn capabilities(&self, dev: &DeviceId) -> Capabilities {
        let manifest = self.manifests.iter().find(|m| {
            m.matches.iter().any(|mm| {
                mm.vid == dev.vendor_id
                    && mm.pid == dev.product_id
                    && mm.usage_page == dev.usage_page
                    && mm.usage == dev.usage
            })
        });
        let mut caps = Capabilities::BATTERY;
        if let Some(m) = manifest {
            if m.dpi.is_some() {
                caps.insert(Capabilities::DPI);
            }
            if m.polling.is_some() {
                caps.insert(Capabilities::POLLING_RATE);
            }
            if m.actuation.is_some() {
                caps.insert(Capabilities::ACTUATION);
            }
        }
        caps
    }

    fn read_state(&self, dev: &DeviceId) -> Result<DeviceState> {
        let manifest = self
            .manifests
            .iter()
            .find(|m| {
                m.matches.iter().any(|mm| {
                    mm.vid == dev.vendor_id
                        && mm.pid == dev.product_id
                        && mm.usage_page == dev.usage_page
                        && mm.usage == dev.usage
                })
            })
            .ok_or(DeckError::Disconnected {
                device_id: dev.canonical_key(),
            })?;

        let device = self
            .transport
            .open_device(manifest.matches[0].vid, manifest.matches[0].pid)?;
        let battery = self.read_battery_report(manifest, &device)?;
        let dpi = self.read_dpi(manifest, &device)?;
        let polling = self.read_polling(manifest, &device)?;
        let actuation = self.read_actuation(manifest, &device)?;

        let mut state = DeviceState::empty_for(dev.clone(), unix_ms());
        state.battery = FieldStatus::available(battery);
        if let Some(dpi) = dpi {
            state.dpi = FieldStatus::available(dpi);
        }
        if let Some(polling) = polling {
            state.polling_rate_hz = FieldStatus::available(polling);
        }
        if let Some(actuation) = actuation {
            state.actuation_mm = FieldStatus::available(actuation);
        }
        Ok(state)
    }

    fn write_dpi(&self, dev: &DeviceId, dpi: u16, _axis: Axis) -> Result<()> {
        let manifest = self
            .manifests
            .iter()
            .find(|m| {
                m.matches.iter().any(|mm| {
                    mm.vid == dev.vendor_id
                        && mm.pid == dev.product_id
                        && mm.usage_page == dev.usage_page
                        && mm.usage == dev.usage
                })
            })
            .ok_or(DeckError::Disconnected {
                device_id: dev.canonical_key(),
            })?;

        let Some(dpi_desc) = &manifest.dpi else {
            return Err(DeckError::Unsupported {
                capability: "DPI".into(),
                backend_id: self.id().into(),
            });
        };
        if !dpi_desc.values.contains(&dpi) {
            return Err(DeckError::InvalidArgument {
                field: "dpi".into(),
                reason: "value not in presets".into(),
            });
        }

        let device = self
            .transport
            .open_device(manifest.matches[0].vid, manifest.matches[0].pid)?;
        let mut buf = vec![0u8; dpi_desc.report_len];
        encode_dpi(&mut buf, dpi_desc, dpi)?;
        self.transport.write_feature_report(&device, &buf)
    }

    fn write_setting(&self, dev: &DeviceId, key: SettingKey, val: SettingValue) -> Result<()> {
        let manifest = self
            .manifests
            .iter()
            .find(|m| {
                m.matches.iter().any(|mm| {
                    mm.vid == dev.vendor_id
                        && mm.pid == dev.product_id
                        && mm.usage_page == dev.usage_page
                        && mm.usage == dev.usage
                })
            })
            .ok_or(DeckError::Disconnected {
                device_id: dev.canonical_key(),
            })?;

        let device = self
            .transport
            .open_device(manifest.matches[0].vid, manifest.matches[0].pid)?;

        match (key, val) {
            (SettingKey::PollingRateHz, SettingValue::PollingRateHz(hz)) => {
                let Some(polling) = &manifest.polling else {
                    return Err(DeckError::Unsupported {
                        capability: "POLLING_RATE".into(),
                        backend_id: self.id().into(),
                    });
                };
                if !polling.rates.contains(&hz) {
                    return Err(DeckError::InvalidArgument {
                        field: "polling".into(),
                        reason: "rate not supported".into(),
                    });
                }
                let mut buf = vec![0u8; polling.report_len];
                encode_polling(&mut buf, polling, hz)?;
                self.transport.write_feature_report(&device, &buf)
            }
            (SettingKey::ActuationPointMm, SettingValue::ActuationPointMm(mm)) => {
                let Some(actuation) = &manifest.actuation else {
                    return Err(DeckError::Unsupported {
                        capability: "ACTUATION".into(),
                        backend_id: self.id().into(),
                    });
                };
                if mm < actuation.min_mm
                    || mm > actuation.max_mm
                    || ((mm - actuation.min_mm) % actuation.step_mm != 0.0)
                {
                    return Err(DeckError::InvalidArgument {
                        field: "actuation".into(),
                        reason: "value out of range".into(),
                    });
                }
                let mut buf = vec![0u8; actuation.report_len];
                encode_actuation(&mut buf, actuation, mm)?;
                self.transport.write_feature_report(&device, &buf)
            }
            _ => Err(DeckError::InvalidArgument {
                field: format!("{:?}", key),
                reason: "not implemented for generic-hid".into(),
            }),
        }
    }
}

/// Logitech HID++ 2.0 backend
pub struct LogitechHidppBackend {
    transport: Arc<dyn HidTransport>,
}

impl LogitechHidppBackend {
    pub fn new() -> Result<Self> {
        let transport = Arc::new(RealHidTransport::new()?);
        Ok(Self { transport })
    }
}

impl akku_core::DeviceBackend for LogitechHidppBackend {
    fn id(&self) -> &'static str {
        "logitech-hidpp"
    }

    fn enumerate(&self) -> Result<Vec<DeviceDescriptor>> {
        Ok(Vec::new())
    }

    fn capabilities(&self, _dev: &DeviceId) -> Capabilities {
        Capabilities::BATTERY | Capabilities::DPI | Capabilities::POLLING_RATE
    }

    fn read_state(&self, _dev: &DeviceId) -> Result<DeviceState> {
        Err(DeckError::NotImplemented {
            backend_id: self.id().into(),
            feature: "read_state".into(),
            reference_url: "https://github.com/Logitech/hidpp-specification".into(),
        })
    }

    fn write_dpi(&self, _dev: &DeviceId, _dpi: u16, _axis: Axis) -> Result<()> {
        Err(DeckError::NotImplemented {
            backend_id: self.id().into(),
            feature: "write_dpi".into(),
            reference_url: "https://github.com/Logitech/hidpp-specification".into(),
        })
    }

    fn write_setting(&self, _dev: &DeviceId, _key: SettingKey, _val: SettingValue) -> Result<()> {
        Err(DeckError::NotImplemented {
            backend_id: self.id().into(),
            feature: "write_setting".into(),
            reference_url: "https://github.com/Logitech/hidpp-specification".into(),
        })
    }
}

/// OpenRGB bridge backend
pub struct OpenRgbBridgeBackend {
    transport: Arc<dyn HidTransport>,
}

impl OpenRgbBridgeBackend {
    pub fn new() -> Result<Self> {
        let transport = Arc::new(RealHidTransport::new()?);
        Ok(Self { transport })
    }
}

impl akku_core::DeviceBackend for OpenRgbBridgeBackend {
    fn id(&self) -> &'static str {
        "openrgb-bridge"
    }

    fn enumerate(&self) -> Result<Vec<DeviceDescriptor>> {
        Ok(Vec::new())
    }

    fn capabilities(&self, _dev: &DeviceId) -> Capabilities {
        Capabilities::LIGHTING
    }

    fn read_state(&self, _dev: &DeviceId) -> Result<DeviceState> {
        Err(DeckError::RequiresVendorService {
            service: "OpenRGB SDK Server (port 6742)".into(),
            reference_url: "https://gitlab.com/CalcProgrammer1/OpenRGB".into(),
        })
    }

    fn write_dpi(&self, _dev: &DeviceId, _dpi: u16, _axis: Axis) -> Result<()> {
        Err(DeckError::Unsupported {
            capability: "DPI".into(),
            backend_id: self.id().into(),
        })
    }

    fn write_setting(&self, _dev: &DeviceId, _key: SettingKey, _val: SettingValue) -> Result<()> {
        Err(DeckError::RequiresVendorService {
            service: "OpenRGB SDK Server (port 6742)".into(),
            reference_url: "https://gitlab.com/CalcProgrammer1/OpenRGB".into(),
        })
    }
}

/// ratbagd D-Bus bridge (Linux only)
#[cfg(target_os = "linux")]
pub struct RatbagBridgeBackend {
    transport: Arc<dyn HidTransport>,
}

#[cfg(target_os = "linux")]
impl RatbagBridgeBackend {
    pub fn new() -> Result<Self> {
        let transport = Arc::new(RealHidTransport::new()?);
        Ok(Self { transport })
    }
}

#[cfg(target_os = "linux")]
impl akku_core::DeviceBackend for RatbagBridgeBackend {
    fn id(&self) -> &'static str {
        "ratbag-bridge"
    }

    fn enumerate(&self) -> Result<Vec<DeviceDescriptor>> {
        Ok(Vec::new())
    }

    fn capabilities(&self, _dev: &DeviceId) -> Capabilities {
        Capabilities::DPI | Capabilities::POLLING_RATE
    }

    fn read_state(&self, _dev: &DeviceId) -> Result<DeviceState> {
        Err(DeckError::NotImplemented {
            backend_id: self.id().into(),
            feature: "read_state".into(),
            reference_url: "https://github.com/libratbag/libratbag".into(),
        })
    }

    fn write_dpi(&self, _dev: &DeviceId, _dpi: u16, _axis: Axis) -> Result<()> {
        Err(DeckError::NotImplemented {
            backend_id: self.id().into(),
            feature: "write_dpi".into(),
            reference_url: "https://github.com/libratbag/libratbag".into(),
        })
    }

    fn write_setting(&self, _dev: &DeviceId, _key: SettingKey, _val: SettingValue) -> Result<()> {
        Err(DeckError::NotImplemented {
            backend_id: self.id().into(),
            feature: "write_setting".into(),
            reference_url: "https://github.com/libratbag/libratbag".into(),
        })
    }
}

/// Vendor SDK stubs (Windows only, feature-gated)
#[cfg(all(target_os = "windows", feature = "vendor-sdk-windows"))]
pub struct VendorSdkBackend {
    transport: Arc<dyn HidTransport>,
}

#[cfg(all(target_os = "windows", feature = "vendor-sdk-windows"))]
impl VendorSdkBackend {
    pub fn new() -> Result<Self> {
        let transport = Arc::new(RealHidTransport::new()?);
        Ok(Self { transport })
    }
}

#[cfg(all(target_os = "windows", feature = "vendor-sdk-windows"))]
impl akku_core::DeviceBackend for VendorSdkBackend {
    fn id(&self) -> &'static str {
        "vendor-sdk"
    }

    fn enumerate(&self) -> Result<Vec<DeviceDescriptor>> {
        Ok(Vec::new())
    }

    fn capabilities(&self, _dev: &DeviceId) -> Capabilities {
        Capabilities::LIGHTING | Capabilities::MACROS
    }

    fn read_state(&self, _dev: &DeviceId) -> Result<DeviceState> {
        Err(DeckError::NotImplemented {
            backend_id: self.id().into(),
            feature: "read_state".into(),
            reference_url: "https://developer.razer.com/chroma-sdk/".into(),
        })
    }

    fn write_dpi(&self, _dev: &DeviceId, _dpi: u16, _axis: Axis) -> Result<()> {
        Err(DeckError::Unsupported {
            capability: "DPI".into(),
            backend_id: self.id().into(),
        })
    }

    fn write_setting(&self, _dev: &DeviceId, _key: SettingKey, _val: SettingValue) -> Result<()> {
        Err(DeckError::NotImplemented {
            backend_id: self.id().into(),
            feature: "write_setting".into(),
            reference_url: "https://developer.razer.com/chroma-sdk/".into(),
        })
    }
}

/// Registry of all available backends
pub struct BackendRegistry {
    backends: Vec<Box<dyn akku_core::DeviceBackend>>,
}

impl BackendRegistry {
    pub fn new() -> Result<Self> {
        let mut backends: Vec<Box<dyn akku_core::DeviceBackend>> = Vec::new();

        // 1. generic-hid-battery (uses bundled manifests)
        let manifests =
            akku_core::load_bundled().map_err(|e| DeckError::Transport { message: e })?;
        backends.push(Box::new(GenericHidBatteryBackend::new(manifests)?));

        // 2. logitech-hidpp
        backends.push(Box::new(LogitechHidppBackend::new()?));

        // 3. openrgb-bridge
        backends.push(Box::new(OpenRgbBridgeBackend::new()?));

        // 4. ratbag-bridge (Linux only)
        #[cfg(target_os = "linux")]
        backends.push(Box::new(RatbagBridgeBackend::new()?));

        // 5. vendor-sdk stubs (Windows only, feature-gated)
        #[cfg(all(target_os = "windows", feature = "vendor-sdk-windows"))]
        backends.push(Box::new(VendorSdkBackend::new()?));

        Ok(Self { backends })
    }

    pub fn all(&self) -> &[Box<dyn akku_core::DeviceBackend>] {
        &self.backends
    }

    pub fn get(&self, id: &str) -> Option<&dyn akku_core::DeviceBackend> {
        self.backends
            .iter()
            .find(|b| b.id() == id)
            .map(|b| b.as_ref())
    }
}

/// Type alias for capability-gated backend
pub type DeviceBackendImpl = akku_core::CapabilityGatedBackend<Box<dyn akku_core::DeviceBackend>>;

fn unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}
