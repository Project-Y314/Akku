//! Core domain types, capability bitflags, error hierarchy, `DeviceBackend` trait,
//! capability-gating middleware, and procedural wide tray pill renderer for PeripheralDeck.
//!
//! This crate is 100% platform-independent so unit tests and protocol parsers run
//! deterministically on any host without touching OS HID handles.

pub mod backend;
pub mod capabilities;
pub mod error;
pub mod model;
pub mod tray_pill;

pub use backend::{CapabilityGatedBackend, DeviceBackend, NotImplementedBackend};
pub use capabilities::{Capabilities, CapabilityName};
pub use error::{DeckError, Result};
pub use model::{
    ActuationSpec, Axis, BatteryState, ConnectionKind, DebounceSpec, DeviceDescriptor,
    DeviceHealth, DeviceId, DeviceKind, DeviceState, DpiSpec, DpiState, FieldStatus, LiftoffSpec,
    LightingEffect, LightingState, PollingSpec, ProfileId, RgbColor, SettingKey, SettingValue,
};
pub use tray_pill::{
    PopoverAnimationFrame, TrayAnimationController, TrayBadgeState, TrayPillDimensions,
    TrayPillRenderer,
};
