use akku_core::{
    BatteryDescriptor, BatteryEncoding, BatteryReport, DpiDescriptor, DpiEncoding, DriverError,
    PollingDescriptor, Result,
};

fn read_uint_le(bytes: &[u8], offset: usize, width: usize) -> Result<u32> {
    if width == 0 || width > 4 || offset + width > bytes.len() {
        return Err(DriverError::Protocol(format!(
            "field at {offset} with width {width} does not fit {} bytes",
            bytes.len()
        )));
    }
    let mut value = 0u32;
    for (shift, byte) in bytes[offset..offset + width].iter().enumerate() {
        value |= u32::from(*byte) << (shift * 8);
    }
    Ok(value)
}

fn write_uint_le(bytes: &mut [u8], offset: usize, width: usize, value: u32) -> Result<()> {
    if width == 0 || width > 4 || offset + width > bytes.len() {
        return Err(DriverError::Protocol(
            "write field does not fit report".into(),
        ));
    }
    for index in 0..width {
        bytes[offset + index] = ((value >> (index * 8)) & 0xFF) as u8;
    }
    Ok(())
}

pub fn decode_battery(bytes: &[u8], descriptor: &BatteryDescriptor) -> Result<BatteryReport> {
    if descriptor.report_id != 0 && bytes.first().copied() != Some(descriptor.report_id) {
        return Err(DriverError::Protocol(format!(
            "expected report id {:#04X}, got {:?}",
            descriptor.report_id,
            bytes.first()
        )));
    }
    let raw = read_uint_le(bytes, descriptor.offset, descriptor.width)?;
    let percent = match descriptor.encoding {
        BatteryEncoding::Percent => Some(raw.min(100) as u8),
        BatteryEncoding::PercentInverted => Some(100u32.saturating_sub(raw.min(100)) as u8),
        BatteryEncoding::VoltageMv => {
            // Most wireless peripherals expose a 1S cell. This is a display
            // estimate only; the raw voltage is always retained.
            let voltage = raw as f32;
            let percent = ((voltage - 3000.0) / 1200.0 * 100.0).clamp(0.0, 100.0);
            Some(percent.round() as u8)
        }
    };
    let voltage_mv = (descriptor.encoding == BatteryEncoding::VoltageMv)
        .then_some(raw.min(u32::from(u16::MAX)) as u16);
    let charging = descriptor
        .charging_offset
        .and_then(|offset| bytes.get(offset))
        .is_some_and(|value| *value != 0);
    Ok(BatteryReport {
        percent,
        voltage_mv,
        charging,
        raw: bytes.to_vec(),
    })
}

pub fn decode_dpi(bytes: &[u8], descriptor: &DpiDescriptor) -> Result<u16> {
    if descriptor.report_id != 0 && bytes.first().copied() != Some(descriptor.report_id) {
        return Err(DriverError::Protocol("unexpected DPI report id".into()));
    }
    let value =
        match descriptor.encoding {
            DpiEncoding::IndexU8 => {
                let index = *bytes
                    .get(descriptor.offset)
                    .ok_or_else(|| DriverError::Protocol("DPI index missing".into()))?
                    as usize;
                descriptor.values.get(index).copied().ok_or_else(|| {
                    DriverError::Protocol(format!("DPI index {index} out of range"))
                })?
            }
            DpiEncoding::U16Le => read_uint_le(bytes, descriptor.offset, 2)? as u16,
        };
    Ok(nearest(&descriptor.values, value))
}

pub fn decode_polling(bytes: &[u8], descriptor: &PollingDescriptor) -> Result<u16> {
    if descriptor.report_id != 0 && bytes.first().copied() != Some(descriptor.report_id) {
        return Err(DriverError::Protocol("unexpected polling report id".into()));
    }
    let value = read_uint_le(bytes, descriptor.offset, 2)? as u16;
    Ok(nearest(&descriptor.rates, value))
}

pub fn decode_actuation(bytes: &[u8], descriptor: &akku_core::ActuationDescriptor) -> Result<f32> {
    if descriptor.report_id != 0 && bytes.first().copied() != Some(descriptor.report_id) {
        return Err(DriverError::Protocol(
            "unexpected actuation report id".into(),
        ));
    }
    let hundredths = read_uint_le(bytes, descriptor.offset, 2)? as f32;
    Ok((hundredths / 100.0).clamp(descriptor.min_mm, descriptor.max_mm))
}

