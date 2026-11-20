use chrono::{TimeZone, Utc};
use ullage_cli::usage_view::{Reading, Severity, UsageNotice, UsageRow, UsageTitle};

use super::super::testing::*;
use super::super::*;
use super::{TitleFit, fit_title, reading_text, reading_width};

pub fn row(name: &str, reading: Reading, ratio: Option<f64>, resets_in: Option<i64>) -> UsageRow {
    UsageRow {
        name: name.into(),
        reading,
        off: false,
        resets_in,
        remaining_ratio: ratio,
        severity: ratio.map(Severity::of),
    }
}

pub fn card(provider: &str, plan: Option<&str>, account: &str, rows: Vec<UsageRow>) -> UsageCard {
    UsageCard {
        title: UsageTitle {
            provider: provider.into(),
            plan: plan.map(Into::into),
            account: account.into(),
        },
        rows,
        notices: Vec::new(),
    }
}

/// `count` cards with two rows each.
pub fn sample_cards(count: usize) -> Vec<UsageCard> {
    (0..count)
        .map(|index| {
            card(
                "claude",
                Some("max_20x"),
                &format!("account-{index}"),
                vec![
                    row(
                        "5h",
                        Reading::Remaining("79%".into()),
                        Some(0.79),
                        Some(9_780),
                    ),
                    row(
                        "weekly",
                        Reading::Remaining("73%".into()),
                        Some(0.73),
                        Some(4 * 86_400),
                    ),
                ],
            )
        })
        .collect()
}

fn input<'a>(layout: Layout, cards: &'a [UsageCard]) -> Input<'a> {
    Input {
        layout,
        text_size: 13,
        body: Body::Cards(cards),
        countdown: (1.0, 120),
        pinned: true,
        hover: None,
        max_window_height: 5000.0,
    }
}

#[test]
fn two_columns_fill_a_grid_row_by_row_with_tops_aligned() {
    let cards = sample_cards(3);
    let scene = build(&input(Layout::Columns, &cards), &FakeMeasure);
    let rects = &scene.card_rects;
    assert_eq!(rects.len(), 3);
    // 800 - 2 frame - 20 padding, minus an 8 DIP gap, shared by two.
    assert_eq!(rects[0].w, 385.0);
    assert_eq!(rects[0].x, 10.0);
    assert_eq!(rects[1].x, 10.0 + 385.0 + 8.0);
    assert_eq!(rects[0].y, rects[1].y);
    assert_eq!(rects[2].x, 10.0);
    // The next grid row starts below the taller card of the first.
    assert_eq!(rects[2].y, rects[0].y + rects[0].h + 8.0);
}

#[test]
fn a_grid_row_is_as_tall_as_its_tallest_card() {
    let mut cards = sample_cards(3);
    cards[0].notices = vec![UsageNotice::Stale];
    let scene = build(&input(Layout::Columns, &cards), &FakeMeasure);
    let rects = &scene.card_rects;
    assert!(rects[0].h > rects[1].h);
    assert_eq!(rects[2].y, rects[0].y + rects[0].h + 8.0);
}

#[test]
fn the_single_layout_stacks_full_width_cards() {
    let cards = sample_cards(2);
    let scene = build(&input(Layout::Single, &cards), &FakeMeasure);
    let rects = &scene.card_rects;
    assert_eq!(rects[0].w, 400.0 - 2.0 - 20.0);
    assert_eq!(rects[1].y, rects[0].y + rects[0].h + 8.0);
    assert_eq!(rects[0].x, rects[1].x);
}

#[test]
fn a_card_is_as_tall_as_its_padding_title_rows_and_tags() {
    let cards = sample_cards(1);
    let scene = build(&input(Layout::Single, &cards), &FakeMeasure);
    // 2 border + 6 + 8 padding, a 28 title row, two 24 rows.
    assert_eq!(scene.card_rects[0].h, 2.0 + 6.0 + 8.0 + 28.0 + 48.0);
    let mut with_tag = sample_cards(1);
    with_tag[0].notices = vec![UsageNotice::Stale];
    let scene = build(&input(Layout::Single, &with_tag), &FakeMeasure);
    assert_eq!(
        scene.card_rects[0].h,
        2.0 + 6.0 + 8.0 + 28.0 + 48.0 + 6.0 + 20.0
    );
    // The content area adds 2 on top and 10 below.
    assert_eq!(scene.content_height, 2.0 + scene.card_rects[0].h + 10.0);
}

