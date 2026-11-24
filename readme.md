# fsrsxp_gpui_orderbook

A trading orderbook dashboard written in Rust on top of Zed's **GPUI**
framework. 40 price levels (20 bid / 20 ask), live cumulative-depth bars, a
rolling tape of recent prints, and a status bar driven by a simulated L2 feed
updating at 10 Hz.

**Runs as a native macOS/Linux/Windows window AND in the browser** from the
same codebase.

| native                                     | web (WebGPU in a `<canvas>`)                  |
| ------------------------------------------ | --------------------------------------------- |
| ![native](docs/screenshot.png)             | ![web](docs/screenshot-web.jpg)               |

## Stack

| crate               | version    | role                                                  |
| ------------------- | ---------- | ----------------------------------------------------- |
| `gpui-pre`          | `=0.3.14`  | Zed's GPUI, published to crates.io as a snapshot      |
| `gpui-pre-platform` | `=0.3.14`  | Native backend (Metal/Vulkan/DX); on wasm re-exports gpui-pre-web |
| `rand`              | `0.8`      | Simulated feed                                        |
| `web-time`          | `1`        | Portable `SystemTime` (std's panics on wasm32)        |

The pins match what [`longbridge/gpui-component`](https://github.com/longbridge/gpui-component)
uses in its own workspace, so a later move to the heavier `gpui-component`
widget set (DataTable, Dock, Tiles) requires no dependency surgery.

## Run

### Native

```bash
cargo run --release
```

Window opens centered at 1280×1000. `Cmd+Q` quits.

### Web

```bash
cargo install trunk       # once
trunk serve               # http://127.0.0.1:8080
```

Requires a browser with WebGPU (Chrome 144+, Safari 26+, Firefox 141+).
`trunk build --release` produces a `dist/` with the `.js`, `.wasm`, and a copy of `index.html`.

#### Web bundle size

| profile                               | raw wasm | gzipped |
| ------------------------------------- | -------: | ------: |
| `trunk serve` (dev)                   |    20 MB |       — |
| `trunk build --release`               |   7.4 MB |  2.5 MB |

Serve `dist/` behind any static host with gzip or brotli enabled. The 2.5 MB
gz floor is the structural cost of GPUI's own text/graphics stack — wgpu,
cosmic-text + swash + harfrust + skrifa, taffy, and ICU — because GPUI
renders everything itself through WebGPU rather than using browser-native
HTML/canvas text. The release profile here is tuned for size
(`lto = "fat"`, `opt-level = "z"`, `panic = "abort"`, `strip = true`) and
`wasm-opt` is deliberately off: on `gpui-pre-web`'s output it needed
`--enable-bulk-memory-opt` and saved under 20 KB when it did run.

## Layout

```
src/
├── main.rs     native launcher — calls `fsrsxp_gpui_orderbook::run_native()`
├── lib.rs      Dashboard entity + native run_native + wasm::start (#[wasm_bindgen(start)])
├── market.rs   Market / Level / Trade data types + simulated feed
└── views.rs    header, orderbook panel, trades panel, status bar, palette

assets/
└── IBMPlexSans-Regular.ttf   bundled into the wasm (web backend ships no fonts)

index.html, Trunk.toml        web entry
```

The `Dashboard` view owns a `Market`. The update strategy is target-specific:

| target  | driver                                                                    |
| ------- | ------------------------------------------------------------------------- |
| native  | `cx.spawn(…)` + `cx.background_executor().timer(100ms)` — ticks even when the window is backgrounded. |
| wasm    | render-loop: each `render()` calls `window.request_animation_frame()`, ticks the market every 6 frames (~60 → 10 Hz). |

The web path doesn't use the background timer because on single-threaded wasm
its runnable resumes on the main thread mid-frame and collides with the paint
borrow of `App` — a `RefCell` panic every tick (tracks zed#65289 and
longbridge/gpui-kit#3414). The render-loop pattern avoids the borrow entirely
because the tick runs inside `Render::render`, already scoped to a `Context`.

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

Depth bars extend from the right edge of the orderbook, width proportional to
`cumulative_size_from_best / max_side_total`.
