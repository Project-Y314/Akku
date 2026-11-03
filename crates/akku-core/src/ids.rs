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
