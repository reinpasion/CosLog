use crate::kind::Kind;
use serde::{de, Deserialize, Deserializer, Serialize};

/// Accept either a JSON string or number and keep it as a string (JS fields are loosely typed).
fn sn<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    match Option::<serde_json::Value>::deserialize(d)? {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::String(s)) => Ok(Some(s)),
        Some(serde_json::Value::Number(n)) => Ok(Some(n.to_string())),
        Some(o) => Err(de::Error::custom(format!("expected string/number, got {o}"))),
    }
}
fn flag<'de, D: Deserializer<'de>>(d: D) -> Result<u8, D::Error> {
    Ok(match serde_json::Value::deserialize(d)? {
        serde_json::Value::Bool(b) => b as u8,
        serde_json::Value::Number(n) => (n.as_f64().unwrap_or(0.0) != 0.0) as u8,
        _ => 0,
    })
}
fn z8(v: &u8) -> bool { *v == 0 }

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Component {
    pub t: Kind,
    pub x: f64,
    pub y: f64,
    #[serde(default)]
    pub r: u8,
    #[serde(default, deserialize_with = "flag", skip_serializing_if = "z8")]
    pub m: u8,
    #[serde(default, deserialize_with = "flag")]
    pub on: u8,
    #[serde(default, deserialize_with = "sn", skip_serializing_if = "Option::is_none")] pub v: Option<String>,
    #[serde(default, deserialize_with = "sn", skip_serializing_if = "Option::is_none")] pub f: Option<String>,
    #[serde(default, deserialize_with = "sn", skip_serializing_if = "Option::is_none")] pub n: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub no: Option<[f64; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub vo: Option<[f64; 2]>,
    #[serde(default, deserialize_with = "sn", skip_serializing_if = "Option::is_none")] pub u: Option<String>,
    #[serde(default, deserialize_with = "sn", skip_serializing_if = "Option::is_none")] pub p: Option<String>,
    #[serde(default, deserialize_with = "sn", skip_serializing_if = "Option::is_none")] pub q: Option<String>,
    #[serde(default, deserialize_with = "sn", skip_serializing_if = "Option::is_none")] pub s: Option<String>,
    #[serde(default, deserialize_with = "sn", skip_serializing_if = "Option::is_none")] pub k: Option<String>,
    /// Position in the file as read (counting skipped parts); the tolerance index. Not saved.
    #[serde(skip)] pub si: Option<usize>,
}

