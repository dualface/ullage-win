//! The window as a list of shapes.
//!
//! Everything here is plain geometry in device-independent pixels (DIPs), so
//! the layout is tested on any platform. The Windows layer measures text for
//! it ([`Measure`]) and draws the shapes with Direct2D.
//!
//! A scene has two layers. The title bar is fixed and its shapes are in
//! window coordinates. The content scrolls and its shapes are in content
//! coordinates, with the origin at the top left of the content area.

mod cards;
mod strip;
mod title;
mod unavailable;

use ullage_cli::usage_view::UsageCard;

use crate::prefs::Layout;
use crate::state::Failure;
use crate::theme::Role;

/// Width of the window frame line, drawn inside the window.
pub const BORDER: f32 = 1.0;
pub const WIDTH_COLUMNS: f32 = 800.0;
pub const WIDTH_SINGLE: f32 = 400.0;
pub const WIDTH_COMPACT: f32 = 290.0;
/// Space kept free below a window that fills the work area.
pub const WORK_AREA_MARGIN: f32 = 32.0;
/// Lines a wheel notch scrolls, times the height of a line.
pub const SCROLL_STEP: f32 = 72.0;
/// Transparent space around the window that holds its shadow.
pub const SHADOW_MARGIN: f32 = 12.0;

/// The size all layout numbers are given for.
const BASE_TEXT_SIZE: f32 = 13.0;
const CONTENT_PAD_TOP: f32 = 2.0;
const CONTENT_PAD_SIDE: f32 = 10.0;
const CONTENT_PAD_BOTTOM: f32 = 10.0;
const CARD_GAP: f32 = 8.0;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    pub fn right(&self) -> f32 {
        self.x + self.w
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }

    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.right() && y >= self.y && y < self.bottom()
    }

    pub fn translated(&self, dx: f32, dy: f32) -> Self {
        Self::new(self.x + dx, self.y + dy, self.w, self.h)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FontKind {
    /// Segoe UI Variable Text, or Segoe UI.
    Ui,
    /// Cascadia Mono, or Consolas.
    Mono,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Left,
    Right,
    Center,
}

/// A font as the scene names it; the Windows layer finds the real one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Font {
    pub kind: FontKind,
    pub size: f32,
    /// 400, 600, or 700.
    pub weight: u16,
}

/// Measures a single line of text.
pub trait Measure {
    fn width(&self, text: &str, font: Font) -> f32;
}

#[derive(Clone, Debug, PartialEq)]
pub struct Text {
    /// The box the text sits in. It is centred vertically in it.
    pub rect: Rect,
    pub text: String,
    pub font: Font,
    pub color: Role,
    pub align: Align,
    /// Cut off the end with an ellipsis when the text is wider than `rect`.
    pub ellipsis: bool,
}

/// How much of a day dot is filled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DotFill {
    Empty,
    Full,
    /// A pie slice from 12 o'clock, clockwise.
    Quarter,
    ThreeQuarters,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    /// The container, with or without the liquid.
    Container {
        liquid: bool,
    },
    Pin {
        filled: bool,
    },
    MoreDots,
    Close,
    Expand,
    Copy,
    Warning,
    PlugOff,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    Rect {
        rect: Rect,
        radius: f32,
        fill: Option<Role>,
        border: Option<(Role, f32)>,
    },
    Text(Text),
    /// A ring of `width`, drawn on top of nothing. `sweep` is the part of it
    /// that is drawn, from 12 o'clock clockwise, 1 for all of it.
    Ring {
        cx: f32,
        cy: f32,
        radius: f32,
        width: f32,
        color: Role,
        sweep: f32,
    },
    Dot {
        cx: f32,
        cy: f32,
        diameter: f32,
        border: f32,
        color: Role,
        fill: DotFill,
    },
    Icon {
        icon: Icon,
        rect: Rect,
        color: Role,
    },
}

/// Something a click can do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Pin,
    More,
    Hide,
    Expand,
    Copy,
    Retry,
    Guide,
}

/// A clickable area. `scrolls` says which layer's coordinates it is in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    pub rect: Rect,
    pub target: Target,
    pub scrolls: bool,
}

