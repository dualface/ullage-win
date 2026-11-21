//! The pairing dialog as a list of shapes.
//!
//! Like the rest of the scene, it is plain geometry in device-independent
//! pixels (DIPs) and colour roles, so it is tested on any platform. The
//! Windows layer owns the controls (two boxes to type in, two labels, two
//! buttons) and draws what is not a control: the title, the frames of the
//! boxes, the status line and the hint. The frames of the boxes are drawn
//! here, and the native boxes sit inside them.

use super::{Align, Font, FontKind, Measure, Rect, Shape, font, text, text_box_width, wrap};
use crate::remote::address::{AddressError, RemoteAddress};
use crate::theme::Role;

pub const TITLE: &str = "Pair with a remote daemon";
pub const SUBTITLE: &str = "Show the usage of an ullage daemon on another host.";
pub const ADDRESS_LABEL: &str = "ADDRESS";
pub const CODE_LABEL: &str = "PAIR CODE";
pub const HINT: &str =
    "Run \"ullage device pair\" on the daemon's host. The code looks like ABC-DEF.";
pub const PLAINTEXT_NOTE: &str = "This address is not known to be on Tailscale (a host name is \
     judged by its spelling): the pairing code and the device token may cross the network as \
     plain text. (plaintext)";
pub const PRIMARY: &str = "Pair";
pub const PRIMARY_PLAINTEXT: &str = "Pair anyway";
pub const CANCEL: &str = "Cancel";

/// Width of the client area.
pub const WIDTH: f32 = 420.0;
pub const PAD: f32 = 24.0;
const PAD_BOTTOM: f32 = 20.0;
const TITLE_LINE: f32 = 24.0;
const SUBTITLE_LINE: f32 = 19.0;
const HEADER_GAP: f32 = 20.0;
const LABEL_HEIGHT: f32 = 16.0;
const LABEL_GAP: f32 = 6.0;
const BOX_HEIGHT: f32 = 36.0;
const BOX_RADIUS: f32 = 6.0;
const BOX_INSET: f32 = 10.0;
/// The height of the line the native box shows its text in.
pub const EDIT_LINE: f32 = 21.0;
const CODE_BOX_WIDTH: f32 = 168.0;
const STATUS_GAP: f32 = 8.0;
const STATUS_LINE: f32 = 18.0;
const NOTE_PAD_X: f32 = 10.0;
const NOTE_PAD_Y: f32 = 8.0;
const NOTE_LINE: f32 = 17.0;
const SECTION_GAP: f32 = 16.0;
const HINT_GAP: f32 = 8.0;
const HINT_LINE: f32 = 17.0;
const BUTTONS_GAP: f32 = 22.0;
const BUTTON_HEIGHT: f32 = 32.0;
const BUTTON_RADIUS: f32 = 4.0;
const BUTTON_PAD: f32 = 16.0;
const BUTTON_GAP: f32 = 8.0;
const BUTTON_MIN_WIDTH: f32 = 88.0;
/// What a button leaves free around what it draws, for the focus ring.
pub const BUTTON_MARGIN: f32 = 2.0;
const RING: f32 = 2.0;

pub const EDIT_FONT: Font = Font {
    kind: FontKind::Ui,
    size: 14.0,
    weight: 400,
};
pub const CODE_FONT: Font = Font {
    kind: FontKind::Mono,
    size: 15.0,
    weight: 400,
};
pub const LABEL_FONT: Font = Font {
    kind: FontKind::Ui,
    size: 11.0,
    weight: 600,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    None,
    Address,
    Code,
}

/// What the line under the address says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    Empty,
    /// The address is not taken, and why.
    Invalid(String),
    /// The address is taken, and its path is not encrypted.
    Plaintext,
}

/// The line under the address, for the text typed in the address box.
pub fn status(address_text: &str) -> Status {
    if address_text.is_empty() {
        return Status::Empty;
    }
    match RemoteAddress::parse(address_text) {
        Err(error) => Status::Invalid(sentence(error)),
        Ok(address) if address.is_encrypted_path() => Status::Empty,
        Ok(_) => Status::Plaintext,
    }
}

