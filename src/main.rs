//! Trading orderbook dashboard built on Zed's GPUI framework
//! (the `gpui-pre` crates.io snapshot, which is what `gpui-component`/`gpui-base`
//! track). Native window on macOS/Linux/Windows; the same view code should
//! port to WASM when `gpui-pre-web` lands as a target for this project.

mod market;
mod views;

use std::time::Duration;

use gpui::{
    actions, div, px, rgb, size, App, AppContext, Application, Context, IntoElement, KeyBinding,
    ParentElement, Render, Styled, Task, Window, WindowBounds, WindowOptions,
};

use crate::market::Market;
use crate::views::{header, orderbook_panel, status_bar, trades_panel, BG_PAGE};

actions!(orderbook, [Quit]);

/// Top-level view. Owns the simulated market and drives it from a GPUI
/// background timer, re-rendering the whole tree each tick.
pub struct Dashboard {
    market: Market,
    render_count: u64,
    _tick_task: Option<Task<()>>,
}

impl Dashboard {
    fn new(cx: &mut Context<Self>) -> Self {
        let mut this = Self {
            market: Market::new(),
            render_count: 0,
            _tick_task: None,
        };

        // Weak so the task can't keep the Dashboard alive (it IS the Dashboard's task).
        let weak = cx.entity().downgrade();
        this._tick_task = Some(cx.spawn(async move |_, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                if weak
                    .update(cx, |this, cx| {
                        this.market.tick();
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        }));

        this
    }
}

impl Render for Dashboard {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        self.render_count = self.render_count.wrapping_add(1);

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
            .child(status_bar(&self.market, self.render_count))
    }
}

fn main() {
    let app: Application = gpui_platform::application();

    app.run(move |cx: &mut App| {
        gpui_base::init(cx);

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

        cx.open_window(options, |_window, cx| cx.new(|cx| Dashboard::new(cx)))
            .expect("failed to open window");

        cx.activate(true);
    });
}
