//! The registry: the system's colour setting and the sign-in start entry.

use std::ffi::c_void;

use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_SZ, RRF_RT_REG_DWORD, RegCloseKey,
    RegDeleteValueW, RegGetValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
};
use windows::core::{Error, Result};

use super::util::{pcwstr, wide};
use crate::contract::{RUN_KEY, RUN_VALUE, run_value_data};
use crate::theme::Mode;

const PERSONALIZE_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";

fn read_dword(subkey: &str, name: &str) -> Option<u32> {
    let (subkey, name) = (wide(subkey), wide(name));
    let mut data = 0_u32;
    let mut size = size_of::<u32>() as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            pcwstr(&subkey),
            pcwstr(&name),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut data as *mut u32 as *mut c_void),
            Some(&mut size),
        )
    };
    (status == ERROR_SUCCESS).then_some(data)
}

/// The colour mode apps are asked to use.
pub fn apps_mode() -> Mode {
    Mode::from_apps_use_light_theme(read_dword(PERSONALIZE_KEY, "AppsUseLightTheme"))
}

/// The colour mode of the taskbar and the notification area.
pub fn system_mode() -> Mode {
    Mode::from_apps_use_light_theme(read_dword(PERSONALIZE_KEY, "SystemUsesLightTheme"))
}

struct Key(HKEY);

impl Key {
    fn open_run(access: windows::Win32::System::Registry::REG_SAM_FLAGS) -> Result<Self> {
        let path = wide(RUN_KEY);
        let mut key = HKEY::default();
        unsafe {
            RegOpenKeyExW(HKEY_CURRENT_USER, pcwstr(&path), None, access, &mut key).ok()?;
        }
        Ok(Self(key))
    }
}

impl Drop for Key {
    fn drop(&mut self) {
        unsafe {
            let _ = RegCloseKey(self.0);
        }
    }
}

/// Whether the sign-in start entry exists. The registry is the only record.
pub fn autostart_enabled() -> bool {
    let Ok(key) = Key::open_run(KEY_QUERY_VALUE) else {
        return false;
    };
    let name = wide(RUN_VALUE);
    unsafe { RegQueryValueExW(key.0, pcwstr(&name), None, None, None, None) == ERROR_SUCCESS }
}

pub fn set_autostart(enable: bool) -> Result<()> {
    let key = Key::open_run(KEY_SET_VALUE)?;
    let name = wide(RUN_VALUE);
    unsafe {
        if enable {
            let exe = std::env::current_exe().map_err(|_| Error::empty())?;
            let data = wide(&run_value_data(&exe));
            let bytes = std::slice::from_raw_parts(data.as_ptr().cast::<u8>(), data.len() * 2);
            RegSetValueExW(key.0, pcwstr(&name), None, REG_SZ, Some(bytes)).ok()
        } else {
            let status = RegDeleteValueW(key.0, pcwstr(&name));
            // A value that is not there is already off.
            if status == ERROR_SUCCESS || status == windows::Win32::Foundation::ERROR_FILE_NOT_FOUND
            {
                Ok(())
            } else {
                status.ok()
            }
        }
    }
}
