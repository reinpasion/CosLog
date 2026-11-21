//! Static (pre-run) and dynamic (runtime) diagnostics, and fault highlighting
//! (port of the JS `statics`, `dyn`, `chk`, `scan`).
use crate::engine::{FrameStats, Sim};
use crate::kind::Kind;
use crate::netlist::{build, groupize, Part};
use crate::project::{Project, Wire};
use crate::spec::{self, StateView};
use crate::units::fm;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level { Error, Warn, Info }
#[derive(Clone, Debug, PartialEq)]
pub struct Diag { pub level: Level, pub msg: String }
fn dg(level: Level, msg: impl Into<String>) -> Diag { Diag { level, msg: msg.into() } }

/// Errors first, then warnings, then info (stable), as in the JS `render`.
pub fn merged(a: &[Diag], b: &[Diag]) -> Vec<Diag> { let mut v: Vec<Diag> = a.iter().chain(b).cloned().collect(); v.sort_by_key(|d| d.level); v }
/// Number shown on the Diagnostics button (info does not count).
pub fn problem_count(v: &[Diag]) -> usize { v.iter().filter(|d| d.level != Level::Info).count() }

struct Uf(Vec<usize>);
impl Uf {
    fn new(n: usize) -> Uf { Uf((0..n).collect()) }
    fn find(&mut self, mut a: usize) -> usize { while self.0[a] != a { self.0[a] = self.0[self.0[a]]; a = self.0[a]; } a }
    fn union(&mut self, a: usize, b: usize) { let (x, y) = (self.find(a), self.find(b)); self.0[x] = y; }
}
fn name(p: &Project, i: usize) -> String { p.c[i].n.clone().filter(|n| !n.is_empty()).unwrap_or_else(|| p.c[i].t.code().to_string()) }
/// Pins that are allowed to float (outputs, optional controls), from the spec pin flags.
fn optional_pin(k: Kind, i: usize) -> bool { spec::part(k.code()).and_then(|p| p.pins.get(i)).map_or(false, |p| p.optional) }

/// Part state seen by spec `when` clauses.
pub struct PartView<'a>(pub &'a Part);
impl StateView for PartView<'_> {
    fn state(&self, key: &str) -> f64 {
        let p = self.0;
        match key { "on" => (p.on != 0) as u8 as f64, "bl" => p.bl as u8 as f64, "st" => p.st as u8 as f64, "qs" => p.qs as u8 as f64, "pc" => p.pc as u8 as f64,
            "cnt" => p.cnt as f64, "av" => p.av, "gl" => p.gl, "w" => p.w, "ang" => p.ang, "hi" => (p.av > p.val) as u8 as f64, other => panic!("unsupported state {other}") }
    }
    fn value(&self) -> f64 { self.0.val }
    fn param(&self, _: &str) -> Option<&str> { None }
}
/// Unions the nodes a part conducts DC between (spec `dcPath`).
fn dc_join(uf: &mut Uf, c: &Part) {
    let Some(s) = spec::part(c.kind.code()) else { return };
    for (a, b) in spec::joins(s.dc_path, &PartView(c)) {
        let n = |i: i8| if i < 0 { 0 } else { c.gnd[i as usize] };
        uf.union(n(a), n(b));
    }
}
/// Unions the nodes a part shorts together at low resistance (spec `shortPath`); returns whether it did.
fn short_join(uf: &mut Uf, c: &Part) -> bool {
    let Some(s) = spec::part(c.kind.code()) else { return false };
    let j = spec::joins(s.short_path, &PartView(c));
    for &(a, b) in &j { uf.union(c.gnd[a as usize], c.gnd[b as usize]); }
    !j.is_empty()
}

