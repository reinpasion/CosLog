//! Reads `../../spec/parts.json` (the single source of truth shared with CosLog Y) and emits into `$OUT_DIR`:
//!   spec_gen.rs    static part tables, palette groups, diagnostics constants, schema hash   (types in src/spec.rs)
//!   kind_gen.rs    the `Kind` enum and its metadata methods                                 (included by src/kind.rs)
//!   params_gen.rs  per-part parameter tables, live sliders, keywords, palette groups        (included by src/params.rs)
//! Parts are emitted in file order of parts.json (serde_json `preserve_order`).
use serde_json::{Map, Value};
use std::{env, fmt::Write as _, fs, path::PathBuf};

fn q(s: &str) -> String { format!("{:?}", s) }
fn opt_str(v: Option<&Value>) -> String {
    match v.and_then(|x| x.as_str()) { Some(s) => format!("Some({})", q(s)), None => "None".into() }
}
fn num(v: &Value) -> f64 { v.as_f64().unwrap_or_else(|| panic!("number expected, got {v}")) }
fn f(x: f64) -> String { format!("{:?}f64", x) }

/// "r,g,b" -> [r, g, b]; panics loudly on anything else.
fn rgb3(s: &str, what: &str) -> [u8; 3] {
    let c: Vec<u8> = s.split(',').map(|x| x.trim().parse().unwrap_or_else(|_| panic!("{what}: {s:?} is not r,g,b bytes"))).collect();
    assert_eq!(c.len(), 3, "{what}: {s:?} needs 3 components");
    [c[0], c[1], c[2]]
}

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
    let (kind, key) = if w.get("value").is_some() { ("Value", "value") } else if let Some(s) = w.get("state") { ("State", s.as_str().unwrap()) }
        else if let Some(p) = w.get("param") { ("Param", p.as_str().unwrap()) } else { panic!("bad when {w}") };
    let eq = w.get("eq").map(|e| format!("Some({})", e.as_i64().unwrap())).unwrap_or("None".into());
    let bit = w.get("bit").map(|e| format!("Some({})", e.as_u64().unwrap())).unwrap_or("None".into());
    let inn: Vec<String> = w.get("in").and_then(|a| a.as_array()).map(|a| a.iter().map(|s| q(s.as_str().unwrap())).collect()).unwrap_or_default();
    let value = match w.get("value").and_then(|v| v.as_str()) { Some(c) => format!("Some(({}, {}))", c == "lt", f(num(&w["than"]))), None => "None".into() };
    format!("Some(When {{ kind: WhenKind::{kind}, key: {}, eq: {eq}, bit: {bit}, within: &[{}], value: {value} }})", q(key), inn.join(","))
}

