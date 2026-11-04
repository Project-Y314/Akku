//! Capability bitflags (`Capabilities`) and wire serialization helpers.
//!
//! Every device exposes a `Capabilities` bitflag. The UI MUST hide/disable controls
//! for unsupported capabilities, and `CapabilityGatedBackend` rejects writes for
//! capabilities not advertised by the device.

use bitflags::bitflags;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

bitflags! {
    /// Device feature capability flags.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct Capabilities: u32 {
        /// Battery percentage, charging state, or cell voltage reporting.
        const BATTERY      = 1 << 0;
        /// Adjustable CPI / DPI sensor resolution (uniform or per-axis X/Y).
        const DPI          = 1 << 1;
        /// Configurable USB / 2.4GHz report polling rate in Hz.
        const POLLING_RATE = 1 << 2;
        /// RGB or monochrome LED lighting zone control.
        const LIGHTING     = 1 << 3;
        /// Onboard memory profile switching.
        const PROFILES     = 1 << 4;
        /// Hardware macro slot programming.
        const MACROS       = 1 << 5;
        /// Sensor lift-off distance (LOD) calibration.
        const LIFTOFF      = 1 << 6;
        /// Switch debounce time in milliseconds.
        const DEBOUNCE     = 1 << 7;
        /// Hall-effect / analog switch actuation point in millimeters (from Akku).
        const ACTUATION    = 1 << 8;
    }
}

/// Canonical string identifiers emitted to the React frontend for capability gating.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CapabilityName {
    Battery,
    Dpi,
    PollingRate,
    Lighting,
    Profiles,
    Macros,
    Liftoff,
    Debounce,
    Actuation,
}

impl CapabilityName {
    pub const ALL: [CapabilityName; 9] = [
        Self::Battery,
        Self::Dpi,
        Self::PollingRate,
        Self::Lighting,
        Self::Profiles,
        Self::Macros,
        Self::Liftoff,
        Self::Debounce,
        Self::Actuation,
    ];

    pub const fn bit(self) -> Capabilities {
        match self {
            Self::Battery => Capabilities::BATTERY,
            Self::Dpi => Capabilities::DPI,
            Self::PollingRate => Capabilities::POLLING_RATE,
            Self::Lighting => Capabilities::LIGHTING,
            Self::Profiles => Capabilities::PROFILES,
            Self::Macros => Capabilities::MACROS,
            Self::Liftoff => Capabilities::LIFTOFF,
            Self::Debounce => Capabilities::DEBOUNCE,
            Self::Actuation => Capabilities::ACTUATION,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Battery => "BATTERY",
            Self::Dpi => "DPI",
            Self::PollingRate => "POLLING_RATE",
            Self::Lighting => "LIGHTING",
            Self::Profiles => "PROFILES",
            Self::Macros => "MACROS",
            Self::Liftoff => "LIFTOFF",
            Self::Debounce => "DEBOUNCE",
            Self::Actuation => "ACTUATION",
        }
    }
}

impl Capabilities {
    /// Returns the list of active capability names in deterministic order.
    pub fn active_names(self) -> Vec<CapabilityName> {
        CapabilityName::ALL
            .into_iter()
            .filter(|name| self.contains(name.bit()))
            .collect()
    }

    /// Reconstructs `Capabilities` from a slice of `CapabilityName`.
    pub fn from_names(names: &[CapabilityName]) -> Self {
        let mut out = Capabilities::empty();
        for name in names {
            out.insert(name.bit());
        }
        out
    }
}

/// Wire format for `Capabilities` so the frontend receives both the raw bitmask
/// (`bits`) and the explicit list of active flag names (`flags`).
#[derive(Serialize, Deserialize)]
struct CapabilitiesWire {
    bits: u32,
    flags: Vec<CapabilityName>,
}

impl Serialize for Capabilities {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let wire = CapabilitiesWire {
            bits: self.bits(),
            flags: self.active_names(),
        };
        wire.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Capabilities {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = CapabilitiesWire::deserialize(deserializer)?;
        let from_bits = Capabilities::from_bits_truncate(wire.bits);
        let from_flags = Capabilities::from_names(&wire.flags);
        Ok(from_bits | from_flags)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_capabilities_json() {
        let caps = Capabilities::BATTERY | Capabilities::DPI | Capabilities::POLLING_RATE;
        let json = serde_json::to_string(&caps).expect("serialize capabilities");
        assert!(json.contains("\"BATTERY\""));
        assert!(json.contains("\"DPI\""));
        assert!(json.contains("\"POLLING_RATE\""));
        assert!(!json.contains("\"MACROS\""));

        let decoded: Capabilities = serde_json::from_str(&json).expect("deserialize capabilities");
        assert_eq!(decoded, caps);
    }
}
