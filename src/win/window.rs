//! The window class, the message loop, and the window procedure.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::OnceLock;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DwmSetWindowAttribute,
};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITOR_DEFAULTTOPRIMARY, MONITORINFO,
    MonitorFromPoint,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    MOD_ALT, MOD_NOREPEAT, MOD_WIN, RegisterHotKey, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CS_DBLCLKS, CreateWindowExW, DefWindowProcW, DispatchMessageW, GWLP_USERDATA, GetMessageW,
    GetWindowLongPtrW, HTCAPTION, HTCLIENT, IDC_ARROW, LoadCursorW, MSG, RegisterClassExW,
    RegisterWindowMessageW, SW_SHOW, SetTimer, SetWindowLongPtrW, ShowWindow, TranslateMessage,
    WM_CLOSE, WM_CONTEXTMENU, WM_DISPLAYCHANGE, WM_DPICHANGED, WM_ENDSESSION, WM_ERASEBKGND,
    WM_EXITSIZEMOVE, WM_HOTKEY, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL,
    WM_NCHITTEST, WM_PAINT, WM_QUERYENDSESSION, WM_RBUTTONUP, WM_SETTINGCHANGE, WM_TIMER,
    WNDCLASSEXW, WS_EX_LAYERED, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};
use windows::core::BOOL;
use windows::core::{PCWSTR, w};

use super::app::{App, Reply};
use super::dialog;
use super::fetch::{Fetcher, WM_FETCH_DONE};
use super::instance::{self, Instance};
use super::pair::{Pairer, WM_PAIR_DONE};
use super::pair_dialog;
use super::popup;
use super::text::Typography;
use super::tray::{self, WM_TRAY};
use super::util::{hiword, loword, pcwstr, wide, x_of, y_of};
use crate::contract::{QUIT_MESSAGE, SHOW_MESSAGE, WINDOW_CLASS};
use crate::pair_form::plaintext_question;
use crate::placement::Edges;
use crate::prefs::{Prefs, load, prefs_path};
use crate::remote::address::RemoteAddress;
use crate::remote::pairing::PairCode;
use crate::scene::Spot;

pub const HOTKEY_ID: i32 = 1;
const TIMER_ID: usize = 1;
const TICK_MS: u32 = 1000;
/// `WM_MOUSELEAVE`, sent after `TrackMouseEvent` with `TME_LEAVE`.
const WM_MOUSELEAVE: u32 = 0x02A3;
/// The `V` of the shortcut `Win+Alt+U` is `U`.
const HOTKEY_KEY: u32 = 0x55;

struct Messages {
    quit: u32,
    show: u32,
    taskbar_created: u32,
}

static MESSAGES: OnceLock<Messages> = OnceLock::new();

fn register_messages() -> &'static Messages {
    MESSAGES.get_or_init(|| unsafe {
        let register = |name: &str| RegisterWindowMessageW(pcwstr(&wide(name)));
        Messages {
            quit: register(QUIT_MESSAGE),
            show: register(SHOW_MESSAGE),
            taskbar_created: register("TaskbarCreated"),
        }
    })
}

unsafe extern "system" fn monitor_edges(
    _: HMONITOR,
    _: HDC,
    rect: *mut RECT,
    data: LPARAM,
) -> BOOL {
    let list = unsafe { &mut *(data.0 as *mut Vec<Edges>) };
    let rect = unsafe { *rect };
    list.push(Edges {
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
    });
    BOOL(1)
}

fn monitors() -> Vec<Edges> {
    let mut list: Vec<Edges> = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(monitor_edges),
            LPARAM(&mut list as *mut Vec<Edges> as isize),
        );
    }
    list
}

fn primary_work_area() -> Edges {
    unsafe {
        let monitor = MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY);
        let mut info = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if GetMonitorInfoW(monitor, &mut info).as_bool() {
            let work = info.rcWork;
            return Edges {
                left: work.left,
                top: work.top,
                right: work.right,
                bottom: work.bottom,
            };
        }
    }
    Edges {
        left: 0,
        top: 0,
        right: 1920,
        bottom: 1040,
    }
}

