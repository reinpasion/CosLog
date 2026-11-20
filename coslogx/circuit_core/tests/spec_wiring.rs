//! The spec-driven diagnostics must behave exactly like the hand-written logic they replaced.
//! The old implementations are kept here, frozen, as oracles.
use circuit_core::diagnostics::{check_part, Level, PartView};
use circuit_core::{kind::Kind, netlist::make_part, project::Component, spec};
use circuit_core::units::fm;

fn lim(a: f64, w: f64, e: f64, m: &str, u: &str) -> Option<(Level, String)> {
    if a > e { Some((Level::Error, format!("{m}{} exceeds {}", fm(a, u), fm(e, u)))) }
    else if a > w { Some((Level::Warn, format!("{m}{} near limit {}", fm(a, u), fm(e, u)))) } else { None }
}
fn legacy_check_part(c: &circuit_core::netlist::Part) -> Option<(Level, String)> {
    use Kind::*;
    let (i, a, vm, p) = (c.pi, c.ia, c.pv, c.pw);
    match c.kind {
        R => { let rt = if c.qn > 0.0 { c.qn } else { 0.25 }; lim(p, rt * 0.75, rt, "overheating ", "W") }
        Lamp => { let rt = if c.pn > 0.0 { c.pn } else { 0.5 }; lim(p, rt * 1.2, rt * 1.6, "overpowered ", "W") }
        C => { let rt = if c.sn > 0.0 { c.sn } else { 50.0 }; lim(vm, rt * 0.8, rt, "overvoltage ", "V") }
        L => lim(i, 3.0, 5.0, "overcurrent ", "A"),
        Tr => lim(a, 2.0, 4.0, "winding overcurrent ", "A").or_else(|| lim(i, 20.0, 40.0, "winding surge ", "A")),
        D => lim(a, 0.7, 1.0, "overcurrent ", "A").or_else(|| lim(i, 10.0, 30.0, "surge ", "A")).or_else(|| lim(vm, 80.0, 100.0, "reverse breakdown ", "V")),
        Sd => lim(a, 2.0, 3.0, "overcurrent ", "A").or_else(|| lim(i, 20.0, 40.0, "surge ", "A")).or_else(|| lim(vm, 32.0, 40.0, "reverse breakdown ", "V")),
        Led => lim(a, 0.025, 0.05, "overcurrent ", "A").or_else(|| lim(i, 0.15, 0.3, "surge ", "A")).or_else(|| lim(vm, 4.0, 6.0, "reverse voltage ", "V")),
        Zd => lim(p, 0.4, 0.5, "overpowered ", "W"),
        Npn | Pnp => lim(vm, 32.0, 40.0, "Vce ", "V").or_else(|| lim(a, 0.15, 0.4, "collector current ", "A")).or_else(|| lim(i, 0.6, 1.5, "surge ", "A")).or_else(|| lim(p, 0.4, 0.6, "dissipating ", "W")),
        Nm | Pm => lim(vm, 48.0, 60.0, "Vds ", "V").or_else(|| lim(a, 1.5, 3.0, "drain current ", "A")).or_else(|| lim(i, 6.0, 12.0, "surge ", "A")).or_else(|| lim(p, 0.8, 1.2, "dissipating ", "W")),
        _ if c.row.is_some() => if a > 15.0 { Some((Level::Error, format!("short circuit: supplying {}", fm(a, "A")))) } else { None },
        Am => lim(a, 3.0, 10.0, "overload ", "A"),
        Fu if c.bl => Some((Level::Warn, "blown (open)".into())),
        _ => None,
    }
}

