//! The tray and "more" menu as data: what it lists, in which order, and
//! which command each entry runs.

use crate::prefs::{Layout, OPACITIES, TEXT_SIZES};

/// What choosing a menu entry does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    ToggleWindow,
    ToggleCompact,
    TogglePin,
    ToggleClickThrough,
    SetLayout(Layout),
    SetTextSize(u32),
    SetOpacity(u32),
    Refresh,
    ToggleAutostart,
    Quit,
}

// Menu item ids start at 1: 0 means that nothing was chosen.
const ID_SIMPLE: [Command; 6] = [
    Command::ToggleWindow,
    Command::ToggleCompact,
    Command::TogglePin,
    Command::ToggleClickThrough,
    Command::Refresh,
    Command::ToggleAutostart,
];
const ID_QUIT: usize = 7;
const ID_LAYOUT: usize = 100;
const ID_SIZE: usize = 200;
const ID_OPACITY: usize = 300;
const LAYOUTS: [Layout; 3] = [Layout::Columns, Layout::Single, Layout::Compact];

impl Command {
    pub fn id(self) -> usize {
        match self {
            Command::SetLayout(layout) => {
                ID_LAYOUT + LAYOUTS.iter().position(|l| *l == layout).unwrap_or(0)
            }
            Command::SetTextSize(size) => {
                ID_SIZE + TEXT_SIZES.iter().position(|s| *s == size).unwrap_or(0)
            }
            Command::SetOpacity(opacity) => {
                ID_OPACITY + OPACITIES.iter().position(|o| *o == opacity).unwrap_or(0)
            }
            Command::Quit => ID_QUIT,
            simple => 1 + ID_SIMPLE.iter().position(|c| *c == simple).unwrap_or(0),
        }
    }

    pub fn from_id(id: usize) -> Option<Self> {
        match id {
            1..=6 => Some(ID_SIMPLE[id - 1]),
            ID_QUIT => Some(Command::Quit),
            ID_LAYOUT..=199 => LAYOUTS.get(id - ID_LAYOUT).map(|l| Command::SetLayout(*l)),
            ID_SIZE..=299 => TEXT_SIZES
                .get(id - ID_SIZE)
                .map(|s| Command::SetTextSize(*s)),
            ID_OPACITY..=399 => OPACITIES
                .get(id - ID_OPACITY)
                .map(|o| Command::SetOpacity(*o)),
            _ => None,
        }
    }
}