/// The words of an `AddressError`, as a sentence.
fn sentence(error: AddressError) -> String {
    let mut note = error.to_string();
    if let Some(first) = note.get_mut(..1) {
        first.make_ascii_uppercase();
    }
    note.push('.');
    note
}

/// The label of the main button: pressing it is also the user's word that
/// the code may go over a path that is not encrypted.
pub fn primary_label(status: &Status) -> &'static str {
    match status {
        Status::Plaintext => PRIMARY_PLAINTEXT,
        _ => PRIMARY,
    }
}

/// What the scene depends on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Model {
    pub address: String,
    pub focus: Focus,
}

/// Where everything is, in DIPs, and the shapes that are not controls.
#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    /// The size of the client area.
    pub size: (f32, f32),
    pub address_label: Rect,
    pub address_box: Rect,
    pub address_edit: Rect,
    pub code_label: Rect,
    pub code_box: Rect,
    pub code_edit: Rect,
    /// The areas of the buttons, with the margin for their focus rings.
    pub cancel: Rect,
    pub primary: Rect,
    /// The plain text note, when there is one: it has to be wholly on the
    /// screen before an address that is not encrypted can be paired with.
    pub note: Option<Rect>,
    pub shapes: Vec<Shape>,
}

/// The layout and the shapes for `model`.
pub fn frame(model: &Model, measure: &dyn Measure) -> Frame {
    let status = status(&model.address);
    let mut note = None;
    let inner = WIDTH - 2.0 * PAD;
    let mut shapes = Vec::new();
    let mut y = PAD;

    // Header.
    let title_font = font(FontKind::Ui, 18.0, 700);
    shapes.push(text(
        Rect::new(PAD, y, inner, TITLE_LINE),
        TITLE,
        title_font,
        Role::Text,
        Align::Left,
        false,
    ));
    y += TITLE_LINE;
    let subtitle_font = font(FontKind::Ui, 13.0, 400);
    for line in wrap(SUBTITLE, inner, subtitle_font, measure) {
        shapes.push(text(
            Rect::new(PAD, y, inner, SUBTITLE_LINE),
            line,
            subtitle_font,
            Role::Sub,
            Align::Left,
            false,
        ));
        y += SUBTITLE_LINE;
    }
    y += HEADER_GAP;

    // Address.
    let address_label = Rect::new(PAD, y, inner, LABEL_HEIGHT);
    y += LABEL_HEIGHT + LABEL_GAP;
    let address_box = Rect::new(PAD, y, inner, BOX_HEIGHT);
    let address_edit = edit_rect(address_box);
    y += BOX_HEIGHT;

    // The status line takes no room when there is nothing to say.
    match &status {
        Status::Empty => {}
        Status::Invalid(why) => {
            y += STATUS_GAP;
            let status_font = font(FontKind::Ui, 13.0, 400);
            for line in wrap(why, inner, status_font, measure) {
                shapes.push(text(
                    Rect::new(PAD, y, inner, STATUS_LINE),
                    line,
                    status_font,
                    Role::Crit,
                    Align::Left,
                    false,
                ));
                y += STATUS_LINE;
            }
        }
        Status::Plaintext => {
            y += STATUS_GAP;
            let note_font = font(FontKind::Ui, 12.0, 400);
            let lines = wrap(PLAINTEXT_NOTE, inner - 2.0 * NOTE_PAD_X, note_font, measure);
            let height = 2.0 * NOTE_PAD_Y + NOTE_LINE * lines.len() as f32;
            note = Some(Rect::new(PAD, y, inner, height));
            shapes.push(Shape::Rect {
                rect: Rect::new(PAD, y, inner, height),
                radius: BOX_RADIUS,
                fill: Some(Role::WarnBg),
                border: None,
            });
            let mut line_y = y + NOTE_PAD_Y;
            for line in lines {
                shapes.push(text(
                    Rect::new(
                        PAD + NOTE_PAD_X,
                        line_y,
                        inner - 2.0 * NOTE_PAD_X,
                        NOTE_LINE,
                    ),
                    line,
                    note_font,
                    Role::WarnText,
                    Align::Left,
                    false,
                ));
                line_y += NOTE_LINE;
            }
            y += height;
        }
    }
    y += SECTION_GAP;

    // Code.
    let code_label = Rect::new(PAD, y, inner, LABEL_HEIGHT);
    y += LABEL_HEIGHT + LABEL_GAP;
    let code_box = Rect::new(PAD, y, CODE_BOX_WIDTH, BOX_HEIGHT);
    let code_edit = edit_rect(code_box);
    y += BOX_HEIGHT + HINT_GAP;
    let hint_font = font(FontKind::Ui, 12.0, 400);
    for line in wrap(HINT, inner, hint_font, measure) {
        shapes.push(text(
            Rect::new(PAD, y, inner, HINT_LINE),
            line,
            hint_font,
            Role::Faint,
            Align::Left,
            false,
        ));
        y += HINT_LINE;
    }
    y += BUTTONS_GAP;

    // Buttons, at the right. The main button keeps the width of its longer
    // label, so that it does not change size when the label does.
    let primary_font = font(FontKind::Ui, 13.0, 600);
    let primary_w = (text_box_width(measure, PRIMARY_PLAINTEXT, primary_font) + 2.0 * BUTTON_PAD)
        .max(BUTTON_MIN_WIDTH);
    let cancel_w = BUTTON_MIN_WIDTH;
    let area_h = BUTTON_HEIGHT + 2.0 * BUTTON_MARGIN;
    let primary_x = WIDTH - PAD - primary_w;
    let cancel_x = primary_x - BUTTON_GAP - cancel_w;
    let primary = Rect::new(
        primary_x - BUTTON_MARGIN,
        y,
        primary_w + 2.0 * BUTTON_MARGIN,
        area_h,
    );
    let cancel = Rect::new(
        cancel_x - BUTTON_MARGIN,
        y,
        cancel_w + 2.0 * BUTTON_MARGIN,
        area_h,
    );
    y += area_h + PAD_BOTTOM;

    // The frames of the boxes, under the native boxes.
    let invalid = matches!(status, Status::Invalid(_));
    shapes.insert(0, box_frame(address_box, Focus::Address, invalid, model));
    shapes.insert(1, box_frame(code_box, Focus::Code, false, model));

    Frame {
        size: (WIDTH, y),
        address_label,
        address_box,
        address_edit,
        code_label,
        code_box,
        code_edit,
        cancel,
        primary,
        note,
        shapes,
    }
}

