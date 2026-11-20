//! Reads `../../spec/parts.json` (the single source of truth shared with CosLog Y) and emits
//! `$OUT_DIR/spec_gen.rs`: static part tables, palette groups and the schema hash.
//! Types live in `src/spec.rs`; this file only writes data.
use serde_json::Value;
use std::{env, fmt::Write as _, fs, path::PathBuf};

fn q(s: &str) -> String { format!("{:?}", s) }
fn opt_str(v: Option<&Value>) -> String {
    match v.and_then(|x| x.as_str()) { Some(s) => format!("Some({})", q(s)), None => "None".into() }
}
fn num(v: &Value) -> f64 { v.as_f64().unwrap_or_else(|| panic!("number expected, got {v}")) }
fn f(x: f64) -> String { format!("{:?}f64", x) }

/// FNV-1a 64 over the file bytes with CRLF normalised to LF. xtask and the web build use the same rule.
fn spec_hash(bytes: &[u8]) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\r' && i + 1 < bytes.len() && bytes[i + 1] == b'\n' { i += 1; continue; }
        h ^= bytes[i] as u64; h = h.wrapping_mul(0x100000001b3); i += 1;
    }
    format!("{:016x}", h)
}

fn when(w: Option<&Value>) -> String {
    let Some(w) = w else { return "None".into() };
    let (kind, key) = if let Some(s) = w.get("state") { ("State", s.as_str().unwrap()) }
        else if let Some(p) = w.get("param") { ("Param", p.as_str().unwrap()) } else { panic!("bad when {w}") };
    let eq = w.get("eq").map(|e| format!("Some({})", e.as_i64().unwrap())).unwrap_or("None".into());
    let bit = w.get("bit").map(|e| format!("Some({})", e.as_u64().unwrap())).unwrap_or("None".into());
    let inn: Vec<String> = w.get("in").and_then(|a| a.as_array()).map(|a| a.iter().map(|s| q(s.as_str().unwrap())).collect()).unwrap_or_default();
    format!("Some(When {{ kind: WhenKind::{kind}, key: {}, eq: {eq}, bit: {bit}, within: &[{}] }})", q(key), inn.join(","))
}

fn main() {
    let root = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("../../spec/parts.json");
    println!("cargo:rerun-if-changed={}", root.display());
    let bytes = fs::read(&root).unwrap_or_else(|e| panic!("cannot read {}: {e}", root.display()));
    let spec: Value = serde_json::from_slice(&bytes).expect("parts.json is not valid JSON");
    assert_eq!(spec["schema"].as_i64(), Some(1), "unsupported parts.json schema");
    let parts = spec["parts"].as_object().expect("parts");
    let mut o = String::new();
    writeln!(o, "pub const SPEC_HASH: &str = {};", q(&spec_hash(&bytes))).unwrap();
    writeln!(o, "pub static PARTS: &[PartSpec] = &[").unwrap();
    for (code, p) in parts {
        let pins: Vec<String> = p["pins"].as_array().unwrap().iter().map(|pn| format!(
            "PinSpec {{ x: {}, y: {}, label: {}, optional: {} }}", num(&pn["x"]) as i32, num(&pn["y"]) as i32,
            opt_str(pn.get("label")), pn.get("optional").and_then(|b| b.as_bool()).unwrap_or(false))).collect();
        let params: Vec<String> = p["params"].as_array().unwrap().iter().map(|pa| {
            let opts: Vec<String> = pa.get("options").and_then(|a| a.as_array()).map(|a| a.iter().map(|s| q(s.as_str().unwrap())).collect()).unwrap_or_default();
            format!("ParamSpec {{ key: {}, name: {}, unit: {}, default: {}, options: &[{}] }}", q(pa["key"].as_str().unwrap()), q(pa["name"].as_str().unwrap()), q(pa["unit"].as_str().unwrap_or("")), opt_str(pa.get("default")), opts.join(","))
        }).collect();
        let bbox = match p.get("bbox") { Some(Value::Array(b)) => format!("Some([{}, {}, {}, {}])", f(num(&b[0])), f(num(&b[1])), f(num(&b[2])), f(num(&b[3]))), _ => "None".into() };
        let live = match p.get("live") { Some(l) => format!("Some(LiveSpec {{ key: {}, min: {}, max: {} }})", q(l["key"].as_str().unwrap()), f(num(&l["min"])), f(num(&l["max"]))), None => "None".into() };
        let limits: Vec<String> = p.get("limits").and_then(|l| l.as_array()).map(|l| l.iter().map(|x| {
            let r = x.get("ref").map(|r| format!("Some(LimitRef {{ key: {}, default: {} }})", q(r["key"].as_str().unwrap()), f(num(&r["default"])))).unwrap_or("None".into());
            format!("LimitSpec {{ q: {}, warn: {}, err: {}, msg: {}, unit: {}, rated: {r} }}", q(x["q"].as_str().unwrap()), f(num(&x["warn"])), f(num(&x["err"])), q(x["msg"].as_str().unwrap()), q(x["unit"].as_str().unwrap())) }).collect()).unwrap_or_default();
        let layers: Vec<String> = p["symbol"].as_array().unwrap().iter().map(|l| format!(
            "SymLayer {{ d: {}, when: {}, spin: {}, wiper: {}, widget: {} }}", q(l.get("d").and_then(|d| d.as_str()).unwrap_or("")), when(l.get("when")),
            l.get("spin").is_some(), l.get("wiper").and_then(|b| b.as_bool()).unwrap_or(false), opt_str(l.get("widget")))).collect();
        let state: Vec<String> = p.get("state").and_then(|s| s.as_array()).map(|s| s.iter().map(|x| q(x.as_str().unwrap())).collect()).unwrap_or_default();
        writeln!(o, "    PartSpec {{ code: {}, name: {}, prefix: {}, group: {}, keywords: {}, pins: &[{}], bbox: {bbox}, nonlinear: {}, row_source: {}, tolerance: {}, params: &[{}], live: {live}, state: &[{}], limits: &[{}], symbol: &[{}] }},",
            q(code), q(p["name"].as_str().unwrap()), opt_str(p.get("prefix")), q(p["group"].as_str().unwrap()), q(p["keywords"].as_str().unwrap_or("")),
            pins.join(", "), p["nonlinear"].as_bool().unwrap_or(false), p["rowSource"].as_bool().unwrap_or(false), p["tolerance"].as_bool().unwrap_or(false),
            params.join(", "), state.join(", "), limits.join(", "), layers.join(", ")).unwrap();
    }
    writeln!(o, "];").unwrap();
    writeln!(o, "pub static GROUPS: &[(&str, &[&str])] = &[").unwrap();
    for g in spec["groups"].as_array().unwrap() {
        let ids: Vec<String> = g["parts"].as_array().unwrap().iter().map(|s| q(s.as_str().unwrap())).collect();
        writeln!(o, "    ({}, &[{}]),", q(g["name"].as_str().unwrap()), ids.join(", ")).unwrap();
    }
    writeln!(o, "];").unwrap();
    writeln!(o, "pub static LED_COLORS: &[(&str, f64, &str)] = &[").unwrap();
    for (k, v) in spec["ledColors"].as_object().unwrap() { writeln!(o, "    ({}, {}, {}),", q(k), f(num(&v["is"])), q(v["rgb"].as_str().unwrap())).unwrap(); }
    writeln!(o, "];").unwrap();
    fs::write(PathBuf::from(env::var("OUT_DIR").unwrap()).join("spec_gen.rs"), o).unwrap();
}
