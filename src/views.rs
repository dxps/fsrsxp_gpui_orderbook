//! Rendering pieces for the orderbook dashboard. Everything is a pure function
//! of a `Market` snapshot — the Dashboard owns the mutable state and re-renders
//! the whole tree each tick. GPUI diffs the paint tree, so this is cheap.

use crate::market::{format_price, format_size, format_time_ms, Market, Side};
use gpui::{
    canvas, div, point, px, rgb, FontWeight, Hsla, IntoElement, ParentElement, PathBuilder,
    SharedString, Styled,
};

// ─────────────────────────── palette ──────────────────────────────────

pub const BG_PAGE: u32 = 0x0A0E1A;
pub const BG_PANEL: u32 = 0x121829;
pub const BG_ROW_ALT: u32 = 0x161E30;
pub const BG_MID: u32 = 0x1B243A;
pub const BG_HEADER: u32 = 0x0F1420;
pub const BORDER: u32 = 0x1E2837;

pub const TEXT_PRIMARY: u32 = 0xE8ECF1;
pub const TEXT_MUTED: u32 = 0x7C8599;
pub const TEXT_DIM: u32 = 0x4F5A72;

pub const BID_GREEN: u32 = 0x26A69A;
pub const ASK_RED: u32 = 0xEF5350;
pub const BID_GREEN_SOFT: u32 = 0x1E3A3A; // depth-bar fill, low contrast
pub const ASK_RED_SOFT: u32 = 0x3A1F22;
pub const ACCENT: u32 = 0xFFB400;

// ─────────────────────────── header ───────────────────────────────────

pub fn header(market: &Market) -> impl IntoElement {
    let dot = div()
        .w(px(8.))
        .h(px(8.))
        .rounded_full()
        .bg(rgb(BID_GREEN));

    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_4()
        .px_4()
        .py_3()
        .bg(rgb(BG_HEADER))
        .border_b_1()
        .border_color(rgb(BORDER))
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(dot)
                .child(
                    div()
                        .text_color(rgb(TEXT_PRIMARY))
                        .font_weight(FontWeight::BOLD)
                        .child(SharedString::from(market.symbol)),
                ),
        )
        .child(stat_block(
            "LAST",
            format_price(market.last_price),
            TEXT_PRIMARY,
        ))
        .child(change_block(market.change_pct()))
        .child(stat_block(
            "BID",
            format_price(market.best_bid().unwrap_or(0.0)),
            BID_GREEN,
        ))
        .child(stat_block(
            "ASK",
            format_price(market.best_ask().unwrap_or(0.0)),
            ASK_RED,
        ))
        .child(stat_block(
            "SPREAD",
            format!("{:.2}", market.spread()),
            TEXT_MUTED,
        ))
        .child(stat_block(
            "MID",
            format_price(market.mid()),
            TEXT_PRIMARY,
        ))
}

fn stat_block(label: &'static str, value: impl Into<SharedString>, value_color: u32) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_0p5()
        .min_w(px(96.))
        .child(
            div()
                .text_xs()
                .text_color(rgb(TEXT_DIM))
                .child(SharedString::from(label)),
        )
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(value_color))
                .child(value.into()),
        )
}

/// 24-hour change stat with a vector up/down triangle. Drawn through
/// `PathBuilder::fill()` so it doesn't rely on any glyph being present in the
/// current font — the web bundle only carries IBM Plex Sans Regular, which
/// skips the Geometric Shapes block (▲/▼).
fn change_block(change: f64) -> impl IntoElement {
    let up = change >= 0.0;
    let color_u32 = if up { BID_GREEN } else { ASK_RED };
    let color: Hsla = rgb(color_u32).into();

    div()
        .flex()
        .flex_col()
        .gap_0p5()
        .min_w(px(96.))
        .child(
            div()
                .text_xs()
                .text_color(rgb(TEXT_DIM))
                .child(SharedString::from("24H")),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_1()
                .child(triangle(up, color))
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(color)
                        .child(format!("{:.2}%", change.abs())),
                ),
        )
}

