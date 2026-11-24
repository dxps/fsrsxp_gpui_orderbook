//! Axum feed service for the GPUI orderbook dashboard.
//!
//! One simulator ticks at 10 Hz on a background task and broadcasts each
//! snapshot through a `tokio::sync::broadcast` channel. Every connected
//! WebSocket is just a subscriber, so adding or dropping clients doesn't
//! affect the tick cadence.
//!
//! Endpoints
//! ---------
//! `GET  /ui-config` — returns `{ "ws_url": "ws://<bind-addr>/feed" }` by
//!                     default. Override with `OB_PUBLIC_WS_URL` for
//!                     reverse-proxied production deployments (use `wss://`
//!                     from an HTTPS page).
//! `GET  /feed`      — WebSocket upgrade; server pushes one JSON-encoded
//!                     `MarketSnapshot` per tick.
//! `GET  /…`         — static files from the UI dist directory
//!                     (default `crates/ui/dist`, override with `OB_UI_DIST`).
//!                     Build it with `cd crates/ui && trunk build[ --release]`.

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
use std::{net::SocketAddr, path::Path, time::Duration};
use tokio::sync::broadcast;
use tower_http::{compression::CompressionLayer, cors::CorsLayer, services::ServeDir};

mod sim;

const BIND_ADDR: &str = "127.0.0.1:7878";
const TICK_PERIOD: Duration = Duration::from_millis(100);
const DEFAULT_UI_DIST: &str = "crates/ui/dist";
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
    // Default is derived from the backend's own bind address, which is also
    // the origin the UI was served from — so same-origin, no CORS, nothing
    // to configure. Set OB_PUBLIC_WS_URL behind a TLS-terminating reverse
    // proxy so the browser gets wss:// + the public host (an HTTPS page
    // refuses a ws:// socket as mixed content).
    let ws_url = std::env::var("OB_PUBLIC_WS_URL")
        .unwrap_or_else(|_| format!("ws://{addr}/feed"));

    // UI dist directory — same binary serves the static bundle and the API,
    // so the whole app is one origin. ServeDir handles anything that isn't a
    // more specific route (`/` → index.html, `/orderbook.js`, `/*.wasm`, …).
    let ui_dist = std::env::var("OB_UI_DIST").unwrap_or_else(|_| DEFAULT_UI_DIST.to_string());
    if !Path::new(&ui_dist).is_dir() {
        eprintln!(
            "warn: UI dist `{ui_dist}` not found — /ui-config and /feed still \
             work, but `/` returns 404 until you run `cd crates/ui && trunk build` \
             (add --release for a shipped bundle)."
        );
    }

    let state = AppState { tx, ws_url };
    let app = Router::new()
        .route("/ui-config", get(ui_config))
        .route("/feed", get(feed_ws))
        .fallback_service(ServeDir::new(&ui_dist))
        // gzip responses the client says it accepts. Big win on the wasm
        // bundle (release: 7.4 MB → ~2.5 MB) and essentially free on the
        // tiny /ui-config + per-tick JSON. Skipped automatically for
        // already-binary content types (fonts, images) and WS upgrades.
        .layer(CompressionLayer::new().gzip(true))
        // Permissive CORS is a safety net for anyone running the UI off a
        // different origin (another dev server, a packaged desktop shell).
        // Harmless when the UI is served from this same process.
        .layer(CorsLayer::permissive())
        .with_state(state);

    println!("orderbook-feed listening on http://{addr}  (ws /feed, UI from {ui_dist})");
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
                if socket.send(Message::Text(text.into())).await.is_err() {
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
