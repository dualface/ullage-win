//! The pairing dialog as data: the template of its window, the ids of its
//! controls and the rule that lets only one be open. What it looks like is in
//! `scene::pair`. The window itself is in `win::pair_dialog`; nothing here
//! knows about Windows, so what can be tested is tested anywhere.

pub use crate::scene::pair::TITLE;
use crate::scene::pair::{ADDRESS_LABEL, CANCEL, CODE_LABEL, PRIMARY};

pub const ID_OK: u16 = 1;
pub const ID_CANCEL: u16 = 2;
pub const ID_ADDRESS_LABEL: u16 = 1001;
pub const ID_ADDRESS: u16 = 1002;
pub const ID_CODE_LABEL: u16 = 1004;
pub const ID_CODE: u16 = 1005;

/// The longest address the box takes. A host name is at most 253 characters.
pub const ADDRESS_LIMIT: usize = 260;
/// What the code box takes before it is formatted. A code is seven
/// characters, but formatting comes after the limit: a paste with white space
/// around the code must not lose the end of the code to the limit.
pub const CODE_LIMIT: usize = 32;

// Window styles, from `winuser.h`.
const WS_POPUP: u32 = 0x8000_0000;
const WS_CHILD: u32 = 0x4000_0000;
const WS_VISIBLE: u32 = 0x1000_0000;
const WS_CAPTION: u32 = 0x00C0_0000;
const WS_CLIPCHILDREN: u32 = 0x0200_0000;
const WS_SYSMENU: u32 = 0x0008_0000;
const WS_GROUP: u32 = 0x0002_0000;
const WS_TABSTOP: u32 = 0x0001_0000;
// Dialog styles.
const DS_SETFONT: u32 = 0x40;
const DS_MODALFRAME: u32 = 0x80;
// Control styles.
const SS_LEFT: u32 = 0;
const SS_NOPREFIX: u32 = 0x80;
const ES_MULTILINE: u32 = 0x4;
const ES_AUTOVSCROLL: u32 = 0x40;
const ES_AUTOHSCROLL: u32 = 0x80;
const BS_PUSHBUTTON: u32 = 0;
const BS_DEFPUSHBUTTON: u32 = 1;
// The window classes of a control in a template, as `0xFFFF` and an atom.
const CLASS_BUTTON: u16 = 0x0080;
const CLASS_EDIT: u16 = 0x0081;
const CLASS_STATIC: u16 = 0x0082;

const FONT: &str = "Segoe UI";
const FONT_POINTS: u16 = 9;

/// The size of the dialog, in dialog units.
const WIDTH: i16 = 260;
const HEIGHT: i16 = 124;

struct Control {
    class: u16,
    style: u32,
    ex_style: u32,
    /// Left, top, width and height, in dialog units.
    rect: (i16, i16, i16, i16),
    id: u16,
    text: &'static str,
}

/// In tab order: the two boxes, then the buttons. The default button is the
/// main one, so that Enter takes the input. The rectangles are placeholders:
/// the dialog puts every control in place when it opens, by `scene::pair`.
fn controls() -> [Control; 6] {
    let visible = WS_CHILD | WS_VISIBLE;
    let label = visible | SS_LEFT | SS_NOPREFIX;
    let edit = visible | WS_TABSTOP | ES_AUTOHSCROLL;
    [
        // The labels stay controls, so that each box has a name for a screen
        // reader: the name of a box is the text of the control before it.
        Control {
            class: CLASS_STATIC,
            style: label,
            ex_style: 0,
            rect: (10, 10, 100, 8),
            id: ID_ADDRESS_LABEL,
            text: ADDRESS_LABEL,
        },
        Control {
            class: CLASS_EDIT,
            style: edit | WS_GROUP,
            ex_style: 0,
            rect: (10, 20, 240, 14),
            id: ID_ADDRESS,
            text: "",
        },
        Control {
            class: CLASS_STATIC,
            style: label,
            ex_style: 0,
            rect: (10, 40, 100, 8),
            id: ID_CODE_LABEL,
            text: CODE_LABEL,
        },
        Control {
            class: CLASS_EDIT,
            // Multi-line, so that the control itself takes a paste with a
            // line break in front of the code: a single-line box takes only
            // the first line of a paste. The box is one line high. Enter is
            // not taken by it (no `ES_WANTRETURN`) and so reaches the default
            // button, and the line breaks are dropped with the other
            // characters that a code does not have.
            // `ES_AUTOVSCROLL`: a multi-line box without it refuses text that
            // needs more lines than the box shows, so the paste would be lost.
            style: edit | ES_MULTILINE | ES_AUTOVSCROLL,
            ex_style: 0,
            rect: (10, 50, 80, 14),
            id: ID_CODE,
            text: "",
        },
        Control {
            class: CLASS_BUTTON,
            style: visible | WS_TABSTOP | BS_DEFPUSHBUTTON,
            ex_style: 0,
            rect: (146, 80, 50, 14),
            id: ID_OK,
            text: PRIMARY,
        },
        Control {
            class: CLASS_BUTTON,
            style: visible | WS_TABSTOP | BS_PUSHBUTTON,
            ex_style: 0,
            rect: (200, 80, 50, 14),
            id: ID_CANCEL,
            text: CANCEL,
        },
    ]
}

