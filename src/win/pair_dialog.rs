//! The pairing dialog: a box for the address of a remote daemon and a box
//! for the pairing code. What may be typed, and what OK needs, is decided by
//! `remote::pairing` and `pair_form`; this module only puts it on the screen.
//!
//! The dialog is modal and runs its own message loop, so it is shown only
//! while the app is not borrowed, like a menu or a message box.

use std::cell::{Cell, RefCell};

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, SetFocus};
use windows::Win32::UI::WindowsAndMessaging::{
    CallWindowProcW, DLGTEMPLATE, DialogBoxIndirectParamW, EN_CHANGE, EndDialog, GWLP_USERDATA,
    GWLP_WNDPROC, GetDlgItem, GetDlgItemTextW, GetParent, GetWindowLongPtrW, GetWindowRect,
    SWP_NOSIZE, SWP_NOZORDER, SendDlgItemMessageW, SetDlgItemTextW, SetForegroundWindow,
    SetWindowLongPtrW, SetWindowPos, WM_CLOSE, WM_COMMAND, WM_INITDIALOG,
};

use super::util::{hiword, loword, pcwstr, wide};
use crate::pair_form::{
    ADDRESS_LIMIT, CODE_LIMIT, ID_ADDRESS, ID_ADDRESS_NOTE, ID_CANCEL, ID_CODE, ID_OK,
    address_note, template,
};
use crate::remote::pairing::{can_submit, format_pair_code_input};

// Messages of an edit control, from `winuser.h`. The `Win32_UI_Controls`
// feature that holds them is not otherwise needed.
const EM_SETSEL: u32 = 0x00B1;
const EM_LIMITTEXT: u32 = 0x00C5;
const EM_REPLACESEL: u32 = 0x00C2;
const EM_UNDO: u32 = 0x00C7;
const WM_SETTEXT: u32 = 0x000C;
const WM_KEYDOWN: u32 = 0x0100;
const WM_CHAR: u32 = 0x0102;
const WM_CUT: u32 = 0x0300;
const WM_PASTE: u32 = 0x0302;
const WM_CLEAR: u32 = 0x0303;
const WM_UNDO: u32 = 0x0304;

/// What the user typed when they pressed OK.
pub struct Entry {
    pub address: String,
    pub code: String,
}

/// Windows could not make the dialog.
#[derive(Debug, PartialEq, Eq)]
pub struct Failed;

struct Shared {
    prefill: String,
    /// The owner, for the monitor the dialog is placed on.
    owner: HWND,
    /// Set while the code box is rewritten, so that its own change is not
    /// formatted again.
    formatting: Cell<bool>,
    /// Set while the code box handles a message that changes its text. The
    /// box can send `EN_CHANGE` in the middle of it (a paste over a selection
    /// first deletes the selection, and tells), and formatting then would
    /// work on a text that is not yet the whole result.
    editing: Cell<bool>,
    entry: RefCell<Option<Entry>>,
}

/// Shows the dialog over `owner` and waits for it to close. `None` is a
/// dialog that was cancelled. The boxes are emptied before the dialog goes.
/// That only one dialog is open at a time is the caller's to see to: the
/// dialog disables the window, but the tray icon still reaches it.
pub fn run(owner: HWND, prefill: Option<&str>) -> Result<Option<Entry>, Failed> {
    // A template must start at a multiple of four bytes, which a `Vec<u8>`
    // does not promise.
    let bytes = template();
    let mut words = vec![0u32; bytes.len().div_ceil(4)];
    for (index, byte) in bytes.iter().enumerate() {
        words[index / 4] |= u32::from(*byte) << (8 * (index % 4));
    }
    let shared = Shared {
        prefill: prefill.unwrap_or_default().to_owned(),
        owner,
        formatting: Cell::new(false),
        editing: Cell::new(false),
        entry: RefCell::new(None),
    };
    let ended = unsafe {
        let module = GetModuleHandleW(None).map_err(|_| Failed)?;
        DialogBoxIndirectParamW(
            Some(module.into()),
            words.as_ptr().cast::<DLGTEMPLATE>(),
            Some(owner),
            Some(dialog_proc),
            LPARAM(&shared as *const Shared as isize),
        )
    };
    // `-1` is a dialog that could not be made. Anything else, `0` for a
    // cancel and `1` for OK, leaves the answer in `shared`.
    if ended == -1 {
        return Err(Failed);
    }
    Ok(shared.entry.into_inner())
}

