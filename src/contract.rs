//! Names other programs rely on. The installer finds, closes, and starts
//! the app through these, so they never change.

use std::path::Path;

/// Class name of the top-level window.
pub const WINDOW_CLASS: &str = "UllageWindow";
/// A mutex of this name plus the user's SID string makes the app run once
/// per user.
pub const MUTEX_PREFIX: &str = "Local\\UllageWindow-";
/// A window message that makes the app save its settings and exit.
pub const QUIT_MESSAGE: &str = "UllageWindowQuit";
/// A window message that makes the app show its window; a second instance
/// sends it to the first.
pub const SHOW_MESSAGE: &str = "UllageWindowShow";
/// The key under `HKCU` that starts programs at sign-in.
pub const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
/// The value of [`RUN_KEY`] that starts this app.
pub const RUN_VALUE: &str = "Ullage Window";
/// Where `Setup guide` goes.
pub const GUIDE_URL: &str = "https://github.com/dualface/ullage-cli#readme";

pub fn mutex_name(sid: &str) -> String {
    format!("{MUTEX_PREFIX}{sid}")
}

/// The data of the [`RUN_VALUE`]: the path of the program in double quotes.
pub fn run_value_data(exe: &Path) -> String {
    format!("\"{}\"", exe.display())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fixed_names_are_the_agreed_ones() {
        assert_eq!(WINDOW_CLASS, "UllageWindow");
        assert_eq!(MUTEX_PREFIX, "Local\\UllageWindow-");
        assert_eq!(QUIT_MESSAGE, "UllageWindowQuit");
        assert_eq!(RUN_VALUE, "Ullage Window");
        assert_eq!(RUN_KEY, r"Software\Microsoft\Windows\CurrentVersion\Run");
        assert_eq!(GUIDE_URL, "https://github.com/dualface/ullage-cli#readme");
    }

    #[test]
    fn the_mutex_name_ends_with_the_sid() {
        assert_eq!(
            mutex_name("S-1-5-21-1-2-3-1001"),
            "Local\\UllageWindow-S-1-5-21-1-2-3-1001"
        );
    }

    #[test]
    fn the_run_value_is_the_quoted_path() {
        assert_eq!(
            run_value_data(Path::new(r"C:\Program Files\Ullage\ullage-win.exe")),
            r#""C:\Program Files\Ullage\ullage-win.exe""#
        );
    }
}