/// Small filled triangle (▲/▼ equivalent) painted as a vector path.
fn triangle(up: bool, color: Hsla) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let center = bounds.center();
            let (half_w, half_h) = (px(4.0), px(3.5));
            let mut path = PathBuilder::fill();
            if up {
                path.move_to(point(center.x, center.y - half_h));
                path.line_to(point(center.x + half_w, center.y + half_h));
                path.line_to(point(center.x - half_w, center.y + half_h));
            } else {
                path.move_to(point(center.x - half_w, center.y - half_h));
                path.line_to(point(center.x + half_w, center.y - half_h));
                path.line_to(point(center.x, center.y + half_h));
            }
            path.close();
            if let Ok(path) = path.build() {
                window.paint_path(path, color);
            }
        },
    )
    .w(px(10.))
    .h(px(10.))
    .flex_none()
}

// ─────────────────────────── orderbook panel ───────────────────────────

const COL_W_PRICE: f32 = 100.;
const COL_W_SIZE: f32 = 90.;
const COL_W_TOTAL: f32 = 90.;
const ROW_H: f32 = 20.;

pub fn orderbook_panel(market: &Market) -> impl IntoElement {
    let bid_totals = market.cumulative_bid_totals();
    let ask_totals = market.cumulative_ask_totals();
    let max_bid = bid_totals.last().copied().unwrap_or(1.0).max(1e-9);
    let max_ask = ask_totals.last().copied().unwrap_or(1.0).max(1e-9);

    // Asks: display worst (furthest from mid) at top, best right above mid.
    // Iterate asks[LEVELS-1..=0] i.e. reversed, so best ends up at the bottom.
    let ask_rows = market
        .asks
        .iter()
        .enumerate()
        .rev()
        .map(|(ix, level)| {
            let total = ask_totals[ix];
            depth_row(
                level.price,
                level.size,
                total,
                total / max_ask,
                Side::Sell,
            )
        });

    let bid_rows = market
        .bids
        .iter()
        .enumerate()
        .map(|(ix, level)| {
            let total = bid_totals[ix];
            depth_row(
                level.price,
                level.size,
                total,
                total / max_bid,
                Side::Buy,
            )
        });

    div()
        .flex()
        .flex_col()
        .flex_1()
        .h_full()
        .bg(rgb(BG_PANEL))
        .border_r_1()
        .border_color(rgb(BORDER))
        .child(panel_title("ORDER BOOK", market.bids.len() + market.asks.len()))
        .child(book_header_row())
        .child(div().flex().flex_col().children(ask_rows))
        .child(mid_band(market))
        .child(div().flex().flex_col().children(bid_rows))
}

fn panel_title(label: &'static str, count: usize) -> impl IntoElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .px_4()
        .py_2()
        .bg(rgb(BG_HEADER))
        .border_b_1()
        .border_color(rgb(BORDER))
        .child(
            div()
                .text_xs()
                .font_weight(FontWeight::BOLD)
                .text_color(rgb(TEXT_PRIMARY))
                .child(SharedString::from(label)),
        )
        .child(
            div()
                .text_xs()
                .text_color(rgb(TEXT_DIM))
                .child(format!("{} levels", count)),
        )
}

fn book_header_row() -> impl IntoElement {
    div()
        .flex()
        .flex_row()
        .px_4()
        .py_1()
        .bg(rgb(BG_HEADER))
        .border_b_1()
        .border_color(rgb(BORDER))
        .text_xs()
        .text_color(rgb(TEXT_DIM))
        .child(
            div()
                .w(px(COL_W_PRICE))
                .child(SharedString::from("PRICE")),
        )
        .child(
            div()
                .w(px(COL_W_SIZE))
                .flex()
                .justify_end()
                .child(SharedString::from("SIZE")),
        )
        .child(
            div()
                .w(px(COL_W_TOTAL))
                .flex()
                .justify_end()
                .child(SharedString::from("TOTAL")),
        )
}