// ------------------------------------------------------------------ spec_gen.rs (unchanged output)
fn gen_spec(spec: &Value, bytes: &[u8]) -> String {
    let parts = spec["parts"].as_object().expect("parts");
    let mut o = String::new();
    writeln!(o, "pub const SPEC_HASH: &str = {};", q(&spec_hash(bytes))).unwrap();
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
            "SymLayer {{ d: {}, when: {}, spin: {}, wiper: {}, widget: {}, fill: {}, fill_when: &[{}] }}", q(l.get("d").and_then(|d| d.as_str()).unwrap_or("")), when(l.get("when")),
            l.get("spin").is_some(), l.get("wiper").and_then(|b| b.as_bool()).unwrap_or(false), opt_str(l.get("widget")), opt_str(l.get("fill")),
            l.get("fillWhen").and_then(|a| a.as_array()).map(|a| a.iter().map(|e| format!("({}, {})", when(e.get("when")).strip_prefix("Some(").unwrap().strip_suffix(")").unwrap(), q(e["color"].as_str().unwrap()))).collect::<Vec<_>>().join(", ")).unwrap_or_default())).collect();
        let rules = |key: &str| -> String { p.get(key).and_then(|a| a.as_array()).map(|a| a.iter().map(|e| {
            let j: Vec<String> = e["join"].as_array().unwrap().iter().map(|pr| { let n = |x: &Value| if x.as_str() == Some("gnd") { -1 } else { x.as_i64().unwrap() }; format!("({}, {})", n(&pr[0]), n(&pr[1])) }).collect();
            format!("PathRule {{ when: {}, join: &[{}] }}", when(e.get("when")), j.join(", "))
        }).collect::<Vec<_>>().join(", ")).unwrap_or_default() };
        let (dcp, shp) = (rules("dcPath"), rules("shortPath"));
        let ctext = opt_str(p.get("centerText").and_then(|c| c.get("text")));
        let state: Vec<String> = p.get("state").and_then(|s| s.as_array()).map(|s| s.iter().map(|x| q(x.as_str().unwrap())).collect()).unwrap_or_default();
        writeln!(o, "    PartSpec {{ code: {}, name: {}, prefix: {}, group: {}, keywords: {}, pins: &[{}], bbox: {bbox}, nonlinear: {}, row_source: {}, tolerance: {}, params: &[{}], live: {live}, state: &[{}], limits: &[{}], symbol: &[{}], dc_path: &[{dcp}], short_path: &[{shp}], center_text: {ctext} }},",
            q(code), q(p["name"].as_str().unwrap()), opt_str(p.get("prefix")), q(p["group"].as_str().unwrap()), q(p["keywords"].as_str().unwrap_or("")),
            pins.join(", "), p["nonlinear"].as_bool().unwrap_or(false), p["rowSource"].as_bool().unwrap_or(false), p["tolerance"].as_bool().unwrap_or(false),
            params.join(", "), state.join(", "), limits.join(", "), layers.join(", ")).unwrap();
    }
    writeln!(o, "];").unwrap();
    let dg = &spec["diagnostics"];
    for (name, key) in [("SOURCE_SHORT_A", "sourceShortCurrentA"), ("SOURCE_RUNAWAY_A", "sourceRunawayCurrentA"), ("LED_WARN_A", "ledWarnCurrentA"), ("NODE_VOLTAGE_ERR_V", "nodeVoltageErrorV"), ("LAG_WARN_FRAMES", "lagWarnFrames"), ("HOLD_MS", "holdMs"), ("DANGLE_LIST_MAX", "dangleListMax"), ("NO_DC_LIST_MAX", "noDcPathListMax")] {
        writeln!(o, "pub const {name}: f64 = {};", f(num(&dg[key]))).unwrap();
    }
    writeln!(o, "pub static MESSAGES: &[(&str, &str)] = &[").unwrap();
    for (k, v) in dg["messages"].as_object().unwrap() { writeln!(o, "    ({}, {}),", q(k), q(v.as_str().unwrap())).unwrap(); }
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

    writeln!(o, "pub static LED_RGB: &[(&str, [u8; 3])] = &[").unwrap();
    for (k, v) in spec["ledColors"].as_object().unwrap() {
        let c = rgb3(v["rgb"].as_str().unwrap(), &format!("ledColors.{k}"));
        writeln!(o, "    ({}, [{}, {}, {}]),", q(k), c[0], c[1], c[2]).unwrap();
    }
    writeln!(o, "];").unwrap();

    // 555 style captions drawn inside the body: (code, [(text, x, y, size, bold)])
    writeln!(o, "pub static PIN_TEXT: &[(&str, &[(&str, f64, f64, f32, bool)])] = &[").unwrap();
    for (code, p) in parts {
        if let Some(a) = p.get("pinText").and_then(|a| a.as_array()) {
            let items: Vec<String> = a.iter().map(|t| format!("({}, {}, {}, {:?}f32, {})",
                q(t["text"].as_str().expect("pinText.text")), f(num(&t["x"])), f(num(&t["y"])), num(&t["size"]), t["bold"].as_bool().unwrap_or(false))).collect();
            writeln!(o, "    ({}, &[{}]),", q(code), items.join(", ")).unwrap();
        }
    }
    writeln!(o, "];").unwrap();

    // centre letter: (code, text, size, bold, dy); font is "bold 14px" style
    writeln!(o, "pub static CENTER_TEXT: &[(&str, &str, f32, bool, f64)] = &[").unwrap();
    for (code, p) in parts {
        if let Some(c) = p.get("centerText") {
            let font = c["font"].as_str().expect("centerText.font");
            let size: f32 = font.split_whitespace().last().unwrap().trim_end_matches("px").parse().unwrap_or_else(|_| panic!("{code}: bad centerText font {font:?}"));
            writeln!(o, "    ({}, {}, {:?}f32, {}, {}),", q(code), q(c["text"].as_str().expect("centerText.text")), size, font.starts_with("bold"), f(num(&c["dy"]))).unwrap();
        }
    }
    writeln!(o, "];").unwrap();

    // glow: (code, radius, Some(rgb)); None = colour comes from the LED colour parameter
    writeln!(o, "pub static GLOW: &[(&str, f64, Option<[u8; 3]>)] = &[").unwrap();
    for (code, p) in parts {
        if let Some(g) = p.get("glow") {
            let col = match &g["color"] {
                Value::String(s) => { let c = rgb3(s, &format!("{code}.glow.color")); format!("Some([{}, {}, {}])", c[0], c[1], c[2]) }
                Value::Object(m) => { assert_eq!(m.get("map").and_then(|x| x.as_str()), Some("ledColors"), "{code}: glow map must be ledColors"); "None".to_string() }
                other => panic!("{code}: bad glow color {other}"),
            };
            writeln!(o, "    ({}, {}, {col}),", q(code), f(num(&g["r"]))).unwrap();
        }
    }
    writeln!(o, "];").unwrap();

    // value label suffixes: (code, [(param key, unit)]) -> " {value}{unit}"
    writeln!(o, "pub static VALUE_SUFFIX: &[(&str, &[(&str, &str)])] = &[").unwrap();
    for (code, p) in parts {
        if let Some(a) = p.get("valueSuffix").and_then(|a| a.as_array()) {
            let items: Vec<String> = a.iter().map(|e| format!("({}, {})", q(e["key"].as_str().expect("valueSuffix.key")), q(e["unit"].as_str().expect("valueSuffix.unit")))).collect();
            writeln!(o, "    ({}, &[{}]),", q(code), items.join(", ")).unwrap();
        }
    }
    writeln!(o, "];").unwrap();

    writeln!(o, "pub static VALUE_FORMAT: &[(&str, &str)] = &[").unwrap();
    for (code, p) in parts { writeln!(o, "    ({}, {}),", q(code), q(p["valueFormat"].as_str().unwrap_or_else(|| panic!("{code}: valueFormat missing")))).unwrap(); }
    writeln!(o, "];").unwrap();
    o
}