// ------------------------------------------------------------------ static checks
pub fn statics(proj: &Project, qs: f64) -> Vec<Diag> {
    if proj.c.is_empty() { return vec![dg(Level::Info, spec::msg("empty", &[]))]; }
    let nl = build(proj, proj.seed());
    let specs = groupize(&nl, qs);
    let n = nl.n_nodes;
    let mut o = vec![];
    if !nl.has_ground { o.push(dg(Level::Error, spec::msg("noGround", &[]))); }
    let (mut deg, mut uf) = (vec![0u32; n], Uf::new(n));
    for (i, c) in nl.parts.iter().enumerate() {
        let nd = &c.gnd; let t = c.kind;
        for &k in nd { deg[k] += 1; }
        if spec::part(t.code()).map_or(false, |s| s.row_source) && nd[0] == nd[1] { o.push(dg(Level::Error, spec::msg("shortedTerminals", &[("name", &name(proj, i))]))); }
        dc_join(&mut uf, c);
    }
    let (mut dang, mut fl): (Vec<String>, Vec<String>) = (vec![], vec![]);
    for (i, c) in nl.parts.iter().enumerate() {
        if !matches!(c.kind, Kind::Gnd | Kind::Lg) {
            for (pi, &k) in c.gnd.iter().enumerate() { if k != 0 && deg[k] == 1 && !optional_pin(c.kind, pi) { dang.push(format!("{} pin {}", name(proj, i), pi + 1)); } }
        }
        if c.kind != Kind::Gnd {
            for &k in &c.gnd { if k != 0 && uf.find(k) != uf.find(0) && !fl.contains(&name(proj, i)) { fl.push(name(proj, i)); } }
        }
    }
    if !dang.is_empty() { let mx = spec::DANGLE_LIST_MAX as usize;
        o.push(dg(Level::Warn, spec::msg("dangling", &[("list", &dang.iter().take(mx).cloned().collect::<Vec<_>>().join(", ")), ("more", if dang.len() > mx { "..." } else { "" })]))); }
    if nl.has_ground && !fl.is_empty() {
        o.push(dg(Level::Warn, spec::msg("noDcPath", &[("list", &fl.iter().take(spec::NO_DC_LIST_MAX as usize).cloned().collect::<Vec<_>>().join(", "))])));
    }
    let dt = specs.iter().map(|g| g.dt).fold(f64::MAX, f64::min); let dt = if specs.is_empty() { 1e-4 * qs } else { dt };
    let q = nl.parts.iter().any(|c| c.kind.nonlinear());
    let ns = specs.len();
    let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
    let th = 1usize.max((cores.saturating_sub(1)).min(16).min(ns));
    let farm = if ns > 0 { spec::msg("farm", &[("n", &ns.to_string()), ("s", if ns > 1 { "s" } else { "" }), ("w", &th.to_string()), ("s2", if th > 1 { "s" } else { "" })]) } else { String::new() };
    let newton = if q { spec::msg("newton", &[]) } else { String::new() };
    o.push(dg(Level::Info, spec::msg("info", &[("parts", &proj.c.len().to_string()), ("nodes", &(n - 1).to_string()), ("dt", &fm(dt, "s")), ("newton", &newton), ("farm", &farm)])));
    o
}

// ------------------------------------------------------------------ runtime
/// Fault marker for a part or wire; shown until `until` (ms), or indefinitely while paused / stopped.
#[derive(Clone, Debug)]
pub struct Fault { pub level: Level, pub msg: String, pub pin: Option<usize>, pub until: f64 }
#[derive(Default, Clone, Debug)]
pub struct Faults { pub comps: HashMap<usize, Fault>, pub wires: HashMap<usize, Fault> }
impl Faults {
    pub fn clear(&mut self) { self.comps.clear(); self.wires.clear(); }
    fn mark_c(&mut self, i: usize, level: Level, msg: &str, hold: f64) {
        if self.comps.get(&i).map_or(true, |f| f.level != Level::Error || level == Level::Error) { self.comps.insert(i, Fault { level, msg: msg.into(), pin: None, until: hold }); }
    }
    fn mark_w(&mut self, i: usize, level: Level, hold: f64) {
        if self.wires.get(&i).map_or(true, |f| f.level != Level::Error || level == Level::Error) { self.wires.insert(i, Fault { level, msg: String::new(), pin: None, until: hold }); }
    }
}

