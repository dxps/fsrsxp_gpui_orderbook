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
