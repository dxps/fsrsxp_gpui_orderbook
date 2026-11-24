//! Cross-target feed client.
//!
//! Native: one dedicated OS thread does blocking HTTP (`/ui-config`) and
//! blocking WebSocket reads via `tungstenite`; snapshots cross back to the
//! GPUI thread through `std::sync::mpsc`. No tokio runtime pulled into the
//! UI binary.
//!
//! Web: `wasm_bindgen_futures::spawn_local` runs the async task on the
//! page's event loop with `gloo-net`; snapshots cross back to the GPUI
//! entity through a single-threaded `Rc<RefCell<…>>`.
//!
//! Both paths expose the same `FeedClient` API: `connect` returns
//! immediately with status `Connecting`, `try_latest` is called from
//! `render()` to drain pending snapshots, and `status` reflects the current
//! connection state.

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ConnStatus {
    Connecting,
    Connected,
    Disconnected,
}

#[cfg(not(target_family = "wasm"))]
mod native;
#[cfg(not(target_family = "wasm"))]
pub use native::FeedClient;

#[cfg(target_family = "wasm")]
mod web;
#[cfg(target_family = "wasm")]
pub use web::FeedClient;
