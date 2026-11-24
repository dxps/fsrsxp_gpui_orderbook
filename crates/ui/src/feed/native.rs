//! Native feed client. One worker OS thread does:
//!   1. Blocking `GET /ui-config` via reqwest::blocking → `UiConfig { ws_url }`.
//!   2. Blocking `tungstenite::connect(ws_url)` → WebSocket.
//!   3. Loop: read text frame, deserialize `MarketSnapshot`, send down channel.
//!
//! GPUI polls `try_latest()` from `render()` / a background timer and never
//! blocks. No tokio in the UI binary; the only new dep tungstenite adds is a
//! small sync websocket implementation.

use super::ConnStatus;
use fsrsxp_orderbook_proto::{MarketSnapshot, UiConfig};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::thread;

pub struct FeedClient {
    snaps:    Receiver<MarketSnapshot>,
    statuses: Receiver<ConnStatus>,
    status:   ConnStatus,
    _worker:  thread::JoinHandle<()>,
}

impl FeedClient {
    pub fn connect(config_url: String) -> Self {
        let (snap_tx, snaps) = channel::<MarketSnapshot>();
        let (status_tx, statuses) = channel::<ConnStatus>();
        let worker = thread::Builder::new()
            .name("orderbook-feed-client".into())
            .spawn(move || run(config_url, snap_tx, status_tx))
            .expect("failed to spawn feed-client thread");
        Self {
            snaps,
            statuses,
            status: ConnStatus::Connecting,
            _worker: worker,
        }
    }

    /// Drain pending snapshots + status updates. Returns the most recent
    /// snapshot (older ones are dropped — no point painting a stale book).
    pub fn try_latest(&mut self) -> Option<MarketSnapshot> {
        while let Ok(s) = self.statuses.try_recv() {
            self.status = s;
        }
        let mut latest = None;
        loop {
            match self.snaps.try_recv() {
                Ok(s) => latest = Some(s),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.status = ConnStatus::Disconnected;
                    break;
                }
            }
        }
        latest
    }

    pub fn status(&self) -> ConnStatus {
        self.status
    }
}

fn run(config_url: String, snaps: Sender<MarketSnapshot>, statuses: Sender<ConnStatus>) {
    // /ui-config
    let config: UiConfig = match reqwest::blocking::get(&config_url) {
        Ok(resp) => match resp.json() {
            Ok(c) => c,
            Err(err) => {
                eprintln!("feed: /ui-config decode: {err}");
                let _ = statuses.send(ConnStatus::Disconnected);
                return;
            }
        },
        Err(err) => {
            eprintln!("feed: /ui-config GET {config_url} failed: {err}");
            let _ = statuses.send(ConnStatus::Disconnected);
            return;
        }
    };

    // WebSocket
    let (mut ws, _resp) = match tungstenite::connect(&config.ws_url) {
        Ok(x) => x,
        Err(err) => {
            eprintln!("feed: ws connect {} failed: {err}", config.ws_url);
            let _ = statuses.send(ConnStatus::Disconnected);
            return;
        }
    };
    let _ = statuses.send(ConnStatus::Connected);

    loop {
        match ws.read() {
            Ok(tungstenite::Message::Text(t)) => {
                // `to_string` works across tungstenite API versions (0.21 had
                // Text(String), 0.24 has Text(Utf8Bytes)).
                let s = t.to_string();
                if let Ok(snap) = serde_json::from_str::<MarketSnapshot>(&s) {
                    if snaps.send(snap).is_err() {
                        return; // UI gone
                    }
                }
            }
            Ok(tungstenite::Message::Close(_)) => {
                let _ = statuses.send(ConnStatus::Disconnected);
                return;
            }
            // Ignore everything else (pings/pongs/binary/raw frames).
            Ok(_) => continue,
            Err(err) => {
                eprintln!("feed: ws read: {err}");
                let _ = statuses.send(ConnStatus::Disconnected);
                return;
            }
        }
    }
}
