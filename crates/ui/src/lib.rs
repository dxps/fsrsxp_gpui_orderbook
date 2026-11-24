//! Trading orderbook dashboard on top of Zed's GPUI framework.
//!
//! The crate has two entry points that share the same view tree:
//! `native::run_native` opens an OS window via `gpui-pre-platform`
//! (Metal/Vulkan/DirectX), and `wasm::start` runs it in a WebGPU `<canvas>`
//! via `gpui-pre-web`. The Dashboard asks the backend for its WebSocket
//! URL via `GET /ui-config`, opens that socket, and applies each
//! `MarketSnapshot` it receives.

pub mod dashboard;
pub mod feed;
pub mod market;
pub mod views;

#[cfg(not(target_family = "wasm"))]
pub mod native;
#[cfg(target_family = "wasm")]
pub mod wasm;

pub use dashboard::Dashboard;

#[cfg(not(target_family = "wasm"))]
pub use native::run_native;