// ------------------------------------------------------------------ kind_gen.rs
/// Rust variant name of a part code: first letter upper-cased ("dc" -> "Dc", "nand" -> "Nand").
fn variant(code: &str) -> String {
    assert!(!code.is_empty() && code.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()) && code.chars().next().unwrap().is_ascii_lowercase(),
        "part code {code:?} must be lowercase ascii letters/digits starting with a letter");
    let mut c = code.chars();
    let h = c.next().unwrap();
    format!("{}{}", h.to_ascii_uppercase(), c.as_str())
}

fn pins_of(p: &Value) -> Vec<(f64, f64)> {
    p["pins"].as_array().expect("pins").iter().map(|pn| (num(&pn["x"]), num(&pn["y"]))).collect()
}

/// One `Kind::X => expr,` arm per part, in spec order (exhaustive: no wildcard arm).
fn kind_arms(parts: &Map<String, Value>, expr: impl Fn(&Value) -> String) -> String {
    let mut s = String::new();
    for (code, p) in parts { writeln!(s, "            Kind::{} => {},", variant(code), expr(p)).unwrap(); }
    s
}

fn gen_kind(spec: &Value) -> String {
    let parts = spec["parts"].as_object().expect("parts");
    let mut o = String::new();
    writeln!(o, "// @generated by build.rs from spec/parts.json. Do not edit.").unwrap();
    writeln!(o, "#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]").unwrap();
    writeln!(o, "pub enum Kind {{").unwrap();
    for code in parts.keys() { writeln!(o, "    #[serde(rename = {})] {},", q(code), variant(code)).unwrap(); }
    writeln!(o, "}}\n").unwrap();
    writeln!(o, "impl Kind {{").unwrap();
    let all: Vec<String> = parts.keys().map(|c| format!("Kind::{}", variant(c))).collect();
    writeln!(o, "    pub const ALL: &'static [Kind] = &[{}];", all.join(", ")).unwrap();

    writeln!(o, "    pub fn code(self) -> &'static str {{\n        match self {{\n{}        }}\n    }}", parts.iter().map(|(c, _)| format!("            Kind::{} => {},\n", variant(c), q(c))).collect::<String>()).unwrap();
    writeln!(o, "    pub fn name(self) -> &'static str {{\n        match self {{\n{}        }}\n    }}", kind_arms(parts, |p| q(p["name"].as_str().expect("name")))).unwrap();
    writeln!(o, "    /// Local pin offsets (before mirror/rotation).\n    pub fn pins(self) -> &'static [P] {{\n        match self {{\n{}        }}\n    }}",
        kind_arms(parts, |p| format!("&[{}]", pins_of(p).iter().map(|(x, y)| format!("({:?}, {:?})", x, y)).collect::<Vec<_>>().join(", ")))).unwrap();
    // bbox: explicit array from the spec, otherwise pin extents +/- 14
    writeln!(o, "    /// Local bounding box (x0, y0, x1, y1).\n    pub fn bbox(self) -> [f64; 4] {{\n        match self {{\n{}        }}\n    }}", kind_arms(parts, |p| {
        let b: [f64; 4] = match p.get("bbox") {
            Some(Value::Array(b)) => { assert_eq!(b.len(), 4, "bbox needs 4 numbers"); [num(&b[0]), num(&b[1]), num(&b[2]), num(&b[3])] }
            _ => {
                let v = pins_of(p);
                let (x0, x1) = v.iter().fold((f64::MAX, f64::MIN), |a, q| (a.0.min(q.0), a.1.max(q.0)));
                let (y0, y1) = v.iter().fold((f64::MAX, f64::MIN), |a, q| (a.0.min(q.1), a.1.max(q.1)));
                [x0 - 14.0, y0 - 14.0, x1 + 14.0, y1 + 14.0]
            }
        };
        format!("[{:?}, {:?}, {:?}, {:?}]", b[0], b[1], b[2], b[3])
    })).unwrap();
    writeln!(o, "    /// Default (value, frequency/aux) strings applied when a part is placed.\n    pub fn defaults(self) -> (Option<&'static str>, Option<&'static str>) {{\n        match self {{\n{}        }}\n    }}",
        kind_arms(parts, |p| format!("({}, {})", opt_str(p["defaults"].get("v")), opt_str(p["defaults"].get("f"))))).unwrap();
    writeln!(o, "    /// Designator prefix (R1, Q2 ...).\n    pub fn prefix(self) -> Option<&'static str> {{\n        match self {{\n{}        }}\n    }}", kind_arms(parts, |p| opt_str(p.get("prefix")))).unwrap();
    writeln!(o, "    pub fn nonlinear(self) -> bool {{\n        match self {{\n{}        }}\n    }}", kind_arms(parts, |p| p["nonlinear"].as_bool().unwrap_or(false).to_string())).unwrap();
    writeln!(o, "    /// Unit shown after the value label (spec `labelUnit`).\n    pub fn label_unit(self) -> &'static str {{\n        match self {{\n{}        }}\n    }}", kind_arms(parts, |p| q(p["labelUnit"].as_str().unwrap_or("")))).unwrap();
    writeln!(o, "}}").unwrap();
    o
}

