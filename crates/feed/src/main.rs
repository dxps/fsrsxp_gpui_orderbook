//! Axum feed service for the GPUI orderbook dashboard.
//!
//! One simulator ticks at 10 Hz on a background task and broadcasts each
//! snapshot through a `tokio::sync::broadcast` channel. Every connected
//! WebSocket is just a subscriber, so adding or dropping clients doesn't
//! affect the tick cadence.
//!
//! Endpoints
//! ---------
//! `GET /ui-config` — returns `{ "ws_url": "ws://<Host>/feed" }`, using the
//! incoming `Host` header so the same binary works whether the UI hits it
//! directly (127.0.0.1:7878) or through Trunk's dev proxy (127.0.0.1:8080).
//!
//! `GET /feed` — WebSocket upgrade; server pushes one JSON-encoded
//! `MarketSnapshot` per tick.

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::{IntoResponse, Json},
    routing::get,
    Router,
};
use fsrsxp_orderbook_proto::{MarketSnapshot, UiConfig};
use std::{net::SocketAddr, time::Duration};
use tokio::sync::broadcast;
use tower_http::cors::CorsLayer;

mod sim;

const BIND_ADDR: &str = "127.0.0.1:7878";
const TICK_PERIOD: Duration = Duration::from_millis(100);
/// Enough buffer that a slow client can fall behind a few ticks before the
/// broadcast channel starts dropping messages on it.
const BROADCAST_BUFFER: usize = 32;

#[derive(Clone)]
struct AppState {
    tx:     broadcast::Sender<MarketSnapshot>,
    ws_url: String,
}

#[tokio::main]
async fn main() {
    let addr: SocketAddr = BIND_ADDR.parse().expect("invalid bind address");
    let (tx, _initial_rx) = broadcast::channel::<MarketSnapshot>(BROADCAST_BUFFER);

    // Central ticker — one simulator shared by every subscriber.
    let tx_tick = tx.clone();
    tokio::spawn(async move {
        let mut sim = sim::Simulator::new();
        let mut interval = tokio::time::interval(TICK_PERIOD);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            interval.tick().await;
            sim.tick();
            // `send` errors only when there are zero receivers, which is the
            // normal "nobody connected yet" state. Ignore.
            let _ = tx_tick.send(sim.snapshot());
        }
    });

    // WebSocket URL the service hands to the UI via /ui-config.
    //
    // Default is derived from the backend's own bind address, which means the
    // browser connects WS directly to the backend — cross-origin from Trunk's
    // dev server, which is fine (WebSockets don't require CORS for the
    // handshake). Set OB_PUBLIC_WS_URL to point at a reverse-proxied URL in
    // production.
    //
    // Trusting the incoming Host header is tempting but misleading: Trunk's
    // dev-server proxy rewrites it to `127.0.0.1` (no port), which gives the
    // browser a URL it can't connect to.
    let ws_url = std::env::var("OB_PUBLIC_WS_URL")
        .unwrap_or_else(|_| format!("ws://{addr}/feed"));

    let state = AppState { tx, ws_url };
    let app = Router::new()
        .route("/ui-config", get(ui_config))
        .route("/feed", get(feed_ws))
        // CORS permissive because the Trunk dev server runs on a different
        // port in development. Lock this down behind a real deployment.
        .layer(CorsLayer::permissive())
        .with_state(state);

    println!("orderbook-feed listening on http://{addr}  (ws /feed)");
    let listener = tokio::net::TcpListener::bind(addr).await.expect("bind");
    axum::serve(listener, app).await.expect("serve");
}

async fn ui_config(State(state): State<AppState>) -> Json<UiConfig> {
    Json(UiConfig {
        ws_url: state.ws_url.clone(),
    })
}

async fn feed_ws(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: AppState) {
    let mut rx = state.tx.subscribe();
    loop {
        match rx.recv().await {
            Ok(snap) => {
                let Ok(text) = serde_json::to_string(&snap) else {
                    continue;
                };
                if socket.send(Message::Text(text)).await.is_err() {
                    return; // peer disconnected
                }
            }
            // A `Lagged` subscriber fell behind BROADCAST_BUFFER messages;
            // skip the gap and keep going rather than tearing the socket down.
            Err(broadcast::error::RecvError::Lagged(_)) => continue,
            Err(broadcast::error::RecvError::Closed) => return,
        }
    }
}
