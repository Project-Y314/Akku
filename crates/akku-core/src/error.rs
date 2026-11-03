use std::fmt;

use thiserror::Error;

/// Errors produced by device drivers and transports.
#[derive(Debug, Error)]
pub enum DriverError {
    #[error("operation is not supported: {0}")]
    Unsupported(&'static str),
    #[error("invalid argument: {0}")]
    InvalidArgument(String),
    #[error("device is busy or exclusively opened")]
    DeviceBusy,
    #[error("device is disconnected")]
    Disconnected,
    #[error("transport error: {0}")]
    Transport(String),
    #[error("protocol error: {0}")]
    Protocol(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

impl DriverError {
    pub fn transport(err: impl fmt::Display) -> Self {
        Self::Transport(err.to_string())
    }

    pub fn protocol(err: impl fmt::Display) -> Self {
        Self::Protocol(err.to_string())
    }
}

pub type Result<T> = std::result::Result<T, DriverError>;
