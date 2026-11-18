# CosLog

A lightweight, single-file electronic circuit simulator that runs directly in your browser with zero dependencies.

## Features

- **Single-File App:** Everything is contained in `CircLiteS.html`—no build tools or servers required.
- **Real-Time Simulation:** Multi-core MNA solver using Web Workers, supporting AC/DC analysis, transient response, and nonlinear semiconductors.
- **45+ Components:** Sources, passives, switches, diodes, BJTs, MOSFETs, op-amps, 555 timer, logic gates, flip-flops, motors, meters, and a dual-channel oscilloscope.
- **Schematic Editor:** Auto-wiring, drag-box selection, movable labels, undo/redo, and JSON save/load.
- **Fault Diagnostics:** Automatically flags missing grounds, floating nodes, short circuits, and overloaded components.

## Usage

1. Download or clone this repository.
2. Open `index_m.html` in any modern web browser.
3. Click **Run** (or press `Space`) to simulate, or load a circuit from the **Examples...** menu.