/// Worst-case statistics gathered over the frames since the last diagnostic pass.
#[derive(Clone, Copy, Debug, Default)]
pub struct DynAcc { pub fail: u32, pub lag: u32, pub mv: f64, pub md: f64, pub bn: usize }
impl DynAcc {
    pub fn add(&mut self, s: &FrameStats) {
        self.fail += s.fails; if s.fails > 0 { self.md = s.md; } if s.lag { self.lag += 1; }
        if s.max_v > self.mv { self.mv = s.max_v; self.bn = s.bn; }
    }
}

/// Per-part stress check against its ratings, driven by the spec `limits` list.
pub fn check_part(c: &Part) -> Option<(Level, String)> {
    let s = spec::part(c.kind.code())?;
    let m = spec::Measured { peak_i: c.pi, avg_i: c.ia, peak_v: c.pv, power: c.pw };
    let r = spec::Ratings { q: c.qn, p: c.pn, s: c.sn };
    if let Some((err, msg, a, e, u)) = spec::eval_limits(s, &m, &r) {
        let t = if err { "limitErr" } else { "limitWarn" };
        return Some((if err { Level::Error } else { Level::Warn }, spec::msg(t, &[("msg", msg), ("value", &fm(a, u)), ("limit", &fm(e, u))])));
    }
    if s.row_source { return c.row.filter(|_| c.ia > spec::SOURCE_SHORT_A).map(|_| (Level::Error, spec::msg("sourceShort", &[("i", &fm(c.ia, "A"))]))); }
    if s.state.contains(&"bl") && c.bl { return Some((Level::Warn, spec::msg("fuseFault", &[]))); }
    None
}

/// Runtime diagnostics + fault marking. `now` is wall-clock milliseconds; faults persist 3 s.
pub fn dynamic(sim: &mut Sim, proj: &Project, faults: &mut Faults, acc: &DynAcc, qa: bool, qs: f64, now: f64) -> Vec<Diag> {
    let mut o = vec![];
    if acc.fail > 0 { o.push(dg(Level::Error, spec::msg("noConverge", &[("n", &acc.fail.to_string()), ("md", &fm(acc.md, "V"))]))); }
    if acc.lag as f64 > spec::LAG_WARN_FRAMES {
        let hint = if qa { if qs > 1.0 { spec::msg("lagHintAuto", &[]) } else { String::new() } } else { spec::msg("lagHintManual", &[]) };
        o.push(dg(Level::Warn, spec::msg("lag", &[("hint", &hint)])));
    }
    if acc.mv > spec::NODE_VOLTAGE_ERR_V { o.push(dg(Level::Error, spec::msg("nodeVoltage", &[("v", &fm(acc.mv, "V"))]))); }
    for i in 0..proj.c.len() {
        let Some(p) = sim.part(i) else { continue };
        let n = name(proj, i);
        if p.row.is_some() { let ix = sim.source_current(i).abs(); if ix > spec::SOURCE_RUNAWAY_A { o.push(dg(Level::Error, spec::msg("sourceRunaway", &[("name", &n), ("i", &fm(ix, "A"))]))); } }
        if p.kind == Kind::Led && p.cur > spec::LED_WARN_A { o.push(dg(Level::Warn, spec::msg("ledCurrent", &[("name", &n), ("i", &fm(p.cur, "A"))]))); }
        // blown-state parts (fuses) come from the spec `state` list, not a hard-coded kind
        if p.bl && spec::part(p.kind.code()).map_or(false, |s| s.state.contains(&"bl")) { o.push(dg(Level::Info, spec::msg("fuseBlown", &[("name", &n)]))); }
    }
    scan(sim, proj, faults, acc, now, &mut o);
    o
}

