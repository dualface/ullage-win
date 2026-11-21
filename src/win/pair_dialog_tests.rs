//! Tests of the pairing dialog: a real dialog on a thread of its own,
//! driven from another thread the way a user would from the keyboard.

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
    let packed = unsafe { SendDlgItemMessageW(hwnd, id.into(), EM_GETSEL, WPARAM(0), LPARAM(0)) }.0;
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
        assert!(!enabled(dialog, ID_OK));
        set_text(dialog, ID_ADDRESS, "");
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
fn a_second_wm_initdialog_from_anywhere_does_nothing() {
    const WM_INITDIALOG: u32 = 0x0110;
    let _turn = TURN.lock().unwrap_or_else(|e| e.into_inner());
    let driver = drive(|dialog| {
        set_text(dialog, ID_CODE, "abc");
        // Sent as another process could send it, with a parameter that
        // points nowhere. The dialog must neither read it nor start over:
        // starting over would take the box's own replacement procedure
        // for the original one, and every message to the box would then
        // call itself.
        for lparam in [0isize, 1, 0xdead_beef] {
            unsafe {
                SendMessageW(dialog, WM_INITDIALOG, Some(WPARAM(0)), Some(LPARAM(lparam)));
            }
        }
        // The box still works: typed, pasted and set text are formatted.
        assert_eq!(text_of(dialog, ID_CODE), "ABC");
        type_key(dialog, ID_CODE, 'd');
        assert_eq!(text_of(dialog, ID_CODE), "ABC-D");
        paste(dialog, ID_CODE, "ef");
        assert_eq!(text_of(dialog, ID_CODE), "ABC-DEF");
        click(dialog, ID_CANCEL);
    });
    let shown = run(HWND::default(), None);
    driver.join().unwrap();
    assert!(shown.unwrap().is_none());
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

/// Moves the keyboard focus to the control `id`, as Tab does.
fn focus(dialog: HWND, id: u16) {
    const WM_NEXTDLGCTL: u32 = 0x0028;
    let control = unsafe { GetDlgItem(Some(dialog), id.into()) }.unwrap();
    unsafe {
        SendMessageW(
            dialog,
            WM_NEXTDLGCTL,
            Some(WPARAM(control.0 as usize)),
            Some(LPARAM(1)),
        );
    }
}

/// Presses Enter: the key goes through the message loop of the dialog, which
/// gives it to the button that has the focus, or else to the default button.
fn press_enter(dialog: HWND) {
    const VK_RETURN: usize = 0x0D;
    unsafe {
        PostMessageW(Some(dialog), WM_KEYDOWN, WPARAM(VK_RETURN), LPARAM(0)).unwrap();
    }
}

#[test]
fn enter_in_a_box_pairs() {
    let _turn = TURN.lock().unwrap_or_else(|e| e.into_inner());
    let driver = drive(|dialog| {
        set_text(dialog, ID_CODE, "abcdef");
        focus(dialog, ID_CODE);
        press_enter(dialog);
    });
    let shown = run(HWND::default(), Some("100.71.140.68:7878"));
    driver.join().unwrap();
    let entry = shown.unwrap().expect("Enter in a box is the main button");
    assert_eq!(entry.address, "100.71.140.68:7878");
    assert_eq!(entry.code, "ABC-DEF");
}

#[test]
fn enter_on_cancel_cancels_and_does_not_pair() {
    let _turn = TURN.lock().unwrap_or_else(|e| e.into_inner());
    let driver = drive(|dialog| {
        // The input is whole, so that the main button would pair.
        set_text(dialog, ID_CODE, "abcdef");
        assert!(enabled(dialog, ID_OK));
        focus(dialog, ID_CANCEL);
        press_enter(dialog);
    });
    let shown = run(HWND::default(), Some("100.71.140.68:7878"));
    driver.join().unwrap();
    assert!(shown.unwrap().is_none(), "Enter on Cancel is Cancel");
}

#[test]
fn messages_with_parameters_that_point_nowhere_do_nothing() {
    const WM_PRINTCLIENT: u32 = 0x0318;
    let _turn = TURN.lock().unwrap_or_else(|e| e.into_inner());
    let driver = drive(|dialog| {
        set_text(dialog, ID_CODE, "abc");
        let address = unsafe { GetDlgItem(Some(dialog), ID_ADDRESS.into()) }.unwrap();
        for value in [0usize, 1, 0xdead_beef] {
            for message in [WM_CTLCOLOREDIT, WM_CTLCOLORSTATIC, WM_PRINTCLIENT] {
                unsafe {
                    SendMessageW(
                        dialog,
                        message,
                        Some(WPARAM(value)),
                        Some(LPARAM(value as isize)),
                    );
                }
            }
        }
        // A real box is asked about with a device context that is not one.
        unsafe {
            SendMessageW(
                dialog,
                WM_CTLCOLOREDIT,
                Some(WPARAM(0xdead_beef)),
                Some(LPARAM(address.0 as isize)),
            );
        }
        // The dialog still works.
        assert_eq!(text_of(dialog, ID_CODE), "ABC");
        type_key(dialog, ID_CODE, 'd');
        assert_eq!(text_of(dialog, ID_CODE), "ABC-D");
        click(dialog, ID_CANCEL);
    });
    let shown = run(HWND::default(), None);
    driver.join().unwrap();
    assert!(shown.unwrap().is_none());
}

#[test]
fn the_main_button_says_pair_anyway_for_a_path_that_is_not_encrypted() {
    let _turn = TURN.lock().unwrap_or_else(|e| e.into_inner());
    let driver = drive(|dialog| {
        assert_eq!(text_of(dialog, ID_OK), "Pair");
        set_text(dialog, ID_ADDRESS, "center:7878");
        assert_eq!(text_of(dialog, ID_OK), "Pair anyway");
        set_text(dialog, ID_ADDRESS, "100.71.140.68:7878");
        assert_eq!(text_of(dialog, ID_OK), "Pair");
        set_text(dialog, ID_ADDRESS, "8.8.8.8:80");
        assert_eq!(text_of(dialog, ID_OK), "Pair");
        set_text(dialog, ID_ADDRESS, "192.168.1.5:7878");
        assert_eq!(text_of(dialog, ID_OK), "Pair anyway");
        click(dialog, ID_CANCEL);
    });
    let shown = run(HWND::default(), None);
    driver.join().unwrap();
    assert!(shown.unwrap().is_none());
}

#[test]
fn the_window_grows_with_the_status_line_and_the_controls_follow() {
    let _turn = TURN.lock().unwrap_or_else(|e| e.into_inner());
    let driver = drive(|dialog| {
        let height = |window: HWND| {
            let mut rect = RECT::default();
            unsafe {
                let _ = GetClientRect(window, &mut rect);
            }
            rect.bottom
        };
        let bottom = |id: u16| {
            let control = unsafe { GetDlgItem(Some(dialog), id.into()) }.unwrap();
            let mut rect = RECT::default();
            unsafe {
                let _ = GetWindowRect(control, &mut rect);
            }
            rect.bottom
        };
        set_text(dialog, ID_ADDRESS, "");
        let (plain_height, plain_bottom) = (height(dialog), bottom(ID_OK));
        set_text(dialog, ID_ADDRESS, "8.8.8.8:80");
        let (bad_height, bad_bottom) = (height(dialog), bottom(ID_OK));
        assert!(bad_height > plain_height);
        assert!(bad_bottom > plain_bottom, "the buttons move down with it");
        set_text(dialog, ID_ADDRESS, "center:1");
        assert!(height(dialog) > bad_height);
        set_text(dialog, ID_ADDRESS, "");
        assert_eq!(height(dialog), plain_height);
        click(dialog, ID_CANCEL);
    });
    let shown = run(HWND::default(), None);
    driver.join().unwrap();
    assert!(shown.unwrap().is_none());
}

/// Saves what the dialog shows as a BMP in the folder that
/// `ULLAGE_DIALOG_SHOTS` names, when it names one. Two pictures are made, one
/// with `PrintWindow` and one by asking the window to print itself, so that a
/// session without a screen still gives one of them.
fn shoot(dialog: HWND, name: &str) {
    // `PrintWindow` is in a part of the `windows` crate that the program
    // does not otherwise need, so it is declared here, for the tests only.
    #[link(name = "user32")]
    unsafe extern "system" {
        fn PrintWindow(
            hwnd: HWND,
            hdc: windows::Win32::Graphics::Gdi::HDC,
            flags: u32,
        ) -> windows::core::BOOL;
    }
    const WM_PRINT: u32 = 0x0317;
    // PRF_CHECKVISIBLE | PRF_NONCLIENT | PRF_CLIENT | PRF_ERASEBKGND | PRF_CHILDREN
    const PRINT_ALL: usize = 0x1 | 0x2 | 0x4 | 0x8 | 0x10;
    let Ok(folder) = std::env::var("ULLAGE_DIALOG_SHOTS") else {
        return;
    };
    let mut rect = RECT::default();
    unsafe {
        let _ = GetWindowRect(dialog, &mut rect);
    }
    let (width, height) = (rect.right - rect.left, rect.bottom - rect.top);
    for (suffix, by_print_window) in [("", true), ("-print", false)] {
        let mut surface = crate::win::surface::Surface::new(width, height).unwrap();
        unsafe {
            if by_print_window {
                let _ = PrintWindow(dialog, surface.hdc, 2);
            } else {
                SendMessageW(
                    dialog,
                    WM_PRINT,
                    Some(WPARAM(surface.hdc.0 as usize)),
                    Some(LPARAM(PRINT_ALL as isize)),
                );
            }
        }
        let pixels = surface.pixels_mut();
        // The alpha of a window that is drawn by the system can be 0.
        for pixel in pixels.as_chunks_mut::<4>().0 {
            pixel[3] = 255;
        }
        let mut file = Vec::new();
        let size = pixels.len() as u32;
        file.extend_from_slice(b"BM");
        file.extend_from_slice(&(54 + size).to_le_bytes());
        file.extend_from_slice(&[0; 4]);
        file.extend_from_slice(&54u32.to_le_bytes());
        file.extend_from_slice(&40u32.to_le_bytes());
        file.extend_from_slice(&width.to_le_bytes());
        // A negative height: the rows run top to bottom.
        file.extend_from_slice(&(-height).to_le_bytes());
        file.extend_from_slice(&1u16.to_le_bytes());
        file.extend_from_slice(&32u16.to_le_bytes());
        file.extend_from_slice(&[0; 24]);
        file.extend_from_slice(pixels);
        let path = std::path::Path::new(&folder).join(format!("{name}{suffix}.bmp"));
        std::fs::write(path, file).unwrap();
    }
}

/// One picture of the dialog in a state. It writes files only when
/// `ULLAGE_DIALOG_SHOTS` is set; the dialog is shown either way.
fn picture(name: &'static str, mode: Mode, address: &'static str, code: &'static str) {
    let driver = drive(move |dialog| {
        if !address.is_empty() {
            set_text(dialog, ID_ADDRESS, address);
        }
        if !code.is_empty() {
            set_text(dialog, ID_CODE, code);
        }
        // The dialog draws on the thread that shows it; give it a moment.
        thread::sleep(Duration::from_millis(300));
        shoot(dialog, name);
        click(dialog, ID_CANCEL);
    });
    let shown = run_with_mode(HWND::default(), None, mode);
    driver.join().unwrap();
    assert!(shown.unwrap().is_none());
}

#[test]
fn the_dialog_can_be_pictured_in_each_state() {
    let _turn = TURN.lock().unwrap_or_else(|e| e.into_inner());
    // The code is a made-up one.
    picture("light", Mode::Light, "100.71.140.68:7878", "abcdef");
    picture("dark", Mode::Dark, "100.71.140.68:7878", "abcdef");
    picture("invalid", Mode::Dark, "8.8.8.8:80", "abcdef");
    picture("plaintext", Mode::Light, "center:7878", "abcdef");
    picture("incomplete", Mode::Light, "", "");
}
