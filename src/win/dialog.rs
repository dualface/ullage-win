//! Message boxes. They run their own message loop, so they are only shown
//! while the app is not borrowed.

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    IDOK, MB_ICONINFORMATION, MB_ICONQUESTION, MB_OK, MB_OKCANCEL, MB_SETFOREGROUND, MessageBoxW,
};

use super::util::{pcwstr, wide};
use crate::remote::address::RemoteAddress;
use crate::remote::pairing::PairCode;

/// What the app asks the window procedure to show.
pub enum Dialog {
    Notice(String),
    /// Asks before a pairing code is sent to `address`.
    ConfirmPair {
        address: RemoteAddress,
        code: PairCode,
        text: String,
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

/// Whether the user chose OK.
pub fn confirm(owner: HWND, text: &str) -> bool {
    let text = wide(text);
    let caption = wide(CAPTION);
    unsafe {
        MessageBoxW(
            Some(owner),
            pcwstr(&text),
            pcwstr(&caption),
            MB_OKCANCEL | MB_ICONQUESTION | MB_SETFOREGROUND,
        ) == IDOK
    }
}
