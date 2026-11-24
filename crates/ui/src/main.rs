// Native launcher. For the WASM entry see `src/lib.rs`'s `wasm` module.

#[cfg(not(target_family = "wasm"))]
fn main() {
    fsrsxp_gpui_orderbook::run_native();
}

#[cfg(target_family = "wasm")]
fn main() {}
