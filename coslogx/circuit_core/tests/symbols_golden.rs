//! Symbol output is pinned to a golden file captured from the hand-written renderer, so the spec-driven
//! interpreter must reproduce it. Regenerate only deliberately: `GOLDEN=write cargo test --test symbols_golden`.
use circuit_core::{kind::Kind, project::Component, symbols::*, netlist::Wave};

fn num(f: f64) -> String { format!("{:.4}", f + 0.0) }
fn col(c: &Col) -> String { match c { Col::Ink => "ink".into(), Col::Wire => "wire".into(), Col::Gray => "gray".into(), Col::Rgba(r, g, b, a) => format!("{r},{g},{b},{a}") } }
fn dump(ps: &[Prim]) -> String {
    let mut o = vec![];
    for p in ps { match p {
        Prim::Stroke { pts, col: c, w } => o.push(format!("S {} {} {}", col(c), w, pts.iter().map(|q| format!("{} {}", num(q.0), num(q.1))).collect::<Vec<_>>().join(" "))),
        Prim::Fill { pts, col: c } => o.push(format!("F {} {}", col(c), pts.iter().map(|q| format!("{} {}", num(q.0), num(q.1))).collect::<Vec<_>>().join(" "))),
        Prim::Text { p, s, size, bold, align, mid, col: c } => o.push(format!("T {} {} {} {} {:?} {} {}", num(p.0), num(p.1), s.replace(' ', "_"), size, align, *bold as u8 + 2 * *mid as u8, col(c))),
    } }
    o.join("\n")
}

fn cases() -> Vec<(String, String)> {
    let mut v = vec![];
    for &k in Kind::ALL {
        for variant in 0..6 {
            let mut c = Component::new(k, 100.0, 100.0, (variant % 4) as u8); c.m = (variant / 3) as u8 % 2;
            if k == Kind::Pot { c.f = Some(["0", "50", "100", "25", "75", "100"][variant].into()); }
            if k == Kind::Sg { c.k = Some(["sine", "square", "triangle", "sawtooth", "sine", "square"][variant].into()); }
            if k == Kind::Led { c.p = Some(["red", "green", "blue", "white", "yellow", "red"][variant].into()); }
            let mut st = SymState::edit(&c);
            st.on = variant % 2 == 1; st.st = variant % 3 == 1; st.bl = variant % 2 == 0 && variant > 1; st.cnt = [0, 5, 10, 15, 3, 9][variant];
            st.ang = [0.0, 1.0, 2.0, 0.3, 4.0, 5.5][variant]; st.gl = [0.0, 0.5, 0.0, 1.0, 0.2, 0.03][variant]; st.high = variant % 2 == 0; st.wave = Wave::Sine;
            let _ = &mut st;
            v.push((format!("{} v{}", k.code(), variant), dump(&draw_component(&c, &st, None, false))));
        }
    }
    v
}

#[test]
fn symbols_match_golden() {
    let all: String = cases().into_iter().map(|(n, d)| format!("## {n}\n{d}\n")).collect();
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/symbols.txt");
    if std::env::var("GOLDEN").is_ok() { std::fs::write(path, &all).unwrap(); return; }
    let want = std::fs::read_to_string(path).expect("golden file");
    let (a, b): (Vec<&str>, Vec<&str>) = (all.lines().collect(), want.lines().collect());
    assert_eq!(a.len(), b.len(), "line count");
    let mut case = String::new();
    for (x, y) in a.iter().zip(&b) {
        if x.starts_with("## ") { case = x.to_string(); }
        let (tx, ty): (Vec<&str>, Vec<&str>) = (x.split(' ').collect(), y.split(' ').collect());
        assert_eq!(tx.len(), ty.len(), "{case}: token count\n{x}\n{y}");
        for (p, q) in tx.iter().zip(&ty) {
            match (p.parse::<f64>(), q.parse::<f64>()) { (Ok(m), Ok(n)) => assert!((m - n).abs() < 2e-3, "{case}: {p} vs {q}\n{x}\n{y}"), _ => assert_eq!(p, q, "{case}\n{x}\n{y}") }
        }
    }
}
