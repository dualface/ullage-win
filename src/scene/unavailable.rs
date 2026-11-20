//! The state for a daemon that does not answer.

use super::{
    Align, BORDER, CONTENT_PAD_BOTTOM, CONTENT_PAD_SIDE, CONTENT_PAD_TOP, Content, FontKind, Hit,
    Icon, Measure, Rect, Shape, Target, font, text, text_box_width, wrap,
};
use crate::state::Failure;
use crate::theme::Role;

const PAD_TOP: f32 = 24.0;
const PAD_SIDE: f32 = 24.0;
const PAD_BOTTOM: f32 = 20.0;
const GAP: f32 = 14.0;
const BADGE: f32 = 40.0;
const TITLE_LINE: f32 = 20.0;
const BODY_LINE: f32 = 19.0;
const TEXT_GAP: f32 = 6.0;
const COMMAND_HEIGHT: f32 = 36.0;
const COPY_BUTTON: (f32, f32) = (32.0, 28.0);
const BUTTON_HEIGHT: f32 = 32.0;
const BUTTON_PAD: f32 = 16.0;
const BUTTON_GAP: f32 = 8.0;

pub(super) fn build(
    body: &mut Content,
    failure: &Failure,
    inner_width: f32,
    hover: Option<Target>,
    measure: &dyn Measure,
) {
    let card_x = CONTENT_PAD_SIDE;
    let card_w = inner_width - 2.0 * CONTENT_PAD_SIDE;
    let ix = card_x + BORDER + PAD_SIDE;
    let iw = card_w - 2.0 * (BORDER + PAD_SIDE);
    let frame_index = body.shapes.len();
    body.shapes.push(Shape::Rect {
        rect: Rect::default(),
        radius: 6.0,
        fill: None,
        border: None,
    });

    let mut y = CONTENT_PAD_TOP + BORDER + PAD_TOP;
    let badge = Rect::new(ix, y, BADGE, BADGE);
    body.shapes.push(Shape::Rect {
        rect: badge,
        radius: BADGE / 2.0,
        fill: Some(Role::WarnBg),
        border: None,
    });
    body.shapes.push(Shape::Icon {
        icon: Icon::PlugOff,
        rect: Rect::new(ix + 10.0, y + 10.0, 20.0, 20.0),
        color: Role::WarnText,
    });
    y += BADGE + GAP;

    let title_font = font(FontKind::Ui, 15.0, 600);
    for line in wrap(&failure.title, iw, title_font, measure) {
        body.shapes.push(text(
            Rect::new(ix, y, iw, TITLE_LINE),
            line,
            title_font,
            Role::Text,
            Align::Left,
            false,
        ));
        y += TITLE_LINE;
    }
    y += TEXT_GAP;
    let body_font = font(FontKind::Ui, 13.0, 400);
    for line in wrap(&failure.body, iw, body_font, measure) {
        body.shapes.push(text(
            Rect::new(ix, y, iw, BODY_LINE),
            line,
            body_font,
            Role::Sub,
            Align::Left,
            false,
        ));
        y += BODY_LINE;
    }
    y += GAP;

    // The command and the button that copies it.
    let command = Rect::new(ix, y, iw, COMMAND_HEIGHT);
    body.shapes.push(Shape::Rect {
        rect: command,
        radius: 4.0,
        fill: Some(Role::CommandBg),
        border: Some((Role::CardBorder, 1.0)),
    });
    let copy = Rect::new(
        command.right() - 4.0 - COPY_BUTTON.0,
        y + (COMMAND_HEIGHT - COPY_BUTTON.1) / 2.0,
        COPY_BUTTON.0,
        COPY_BUTTON.1,
    );
    body.shapes.push(text(
        Rect::new(
            command.x + 12.0,
            y,
            (copy.x - 8.0 - command.x - 12.0).max(0.0),
            COMMAND_HEIGHT,
        ),
        failure.command,
        font(FontKind::Mono, 12.0, 400),
        Role::Text,
        Align::Left,
        true,
    ));
    hover_fill(body, copy, Target::Copy, hover);
    body.shapes.push(Shape::Icon {
        icon: Icon::Copy,
        rect: Rect::new(copy.x + 9.0, copy.y + 7.0, 14.0, 14.0),
        color: Role::Sub,
    });
    body.hits.push(Hit {
        rect: copy,
        target: Target::Copy,
        scrolls: true,
    });
    y += COMMAND_HEIGHT + GAP;

    // Buttons.
    let primary_font = font(FontKind::Ui, 13.0, 600);
    let secondary_font = font(FontKind::Ui, 13.0, 400);
    let retry_w = text_box_width(measure, "Retry now", primary_font) + 2.0 * BUTTON_PAD;
    let retry = Rect::new(ix, y, retry_w, BUTTON_HEIGHT);
    body.shapes.push(Shape::Rect {
        rect: retry,
        radius: 4.0,
        fill: Some(Role::PrimaryBg),
        border: None,
    });
    body.shapes.push(text(
        retry,
        "Retry now",
        primary_font,
        Role::PrimaryText,
        Align::Center,
        false,
    ));
    hover_fill(body, retry, Target::Retry, hover);
    body.hits.push(Hit {
        rect: retry,
        target: Target::Retry,
        scrolls: true,
    });

    let guide_w = text_box_width(measure, "Setup guide", secondary_font) + 2.0 * BUTTON_PAD;
    let guide = Rect::new(retry.right() + BUTTON_GAP, y, guide_w, BUTTON_HEIGHT);
    body.shapes.push(Shape::Rect {
        rect: guide,
        radius: 4.0,
        fill: Some(Role::Card),
        border: Some((Role::ButtonBorder, 1.0)),
    });
    body.shapes.push(text(
        guide,
        "Setup guide",
        secondary_font,
        Role::Text,
        Align::Center,
        false,
    ));
    hover_fill(body, guide, Target::Guide, hover);
    body.hits.push(Hit {
        rect: guide,
        target: Target::Guide,
        scrolls: true,
    });
    y += BUTTON_HEIGHT + PAD_BOTTOM;

    let frame = Rect::new(
        card_x,
        CONTENT_PAD_TOP,
        card_w,
        y + BORDER - CONTENT_PAD_TOP,
    );
    body.shapes[frame_index] = Shape::Rect {
        rect: frame,
        radius: 6.0,
        fill: Some(Role::Card),
        border: Some((Role::CardBorder, 1.0)),
    };
    body.card_rects.push(frame);
    body.height = frame.bottom() + CONTENT_PAD_BOTTOM;
}

