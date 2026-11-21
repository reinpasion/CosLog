# CosLog

A fast, free, and open-source electronic circuit simulator built for EE, ECE, and CpE students—no expensive licenses or heavy installs required.

## Two Editions, One Circuit File

* **CosLog Y (Web Edition):** Runs directly in any browser with zero installation—great for quick homework checks, laptops, or phones. [Try it live here](https://reinpasion.github.io/CosLog/) or open `index.html`.
* **CosLog X (Desktop Edition):** A native Rust desktop app built for speed, larger circuits, and heavy lab simulations.

Both editions share the exact same `.json` save files and component library—sketch a circuit in your browser during class and open it on your desktop later.

## What You Can Do

* **Build & Test Fast:** Drag and drop 50+ parts—power sources, resistors, capacitors, diodes, transistors, op-amps, 555 timers, logic gates, flip-flops, motors, and a dual-channel oscilloscope.
* **Spot Mistakes Automatically:** Flags missing grounds, floating wires, short circuits, and overloaded components before you burn anything out.
* **Realistic Behavior:** Simulates real-world component tolerances (like 5% resistors) consistently every time you open your circuit.
* **Works Offline:** The entire web edition fits inside a single ~84 KB file (`index.html`), so you can save it to a USB drive and run it anywhere without internet.

## Quick Start

### CosLog Y (Web Edition — No Install)
1. **Use Online:** Open [reinpasion.github.io/CosLog](https://reinpasion.github.io/CosLog/).
2. **Use Offline:** Download `index.html` from this repository and double-click it to open in Chrome, Edge, Firefox, or Safari.
3. Press **Space** (or click **Run**) to start simulating, or pick a pre-built circuit from the **Examples...** menu.

### CosLog X (Desktop Edition)
1. Install [Rust](https://rustup.rs/) if you don't have it yet.
2. Download or clone this repository, then open a terminal inside the `coslogx` folder.
3. Run `cargo run --release -p circuit_app` to launch the desktop app.

## Project Folders

* `index.html` — Ready-to-run browser version (CosLog Y)
* `coslogx/` — Native desktop version written in Rust (CosLog X)
* `spec/` — Shared component definitions and save-file rules that keep both versions in sync

## License

Free and open source under the [MIT License](LICENSE).
