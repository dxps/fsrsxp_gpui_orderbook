# fsrsxp_gpui_orderbook

A trading orderbook dashboard written in Rust on top of Zed's **GPUI**
framework, fed by a separate **Axum** service over a **WebSocket**. 40 price
levels (20 bid / 20 ask), live cumulative-depth bars, a rolling tape of
recent prints, and a status bar showing the live connection state.

**UI runs as a native macOS/Linux/Windows window AND in the browser** from
the same codebase. One backend serves both.

| native                                     | web (WebGPU in a `<canvas>`)                  |
| ------------------------------------------ | --------------------------------------------- |
| ![native](docs/screenshot.png)             | ![web](docs/screenshot-web.jpg)               |

## Architecture

```
┌──────────────────────┐      HTTP   GET /ui-config → { ws_url }      ┌──────────────────────┐
│        UI            │  ───────────────────────────────────────▶    │   orderbook-feed     │
│  (gpui-pre, native   │                                              │   (axum 0.7)         │
│   or WebGPU in a     │  ◀─────────────────────────────────────────  │                      │
│   browser canvas)    │      WebSocket  /feed  (MarketSnapshot /     │  Simulator @ 10 Hz   │
│                      │                  tick, JSON-encoded)         │  tokio broadcast     │
└──────────────────────┘                                              └──────────────────────┘
```

- UI fetches `/ui-config` at init time to discover the WebSocket URL, so the
  same static bundle can point at dev, staging or prod backends without a
  rebuild.
- Backend keeps one simulator running regardless of client count — a
  `tokio::sync::broadcast` channel fans the same snapshot out to every
  connected WebSocket.
- Wire format is the `fsrsxp-orderbook-proto` crate (plain serde structs),
  shared between the two binaries via a Cargo workspace.

## Workspace

```
.
├── Cargo.toml                 # workspace root (resolver 2, tuned release profile)
└── crates/
    ├── proto/                 # Side, Level, Trade, MarketSnapshot, UiConfig  (serde)
    ├── feed/                  # Axum service — binary `orderbook-feed`
    │   └── src/
    │       ├── main.rs        # /ui-config  + /feed (WS upgrade) + ticker
    │       └── sim.rs         # simulator (ex-market.rs tick logic)
    └── ui/                    # GPUI dashboard — binary `orderbook` + wasm cdylib
        ├── src/
        │   ├── main.rs        # native launcher
        │   ├── lib.rs         # Dashboard entity + native/wasm entry points
        │   ├── market.rs      # view model (type alias on proto + MarketExt trait)
        │   ├── views.rs       # header, orderbook panel, trades panel, status bar
        │   └── feed/          # cross-target WS client
        │       ├── mod.rs
        │       ├── native.rs  # tungstenite on a dedicated OS thread
        │       └── web.rs     # gloo-net via wasm_bindgen_futures::spawn_local
        ├── assets/IBMPlexSans-Regular.ttf
        ├── index.html
        └── Trunk.toml         # dev-server proxy for /ui-config + /feed
```

## Stack

