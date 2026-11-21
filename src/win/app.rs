//! The state of the running window and what it does about each event.

use std::io;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc::Receiver;
use std::time::Instant;

use chrono::Utc;
use ullage_cli::usage_view::usage_cards;
use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::Graphics::Direct2D::{
    D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1CreateFactory, ID2D1Factory,
};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, EndPaint, GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO,
    MonitorFromWindow, PAINTSTRUCT,
};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture, UnregisterHotKey};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{
    DestroyIcon, DestroyWindow, GWL_EXSTYLE, GetCursorPos, GetWindowLongPtrW, GetWindowRect, HICON,
    HWND_NOTOPMOST, HWND_TOPMOST, IsWindowVisible, PostQuitMessage, SW_HIDE, SW_SHOW,
    SW_SHOWNORMAL, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SetForegroundWindow,
    SetWindowLongPtrW, SetWindowPos, ShowWindow, WS_EX_TRANSPARENT,
};
use windows::core::w;

use super::clipboard;
use super::credstore::CredStore;
use super::dialog::Dialog;
use super::fetch::{Delivery, Fetcher, Source};
use super::gfx::Renderer;
use super::pair::{PairResult, Pairer};
use super::popup::{Anchor, Opens};
use super::registry;
use super::text::Typography;
use super::tray;
use super::util::{pcwstr, wide};
use crate::contract::GUIDE_URL;
use crate::menu::{Command, MenuState};
use crate::pair_form::OnePlace;
use crate::placement::{
    Edges, context_menu_origin, keep_inside, restore_position, to_px, window_size_px,
};
use crate::prefs::{Connection, Prefs, save};
use crate::remote::address::RemoteAddress;
use crate::remote::pairing::{self, ApplyError, ForgetError, PairCode, PairingEpoch};
use crate::remote::store::{Credential, credential_for};
use crate::scene::{self, Body, Input, Scene, Spot, Target, WORK_AREA_MARGIN};
use crate::state::{Schedule, State, countdown};
use crate::theme::Mode;

/// What the window procedure does with a message after the app saw it.
pub enum Reply {
    /// Handled, with this result.
    Done(isize),
    /// Left to the default procedure.
    Default,
    /// Open the menu. The menu runs its own message loop, so the window
    /// procedure opens it after it has let go of the app.
    Menu(Anchor),
    /// Show a message box, after the app has been let go of.
    Dialog(Dialog),
}

pub struct App {
    pub hwnd: HWND,
    pub prefs: Prefs,
    pub prefs_path: Option<PathBuf>,
    state: State,
    schedule: Schedule,
    mode: Mode,
    pub dpi: u32,
    text: Rc<Typography>,
    renderer: Option<Renderer>,
    scene: Scene,
    scroll: f32,
    hover: Option<Target>,
    pressed: Option<Target>,
    pub tracking_mouse: bool,
    pub shortcut: bool,
    fetcher: Option<Fetcher>,
    results: Option<Receiver<Delivery>>,
    /// Counts the changes of connection. A result of an earlier count is of
    /// a connection that was left, and is dropped.
    generation: u32,
    pairer: Option<Pairer>,
    pairs: Option<Receiver<PairResult>>,
    /// Which pairing may still be kept.
    pairing: PairingEpoch,
    /// Whether the pairing dialog is open. It runs its own message loop, so
    /// a second `Pair…` can come in while it is.
    pair_dialog: OnePlace,
    tray_icon: Option<HICON>,
    tray_mode: Mode,
}

fn to_dip(pixels: i32, dpi: u32) -> f32 {
    pixels as f32 * 96.0 / dpi as f32
}