/// Legacy DC-path unions as (pin, pin) pairs, pin index usize::MAX = ground.
fn legacy_dc(t: Kind, on: bool) -> Vec<(usize, usize)> {
    use Kind::*;
    let g = usize::MAX;
    if matches!(t, R | L | Dc | Ac | Sg | D | Sd | Led | Zd | Lamp | Mo | Am | Vm | Fu | Ldr | Ntc | Bz | Sol) || (matches!(t, Sw | Bt) && on) || (t == Nb && !on) { return vec![(0, 1)]; }
    match t {
        Pot => vec![(0, 2), (2, 1)],
        Sp => vec![(0, if on { 2 } else { 1 })],
        Tr => vec![(0, 1), (2, 3)],
        Rl => { let mut v = vec![(0, 1)]; if on { v.push((2, 3)); } v }
        Npn | Pnp => vec![(1, 2), (0, 2)],
        Nm | Pm => vec![(1, 2)],
        Op | And | Or | Nand | Nor | Xor => vec![(2, g)],
        Not => vec![(1, g)],
        Dff | Tf => vec![(2, g), (3, g)],
        Jk | Srf => vec![(3, g), (4, g)],
        Reg => vec![(2, 1)],
        Tm => vec![(0, 1), (7, 0), (6, 0), (5, g), (4, 1)],
        Br => vec![(0, 2), (1, 2), (3, 0)],
        Ctr => (2..6).map(|k| (k, g)).collect(),
        _ => vec![],
    }
}
fn legacy_short(t: Kind, on: bool, bl: bool, val: f64) -> Vec<(usize, usize)> {
    use Kind::*;
    if (matches!(t, Sw | Bt) && on) || (t == Nb && !on) || t == Am || (t == Fu && !bl) || (t == R && val < 0.5) { vec![(0, 1)] }
    else if t == Sp { vec![(0, if on { 2 } else { 1 })] } else { vec![] }
}

fn canon(v: Vec<(usize, usize)>) -> Vec<(usize, usize)> { let mut v: Vec<_> = v.into_iter().map(|(a, b)| (a.min(b), a.max(b))).collect(); v.sort(); v }

#[test]
fn spec_paths_match_legacy_unions() {
    for &k in Kind::ALL {
        let n = spec::part(k.code()).unwrap().pins.len();
        for on in [0u8, 1] { for bl in [false, true] { for val in [0.1, 0.5, 2.0] {
            let mut c = Component::new(k, 0.0, 0.0, 0); c.on = on;
            let mut p = make_part(&c, 0, (1..=n).collect(), 0x5EED);
            p.bl = bl; p.val = val; p.on = on;
            let s = spec::part(k.code()).unwrap();
            let view = PartView(&p);
            let conv = |v: Vec<(i8, i8)>| canon(v.into_iter().map(|(a, b)| (if a < 0 { usize::MAX } else { a as usize }, if b < 0 { usize::MAX } else { b as usize })).collect());
            assert_eq!(conv(spec::joins(s.dc_path, &view)), canon(legacy_dc(k, on != 0)), "dc {:?} on={on}", k);
            assert_eq!(conv(spec::joins(s.short_path, &view)), canon(legacy_short(k, on != 0, bl, val)), "short {:?} on={on} bl={bl} val={val}", k);
        } } }
    }
}

#[test]
fn spec_check_part_matches_legacy() {
    let vals = [0.0, 0.001, 0.02, 0.03, 0.2, 0.45, 0.7, 0.9, 1.1, 1.3, 2.5, 3.5, 5.5, 9.0, 11.0, 14.0, 16.0, 25.0, 33.0, 45.0, 55.0, 70.0, 90.0, 120.0];
    let ratings = [(0.0, 0.0, 0.0), (0.5, 1.0, 12.0), (0.125, 0.1, 100.0)];
    let mut n = 0;
    for &k in Kind::ALL {
        let np = spec::part(k.code()).unwrap().pins.len();
        let base = make_part(&Component::new(k, 0.0, 0.0, 0), 0, vec![0; np], 0x5EED);
        for &(q, p, sv) in &ratings { for &a in &vals { for &b in &vals { for (bl, row) in [(false, None), (true, None), (false, Some(0usize)), (true, Some(0))] {
            let mut c = base.clone();
            c.qn = q; c.pn = p; c.sn = sv; c.pi = a; c.ia = b; c.pv = a; c.pw = b; c.bl = bl; c.row = row;
            // a part that is not a voltage source never has a row
            if row.is_some() && !spec::part(k.code()).unwrap().row_source { continue; }
            assert_eq!(check_part(&c), legacy_check_part(&c), "{:?} q={q} p={p} s={sv} a={a} b={b} bl={bl} row={row:?}", k);
            n += 1;
        } } } }
    }
    assert!(n > 100_000);
}
