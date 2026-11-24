//! Simulated L2 orderbook producer. The same arithmetic the old UI-side
//! `Market::tick` used, now behind a `snapshot()` that hands out immutable
//! wire types.

use fsrsxp_orderbook_proto::{Level, MarketSnapshot, Side, Trade};
use rand::Rng;
use std::collections::VecDeque;
use std::time::{SystemTime, UNIX_EPOCH};

const LEVELS_PER_SIDE: usize = 20;
const TICK: f64 = 0.5;
const TRADE_WINDOW: usize = 24;

pub struct Simulator {
    symbol:      String,
    bids:        Vec<Level>,
    asks:        Vec<Level>,
    trades:      VecDeque<Trade>,
    last_price:  f64,
    open_price:  f64,
    ref_price:   f64,
    ticks:       u64,
    trade_count: u64,
}

impl Simulator {
    pub fn new() -> Self {
        let ref_price = 64_982.50;
        let mut s = Self {
            symbol:      "BTC-USD".into(),
            bids:        Vec::with_capacity(LEVELS_PER_SIDE),
            asks:        Vec::with_capacity(LEVELS_PER_SIDE),
            trades:      VecDeque::with_capacity(TRADE_WINDOW),
            last_price:  ref_price,
            open_price:  ref_price * 0.9860,
            ref_price,
            ticks:       0,
            trade_count: 0,
        };
        s.rebuild_book();
        s
    }

    pub fn snapshot(&self) -> MarketSnapshot {
        MarketSnapshot {
            symbol:      self.symbol.clone(),
            bids:        self.bids.clone(),
            asks:        self.asks.clone(),
            trades:      self.trades.iter().copied().collect(),
            last_price:  self.last_price,
            open_price:  self.open_price,
            ticks:       self.ticks,
            trade_count: self.trade_count,
        }
    }

    pub fn tick(&mut self) {
        let mut rng = rand::thread_rng();
        self.ticks = self.ticks.wrapping_add(1);

        // Occasional reference-price drift → whole book relays.
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
                    self.asks.first().map(|l| l.price).unwrap_or(self.ref_price),
                    Side::Buy,
                )
            } else {
                (
                    self.bids.first().map(|l| l.price).unwrap_or(self.ref_price),
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
}

fn round4(x: f64) -> f64 {
    (x * 10_000.0).round() / 10_000.0
}