pub fn encode_dpi(buffer: &mut [u8], descriptor: &DpiDescriptor, dpi: u16) -> Result<()> {
    let (index, value) = descriptor
        .values
        .iter()
        .copied()
        .enumerate()
        .find(|(_, value)| *value == dpi)
        .ok_or_else(|| DriverError::InvalidArgument(format!("{dpi} DPI is not supported")))?;
    buffer[0] = descriptor.report_id;
    match descriptor.encoding {
        DpiEncoding::IndexU8 => {
            *buffer
                .get_mut(descriptor.offset)
                .ok_or_else(|| DriverError::Protocol("DPI write offset is invalid".into()))? =
                index as u8;
        }
        DpiEncoding::U16Le => write_uint_le(buffer, descriptor.offset, 2, u32::from(value))?,
    }
    Ok(())
}

pub fn encode_polling(buffer: &mut [u8], descriptor: &PollingDescriptor, hz: u16) -> Result<()> {
    if !descriptor.rates.contains(&hz) {
        return Err(DriverError::InvalidArgument(format!(
            "{hz} Hz is not supported"
        )));
    }
    buffer[0] = descriptor.report_id;
    write_uint_le(buffer, descriptor.offset, 2, u32::from(hz))
}

pub fn encode_actuation(
    buffer: &mut [u8],
    descriptor: &akku_core::ActuationDescriptor,
    millimetres: f32,
) -> Result<()> {
    if !millimetres.is_finite() {
        return Err(DriverError::InvalidArgument(
            "actuation must be finite".into(),
        ));
    }
    let bounded = millimetres.clamp(descriptor.min_mm, descriptor.max_mm);
    buffer[0] = descriptor.report_id;
    write_uint_le(
        buffer,
        descriptor.offset,
        2,
        (bounded * 100.0).round() as u32,
    )
}

fn nearest(values: &[u16], value: u16) -> u16 {
    values
        .iter()
        .copied()
        .min_by_key(|candidate| candidate.abs_diff(value))
        .unwrap_or(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use akku_core::{ActuationDescriptor, BatteryDescriptor};

    fn battery_descriptor() -> BatteryDescriptor {
        BatteryDescriptor {
            report_id: 0x05,
            report_len: 65,
            offset: 1,
            width: 1,
            encoding: BatteryEncoding::Percent,
            charging_offset: None,
        }
    }

    #[test]
    fn decodes_percent_and_charging() {
        let mut descriptor = battery_descriptor();
        descriptor.charging_offset = Some(2);
        let report = [0x05, 87, 1, 0];
        let battery = decode_battery(&report, &descriptor).unwrap();
        assert_eq!(battery.percent, Some(87));
        assert!(battery.charging);
    }

    #[test]
    fn rejects_missing_report_id() {
        assert!(decode_battery(&[0x04, 80], &battery_descriptor()).is_err());
    }

    #[test]
    fn decodes_index_dpi() {
        let descriptor = DpiDescriptor {
            report_id: 5,
            report_len: 65,
            offset: 2,
            encoding: DpiEncoding::IndexU8,
            values: vec![400, 800, 1600],
        };
        assert_eq!(decode_dpi(&[5, 80, 2], &descriptor).unwrap(), 1600);
    }

    #[test]
    fn encodes_polling() {
        let descriptor = PollingDescriptor {
            report_id: 5,
            report_len: 65,
            offset: 3,
            rates: vec![125, 1000],
        };
        let mut buffer = [0u8; 65];
        encode_polling(&mut buffer, &descriptor, 1000).unwrap();
        assert_eq!(&buffer[..5], &[5, 0, 0, 0xE8, 0x03]);
        assert!(encode_polling(&mut buffer, &descriptor, 500).is_err());
    }

    #[test]
    fn encodes_actuation_hundredths() {
        let descriptor = ActuationDescriptor {
            report_id: 0,
            report_len: 65,
            offset: 2,
            min_mm: 0.1,
            max_mm: 4.0,
            step_mm: 0.1,
        };
        let mut buffer = [0u8; 65];
        encode_actuation(&mut buffer, &descriptor, 2.34).unwrap();
        assert_eq!(decode_actuation(&buffer, &descriptor).unwrap(), 2.34);
    }
}
