//! Windows HID transport, device selection and the background polling worker.
//!
//! Drivers are read-only unless the matching manifest sets `writes_verified`.
//! Feature handles are opened per operation and dropped immediately, which
//! avoids holding an interface exclusively across sleep/wake or vendor software.

pub mod backends;
pub mod decode;
pub mod driver;
pub mod enumerate;
pub mod mock;
pub mod worker;

pub use backends::{BackendRegistry, DeviceBackendImpl};
pub use decode::{
    decode_actuation, decode_battery, decode_dpi, decode_polling, encode_actuation, encode_dpi,
    encode_polling,
};
pub use driver::{key_for_device, HidDriver};
pub use enumerate::{describe, device_key, enumerate, find_manifest, map_error};
pub use mock::MockDriver;
pub use worker::{probe, CapabilitySet, DeviceSnapshot, HidWorker, WorkerCommand, WorkerEvent};

/// Re-exports so command line tools can control the Hid flow without depending
/// on async-hid directly.
pub use async_hid::{Device, HidBackend};
