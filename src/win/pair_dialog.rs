//! The pairing dialog: a box for the address of a remote daemon and a box
//! for the pairing code. What may be typed, and what the main button needs, is
//! decided by `remote::pairing` and `scene::pair`; this module puts it on the
//! screen, with the controls of the system and the look of `scene::pair`.
//!
//! The dialog is modal and runs its own message loop, so it is shown only
//! while the app is not borrowed, like a menu or a message box.
//!
//! Every message that reaches the dialog or a control of it, from this
//! process or from another, is handled with the state of this thread, found
//! by the handle of the window. A message parameter that is a pointer is
//! never followed: `WM_INITDIALOG` and the colour messages read none, and
//! the buttons are the system's, so there is no `WM_DRAWITEM`.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{DWMWINDOWATTRIBUTE, DwmSetWindowAttribute};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, EndPaint, GetMonitorInfoW, HDC, InvalidateRect, MONITOR_DEFAULTTONEAREST,
    MONITORINFO, MonitorFromWindow, PAINTSTRUCT, RDW_ALLCHILDREN, RDW_INVALIDATE, RedrawWindow,
    SetBkColor, SetTextColor,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{AdjustWindowRectExForDpi, GetDpiForWindow};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    EnableWindow, IsWindowEnabled, SetFocus, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallWindowProcW, DLGTEMPLATE, DWLP_MSGRESULT, DefWindowProcW, DialogBoxIndirectParamW,
    EN_CHANGE, EndDialog, GWL_EXSTYLE, GWL_STYLE, GWLP_WNDPROC, GetClientRect, GetDlgCtrlID,
    GetDlgItem, GetDlgItemTextW, GetParent, GetWindowLongPtrW, GetWindowRect, GetWindowTextW,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SendDlgItemMessageW, SendMessageW,
    SetDlgItemTextW, SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, WINDOW_LONG_PTR_INDEX,
    WM_CLOSE, WM_COMMAND, WM_INITDIALOG,
};

use super::pair_paint::{Ui, colorref, is_dc};
use super::registry;
use super::util::{hiword, loword, pcwstr, wide};
use crate::pair_form::{
    ADDRESS_LIMIT, CODE_LIMIT, ID_ADDRESS, ID_ADDRESS_LABEL, ID_CANCEL, ID_CODE, ID_CODE_LABEL,
    ID_OK, template,
};
use crate::remote::pairing::{can_submit, format_pair_code_input};
use crate::scene::Rect;
use crate::scene::pair::{
    ButtonState, Focus, Kind, Model, Status, frame, primary_label, status, to_pixels,
};
use crate::theme::{Mode, Role};

// Messages and notifications of the controls, and of windows, from
// `winuser.h`. The `Win32_UI_Controls` feature that holds some of them is not
// otherwise needed.
const EM_SETSEL: u32 = 0x00B1;
const EM_LIMITTEXT: u32 = 0x00C5;
const EM_REPLACESEL: u32 = 0x00C2;
const EM_UNDO: u32 = 0x00C7;
const EN_SETFOCUS: u32 = 0x0100;
const EN_KILLFOCUS: u32 = 0x0200;
const BM_GETSTATE: u32 = 0x00F2;
const BST_PUSHED: isize = 0x4;
const BST_FOCUS: isize = 0x8;
const WM_SETTEXT: u32 = 0x000C;
const WM_SETFONT: u32 = 0x0030;
const WM_ENABLE: u32 = 0x000A;
const WM_ERASEBKGND: u32 = 0x0014;
const WM_PAINT: u32 = 0x000F;
const WM_SETTINGCHANGE: u32 = 0x001A;
const WM_CTLCOLOREDIT: u32 = 0x0133;
const WM_CTLCOLORSTATIC: u32 = 0x0138;
const WM_KEYDOWN: u32 = 0x0100;
const WM_KEYUP: u32 = 0x0101;
const WM_CHAR: u32 = 0x0102;
const WM_MOUSEMOVE: u32 = 0x0200;
const WM_LBUTTONDOWN: u32 = 0x0201;
const WM_LBUTTONUP: u32 = 0x0202;
const WM_CAPTURECHANGED: u32 = 0x0215;
const WM_MOUSELEAVE: u32 = 0x02A3;
const WM_PRINTCLIENT: u32 = 0x0318;
const WM_QUERYUISTATE: u32 = 0x0129;
const WM_UPDATEUISTATE: u32 = 0x0128;
const WM_CUT: u32 = 0x0300;
const WM_PASTE: u32 = 0x0302;
const WM_CLEAR: u32 = 0x0303;
const WM_UNDO: u32 = 0x0304;
const WM_DPICHANGED: u32 = 0x02E0;
const BM_SETSTATE: u32 = 0x00F3;
const UISF_HIDEFOCUS: isize = 0x1;
/// A message of the tests: draw as `WM_PAINT` does, on the device context in
/// `wparam`.
#[cfg(test)]
const TEST_PAINT_WINDOW: u32 = 0x8000 + 0x77;
/// `DWMWA_USE_IMMERSIVE_DARK_MODE`.
const DARK_TITLE_BAR: DWMWINDOWATTRIBUTE = DWMWINDOWATTRIBUTE(20);

