//! The generated spec tables (from spec/parts.json) must agree with the hand-written Rust tables.
//! Anything that fails here is a real divergence between the spec and CosLog X.
use circuit_core::{diagnostics::check_part, kind::Kind, netlist::make_part, params, project::Component, spec::*};

#[test]
fn every_kind_has_exactly_one_spec_and_vice_versa() {
    assert_eq!(PARTS.len(), Kind::ALL.len());
    for &k in Kind::ALL { assert!(part(k.code()).is_some(), "no spec for {:?}", k); }
    for p in PARTS { assert!(Kind::ALL.iter().any(|k| k.code() == p.code), "spec part {} has no Kind", p.code); }
}

#[test]
fn names_prefixes_pins_flags_and_bbox_match() {
    for &k in Kind::ALL {
        let s = part(k.code()).unwrap();
        assert_eq!(s.name, k.name(), "{:?} name", k);
        assert_eq!(s.prefix, k.prefix(), "{:?} prefix", k);
        assert_eq!(s.nonlinear, k.nonlinear(), "{:?} nonlinear", k);
        let pins: Vec<(f64, f64)> = s.pins.iter().map(|p| (p.x as f64, p.y as f64)).collect();
        assert_eq!(pins, k.pins().to_vec(), "{:?} pins", k);
        if let Some(b) = s.bbox { assert_eq!(b, k.bbox(), "{:?} bbox override", k); }
        assert_eq!(s.tolerance, matches!(k, Kind::R | Kind::L | Kind::C), "{:?} tolerance scope", k);
        assert_eq!(s.row_source, matches!(k, Kind::Dc | Kind::Ac | Kind::Sg), "{:?} row source", k);
        // optional pins (outputs / controls allowed to float) match diagnostics::optional_pin
        let opt: Vec<usize> = s.pins.iter().enumerate().filter(|(_, p)| p.optional).map(|(i, _)| i).collect();
        let want: Vec<usize> = (0..s.pins.len()).filter(|&i| (k == Kind::Dff && i > 1) || (k == Kind::Ctr && i > 0) || (k == Kind::Tm && i > 5) || (k == Kind::Tf && i > 1) || (matches!(k, Kind::Jk | Kind::Srf) && i > 2)).collect();
        assert_eq!(opt, want, "{:?} optional pins", k);
    }
}

#[test]
fn bbox_default_rule_is_pin_extents_plus_14() {
    for &k in Kind::ALL { let s = part(k.code()).unwrap(); if s.bbox.is_none() {
        let xs: Vec<f64> = s.pins.iter().map(|p| p.x as f64).collect(); let ys: Vec<f64> = s.pins.iter().map(|p| p.y as f64).collect();
        let m = |v: &Vec<f64>, f: fn(f64, f64) -> f64, i: f64| v.iter().cloned().fold(i, f);
        assert_eq!(k.bbox(), [m(&xs, f64::min, f64::MAX) - 14., m(&ys, f64::min, f64::MAX) - 14., m(&xs, f64::max, f64::MIN) + 14., m(&ys, f64::max, f64::MIN) + 14.], "{:?}", k);
    } }
}

#[test]
fn params_match_including_the_new_q_and_s_menu_entries() {
    for &k in Kind::ALL {
        let s = part(k.code()).unwrap(); let r = params::params(k);
        assert_eq!(s.params.len(), r.len(), "{:?} param count", k);
        for (a, b) in s.params.iter().zip(&r) {
            assert_eq!((a.key, a.name, a.unit), (b.key, b.label, b.unit), "{:?}", k);
            // `v`/`f` defaults in the spec are the placement values from the old `T` table; Rust applies those elsewhere.
            if b.def.is_some() { assert_eq!(a.default, b.def, "{:?} {} default", k, a.key); }
            else if !matches!(a.key, "v" | "f") { assert_eq!(a.default, None, "{:?} {}", k, a.key); }
            assert_eq!(a.options, b.opts.unwrap_or(&[]), "{:?} {} options", k, a.key);
        }
        assert_eq!(s.live.map(|l| (l.min, l.max)), params::live_slider(k), "{:?} live", k);
    }
}

#[test]
fn palette_groups_and_keywords_match() {
    assert_eq!(GROUPS.len(), params::GROUPS.len());
    for ((sn, sp), (rn, rk)) in GROUPS.iter().zip(params::GROUPS) {
        assert_eq!(sn, rn);
        assert_eq!(sp.to_vec(), rk.iter().map(|k| k.code()).collect::<Vec<_>>(), "group {}", sn);
    }
    for &k in Kind::ALL { assert_eq!(part(k.code()).unwrap().keywords, params::keywords(k), "{:?} keywords", k); }
}

/// The spec `limits` list must reproduce `check_part` for every part over a sweep of operating points.
#[test]
fn spec_limits_reproduce_check_part() {
    let vals = [0.0, 0.001, 0.02, 0.03, 0.2, 0.45, 0.7, 0.9, 1.1, 1.3, 2.5, 3.5, 5.5, 9.0, 11.0, 14.0, 16.0, 25.0, 33.0, 45.0, 55.0, 70.0, 90.0, 120.0];
    let ratings = [(0.0, 0.0, 0.0), (0.5, 1.0, 12.0), (0.125, 0.1, 100.0)];
    let mut n = 0;
    for &k in Kind::ALL {
        let s = part(k.code()).unwrap();
        let base = make_part(&Component::new(k, 0.0, 0.0, 0), 0, vec![0; s.pins.len()], 0x5EED);
        for &(q, p, sv) in &ratings { for &a in &vals { for &b in &vals {
            let mut c = base.clone();
            c.qn = q; c.pn = p; c.sn = sv; c.pi = a; c.ia = b; c.pv = a; c.pw = b; c.bl = false; c.row = None;
            // check_part has source/ammeter/fuse fallbacks that are not part-limit lists; compare only part limits
            let want = match k { Kind::Am | Kind::Fu | Kind::Dc | Kind::Ac | Kind::Sg => continue, _ => check_part(&c) };
            let m = Measured { peak_i: c.pi, avg_i: c.ia, peak_v: c.pv, power: c.pw };
            let got = eval_limits(s, &m, &Ratings { q, p, s: sv });
            let want_l = want.as_ref().map(|(l, msg)| (format!("{:?}", l), msg.clone()));
            let got_l = got.map(|(e, msg, v, lim, u)| (if e { "Error" } else { "Warn" }.to_string(), format!("{msg}{} {} {}", circuit_core::units::fm(v, u), if e { "exceeds" } else { "near limit" }, circuit_core::units::fm(lim, u))));
            assert_eq!(got_l, want_l, "{:?} i={a} a={b} q={q} p={p} s={sv}", k);
            n += 1;
        } } }
    }
    assert!(n > 10_000, "sweep ran {n} cases");
}

#[test]
fn spec_hash_is_16_hex_digits() {
    assert_eq!(SPEC_HASH.len(), 16); assert!(SPEC_HASH.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn spec_hash_matches_fnv_of_file() {
    let raw = include_bytes!("../../../spec/parts.json");
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in raw.iter() {
        if b == b'\r' { continue; }
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    assert_eq!(format!("{:016x}", h), SPEC_HASH);
}