#[test]
fn row_height_and_text_scale_with_the_text_size() {
    let cards = sample_cards(1);
    let mut input = input(Layout::Single, &cards);
    input.text_size = 16;
    let scene = build(&input, &FakeMeasure);
    let scale = 16.0 / 13.0;
    let expected = 2.0 + 6.0 + 8.0 + 28.0 * scale + 2.0 * 24.0 * scale;
    assert!((scene.card_rects[0].h - expected).abs() < 1e-3);
    let provider = text_of(&scene.content, "claude");
    assert!((provider.font.size - 16.0).abs() < 1e-4);
    // The title bar does not scale.
    assert_eq!(scene.content_top, 41.0);
}

#[test]
fn every_card_shares_one_reading_column_that_grows_with_the_widest_reading() {
    let mut cards = sample_cards(2);
    cards[1].rows[0].reading = Reading::Credits("120 of 500".into());
    let scene = build(&input(Layout::Columns, &cards), &FakeMeasure);
    // The `73%` readings of both cards end at the same offset in their card.
    let ends: Vec<f32> = texts(&scene.content)
        .into_iter()
        .filter(|text| text.text == "73%")
        .map(|text| text.rect.right())
        .collect();
    assert_eq!(ends.len(), 2);
    let offsets: Vec<f32> = ends
        .iter()
        .zip(&scene.card_rects)
        .map(|(end, card)| end - card.x)
        .collect();
    assert_eq!(offsets[0], offsets[1]);
    // And the long reading ends there too.
    let long = text_of(&scene.content, "120 of 500");
    assert_eq!(long.rect.right() - scene.card_rects[1].x, offsets[1]);
}

#[test]
fn the_reading_column_is_at_least_72_and_then_as_wide_as_the_widest_reading() {
    let name_width = |reading: Reading| {
        let cards = vec![card(
            "claude",
            None,
            "a",
            vec![row("name", reading, None, None)],
        )];
        let scene = build(&input(Layout::Single, &cards), &FakeMeasure);
        text_of(&scene.content, "name").rect.w
    };
    // The name column is what the reading column leaves; the name is short
    // enough to fit both times, so look at the reading's left edge instead.
    let reading_left = |reading: Reading, value: &str| {
        let cards = vec![card(
            "claude",
            None,
            "a",
            vec![row("name", reading, None, None)],
        )];
        let scene = build(&input(Layout::Single, &cards), &FakeMeasure);
        let card = scene.card_rects[0];
        let end = text_of(&scene.content, value).rect.right();
        (end, card.x)
    };
    let _ = name_width;
    // A short reading: the column is 72 wide, ending 10 before the reset
    // column (72 wide) and the bar (68 wide).
    let (end, card_x) = reading_left(Reading::Remaining("9%".into()), "9%");
    let inner_right = card_x + 400.0 - 2.0 - 20.0 - 1.0 - 12.0;
    assert_eq!(end, inner_right - 68.0 - 10.0 - 72.0 - 10.0);
    // A long one moves nothing on the right, only the name column shrinks.
    let (end_long, _) = reading_left(Reading::Credits("120 of 500".into()), "120 of 500");
    assert_eq!(end_long, end);
    let value_font = font(FontKind::Mono, 13.0, 600);
    let prefix_font = font(FontKind::Mono, 12.0, 400);
    let short = reading_text(&Reading::Remaining("9%".into()), None);
    assert!(reading_width(&short, value_font, prefix_font, &FakeMeasure) < 72.0);
    let long = reading_text(&Reading::Credits("120 of 500".into()), None);
    assert!(reading_width(&long, value_font, prefix_font, &FakeMeasure) > 72.0);
}

