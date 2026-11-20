use circuit_core::{kind::Kind, project::*, symbols::*};

fn strokes(p: &[Prim]) -> usize { p.iter().filter(|x| matches!(x, Prim::Stroke { .. })).count() }
fn texts(p: &[Prim]) -> Vec<String> { p.iter().filter_map(|x| if let Prim::Text { s, .. } = x { Some(s.clone()) } else { None }).collect() }

#[test]
fn every_kind_draws_inside_its_bbox() {
    for &k in Kind::ALL {
        let c = Component::new(k, 200.0, 200.0, 0);
        let prims = draw_component(&c, &SymState::edit(&c), None, false);
        assert!(strokes(&prims) > 0, "{k:?} draws nothing");
        let b = c.bbox();
        // LDR/NTC/solar circles are deliberately larger than their (JS) hit boxes
        let tol = if matches!(k, Kind::Ldr | Kind::Ntc | Kind::Sol) { 30.0 } else { 14.0 };
        for p in &prims {
            if let Prim::Stroke { pts, .. } = p {
                for q in pts {
                    assert!(q.0.is_finite() && q.1.is_finite(), "{k:?} NaN");
                    // symbols must stay within the hit box (plus a small tolerance for glyph tips)
                    assert!(q.0 >= b[0] - tol && q.0 <= b[2] + tol && q.1 >= b[1] - tol && q.1 <= b[3] + tol, "{k:?} point {q:?} outside {b:?}");
                }
            }
        }
    }
}

#[test]
fn rotation_and_mirror_move_geometry() {
    let mut c = Component::new(Kind::Npn, 0.0, 0.0, 0);
    let minx = |c: &Component| { let mut m = f64::MAX; for p in draw_component(c, &SymState::edit(c), None, true) { if let Prim::Stroke { pts, .. } = p { for q in pts { m = m.min(q.0); } } } m };
    assert!((minx(&c) + 40.0).abs() < 1e-9); // base lead on the left
    c.m = 1;
    assert!((minx(&c) + 20.0).abs() < 1e-9); // mirrored: collector/emitter leads now on the left
}

#[test]
fn labels_and_readings() {
    let mut c = Component::new(Kind::Ac, 0.0, 0.0, 0); c.f = Some("60".into());
    assert_eq!(label(&c), "12V 60Hz");
    let mut p = Component::new(Kind::Pot, 0.0, 0.0, 0); p.f = Some("25".into());
    assert_eq!(label(&p), "10kΩ 25%");
    let mut n = Component::new(Kind::Ntc, 0.0, 0.0, 0); n.u = Some("".into());
    assert_eq!(label(&n), "10k 25°C");
    assert_eq!(label(&Component::new(Kind::Gnd, 0.0, 0.0, 0)), "");
    assert_eq!(reading(Kind::Vm, 5.0, 25.0, 0.0, 1.0), "5 V");
    assert_eq!(reading(Kind::Vm, 0.0, 50.0, 0.0, 1.0), "7.07 V rms");
    assert_eq!(reading(Kind::Om, 2e9, 0.0, 0.0, 1.0), "OL");
    assert_eq!(reading(Kind::Lg, 3.0, 0.0, 0.0, 2.5), "HIGH");
    assert_eq!(reading(Kind::Mo, 0.0, 0.0, 100.0, 12.0), "955 rpm");
    let mut v = Component::new(Kind::Vm, 0.0, 0.0, 0); v.n = Some("VM1".into());
    let t = texts(&draw_component(&v, &SymState::edit(&v), None, false));
    assert!(t.contains(&"V".to_string()) && t.contains(&"VM1".to_string()) && t.contains(&"0 V".to_string()), "{t:?}");
}

#[test]
fn pin_labels_follow_rotation() {
    let mut c = Component::new(Kind::And, 0.0, 0.0, 0);
    let t = texts(&draw_component(&c, &SymState::edit(&c), None, false));
    for l in ["A", "B", "Y"] { assert!(t.contains(&l.to_string())); }
    c.r = 1;
    assert!(texts(&draw_component(&c, &SymState::edit(&c), None, false)).contains(&"Y".to_string()));
}

#[test]
fn led_glow_and_state_dependent_symbols() {
    let mut c = Component::new(Kind::Led, 0.0, 0.0, 0); c.p = Some("green".into());
    let mut st = SymState::edit(&c);
    let base = draw_component(&c, &st, None, false).len();
    st.gl = 0.9;
    let lit = draw_component(&c, &st, None, false);
    assert_eq!(lit.len(), base + 1);
    assert!(matches!(lit[0], Prim::Fill { col: Col::Rgba(60, 220, 90, _), .. }));
    let sw = Component::new(Kind::Sw, 0.0, 0.0, 0);
    let (mut a, mut b) = (SymState::edit(&sw), SymState::edit(&sw));
    b.on = true; a.on = false;
    let ex = |s: &SymState| format!("{:?}", draw_component(&sw, s, None, true));
    assert_ne!(ex(&a), ex(&b));
}

#[test]
fn junction_dots() {
    let mut p = Project::empty();
    let w = |a: f64, b: f64, c: f64, d: f64| Wire { x1: a, y1: b, x2: c, y2: d };
    // T junction: a vertical wire ends on the middle of a horizontal one
    p.w = vec![w(0., 0., 200., 0.), w(100., 0., 100., 100.)];
    assert_eq!(junctions(&p), vec![(100.0, 0.0)]);
    // plain corner (two ends) is not a junction
    p.w = vec![w(0., 0., 100., 0.), w(100., 0., 100., 100.)];
    assert!(junctions(&p).is_empty());
    // three wire ends at one point
    p.w = vec![w(0., 0., 100., 0.), w(100., 0., 100., 100.), w(100., 0., 200., 0.)];
    assert_eq!(junctions(&p), vec![(100.0, 0.0)]);
}

#[test]
fn scope_draws_traces_and_scale_text() {
    let c = Component::new(Kind::Os, 0.0, 0.0, 0);
    let mut st = SymState::edit(&c);
    st.scope = Some(((0..200).map(|i| (i as f32 / 20.0).sin() * 2.0).collect(), vec![0.0; 200]));
    let prims = draw_component(&c, &st, None, false);
    let t = texts(&prims);
    assert!(t.iter().any(|s| s.starts_with("CH1 ") && s.contains("Vpp")), "{t:?}");
    assert!(t.iter().any(|s| s.ends_with("V/div auto")));
    assert!(t.contains(&"5 ms/div".to_string()));
}

#[test]
fn demo_svg_exports() {
    let p = Project::from_json(&std::fs::read_to_string("assets/demo.json").unwrap()).unwrap();
    let svg = circuit_core::svg::project_svg(&p, None);
    assert!(svg.starts_with("<svg") && svg.trim_end().ends_with("</svg>") && svg.contains("<polyline"));
}
