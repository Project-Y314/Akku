use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisplayMode {
    pub width: u32,
    pub height: u32,
    pub refresh_hz: u32,
}

impl fmt::Display for DisplayMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}x{} @ {} Hz", self.width, self.height, self.refresh_hz)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonitorInfo {
    /// `\\.\DISPLAY1`, used as the stable key for mode changes.
    pub id: String,
    pub label: String,
    pub adapter: String,
    pub current: DisplayMode,
    pub modes: Vec<DisplayMode>,
}

#[derive(Debug, Error)]
pub enum DisplayError {
    #[error("monitor not found: {0}")]
    NotFound(String),
    #[error("mode is not supported: {0}")]
    UnsupportedMode(String),
    #[error("display change failed: {0}")]
    ChangeFailed(String),
    #[error("Windows error: {0}")]
    Windows(String),
}

pub type Result<T> = std::result::Result<T, DisplayError>;

/// Deduplicate and sort modes. Kept platform-independent for tests.
pub fn normalize_modes(modes: impl IntoIterator<Item = DisplayMode>) -> Vec<DisplayMode> {
    let mut modes = modes
        .into_iter()
        .filter(|mode| mode.width >= 640 && mode.height >= 480 && mode.refresh_hz > 0)
        .collect::<Vec<_>>();
    modes.sort_by_key(|mode| (mode.width, mode.height, mode.refresh_hz));
    modes.dedup();
    modes
}

/// Choose an exact refresh at the current resolution. The UI only offers modes
/// returned by `list_monitors`, so this is a defensive check for CLI input.
pub fn find_mode(
    modes: &[DisplayMode],
    current: &DisplayMode,
    refresh_hz: u32,
) -> Option<DisplayMode> {
    modes
        .iter()
        .find(|mode| {
            mode.width == current.width
                && mode.height == current.height
                && mode.refresh_hz == refresh_hz
        })
        .cloned()
}

#[cfg(windows)]
mod win {
    use super::*;
    use std::ffi::c_void;
    use windows::Win32::Graphics::Gdi::{
        CDS_TEST, CDS_TYPE, CDS_UPDATEREGISTRY, ChangeDisplaySettingsExW, DEVMODEW,
        DISP_CHANGE_SUCCESSFUL, DISPLAY_DEVICEW, ENUM_CURRENT_SETTINGS, ENUM_DISPLAY_SETTINGS_MODE,
        EnumDisplayDevicesW, EnumDisplaySettingsW,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CMONITORS};
    use windows::core::PCWSTR;

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn device_string(buffer: &[u16]) -> String {
        let len = buffer.iter().position(|c| *c == 0).unwrap_or(buffer.len());
        String::from_utf16_lossy(&buffer[..len])
    }

    fn mode_from_devmode(mode: &DEVMODEW) -> DisplayMode {
        DisplayMode {
            width: mode.dmPelsWidth,
            height: mode.dmPelsHeight,
            refresh_hz: mode.dmDisplayFrequency,
        }
    }

    fn current_devmode(device: &str) -> Result<DEVMODEW> {
        let name = wide(device);
        let mut mode = DEVMODEW {
            dmSize: std::mem::size_of::<DEVMODEW>() as u16,
            ..Default::default()
        };
        let ok = unsafe {
            EnumDisplaySettingsW(PCWSTR(name.as_ptr()), ENUM_CURRENT_SETTINGS, &mut mode)
        };
        if !ok.as_bool() {
            return Err(DisplayError::NotFound(device.to_string()));
        }
        Ok(mode)
    }

    fn modes_for(device: &str) -> Result<Vec<DisplayMode>> {
        let name = wide(device);
        let mut modes = Vec::new();
        for index in 0..512u32 {
            let mut mode = DEVMODEW {
                dmSize: std::mem::size_of::<DEVMODEW>() as u16,
                ..Default::default()
            };
            let ok = unsafe {
                EnumDisplaySettingsW(
                    PCWSTR(name.as_ptr()),
                    ENUM_DISPLAY_SETTINGS_MODE(index),
                    &mut mode,
                )
            };
            if !ok.as_bool() {
                break;
            }
            modes.push(mode_from_devmode(&mode));
        }
        Ok(normalize_modes(modes))
    }

