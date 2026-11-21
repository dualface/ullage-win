//! The title bar, for the full window and for the compact strip.

use super::{
    Align, BORDER, FontKind, Hit, Icon, Input, Measure, Rect, Shape, Target, font, text,
    text_box_width,
};
use crate::display::countdown_label;
use crate::state::REFRESH_SECS;
use crate::theme::Role;

const FULL_HEIGHT: f32 = 40.0;
const STRIP_HEIGHT: f32 = 34.0;

pub struct Title {
    pub shapes: Vec<Shape>,
    pub hits: Vec<Hit>,
    pub height: f32,
}

/// Where the pieces of a title bar go; the two bars differ only in numbers.
struct Metrics {
    height: f32,
    pad_left: f32,
    pad_right: f32,
    icon: f32,
    icon_gap: f32,
    name_size: f32,
    ring: f32,
    label_size: f32,
    /// Gap between the ring and its label, and between label and button.
    ring_gap: f32,
    button: (f32, f32),
}

const FULL: Metrics = Metrics {
    height: FULL_HEIGHT,
    pad_left: 12.0,
    pad_right: 4.0,
    icon: 16.0,
    icon_gap: 8.0,
    name_size: 13.0,
    ring: 14.0,
    label_size: 12.0,
    ring_gap: 6.0,
    button: (36.0, 32.0),
};

const STRIP: Metrics = Metrics {
    height: STRIP_HEIGHT,
    pad_left: 10.0,
    pad_right: 2.0,
    icon: 14.0,
    icon_gap: 6.0,
    name_size: 12.0,
    ring: 12.0,
    label_size: 11.0,
    ring_gap: 6.0,
    button: (32.0, 30.0),
};

/// The least room kept between the name and the ring on the strip.
pub(super) const STRIP_NAME_GAP: f32 = 8.0;

/// The narrowest window whose strip title bar holds everything: the icon,
/// the name, the ring, the longest countdown label (`REFRESH_SECS` as
/// minutes and seconds, the most the label ever reads) and the button, with
/// [`STRIP_NAME_GAP`] before the ring. It uses the same numbers as [`build`].
pub(super) fn strip_min_width(measure: &dyn Measure) -> f32 {
    let m = &STRIP;
    let name_w = text_box_width(measure, "Ullage", font(FontKind::Ui, m.name_size, 600));
    let label = countdown_label(REFRESH_SECS);
    let label_w = text_box_width(measure, &label, font(FontKind::Mono, m.label_size, 400));
    BORDER
        + m.pad_left
        + m.icon
        + m.icon_gap
        + name_w
        + STRIP_NAME_GAP
        + m.ring
        + m.ring_gap
        + label_w
        + m.ring_gap
        + m.button.0
        + m.pad_right
        + BORDER
}