| crate               | version    | role                                                   |
| ------------------- | ---------- | ------------------------------------------------------ |
| `gpui-pre`          | `=0.3.14`  | Zed's GPUI, published to crates.io as a snapshot       |
| `gpui-pre-platform` | `=0.3.14`  | Native backend (Metal/Vulkan/DX); on wasm re-exports `gpui-pre-web` |
| `axum`              | `0.7`      | HTTP + WebSocket server                                |
| `tokio`             | `1`        | Async runtime for the feed service                     |
| `tower-http`        | `0.5`      | Permissive CORS for the dev server                     |
| `reqwest`           | `0.12`     | Blocking `GET /ui-config` on native (TLS features off) |
| `tungstenite`       | `0.24`     | Sync WebSocket client on native                        |
| `gloo-net`          | `0.6`      | HTTP + WebSocket client on wasm                        |
| `web-time`          | `1`        | Portable `SystemTime` (std's panics on wasm32)         |
| `serde`             | `1`        | Wire format                                            |

The UI pins still match what [`longbridge/gpui-component`](https://github.com/longbridge/gpui-component)
uses in its own workspace, so a later move to the heavier `gpui-component`
widget set (DataTable, Dock, Tiles) requires no dependency surgery.

## Run

### 1. Start the feed service

```bash
cargo run -p fsrsxp-orderbook-feed
```

Listens on `http://127.0.0.1:7878`. Exposes `GET /ui-config` and `GET /feed`
(WebSocket upgrade). One simulator per process; add a client, it starts
receiving snapshots at the next tick.

### 2a. Native UI

```bash
cargo run --release
```

Default target is `crates/ui`, so no `-p` needed. Opens a 1280×1000 window
centred on the display; `Cmd+Q` quits. The UI connects to
`http://127.0.0.1:7878` by default — override with `OB_BACKEND_URL`:

```bash
OB_BACKEND_URL=http://feed.internal:7878 cargo run --release
```

### 2b. Web UI

```bash
cargo install trunk       # once
cd crates/ui
trunk serve               # http://127.0.0.1:8080
```

Requires a browser with WebGPU (Chrome 144+, Safari 26+, Firefox 141+).
Trunk's dev server proxies `/ui-config` and `/feed` to the backend on
`127.0.0.1:7878` (see `crates/ui/Trunk.toml`), so the browser treats
everything as same-origin.

`trunk build --release` produces a `dist/` with `.js`, `.wasm` and a copy
of `index.html`.

### Production deployment

Two shapes, pick one:

**Reverse proxy** — any static host (nginx, Caddy, S3 + CloudFront, Cloudflare
Pages) serves `dist/`, and `/ui-config` + `/feed` get proxied to the axum
service. Set `OB_PUBLIC_WS_URL` on the feed process so `/ui-config` hands the
browser the public URL — and `wss://` instead of `ws://`, otherwise an HTTPS
page refuses the socket as mixed content:

```bash
OB_PUBLIC_WS_URL=wss://orderbook.example.com/feed \
  ./orderbook-feed
```

```caddyfile
orderbook.example.com {
    root * /var/www/orderbook
    file_server

    reverse_proxy /ui-config  orderbook-feed.internal:7878
    reverse_proxy /feed       orderbook-feed.internal:7878
}
```

**Single process** — bolt `tower_http::services::ServeDir` onto the axum
router so one binary serves `dist/` at `/` alongside `/ui-config` and `/feed`.
Same-origin, no reverse proxy, no CORS, one thing to deploy — good for a demo
box. Scaling the feed independently of the static assets later means splitting
them back out.

### Web bundle size

| profile                               | raw wasm | gzipped |
| ------------------------------------- | -------: | ------: |
| `trunk serve` (dev)                   |    20 MB |       — |
| `trunk build --release`               |   7.4 MB |  2.5 MB |

Serve `dist/` behind any static host with gzip or brotli enabled. The
2.5 MB gz floor is the structural cost of GPUI's own text/graphics stack —
wgpu, cosmic-text + swash + harfrust + skrifa, taffy, and ICU — because
GPUI renders everything itself through WebGPU rather than using
browser-native HTML/canvas text. The release profile is tuned for size
(`lto = "fat"`, `opt-level = "z"`, `panic = "abort"`, `strip = true`) and
`wasm-opt` is deliberately off: on `gpui-pre-web`'s output it needed
`--enable-bulk-memory-opt` and saved under 20 KB when it did run.

## Update strategy

The `Dashboard` view owns a `Market` (the latest `MarketSnapshot`), a
`FeedClient`, and a `ConnStatus`. Both targets poll the feed and repaint:

| target  | driver                                                                    |
| ------- | ------------------------------------------------------------------------- |
| native  | `cx.spawn(…)` + `cx.background_executor().timer(50ms)` polls the feed and calls `cx.notify()`. Keeps ticking even when the window is backgrounded. |
| wasm    | render-loop: each `render()` drains the feed and calls `window.request_animation_frame()`. Snapshots arrive at 10 Hz; the repaint happens at ~60 Hz. |

The web path doesn't use the background timer because on single-threaded wasm
its runnable resumes on the main thread mid-frame and collides with the paint
borrow of `App` — a `RefCell` panic every tick (tracks zed#65289 and
longbridge/gpui-kit#3414). The render-loop pattern avoids the borrow entirely
because the poll runs inside `Render::render`, already scoped to a `Context`.

## Feed-client transports

| target  | HTTP (one-shot `/ui-config`) | WebSocket (`/feed`)   | runs on                           |
| ------- | ---------------------------- | --------------------- | --------------------------------- |
| native  | `reqwest::blocking` (no TLS) | `tungstenite` (sync)  | dedicated OS thread; snapshots cross back via `std::sync::mpsc` |
| wasm    | `gloo-net::http::Request`    | `gloo-net::websocket` | page event loop via `wasm_bindgen_futures::spawn_local`; `Rc<RefCell<…>>` to the renderer |

No tokio in the UI binary — the dedicated-thread + sync-client design keeps
the native build small and avoids the async-runtime-inside-async-runtime
problem GPUI's executor would otherwise hit.

### Why not the other GPUI variants?

The brief asked for a Web UI with a choice of five GPUI variants. In late
2026:

- **Zed upstream `gpui_web`** — experimental, stable-Rust web build still
  being fixed ([zed#65289](https://github.com/zed-industries/zed/pull/65289)).
- **`gpui-pre-web` on crates.io** — the WASM path we use, via its
  `gpui-pre-platform` shim. Single canvas, needs WebGPU, falls back to
  WebGL2. Opt out of its `multithreaded` default feature to compile on stable
  Rust (`wasm_thread` otherwise pulls `#![feature(stdarch_wasm_atomic_wait)]`,
  nightly only).
- **`IAmJSD/gpui` fork** — niche, advertises a WASM backend; status table
  unverified.
- **`longbridge/gpui-kit` / `gpui-component`** — mature widget set. Desktop
  first; web support tracked open
  ([#3414](https://github.com/longbridge/gpui-kit/issues/3414)).
- **`pierreaubert/gpui-toolkit`** — advertises working WASM demos, but
  vendors its own GPUI version that conflicts with other sources.

## Colors / layout

| region | color      |
| ------ | ---------- |
| page   | `#0A0E1A`  |
| panel  | `#121829`  |
| header | `#0F1420`  |
| mid    | `#1B243A`  |
| bid    | `#26A69A`  |
| ask    | `#EF5350`  |
| accent | `#FFB400`  |

Depth bars extend from the right edge of the orderbook, width proportional
to `cumulative_size_from_best / max_side_total`.
