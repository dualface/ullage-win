//! The compact strip: one line for each account.

use ullage_cli::usage_view::{UsageCard, UsageRow};

use super::cards::reading_width;
use super::{Align, Content, FontKind, Measure, Rect, Shape, font, text, text_box_width};
use crate::compact::lowest_row;
use crate::display::{BAR_CELLS, Bar, ReadingText, bar, reading_text};
use crate::theme::Role;

const PAD_SIDE: f32 = 4.0;
const PAD_BOTTOM: f32 = 4.0;
const ROW_HEIGHT: f32 = 28.0;
const ROW_PAD: f32 = 8.0;
const COLUMN_GAP: f32 = 8.0;
const MIN_READING_WIDTH: f32 = 36.0;
const BAR_WIDTH: f32 = 49.0;
const CELL: (f32, f32) = (4.0, 8.0);
const CELL_GAP: f32 = 1.0;
/// A row name never takes more than this, so the account keeps its room.
const MAX_NAME_WIDTH: f32 = 90.0;

/// What one line shows; an account with no row reads `no data`.
struct Line<'a> {
    card: &'a UsageCard,
    row: Option<&'a UsageRow>,
}

pub(super) fn empty(body: &mut Content, inner_width: f32, scale: f32, measure: &dyn Measure) {
    let _ = measure;
    let height = ROW_HEIGHT * scale;
    body.shapes.push(text(
        Rect::new(PAD_SIDE, 0.0, inner_width - 2.0 * PAD_SIDE, height),
        "No subscription snapshots",
        font(FontKind::Ui, 12.0 * scale, 400),
        Role::Faint,
        Align::Center,
        true,
    ));
    body.height = height + PAD_BOTTOM;
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
    let lines: Vec<Line> = cards
        .iter()
        .map(|card| Line {
            card,
            row: lowest_row(card),
        })
        .collect();
    let readings: Vec<ReadingText> = lines
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
    let reading_col = readings
        .iter()
        .map(|reading| reading_width(reading, value_font, prefix_font, measure))
        .fold(MIN_READING_WIDTH, f32::max);

    let row_height = ROW_HEIGHT * scale;
    let left = PAD_SIDE + ROW_PAD;
    let right = inner_width - PAD_SIDE - ROW_PAD;
    let name_font = font(FontKind::Mono, 11.0 * scale, 400);
    let label_font = font(FontKind::Mono, 12.0 * scale, 700);
    let account_font = font(FontKind::Mono, 12.0 * scale, 400);

    for (index, (line, reading)) in lines.iter().zip(&readings).enumerate() {
        let y = index as f32 * row_height;
        body.shapes.push(Shape::Rect {
            rect: Rect::new(PAD_SIDE, y, inner_width - 2.0 * PAD_SIDE, 1.0),
            radius: 0.0,
            fill: Some(Role::CardBorder),
            border: None,
        });

        // Columns from the right: bar, reading, name, then the label.
        let bar_x = right - BAR_WIDTH;
        let reading_right = bar_x - COLUMN_GAP;
        let name = line.row.map(|row| row.name.as_str()).unwrap_or("");
        let name_w = if name.is_empty() {
            0.0
        } else {
            text_box_width(measure, name, name_font).min(MAX_NAME_WIDTH)
        };
        let name_right = reading_right - reading_col - COLUMN_GAP;
        let label_right = name_right - name_w - if name_w > 0.0 { COLUMN_GAP } else { 0.0 };

        if name_w > 0.0 {
            body.shapes.push(text(
                Rect::new(name_right - name_w, y, name_w, row_height),
                name,
                name_font,
                Role::Faint,
                Align::Right,
                true,
            ));
        }

        let value_w = text_box_width(measure, &reading.value, value_font);
        body.shapes.push(text(
            Rect::new(reading_right - value_w, y, value_w, row_height),
            reading.value.clone(),
            value_font,
            reading.color,
            Align::Right,
            false,
        ));
        if let Some(prefix) = reading.prefix {
            let prefix_w = text_box_width(measure, prefix, prefix_font);
            let space = measure.width(" ", prefix_font);
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

        // `provider account`, cut back with an ellipsis when it is too long.
        let avail = (label_right - left).max(0.0);
        let provider = &line.card.title.provider;
        let provider_w = text_box_width(measure, provider, label_font).min(avail);
        body.shapes.push(text(
            Rect::new(left, y, provider_w, row_height),
            provider.clone(),
            label_font,
            Role::Accent,
            Align::Left,
            true,
        ));
        let space = measure.width(" ", account_font);
        let account_x = left + provider_w + space;
        let account_avail = (label_right - account_x).max(0.0);
        if account_avail > 0.0 {
            let account = &line.card.title.account;
            let account_w = text_box_width(measure, account, account_font).min(account_avail);
            body.shapes.push(text(
                Rect::new(account_x, y, account_w, row_height),
                account.clone(),
                account_font,
                Role::Faint,
                Align::Left,
                true,
            ));
        }
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
        text_of(&scene.content, "account-5");
        assert!(!texts(&scene.content).iter().any(|t| t.text == "auto"));
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
    fn the_empty_state_is_one_faint_line() {
        let scene = build(
            &Input {
                body: Body::Empty,
                ..input(&[])
            },
            &FakeMeasure,
        );
        let line = text_of(&scene.content, "No subscription snapshots");
        assert_eq!(line.color, Role::Faint);
        assert_eq!(scene.content_height, 28.0 + 4.0);
    }

    #[test]
    fn a_long_label_is_cut_back_before_it_touches_the_name() {
        let cards = vec![card(
            "opencode",
            None,
            "account-with-a-very-long-name",
            vec![row(
                "monthly",
                Reading::Remaining("99%".into()),
                Some(0.99),
                None,
            )],
        )];
        let scene = build(&input(&cards), &FakeMeasure);
        let provider = text_of(&scene.content, "opencode");
        let account = text_of(&scene.content, "account-with-a-very-long-name");
        let name = text_of(&scene.content, "monthly");
        assert!(account.ellipsis);
        assert!(account.rect.right() <= name.rect.x);
        assert!(provider.rect.right() <= account.rect.x);
    }
}
