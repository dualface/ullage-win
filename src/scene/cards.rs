//! The expanded layouts: cards in one column or two.

use ullage_cli::usage_view::{DayDot, ResetView, UsageCard, UsageNotice, UsageRow, reset_view};

use super::{
    Align, CARD_GAP, CONTENT_PAD_BOTTOM, CONTENT_PAD_SIDE, CONTENT_PAD_TOP, Content, DotFill,
    FontKind, Icon, Measure, Rect, Shape, font, text, text_box_width,
};
use crate::display::{BAR_CELLS, Bar, ReadingText, bar, reading_text};
use crate::prefs::Layout;
use crate::theme::Role;

const CARD_RADIUS: f32 = 6.0;
const CARD_PAD_TOP: f32 = 6.0;
const CARD_PAD_SIDE: f32 = 12.0;
const CARD_PAD_BOTTOM: f32 = 8.0;
const TITLE_HEIGHT: f32 = 28.0;
const ROW_HEIGHT: f32 = 24.0;
const COLUMN_GAP: f32 = 10.0;
const MIN_READING_WIDTH: f32 = 72.0;
const RESET_WIDTH: f32 = 72.0;
const BAR_WIDTH: f32 = 68.0;
const CELL: (f32, f32) = (5.0, 8.0);
const CELL_GAP: f32 = 2.0;
const DOT_DIAMETER: f32 = 7.0;
const DOT_GAP: f32 = 3.0;
const DOT_BORDER: f32 = 1.25;
const TAG_HEIGHT: f32 = 20.0;
const TAG_TOP: f32 = 6.0;
const TAG_GAP: f32 = 6.0;
const EMPTY_PAD: f32 = 16.0;
const EMPTY_LINE: f32 = 18.0;

/// The size of the text in one card, from the text size setting.
struct Sizes {
    scale: f32,
}

impl Sizes {
    fn size(&self, base: f32) -> f32 {
        base * self.scale
    }
}

/// The widths the title of a card gets, after the longest part has been cut
/// back: first the plan, then the account, and the provider last. A width
/// of zero means the part is left out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct TitleFit {
    pub provider: f32,
    pub plan: f32,
    pub account: f32,
}

pub(super) fn fit_title(avail: f32, provider: f32, plan: Option<f32>, account: f32) -> TitleFit {
    let mut widths = [provider, plan.unwrap_or(0.0), account];
    let total = |widths: &[f32; 3]| {
        let present = widths.iter().filter(|width| **width > 0.0).count();
        widths.iter().sum::<f32>() + COLUMN_GAP * present.saturating_sub(1) as f32
    };
    // The plan is cut first, then the account, then the provider.
    for index in [1, 2, 0] {
        let excess = total(&widths) - avail;
        if excess <= 0.0 {
            break;
        }
        widths[index] = (widths[index] - excess).max(0.0);
    }
    TitleFit {
        provider: widths[0],
        plan: widths[1],
        account: widths[2],
    }
}

fn notice_text(notice: UsageNotice) -> String {
    match notice {
        UsageNotice::Stale => "stale snapshot".into(),
        UsageNotice::LimitReached => "limit reached".into(),
        UsageNotice::Unavailable(count) => format!("{count} item(s) unavailable"),
        UsageNotice::NoMetrics => "no summarized metrics".into(),
    }
}

/// The width a reading needs: its prefix, a space, and its value.
pub(super) fn reading_width(
    reading: &ReadingText,
    value: super::Font,
    prefix: super::Font,
    measure: &dyn Measure,
) -> f32 {
    let value_w = text_box_width(measure, &reading.value, value);
    match reading.prefix {
        Some(word) => value_w + text_box_width(measure, word, prefix) + measure.width(" ", prefix),
        None => value_w,
    }
}

/// Space around the content of a layout: above, at each side, and below.
#[derive(Clone, Copy, Debug)]
pub(super) struct ContentPad {
    pub top: f32,
    pub side: f32,
    pub bottom: f32,
}

/// The content area of the expanded layouts.
pub(super) const EXPANDED_PAD: ContentPad = ContentPad {
    top: CONTENT_PAD_TOP,
    side: CONTENT_PAD_SIDE,
    bottom: CONTENT_PAD_BOTTOM,
};

