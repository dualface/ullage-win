//! The settings file `%LOCALAPPDATA%\Ullage\window.json`.
//!
//! Anything wrong with the file (missing, unreadable, not JSON, another
//! version) reads as the defaults; the window never shows an error for it.
//! Whether the app starts at sign-in is not stored here: the registry is the
//! source of truth for that.

use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const VERSION: u32 = 1;
pub const TEXT_SIZES: [u32; 4] = [12, 13, 14, 16];
pub const OPACITIES: [u32; 5] = [100, 95, 90, 80, 70];
pub const DEFAULT_TEXT_SIZE: u32 = 13;
pub const DEFAULT_OPACITY: u32 = 95;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Layout {
    /// Two cards per row.
    #[default]
    Columns,
    /// One card per row.
    Single,
    /// One line per account.
    Compact,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    pub version: u32,
    /// Window position in physical pixels of the virtual desktop; `None`
    /// puts the window at the top right of the primary work area.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub y: Option<i32>,
    pub layout: Layout,
    /// The last expanded layout: `columns` or `single`.
    pub expanded_layout: Layout,
    pub always_on_top: bool,
    pub click_through: bool,
    pub text_size: u32,
    pub opacity: u32,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            version: VERSION,
            x: None,
            y: None,
            layout: Layout::Columns,
            expanded_layout: Layout::Columns,
            always_on_top: true,
            click_through: false,
            text_size: DEFAULT_TEXT_SIZE,
            opacity: DEFAULT_OPACITY,
        }
    }
}

impl Prefs {
    /// Replaces every value outside what the menus offer with its default.
    fn sanitized(mut self) -> Self {
        if !TEXT_SIZES.contains(&self.text_size) {
            self.text_size = DEFAULT_TEXT_SIZE;
        }
        if !OPACITIES.contains(&self.opacity) {
            self.opacity = DEFAULT_OPACITY;
        }
        if self.expanded_layout == Layout::Compact {
            self.expanded_layout = Layout::Columns;
        }
        self
    }

    /// Switches to `layout`, remembering it as the expanded layout when it
    /// is one.
    pub fn set_layout(&mut self, layout: Layout) {
        self.layout = layout;
        if layout != Layout::Compact {
            self.expanded_layout = layout;
        }
    }

    /// The compact strip, or the layout the user last expanded to.
    pub fn toggle_compact(&mut self) {
        let next = if self.layout == Layout::Compact {
            self.expanded_layout
        } else {
            Layout::Compact
        };
        self.set_layout(next);
    }
}

/// `%LOCALAPPDATA%\Ullage\window.json`, or `None` without `LOCALAPPDATA`.
pub fn prefs_path(local_app_data: Option<&OsStr>) -> Option<PathBuf> {
    let base = local_app_data.filter(|value| !value.is_empty())?;
    Some(Path::new(base).join("Ullage").join("window.json"))
}

pub fn load(path: &Path) -> Prefs {
    let Ok(text) = fs::read_to_string(path) else {
        return Prefs::default();
    };
    match serde_json::from_str::<Prefs>(&text) {
        Ok(prefs) if prefs.version == VERSION => prefs.sanitized(),
        _ => Prefs::default(),
    }
}

