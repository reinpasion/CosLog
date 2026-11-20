# CosLog

A lightweight electronic circuit simulator that runs in the browser with no dependencies, plus a native Rust port. Both editions are generated from, or checked against, one shared part specification.

## Features

- **Single-file web app:** `index.html` contains everything (about 84 KB). No build tools or servers are needed to use it.
- **Real-time simulation:** MNA solver with AC/DC analysis, transient response and nonlinear semiconductors.
- **50 parts:** Sources, passives, switches, diodes, BJTs, MOSFETs, op-amps, 555 timer, logic gates, flip-flops, motors, meters and a dual-channel oscilloscope.
- **Schematic editor:** Auto-wiring, drag-box selection, movable labels, undo/redo, and JSON save/load.
- **Fault diagnostics:** Flags missing grounds, floating nodes, short circuits and overloaded components.
- **Component tolerance:** Resistors, inductors and capacitors vary deterministically from a project seed (default `0x5EED`), so a saved file always behaves the same way.
- **Safe loading:** Unknown part codes are skipped with a warning banner instead of failing the whole file.

## Usage

1. Download or clone this repository.
2. Open `index.html` in any modern web browser.
3. Click **Run** (or press `Space`) to simulate, or load a circuit from the **Examples...** menu.

## Repository layout

| Path | Contents |
| --- | --- |
| `index.html` | The web edition, a build of `coslog-y/CosLog-Y.html`. |
| `spec/` | Single source of truth: `parts.json` (all parts, pins, symbols, limits, dcPath/shortPath rules, messages) and `project_schema.md` (save-file format, seed and tolerance rules). `spec/tools/` holds the generator and verification scripts. |
| `coslogx/` | Rust workspace: `circuit_core` (headless model, solver, diagnostics, symbols), `circuit_app` (egui front end) and `xtask` (spec checker, minifier and web builder). See `coslogx/README.md`. |
| `coslog-y/` | Web edition sources: `src/template.html` and the tools that produce it. |

## Building

Run these from `coslogx/` with a recent Rust toolchain:

```
cargo xtask check      # validate spec/parts.json
cargo xtask hash       # print the spec hash embedded in both editions
cargo xtask build-y    # minify the spec and inject it into coslog-y/CosLog-Y.html
cargo test --workspace # unit, parity, golden-symbol and tolerance-vector tests
```

To publish a new web build, run `cargo xtask build-y` and copy `coslog-y/CosLog-Y.html` over the root `index.html`. The build checks a size budget of 100 KB.

`coslog-y/src/template.html` is produced from the legacy page by `coslog-y/tools/migrate-legacy.mjs` followed by `coslog-y/tools/wire-spec.mjs`, which replaces hard-coded part tables and diagnostics with spec lookups.

## Changing a part

Edit `spec/parts.json`, run `cargo xtask check`, then rebuild. The Rust build script regenerates its tables from the same file, and the parity tests fail if the hand-written Rust tables drift from the spec.

## Known limits

- The 555 timer pin captions are still hard-coded in both editions.
- The web edition cannot read tolerance seeds above 2^53 and falls back to the default with a warning. Rust supports them.

## License

See `LICENSE`.
