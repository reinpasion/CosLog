//! Built-in example circuits (port of the JS `DEMO`, `D555()` and `stress(n)`).
use crate::kind::Kind;
use crate::project::{Component, Project, View, Wire};
use std::collections::HashSet;

pub const DEMO_JSON: &str = include_str!("../assets/demo.json");

/// Rectifier + zener/LED + oscilloscope, and a BJT common-emitter amplifier.
pub fn demo() -> Project { Project::from_json(DEMO_JSON).unwrap() }

fn assign_names(p: &mut Project) {
    let mut used: HashSet<String> = p.c.iter().filter_map(|c| c.n.clone()).collect();
    for c in p.c.iter_mut() {
        if c.n.is_none() {
            if let Some(pre) = c.t.prefix() {
                let n = (1..).map(|i| format!("{pre}{i}")).find(|n| !used.contains(n)).unwrap();
                used.insert(n.clone()); c.n = Some(n);
            }
        }
    }
}
fn part(t: Kind, x: f64, y: f64, r: u8, f: impl FnOnce(&mut Component)) -> Component { let mut c = Component::new(t, x, y, r); f(&mut c); c }
fn w(a: f64, b: f64, c: f64, d: f64) -> Wire { Wire { x1: a, y1: b, x2: c, y2: d } }

/// 555 astable LED blinker (~3 Hz).
pub fn d555() -> Project {
    let v = |c: &mut Component, s: &str| c.v = Some(s.into());
    let mut p = Project::empty();
    p.c = vec![
        part(Kind::Dc, 100., 200., 0, |c| v(c, "9")), part(Kind::Gnd, 100., 380., 0, |_| {}), part(Kind::Tm, 360., 200., 0, |_| {}),
        part(Kind::R, 480., 160., 1, |c| v(c, "1k")), part(Kind::R, 480., 260., 1, |c| v(c, "22k")), part(Kind::C, 280., 340., 1, |c| v(c, "10u")),
        part(Kind::R, 600., 180., 0, |c| v(c, "470")), part(Kind::Led, 700., 220., 1, |c| c.p = Some("red".into())),
    ];
    p.w = vec![w(100., 160., 100., 100.), w(100., 100., 480., 100.), w(360., 100., 360., 140.), w(480., 100., 480., 120.), w(320., 160., 280., 160.),
        w(280., 160., 280., 100.), w(480., 200., 480., 220.), w(400., 220., 480., 220.), w(320., 180., 280., 180.), w(280., 180., 280., 300.),
        w(320., 220., 280., 220.), w(480., 300., 280., 300.), w(100., 240., 100., 380.), w(100., 380., 700., 380.), w(360., 260., 360., 380.),
        w(400., 180., 560., 180.), w(640., 180., 700., 180.), w(700., 260., 700., 380.)];
    p.view = Some(View { x: 40., y: 30., z: 0.9 });
    assign_names(&mut p); p
}

/// `n` independent AC -> diode -> RC -> LED channels (one island each), for benchmarking the multi-core engine.
pub fn stress(n: usize) -> Project {
    let mut p = Project::empty();
    for i in 0..n {
        let y = 100.0 + i as f64 * 120.0;
        let v = |c: &mut Component, s: &str| c.v = Some(s.into());
        p.c.push(part(Kind::Ac, 100., y + 40., 0, |c| { v(c, &format!("{}", 10 + i % 5 * 4)); c.f = Some("200".into()); }));
        p.c.push(part(Kind::Gnd, 100., y + 80., 0, |_| {}));
        p.c.push(part(Kind::R, 200., y, 0, |c| v(c, "1k")));
        p.c.push(part(Kind::D, 320., y, 0, |_| {}));
        p.c.push(part(Kind::C, 420., y + 40., 1, |c| v(c, "10u")));
        p.c.push(part(Kind::R, 500., y + 40., 1, |c| v(c, "2k")));
        p.c.push(part(Kind::R, 580., y, 0, |c| v(c, "330")));
        p.c.push(part(Kind::Led, 700., y, 0, |c| c.p = Some("red".into())));
        p.w.extend([w(100., y, 160., y), w(240., y, 280., y), w(360., y, 540., y), w(620., y, 660., y), w(740., y, 740., y + 80.), w(100., y + 80., 740., y + 80.)]);
    }
    p.view = Some(View { x: 10., y: 10., z: (700.0 / (120.0 * n as f64 + 100.0)).clamp(0.12, 0.7) });
    assign_names(&mut p); p
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Example { Demo, Timer555, Stress(usize) }
pub const LIST: &[(&str, Example)] = &[
    ("Rectifier + transistor amp", Example::Demo), ("555 astable LED blinker", Example::Timer555),
    ("Stress test: 6 channels (48 parts)", Example::Stress(6)), ("Stress test: 16 channels (128 parts)", Example::Stress(16)),
    ("Stress test: 32 channels (256 parts)", Example::Stress(32)),
];
impl Example {
    pub fn build(self) -> Project { match self { Example::Demo => demo(), Example::Timer555 => d555(), Example::Stress(n) => stress(n) } }
}
