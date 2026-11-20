//! Backend-neutral vector symbol renderer (port of the JS `sym`, `osc`, `tml`, `pinLbl`, `lbl`, `junc`).
//! Everything is emitted as flattened polylines / polygons / text so it can be drawn by egui,
//! exported to SVG, or unit-tested.
use crate::kind::Kind;
use crate::netlist::{Part, Wave, NS};
use crate::project::{Component, Project};
use crate::units::{fm, pv};
use std::f64::consts::PI;

pub type Pt = (f64, f64);

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Col { Ink, Wire, Gray, Rgba(u8, u8, u8, u8) }
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Align { Left, Center, Right }

#[derive(Clone, Debug)]
pub enum Prim {
    Stroke { pts: Vec<Pt>, col: Col, w: f32 },
    Fill { pts: Vec<Pt>, col: Col },
    Text { p: Pt, s: String, size: f32, bold: bool, align: Align, mid: bool, col: Col },
}

// ---------------------------------------------------------------- path builder (canvas semantics)
#[derive(Default)]
struct Cv { prims: Vec<Prim>, subs: Vec<Vec<Pt>>, col: Option<Col>, w: f32 }

impl Cv {
    fn begin(&mut self) { self.subs.clear(); }
    fn mv(&mut self, x: f64, y: f64) { self.subs.push(vec![(x, y)]); }
    fn ln(&mut self, x: f64, y: f64) {
        match self.subs.last_mut() { Some(s) => s.push((x, y)), None => self.mv(x, y) }
    }
    fn pp(&mut self, a: &[f64]) { self.mv(a[0], a[1]); for c in a[2..].chunks(2) { self.ln(c[0], c[1]); } }
    fn cur(&self) -> Option<Pt> { self.subs.last().and_then(|s| s.last().copied()) }
    fn arc(&mut self, cx: f64, cy: f64, r: f64, a0: f64, a1: f64, ccw: bool) {
        let tau = 2.0 * PI;
        let mut sw = a1 - a0;
        if !ccw { if sw >= tau { sw = tau } else { while sw < 0.0 { sw += tau } } }
        else if -sw >= tau { sw = -tau } else { while sw > 0.0 { sw -= tau } }
        let n = ((sw.abs() / tau * 48.0).ceil() as usize).max(4);
        for i in 0..=n {
            let a = a0 + sw * i as f64 / n as f64;
            let p = (cx + r * a.cos(), cy + r * a.sin());
            if i == 0 {
                match self.cur() {
                    Some(c) if (c.0 - p.0).abs() < 1e-9 && (c.1 - p.1).abs() < 1e-9 => {}
                    Some(_) => self.ln(p.0, p.1),
                    None => self.mv(p.0, p.1),
                }
            } else { self.ln(p.0, p.1); }
        }
    }
    fn circle(&mut self, x: f64, y: f64, r: f64) { self.mv(x + r, y); self.arc(x, y, r, 0.0, 2.0 * PI, false); }
    fn bez(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, x: f64, y: f64) {
        let (x0, y0) = self.cur().unwrap_or((x1, y1));
        for i in 1..=16 {
            let t = i as f64 / 16.0; let u = 1.0 - t;
            self.ln(u * u * u * x0 + 3.0 * u * u * t * x1 + 3.0 * u * t * t * x2 + t * t * t * x,
                    u * u * u * y0 + 3.0 * u * u * t * y1 + 3.0 * u * t * t * y2 + t * t * t * y);
        }
    }
    fn quad(&mut self, x1: f64, y1: f64, x: f64, y: f64) {
        let (x0, y0) = self.cur().unwrap_or((x1, y1));
        for i in 1..=16 {
            let t = i as f64 / 16.0; let u = 1.0 - t;
            self.ln(u * u * x0 + 2.0 * u * t * x1 + t * t * x, u * u * y0 + 2.0 * u * t * y1 + t * t * y);
        }
    }
    fn style(&mut self, col: Col, w: f32) { self.col = Some(col); self.w = w; }
    fn stroke(&mut self) {
        let col = self.col.unwrap_or(Col::Ink);
        for s in &self.subs { if s.len() > 1 { self.prims.push(Prim::Stroke { pts: s.clone(), col, w: self.w }); } }
    }
    fn fill(&mut self, col: Col) {
        for s in &self.subs { if s.len() > 2 { self.prims.push(Prim::Fill { pts: s.clone(), col }); } }
    }
    fn rect(&self, x: f64, y: f64, w: f64, h: f64) -> Vec<Pt> { vec![(x, y), (x + w, y), (x + w, y + h), (x, y + h), (x, y)] }
    fn fill_rect(&mut self, x: f64, y: f64, w: f64, h: f64, col: Col) { let r = self.rect(x, y, w, h); self.prims.push(Prim::Fill { pts: r, col }); }
    fn stroke_rect(&mut self, x: f64, y: f64, w: f64, h: f64, col: Col, lw: f32) { let r = self.rect(x, y, w, h); self.prims.push(Prim::Stroke { pts: r, col, w: lw }); }
    fn text(&mut self, x: f64, y: f64, s: &str, size: f32, bold: bool, align: Align, mid: bool, col: Col) {
        self.prims.push(Prim::Text { p: (x, y), s: s.into(), size, bold, align, mid, col });
    }
}

