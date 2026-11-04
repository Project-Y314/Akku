//! Autostart enablement for the current Windows user via
//! `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`.

const VALUE_NAME: &str = "Akku";
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

/// Quotes the executable path so `cmd`-style parsing survives spaces.
fn command_line() -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|error| format!("current_exe: {error}"))?;
    Ok(format!("\"{}\"", exe.display()))
}

#[cfg(windows)]
mod imp {
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegSetValueExW, HKEY_CURRENT_USER,
        KEY_SET_VALUE, REG_SZ,
    };

    use super::{RUN_KEY, VALUE_NAME};

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn open_r_key(access: u32) -> Result<*mut std::ffi::c_void, String> {
        let mut key: *mut std::ffi::c_void = std::ptr::null_mut();
        let status = unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                wide(RUN_KEY).as_ptr(),
                0,
                access,
                &mut key,
            )
        };
        if status != 0 {
            Err(format!("cannot open the HKCU Run key (status {status})"))
        } else {
            Ok(key)
        }
    }

    pub fn set_enabled(enabled: bool) -> Result<(), String> {
        let key = open_r_key(KEY_SET_VALUE)?;
        let outcome = if enabled {
            let command = super::command_line()?;
            let data = wide(&command);
            let status = unsafe {
                RegSetValueExW(
                    key,
                    wide(VALUE_NAME).as_ptr(),
                    0,
                    REG_SZ,
                    data.as_ptr().cast(),
                    (data.len() as u32) * 2,
                )
            };
            if status == 0 {
                Ok(())
            } else {
                Err(format!(
                    "cannot write the autostart value (status {status})"
                ))
            }
        } else {
            let status = unsafe { RegDeleteValueW(key, wide(VALUE_NAME).as_ptr()) };
            // ERROR_FILE_NOT_FOUND (2): already disabled, which is the goal.
            if status == 0 || status == 2 {
                Ok(())
            } else {
                Err(format!(
                    "cannot delete the autostart value (status {status})"
                ))
            }
        };
        unsafe { RegCloseKey(key) };
        outcome
    }
}

#[cfg(windows)]
pub use imp::set_enabled;

#[cfg(not(windows))]
pub fn set_enabled(_enabled: bool) -> Result<(), String> {
    Err("autostart is Windows-only".into())
}
