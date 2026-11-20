//! SVG export of a project (uses the same primitives as the canvas).
use crate::project::Project;
use crate::symbols::*;

fn col(c: Col, ink: &str) -> String {
    match c { Col::Ink => ink.into(), Col::Wire => "#0a7d3b".into(), Col::Gray => "#c9cfda".into(),
        Col::Rgba(r, g, b, a) => format!("rgba({r},{g},{b},{:.3})", a as f32 / 255.0) }
}
fn pts(p: &[Pt]) -> String { p.iter().map(|q| format!("{:.2},{:.2}", q.0, q.1)).collect::<Vec<_>>().join(" ") }

pub fn prims_svg(prims: &[Prim], out: &mut String, ink: &str) {
    for p in prims {
        match p {
            Prim::Stroke { pts: q, col: c, w } => out.push_str(&format!("<polyline points=\"{}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{}\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/>\n", pts(q), col(*c, ink), w)),
            Prim::Fill { pts: q, col: c } => out.push_str(&format!("<polygon points=\"{}\" fill=\"{}\"/>\n", pts(q), col(*c, ink))),
            Prim::Text { p, s, size, bold, align, mid, col: c } => out.push_str(&format!(
                "<text x=\"{:.2}\" y=\"{:.2}\" font-size=\"{}\" font-family=\"sans-serif\" font-weight=\"{}\" text-anchor=\"{}\" dominant-baseline=\"{}\" fill=\"{}\">{}</text>\n",
                p.0, p.1, size, if *bold { "bold" } else { "normal" },
                match align { Align::Left => "start", Align::Center => "middle", Align::Right => "end" },
                if *mid { "central" } else { "alphabetic" }, col(*c, ink), s.replace('&', "&amp;").replace('<', "&lt;"))),
        }
    }
}

/// Whole schematic as an SVG string (edit-mode symbol state).
pub fn project_svg(p: &Project, bounds: Option<(f64, f64, f64, f64)>) -> String {
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for c in &p.c { let b = c.bbox(); x0 = x0.min(b[0]); y0 = y0.min(b[1]); x1 = x1.max(b[2]); y1 = y1.max(b[3]); }
    for w in &p.w { x0 = x0.min(w.x1.min(w.x2)); x1 = x1.max(w.x1.max(w.x2)); y0 = y0.min(w.y1.min(w.y2)); y1 = y1.max(w.y1.max(w.y2)); }
    if let Some(b) = bounds { (x0, y0, x1, y1) = b; }
    let m = 30.0;
    let mut s = format!("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{} {} {} {}\" width=\"{}\" height=\"{}\"><rect x=\"{}\" y=\"{}\" width=\"100%\" height=\"100%\" fill=\"#ffffff\"/>\n",
        x0 - m, y0 - m, x1 - x0 + 2. * m, y1 - y0 + 2. * m, (x1 - x0 + 2. * m) * 0.8, (y1 - y0 + 2. * m) * 0.8, x0 - m, y0 - m);
    let wc = "#0a7d3b";
    for w in &p.w { s.push_str(&format!("<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"{wc}\" stroke-width=\"2\" stroke-linecap=\"round\"/>\n", w.x1, w.y1, w.x2, w.y2)); }
    for j in junctions(p) { s.push_str(&format!("<circle cx=\"{}\" cy=\"{}\" r=\"4.5\" fill=\"{wc}\"/>\n", j.0, j.1)); }
    for c in &p.c { prims_svg(&draw_component(c, &SymState::edit(c), None, false), &mut s, "#111111"); }
    s.push_str("</svg>\n");
    s
}