/// What the user typed when they pressed the main button.
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
    /// A mode to use instead of the system's, for the tests that take a
    /// picture of each.
    forced_mode: Option<Mode>,
    /// What the dialog draws with, made when it opens.
    ui: RefCell<Option<Rc<Ui>>>,
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

/// What the dialog of this thread holds. The dialog is modal and runs on the
/// thread that shows it, and every message to it, from this process or from
/// another, is delivered on that thread. So the state is this thread's own,
/// found by the handle of the dialog, and nothing is read back from a message
/// parameter or from a value that another process could have set.
enum State {
    Idle,
    /// The dialog is being made: its first `WM_INITDIALOG` takes this.
    Waiting(Rc<Shared>),
    Open(Open),
}

struct Open {
    dialog: HWND,
    shared: Rc<Shared>,
    /// The window procedures that were replaced, by the handle of the control.
    originals: Vec<(isize, isize)>,
}

thread_local! {
    static STATE: RefCell<State> = const { RefCell::new(State::Idle) };
}

/// The state of the open dialog `hwnd`, when `hwnd` is the dialog.
fn shared_of(hwnd: HWND) -> Option<Rc<Shared>> {
    STATE.with_borrow(|state| match state {
        State::Open(open) if open.dialog == hwnd => Some(open.shared.clone()),
        _ => None,
    })
}

/// The window procedure that the control `control` of the dialog had before
/// it was replaced.
fn original_of(dialog: HWND, control: HWND) -> Option<isize> {
    STATE.with_borrow(|state| match state {
        State::Open(open) if open.dialog == dialog => open
            .originals
            .iter()
            .find(|(handle, _)| *handle == control.0 as isize)
            .map(|(_, original)| *original),
        _ => None,
    })
}

fn ui_of(shared: &Shared) -> Option<Rc<Ui>> {
    shared.ui.borrow().clone()
}

/// Shows the dialog over `owner` and waits for it to close. `None` is a
/// dialog that was cancelled. The boxes are emptied before the dialog goes.
/// That only one dialog is open at a time is the caller's to see to: the
/// dialog disables the window, but the tray icon still reaches it.
pub fn run(owner: HWND, prefill: Option<&str>) -> Result<Option<Entry>, Failed> {
    run_in(owner, prefill, None)
}

/// [`run`] in the colours of `mode`, whatever the system uses.
#[cfg(test)]
pub fn run_with_mode(
    owner: HWND,
    prefill: Option<&str>,
    mode: Mode,
) -> Result<Option<Entry>, Failed> {
    run_in(owner, prefill, Some(mode))
}

