//! The compact strip: one line for each account, with its label, the
//! weekly row (or the row with the least left when there is none), that
//! row's reading and reset, and a bar. No line names its row.

use ullage_cli::usage_view::{Reading, UsageCard, UsageRow};

use super::cards::ContentPad;
use super::{
    Align, BORDER, Content, Font, FontKind, Measure, Rect, Shape, font, text, text_box_width,
};
use crate::compact::{labels, reset_text, strip_row};
use crate::display::{BAR_CELLS, Bar, ReadingText, bar, reading_text};
use crate::theme::Role;

const PAD_SIDE: f32 = 4.0;
const PAD_BOTTOM: f32 = 4.0;
/// The content area of the strip: no space above, 4 DIPs at the sides and
/// below.
pub(super) const CONTENT_PAD: ContentPad = ContentPad {
    top: 0.0,
    side: PAD_SIDE,
    bottom: PAD_BOTTOM,
};
const ROW_HEIGHT: f32 = 28.0;
const ROW_PAD: f32 = 8.0;
const COLUMN_GAP: f32 = 8.0;
const BAR_WIDTH: f32 = 49.0;
const CELL: (f32, f32) = (4.0, 8.0);
const CELL_GAP: f32 = 1.0;
/// The most room a label keeps for itself before the reading takes its
/// own: enough for `claude-2` at the default text size. A shorter label
/// keeps its own width.
const MIN_LABEL_WIDTH: f32 = 60.0;
/// The longest reset text `reset_text` gives; the reset column is always
/// this wide, so the window does not change width when a reset appears.
const RESET_RESERVE: &str = "36h00m";
/// The widest number a reading holds; the reading column reserves it for
/// every reading that is a number.
const READING_RESERVE: &str = "100%";

/// What one line shows; an account with no row reads `no data`.
struct Line<'a> {
    /// The provider, numbered when it has more than one account.
    label: String,
    row: Option<&'a UsageRow>,
    /// How long until the row resets, when the row says.
    reset: Option<String>,
}

/// The four fonts of a line, at the strip's scale.
struct Fonts {
    value: Font,
    prefix: Font,
    reset: Font,
    label: Font,
}

impl Fonts {
    fn new(scale: f32) -> Self {
        Self {
            value: font(FontKind::Mono, 12.0 * scale, 600),
            prefix: font(FontKind::Mono, 11.0 * scale, 400),
            reset: font(FontKind::Mono, 11.0 * scale, 400),
            label: font(FontKind::Mono, 12.0 * scale, 700),
        }
    }
}

/// What a line's reading says; an account with no row reads `no data`.
fn line_reading(row: Option<&UsageRow>) -> ReadingText {
    match row {
        Some(row) => reading_text(&row.reading, row.severity),
        None => ReadingText {
            prefix: None,
            value: "no data".into(),
            color: Role::Faint,
        },
    }
}

/// A prefix and the space after it.
fn prefix_width(word: &str, fonts: &Fonts, measure: &dyn Measure) -> f32 {
    text_box_width(measure, word, fonts.prefix) + measure.width(" ", fonts.prefix)
}

/// The room a line's reading is given, whatever its number says: a number
/// takes the room of `100%`, with its prefix when it has one. Words
/// (`unlimited`, `disabled`, `no data`) take their own width, so the width
/// changes only when a reading changes kind.
fn reserved_reading_width(row: Option<&UsageRow>, fonts: &Fonts, measure: &dyn Measure) -> f32 {
    let reading = line_reading(row);
    let is_word = matches!(
        row.map(|row| &row.reading),
        None | Some(Reading::CreditsUnlimited | Reading::Disabled)
    );
    let value = if is_word {
        &reading.value
    } else {
        READING_RESERVE
    };
    text_box_width(measure, value, fonts.value)
        + reading
            .prefix
            .map_or(0.0, |word| prefix_width(word, fonts, measure))
}

fn reset_column_width(fonts: &Fonts, measure: &dyn Measure) -> f32 {
    text_box_width(measure, RESET_RESERVE, fonts.reset)
}