// ---------------------------------------------------------------- symbol state
#[derive(Clone, Debug)]
pub struct SymState {
    pub on: bool, pub st: bool, pub bl: bool, pub cnt: u32,
    pub ang: f64, pub gl: f64, pub wiper: f64, pub wave: Wave, pub high: bool, pub rgb: [u8; 3],
    pub scope: Option<(Vec<f32>, Vec<f32>)>, pub tdiv: f64, pub vdiv: f64,
}

pub fn led_rgb(p: Option<&str>) -> [u8; 3] {
    match p { Some("yellow") => [255, 210, 40], Some("green") => [60, 220, 90], Some("blue") => [70, 130, 255], Some("white") => [255, 255, 235], _ => [255, 70, 50] }
}
fn nz(x: f64) -> f64 { if x.is_nan() { 0.0 } else { x } }
fn wave_of(c: &Component) -> Wave { match c.k.as_deref() { Some("square") => Wave::Square, Some("triangle") => Wave::Triangle, Some("sawtooth") => Wave::Sawtooth, _ => Wave::Sine } }

impl SymState {
    /// State for edit mode (no simulation running).
    pub fn edit(c: &Component) -> SymState {
        let tdiv = pv(c.v.as_deref().unwrap_or("")); 
        SymState { on: c.on != 0, st: false, bl: false, cnt: 0, ang: 0.0, gl: 0.0,
            wiper: nz(pv(c.f.as_deref().unwrap_or(""))), wave: wave_of(c), high: false, rgb: led_rgb(c.p.as_deref()),
            scope: if c.t == Kind::Os { Some((vec![0.0; NS], vec![0.0; NS])) } else { None },
            tdiv: if tdiv > 0.0 { tdiv } else { 1e-3 }, vdiv: pv(c.f.as_deref().unwrap_or("")) }
    }
    /// State while running, from the live part.
    pub fn run(c: &Component, p: &Part) -> SymState {
        let mut s = SymState::edit(c);
        s.on = p.on != 0; s.st = p.st; s.bl = p.bl; s.cnt = p.cnt; s.ang = p.ang; s.gl = p.gl; s.high = p.av > p.val;
        if c.t == Kind::Os { s.scope = Some((p.ta.clone(), p.tb.clone())); s.tdiv = p.val; }
        s
    }
}

// ---------------------------------------------------------------- symbols (local coordinates)
const FOUR: [f64; 4] = [-40.0, -20.0, 20.0, 40.0];

fn nice(x: f64) -> f64 { let e = 10f64.powf(x.log10().floor()); let f = x / e; (if f <= 1.0 { 1.0 } else if f <= 2.0 { 2.0 } else if f <= 5.0 { 5.0 } else { 10.0 }) * e }

fn resistor(v: &mut Cv) { v.pp(&[-40., 0., -20., 0., -15., -8., -5., 8., 5., -8., 15., 8., 20., 0., 40., 0.]); }