fn scan(sim: &mut Sim, proj: &Project, faults: &mut Faults, acc: &DynAcc, now: f64, o: &mut Vec<Diag>) {
    use Kind::*;
    let hold = now + spec::HOLD_MS;
    faults.comps.retain(|_, f| f.until >= now); faults.wires.retain(|_, f| f.until >= now);
    let n = sim.n_nodes; let nc = proj.c.len();
    let mut deg = vec![0u32; n];
    for i in 0..nc { if let Some(p) = sim.part(i) { for &k in &p.gnd { deg[k] += 1; } } }
    // unconnected pins
    for i in 0..nc {
        let Some(p) = sim.part(i) else { continue };
        if matches!(p.kind, Gnd | Lg) { continue; }
        for (pi, &k) in p.gnd.iter().enumerate() {
            if k != 0 && deg[k] == 1 && !optional_pin(p.kind, pi) {
                if faults.comps.get(&i).map_or(true, |f| f.level != Level::Error) { faults.comps.insert(i, Fault { level: Level::Warn, msg: spec::msg("pinFault", &[("n", &(pi + 1).to_string())]), pin: Some(pi), until: hold }); }
                for (wi, wn) in sim.wire_nodes.iter().enumerate() { if *wn == Some(k) { faults.mark_w(wi, Level::Warn, hold); } }
            }
        }
    }
    // shorted voltage sources: trace the low-resistance path
    let mut uf = Uf::new(n); let mut low: Vec<usize> = vec![];
    for i in 0..nc {
        let Some(p) = sim.part(i) else { continue };
        if short_join(&mut uf, p) { low.push(i); }
    }
    let wroot: Vec<Option<usize>> = sim.wire_nodes.iter().map(|w| w.map(|k| uf.find(k))).collect();
    for i in 0..nc {
        let Some(p) = sim.part(i) else { continue };
        if p.row.is_none() || uf.find(p.gnd[0]) != uf.find(p.gnd[1]) { continue; }
        let rt = uf.find(p.gnd[0]);
        let via: Vec<usize> = low.iter().copied().filter(|&l| { let g0 = sim.part(l).unwrap().gnd[0]; uf.find(g0) == rt }).collect();
        faults.mark_c(i, Level::Error, &spec::msg("faultShort", &[]), hold);
        for &l in &via { faults.mark_c(l, Level::Error, &spec::msg("faultPath", &[]), hold); }
        let touch = |w: &Wire, q: (f64, f64)| (w.x1, w.y1) == q || (w.x2, w.y2) == q || w.on_seg(q);
        let mut seen: HashSet<usize> = HashSet::new(); let mut fr: Vec<usize> = vec![];
        for q in proj.c[i].pins() { for (wi, w) in proj.w.iter().enumerate() { if touch(w, q) && wroot[wi] == Some(rt) && seen.insert(wi) { fr.push(wi); } } }
        while let Some(wi) = fr.pop() {
            faults.mark_w(wi, Level::Error, hold);
            let w = proj.w[wi];
            for (vi, v) in proj.w.iter().enumerate() {
                if !seen.contains(&vi) && wroot[vi] == Some(rt) && (touch(v, (w.x1, w.y1)) || touch(v, (w.x2, w.y2)) || touch(&w, (v.x1, v.y1)) || touch(&w, (v.x2, v.y2))) { seen.insert(vi); fr.push(vi); }
            }
        }
        let through = if via.is_empty() { spec::msg("shortWiring", &[]) } else { via.iter().map(|&l| name(proj, l)).collect::<Vec<_>>().join(", ") };
        o.push(dg(Level::Error, spec::msg("shortTrace", &[("name", &name(proj, i)), ("via", &through)])));
    }
    // ratings
    for i in 0..nc {
        let Some(p) = sim.part(i) else { continue };
        if let Some((l, m)) = check_part(p) {
            faults.mark_c(i, l, &m, hold);
            o.push(dg(l, spec::msg("partFault", &[("name", &name(proj, i)), ("msg", &m)])));
            if p.row.is_some() && l == Level::Error {
                for (wi, wn) in sim.wire_nodes.iter().enumerate() { if let Some(k) = *wn { if k > 0 && (k == p.gnd[0] || k == p.gnd[1]) { faults.mark_w(wi, Level::Error, hold); } } }
            }
        }
    }
    for i in 0..nc { if let Some(p) = sim.part_mut(i) { p.pi = 0.0; p.pv = 0.0; } }
    if acc.mv > spec::NODE_VOLTAGE_ERR_V { for (wi, wn) in sim.wire_nodes.iter().enumerate() { if *wn == Some(acc.bn) { faults.mark_w(wi, Level::Error, hold); } } }
}
