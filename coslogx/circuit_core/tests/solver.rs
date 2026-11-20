use circuit_core::{engine::Sim, kind::Kind, project::*};

struct B { c: Vec<Component>, w: Vec<Wire> }
impl B {
    fn new() -> B { B { c: vec![], w: vec![] } }
    fn add(&mut self, k: Kind, x: f64, y: f64, v: Option<&str>) -> usize {
        let mut c = Component::new(k, x, y, 0);
        if let Some(v) = v { c.v = Some(v.into()); }
        self.c.push(c); self.c.len() - 1
    }
    fn pin(&self, i: usize, p: usize) -> (f64, f64) { self.c[i].pins()[p] }
    fn wire(&mut self, a: (f64, f64), b: (f64, f64)) { self.w.push(Wire { x1: a.0, y1: a.1, x2: b.0, y2: b.1 }); }
    fn conn(&mut self, a: usize, pa: usize, b: usize, pb: usize) { let (p, q) = (self.pin(a, pa), self.pin(b, pb)); self.wire(p, q); }
    fn proj(&self) -> Project { Project { app: "CircuitLite".into(), ver: 1, c: self.c.clone(), w: self.w.clone(), view: None } }
}

#[test]
fn voltage_divider() {
    let mut b = B::new();
    let v = b.add(Kind::Dc, 0.0, 0.0, Some("10"));
    let r1 = b.add(Kind::R, 200.0, 0.0, Some("1k"));
    let r2 = b.add(Kind::R, 400.0, 0.0, Some("3k"));
    let g = b.add(Kind::Gnd, 600.0, 0.0, None);
    b.conn(v, 0, r1, 0); b.conn(r1, 1, r2, 0); b.conn(r2, 1, g, 0); b.conn(v, 1, g, 0);
    let mut s = Sim::new(&b.proj(), 1.0);
    s.advance_to(0.001, None);
    assert!((s.pin_v(r1, 0) - 10.0).abs() < 1e-4);
    assert!((s.pin_v(r1, 1) - 7.5).abs() < 1e-4, "{}", s.pin_v(r1, 1));
    assert_eq!(s.islands.len(), 1);
}

#[test]
fn rc_step_response() {
    let mut b = B::new();
    let v = b.add(Kind::Dc, 0.0, 0.0, Some("5"));
    let r = b.add(Kind::R, 200.0, 0.0, Some("1k"));
    let c = b.add(Kind::C, 400.0, 0.0, Some("10u"));
    let g = b.add(Kind::Gnd, 600.0, 0.0, None);
    b.conn(v, 0, r, 0); b.conn(r, 1, c, 0); b.conn(c, 1, g, 0); b.conn(v, 1, g, 0);
    let mut s = Sim::new(&b.proj(), 1.0);
    s.advance_to(0.01, None); // one time constant
    let vc = s.pin_v(c, 0);
    assert!((vc - 5.0 * (1.0 - (-1f64).exp())).abs() < 0.02, "vc={vc}");
    s.advance_to(0.06, None);
    assert!((s.pin_v(c, 0) - 5.0).abs() < 0.05);
}

#[test]
fn bridge_rectifier() {
    let mut b = B::new();
    let ac = b.add(Kind::Ac, 0.0, 0.0, Some("10"));
    let br = b.add(Kind::Br, 200.0, 0.0, None);
    let r = b.add(Kind::R, 400.0, 0.0, Some("1k"));
    let g = b.add(Kind::Gnd, 600.0, 0.0, None);
    b.conn(ac, 0, br, 0); b.conn(ac, 1, br, 1);
    b.conn(br, 3, g, 0); b.conn(br, 2, r, 0); b.conn(r, 1, g, 0);
    let mut s = Sim::new(&b.proj(), 1.0);
    assert!(s.islands[0].q);
    let (mut mx, mut mn) = (f64::MIN, f64::MAX);
    let mut t = 0.0;
    while t < 0.06 { t += 1e-4; s.advance_to(t, None); if t > 0.02 { let v = s.pin_v(r, 0); mx = mx.max(v); mn = mn.min(v); } }
    assert!(mx > 8.0 && mx < 9.2, "peak {mx}");
    assert!(mn > -0.05, "min {mn}");
}