#[test]
fn a_title_that_does_not_fit_loses_its_plan_first_then_the_account_then_the_provider() {
    // 3 + 10 + 3 + 10 + 3 = 29 DIPs of text and gaps.
    assert_eq!(
        fit_title(29.0, 3.0, Some(3.0), 3.0),
        TitleFit {
            provider: 3.0,
            plan: 3.0,
            account: 3.0
        }
    );
    // Two short: the plan gives up what is missing.
    let fit = fit_title(27.0, 3.0, Some(30.0), 3.0);
    assert_eq!((fit.provider, fit.account), (3.0, 3.0));
    assert_eq!(fit.plan, 27.0 - 3.0 - 3.0 - 20.0);
    // No room for a plan at all: it is dropped with its gap.
    let fit = fit_title(17.0, 3.0, Some(30.0), 4.0);
    assert_eq!((fit.plan, fit.provider, fit.account), (0.0, 3.0, 4.0));
    // Then the account.
    let fit = fit_title(20.0, 3.0, Some(30.0), 30.0);
    assert_eq!(fit.plan, 0.0);
    assert_eq!(fit.provider, 3.0);
    assert_eq!(fit.account, 20.0 - 3.0 - 10.0);
    // The provider last.
    let fit = fit_title(5.0, 30.0, Some(30.0), 30.0);
    assert_eq!((fit.plan, fit.account, fit.provider), (0.0, 0.0, 5.0));
    // No plan, no gap for it.
    let fit = fit_title(16.0, 3.0, None, 3.0);
    assert_eq!((fit.provider, fit.account), (3.0, 3.0));
}

fn drawn(scene: &Scene) -> Vec<(String, Role)> {
    texts(&scene.content)
        .iter()
        .map(|t| (t.text.clone(), t.color))
        .collect()
}

#[test]
fn a_card_title_is_provider_plan_and_account() {
    let cards = sample_cards(1);
    let scene = build(&input(Layout::Single, &cards), &FakeMeasure);
    let shown = drawn(&scene);
    assert!(shown.contains(&("claude".into(), Role::Accent)));
    assert!(shown.contains(&("max_20x".into(), Role::Sub)));
    assert!(shown.contains(&("account-0".into(), Role::Faint)));
    let provider = text_of(&scene.content, "claude");
    assert_eq!(provider.font.weight, 700);
    assert_eq!(provider.font.kind, FontKind::Mono);
}

#[test]
fn the_prefix_of_a_reading_is_a_faint_word_before_the_value() {
    let cards = vec![card(
        "x",
        None,
        "a",
        vec![
            row("api", Reading::Balance("$12.50".into()), None, None),
            row("pool", Reading::Credits("120 of 500".into()), None, None),
        ],
    )];
    let scene = build(&input(Layout::Single, &cards), &FakeMeasure);
    let balance = text_of(&scene.content, "balance");
    assert_eq!(balance.color, Role::Faint);
    let value = text_of(&scene.content, "$12.50");
    assert_eq!(value.color, Role::Text);
    assert!(balance.rect.right() < value.rect.x);
    text_of(&scene.content, "credits");
    text_of(&scene.content, "120 of 500");
}

#[test]
fn a_row_marked_off_shows_a_faint_off_after_its_name() {
    let mut cards = sample_cards(1);
    cards[0].rows[0].off = true;
    let scene = build(&input(Layout::Single, &cards), &FakeMeasure);
    let off = text_of(&scene.content, " off");
    assert_eq!(off.color, Role::Faint);
    let name = text_of(&scene.content, "5h");
    assert_eq!(off.rect.x, name.rect.right());
    assert_eq!(
        texts(&scene.content)
            .iter()
            .filter(|t| t.text == " off")
            .count(),
        1
    );
}

fn bar_cells(scene: &Scene) -> Vec<Role> {
    scene
        .content
        .iter()
        .filter_map(|shape| match shape {
            Shape::Rect {
                rect,
                fill: Some(role),
                radius,
                ..
            } if rect.w == 5.0 && rect.h == 8.0 && *radius == 1.5 => Some(*role),
            _ => None,
        })
        .collect()
}

#[test]
fn a_bar_fills_from_the_right_in_the_colour_of_the_reading() {
    let cards = vec![card(
        "x",
        None,
        "a",
        vec![row(
            "5h",
            Reading::Remaining("18%".into()),
            Some(0.18),
            None,
        )],
    )];
    let scene = build(&input(Layout::Single, &cards), &FakeMeasure);
    let cells = bar_cells(&scene);
    assert_eq!(cells.len(), 10);
    let expected: Vec<Role> = (0..10)
        .map(|i| if i >= 8 { Role::Low } else { Role::Track })
        .collect();
    assert_eq!(cells, expected);
}

