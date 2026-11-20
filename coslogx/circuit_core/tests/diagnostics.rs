use circuit_core::{diagnostics::*, engine::Sim, examples, kind::Kind, project::*};

struct B { c: Vec<Component>, w: Vec<Wire> }
impl B {
    fn new() -> B { B { c: vec![], w: vec![] } }
    fn add(&mut self, k: Kind, x: f64, y: f64, v: Option<&str>, n: &str) -> usize {
        let mut c = Component::new(k, x, y, 0); if let Some(v) = v { c.v = Some(v.into()); } c.n = Some(n.into());
        self.c.push(c); self.c.len() - 1
    }
    fn conn(&mut self, a: usize, pa: usize, b: usize, pb: usize) { let (p, q) = (self.c[a].pins()[pa], self.c[b].pins()[pb]); self.w.push(Wire { x1: p.0, y1: p.1, x2: q.0, y2: q.1 }); }
    fn proj(&self) -> Project { Project { app: "CircuitLite".into(), ver: 1, seed: None, c: self.c.clone(), w: self.w.clone(), view: None } }
}
fn has(v: &[Diag], l: Level, s: &str) -> bool { v.iter().any(|d| d.level == l && d.msg.contains(s)) }

/// dc -> R -> gnd, the simplest healthy circuit
fn divider(r: &str) -> (B, usize, usize) {
    let mut b = B::new();
    let v = b.add(Kind::Dc, 0., 0., Some("10"), "V1"); let rr = b.add(Kind::R, 200., 0., Some(r), "R1"); let g = b.add(Kind::Gnd, 400., 0., None, "");
    b.conn(v, 0, rr, 0); b.conn(rr, 1, g, 0); b.conn(v, 1, g, 0);
    (b, v, rr)
}
fn run(p: &Project, t: f64) -> Sim { let mut s = Sim::new(p, 1.0); s.advance_to(t, None); s }

#[test]
fn statics_clean_and_empty() {
    assert!(has(&statics(&Project::empty(), 1.0), Level::Info, "Place parts"));
    let (b, ..) = divider("1k");
    let d = statics(&b.proj(), 1.0);
    assert_eq!(problem_count(&d), 0, "{d:?}");
    assert!(has(&d, Level::Info, "3 parts, 1 nodes, step 100 µs"), "{d:?}");
    assert!(has(&d, Level::Info, "1 independent section"));
}

#[test]
fn statics_missing_ground_and_dangling_and_floating() {
    let mut b = B::new();
    let v = b.add(Kind::Dc, 0., 0., Some("5"), "V1"); let r = b.add(Kind::R, 200., 0., Some("1k"), "R1");
    b.conn(v, 0, r, 0);                                  // R1 pin 2 and V1 pin 2 dangle
    let d = statics(&b.proj(), 1.0);
    assert!(has(&d, Level::Error, "No Ground part"));
    assert!(has(&d, Level::Warn, "Unconnected pin(s): V1 pin 2, R1 pin 2"), "{d:?}");
    assert_eq!(d[0].level, Level::Error);
    // capacitor-only branch has no DC path to ground
    let mut b = B::new();
    let v = b.add(Kind::Dc, 0., 0., Some("5"), "V1"); let c = b.add(Kind::C, 200., 0., Some("1u"), "C1"); let r = b.add(Kind::R, 400., 0., Some("1k"), "R1"); let g = b.add(Kind::Gnd, 600., 0., None, "");
    b.conn(v, 0, c, 0); b.conn(c, 1, r, 0); b.conn(r, 1, g, 0); b.conn(v, 1, g, 0);
    let d = statics(&b.proj(), 1.0);
    assert!(!has(&d, Level::Warn, "No DC path"), "R1 return path to ground exists: {d:?}");
    let mut b2 = b; b2.w.pop(); b2.w.remove(0);   // cut the source loop: C1/R1 are now islands without DC reference
    let d = statics(&b2.proj(), 1.0);
    assert!(d.iter().any(|x| x.msg.contains("No DC path to ground") || x.msg.contains("Unconnected")), "{d:?}");
}

#[test]
fn statics_shorted_source_and_optional_pins() {
    let mut b = B::new();
    let v = b.add(Kind::Dc, 0., 0., Some("5"), "V1"); let g = b.add(Kind::Gnd, 200., 0., None, "");
    b.conn(v, 0, v, 1); b.conn(v, 0, g, 0);
    assert!(has(&statics(&b.proj(), 1.0), Level::Error, "V1 has both terminals on one node"));
    // unwired 555 control pin and flip-flop outputs are not "unconnected"
    let mut b = B::new();
    let t = b.add(Kind::Tm, 0., 0., None, "U1"); let g = b.add(Kind::Gnd, 300., 0., None, "");
    for p in [0usize, 1, 2, 3, 4, 5, 6] { b.conn(t, p, g, 0); }
    let d = statics(&b.proj(), 1.0);
    assert!(!d.iter().any(|x| x.msg.contains("Unconnected")), "{d:?}");
}

