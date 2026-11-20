//! `cargo xtask <command>`: build tooling for the CosLog two-edition repo.
//!
//!   check            validate spec/parts.json (structure, paths, references)
//!   hash             print the spec hash embedded in both editions
//!   minify [FILE]    write the compact runtime spec used by CosLog Y (default: stdout)
//!   build-y          inject the compact spec into coslog-y/src/template.html and write
//!                    coslog-y/CosLog-Y.html, failing above the 100 KB budget
//!
//! The repo root is found by walking up from the current directory until `spec/parts.json` exists.
use serde_json::{json, Map, Value};
use std::{collections::BTreeSet, env, fs, path::{Path, PathBuf}, process::exit};

const BUDGET: usize = 100 * 1024;
const TOKEN: &str = "/*@@SPEC@@*/null";
const KEYS: [&str; 6] = ["v", "f", "p", "q", "s", "k"];

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let root = find_root();
    let res = match args.first().map(String::as_str) {
        Some("check") => check(&root).map(|n| println!("spec ok: {n} parts")),
        Some("hash") => load(&root).map(|(b, _)| println!("{}", hash(&b))),
        Some("minify") => minify_cmd(&root, args.get(1)),
        Some("build-y") => build_y(&root),
        _ => { eprintln!("usage: cargo xtask <check|hash|minify [FILE]|build-y>"); exit(2) }
    };
    if let Err(e) = res { eprintln!("error: {e}"); exit(1); }
}

fn find_root() -> PathBuf {
    let mut d = env::current_dir().unwrap();
    loop {
        if d.join("spec/parts.json").exists() { return d; }
        if !d.pop() { eprintln!("error: run inside the CosLog repo (spec/parts.json not found)"); exit(2); }
    }
}
fn load(root: &Path) -> Result<(Vec<u8>, Value), String> {
    let b = fs::read(root.join("spec/parts.json")).map_err(|e| e.to_string())?;
    let v = serde_json::from_slice(&b).map_err(|e| format!("parts.json: {e}"))?;
    Ok((b, v))
}
/// FNV-1a 64 over the bytes with CRLF folded to LF. Must match circuit_core/build.rs.
fn hash(b: &[u8]) -> String {
    let mut h: u64 = 0xcbf29ce484222325; let mut i = 0;
    while i < b.len() { if b[i] == b'\r' && b.get(i + 1) == Some(&b'\n') { i += 1; continue; } h ^= b[i] as u64; h = h.wrapping_mul(0x100000001b3); i += 1; }
    format!("{h:016x}")
}

