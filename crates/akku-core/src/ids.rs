use serde::{Deserialize, Serialize};

/// Stable manifest identifier. Never derive identity from a display name.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ManifestId(pub String);

impl ManifestId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ManifestId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// The parts of a HID identity that must match a manifest.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DeviceKey {
    pub vendor_id: u16,
    pub product_id: u16,
    pub usage_page: u16,
    pub usage: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub serial: Option<String>,
}

impl DeviceKey {
    pub fn new(vendor_id: u16, product_id: u16, usage_page: u16, usage: u16) -> Self {
        Self {
            vendor_id,
            product_id,
            usage_page,
            usage,
            serial: None,
        }
    }

    pub fn label(&self) -> String {
        format!(
            "{:04X}:{:04X} page {:04X} usage {:04X}",
            self.vendor_id, self.product_id, self.usage_page, self.usage
        )
    }
}

/// Uniquely identifies a physical or wireless-slot peripheral across enumerations.
///
/// For multi-device wireless receivers (Logitech Unifying `046D:C52B`, Logitech Bolt `046D:C548`),
/// `receiver_slot` (1..=6) distinguishes paired devices sharing a single USB receiver PID.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct DeviceId {
    /// Backend identifier that owns this device (e.g. `"logitech-hidpp"`, `"generic-hid-battery"`).
    pub backend_id: String,
    /// USB/Bluetooth Vendor ID (e.g. `0x046D` for Logitech).
    pub vendor_id: u16,
    /// USB/Bluetooth Product ID (or wireless receiver PID if paired through a dongle).
    pub product_id: u16,
    /// Primary HID Usage Page matched during enumeration.
    pub usage_page: u16,
    /// Primary HID Usage ID matched during enumeration.
    pub usage: u16,
    /// Wireless sub-device index (1..=6 for Unifying/Bolt, `0xFF` for receiver itself, `None` for direct USB/BLE).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receiver_slot: Option<u8>,
    /// Optional hardware serial number or MAC suffix.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub serial: Option<String>,
}

impl DeviceId {
    pub fn new(
        backend_id: impl Into<String>,
        vendor_id: u16,
        product_id: u16,
        usage_page: u16,
        usage: u16,
    ) -> Self {
        Self {
            backend_id: backend_id.into(),
            vendor_id,
            product_id,
            usage_page,
            usage,
            receiver_slot: None,
            serial: None,
        }
    }

    pub fn with_receiver_slot(mut self, slot: u8) -> Self {
        self.receiver_slot = Some(slot);
        self
    }

    pub fn with_serial(mut self, serial: impl Into<String>) -> Self {
        self.serial = Some(serial.into());
        self
    }

    /// Stable, URL-safe key used in profile persistence and React list keys.
    pub fn canonical_key(&self) -> String {
        let slot_part = self
            .receiver_slot
            .map(|s| format!(":slot{s}"))
            .unwrap_or_default();
        let serial_part = self
            .serial
            .as_deref()
            .filter(|s| !s.is_empty())
            .map(|s| format!(":{s}"))
            .unwrap_or_default();
        format!(
            "{}:{:04X}:{:04X}:{:04X}:{:04X}{}{}",
            self.backend_id,
            self.vendor_id,
            self.product_id,
            self.usage_page,
            self.usage,
            slot_part,
            serial_part
        )
    }
}

impl std::fmt::Display for DeviceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.canonical_key())
    }
}