fn sym(k: Kind, st: &SymState, v: &mut Cv) {
    use Kind::*;
    v.begin();
    match k {
        R => resistor(v),
        C => { v.pp(&[-40., 0., -4., 0.]); v.pp(&[4., 0., 40., 0.]); v.pp(&[-4., -12., -4., 12.]); v.pp(&[4., -12., 4., 12.]); }
        L => { v.pp(&[-40., 0., -24., 0.]); v.pp(&[24., 0., 40., 0.]); for i in 0..4 { let i = i as f64; v.mv(-24. + 12. * i, 0.); v.arc(-18. + 12. * i, 0., 6., PI, 0., false); } }
        Dc => { v.pp(&[0., -40., 0., -6.]); v.pp(&[0., 6., 0., 40.]); v.pp(&[-12., -6., 12., -6.]); v.pp(&[-6., 6., 6., 6.]); }
        Ac => { v.pp(&[0., -40., 0., -16.]); v.pp(&[0., 16., 0., 40.]); v.mv(16., 0.); v.arc(0., 0., 16., 0., 2. * PI, false); v.mv(-9., 0.); v.bez(-5., -12., -1., -12., 0., 0.); v.bez(1., 12., 5., 12., 9., 0.); }
        Sw => { v.pp(&[-40., 0., -14., 0.]); v.pp(&[14., 0., 40., 0.]); if st.on { v.pp(&[-14., 0., 14., 0.]) } else { v.pp(&[-14., 0., 12., -14.]) } }
        Bt => { v.pp(&[-40., 0., -12., 0.]); v.pp(&[12., 0., 40., 0.]); if st.on { v.pp(&[-12., 0., 12., 0.]) } let y = if st.on { -4. } else { -14. }; v.pp(&[-10., y, 10., y]); v.pp(&[0., y, 0., y - 8.]); }
        Nb => { v.pp(&[-40., 0., -12., 0.]); v.pp(&[12., 0., 40., 0.]); if !st.on { v.pp(&[-12., 0., 12., 0.]) } let y = if st.on { -14. } else { -4. }; v.pp(&[-10., y, 10., y]); v.pp(&[0., y, 0., y - 8.]); }
        Tr => {
            v.pp(&[-40., -20., -16., -20.]); v.pp(&[-40., 20., -16., 20.]); v.pp(&[16., -20., 40., -20.]); v.pp(&[16., 20., 40., 20.]);
            v.pp(&[-3., -20., -3., 20.]); v.pp(&[3., -20., 3., 20.]);
            v.mv(-16., -20.); v.arc(-16., -10., 10., -PI / 2., PI / 2., false); v.mv(-16., 0.); v.arc(-16., 10., 10., -PI / 2., PI / 2., false);
            v.mv(16., -20.); v.arc(16., -10., 10., -PI / 2., PI / 2., true); v.mv(16., 0.); v.arc(16., 10., 10., -PI / 2., PI / 2., true);
        }
        Npn | Pnp => {
            v.pp(&[-40., 0., -8., 0.]); v.pp(&[-8., -16., -8., 16.]); v.pp(&[-8., -6., 20., -24., 20., -40.]); v.pp(&[-8., 6., 20., 24., 20., 40.]);
            let n = k == Npn; let tp = if n { (14.4, 20.4) } else { (-1.0, 10.5) }; let s = if n { 1.0 } else { -1.0 };
            let (bx, by) = (tp.0 - s * 6.7, tp.1 - s * 4.3);
            v.pp(&[bx - 2.2, by + 3.4, tp.0, tp.1, bx + 2.2, by - 3.4]);
        }
        Vm | Am | Om => { v.pp(&[-40., 0., -16., 0.]); v.pp(&[16., 0., 40., 0.]); v.circle(0., 0., 16.); }
        Lg => { v.pp(&[0., 0., 0., 8.]); v.circle(0., 15., 7.); }
        D | Sd | Led | Zd => {
            v.pp(&[-40., 0., -8., 0.]); v.pp(&[8., 0., 40., 0.]); v.pp(&[-8., -10., 8., 0., -8., 10., -8., -10.]); v.pp(&[8., -10., 8., 10.]);
            if k == Zd { v.pp(&[8., -10., 12., -13.]); v.pp(&[8., 10., 4., 13.]); }
            if k == Sd { v.pp(&[8., -10., 4., -7.]); v.pp(&[8., 10., 12., 7.]); }
            if k == Led { v.pp(&[-2., -13., 4., -21.]); v.pp(&[5., -12., 11., -20.]); }
        }
        Pot => {
            resistor(v);
            let wx = -30.0 + 60.0 * (st.wiper / 100.0).clamp(0.0, 1.0);
            v.pp(&[0., -40., 0., -26., wx, -26., wx, -12.]); v.pp(&[wx - 4., -17., wx, -11., wx + 4., -17.]);
        }
        Lamp => { v.pp(&[-40., 0., -14., 0.]); v.pp(&[14., 0., 40., 0.]); v.circle(0., 0., 14.); v.pp(&[-10., -10., 10., 10.]); v.pp(&[-10., 10., 10., -10.]); }
        Cs => { v.pp(&[0., -40., 0., -16.]); v.pp(&[0., 16., 0., 40.]); v.circle(0., 0., 16.); v.pp(&[0., 9., 0., -9.]); v.pp(&[-4., -4., 0., -9., 4., -4.]); }
        Op => { v.pp(&[-40., -20., -20., -20.]); v.pp(&[-40., 20., -20., 20.]); v.pp(&[30., 0., 40., 0.]); v.pp(&[-20., -30., -20., 30., 30., 0., -20., -30.]); v.pp(&[-17., -20., -11., -20.]); v.pp(&[-14., -23., -14., -17.]); v.pp(&[-17., 20., -11., 20.]); }
        Nm | Pm => {
            v.pp(&[-40., 0., -12., 0.]); v.pp(&[-12., -14., -12., 14.]); v.pp(&[-6., -16., -6., -6.]); v.pp(&[-6., -4., -6., 4.]); v.pp(&[-6., 6., -6., 16.]);
            v.pp(&[-6., -11., 20., -11., 20., -40.]); v.pp(&[-6., 11., 20., 11., 20., 40.]); v.pp(&[-6., 0., 0., 0.]);
            if k == Nm { v.pp(&[-6., 0., 0., -3., 0., 3., -6., 0.]) } else { v.pp(&[0., 0., -6., -3., -6., 3., 0., 0.]) }
        }
        Mo => {
            v.pp(&[-40., 0., -16., 0.]); v.pp(&[16., 0., 40., 0.]); v.circle(0., 0., 16.);
            for i in 0..3 { let an = st.ang + i as f64 * 2.094; let (c, s) = (an.cos(), an.sin()); v.pp(&[7. * c, 7. * s, 14. * c, 14. * s]); }
        }
        Sg => {
            v.pp(&[0., -40., 0., -16.]); v.pp(&[0., 16., 0., 40.]); v.circle(0., 0., 16.);
            match st.wave {
                Wave::Square => v.pp(&[-9., 4., -9., -4., 0., -4., 0., 4., 9., 4.]),
                Wave::Triangle => v.pp(&[-9., 3., -4., -5., 4., 5., 9., -3.]),
                Wave::Sawtooth => v.pp(&[-9., 4., 0., -4., 0., 4., 9., -4.]),
                Wave::Sine => { v.mv(-9., 0.); v.bez(-5., -12., -1., -12., 0., 0.); v.bez(1., 12., 5., 12., 9., 0.); }
            }
        }
        Fu => { v.pp(&[-40., 0., -20., 0.]); v.pp(&[20., 0., 40., 0.]); v.pp(&[-20., -6., 20., -6., 20., 6., -20., 6., -20., -6.]); if st.bl { v.pp(&[-20., 0., -6., 0.]); v.pp(&[6., 0., 20., 0.]); } else { v.pp(&[-20., 0., 20., 0.]); } }
        Rl => {
            v.pp(&[-40., -20., -18., -20.]); v.pp(&[-40., 20., -18., 20.]); v.pp(&[-18., -26., -6., -26., -6., 26., -18., 26., -18., -26.]); v.pp(&[-18., -12., -6., -4.]);
            v.pp(&[40., -20., 22., -20.]); v.pp(&[40., 20., 22., 20.]);
            if st.on { v.pp(&[22., -20., 22., 20.]) } else { v.pp(&[22., -20., 14., 12.]) }
        }
        Br => { v.pp(&[-40., -20., -24., -20.]); v.pp(&[-40., 20., -24., 20.]); v.pp(&[24., -20., 40., -20.]); v.pp(&[24., 20., 40., 20.]); v.pp(&[-24., -30., 24., -30., 24., 30., -24., 30., -24., -30.]); v.pp(&[0., -22., 18., 0., 0., 22., -18., 0., 0., -22.]); }
        Sol => {
            v.pp(&[0., -40., 0., -6.]); v.pp(&[0., 6., 0., 40.]); v.pp(&[-12., -6., 12., -6.]); v.pp(&[-6., 6., 6., 6.]); v.circle(0., 0., 24.);
            v.pp(&[-36., -26., -28., -18.]); v.pp(&[-28., -18., -33., -19.]); v.pp(&[-28., -18., -29., -23.]); v.pp(&[-32., -34., -24., -26.]); v.pp(&[-24., -26., -29., -27.]); v.pp(&[-24., -26., -25., -31.]);
        }
        Ctr => {
            v.pp(&[-40., -20., -24., -20.]); v.pp(&[-40., 20., -24., 20.]);
            for y in FOUR { v.pp(&[24., y, 40., y]); }
            v.pp(&[-24., -50., 24., -50., 24., 50., -24., 50., -24., -50.]); v.pp(&[-24., -28., -14., -20., -24., -12.]);
            for i in 0..4 { if (st.cnt >> i) & 1 != 0 { v.circle(18., FOUR[i], 2.5); } }
        }
        Bz => { v.pp(&[-40., 0., -14., 0.]); v.pp(&[14., 0., 40., 0.]); v.pp(&[-14., -6., -6., -6., 4., -14., 4., 14., -6., 6., -14., 6., -14., -6.]); v.mv(12., -8.); v.arc(6., 0., 10., -0.8, 0.8, false); }
        Ldr | Ntc => {
            resistor(v); v.circle(0., 0., 26.);
            if k == Ldr { v.pp(&[-34., -30., -22., -20.]); v.pp(&[-22., -20., -27., -21.]); v.pp(&[-22., -20., -23., -25.]); v.pp(&[-24., -34., -12., -24.]); v.pp(&[-12., -24., -17., -25.]); v.pp(&[-12., -24., -13., -29.]); }
            else { v.pp(&[-26., 18., 26., -18.]); v.pp(&[26., -18., 26., -24.]); }
        }
        Dff => {
            v.pp(&[-40., -20., -24., -20.]); v.pp(&[-40., 20., -24., 20.]); v.pp(&[24., -20., 40., -20.]); v.pp(&[32., 20., 40., 20.]);
            v.pp(&[-24., -30., 24., -30., 24., 30., -24., 30., -24., -30.]); v.pp(&[-24., 12., -14., 20., -24., 28.]); v.circle(28., 20., 4.);
            if st.st { v.circle(4., -20., 4.); }
        }
        Reg => { v.pp(&[-40., -20., -24., -20.]); v.pp(&[24., -20., 40., -20.]); v.pp(&[0., 20., 0., 40.]); v.pp(&[-24., -30., 24., -30., 24., 20., -24., 20., -24., -30.]); }
        Sp => { v.pp(&[-40., 0., -14., 0.]); v.pp(&[30., -20., 40., -20.]); v.pp(&[30., 20., 40., 20.]); if st.on { v.pp(&[-14., 0., 30., 18.]) } else { v.pp(&[-14., 0., 30., -18.]) } }
        And | Or | Nand | Nor | Xor => {
            let inv = matches!(k, Nand | Nor);
            v.pp(&[-40., -20., -20., -20.]); v.pp(&[-40., 20., -20., 20.]);
            v.pp(&[if inv { 32. } else if k == And { 26. } else { 28. }, 0., 40., 0.]);
            if inv { v.circle(29., 0., 3.); }
            if k == Xor { v.pp(&[-25., -26., -15., 0., -25., 26.]); }
            if matches!(k, And | Nand) { v.pp(&[-20., -26., -20., 26., 0., 26.]); v.arc(0., 0., 26., PI / 2., -PI / 2., true); v.ln(-20., -26.); }
            else { v.mv(-20., -26.); v.quad(14., -26., 28., 0.); v.quad(14., 26., -20., 26.); v.quad(-8., 0., -20., -26.); }
        }
        Not => { v.pp(&[-40., 0., -14., 0.]); v.pp(&[-14., -14., -14., 14., 14., 0., -14., -14.]); v.circle(17., 0., 4.); v.pp(&[21., 0., 40., 0.]); }
        Jk | Srf | Tf => {
            let (l, ck): (&[f64], f64) = if k == Tf { (&[-20., 20.], 20.) } else { (&[-20., 0., 20.], 0.) };
            for &y in l { v.pp(&[-40., y, -24., y]); }
            v.pp(&[24., -20., 40., -20.]); v.pp(&[24., 20., 40., 20.]); v.pp(&[-24., -30., 24., -30., 24., 30., -24., 30., -24., -30.]);
            v.pp(&[-24., ck - 8., -14., ck, -24., ck + 8.]);
            if st.st { v.circle(4., -20., 4.); }
        }
        Tm => {
            for y in FOUR { v.pp(&[-40., y, -28., y]); }
            v.pp(&[28., -20., 40., -20.]); v.pp(&[28., 20., 40., 20.]); v.pp(&[0., -60., 0., -54.]); v.pp(&[0., 54., 0., 60.]);
            v.pp(&[-28., -54., 28., -54., 28., 54., -28., 54., -28., -54.]);
            v.stroke(); v.begin();
            for (t, x, y, f) in [("RST", -15., -40., 8.), ("TRG", -15., -20., 8.), ("THR", -15., 20., 8.), ("CTL", -15., 40., 8.), ("OUT", 15., -20., 8.), ("DIS", 15., 20., 8.), ("VCC", 0., -46., 8.), ("GND", 0., 47., 8.), ("555", 0., 0., 13.)] {
                v.text(x, y, t, f, f > 9.0, Align::Center, true, Col::Ink);
            }
        }
        Os => osc(st, v),
        Gnd => { v.pp(&[0., 0., 0., 6.]); v.pp(&[-10., 6., 10., 6.]); v.pp(&[-6., 10., 6., 10.]); v.pp(&[-2., 14., 2., 14.]); }
    }
    v.stroke();
}