impl App {
    pub fn new(
        hwnd: HWND,
        prefs: Prefs,
        prefs_path: Option<PathBuf>,
        text: Rc<Typography>,
    ) -> Self {
        let now = Instant::now();
        let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
        let mode = registry::apps_mode();
        let state = State::Loading;
        let scene = scene::build(
            &Input {
                layout: prefs.layout,
                text_size: prefs.text_size,
                body: Body::Loading,
                countdown: (0.0, 0),
                pinned: prefs.always_on_top,
                hover: None,
                max_window_height: f32::MAX,
            },
            &*text,
        );
        Self {
            hwnd,
            prefs,
            prefs_path,
            state,
            schedule: Schedule::new(now),
            mode,
            dpi,
            text,
            renderer: None,
            scene,
            scroll: 0.0,
            hover: None,
            pressed: None,
            tracking_mouse: false,
            shortcut: false,
            fetcher: None,
            results: None,
            generation: 0,
            pairer: None,
            pairs: None,
            pairing: PairingEpoch::default(),
            pair_dialog: OnePlace::default(),
            tray_icon: None,
            tray_mode: registry::system_mode(),
        }
    }

    pub fn attach_fetcher(&mut self, fetcher: Fetcher, results: Receiver<Delivery>) {
        self.fetcher = Some(fetcher);
        self.results = Some(results);
    }

    pub fn attach_pairer(&mut self, pairer: Pairer, results: Receiver<PairResult>) {
        self.pairer = Some(pairer);
        self.pairs = Some(results);
    }

    pub fn visible(&self) -> bool {
        unsafe { IsWindowVisible(self.hwnd).as_bool() }
    }

    // ---- geometry ----

    fn window_rect(&self) -> RECT {
        let mut rect = RECT::default();
        unsafe {
            let _ = GetWindowRect(self.hwnd, &mut rect);
        }
        rect
    }

