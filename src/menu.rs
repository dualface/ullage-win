//! The tray and "more" menu as data: what it lists, in which order, and
//! which command each entry runs.

use crate::prefs::{Connection, Layout, OPACITIES, TEXT_SIZES};
use crate::remote::address::RemoteAddress;

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
    SetConnection(Connection),
    /// Reads `<ip>:<port> <code>` from the clipboard and pairs with it.
    PairFromClipboard,
    /// Deletes the saved token and address of the remote daemon.
    ForgetRemote,
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
const ID_PAIR: usize = 8;
const ID_FORGET: usize = 9;
const ID_LAYOUT: usize = 100;
const ID_SIZE: usize = 200;
const ID_OPACITY: usize = 300;
const ID_CONNECTION: usize = 400;
const CONNECTIONS: [Connection; 2] = [Connection::Local, Connection::Remote];
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
            Command::SetConnection(connection) => {
                ID_CONNECTION
                    + CONNECTIONS
                        .iter()
                        .position(|c| *c == connection)
                        .unwrap_or(0)
            }
            Command::PairFromClipboard => ID_PAIR,
            Command::ForgetRemote => ID_FORGET,
            Command::Quit => ID_QUIT,
            simple => 1 + ID_SIMPLE.iter().position(|c| *c == simple).unwrap_or(0),
        }
    }

    pub fn from_id(id: usize) -> Option<Self> {
        match id {
            1..=6 => Some(ID_SIMPLE[id - 1]),
            ID_QUIT => Some(Command::Quit),
            ID_PAIR => Some(Command::PairFromClipboard),
            ID_FORGET => Some(Command::ForgetRemote),
            ID_LAYOUT..=199 => LAYOUTS.get(id - ID_LAYOUT).map(|l| Command::SetLayout(*l)),
            ID_SIZE..=299 => TEXT_SIZES
                .get(id - ID_SIZE)
                .map(|s| Command::SetTextSize(*s)),
            ID_OPACITY..=399 => OPACITIES
                .get(id - ID_OPACITY)
                .map(|o| Command::SetOpacity(*o)),
            ID_CONNECTION..=499 => CONNECTIONS
                .get(id - ID_CONNECTION)
                .map(|c| Command::SetConnection(*c)),
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
    pub connection: Connection,
    /// The remote daemon the settings name, if any.
    pub remote: Option<RemoteAddress>,
    /// Whether a token is saved for that address: only then can the remote
    /// daemon be chosen.
    pub remote_paired: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Item {
    Command {
        command: Command,
        label: String,
        /// Faint text on the right, such as the shortcut.
        right: Option<String>,
        checked: bool,
        /// A command that cannot run now is shown grayed.
        enabled: bool,
    },
    Separator,
    Submenu {
        label: String,
        right: Option<String>,
        /// Radio entries: exactly one is checked.
        items: Vec<Item>,
    },
}

pub const SHORTCUT_TEXT: &str = "Win+Alt+U";

fn command(command: Command, label: &str, checked: bool) -> Item {
    Item::Command {
        command,
        label: label.to_owned(),
        right: None,
        checked,
        enabled: true,
    }
}

/// The `Connection` submenu: which daemon to ask, and how to pair with a
/// remote one. A remote address that is not on an encrypted path says so.
fn connection_menu(state: &MenuState) -> Item {
    let remote_label = match &state.remote {
        Some(address) if address.is_encrypted_path() => format!("Remote daemon ({address})"),
        Some(address) => format!("Remote daemon ({address}) (plaintext)"),
        None => "Remote daemon".to_owned(),
    };
    Item::Submenu {
        label: "Connection".to_owned(),
        right: None,
        items: vec![
            command(
                Command::SetConnection(Connection::Local),
                "Local daemon",
                state.connection == Connection::Local,
            ),
            Item::Command {
                command: Command::SetConnection(Connection::Remote),
                label: remote_label,
                right: None,
                checked: state.connection == Connection::Remote,
                enabled: state.remote_paired,
            },
            Item::Separator,
            command(
                Command::PairFromClipboard,
                "Pair from clipboard\u{2026}",
                false,
            ),
            Item::Command {
                command: Command::ForgetRemote,
                label: "Forget remote daemon".to_owned(),
                right: None,
                checked: false,
                // An address without a token can still be forgotten.
                enabled: state.remote.is_some(),
            },
        ],
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
            }
            .to_owned(),
            right: state.shortcut.then(|| SHORTCUT_TEXT.to_owned()),
            checked: false,
            enabled: true,
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
            label: "Layout".to_owned(),
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
            label: "Text size".to_owned(),
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
            label: "Opacity".to_owned(),
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
        connection_menu(state),
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
            connection: Connection::Local,
            remote: None,
            remote_paired: false,
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
                "Connection",
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
                label: "Hide window".into(),
                right: Some("Win+Alt+U".into()),
                checked: false,
                enabled: true
            }
        );
        state.window_visible = false;
        state.shortcut = false;
        assert_eq!(
            first(&state),
            Item::Command {
                command: Command::ToggleWindow,
                label: "Show window".into(),
                right: None,
                checked: false,
                enabled: true
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
        assert!(checked(&items[11]), "Start at sign-in");
        assert!(!checked(&items[10]));
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

    fn connection_items(state: &MenuState) -> Vec<Item> {
        let Item::Submenu { items, label, .. } = items(state).remove(8) else {
            panic!()
        };
        assert_eq!(label, "Connection");
        items
    }

    fn enabled(item: &Item) -> bool {
        matches!(item, Item::Command { enabled: true, .. })
    }

    #[test]
    fn the_connection_menu_before_pairing_offers_the_local_daemon_and_pairing() {
        let items = connection_items(&state());
        assert_eq!(
            items.iter().map(label).collect::<Vec<_>>(),
            [
                "Local daemon",
                "Remote daemon",
                "-",
                "Pair from clipboard\u{2026}",
                "Forget remote daemon"
            ]
        );
        assert!(checked(&items[0]));
        assert!(enabled(&items[0]));
        assert!(!enabled(&items[1]), "no remote daemon to choose yet");
        assert!(enabled(&items[3]));
        assert!(!enabled(&items[4]), "nothing to forget yet");
    }

    #[test]
    fn a_paired_remote_daemon_shows_its_address_and_can_be_chosen_or_forgotten() {
        let mut state = state();
        state.remote = RemoteAddress::parse("100.71.140.68:7878").ok();
        state.remote_paired = true;
        state.connection = Connection::Remote;
        let items = connection_items(&state);
        assert_eq!(label(&items[1]), "Remote daemon (100.71.140.68:7878)");
        assert!(!checked(&items[0]));
        assert!(checked(&items[1]));
        assert!(enabled(&items[1]));
        assert!(enabled(&items[4]));
    }

    #[test]
    fn an_address_without_a_token_cannot_be_chosen_but_can_be_forgotten() {
        let mut state = state();
        state.remote = RemoteAddress::parse("100.71.140.68:7878").ok();
        state.remote_paired = false;
        let items = connection_items(&state);
        assert_eq!(label(&items[1]), "Remote daemon (100.71.140.68:7878)");
        assert!(!enabled(&items[1]));
        assert!(enabled(&items[4]));
    }

    #[test]
    fn an_address_off_the_encrypted_paths_says_plaintext() {
        let mut state = state();
        state.remote = RemoteAddress::parse("192.168.50.11:7878").ok();
        assert_eq!(
            label(&connection_items(&state)[1]),
            "Remote daemon (192.168.50.11:7878) (plaintext)"
        );
        state.remote = RemoteAddress::parse("127.0.0.1:7878").ok();
        assert_eq!(
            label(&connection_items(&state)[1]),
            "Remote daemon (127.0.0.1:7878)"
        );
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
        assert_eq!(commands.len(), 6 + 1 + 3 + 4 + 5 + 4);
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