fn osc(st: &SymState, v: &mut Cv) {
    let rgb = |s: &str| { let n = u32::from_str_radix(&s[1..], 16).unwrap(); Col::Rgba((n >> 16) as u8, (n >> 8) as u8, n as u8, 255) };
    v.fill_rect(-30., -60., 200., 120., rgb("#08140e"));
    v.begin();
    for i in 1..10 { let x = -30. + i as f64 * 20.; v.pp(&[x, -60., x, 60.]); }
    for j in 1..6 { let y = -60. + j as f64 * 20.; v.pp(&[-30., y, 170., y]); }
    v.style(rgb("#1f3b2c"), 1.0); v.stroke();
    v.begin(); v.pp(&[-30., 0., 170., 0.]); v.pp(&[70., -60., 70., 60.]); v.style(rgb("#3e6b52"), 1.0); v.stroke();
    v.stroke_rect(-30., -60., 200., 120., Col::Ink, 2.0);
    v.begin(); v.pp(&[-40., -20., -30., -20.]); v.pp(&[-40., 20., -30., 20.]); v.style(Col::Ink, 2.0); v.stroke(); v.begin();
    let Some((ta, tb)) = &st.scope else { return };
    let mm = |a: &[f32]| a.iter().fold((f64::MAX, f64::MIN), |(m, x), &p| (m.min(p as f64), x.max(p as f64)));
    let ((m1, x1), (m2, x2)) = (mm(ta), mm(tb));
    let vd = if st.vdiv > 0.0 { st.vdiv } else { nice(m1.abs().max(x1.abs()).max(m2.abs()).max(x2.abs()).max(1e-3) / 2.6) };
    for (a, cl) in [(ta, "#f5d020"), (tb, "#35c7ff")] {
        v.begin();
        for i in 0..NS.min(a.len()) {
            let x = -30. + i as f64 * 200. / NS as f64; let y = (-(a[i] as f64) / vd * 20.).clamp(-60., 60.);
            if i == 0 { v.mv(x, y) } else { v.ln(x, y) }
        }
        v.style(rgb(cl), 1.4); v.stroke();
    }
    v.text(-27., 56., &format!("CH1 {}", fm(x1 - m1, "Vpp")), 9., false, Align::Left, false, rgb("#f5d020"));
    v.text(50., 56., &format!("CH2 {}", fm(x2 - m2, "Vpp")), 9., false, Align::Left, false, rgb("#35c7ff"));
    v.text(-27., -51., &format!("{}/div{}", fm(vd, "V"), if st.vdiv > 0.0 { "" } else { " auto" }), 9., false, Align::Left, false, rgb("#9aabbb"));
    v.text(167., -51., &format!("{}/div", fm(st.tdiv, "s")), 9., false, Align::Right, false, rgb("#9aabbb"));
    v.style(Col::Ink, 2.0);
}