fn put_u16(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn put_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn put_i16(bytes: &mut Vec<u8>, value: i16) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn put_text(bytes: &mut Vec<u8>, text: &str) {
    for unit in text.encode_utf16().chain(std::iter::once(0)) {
        put_u16(bytes, unit);
    }
}

/// Pads to a multiple of four bytes: each item of a template starts on one.
fn align(bytes: &mut Vec<u8>) {
    while !bytes.len().is_multiple_of(4) {
        bytes.push(0);
    }
}

/// The `DLGTEMPLATE` of the dialog and the `DLGITEMTEMPLATE`s that follow
/// it, as the bytes `DialogBoxIndirectParamW` reads. There is no resource
/// file in this program, so the template is built here. The caller keeps it
/// at an address that is a multiple of four.
pub fn template() -> Vec<u8> {
    let controls = controls();
    let mut bytes = Vec::new();
    put_u32(
        &mut bytes,
        WS_POPUP | WS_CAPTION | WS_SYSMENU | WS_CLIPCHILDREN | DS_MODALFRAME | DS_SETFONT,
    );
    put_u32(&mut bytes, 0);
    put_u16(&mut bytes, controls.len() as u16);
    put_i16(&mut bytes, 0);
    put_i16(&mut bytes, 0);
    put_i16(&mut bytes, WIDTH);
    put_i16(&mut bytes, HEIGHT);
    // No menu, and the class of a dialog box.
    put_u16(&mut bytes, 0);
    put_u16(&mut bytes, 0);
    put_text(&mut bytes, TITLE);
    // `DS_SETFONT`: the size and the name of the font.
    put_u16(&mut bytes, FONT_POINTS);
    put_text(&mut bytes, FONT);
    for control in &controls {
        align(&mut bytes);
        put_u32(&mut bytes, control.style);
        put_u32(&mut bytes, control.ex_style);
        let (x, y, width, height) = control.rect;
        put_i16(&mut bytes, x);
        put_i16(&mut bytes, y);
        put_i16(&mut bytes, width);
        put_i16(&mut bytes, height);
        put_u16(&mut bytes, control.id);
        put_u16(&mut bytes, 0xFFFF);
        put_u16(&mut bytes, control.class);
        put_text(&mut bytes, control.text);
        // No creation data.
        put_u16(&mut bytes, 0);
    }
    bytes
}

/// One pairing dialog at a time. The dialog disables the window, but the
/// tray icon still reaches it, and so does a second `Pair…`: whoever opens a
/// dialog takes the place first, and gives it back when the dialog is gone.
#[derive(Debug, Default)]
pub struct OnePlace {
    taken: bool,
}

impl OnePlace {
    /// Takes the place. `false` when it is taken.
    pub fn take(&mut self) -> bool {
        !std::mem::replace(&mut self.taken, true)
    }

    pub fn give_back(&mut self) {
        self.taken = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A reader for the template, as strict as it needs to be to show that
    /// the bytes have the shape `DialogBoxIndirectParamW` reads.
    struct Reader<'a> {
        bytes: &'a [u8],
        at: usize,
    }

    impl Reader<'_> {
        fn u16(&mut self) -> u16 {
            let value = u16::from_le_bytes([self.bytes[self.at], self.bytes[self.at + 1]]);
            self.at += 2;
            value
        }

        fn u32(&mut self) -> u32 {
            let low = u32::from(self.u16());
            low | (u32::from(self.u16()) << 16)
        }

        fn text(&mut self) -> String {
            let mut units = Vec::new();
            loop {
                match self.u16() {
                    0 => break,
                    unit => units.push(unit),
                }
            }
            String::from_utf16(&units).unwrap()
        }

        fn align(&mut self) {
            while !self.at.is_multiple_of(4) {
                assert_eq!(self.bytes[self.at], 0, "padding");
                self.at += 1;
            }
        }
    }

    #[test]
    fn the_template_has_the_shape_a_dialog_box_reads() {
        let bytes = template();
        let mut read = Reader {
            bytes: &bytes,
            at: 0,
        };
        let style = read.u32();
        assert_eq!(style & DS_SETFONT, DS_SETFONT);
        assert_eq!(style & DS_MODALFRAME, DS_MODALFRAME);
        assert_eq!(style & WS_POPUP, WS_POPUP);
        assert_eq!(style & WS_SYSMENU, WS_SYSMENU);
        assert_eq!(style & WS_CLIPCHILDREN, WS_CLIPCHILDREN);
        assert_eq!(read.u32(), 0, "extended style");
        let count = read.u16();
        assert_eq!(usize::from(count), controls().len());
        read.at += 8; // x, y, width, height
        assert_eq!(read.u16(), 0, "menu");
        assert_eq!(read.u16(), 0, "class");
        assert_eq!(read.text(), TITLE);
        assert_eq!(read.u16(), FONT_POINTS);
        assert_eq!(read.text(), FONT);
        let mut seen = Vec::new();
        for _ in 0..count {
            read.align();
            let style = read.u32();
            read.u32(); // extended style
            read.at += 8; // x, y, width, height
            let id = read.u16();
            assert_eq!(read.u16(), 0xFFFF, "a class is an atom");
            let class = read.u16();
            let text = read.text();
            assert_eq!(read.u16(), 0, "no creation data");
            seen.push((id, class, style, text));
        }
        // Nothing is left over.
        assert_eq!(read.at, bytes.len());
        let ids: Vec<u16> = seen.iter().map(|item| item.0).collect();
        assert_eq!(
            ids,
            [
                ID_ADDRESS_LABEL,
                ID_ADDRESS,
                ID_CODE_LABEL,
                ID_CODE,
                ID_OK,
                ID_CANCEL
            ]
        );
        let text_of = |wanted: u16| &seen.iter().find(|item| item.0 == wanted).unwrap().3;
        assert_eq!(text_of(ID_ADDRESS_LABEL), "ADDRESS");
        assert_eq!(text_of(ID_CODE_LABEL), "PAIR CODE");
        assert_eq!(text_of(ID_OK), "Pair");
        assert_eq!(text_of(ID_CANCEL), "Cancel");
    }

    #[test]
    fn the_boxes_and_the_buttons_are_tab_stops_and_the_main_button_is_the_default() {
        let controls = controls();
        let find = |id: u16| controls.iter().find(|control| control.id == id).unwrap();
        for id in [ID_ADDRESS, ID_CODE, ID_OK, ID_CANCEL] {
            assert_eq!(find(id).style & WS_TABSTOP, WS_TABSTOP, "{id}");
        }
        for id in [ID_ADDRESS_LABEL, ID_CODE_LABEL] {
            assert_eq!(find(id).style & WS_TABSTOP, 0, "{id}");
        }
        assert_eq!(find(ID_OK).style & 0xF, BS_DEFPUSHBUTTON);
        assert_eq!(find(ID_CANCEL).style & 0xF, BS_PUSHBUTTON);
        // Neither is owner-drawn: the system keeps the default button, Enter
        // and Space, and no `WM_DRAWITEM` with a pointer comes in.
        const BS_OWNERDRAW: u32 = 0xB;
        assert_ne!(find(ID_OK).style & 0xF, BS_OWNERDRAW);
        assert_ne!(find(ID_CANCEL).style & 0xF, BS_OWNERDRAW);
        // The boxes have no edge of their own: the dialog draws their frame.
        assert_eq!(find(ID_ADDRESS).ex_style, 0);
        assert_eq!(find(ID_CODE).ex_style, 0);
        // The boxes have a scroll for what does not fit.
        assert_eq!(find(ID_ADDRESS).style & ES_AUTOHSCROLL, ES_AUTOHSCROLL);
        assert_eq!(find(ID_CODE).style & ES_AUTOHSCROLL, ES_AUTOHSCROLL);
        // Only the code box takes a paste of more than one line, and neither
        // takes Enter: it reaches the default button.
        const ES_WANTRETURN: u32 = 0x1000;
        assert_eq!(find(ID_ADDRESS).style & ES_MULTILINE, 0);
        assert_eq!(find(ID_CODE).style & ES_MULTILINE, ES_MULTILINE);
        assert_eq!(find(ID_CODE).style & ES_AUTOVSCROLL, ES_AUTOVSCROLL);
        for id in [ID_ADDRESS, ID_CODE] {
            assert_eq!(find(id).style & ES_WANTRETURN, 0, "{id}");
        }
    }

    // A code is seven characters, and so is what formatting leaves.
    const _: () = assert!(CODE_LIMIT > 7);
    // The longest host name, a colon and the longest port.
    const _: () = assert!(ADDRESS_LIMIT >= 253 + ":65535".len());

    #[test]
    fn a_second_dialog_cannot_take_the_place_until_the_first_gives_it_back() {
        let mut place = OnePlace::default();
        assert!(place.take());
        assert!(!place.take());
        assert!(!place.take());
        place.give_back();
        assert!(place.take());
        // Giving back what is not taken changes nothing.
        place.give_back();
        place.give_back();
        assert!(place.take());
        assert!(!place.take());
    }
}