fn run_in(
    owner: HWND,
    prefill: Option<&str>,
    forced_mode: Option<Mode>,
) -> Result<Option<Entry>, Failed> {
    // A template must start at a multiple of four bytes, which a `Vec<u8>`
    // does not promise.
    let bytes = template();
    let mut words = vec![0u32; bytes.len().div_ceil(4)];
    for (index, byte) in bytes.iter().enumerate() {
        words[index / 4] |= u32::from(*byte) << (8 * (index % 4));
    }
    let shared = Rc::new(Shared {
        prefill: prefill.unwrap_or_default().to_owned(),
        owner,
        forced_mode,
        ui: RefCell::new(None),
        formatting: Cell::new(false),
        editing: Cell::new(false),
        entry: RefCell::new(None),
    });
    // One dialog per thread: a dialog that is made while another is open or
    // being made is refused.
    let taken = STATE.with_borrow_mut(|state| match state {
        State::Idle => {
            *state = State::Waiting(shared.clone());
            true
        }
        _ => false,
    });
    if !taken {
        return Err(Failed);
    }
    let ended = unsafe {
        GetModuleHandleW(None).map(|module| {
            DialogBoxIndirectParamW(
                Some(module.into()),
                words.as_ptr().cast::<DLGTEMPLATE>(),
                Some(owner),
                Some(dialog_proc),
                LPARAM(0),
            )
        })
    };
    // The dialog is gone, or was never made.
    STATE.with_borrow_mut(|state| *state = State::Idle);
    let Ok(ended) = ended else {
        return Err(Failed);
    };
    // `-1` is a dialog that could not be made. Anything else, `0` for a
    // cancel and `1` for OK, leaves the answer in `shared`.
    if ended == -1 {
        return Err(Failed);
    }
    let entry = shared.entry.borrow_mut().take();
    Ok(entry)
}

/// Sets the result that a dialog procedure gives for a message it handled.
fn answer(hwnd: HWND, result: isize) -> isize {
    unsafe {
        SetWindowLongPtrW(hwnd, WINDOW_LONG_PTR_INDEX(DWLP_MSGRESULT as i32), result);
    }
    1
}

