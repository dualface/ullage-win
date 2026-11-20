//! The state of the running window and what it does about each event.

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

use super::fetch::{Fetcher, Outcome};
use super::gfx::Renderer;
use super::popup::Anchor;
use super::registry;
use super::text::Typography;
use super::tray;
use super::util::{pcwstr, wide};
use crate::contract::GUIDE_URL;
use crate::menu::{Command, MenuState};
use crate::placement::{Edges, keep_inside, restore_position};
use crate::prefs::{Prefs, save};
use crate::scene::{self, Body, Input, SHADOW_MARGIN, Scene, Spot, Target, WORK_AREA_MARGIN};
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
    results: Option<Receiver<Outcome>>,
    tray_icon: Option<HICON>,
    tray_mode: Mode,
}

fn to_dip(pixels: i32, dpi: u32) -> f32 {
    pixels as f32 * 96.0 / dpi as f32
}

fn to_px(dip: f32, dpi: u32) -> i32 {
    (dip * dpi as f32 / 96.0).ceil() as i32
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
            tray_icon: None,
            tray_mode: registry::system_mode(),
        }
    }

    pub fn attach_fetcher(&mut self, fetcher: Fetcher, results: Receiver<Outcome>) {
        self.fetcher = Some(fetcher);
        self.results = Some(results);
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
        // The window is the scene and the transparent margin of its shadow.
        (
            to_px(self.scene.width + 2.0 * SHADOW_MARGIN, self.dpi),
            to_px(self.scene.height + 2.0 * SHADOW_MARGIN, self.dpi),
        )
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

    /// Gives the window the size of the scene, moving it only as far as
    /// needed to keep it on the screen.
    fn fit_window(&mut self) {
        let (width, height) = self.window_size_px();
        let rect = self.window_rect();
        if rect.right - rect.left == width && rect.bottom - rect.top == height {
            return;
        }
        // The shadow may reach past the work area; the window may not.
        let margin = to_px(SHADOW_MARGIN, self.dpi);
        let work = self.work_area().grown(margin);
        let (x, y) = keep_inside(work, (rect.left, rect.top), (width, height));
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
        let margin = to_px(SHADOW_MARGIN, self.dpi);
        let (x, y) = restore_position(saved, size, monitors, primary.grown(margin), self.dpi);
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

    fn start_fetch(&mut self) {
        if let Some(fetcher) = &self.fetcher {
            self.schedule.start();
            fetcher.request();
        }
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
        let outcomes: Vec<Outcome> = results.try_iter().collect();
        for outcome in outcomes {
            let state = std::mem::replace(&mut self.state, State::Loading);
            self.state = state.after_fetch(outcome);
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
            to_dip(screen.x - rect.left, self.dpi) - SHADOW_MARGIN,
            to_dip(screen.y - rect.top, self.dpi) - SHADOW_MARGIN,
        )
    }

    pub fn hit_test(&self, screen: POINT) -> Spot {
        let (x, y) = self.point_in_dips(screen);
        self.scene.spot(x, y, self.scroll)
    }

    fn client_dips(&self, x: i32, y: i32) -> (f32, f32) {
        (
            to_dip(x, self.dpi) - SHADOW_MARGIN,
            to_dip(y, self.dpi) - SHADOW_MARGIN,
        )
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
            Target::Pin => self.run(Command::TogglePin),
            Target::Hide => self.hide(),
            Target::Expand => self.run(Command::ToggleCompact),
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
                        x: window.left + to_px(SHADOW_MARGIN + rect.right(), self.dpi),
                        y: window.top + to_px(SHADOW_MARGIN + rect.bottom(), self.dpi),
                        below_button: true,
                    });
                }
            }
            Target::Copy => {
                if let State::Unavailable(failure) = &self.state {
                    let _ = super::clipboard::set_text(self.hwnd, failure.command);
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
            below_button: false,
        }
    }

    pub fn run(&mut self, command: Command) {
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
            Command::Refresh => self.refresh_now(),
            Command::ToggleAutostart => {
                let _ = registry::set_autostart(!registry::autostart_enabled());
            }
            Command::Quit => self.quit(),
        }
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
