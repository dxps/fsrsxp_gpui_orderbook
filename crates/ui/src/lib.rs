//! Trading orderbook dashboard on top of Zed's GPUI framework.
//!
//! The shared view code lives here. Two entry points sit in the same crate:
//! `run_native` opens an OS window via `gpui-pre-platform` (Metal/Vulkan/
//! DirectX), and `wasm::start` runs the same tree in a `<canvas>` via
//! `gpui-pre-web` (WebGPU with a WebGL2 fallback).
//!
//! State no longer comes from a local simulator — the Dashboard asks the
//! backend for its WebSocket URL via `GET /ui-config`, opens that socket,
//! and applies each `MarketSnapshot` it receives.

pub mod feed;
pub mod market;
pub mod views;

use gpui::{
    actions, div, rgb, App, AppContext, Context, IntoElement, ParentElement, Render, Styled,
    Window, WindowOptions,
};

#[cfg(not(target_family = "wasm"))]
use gpui::{px, size, KeyBinding, Task, WindowBounds};

use crate::feed::{ConnStatus, FeedClient};
use crate::market::Market;
use crate::views::{header, orderbook_panel, status_bar, trades_panel, BG_PAGE};

actions!(orderbook, [Quit]);

pub struct Dashboard {
    market:       Market,
    feed:         FeedClient,
    status:       ConnStatus,
    render_count: u64,
    // Native drives the render loop from a background timer that keeps firing
    // even if the OS throttles redraws on an unfocused window. Web's
    // single-threaded executor can't do that without colliding with the
    // main-thread paint borrow (RefCell panic), so there we repaint from
    // `window.request_animation_frame()` inside render() itself.
    #[cfg(not(target_family = "wasm"))]
    _tick_task:   Option<Task<()>>,
}

impl Dashboard {
    pub fn new(_cx: &mut Context<Self>) -> Self {
        let config_url = resolve_config_url();
        let feed = FeedClient::connect(config_url);

        #[cfg(not(target_family = "wasm"))]
        let mut this = Self {
            market: Market::default(),
            feed,
            status: ConnStatus::Connecting,
            render_count: 0,
            _tick_task: None,
        };
        #[cfg(target_family = "wasm")]
        let this = Self {
            market: Market::default(),
            feed,
            status: ConnStatus::Connecting,
            render_count: 0,
        };

        #[cfg(not(target_family = "wasm"))]
        {
            use std::time::Duration;
            let weak = _cx.entity().downgrade();
            this._tick_task = Some(_cx.spawn(async move |_, cx| {
                loop {
                    // 50 ms ≈ 20 fps — twice the feed's 10 Hz cadence, so a
                    // snapshot is at most one poll stale.
                    cx.background_executor()
                        .timer(Duration::from_millis(50))
                        .await;
                    if weak
                        .update(cx, |this, cx| {
                            if let Some(snap) = this.feed.try_latest() {
                                this.market = snap;
                            }
                            this.status = this.feed.status();
                            cx.notify();
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            }));
        }

        this
    }
}

impl Render for Dashboard {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        self.render_count = self.render_count.wrapping_add(1);

        #[cfg(target_family = "wasm")]
        {
            if let Some(snap) = self.feed.try_latest() {
                self.market = snap;
            }
            self.status = self.feed.status();
            _window.request_animation_frame();
        }

        div()
            .flex()
            .flex_col()
            .w_full()
            .h_full()
            .bg(rgb(BG_PAGE))
            .child(header(&self.market))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .w_full()
                    .overflow_hidden()
                    .child(orderbook_panel(&self.market))
                    .child(trades_panel(&self.market)),
            )
            .child(status_bar(&self.market, self.status, self.render_count))
    }
}

/// Where to fetch `UiConfig` from.
///
/// Web: same-origin — Trunk's dev server proxies `/ui-config` and `/feed` to
/// the backend (see `Trunk.toml`), so a static-hosted bundle "just works"
/// when its host is also configured to proxy.
///
/// Native: `OB_BACKEND_URL` env var (default `http://127.0.0.1:7878`).
fn resolve_config_url() -> String {
    #[cfg(target_family = "wasm")]
    {
        "/ui-config".to_string()
    }
    #[cfg(not(target_family = "wasm"))]
    {
        let base = std::env::var("OB_BACKEND_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:7878".into());
        format!("{}/ui-config", base.trim_end_matches('/'))
    }
}

// ─────────────────────────── native entry ─────────────────────────────

#[cfg(not(target_family = "wasm"))]
pub fn run_native() {
    let app = gpui_platform::application();

    app.run(move |cx: &mut App| {
        cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
        cx.on_action(|_: &Quit, cx: &mut App| cx.quit());
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();

        let bounds = WindowBounds::centered(None, size(px(1280.), px(1000.)), cx);
        let options = WindowOptions {
            window_bounds: Some(bounds),
            ..WindowOptions::default()
        };

        cx.open_window(options, |_, cx| cx.new(Dashboard::new))
            .expect("failed to open window");

        cx.activate(true);
    });
}

// ─────────────────────────── web entry ────────────────────────────────

#[cfg(target_family = "wasm")]
pub mod wasm {
    use super::*;
    use std::borrow::Cow;
    use std::cell::RefCell;
    use wasm_bindgen::prelude::*;

    // The web backend ships no fonts; without an explicit bundle the first
    // text layout panics and the canvas stays blank. One Regular weight is
    // enough — GPUI uses system-ui as the alias target, satisfied by any
    // font registered under the `ui-sans-serif` family group.
    const FONT_BYTES: &[u8] = include_bytes!("../assets/IBMPlexSans-Regular.ttf");

    thread_local! {
        static APPLICATION: RefCell<Option<gpui::ApplicationHandle>> = const { RefCell::new(None) };
    }

    #[wasm_bindgen(start)]
    pub fn start() -> Result<(), JsValue> {
        console_error_panic_hook::set_once();
        let _ = console_log::init_with_level(log::Level::Info);
        gpui_platform::web_init();

        let app = gpui_platform::single_threaded_web();
        let handle = app.run_embedded(move |cx: &mut App| {
            cx.text_system()
                .add_fonts(vec![Cow::Borrowed(FONT_BYTES)])
                .expect("failed to load bundled font");
            cx.open_window(WindowOptions::default(), |_, cx| cx.new(Dashboard::new))
                .expect("failed to open window");
            cx.activate(true);
        });
        APPLICATION.with(|slot| *slot.borrow_mut() = Some(handle));
        Ok(())
    }
}
