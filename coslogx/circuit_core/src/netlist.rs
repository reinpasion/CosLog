//! Netlist compiler: union-find node coalescing + island partitioning.
use crate::kind::Kind;
use crate::project::{Component, Project};
use crate::units::pv;
use std::collections::HashMap;

pub const NS: usize = 200; // oscilloscope samples

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Wave { Sine, Square, Triangle, Sawtooth }

/// A compiled part: parsed parameters + all runtime state.
#[derive(Clone, Debug)]
pub struct Part {
    pub kind: Kind,
    pub src: usize,            // index into Project.c
    pub nd: Vec<usize>,        // node ids (global in Netlist, island-local in Island)
    pub gnd: Vec<usize>,       // global node ids (kept for lookups/diagnostics)
    pub name: Option<String>,
    pub val: f64, pub fr: f64, pub pn: f64, pub qn: f64, pub sn: f64,
    pub led_is: f64, pub wave: Wave,
    pub row: Option<usize>,    // voltage-source row (island-local)
    pub g: [f64; 3],           // transformer inverse-inductance matrix
    pub iv: f64, pub nt: f64,  // scope sample interval / next sample time
    // ---- dynamic state ----
    pub on: u8, pub pp: f64, pub rv: f64,
    pub vp: f64, pub ip: f64, pub is: f64, pub av: f64, pub sq: f64, pub gl: f64,
    pub cnt: u32, pub pc: bool, pub st: bool, pub qs: bool, pub bv: f64, pub ht: f64, pub bl: bool,
    pub w: f64, pub ang: f64, pub cur: f64,
    pub pi: f64, pub pv: f64, pub ia: f64, pub pw: f64,
    pub ta: Vec<f32>, pub tb: Vec<f32>, pub ti: usize, pub wt: usize, pub py: f64, pub tl: f64,
}

impl Part {
    /// Copy runtime state from another instance of the same part (used when rebuilding).
    pub fn copy_dyn(&mut self, o: &Part) {
        if self.kind != o.kind { return; }
        self.on = o.on; self.vp = o.vp; self.ip = o.ip; self.is = o.is; self.av = o.av; self.sq = o.sq;
        self.gl = o.gl; self.cnt = o.cnt; self.pc = o.pc; self.st = o.st; self.qs = o.qs; self.bv = o.bv;
        self.ht = o.ht; self.bl = o.bl; self.w = o.w; self.ang = o.ang; self.cur = o.cur;
        self.pi = o.pi; self.pv = o.pv; self.ia = o.ia; self.pw = o.pw;
        if !o.ta.is_empty() { self.ta = o.ta.clone(); self.tb = o.tb.clone(); self.ti = o.ti; self.wt = o.wt; self.py = o.py; self.tl = o.tl; self.nt = o.nt; }
    }
    /// LDR / NTC resistance from the light / temperature setting; solar light fraction.
    pub fn rlive(&mut self, f: &str) {
        let nz = |x: f64, d: f64| if x.is_nan() || x == 0.0 { d } else { x };
        match self.kind {
            Kind::Sol => self.pp = (nz(pv(f), 0.0) / 100.0).clamp(0.0, 1.0),
            Kind::Ldr => { let l = (nz(pv(f), 0.0) / 100.0).clamp(0.0, 1.0); self.rv = (200.0 + (self.val - 200.0) * (1.0 - l) * (1.0 - l)).max(1.0); }
            Kind::Ntc => { let t = nz(pv(f), 25.0) + 273.15; self.rv = (self.val * (3950.0 * (1.0 / t - 1.0 / 298.15)).exp()).max(1.0); }
            _ => {}
        }
    }
}

fn nzf(x: f64) -> f64 { if x.is_nan() { 0.0 } else { x } }

fn led_is(p: &str) -> f64 {
    match p { "yellow" => 1e-19, "green" => 1e-20, "blue" => 1e-27, "white" => 1e-26, _ => 1e-18 }
}

/// Deterministic pseudo-random in [-1,1) for component tolerance.
pub fn rnd(seed: u64, i: usize) -> f64 {
    let mut z = seed.wrapping_add((i as u64 + 1).wrapping_mul(0x9E3779B97F4A7C15));
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^= z >> 31;
    (z >> 11) as f64 / (1u64 << 52) as f64 - 1.0
}

