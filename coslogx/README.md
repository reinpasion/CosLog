# CircuitLite (Rust port)

Native port of the single-file HTML/JS circuit simulator, with the same `.json` project format.

```
cargo run -p circuit_app --release                    # the editor
cargo run -p circuit_app --release --features audio   # + buzzer tone (Linux needs libasound2-dev)
cargo test --workspace                                # 59 tests, no window needed
cargo run -p circuit_core --example gallery > gallery.svg   # render every symbol to SVG
```

* `circuit_core` – model, SI units, netlist compiler, MNA engine (rayon islands), symbols, editor logic,
  diagnostics, examples. No GUI dependency.
* `circuit_app` – eframe/egui front end.

Shortcuts: W wire, R / Shift+R rotate, M / Shift+M mirror, Del delete, Space run/pause, `/` search,
Ctrl+A/C/X/V/D select/copy/cut/paste/duplicate, Ctrl+Z / Ctrl+Y undo/redo, Esc cancel.
Mouse: wheel zoom, right/middle drag pan, drag left→right window-select, right→left crossing-select.