/// One orderbook row with a depth bar anchored to the right.
fn depth_row(price: f64, size: f64, total: f64, depth_ratio: f64, side: Side) -> impl IntoElement {
    let (text_c, bar_c) = match side {
        Side::Buy => (BID_GREEN, BID_GREEN_SOFT),
        Side::Sell => (ASK_RED, ASK_RED_SOFT),
    };
    let bar_w_pct = (depth_ratio.clamp(0.0, 1.0) * 100.0) as f32;

    let bar = div()
        .absolute()
        .top_0()
        .bottom_0()
        .right_0()
        .w(gpui::relative(bar_w_pct / 100.0))
        .bg(rgb(bar_c));

    let content = div()
        .flex()
        .flex_row()
        .items_center()
        .px_4()
        .h(px(ROW_H))
        .text_xs()
        .child(
            div()
                .w(px(COL_W_PRICE))
                .text_color(rgb(text_c))
                .font_weight(FontWeight::MEDIUM)
                .child(format_price(price)),
        )
        .child(
            div()
                .w(px(COL_W_SIZE))
                .flex()
                .justify_end()
                .text_color(rgb(TEXT_PRIMARY))
                .child(format_size(size)),
        )
        .child(
            div()
                .w(px(COL_W_TOTAL))
                .flex()
                .justify_end()
                .text_color(rgb(TEXT_MUTED))
                .child(format_size(total)),
        );

    div().relative().w_full().child(bar).child(content)
}

fn mid_band(market: &Market) -> impl IntoElement {
    let mid = market.mid();
    let spread = market.spread();
    div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .px_4()
        .h(px(28.))
        .bg(rgb(BG_MID))
        .border_t_1()
        .border_b_1()
        .border_color(rgb(BORDER))
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::BOLD)
                .text_color(rgb(ACCENT))
                .child(format_price(mid)),
        )
        .child(
            div()
                .text_xs()
                .text_color(rgb(TEXT_MUTED))
                .child(format!("spread {:.2}", spread)),
        )
}

// ─────────────────────────── trades panel ──────────────────────────────

pub fn trades_panel(market: &Market) -> impl IntoElement {
    let rows = market.trades.iter().enumerate().map(|(ix, trade)| {
        let color = match trade.side {
            Side::Buy => BID_GREEN,
            Side::Sell => ASK_RED,
        };
        let bg = if ix % 2 == 0 { BG_PANEL } else { BG_ROW_ALT };

        div()
            .flex()
            .flex_row()
            .items_center()
            .px_4()
            .h(px(ROW_H))
            .bg(rgb(bg))
            .text_xs()
            .child(
                div()
                    .w(px(COL_W_PRICE))
                    .text_color(rgb(color))
                    .font_weight(FontWeight::MEDIUM)
                    .child(format_price(trade.price)),
            )
            .child(
                div()
                    .w(px(COL_W_SIZE))
                    .flex()
                    .justify_end()
                    .text_color(rgb(TEXT_PRIMARY))
                    .child(format_size(trade.size)),
            )
            .child(
                div()
                    .flex_1()
                    .flex()
                    .justify_end()
                    .text_color(rgb(TEXT_DIM))
                    .child(format_time_ms(trade.ts_ms)),
            )
    });

    div()
        .flex()
        .flex_col()
        .w(px(360.))
        .h_full()
        .bg(rgb(BG_PANEL))
        .child(panel_title("RECENT TRADES", market.trades.len()))
        .child(trades_header_row())
        .child(div().flex().flex_col().children(rows))
}

fn trades_header_row() -> impl IntoElement {
    div()
        .flex()
        .flex_row()
        .px_4()
        .py_1()
        .bg(rgb(BG_HEADER))
        .border_b_1()
        .border_color(rgb(BORDER))
        .text_xs()
        .text_color(rgb(TEXT_DIM))
        .child(
            div()
                .w(px(COL_W_PRICE))
                .child(SharedString::from("PRICE")),
        )
        .child(
            div()
                .w(px(COL_W_SIZE))
                .flex()
                .justify_end()
                .child(SharedString::from("SIZE")),
        )
        .child(
            div()
                .flex_1()
                .flex()
                .justify_end()
                .child(SharedString::from("TIME")),
        )
}

// ─────────────────────────── status bar ────────────────────────────────

pub fn status_bar(market: &Market, render_count: u64) -> impl IntoElement {
    let dot = div()
        .w(px(6.))
        .h(px(6.))
        .rounded_full()
        .bg(rgb(BID_GREEN));

    div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .px_4()
        .py_1()
        .bg(rgb(BG_HEADER))
        .border_t_1()
        .border_color(rgb(BORDER))
        .text_xs()
        .text_color(rgb(TEXT_DIM))
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(dot)
                .child(SharedString::from("SIM"))
                .child(format!(
                    "· {} ticks · {} prints · frame {}",
                    market.ticks, market.trade_count, render_count
                )),
        )
        .child(SharedString::from("Zed GPUI · gpui-pre 0.3.14"))
}
