//! Small helpers shared by the Windows modules.

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;
use windows::core::PCWSTR;

/// Posts `message` to the window whose handle value is `target`. A raw
/// handle value can cross threads; an `HWND` cannot.
pub fn post(target: isize, message: u32) {
    unsafe {
        let _ = PostMessageW(Some(HWND(target as *mut _)), message, WPARAM(0), LPARAM(0));
    }
}

/// UTF-16 with a closing NUL, for the wide Win32 functions.
pub fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn pcwstr(text: &[u16]) -> PCWSTR {
    PCWSTR(text.as_ptr())
}

pub fn loword(value: isize) -> i32 {
    i32::from(value as u16)
}

pub fn hiword(value: isize) -> i32 {
    i32::from((value >> 16) as u16)
}

/// The signed x coordinate packed in the low word of an `lparam`.
pub fn x_of(lparam: isize) -> i32 {
    i32::from(lparam as i16)
}

/// The signed y coordinate packed in the high word of an `lparam`.
pub fn y_of(lparam: isize) -> i32 {
    i32::from((lparam >> 16) as i16)
}
