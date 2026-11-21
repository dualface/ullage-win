//! The width of the compact strip: what it follows and what it does not.

use ullage_cli::usage_view::{Reading, UsageCard};

use super::super::cards::tests::{card, row};
use super::super::testing::*;
use super::super::*;
use super::tests::{assert_lines_fit, input};

/// One account with one weekly row.
fn account(provider: &str, reading: Reading, resets_in: Option<i64>) -> UsageCard {
    card(
        provider,
        None,
        "account-1",
        vec![row("weekly", reading, Some(0.5), resets_in)],
    )
}

fn remaining(text: &str) -> Reading {
    Reading::Remaining(text.into())
}

/// The four accounts of the screenshot: `chatgpt`, `claude-1`, `claude-2`
/// and `grok`.
fn four(readings: [Reading; 4], resets: [Option<i64>; 4]) -> Vec<UsageCard> {
    let providers = ["chatgpt", "claude", "claude", "grok"];
    providers
        .iter()
        .zip(readings)
        .zip(resets)
        .map(|((provider, reading), reset)| account(provider, reading, reset))
        .collect()
}

fn four_remaining() -> Vec<UsageCard> {
    four(
        [
            remaining("72%"),
            remaining("48%"),
            remaining("0%"),
            remaining("96%"),
        ],
        [
            Some(5 * 86_400),
            Some(5 * 86_400),
            Some(7 * 3600 + 48 * 60),
            Some(6 * 86_400),
        ],
    )
}

fn width_of(cards: &[UsageCard]) -> f32 {
    build(&input(cards), &FakeMeasure).width
}

/// The width of a box that holds `chars` characters of `size` DIPs, as
/// `text_box_width` makes it with the fake measure (0.6 of the size each).
fn box_of(chars: usize, size: f32) -> f32 {
    (chars as f32 * size * 0.6).ceil() + 1.0
}

#[test]
fn four_accounts_with_percentages_are_narrower_than_the_old_strip() {
    let cards = four_remaining();
    let scene = build(&input(&cards), &FakeMeasure);
    assert_eq!(scene.width, 229.0);
    assert!(scene.width < 250.0);
    assert_lines_fit(&scene);
}

#[test]
fn a_used_reading_keeps_its_prefix_and_widens_the_strip() {
    let mut readings = [
        remaining("72%"),
        remaining("48%"),
        remaining("0%"),
        remaining("96%"),
    ];
    let base = width_of(&four(readings.clone(), [None; 4]));
    readings[1] = Reading::Used("12%".into());
    let cards = four(readings, [None; 4]);
    let scene = build(&input(&cards), &FakeMeasure);
    assert!(scene.width > base && scene.width < 270.0, "{}", scene.width);
    let used = text_of(&scene.content, "used");
    assert!(!used.ellipsis);
    assert!(!text_of(&scene.content, "12%").ellipsis);
    assert_lines_fit(&scene);
}

/// Two accounts: a short label with a percentage, and the widest label with
/// `reading`, or with no rows at all.
fn with_widest_label(reading: Option<Reading>) -> Scene {
    let widest = match reading {
        Some(reading) => account("openrouter", reading, None),
        None => card("openrouter", None, "account-2", Vec::new()),
    };
    let cards = vec![account("grok", remaining("50%"), None), widest];
    let scene = build(&input(&cards), &FakeMeasure);
    // The label is not cut back.
    assert_eq!(
        text_of(&scene.content, "openrouter").rect.w,
        box_of(10, 12.0)
    );
    assert_lines_fit(&scene);
    scene
}

#[test]
fn unlimited_is_not_cut_on_the_widest_label() {
    let scene = with_widest_label(Some(Reading::CreditsUnlimited));
    let value = text_of(&scene.content, "unlimited");
    assert_eq!((value.rect.w, value.ellipsis), (box_of(9, 12.0), false));
}

#[test]
fn disabled_is_not_cut_on_the_widest_label() {
    let scene = with_widest_label(Some(Reading::Disabled));
    let value = text_of(&scene.content, "disabled");
    assert_eq!((value.rect.w, value.ellipsis), (box_of(8, 12.0), false));
}

#[test]
fn no_data_is_not_cut_on_the_widest_label() {
    let scene = with_widest_label(None);
    let value = text_of(&scene.content, "no data");
    assert_eq!((value.rect.w, value.ellipsis), (box_of(7, 12.0), false));
}

#[test]
fn a_very_long_label_stops_at_the_widest_the_strip_gets() {
    let long = "a-provider-name-that-is-forty-chars-long";
    assert_eq!(long.len(), 40);
    let cards = vec![account(long, remaining("50%"), Some(3600))];
    let scene = build(&input(&cards), &FakeMeasure);
    assert_eq!(scene.width, 290.0);
    let label = text_of(&scene.content, long);
    assert!(label.ellipsis && label.rect.w < box_of(40, 12.0));
    assert_lines_fit(&scene);
}

#[test]
fn the_width_does_not_follow_the_numbers_or_the_resets() {
    let plain = |value: &str| {
        [
            remaining(value),
            remaining(value),
            remaining(value),
            remaining(value),
        ]
    };
    let expected = width_of(&four_remaining());
    let variants = [
        (plain("0%"), [Some(0), Some(0), Some(0), Some(0)]),
        (
            plain("7%"),
            [Some(36 * 3600), Some(2 * 86_400), None, Some(59)],
        ),
        (plain("100%"), [None; 4]),
    ];
    for (readings, resets) in variants {
        assert_eq!(width_of(&four(readings, resets)), expected);
    }

    let used = |value: &str| {
        [
            remaining("72%"),
            Reading::Used(value.into()),
            remaining("0%"),
            remaining("96%"),
        ]
    };
    let expected = width_of(&four(used("3%"), [None; 4]));
    for value in ["100%", "45%"] {
        for resets in [[None; 4], [Some(1); 4], [Some(36 * 3600); 4]] {
            assert_eq!(width_of(&four(used(value), resets)), expected);
        }
    }
}

#[test]
fn a_reading_of_another_kind_changes_the_width_and_back() {
    let base = width_of(&four_remaining());
    let mut readings = [
        remaining("72%"),
        remaining("48%"),
        remaining("0%"),
        remaining("96%"),
    ];
    readings[3] = Reading::CreditsUnlimited;
    let wide = width_of(&four(readings.clone(), [None; 4]));
    assert!(wide > base);
    readings[3] = remaining("96%");
    assert_eq!(width_of(&four(readings, [None; 4])), base);
}

#[test]
fn a_longer_label_widens_the_strip_until_the_cap_and_removing_it_restores_the_width() {
    let mut cards = four_remaining();
    let base = width_of(&cards);

    cards.push(account("openrouter", remaining("50%"), None));
    let longer = width_of(&cards);
    assert!(longer > base);

    cards.push(account(
        "a-provider-name-that-is-forty-chars-long",
        remaining("50%"),
        None,
    ));
    assert_eq!(width_of(&cards), 290.0);

    cards.pop();
    assert_eq!(width_of(&cards), longer);
    cards.pop();
    assert_eq!(width_of(&cards), base);
}