// ---------------------------------------------------------------- world-space component drawing
fn xf(c: &Component, p: Pt) -> Pt {
    let (mut x, mut y) = (if c.m != 0 { -p.0 } else { p.0 }, p.1);
    for _ in 0..c.r & 3 { (x, y) = (-y, x); }
    (c.x + x, c.y + y)
}

pub fn unit(k: Kind) -> &'static str {
    use Kind::*;
    match k {
        R | Pot | Lamp | Bz | Ldr | Ntc => "Ω", C => "F", L => "H",
        Dc | Ac | Zd | Mo | Op | Nm | Pm | And | Or | Not | Lg | Sg | Rl | Sol | Ctr | Reg | Dff | Jk | Srf | Tf => "V",
        Cs | Fu => "A", Os => "s/div", Tr => ":1", _ => "",
    }
}
/// Value label text (JS `lab`).
pub fn label(c: &Component) -> String {
    let Some(v) = &c.v else { return String::new() };
    let mut s = format!("{}{}", v, c.u.as_deref().unwrap_or(unit(c.t)));
    let f = c.f.as_deref().unwrap_or("");
    match c.t { Kind::Ac | Kind::Sg => s += &format!(" {f}Hz"), Kind::Pot | Kind::Ldr | Kind::Sol => s += &format!(" {f}%"), Kind::Ntc => s += &format!(" {f}\u{b0}C"), _ => {} }
    s
}
/// Meter / probe / motor reading (JS `rd`).
pub fn reading(k: Kind, av: f64, sq: f64, w: f64, val: f64) -> String {
    if k == Kind::Mo { let r = (w * 9.549).round(); return format!("{} rpm", if r == 0.0 { 0.0 } else { r }); }
    if k == Kind::Lg { return if av > val { "HIGH".into() } else { "LOW".into() }; }
    let rm = sq.sqrt(); let ac = (sq - av * av).max(0.0).sqrt();
    if k == Kind::Om { return if av > 1e8 { "OL".into() } else { fm(av, "Ω") }; }
    let u = if k == Kind::Vm { "V" } else { "A" };
    if ac > 0.02 * rm && ac > 1e-6 { format!("{} rms", fm(rm, u)) } else { fm(av, u) }
}

