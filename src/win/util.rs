//! Small helpers shared by the Windows modules.

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, HICON, IMAGE_ICON, LR_SHARED, LoadImageW, PostMessageW, SM_CXICON,
    SM_CXSMICON, SM_CYICON, SM_CYSMICON,
};
use windows::core::PCWSTR;

/// The id of the app icon in the resources of the program, as
/// `packaging/windows/app-icon.rc` numbers it.
const APP_ICON_ID: usize = 1;

/// The app icon at the size of a small icon (title bars) or a large one
/// (Alt+Tab). It is loaded shared, so it is never destroyed. `None` when the
/// program was built without it.
pub fn app_icon(small: bool) -> Option<HICON> {
    let (cx, cy) = if small {
        (SM_CXSMICON, SM_CYSMICON)
    } else {
        (SM_CXICON, SM_CYICON)
    };
    unsafe {
        let module = GetModuleHandleW(None).ok()?;
        let icon = LoadImageW(
            Some(module.into()),
            // MAKEINTRESOURCE: a resource id in place of a name.
            PCWSTR(APP_ICON_ID as *const u16),
            IMAGE_ICON,
            GetSystemMetrics(cx),
            GetSystemMetrics(cy),
            LR_SHARED,
        )
        .ok()?;
        Some(HICON(icon.0))
    }
}

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
