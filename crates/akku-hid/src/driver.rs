//! HID driver using the new `DeviceBackend` trait.

#![allow(unused_imports)]

use akku_core::{
    Axis, BatteryReport, Capabilities, DeviceKey, DpiSpec, DriverError, Manifest, PeripheralDriver,
    PollingSpec, Result,
};
use async_hid::{AsyncHidFeatureHandle, Device};
use async_trait::async_trait;

use crate::decode::{
    decode_actuation, decode_battery, decode_dpi, decode_polling, encode_actuation, encode_dpi,
    encode_polling,
};

/// A feature-report driver. It opens a short-lived feature handle for every
/// operation rather than holding the interface exclusively; this coexists with
/// vendor software and survives sleep/wake cycles.
pub struct HidDriver {
    manifest: Manifest,
    device: Device,
    key: DeviceKey,
}

impl HidDriver {
    pub fn new(manifest: Manifest, device: Device) -> Self {
        let key = key_for_device(&manifest, &device);
        Self {
            manifest,
            device,
            key,
        }
    }

    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    fn report_len(&self) -> usize {
        self.manifest
            .battery
            .as_ref()
            .map(|descriptor| descriptor.report_len)
            .or_else(|| {
                self.manifest
                    .dpi
                    .as_ref()
                    .map(|descriptor| descriptor.report_len)
            })
            .or_else(|| {
                self.manifest
                    .polling
                    .as_ref()
                    .map(|descriptor| descriptor.report_len)
            })
            .or_else(|| {
                self.manifest
                    .actuation
                    .as_ref()
                    .map(|descriptor| descriptor.report_len)
            })
            .unwrap_or(65)
            .max(2)
    }

    async fn read_feature(&self, report_id: u8) -> Result<Vec<u8>> {
        let mut handle = self
            .device
            .open_feature_handle()
            .await
            .map_err(map_hid_error)?;
        let mut buffer = vec![0u8; self.report_len()];
        buffer[0] = report_id;
        let read = handle
            .read_feature_report(&mut buffer)
            .await
            .map_err(map_hid_error)?;
        buffer.truncate(read);
        Ok(buffer)
    }

    async fn write_feature(&self, mut buffer: Vec<u8>) -> Result<()> {
        if !self.manifest.writes_verified {
            return Err(DriverError::Unsupported {
                capability: "write".into(),
                backend_id: "hid".into(),
            });
        }
        if buffer.is_empty() {
            return Err(DriverError::InvalidArgument {
                field: "buffer".into(),
                reason: "empty feature report".into(),
            });
        }
        let mut handle = self
            .device
            .open_feature_handle()
            .await
            .map_err(map_hid_error)?;
        handle
            .write_feature_report(&buffer)
            .await
            .map_err(map_hid_error)?;
        buffer.fill(0);
        Ok(())
    }
}

fn map_hid_error(error: async_hid::HidError) -> DriverError {
    let message = error.to_string();
    if message.contains("Access is denied") || message.contains("0x80070005") {
        DriverError::DeviceBusy {
            device_id: "unknown".into(),
            retries_attempted: 0,
        }
    } else {
        DriverError::Transport { message }
    }
}

#[async_trait]
impl akku_core::DpiCapability for HidDriver {
    fn dpi_spec(&self) -> Option<DpiSpec> {
        self.manifest
            .dpi
            .as_ref()
            .map(|descriptor| descriptor.spec())
    }

    async fn read_dpi(&mut self) -> Result<Option<u16>> {
        let Some(descriptor) = self.manifest.dpi.clone() else {
            return Ok(None);
        };
        let report = self.read_feature(descriptor.report_id).await?;
        Ok(Some(decode_dpi(&report, &descriptor)?))
    }

    async fn set_dpi(&mut self, dpi: u16) -> Result<()> {
        let descriptor = self.manifest.dpi.clone().ok_or(DriverError::Unsupported {
            capability: "DPI".into(),
            backend_id: "hid".into(),
        })?;
        let mut buffer = vec![0u8; descriptor.report_len];
        encode_dpi(&mut buffer, &descriptor, dpi)?;
        self.write_feature(buffer).await
    }
}

#[async_trait]
impl akku_core::PollingRateCapability for HidDriver {
    fn polling_spec(&self) -> Option<PollingSpec> {
        self.manifest
            .polling
            .as_ref()
            .map(|descriptor| descriptor.spec())
    }

    async fn read_polling_rate(&mut self) -> Result<Option<u16>> {
        let Some(descriptor) = self.manifest.polling.clone() else {
            return Ok(None);
        };
        let report = self.read_feature(descriptor.report_id).await?;
        Ok(Some(decode_polling(&report, &descriptor)?))
    }

    async fn set_polling_rate(&mut self, hz: u16) -> Result<()> {
        let descriptor = self
            .manifest
            .polling
            .clone()
            .ok_or(DriverError::Unsupported {
                capability: "polling rate".into(),
                backend_id: "hid".into(),
            })?;
        let mut buffer = vec![0u8; descriptor.report_len];
        encode_polling(&mut buffer, &descriptor, hz)?;
        self.write_feature(buffer).await
    }
}

#[async_trait]
impl akku_core::ActuationCapability for HidDriver {
    fn actuation_spec(&self) -> Option<akku_core::ActuationSpec> {
        self.manifest
            .actuation
            .as_ref()
            .map(|descriptor| descriptor.spec())
    }

    async fn read_actuation(&mut self) -> Result<Option<f32>> {
        let Some(descriptor) = self.manifest.actuation.clone() else {
            return Ok(None);
        };
        let report = self.read_feature(descriptor.report_id).await?;
        Ok(Some(decode_actuation(&report, &descriptor)?))
    }

    async fn set_actuation(&mut self, millimetres: f32) -> Result<()> {
        let descriptor = self
            .manifest
            .actuation
            .clone()
            .ok_or(DriverError::Unsupported {
                capability: "actuation".into(),
                backend_id: "hid".into(),
            })?;
        let mut buffer = vec![0u8; descriptor.report_len];
        encode_actuation(&mut buffer, &descriptor, millimetres)?;
        self.write_feature(buffer).await
    }
}

#[async_trait]
impl PeripheralDriver for HidDriver {
    fn key(&self) -> &DeviceKey {
        &self.key
    }

    fn capabilities(&self) -> Capabilities {
        let mut caps = Capabilities::empty();
        if self.manifest.dpi.is_some() {
            caps.insert(Capabilities::DPI);
        }
        if self.manifest.polling.is_some() {
            caps.insert(Capabilities::POLLING_RATE);
        }
        if self.manifest.actuation.is_some() {
            caps.insert(Capabilities::ACTUATION);
        }
        caps
    }

    async fn read_battery(&mut self) -> Result<BatteryReport> {
        let descriptor = self
            .manifest
            .battery
            .clone()
            .ok_or(DriverError::Unsupported {
                capability: "battery".into(),
                backend_id: "hid".into(),
            })?;
        let report = self.read_feature(descriptor.report_id).await?;
        decode_battery(&report, &descriptor)
    }

    async fn refresh(&mut self) -> Result<()> {
        Ok(())
    }
}

pub fn key_for_device(_manifest: &Manifest, device: &Device) -> DeviceKey {
    DeviceKey {
        vendor_id: device.vendor_id,
        product_id: device.product_id,
        usage_page: device.usage_page,
        usage: device.usage_id,
        serial: device.serial_number.clone(),
    }
}

// Re-export the old capability traits for compatibility
pub use akku_core::{ActuationCapability, DpiCapability, PollingRateCapability};
