//! circuit_core: model, units and project schema for CircuitLite (Rust port).
pub mod units;
pub mod kind;
pub mod project;
pub mod netlist;
pub mod engine;
pub const GRID: f64 = 20.0;
pub mod symbols;
pub mod svg;
pub mod params;
pub mod editor;
pub mod examples;
pub mod diagnostics;
pub mod spec;
