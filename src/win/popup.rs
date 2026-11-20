//! The popup menu, made from the menu model.

use windows::Win32::Foundation::HWND;
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CheckMenuRadioItem, CreatePopupMenu, DestroyMenu, HMENU, MF_BYCOMMAND, MF_CHECKED,
    MF_GRAYED, MF_POPUP, MF_SEPARATOR, MF_STRING, PostMessageW, SetForegroundWindow,
    TPM_BOTTOMALIGN, TPM_LEFTALIGN, TPM_RETURNCMD, TPM_RIGHTALIGN, TPM_RIGHTBUTTON, TPM_TOPALIGN,
    TrackPopupMenuEx, WM_NULL,
};
use windows::core::{PCSTR, Result, w};

use super::util::{pcwstr, wide};
use crate::menu::{Command, Item, MenuState, items};

/// Where the menu opens, in screen pixels.
#[derive(Clone, Copy, Debug)]
pub struct Anchor {
    pub x: i32,
    pub y: i32,
    /// The menu hangs under `y` and ends at `x`, as under a button; without
    /// it the menu rises from `y`, as from the notification area.
    pub below_button: bool,
}

/// Asks the menus of this app to follow the system's dark setting. Windows
/// has no public call for it: this is `SetPreferredAppMode`, ordinal 135 of
/// uxtheme, and `FlushMenuThemes`, ordinal 136. A system without them keeps
/// light menus.
pub fn follow_system_theme() {
    unsafe {
        let Ok(module) = LoadLibraryW(w!("uxtheme.dll")) else {
            return;
        };
        let set_mode = GetProcAddress(module, PCSTR(135 as *const u8));
        let flush = GetProcAddress(module, PCSTR(136 as *const u8));
        if let (Some(set_mode), Some(flush)) = (set_mode, flush) {
            let set_mode: extern "system" fn(i32) -> i32 = std::mem::transmute(set_mode);
            let flush: extern "system" fn() = std::mem::transmute(flush);
            // 1 is "allow dark": follow the system.
            set_mode(1);
            flush();
        }
    }
}

fn text_of(label: &str, right: Option<&str>) -> Vec<u16> {
    match right {
        // A tab puts the rest on the right, where shortcuts go.
        Some(right) => wide(&format!("{label}\t{right}")),
        None => wide(label),
    }
}

fn build(entries: &[Item]) -> Result<HMENU> {
    unsafe {
        let menu = CreatePopupMenu()?;
        for item in entries {
            match item {
                Item::Separator => AppendMenuW(menu, MF_SEPARATOR, 0, None)?,
                Item::Command {
                    command,
                    label,
                    right,
                    checked,
                    enabled,
                } => {
                    let text = text_of(label, right.as_deref());
                    let mut flags = MF_STRING;
                    if *checked {
                        flags |= MF_CHECKED;
                    }
                    if !*enabled {
                        flags |= MF_GRAYED;
                    }
                    AppendMenuW(menu, flags, command.id(), pcwstr(&text))?;
                }
                Item::Submenu {
                    label,
                    right,
                    items,
                } => {
                    let sub = build(items)?;
                    mark_radio_group(sub, items);
                    let text = text_of(label, right.as_deref());
                    AppendMenuW(menu, MF_POPUP, sub.0 as usize, pcwstr(&text))?;
                }
            }
        }
        Ok(menu)
    }
}

/// Shows the checked entry of a group of options as a radio button.
fn mark_radio_group(menu: HMENU, items: &[Item]) {
    let ids: Vec<(usize, bool)> = items
        .iter()
        .filter_map(|item| match item {
            Item::Command {
                command, checked, ..
            } => Some((command.id(), *checked)),
            _ => None,
        })
        .collect();
    // The entries of a group are not always in the order of their ids, and
    // a group may hold entries that are not options.
    let (Some(first), Some(last)) = (
        ids.iter().min_by_key(|(id, _)| *id),
        ids.iter().max_by_key(|(id, _)| *id),
    ) else {
        return;
    };
    let Some(checked) = ids.iter().find(|(_, checked)| *checked) else {
        return;
    };
    unsafe {
        let _ = CheckMenuRadioItem(
            menu,
            first.0 as u32,
            last.0 as u32,
            checked.0 as u32,
            MF_BYCOMMAND.0,
        );
    }
}

/// Shows the menu and returns the command chosen, if any. Runs the menu's
/// own message loop, so it returns when the menu closes.
pub fn show(hwnd: HWND, state: &MenuState, anchor: Anchor) -> Option<Command> {
    let entries = items(state);
    unsafe {
        let menu = build(&entries).ok()?;
        // Without this a menu opened from the notification area does not
        // close when the user clicks elsewhere.
        let _ = SetForegroundWindow(hwnd);
        let align = if anchor.below_button {
            TPM_RIGHTALIGN | TPM_TOPALIGN
        } else {
            TPM_LEFTALIGN | TPM_BOTTOMALIGN
        };
        let chosen = TrackPopupMenuEx(
            menu,
            (TPM_RETURNCMD | TPM_RIGHTBUTTON | align).0,
            anchor.x,
            anchor.y,
            hwnd,
            None,
        );
        let _ = PostMessageW(Some(hwnd), WM_NULL, Default::default(), Default::default());
        let _ = DestroyMenu(menu);
        Command::from_id(chosen.0 as usize)
    }
}
