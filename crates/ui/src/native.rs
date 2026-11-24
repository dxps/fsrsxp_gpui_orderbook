//! Native entry point. Opens a real OS window via `gpui-pre-platform`
//! (Metal on macOS, Vulkan on Linux, DirectX on Windows) and binds Cmd+Q
//! to quit.

use gpui::{actions, px, size, App, AppContext, KeyBinding, WindowBounds, WindowOptions};

use crate::Dashboard;

actions!(orderbook, [Quit]);

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