#[test]
fn healthy_circuit_has_no_runtime_problems() {
    let (b, ..) = divider("1k"); let p = b.proj();
    let mut s = run(&p, 0.2); let mut f = Faults::default();
    let d = dynamic(&mut s, &p, &mut f, &DynAcc::default(), true, 1.0, 0.0);
    assert!(d.is_empty(), "{d:?}"); assert!(f.comps.is_empty() && f.wires.is_empty());
}

#[test]
fn overheating_resistor_is_flagged_with_ratings() {
    let (b, _, rr) = divider("10");           // 10 V across 10 Ω = 10 W in a 0.25 W resistor
    let p = b.proj(); let mut s = run(&p, 0.6); let mut f = Faults::default();
    let d = dynamic(&mut s, &p, &mut f, &DynAcc::default(), true, 1.0, 1000.0);
    assert!(has(&d, Level::Error, "R1: overheating"), "{d:?}");
    assert!(has(&d, Level::Error, "exceeds 250 mW"));
    assert_eq!(f.comps[&rr].level, Level::Error);
    // near-limit is a warning: 0.2 W in 0.25 W
    let (b, _, rr) = divider("500"); let p = b.proj(); let mut s = run(&p, 1.5); let mut f = Faults::default();
    let d = dynamic(&mut s, &p, &mut f, &DynAcc::default(), true, 1.0, 0.0);
    assert!(has(&d, Level::Warn, "near limit 250 mW"), "{d:?}"); assert_eq!(f.comps[&rr].level, Level::Warn);
}

#[test]
fn led_without_resistor_warns() {
    let mut b = B::new();
    let v = b.add(Kind::Dc, 0., 0., Some("5"), "V1"); let r = b.add(Kind::R, 200., 0., Some("10"), "R1"); let l = b.add(Kind::Led, 400., 0., None, "LED1"); let g = b.add(Kind::Gnd, 600., 0., None, "");
    b.conn(v, 0, r, 0); b.conn(r, 1, l, 0); b.conn(l, 1, g, 0); b.conn(v, 1, g, 0);
    let p = b.proj(); let mut s = run(&p, 0.05); let mut f = Faults::default();
    let d = dynamic(&mut s, &p, &mut f, &DynAcc::default(), true, 1.0, 0.0);
    assert!(has(&d, Level::Warn, "LED1 carries") && has(&d, Level::Warn, "add a current-limiting resistor"), "{d:?}");
}

#[test]
fn short_circuit_is_traced_through_ammeter_and_wires() {
    let mut b = B::new();
    let v = b.add(Kind::Dc, 0., 0., Some("9"), "V1"); let a = b.add(Kind::Am, 200., 0., None, "AM1"); let g = b.add(Kind::Gnd, 400., 0., None, "");
    b.conn(v, 0, a, 0); b.conn(a, 1, g, 0); b.conn(v, 1, g, 0);
    let p = b.proj(); let mut s = run(&p, 0.01); let mut f = Faults::default();
    let d = dynamic(&mut s, &p, &mut f, &DynAcc::default(), true, 1.0, 0.0);
    assert!(has(&d, Level::Error, "V1 is short-circuited through AM1. Marked in red"), "{d:?}");
    assert!(has(&d, Level::Error, "V1 supplies"), "{d:?}");
    assert!(f.comps[&v].msg.starts_with("short circuit")); assert_eq!(f.comps[&a].level, Level::Error); // (its overload rating error then replaces the label, as in the JS)
    assert_eq!(f.wires.len(), 3, "all three wires carry the short"); assert!(f.wires.values().all(|w| w.level == Level::Error));
}

#[test]
fn faults_expire_after_hold_and_pins_are_marked() {
    let mut b = B::new();
    let v = b.add(Kind::Dc, 0., 0., Some("5"), "V1"); let r = b.add(Kind::R, 200., 0., Some("1k"), "R1"); let g = b.add(Kind::Gnd, 400., 0., None, "");
    b.conn(v, 0, r, 0); b.conn(v, 1, g, 0); b.conn(r, 1, g, 0);
    b.add(Kind::R, 600., 0., Some("1k"), "R2"); let r2 = b.c.len() - 1; b.conn(r2, 0, g, 0); // R2 pin 2 dangles
    let p = b.proj(); let mut s = run(&p, 0.01); let mut f = Faults::default();
    dynamic(&mut s, &p, &mut f, &DynAcc::default(), true, 1.0, 0.0);
    let fl = &f.comps[&r2]; assert_eq!((fl.level, fl.pin, fl.until), (Level::Warn, Some(1), 3000.0));
    // fix the circuit (remove R2's pin problem by removing R2) and rescan later: marker is gone after the hold
    f.comps.insert(0, Fault { level: Level::Error, msg: "x".into(), pin: None, until: 100.0 });
    dynamic(&mut s, &p, &mut f, &DynAcc::default(), true, 1.0, 200.0);
    assert!(!f.comps.contains_key(&0));
}