/// The window width that fits every line: the widest label, the widest
/// reserved reading, the reset column and the bar, with the paddings, the
/// gaps and the frame. [`build`] lays its lines out in the width this
/// gives, or in less when the window is capped.
pub(super) fn needed_width(cards: &[UsageCard], scale: f32, measure: &dyn Measure) -> f32 {
    let fonts = Fonts::new(scale);
    let (mut label, mut reading) = (0.0f32, 0.0f32);
    for (card, name) in cards.iter().zip(labels(cards)) {
        label = label.max(text_box_width(measure, &name, fonts.label));
        reading = reading.max(reserved_reading_width(strip_row(card), &fonts, measure));
    }
    2.0 * (PAD_SIDE + ROW_PAD)
        + label
        + COLUMN_GAP
        + reading
        + COLUMN_GAP
        + reset_column_width(&fonts, measure)
        + COLUMN_GAP
        + BAR_WIDTH
        + 2.0 * BORDER
}

pub(super) fn build(
    body: &mut Content,
    cards: &[UsageCard],
    inner_width: f32,
    scale: f32,
    measure: &dyn Measure,
) {
    let fonts = Fonts::new(scale);
    let lines: Vec<Line> = cards
        .iter()
        .zip(labels(cards))
        .map(|(card, label)| {
            let row = strip_row(card);
            Line {
                label,
                row,
                reset: row.and_then(|row| row.resets_in).map(reset_text),
            }
        })
        .collect();
    let reset_col = reset_column_width(&fonts, measure);

    let row_height = ROW_HEIGHT * scale;
    let left = PAD_SIDE + ROW_PAD;
    let right = inner_width - PAD_SIDE - ROW_PAD;
    // Shared columns from the right: bar, reset, then the right edge of the
    // readings. The reset column keeps its room when a line has no reset.
    // Each line lays out its own reading and label.
    let bar_x = right - BAR_WIDTH;
    let reset_right = bar_x - COLUMN_GAP;
    let reading_right = reset_right - reset_col - COLUMN_GAP;

    for (index, line) in lines.iter().enumerate() {
        let y = index as f32 * row_height;
        body.shapes.push(Shape::Rect {
            rect: Rect::new(PAD_SIDE, y, inner_width - 2.0 * PAD_SIDE, 1.0),
            radius: 0.0,
            fill: Some(Role::CardBorder),
            border: None,
        });

        let reading = line_reading(line.row);
        let label_full = text_box_width(measure, &line.label, fonts.label);
        let prefix = reading
            .prefix
            .map(|word| (word, prefix_width(word, &fonts, measure)));
        let fit = fit_reading(
            reading_right - left - label_full.min(MIN_LABEL_WIDTH) - COLUMN_GAP,
            text_box_width(measure, &reading.value, fonts.value),
            prefix.map_or(0.0, |(_, width)| width),
        );

        body.shapes.push(text(
            Rect::new(reading_right - fit.value, y, fit.value, row_height),
            reading.value.clone(),
            fonts.value,
            reading.color,
            Align::Right,
            fit.value_cut,
        ));
        let mut reading_left = reading_right - fit.value;
        if let Some((word, width)) = prefix.filter(|_| fit.prefix) {
            reading_left -= width;
            body.shapes.push(text(
                Rect::new(
                    reading_left,
                    y,
                    text_box_width(measure, word, fonts.prefix),
                    row_height,
                ),
                word,
                fonts.prefix,
                Role::Faint,
                Align::Right,
                false,
            ));
        }

        if let Some(reset) = &line.reset {
            let reset_w = text_box_width(measure, reset, fonts.reset);
            body.shapes.push(text(
                Rect::new(reset_right - reset_w, y, reset_w, row_height),
                reset.clone(),
                fonts.reset,
                Role::Sub,
                Align::Right,
                false,
            ));
        }

        if let Some(Bar { filled, color }) = line.row.and_then(bar) {
            let top = y + (row_height - CELL.1) / 2.0;
            for cell in 0..BAR_CELLS {
                let lit = cell >= BAR_CELLS - filled;
                body.shapes.push(Shape::Rect {
                    rect: Rect::new(
                        bar_x + f32::from(cell) * (CELL.0 + CELL_GAP),
                        top,
                        CELL.0,
                        CELL.1,
                    ),
                    radius: 1.0,
                    fill: Some(if lit { color } else { Role::Track }),
                    border: None,
                });
            }
        }

        // The label, cut back with an ellipsis when it is too long.
        let avail = (reading_left - COLUMN_GAP - left).max(0.0);
        body.shapes.push(text(
            Rect::new(left, y, label_full.min(avail), row_height),
            line.label.clone(),
            fonts.label,
            Role::Accent,
            Align::Left,
            true,
        ));
    }
    body.height = lines.len() as f32 * row_height + PAD_BOTTOM;
}