// ------------------------------------------------------------------ check
fn check_path(d: &str, wiper: bool) -> Result<(), String> {
    let mut rest = d.trim(); if rest.is_empty() { return Err("empty path".into()); }
    while !rest.is_empty() {
        let c = rest.chars().next().unwrap();
        let n = match c { 'M' | 'L' => 2, 'A' => 6, 'C' => 6, 'Q' => 4, _ => return Err(format!("bad command '{c}' in \"{d}\"")) };
        let end = rest[1..].find(|ch: char| "MLACQ".contains(ch)).map(|i| i + 1).unwrap_or(rest.len());
        let args: Vec<&str> = rest[1..end].split_whitespace().collect();
        if args.len() != n { return Err(format!("'{c}' needs {n} numbers, got {} in \"{d}\"", args.len())); }
        for a in &args {
            let ok = a.parse::<f64>().is_ok() || (wiper && (*a == "{w}" || (a.starts_with("{w+") || a.starts_with("{w-")) && a.ends_with('}') && a[3..a.len() - 1].parse::<f64>().is_ok()));
            if !ok { return Err(format!("bad number '{a}' in \"{d}\"")); }
        }
        rest = rest[end..].trim_start();
    }
    Ok(())
}
fn check(root: &Path) -> Result<usize, String> {
    let (_, s) = load(root)?;
    let mut errs: Vec<String> = vec![];
    if s["schema"] != 1 { errs.push("schema must be 1".into()); }
    let parts = s["parts"].as_object().ok_or("parts missing")?;
    let state_fields: BTreeSet<&str> = s["stateFields"].as_array().ok_or("stateFields missing")?.iter().filter_map(Value::as_str).collect();
    let quantities: BTreeSet<&str> = s["quantities"].as_object().ok_or("quantities missing")?.keys().map(String::as_str).collect();
    let leds: BTreeSet<&str> = s["ledColors"].as_object().ok_or("ledColors missing")?.keys().map(String::as_str).collect();
    let mut grouped: Vec<&str> = vec![];
    for g in s["groups"].as_array().ok_or("groups missing")? { if let Some(a) = g["parts"].as_array() { for c in a { grouped.push(c.as_str().unwrap_or("")); } } }
    for (code, p) in parts {
        let mut e = |m: String| errs.push(format!("{code}: {m}"));
        let pins = p["pins"].as_array().map(|a| a.len()).unwrap_or(0);
        if pins == 0 { e("no pins".into()); }
        for pn in p["pins"].as_array().unwrap_or(&vec![]) { if !(pn["x"].is_i64() && pn["y"].is_i64()) { e("pin coordinates must be integers".into()); } }
        if grouped.iter().filter(|g| **g == code.as_str()).count() != 1 { e("must appear in exactly one palette group".into()); }
        let mut seen = BTreeSet::new(); let mut keys = BTreeSet::new();
        for pa in p["params"].as_array().unwrap_or(&vec![]) {
            let k = pa["key"].as_str().unwrap_or("");
            if !KEYS.contains(&k) { e(format!("param key '{k}' is not one of v f p q s k")); }
            if !seen.insert(k.to_string()) { e(format!("duplicate param key '{k}'")); }
            keys.insert(k.to_string());
        }
        for l in p["symbol"].as_array().unwrap_or(&vec![]) {
            if l["widget"].is_string() { continue; }
            match l["d"].as_str() { Some(d) => if let Err(m) = check_path(d, l["wiper"] == true) { e(m) }, None => e("layer without d or widget".into()) }
            if let Some(w) = l.get("when") {
                if let Some(st) = w["state"].as_str() { if !state_fields.contains(st) { e(format!("when.state '{st}' is not a state field")); } }
                else if let Some(pk) = w["param"].as_str() { if !keys.contains(pk) { e(format!("when.param '{pk}' is not a param of this part")); } }
                else { e("when needs state or param".into()); }
            }
        }
        for st in p["state"].as_array().unwrap_or(&vec![]) { if !state_fields.contains(st.as_str().unwrap_or("")) && !["ta", "tb"].contains(&st.as_str().unwrap_or("")) { e(format!("state '{st}' unknown")); } }
        for l in p["limits"].as_array().unwrap_or(&vec![]) {
            if !quantities.contains(l["q"].as_str().unwrap_or("")) { e(format!("limit quantity '{}' unknown", l["q"])); }
            if !(l["warn"].as_f64().unwrap_or(f64::NAN) <= l["err"].as_f64().unwrap_or(f64::NAN)) { e("limit warn must be <= err".into()); }
            if let Some(r) = l.get("ref") { if !keys.contains(r["key"].as_str().unwrap_or("")) { e(format!("limit ref '{}' is not a param of this part", r["key"])); } }
        }
        for key in ["dcPath", "shortPath"] { for g in p[key].as_array().unwrap_or(&vec![]) { for pr in g["join"].as_array().unwrap_or(&vec![]) { for x in pr.as_array().unwrap_or(&vec![]) {
            if let Some(i) = x.as_u64() { if i as usize >= pins { e(format!("{key} pin {i} out of range")); } } else if x != "gnd" { e(format!("{key} entry {x} invalid")); } } } } }
        if let Some(g) = p.get("glow") { if let Some(m) = g["color"]["map"].as_str() { if m != "ledColors" || leds.is_empty() { e("glow map must be ledColors".into()); } } }
    }
    let tol: Vec<&str> = parts.iter().filter(|(_, p)| p["tolerance"] == true).map(|(c, _)| c.as_str()).collect();
    let want: Vec<&str> = s["tolerance"]["parts"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
    let (mut a, mut b) = (tol.clone(), want.clone()); a.sort(); b.sort();
    if a != b { errs.push(format!("tolerance parts mismatch: flags {tol:?} vs top-level {want:?}")); }
    for g in grouped { if !parts.contains_key(g) { errs.push(format!("group lists unknown part '{g}'")); } }
    if errs.is_empty() { Ok(parts.len()) } else { Err(format!("{} problem(s):\n  {}", errs.len(), errs.join("\n  "))) }
}

// ------------------------------------------------------------------ compact runtime spec for CosLog Y
// {h, G:[[group,[codes]]], L:{color:[is,rgb]}, P:{code:{n,x,p,l,b,N,v,f,a,u,k,y}}}
//   n name  x prefix  p pins [[x,y]]  l pin labels  b bbox override  N nonlinear  v/f placement defaults
//   a params [[key,name,unit,default?,options?]]  u value-label unit  k keywords
//   y layers: "path" | [path, when?, flags?] | ["@scope"]; when = ["s",state,eq] | ["b",state,bit] | ["p",param,[in..]]; flags: 1 wiper, 2 spin
fn compact_when(w: &Value) -> Value {
    if let Some(s) = w["state"].as_str() { if let Some(b) = w.get("bit") { return json!(["b", s, b]); } return json!(["s", s, w["eq"]]); }
    json!(["p", w["param"], w["in"]])
}
fn compact(s: &Value, h: &str) -> Value {
    let mut parts = Map::new();
    for (code, p) in s["parts"].as_object().unwrap() {
        let mut o = Map::new();
        o.insert("n".into(), p["name"].clone());
        if let Some(x) = p.get("prefix") { o.insert("x".into(), x.clone()); }
        o.insert("p".into(), Value::Array(p["pins"].as_array().unwrap().iter().map(|q| json!([q["x"], q["y"]])).collect()));
        if p["pins"].as_array().unwrap().iter().any(|q| q.get("label").is_some()) {
            o.insert("l".into(), Value::Array(p["pins"].as_array().unwrap().iter().map(|q| q.get("label").cloned().unwrap_or(Value::Null)).collect())); }
        if p["bbox"].is_array() { o.insert("b".into(), p["bbox"].clone()); }
        if p["nonlinear"] == true { o.insert("N".into(), json!(1)); }
        for k in ["v", "f"] { if let Some(d) = p["defaults"].get(k) { if !d.is_null() { o.insert(k.into(), d.clone()); } } }
        let a: Vec<Value> = p["params"].as_array().unwrap().iter().map(|pa| {
            let k = pa["key"].as_str().unwrap();
            let mut v = vec![pa["key"].clone(), pa["name"].clone(), pa["unit"].clone()];
            // v/f defaults equal to the placement value are implied by v/f above
            let def = pa.get("default").filter(|d| !(( k == "v" || k == "f") && p["defaults"].get(k) == Some(*d)));
            if def.is_some() || pa.get("options").is_some() { v.push(def.cloned().unwrap_or(Value::Null)); }
            if let Some(op) = pa.get("options") { v.push(op.clone()); }
            Value::Array(v) }).collect();
        if !a.is_empty() { o.insert("a".into(), Value::Array(a)); }
        if let Some(u) = p["labelUnit"].as_str() { if !u.is_empty() { o.insert("u".into(), json!(u)); } }
        o.insert("k".into(), p["keywords"].clone());
        o.insert("y".into(), Value::Array(p["symbol"].as_array().unwrap().iter().map(|l| {
            if let Some(w) = l["widget"].as_str() { return json!([format!("@{w}")]); }
            let d = l["d"].clone(); let fl = if l["wiper"] == true { 1 } else if l.get("spin").is_some() { 2 } else { 0 };
            if l.get("when").is_none() && fl == 0 { d } else if fl == 0 { json!([d, compact_when(&l["when"])]) } else { json!([d, if l.get("when").is_some() { compact_when(&l["when"]) } else { Value::Null }, fl]) }
        }).collect()));
        parts.insert(code.clone(), Value::Object(o));
    }
    let led: Map<String, Value> = s["ledColors"].as_object().unwrap().iter().map(|(k, v)| (k.clone(), json!([v["is"], v["rgb"]]))).collect();
    json!({ "h": h, "G": s["groups"].as_array().unwrap().iter().map(|g| json!([g["name"], g["parts"]])).collect::<Vec<_>>(), "L": led, "P": parts })
}
fn compact_string(root: &Path) -> Result<(String, String), String> {
    check(root)?; let (b, s) = load(root)?; let h = hash(&b);
    Ok((serde_json::to_string(&compact(&s, &h)).unwrap(), h))
}
fn minify_cmd(root: &Path, out: Option<&String>) -> Result<(), String> {
    let (c, h) = compact_string(root)?;
    match out { Some(f) => { fs::write(f, &c).map_err(|e| e.to_string())?; eprintln!("wrote {f}: {} bytes (spec {h})", c.len()); } None => println!("{c}") }
    Ok(())
}

// ------------------------------------------------------------------ build-y
/// Conservative whitespace pass: drop blank lines, full-line // and /* */ comments, and leading indentation.
/// Never touches text inside a line, so strings and regexes are safe.
fn squeeze(src: &str) -> String {
    let mut o = String::with_capacity(src.len());
    for l in src.lines() {
        let t = l.trim_start();
        if t.is_empty() || t.starts_with("//") || (t.starts_with("/*") && t.trim_end().ends_with("*/") && !t.contains("@@")) { continue; }
        o.push_str(t.trim_end()); o.push('\n');
    }
    o
}
fn build_y(root: &Path) -> Result<(), String> {
    let tpl_path = root.join("coslog-y/src/template.html");
    let tpl = fs::read_to_string(&tpl_path).map_err(|e| format!("{}: {e}", tpl_path.display()))?;
    if tpl.matches(TOKEN).count() != 1 { return Err(format!("template must contain {TOKEN} exactly once")); }
    let (spec, h) = compact_string(root)?;
    let body = squeeze(&tpl).replace(TOKEN, &spec);
    let out = root.join("coslog-y/CosLog-Y.html");
    fs::write(&out, &body).map_err(|e| e.to_string())?;
    let (t, sp) = (squeeze(&tpl).len() - TOKEN.len(), spec.len());
    println!("CosLog-Y.html  {} bytes ({:.1} KB) of {} budget", body.len(), body.len() as f64 / 1024.0, BUDGET);
    println!("  code + css + html {t} bytes, spec data {sp} bytes, spec hash {h}");
    if body.len() > BUDGET { return Err(format!("over budget by {} bytes", body.len() - BUDGET)); }
    Ok(())
}
