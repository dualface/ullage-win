//! The pairing dialog as data: the template of its window, and the words it
//! shows. The window itself is in `win::pair_dialog`; nothing here knows
//! about Windows, so what can be tested is tested anywhere.

use crate::remote::address::RemoteAddress;

pub const TITLE: &str = "Pair with a remote daemon";
pub const HINT: &str =
    "Run \"ullage device pair\" on the daemon's host. The code looks like ABC-DEF.";

pub const ID_OK: u16 = 1;
pub const ID_CANCEL: u16 = 2;
pub const ID_ADDRESS_LABEL: u16 = 1001;
pub const ID_ADDRESS: u16 = 1002;
pub const ID_ADDRESS_NOTE: u16 = 1003;
pub const ID_CODE_LABEL: u16 = 1004;
pub const ID_CODE: u16 = 1005;
pub const ID_CODE_HINT: u16 = 1006;

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
const WS_SYSMENU: u32 = 0x0008_0000;
const WS_GROUP: u32 = 0x0002_0000;
const WS_TABSTOP: u32 = 0x0001_0000;
const WS_EX_CLIENTEDGE: u32 = 0x0000_0200;
// Dialog styles.
const DS_SETFONT: u32 = 0x40;
const DS_MODALFRAME: u32 = 0x80;
// Control styles.
const SS_LEFT: u32 = 0;
const SS_NOPREFIX: u32 = 0x80;
const ES_MULTILINE: u32 = 0x4;
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

