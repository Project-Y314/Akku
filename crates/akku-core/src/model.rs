use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::ids::{DeviceKey, ManifestId};

/// A single battery reading as decoded from a report.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BatteryReport {
    pub percent: Option<u8>,
    pub voltage_mv: Option<u16>,
    pub charging: bool,
    pub raw: Vec<u8>,
}

impl BatteryReport {
    pub fn percent(percent: u8) -> Self {
        Self {
            percent: Some(percent.min(100)),
            ..Self::default()
        }
    }
}

/// A battery reading plus freshness metadata used by the UI.
#[derive(Clone, Debug)]
pub struct BatterySnapshot {
    pub report: BatteryReport,
    pub updated_at: SystemTime,
    pub stale: bool,
}

impl BatterySnapshot {
    pub fn unknown() -> Self {
        Self {
            report: BatteryReport::default(),
            updated_at: SystemTime::UNIX_EPOCH,
            stale: true,
        }
    }

    pub fn display_percent(&self) -> String {
        match self.report.percent {
            Some(p) => format!("{p}%"),
            None => "--".to_string(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct CapabilitySet {
    pub dpi: bool,
    pub polling_rate: bool,
    pub actuation: bool,
    pub macros: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeviceState {
    Connected,
    Sleeping,
    Stale,
    Disconnected,
    Error(String),
}

impl DeviceState {
    pub fn label(&self) -> &str {
        match self {
            Self::Connected => "CONNECTED",
            Self::Sleeping => "SLEEPING",
            Self::Stale => "STALE",
            Self::Disconnected => "DISCONNECTED",
            Self::Error(_) => "ERROR",
        }
    }
}

/// A complete, UI-facing device snapshot. HID worker owns the driver; the UI
/// receives these values over a channel and never touches a device handle.
#[derive(Clone, Debug)]
pub struct DeviceSnapshot {
    pub manifest_id: ManifestId,
    pub key: DeviceKey,
    pub name: String,
    pub vendor: String,
    pub kind: crate::manifest::DeviceKind,
    pub connection: crate::manifest::ConnectionKind,
    pub state: DeviceState,
    pub battery: BatterySnapshot,
    pub capabilities: CapabilitySet,
    /// Writes stay disabled until a real capture has been verified.
    pub writable: bool,
    pub dpi: Option<u16>,
    pub polling_hz: Option<u16>,
    pub actuation_mm: Option<f32>,
    pub last_error: Option<String>,
}

impl DeviceSnapshot {
    pub fn new(
        manifest_id: ManifestId,
        key: DeviceKey,
        name: impl Into<String>,
        vendor: impl Into<String>,
        kind: crate::manifest::DeviceKind,
        connection: crate::manifest::ConnectionKind,
    ) -> Self {
        Self {
            manifest_id,
            key,
            name: name.into(),
            vendor: vendor.into(),
            kind,
            connection,
            state: DeviceState::Disconnected,
            battery: BatterySnapshot::unknown(),
            capabilities: CapabilitySet::default(),
            writable: false,
            dpi: None,
            polling_hz: None,
            actuation_mm: None,
            last_error: None,
        }
    }
}

/// Strictly two themes. No accent colours, no per-device colours.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    Black,
    White,
}

impl Theme {
    pub fn toggled(self) -> Self {
        match self {
            Self::Black => Self::White,
            Self::White => Self::Black,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Black => "PURE BLACK",
            Self::White => "PURE WHITE",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Settings {
    pub theme: Theme,
    pub poll_interval_secs: u64,
    pub preferred_refresh_hz: Option<u32>,
    pub autostart: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: Theme::Black,
            poll_interval_secs: 60,
            preferred_refresh_hz: None,
            autostart: false,
        }
    }
}