/// How a reading is shown: whether it keeps its prefix, and the width of
/// its value.
#[derive(Debug, PartialEq)]
struct ReadingFit {
    prefix: bool,
    value: f32,
    value_cut: bool,
}

/// Fits a reading into `room`, the space between the label's reserved
/// width and the right edge of the readings. `prefix` is the prefix with
/// the space after it, 0 without one. What does not fit gives way in this
/// order: the prefix, then the value is cut back.
fn fit_reading(room: f32, value: f32, prefix: f32) -> ReadingFit {
    let room = room.max(0.0);
    if value + prefix <= room {
        return ReadingFit {
            prefix: true,
            value,
            value_cut: false,
        };
    }
    ReadingFit {
        prefix: false,
        value: value.min(room),
        value_cut: value > room,
    }
}

#[cfg(test)]
mod width_tests;

#[cfg(test)]
mod tests {
    use ullage_cli::usage_view::{Reading, UsageNotice};

    use super::super::cards::tests::{card, row, sample_cards};
    use super::super::testing::*;
    use super::super::*;
    use super::{ReadingFit, fit_reading};
    use crate::prefs::Layout;

    pub(super) fn input<'a>(cards: &'a [UsageCard]) -> Input<'a> {
        Input {
            layout: Layout::Compact,
            text_size: 13,
            body: Body::Cards(cards),
            countdown: (1.0, 120),
            pinned: true,
            hover: None,
            max_window_height: 5000.0,
        }
    }

    #[test]
    fn each_account_is_one_28_dip_line_with_a_divider() {
        let cards = sample_cards(3);
        let scene = build(&input(&cards), &FakeMeasure);
        assert_eq!(scene.width, 229.0);
        assert_eq!(scene.content_height, 3.0 * 28.0 + 4.0);
        let dividers: Vec<f32> = scene
            .content
            .iter()
            .filter_map(|shape| match shape {
                Shape::Rect {
                    rect,
                    fill: Some(Role::CardBorder),
                    ..
                } => Some(rect.y),
                _ => None,
            })
            .collect();
        assert_eq!(dividers, [0.0, 28.0, 56.0]);
        assert!(scene.card_rects.is_empty());
    }

    #[test]
    fn without_a_weekly_row_a_line_shows_the_row_with_the_least_left_unnamed() {
        let cards = vec![card(
            "cursor",
            Some("Ultra"),
            "account-5",
            vec![
                row("auto", Reading::Remaining("52%".into()), Some(0.52), None),
                row("api", Reading::Remaining("18%".into()), Some(0.18), None),
                row("usage", Reading::Remaining("52%".into()), Some(0.52), None),
            ],
        )];
        let scene = build(&input(&cards), &FakeMeasure);
        let value = text_of(&scene.content, "18%");
        assert_eq!(
            (value.font.size, value.font.weight, value.color),
            (12.0, 600, Role::Low)
        );
        let provider = text_of(&scene.content, "cursor");
        assert_eq!((provider.font.weight, provider.color), (700, Role::Accent));
        let shown: Vec<&str> = texts(&scene.content)
            .iter()
            .map(|t| t.text.as_str())
            .collect();
        assert_eq!(shown.len(), 2, "only the label and the reading: {shown:?}");
    }

    #[test]
    fn the_weekly_row_is_shown_even_with_another_row_lower() {
        let mut cards = sample_cards(1);
        cards[0].rows[0].reading = Reading::Remaining("10%".into());
        cards[0].rows[0].remaining_ratio = Some(0.1);
        let scene = build(&input(&cards), &FakeMeasure);
        text_of(&scene.content, "73%");
        text_of(&scene.content, "4d");
        let shown: Vec<&str> = texts(&scene.content)
            .iter()
            .map(|t| t.text.as_str())
            .collect();
        for hidden in ["10%", "weekly", "5h", "account-0"] {
            assert!(!shown.contains(&hidden), "{hidden} is drawn");
        }
        // The bar is the weekly row's too: 73% lights seven cells.
        let lit = scene
            .content
            .iter()
            .filter(|shape| {
                matches!(shape, Shape::Rect { radius, rect, fill: Some(role), .. }
                    if *radius == 1.0 && rect.w == 4.0 && *role != Role::Track)
            })
            .count();
        assert_eq!(lit, 7);
    }

    #[test]
    fn a_label_wider_than_its_reserved_room_keeps_that_room() {
        let mut cards = sample_cards(1);
        cards[0].title.provider = "openrouter".into();
        cards[0].rows[1].reading = Reading::Credits("1234 of 50000".into());
        cards[0].rows[1].resets_in = Some(36 * 3600);
        let scene = build(&input(&cards), &FakeMeasure);
        let label = text_of(&scene.content, "openrouter");
        assert_eq!(label.rect.w, 60.0);
        assert!(text_of(&scene.content, "1234 of 50000").ellipsis);
        assert_lines_fit(&scene);
    }

    #[test]
    fn a_selected_row_marked_off_shows_no_off_marker() {
        let mut off_row = row("api", Reading::Remaining("18%".into()), Some(0.18), None);
        off_row.off = true;
        let cards = vec![card(
            "cursor",
            None,
            "account-5",
            vec![
                row("auto", Reading::Remaining("52%".into()), Some(0.52), None),
                off_row,
            ],
        )];
        let scene = build(&input(&cards), &FakeMeasure);
        text_of(&scene.content, "18%");
        assert!(
            !texts(&scene.content)
                .iter()
                .any(|t| t.text.contains("off") || t.text == "api")
        );
    }

    #[test]
    fn the_bar_has_ten_cells_of_four_by_eight() {
        let cards = sample_cards(1);
        let scene = build(&input(&cards), &FakeMeasure);
        let cells: Vec<(Rect, Role)> = scene
            .content
            .iter()
            .filter_map(|shape| match shape {
                Shape::Rect {
                    rect,
                    fill: Some(role),
                    radius,
                    ..
                } if *radius == 1.0 && rect.w == 4.0 => Some((*rect, *role)),
                _ => None,
            })
            .collect();
        assert_eq!(cells.len(), 10);
        assert_eq!(cells[1].0.x - cells[0].0.x, 5.0);
        assert!(cells.iter().all(|(rect, _)| rect.h == 8.0));
        // 73% remaining: seven cells lit from the right.
        assert_eq!(
            cells
                .iter()
                .filter(|(_, role)| *role == Role::Ample)
                .count(),
            7
        );
        assert_eq!(cells[0].1, Role::Track);
    }

    #[test]
    fn an_account_without_rows_reads_no_data_and_has_no_bar() {
        let mut cards = sample_cards(1);
        cards.push(card("grok", None, "account-4", Vec::new()));
        cards[1].notices = vec![UsageNotice::NoMetrics];
        let scene = build(&input(&cards), &FakeMeasure);
        let none = text_of(&scene.content, "no data");
        assert_eq!(none.color, Role::Faint);
        let cells = scene
            .content
            .iter()
            .filter(|shape| matches!(shape, Shape::Rect { radius, rect, .. } if *radius == 1.0 && rect.w == 4.0))
            .count();
        assert_eq!(cells, 10, "only the first account has a bar");
    }

    #[test]
    fn the_reading_column_is_shared_and_right_aligned() {
        let mut cards = sample_cards(2);
        cards[1].rows[1].reading = Reading::Credits("120 of 500".into());
        let scene = build(&input(&cards), &FakeMeasure);
        let first = text_of(&scene.content, "73%");
        let second = text_of(&scene.content, "120 of 500");
        assert_eq!(first.rect.right(), second.rect.right());
        assert_eq!(first.align, Align::Right);
    }

    #[test]
    fn the_empty_state_is_one_card_with_a_faint_line_at_the_strip_width() {
        let scene = build(
            &Input {
                body: Body::Empty,
                ..input(&[])
            },
            &FakeMeasure,
        );
        let line = text_of(&scene.content, "No subscription snapshots");
        assert_eq!(line.color, Role::Faint);
        assert_eq!(scene.width, 290.0);
        assert_eq!(scene.card_rects.len(), 1);
        // The strip's content area: 4 DIPs at each side, none above.
        assert_eq!(scene.card_rects[0].w, 290.0 - 2.0 - 8.0);
        assert_eq!(scene.card_rects[0].x, 4.0);
        assert_eq!(scene.card_rects[0].y, 0.0);
        assert_eq!(scene.content_height, scene.card_rects[0].bottom() + 4.0);
        let frame = scene.content.iter().any(|shape| {
            matches!(
                shape,
                Shape::Rect {
                    fill: Some(Role::Card),
                    border: Some((Role::CardBorder, _)),
                    ..
                }
            )
        });
        assert!(frame, "the text sits on a card");
    }

    #[test]
    fn a_long_label_is_cut_back_before_it_touches_the_reading() {
        let cards = vec![card(
            "a-provider-with-a-very-long-name",
            None,
            "account-5",
            vec![row(
                "monthly",
                Reading::Remaining("99%".into()),
                Some(0.99),
                Some(86_400),
            )],
        )];
        let scene = build(&input(&cards), &FakeMeasure);
        let label = text_of(&scene.content, "a-provider-with-a-very-long-name");
        let reading = text_of(&scene.content, "99%");
        assert!(label.ellipsis);
        assert_eq!(label.rect.right() + 8.0, reading.rect.x);
    }

    #[test]
    fn the_label_is_the_provider_numbered_only_when_it_has_several_accounts() {
        let mut cards = sample_cards(2);
        cards.push(card(
            "codex",
            None,
            "account-9",
            vec![row("5h", Reading::Remaining("50%".into()), Some(0.5), None)],
        ));
        let scene = build(&input(&cards), &FakeMeasure);
        let label = text_of(&scene.content, "claude-1");
        assert_eq!(
            (label.font.weight, label.color, label.rect.y),
            (700, Role::Accent, 0.0)
        );
        assert_eq!(text_of(&scene.content, "claude-2").rect.y, 28.0);
        text_of(&scene.content, "codex");
        let shown: Vec<&str> = texts(&scene.content)
            .iter()
            .map(|t| t.text.as_str())
            .collect();
        for hidden in ["account-0", "account-1", "account-9", "weekly", "5h"] {
            assert!(!shown.contains(&hidden), "{hidden} is drawn");
        }
    }

    #[test]
    fn the_reset_sits_between_the_reading_and_the_bar() {
        // The weekly row, 4 days off.
        let cards = sample_cards(1);
        let scene = build(&input(&cards), &FakeMeasure);
        let reset = text_of(&scene.content, "4d");
        assert_eq!(
            (reset.font.size, reset.font.weight, reset.color, reset.align),
            (11.0, 400, Role::Sub, Align::Right)
        );
        let reading = text_of(&scene.content, "73%");
        // The window's width, less the 1 DIP frame, 4 of padding and 8 inside
        // the row: the bar starts 49 DIPs further left, the reset 8 before it.
        let bar_x = scene.width - 2.0 - 4.0 - 8.0 - 49.0;
        assert_eq!(reset.rect.right(), bar_x - 8.0);
        assert!(reading.rect.right() + 8.0 <= reset.rect.x);
    }

    #[test]
    fn resets_share_a_right_edge_and_a_line_without_one_shows_none() {
        let mut cards = sample_cards(2);
        cards[1].rows[1].resets_in = Some(5 * 3600 + 3 * 60);
        cards.push(card(
            "codex",
            None,
            "account-9",
            // The selected row has no reset; the other row's reset is not
            // borrowed.
            vec![
                row("5h", Reading::Remaining("50%".into()), Some(0.5), None),
                row(
                    "auto",
                    Reading::Remaining("90%".into()),
                    Some(0.9),
                    Some(3600),
                ),
            ],
        ));
        cards.push(card("grok", None, "account-4", Vec::new()));
        let scene = build(&input(&cards), &FakeMeasure);
        let days = text_of(&scene.content, "4d");
        let hours = text_of(&scene.content, "5h03m");
        assert_eq!(days.rect.right(), hours.rect.right());
        let resets = texts(&scene.content)
            .iter()
            .filter(|t| t.color == Role::Sub)
            .count();
        assert_eq!(resets, 2, "the codex and grok lines have no reset");
        // The reading column ends before the reset column, which is as wide as
        // `36h00m`: 6 characters of 6.6 DIPs, rounded up, plus one.
        let reset_right = scene.width - 2.0 - 4.0 - 8.0 - 49.0 - 8.0;
        assert_eq!(days.rect.right(), reset_right);
        let reading = text_of(&scene.content, "50%");
        assert_eq!(reading.rect.right(), reset_right - 41.0 - 8.0);
    }

    #[test]
    fn without_any_reset_the_reading_keeps_the_room_of_the_reset_column() {
        let cards = vec![card(
            "codex",
            None,
            "account-9",
            vec![row("5h", Reading::Remaining("50%".into()), Some(0.5), None)],
        )];
        let scene = build(&input(&cards), &FakeMeasure);
        let reading = text_of(&scene.content, "50%");
        assert_eq!(
            reading.rect.right(),
            scene.width - 2.0 - 4.0 - 8.0 - 49.0 - 8.0 - 41.0 - 8.0
        );
    }

    /// Fails when two texts of one line overlap or a text leaves the strip.
    pub(super) fn assert_lines_fit(scene: &Scene) {
        let inner = scene.width - 2.0 * BORDER;
        let mut shown: Vec<&Text> = texts(&scene.content);
        shown.sort_by(|a, b| {
            (a.rect.y, a.rect.x)
                .partial_cmp(&(b.rect.y, b.rect.x))
                .unwrap()
        });
        for pair in shown.windows(2) {
            if pair[0].rect.y == pair[1].rect.y {
                assert!(
                    pair[0].rect.right() <= pair[1].rect.x,
                    "{:?} overlaps {:?}",
                    pair[0].text,
                    pair[1].text
                );
            }
        }
        for text in shown {
            assert!(
                text.rect.x >= 0.0 && text.rect.right() <= inner,
                "{:?}",
                text.text
            );
        }
    }

    #[test]
    fn a_wide_reading_and_a_reset_leave_the_label_its_room() {
        let mut cards = sample_cards(2);
        cards[0].rows[1].resets_in = Some(36 * 3600);
        cards[1].rows[1].reading = Reading::Credits("120 of 500".into());
        let scene = build(&input(&cards), &FakeMeasure);
        for label in ["claude-1", "claude-2"] {
            // Eight 7.2 DIP characters, rounded up, plus one: not cut back.
            assert_eq!(text_of(&scene.content, label).rect.w, 59.0);
        }
        text_of(&scene.content, "36h00m");
        assert!(!text_of(&scene.content, "120 of 500").ellipsis);
        // The wide reading gives up its prefix.
        assert!(!texts(&scene.content).iter().any(|t| t.text == "credits"));
        assert_lines_fit(&scene);
    }

    #[test]
    fn a_value_wider_than_its_room_is_cut_back_with_an_ellipsis() {
        let mut cards = sample_cards(1);
        cards[0].rows[1].reading = Reading::Credits("123456789 of 999999999".into());
        let scene = build(&input(&cards), &FakeMeasure);
        assert!(text_of(&scene.content, "123456789 of 999999999").ellipsis);
        // The label is not cut back: 6 × 7.2 DIPs, rounded up, plus one.
        assert_eq!(text_of(&scene.content, "claude").rect.w, 45.0);
        assert_lines_fit(&scene);
    }

    #[test]
    fn a_reading_that_fits_keeps_its_prefix() {
        let mut cards = sample_cards(1);
        cards[0].rows[1].reading = Reading::Used("12%".into());
        let scene = build(&input(&cards), &FakeMeasure);
        let used = text_of(&scene.content, "used");
        assert_eq!(used.color, Role::Faint);
        assert_lines_fit(&scene);
    }

    #[test]
    fn one_wide_reading_does_not_take_the_prefix_of_another_line() {
        let mut cards = sample_cards(2);
        cards[0].rows[1].resets_in = Some(36 * 3600);
        cards[0].rows[1].reading = Reading::Credits("12 of 50".into());
        cards[1].rows[1].reading = Reading::Used("12%".into());
        let scene = build(&input(&cards), &FakeMeasure);
        assert!(!texts(&scene.content).iter().any(|t| t.text == "credits"));
        let used = text_of(&scene.content, "used");
        assert_eq!(used.rect.y, 28.0);
        // The readings still share their right edge.
        assert_eq!(
            text_of(&scene.content, "12 of 50").rect.right(),
            text_of(&scene.content, "12%").rect.right()
        );
        assert_lines_fit(&scene);
    }

    #[test]
    fn a_reading_with_room_keeps_its_prefix() {
        assert_eq!(
            fit_reading(100.0, 30.0, 20.0),
            ReadingFit {
                prefix: true,
                value: 30.0,
                value_cut: false
            }
        );
    }

    #[test]
    fn the_prefix_goes_before_the_value_is_cut() {
        assert_eq!(
            fit_reading(40.0, 30.0, 20.0),
            ReadingFit {
                prefix: false,
                value: 30.0,
                value_cut: false
            }
        );
        assert_eq!(
            fit_reading(25.0, 30.0, 20.0),
            ReadingFit {
                prefix: false,
                value: 25.0,
                value_cut: true
            }
        );
        assert_eq!(fit_reading(-5.0, 30.0, 0.0).value, 0.0);
    }
}