fn hover_fill(body: &mut Content, rect: Rect, target: Target, hover: Option<Target>) {
    if hover == Some(target) {
        body.shapes.push(Shape::Rect {
            rect,
            radius: 4.0,
            fill: Some(Role::Hover),
            border: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use ullage_cli::usage_view::FetchError;

    use super::super::testing::*;
    use super::super::*;
    use crate::prefs::Layout;
    use crate::state::failure_for;

    fn scene(error: &FetchError, hover: Option<Target>) -> Scene {
        let failure = failure_for(error);
        build(
            &Input {
                layout: Layout::Single,
                text_size: 13,
                body: Body::Unavailable(&failure),
                countdown: (0.0, 0),
                pinned: true,
                hover,
                max_window_height: 5000.0,
            },
            &FakeMeasure,
        )
    }

    #[test]
    fn the_state_shows_title_explanation_command_and_two_buttons() {
        let scene = scene(&FetchError::DaemonUnavailable, None);
        let title = text_of(&scene.content, "Can't reach the ullage daemon");
        assert_eq!((title.font.size, title.font.weight), (15.0, 600));
        let command = text_of(&scene.content, "ullage daemon install");
        assert_eq!(command.font.kind, FontKind::Mono);
        assert_eq!(command.font.size, 12.0);
        let retry = text_of(&scene.content, "Retry now");
        assert_eq!((retry.color, retry.font.weight), (Role::PrimaryText, 600));
        text_of(&scene.content, "Setup guide");
        for target in [Target::Copy, Target::Retry, Target::Guide] {
            assert!(
                scene
                    .hits
                    .iter()
                    .any(|hit| hit.target == target && hit.scrolls)
            );
        }
    }

    #[test]
    fn the_explanation_wraps_at_the_card_width_in_19_dip_lines() {
        let scene = scene(&FetchError::DaemonUnavailable, None);
        let lines: Vec<&Text> = texts(&scene.content)
            .into_iter()
            .filter(|t| t.color == Role::Sub && t.font.size == 13.0 && t.font.kind == FontKind::Ui)
            .collect();
        assert!(lines.len() >= 2);
        assert_eq!(lines[1].rect.y - lines[0].rect.y, 19.0);
        let joined: Vec<&str> = lines.iter().map(|line| line.text.as_str()).collect();
        assert_eq!(
            joined.join(" "),
            "Install it once from a terminal. This window tries again every 2 minutes and shows your usage as soon as it answers."
        );
    }

    #[test]
    fn the_command_follows_the_failure_kind() {
        let scene = scene(&FetchError::InvalidEndpoint, None);
        text_of(&scene.content, "ullage daemon status");
    }

    #[test]
    fn the_card_is_padded_24_and_the_buttons_sit_side_by_side() {
        let scene = scene(&FetchError::DaemonUnavailable, None);
        let card = scene.card_rects[0];
        assert_eq!(card.x, 10.0);
        assert_eq!(card.w, 400.0 - 2.0 - 20.0);
        let rect_of = |target| {
            scene
                .hits
                .iter()
                .find(|hit| hit.target == target)
                .unwrap()
                .rect
        };
        let (retry, guide, copy) = (
            rect_of(Target::Retry),
            rect_of(Target::Guide),
            rect_of(Target::Copy),
        );
        assert_eq!(retry.x, card.x + 1.0 + 24.0);
        assert_eq!(guide.x, retry.right() + 8.0);
        assert_eq!((retry.h, guide.h), (32.0, 32.0));
        assert_eq!((copy.w, copy.h), (32.0, 28.0));
        // 20 DIPs below the buttons, then the card border.
        assert_eq!(card.bottom(), retry.bottom() + 20.0 + 1.0);
        assert_eq!(scene.content_height, card.bottom() + 10.0);
    }

    #[test]
    fn hovering_a_button_draws_its_hover_fill() {
        let fills = |hover| {
            scene(&FetchError::DaemonUnavailable, hover)
                .content
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
        };
        assert_eq!(fills(None), 0);
        assert_eq!(fills(Some(Target::Retry)), 1);
    }
}
