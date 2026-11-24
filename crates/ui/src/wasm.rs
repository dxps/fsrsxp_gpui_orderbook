//! Web entry point. Runs the same Dashboard tree in a WebGPU `<canvas>`
//! via `gpui-pre-web`; `#[wasm_bindgen(start)]` wires JS glue to call
//! `start()` on module load.

use gpui::{App, AppContext, WindowOptions};
use std::borrow::Cow;
use std::cell::RefCell;
use wasm_bindgen::prelude::*;

use crate::Dashboard;

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
    // `web_init()` installs the panic hook and gpui-pre-web's ConsoleLogger,
    // then force-sets `log::set_max_level` to Debug in `cfg!(debug_assertions)`
    // builds. That flips on cosmic-text's `log::debug!` fallback-resolution
    // traces — "No default font match for SansSerif at weight 400" and
    // "Failed to find preset fallback for [] locale 'en-US', used 'IBM Plex
    // Sans': …" — which fire on every text-layout pass. They're informational
    // (we only ship one font, and the layout still succeeds via the primary
    // family), so clamp the level to Info here regardless of build profile.
    gpui_platform::web_init();
    log::set_max_level(log::LevelFilter::Info);

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
