//! What each account shows on the compact strip.

use ullage_cli::usage_view::{UsageCard, UsageRow};

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
        UsageCard {
            title: UsageTitle {
                provider: "claude".into(),
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
}