fn dispatch(app: &mut App, msg: u32, wparam: WPARAM, lparam: LPARAM) -> Reply {
    let messages = register_messages();
    if msg == messages.quit {
        app.quit();
        return Reply::Done(0);
    }
    if msg == messages.show {
        app.show();
        return Reply::Done(0);
    }
    if msg == messages.taskbar_created {
        app.add_tray_icon();
        return Reply::Done(0);
    }
    match msg {
        WM_NCHITTEST => {
            let point = POINT {
                x: x_of(lparam.0),
                y: y_of(lparam.0),
            };
            let code = match app.hit_test(point) {
                Spot::Drag => HTCAPTION,
                Spot::Button(_) | Spot::Client => HTCLIENT,
            };
            Reply::Done(code as isize)
        }
        WM_ERASEBKGND => Reply::Done(1),
        WM_PAINT => {
            app.paint();
            Reply::Done(0)
        }
        WM_TIMER if wparam.0 == TIMER_ID => {
            app.tick();
            Reply::Done(0)
        }
        WM_FETCH_DONE => {
            app.take_results();
            Reply::Done(0)
        }
        WM_PAIR_DONE => match app.take_pairings() {
            Some(shown) => Reply::Dialog(shown),
            None => Reply::Done(0),
        },
        WM_TRAY => match loword(lparam.0) as u32 {
            WM_LBUTTONUP => {
                app.toggle_visible();
                Reply::Done(0)
            }
            WM_RBUTTONUP | WM_CONTEXTMENU => Reply::Menu(app.tray_menu_anchor()),
            _ => Reply::Done(0),
        },
        WM_HOTKEY if wparam.0 as i32 == HOTKEY_ID => {
            app.toggle_visible();
            Reply::Done(0)
        }
        WM_MOUSEMOVE => {
            if !app.tracking_mouse {
                app.tracking_mouse = true;
                let mut track = TRACKMOUSEEVENT {
                    cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: app.hwnd,
                    dwHoverTime: 0,
                };
                unsafe {
                    let _ = TrackMouseEvent(&mut track);
                }
            }
            app.mouse_moved(x_of(lparam.0), y_of(lparam.0));
            Reply::Done(0)
        }
        WM_MOUSELEAVE => {
            app.mouse_left();
            Reply::Done(0)
        }
        WM_LBUTTONDOWN => {
            app.mouse_down(x_of(lparam.0), y_of(lparam.0));
            Reply::Done(0)
        }
        WM_LBUTTONUP => app.mouse_up(x_of(lparam.0), y_of(lparam.0)),
        WM_MOUSEWHEEL => {
            app.wheel(hiword(wparam.0 as isize) as i16 as i32);
            Reply::Done(0)
        }
        WM_DPICHANGED => {
            let suggested = unsafe { *(lparam.0 as *const RECT) };
            app.dpi_changed(loword(wparam.0 as isize) as u32, suggested);
            Reply::Done(0)
        }
        WM_DISPLAYCHANGE => {
            app.rebuild_scene();
            Reply::Done(0)
        }
        WM_SETTINGCHANGE => {
            app.refresh_theme();
            Reply::Default
        }
        WM_EXITSIZEMOVE => {
            app.save_prefs();
            Reply::Done(0)
        }
        // The window is not closed by the system: it hides. Quitting is the
        // menu's job, and the installer's.
        WM_CLOSE => {
            app.run(crate::menu::Command::ToggleWindow);
            Reply::Done(0)
        }
        // Windows Restart Manager and installers ask whether the session
        // may end; this app never stands in the way.
        WM_QUERYENDSESSION => Reply::Done(1),
        WM_ENDSESSION => {
            if wparam.0 != 0 {
                app.save_prefs();
                tray::remove(app.hwnd);
                std::process::exit(0);
            }
            Reply::Done(0)
        }
        _ => Reply::Default,
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *const RefCell<App>;
    if ptr.is_null() {
        return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
    }
    let cell = unsafe { &*ptr };
    // A message sent while the app is busy (for example by a call it made to
    // Windows) gets the default treatment instead of a second borrow.
    let reply = match cell.try_borrow_mut() {
        Ok(mut app) => dispatch(&mut app, msg, wparam, lparam),
        Err(_) => Reply::Default,
    };
    match reply {
        Reply::Done(result) => LRESULT(result),
        Reply::Default => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
        Reply::Menu(anchor) => {
            open_menu(cell, hwnd, anchor);
            LRESULT(0)
        }
        Reply::Dialog(shown) => {
            show_dialog(cell, hwnd, shown);
            LRESULT(0)
        }
    }
}

/// The menu runs its own message loop; the app is not borrowed while it
/// does, so the timer and the fetch results keep being handled.
fn open_menu(cell: &RefCell<App>, hwnd: HWND, anchor: popup::Anchor) {
    let state = match cell.try_borrow() {
        Ok(app) => app.menu_state(),
        Err(_) => return,
    };
    if let Some(command) = popup::show(hwnd, &state, anchor) {
        let shown = match cell.try_borrow_mut() {
            Ok(mut app) => app.run(command),
            Err(_) => None,
        };
        if let Some(shown) = shown {
            show_dialog(cell, hwnd, shown);
        }
    }
}

/// A message box or a dialog runs its own message loop, so it is shown while
/// the app is not borrowed. A pairing the user agrees to starts after it
/// closes.
fn show_dialog(cell: &RefCell<App>, hwnd: HWND, shown: dialog::Dialog) {
    match shown {
        dialog::Dialog::Notice(text) => dialog::notice(hwnd, &text),
        dialog::Dialog::Pair { prefill } => pair_from_dialog(cell, hwnd, prefill.as_deref()),
    }
}

/// Asks for an address and a code, and starts the pairing they name.
fn pair_from_dialog(cell: &RefCell<App>, hwnd: HWND, prefill: Option<&str>) {
    // One dialog at a time: the dialog disables the window, but the tray
    // icon still reaches it, and so does a second `Pair…`.
    let free = cell
        .try_borrow_mut()
        .is_ok_and(|mut app| app.open_pair_dialog());
    if !free {
        return;
    }
    let shown = pair_dialog::run(hwnd, prefill);
    if let Ok(mut app) = cell.try_borrow_mut() {
        app.close_pair_dialog();
    }
    let entry = match shown {
        Ok(Some(entry)) => entry,
        // Cancelled.
        Ok(None) => return,
        Err(pair_dialog::Failed) => {
            dialog::notice(hwnd, "The pairing dialog could not be shown.");
            return;
        }
    };
    // The dialog took only a good address and a whole code. What it hands
    // over is checked again, so that this does not depend on the dialog.
    let (Ok(address), Some(code)) = (
        RemoteAddress::parse(&entry.address),
        PairCode::parse(&entry.code),
    ) else {
        dialog::notice(
            hwnd,
            "The address or the pairing code is not valid. No pairing was started.",
        );
        return;
    };
    if let Some(question) = plaintext_question(&address)
        && !dialog::confirm(hwnd, &question)
    {
        return;
    }
    if let Ok(mut app) = cell.try_borrow_mut() {
        app.begin_pairing(address, code);
    }
}

fn register_class() -> bool {
    unsafe {
        let Ok(module) = GetModuleHandleW(None) else {
            return false;
        };
        let class_name = wide(WINDOW_CLASS);
        let class = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            style: CS_DBLCLKS,
            lpfnWndProc: Some(window_proc),
            hInstance: module.into(),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            lpszClassName: PCWSTR(class_name.as_ptr()),
            ..Default::default()
        };
        RegisterClassExW(&class) != 0
    }
}

