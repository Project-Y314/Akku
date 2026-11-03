use akku_core::{DeviceKey, DriverError, Manifest, Result};
use async_hid::{Device, HidBackend, HidError};
use futures_lite::StreamExt;

/// Enumerate every accessible HID interface. Interfaces that cannot be
/// described are omitted by async-hid itself.
pub async fn enumerate(backend: &HidBackend) -> Result<Vec<Device>> {
    let stream = backend
        .enumerate()
        .await
        .map_err(map_error)?;
    let mut devices = Vec::new();
    let mut stream = std::pin::pin!(stream);
    while let Some(device) = stream.next().await {
        devices.push(device);
    }
    Ok(devices)
}

pub fn device_key(device: &Device) -> DeviceKey {
    DeviceKey {
        vendor_id: device.vendor_id,
        product_id: device.product_id,
        usage_page: device.usage_page,
        usage: device.usage_id,
        serial: device.serial_number.clone(),
    }
}

pub fn find_manifest<'a>(manifests: &'a [Manifest], device: &Device) -> Option<&'a Manifest> {
    let key = device_key(device);
    manifests.iter().find(|manifest| manifest.matches(&key))
}

pub fn map_error(error: HidError) -> DriverError {
    let message = error.to_string();
    if message.contains("Access is denied") || message.contains("0x80070005") {
        DriverError::DeviceBusy
    } else {
        DriverError::Transport(message)
    }
}

/// Human-readable dump used by `akku-cli list-hid` and `dump`.
pub fn describe(device: &Device) -> String {
    format!(
        "{:04X}:{:04X}  page {:04X} usage {:04X}  {}{}",
        device.vendor_id,
        device.product_id,
        device.usage_page,
        device.usage_id,
        device.name,
        device
            .manufacturer
            .as_ref()
            .map(|manufacturer| format!("  ({manufacturer})"))
            .unwrap_or_default()
    )
}
