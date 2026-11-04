use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::ids::{DeviceKey, ManifestId};
use crate::model::{ActuationSpec, DpiSpec, PollingSpec};

pub const MANIFEST_SCHEMA: u32 = 1;

const ATTACK_SHARK_MANIFEST: &str = include_str!("../../../devices/attack_shark.toml");
const EPOMAKER_MANIFEST: &str = include_str!("../../../devices/epomaker_p65.toml");

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeviceKind {
    Mouse,
    Keyboard,
}

impl DeviceKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Mouse => "MOUSE",
            Self::Keyboard => "KEYBOARD",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionKind {
    UsbReceiver,
    Wired,
    BluetoothLe,
}

impl ConnectionKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::UsbReceiver => "2.4G",
            Self::Wired => "USB",
            Self::BluetoothLe => "BLE",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransportKind {
    HidFeature,
    ViaRaw,
    BleGatt,
}

impl TransportKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::HidFeature => "HID FEATURE",
            Self::ViaRaw => "VIA RAW HID",
            Self::BleGatt => "BLE GATT",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestMatch {
    pub vid: u16,
    pub pid: u16,
    pub usage_page: u16,
    #[serde(default = "default_usage")]
    pub usage: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub serial: Option<String>,
}

fn default_usage() -> u16 {
    0x0001
}

