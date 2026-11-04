//! Full serde data model: `DeviceId`, `DeviceDescriptor`, `DeviceState`, `FieldStatus`,
//! `Axis`, `SettingKey`, `SettingValue`, and hardware range specifications.

use serde::{Deserialize, Serialize};

use crate::capabilities::{Capabilities, CapabilityName};

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

/// High-level device category.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceKind {
    Mouse,
    Keyboard,
    Receiver,
    Gamepad,
    Other,
}

/// Physical transport / connection type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionKind {
    UsbReceiver,
    Wired,
    BluetoothLe,
}

/// Supported DPI steps or continuous range for a mouse sensor.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DpiSpec {
    pub min: u16,
    pub max: u16,
    pub step: u16,
    /// Discrete preset values if the device only accepts fixed steps.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub presets: Vec<u16>,
    /// True if X and Y axes can be configured independently.
    pub independent_xy: bool,
}

impl DpiSpec {
    pub fn discrete(presets: Vec<u16>) -> Self {
        let min = presets.first().copied().unwrap_or(400);
        let max = presets.last().copied().unwrap_or(3200);
        Self {
            min,
            max,
            step: 0,
            presets,
            independent_xy: false,
        }
    }

    pub fn continuous(min: u16, max: u16, step: u16, independent_xy: bool) -> Self {
        Self {
            min,
            max,
            step: step.max(1),
            presets: vec![400, 800, 1200, 1600, 2400, 3200, 6400]
                .into_iter()
                .filter(|&v| v >= min && v <= max)
                .collect(),
            independent_xy,
        }
    }

    /// Validates whether a target DPI is accepted by this sensor specification.
    pub fn accepts(&self, dpi: u16) -> bool {
        if dpi < self.min || dpi > self.max {
            return false;
        }
        if self.step == 0 && !self.presets.is_empty() {
            return self.presets.contains(&dpi);
        }
        if self.step > 0 {
            return (dpi - self.min) % self.step == 0;
        }
        true
    }
}

/// Supported USB/wireless report rates in Hz.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PollingSpec {
    pub rates_hz: Vec<u16>,
}

/// Supported sensor lift-off distance (LOD) values in millimeters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LiftoffSpec {
    pub values_mm: Vec<f32>,
}

/// Supported switch debounce range in milliseconds.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DebounceSpec {
    pub min_ms: u8,
    pub max_ms: u8,
    pub step_ms: u8,
}

/// Supported Hall-effect / analog switch actuation travel in millimeters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ActuationSpec {
    pub min_mm: f32,
    pub max_mm: f32,
    pub step_mm: f32,
}

/// Static metadata and capability constraints discovered during `DeviceBackend::enumerate`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DeviceDescriptor {
    pub id: DeviceId,
    pub name: String,
    pub vendor_name: String,
    pub kind: DeviceKind,
    pub connection: ConnectionKind,
    pub capabilities: Capabilities,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dpi_spec: Option<DpiSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub polling_spec: Option<PollingSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub liftoff_spec: Option<LiftoffSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub debounce_spec: Option<DebounceSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actuation_spec: Option<ActuationSpec>,
}

/// Explicit state wrapper for every telemetry or configuration field.
///
/// Guarantees the UI never fakes a value or shows an infinite loading spinner.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum FieldStatus<T> {
    /// Fresh, verified reading from the device.
    Available { value: T },
    /// The device or backend does not support this capability.
    Unsupported,
    /// The device timed out or failed to answer; preserves optional last known reading.
    NotResponding {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        last_known: Option<T>,
        since_unix_ms: u64,
    },
    /// Wireless device is in deep sleep (e.g. HID++ `DJ`/`0x1000` offline notification).
    Sleeping {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        last_known: Option<T>,
        since_unix_ms: u64,
    },
    /// Feature requires an external daemon (e.g. OpenRGB SDK server or ratbagd).
    RequiresVendorService {
        service_name: String,
        reason: String,
    },
    /// OS rejected device open due to missing udev rule or permissions.
    RequiresElevation {
        reason: String,
    },
}

impl<T> FieldStatus<T> {
    pub fn available(value: T) -> Self {
        Self::Available { value }
    }

    pub fn is_supported(&self) -> bool {
        !matches!(self, Self::Unsupported)
    }

    pub fn value_or_last_known(&self) -> Option<&T> {
        match self {
            Self::Available { value } => Some(value),
            Self::NotResponding { last_known, .. } | Self::Sleeping { last_known, .. } => {
                last_known.as_ref()
            }
            _ => None,
        }
    }
}

/// High-level connectivity status of the device.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum DeviceHealth {
    Online,
    Sleeping,
    NotResponding { reason: String },
    RequiresVendorService { service_name: String },
    RequiresElevation { reason: String },
    Disconnected,
}

/// Battery telemetry reading.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatteryState {
    /// Battery charge level in `0..=100`.
    pub percent: u8,
    /// True when external power is connected or charging is in progress.
    pub charging: bool,
    /// Optional raw cell voltage in millivolts (e.g. 3850 mV on HID++ 0x1001).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voltage_mv: Option<u16>,
}

/// Sensor DPI / CPI reading.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DpiState {
    pub x: u16,
    pub y: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_stage: Option<u8>,
}

impl DpiState {
    pub fn uniform(dpi: u16) -> Self {
        Self {
            x: dpi,
            y: dpi,
            active_stage: None,
        }
    }
}

/// Axis selector for `DeviceBackend::write_dpi`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Axis {
    /// Set both X and Y sensor axes to the same CPI/DPI value.
    Uniform,
    /// Set horizontal X axis only.
    X,
    /// Set vertical Y axis only.
    Y,
}

/// 24-bit RGB color value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RgbColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

