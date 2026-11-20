#![cfg_attr(windows, windows_subsystem = "windows")]
// Only the Windows build runs the window; elsewhere the platform-independent
// modules exist for their tests, so most of them are unused there.
#![cfg_attr(not(windows), allow(dead_code))]

mod compact;
mod contract;
mod display;
mod icons;
mod menu;
mod placement;
mod prefs;
mod scene;
mod state;
mod svgpath;
mod theme;
#[cfg(windows)]
mod win;

#[cfg(windows)]
fn main() {
    win::run();
}

/// The window is Windows only. Other platforms build the platform-independent
/// modules so that their tests run.
#[cfg(not(windows))]
fn main() {
    eprintln!("ullage-win runs on Windows only");
    std::process::exit(1);
}