    fn work_area(&self) -> Edges {
        unsafe {
            let monitor = MonitorFromWindow(self.hwnd, MONITOR_DEFAULTTONEAREST);
            let mut info = MONITORINFO {
                cbSize: size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            if GetMonitorInfoW(monitor, &mut info).as_bool() {
                let work = info.rcWork;
                Edges {
                    left: work.left,
                    top: work.top,
                    right: work.right,
                    bottom: work.bottom,
                }
            } else {
                Edges {
                    left: 0,
                    top: 0,
                    right: 1920,
                    bottom: 1040,
                }
            }
        }
    }

    pub fn window_size_px(&self) -> (i32, i32) {
        window_size_px((self.scene.width, self.scene.height), self.dpi)
    }

    /// Builds the scene from the current state and time, and resizes the
    /// window to it.
    pub fn rebuild_scene(&mut self) {
        let work = self.work_area();
        let max_height = to_dip(work.bottom - work.top, self.dpi) - WORK_AREA_MARGIN;
        let cards = match self.state.snapshots() {
            Some(snapshots) => usage_cards(snapshots, Utc::now()),
            None => Vec::new(),
        };
        let body = match &self.state {
            State::Loading => Body::Loading,
            State::Ready(snapshots) | State::Stalled(snapshots) if snapshots.is_empty() => {
                Body::Empty
            }
            State::Ready(_) | State::Stalled(_) => Body::Cards(&cards),
            State::Unavailable(failure) => Body::Unavailable(failure),
        };
        let scene = scene::build(
            &Input {
                layout: self.prefs.layout,
                text_size: self.prefs.text_size,
                body,
                countdown: countdown(&self.state, &self.schedule, Instant::now()),
                pinned: self.prefs.always_on_top,
                hover: self.hover,
                max_window_height: max_height,
            },
            &*self.text,
        );
        self.scene = scene;
        self.scroll = self.scroll.clamp(0.0, self.scene.max_scroll());
        self.fit_window();
        self.redraw();
    }

    /// Moves the window just far enough to lie inside the work area of the
    /// nearest monitor, and leaves it where it is when it already does. Used
    /// only when the monitors or the work area change, and before the window
    /// is shown: calling it on every tick would pull a window back while the
    /// user drags it, or off a place across two monitors.
    pub fn ensure_on_screen(&mut self) {
        let rect = self.window_rect();
        let size = (rect.right - rect.left, rect.bottom - rect.top);
        let (x, y) = keep_inside(self.work_area(), (rect.left, rect.top), size);
        if (x, y) == (rect.left, rect.top) {
            return;
        }
        unsafe {
            let _ = SetWindowPos(
                self.hwnd,
                None,
                x,
                y,
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
    }

    /// Gives the window the size of the scene, moving it only as far as
    /// needed to keep it on the screen.
    fn fit_window(&mut self) {
        let (width, height) = self.window_size_px();
        let rect = self.window_rect();
        if rect.right - rect.left == width && rect.bottom - rect.top == height {
            return;
        }
        let (x, y) = keep_inside(self.work_area(), (rect.left, rect.top), (width, height));
        unsafe {
            let _ = SetWindowPos(
                self.hwnd,
                None,
                x,
                y,
                width,
                height,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
        let dpi = self.dpi;
        if let Some(renderer) = &mut self.renderer {
            let _ = renderer.resize((width, height), dpi);
        }
    }

    /// Puts the window where it was saved when that place is still on a
    /// monitor, and at the top right of the primary work area otherwise.
    pub fn place_initial(&mut self, saved: Option<(i32, i32)>, monitors: &[Edges], primary: Edges) {
        self.rebuild_scene();
        let size = self.window_size_px();
        let (x, y) = restore_position(saved, size, monitors, primary, self.dpi);
        unsafe {
            let _ = SetWindowPos(
                self.hwnd,
                None,
                x,
                y,
                size.0,
                size.1,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
            // The monitor the window landed on may have another scale.
            let dpi = GetDpiForWindow(self.hwnd).max(96);
            if dpi != self.dpi {
                self.dpi = dpi;
                self.rebuild_scene();
            }
        }
    }

    /// Draws the window again. A window with per-pixel alpha gets no
    /// `WM_PAINT`, so every change of what it shows ends here.
    pub fn redraw(&mut self) {
        if !self.visible() {
            return;
        }
        if self.renderer.is_none() {
            let (width, height) = self.window_size_px();
            self.renderer = Renderer::new((width, height), self.dpi, self.text.clone()).ok();
        }
        let opacity = (self.prefs.opacity * 255 / 100) as u8;
        let (size, dpi) = (self.window_size_px(), self.dpi);
        if let Some(renderer) = &mut self.renderer
            // The window may have reached its size without a call that
            // resizes the bitmap, as when Windows suggests the new size on a
            // change of scale; the bitmap is made to fit before each draw.
            && renderer.resize(size, dpi).is_ok()
            && renderer
                .draw(self.hwnd, &self.scene, self.mode, self.scroll, opacity)
                .is_err()
        {
            // The surface is made again on the next draw.
            self.renderer = None;
        }
    }

    pub fn paint(&mut self) {
        unsafe {
            let mut paint = PAINTSTRUCT::default();
            BeginPaint(self.hwnd, &mut paint);
            let _ = EndPaint(self.hwnd, &paint);
        }
        self.redraw();
    }

    pub fn dpi_changed(&mut self, dpi: u32, suggested: RECT) {
        self.dpi = dpi.max(96);
        unsafe {
            let _ = SetWindowPos(
                self.hwnd,
                None,
                suggested.left,
                suggested.top,
                suggested.right - suggested.left,
                suggested.bottom - suggested.top,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
        self.rebuild_scene();
    }

    // ---- time and data ----

    /// The remote daemon the window asks, when it asks one.
    fn remote(&self) -> Option<RemoteAddress> {
        match self.prefs.connection {
            Connection::Remote => self.prefs.remote_address.clone(),
            Connection::Local => None,
        }
    }

    fn start_fetch(&mut self) {
        let source = self.remote().map_or(Source::Local, Source::Remote);
        if let Some(fetcher) = &self.fetcher {
            self.schedule.start();
            fetcher.request(source, self.generation);
        }
    }

    /// The window now asks another daemon: what it knew of the old one is
    /// gone, results still on their way from it are dropped, and the new one
    /// is asked at once.
    fn connection_changed(&mut self) {
        // A pairing that is still on its way was asked for under the old
        // connection. Its result would write a credential and switch to the
        // remote daemon, undoing what the user has just done.
        self.pairing.invalidate();
        self.generation = self.generation.wrapping_add(1);
        self.state = State::Loading;
        self.schedule = Schedule::new(Instant::now());
        self.scroll = 0.0;
        self.start_due_fetch();
        self.rebuild_scene();
    }

    /// Starts the first fetch, and any later one that is due.
    pub fn start_due_fetch(&mut self) {
        if self.schedule.due(Instant::now()) {
            self.start_fetch();
        }
    }

    pub fn tick(&mut self) {
        self.refresh_theme();
        self.take_results();
        self.start_due_fetch();
        if self.visible() {
            self.rebuild_scene();
        }
    }

    /// Takes every result the fetch thread has sent.
    pub fn take_results(&mut self) {
        let Some(results) = &self.results else {
            return;
        };
        let deliveries: Vec<Delivery> = results.try_iter().collect();
        let remote = self.remote();
        for delivery in deliveries {
            // The answer of a connection the user has left.
            if delivery.generation != self.generation {
                continue;
            }
            let state = std::mem::replace(&mut self.state, State::Loading);
            self.state = state.after_fetch(delivery.outcome, remote.as_ref());
            self.schedule.finish(Instant::now());
        }
        // A refresh asked for during the fetch is due now.
        self.start_due_fetch();
        if self.visible() {
            self.rebuild_scene();
        }
    }

    pub fn refresh_now(&mut self) {
        self.schedule.request_now(Instant::now());
        self.start_due_fetch();
        self.rebuild_scene();
    }

    pub fn refresh_theme(&mut self) {
        let mode = registry::apps_mode();
        if mode != self.mode {
            self.mode = mode;
            self.redraw();
        }
        if registry::system_mode() != self.tray_mode {
            self.update_tray_icon();
        }
    }

    // ---- tray ----

    fn make_tray_icon(&self) -> Option<HICON> {
        unsafe {
            let factory: ID2D1Factory =
                D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None).ok()?;
            tray::make_icon(&factory, self.text.clone(), registry::system_mode()).ok()
        }
    }

    /// Adds the icon to the notification area; also after the taskbar was
    /// recreated.
    pub fn add_tray_icon(&mut self) {
        self.tray_mode = registry::system_mode();
        if self.tray_icon.is_none() {
            self.tray_icon = self.make_tray_icon();
        }
        if let Some(icon) = self.tray_icon {
            tray::add(self.hwnd, icon);
        }
    }

    fn update_tray_icon(&mut self) {
        self.tray_mode = registry::system_mode();
        if let Some(icon) = self.make_tray_icon() {
            tray::update(self.hwnd, icon);
            if let Some(old) = self.tray_icon.replace(icon) {
                unsafe {
                    let _ = DestroyIcon(old);
                }
            }
        }
    }

    // ---- mouse ----

    fn point_in_dips(&self, screen: POINT) -> (f32, f32) {
        let rect = self.window_rect();
        (
            to_dip(screen.x - rect.left, self.dpi),
            to_dip(screen.y - rect.top, self.dpi),
        )
    }

    pub fn hit_test(&self, screen: POINT) -> Spot {
        let (x, y) = self.point_in_dips(screen);
        self.scene.spot(x, y, self.scroll)
    }

    fn client_dips(&self, x: i32, y: i32) -> (f32, f32) {
        (to_dip(x, self.dpi), to_dip(y, self.dpi))
    }

    pub fn mouse_moved(&mut self, x: i32, y: i32) {
        let (dx, dy) = self.client_dips(x, y);
        let hover = self.scene.button_target(dx, dy, self.scroll);
        if hover != self.hover {
            self.hover = hover;
            self.rebuild_scene();
        }
    }

    pub fn mouse_left(&mut self) {
        self.tracking_mouse = false;
        if self.hover.take().is_some() {
            self.rebuild_scene();
        }
    }

    pub fn mouse_down(&mut self, x: i32, y: i32) {
        let (dx, dy) = self.client_dips(x, y);
        self.pressed = self.scene.button_target(dx, dy, self.scroll);
        if self.pressed.is_some() {
            unsafe {
                SetCapture(self.hwnd);
            }
        }
    }

    /// A press that ends on the button it began on is a click.
    pub fn mouse_up(&mut self, x: i32, y: i32) -> Reply {
        let pressed = self.pressed.take();
        unsafe {
            let _ = ReleaseCapture();
        }
        let (dx, dy) = self.client_dips(x, y);
        match (pressed, self.scene.button_target(dx, dy, self.scroll)) {
            (Some(pressed), Some(now)) if pressed == now => self.click(pressed),
            _ => Reply::Done(0),
        }
    }

    pub fn wheel(&mut self, delta: i32) {
        let notches = delta as f32 / 120.0;
        let scroll =
            (self.scroll - notches * scene::SCROLL_STEP).clamp(0.0, self.scene.max_scroll());
        if scroll != self.scroll {
            self.scroll = scroll;
            self.redraw();
        }
    }

    fn click(&mut self, target: Target) -> Reply {
        match target {
            // These commands open no message box.
            Target::Pin => {
                self.run(Command::TogglePin);
            }
            Target::Hide => self.hide(),
            Target::Expand => {
                self.run(Command::ToggleCompact);
            }
            Target::More => {
                let rect = self
                    .scene
                    .hits
                    .iter()
                    .find(|hit| hit.target == Target::More)
                    .map(|hit| hit.rect);
                let window = self.window_rect();
                if let Some(rect) = rect {
                    return Reply::Menu(Anchor {
                        x: window.left + to_px(rect.right(), self.dpi),
                        y: window.top + to_px(rect.bottom(), self.dpi),
                        opens: Opens::BelowButton,
                    });
                }
            }
            Target::Copy => {
                if let State::Unavailable(failure) = &self.state {
                    let _ = clipboard::set_text(self.hwnd, failure.command);
                }
            }
            Target::Retry => self.refresh_now(),
            Target::Guide => unsafe {
                let url = wide(GUIDE_URL);
                ShellExecuteW(
                    Some(self.hwnd),
                    w!("open"),
                    pcwstr(&url),
                    None,
                    None,
                    SW_SHOWNORMAL,
                );
            },
        }
        Reply::Done(0)
    }

    // ---- commands ----

    pub fn menu_state(&self) -> MenuState {
        MenuState {
            window_visible: self.visible(),
            layout: self.prefs.layout,
            always_on_top: self.prefs.always_on_top,
            click_through: self.prefs.click_through,
            text_size: self.prefs.text_size,
            opacity: self.prefs.opacity,
            autostart: registry::autostart_enabled(),
            shortcut: self.shortcut,
            connection: self.prefs.connection,
            remote: self.prefs.remote_address.clone(),
            remote_paired: self.remote_paired(),
        }
    }

    pub fn tray_menu_anchor(&self) -> Anchor {
        let mut point = POINT::default();
        unsafe {
            let _ = GetCursorPos(&mut point);
        }
        Anchor {
            x: point.x,
            y: point.y,
            opens: Opens::AboveTray,
        }
    }

    /// The menu of a right click on the window, or of Shift+F10 or the menu
    /// key: `point` is what `WM_CONTEXTMENU` carries.
    pub fn context_menu_anchor(&self, point: (i32, i32)) -> Anchor {
        let window = self.window_rect();
        let (x, y) = context_menu_origin(point, (window.left, window.top), self.dpi);
        Anchor {
            x,
            y,
            opens: Opens::AtCursor,
        }
    }

    pub fn run(&mut self, command: Command) -> Option<Dialog> {
        let mut dialog = None;
        match command {
            Command::ToggleWindow => self.toggle_visible(),
            Command::ToggleCompact => {
                self.prefs.toggle_compact();
                self.layout_changed();
            }
            Command::TogglePin => {
                self.prefs.always_on_top = !self.prefs.always_on_top;
                self.apply_topmost();
                self.save_prefs();
                self.rebuild_scene();
            }
            Command::ToggleClickThrough => {
                self.prefs.click_through = !self.prefs.click_through;
                self.apply_click_through();
                self.save_prefs();
            }
            Command::SetLayout(layout) => {
                self.prefs.set_layout(layout);
                self.layout_changed();
            }
            Command::SetTextSize(size) => {
                self.prefs.text_size = size;
                self.layout_changed();
            }
            Command::SetOpacity(opacity) => {
                self.prefs.opacity = opacity;
                self.save_prefs();
                self.redraw();
            }
            Command::SetConnection(connection) => dialog = self.set_connection(connection),
            Command::Pair => dialog = self.pair(),
            Command::ForgetRemote => dialog = self.forget_remote(),
            Command::Refresh => self.refresh_now(),
            Command::ToggleAutostart => {
                let _ = registry::set_autostart(!registry::autostart_enabled());
            }
            Command::Quit => self.quit(),
        }
        dialog
    }

    /// Whether a token is saved for the address of the settings. It is the
    /// same test a fetch makes before it sends anything.
    fn remote_paired(&self) -> bool {
        self.prefs
            .remote_address
            .as_ref()
            .is_some_and(|address| credential_for(&CredStore, address).is_some())
    }

    /// Saves `next` as the settings. The caller makes it the settings in
    /// memory only when this worked, so a change is never shown that the next
    /// start would not have.
    fn persist(&self, next: &Prefs) -> io::Result<()> {
        match &self.prefs_path {
            Some(path) => save(path, next),
            None => Err(io::Error::other("there is no settings file")),
        }
    }

    fn set_connection(&mut self, connection: Connection) -> Option<Dialog> {
        if connection == self.prefs.connection {
            return None;
        }
        if connection == Connection::Remote && !self.remote_paired() {
            return Some(Dialog::Notice(
                "This device is not paired with a remote daemon. Choose Pair\u{2026}.".into(),
            ));
        }
        let mut next = self.prefs.clone();
        next.connection = connection;
        if self.persist(&next).is_err() {
            return Some(Dialog::Notice(
                "The settings could not be saved. The connection was not changed.".into(),
            ));
        }
        self.prefs = next;
        self.connection_changed();
        None
    }

    /// Opens the pairing dialog, with the address of the settings in it. A
    /// pairing is kept in the settings, so without a place to save them none
    /// is started: it would still use up the code and register this device at
    /// the daemon.
    fn pair(&mut self) -> Option<Dialog> {
        if self.prefs_path.is_none() {
            return Some(Dialog::Notice(
                "There is no place to save the settings, so no pairing was started.".into(),
            ));
        }
        Some(Dialog::Pair {
            prefill: self.prefs.remote_address.as_ref().map(ToString::to_string),
        })
    }

    /// Takes the one place for the pairing dialog. `false` when it is taken.
    pub fn open_pair_dialog(&mut self) -> bool {
        self.pair_dialog.take()
    }

    pub fn close_pair_dialog(&mut self) {
        self.pair_dialog.give_back();
    }

    /// The user typed an address and a code and agreed: ask the daemon, on a
    /// thread.
    pub fn begin_pairing(&mut self, address: RemoteAddress, code: PairCode) {
        let epoch = self.pairing.begin();
        if let Some(pairer) = &self.pairer {
            pairer.start(address, code, epoch);
        }
    }

    /// Takes every pairing that has ended. A new token and its address are
    /// kept as one step, and the window then asks that daemon.
    pub fn take_pairings(&mut self) -> Option<Dialog> {
        let results = self.pairs.as_ref()?;
        let ended: Vec<PairResult> = results.try_iter().collect();
        let mut dialog = None;
        for ended in ended {
            // A pairing the user has made stale, by forgetting the remote
            // daemon or choosing a connection or starting another pairing, is
            // dropped, whether it worked or not. The daemon may have
            // registered this device for it: `ullage device list` shows it.
            if !self.pairing.is_current(ended.epoch) {
                continue;
            }
            let address = ended.address;
            dialog = Some(Dialog::Notice(match ended.outcome {
                Err(error) => format!("Pairing with {address} failed. {error}"),
                Ok(token) => {
                    let credential = Credential {
                        address: address.clone(),
                        token,
                    };
                    let mut next = self.prefs.clone();
                    next.connection = Connection::Remote;
                    next.remote_address = Some(address.clone());
                    let kept = pairing::apply(&CredStore, &credential, || self.persist(&next));
                    match kept {
                        Ok(()) => {
                            self.prefs = next;
                            self.connection_changed();
                            format!("Paired with {address}. The window now shows its usage.")
                        }
                        Err(ApplyError::Store) => format!(
                            "Pairing with {address} worked, but the device token could not be \
                             saved in the Windows Credential Manager. Nothing was changed."
                        ),
                        Err(ApplyError::Settings) => format!(
                            "Pairing with {address} worked, but the settings could not be \
                             saved. Nothing was changed."
                        ),
                        Err(ApplyError::Rollback) => format!(
                            "Pairing with {address} worked, but the settings could not be \
                             saved, and an earlier pairing could not be put back. Pair again \
                             with Pair\u{2026}."
                        ),
                    }
                }
            }));
        }
        dialog
    }

    /// Deletes the token and the address, and goes back to the local daemon.
    fn forget_remote(&mut self) -> Option<Dialog> {
        let mut next = self.prefs.clone();
        next.connection = Connection::Local;
        next.remote_address = None;
        let notice = match pairing::forget(&CredStore, || self.persist(&next)) {
            Ok(()) => {
                self.prefs = next;
                self.connection_changed();
                return None;
            }
            Err(ForgetError::Read) => {
                "The device token could not be read from the Windows Credential Manager. \
                 Nothing was changed."
            }
            Err(ForgetError::Delete) => {
                "The device token could not be removed from the Windows Credential Manager. \
                 Nothing was changed."
            }
            Err(ForgetError::Settings) => "The settings could not be saved. Nothing was changed.",
            Err(ForgetError::Rollback) => {
                "The settings could not be saved, and the device token could not be put back. \
                 Pair again with Pair\u{2026}."
            }
        };
        Some(Dialog::Notice(notice.into()))
    }

    fn layout_changed(&mut self) {
        self.scroll = 0.0;
        self.rebuild_scene();
        self.save_prefs();
    }

    pub fn apply_topmost(&self) {
        unsafe {
            let _ = SetWindowPos(
                self.hwnd,
                Some(if self.prefs.always_on_top {
                    HWND_TOPMOST
                } else {
                    HWND_NOTOPMOST
                }),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
    }

    pub fn apply_click_through(&self) {
        unsafe {
            let style = GetWindowLongPtrW(self.hwnd, GWL_EXSTYLE);
            let flag = WS_EX_TRANSPARENT.0 as isize;
            let style = if self.prefs.click_through {
                style | flag
            } else {
                style & !flag
            };
            SetWindowLongPtrW(self.hwnd, GWL_EXSTYLE, style);
        }
    }

    pub fn toggle_visible(&mut self) {
        if self.visible() {
            self.hide();
        } else {
            self.show();
        }
    }

    pub fn show(&mut self) {
        // The monitors may have changed while the window was hidden; move it
        // before it shows, so it does not flash off the screen.
        self.ensure_on_screen();
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_SHOW);
            let _ = SetForegroundWindow(self.hwnd);
        }
        self.rebuild_scene();
    }

    fn hide(&mut self) {
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_HIDE);
        }
    }

    // ---- settings and exit ----

    /// Remembers where the window is.
    pub fn remember_position(&mut self) {
        let rect = self.window_rect();
        self.prefs.x = Some(rect.left);
        self.prefs.y = Some(rect.top);
    }

    pub fn save_prefs(&mut self) {
        self.remember_position();
        if let Some(path) = &self.prefs_path {
            let _ = save(path, &self.prefs);
        }
    }

    /// Saves the settings and takes the window, the icon and the shortcut
    /// away; the message loop ends.
    pub fn quit(&mut self) {
        self.save_prefs();
        tray::remove(self.hwnd);
        unsafe {
            let _ = UnregisterHotKey(Some(self.hwnd), super::window::HOTKEY_ID);
            let _ = DestroyWindow(self.hwnd);
            PostQuitMessage(0);
        }
    }
}
