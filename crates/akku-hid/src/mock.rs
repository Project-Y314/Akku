use akku_core::{
    ActuationCapability, ActuationSpec, BatteryReport, CapabilitySet, DeviceKey, DpiCapability,
    DpiSpec, DriverError, PeripheralDriver, PollingRateCapability, PollingSpec, Result,
};
use async_trait::async_trait;

/// Deterministic in-memory driver used by tests and the `--demo` UI mode.
pub struct MockDriver {
    key: DeviceKey,
    battery: BatteryReport,
    dpi_spec: Option<DpiSpec>,
    dpi: Option<u16>,
    polling_spec: Option<PollingSpec>,
    polling: Option<u16>,
    actuation_spec: Option<ActuationSpec>,
    actuation: Option<f32>,
    writable: bool,
}

impl MockDriver {
    pub fn mouse() -> Self {
        Self {
            key: DeviceKey::new(0x3554, 0xFA09, 0xFFFF, 0x0001),
            battery: BatteryReport::percent(87),
            dpi_spec: Some(DpiSpec::new(vec![400, 800, 1600, 3200])),
            dpi: Some(1600),
            polling_spec: Some(PollingSpec::new(vec![125, 500, 1000])),
            polling: Some(1000),
            actuation_spec: None,
            actuation: None,
            writable: true,
        }
    }

    pub fn keyboard() -> Self {
        Self {
            key: DeviceKey::new(0x320F, 0x5055, 0xFF60, 0x0061),
            battery: BatteryReport::percent(64),
            dpi_spec: None,
            dpi: None,
            polling_spec: Some(PollingSpec::new(vec![125, 250, 500, 1000])),
            polling: Some(1000),
            actuation_spec: Some(ActuationSpec {
                min_mm: 0.1,
                max_mm: 4.0,
                step_mm: 0.1,
            }),
            actuation: Some(2.0),
            writable: true,
        }
    }

    pub fn with_battery(mut self, percent: u8) -> Self {
        self.battery = BatteryReport::percent(percent);
        self
    }
}

#[async_trait]
impl DpiCapability for MockDriver {
    fn dpi_spec(&self) -> Option<DpiSpec> {
        self.dpi_spec.clone()
    }

    async fn read_dpi(&mut self) -> Result<Option<u16>> {
        Ok(self.dpi)
    }

    async fn set_dpi(&mut self, dpi: u16) -> Result<()> {
        if !self.writable {
            return Err(DriverError::Unsupported("mock is read-only"));
        }
        if !self
            .dpi_spec
            .as_ref()
            .is_some_and(|spec| spec.values.contains(&dpi))
        {
            return Err(DriverError::InvalidArgument(format!("{dpi} DPI")));
        }
        self.dpi = Some(dpi);
        Ok(())
    }
}

#[async_trait]
impl PollingRateCapability for MockDriver {
    fn polling_spec(&self) -> Option<PollingSpec> {
        self.polling_spec.clone()
    }

    async fn read_polling_rate(&mut self) -> Result<Option<u16>> {
        Ok(self.polling)
    }

    async fn set_polling_rate(&mut self, hz: u16) -> Result<()> {
        if !self.writable {
            return Err(DriverError::Unsupported("mock is read-only"));
        }
        if !self
            .polling_spec
            .as_ref()
            .is_some_and(|spec| spec.rates.contains(&hz))
        {
            return Err(DriverError::InvalidArgument(format!("{hz} Hz")));
        }
        self.polling = Some(hz);
        Ok(())
    }
}

#[async_trait]
impl ActuationCapability for MockDriver {
    fn actuation_spec(&self) -> Option<ActuationSpec> {
        self.actuation_spec.clone()
    }

    async fn read_actuation(&mut self) -> Result<Option<f32>> {
        Ok(self.actuation)
    }

    async fn set_actuation(&mut self, millimetres: f32) -> Result<()> {
        if !self.writable {
            return Err(DriverError::Unsupported("mock is read-only"));
        }
        let spec = self
            .actuation_spec
            .as_ref()
            .ok_or(DriverError::Unsupported("actuation"))?;
        if !millimetres.is_finite() {
            return Err(DriverError::InvalidArgument("actuation".into()));
        }
        self.actuation = Some(spec.clamp(millimetres));
        Ok(())
    }
}

#[async_trait]
impl PeripheralDriver for MockDriver {
    fn key(&self) -> &DeviceKey {
        &self.key
    }

    fn capabilities(&self) -> CapabilitySet {
        CapabilitySet {
            dpi: self.dpi_spec.is_some(),
            polling_rate: self.polling_spec.is_some(),
            actuation: self.actuation_spec.is_some(),
            macros: false,
        }
    }

    async fn read_battery(&mut self) -> Result<BatteryReport> {
        Ok(self.battery.clone())
    }

    async fn refresh(&mut self) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn reads_and_writes_mock_state() {
        let mut mouse = MockDriver::mouse();
        assert_eq!(mouse.read_battery().await.unwrap().percent, Some(87));
        mouse.set_dpi(800).await.unwrap();
        assert_eq!(mouse.read_dpi().await.unwrap(), Some(800));
        assert!(mouse.set_dpi(1234).await.is_err());
    }

    #[tokio::test]
    async fn keyboard_actuation_is_clamped() {
        let mut keyboard = MockDriver::keyboard();
        keyboard.set_actuation(99.0).await.unwrap();
        assert_eq!(keyboard.read_actuation().await.unwrap(), Some(4.0));
    }
}
