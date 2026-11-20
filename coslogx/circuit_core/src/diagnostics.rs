//! Static (pre-run) and dynamic (runtime) diagnostics, and fault highlighting
//! (port of the JS `statics`, `dyn`, `chk`, `scan`).
use crate::engine::{FrameStats, Sim};
use crate::kind::Kind;
use crate::netlist::{build, groupize, Part};
use crate::project::{Project, Wire};
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
fn name(p: &Project, i: usize) -> String { p.c[i].n.clone().unwrap_or_else(|| p.c[i].t.code().to_string()) }
/// Pins that are allowed to float (outputs, optional controls).
fn optional_pin(k: Kind, i: usize) -> bool {
    (k == Kind::Dff && i > 1) || (k == Kind::Ctr && i > 0) || (k == Kind::Tm && i > 5) || (k == Kind::Tf && i > 1) || (matches!(k, Kind::Jk | Kind::Srf) && i > 2)
}

// ------------------------------------------------------------------ static checks
pub fn statics(proj: &Project, qs: f64) -> Vec<Diag> {
    if proj.c.is_empty() { return vec![dg(Level::Info, "Place parts, wire them (click a pin to start a wire), add a Ground, then press Run.")]; }
    let nl = build(proj, 1);
    let specs = groupize(&nl, qs);
    let n = nl.n_nodes;
    let mut o = vec![];
    if !nl.has_ground { o.push(dg(Level::Error, "No Ground part: node voltages and meters have no reference. Add a Ground.")); }
    let (mut deg, mut uf) = (vec![0u32; n], Uf::new(n));
    for (i, c) in nl.parts.iter().enumerate() {
        let nd = &c.gnd; let t = c.kind;
        for &k in nd { deg[k] += 1; }
        if matches!(t, Kind::Dc | Kind::Ac | Kind::Sg) && nd[0] == nd[1] { o.push(dg(Level::Error, format!("{} has both terminals on one node (shorted by wiring).", name(proj, i)))); }
        use Kind::*;
        if matches!(t, R | L | Dc | Ac | Sg | D | Sd | Led | Zd | Lamp | Mo | Am | Vm | Fu | Ldr | Ntc | Bz | Sol) || (matches!(t, Sw | Bt) && c.on != 0) || (t == Nb && c.on == 0) { uf.union(nd[0], nd[1]); }
        else { match t {
            Pot => { uf.union(nd[0], nd[2]); uf.union(nd[2], nd[1]); }
            Sp => uf.union(nd[0], if c.on != 0 { nd[2] } else { nd[1] }),
            Tr => { uf.union(nd[0], nd[1]); uf.union(nd[2], nd[3]); }
            Rl => { uf.union(nd[0], nd[1]); if c.on != 0 { uf.union(nd[2], nd[3]); } }
            Npn | Pnp => { uf.union(nd[1], nd[2]); uf.union(nd[0], nd[2]); }
            Nm | Pm => uf.union(nd[1], nd[2]),
            Op | And | Or | Nand | Nor | Xor => uf.union(nd[2], 0),
            Not => uf.union(nd[1], 0),
            Dff | Tf => { uf.union(nd[2], 0); uf.union(nd[3], 0); }
            Jk | Srf => { uf.union(nd[3], 0); uf.union(nd[4], 0); }
            Reg => uf.union(nd[2], nd[1]),
            Tm => { uf.union(nd[0], nd[1]); uf.union(nd[7], nd[0]); uf.union(nd[6], nd[0]); uf.union(nd[5], 0); uf.union(nd[4], nd[1]); }
            Br => { uf.union(nd[0], nd[2]); uf.union(nd[1], nd[2]); uf.union(nd[3], nd[0]); }
            Ctr => for k in 2..6 { uf.union(nd[k], 0); },
            _ => {}
        } }
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
    if !dang.is_empty() { o.push(dg(Level::Warn, format!("Unconnected pin(s): {}{}. Wire them or remove the part.", dang.iter().take(6).cloned().collect::<Vec<_>>().join(", "), if dang.len() > 6 { "..." } else { "" }))); }
    if nl.has_ground && !fl.is_empty() {
        o.push(dg(Level::Warn, format!("No DC path to ground at: {}. Readings there are unreliable. Connect to ground or add a bleed resistor (capacitors and open switches block DC).", fl.iter().take(6).cloned().collect::<Vec<_>>().join(", "))));
    }
    let dt = specs.iter().map(|g| g.dt).fold(f64::MAX, f64::min); let dt = if specs.is_empty() { 1e-4 * qs } else { dt };
    let q = nl.parts.iter().any(|c| c.kind.nonlinear());
    let ns = specs.len();
    let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
    let th = 1usize.max((cores.saturating_sub(1)).min(16).min(ns));
    let farm = if ns > 0 { format!(", {} independent section{} on {} thread{}", ns, if ns > 1 { "s" } else { "" }, th, if th > 1 { "s" } else { "" }) } else { String::new() };
    o.push(dg(Level::Info, format!("{} parts, {} nodes, step {}{}{}", proj.c.len(), n - 1, fm(dt, "s"), if q { ", Newton solver active" } else { "" }, farm)));
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

fn lim(a: f64, w: f64, e: f64, m: &str, u: &str) -> Option<(Level, String)> {
    if a > e { Some((Level::Error, format!("{m}{} exceeds {}", fm(a, u), fm(e, u)))) }
    else if a > w { Some((Level::Warn, format!("{m}{} near limit {}", fm(a, u), fm(e, u)))) } else { None }
}
/// Per-part stress check against its ratings (peak/average current, voltage, power).
pub fn check_part(c: &Part) -> Option<(Level, String)> {
    use Kind::*;
    let (i, a, vm, p) = (c.pi, c.ia, c.pv, c.pw);
    match c.kind {
        R => { let rt = if c.qn > 0.0 { c.qn } else { 0.25 }; lim(p, rt * 0.75, rt, "overheating ", "W") }
        Lamp => { let rt = if c.pn > 0.0 { c.pn } else { 0.5 }; lim(p, rt * 1.2, rt * 1.6, "overpowered ", "W") }
        C => { let rt = if c.sn > 0.0 { c.sn } else { 50.0 }; lim(vm, rt * 0.8, rt, "overvoltage ", "V") }
        L => lim(i, 3.0, 5.0, "overcurrent ", "A"),
        Tr => lim(a, 2.0, 4.0, "winding overcurrent ", "A").or_else(|| lim(i, 20.0, 40.0, "winding surge ", "A")),
        D => lim(a, 0.7, 1.0, "overcurrent ", "A").or_else(|| lim(i, 10.0, 30.0, "surge ", "A")).or_else(|| lim(vm, 80.0, 100.0, "reverse breakdown ", "V")),
        Sd => lim(a, 2.0, 3.0, "overcurrent ", "A").or_else(|| lim(i, 20.0, 40.0, "surge ", "A")).or_else(|| lim(vm, 32.0, 40.0, "reverse breakdown ", "V")),
        Led => lim(a, 0.025, 0.05, "overcurrent ", "A").or_else(|| lim(i, 0.15, 0.3, "surge ", "A")).or_else(|| lim(vm, 4.0, 6.0, "reverse voltage ", "V")),
        Zd => lim(p, 0.4, 0.5, "overpowered ", "W"),
        Npn | Pnp => lim(vm, 32.0, 40.0, "Vce ", "V").or_else(|| lim(a, 0.15, 0.4, "collector current ", "A")).or_else(|| lim(i, 0.6, 1.5, "surge ", "A")).or_else(|| lim(p, 0.4, 0.6, "dissipating ", "W")),
        Nm | Pm => lim(vm, 48.0, 60.0, "Vds ", "V").or_else(|| lim(a, 1.5, 3.0, "drain current ", "A")).or_else(|| lim(i, 6.0, 12.0, "surge ", "A")).or_else(|| lim(p, 0.8, 1.2, "dissipating ", "W")),
        _ if c.row.is_some() => if a > 15.0 { Some((Level::Error, format!("short circuit: supplying {}", fm(a, "A")))) } else { None },
        Am => lim(a, 3.0, 10.0, "overload ", "A"),
        Fu if c.bl => Some((Level::Warn, "blown (open)".into())),
        _ => None,
    }
}

/// Runtime diagnostics + fault marking. `now` is wall-clock milliseconds; faults persist 3 s.
pub fn dynamic(sim: &mut Sim, proj: &Project, faults: &mut Faults, acc: &DynAcc, qa: bool, qs: f64, now: f64) -> Vec<Diag> {
    let mut o = vec![];
    if acc.fail > 0 { o.push(dg(Level::Error, format!("Solver did not converge in {} step(s) (last change {}). Try: series resistance on diodes/LEDs/sources, lower op-amp gain, check transistor polarity, or lower the speed.", acc.fail, fm(acc.md, "V")))); }
    if acc.lag > 3 {
        o.push(dg(Level::Warn, format!("Cannot keep real-time on this CPU, so the simulation runs slower than the speed setting. {}",
            if qa { if qs > 1.0 { "Auto quality has coarsened the time step to keep up." } else { "" } } else { "Set Quality to Auto or Fast, or simplify the circuit." })));
    }
    if acc.mv > 1e3 { o.push(dg(Level::Error, format!("Node voltage reached {}: usually a floating node, a shorted ideal source, or a missing ground path.", fm(acc.mv, "V")))); }
    for i in 0..proj.c.len() {
        let Some(p) = sim.part(i) else { continue };
        let n = name(proj, i);
        if p.row.is_some() { let ix = sim.source_current(i).abs(); if ix > 20.0 { o.push(dg(Level::Error, format!("{n} supplies {}: probably a short circuit. Add series resistance.", fm(ix, "A")))); } }
        if p.kind == Kind::Led && p.cur > 0.05 { o.push(dg(Level::Warn, format!("{n} carries {}: add a current-limiting resistor.", fm(p.cur, "A")))); }
        if p.kind == Kind::Fu && p.bl { o.push(dg(Level::Info, format!("{n} has blown (open). Press Reset to replace it."))); }
    }
    scan(sim, proj, faults, acc, now, &mut o);
    o
}

fn scan(sim: &mut Sim, proj: &Project, faults: &mut Faults, acc: &DynAcc, now: f64, o: &mut Vec<Diag>) {
    use Kind::*;
    let hold = now + 3000.0;
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
                if faults.comps.get(&i).map_or(true, |f| f.level != Level::Error) { faults.comps.insert(i, Fault { level: Level::Warn, msg: format!("pin {} unconnected", pi + 1), pin: Some(pi), until: hold }); }
                for (wi, wn) in sim.wire_nodes.iter().enumerate() { if *wn == Some(k) { faults.mark_w(wi, Level::Warn, hold); } }
            }
        }
    }
    // shorted voltage sources: trace the low-resistance path
    let mut uf = Uf::new(n); let mut low: Vec<usize> = vec![];
    for i in 0..nc {
        let Some(p) = sim.part(i) else { continue };
        let nd = &p.gnd;
        if (matches!(p.kind, Sw | Bt) && p.on != 0) || (p.kind == Nb && p.on == 0) || p.kind == Am || (p.kind == Fu && !p.bl) || (p.kind == R && p.val < 0.5) { uf.union(nd[0], nd[1]); low.push(i); }
        else if p.kind == Sp { uf.union(nd[0], if p.on != 0 { nd[2] } else { nd[1] }); low.push(i); }
    }
    let wroot: Vec<Option<usize>> = sim.wire_nodes.iter().map(|w| w.map(|k| uf.find(k))).collect();
    for i in 0..nc {
        let Some(p) = sim.part(i) else { continue };
        if p.row.is_none() || uf.find(p.gnd[0]) != uf.find(p.gnd[1]) { continue; }
        let rt = uf.find(p.gnd[0]);
        let via: Vec<usize> = low.iter().copied().filter(|&l| { let g0 = sim.part(l).unwrap().gnd[0]; uf.find(g0) == rt }).collect();
        faults.mark_c(i, Level::Error, "short circuit", hold);
        for &l in &via { faults.mark_c(l, Level::Error, "in short-circuit path", hold); }
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
        let through = if via.is_empty() { "wiring".to_string() } else { via.iter().map(|&l| name(proj, l)).collect::<Vec<_>>().join(", ") };
        o.push(dg(Level::Error, format!("{} is short-circuited through {}. Marked in red on the schematic.", name(proj, i), through)));
    }
    // ratings
    for i in 0..nc {
        let Some(p) = sim.part(i) else { continue };
        if let Some((l, m)) = check_part(p) {
            faults.mark_c(i, l, &m, hold);
            o.push(dg(l, format!("{}: {}.", name(proj, i), m)));
            if p.row.is_some() && l == Level::Error {
                for (wi, wn) in sim.wire_nodes.iter().enumerate() { if let Some(k) = *wn { if k > 0 && (k == p.gnd[0] || k == p.gnd[1]) { faults.mark_w(wi, Level::Error, hold); } } }
            }
        }
    }
    for i in 0..nc { if let Some(p) = sim.part_mut(i) { p.pi = 0.0; p.pv = 0.0; } }
    if acc.mv > 1e3 { for (wi, wn) in sim.wire_nodes.iter().enumerate() { if *wn == Some(acc.bn) { faults.mark_w(wi, Level::Error, hold); } } }
}