#[test]
fn solver_lag_and_overvoltage_messages() {
    let (b, ..) = divider("1k"); let p = b.proj(); let mut s = run(&p, 0.01); let mut f = Faults::default();
    let acc = DynAcc { fail: 3, lag: 5, mv: 2500.0, md: 0.5, bn: 1 };
    let d = dynamic(&mut s, &p, &mut f, &acc, true, 2.0, 0.0);
    assert!(has(&d, Level::Error, "Solver did not converge in 3 step(s) (last change 500 mV)"));
    assert!(has(&d, Level::Warn, "Auto quality has coarsened the time step"));
    assert!(has(&d, Level::Error, "Node voltage reached 2.5 kV"));
    let d = dynamic(&mut s, &p, &mut f, &acc, false, 1.0, 0.0);
    assert!(has(&d, Level::Warn, "Set Quality to Auto or Fast"));
    assert!(f.wires.values().any(|w| w.level == Level::Error), "wires on the runaway node are marked");
}

#[test]
fn merged_sorts_errors_first_and_counts_problems() {
    let v = merged(&[Diag { level: Level::Info, msg: "i".into() }], &[Diag { level: Level::Warn, msg: "w".into() }, Diag { level: Level::Error, msg: "e".into() }]);
    assert_eq!(v.iter().map(|d| d.msg.as_str()).collect::<String>(), "ewi"); assert_eq!(problem_count(&v), 2);
}

#[test]
fn buzzer_tone_follows_drive_voltage() {
    let mut b = B::new();
    let v = b.add(Kind::Dc, 0., 0., Some("5"), "V1"); let z = b.add(Kind::Bz, 200., 0., None, "BZ1"); let g = b.add(Kind::Gnd, 400., 0., None, "");
    b.conn(v, 0, z, 0); b.conn(z, 1, g, 0); b.conn(v, 1, g, 0);
    let p = b.proj();
    assert_eq!(run(&p, 0.0003).buzzer_tone(), None);
    assert_eq!(run(&p, 0.5).buzzer_tone(), Some(2000.0));
}

#[test]
fn examples_are_clean_and_run() {
    for (n, e) in examples::LIST {
        let p = e.build();
        let names: Vec<_> = p.c.iter().filter_map(|c| c.n.clone()).collect();
        let mut u = names.clone(); u.sort(); u.dedup();
        assert_eq!(u.len(), names.len(), "{n}: duplicate designators");
        assert!(p.c.iter().all(|c| c.t.prefix().is_none() || c.n.is_some()), "{n}: unnamed part");
        let d = statics(&p, 1.0);
        assert!(!d.iter().any(|x| x.level == Level::Error), "{n}: {d:?}");
    }
    assert_eq!(examples::stress(6).c.len(), 48); assert_eq!(examples::stress(16).c.len(), 128); assert_eq!(examples::stress(32).c.len(), 256);
    let mut s = Sim::new(&examples::stress(6), 1.0);
    assert_eq!(s.islands.len(), 6);
    let st = s.advance_to(0.02, None); assert_eq!(st.fails, 0);
}

#[test]
fn d555_example_blinks_the_led() {
    let p = examples::d555();
    let led = p.c.iter().position(|c| c.t == Kind::Led).unwrap();
    let mut s = Sim::new(&p, 1.0);
    let (mut on, mut off, mut t) = (false, false, 0.0);
    while t < 1.5 { t += 5e-3; s.advance_to(t, None); let g = s.part(led).unwrap().gl; if g > 0.5 { on = true } if t > 0.2 && g < 0.1 { off = true } }
    assert!(on && off, "LED should both light and go dark");
}

#[test]
fn loading_fills_missing_fields_like_the_js() {
    let p = Project::from_json(r#"{"c":[{"t":"c","x":0,"y":0,"r":0,"v":"1u"},{"t":"sg","x":100,"y":0}],"w":[]}"#).unwrap();
    assert_eq!(p.c[0].s.as_deref(), Some("50"));       // voltage rating default
    assert_eq!(p.c[0].v.as_deref(), Some("1u"));       // file value wins
    assert_eq!((p.c[1].k.as_deref(), p.c[1].q.as_deref(), p.c[1].f.as_deref()), (Some("sine"), Some("50"), Some("1k")));
}
