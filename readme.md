# fsrsxp_gpui_orderbook

A trading orderbook dashboard written in Rust on top of Zed's **GPUI**
framework. 40 price levels (20 bid / 20 ask), live cumulative-depth bars, a
rolling tape of recent prints, and a status bar driven by a simulated L2 feed
updating at 10 Hz.

![orderbook screenshot](docs/screenshot.png)

## Stack

| crate               | version    | role                                                  |
| ------------------- | ---------- | ----------------------------------------------------- |
| `gpui-pre`          | `=0.3.14`  | Zed's GPUI, published to crates.io as a snapshot      |
| `gpui-pre-platform` | `=0.3.14`  | Native platform backend (Metal on macOS)              |
| `gpui-base`         | `=0.11.0`  | Widget primitives (not used here, pulled transitively)|
| `rand`              | `0.8`      | Simulated market feed                                 |

The pins match what [`longbridge/gpui-component`](https://github.com/longbridge/gpui-component)
uses in its own workspace, so a later move to the heavier `gpui-component`
widget set (DataTable, Dock, Tiles) requires no dependency surgery.

## Why these crates, not "the other GPUI"?

The brief asked for a **Web UI**, with a choice of five GPUI variants. In
late 2026 the picture is:

- **Zed upstream `gpui_web`** — experimental; stable-Rust web build still being
  fixed ([zed#65289](https://github.com/zed-industries/zed/pull/65289)).
- **`gpui-pre-web` on crates.io** — the WASM path. Works, single canvas, needs
  WebGPU (Chrome 144+, Safari 26+, Firefox 141+). No mature widget library yet.
- **`IAmJSD/gpui` fork** — niche, advertises a WASM backend; status table
  unverified.
- **`longbridge/gpui-kit` / `gpui-component`** — mature widget set (Table, Dock,
  Tiles, DataTable). Desktop first; web support tracked open in
  [#3414](https://github.com/longbridge/gpui-kit/issues/3414).
- **`pierreaubert/gpui-toolkit`** — advertises working WASM demos; vendors its
  own GPUI, which conflicts if combined with another source.

**This project uses `gpui-pre` directly**, writing views against primitive
elements (`div`, flex layout, `rgb`, `px`). That keeps the surface small, the
code portable, and lets us swap to either widget library later without
rewriting any view logic.

A WASM target is intentionally not wired up here yet, but the view code is
pure — the step is adding a `crate-type = ["cdylib"]` sibling crate that calls
`gpui_platform::single_threaded_web()` and `run_embedded`, bundling one font
(the web backend ships none), and serving the resulting `.wasm` with Trunk.
See the entry pattern in
[`crates/base/examples/wasm/src/lib.rs`](https://github.com/longbridge/gpui-component/blob/main/crates/base/examples/wasm/src/lib.rs)
in the gpui-component repo.

## Run

```bash
cargo run --release
```

Window opens centered at 1280×1000. `Cmd+Q` quits.

## Layout

```
src/
├── main.rs     entry point, window setup, Dashboard entity + 100ms tick loop
├── market.rs   Market / Level / Trade data types + simulated feed
└── views.rs    header, orderbook panel, trades panel, status bar, palette
```

The `Dashboard` view owns a `Market` and a GPUI `Task` that ticks it every
100 ms via `cx.background_executor().timer()`. Each tick calls `cx.notify()`
and the whole tree re-renders — GPUI diffs the paint tree, so this is cheap
even at 60 Hz.

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
