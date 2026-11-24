//! UI-side view of one instrument. The wire data comes straight from
//! `fsrsxp-orderbook-proto`; this module adds the computed-field helpers
//! (best bid/ask, spread, cumulative totals) and the formatters that
//! `views.rs` needs, as an extension trait on `MarketSnapshot`.
//!
//! The old simulator that lived here moved into `crates/feed/src/sim.rs`.
//! `Market` is now a pure view model fed by the WebSocket client.

pub use fsrsxp_orderbook_proto::{Level, MarketSnapshot, Side, Trade};

pub type Market = MarketSnapshot;

pub trait MarketExt {
    fn best_bid(&self) -> Option<f64>;
    fn best_ask(&self) -> Option<f64>;
    fn mid(&self) -> f64;
    fn spread(&self) -> f64;
    fn change_pct(&self) -> f64;
    fn cumulative_bid_totals(&self) -> Vec<f64>;
    fn cumulative_ask_totals(&self) -> Vec<f64>;
}

impl MarketExt for Market {
    fn best_bid(&self) -> Option<f64> {
        self.bids.first().map(|l| l.price)
    }
    fn best_ask(&self) -> Option<f64> {
        self.asks.first().map(|l| l.price)
    }
    fn mid(&self) -> f64 {
        match (self.best_bid(), self.best_ask()) {
            (Some(b), Some(a)) => (b + a) / 2.0,
            _ => 0.0,
        }
    }
    fn spread(&self) -> f64 {
        match (self.best_bid(), self.best_ask()) {
            (Some(b), Some(a)) => a - b,
            _ => 0.0,
        }
    }
    fn change_pct(&self) -> f64 {
        if self.open_price == 0.0 {
            0.0
        } else {
            (self.last_price - self.open_price) / self.open_price * 100.0
        }
    }
    fn cumulative_bid_totals(&self) -> Vec<f64> {
        let mut out = Vec::with_capacity(self.bids.len());
        let mut acc = 0.0;
        for l in &self.bids {
            acc += l.size;
            out.push(acc);
        }
        out
    }
    fn cumulative_ask_totals(&self) -> Vec<f64> {
        let mut out = Vec::with_capacity(self.asks.len());
        let mut acc = 0.0;
        for l in &self.asks {
            acc += l.size;
            out.push(acc);
        }
        out
    }
}

/// Format a ms-since-epoch value as `HH:MM:SS.mmm` in UTC.
pub fn format_time_ms(ts_ms: u64) -> String {
    let secs = ts_ms / 1000;
    let ms = ts_ms % 1000;
    let hh = (secs / 3600) % 24;
    let mm = (secs / 60) % 60;
    let ss = secs % 60;
    format!("{:02}:{:02}:{:02}.{:03}", hh, mm, ss, ms)
}

pub fn format_price(p: f64) -> String {
    // Rust's `format!` has no thousands separator, so group the whole part by hand.
    let cents = (p * 100.0).round() as i64;
    let sign = if cents < 0 { "-" } else { "" };
    let abs = cents.unsigned_abs() as u128;
    let whole = abs / 100;
    let frac = abs % 100;
    let whole_s = whole.to_string();
    let bytes = whole_s.as_bytes();
    let n = bytes.len();
    let mut grouped = String::with_capacity(n + n / 3);
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && (n - i) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(*b as char);
    }
    format!("{sign}{grouped}.{frac:02}")
}

pub fn format_size(s: f64) -> String {
    format!("{:.4}", s)
}
