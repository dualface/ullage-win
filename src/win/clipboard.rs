//! Putting text on the clipboard.

use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL, HWND};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    SetClipboardData,
};
use windows::Win32::System::Memory::{
    GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock,
};
use windows::core::Result;

use super::util::wide;

/// `CF_UNICODETEXT`.
const UNICODE_TEXT: u32 = 13;

/// The most of the clipboard's text that is read: far more than a pairing
/// needs, and a bound on what anything else on the clipboard can cost.
const MAX_READ_UNITS: usize = 1024;

/// The text on the clipboard, or `None` when there is none. At most
/// [`MAX_READ_UNITS`] UTF-16 units are read.
pub fn get_text(owner: HWND) -> Option<String> {
    unsafe {
        IsClipboardFormatAvailable(UNICODE_TEXT).ok()?;
        OpenClipboard(Some(owner)).ok()?;
        let text = (|| {
            let handle = GetClipboardData(UNICODE_TEXT).ok()?;
            let memory = HGLOBAL(handle.0);
            let held = GlobalSize(memory) / size_of::<u16>();
            let units = held.min(MAX_READ_UNITS);
            let source = GlobalLock(memory).cast::<u16>();
            if source.is_null() {
                return None;
            }
            let all = std::slice::from_raw_parts(source, units);
            // Text that goes on past what is read is not read at all: its
            // front part could pass for a complete input.
            let text = match all.iter().position(|unit| *unit == 0) {
                Some(end) => Some(String::from_utf16_lossy(&all[..end])),
                None if held > units => None,
                None => Some(String::from_utf16_lossy(all)),
            };
            // The text is copied: the memory may be let go of now.
            let _ = GlobalUnlock(memory);
            text
        })();
        let _ = CloseClipboard();
        text
    }
}

pub fn set_text(owner: HWND, text: &str) -> Result<()> {
    let data = wide(text);
    unsafe {
        OpenClipboard(Some(owner))?;
        let result = (|| {
            EmptyClipboard()?;
            let bytes = data.len() * size_of::<u16>();
            let memory: HGLOBAL = GlobalAlloc(GMEM_MOVEABLE, bytes)?;
            let target = GlobalLock(memory);
            if target.is_null() {
                let _ = GlobalFree(Some(memory));
                return Err(windows::core::Error::from_thread());
            }
            std::ptr::copy_nonoverlapping(data.as_ptr(), target.cast::<u16>(), data.len());
            let _ = GlobalUnlock(memory);
            // The clipboard owns the memory once this succeeds.
            if let Err(error) = SetClipboardData(UNICODE_TEXT, Some(HANDLE(memory.0))) {
                let _ = GlobalFree(Some(memory));
                return Err(error);
            }
            Ok(())
        })();
        let _ = CloseClipboard();
        result
    }
}
