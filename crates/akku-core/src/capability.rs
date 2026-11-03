use async_trait::async_trait;

use crate::error::Result;
use crate::ids::DeviceKey;
use crate::model::{BatteryReport, CapabilitySet};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DpiSpec {
    /// Values accepted by the device, in DPI, ascending and unique.
    pub values: Vec<u16>,
}

impl DpiSpec {
    pub fn new(values: Vec<u16>) -> Self {
        Self { values }
    }

    pub fn min(&self) -> Option<u16> {
        self.values.first().copied()
    }

    pub fn max(&self) -> Option<u16> {
        self.values.last().copied()
    }

    pub fn nearest(&self, dpi: u16) -> Option<u16> {
        self.values
            .iter()
            .copied()
            .min_by_key(|value| value.abs_diff(dpi))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PollingSpec {
    /// Supported report rates in Hz, ascending and unique.
    pub rates: Vec<u16>,
}

impl PollingSpec {
    pub fn new(rates: Vec<u16>) -> Self {
        Self { rates }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ActuationSpec {
    pub min_mm: f32,
    pub max_mm: f32,
    pub step_mm: f32,
}

impl ActuationSpec {
    pub fn clamp(&self, value: f32) -> f32 {
        value.clamp(self.min_mm, self.max_mm)
    }
}

/// DPI control. Drivers that do not expose DPI return `None` from `dpi_spec`
/// and `DriverError::Unsupported` from writes.
#[async_trait]
pub trait DpiCapability: Send + Sync {
    fn dpi_spec(&self) -> Option<DpiSpec>;
    async fn read_dpi(&mut self) -> Result<Option<u16>>;
    async fn set_dpi(&mut self, dpi: u16) -> Result<()>;
}

#[async_trait]
pub trait PollingRateCapability: Send + Sync {
    fn polling_spec(&self) -> Option<PollingSpec>;
    async fn read_polling_rate(&mut self) -> Result<Option<u16>>;
    async fn set_polling_rate(&mut self, hz: u16) -> Result<()>;
}

#[async_trait]
pub trait ActuationCapability: Send + Sync {
    fn actuation_spec(&self) -> Option<ActuationSpec>;
    async fn read_actuation(&mut self) -> Result<Option<f32>>;
    async fn set_actuation(&mut self, millimetres: f32) -> Result<()>;
}

/// Protocol-level macro support. Deliberately separate from `PeripheralDriver`
/// so a keyboard can expose actuation without pretending macros are available.
#[async_trait]
pub trait MacroCapability: Send + Sync {
    fn supports_macros(&self) -> bool;
    async fn write_macro(&mut self, name: &str, bytes: &[u8]) -> Result<()>;
}

#[async_trait]
pub trait PeripheralDriver:
    DpiCapability + PollingRateCapability + ActuationCapability + Send + Sync
{
    fn key(&self) -> &DeviceKey;
    fn capabilities(&self) -> CapabilitySet;
    async fn read_battery(&mut self) -> Result<BatteryReport>;
    /// Re-read slow-changing state after the flyout opens or the machine resumes.
    async fn refresh(&mut self) -> Result<()>;
}