pub fn make_part(c: &Component, src: usize, nd: Vec<usize>, seed: u64) -> Part {
    let t = c.t;
    let s = |o: &Option<String>| pv(o.as_deref().unwrap_or("undefined"));
    let mut val = s(&c.v);
    if !(val > 0.0) && t != Kind::Dc { val = if t == Kind::Tr { 1.0 } else { 1e-3 }; }
    let mut fr = s(&c.f);
    if !(fr > 0.0) { fr = 50.0; }
    let (pn, qn, sn) = (s(&c.p), s(&c.q), s(&c.s));
    if matches!(t, Kind::R | Kind::L | Kind::C) && pn > 0.0 { val *= 1.0 + pn / 100.0 * rnd(seed, c.si.unwrap_or(src)); }
    let wave = match c.k.as_deref() { Some("square") => Wave::Square, Some("triangle") => Wave::Triangle, Some("sawtooth") => Wave::Sawtooth, _ => Wave::Sine };
    let os = t == Kind::Os;
    let mut p = Part {
        kind: t, src, gnd: nd.clone(), nd, name: c.n.clone(), val, fr, pn, qn, sn,
        led_is: led_is(c.p.as_deref().unwrap_or("red")), wave, row: None, g: [0.0; 3], iv: 0.0, nt: 0.0,
        on: c.on, pp: 0.0, rv: 0.0, vp: 0.0, ip: 0.0, is: 0.0, av: 0.0, sq: 0.0, gl: 0.0,
        cnt: 0, pc: false, st: false, qs: false, bv: 0.0, ht: 0.0, bl: false, w: 0.0, ang: 0.0, cur: 0.0,
        pi: 0.0, pv: 0.0, ia: 0.0, pw: 0.0,
        ta: if os { vec![0.0; NS] } else { vec![] }, tb: if os { vec![0.0; NS] } else { vec![] },
        ti: NS, wt: 0, py: 0.0, tl: 0.0,
    };
    if t == Kind::C { p.vp = nzf(qn); }
    if t == Kind::L { p.ip = nzf(qn); }
    if t == Kind::Pot { p.pp = (nzf(s(&c.f)) / 100.0).clamp(0.0, 1.0); }
    if matches!(t, Kind::Ldr | Kind::Ntc | Kind::Sol) { p.rlive(c.f.as_deref().unwrap_or("undefined")); }
    p
}

pub struct Netlist {
    pub n_nodes: usize,        // includes ground (node 0)
    pub parts: Vec<Part>,      // same order as Project.c, nodes are global
    pub wire_nodes: Vec<Option<usize>>,
    pub has_ground: bool,
}

fn find(uf: &mut Vec<usize>, mut a: usize) -> usize {
    while uf[a] != a { uf[a] = uf[uf[a]]; a = uf[a]; }
    a
}
fn key(p: (f64, f64)) -> (u64, u64) { ((p.0 + 0.0).to_bits(), (p.1 + 0.0).to_bits()) }

/// Port of JS `build()` node coalescing: pins/wire endpoints -> union-find -> contiguous node ids.
pub fn build(p: &Project, seed: u64) -> Netlist {
    let mut uf: Vec<usize> = vec![];
    let mut pm: HashMap<(u64, u64), usize> = HashMap::new();
    let mut pts: Vec<(f64, f64)> = vec![];
    let mut id = |pt: (f64, f64), uf: &mut Vec<usize>| -> usize {
        *pm.entry(key(pt)).or_insert_with(|| { uf.push(uf.len()); pts.push(pt); uf.len() - 1 })
    };
    let cid: Vec<Vec<usize>> = p.c.iter().map(|c| c.pins().into_iter().map(|q| id(q, &mut uf)).collect()).collect();
    let wid: Vec<(usize, usize)> = p.w.iter().map(|w| (id((w.x1, w.y1), &mut uf), id((w.x2, w.y2), &mut uf))).collect();
    let g = uf.len();
    uf.push(g);
    let mut has_ground = false;
    for (c, ids) in p.c.iter().zip(&cid) {
        if c.t == Kind::Gnd { has_ground = true; let (a, b) = (find(&mut uf, ids[0]), find(&mut uf, g)); uf[a] = b; }
    }
    for (w, &(a, b)) in p.w.iter().zip(&wid) {
        let (ra, rb) = (find(&mut uf, a), find(&mut uf, b)); uf[ra] = rb;
        for (i, &pt) in pts.iter().enumerate() {
            if w.on_seg(pt) { let (x, y) = (find(&mut uf, i), find(&mut uf, a)); uf[x] = y; }
        }
    }
    let mut idx: HashMap<usize, usize> = HashMap::new();
    let r0 = find(&mut uf, g);
    idx.insert(r0, 0);
    let mut n = 1;
    let mut parts = vec![];
    for (si, (c, ids)) in p.c.iter().zip(&cid).enumerate() {
        let nd: Vec<usize> = ids.iter().map(|&i| { let r = find(&mut uf, i); *idx.entry(r).or_insert_with(|| { n += 1; n - 1 }) }).collect();
        parts.push(make_part(c, si, nd, seed));
    }
    let wire_nodes = wid.iter().map(|&(a, _)| { let r = find(&mut uf, a); idx.get(&r).copied() }).collect();
    Netlist { n_nodes: n, parts, wire_nodes, has_ground }
}

