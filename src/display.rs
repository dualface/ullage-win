//! How a card row reads on the floating window: which words, which colour,
//! and how many bar cells. The card model says what a row means; the rules
//! here are the window's own.

use ullage_cli::usage_view::{Reading, Severity, UsageRow};

use crate::theme::Role;

/// Cells in the progress bar of a card row.
pub const BAR_CELLS: u8 = 10;

/// A reading split into the faint prefix and the coloured value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadingText {
    pub prefix: Option<&'static str>,
    pub value: String,
    pub color: Role,
}

/// A progress bar: how many of its cells are filled, from the right, and in
/// which colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bar {
    pub filled: u8,
    pub color: Role,
}

pub fn severity_role(severity: Severity) -> Role {
    match severity {
        Severity::Ample => Role::Ample,
        Severity::Low => Role::Low,
        Severity::Critical => Role::Crit,
    }
}

/// The words and the colour of a row's reading.
pub fn reading_text(reading: &Reading, severity: Option<Severity>) -> ReadingText {
    let (prefix, value) = match reading {
        Reading::Remaining(text) => (None, text.clone()),
        Reading::UsedUp => (None, "0%".to_owned()),
        Reading::Used(text) | Reading::Counted(text) => (Some("used"), text.clone()),
        Reading::Balance(text) => (Some("balance"), text.clone()),
        Reading::Spent(text) => (None, text.clone()),
        Reading::Credits(text) => (Some("credits"), text.clone()),
        Reading::CreditsUnlimited => (None, "unlimited".to_owned()),
        Reading::Disabled => (None, "disabled".to_owned()),
    };
    let color = match (reading, severity) {
        (Reading::UsedUp, _) => Role::Crit,
        (Reading::CreditsUnlimited, _) => Role::Ample,
        (Reading::Disabled, _) => Role::Faint,
        (_, Some(severity)) => severity_role(severity),
        (_, None) => Role::Text,
    };
    ReadingText {
        prefix,
        value,
        color,
    }
}

/// The bar of a row, or `None` when the row has none.
/// The bar of `row` drawn with `cells` cells.
pub fn bar(row: &UsageRow, cells: u8) -> Option<Bar> {
    match row.reading {
        Reading::CreditsUnlimited => {
            return Some(Bar {
                filled: cells,
                color: Role::Ample,
            });
        }
        Reading::UsedUp => {
            return Some(Bar {
                filled: 0,
                color: Role::Crit,
            });
        }
        Reading::Balance(_) | Reading::Spent(_) | Reading::Disabled => return None,
        _ => {}
    }
    let ratio = row.remaining_ratio?;
    let filled = (ratio.clamp(0.0, 1.0) * f64::from(cells)).round() as u8;
    Some(Bar {
        filled,
        color: row.severity.map_or(Role::Text, severity_role),
    })
}