// ------------------------------------------------------------------ params_gen.rs
fn gen_params(spec: &Value) -> String {
    let parts = spec["parts"].as_object().expect("parts");
    let mut o = String::new();
    writeln!(o, "// @generated by build.rs from spec/parts.json. Do not edit.").unwrap();
    for (code, p) in parts {
        let items: Vec<String> = p["params"].as_array().expect("params").iter().map(|pa| {
            let key = pa["key"].as_str().expect("param key");
            // v / f placement defaults live in Kind::defaults(); only the other keys carry a creation default here.
            let def = if key == "v" || key == "f" { "None".to_string() } else { opt_str(pa.get("default")) };
            let opts = match pa.get("options").and_then(|a| a.as_array()) {
                Some(a) => format!("Some(&[{}])", a.iter().map(|s| q(s.as_str().expect("option"))).collect::<Vec<_>>().join(", ")),
                None => "None".into(),
            };
            format!("Param {{ key: {}, label: {}, unit: {}, def: {def}, opts: {opts} }}", q(key), q(pa["name"].as_str().expect("param name")), q(pa["unit"].as_str().unwrap_or("")))
        }).collect();
        writeln!(o, "static P_{}: &[Param] = &[{}];", code.to_uppercase(), items.join(", ")).unwrap();
    }
    let arms: String = parts.keys().map(|c| format!("        Kind::{} => P_{},\n", variant(c), c.to_uppercase())).collect();
    writeln!(o, "fn table(k: Kind) -> &'static [Param] {{\n    match k {{\n{arms}    }}\n}}").unwrap();
    writeln!(o, "pub fn params(k: Kind) -> Vec<Param> {{ table(k).to_vec() }}").unwrap();

    let live: String = parts.iter().filter_map(|(c, p)| p.get("live").map(|l| format!("        Kind::{} => Some(({:?}, {:?})),\n", variant(c), num(&l["min"]), num(&l["max"])))).collect();
    writeln!(o, "/// Parts whose `f` is a live slider (adjustable while the simulation runs).\npub fn live_slider(k: Kind) -> Option<(f64, f64)> {{\n    match k {{\n{live}        _ => None,\n    }}\n}}").unwrap();

    let kw: String = parts.iter().map(|(c, p)| format!("        Kind::{} => {},\n", variant(c), q(p["keywords"].as_str().unwrap_or("")))).collect();
    writeln!(o, "pub fn keywords(k: Kind) -> &'static str {{\n    match k {{\n{kw}    }}\n}}").unwrap();

    writeln!(o, "pub const GROUPS: &[(&str, &[Kind])] = &[").unwrap();
    for g in spec["groups"].as_array().expect("groups") {
        let ks: Vec<String> = g["parts"].as_array().expect("group parts").iter().map(|s| {
            let c = s.as_str().expect("group part code");
            assert!(parts.contains_key(c), "group lists unknown part {c}");
            format!("Kind::{}", variant(c))
        }).collect();
        writeln!(o, "    ({}, &[{}]),", q(g["name"].as_str().expect("group name")), ks.join(", ")).unwrap();
    }
    writeln!(o, "];").unwrap();
    o
}

fn main() {
    let root = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("../../spec/parts.json");
    println!("cargo:rerun-if-changed={}", root.display());
    let bytes = fs::read(&root).unwrap_or_else(|e| panic!("cannot read {}: {e}", root.display()));
    let spec: Value = serde_json::from_slice(&bytes).expect("parts.json is not valid JSON");
    assert_eq!(spec["schema"].as_i64(), Some(1), "unsupported parts.json schema");
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    fs::write(out.join("spec_gen.rs"), gen_spec(&spec, &bytes)).unwrap();
    fs::write(out.join("kind_gen.rs"), gen_kind(&spec)).unwrap();
    fs::write(out.join("params_gen.rs"), gen_params(&spec)).unwrap();
}