/// The empty state: one card with a faint line of text, in the content area
/// of the layout that shows it.
pub(super) fn empty(body: &mut Content, inner_width: f32, pad: ContentPad) {
    let width = inner_width - 2.0 * pad.side;
    let rect = Rect::new(pad.side, pad.top, width, 2.0 + 2.0 * EMPTY_PAD + EMPTY_LINE);
    body.shapes.push(card_frame(rect));
    body.shapes.push(text(
        Rect::new(rect.x, rect.y + 1.0 + EMPTY_PAD, rect.w, EMPTY_LINE),
        "No subscription snapshots",
        font(FontKind::Ui, 13.0, 400),
        Role::Faint,
        Align::Center,
        true,
    ));
    body.card_rects.push(rect);
    body.height = rect.bottom() + pad.bottom;
}

fn card_frame(rect: Rect) -> Shape {
    Shape::Rect {
        rect,
        radius: CARD_RADIUS,
        fill: Some(Role::Card),
        border: Some((Role::CardBorder, 1.0)),
    }
}

pub(super) fn build(
    body: &mut Content,
    cards: &[UsageCard],
    layout: Layout,
    inner_width: f32,
    scale: f32,
    measure: &dyn Measure,
) {
    let sizes = Sizes { scale };
    let columns = if layout == Layout::Columns { 2 } else { 1 };
    let area = inner_width - 2.0 * CONTENT_PAD_SIDE;
    let card_width = (area - CARD_GAP * (columns - 1) as f32) / columns as f32;
    let inner = card_width - 2.0 - 2.0 * CARD_PAD_SIDE;

    let value_font = font(FontKind::Mono, sizes.size(13.0), 600);
    let prefix_font = font(FontKind::Mono, sizes.size(12.0), 400);
    let reading_col = cards
        .iter()
        .flat_map(|card| &card.rows)
        .map(|row| {
            reading_width(
                &reading_text(&row.reading, row.severity),
                value_font,
                prefix_font,
                measure,
            )
        })
        .fold(MIN_READING_WIDTH, f32::max);

    let mut y = CONTENT_PAD_TOP;
    for grid_row in cards.chunks(columns) {
        let mut row_height: f32 = 0.0;
        for (column, card) in grid_row.iter().enumerate() {
            let x = CONTENT_PAD_SIDE + column as f32 * (card_width + CARD_GAP);
            let height = card_shapes(
                body,
                card,
                Rect::new(x, y, card_width, 0.0),
                inner,
                reading_col,
                &sizes,
                measure,
            );
            row_height = row_height.max(height);
        }
        y += row_height + CARD_GAP;
    }
    body.height = if cards.is_empty() {
        CONTENT_PAD_TOP + CONTENT_PAD_BOTTOM
    } else {
        y - CARD_GAP + CONTENT_PAD_BOTTOM
    };
}

/// Adds one card at `at` and returns its height.
fn card_shapes(
    body: &mut Content,
    card: &UsageCard,
    at: Rect,
    inner: f32,
    reading_col: f32,
    sizes: &Sizes,
    measure: &dyn Measure,
) -> f32 {
    let title_h = sizes.size(TITLE_HEIGHT);
    let row_h = sizes.size(ROW_HEIGHT);
    let ix = at.x + 1.0 + CARD_PAD_SIDE;

    // The frame is pushed first so it is drawn under the card's content,
    // and its height is set once the content is laid out.
    let frame_index = body.shapes.len();
    body.shapes.push(card_frame(at));

    let mut y = at.y + 1.0 + CARD_PAD_TOP;
    title_shapes(body, card, ix, y, inner, title_h, sizes, measure);
    y += title_h;
    for row in &card.rows {
        row_shapes(body, row, ix, y, inner, row_h, reading_col, sizes, measure);
        y += row_h;
    }
    y = notice_shapes(body, &card.notices, ix, y, inner, measure);

    let height = y + CARD_PAD_BOTTOM + 1.0 - at.y;
    let rect = Rect::new(at.x, at.y, at.w, height);
    body.shapes[frame_index] = card_frame(rect);
    body.card_rects.push(rect);
    height
}