/// The frame of a box: a ring in the accent colour while it has the focus,
/// and in the colour of errors while what is in it is not taken.
fn box_frame(rect: Rect, owner: Focus, invalid: bool, model: &Model) -> Shape {
    let border = if invalid {
        (Role::Crit, RING)
    } else if model.focus == owner {
        (Role::Accent, RING)
    } else {
        (Role::CardBorder, 1.0)
    };
    Shape::Rect {
        rect,
        radius: BOX_RADIUS,
        fill: Some(Role::Card),
        border: Some(border),
    }
}

/// Where the native box sits in its frame: as high as its line of text, in
/// the middle, with room at both sides.
fn edit_rect(frame: Rect) -> Rect {
    Rect::new(
        frame.x + BOX_INSET,
        frame.y + (frame.h - EDIT_LINE) / 2.0,
        frame.w - 2.0 * BOX_INSET,
        EDIT_LINE,
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// The button that pairs.
    Primary,
    Secondary,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ButtonState {
    pub hover: bool,
    pub pressed: bool,
    pub disabled: bool,
    /// Whether the keyboard focus ring shows.
    pub focus_ring: bool,
}

/// The shapes of a button, in the coordinates of the button's own area
/// (`area` is its size, with the margin for the ring). The area is cleared in
/// the colour of the dialog first, because the button is drawn with round
/// corners in a rectangular window.
pub fn button_shapes(kind: Kind, state: ButtonState, label: &str, area: Rect) -> Vec<Shape> {
    let body = Rect::new(
        area.x + BUTTON_MARGIN,
        area.y + BUTTON_MARGIN,
        area.w - 2.0 * BUTTON_MARGIN,
        area.h - 2.0 * BUTTON_MARGIN,
    );
    let mut shapes = vec![Shape::Rect {
        rect: area,
        radius: 0.0,
        fill: Some(Role::Win),
        border: None,
    }];
    let (fill, border, label_color) = match (kind, state.disabled) {
        (Kind::Primary, false) => (Role::PrimaryBg, None, Role::PrimaryText),
        (Kind::Primary, true) => (Role::Track, None, Role::Faint),
        (Kind::Secondary, false) => (Role::Card, Some((Role::ButtonBorder, 1.0)), Role::Text),
        (Kind::Secondary, true) => (Role::Card, Some((Role::ButtonBorder, 1.0)), Role::Faint),
    };
    shapes.push(Shape::Rect {
        rect: body,
        radius: BUTTON_RADIUS,
        fill: Some(fill),
        border,
    });
    // Hover and pressed lay the hover colour over the button, once and twice.
    let overlays = match (state.disabled, state.hover, state.pressed) {
        (true, _, _) => 0,
        (false, _, true) => 2,
        (false, true, false) => 1,
        _ => 0,
    };
    for _ in 0..overlays {
        shapes.push(Shape::Rect {
            rect: body,
            radius: BUTTON_RADIUS,
            fill: Some(Role::Hover),
            border: None,
        });
    }
    let weight = if kind == Kind::Primary { 600 } else { 400 };
    shapes.push(text(
        body,
        label,
        font(FontKind::Ui, 13.0, weight),
        label_color,
        Align::Center,
        false,
    ));
    if state.focus_ring && !state.disabled {
        shapes.push(Shape::Rect {
            rect: Rect::new(
                area.x + RING / 2.0,
                area.y + RING / 2.0,
                area.w - RING,
                area.h - RING,
            ),
            radius: BUTTON_RADIUS + BUTTON_MARGIN,
            fill: None,
            border: Some((Role::Accent, RING)),
        });
    }
    shapes
}

/// A rectangle in DIPs as `(x, y, width, height)` in pixels at `dpi`.
pub fn to_pixels(rect: Rect, dpi: u32) -> (i32, i32, i32, i32) {
    let scale = dpi as f32 / 96.0;
    (
        (rect.x * scale).round() as i32,
        (rect.y * scale).round() as i32,
        (rect.w * scale).round() as i32,
        (rect.h * scale).round() as i32,
    )
}

#[cfg(test)]
mod tests {
    use super::super::Text;
    use super::super::testing::{FakeMeasure, texts};
    use super::*;

    fn model(address: &str, focus: Focus) -> Model {
        Model {
            address: address.to_owned(),
            focus,
        }
    }

    fn frame_of(address: &str, focus: Focus) -> Frame {
        frame(&model(address, focus), &FakeMeasure)
    }

    /// Every role a shape uses.
    fn roles(shapes: &[Shape]) -> Vec<Role> {
        let mut found = Vec::new();
        for shape in shapes {
            match shape {
                Shape::Rect { fill, border, .. } => {
                    found.extend(*fill);
                    found.extend(border.map(|(role, _)| role));
                }
                Shape::Text(text) => found.push(text.color),
                _ => {}
            }
        }
        found
    }

    #[test]
    fn the_status_line_has_three_results() {
        assert_eq!(status(""), Status::Empty);
        assert_eq!(status("center:7878"), Status::Plaintext);
        assert_eq!(status("100.71.140.68:7878"), Status::Empty);
        assert_eq!(status("127.0.0.1:7878"), Status::Empty);
        assert_eq!(status("localhost:7878"), Status::Empty);
        assert_eq!(status("center.tailnet.ts.net:7878"), Status::Empty);
        assert_eq!(status("192.168.50.11:7878"), Status::Plaintext);
        let Status::Invalid(why) = status("8.8.8.8:80") else {
            panic!("a public address is not taken");
        };
        assert_eq!(
            why,
            "The address is not a loopback, Tailscale or private network address."
        );
        for text in ["center", "under_score:1", "center:0"] {
            assert!(matches!(status(text), Status::Invalid(_)), "{text}");
        }
    }

    #[test]
    fn the_main_button_says_when_the_path_is_plain_text() {
        assert_eq!(primary_label(&status("")), "Pair");
        assert_eq!(primary_label(&status("100.71.140.68:1")), "Pair");
        assert_eq!(primary_label(&status("8.8.8.8:1")), "Pair");
        assert_eq!(primary_label(&status("center:1")), "Pair anyway");
        assert!(PLAINTEXT_NOTE.contains("(plaintext)"));
    }

    #[test]
    fn what_the_frame_draws_is_inside_the_client_area() {
        for address in ["", "center:7878", "8.8.8.8:80", "center"] {
            let frame = frame_of(address, Focus::Address);
            let (width, height) = frame.size;
            let area = Rect::new(0.0, 0.0, width, height);
            let inside = |rect: Rect| {
                rect.x >= area.x
                    && rect.y >= area.y
                    && rect.right() <= area.right() + 0.01
                    && rect.bottom() <= area.bottom() + 0.01
            };
            for shape in &frame.shapes {
                match shape {
                    Shape::Rect { rect, .. } => assert!(inside(*rect), "{address}: {rect:?}"),
                    Shape::Text(text) => assert!(inside(text.rect), "{address}: {:?}", text.text),
                    _ => {}
                }
            }
            for rect in [
                frame.address_label,
                frame.address_box,
                frame.address_edit,
                frame.code_label,
                frame.code_box,
                frame.code_edit,
                frame.cancel,
                frame.primary,
            ] {
                assert!(inside(rect), "{address}: {rect:?}");
            }
        }
    }

    #[test]
    fn the_controls_do_not_overlap_and_the_edits_are_in_their_frames() {
        for address in ["", "center:7878", "8.8.8.8:80"] {
            let frame = frame_of(address, Focus::None);
            let apart = |a: Rect, b: Rect| {
                a.right() <= b.x || b.right() <= a.x || a.bottom() <= b.y || b.bottom() <= a.y
            };
            let rects = [
                frame.address_label,
                frame.address_box,
                frame.code_label,
                frame.code_box,
                frame.cancel,
                frame.primary,
            ];
            for (index, a) in rects.iter().enumerate() {
                for b in &rects[index + 1..] {
                    assert!(apart(*a, *b), "{address}: {a:?} and {b:?}");
                }
            }
            for (edit, frame) in [
                (frame.address_edit, frame.address_box),
                (frame.code_edit, frame.code_box),
            ] {
                assert!(edit.x >= frame.x && edit.right() <= frame.right());
                assert!(edit.y >= frame.y && edit.bottom() <= frame.bottom());
                // Centred in the frame.
                assert!((edit.y - frame.y - (frame.bottom() - edit.bottom())).abs() < 0.01);
            }
        }
    }

    #[test]
    fn the_buttons_are_at_the_right_and_the_main_one_is_last() {
        let frame = frame_of("", Focus::None);
        assert!(frame.cancel.right() <= frame.primary.x);
        assert!((frame.primary.right() - BUTTON_MARGIN - (WIDTH - PAD)).abs() < 0.01);
        assert_eq!(frame.cancel.y, frame.primary.y);
        // The main button does not change size with its label.
        let plain = frame_of("center:1", Focus::None);
        assert_eq!(
            (plain.primary.x, plain.primary.w, plain.primary.h),
            (frame.primary.x, frame.primary.w, frame.primary.h)
        );
        assert_eq!(
            (plain.cancel.x, plain.cancel.w, plain.cancel.h),
            (frame.cancel.x, frame.cancel.w, frame.cancel.h)
        );
    }

    #[test]
    fn the_client_area_grows_with_the_status_line() {
        let none = frame_of("", Focus::None).size.1;
        let invalid = frame_of("8.8.8.8:80", Focus::None).size.1;
        let plain = frame_of("center:1", Focus::None).size.1;
        assert!(invalid > none);
        assert!(plain > invalid, "the plain text note takes the most room");
        // Taking the status away gives the room back.
        assert_eq!(frame_of("100.71.140.68:1", Focus::None).size.1, none);
    }

    #[test]
    fn the_note_has_a_rect_only_for_a_path_that_is_not_encrypted_and_it_is_drawn_there() {
        for address in ["", "100.71.140.68:7878", "8.8.8.8:80"] {
            assert_eq!(frame_of(address, Focus::None).note, None, "{address}");
        }
        let frame = frame_of("center:7878", Focus::None);
        let note = frame.note.expect("a note");
        assert!(note.bottom() <= frame.size.1 && note.right() <= frame.size.0);
        // The yellow plate is the note, and its text is inside it.
        assert!(frame.shapes.iter().any(|shape| matches!(
            shape,
            Shape::Rect { rect, fill: Some(Role::WarnBg), .. } if *rect == note
        )));
        for text in texts(&frame.shapes) {
            if text.color == Role::WarnText {
                assert!(text.rect.y >= note.y && text.rect.bottom() <= note.bottom());
            }
        }
        // Under the address box, above the code.
        assert!(note.y >= frame.address_box.bottom());
        assert!(note.bottom() <= frame.code_label.y);
    }

    #[test]
    fn the_longest_status_line_is_wrapped_and_not_cut_off() {
        let frame = frame_of("center:1", Focus::None);
        let lines: Vec<&Text> = texts(&frame.shapes)
            .into_iter()
            .filter(|text| text.color == Role::WarnText)
            .collect();
        assert!(lines.len() > 1, "the note needs more than one line");
        let joined = lines
            .iter()
            .map(|text| text.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(
            joined,
            PLAINTEXT_NOTE
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        );
        for line in &lines {
            assert!(!line.ellipsis);
            assert!(
                FakeMeasure.width(&line.text, line.font) <= line.rect.w,
                "{}",
                line.text
            );
        }
        // The hint and the subtitle are whole too.
        let all = texts(&frame.shapes);
        for (color, whole) in [(Role::Faint, HINT), (Role::Sub, SUBTITLE)] {
            let joined = all
                .iter()
                .filter(|text| text.color == color)
                .map(|text| text.text.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            assert_eq!(joined, whole);
        }
    }

    #[test]
    fn a_box_has_a_ring_only_while_it_has_the_focus_or_is_not_taken() {
        let ring = |frame: &Frame, index: usize| match &frame.shapes[index] {
            Shape::Rect { border, .. } => border.unwrap(),
            other => panic!("{other:?}"),
        };
        let none = frame_of("", Focus::None);
        assert_eq!(ring(&none, 0), (Role::CardBorder, 1.0));
        assert_eq!(ring(&none, 1), (Role::CardBorder, 1.0));
        let address = frame_of("center:1", Focus::Address);
        assert_eq!(ring(&address, 0), (Role::Accent, RING));
        assert_eq!(ring(&address, 1), (Role::CardBorder, 1.0));
        let code = frame_of("center:1", Focus::Code);
        assert_eq!(ring(&code, 0), (Role::CardBorder, 1.0));
        assert_eq!(ring(&code, 1), (Role::Accent, RING));
        // An address that is not taken is marked, with or without the focus.
        for focus in [Focus::None, Focus::Address, Focus::Code] {
            let bad = frame_of("8.8.8.8:80", focus);
            assert_eq!(ring(&bad, 0), (Role::Crit, RING), "{focus:?}");
        }
        // An empty address is not marked.
        assert_eq!(ring(&frame_of("", Focus::Address), 0), (Role::Accent, RING));
    }

    #[test]
    fn the_status_line_uses_the_roles_of_its_meaning() {
        let invalid = frame_of("8.8.8.8:80", Focus::None);
        assert!(roles(&invalid.shapes).contains(&Role::Crit));
        assert!(!roles(&invalid.shapes).contains(&Role::WarnBg));
        let plain = frame_of("center:1", Focus::None);
        assert!(roles(&plain.shapes).contains(&Role::WarnBg));
        assert!(roles(&plain.shapes).contains(&Role::WarnText));
        let fine = frame_of("100.71.140.68:1", Focus::None);
        for role in [Role::Crit, Role::WarnBg, Role::WarnText] {
            assert!(!roles(&fine.shapes).contains(&role), "{role:?}");
        }
    }

    #[test]
    fn the_title_and_the_header_are_where_they_are_expected() {
        let frame = frame_of("", Focus::None);
        let all = texts(&frame.shapes);
        let title = all.iter().find(|text| text.text == TITLE).unwrap();
        assert_eq!(title.font.weight, 700);
        assert_eq!(title.color, Role::Text);
        assert_eq!(title.rect.x, PAD);
        let subtitle = all.iter().find(|text| text.color == Role::Sub).unwrap();
        assert!(subtitle.rect.y >= title.rect.bottom());
        // The labels are controls and are not drawn here.
        assert!(all.iter().all(|text| text.text != ADDRESS_LABEL));
        assert!(all.iter().all(|text| text.text != CODE_LABEL));
    }

    #[test]
    fn the_layout_scales_with_the_dpi() {
        let frame = frame_of("center:1", Focus::None);
        for rect in [
            frame.address_label,
            frame.address_box,
            frame.address_edit,
            frame.code_label,
            frame.code_box,
            frame.code_edit,
            frame.primary,
            frame.cancel,
        ] {
            let base = to_pixels(rect, 96);
            for dpi in [144, 192] {
                let scaled = to_pixels(rect, dpi);
                let scale = dpi as f32 / 96.0;
                for (a, b) in [
                    (base.0, scaled.0),
                    (base.1, scaled.1),
                    (base.2, scaled.2),
                    (base.3, scaled.3),
                ] {
                    assert!((a as f32 * scale - b as f32).abs() <= 1.0, "{rect:?} {dpi}");
                }
            }
        }
        assert_eq!(
            to_pixels(Rect::new(10.0, 20.0, 30.0, 40.0), 96),
            (10, 20, 30, 40)
        );
    }

    #[test]
    fn no_two_pieces_of_text_overlap() {
        for address in ["", "center:7878", "8.8.8.8:80"] {
            let frame = frame_of(address, Focus::None);
            let all = texts(&frame.shapes);
            for (index, a) in all.iter().enumerate() {
                for b in &all[index + 1..] {
                    let (a, b) = (a.rect, b.rect);
                    let apart = a.right() <= b.x
                        || b.right() <= a.x
                        || a.bottom() <= b.y
                        || b.bottom() <= a.y;
                    assert!(apart, "{address}: {a:?} and {b:?}");
                }
            }
        }
    }

    fn button(kind: Kind, state: ButtonState) -> Vec<Shape> {
        button_shapes(kind, state, "Pair", Rect::new(0.0, 0.0, 92.0, 36.0))
    }

    fn overlays(shapes: &[Shape]) -> usize {
        shapes
            .iter()
            .filter(|shape| {
                matches!(
                    shape,
                    Shape::Rect {
                        fill: Some(Role::Hover),
                        ..
                    }
                )
            })
            .count()
    }

    #[test]
    fn a_button_is_cleared_in_the_colour_of_the_dialog_first() {
        for kind in [Kind::Primary, Kind::Secondary] {
            let shapes = button(kind, ButtonState::default());
            let Shape::Rect {
                rect, fill, radius, ..
            } = &shapes[0]
            else {
                panic!("{:?}", shapes[0]);
            };
            assert_eq!(*fill, Some(Role::Win));
            assert_eq!(*radius, 0.0);
            assert_eq!(*rect, Rect::new(0.0, 0.0, 92.0, 36.0));
        }
    }

    #[test]
    fn the_two_buttons_have_their_own_colours() {
        let primary = button(Kind::Primary, ButtonState::default());
        assert!(roles(&primary).contains(&Role::PrimaryBg));
        assert!(roles(&primary).contains(&Role::PrimaryText));
        let secondary = button(Kind::Secondary, ButtonState::default());
        assert!(roles(&secondary).contains(&Role::Card));
        assert!(roles(&secondary).contains(&Role::ButtonBorder));
        assert!(roles(&secondary).contains(&Role::Text));
        assert!(!roles(&secondary).contains(&Role::PrimaryBg));
    }

    #[test]
    fn hover_and_pressed_lay_the_hover_colour_over_a_button() {
        let state = |hover, pressed| ButtonState {
            hover,
            pressed,
            ..Default::default()
        };
        assert_eq!(overlays(&button(Kind::Primary, state(false, false))), 0);
        assert_eq!(overlays(&button(Kind::Primary, state(true, false))), 1);
        assert_eq!(overlays(&button(Kind::Primary, state(true, true))), 2);
        assert_eq!(overlays(&button(Kind::Secondary, state(false, true))), 2);
    }

    #[test]
    fn a_button_that_is_off_is_grey_and_does_not_react() {
        let off = ButtonState {
            disabled: true,
            hover: true,
            pressed: true,
            focus_ring: true,
        };
        let shapes = button(Kind::Primary, off);
        assert!(roles(&shapes).contains(&Role::Track));
        assert!(roles(&shapes).contains(&Role::Faint));
        assert!(!roles(&shapes).contains(&Role::PrimaryBg));
        assert_eq!(overlays(&shapes), 0);
        assert!(
            !roles(&shapes).contains(&Role::Accent),
            "no ring on a button that is off"
        );
    }

    #[test]
    fn the_focus_ring_shows_only_when_asked_for() {
        let with = |ring| ButtonState {
            focus_ring: ring,
            ..Default::default()
        };
        for kind in [Kind::Primary, Kind::Secondary] {
            assert!(!roles(&button(kind, with(false))).contains(&Role::Accent));
            assert!(roles(&button(kind, with(true))).contains(&Role::Accent));
        }
    }

    #[test]
    fn what_a_button_draws_stays_in_its_area() {
        let area = Rect::new(0.0, 0.0, 92.0, 36.0);
        for kind in [Kind::Primary, Kind::Secondary] {
            for ring in [false, true] {
                let state = ButtonState {
                    focus_ring: ring,
                    hover: true,
                    ..Default::default()
                };
                for shape in button_shapes(kind, state, "Pair anyway", area) {
                    let rect = match shape {
                        Shape::Rect { rect, .. } => rect,
                        Shape::Text(text) => text.rect,
                        _ => continue,
                    };
                    assert!(
                        rect.x >= 0.0
                            && rect.y >= 0.0
                            && rect.right() <= 92.01
                            && rect.bottom() <= 36.01,
                        "{rect:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_shape_names_a_role_and_never_a_colour() {
        // `Shape` holds only roles: this fails to compile when a colour is
        // added to it. The frame and the buttons use the roles of the palette.
        let all = roles(&frame_of("center:1", Focus::Address).shapes);
        assert!(!all.is_empty());
        assert!(all.iter().all(|role| {
            matches!(
                role,
                Role::Text
                    | Role::Sub
                    | Role::Faint
                    | Role::Card
                    | Role::CardBorder
                    | Role::Accent
                    | Role::Crit
                    | Role::WarnBg
                    | Role::WarnText
            )
        }));
    }
}