const PIN_LABELS: fn(Kind) -> Option<&'static [&'static str]> = |k| {
    use Kind::*;
    Some(match k {
        Op => &["+", "\u{2212}", "OUT"], And | Or | Nand | Nor | Xor => &["A", "B", "Y"], Not => &["A", "Y"],
        Dff => &["D", "CK", "Q", "Q'"], Jk => &["J", "CK", "K", "Q", "Q'"], Srf => &["S", "CK", "R", "Q", "Q'"], Tf => &["T", "CK", "Q", "Q'"],
        Reg => &["IN", "GND", "OUT"], Ctr => &["CK", "RST", "Q0", "Q1", "Q2", "Q3"], _ => return None,
    })
};

/// Text shown under a part: live reading for meters/probes/motors, otherwise the value label.
pub fn value_text(c: &Component, reading_txt: Option<&str>) -> String {
    if matches!(c.t, Kind::Vm | Kind::Am | Kind::Om | Kind::Mo | Kind::Lg) {
        reading_txt.map(String::from).unwrap_or_else(|| reading(c.t, 0., 0., 0., 1.))
    } else if c.t == Kind::Gnd { String::new() } else { label(c) }
}

/// Draw a component in world coordinates. `reading` overrides the value text for meters/probes/motors.
/// `ghost` returns only the (half-transparent) symbol, for placement previews.
pub fn draw_component(c: &Component, st: &SymState, reading_txt: Option<&str>, ghost: bool) -> Vec<Prim> {
    let mut v = Cv::default();
    v.style(Col::Ink, 2.0);
    let mut local = Cv::default();
    local.style(Col::Ink, 2.0);
    if st.gl > 0.03 {
        let (r, g, b) = if c.t == Kind::Led { (st.rgb[0], st.rgb[1], st.rgb[2]) } else { (255, 210, 60) };
        local.begin(); local.circle(0., 0., if c.t == Kind::Led { 18. } else { 16. });
        local.fill(Col::Rgba(r, g, b, (st.gl.min(1.0) * 0.85 * 255.0) as u8));
    }
    sym(c.t, st, &mut local);
    if c.t == Kind::Lg {
        // the probe lamp is a fill of the (last) circle subpath
        let col = if st.high { Col::Rgba(238, 17, 17, 255) } else { Col::Gray };
        if let Some(s) = local.subs.last() { local.prims.push(Prim::Fill { pts: s.clone(), col }); }
    }
    for p in local.prims {
        v.prims.push(match p {
            Prim::Stroke { pts, col, w } => Prim::Stroke { pts: pts.into_iter().map(|q| xf(c, q)).collect(), col, w },
            Prim::Fill { pts, col } => Prim::Fill { pts: pts.into_iter().map(|q| xf(c, q)).collect(), col },
            Prim::Text { p, s, size, bold, align, mid, col } => Prim::Text { p: xf(c, p), s, size, bold, align, mid, col },
        });
    }
    if ghost { return v.prims; }
    // pin labels
    if let Some(lbls) = PIN_LABELS(c.t) {
        for (i, (lp, wp)) in c.t.pins().iter().zip(c.pins()).enumerate() {
            let ld = if lp.0.abs() >= 40.0 { (if lp.0 > 0.0 { -1.0 } else { 1.0 }, 0.0) } else { (0.0, if lp.1 > 0.0 { -1.0 } else { 1.0 }) };
            let (mut sx, sy) = (if c.m != 0 { -ld.0 } else { ld.0 }, ld.1);
            let mut sy_ = sy;
            for _ in 0..c.r & 3 { let t = sx; sx = -sy_; sy_ = t; }
            let (mut x, mut y) = (wp.0 + sx * 9.0, wp.1 + sy_ * 9.0);
            if sy_ == 0.0 { y -= 6.0 } else { x += 8.0 }
            v.text(x, y, lbls[i], 8., false, Align::Center, true, Col::Ink);
        }
    }
    let meter = match c.t { Kind::Vm => Some("V"), Kind::Am => Some("A"), Kind::Om => Some("\u{3a9}"), Kind::Mo => Some("M"), _ => None };
    if let Some(l) = meter { v.text(c.x, c.y + 5., l, 14., true, Align::Center, false, Col::Ink); }
    let b = c.bbox();
    let no = c.no.unwrap_or([0., 0.]); let vo = c.vo.unwrap_or([0., 0.]);
    if let Some(n) = &c.n { v.text(c.x + no[0], b[1] - 4. + no[1], n, 11., true, Align::Center, false, Col::Ink); }
    let vs = value_text(c, reading_txt);
    if !vs.is_empty() { v.text(c.x + vo[0], b[3] + 10. + vo[1], &vs, 11., false, Align::Center, false, Col::Ink); }
    for p in c.pins() { v.begin(); v.circle(p.0, p.1, 2.5); v.fill(Col::Wire); }
    v.prims
}

