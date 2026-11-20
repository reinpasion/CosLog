//! MNA engine: dense Gaussian elimination, Newton-Raphson, device stamps, behavioral updates,
//! and the rayon-driven multi-island `Sim`.
use crate::kind::Kind;
use crate::netlist::*;
use crate::project::Project;
use rayon::prelude::*;
use std::f64::consts::PI;
use std::time::Instant;

fn nz(x: f64) -> f64 { if x.is_nan() { 0.0 } else { x } }
fn wv(w: Wave, p: f64, d: f64) -> f64 {
    match w {
        Wave::Square => if p < d { 1.0 } else { -1.0 },
        Wave::Triangle => if p < 0.5 { 4.0 * p - 1.0 } else { 3.0 - 4.0 * p },
        Wave::Sawtooth => 2.0 * p - 1.0,
        Wave::Sine => (2.0 * PI * p).sin(),
    }
}

pub struct Island {
    pub id: usize,
    pub parts: Vec<Part>,
    pub gn: Vec<usize>,
    pub n: usize, pub m: usize,
    a: Vec<f64>, z: Vec<f64>, x: Vec<f64>, xp: Vec<f64>,
    pub v: Vec<f64>,
    pub t: f64, pub dt: f64, pub q: bool, pub cost: f64,
    kf: f64, kp: f64, sc: u64,
    pub fail: u32, pub lag: bool, pub mv: f64, pub mn: usize, pub md: f64,
}

impl Island {
    pub fn new(id: usize, s: IslandSpec, t: f64) -> Island {
        let mut parts = s.parts;
        let dt = s.dt;
        for c in parts.iter_mut() {
            match c.kind {
                Kind::Os => { c.iv = dt.max(c.val * 10.0 / NS as f64); c.nt = 0.0; }
                Kind::Tr => {
                    let nn = c.val; let l2 = 1.0 / (nn * nn);
                    let mu = (if c.pn > 0.0 && c.pn < 1.0 { c.pn } else { 0.995 }) * l2.sqrt();
                    let d = l2 - mu * mu;
                    c.g = [dt * l2 / d, -dt * mu / d, dt / d];
                }
                _ => {}
            }
        }
        let m = s.n - 1 + s.nv;
        Island { id, parts, gn: s.gn, n: s.n, m, a: vec![0.0; m * m], z: vec![0.0; m], x: vec![0.0; m], xp: vec![0.0; m],
            v: vec![0.0; s.n], t, dt, q: s.q, cost: s.cost, kf: (-dt / 0.04).exp(), kp: (-4.0 * dt / 0.3).exp(), sc: 0,
            fail: 0, lag: false, mv: 0.0, mn: 0, md: 0.0 }
    }