pub fn build(input: &Input, width: f32, strip: bool, measure: &dyn Measure) -> Title {
    let metrics = if strip { &STRIP } else { &FULL };
    let unavailable = matches!(input.body, super::Body::Unavailable(_));
    let mut shapes = Vec::new();
    let mut hits = Vec::new();
    let mid = BORDER + metrics.height / 2.0;

    let icon = Rect::new(
        BORDER + metrics.pad_left,
        mid - metrics.icon / 2.0,
        metrics.icon,
        metrics.icon,
    );
    shapes.push(Shape::Icon {
        icon: Icon::Container {
            liquid: !unavailable,
        },
        rect: icon,
        color: Role::Text,
    });
    let name_font = font(FontKind::Ui, metrics.name_size, 600);
    let name_x = icon.right() + metrics.icon_gap;
    shapes.push(text(
        Rect::new(
            name_x,
            mid - metrics.height / 2.0,
            text_box_width(measure, "Ullage", name_font),
            metrics.height,
        ),
        "Ullage",
        name_font,
        Role::Text,
        Align::Left,
        false,
    ));

    // Buttons, from the right edge inwards.
    let (button_w, button_h) = metrics.button;
    let mut right = width - BORDER - metrics.pad_right;
    // The buttons of the full title bar are 8 DIPs apart; the strip has one.
    // `right` is the next button's right edge: after a button it is that
    // button's left edge minus this gap.
    let button_gap = if strip { 0.0 } else { 8.0 };
    let mut add_button = |target: Target, icon: Icon, color: Role, shapes: &mut Vec<Shape>| {
        let rect = Rect::new(right - button_w, mid - button_h / 2.0, button_w, button_h);
        right = rect.x - button_gap;
        if input.hover == Some(target) {
            shapes.push(Shape::Rect {
                rect,
                radius: 4.0,
                fill: Some(Role::Hover),
                border: None,
            });
        }
        let glyph = metrics.icon;
        shapes.push(Shape::Icon {
            icon,
            rect: Rect::new(
                rect.x + (rect.w - glyph) / 2.0,
                rect.y + (rect.h - glyph) / 2.0,
                glyph,
                glyph,
            ),
            color,
        });
        hits.push(Hit {
            rect,
            target,
            scrolls: false,
        });
    };
    if strip {
        add_button(Target::Expand, Icon::Expand, Role::Sub, &mut shapes);
    } else {
        add_button(Target::Hide, Icon::Close, Role::Sub, &mut shapes);
        add_button(Target::More, Icon::MoreDots, Role::Sub, &mut shapes);
        if !unavailable {
            let (icon, color) = if input.pinned {
                (Icon::Pin { filled: true }, Role::Text)
            } else {
                (Icon::Pin { filled: false }, Role::Sub)
            };
            add_button(Target::Pin, icon, color, &mut shapes);
        }
    }

    // The countdown sits left of the buttons: ring, then label.
    let (fraction, seconds) = input.countdown;
    let label = countdown_label(seconds);
    let label_font = font(FontKind::Mono, metrics.label_size, 400);
    let label_w = text_box_width(measure, &label, label_font);
    // `right` is now 8 DIPs left of the last button. The ring and its label
    // end another 4 DIPs further left, which is 12 DIPs from the button: the
    // 8 DIP gap plus the 4 DIPs of padding after the label. On the strip they
    // are `ring_gap` from the button.
    let label_right = if strip {
        right - metrics.ring_gap
    } else {
        right - 4.0
    };
    shapes.push(text(
        Rect::new(
            label_right - label_w,
            mid - metrics.height / 2.0,
            label_w,
            metrics.height,
        ),
        label,
        label_font,
        Role::Sub,
        Align::Right,
        false,
    ));
    let ring_cx = label_right - label_w - metrics.ring_gap - metrics.ring / 2.0;
    // The ring is 14 wide with a radius of 5.5 and a line of 2; other sizes
    // keep the same proportions.
    let radius = metrics.ring * (5.5 / 14.0);
    let line = metrics.ring * (2.0 / 14.0);
    shapes.push(Shape::Ring {
        cx: ring_cx,
        cy: mid,
        radius,
        width: line,
        color: Role::Track,
        sweep: 1.0,
    });
    if fraction > 0.0 {
        shapes.push(Shape::Ring {
            cx: ring_cx,
            cy: mid,
            radius,
            width: line,
            color: Role::Sub,
            sweep: fraction.min(1.0),
        });
    }

    Title {
        shapes,
        hits,
        height: metrics.height,
    }
}

#[cfg(test)]
mod tests {
    use super::super::testing::*;
    use super::super::*;
    use crate::prefs::Layout;