/// Lighting effect mode.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum LightingEffect {
    Off,
    Static { color: RgbColor, brightness_pct: u8 },
    Breathing { color: RgbColor, speed: u8 },
    SpectrumCycle { speed: u8, brightness_pct: u8 },
}

/// Device lighting state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LightingState {
    pub effect: LightingEffect,
    pub zone_count: u8,
}

/// Onboard profile identifier.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProfileId(pub String);

/// Complete point-in-time state returned by `DeviceBackend::read_state`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DeviceState {
    pub id: DeviceId,
    pub health: DeviceHealth,
    pub battery: FieldStatus<BatteryState>,
    pub dpi: FieldStatus<DpiState>,
    pub polling_rate_hz: FieldStatus<u16>,
    pub lighting: FieldStatus<LightingState>,
    pub active_profile: FieldStatus<ProfileId>,
    pub liftoff_mm: FieldStatus<f32>,
    pub debounce_ms: FieldStatus<u8>,
    pub actuation_mm: FieldStatus<f32>,
    pub updated_at_unix_ms: u64,
}

impl DeviceState {
    /// Creates an initial `DeviceState` where all fields default to `FieldStatus::Unsupported`.
    pub fn empty_for(id: DeviceId, updated_at_unix_ms: u64) -> Self {
        Self {
            id,
            health: DeviceHealth::Online,
            battery: FieldStatus::Unsupported,
            dpi: FieldStatus::Unsupported,
            polling_rate_hz: FieldStatus::Unsupported,
            lighting: FieldStatus::Unsupported,
            active_profile: FieldStatus::Unsupported,
            liftoff_mm: FieldStatus::Unsupported,
            debounce_ms: FieldStatus::Unsupported,
            actuation_mm: FieldStatus::Unsupported,
            updated_at_unix_ms,
        }
    }

    /// Enforces capability gating on read state: any field whose bit is absent in `caps`
    /// is overwritten with `FieldStatus::Unsupported` so backends can never leak phantom values.
    pub fn mask_to_capabilities(mut self, caps: Capabilities) -> Self {
        if !caps.contains(Capabilities::BATTERY) {
            self.battery = FieldStatus::Unsupported;
        }
        if !caps.contains(Capabilities::DPI) {
            self.dpi = FieldStatus::Unsupported;
        }
        if !caps.contains(Capabilities::POLLING_RATE) {
            self.polling_rate_hz = FieldStatus::Unsupported;
        }
        if !caps.contains(Capabilities::LIGHTING) {
            self.lighting = FieldStatus::Unsupported;
        }
        if !caps.contains(Capabilities::PROFILES) {
            self.active_profile = FieldStatus::Unsupported;
        }
        if !caps.contains(Capabilities::LIFTOFF) {
            self.liftoff_mm = FieldStatus::Unsupported;
        }
        if !caps.contains(Capabilities::DEBOUNCE) {
            self.debounce_ms = FieldStatus::Unsupported;
        }
        if !caps.contains(Capabilities::ACTUATION) {
            self.actuation_mm = FieldStatus::Unsupported;
        }
        self
    }
}

/// Keys accepted by `DeviceBackend::write_setting`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingKey {
    PollingRateHz,
    Lighting,
    ActiveProfile,
    MacroBinary,
    LiftoffDistanceMm,
    DebounceMs,
    ActuationPointMm,
}

impl SettingKey {
    /// Returns the `Capabilities` bit required to modify this setting.
    pub const fn required_capability(self) -> Capabilities {
        match self {
            Self::PollingRateHz => Capabilities::POLLING_RATE,
            Self::Lighting => Capabilities::LIGHTING,
            Self::ActiveProfile => Capabilities::PROFILES,
            Self::MacroBinary => Capabilities::MACROS,
            Self::LiftoffDistanceMm => Capabilities::LIFTOFF,
            Self::DebounceMs => Capabilities::DEBOUNCE,
            Self::ActuationPointMm => Capabilities::ACTUATION,
        }
    }

    pub const fn capability_name(self) -> CapabilityName {
        match self {
            Self::PollingRateHz => CapabilityName::PollingRate,
            Self::Lighting => CapabilityName::Lighting,
            Self::ActiveProfile => CapabilityName::Profiles,
            Self::MacroBinary => CapabilityName::Macros,
            Self::LiftoffDistanceMm => CapabilityName::Liftoff,
            Self::DebounceMs => CapabilityName::Debounce,
            Self::ActuationPointMm => CapabilityName::Actuation,
        }
    }
}

/// Typed value payload paired with `SettingKey` in `DeviceBackend::write_setting`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum SettingValue {
    PollingRateHz(u16),
    Lighting(LightingEffect),
    ActiveProfile(ProfileId),
    MacroBinary { slot: u8, payload: Vec<u8> },
    LiftoffDistanceMm(f32),
    DebounceMs(u8),
    ActuationPointMm(f32),
}

impl SettingValue {
    /// Verifies that the `SettingValue` variant matches the `SettingKey`.
    pub fn matches_key(&self, key: SettingKey) -> bool {
        matches!(
            (key, self),
            (SettingKey::PollingRateHz, SettingValue::PollingRateHz(_))
                | (SettingKey::Lighting, SettingValue::Lighting(_))
                | (SettingKey::ActiveProfile, SettingValue::ActiveProfile(_))
                | (SettingKey::MacroBinary, SettingValue::MacroBinary { .. })
                | (SettingKey::LiftoffDistanceMm, SettingValue::LiftoffDistanceMm(_))
                | (SettingKey::DebounceMs, SettingValue::DebounceMs(_))
                | (SettingKey::ActuationPointMm, SettingValue::ActuationPointMm(_))
        )
    }
}