    // ---- stamping primitives (index 0 = ground, skipped) ----
    #[inline] fn s(&mut self, p: usize, q: usize, r: usize, s: usize, g: f64) {
        let m = self.m;
        if p != 0 { if r != 0 { self.a[(p - 1) * m + r - 1] += g; } if s != 0 { self.a[(p - 1) * m + s - 1] -= g; } }
        if q != 0 { if r != 0 { self.a[(q - 1) * m + r - 1] -= g; } if s != 0 { self.a[(q - 1) * m + s - 1] += g; } }
    }
    #[inline] fn g(&mut self, a: usize, b: usize, g: f64) { self.s(a, b, a, b, g) }
    #[inline] fn i(&mut self, a: usize, b: usize, i: f64) { if a != 0 { self.z[a - 1] += i; } if b != 0 { self.z[b - 1] -= i; } }
    /// Logic-style output: 50 ohm driver to `v` or 0.
    #[inline] fn drive(&mut self, o: usize, level: f64) { self.g(o, 0, 0.02); self.i(o, 0, 0.02 * level); }
    fn dst(&mut self, an: usize, ca: usize, is: f64, vt: f64) {
        let v0 = (self.v[an] - self.v[ca]).min(vt * (100.0 / is).ln());
        let ef = (v0 / vt).exp(); let i0 = is * (ef - 1.0); let g = is / vt * ef;
        self.g(an, ca, g); self.i(an, ca, -(i0 - g * v0));
    }
    fn ts(&mut self, r: usize, b: usize, c: usize, e: usize, s: f64, q0: f64, qx: f64, qy: f64) {
        if r != 0 {
            let o = (r - 1) * self.m;
            if b != 0 { self.a[o + b - 1] += qx + qy; }
            if e != 0 { self.a[o + e - 1] -= qx; }
            if c != 0 { self.a[o + c - 1] -= qy; }
            self.z[r - 1] -= s * q0;
        }
    }

    fn solve(&mut self) {
        let m = self.m;
        let (a, z, x) = (&mut self.a, &mut self.z, &mut self.x);
        for i in 0..m {
            let (mut p, mut mx) = (i, a[i * m + i].abs());
            for r in i + 1..m { let v = a[r * m + i].abs(); if v > mx { mx = v; p = r; } }
            if mx < 1e-30 { continue; }
            if p != i { for k in i..m { a.swap(i * m + k, p * m + k); } z.swap(i, p); }
            let d = a[i * m + i];
            for r in i + 1..m {
                let f = a[r * m + i] / d;
                if f != 0.0 && f == f { for k in i..m { a[r * m + k] -= f * a[i * m + k]; } z[r] -= f * z[i]; }
            }
        }
        for i in (0..m).rev() {
            let mut s = z[i];
            for k in i + 1..m { s -= a[i * m + k] * x[k]; }
            x[i] = if a[i * m + i] != 0.0 { s / a[i * m + i] } else { 0.0 };
        }
    }

    fn stamp(&mut self, c: &Part, dt: f64) {
        use Kind::*;
        let (a, b) = (c.nd[0], *c.nd.get(1).unwrap_or(&0));
        let d = *c.nd.get(2).unwrap_or(&0);
        let e = *c.nd.get(3).unwrap_or(&0);
        let v = c.val;
        let hi = |s: &Self, k: usize| s.v[k] > v / 2.0;
        match c.kind {
            R | Lamp | Bz => self.g(a, b, 1.0 / v),
            Sw | Bt => if c.on != 0 { self.g(a, b, 1e3) },
            Nb => if c.on == 0 { self.g(a, b, 1e3) },
            Mo => { let g = 1.0 / if c.pn > 0.0 { c.pn } else { 2.0 }; let em = v / 300.0 * c.w; self.g(a, b, g); self.i(a, b, g * em); }
            Sp => { let o = if c.on != 0 { d } else { b }; self.g(a, o, 1e3); }
            Br => { for (x, y) in [(a, d), (b, d), (e, a), (e, b)] { self.dst(x, y, 1e-14, 0.02585); } }
            Sol => { let isc = if c.pn > 0.0 { c.pn } else { 0.1 }; self.dst(a, b, isc * 1e-9, v / 20.7); self.i(a, b, isc * c.pp); }
            Ctr => for k in 0..4 { let o = c.nd[2 + k]; self.drive(o, if (c.cnt >> k) & 1 != 0 { v } else { 0.0 }); },
            Ldr | Ntc => self.g(a, b, 1.0 / c.rv),
            Jk | Srf | Tf => { let o = if c.kind == Tf { 2 } else { 3 }; let (q, qb) = (c.nd[o], c.nd[o + 1]); self.drive(q, if c.st { v } else { 0.0 }); self.drive(qb, if c.st { 0.0 } else { v }); }
            Dff => { self.drive(d, if c.st { v } else { 0.0 }); self.drive(e, if c.st { 0.0 } else { v }); }
            Reg => { let vt = 0f64.max(v.min(self.v[a] - self.v[b] - 2.0)); let io = 10.0 * (vt - (self.v[d] - self.v[b])); self.g(d, b, 10.0); self.i(d, b, 10.0 * vt); self.i(a, b, -io); }
            Fu => if !c.bl { self.g(a, b, 100.0) },
            Rl => { self.g(a, b, 1.0 / if c.pn > 0.0 { c.pn } else { 100.0 }); if c.on != 0 { self.g(d, e, 1e3); } }
            And | Or | Not | Nand | Nor | Xor => {
                let (ha, hb) = (hi(self, a), hi(self, b));
                let o = match c.kind { Not => !ha, And => ha && hb, Or => ha || hb, Nand => !(ha && hb), Nor => !(ha || hb), _ => ha != hb };
                let ot = if c.kind == Not { b } else { d };
                self.drive(ot, if o { v } else { 0.0 });
            }
            Am => self.g(a, b, 1e3),
            Vm => self.g(a, b, 1e-7),
            Om => self.i(a, b, 1e-3),
            C => { self.g(a, b, v / dt); self.i(a, b, v / dt * c.vp); }
            L => { self.g(a, b, dt / v); self.i(a, b, -c.ip); }
            Tr => { self.g(a, b, c.g[0]); self.g(d, e, c.g[2]); self.s(a, b, d, e, c.g[1]); self.s(d, e, a, b, c.g[1]); self.i(a, b, -c.ip); self.i(d, e, -c.is); }
            Tm => {
                let n = &c.nd; let (vc, gd, ds, ou, rs, ct) = (n[0], n[1], n[4], n[5], n[6], n[7]);
                self.g(vc, ct, 2e-4); self.g(ct, gd, 1e-4); self.g(vc, rs, 1e-6);
                let lvl = self.v[gd] + if c.qs { self.v[vc] - self.v[gd] } else { 0.0 };
                self.drive(ou, lvl);
                if !c.qs { self.g(ds, gd, 0.05); }
            }
            Cs => self.i(a, b, v),
            Pot => { self.g(a, d, 1.0 / (v * c.pp + 1e-3)); self.g(d, b, 1.0 / (v * (1.0 - c.pp) + 1e-3)); }
            D | Sd | Led | Zd => {
                let nn = if c.kind == Led { 2.0 } else { 1.0 };
                let is = if c.kind == Led { c.led_is } else if c.kind == Sd { 1e-8 } else { 1e-14 };
                let vt = 0.02585 * nn;
                let lo = if c.kind == Zd { -(v - 0.65 + 0.98) } else { -1e3 };
                let v0 = lo.max((self.v[a] - self.v[b]).min(vt * (100.0 / is).ln()));
                let ef = (v0 / vt).exp(); let mut i0 = is * (ef - 1.0); let mut g = is / vt * ef;
                if c.kind == Zd { let er = ((-v0 - v + 0.65) / 0.02585).exp(); i0 -= is * (er - 1.0); g += is / 0.02585 * er; }
                self.g(a, b, g); self.i(a, b, -(i0 - g * v0));
            }
            Op => {
                let vd = self.v[a] - self.v[b];
                let gain = if c.pn > 0.0 { c.pn } else { 1e4 };
                let th = (gain * vd / v).clamp(-20.0, 20.0).tanh();
                let k = gain * (1.0 - th * th); let go = 0.1; let m = self.m;
                if d != 0 {
                    let r = (d - 1) * m;
                    self.a[r + d - 1] += go;
                    if a != 0 { self.a[r + a - 1] -= go * k; }
                    if b != 0 { self.a[r + b - 1] += go * k; }
                    self.z[d - 1] += go * (v * th - k * vd);
                }
            }
            Nm | Pm => {
                let s = if c.kind == Nm { 1.0 } else { -1.0 };
                let (gg, dd, ss) = (a, b, d);
                let vgs = s * (self.v[gg] - self.v[ss]); let vds = s * (self.v[dd] - self.v[ss]); let vo = vgs - v;
                let be = if c.pn > 0.0 { c.pn } else { 0.01 };
                let (mut id, mut gm, mut gd) = (0.0, 0.0, 0.0);
                if vo > 0.0 {
                    if vds < vo { id = be * (vo * vds - vds * vds / 2.0); gm = be * vds; gd = be * (vo - vds); }
                    else { id = be / 2.0 * vo * vo; gm = be * vo; }
                }
                let q0 = s * (id - gm * vgs - gd * vds); let m = self.m;
                for (r, sg) in [(dd, 1.0), (ss, -1.0)] {
                    if r != 0 {
                        let o = (r - 1) * m;
                        if gg != 0 { self.a[o + gg - 1] += sg * gm; }
                        if dd != 0 { self.a[o + dd - 1] += sg * gd; }
                        if ss != 0 { self.a[o + ss - 1] -= sg * (gm + gd); }
                        self.z[r - 1] -= sg * q0;
                    }
                }
            }
            Dc | Ac | Sg => {
                let m = self.m; let r = self.n - 1 + c.row.unwrap();
                let e_ = match c.kind {
                    Dc => v,
                    Sg => nz(c.pn) + v * wv(c.wave, (c.fr * self.t) % 1.0, (if c.qn > 0.0 { c.qn } else { 50.0 }) / 100.0),
                    _ => nz(c.pn) + v * (2.0 * PI * c.fr * self.t + nz(c.qn) * PI / 180.0).sin(),
                };
                if a != 0 { self.a[(a - 1) * m + r] += 1.0; self.a[r * m + a - 1] += 1.0; }
                if b != 0 { self.a[(b - 1) * m + r] -= 1.0; self.a[r * m + b - 1] -= 1.0; }
                self.z[r] = e_;
                self.a[r * m + r] -= nz(if c.kind == Dc { c.pn } else { c.sn });
            }
            Npn | Pnp => {
                let s = if c.kind == Npn { 1.0 } else { -1.0 };
                let (bb, cc, em) = (a, b, d);
                let be = (s * (self.v[bb] - self.v[em])).min(0.9); let bc = (s * (self.v[bb] - self.v[cc])).min(0.9);
                let (is, vt) = (1e-14, 0.02585);
                let (ef, er) = ((be / vt).exp(), (bc / vt).exp());
                let (i1, i2, gf, gr) = (is * (ef - 1.0), is * (er - 1.0), is / vt * ef, is / vt * er);
                let ic = i1 - 2.0 * i2; let bf = if c.pn > 0.0 { c.pn } else { 100.0 }; let ib = i1 / bf + i2;
                let (cx, cy, bx, by) = (gf, -2.0 * gr, gf / bf, gr);
                let c0 = ic - cx * be - cy * bc; let b0 = ib - bx * be - by * bc;
                self.ts(cc, bb, cc, em, s, c0, cx, cy);
                self.ts(bb, bb, cc, em, s, b0, bx, by);
                self.ts(em, bb, cc, em, s, -c0 - b0, -cx - bx, -cy - by);
            }
            _ => {}
        }
    }

    /// One backward-Euler timestep with Newton iteration, then behavioral/state updates.
    pub fn step(&mut self) {
        self.sc += 1;
        let dt = self.dt; self.t += dt;
        let mut parts = std::mem::take(&mut self.parts);
        let iters = if self.q { 40 } else { 1 };
        for it in 0..iters {
            self.a.fill(0.0); self.z.fill(0.0);
            let m = self.m;
            for nn in 1..self.n { self.a[(nn - 1) * m + nn - 1] = 1e-9; }
            self.xp.copy_from_slice(&self.x);
            for c in parts.iter() { self.stamp(c, dt); }
            self.solve();
            let mut md: f64 = 0.0;
            for i in 0..m {
                if self.x[i].is_nan() { self.x.fill(0.0); }
                md = md.max((self.x[i] - self.xp[i]).abs());
            }
            for nn in 1..self.n {
                self.v[nn] = self.x[nn - 1];
                if self.v[nn].abs() > self.mv { self.mv = self.v[nn].abs(); self.mn = nn; }
            }
            if md < 2e-5 { break; }
            if it == 39 && self.q { self.fail += 1; self.md = md; }
        }
        for c in parts.iter_mut() { self.post(c); }
        self.parts = parts;
    }

    fn post(&mut self, c: &mut Part) {
        use Kind::*;
        let (a, b) = (c.nd[0], *c.nd.get(1).unwrap_or(&0));
        let d = *c.nd.get(2).unwrap_or(&0);
        let e = *c.nd.get(3).unwrap_or(&0);
        let v = self.v[a] - self.v[b];
        let (dt, k) = (self.dt, self.kf);
        match c.kind {
            C => c.vp = v,
            L => c.ip += dt / c.val * v,
            Tr => { let v2 = self.v[d] - self.v[e]; c.ip += c.g[0] * v + c.g[1] * v2; c.is += c.g[1] * v + c.g[2] * v2; }
            Vm | Am | Om | Lg => {
                let r = match c.kind { Vm => v, Lg => self.v[a], _ => v * 1e3 };
                c.av = c.av * k + r * (1.0 - k); c.sq = c.sq * k + r * r * (1.0 - k);
            }
            _ => {}
        }
        match c.kind {
            Lamp => c.gl = c.gl * k + (v * v / c.val / if c.pn > 0.0 { c.pn } else { 0.5 }).min(1.0) * (1.0 - k),
            Ctr => {
                let ck = self.v[a] > c.val / 2.0;
                if self.v[b] > c.val / 2.0 { c.cnt = 0; } else if ck && !c.pc { c.cnt = (c.cnt + 1) & 15; }
                c.pc = ck;
            }
            Jk | Srf | Tf => {
                let h = |i: usize| self.v[c.nd[i]] > c.val / 2.0;
                let ck = h(1);
                if ck && !c.pc {
                    let (a1, a2) = (h(0), h(2));
                    match c.kind {
                        Tf => if a1 { c.st = !c.st },
                        Jk => if a1 && a2 { c.st = !c.st } else if a1 { c.st = true } else if a2 { c.st = false },
                        _ => if a1 && !a2 { c.st = true } else if a2 && !a1 { c.st = false },
                    }
                }
                c.pc = ck;
            }
            Tm => {
                let n = &c.nd; let g = self.v[n[1]];
                let (vc, t1, t2) = (self.v[n[7]] - g, self.v[n[2]] - g, self.v[n[3]] - g);
                if self.v[n[0]] - g < 1.5 || self.v[n[6]] - g < 0.7 { c.qs = false; }
                else { if t2 > vc { c.qs = false; } if t1 < vc / 2.0 { c.qs = true; } }
            }
            Bz => c.bv = c.bv * k + v.abs() * (1.0 - k),
            Dff => {
                let ck = self.v[b] > c.val / 2.0;
                if ck && !c.pc { c.st = self.v[a] > c.val / 2.0; }
                c.pc = ck;
            }
            Fu if !c.bl => { let i = v * 100.0; c.ht = 0f64.max(c.ht + dt * ((i / c.val).powi(2) - 1.0) * 20.0); if c.ht > 1.0 { c.bl = true; } }
            Rl => { let av = v.abs(); if av > c.val { c.on = 1; } else if av < c.val * 0.4 { c.on = 0; } }
            Mo => {
                let kk = c.val / 300.0; let r = if c.pn > 0.0 { c.pn } else { 2.0 };
                c.w += dt * ((v - kk * c.w) / r / (0.165 * kk) - 0.303 * c.w);
                c.ang += c.w * dt * 0.05;
            }
            Led => c.gl = c.gl * k + (0f64.max(c.cur) / 0.01).min(1.0) * (1.0 - k),
            Os if self.t >= c.nt => {
                c.nt = (c.nt + c.iv).max(self.t);
                let y = self.v[a];
                if c.ti >= NS { c.wt += 1; if (c.py <= c.tl && y > c.tl) || c.wt > NS * 2 { c.ti = 0; c.wt = 0; } }
                if c.ti < NS { c.ta[c.ti] = y as f32; c.tb[c.ti] = self.v[b] as f32; c.ti += 1; }
                c.py = y;
            }
            _ => {}
        }
        // device currents recorded by the stamps are recomputed here from the converged solution
        match c.kind {
            D | Sd | Led | Zd => c.cur = self.diode_i(c),
            Npn | Pnp => c.cur = self.bjt_ic(c),
            Nm | Pm => c.cur = self.mos_id(c),
            _ => {}
        }
        if self.sc & 3 == 0 { self.pk(c, v); }
    }

    fn diode_i(&self, c: &Part) -> f64 {
        use Kind::*;
        let (a, b, v) = (c.nd[0], c.nd[1], c.val);
        let nn = if c.kind == Led { 2.0 } else { 1.0 };
        let is = if c.kind == Led { c.led_is } else if c.kind == Sd { 1e-8 } else { 1e-14 };
        let vt = 0.02585 * nn;
        let lo = if c.kind == Zd { -(v - 0.65 + 0.98) } else { -1e3 };
        let vd = self.v[a] - self.v[b];
        let v0 = lo.max(vd.min(vt * (100.0 / is).ln()));
        let ef = (v0 / vt).exp(); let mut i0 = is * (ef - 1.0); let mut g = is / vt * ef;
        if c.kind == Zd { let er = ((-v0 - v + 0.65) / 0.02585).exp(); i0 -= is * (er - 1.0); g += is / 0.02585 * er; }
        i0 + g * (vd - v0)
    }
    fn bjt_ic(&self, c: &Part) -> f64 {
        let s = if c.kind == Kind::Npn { 1.0 } else { -1.0 };
        let (bb, cc, em) = (c.nd[0], c.nd[1], c.nd[2]);
        let be = (s * (self.v[bb] - self.v[em])).min(0.9); let bc = (s * (self.v[bb] - self.v[cc])).min(0.9);
        let (is, vt) = (1e-14, 0.02585);
        s * (is * ((be / vt).exp() - 1.0) - 2.0 * is * ((bc / vt).exp() - 1.0))
    }
    fn mos_id(&self, c: &Part) -> f64 {
        let s = if c.kind == Kind::Nm { 1.0 } else { -1.0 };
        let (gg, dd, ss) = (c.nd[0], c.nd[1], c.nd[2]);
        let vgs = s * (self.v[gg] - self.v[ss]); let vds = s * (self.v[dd] - self.v[ss]); let vo = vgs - c.val;
        let be = if c.pn > 0.0 { c.pn } else { 0.01 };
        let id = if vo <= 0.0 { 0.0 } else if vds < vo { be * (vo * vds - vds * vds / 2.0) } else { be / 2.0 * vo * vo };
        s * id
    }

    /// Peak / averaged current, voltage, power tracking (used by the dynamic diagnostics).
    fn pk(&self, c: &mut Part, v: f64) {
        use Kind::*;
        let av = v.abs();
        let (mut i, mut p, mut vm) = (0.0, 0.0, av);
        match c.kind {
            R | Lamp => { i = av / c.val; p = av * i; }
            C => {}
            L => i = c.ip.abs(),
            Tr => i = c.ip.abs().max(c.is.abs()),
            D | Sd | Led | Zd => { i = c.cur.abs(); p = av * i; vm = if v < 0.0 { av } else { 0.0 }; }
            Npn | Pnp | Nm | Pm => { vm = (self.v[c.nd[1]] - self.v[c.nd[2]]).abs(); i = c.cur.abs(); p = vm * i; }
            _ => if let Some(r) = c.row { i = self.x[self.n - 1 + r].abs(); } else if c.kind == Am { i = av * 1e3; } else { return; }
        }
        if i > c.pi { c.pi = i; }
        c.ia = c.ia * self.kp + i * (1.0 - self.kp);
        if vm > c.pv { c.pv = vm; }
        c.pw = c.pw * self.kp + p * (1.0 - self.kp);
    }

    /// Source current for a voltage-source part (positive out of the + terminal... as solved).
    pub fn source_current(&self, c: &Part) -> f64 { c.row.map(|r| self.x[self.n - 1 + r]).unwrap_or(0.0) }

    /// Advance to simulation time `tg`, optionally stopping at a wall-clock deadline.
    pub fn advance(&mut self, tg: f64, deadline: Option<Instant>) {
        let mut i = 0u32;
        while self.t < tg {
            self.step();
            i += 1;
            if i & 7 == 0 { if let Some(d) = deadline { if Instant::now() > d { break; } } }
        }
        self.lag = self.t < tg - self.dt;
        for c in self.parts.iter_mut().filter(|c| c.kind == Kind::Os) {
            let (mn, mx) = c.ta.iter().fold((f32::MAX, f32::MIN), |(a, b), &x| (a.min(x), b.max(x)));
            c.tl = if (mx - mn) as f64 > 1e-3 { (mx + mn) as f64 / 2.0 } else { 0.0 };
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct FrameStats {
    pub lag: bool, pub fails: u32,
    /// Largest node voltage seen this frame and the (global) node it occurred on.
    pub max_v: f64, pub bn: usize,
    /// Size of the last Newton step that failed to converge.
    pub md: f64,
}

pub struct Sim {
    pub islands: Vec<Island>,
    pub n_nodes: usize,
    pub has_ground: bool,
    pub qs: f64,
    pub lag_count: u32,
    pub wire_nodes: Vec<Option<usize>>,
    comp_map: Vec<Option<(usize, usize)>>,
    node_map: Vec<Option<(usize, usize)>>,
    t_idle: f64,
}

impl Sim {
    pub fn new(p: &Project, qs: f64) -> Sim { Sim::build(p, qs, None) }
    /// Rebuild after an edit or a quality change, carrying over runtime state and time.
    pub fn rebuild(&self, p: &Project, qs: f64) -> Sim { Sim::build(p, qs, Some(self)) }

    fn build(p: &Project, qs: f64, prev: Option<&Sim>) -> Sim {
        let nl = build(p, p.seed());
        let t0 = prev.map(|s| s.time()).unwrap_or(0.0);
        let specs = groupize(&nl, qs);
        let mut comp_map = vec![None; p.c.len()];
        let mut node_map = vec![None; nl.n_nodes];
        let mut islands = vec![];
        for (gi, mut s) in specs.into_iter().enumerate() {
            for (li, c) in s.parts.iter_mut().enumerate() {
                comp_map[c.src] = Some((gi, li));
                if let Some(pv) = prev.and_then(|o| o.part(c.src)) { c.copy_dyn(pv); }
            }
            for (l, &g) in s.gn.iter().enumerate() { if g != 0 { node_map[g] = Some((gi, l)); } }
            islands.push(Island::new(gi, s, t0));
        }
        Sim { islands, n_nodes: nl.n_nodes, has_ground: nl.has_ground, qs, lag_count: 0, wire_nodes: nl.wire_nodes, comp_map, node_map, t_idle: t0 }
    }

    pub fn time(&self) -> f64 { self.islands.iter().map(|i| i.t).fold(None, |a: Option<f64>, t| Some(a.map_or(t, |x| x.min(t)))).unwrap_or(self.t_idle) }
    pub fn part(&self, comp: usize) -> Option<&Part> { self.comp_map.get(comp).copied().flatten().map(|(g, l)| &self.islands[g].parts[l]) }
    pub fn part_mut(&mut self, comp: usize) -> Option<&mut Part> { self.comp_map.get(comp).copied().flatten().map(move |(g, l)| &mut self.islands[g].parts[l]) }
    /// Voltage at a global node id (0 = ground).
    pub fn node_v(&self, node: usize) -> f64 {
        if node == 0 { return 0.0; }
        self.node_map.get(node).copied().flatten().map(|(g, l)| self.islands[g].v[l]).unwrap_or(0.0)
    }
    /// Global node id of pin `pin` of component `comp`.
    pub fn pin_node(&self, comp: usize, pin: usize) -> Option<usize> { self.part(comp).and_then(|p| p.gnd.get(pin).copied()) }
    pub fn pin_v(&self, comp: usize, pin: usize) -> f64 { self.pin_node(comp, pin).map(|n| self.node_v(n)).unwrap_or(0.0) }

    /// Live poke (switch/button toggle, pot wiper, LDR/NTC/solar slider) without rebuilding.
    pub fn poke(&mut self, comp: usize, on: Option<u8>, pp: Option<f64>, f: Option<&str>) {
        if let Some(c) = self.part_mut(comp) {
            if let Some(o) = on { c.on = o; }
            if let Some(w) = pp { c.pp = w.clamp(0.0, 1.0); }
            if let Some(s) = f { if matches!(c.kind, Kind::Ldr | Kind::Ntc | Kind::Sol) { c.rlive(s); } }
        }
    }

    /// Advance every island (in parallel) to time `tg`.
    pub fn advance_to(&mut self, tg: f64, budget_ms: Option<f64>) -> FrameStats {
        let dl = budget_ms.map(|b| Instant::now() + std::time::Duration::from_secs_f64(b / 1000.0));
        self.islands.par_iter_mut().for_each(|i| i.advance(tg, dl));
        let mut s = FrameStats::default();
        for i in self.islands.iter_mut() {
            s.lag |= i.lag;
            let f = std::mem::take(&mut i.fail); s.fails += f; if f > 0 { s.md = i.md; }
            let mv = std::mem::take(&mut i.mv);
            if mv > s.max_v { s.max_v = mv; s.bn = i.gn[i.mn]; }
        }
        if s.lag { self.lag_count += 1; }
        s
    }
    /// Current through a voltage-source part (dc / ac / sg), as solved.
    pub fn source_current(&self, comp: usize) -> f64 {
        self.comp_map.get(comp).copied().flatten().map(|(g, l)| { let i = &self.islands[g]; i.source_current(&i.parts[l]) }).unwrap_or(0.0)
    }
    /// Frequency of the first buzzer whose averaged drive exceeds 2 V (None = silence).
    pub fn buzzer_tone(&self) -> Option<f64> {
        self.islands.iter().flat_map(|i| i.parts.iter()).find(|c| c.kind == Kind::Bz && c.bv > 2.0).map(|c| c.fr)
    }
    /// One UI frame: wall-clock `el` seconds at `speed`x, with a work budget in ms.
    pub fn frame(&mut self, el: f64, speed: f64, budget_ms: f64) -> FrameStats {
        let tg = self.time() + el.min(0.1) * speed;
        self.advance_to(tg, Some(budget_ms))
    }
}
