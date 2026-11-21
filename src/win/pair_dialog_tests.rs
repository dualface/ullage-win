//! Tests of the pairing dialog: a real dialog on a thread of its own,
//! driven from another thread the way a user would from the keyboard.

use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use windows::Win32::Graphics::Gdi::SetTextColor;
use windows::Win32::UI::Input::KeyboardAndMouse::IsWindowEnabled;
use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, IsWindow, PostMessageW};
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
        // The window is there while it is still being made: wait until its
        // controls are, and until the dialog has answered a message, which it
        // does only after it has opened.
        if let Ok(hwnd) = found
            && unsafe { GetDlgItem(Some(hwnd), ID_CANCEL.into()) }.is_ok()
        {
            unsafe {
                SendMessageW(hwnd, 0x0000, None, None);
            }
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
        // The note of a path that is not encrypted is drawn first.
        paint_on_window(dialog);
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

/// Has the dialog draw itself as `WM_PAINT` makes it draw on the window. A
/// session without a screen never sends `WM_PAINT` to a window that is not
/// shown, so the dialog has a message for this in the tests.
fn paint_on_window(dialog: HWND) {
    let mut rect = RECT::default();
    unsafe {
        let _ = GetClientRect(dialog, &mut rect);
    }
    // The device context stands for the window's: copying to a window that is
    // not shown does not work, and the drawing is what the dialog notes.
    let surface = crate::win::surface::Surface::new(rect.right.max(1), rect.bottom.max(1)).unwrap();
    unsafe {
        SendMessageW(
            dialog,
            TEST_PAINT_WINDOW,
            Some(WPARAM(surface.hdc.0 as usize)),
            None,
        );
    }
}

/// Has the dialog print itself into memory, as a picture of it does. That
/// is not the screen.
fn print_to_memory(dialog: HWND) {
    const WM_PRINT: u32 = 0x0317;
    let mut rect = RECT::default();
    unsafe {
        let _ = GetClientRect(dialog, &mut rect);
    }
    let surface = crate::win::surface::Surface::new(rect.right.max(1), rect.bottom.max(1)).unwrap();
    unsafe {
        SendMessageW(
            dialog,
            WM_PRINT,
            Some(WPARAM(surface.hdc.0 as usize)),
            Some(LPARAM(0x4 | 0x8)),
        );
    }
}

/// Waits until the dialog is gone, and fails when it is not: a key that was
/// meant to close it did not, and what closes it after the script is the
/// cleanup, which must not be taken for the key.
fn wait_closed(dialog: HWND) {
    use windows::Win32::UI::WindowsAndMessaging::IsWindow;
    let started = Instant::now();
    while unsafe { IsWindow(Some(dialog)) }.as_bool() {
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "the key did not close the dialog"
        );
        thread::sleep(Duration::from_millis(20));
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
        wait_closed(dialog);
    });
    let shown = run(HWND::default(), Some("100.71.140.68:7878"));
    driver.join().unwrap();
    let entry = shown.unwrap().expect("Enter in a box is the main button");
    assert_eq!(entry.address, "100.71.140.68:7878");
    assert_eq!(entry.code, "ABC-DEF");
}

