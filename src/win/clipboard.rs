//! Putting text on the clipboard.

use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL, HWND};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
use windows::core::Result;

use super::util::wide;

/// `CF_UNICODETEXT`.
const UNICODE_TEXT: u32 = 13;

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
