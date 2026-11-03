//! Windows HID transport, device selection and the background polling worker.
//!
//! Drivers are read-only unless the matching manifest sets `writes_verified`.
//! Feature handles are opened per operation and dropped immediately, which
//! avoids holding an interface exclusively across sleep/wake or vendor software.

mod decode;
mod driver;
mod enumerate;
pub mod mock;
mod worker;

pub use decode::{
    decode_actuation, decode_battery, decode_dpi, decode_polling, encode_actuation, encode_dpi,
    encode_polling,
};
pub use driver::{HidDriver, key_for_device};
pub use enumerate::{describe, device_key, enumerate, find_manifest, map_error};
pub use mock::MockDriver;
pub use worker::{HidWorker, WorkerCommand, WorkerEvent, probe};