/// In tab order: the two boxes, then the buttons. The default button is
/// `OK`, so that Enter takes the input.
fn controls() -> [Control; 8] {
    let visible = WS_CHILD | WS_VISIBLE;
    let label = visible | SS_LEFT | SS_NOPREFIX;
    let edit = visible | WS_TABSTOP | ES_AUTOHSCROLL;
    [
        Control {
            class: CLASS_STATIC,
            style: label,
            ex_style: 0,
            rect: (10, 10, 240, 8),
            id: ID_ADDRESS_LABEL,
            text: "Address",
        },
        Control {
            class: CLASS_EDIT,
            style: edit | WS_GROUP,
            ex_style: WS_EX_CLIENTEDGE,
            rect: (10, 20, 240, 14),
            id: ID_ADDRESS,
            text: "",
        },
        Control {
            class: CLASS_STATIC,
            style: label,
            ex_style: 0,
            rect: (10, 36, 240, 16),
            id: ID_ADDRESS_NOTE,
            text: "",
        },
        Control {
            class: CLASS_STATIC,
            style: label,
            ex_style: 0,
            rect: (10, 56, 240, 8),
            id: ID_CODE_LABEL,
            text: "Pair code",
        },
        Control {
            class: CLASS_EDIT,
            // Multi-line, so that a paste with a line break in front of the
            // code is taken whole: a single-line box takes only the first
            // line of a paste. The box is one line high, Enter is not
            // taken by it (no `ES_WANTRETURN`) and so reaches the default
            // button, and the line breaks are dropped with the other
            // characters that a code does not have.
            style: edit | ES_MULTILINE,
            ex_style: WS_EX_CLIENTEDGE,
            rect: (10, 66, 80, 14),
            id: ID_CODE,
            text: "",
        },
        Control {
            class: CLASS_STATIC,
            style: label,
            ex_style: 0,
            rect: (10, 84, 240, 16),
            id: ID_CODE_HINT,
            text: HINT,
        },
        Control {
            class: CLASS_BUTTON,
            style: visible | WS_TABSTOP | BS_DEFPUSHBUTTON,
            ex_style: 0,
            rect: (146, 104, 50, 14),
            id: ID_OK,
            text: "OK",
        },
        Control {
            class: CLASS_BUTTON,
            style: visible | WS_TABSTOP | BS_PUSHBUTTON,
            ex_style: 0,
            rect: (200, 104, 50, 14),
            id: ID_CANCEL,
            text: "Cancel",
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
        WS_POPUP | WS_CAPTION | WS_SYSMENU | DS_MODALFRAME | DS_SETFONT,
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

/// What the line under the address box says: nothing for an empty box or an
/// address that is taken, and why not for any other.
pub fn address_note(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }
    match RemoteAddress::parse(text) {
        Ok(_) => String::new(),
        Err(error) => {
            let mut note = error.to_string();
            if let Some(first) = note.get_mut(..1) {
                first.make_ascii_uppercase();
            }
            note.push('.');
            note
        }
    }
}

/// The question that is asked before a pairing code is sent over a path that
/// is not encrypted. An encrypted path has none: the user has typed the
/// address, and says so by pressing OK.
pub fn plaintext_question(address: &RemoteAddress) -> Option<String> {
    if address.is_encrypted_path() {
        return None;
    }
    Some(format!(
        "Pair this device with the ullage daemon at {address}?\n\n\
         This address is not known to be on Tailscale (a host name is judged by \
         its spelling): the pairing code and the device token may cross the \
         network as plain text. (plaintext)"
    ))
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
                ID_ADDRESS_NOTE,
                ID_CODE_LABEL,
                ID_CODE,
                ID_CODE_HINT,
                ID_OK,
                ID_CANCEL
            ]
        );
        let text_of = |wanted: u16| &seen.iter().find(|item| item.0 == wanted).unwrap().3;
        assert_eq!(text_of(ID_ADDRESS_LABEL), "Address");
        assert_eq!(text_of(ID_CODE_LABEL), "Pair code");
        assert_eq!(text_of(ID_CODE_HINT), HINT);
        assert_eq!(text_of(ID_OK), "OK");
        assert_eq!(text_of(ID_CANCEL), "Cancel");
    }

    #[test]
    fn the_boxes_and_the_buttons_are_tab_stops_and_ok_is_the_default() {
        let controls = controls();
        let find = |id: u16| controls.iter().find(|control| control.id == id).unwrap();
        for id in [ID_ADDRESS, ID_CODE, ID_OK, ID_CANCEL] {
            assert_eq!(find(id).style & WS_TABSTOP, WS_TABSTOP, "{id}");
        }
        for id in [
            ID_ADDRESS_LABEL,
            ID_ADDRESS_NOTE,
            ID_CODE_LABEL,
            ID_CODE_HINT,
        ] {
            assert_eq!(find(id).style & WS_TABSTOP, 0, "{id}");
        }
        assert_eq!(find(ID_OK).style & 0xF, BS_DEFPUSHBUTTON);
        assert_eq!(find(ID_CANCEL).style & 0xF, BS_PUSHBUTTON);
        // The boxes have a scroll for what does not fit.
        assert_eq!(find(ID_ADDRESS).style & ES_AUTOHSCROLL, ES_AUTOHSCROLL);
        assert_eq!(find(ID_CODE).style & ES_AUTOHSCROLL, ES_AUTOHSCROLL);
        // Only the code box takes a paste of more than one line, and neither
        // takes Enter.
        assert_eq!(find(ID_ADDRESS).style & ES_MULTILINE, 0);
        assert_eq!(find(ID_CODE).style & ES_MULTILINE, ES_MULTILINE);
        const ES_WANTRETURN: u32 = 0x1000;
        assert_eq!(find(ID_ADDRESS).style & ES_WANTRETURN, 0);
        assert_eq!(find(ID_CODE).style & ES_WANTRETURN, 0);
    }

    #[test]
    fn every_control_is_inside_the_dialog_and_no_two_boxes_overlap() {
        let controls = controls();
        for control in &controls {
            let (x, y, width, height) = control.rect;
            assert!(x >= 0 && y >= 0, "{}", control.id);
            assert!(x + width <= WIDTH && y + height <= HEIGHT, "{}", control.id);
        }
        for (index, a) in controls.iter().enumerate() {
            for b in &controls[index + 1..] {
                let (ax, ay, aw, ah) = a.rect;
                let (bx, by, bw, bh) = b.rect;
                let apart = ax + aw <= bx || bx + bw <= ax || ay + ah <= by || by + bh <= ay;
                assert!(apart, "{} and {}", a.id, b.id);
            }
        }
    }

    // A code is seven characters, and so is what formatting leaves.
    const _: () = assert!(CODE_LIMIT > 7);
    // The longest host name, a colon and the longest port.
    const _: () = assert!(ADDRESS_LIMIT >= 253 + ":65535".len());

    #[test]
    fn the_note_under_the_address_says_why_it_is_not_taken() {
        assert_eq!(address_note(""), "");
        assert_eq!(address_note("center:7878"), "");
        assert_eq!(address_note("100.71.140.68:7878"), "");
        assert_eq!(
            address_note("center"),
            "The address is not of the form <host>:<port>."
        );
        assert_eq!(
            address_note("under_score:1"),
            "The host is not a host name or an IPv4 address."
        );
        assert_eq!(
            address_note("center:0"),
            "The port is not a number from 1 to 65535."
        );
        assert_eq!(
            address_note("8.8.8.8:80"),
            "The address is not a loopback, Tailscale or private network address."
        );
    }

    #[test]
    fn only_a_path_that_is_not_encrypted_is_asked_about() {
        let ask = |text: &str| plaintext_question(&RemoteAddress::parse(text).unwrap());
        assert_eq!(ask("100.71.140.68:7878"), None);
        assert_eq!(ask("127.0.0.1:7878"), None);
        assert_eq!(ask("localhost:7878"), None);
        assert_eq!(ask("center.tailnet.ts.net:7878"), None);
        for text in ["192.168.50.11:7878", "10.0.0.1:7878", "center:7878"] {
            let question = ask(text).unwrap();
            assert!(question.contains(text), "{text}");
            assert!(question.contains("(plaintext)"), "{text}");
            assert!(question.contains("pairing code"), "{text}");
        }
    }
}