fn create_window(prefs: &Prefs, at: (i32, i32)) -> Option<HWND> {
    unsafe {
        let module = GetModuleHandleW(None).ok()?;
        let class_name = wide(WINDOW_CLASS);
        let mut ex_style = WS_EX_TOOLWINDOW | WS_EX_LAYERED;
        if prefs.always_on_top {
            ex_style |= WS_EX_TOPMOST;
        }
        if prefs.click_through {
            ex_style |= WS_EX_TRANSPARENT;
        }
        CreateWindowExW(
            ex_style,
            pcwstr(&class_name),
            w!("Ullage"),
            WS_POPUP,
            at.0,
            at.1,
            100,
            100,
            None,
            None,
            Some(module.into()),
            None,
        )
        .ok()
    }
}

/// Rounded corners, where Windows has them. This is best-effort: a window
/// without them is still correct.
fn decorate(hwnd: HWND) {
    unsafe {
        let preference = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &preference as *const _ as *const _,
            size_of_val(&preference) as u32,
        );
    }
}

pub fn run() {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    if let Instance::Running = instance::acquire() {
        instance::show_running();
        return;
    }
    register_messages();
    popup::follow_system_theme();

    let prefs_file = prefs_path(std::env::var_os("LOCALAPPDATA").as_deref());
    let prefs = prefs_file.as_deref().map(load).unwrap_or_default();
    let Ok(text) = Typography::new() else {
        return;
    };
    if !register_class() {
        return;
    }
    let primary = primary_work_area();
    let provisional = match (prefs.x, prefs.y) {
        (Some(x), Some(y)) => (x, y),
        _ => (primary.left, primary.top),
    };
    let Some(hwnd) = create_window(&prefs, provisional) else {
        return;
    };

    let saved = prefs.x.zip(prefs.y);
    let app = Box::new(RefCell::new(App::new(
        hwnd,
        prefs,
        prefs_file,
        Rc::new(text),
    )));
    let (fetcher, results) = Fetcher::spawn(hwnd);
    let (pairer, pairs) = Pairer::spawn(hwnd);
    {
        let mut app = app.borrow_mut();
        app.attach_fetcher(fetcher, results);
        app.attach_pairer(pairer, pairs);
        decorate(hwnd);
        app.place_initial(saved, &monitors(), primary);
        unsafe {
            app.shortcut = RegisterHotKey(
                Some(hwnd),
                HOTKEY_ID,
                MOD_WIN | MOD_ALT | MOD_NOREPEAT,
                HOTKEY_KEY,
            )
            .is_ok();
        }
        app.add_tray_icon();
        app.start_due_fetch();
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOW);
        }
        // Per-pixel alpha windows show nothing until they are drawn.
        app.rebuild_scene();
    }
    unsafe {
        // From here on the window procedure reaches the app.
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(app) as isize);
        SetTimer(Some(hwnd), TIMER_ID, TICK_MS, None);
        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}
