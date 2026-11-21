//! The Windows application.

mod app;
mod clipboard;
mod credstore;
mod dialog;
mod fetch;
mod gfx;
mod instance;
mod pair;
mod pair_dialog;
mod pair_paint;
mod popup;
mod registry;
mod surface;
mod text;
mod tray;
mod util;
mod window;

pub use window::run;