/// What the content area holds.
#[derive(Clone, Copy, Debug)]
pub enum Body<'a> {
    /// No result yet.
    Loading,
    /// The daemon answered with no snapshots.
    Empty,
    Cards(&'a [UsageCard]),
    /// Nothing ever answered.
    Unavailable(&'a Failure),
}

pub struct Input<'a> {
    pub layout: Layout,
    pub text_size: u32,
    pub body: Body<'a>,
    /// Filled part of the title bar ring, and the seconds on its label.
    pub countdown: (f32, u64),
    pub pinned: bool,
    /// The button the pointer is on.
    pub hover: Option<Target>,
    /// Tallest the window may be, in DIPs.
    pub max_window_height: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Scene {
    pub width: f32,
    pub height: f32,
    /// Top of the content area in window coordinates.
    pub content_top: f32,
    /// Height of the visible part of the content area.
    pub view_height: f32,
    pub content_height: f32,
    pub title: Vec<Shape>,
    pub content: Vec<Shape>,
    pub hits: Vec<Hit>,
    /// Where cards are, in content coordinates. A press on the content area
    /// outside them drags the window.
    pub card_rects: Vec<Rect>,
}

/// What a point of the window is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spot {
    /// Not on the window at all, as in the margin that holds its shadow.
    Outside,
    Button(Target),
    /// Part of the window that moves it when pressed.
    Drag,
    /// Anything else that takes the click itself.
    Client,
}

impl Scene {
    pub fn max_scroll(&self) -> f32 {
        (self.content_height - self.view_height).max(0.0)
    }

    pub fn spot(&self, x: f32, y: f32, scroll: f32) -> Spot {
        if x < 0.0 || y < 0.0 || x >= self.width || y >= self.height {
            return Spot::Outside;
        }
        // Content shapes are inside the frame line.
        let (x, content_y) = (x - BORDER, y - self.content_top + scroll);
        let in_view = y >= self.content_top && y < self.content_top + self.view_height;
        for hit in &self.hits {
            let inside = if hit.scrolls {
                in_view && hit.rect.contains(x, content_y)
            } else {
                hit.rect.contains(x, y)
            };
            if inside {
                return Spot::Button(hit.target);
            }
        }
        if y < self.content_top {
            return Spot::Drag;
        }
        if in_view {
            let on_card = self
                .card_rects
                .iter()
                .any(|rect| rect.contains(x, content_y));
            return if on_card { Spot::Client } else { Spot::Drag };
        }
        // The bottom border line.
        Spot::Drag
    }

    pub fn button_target(&self, x: f32, y: f32, scroll: f32) -> Option<Target> {
        match self.spot(x, y, scroll) {
            Spot::Button(target) => Some(target),
            _ => None,
        }
    }
}

pub fn window_width(layout: Layout, body: &Body) -> f32 {
    match (layout, body) {
        (Layout::Columns, _) => WIDTH_COLUMNS,
        (Layout::Single, _) | (Layout::Compact, Body::Unavailable(_)) => WIDTH_SINGLE,
        (Layout::Compact, _) => WIDTH_COMPACT,
    }
}

/// Whether the window draws the compact strip: the unavailable state keeps
/// the full title bar and its own card.
fn is_strip(layout: Layout, body: &Body) -> bool {
    layout == Layout::Compact && !matches!(body, Body::Unavailable(_))
}

pub fn build(input: &Input, measure: &dyn Measure) -> Scene {
    let width = window_width(input.layout, &input.body);
    let strip = is_strip(input.layout, &input.body);
    let scale = input.text_size as f32 / BASE_TEXT_SIZE;
    let inner_width = width - 2.0 * BORDER;

    let title = title::build(input, width, strip, measure);

    let mut body = Content::default();
    match input.body {
        Body::Loading => {}
        Body::Empty if strip => strip::empty(&mut body, inner_width, scale, measure),
        Body::Empty => cards::empty(&mut body, inner_width, scale, measure),
        Body::Cards(list) if strip => strip::build(&mut body, list, inner_width, scale, measure),
        Body::Cards(list) => {
            cards::build(&mut body, list, input.layout, inner_width, scale, measure)
        }
        Body::Unavailable(failure) => {
            unavailable::build(&mut body, failure, inner_width, input.hover, measure);
        }
    }

    let content_top = BORDER + title.height;
    let max_view = (input.max_window_height - content_top - BORDER).max(0.0);
    let view_height = body.height.min(max_view);
    let mut hits = title.hits;
    hits.extend(body.hits);
    Scene {
        width,
        height: content_top + view_height + BORDER,
        content_top,
        view_height,
        content_height: body.height,
        title: title.shapes,
        content: body.shapes,
        hits,
        card_rects: body.card_rects,
    }
}