unsafe extern "system" fn dialog_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> isize {
    match message {
        WM_INITDIALOG => {
            // Only the first one, which is the system's: the state waits for
            // it. Another one, from anywhere, finds nothing to take and does
            // nothing. `lparam` is not read.
            let shared = STATE.with_borrow_mut(|state| {
                let State::Waiting(shared) = state else {
                    return None;
                };
                let shared = shared.clone();
                *state = State::Open(Open {
                    dialog: hwnd,
                    shared: shared.clone(),
                    originals: Vec::new(),
                });
                Some(shared)
            });
            let Some(shared) = shared else {
                return 0;
            };
            if initialize(hwnd, &shared).is_err() {
                unsafe {
                    let _ = EndDialog(hwnd, -1);
                }
            }
            // The focus is set there, so the dialog manager must not.
            0
        }
        // For the tests, which run where the window is never shown, is never
        // sent `WM_PAINT`, and cannot be copied to: draw on the device context
        // that `wparam` is, as `WM_PAINT` draws on the window's.
        #[cfg(test)]
        TEST_PAINT_WINDOW if shared_of(hwnd).is_some() => {
            draw_dialog_on(hwnd, HDC(wparam.0 as *mut _), true);
            1
        }
        // The rest have no state to act on before `WM_INITDIALOG`, and are
        // left to the default procedure then.
        WM_ERASEBKGND if shared_of(hwnd).is_some() => answer(hwnd, 1),
        WM_PAINT if shared_of(hwnd).is_some() => {
            paint_dialog(hwnd, None);
            1
        }
        // `wparam` is the device context to draw on. It is only used as the
        // target of drawing, after it is checked to be one.
        WM_PRINTCLIENT if shared_of(hwnd).is_some() => {
            paint_dialog(hwnd, Some(HDC(wparam.0 as *mut _)));
            answer(hwnd, 0)
        }
        // `lparam` is the control that is asked about. It is compared with
        // the handles of the controls of this dialog, and read no further.
        WM_CTLCOLOREDIT | WM_CTLCOLORSTATIC => color_of_control(hwnd, message, wparam, lparam),
        WM_SETTINGCHANGE => {
            if let Some(shared) = shared_of(hwnd) {
                restyle_for_system(hwnd, &shared);
            }
            0
        }
        // The DPI is the one that the window has (`GetDpiForWindow`), not the
        // one in `wparam`, which can be anything. The rectangle that `lparam`
        // points to is not read: the layout is made again.
        WM_DPICHANGED => {
            if let Some(shared) = shared_of(hwnd)
                && let Some(ui) = ui_of(&shared)
            {
                // Not the DPI of the message, which can be anything: the one
                // that the window has. A DPI that is 0 is no DPI.
                let dpi = unsafe { GetDpiForWindow(hwnd) };
                if dpi != 0 && dpi != ui.dpi() {
                    let old = ui.restyle(ui.mode(), dpi);
                    dress_controls(hwnd, &ui);
                    drop(old);
                    layout(hwnd, &ui);
                }
            }
            1
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
                (ID_CODE, EN_CHANGE) => code_changed(hwnd, &shared),
                (ID_ADDRESS, EN_SETFOCUS) => focus_changed(hwnd, &shared, Focus::Address),
                (ID_CODE, EN_SETFOCUS) => focus_changed(hwnd, &shared, Focus::Code),
                (ID_ADDRESS | ID_CODE, EN_KILLFOCUS) => focus_changed(hwnd, &shared, Focus::None),
                // `BN_CLICKED`, which is also what Enter and Esc send.
                (ID_OK, 0) => accept(hwnd, &shared),
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
    // The state of the dialog, found by the handle of the dialog, and the
    // edit control's own procedure, kept in it. Without them the message is
    // left to the default procedure.
    let found = unsafe { GetParent(hwnd) }.ok().and_then(|dialog| {
        let shared = shared_of(dialog)?;
        let original = original_of(dialog, hwnd)?;
        Some((dialog, shared, original))
    });
    let Some((dialog, shared, original)) = found else {
        return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
    };
    // SAFETY: the value was the window procedure of the box when the dialog
    // was set up, which is a function with this signature.
    let original: windows::Win32::UI::WindowsAndMessaging::WNDPROC =
        unsafe { std::mem::transmute(original) };
    let changes = CHANGES_TEXT.contains(&message);
    let outer = shared.editing.replace(changes || shared.editing.get());
    let result = unsafe { CallWindowProcW(original, hwnd, message, wparam, lparam) };
    shared.editing.set(outer);
    // Only the outermost message formats: the text is whole by then.
    if changes && !outer {
        code_changed(dialog, &shared);
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

/// Which button a control is, and `None` for a control that is not one.
fn kind_of(button: HWND) -> Option<Kind> {
    match unsafe { GetDlgCtrlID(button) } {
        id if id == i32::from(ID_OK) => Some(Kind::Primary),
        id if id == i32::from(ID_CANCEL) => Some(Kind::Secondary),
        _ => None,
    }
}

/// The window procedure of a button: the system's own, but it is drawn by
/// `scene::pair`. The state it is drawn in is read from the button itself
/// (`BM_GETSTATE`, enabled or not) and from the dialog's own state, and no
/// message parameter is followed.
unsafe extern "system" fn button_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let found = unsafe { GetParent(hwnd) }.ok().and_then(|dialog| {
        let shared = shared_of(dialog)?;
        let original = original_of(dialog, hwnd)?;
        Some((shared, original))
    });
    let Some((shared, original)) = found else {
        return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
    };
    let ui = ui_of(&shared);
    match message {
        WM_ERASEBKGND => return LRESULT(1),
        WM_PAINT => {
            paint_button(hwnd, ui.as_deref(), None);
            return LRESULT(0);
        }
        WM_PRINTCLIENT => {
            paint_button(hwnd, ui.as_deref(), Some(HDC(wparam.0 as *mut _)));
            return LRESULT(0);
        }
        WM_MOUSEMOVE => {
            if let Some(ui) = &ui {
                let kind = kind_of(hwnd);
                if ui.hover.get() != kind {
                    ui.hover.set(kind);
                    invalidate(hwnd);
                }
                if !ui.tracking.get() {
                    let mut track = TRACKMOUSEEVENT {
                        cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE,
                        hwndTrack: hwnd,
                        dwHoverTime: 0,
                    };
                    if unsafe { TrackMouseEvent(&mut track) }.is_ok() {
                        ui.tracking.set(true);
                    }
                }
            }
        }
        WM_MOUSELEAVE => {
            if let Some(ui) = &ui {
                ui.tracking.set(false);
                ui.hover.set(None);
            }
            invalidate(hwnd);
            return LRESULT(0);
        }
        _ => {}
    }
    // SAFETY: the value was the window procedure of the button when the
    // dialog was set up, which is a function with this signature.
    let original: windows::Win32::UI::WindowsAndMessaging::WNDPROC =
        unsafe { std::mem::transmute(original) };
    let result = unsafe { CallWindowProcW(original, hwnd, message, wparam, lparam) };
    // Anything that can change how the button looks.
    if matches!(
        message,
        WM_SETTEXT
            | WM_ENABLE
            | BM_SETSTATE
            | WM_UPDATEUISTATE
            | WM_LBUTTONDOWN
            | WM_LBUTTONUP
            | WM_CAPTURECHANGED
            | WM_KEYDOWN
            | WM_KEYUP
    ) || matches!(message, 0x0007 | 0x0008)
    {
        invalidate(hwnd);
    }
    result
}

fn invalidate(hwnd: HWND) {
    unsafe {
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
}

fn client_pixels(hwnd: HWND) -> (i32, i32) {
    let mut rect = RECT::default();
    unsafe {
        let _ = GetClientRect(hwnd, &mut rect);
    }
    (rect.right - rect.left, rect.bottom - rect.top)
}

fn paint_button(hwnd: HWND, ui: Option<&Ui>, on: Option<HDC>) {
    let mut paint = PAINTSTRUCT::default();
    let hdc = match on {
        Some(hdc) => hdc,
        None => unsafe { BeginPaint(hwnd, &mut paint) },
    };
    if let (Some(ui), Some(kind)) = (ui, kind_of(hwnd))
        && is_dc(hdc)
    {
        let flags = unsafe { SendMessageW(hwnd, BM_GETSTATE, None, None) }.0;
        // A dialog hides the focus ring until the keyboard is used.
        let ui_state = unsafe { SendMessageW(hwnd, WM_QUERYUISTATE, None, None) }.0;
        let mut label = [0u16; 64];
        let length = unsafe { GetWindowTextW(hwnd, &mut label) } as usize;
        let label = String::from_utf16_lossy(&label[..length.min(64)]);
        let state = ButtonState {
            hover: ui.hover.get() == Some(kind),
            pressed: flags & BST_PUSHED != 0,
            disabled: !unsafe { IsWindowEnabled(hwnd) }.as_bool(),
            focus_ring: flags & BST_FOCUS != 0 && ui_state & UISF_HIDEFOCUS == 0,
        };
        let _ = ui.draw_button(hdc, client_pixels(hwnd), kind, state, &label);
    }
    if on.is_none() {
        unsafe {
            let _ = EndPaint(hwnd, &paint);
        }
    }
}

/// Draws the dialog on `on`, a device context that was asked for (a print), or,
/// when there is none, on the window itself (`WM_PAINT`).
fn paint_dialog(hwnd: HWND, on: Option<HDC>) {
    let mut paint = PAINTSTRUCT::default();
    let hdc = match on {
        Some(hdc) => hdc,
        None => unsafe { BeginPaint(hwnd, &mut paint) },
    };
    draw_dialog_on(hwnd, hdc, on.is_none());
    if on.is_none() {
        unsafe {
            let _ = EndPaint(hwnd, &paint);
        }
    }
}

/// Draws the dialog on `hdc`, which `on_screen` says is the window's own.
fn draw_dialog_on(hwnd: HWND, hdc: HDC, on_screen: bool) {
    // No borrow is held while drawing: the state is cloned out first.
    if let Some(shared) = shared_of(hwnd)
        && let Some(ui) = ui_of(&shared)
        && is_dc(hdc)
    {
        let address = text_of(hwnd, ID_ADDRESS);
        let _ = ui.draw_dialog(hdc, client_pixels(hwnd), &address, on_screen);
    }
}

/// The colour of the controls that the dialog colours: the two boxes (text on
/// the colour of their frame) and the two labels (on the dialog's).
fn color_of_control(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> isize {
    let Some(shared) = shared_of(hwnd) else {
        return 0;
    };
    let Some(ui) = ui_of(&shared) else {
        return 0;
    };
    let control = lparam.0;
    let is = |id: u16| {
        unsafe { GetDlgItem(Some(hwnd), id.into()) }.is_ok_and(|item| item.0 as isize == control)
    };
    let hdc = HDC(wparam.0 as *mut _);
    let mode = ui.mode();
    if message == WM_CTLCOLOREDIT && (is(ID_ADDRESS) || is(ID_CODE)) {
        unsafe {
            SetTextColor(hdc, colorref(mode, Role::Text));
            SetBkColor(hdc, colorref(mode, Role::Card));
        }
        return ui.card_brush().0 as isize;
    }
    if message == WM_CTLCOLORSTATIC && (is(ID_ADDRESS_LABEL) || is(ID_CODE_LABEL)) {
        unsafe {
            SetTextColor(hdc, colorref(mode, Role::Faint));
            SetBkColor(hdc, colorref(mode, Role::Win));
        }
        return ui.win_brush().0 as isize;
    }
    0
}

fn focus_changed(hwnd: HWND, shared: &Shared, focus: Focus) {
    if let Some(ui) = ui_of(shared) {
        ui.focus.set(focus);
        invalidate(hwnd);
    }
}

/// The mode the dialog is drawn in: the system's, unless a test sets one.
fn mode_now(shared: &Shared) -> Mode {
    shared.forced_mode.unwrap_or_else(registry::apps_mode)
}

/// The system changed a setting: draw in the new mode when it is another.
fn restyle_for_system(hwnd: HWND, shared: &Shared) {
    let Some(ui) = ui_of(shared) else {
        return;
    };
    let mode = mode_now(shared);
    if mode == ui.mode() {
        return;
    }
    let old = ui.restyle(mode, ui.dpi());
    dark_title_bar(hwnd, mode);
    dress_controls(hwnd, &ui);
    drop(old);
    unsafe {
        let _ = RedrawWindow(Some(hwnd), None, None, RDW_INVALIDATE | RDW_ALLCHILDREN);
    }
}

/// A dark title bar in the dark mode. A system that does not know the
/// attribute leaves it light.
fn dark_title_bar(hwnd: HWND, mode: Mode) {
    let on: i32 = (mode == Mode::Dark).into();
    unsafe {
        let _ = DwmSetWindowAttribute(
            hwnd,
            DARK_TITLE_BAR,
            (&on as *const i32).cast(),
            size_of::<i32>() as u32,
        );
    }
}

/// The fonts of the controls.
fn dress_controls(hwnd: HWND, ui: &Ui) {
    let fonts = [
        (ID_ADDRESS_LABEL, ui.label_font()),
        (ID_ADDRESS, ui.edit_font()),
        (ID_CODE_LABEL, ui.label_font()),
        (ID_CODE, ui.code_font()),
        (ID_OK, ui.edit_font()),
        (ID_CANCEL, ui.edit_font()),
    ];
    for (id, font) in fonts {
        unsafe {
            SendDlgItemMessageW(
                hwnd,
                id.into(),
                WM_SETFONT,
                WPARAM(font.0 as usize),
                LPARAM(1),
            );
        }
    }
}

/// The DPI of the screen the window is on, 96 when it is not known.
fn dpi_of(hwnd: HWND) -> u32 {
    match unsafe { GetDpiForWindow(hwnd) } {
        0 => 96,
        dpi => dpi,
    }
}

fn initialize(hwnd: HWND, shared: &Rc<Shared>) -> Result<(), Failed> {
    let mode = mode_now(shared);
    // The dialog is first laid out for the DPI of the owner, and then, when
    // it has been moved to the owner's screen, for the DPI of its own.
    let first = if shared.owner.0.is_null() {
        dpi_of(hwnd)
    } else {
        dpi_of(shared.owner)
    };
    let ui = Rc::new(Ui::new(mode, first).map_err(|_| Failed)?);
    *shared.ui.borrow_mut() = Some(ui.clone());
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
    }
    // The code box formats what is typed or pasted as soon as it is, and the
    // buttons are drawn by the dialog.
    subclass(hwnd, ID_CODE, code_box_proc as *const () as isize);
    subclass(hwnd, ID_OK, button_proc as *const () as isize);
    subclass(hwnd, ID_CANCEL, button_proc as *const () as isize);
    dark_title_bar(hwnd, mode);
    dress_controls(hwnd, &ui);
    refresh(hwnd);
    center_on_owner(hwnd, shared.owner);
    // On another screen, the DPI can be another.
    let own = dpi_of(hwnd);
    if own != ui.dpi() {
        let old = ui.restyle(mode, own);
        dress_controls(hwnd, &ui);
        drop(old);
        layout(hwnd, &ui);
    }
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
    Ok(())
}

/// Replaces the window procedure of the control `id`, and keeps the old one.
fn subclass(dialog: HWND, id: u16, procedure: isize) {
    unsafe {
        let Ok(control) = GetDlgItem(Some(dialog), id.into()) else {
            return;
        };
        let original = GetWindowLongPtrW(control, GWLP_WNDPROC);
        STATE.with_borrow_mut(|state| {
            if let State::Open(open) = state {
                open.originals.push((control.0 as isize, original));
            }
        });
        SetWindowLongPtrW(control, GWLP_WNDPROC, procedure);
    }
}

/// Puts every control where `scene::pair` wants it, for the text in the
/// address box, and makes the window the size that fits.
fn layout(hwnd: HWND, ui: &Ui) {
    let model = Model {
        address: text_of(hwnd, ID_ADDRESS),
        focus: ui.focus.get(),
    };
    let frame = frame(&model, ui.typography());
    let dpi = ui.dpi();
    let (_, _, width, height) = to_pixels(Rect::new(0.0, 0.0, frame.size.0, frame.size.1), dpi);
    unsafe {
        // The outer size is the size of the client area, and the frame and
        // the title bar of the dialog at this DPI.
        let mut outer = RECT {
            left: 0,
            top: 0,
            right: width,
            bottom: height,
        };
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
        let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        let _ = AdjustWindowRectExForDpi(
            &mut outer,
            windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE(style),
            false,
            windows::Win32::UI::WindowsAndMessaging::WINDOW_EX_STYLE(ex_style),
            dpi,
        );
        if client_pixels(hwnd) != (width, height) {
            let _ = SetWindowPos(
                hwnd,
                None,
                0,
                0,
                outer.right - outer.left,
                outer.bottom - outer.top,
                SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
        let places = [
            (ID_ADDRESS_LABEL, frame.address_label),
            (ID_ADDRESS, frame.address_edit),
            (ID_CODE_LABEL, frame.code_label),
            (ID_CODE, frame.code_edit),
            (ID_OK, frame.primary),
            (ID_CANCEL, frame.cancel),
        ];
        for (id, rect) in places {
            if let Ok(control) = GetDlgItem(Some(hwnd), id.into()) {
                let (x, y, w, h) = to_pixels(rect, dpi);
                let _ = SetWindowPos(control, None, x, y, w, h, SWP_NOZORDER | SWP_NOACTIVATE);
            }
        }
    }
    invalidate(hwnd);
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

/// The line under the address and the label of the main button follow the
/// address, the main button works when the input is whole, and the dialog is
/// drawn again.
fn refresh(hwnd: HWND) {
    let address = text_of(hwnd, ID_ADDRESS);
    let code = text_of(hwnd, ID_CODE);
    unsafe {
        if let Ok(ok) = GetDlgItem(Some(hwnd), ID_OK.into()) {
            let _ = EnableWindow(ok, can_submit(&address, &code));
        }
    }
    let Some(shared) = shared_of(hwnd) else {
        return;
    };
    let Some(ui) = ui_of(&shared) else {
        return;
    };
    // The note that was seen was the note of an address: another address has
    // none yet.
    if ui.noted.borrow().as_deref() != Some(address.as_str()) {
        *ui.noted.borrow_mut() = None;
    }
    let label = primary_label(&status(&address));
    if text_of(hwnd, ID_OK) != label {
        set_text(hwnd, ID_OK, label);
    }
    layout(hwnd, &ui);
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
    // The main button is disabled until the input is good, but Enter can
    // still reach here: the check is made again.
    if !can_submit(&address, &code) {
        return;
    }
    // A path that is not encrypted is paired with only after its note has been
    // on the screen: an Enter that was queued before the note was drawn is not
    // the user's word. The dialog is drawn again, and the next press counts.
    if let Some(ui) = ui_of(shared)
        && status(&address) == Status::Plaintext
        && ui.noted.borrow().as_deref() != Some(address.as_str())
    {
        invalidate(hwnd);
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
        clear(hwnd, &shared);
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
#[path = "pair_dialog_pictures.rs"]
mod pictures;
#[cfg(test)]
#[path = "pair_dialog_tests.rs"]
mod tests;
