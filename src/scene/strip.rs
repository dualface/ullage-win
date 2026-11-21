//! The compact strip: one line for each account, with its label, the row
//! with the least left, that row's reading and reset, and a bar.

use ullage_cli::usage_view::{UsageCard, UsageRow};

use super::cards::{ContentPad, reading_width};
use super::{Align, Content, FontKind, Measure, Rect, Shape, font, text, text_box_width};
use crate::compact::{labels, lowest_row, reset_text};
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
const MIN_READING_WIDTH: f32 = 36.0;
const BAR_WIDTH: f32 = 49.0;
const CELL: (f32, f32) = (4.0, 8.0);
const CELL_GAP: f32 = 1.0;
/// A row name never takes more than this, so the account keeps its room.
const MAX_NAME_WIDTH: f32 = 90.0;
/// The room the label always keeps, enough for `claude-2` at the default
/// text size: the name gives way first, then the reading.
const MIN_LABEL_WIDTH: f32 = 60.0;
/// What follows the name of a metric that is switched off.
const OFF_MARKER: &str = " off";

/// What one line shows; an account with no row reads `no data`.
struct Line<'a> {
    /// The provider, numbered when it has more than one account.
    label: String,
    row: Option<&'a UsageRow>,
    /// How long until the row resets, when the row says.
    reset: Option<String>,
}