#[test]
fn rows_without_a_ratio_draw_no_bar_and_unlimited_a_full_one() {
    let cards = vec![card(
        "x",
        None,
        "a",
        vec![
            row("api", Reading::Balance("$1".into()), None, None),
            row("w", Reading::CreditsUnlimited, None, None),
        ],
    )];
    let scene = build(&input(Layout::Single, &cards), &FakeMeasure);
    let all: Vec<Role> = scene
        .content
        .iter()
        .filter_map(|shape| match shape {
            Shape::Rect {
                rect,
                fill: Some(role),
                ..
            } if rect.w == 5.0 && rect.h == 8.0 => Some(*role),
            _ => None,
        })
        .collect();
    // Ten cells, all lit, for the second row only.
    assert_eq!(all, vec![Role::Ample; 10]);
    let unlimited = text_of(&scene.content, "unlimited");
    assert_eq!(unlimited.color, Role::Ample);
}

#[test]
fn a_reset_under_a_day_is_a_countdown() {
    let cards = sample_cards(1);
    let scene = build(&input(Layout::Single, &cards), &FakeMeasure);
    let label = text_of(&scene.content, "2h43m");
    assert_eq!(label.color, Role::Sub);
    assert_eq!(label.font.size, 12.0);
    assert_eq!(label.align, Align::Right);
}

#[test]
fn a_reset_of_one_to_seven_days_is_seven_dots_in_the_five_forms() {
    let mut cards = sample_cards(1);
    // Four days and ten hours is five days to wait: two dots counted off,
    // then a part day of more than six hours, then four whole days.
    cards[0].rows[1].resets_in = Some(4 * 86_400 + 10 * 3_600);
    let scene = build(&input(Layout::Single, &cards), &FakeMeasure);
    let dots: Vec<_> = scene
        .content
        .iter()
        .filter_map(|shape| match shape {
            Shape::Dot {
                color,
                fill,
                diameter,
                border,
                ..
            } => Some((*color, *fill, *diameter, *border)),
            _ => None,
        })
        .collect();
    assert_eq!(dots.len(), 7);
    assert_eq!(dots[0], (Role::Track, DotFill::Empty, 7.0, 1.25));
    assert_eq!(dots[1], (Role::Track, DotFill::Empty, 7.0, 1.25));
    assert_eq!(dots[2], (Role::Sub, DotFill::ThreeQuarters, 7.0, 1.25));
    assert_eq!(dots[3], (Role::Sub, DotFill::Full, 7.0, 1.25));
    assert_eq!(dots[6], (Role::Sub, DotFill::Full, 7.0, 1.25));
    // 7 DIP dots with 3 DIP gaps.
    let centres: Vec<f32> = scene
        .content
        .iter()
        .filter_map(|shape| match shape {
            Shape::Dot { cx, .. } => Some(*cx),
            _ => None,
        })
        .collect();
    assert_eq!(centres[1] - centres[0], 10.0);
}

#[test]
fn the_other_day_dot_forms() {
    for (seconds, expected) in [
        (86_400 + 3_600, (Role::Sub, DotFill::Empty)),
        (86_400 + 4 * 3_600, (Role::Sub, DotFill::Quarter)),
        (2 * 86_400, (Role::Sub, DotFill::Full)),
    ] {
        let mut cards = sample_cards(1);
        cards[0].rows[1].resets_in = Some(seconds);
        let scene = build(&input(Layout::Single, &cards), &FakeMeasure);
        let dots: Vec<_> = scene
            .content
            .iter()
            .filter_map(|shape| match shape {
                Shape::Dot { color, fill, .. } => Some((*color, *fill)),
                _ => None,
            })
            .collect();
        assert!(dots.contains(&expected), "{seconds}: {dots:?}");
    }
}

