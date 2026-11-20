use circuit_core::{kind::Kind, project::*, units::*};

#[test]
fn si_parse() {
    assert_eq!(pv("1k"), 1000.0);
    assert!((pv("4.7u") - 4.7e-6).abs() < 1e-18);
    assert_eq!(pv("2meg"), 2e6);
    assert_eq!(pv("2M"), 2e6);
    assert_eq!(pv("5m"), 5e-3);
    assert_eq!(pv("1e3"), 1000.0);
    assert_eq!(pv(" 100 Ω"), 100.0);
    assert!(pv("abc").is_nan());
}
#[test]
fn si_format() {
    assert_eq!(fm(0.0047, "A"), "4.7 mA");
    assert_eq!(fm(1234.0, "Ω"), "1.23 kΩ");
    assert_eq!(fm(0.0, "V"), "0 V");
    assert_eq!(fm(-9.0, "V"), "-9 V");
    assert_eq!(fm(2.2e-5, "s"), "22 µs");
}
#[test]
fn geometry() {
    let mut c = Component::new(Kind::Npn, 100.0, 100.0, 0);
    assert_eq!(c.pins()[1], (120.0, 60.0));
    c.m = 1;
    assert_eq!(c.pins()[0], (140.0, 100.0));
    c.m = 0; c.r = 1; // (x,y)->(-y,x)
    assert_eq!(c.pins()[1], (140.0, 120.0));
    let b = Component::new(Kind::R, 0.0, 0.0, 0).bbox();
    assert_eq!(b, [-54.0, -14.0, 54.0, 14.0]);
    assert_eq!(Kind::ALL.len(), 50);
}
#[test]
fn demo_roundtrip() {
    let s = std::fs::read_to_string("assets/demo.json").unwrap();
    let p = Project::from_json(&s).unwrap();
    assert_eq!(p.c.len(), 29);
    assert_eq!(p.c.iter().filter(|c| c.t == Kind::Npn).count(), 1);
    assert_eq!(p.c.iter().find(|c| c.n.as_deref() == Some("R1")).unwrap().q.as_deref(), Some("0.5"));
    let p2 = Project::from_json(&p.to_json()).unwrap();
    assert_eq!(p, p2);
    // JS emits 100, serde emits 100.0: identical numerically, so compare numerically.
    let a: Project = Project::from_json(&s).unwrap();
    assert_eq!(a.w.len(), 52);
    assert_eq!(a.view, Some(View { x: 20.0, y: 10.0, z: 0.7 }));
}
