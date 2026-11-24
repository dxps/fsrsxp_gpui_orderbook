//! The dashboard entity: builds the header, orderbook, trades and status
//! bar from the latest `MarketSnapshot`, and polls the feed for updates.
//!
//! Polling strategy differs by target. Native uses a background timer that
//! keeps firing even if the OS throttles redraws on an unfocused window.
//! Web's single-threaded executor can't do that without colliding with the
//! main-thread paint borrow (RefCell panic), so there the poll runs inside
//! `render()` via `window.request_animation_frame()`.

use gpui::{div, rgb, Context, IntoElement, ParentElement, Render, Styled, Window};

#[cfg(not(target_family = "wasm"))]
use gpui::Task;

use crate::feed::{ConnStatus, FeedClient};
use crate::market::Market;
use crate::views::{header, orderbook_panel, status_bar, trades_panel, BG_PAGE};

pub struct Dashboard {
    market:       Market,
    feed:         FeedClient,
    status:       ConnStatus,
    render_count: u64,
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
/// Web: same-origin `/ui-config` — the feed service is the one that serves
/// index.html, so the browser is already on the right host.
///
/// Native: `OB_BACKEND_URL` env var (default `http://127.0.0.1:7878`).
fn resolve_config_url() -> String {
    #[cfg(target_family = "wasm")]
    {
        "/ui-config".to_string()
    }
    #[cfg(not(target_family = "wasm"))]
    {
        let base =
            std::env::var("OB_BACKEND_URL").unwrap_or_else(|_| "http://127.0.0.1:7878".into());
        format!("{}/ui-config", base.trim_end_matches('/'))
    }
}
