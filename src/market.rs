use rand::Rng;
use std::collections::VecDeque;
use std::time::{SystemTime, UNIX_EPOCH};

const LEVELS_PER_SIDE: usize = 20;
const TICK: f64 = 0.5;
const TRADE_WINDOW: usize = 24;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Side {
    Buy,
    Sell,
}

#[derive(Copy, Clone, Debug)]
pub struct Level {
    pub price: f64,
    pub size: f64,
}

#[derive(Copy, Clone, Debug)]
pub struct Trade {
    pub price: f64,
    pub size: f64,
    pub side: Side,
    pub ts_ms: u64,
}

/// Simulated L2 orderbook for one instrument. Deliberately self-contained so
/// the dashboard has no external dependency — a real feed drops in as a
/// different producer of the same mutations.
pub struct Market {
    pub symbol: &'static str,
    pub bids: Vec<Level>,
    pub asks: Vec<Level>,
    pub trades: VecDeque<Trade>,
    pub last_price: f64,
    pub open_price: f64,
    pub ref_price: f64,
    pub ticks: u64,
    pub trade_count: u64,
}

impl Market {
    pub fn new() -> Self {
        let ref_price = 64_982.50;
        let mut m = Self {
            symbol: "BTC-USD",
            bids: Vec::with_capacity(LEVELS_PER_SIDE),
            asks: Vec::with_capacity(LEVELS_PER_SIDE),
            trades: VecDeque::with_capacity(TRADE_WINDOW),
            last_price: ref_price,
            open_price: ref_price * 0.9860,
            ref_price,
            ticks: 0,
            trade_count: 0,
        };
        m.rebuild_book();
        m
    }

    pub fn best_bid(&self) -> Option<f64> {
        self.bids.first().map(|l| l.price)
    }

    pub fn best_ask(&self) -> Option<f64> {
        self.asks.first().map(|l| l.price)
    }

    pub fn mid(&self) -> f64 {
        match (self.best_bid(), self.best_ask()) {
            (Some(b), Some(a)) => (b + a) / 2.0,
            _ => self.ref_price,
        }
    }

    pub fn spread(&self) -> f64 {
        match (self.best_bid(), self.best_ask()) {
            (Some(b), Some(a)) => a - b,
            _ => 0.0,
        }
    }

    pub fn change_pct(&self) -> f64 {
        if self.open_price == 0.0 {
            0.0
        } else {
            (self.last_price - self.open_price) / self.open_price * 100.0
        }
    }

    pub fn cumulative_bid_totals(&self) -> Vec<f64> {
        let mut out = Vec::with_capacity(self.bids.len());
        let mut acc = 0.0;
        for l in &self.bids {
            acc += l.size;
            out.push(acc);
        }
        out
    }

    pub fn cumulative_ask_totals(&self) -> Vec<f64> {
        let mut out = Vec::with_capacity(self.asks.len());
        let mut acc = 0.0;
        for l in &self.asks {
            acc += l.size;
            out.push(acc);
        }
        out
    }

    /// Rebuild both sides of the book around `ref_price`.
    fn rebuild_book(&mut self) {
        let mut rng = rand::thread_rng();
        self.bids.clear();
        self.asks.clear();

        for i in 0..LEVELS_PER_SIDE {
            let price = self.ref_price - TICK * (i as f64 + 1.0);
            let base = rng.gen_range(0.05..5.0);
            let taper = (1.5 - i as f64 * 0.02).max(0.3);
            self.bids.push(Level {
                price,
                size: round4(base * taper),
            });
        }
        for i in 0..LEVELS_PER_SIDE {
            let price = self.ref_price + TICK * (i as f64 + 1.0);
            let base = rng.gen_range(0.05..5.0);
            let taper = (1.5 - i as f64 * 0.02).max(0.3);
            self.asks.push(Level {
                price,
                size: round4(base * taper),
            });
        }
    }

    /// Advance one simulation step. Called by the Dashboard's background
    /// timer at a steady cadence.
    pub fn tick(&mut self) {
        let mut rng = rand::thread_rng();
        self.ticks = self.ticks.wrapping_add(1);

        // Occasional small reference-price drift → relayer book.
        if rng.gen_bool(0.08) {
            let step = rng.gen_range(-0.5..=0.5);
            self.ref_price = (self.ref_price + step).max(1.0);
            self.rebuild_book();
        } else {
            // Perturb a handful of levels so the UI sees motion every frame.
            for _ in 0..3 {
                let hit_bid = rng.gen_bool(0.5);
                let book = if hit_bid { &mut self.bids } else { &mut self.asks };
                if book.is_empty() {
                    continue;
                }
                let i = rng.gen_range(0..book.len());
                let delta = rng.gen_range(-0.6..0.7);
                book[i].size = round4((book[i].size + delta).max(0.01));
            }
        }

        // Prints: a trade crosses the spread with some probability.
        if rng.gen_bool(0.35) {
            let buy = rng.gen_bool(0.5);
            let (price, side) = if buy {
                (
                    self.best_ask().unwrap_or(self.ref_price),
                    Side::Buy,
                )
            } else {
                (
                    self.best_bid().unwrap_or(self.ref_price),
                    Side::Sell,
                )
            };
            let size = round4(rng.gen_range(0.01..0.30));
            self.last_price = price;
            self.trade_count = self.trade_count.wrapping_add(1);
            let ts_ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            self.trades.push_front(Trade {
                price,
                size,
                side,
                ts_ms,
            });
            while self.trades.len() > TRADE_WINDOW {
                self.trades.pop_back();
            }
        }
    }
}

fn round4(x: f64) -> f64 {
    (x * 10_000.0).round() / 10_000.0
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