fn shared_of(hwnd: HWND) -> Option<&'static Shared> {
    // The pointer is set in `WM_INITDIALOG` to a `Shared` that outlives the
    // dialog, and is never written through: the fields that change are cells.
    let pointer = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *const Shared;
    unsafe { pointer.as_ref() }
}

unsafe extern "system" fn dialog_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> isize {
    match message {
        WM_INITDIALOG => {
            unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, lparam.0) };
            let Some(shared) = shared_of(hwnd) else {
                return 0;
            };
            initialize(hwnd, shared);
            // The focus is set here, so the dialog manager must not.
            0
        }
        WM_COMMAND => {
            // Messages that come before `WM_INITDIALOG` have no state to act on.
            let Some(shared) = shared_of(hwnd) else {
                return 0;
            };
            let id = loword(wparam.0 as isize) as u16;
            let notification = hiword(wparam.0 as isize) as u32;
            match (id, notification) {
                (ID_ADDRESS, EN_CHANGE) => refresh(hwnd),
                (ID_CODE, EN_CHANGE) if shared.editing.get() => refresh(hwnd),
                (ID_CODE, EN_CHANGE) => code_changed(hwnd, shared),
                // `BN_CLICKED`, which is also what Enter and Esc send.
                (ID_OK, 0) => accept(hwnd, shared),
                (ID_CANCEL, 0) => cancel(hwnd),
                _ => return 0,
            }
            1
        }
        WM_CLOSE => {
            cancel(hwnd);
            1
        }
        _ => 0,
    }
}

/// The window procedure of the code box: the edit control's own, and after
/// each message that can change the text, the text is made into the form of a
/// code. `EN_CHANGE` does that too, but a multi-line box does not send it as
/// soon as the text is changed, and what is typed or pasted must not wait for
/// it. The formatting is the same either way, so doing it twice changes
/// nothing.
unsafe extern "system" fn code_box_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // The edit control's own procedure, kept in the user data of the box.
    let original = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) };
    // SAFETY: the value was the window procedure of the box when the dialog
    // was set up, which is a function with this signature.
    let original: windows::Win32::UI::WindowsAndMessaging::WNDPROC =
        unsafe { std::mem::transmute(original) };
    let changes = CHANGES_TEXT.contains(&message);
    let shared = unsafe { GetParent(hwnd) }
        .ok()
        .and_then(|dialog| shared_of(dialog).map(|shared| (dialog, shared)));
    let outer = shared.map(|(_, shared)| shared.editing.replace(changes || shared.editing.get()));
    let result = unsafe { CallWindowProcW(original, hwnd, message, wparam, lparam) };
    if let (Some((dialog, shared)), Some(outer)) = (shared, outer) {
        shared.editing.set(outer);
        // Only the outermost message formats: the text is whole by then.
        if changes && !outer {
            code_changed(dialog, shared);
        }
    }
    result
}

/// The messages after which the text of the code box may be another.
const CHANGES_TEXT: [u32; 9] = [
    WM_SETTEXT,
    WM_KEYDOWN,
    WM_CHAR,
    WM_CUT,
    WM_PASTE,
    WM_CLEAR,
    WM_UNDO,
    EM_REPLACESEL,
    EM_UNDO,
];

fn initialize(hwnd: HWND, shared: &Shared) {
    unsafe {
        SendDlgItemMessageW(
            hwnd,
            ID_ADDRESS.into(),
            EM_LIMITTEXT,
            WPARAM(ADDRESS_LIMIT),
            LPARAM(0),
        );
        SendDlgItemMessageW(
            hwnd,
            ID_CODE.into(),
            EM_LIMITTEXT,
            WPARAM(CODE_LIMIT),
            LPARAM(0),
        );
        let prefill = wide(&shared.prefill);
        let _ = SetDlgItemTextW(hwnd, ID_ADDRESS.into(), pcwstr(&prefill));
        // The code box formats what is typed or pasted as soon as it is.
        if let Ok(code) = GetDlgItem(Some(hwnd), ID_CODE.into()) {
            let original = GetWindowLongPtrW(code, GWLP_WNDPROC);
            SetWindowLongPtrW(code, GWLP_USERDATA, original);
            SetWindowLongPtrW(code, GWLP_WNDPROC, code_box_proc as *const () as isize);
        }
    }
    refresh(hwnd);
    center_on_owner(hwnd, shared.owner);
    unsafe {
        let _ = SetForegroundWindow(hwnd);
        // Typing starts in the box that is still empty.
        let first = if shared.prefill.is_empty() {
            ID_ADDRESS
        } else {
            ID_CODE
        };
        if let Ok(target) = GetDlgItem(Some(hwnd), first.into()) {
            let _ = SetFocus(Some(target));
        }
    }
}