/// Writes the file through a temporary file and a rename, so a crash never
/// leaves half a file behind.
pub fn save(path: &Path, prefs: &Prefs) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut temp = path.as_os_str().to_owned();
    temp.push(".tmp");
    let temp = PathBuf::from(temp);
    let mut text = serde_json::to_string(prefs).map_err(io::Error::other)?;
    text.push('\n');
    fs::write(&temp, text)?;
    fs::rename(&temp, path).inspect_err(|_| {
        let _ = fs::remove_file(&temp);
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_spec() {
        let prefs = Prefs::default();
        assert_eq!(prefs.layout, Layout::Columns);
        assert_eq!(prefs.expanded_layout, Layout::Columns);
        assert!(prefs.always_on_top);
        assert!(!prefs.click_through);
        assert_eq!(prefs.text_size, 13);
        assert_eq!(prefs.opacity, 95);
        assert_eq!((prefs.x, prefs.y), (None, None));
    }

    #[test]
    fn the_path_is_under_local_app_data() {
        let path = prefs_path(Some(OsStr::new("base"))).unwrap();
        assert_eq!(path, Path::new("base").join("Ullage").join("window.json"));
        assert_eq!(prefs_path(None), None);
        assert_eq!(prefs_path(Some(OsStr::new(""))), None);
    }

    #[test]
    fn a_saved_file_loads_back_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Ullage").join("window.json");
        let prefs = Prefs {
            x: Some(-1200),
            y: Some(80),
            layout: Layout::Compact,
            expanded_layout: Layout::Single,
            always_on_top: false,
            click_through: true,
            text_size: 16,
            opacity: 70,
            ..Prefs::default()
        };
        save(&path, &prefs).unwrap();
        assert_eq!(load(&path), prefs);
        let mut temp = path.as_os_str().to_owned();
        temp.push(".tmp");
        assert!(!Path::new(&temp).exists());
    }

    #[test]
    fn the_file_format_is_the_documented_one() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("window.json");
        fs::write(
            &path,
            r#"{"version":1,"x":1200,"y":80,"layout":"columns","expanded_layout":"columns","always_on_top":true,"click_through":false,"text_size":13,"opacity":95}"#,
        )
        .unwrap();
        let prefs = load(&path);
        assert_eq!((prefs.x, prefs.y), (Some(1200), Some(80)));
        let shown = serde_json::to_value(&prefs).unwrap();
        assert_eq!(shown["layout"], "columns");
        assert_eq!(shown["version"], 1);
    }

    #[test]
    fn a_missing_file_reads_as_the_defaults() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(load(&dir.path().join("none.json")), Prefs::default());
    }

    #[test]
    fn a_broken_or_foreign_file_reads_as_the_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("window.json");
        for text in [
            "",
            "not json",
            "{\"version\":2,\"x\":5}",
            "{\"version\":1,\"x\":\"a\"}",
        ] {
            fs::write(&path, text).unwrap();
            assert_eq!(load(&path), Prefs::default(), "{text:?}");
        }
        // A directory where the file should be cannot be read either.
        let as_dir = dir.path().join("dir");
        fs::create_dir(&as_dir).unwrap();
        assert_eq!(load(&as_dir), Prefs::default());
    }

    #[test]
    fn missing_fields_take_their_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("window.json");
        fs::write(&path, r#"{"version":1,"text_size":14}"#).unwrap();
        let prefs = load(&path);
        assert_eq!(prefs.text_size, 14);
        assert_eq!(prefs.opacity, 95);
        assert_eq!(prefs.layout, Layout::Columns);
    }

    #[test]
    fn values_the_menus_do_not_offer_fall_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("window.json");
        fs::write(
            &path,
            r#"{"version":1,"text_size":99,"opacity":3,"expanded_layout":"compact"}"#,
        )
        .unwrap();
        let prefs = load(&path);
        assert_eq!(prefs.text_size, 13);
        assert_eq!(prefs.opacity, 95);
        assert_eq!(prefs.expanded_layout, Layout::Columns);
    }

    #[test]
    fn choosing_an_expanded_layout_remembers_it() {
        let mut prefs = Prefs::default();
        prefs.set_layout(Layout::Single);
        assert_eq!(prefs.expanded_layout, Layout::Single);
        prefs.set_layout(Layout::Compact);
        assert_eq!(prefs.layout, Layout::Compact);
        assert_eq!(prefs.expanded_layout, Layout::Single);
    }

    #[test]
    fn compact_toggles_to_the_last_expanded_layout() {
        let mut prefs = Prefs::default();
        prefs.toggle_compact();
        assert_eq!(prefs.layout, Layout::Compact);
        prefs.toggle_compact();
        assert_eq!(prefs.layout, Layout::Columns);

        prefs.set_layout(Layout::Single);
        prefs.toggle_compact();
        prefs.toggle_compact();
        assert_eq!(prefs.layout, Layout::Single);
    }
}