pub(super) fn build(
    body: &mut Content,
    cards: &[UsageCard],
    inner_width: f32,
    scale: f32,
    measure: &dyn Measure,
) {
    let value_font = font(FontKind::Mono, 12.0 * scale, 600);
    let prefix_font = font(FontKind::Mono, 11.0 * scale, 400);
    let reset_font = font(FontKind::Mono, 11.0 * scale, 400);
    let lines: Vec<Line> = cards
        .iter()
        .zip(labels(cards))
        .map(|(card, label)| {
            let row = lowest_row(card);
            Line {
                label,
                row,
                reset: row.and_then(|row| row.resets_in).map(reset_text),
            }
        })
        .collect();
    let mut readings: Vec<ReadingText> = lines
        .iter()
        .map(|line| match line.row {
            Some(row) => reading_text(&row.reading, row.severity),
            None => ReadingText {
                prefix: None,
                value: "no data".into(),
                color: Role::Faint,
            },
        })
        .collect();
    // The reset column is as wide as its longest text, and takes no room
    // when no line has a reset.
    let reset_col = lines
        .iter()
        .filter_map(|line| line.reset.as_deref())
        .map(|reset| text_box_width(measure, reset, reset_font))
        .fold(0.0, f32::max);

    let row_height = ROW_HEIGHT * scale;
    let left = PAD_SIDE + ROW_PAD;
    let right = inner_width - PAD_SIDE - ROW_PAD;
    let name_font = font(FontKind::Mono, 11.0 * scale, 400);
    let label_font = font(FontKind::Mono, 12.0 * scale, 700);

    // Columns from the right: bar, reset, reading, name, then the label.
    let bar_x = right - BAR_WIDTH;
    let reset_right = bar_x - COLUMN_GAP;
    let reading_right = if reset_col > 0.0 {
        reset_right - reset_col - COLUMN_GAP
    } else {
        reset_right
    };
    // The leftmost point the name and the reading may reach, so the label
    // keeps its room.
    let label_floor = left + MIN_LABEL_WIDTH + COLUMN_GAP;
    let room = (reading_right - label_floor).max(0.0);
    let column = |readings: &[ReadingText]| {
        readings
            .iter()
            .map(|reading| reading_width(reading, value_font, prefix_font, measure))
            .fold(MIN_READING_WIDTH, f32::max)
    };
    // When the readings do not fit with their prefixes, every line leaves
    // its prefix out, and a value still too wide is cut back.
    let mut reading_col = column(&readings);
    if reading_col > room {
        for reading in &mut readings {
            reading.prefix = None;
        }
        reading_col = column(&readings).min(room);
    }
    let name_right = reading_right - reading_col - COLUMN_GAP;
    // A name with less room than this is left out rather than shown as a
    // bare ellipsis.
    let min_name_w = text_box_width(measure, "mm", name_font);

    for (index, (line, reading)) in lines.iter().zip(&readings).enumerate() {
        let y = index as f32 * row_height;
        body.shapes.push(Shape::Rect {
            rect: Rect::new(PAD_SIDE, y, inner_width - 2.0 * PAD_SIDE, 1.0),
            radius: 0.0,
            fill: Some(Role::CardBorder),
            border: None,
        });

        let name = line.row.map(|row| row.name.as_str()).unwrap_or("");
        let off = line.row.is_some_and(|row| row.off);
        // The `off` marker follows the name, in the same faint colour as the
        // name, and is cut last.
        let off_w = if off {
            text_box_width(measure, OFF_MARKER, name_font)
        } else {
            0.0
        };
        let name_w = if name.is_empty() {
            0.0
        } else {
            (text_box_width(measure, name, name_font) + off_w)
                .min(MAX_NAME_WIDTH)
                .min((name_right - label_floor).max(0.0))
        };
        let name_w = if name_w < min_name_w { 0.0 } else { name_w };
        let label_right = name_right - name_w - if name_w > 0.0 { COLUMN_GAP } else { 0.0 };

        if name_w > 0.0 {
            let name_box = (name_w - off_w).max(0.0);
            body.shapes.push(text(
                Rect::new(name_right - name_w, y, name_box, row_height),
                name,
                name_font,
                Role::Faint,
                Align::Left,
                true,
            ));
            if off && name_w > name_box {
                body.shapes.push(text(
                    Rect::new(
                        name_right - name_w + name_box,
                        y,
                        name_w - name_box,
                        row_height,
                    ),
                    OFF_MARKER,
                    name_font,
                    Role::Faint,
                    Align::Left,
                    false,
                ));
            }
        }

        let full_w = text_box_width(measure, &reading.value, value_font);
        let value_w = full_w.min(reading_col);
        body.shapes.push(text(
            Rect::new(reading_right - value_w, y, value_w, row_height),
            reading.value.clone(),
            value_font,
            reading.color,
            Align::Right,
            value_w < full_w,
        ));
        if let Some(prefix) = reading.prefix {
            let prefix_w = text_box_width(measure, prefix, prefix_font);
            let space = measure.width(" ", prefix_font);
            {
                body.shapes.push(text(
                    Rect::new(
                        reading_right - value_w - space - prefix_w,
                        y,
                        prefix_w,
                        row_height,
                    ),
                    prefix,
                    prefix_font,
                    Role::Faint,
                    Align::Right,
                    false,
                ));
            }
        }

        if let Some(reset) = &line.reset {
            let reset_w = text_box_width(measure, reset, reset_font);
            body.shapes.push(text(
                Rect::new(reset_right - reset_w, y, reset_w, row_height),
                reset.clone(),
                reset_font,
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
        let avail = (label_right - left).max(0.0);
        let label_w = text_box_width(measure, &line.label, label_font).min(avail);
        body.shapes.push(text(
            Rect::new(left, y, label_w, row_height),
            line.label.clone(),
            label_font,
            Role::Accent,
            Align::Left,
            true,
        ));
    }
    body.height = lines.len() as f32 * row_height + PAD_BOTTOM;
}

#[cfg(test)]
mod tests {
    use ullage_cli::usage_view::{Reading, UsageNotice};

    use super::super::cards::tests::{card, row, sample_cards};
    use super::super::testing::*;
    use super::super::*;
    use crate::prefs::Layout;

    fn input<'a>(cards: &'a [UsageCard]) -> Input<'a> {
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
        assert_eq!(scene.width, 290.0);
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
    fn a_line_shows_the_account_row_with_the_least_left() {
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
        let name = text_of(&scene.content, "api");
        assert_eq!((name.font.size, name.color), (11.0, Role::Faint));
        let value = text_of(&scene.content, "18%");
        assert_eq!(
            (value.font.size, value.font.weight, value.color),
            (12.0, 600, Role::Low)
        );
        let provider = text_of(&scene.content, "cursor");
        assert_eq!((provider.font.weight, provider.color), (700, Role::Accent));
        assert!(!texts(&scene.content).iter().any(|t| t.text == "auto"));
    }

    #[test]
    fn a_selected_row_marked_off_shows_the_faint_off_after_its_name() {
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
        let name = text_of(&scene.content, "api");
        let off = text_of(&scene.content, " off");
        assert_eq!(off.color, Role::Faint);
        assert_eq!(off.rect.x, name.rect.right());
        // Without the marker there is no `off`.
        let plain = vec![card(
            "cursor",
            None,
            "account-5",
            vec![row(
                "api",
                Reading::Remaining("18%".into()),
                Some(0.18),
                None,
            )],
        )];
        let scene = build(&input(&plain), &FakeMeasure);
        assert!(!texts(&scene.content).iter().any(|t| t.text == " off"));
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
        cards[1].rows[0].reading = Reading::Credits("120 of 500".into());
        cards[1].rows[0].remaining_ratio = Some(0.05);
        cards[1].rows[1].remaining_ratio = Some(0.9);
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
    fn a_long_label_is_cut_back_before_it_touches_the_name() {
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
        let name = text_of(&scene.content, "monthly");
        assert!(label.ellipsis);
        assert!(label.rect.right() <= name.rect.x);
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
        assert_eq!((label.font.weight, label.color), (700, Role::Accent));
        text_of(&scene.content, "claude-2");
        text_of(&scene.content, "codex");
        let shown: Vec<&str> = texts(&scene.content)
            .iter()
            .map(|t| t.text.as_str())
            .collect();
        for account in ["account-0", "account-1", "account-9"] {
            assert!(!shown.contains(&account), "{account} is drawn");
        }
    }

    #[test]
    fn the_reset_sits_between_the_reading_and_the_bar() {
        // The weekly row, 4 days off, has the least left.
        let cards = sample_cards(1);
        let scene = build(&input(&cards), &FakeMeasure);
        let reset = text_of(&scene.content, "4d");
        assert_eq!(
            (reset.font.size, reset.font.weight, reset.color, reset.align),
            (11.0, 400, Role::Sub, Align::Right)
        );
        let reading = text_of(&scene.content, "73%");
        // 290 wide, less the 1 DIP frame, 4 of padding and 8 inside the row:
        // the bar starts 49 DIPs further left, the reset 8 before it.
        let bar_x = 290.0 - 2.0 - 4.0 - 8.0 - 49.0;
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
                    "weekly",
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
        // The reading column moves left by the widest reset.
        let reading = text_of(&scene.content, "50%");
        assert_eq!(reading.rect.right(), hours.rect.x - 8.0);
    }

    #[test]
    fn without_any_reset_the_reading_sits_next_to_the_bar() {
        let cards = vec![card(
            "codex",
            None,
            "account-9",
            vec![row("5h", Reading::Remaining("50%".into()), Some(0.5), None)],
        )];
        let scene = build(&input(&cards), &FakeMeasure);
        let reading = text_of(&scene.content, "50%");
        assert_eq!(reading.rect.right(), 290.0 - 2.0 - 4.0 - 8.0 - 49.0 - 8.0);
    }

    /// Fails when two texts of one line overlap or a text leaves the strip.
    fn assert_lines_fit(scene: &Scene) {
        let inner = 290.0 - 2.0;
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
        cards[1].rows[0].reading = Reading::Credits("120 of 500".into());
        cards[1].rows[0].remaining_ratio = Some(0.05);
        let scene = build(&input(&cards), &FakeMeasure);
        for label in ["claude-1", "claude-2"] {
            // Eight 7.2 DIP characters, rounded up, plus one: not cut back.
            assert_eq!(text_of(&scene.content, label).rect.w, 59.0);
        }
        text_of(&scene.content, "36h00m");
        assert!(!text_of(&scene.content, "120 of 500").ellipsis);
        // Without room for both, the prefix gives way.
        assert!(!texts(&scene.content).iter().any(|t| t.text == "credits"));
        assert_lines_fit(&scene);
    }

    #[test]
    fn a_value_wider_than_its_room_is_cut_back_with_an_ellipsis() {
        let mut cards = sample_cards(1);
        cards[0].rows[0].reading = Reading::Credits("123456789 of 999999999".into());
        cards[0].rows[0].remaining_ratio = Some(0.05);
        let scene = build(&input(&cards), &FakeMeasure);
        assert!(text_of(&scene.content, "123456789 of 999999999").ellipsis);
        // The label is not cut back: 6 × 7.2 DIPs, rounded up, plus one.
        assert_eq!(text_of(&scene.content, "claude").rect.w, 45.0);
        assert_lines_fit(&scene);
    }

    #[test]
    fn readings_that_fit_keep_their_prefix_and_names() {
        let mut cards = sample_cards(1);
        cards[0].rows[1].reading = Reading::Used("12%".into());
        let scene = build(&input(&cards), &FakeMeasure);
        text_of(&scene.content, "used");
        text_of(&scene.content, "weekly");
        assert_lines_fit(&scene);
    }
}