#[allow(clippy::too_many_arguments)]
fn title_shapes(
    body: &mut Content,
    card: &UsageCard,
    ix: f32,
    y: f32,
    inner: f32,
    height: f32,
    sizes: &Sizes,
    measure: &dyn Measure,
) {
    let provider_font = font(FontKind::Mono, sizes.size(13.0), 700);
    let plan_font = font(FontKind::Mono, sizes.size(12.0), 400);
    let account_font = plan_font;
    let provider_w = text_box_width(measure, &card.title.provider, provider_font);
    let plan = card.title.plan.as_deref().filter(|plan| !plan.is_empty());
    let plan_w = plan.map(|plan| text_box_width(measure, plan, plan_font));
    let account_w = text_box_width(measure, &card.title.account, account_font);
    let fit = fit_title(inner, provider_w, plan_w, account_w);

    let mut shape = |x: f32, width: f32, value: &str, font, color, align| {
        if width > 0.0 {
            body.shapes.push(text(
                Rect::new(x, y, width, height),
                value,
                font,
                color,
                align,
                true,
            ));
        }
    };
    shape(
        ix,
        fit.provider,
        &card.title.provider,
        provider_font,
        Role::Accent,
        Align::Left,
    );
    if let Some(plan) = plan {
        shape(
            ix + fit.provider + COLUMN_GAP,
            fit.plan,
            plan,
            plan_font,
            Role::Sub,
            Align::Left,
        );
    }
    shape(
        ix + inner - fit.account,
        fit.account,
        &card.title.account,
        account_font,
        Role::Faint,
        Align::Right,
    );
}

#[allow(clippy::too_many_arguments)]
fn row_shapes(
    body: &mut Content,
    row: &UsageRow,
    ix: f32,
    y: f32,
    inner: f32,
    height: f32,
    reading_col: f32,
    sizes: &Sizes,
    measure: &dyn Measure,
) {
    let bar_x = ix + inner - BAR_WIDTH;
    let reset_right = bar_x - COLUMN_GAP;
    let reading_right = reset_right - RESET_WIDTH - COLUMN_GAP;
    let name_width = (reading_right - reading_col - COLUMN_GAP - ix).max(0.0);

    // Name, and the faint `off` after it.
    let name_font = font(FontKind::Mono, sizes.size(13.0), 400);
    let mut name_box = text_box_width(measure, &row.name, name_font).min(name_width);
    if row.off {
        let off = " off";
        let off_w = text_box_width(measure, off, name_font);
        name_box = text_box_width(measure, &row.name, name_font).min((name_width - off_w).max(0.0));
        if name_width > name_box {
            body.shapes.push(text(
                Rect::new(ix + name_box, y, (name_width - name_box).min(off_w), height),
                off,
                name_font,
                Role::Faint,
                Align::Left,
                false,
            ));
        }
    }
    body.shapes.push(text(
        Rect::new(ix, y, name_box, height),
        row.name.clone(),
        name_font,
        Role::Text,
        Align::Left,
        true,
    ));

    reading_shapes(body, row, reading_right, y, height, sizes, measure);
    reset_shapes(body, row, reset_right, y, height, sizes, measure);

    if let Some(Bar { filled, color }) = bar(row, BAR_CELLS) {
        let top = y + (height - CELL.1) / 2.0;
        for cell in 0..BAR_CELLS {
            let lit = cell >= BAR_CELLS - filled;
            body.shapes.push(Shape::Rect {
                rect: Rect::new(
                    bar_x + f32::from(cell) * (CELL.0 + CELL_GAP),
                    top,
                    CELL.0,
                    CELL.1,
                ),
                radius: 1.5,
                fill: Some(if lit { color } else { Role::Track }),
                border: None,
            });
        }
    }
}

fn reading_shapes(
    body: &mut Content,
    row: &UsageRow,
    right: f32,
    y: f32,
    height: f32,
    sizes: &Sizes,
    measure: &dyn Measure,
) {
    let reading = reading_text(&row.reading, row.severity);
    let value_font = font(FontKind::Mono, sizes.size(13.0), 600);
    let prefix_font = font(FontKind::Mono, sizes.size(12.0), 400);
    let value_w = text_box_width(measure, &reading.value, value_font);
    body.shapes.push(text(
        Rect::new(right - value_w, y, value_w, height),
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
            Rect::new(right - value_w - space - prefix_w, y, prefix_w, height),
            prefix,
            prefix_font,
            Role::Faint,
            Align::Right,
            false,
        ));
    }
}

