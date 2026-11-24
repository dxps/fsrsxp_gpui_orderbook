//! Wire types shared by the orderbook feed service and the GPUI UI.
//!
//! These are the *only* types that cross the WebSocket and the `/ui-config`
//! boundary. Keep them small, serde-able, and free of behaviour — the UI
//! attaches its own view helpers, the feed its own simulation.

use serde::{Deserialize, Serialize};

/// Trade aggressor side. `Buy` means a market buy lifted an ask; `Sell` means
/// a market sell hit a bid.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Buy,
    Sell,
}

/// One price level on a book side.
#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub struct Level {
    pub price: f64,
    pub size:  f64,
}

/// One print on the tape.
#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub struct Trade {
    pub price: f64,
    pub size:  f64,
    pub side:  Side,
    pub ts_ms: u64,
}

/// Full snapshot of one instrument's state at a point in time. The feed
/// service broadcasts one of these per tick; the UI treats each as the new
/// truth (no delta reconstruction needed at this stage).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MarketSnapshot {
    pub symbol:      String,
    pub bids:        Vec<Level>,
    pub asks:        Vec<Level>,
    pub trades:      Vec<Trade>,
    pub last_price:  f64,
    pub open_price:  f64,
    pub ticks:       u64,
    pub trade_count: u64,
}

/// Returned by `GET /ui-config`. The UI fetches this at init time, then opens
/// the WebSocket at the given URL. Keeping it a dedicated endpoint means the
/// UI can be deployed as a static bundle and still find a configurable
/// backend (dev, staging, prod).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UiConfig {
    pub ws_url: String,
}
