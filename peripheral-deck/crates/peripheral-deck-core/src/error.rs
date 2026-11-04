//! Structured error enum (`DeckError`) shared across backends, supervisor, and Tauri IPC.
//!
//! Explicitly models `Unsupported`, `NotResponding`, `RequiresVendorService`,
//! `RequiresElevation`, `DeviceBusy`, and `NotImplemented` so the UI never spins
//! indefinitely or guesses hardware state.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Canonical error type for all PeripheralDeck backends and supervisor tasks.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Error)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DeckError {
    #[error("capability '{capability}' is not supported by device on backend '{backend_id}'")]
    Unsupported {
        capability: String,
        backend_id: String,
    },

    #[error("device '{device_id}' did not respond within {timeout_ms} ms")]
    NotResponding {
        device_id: String,
        timeout_ms: u64,
    },

    #[error("feature requires vendor or companion service '{service}' ({reference_url})")]
    RequiresVendorService {
        service: String,
        reference_url: String,
    },

    #[error("access to '{device_path}' requires additional permissions ({os_hint})")]
    RequiresElevation {
        device_path: String,
        os_hint: String,
    },

    #[error("device '{device_id}' is busy after {retries_attempted} retry attempt(s)")]
    DeviceBusy {
        device_id: String,
        retries_attempted: u8,
    },

    #[error("device '{device_id}' is disconnected")]
    Disconnected {
        device_id: String,
    },

    #[error("invalid argument for '{field}': {reason}")]
    InvalidArgument {
        field: String,
        reason: String,
    },

    #[error("protocol error in backend '{backend_id}': {message}")]
    Protocol {
        backend_id: String,
        message: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reference_url: Option<String>,
    },

    #[error("HID transport error: {message}")]
    Transport {
        message: String,
    },

    #[error("backend '{backend_id}' feature '{feature}' is not implemented (reference: {reference_url})")]
    NotImplemented {
        backend_id: String,
        feature: String,
        reference_url: String,
    },

    #[error("I/O error: {message}")]
    Io {
        message: String,
    },
}

impl DeckError {
    /// Returns `true` if the operation should be retried once by the capability-gated wrapper.
    pub fn is_device_busy(&self) -> bool {
        matches!(self, Self::DeviceBusy { .. })
    }

    /// Classifies raw OS / `hidapi` error strings into `RequiresElevation` or `DeviceBusy`
    /// where applicable without requiring administrator privileges.
    pub fn from_hid_error(device_id: impl Into<String>, raw_message: impl Into<String>) -> Self {
        let dev = device_id.into();
        let msg = raw_message.into();
        let lower = msg.to_ascii_lowercase();

        if lower.contains("sharing violation")
            || lower.contains("resource busy")
            || lower.contains("ebusy")
            || lower.contains("exclusively opened")
        {
            return Self::DeviceBusy {
                device_id: dev,
                retries_attempted: 0,
            };
        }

        if lower.contains("access is denied")
            || lower.contains("0x80070005")
            || lower.contains("permission denied")
            || lower.contains("eacces")
            || lower.contains("not permitted")
        {
            return Self::RequiresElevation {
                device_path: dev,
                os_hint: "Install udev rule on Linux (MODE=\"0660\", TAG+=\"uaccess\") or grant Input Monitoring on macOS; on Windows ensure interface is not a system-exclusive keyboard/mouse collection".to_string(),
            };
        }

        Self::Transport { message: msg }
    }
}

impl From<std::io::Error> for DeckError {
    fn from(err: std::io::Error) -> Self {
        Self::Io {
            message: err.to_string(),
        }
    }
}

pub type Result<T> = std::result::Result<T, DeckError>;