impl Component {
    pub fn new(t: Kind, x: f64, y: f64, r: u8) -> Self {
        let (v, f) = t.defaults();
        let mut c = Component { t, x, y, r: if t == Kind::Os { 0 } else { r }, m: 0, on: 0,
            v: v.map(Into::into), f: f.map(Into::into), n: None, no: None, vo: None,
            u: None, p: None, q: None, s: None, k: None, si: None };
        for pr in crate::params::params(t) { if let Some(d) = pr.def { c.set(pr.key, d.into()); } }
        c
    }
    pub fn get(&self, key: &str) -> Option<&str> {
        match key { "v" => &self.v, "f" => &self.f, "p" => &self.p, "q" => &self.q, "s" => &self.s, "k" => &self.k, "n" => &self.n, "u" => &self.u, _ => &None }.as_deref()
    }
    pub fn set(&mut self, key: &str, val: String) {
        let v = Some(val);
        match key { "v" => self.v = v, "f" => self.f = v, "p" => self.p = v, "q" => self.q = v, "s" => self.s = v, "k" => self.k = v, "n" => self.n = v, "u" => self.u = v, _ => {} }
    }
    fn xf(&self, px: f64, py: f64) -> (f64, f64) {
        let (mut x, mut y) = (if self.m != 0 { -px } else { px }, py);
        for _ in 0..self.r & 3 { (x, y) = (-y, x); }
        (self.x + x, self.y + y)
    }
    /// World-space pin positions: mirror X, rotate r×90° clockwise, translate.
    pub fn pins(&self) -> Vec<(f64, f64)> {
        self.t.pins().iter().map(|p| self.xf(p.0, p.1)).collect()
    }
    /// World-space axis-aligned bounding box.
    pub fn bbox(&self) -> [f64; 4] {
        let b = self.t.bbox();
        let q = [(b[0], b[1]), (b[2], b[1]), (b[0], b[3]), (b[2], b[3])].map(|p| self.xf(p.0, p.1));
        let f = |g: fn(&(f64, f64)) -> f64, m: fn(f64, f64) -> f64, s: f64| q.iter().map(g).fold(s, m);
        [f(|a| a.0, f64::min, f64::MAX), f(|a| a.1, f64::min, f64::MAX),
         f(|a| a.0, f64::max, f64::MIN), f(|a| a.1, f64::max, f64::MIN)]
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct Wire { pub x1: f64, pub y1: f64, pub x2: f64, pub y2: f64 }
impl Wire {
    /// Is the point on this axis-aligned segment? (JS `onSeg`)
    pub fn on_seg(&self, p: (f64, f64)) -> bool {
        (self.x1 == self.x2 && p.0 == self.x1 && p.1 >= self.y1.min(self.y2) && p.1 <= self.y1.max(self.y2))
            || (self.y1 == self.y2 && p.1 == self.y1 && p.0 >= self.x1.min(self.x2) && p.0 <= self.x1.max(self.x2))
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct View { pub x: f64, pub y: f64, pub z: f64 }
impl Default for View { fn default() -> Self { View { x: 60.0, y: 40.0, z: 1.0 } } }

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Project {
    #[serde(default = "app_name")] pub app: String,
    #[serde(default = "one")] pub ver: u32,
    /// Tolerance seed; `None` means the default 0x5EED.
    #[serde(default, skip_serializing_if = "default_seed", serialize_with = "seed_ser")] pub seed: Option<u64>,
    pub c: Vec<Component>,
    pub w: Vec<Wire>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub view: Option<View>,
}
fn app_name() -> String { "CosLog".into() }
pub const DEFAULT_SEED: u64 = 0x5EED;
fn default_seed(v: &Option<u64>) -> bool { v.map_or(true, |x| x == DEFAULT_SEED) }
const MAX_SAFE: u64 = 1 << 53;
/// Seeds above 2^53 are written as strings so JSON readers do not lose precision.
fn seed_ser<S: serde::Serializer>(v: &Option<u64>, s: S) -> Result<S::Ok, S::Error> {
    match v { Some(x) if *x != DEFAULT_SEED && *x > MAX_SAFE => s.serialize_str(&x.to_string()), Some(x) if *x != DEFAULT_SEED => s.serialize_u64(*x), _ => s.serialize_none() }
}
fn parse_seed(v: &serde_json::Value) -> Option<u64> {
    match v {
        serde_json::Value::Number(n) => n.as_u64().or_else(|| n.as_f64().filter(|f| *f >= 0.0 && f.fract() == 0.0 && *f <= MAX_SAFE as f64).map(|f| f as u64)),
        serde_json::Value::String(t) => { let t = t.trim(); match t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) { Some(h) => u64::from_str_radix(h, 16).ok(), None => t.parse().ok() } }
        _ => None,
    }
}
fn one() -> u32 { 1 }

/// Result of reading a project file: the project plus human-readable warnings (skipped parts, odd seed, ...).
#[derive(Debug)]
pub struct Loaded { pub project: Project, pub warnings: Vec<String> }

impl Project {
    pub fn seed(&self) -> u64 { self.seed.unwrap_or(DEFAULT_SEED) }
    /// Reads a project, dropping any warnings. Use [`Project::load_json`] to see them.
    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> { Ok(Self::load_json(s)?.project) }
    /// Reads a project file. Fails only when the text is not JSON or `c`/`w` are not arrays. Each part is
    /// parsed on its own, so an unknown or malformed part is skipped with a warning and the rest loads.
    pub fn load_json(s: &str) -> Result<Loaded, serde_json::Error> {
        use serde_json::Value;
        let v: Value = serde_json::from_str(s)?;
        let bad = |m: &str| <serde_json::Error as de::Error>::custom(m.to_string());
        let o = v.as_object().ok_or_else(|| bad("project must be an object"))?;
        let cs = o.get("c").and_then(Value::as_array).ok_or_else(|| bad("`c` must be an array"))?;
        let ws = o.get("w").and_then(Value::as_array).ok_or_else(|| bad("`w` must be an array"))?;
        let mut warnings = vec![];
        let app = o.get("app").and_then(Value::as_str).map(String::from).unwrap_or_else(app_name);
        if !matches!(app.as_str(), "CosLog" | "CircuitLite") { warnings.push(format!("Unrecognised app \"{app}\"; loading anyway")); }
        let ver = o.get("ver").and_then(Value::as_u64).unwrap_or(1) as u32;
        if ver > 1 { warnings.push("This file was saved by a newer CosLog".into()); }
        let seed = match o.get("seed") { None | Some(Value::Null) => None, Some(x) => match parse_seed(x) { Some(n) => Some(n), None => { warnings.push("Unreadable tolerance seed; using the default".into()); None } } };
        let (mut c, mut skipped) = (vec![], vec![]);
        for (i, e) in cs.iter().enumerate() {
            match serde_json::from_value::<Component>(e.clone()) {
                Ok(mut p) => { p.si = Some(i); if p.n.as_deref() == Some("") { p.n = None; } c.push(p); }
                Err(_) => skipped.push(e.get("t").and_then(Value::as_str).unwrap_or("?").to_string()),
            }
        }
        if !skipped.is_empty() {
            let mut u: Vec<String> = vec![]; for k in &skipped { if !u.contains(k) { u.push(k.clone()); } }
            warnings.push(format!("Skipped {} unknown part(s): {} (made with a newer or different CosLog)", skipped.len(), u.join(", ")));
        }
        let w: Vec<Wire> = serde_json::from_value(Value::Array(ws.clone()))?;
        let view = o.get("view").and_then(|x| serde_json::from_value::<View>(x.clone()).ok());
        let mut project = Project { app, ver, seed, c, w, view };
        project.apply_defaults();
        Ok(Loaded { project, warnings })
    }
    /// Like the JS `load()`: fields missing from a file take the part's defaults (value, frequency, tolerance, ...).
    pub fn apply_defaults(&mut self) {
        for c in &mut self.c {
            let d = Component::new(c.t, 0.0, 0.0, 0);
            if c.v.is_none() { c.v = d.v }
            if c.f.is_none() { c.f = d.f }
            if c.p.is_none() { c.p = d.p }
            if c.q.is_none() { c.q = d.q }
            if c.s.is_none() { c.s = d.s }
            if c.k.is_none() { c.k = d.k }
        }
    }
    pub fn to_json(&self) -> String { serde_json::to_string_pretty(self).unwrap() }
}

impl Project {
    pub fn empty() -> Self { Project { app: "CosLog".into(), ver: 1, seed: None, c: vec![], w: vec![], view: None } }
    /// One of every part type on a grid (used for the symbol gallery / visual checks).
    pub fn gallery() -> Self {
        let mut p = Project::empty();
        let mut i = 0;
        for &k in Kind::ALL.iter().filter(|k| **k != Kind::Os) {
            let mut c = Component::new(k, 110.0 + (i % 8) as f64 * 200.0, 120.0 + (i / 8) as f64 * 200.0, 0);
            c.n = Some(k.code().to_uppercase());
            if k == Kind::Led { c.p = Some("green".into()); }
            if k == Kind::Sg { c.k = Some("triangle".into()); }
            p.c.push(c); i += 1;
        }
        let mut o = Component::new(Kind::Os, 110.0, 1500.0, 0); o.n = Some("OSC".into()); p.c.push(o);
        let mut a = Component::new(Kind::Npn, 500.0, 1500.0, 1); a.n = Some("rot90".into()); p.c.push(a);
        let mut b = Component::new(Kind::Pnp, 700.0, 1500.0, 0); b.m = 1; b.n = Some("mirror".into()); p.c.push(b);
        let mut s = Component::new(Kind::Sw, 900.0, 1500.0, 0); s.on = 1; s.n = Some("closed".into()); p.c.push(s);
        let mut x = Component::new(Kind::Op, 1100.0, 1500.0, 3); x.n = Some("rot270".into()); p.c.push(x);
        p
    }
}