impl ManifestMatch {
    pub fn key(&self) -> DeviceKey {
        DeviceKey {
            vendor_id: self.vid,
            product_id: self.pid,
            usage_page: self.usage_page,
            usage: self.usage,
            serial: self.serial.clone(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BatteryEncoding {
    Percent,
    PercentInverted,
    VoltageMv,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BatteryDescriptor {
    pub report_id: u8,
    pub report_len: usize,
    pub offset: usize,
    pub width: usize,
    pub encoding: BatteryEncoding,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub charging_offset: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DpiEncoding {
    IndexU8,
    U16Le,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DpiDescriptor {
    pub report_id: u8,
    pub report_len: usize,
    pub offset: usize,
    pub encoding: DpiEncoding,
    pub values: Vec<u16>,
}

impl DpiDescriptor {
    pub fn spec(&self) -> DpiSpec {
        DpiSpec::discrete(self.values.clone())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PollingDescriptor {
    pub report_id: u8,
    pub report_len: usize,
    pub offset: usize,
    pub rates: Vec<u16>,
}

impl PollingDescriptor {
    pub fn spec(&self) -> PollingSpec {
        PollingSpec {
            rates_hz: self.rates.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActuationDescriptor {
    pub report_id: u8,
    pub report_len: usize,
    pub offset: usize,
    pub min_mm: f32,
    pub max_mm: f32,
    pub step_mm: f32,
}

impl ActuationDescriptor {
    pub fn spec(&self) -> ActuationSpec {
        ActuationSpec {
            min_mm: self.min_mm,
            max_mm: self.max_mm,
            step_mm: self.step_mm,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema: u32,
    pub id: String,
    pub name: String,
    pub vendor: String,
    pub kind: DeviceKind,
    pub connection: ConnectionKind,
    pub transport: TransportKind,
    #[serde(rename = "match")]
    pub matches: Vec<ManifestMatch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub battery: Option<BatteryDescriptor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dpi: Option<DpiDescriptor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub polling: Option<PollingDescriptor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actuation: Option<ActuationDescriptor>,
    /// Must be explicitly set only after a captured write has been verified on
    /// real hardware. Read paths work with this false.
    #[serde(default)]
    pub writes_verified: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

impl Manifest {
    pub fn manifest_id(&self) -> ManifestId {
        ManifestId(self.id.clone())
    }

    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();
        if self.schema != MANIFEST_SCHEMA {
            errors.push(format!(
                "schema must be {MANIFEST_SCHEMA}, got {}",
                self.schema
            ));
        }
        if self.id.trim().is_empty()
            || !self
                .id
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            errors.push("id must be a lowercase ASCII slug".into());
        }
        if self.name.trim().is_empty() {
            errors.push("name must not be empty".into());
        }
        if self.matches.is_empty() {
            errors.push("at least one [[match]] is required".into());
        }
        for (index, m) in self.matches.iter().enumerate() {
            if m.vid == 0 || m.pid == 0 {
                errors.push(format!("match[{index}] has a zero VID or PID"));
            }
            if m.usage_page == 0 && m.usage == 0 {
                errors.push(format!("match[{index}] has no usage information"));
            }
        }

        if let Some(battery) = &self.battery {
            if battery.width == 0 || battery.width > 4 {
                errors.push("battery.width must be 1..=4".into());
            }
            if battery.report_len == 0 || battery.offset + battery.width > battery.report_len {
                errors.push("battery descriptor does not fit report_len".into());
            }
            if battery
                .charging_offset
                .is_some_and(|charging| charging >= battery.report_len)
            {
                errors.push("battery.charging_offset is outside report_len".into());
            }
        } else {
            errors.push("battery descriptor is required for this release".into());
        }

        if let Some(dpi) = &self.dpi {
            if dpi.values.is_empty() {
                errors.push("dpi.values must not be empty".into());
            }
            if dpi.values.windows(2).any(|w| w[0] >= w[1]) {
                errors.push("dpi.values must be strictly ascending".into());
            }
            let width = match dpi.encoding {
                DpiEncoding::IndexU8 => 1,
                DpiEncoding::U16Le => 2,
            };
            if dpi.offset + width > dpi.report_len {
                errors.push("dpi descriptor does not fit report_len".into());
            }
        }

        if let Some(polling) = &self.polling {
            if polling.rates.is_empty() {
                errors.push("polling.rates must not be empty".into());
            }
            if polling.rates.windows(2).any(|w| w[0] >= w[1]) {
                errors.push("polling.rates must be strictly ascending".into());
            }
            if polling.offset + 2 > polling.report_len {
                errors.push("polling descriptor does not fit report_len".into());
            }
        }

        if let Some(actuation) = &self.actuation {
            if !actuation.min_mm.is_finite()
                || !actuation.max_mm.is_finite()
                || !actuation.step_mm.is_finite()
                || actuation.min_mm <= 0.0
                || actuation.max_mm <= actuation.min_mm
                || actuation.step_mm <= 0.0
            {
                errors.push("actuation range is invalid".into());
            }
            if actuation.offset + 2 > actuation.report_len {
                errors.push("actuation descriptor does not fit report_len".into());
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    pub fn matches(&self, key: &DeviceKey) -> bool {
        self.matches.iter().any(|m| {
            m.vid == key.vendor_id
                && m.pid == key.product_id
                && m.usage_page == key.usage_page
                && m.usage == key.usage
                && m.serial.as_ref().map_or(true, |serial| {
                    key.serial.as_deref() == Some(serial.as_str())
                })
        })
    }
}

pub fn parse_manifest(source: &str) -> Result<Manifest, String> {
    let manifest: Manifest =
        toml::from_str(source).map_err(|err| format!("manifest parse error: {err}"))?;
    manifest
        .validate()
        .map_err(|errors| format!("manifest validation failed: {}", errors.join("; ")))?;
    Ok(manifest)
}

pub fn load_bundled() -> Result<Vec<Manifest>, String> {
    [ATTACK_SHARK_MANIFEST, EPOMAKER_MANIFEST]
        .into_iter()
        .map(parse_manifest)
        .collect()
}

/// User manifests override bundled manifests with the same `id`.
pub fn load_dir(path: &Path) -> Result<Vec<Manifest>, String> {
    let mut manifests = load_bundled()?;
    if !path.is_dir() {
        return Ok(manifests);
    }
    let mut entries = std::fs::read_dir(path)
        .map_err(|err| format!("cannot read {}: {err}", path.display()))?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "toml"))
        .collect::<Vec<_>>();
    entries.sort();

    for entry in entries {
        let source = std::fs::read_to_string(&entry)
            .map_err(|err| format!("cannot read {}: {err}", entry.display()))?;
        let manifest =
            parse_manifest(&source).map_err(|err| format!("{}: {err}", entry.display()))?;
        if let Some(existing) = manifests
            .iter_mut()
            .find(|existing| existing.id == manifest.id)
        {
            *existing = manifest;
        } else {
            manifests.push(manifest);
        }
    }
    Ok(manifests)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_manifests_validate() {
        let manifests = load_bundled().unwrap();
        assert_eq!(manifests.len(), 2);
        assert!(manifests.iter().any(|m| m.kind == DeviceKind::Mouse));
        assert!(manifests.iter().any(|m| m.kind == DeviceKind::Keyboard));
    }

    #[test]
    fn rejects_unsorted_dpi() {
        let source = r#"
schema = 1
id = "bad"
name = "Bad"
vendor = "Test"
kind = "mouse"
connection = "usb_receiver"
transport = "hid_feature"
writes_verified = false

[[match]]
vid = 0x1
pid = 0x2
usage_page = 0xFFFF

[battery]
report_id = 5
report_len = 8
offset = 1
width = 1
encoding = "percent"

[dpi]
report_id = 5
report_len = 8
offset = 2
encoding = "index_u8"
values = [800, 400]
"#;
        let error = parse_manifest(source).unwrap_err();
        assert!(error.contains("strictly ascending"));
    }

    #[test]
    fn user_manifest_overrides_bundled() {
        let dir = std::env::temp_dir().join(format!("akku-manifest-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../devices/attack_shark.toml"),
        )
        .unwrap()
        .replace("Attack Shark (experimental)", "Attack Shark Override");
        std::fs::write(dir.join("attack_shark.toml"), source).unwrap();
        let manifests = load_dir(&dir).unwrap();
        assert!(manifests.iter().any(|m| m.name == "Attack Shark Override"));
        std::fs::remove_dir_all(dir).ok();
    }
}