/// What a layout routine adds to the content area.
#[derive(Default)]
struct Content {
    shapes: Vec<Shape>,
    hits: Vec<Hit>,
    card_rects: Vec<Rect>,
    height: f32,
}

fn font(kind: FontKind, size: f32, weight: u16) -> Font {
    Font { kind, size, weight }
}

fn text(
    rect: Rect,
    text: impl Into<String>,
    font: Font,
    color: Role,
    align: Align,
    ellipsis: bool,
) -> Shape {
    Shape::Text(Text {
        rect,
        text: text.into(),
        font,
        color,
        align,
        ellipsis,
    })
}

/// The width a single line of text needs, rounded up to a whole pixel and
/// with a pixel of slack, so a box of this width never cuts it short.
fn text_box_width(measure: &dyn Measure, text: &str, font: Font) -> f32 {
    measure.width(text, font).ceil() + 1.0
}

/// Breaks `text` into lines no wider than `width`, at spaces. A word wider
/// than a line stays whole on its own line.
fn wrap(text: &str, width: f32, font: Font, measure: &dyn Measure) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if line.is_empty() {
            line.push_str(word);
            continue;
        }
        let candidate = format!("{line} {word}");
        if measure.width(&candidate, font) <= width {
            line = candidate;
        } else {
            lines.push(std::mem::take(&mut line));
            line.push_str(word);
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

#[cfg(test)]
pub(crate) mod testing {
    use super::*;

    /// Every character is as wide as a fixed share of the font size.
    pub struct FakeMeasure;

    impl Measure for FakeMeasure {
        fn width(&self, text: &str, font: Font) -> f32 {
            let share = match font.kind {
                FontKind::Mono => 0.6,
                FontKind::Ui => 0.5,
            };
            text.chars().count() as f32 * font.size * share
        }
    }

    pub fn texts(shapes: &[Shape]) -> Vec<&Text> {
        shapes
            .iter()
            .filter_map(|shape| match shape {
                Shape::Text(text) => Some(text),
                _ => None,
            })
            .collect()
    }

    pub fn text_of<'a>(shapes: &'a [Shape], needle: &str) -> &'a Text {
        texts(shapes)
            .into_iter()
            .find(|text| text.text == needle)
            .unwrap_or_else(|| panic!("no text {needle:?}"))
    }
}

#[cfg(test)]
mod tests {
    use super::testing::*;
    use super::*;