#[test]
fn inverting_opamp() {
    let mut b = B::new();
    let v = b.add(Kind::Dc, 0.0, 0.0, Some("1"));
    let r1 = b.add(Kind::R, 200.0, 0.0, Some("1k"));
    let rf = b.add(Kind::R, 200.0, 200.0, Some("5k"));
    let op = b.add(Kind::Op, 400.0, 0.0, Some("12"));
    let g = b.add(Kind::Gnd, 600.0, 0.0, None);
    b.conn(v, 0, r1, 0); b.conn(r1, 1, op, 1); b.conn(rf, 0, op, 1); b.conn(rf, 1, op, 2);
    b.conn(op, 0, g, 0); b.conn(v, 1, g, 0);
    let mut s = Sim::new(&b.proj(), 1.0);
    s.advance_to(0.001, None);
    assert!((s.pin_v(op, 2) + 5.0).abs() < 0.05, "vout={}", s.pin_v(op, 2));
    assert_eq!(s.islands[0].fail, 0);
}

fn d555() -> Project {
    let mut b = B::new();
    let v = b.add(Kind::Dc, 0.0, 0.0, Some("9"));
    let g = b.add(Kind::Gnd, 700.0, 0.0, None);
    let t = b.add(Kind::Tm, 200.0, 0.0, None);
    let r1 = b.add(Kind::R, 400.0, 0.0, Some("1k"));
    let r2 = b.add(Kind::R, 400.0, 200.0, Some("22k"));
    let c = b.add(Kind::C, 600.0, 200.0, Some("10u"));
    // pins: 0 VCC 1 GND 2 TRG 3 THR 4 DIS 5 OUT 6 RST 7 CTL
    b.conn(v, 0, t, 0); b.conn(v, 0, t, 6); b.conn(v, 0, r1, 0);
    b.conn(r1, 1, t, 4); b.conn(r1, 1, r2, 0);
    b.conn(r2, 1, t, 3); b.conn(t, 3, t, 2); b.conn(t, 3, c, 0);
    b.conn(c, 1, g, 0); b.conn(t, 1, g, 0); b.conn(v, 1, g, 0);
    b.proj()
}

#[test]
fn timer555_astable_frequency() {
    let p = d555();
    let mut s = Sim::new(&p, 1.0);
    assert!((s.islands[0].dt - 2e-5).abs() < 1e-12);
    let (mut edges, mut prev) = (vec![], false);
    let mut t = 0.0;
    while t < 2.0 { t += 1e-3; s.advance_to(t, None); let hi = s.pin_v(2, 5) > 4.5; if hi && !prev { edges.push(t); } prev = hi; }
    assert!(edges.len() >= 5, "edges {:?}", edges);
    let n = edges.len();
    let period = (edges[n - 1] - edges[1]) / (n - 2) as f64;
    // ideal: ln2*(R1+2*R2)*C = 0.3119 s
    assert!((period - 0.3119).abs() < 0.015, "period {period}");
}

#[test]
fn demo_project_runs_clean() {
    let p = Project::from_json(&std::fs::read_to_string("assets/demo.json").unwrap()).unwrap();
    let mut s = Sim::new(&p, 1.0);
    // rectifier+zener/LED, scope, and BJT amp are only coupled through ground
    assert!(s.islands.len() >= 2, "islands {}", s.islands.len());
    let mut fails = 0;
    let mut t = 0.0;
    while t < 0.2 { t += 5e-3; fails += s.advance_to(t, None).fails; }
    assert_eq!(fails, 0);
    for i in &s.islands { assert!(i.v.iter().all(|x| x.is_finite() && x.abs() < 100.0)); }
    // BJT amp: collector sits between rails
    let q = p.c.iter().position(|c| c.t == Kind::Npn).unwrap();
    let vc = s.pin_v(q, 1);
    assert!(vc > 0.5 && vc < 12.0, "Vc={vc}");
}

#[test]
fn islands_split_at_ground_and_rebuild_keeps_state() {
    let mut b = B::new();
    for k in 0..3 {
        let y = 300.0 * k as f64;
        let v = b.add(Kind::Dc, 0.0, y, Some("5"));
        let r = b.add(Kind::R, 200.0, y, Some("1k"));
        let c = b.add(Kind::C, 400.0, y, Some("10u"));
        let g = b.add(Kind::Gnd, 600.0, y, None);
        b.conn(v, 0, r, 0); b.conn(r, 1, c, 0); b.conn(c, 1, g, 0); b.conn(v, 1, g, 0);
    }
    let p = b.proj();
    let mut s = Sim::new(&p, 1.0);
    assert_eq!(s.islands.len(), 3);
    s.advance_to(0.01, None);
    let vc = s.pin_v(2, 0);
    let s2 = s.rebuild(&p, 2.0);
    assert!((s2.time() - s.time()).abs() < 1e-12);
    assert!((s2.pin_v(2, 0) - vc).abs() < 1e-9 || s2.part(2).unwrap().vp > 0.0);
    assert!((s2.islands[0].dt - 2.0 * s.islands[0].dt).abs() < 1e-12);
}