#[test]
fn a_reset_beyond_a_week_is_a_pill_with_the_number_of_days() {
    let pill_at = |text_size: u32| {
        let mut cards = sample_cards(1);
        cards[0].rows[1].resets_in = Some(22 * 86_400 + 3_600);
        let mut input = input(Layout::Single, &cards);
        input.text_size = text_size;
        let scene = build(&input, &FakeMeasure);
        let label = text_of(&scene.content, "23d").clone();
        let pill = scene
            .content
            .iter()
            .find_map(|shape| match shape {
                Shape::Rect {
                    rect,
                    fill: Some(Role::Pill),
                    radius,
                    ..
                } => Some((*rect, *radius)),
                _ => None,
            })
            .unwrap();
        (label, pill)
    };
    let (label, pill) = pill_at(13);
    assert_eq!(label.font.kind, FontKind::Mono);
    assert_eq!(label.font.size, 11.0);
    assert_eq!(label.color, Role::Sub);
    assert_eq!(pill.1, 9.0);
    assert_eq!(pill.0.h, 18.0);
    // 8 DIPs of padding each side of the text.
    assert_eq!(pill.0.w, FakeMeasure.width("23d", label.font).ceil() + 16.0);

    // At the largest text size the text and the pill both grow.
    let (big_label, big_pill) = pill_at(16);
    let scale = 16.0 / 13.0;
    assert_eq!(big_label.font.kind, FontKind::Mono);
    assert!((big_label.font.size - 11.0 * scale).abs() < 1e-4);
    assert!((big_pill.0.h - 18.0 * scale).abs() < 1e-4);
    assert!((big_pill.1 - 9.0 * scale).abs() < 1e-4);
    assert_eq!(
        big_pill.0.w,
        FakeMeasure.width("23d", big_label.font).ceil() + 16.0
    );
}

#[test]
fn a_row_with_no_reset_draws_nothing_in_the_reset_column() {
    let cards = vec![card(
        "x",
        None,
        "a",
        vec![row(
            "5h",
            Reading::Remaining("100%".into()),
            Some(1.0),
            None,
        )],
    )];
    let scene = build(&input(Layout::Single, &cards), &FakeMeasure);
    assert!(
        !scene
            .content
            .iter()
            .any(|shape| matches!(shape, Shape::Dot { .. }))
    );
    assert_eq!(texts(&scene.content).len(), 3 + 1); // title parts and name, value
}

#[test]
fn the_four_notices_are_tags_under_the_rows() {
    let mut cards = sample_cards(1);
    cards[0].notices = vec![
        UsageNotice::Stale,
        UsageNotice::LimitReached,
        UsageNotice::Unavailable(3),
        UsageNotice::NoMetrics,
    ];
    let scene = build(&input(Layout::Single, &cards), &FakeMeasure);
    for label in [
        "stale snapshot",
        "limit reached",
        "3 item(s) unavailable",
        "no summarized metrics",
    ] {
        let shown = text_of(&scene.content, label);
        assert_eq!(shown.color, Role::WarnText);
        assert_eq!(shown.font.size, 12.0);
    }
    let tags: Vec<Rect> = scene
        .content
        .iter()
        .filter_map(|shape| match shape {
            Shape::Rect {
                rect,
                fill: Some(Role::WarnBg),
                radius,
                ..
            } if *radius == 4.0 => Some(*rect),
            _ => None,
        })
        .collect();
    assert_eq!(tags.len(), 4);
    assert!(tags.iter().all(|tag| tag.h == 20.0));
    // They flow left to right, and wrap in a narrow card.
    assert_eq!(tags[0].y, tags[1].y);
    assert!(tags[3].y > tags[0].y);
    let warnings = scene
        .content
        .iter()
        .filter(|shape| {
            matches!(
                shape,
                Shape::Icon {
                    icon: Icon::Warning,
                    ..
                }
            )
        })
        .count();
    assert_eq!(warnings, 4);
}

#[test]
fn the_empty_state_is_one_card_with_a_faint_line() {
    let scene = build(
        &Input {
            body: Body::Empty,
            ..input(Layout::Columns, &[])
        },
        &FakeMeasure,
    );
    let line = text_of(&scene.content, "No subscription snapshots");
    assert_eq!(line.color, Role::Faint);
    assert_eq!(line.font.kind, FontKind::Ui);
    assert_eq!(line.font.size, 13.0);
    assert_eq!(line.align, Align::Center);
    assert_eq!(scene.card_rects.len(), 1);
    assert_eq!(scene.card_rects[0].w, 800.0 - 2.0 - 20.0);
}

#[test]
fn a_card_without_rows_still_has_a_title() {
    let mut cards = vec![card("x", None, "a", Vec::new())];
    cards[0].notices = vec![UsageNotice::NoMetrics];
    let scene = build(&input(Layout::Single, &cards), &FakeMeasure);
    text_of(&scene.content, "no summarized metrics");
    let _ = Utc.timestamp_opt(0, 0);
}
