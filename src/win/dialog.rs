//! Message boxes. They run their own message loop, so they are only shown
//! while the app is not borrowed.

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    MB_ICONINFORMATION, MB_OK, MB_SETFOREGROUND, MessageBoxW,
};

use super::util::{pcwstr, wide};

/// What the app asks the window procedure to show.
pub enum Dialog {
    Notice(String),
    /// Asks for the address of a remote daemon and a pairing code. The box
    /// for the address starts with `prefill`.
    Pair {
        prefill: Option<String>,
    },
}

const CAPTION: &str = "Ullage";

pub fn notice(owner: HWND, text: &str) {
    let text = wide(text);
    let caption = wide(CAPTION);
    unsafe {
        MessageBoxW(
            Some(owner),
            pcwstr(&text),
            pcwstr(&caption),
            MB_OK | MB_ICONINFORMATION | MB_SETFOREGROUND,
        );
    }
}
