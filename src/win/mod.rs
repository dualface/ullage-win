//! The Windows application.

mod app;
mod clipboard;
mod fetch;
mod gfx;
mod instance;
mod popup;
mod registry;
mod text;
mod tray;
mod util;
mod window;

pub use window::run;