#[test]
fn escape_cancels_and_does_not_pair() {
    const VK_ESCAPE: usize = 0x1B;
    let _turn = TURN.lock().unwrap_or_else(|e| e.into_inner());
    let driver = drive(|dialog| {
        set_text(dialog, ID_CODE, "abcdef");
        assert!(enabled(dialog, ID_OK));
        focus(dialog, ID_CODE);
        unsafe {
            PostMessageW(Some(dialog), WM_KEYDOWN, WPARAM(VK_ESCAPE), LPARAM(0)).unwrap();
        }
        wait_closed(dialog);
    });
    let shown = run(HWND::default(), Some("100.71.140.68:7878"));
    driver.join().unwrap();
    assert!(shown.unwrap().is_none(), "Esc is Cancel");
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
        wait_closed(dialog);
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
        let label = unsafe { GetDlgItem(Some(dialog), ID_ADDRESS_LABEL.into()) }.unwrap();
        let surface = crate::win::surface::Surface::new(8, 8).unwrap();
        let ask = |message: u32, hdc: usize, control: usize| unsafe {
            SendMessageW(
                dialog,
                message,
                Some(WPARAM(hdc)),
                Some(LPARAM(control as isize)),
            )
            .0
        };
        // What the dialog gives its own controls: a brush of its own.
        let hdc = surface.hdc.0 as usize;
        let frame_brush = ask(WM_CTLCOLOREDIT, hdc, address.0 as usize);
        let label_brush = ask(WM_CTLCOLORSTATIC, hdc, label.0 as usize);
        assert!(frame_brush != 0 && label_brush != 0);
        assert_ne!(frame_brush, label_brush);
        // What it gives anything else is the system's answer, not its brush.
        for value in [0usize, 1, 0xdead_beef] {
            for hdc in [0, 1, 0xdead_beef, surface.hdc.0 as usize] {
                assert_ne!(
                    ask(WM_CTLCOLOREDIT, hdc, value),
                    frame_brush,
                    "{hdc:#x} {value:#x}"
                );
                assert_ne!(
                    ask(WM_CTLCOLORSTATIC, hdc, value),
                    label_brush,
                    "{hdc:#x} {value:#x}"
                );
                ask(WM_PRINTCLIENT, hdc, value);
            }
        }
        // A label is not a box, and a box is not a label.
        assert_ne!(ask(WM_CTLCOLOREDIT, hdc, label.0 as usize), frame_brush);
        assert_ne!(ask(WM_CTLCOLORSTATIC, hdc, address.0 as usize), label_brush);
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

/// Saves what the dialog shows as a BMP named `name` in the folder that
/// `ULLAGE_DIALOG_SHOTS` names, when it names one. `PrintWindow` is asked
/// first, because it gives the whole window, with its title bar. When what it
/// gives is one colour (a session without a screen gives black), the window is
/// asked to print itself, which gives its client area and its controls, and
/// the text of the address box is added (see `write_address`). Only the
/// picture that is used is saved.
fn shoot(dialog: HWND, name: &str, mode: Mode) {
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
    // PRF_NONCLIENT | PRF_CLIENT | PRF_ERASEBKGND | PRF_CHILDREN
    const PRINT_ALL: usize = 0x2 | 0x4 | 0x8 | 0x10;
    let Ok(folder) = std::env::var("ULLAGE_DIALOG_SHOTS") else {
        return;
    };
    let mut rect = RECT::default();
    unsafe {
        let _ = GetWindowRect(dialog, &mut rect);
    }
    let (width, height) = (rect.right - rect.left, rect.bottom - rect.top);
    let one_colour = |surface: &mut crate::win::surface::Surface| {
        let first = surface.pixels_mut()[..3].to_vec();
        surface
            .pixels_mut()
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| pixel[..3] == first[..])
    };
    let mut surface = crate::win::surface::Surface::new(width, height).unwrap();
    unsafe {
        let _ = PrintWindow(dialog, surface.hdc, 2);
    }
    if one_colour(&mut surface) {
        surface = crate::win::surface::Surface::new(width, height).unwrap();
        unsafe {
            SendMessageW(
                dialog,
                WM_PRINT,
                Some(WPARAM(surface.hdc.0 as usize)),
                Some(LPARAM(PRINT_ALL as isize)),
            );
        }
        write_address(dialog, &surface, rect, mode);
    }
    // Last, when the window gave nothing: the scene of the dialog, drawn the
    // way the dialog draws it (`Ui`), without the native controls.
    if one_colour(&mut surface) {
        surface = crate::win::surface::Surface::new(width, height).unwrap();
        let ui = Ui::new(mode, 96).unwrap();
        let _ = ui.draw_dialog(
            surface.hdc,
            (width, height),
            &text_of(dialog, ID_ADDRESS),
            false,
        );
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
    let path = std::path::Path::new(&folder).join(format!("{name}.bmp"));
    std::fs::write(path, file).unwrap();
}

/// A single-line edit control does not answer `WM_PRINT` in a session that has
/// no screen (a multi-line one does), so the text of the address box is drawn
/// into the picture by the test, in the font and the colour of the box, where
/// the box is. This is the one part of a picture that the test draws.
fn write_address(dialog: HWND, surface: &crate::win::surface::Surface, window: RECT, mode: Mode) {
    use windows::Win32::Graphics::Gdi::{HGDIOBJ, SelectObject, SetBkMode, TRANSPARENT, TextOutW};
    const WM_GETFONT: u32 = 0x0031;
    let Ok(edit) = (unsafe { GetDlgItem(Some(dialog), ID_ADDRESS.into()) }) else {
        return;
    };
    let mut at = RECT::default();
    unsafe {
        let _ = GetWindowRect(edit, &mut at);
    }
    let font = unsafe { SendMessageW(edit, WM_GETFONT, None, None) }.0;
    let text: Vec<u16> = text_of(dialog, ID_ADDRESS).encode_utf16().collect();
    unsafe {
        let previous = SelectObject(surface.hdc, HGDIOBJ(font as *mut _));
        SetBkMode(surface.hdc, TRANSPARENT);
        SetTextColor(surface.hdc, colorref(mode, Role::Text));
        let _ = TextOutW(
            surface.hdc,
            at.left - window.left,
            at.top - window.top,
            &text,
        );
        SelectObject(surface.hdc, previous);
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
        focus(dialog, ID_OK);
        // The dialog draws on the thread that shows it; give it a moment.
        thread::sleep(Duration::from_millis(300));
        shoot(dialog, name, mode);
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

#[test]
fn the_dialog_draws_its_background_and_its_shapes_on_a_surface() {
    use crate::win::surface::Surface;
    for mode in [Mode::Light, Mode::Dark] {
        let ui = Ui::new(mode, 96).expect("the fonts and the factory");
        let mut surface = Surface::new(420, 300).unwrap();
        ui.draw_dialog(surface.hdc, (420, 300), "center:7878", false)
            .expect("drawing on a surface works");
        let background = colorref(mode, Role::Win);
        let pixels = surface.pixels_mut();
        let corner = &pixels[..4];
        // B, G, R of the dialog's colour at the top left, where nothing else
        // is drawn.
        assert_eq!(
            (corner[2], corner[1], corner[0]),
            (
                (background.0 & 0xFF) as u8,
                ((background.0 >> 8) & 0xFF) as u8,
                ((background.0 >> 16) & 0xFF) as u8
            ),
            "{mode:?}"
        );
        // And something else is drawn too: the title, the frames, the note.
        let different = pixels
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|pixel| pixel[..3] != corner[..3])
            .count();
        assert!(different > 2000, "{mode:?}: {different} pixels differ");
    }
}

#[test]
fn the_window_prints_itself_in_the_colours_of_the_mode() {
    const WM_PRINT: u32 = 0x0317;
    // PRF_NONCLIENT | PRF_CLIENT | PRF_ERASEBKGND | PRF_CHILDREN
    const PRINT_ALL: usize = 0x2 | 0x4 | 0x8 | 0x10;
    let _turn = TURN.lock().unwrap_or_else(|e| e.into_inner());
    for mode in [Mode::Light, Mode::Dark] {
        let driver = drive(move |dialog| {
            set_text(dialog, ID_CODE, "abcdef");
            let mut rect = RECT::default();
            unsafe {
                let _ = GetClientRect(dialog, &mut rect);
            }
            let mut surface = crate::win::surface::Surface::new(rect.right, rect.bottom).unwrap();
            unsafe {
                SendMessageW(
                    dialog,
                    WM_PRINT,
                    Some(WPARAM(surface.hdc.0 as usize)),
                    Some(LPARAM((PRINT_ALL & !0x2) as isize)),
                );
            }
            let background = colorref(mode, Role::Win);
            let want = [
                (background.0 >> 16) as u8,
                (background.0 >> 8) as u8,
                background.0 as u8,
            ];
            let pixels = surface.pixels_mut();
            // The corner of the client area is the colour of the dialog, and
            // there is more drawn than that.
            assert_eq!(pixels[..3], want, "{mode:?}");
            let distinct: std::collections::HashSet<[u8; 4]> =
                pixels.as_chunks::<4>().0.iter().copied().collect();
            assert!(distinct.len() > 20, "{mode:?}: {} colours", distinct.len());
            click(dialog, ID_CANCEL);
        });
        let shown = run_with_mode(HWND::default(), None, mode);
        driver.join().unwrap();
        assert!(shown.unwrap().is_none());
    }
}

#[test]
fn a_path_that_is_not_encrypted_is_paired_with_only_after_its_note_was_drawn() {
    let _turn = TURN.lock().unwrap_or_else(|e| e.into_inner());
    let driver = drive(|dialog| {
        // The code is whole. The address is typed, and the main button is
        // pressed at once: the press is queued before the note is drawn, and
        // is not the user's word.
        set_text(dialog, ID_CODE, "abcdef");
        set_text(dialog, ID_ADDRESS, "center:7878");
        click(dialog, ID_OK);
        // The dialog has handled the press (a message after it is answered
        // only after it), and is still there.
        let still_there = |dialog: HWND| {
            unsafe {
                SendMessageW(dialog, 0x0000, None, None);
            }
            unsafe { IsWindow(Some(dialog)) }.as_bool()
        };
        assert!(still_there(dialog), "paired too early");
        // A print into memory is not the screen: it does not count either.
        print_to_memory(dialog);
        click(dialog, ID_OK);
        assert!(still_there(dialog), "paired after a print into memory");
        // Once the note is on the window, the next press counts.
        paint_on_window(dialog);
        click(dialog, ID_OK);
        wait_closed(dialog);
    });
    let shown = run(HWND::default(), None);
    driver.join().unwrap();
    let entry = shown.unwrap().expect("the press after the paint pairs");
    assert_eq!(entry.address, "center:7878");
}

#[test]
fn a_note_that_was_seen_is_forgotten_when_the_address_changes() {
    let _turn = TURN.lock().unwrap_or_else(|e| e.into_inner());
    let driver = drive(|dialog| {
        set_text(dialog, ID_CODE, "abcdef");
        set_text(dialog, ID_ADDRESS, "center:7878");
        paint_on_window(dialog);
        // Another address, and back: the note of the first is not seen again.
        set_text(dialog, ID_ADDRESS, "nas:7878");
        set_text(dialog, ID_ADDRESS, "center:7878");
        click(dialog, ID_OK);
        unsafe {
            SendMessageW(dialog, 0x0000, None, None);
        }
        assert!(
            unsafe { IsWindow(Some(dialog)) }.as_bool(),
            "paired with a note that was not seen again"
        );
        paint_on_window(dialog);
        click(dialog, ID_OK);
        wait_closed(dialog);
    });
    let shown = run(HWND::default(), None);
    driver.join().unwrap();
    assert!(shown.unwrap().is_some());
}

#[test]
fn a_path_that_is_encrypted_needs_no_note() {
    let _turn = TURN.lock().unwrap_or_else(|e| e.into_inner());
    let driver = drive(|dialog| {
        set_text(dialog, ID_CODE, "abcdef");
        set_text(dialog, ID_ADDRESS, "100.71.140.68:7878");
        click(dialog, ID_OK);
        wait_closed(dialog);
    });
    let shown = run(HWND::default(), None);
    driver.join().unwrap();
    assert!(shown.unwrap().is_some());
}

#[test]
fn a_handle_that_is_not_a_device_context_is_not_drawn_on() {
    use windows::Win32::Graphics::Gdi::CreateSolidBrush;
    let _turn = TURN.lock().unwrap_or_else(|e| e.into_inner());
    let surface = crate::win::surface::Surface::new(8, 8).unwrap();
    assert!(is_dc(surface.hdc));
    assert!(!is_dc(HDC(std::ptr::null_mut())));
    assert!(!is_dc(HDC(0xdead_beef as *mut _)));
    let brush = unsafe { CreateSolidBrush(windows::Win32::Foundation::COLORREF(0)) };
    // A brush and a bitmap are handles of GDI objects, and not device contexts.
    assert!(!is_dc(HDC(brush.0)));
    assert!(!is_dc(HDC(surface.bitmap.0)));
    // Handle values cross threads; handles do not.
    let handles = [brush.0 as usize, surface.bitmap.0 as usize];
    let driver = drive(move |dialog| {
        for handle in handles {
            for message in [0x0318u32 /* WM_PRINTCLIENT */, WM_CTLCOLOREDIT] {
                unsafe {
                    SendMessageW(dialog, message, Some(WPARAM(handle)), Some(LPARAM(0)));
                }
            }
        }
        click(dialog, ID_CANCEL);
    });
    let shown = run(HWND::default(), None);
    driver.join().unwrap();
    assert!(shown.unwrap().is_none());
    // The brush is the test's, and is let go of here.
    unsafe {
        let _ = windows::Win32::Graphics::Gdi::DeleteObject(
            windows::Win32::Graphics::Gdi::HGDIOBJ(brush.0),
        );
    }
}