/// `DS_CENTER` centers on the monitor of the cursor or the primary one. The
/// dialog goes to the work area of the monitor the window is on.
fn center_on_owner(hwnd: HWND, owner: HWND) {
    unsafe {
        let monitor = MonitorFromWindow(owner, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        let mut rect = RECT::default();
        if !GetMonitorInfoW(monitor, &mut info).as_bool() || GetWindowRect(hwnd, &mut rect).is_err()
        {
            return;
        }
        let work = info.rcWork;
        let x = work.left + (work.right - work.left - (rect.right - rect.left)) / 2;
        let y = work.top + (work.bottom - work.top - (rect.bottom - rect.top)) / 2;
        let _ = SetWindowPos(hwnd, None, x, y, 0, 0, SWP_NOSIZE | SWP_NOZORDER);
    }
}

fn text_of(hwnd: HWND, id: u16) -> String {
    let mut buffer = [0u16; 512];
    let length = unsafe { GetDlgItemTextW(hwnd, id.into(), &mut buffer) } as usize;
    String::from_utf16_lossy(&buffer[..length.min(buffer.len())])
}

fn set_text(hwnd: HWND, id: u16, text: &str) {
    let text = wide(text);
    unsafe {
        let _ = SetDlgItemTextW(hwnd, id.into(), pcwstr(&text));
    }
}

/// The line under the address, and whether OK may be pressed.
fn refresh(hwnd: HWND) {
    let address = text_of(hwnd, ID_ADDRESS);
    let code = text_of(hwnd, ID_CODE);
    set_text(hwnd, ID_ADDRESS_NOTE, &address_note(&address));
    unsafe {
        if let Ok(ok) = GetDlgItem(Some(hwnd), ID_OK.into()) {
            let _ = EnableWindow(ok, can_submit(&address, &code));
        }
    }
}

/// Makes what was typed or pasted in the code box into the form of a code.
fn code_changed(hwnd: HWND, shared: &Shared) {
    if shared.formatting.get() {
        return;
    }
    let typed = text_of(hwnd, ID_CODE);
    let formatted = format_pair_code_input(&typed);
    if formatted != typed {
        shared.formatting.set(true);
        set_text(hwnd, ID_CODE, &formatted);
        // The caret goes to the end of the new text.
        let end = formatted.len();
        unsafe {
            SendDlgItemMessageW(
                hwnd,
                ID_CODE.into(),
                EM_SETSEL,
                WPARAM(end),
                LPARAM(end as isize),
            );
        }
        shared.formatting.set(false);
    }
    refresh(hwnd);
}

fn accept(hwnd: HWND, shared: &Shared) {
    let address = text_of(hwnd, ID_ADDRESS);
    let code = text_of(hwnd, ID_CODE);
    // The default button is disabled until the input is good, but Enter can
    // still reach here: the check is made again.
    if !can_submit(&address, &code) {
        return;
    }
    *shared.entry.borrow_mut() = Some(Entry { address, code });
    clear(hwnd, shared);
    unsafe {
        let _ = EndDialog(hwnd, 1);
    }
}

fn cancel(hwnd: HWND) {
    if let Some(shared) = shared_of(hwnd) {
        clear(hwnd, shared);
    }
    unsafe {
        let _ = EndDialog(hwnd, 0);
    }
}

/// The code is a secret for five minutes: it does not stay in a box that is
/// going away.
fn clear(hwnd: HWND, shared: &Shared) {
    shared.formatting.set(true);
    set_text(hwnd, ID_CODE, "");
    set_text(hwnd, ID_ADDRESS, "");
    shared.formatting.set(false);
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::thread;
    use std::time::{Duration, Instant};

    use windows::Win32::UI::Input::KeyboardAndMouse::IsWindowEnabled;
    use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, PostMessageW};
    use windows::core::PCWSTR;

    use super::*;
    use crate::pair_form::TITLE;

    /// The dialog is one per process, so the tests take turns.
    static TURN: Mutex<()> = Mutex::new(());

    /// The dialog is driven from another thread, the way a user would from
    /// the keyboard: the dialog runs on the thread of the test.
    fn find_dialog() -> HWND {
        let title = wide(TITLE);
        let started = Instant::now();
        loop {
            let found = unsafe { FindWindowW(PCWSTR::null(), pcwstr(&title)) };
            if let Ok(hwnd) = found {
                return hwnd;
            }
            assert!(started.elapsed() < Duration::from_secs(10), "no dialog");
            thread::sleep(Duration::from_millis(20));
        }
    }

    fn enabled(hwnd: HWND, id: u16) -> bool {
        let item = unsafe { GetDlgItem(Some(hwnd), id.into()) }.unwrap();
        unsafe { IsWindowEnabled(item) }.as_bool()
    }

    /// What the box takes at most, and where the caret is.
    fn limit_of(hwnd: HWND, id: u16) -> usize {
        const EM_GETLIMITTEXT: u32 = 0x00D5;
        unsafe { SendDlgItemMessageW(hwnd, id.into(), EM_GETLIMITTEXT, WPARAM(0), LPARAM(0)) }.0
            as usize
    }

    /// The end of the selection, which is the caret when nothing is selected.
    fn caret_of(hwnd: HWND, id: u16) -> usize {
        const EM_GETSEL: u32 = 0x00B0;
        let packed =
            unsafe { SendDlgItemMessageW(hwnd, id.into(), EM_GETSEL, WPARAM(0), LPARAM(0)) }.0;
        (packed >> 16) as usize & 0xFFFF
    }

    /// Types one character into the box, as the keyboard does.
    fn type_key(hwnd: HWND, id: u16, key: char) {
        unsafe {
            SendDlgItemMessageW(hwnd, id.into(), WM_CHAR, WPARAM(key as usize), LPARAM(0));
        }
    }

    /// Selects `from..to` in the box.
    fn select(hwnd: HWND, id: u16, from: usize, to: usize) {
        unsafe {
            SendDlgItemMessageW(
                hwnd,
                id.into(),
                EM_SETSEL,
                WPARAM(from),
                LPARAM(to as isize),
            );
        }
    }

    /// Pastes `text` from the clipboard into the box, as Ctrl+V does.
    fn paste(hwnd: HWND, id: u16, text: &str) {
        crate::win::clipboard::set_text(HWND::default(), text).unwrap();
        unsafe {
            SendDlgItemMessageW(hwnd, id.into(), WM_PASTE, WPARAM(0), LPARAM(0));
        }
    }

    fn click(hwnd: HWND, id: u16) {
        unsafe {
            PostMessageW(Some(hwnd), WM_COMMAND, WPARAM(usize::from(id)), LPARAM(0)).unwrap();
        }
    }

    // The values are in `Send` form: a raw handle crosses threads, an `HWND`
    // does not.
    /// Closes the dialog when the script ends, also when an assertion in it
    /// failed: the thread that shows the dialog waits for it, and would
    /// never report the failure.
    struct CloseOnDrop(HWND);

    impl Drop for CloseOnDrop {
        fn drop(&mut self) {
            // The dialog is gone already when the script closed it.
            unsafe {
                let _ = PostMessageW(Some(self.0), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
        }
    }

    fn drive(script: impl FnOnce(HWND) + Send + 'static) -> thread::JoinHandle<()> {
        thread::spawn(move || {
            let dialog = find_dialog();
            let _close = CloseOnDrop(dialog);
            script(dialog)
        })
    }

    #[test]
    fn a_dialog_formats_the_code_and_takes_only_a_whole_one() {
        let _turn = TURN.lock().unwrap_or_else(|e| e.into_inner());
        let driver = drive(|dialog| {
            // The address was filled in, so the code is what is missing.
            assert_eq!(text_of(dialog, ID_ADDRESS), "center:7878");
            assert_eq!(text_of(dialog, ID_ADDRESS_NOTE), "");
            assert!(!enabled(dialog, ID_OK));
            assert_eq!(limit_of(dialog, ID_ADDRESS), ADDRESS_LIMIT);
            assert_eq!(limit_of(dialog, ID_CODE), CODE_LIMIT);
            // Typed and pasted codes are made into the form of a code.
            set_text(dialog, ID_CODE, "abc");
            assert_eq!(text_of(dialog, ID_CODE), "ABC");
            assert!(!enabled(dialog, ID_OK));
            set_text(dialog, ID_CODE, "abcd");
            assert_eq!(text_of(dialog, ID_CODE), "ABC-D");
            // The caret is after the text that was written back.
            assert_eq!(caret_of(dialog, ID_CODE), 5);
            set_text(dialog, ID_CODE, "a0b1c2");
            assert_eq!(text_of(dialog, ID_CODE), "ABC-2");
            set_text(dialog, ID_CODE, " abc-def\r\n");
            assert_eq!(text_of(dialog, ID_CODE), "ABC-DEF");
            // A real paste, with a line break in front of the code and one
            // inside it, which a single-line box would cut short.
            for pasted in ["\r\nABC-DEF\r\n", "ABC\r\nDEF", "\r\n\r\nabcdef"] {
                set_text(dialog, ID_CODE, "");
                paste(dialog, ID_CODE, pasted);
                assert_eq!(text_of(dialog, ID_CODE), "ABC-DEF", "{pasted:?}");
            }
            assert!(enabled(dialog, ID_OK));
            // Typed key by key.
            set_text(dialog, ID_CODE, "");
            for key in "abcd".chars() {
                type_key(dialog, ID_CODE, key);
            }
            assert_eq!(text_of(dialog, ID_CODE), "ABC-D");
            assert_eq!(caret_of(dialog, ID_CODE), 5);
            // A paste replaces the selection, and the whole text is formatted
            // again, with the caret after it.
            set_text(dialog, ID_CODE, "abcdef");
            select(dialog, ID_CODE, 0, 1);
            paste(dialog, ID_CODE, "x");
            assert_eq!(text_of(dialog, ID_CODE), "XBC-DEF");
            assert_eq!(caret_of(dialog, ID_CODE), 7);
            // The box takes no more than its limit, whatever the paste is:
            // a code after that many spaces is not taken.
            set_text(dialog, ID_CODE, "");
            paste(
                dialog,
                ID_CODE,
                &format!("{}ABC-DEF", " ".repeat(CODE_LIMIT)),
            );
            assert_eq!(text_of(dialog, ID_CODE), "");
            assert!(!enabled(dialog, ID_OK));
            set_text(dialog, ID_CODE, "abcdef");
            assert!(enabled(dialog, ID_OK));
            // An address that is not taken says why, and holds OK back.
            set_text(dialog, ID_ADDRESS, "8.8.8.8:80");
            assert!(text_of(dialog, ID_ADDRESS_NOTE).contains("not a loopback"));
            assert!(!enabled(dialog, ID_OK));
            set_text(dialog, ID_ADDRESS, "");
            assert_eq!(text_of(dialog, ID_ADDRESS_NOTE), "");
            assert!(!enabled(dialog, ID_OK));
            set_text(dialog, ID_ADDRESS, "Center:7878");
            assert!(enabled(dialog, ID_OK));
            click(dialog, ID_OK);
        });
        let shown = run(HWND::default(), Some("center:7878"));
        // The failure of the script is the one to report.
        driver.join().unwrap();
        let entry = shown.unwrap().unwrap();
        assert_eq!(entry.address, "Center:7878");
        assert_eq!(entry.code, "ABC-DEF");
    }

    #[test]
    fn ok_does_nothing_until_the_input_is_good_and_cancel_sends_nothing() {
        let _turn = TURN.lock().unwrap_or_else(|e| e.into_inner());
        let driver = drive(|dialog| {
            // Without a prefill the address is what is missing.
            assert_eq!(text_of(dialog, ID_ADDRESS), "");
            set_text(dialog, ID_CODE, "abcdef");
            assert!(!enabled(dialog, ID_OK));
            // Enter reaches OK even when the button is held back: nothing
            // happens, and the dialog stays.
            click(dialog, ID_OK);
            thread::sleep(Duration::from_millis(300));
            assert_eq!(text_of(dialog, ID_CODE), "ABC-DEF");
            // Close is Cancel.
            unsafe {
                PostMessageW(Some(dialog), WM_CLOSE, WPARAM(0), LPARAM(0)).unwrap();
            }
        });
        let shown = run(HWND::default(), None);
        driver.join().unwrap();
        assert!(shown.unwrap().is_none());
        // The dialog is closed, so another may open.
        let driver = drive(|dialog| click(dialog, ID_CANCEL));
        let shown = run(HWND::default(), None);
        driver.join().unwrap();
        assert!(shown.unwrap().is_none());
    }
}