    fn input(layout: Layout, body: Body<'_>) -> Input<'_> {
        Input {
            layout,
            text_size: 13,
            body,
            countdown: (1.0, 120),
            pinned: true,
            hover: None,
            max_window_height: 2000.0,
        }
    }

    #[test]
    fn the_window_width_follows_the_layout() {
        let loading = Body::Loading;
        assert_eq!(window_width(Layout::Columns, &loading), 800.0);
        assert_eq!(window_width(Layout::Single, &loading), 400.0);
        assert_eq!(window_width(Layout::Compact, &loading), 290.0);
    }

    #[test]
    fn the_unavailable_state_is_never_narrower_than_the_single_layout() {
        let failure =
            crate::state::failure_for(&ullage_cli::usage_view::FetchError::DaemonUnavailable);
        let body = Body::Unavailable(&failure);
        assert_eq!(window_width(Layout::Compact, &body), 400.0);
        assert_eq!(window_width(Layout::Columns, &body), 800.0);
    }

    #[test]
    fn a_loading_window_is_a_title_bar_and_an_empty_content_area() {
        let scene = build(&input(Layout::Columns, Body::Loading), &FakeMeasure);
        assert_eq!(scene.content_height, 0.0);
        assert_eq!(scene.height, BORDER + 40.0 + BORDER);
        assert!(scene.content.is_empty());
    }

    #[test]
    fn wrapping_breaks_at_spaces_and_keeps_long_words_whole() {
        let font = font(FontKind::Ui, 10.0, 400);
        let lines = wrap("aa bb cc dd", 14.0, font, &FakeMeasure);
        assert_eq!(lines, ["aa", "bb", "cc", "dd"]);
        let lines = wrap("aa bb cc dd", 25.0, font, &FakeMeasure);
        assert_eq!(lines, ["aa bb", "cc dd"]);
        let lines = wrap("abcdefghij k", 20.0, font, &FakeMeasure);
        assert_eq!(lines, ["abcdefghij", "k"]);
        assert!(wrap("", 20.0, font, &FakeMeasure).is_empty());
    }

    #[test]
    fn the_window_is_never_taller_than_the_cap_and_the_rest_scrolls() {
        use super::cards::tests::sample_cards;
        let cards = sample_cards(7);
        let mut input = input(Layout::Single, Body::Cards(&cards));
        input.max_window_height = 300.0;
        let scene = build(&input, &FakeMeasure);
        assert_eq!(scene.height, 300.0);
        assert!(scene.content_height > scene.view_height);
        assert_eq!(scene.max_scroll(), scene.content_height - scene.view_height);
        // The title bar stays: its hit areas do not move with the scroll.
        let at = |scroll| scene.button_target(scene.width - 20.0, 20.0, scroll);
        assert_eq!(at(0.0), at(100.0));
    }

    #[test]
    fn a_short_window_fits_its_content() {
        use super::cards::tests::sample_cards;
        let cards = sample_cards(1);
        let scene = build(&input(Layout::Single, Body::Cards(&cards)), &FakeMeasure);
        assert_eq!(scene.view_height, scene.content_height);
        assert_eq!(scene.max_scroll(), 0.0);
        assert_eq!(
            scene.height,
            scene.content_top + scene.content_height + BORDER
        );
    }

    #[test]
    fn pressing_blank_content_or_the_title_drags_and_a_card_does_not() {
        use super::cards::tests::sample_cards;
        let cards = sample_cards(1);
        let scene = build(&input(Layout::Columns, Body::Cards(&cards)), &FakeMeasure);
        // The title bar, left of the buttons.
        assert_eq!(scene.spot(300.0, 20.0, 0.0), Spot::Drag);
        // On the card.
        let card = scene.card_rects[0];
        let (x, y) = (card.x + 5.0, scene.content_top + card.y + 5.0);
        assert_eq!(scene.spot(x, y, 0.0), Spot::Client);
        // The blank half of the first grid row.
        assert_eq!(
            scene.spot(700.0, scene.content_top + card.y + 5.0, 0.0),
            Spot::Drag
        );
    }

    #[test]
    fn a_point_outside_the_window_is_outside() {
        use super::cards::tests::sample_cards;
        let cards = sample_cards(1);
        let scene = build(&input(Layout::Columns, Body::Cards(&cards)), &FakeMeasure);
        assert_eq!(scene.spot(-1.0, 10.0, 0.0), Spot::Outside);
        assert_eq!(scene.spot(10.0, scene.height, 0.0), Spot::Outside);
        assert_eq!(scene.spot(scene.width, 10.0, 0.0), Spot::Outside);
        assert_ne!(scene.spot(0.0, 0.0, 0.0), Spot::Outside);
    }

    #[test]
    fn content_hits_follow_the_scroll() {
        let failure =
            crate::state::failure_for(&ullage_cli::usage_view::FetchError::DaemonUnavailable);
        let scene = build(
            &input(Layout::Single, Body::Unavailable(&failure)),
            &FakeMeasure,
        );
        let retry = scene
            .hits
            .iter()
            .find(|hit| hit.target == Target::Retry)
            .unwrap()
            .rect;
        let (x, y) = (retry.x + 2.0, scene.content_top + retry.y + 2.0);
        assert_eq!(scene.button_target(x, y, 0.0), Some(Target::Retry));
        assert_eq!(scene.button_target(x, y, 100.0), None);
        assert_eq!(
            scene.button_target(x, y - 100.0, 100.0),
            Some(Target::Retry)
        );
    }
}
