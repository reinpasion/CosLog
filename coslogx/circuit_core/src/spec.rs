//! Part definitions generated at build time from `spec/parts.json` (shared with CosLog Y).
//! The tables are data only; behaviour (stamps, drawing) is still hand-written and is checked
//! against these tables by `tests/spec_parity.rs`.

#[derive(Clone, Copy, Debug)]
pub struct PinSpec { pub x: i32, pub y: i32, pub label: Option<&'static str>, pub optional: bool }
#[derive(Clone, Copy, Debug)]
pub struct ParamSpec { pub key: &'static str, pub name: &'static str, pub unit: &'static str, pub default: Option<&'static str>, pub options: &'static [&'static str] }
#[derive(Clone, Copy, Debug)]
pub struct LiveSpec { pub key: &'static str, pub min: f64, pub max: f64 }
/// Rated limit: threshold = rating * warn/err, where the rating is the saved parameter `key` (> 0) or `default`.
#[derive(Clone, Copy, Debug)]
pub struct LimitRef { pub key: &'static str, pub default: f64 }
#[derive(Clone, Copy, Debug)]
pub struct LimitSpec { pub q: &'static str, pub warn: f64, pub err: f64, pub msg: &'static str, pub unit: &'static str, pub rated: Option<LimitRef> }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WhenKind { State, Param }
#[derive(Clone, Copy, Debug)]
pub struct When { pub kind: WhenKind, pub key: &'static str, pub eq: Option<i64>, pub bit: Option<u8>, pub within: &'static [&'static str] }
#[derive(Clone, Copy, Debug)]
pub struct SymLayer { pub d: &'static str, pub when: Option<When>, pub spin: bool, pub wiper: bool, pub widget: Option<&'static str> }
#[derive(Clone, Copy, Debug)]
pub struct PartSpec {
    pub code: &'static str, pub name: &'static str, pub prefix: Option<&'static str>, pub group: &'static str, pub keywords: &'static str,
    pub pins: &'static [PinSpec], pub bbox: Option<[f64; 4]>, pub nonlinear: bool, pub row_source: bool, pub tolerance: bool,
    pub params: &'static [ParamSpec], pub live: Option<LiveSpec>, pub state: &'static [&'static str],
    pub limits: &'static [LimitSpec], pub symbol: &'static [SymLayer],
}

include!(concat!(env!("OUT_DIR"), "/spec_gen.rs"));

pub fn part(code: &str) -> Option<&'static PartSpec> { PARTS.iter().find(|p| p.code == code) }

/// Measured quantities a limit can reference, in spec names.
#[derive(Clone, Copy, Debug, Default)]
pub struct Measured { pub peak_i: f64, pub avg_i: f64, pub peak_v: f64, pub power: f64 }
#[derive(Clone, Copy, Debug, Default)]
pub struct Ratings { pub q: f64, pub p: f64, pub s: f64 }

/// Evaluates the spec `limits` list for a part: first hit wins; errors above `err`, warnings above `warn`.
/// Returns `(is_error, message-before-formatting, value, limit, unit)`.
pub fn eval_limits(p: &PartSpec, m: &Measured, r: &Ratings) -> Option<(bool, &'static str, f64, f64, &'static str)> {
    for l in p.limits {
        let a = match l.q { "peakI" => m.peak_i, "avgI" => m.avg_i, "peakV" => m.peak_v, "power" => m.power, other => panic!("unknown quantity {other}") };
        let rating = match l.rated { Some(x) => { let v = match x.key { "q" => r.q, "p" => r.p, "s" => r.s, k => panic!("bad rating key {k}") }; if v > 0.0 { v } else { x.default } } None => 1.0 };
        let (w, e) = (l.warn * rating, l.err * rating);
        if a > e { return Some((true, l.msg, a, e, l.unit)); }
        if a > w { return Some((false, l.msg, a, e, l.unit)); }
    }
    None
}
