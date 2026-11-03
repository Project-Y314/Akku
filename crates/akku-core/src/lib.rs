//! Core contracts shared by the Akku application, drivers and command line tools.
//!
//! This crate deliberately contains no Windows API calls. It owns the device model,
//! the driver traits, manifest validation, and the pure math used by the UI.

mod actuation;
mod capability;
mod discharge;
mod error;
mod ids;
mod manifest;
mod model;
mod settings;

pub use actuation::{DeadzoneLayer, effective_actuation};
pub use capability::{
    ActuationCapability, ActuationSpec, DpiCapability, DpiSpec, MacroCapability,
    PeripheralDriver, PollingRateCapability, PollingSpec,
};
pub use discharge::{DischargeSample, DischargeTracker};
pub use error::{DriverError, Result};
pub use ids::{DeviceKey, ManifestId};
pub use manifest::{
    ActuationDescriptor, BatteryDescriptor, BatteryEncoding, ConnectionKind, DeviceKind,
    DpiDescriptor, DpiEncoding, Manifest, ManifestMatch, PollingDescriptor, TransportKind,
    load_bundled, load_dir, parse_manifest,
};
pub use model::{
    BatteryReport, BatterySnapshot, CapabilitySet, DeviceSnapshot, DeviceState, Settings,
    Theme,
};
pub use settings::{SettingsStore, default_settings_path};