    fn input(layout: Layout, hover: Option<Target>) -> Input<'static> {
        Input {
            layout,
            text_size: 13,
            body: Body::Loading,
            countdown: (92.0 / 120.0, 92),
            pinned: true,
            hover,
            max_window_height: 1000.0,
        }
    }

    fn hit(scene: &Scene, target: Target) -> Option<Rect> {
        scene
            .hits
            .iter()
            .find(|hit| hit.target == target)
            .map(|hit| hit.rect)
    }

    #[test]
    fn the_full_title_bar_has_three_buttons_of_the_specified_size() {
        let scene = build(&input(Layout::Columns, None), &FakeMeasure);
        let (pin, more, hide) = (
            hit(&scene, Target::Pin).unwrap(),
            hit(&scene, Target::More).unwrap(),
            hit(&scene, Target::Hide).unwrap(),
        );
        for rect in [pin, more, hide] {
            assert_eq!((rect.w, rect.h), (36.0, 32.0));
        }
        // 4 DIPs of padding inside the 1 DIP frame, 8 DIPs between buttons.
        assert_eq!(hide.right(), 800.0 - 1.0 - 4.0);
        assert_eq!(more.right(), hide.x - 8.0);
        assert_eq!(pin.right(), more.x - 8.0);
        // Centred in the 40 DIP bar.
        assert_eq!(hide.y, 1.0 + 4.0);
        assert!(hit(&scene, Target::Expand).is_none());
        // The ring and its label end 12 DIPs left of the first button: the
        // 8 DIP gap plus 4 DIPs of padding.
        let label = text_of(&scene.title, "1m32s");
        assert_eq!(label.rect.right(), pin.x - 12.0);
    }

    #[test]
    fn the_title_bar_names_the_app_and_counts_down() {
        let scene = build(&input(Layout::Columns, None), &FakeMeasure);
        assert!(texts(&scene.title).iter().any(|t| t.text == "Ullage"));
        let label = text_of(&scene.title, "1m32s");
        assert_eq!(label.font.kind, FontKind::Mono);
        assert_eq!(label.font.size, 12.0);
        assert_eq!(label.color, Role::Sub);
        let rings: Vec<_> = scene
            .title
            .iter()
            .filter_map(|shape| match shape {
                Shape::Ring {
                    color,
                    sweep,
                    radius,
                    width,
                    ..
                } => Some((*color, *sweep, *radius, *width)),
                _ => None,
            })
            .collect();
        assert_eq!(rings.len(), 2);
        assert_eq!(rings[0], (Role::Track, 1.0, 5.5, 2.0));
        assert_eq!(rings[1].0, Role::Sub);
        assert!((rings[1].1 - 92.0 / 120.0).abs() < 1e-6);
    }

    #[test]
    fn an_empty_countdown_draws_only_the_track_ring() {
        let mut input = input(Layout::Columns, None);
        input.countdown = (0.0, 0);
        let scene = build(&input, &FakeMeasure);
        let rings = scene
            .title
            .iter()
            .filter(|shape| matches!(shape, Shape::Ring { .. }))
            .count();
        assert_eq!(rings, 1);
        text_of(&scene.title, "0m00s");
    }

    #[test]
    fn the_pin_button_shows_its_state() {
        let pin_icon = |pinned: bool| {
            let mut input = input(Layout::Columns, None);
            input.pinned = pinned;
            let scene = build(&input, &FakeMeasure);
            scene
                .title
                .iter()
                .find_map(|shape| match shape {
                    Shape::Icon {
                        icon: Icon::Pin { filled },
                        color,
                        ..
                    } => Some((*filled, *color)),
                    _ => None,
                })
                .unwrap()
        };
        assert_eq!(pin_icon(true), (true, Role::Text));
        assert_eq!(pin_icon(false), (false, Role::Sub));
    }

    #[test]
    fn hovering_a_button_draws_its_hover_fill() {
        let hover_rects = |hover| {
            let scene = build(&input(Layout::Columns, hover), &FakeMeasure);
            scene
                .title
                .iter()
                .filter_map(|shape| match shape {
                    Shape::Rect {
                        rect,
                        fill: Some(Role::Hover),
                        radius,
                        ..
                    } => Some((*rect, *radius)),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        assert!(hover_rects(None).is_empty());
        let rects = hover_rects(Some(Target::More));
        assert_eq!(rects.len(), 1);
        assert_eq!(rects[0].1, 4.0);
    }

    #[test]
    fn the_compact_strip_has_one_expand_button() {
        let scene = build(&input(Layout::Compact, None), &FakeMeasure);
        let expand = hit(&scene, Target::Expand).unwrap();
        assert_eq!((expand.w, expand.h), (32.0, 30.0));
        assert_eq!(expand.right(), 290.0 - 1.0 - 2.0);
        assert!(hit(&scene, Target::Pin).is_none());
        assert!(hit(&scene, Target::Hide).is_none());
        assert_eq!(scene.content_top, 1.0 + 34.0);
        let label = text_of(&scene.title, "1m32s");
        assert_eq!(label.font.size, 11.0);
        let name = text_of(&scene.title, "Ullage");
        assert_eq!(name.font.size, 12.0);
    }

    /// Where each piece of the strip title bar is, left to right: the
    /// icon, the name, the ring, the countdown label and the button.
    fn strip_pieces(scene: &Scene) -> Vec<(&'static str, Rect)> {
        let icon = scene
            .title
            .iter()
            .find_map(|shape| match shape {
                Shape::Icon {
                    icon: Icon::Container { .. },
                    rect,
                    ..
                } => Some(*rect),
                _ => None,
            })
            .unwrap();
        let ring = scene
            .title
            .iter()
            .find_map(|shape| match shape {
                Shape::Ring {
                    cx,
                    cy,
                    radius,
                    width,
                    ..
                } => {
                    let reach = radius + width / 2.0;
                    Some(Rect::new(cx - reach, cy - reach, 2.0 * reach, 2.0 * reach))
                }
                _ => None,
            })
            .unwrap();
        let mut pieces = vec![
            ("icon", icon),
            ("name", text_of(&scene.title, "Ullage").rect),
            ("ring", ring),
            ("label", text_of(&scene.title, "2m00s").rect),
            ("button", hit(scene, Target::Expand).unwrap()),
        ];
        pieces.sort_by(|a, b| a.1.x.partial_cmp(&b.1.x).unwrap());
        pieces
    }

    #[test]
    fn the_narrowest_strip_holds_its_whole_title_bar_with_the_longest_countdown() {
        use super::super::cards::tests::{card, row};
        use ullage_cli::usage_view::Reading;
        let cards = vec![card(
            "grok",
            None,
            "account-1",
            vec![row(
                "weekly",
                Reading::Remaining("5%".into()),
                Some(0.05),
                None,
            )],
        )];
        let mut input = input(Layout::Compact, None);
        input.body = Body::Cards(&cards);
        input.countdown = (1.0, 120);
        let scene = build(&input, &FakeMeasure);
        // One short label: the strip is about as narrow as it gets.
        assert!(scene.width >= title::strip_min_width(&FakeMeasure));
        assert!(scene.width < 180.0);
        let pieces = strip_pieces(&scene);
        for (name, rect) in &pieces {
            assert!(
                rect.x >= BORDER
                    && rect.right() <= scene.width - BORDER
                    && rect.y >= BORDER
                    && rect.bottom() <= scene.content_top,
                "{name} leaves the window: {rect:?}"
            );
        }
        for pair in pieces.windows(2) {
            assert!(
                pair[0].1.right() <= pair[1].1.x,
                "{} overlaps {}",
                pair[0].0,
                pair[1].0
            );
        }
    }

    #[test]
    fn the_least_width_of_the_strip_title_bar_leaves_the_name_its_gap() {
        let mut input = input(Layout::Compact, None);
        input.countdown = (1.0, 120);
        let width = title::strip_min_width(&FakeMeasure);
        let bar = title::build(&input, width, true, &FakeMeasure);
        let scene = Scene {
            width,
            height: 0.0,
            content_top: 0.0,
            view_height: 0.0,
            content_height: 0.0,
            title: bar.shapes,
            content: Vec::new(),
            hits: bar.hits,
            card_rects: Vec::new(),
        };
        let pieces = strip_pieces(&scene);
        let gap = |a: &str, b: &str| {
            let find = |name: &str| pieces.iter().find(|piece| piece.0 == name).unwrap().1;
            find(b).x - find(a).right()
        };
        // The ring is 12 wide; its stroke is a little narrower than that.
        assert!((gap("name", "ring") - title::STRIP_NAME_GAP).abs() < 0.6);
        assert_eq!(pieces.last().unwrap().1.right(), width - BORDER - 2.0);
    }

    #[test]
    fn the_unavailable_title_bar_has_only_more_and_hide() {
        let failure =
            crate::state::failure_for(&ullage_cli::usage_view::FetchError::DaemonUnavailable);
        let mut input = input(Layout::Columns, None);
        input.body = Body::Unavailable(&failure);
        input.countdown = (0.0, 0);
        let scene = build(&input, &FakeMeasure);
        assert!(hit(&scene, Target::More).is_some());
        assert!(hit(&scene, Target::Hide).is_some());
        assert!(hit(&scene, Target::Pin).is_none());
        let liquid = scene.title.iter().any(|shape| {
            matches!(
                shape,
                Shape::Icon {
                    icon: Icon::Container { liquid: true },
                    ..
                }
            )
        });
        assert!(!liquid);
    }
}