/// Solder dots: points where more than two wire ends / pins meet, or a wire end touches a wire's interior.
pub fn junctions(p: &Project) -> Vec<Pt> {
    use std::collections::HashMap;
    let mut d: HashMap<(u64, u64), (Pt, u32)> = HashMap::new();
    let mut order: Vec<(u64, u64)> = vec![];
    fn add(d: &mut HashMap<(u64, u64), (Pt, u32)>, order: &mut Vec<(u64, u64)>, q: Pt, n: u32) {
        let k = (((q.0 + 0.0).to_bits()), ((q.1 + 0.0).to_bits()));
        let e = d.entry(k).or_insert_with(|| { order.push(k); (q, 0) }); e.1 += n;
    }
    let ws: Vec<_> = p.w.iter().filter(|w| w.x1 != w.x2 || w.y1 != w.y2).collect();
    for w in &ws { add(&mut d, &mut order, (w.x1, w.y1), 1); add(&mut d, &mut order, (w.x2, w.y2), 1); }
    for c in &p.c { for q in c.pins() { add(&mut d, &mut order, q, 1); } }
    let pts: Vec<Pt> = order.iter().map(|k| d[k].0).collect();
    for w in &ws { for &q in &pts { if w.on_seg(q) && !((q.0 == w.x1 && q.1 == w.y1) || (q.0 == w.x2 && q.1 == w.y2)) { add(&mut d, &mut order, q, 2); } } }
    order.iter().filter(|k| d[*k].1 > 2).map(|k| d[k].0).collect()
}