/// The time to the next fetch as `1m32s`: minutes without padding, seconds
/// padded to two digits.
pub fn countdown_label(seconds: u64) -> String {
    format!("{}m{:02}s", seconds / 60, seconds % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(reading: Reading, ratio: Option<f64>, severity: Option<Severity>) -> UsageRow {
        UsageRow {
            name: "5h".into(),
            reading,
            off: false,
            resets_in: None,
            remaining_ratio: ratio,
            severity,
        }
    }

    #[test]
    fn remaining_reads_as_the_bare_number_in_the_severity_colour() {
        let text = reading_text(&Reading::Remaining("83%".into()), Some(Severity::Ample));
        assert_eq!(text.prefix, None);
        assert_eq!(text.value, "83%");
        assert_eq!(text.color, Role::Ample);
        let text = reading_text(&Reading::Remaining("<1%".into()), Some(Severity::Critical));
        assert_eq!((text.value.as_str(), text.color), ("<1%", Role::Crit));
    }

    #[test]
    fn used_up_reads_as_zero_percent_in_the_critical_colour() {
        let text = reading_text(&Reading::UsedUp, Some(Severity::Critical));
        assert_eq!(text.prefix, None);
        assert_eq!(text.value, "0%");
        assert_eq!(text.color, Role::Crit);
        // Even a row that carries no severity is critical.
        assert_eq!(reading_text(&Reading::UsedUp, None).color, Role::Crit);
    }

    #[test]
    fn other_readings_keep_a_faint_prefix() {
        let cases = [
            (Reading::Used("34%".into()), "used", "34%"),
            (Reading::Counted("12 of 50".into()), "used", "12 of 50"),
            (Reading::Balance("$12.50".into()), "balance", "$12.50"),
            (
                Reading::Credits("120 of 500".into()),
                "credits",
                "120 of 500",
            ),
        ];
        for (reading, prefix, value) in cases {
            let text = reading_text(&reading, None);
            assert_eq!(text.prefix, Some(prefix));
            assert_eq!(text.value, value);
            assert_eq!(text.color, Role::Text);
        }
        let spent = reading_text(&Reading::Spent("$3.20".into()), None);
        assert_eq!((spent.prefix, spent.value.as_str()), (None, "$3.20"));
    }

    #[test]
    fn unlimited_and_disabled_have_their_own_colours() {
        let unlimited = reading_text(&Reading::CreditsUnlimited, None);
        assert_eq!(unlimited.value, "unlimited");
        assert_eq!(unlimited.color, Role::Ample);
        let disabled = reading_text(&Reading::Disabled, None);
        assert_eq!(disabled.value, "disabled");
        assert_eq!(disabled.color, Role::Faint);
    }

    #[test]
    fn severity_colours_follow_the_three_levels() {
        assert_eq!(severity_role(Severity::Ample), Role::Ample);
        assert_eq!(severity_role(Severity::Low), Role::Low);
        assert_eq!(severity_role(Severity::Critical), Role::Crit);
        let low = reading_text(&Reading::Used("80%".into()), Some(Severity::Low));
        assert_eq!(low.color, Role::Low);
    }

    #[test]
    fn bar_fills_round_half_up_from_the_remaining_ratio() {
        let filled = |ratio: f64, cells: u8| {
            bar(
                &row(
                    Reading::Remaining("x".into()),
                    Some(ratio),
                    Some(Severity::of(ratio)),
                ),
                cells,
            )
            .unwrap()
            .filled
        };
        assert_eq!(filled(1.0, 10), 10);
        assert_eq!(filled(0.83, 10), 8);
        assert_eq!(filled(0.85, 10), 9);
        assert_eq!(filled(0.54, 10), 5);
        assert_eq!(filled(0.045, 10), 0);
        assert_eq!(filled(0.05, 10), 1);
        assert_eq!(filled(0.0, 10), 0);
        // Five cells: each is a fifth.
        assert_eq!(filled(1.0, 5), 5);
        assert_eq!(filled(0.73, 5), 4);
        assert_eq!(filled(0.69, 5), 3);
        assert_eq!(filled(0.09, 5), 0);
        assert_eq!(filled(0.1, 5), 1);
    }

    #[test]
    fn bar_takes_the_colour_of_the_reading() {
        let low = bar(
            &row(Reading::Used("80%".into()), Some(0.2), Some(Severity::Low)),
            BAR_CELLS,
        )
        .unwrap();
        assert_eq!((low.filled, low.color), (2, Role::Low));
    }

    #[test]
    fn unlimited_is_a_full_ample_bar_and_used_up_an_empty_critical_one() {
        let full = bar(&row(Reading::CreditsUnlimited, None, None), BAR_CELLS).unwrap();
        assert_eq!((full.filled, full.color), (10, Role::Ample));
        let empty = bar(
            &row(Reading::UsedUp, Some(0.0), Some(Severity::Critical)),
            BAR_CELLS,
        )
        .unwrap();
        assert_eq!((empty.filled, empty.color), (0, Role::Crit));
    }

    #[test]
    fn rows_without_a_ratio_have_no_bar() {
        assert_eq!(
            bar(&row(Reading::Balance("$1".into()), None, None), BAR_CELLS),
            None
        );
        assert_eq!(
            bar(&row(Reading::Spent("$1".into()), None, None), BAR_CELLS),
            None
        );
        assert_eq!(bar(&row(Reading::Disabled, None, None), BAR_CELLS), None);
        assert_eq!(
            bar(&row(Reading::Credits("5".into()), None, None), BAR_CELLS),
            None
        );
        assert_eq!(
            bar(&row(Reading::Remaining("-".into()), None, None), BAR_CELLS),
            None
        );
    }

    #[test]
    fn countdown_pads_seconds_and_not_minutes() {
        assert_eq!(countdown_label(92), "1m32s");
        assert_eq!(countdown_label(120), "2m00s");
        assert_eq!(countdown_label(0), "0m00s");
        assert_eq!(countdown_label(9), "0m09s");
    }
}