/// The current settings the menu shows.
#[derive(Clone, Copy, Debug)]
pub struct MenuState {
    pub window_visible: bool,
    pub layout: Layout,
    pub always_on_top: bool,
    pub click_through: bool,
    pub text_size: u32,
    pub opacity: u32,
    pub autostart: bool,
    /// Whether the global shortcut is registered.
    pub shortcut: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Item {
    Command {
        command: Command,
        label: &'static str,
        /// Faint text on the right, such as the shortcut.
        right: Option<String>,
        checked: bool,
    },
    Separator,
    Submenu {
        label: &'static str,
        right: Option<String>,
        /// Radio entries: exactly one is checked.
        items: Vec<Item>,
    },
}

pub const SHORTCUT_TEXT: &str = "Win+Alt+U";

fn command(command: Command, label: &'static str, checked: bool) -> Item {
    Item::Command {
        command,
        label,
        right: None,
        checked,
    }
}

pub fn items(state: &MenuState) -> Vec<Item> {
    let layout_label = |layout| match layout {
        Layout::Columns => "Columns",
        Layout::Single => "One per row",
        Layout::Compact => "Compact strip",
    };
    let size_labels = ["12", "13", "14", "16"];
    let opacity_labels = ["100%", "95%", "90%", "80%", "70%"];
    vec![
        Item::Command {
            command: Command::ToggleWindow,
            label: if state.window_visible {
                "Hide window"
            } else {
                "Show window"
            },
            right: state.shortcut.then(|| SHORTCUT_TEXT.to_owned()),
            checked: false,
        },
        command(
            Command::ToggleCompact,
            "Compact strip",
            state.layout == Layout::Compact,
        ),
        Item::Separator,
        command(Command::TogglePin, "Always on top", state.always_on_top),
        command(
            Command::ToggleClickThrough,
            "Click-through",
            state.click_through,
        ),
        Item::Submenu {
            label: "Layout",
            right: None,
            items: LAYOUTS
                .iter()
                .map(|layout| {
                    command(
                        Command::SetLayout(*layout),
                        layout_label(*layout),
                        *layout == state.layout,
                    )
                })
                .collect(),
        },
        Item::Submenu {
            label: "Text size",
            right: Some(state.text_size.to_string()),
            items: TEXT_SIZES
                .iter()
                .zip(size_labels)
                .map(|(size, label)| {
                    command(Command::SetTextSize(*size), label, *size == state.text_size)
                })
                .collect(),
        },
        Item::Submenu {
            label: "Opacity",
            right: Some(format!("{}%", state.opacity)),
            items: OPACITIES
                .iter()
                .zip(opacity_labels)
                .map(|(opacity, label)| {
                    command(
                        Command::SetOpacity(*opacity),
                        label,
                        *opacity == state.opacity,
                    )
                })
                .collect(),
        },
        Item::Separator,
        command(Command::Refresh, "Refresh now", false),
        command(
            Command::ToggleAutostart,
            "Start at sign-in",
            state.autostart,
        ),
        Item::Separator,
        command(Command::Quit, "Quit Ullage", false),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> MenuState {
        MenuState {
            window_visible: true,
            layout: Layout::Columns,
            always_on_top: true,
            click_through: false,
            text_size: 13,
            opacity: 95,
            autostart: false,
            shortcut: true,
        }
    }

    fn label(item: &Item) -> &str {
        match item {
            Item::Command { label, .. } | Item::Submenu { label, .. } => label,
            Item::Separator => "-",
        }
    }

    #[test]
    fn the_menu_lists_its_entries_in_the_specified_order() {
        let items = items(&state());
        let labels: Vec<&str> = items.iter().map(label).collect();
        assert_eq!(
            labels,
            [
                "Hide window",
                "Compact strip",
                "-",
                "Always on top",
                "Click-through",
                "Layout",
                "Text size",
                "Opacity",
                "-",
                "Refresh now",
                "Start at sign-in",
                "-",
                "Quit Ullage"
            ]
        );
    }

    #[test]
    fn the_first_entry_follows_the_window_and_shows_the_shortcut() {
        let mut state = state();
        let first = |state: &MenuState| items(state).remove(0);
        assert_eq!(
            first(&state),
            Item::Command {
                command: Command::ToggleWindow,
                label: "Hide window",
                right: Some("Win+Alt+U".into()),
                checked: false
            }
        );
        state.window_visible = false;
        state.shortcut = false;
        assert_eq!(
            first(&state),
            Item::Command {
                command: Command::ToggleWindow,
                label: "Show window",
                right: None,
                checked: false
            }
        );
    }

    fn checked(item: &Item) -> bool {
        matches!(item, Item::Command { checked: true, .. })
    }

    #[test]
    fn checks_follow_the_settings() {
        let mut state = state();
        state.layout = Layout::Compact;
        state.click_through = true;
        state.autostart = true;
        let items = items(&state);
        assert!(checked(&items[1]), "Compact strip");
        assert!(checked(&items[3]), "Always on top");
        assert!(checked(&items[4]), "Click-through");
        assert!(checked(&items[10]), "Start at sign-in");
        assert!(!checked(&items[9]));
        let Item::Submenu { items: layouts, .. } = &items[5] else {
            panic!()
        };
        assert_eq!(layouts.iter().filter(|item| checked(item)).count(), 1);
        assert!(checked(&layouts[2]));
    }

    #[test]
    fn submenus_have_their_options_and_show_the_current_value() {
        let items = items(&state());
        let Item::Submenu {
            items: layouts,
            right,
            ..
        } = &items[5]
        else {
            panic!()
        };
        assert_eq!(right, &None);
        assert_eq!(
            layouts.iter().map(label).collect::<Vec<_>>(),
            ["Columns", "One per row", "Compact strip"]
        );
        let Item::Submenu {
            items: sizes,
            right,
            ..
        } = &items[6]
        else {
            panic!()
        };
        assert_eq!(right.as_deref(), Some("13"));
        assert_eq!(
            sizes.iter().map(label).collect::<Vec<_>>(),
            ["12", "13", "14", "16"]
        );
        assert!(checked(&sizes[1]));
        let Item::Submenu {
            items: opacities,
            right,
            ..
        } = &items[7]
        else {
            panic!()
        };
        assert_eq!(right.as_deref(), Some("95%"));
        assert_eq!(
            opacities.iter().map(label).collect::<Vec<_>>(),
            ["100%", "95%", "90%", "80%", "70%"]
        );
        assert!(checked(&opacities[1]));
    }

    #[test]
    fn command_ids_are_unique_nonzero_and_round_trip() {
        fn walk(items: &[Item], out: &mut Vec<Command>) {
            for item in items {
                match item {
                    Item::Command { command, .. } => out.push(*command),
                    Item::Submenu { items, .. } => walk(items, out),
                    Item::Separator => {}
                }
            }
        }
        let mut commands = Vec::new();
        walk(&items(&state()), &mut commands);
        assert_eq!(commands.len(), 6 + 1 + 3 + 4 + 5);
        let mut ids: Vec<usize> = commands.iter().map(|c| c.id()).collect();
        for (command, id) in commands.iter().zip(&ids) {
            assert_ne!(*id, 0);
            assert_eq!(Command::from_id(*id), Some(*command));
        }
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), commands.len());
        assert_eq!(Command::from_id(0), None);
        assert_eq!(Command::from_id(99), None);
    }
}
