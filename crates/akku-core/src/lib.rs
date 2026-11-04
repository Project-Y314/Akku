//! Core domain types, capability bitflags, error hierarchy, `DeviceBackend` trait,
//! capability-gating middleware, and procedural wide tray pill renderer for PeripheralDeck.
//!
//! This crate is 100% platform-independent so unit tests and protocol parsers run
//! deterministically on any host without touching OS HID handles.

pub mod actuation;
pub mod backend;
pub mod capabilities;
pub mod error;
pub mod ids;
pub mod manifest;
pub mod model;
pub mod settings;
pub mod tray_pill;

pub use actuation::effective_actuation;
pub use backend::{CapabilityGatedBackend, DeviceBackend, NotImplementedBackend};
pub use capabilities::{Capabilities, CapabilityName};
pub use error::{DeckError, DriverError, Result};
pub use ids::{DeviceKey, ManifestId};
pub use manifest::{
    load_bundled, load_dir, parse_manifest, ActuationDescriptor, BatteryDescriptor,
    BatteryEncoding, ConnectionKind, DeviceKind, DpiDescriptor, DpiEncoding, Manifest,
    ManifestMatch, PollingDescriptor, TransportKind,
};
pub use model::{
    ActuationCapability, ActuationSpec, Axis, BatteryReport, BatteryState,
    Capabilities as ModelCapabilities, ConnectionKind as ModelConnectionKind, DebounceSpec,
    DeviceDescriptor, DeviceHealth, DeviceId, DeviceKind as ModelDeviceKind, DeviceState,
    DpiCapability, DpiSpec, DpiState, FieldStatus, LiftoffSpec, LightingEffect, LightingState,
    PeripheralDriver, PollingRateCapability, PollingSpec, ProfileId, RgbColor, SettingKey,
    SettingValue, Settings, Theme,
};
pub use settings::{default_settings_path, SettingsStore};
pub use tray_pill::{
    PopoverAnimationFrame, TrayAnimationController, TrayBadgeState, TrayPillDimensions,
    TrayPillRenderer,
};

// Re-export capability traits and spec types from model
pub mod capability {
    pub use crate::model::{
        ActuationCapability, ActuationSpec, Capabilities, CapabilityName, DpiCapability, DpiSpec,
        PollingRateCapability, PollingSpec,
    };
}