    pub fn list_monitors() -> Result<Vec<MonitorInfo>> {
        let mut output = Vec::new();
        for index in 0..32u32 {
            let mut device = DISPLAY_DEVICEW {
                cb: std::mem::size_of::<DISPLAY_DEVICEW>() as u32,
                ..Default::default()
            };
            let ok = unsafe { EnumDisplayDevicesW(PCWSTR::null(), index, &mut device, 0) };
            if !ok.as_bool() {
                break;
            }
            let id = device_string(&device.DeviceName);
            let adapter = device_string(&device.DeviceString);
            if id.is_empty() {
                continue;
            }
            let modes = modes_for(&id)?;
            if modes.is_empty() {
                continue;
            }
            let current = mode_from_devmode(&current_devmode(&id)?);
            let label = format!("{adapter} ({id})");
            output.push(MonitorInfo {
                id,
                label,
                adapter,
                current,
                modes,
            });
        }
        if output.is_empty() {
            return Err(DisplayError::Windows(format!(
                "no displays reported (GetSystemMetrics(SM_CMONITORS) = {})",
                unsafe { GetSystemMetrics(SM_CMONITORS) }
            )));
        }
        Ok(output)
    }

    pub fn set_refresh(device: &str, refresh_hz: u32, persist: bool) -> Result<DisplayMode> {
        let current = current_devmode(device)?;
        let current_mode = mode_from_devmode(&current);
        let modes = modes_for(device)?;
        let target = find_mode(&modes, &current_mode, refresh_hz)
            .ok_or_else(|| DisplayError::UnsupportedMode(format!("{refresh_hz} Hz")))?;

        let name = wide(device);
        let mut desired = current;
        desired.dmDisplayFrequency = target.refresh_hz;
        desired.dmFields = windows::Win32::Graphics::Gdi::DEVMODE_FIELD_FLAGS(
            desired.dmFields.0 | windows::Win32::Graphics::Gdi::DM_DISPLAYFREQUENCY.0,
        );

        let test_flags = CDS_TYPE(CDS_TEST.0);
        let test = unsafe {
            ChangeDisplaySettingsExW(
                PCWSTR(name.as_ptr()),
                Some(&desired),
                None,
                test_flags,
                None::<*const c_void>,
            )
        };
        if test != DISP_CHANGE_SUCCESSFUL {
            return Err(DisplayError::UnsupportedMode(format!(
                "{} (CDS_TEST returned {:?})",
                target, test
            )));
        }

        let flags = if persist {
            CDS_TYPE(CDS_UPDATEREGISTRY.0)
        } else {
            CDS_TYPE(0)
        };
        let changed = unsafe {
            ChangeDisplaySettingsExW(
                PCWSTR(name.as_ptr()),
                Some(&desired),
                None,
                flags,
                None::<*const c_void>,
            )
        };
        if changed != DISP_CHANGE_SUCCESSFUL {
            return Err(DisplayError::ChangeFailed(format!(
                "{} returned {:?}",
                target, changed
            )));
        }
        Ok(target)
    }
}

#[cfg(windows)]
pub use win::{list_monitors, set_refresh};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deduplicates_and_sorts_modes() {
        let modes = vec![
            DisplayMode {
                width: 1920,
                height: 1080,
                refresh_hz: 144,
            },
            DisplayMode {
                width: 1920,
                height: 1080,
                refresh_hz: 60,
            },
            DisplayMode {
                width: 1920,
                height: 1080,
                refresh_hz: 144,
            },
        ];
        let normalized = normalize_modes(modes);
        assert_eq!(normalized.len(), 2);
        assert_eq!(normalized[0].refresh_hz, 60);
        assert_eq!(normalized[1].refresh_hz, 144);
    }

    #[test]
    fn finds_only_current_resolution() {
        let current = DisplayMode {
            width: 2560,
            height: 1440,
            refresh_hz: 60,
        };
        let modes = vec![
            current.clone(),
            DisplayMode {
                width: 1920,
                height: 1080,
                refresh_hz: 144,
            },
        ];
        assert!(find_mode(&modes, &current, 144).is_none());
    }
}