/// One independent section of the circuit (parts only coupled through Ground are separate islands).
pub struct IslandSpec {
    pub gn: Vec<usize>,        // local -> global node (gn[0] = 0)
    pub parts: Vec<Part>,      // nodes remapped to local
    pub n: usize, pub nv: usize, pub q: bool, pub dt: f64, pub cost: f64,
}

pub fn groupize(nl: &Netlist, qs: f64) -> Vec<IslandSpec> {
    let n = nl.n_nodes;
    let mut uf: Vec<usize> = (0..n).collect();
    for c in &nl.parts {
        let mut r = None;
        for &k in c.nd.iter().filter(|&&k| k != 0) {
            match r { None => r = Some(k), Some(r0) => { let (a, b) = (find(&mut uf, k), find(&mut uf, r0)); uf[a] = b; } }
        }
    }
    let mut gm: HashMap<usize, usize> = HashMap::new();
    let mut groups: Vec<Vec<usize>> = vec![]; // part indices
    for (i, c) in nl.parts.iter().enumerate() {
        if let Some(&k) = c.nd.iter().find(|&&k| k != 0) {
            let r = find(&mut uf, k);
            let gi = *gm.entry(r).or_insert_with(|| { groups.push(vec![]); groups.len() - 1 });
            groups[gi].push(i);
        }
    }
    groups.into_iter().map(|ids| {
        let mut nodes: Vec<usize> = ids.iter().flat_map(|&i| nl.parts[i].nd.iter().copied().filter(|&k| k != 0)).collect();
        nodes.sort_unstable(); nodes.dedup();
        let mut gn = vec![0]; gn.extend(nodes);
        let loc: HashMap<usize, usize> = gn.iter().enumerate().map(|(i, &k)| (k, i)).collect();
        let (mut nv, mut fmx, mut tm, mut q) = (0usize, 0f64, false, false);
        let mut parts: Vec<Part> = ids.iter().map(|&i| nl.parts[i].clone()).collect();
        for c in parts.iter_mut() {
            c.nd = c.nd.iter().map(|k| if *k == 0 { 0 } else { loc[k] }).collect();
            if matches!(c.kind, Kind::Dc | Kind::Ac | Kind::Sg) { c.row = Some(nv); nv += 1; }
            if matches!(c.kind, Kind::Ac | Kind::Sg) { fmx = fmx.max(c.fr); }
            if c.kind == Kind::Tm { tm = true; }
            if c.kind.nonlinear() { q = true; }
        }
        let base = if fmx > 0.0 { 1.0 / (fmx * 60f64.max((6e3 / fmx).ceil())) } else if tm { 2e-5 } else { 1e-4 };
        let dt = 2e-7f64.max(base) * qs;
        let nn = gn.len();
        let m = (nn - 1 + nv) as f64;
        let cost = (m * m * m / 3.0 + 8.0 * m * m + 60.0 * parts.len() as f64) * if q { 5.0 } else { 1.0 } / dt;
        IslandSpec { gn, parts, n: nn, nv, q, dt, cost }
    }).collect()
}