fn reset_shapes(
    body: &mut Content,
    row: &UsageRow,
    right: f32,
    y: f32,
    height: f32,
    sizes: &Sizes,
    measure: &dyn Measure,
) {
    let Some(seconds) = row.resets_in else {
        return;
    };
    let mid = y + height / 2.0;
    match reset_view(seconds) {
        ResetView::Countdown(label) => {
            let label_font = font(FontKind::Mono, sizes.size(12.0), 400);
            let width = text_box_width(measure, &label, label_font);
            body.shapes.push(text(
                Rect::new(right - width, y, width, height),
                label,
                label_font,
                Role::Sub,
                Align::Right,
                false,
            ));
        }
        ResetView::Days(dots) => {
            let total = dots.len() as f32 * DOT_DIAMETER + (dots.len() - 1) as f32 * DOT_GAP;
            let start = right - total;
            for (index, dot) in dots.iter().enumerate() {
                let (color, fill) = match dot {
                    DayDot::Spent => (Role::Track, DotFill::Empty),
                    DayDot::Left => (Role::Sub, DotFill::Full),
                    DayDot::UnderTwoHours => (Role::Sub, DotFill::Empty),
                    DayDot::UnderSixHours => (Role::Sub, DotFill::Quarter),
                    DayDot::UpToSixteenHours => (Role::Sub, DotFill::ThreeQuarters),
                };
                body.shapes.push(Shape::Dot {
                    cx: start + index as f32 * (DOT_DIAMETER + DOT_GAP) + DOT_DIAMETER / 2.0,
                    cy: mid,
                    diameter: DOT_DIAMETER,
                    border: DOT_BORDER,
                    color,
                    fill,
                });
            }
        }
        ResetView::Far(days) => {
            let label = format!("{days}d");
            // The card's own font, at the size the user chose, like the
            // other text in the reset column.
            let label_font = font(FontKind::Mono, sizes.size(11.0), 400);
            let width = measure.width(&label, label_font).ceil() + 16.0;
            let height = sizes.size(18.0);
            let pill = Rect::new(right - width, mid - height / 2.0, width, height);
            body.shapes.push(Shape::Rect {
                rect: pill,
                radius: height / 2.0,
                fill: Some(Role::Pill),
                border: None,
            });
            body.shapes.push(text(
                pill,
                label,
                label_font,
                Role::Sub,
                Align::Center,
                false,
            ));
        }
    }
}

/// Lays the tags out in a flow under the rows and returns the new bottom.
fn notice_shapes(
    body: &mut Content,
    notices: &[UsageNotice],
    ix: f32,
    mut y: f32,
    inner: f32,
    measure: &dyn Measure,
) -> f32 {
    if notices.is_empty() {
        return y;
    }
    let label_font = font(FontKind::Ui, 12.0, 400);
    y += TAG_TOP;
    let mut x = ix;
    for notice in notices {
        let label = notice_text(notice.to_owned());
        let label_w = text_box_width(measure, &label, label_font);
        let width = 8.0 + 12.0 + 6.0 + label_w + 8.0;
        if x > ix && x + width > ix + inner {
            x = ix;
            y += TAG_HEIGHT + TAG_GAP;
        }
        let tag = Rect::new(x, y, width, TAG_HEIGHT);
        body.shapes.push(Shape::Rect {
            rect: tag,
            radius: 4.0,
            fill: Some(Role::WarnBg),
            border: None,
        });
        body.shapes.push(Shape::Icon {
            icon: Icon::Warning,
            rect: Rect::new(x + 8.0, y + (TAG_HEIGHT - 12.0) / 2.0, 12.0, 12.0),
            color: Role::WarnText,
        });
        body.shapes.push(text(
            Rect::new(x + 8.0 + 12.0 + 6.0, y, label_w, TAG_HEIGHT),
            label,
            label_font,
            Role::WarnText,
            Align::Left,
            false,
        ));
        x += width + TAG_GAP;
    }
    y + TAG_HEIGHT
}

#[cfg(test)]
pub(crate) mod tests;
