//! One instance per user.

use std::time::Duration;

use windows::Win32::Foundation::{
    CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, HLOCAL, LocalFree,
};
use windows::Win32::Security::Authorization::ConvertSidToStringSidW;
use windows::Win32::Security::{GetTokenInformation, TOKEN_QUERY, TOKEN_USER, TokenUser};
use windows::Win32::System::Threading::{CreateMutexW, GetCurrentProcess, OpenProcessToken};
use windows::Win32::UI::WindowsAndMessaging::{
    AllowSetForegroundWindow, FindWindowW, GetWindowThreadProcessId, PostMessageW,
    RegisterWindowMessageW,
};
use windows::core::{PWSTR, Result};

use super::util::{pcwstr, wide};
use crate::contract::{SHOW_MESSAGE, WINDOW_CLASS, mutex_name};

/// The SID string of the user this process runs as.
fn current_user_sid() -> Result<String> {
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token)?;
        let result = (|| {
            let mut length = 0;
            // The first call only reports the size.
            let _ = GetTokenInformation(token, TokenUser, None, 0, &mut length);
            // u64 elements keep the buffer aligned for TOKEN_USER.
            let mut buffer = vec![0_u64; (length as usize).div_ceil(8).max(1)];
            GetTokenInformation(
                token,
                TokenUser,
                Some(buffer.as_mut_ptr().cast()),
                length,
                &mut length,
            )?;
            let user = &*(buffer.as_ptr().cast::<TOKEN_USER>());
            let mut text = PWSTR::null();
            ConvertSidToStringSidW(user.User.Sid, &mut text)?;
            let sid = text.to_string();
            let _ = LocalFree(Some(HLOCAL(text.0.cast())));
            Ok(sid?)
        })();
        let _ = CloseHandle(token);
        result
    }
}

pub enum Instance {
    /// This process is the first. The mutex lives until the process ends.
    First,
    /// Another process of this user is running.
    Running,
}

pub fn acquire() -> Instance {
    let sid = current_user_sid().unwrap_or_else(|_| "unknown".to_owned());
    let name = wide(&mutex_name(&sid));
    unsafe {
        match CreateMutexW(None, true, pcwstr(&name)) {
            // The handle is kept open on purpose: the mutex goes away with
            // the process.
            Ok(_handle) if GetLastError() != ERROR_ALREADY_EXISTS => Instance::First,
            Ok(handle) => {
                let _ = CloseHandle(handle);
                Instance::Running
            }
            // A mutex that cannot be made must not keep the app from
            // starting.
            Err(_) => Instance::First,
        }
    }
}

/// Asks the running instance to show its window. It may still be starting,
/// so the window is looked for a few times.
pub fn show_running() {
    let class = wide(WINDOW_CLASS);
    let message = wide(SHOW_MESSAGE);
    unsafe {
        let id = RegisterWindowMessageW(pcwstr(&message));
        for _ in 0..30 {
            if let Ok(hwnd) = FindWindowW(pcwstr(&class), None) {
                let mut pid = 0;
                GetWindowThreadProcessId(hwnd, Some(&mut pid));
                let _ = AllowSetForegroundWindow(pid);
                let _ = PostMessageW(Some(hwnd), id, Default::default(), Default::default());
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}
