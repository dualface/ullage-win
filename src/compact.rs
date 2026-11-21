//! What each account shows on the compact strip.

use ullage_cli::usage_view::{UsageCard, UsageRow};

const SECONDS_PER_MINUTE: i64 = 60;
const SECONDS_PER_HOUR: i64 = 60 * SECONDS_PER_MINUTE;
const SECONDS_PER_DAY: i64 = 24 * SECONDS_PER_HOUR;
/// Up to a day and a half the strip spells out hours and minutes; above it,
/// days.
const DAYS_FROM: i64 = SECONDS_PER_DAY + SECONDS_PER_DAY / 2;

/// The row the strip shows for a card: the one with the least left, the
/// earlier one on a tie, and the first row when no row has a ratio.
/// `None` for a card without rows.
pub fn lowest_row(card: &UsageCard) -> Option<&UsageRow> {
    let mut lowest: Option<(&UsageRow, f64)> = None;
    for row in &card.rows {
        let Some(ratio) = row.remaining_ratio else {
            continue;
        };
        if lowest.is_none_or(|(_, best)| ratio < best) {
            lowest = Some((row, ratio));
        }
    }
    lowest.map(|(row, _)| row).or_else(|| card.rows.first())
}

/// The label of each account on the strip, in the order of `cards`: the
/// provider alone when it has one account, otherwise `provider-1`,
/// `provider-2`, ... in the order the accounts come.
pub fn labels(cards: &[UsageCard]) -> Vec<String> {
    let total = |provider: &str| {
        cards
            .iter()
            .filter(|card| card.title.provider == provider)
            .count()
    };
    cards
        .iter()
        .enumerate()
        .map(|(index, card)| {
            let provider = &card.title.provider;
            if total(provider) == 1 {
                return provider.clone();
            }
            let number = cards[..=index]
                .iter()
                .filter(|earlier| earlier.title.provider == *provider)
                .count();
            format!("{provider}-{number}")
        })
        .collect()
}

/// How long until a reset, as the strip shows it: whole days rounded to the
/// nearest (`2d`) above a day and a half, otherwise hours and two-digit
/// minutes (`5h03m`, `36h00m`). A negative wait counts as none.
pub fn reset_text(seconds: i64) -> String {
    let seconds = seconds.max(0);
    if seconds > DAYS_FROM {
        // Rounded without adding half a day first, so a wait near
        // `i64::MAX` cannot overflow.
        let days =
            seconds / SECONDS_PER_DAY + i64::from(seconds % SECONDS_PER_DAY >= SECONDS_PER_DAY / 2);
        return format!("{days}d");
    }
    let hours = seconds / SECONDS_PER_HOUR;
    let minutes = seconds % SECONDS_PER_HOUR / SECONDS_PER_MINUTE;
    format!("{hours}h{minutes:02}m")
}

#[cfg(test)]
mod tests {
    use ullage_cli::usage_view::{Reading, UsageTitle};

    use super::*;

    fn row(name: &str, ratio: Option<f64>) -> UsageRow {
        UsageRow {
            name: name.into(),
            reading: Reading::Remaining("x".into()),
            off: false,
            resets_in: None,
            remaining_ratio: ratio,
            severity: None,
        }
    }

    fn card(rows: Vec<UsageRow>) -> UsageCard {
        card_of("claude", rows)
    }

    fn card_of(provider: &str, rows: Vec<UsageRow>) -> UsageCard {
        UsageCard {
            title: UsageTitle {
                provider: provider.into(),
                plan: None,
                account: "a".into(),
            },
            rows,
            notices: Vec::new(),
        }
    }

    #[test]
    fn the_row_with_the_least_left_wins() {
        let card = card(vec![
            row("5h", Some(0.79)),
            row("weekly", Some(0.18)),
            row("fable", Some(0.5)),
        ]);
        assert_eq!(lowest_row(&card).unwrap().name, "weekly");
    }

    #[test]
    fn a_tie_goes_to_the_earlier_row() {
        let card = card(vec![
            row("a", Some(0.5)),
            row("b", Some(0.2)),
            row("c", Some(0.2)),
        ]);
        assert_eq!(lowest_row(&card).unwrap().name, "b");
    }

    #[test]
    fn rows_without_a_ratio_lose_to_rows_with_one() {
        let card = card(vec![row("a", None), row("b", Some(0.9))]);
        assert_eq!(lowest_row(&card).unwrap().name, "b");
    }

    #[test]
    fn with_no_ratio_anywhere_the_first_row_wins() {
        let card = card(vec![row("a", None), row("b", None)]);
        assert_eq!(lowest_row(&card).unwrap().name, "a");
    }

    #[test]
    fn a_zero_ratio_is_a_ratio() {
        let card = card(vec![row("a", Some(0.3)), row("b", Some(0.0))]);
        assert_eq!(lowest_row(&card).unwrap().name, "b");
    }

    #[test]
    fn a_card_without_rows_has_no_row() {
        assert!(lowest_row(&card(Vec::new())).is_none());
    }

    #[test]
    fn a_provider_with_one_account_is_its_own_label() {
        let cards = vec![card_of("claude", Vec::new()), card_of("codex", Vec::new())];
        assert_eq!(labels(&cards), ["claude", "codex"]);
    }

    #[test]
    fn accounts_of_one_provider_are_numbered_in_order() {
        let cards = vec![
            card_of("claude", Vec::new()),
            card_of("codex", Vec::new()),
            card_of("claude", Vec::new()),
            card_of("claude", Vec::new()),
        ];
        assert_eq!(
            labels(&cards),
            ["claude-1", "codex", "claude-2", "claude-3"]
        );
    }

    #[test]
    fn up_to_a_day_and_a_half_the_wait_is_hours_and_minutes() {
        assert_eq!(reset_text(0), "0h00m");
        assert_eq!(reset_text(720), "0h12m");
        assert_eq!(reset_text(5 * 3600 + 3 * 60 + 59), "5h03m");
        assert_eq!(reset_text(35 * 3600 + 59 * 60), "35h59m");
        assert_eq!(reset_text(36 * 3600), "36h00m");
    }

    #[test]
    fn above_a_day_and_a_half_the_wait_is_days_rounded_to_the_nearest() {
        assert_eq!(reset_text(36 * 3600 + 1), "2d");
        assert_eq!(reset_text(36 * 3600 + 60), "2d");
        assert_eq!(reset_text(2 * 86_400 + 11 * 3600), "2d");
        assert_eq!(reset_text(2 * 86_400 + 12 * 3600), "3d");
        assert_eq!(reset_text(i64::MAX), "106751991167301d");
    }

    #[test]
    fn a_negative_wait_is_no_wait() {
        assert_eq!(reset_text(-30), "0h00m");
    }
}
