//! Zoomable schematic canvas: grid, wires, junction dots, component symbols, hover ring, fault-free view.
//! Drawing is driven entirely by `circuit_core::symbols` primitives.
use circuit_core::engine::Sim;
use circuit_core::diagnostics::{Fault, Faults, Level};
use circuit_core::editor::{snap, Drag, Editor, Tool};
use circuit_core::kind::Kind;
use circuit_core::project::{Component, View};
use circuit_core::symbols::*;
use eframe::egui::{self, epaint::PathShape, Align2, Color32, FontId, Painter, Pos2, Rect, Shape, Stroke};

pub const GRID: f64 = 20.0;

#[derive(Clone, Copy)]
pub struct Theme { pub bg: Color32, pub wire: Color32, pub ink: Color32, pub accent: Color32, pub grid: Color32 }
impl Theme {
    pub fn from_visuals(dark: bool) -> Theme {
        let c = |r, g, b| Color32::from_rgb(r, g, b);
        if dark { Theme { bg: c(0x14, 0x17, 0x1c), wire: c(0x4a, 0xde, 0x80), ink: c(0xe6, 0xe9, 0xef), accent: c(0x5b, 0x9c, 0xff), grid: c(0x2d, 0x34, 0x40) } }
        else { Theme { bg: c(0xf4, 0xf5, 0xf7), wire: c(0x0a, 0x7d, 0x3b), ink: c(0x11, 0x11, 0x11), accent: c(0x25, 0x63, 0xeb), grid: c(0xc9, 0xcf, 0xda) } }
    }
}

/// World <-> screen mapping for one frame.
#[derive(Clone, Copy)]
pub struct Xf { pub origin: Pos2, pub view: View }
impl Xf {
    pub fn to_screen(&self, p: Pt) -> Pos2 { Pos2::new(self.origin.x + (self.view.x + p.0 * self.view.z) as f32, self.origin.y + (self.view.y + p.1 * self.view.z) as f32) }
    pub fn to_world(&self, s: Pos2) -> Pt { (((s.x - self.origin.x) as f64 - self.view.x) / self.view.z, ((s.y - self.origin.y) as f64 - self.view.y) / self.view.z) }
}

/// Zoom about the screen point `s` (relative to the canvas origin), limits 0.4x..3x like the JS.
pub fn zoom_at(view: &mut View, s: (f64, f64), factor: f64) {
    let z0 = view.z; let z1 = (z0 * factor).clamp(0.4, 3.0);
    view.x = s.0 - (s.0 - view.x) * z1 / z0; view.y = s.1 - (s.1 - view.y) * z1 / z0; view.z = z1;
}

fn color(c: Col, th: &Theme, selected: bool, alpha: f32) -> Color32 {
    let base = match c { Col::Ink => if selected { th.accent } else { th.ink }, Col::Wire => th.wire, Col::Gray => th.grid, Col::Rgba(r, g, b, a) => Color32::from_rgba_unmultiplied(r, g, b, a) };
    if alpha >= 1.0 { base } else { base.gamma_multiply(alpha) }
}

pub fn draw_prims(painter: &Painter, xf: &Xf, th: &Theme, prims: &[Prim], selected: bool, alpha: f32) {
    let z = xf.view.z as f32;
    for p in prims {
        match p {
            Prim::Stroke { pts, col, w } => {
                let v: Vec<Pos2> = pts.iter().map(|&q| xf.to_screen(q)).collect();
                painter.add(Shape::line(v, Stroke::new(w * z, color(*col, th, selected, alpha))));
            }
            Prim::Fill { pts, col } => {
                let v: Vec<Pos2> = pts.iter().map(|&q| xf.to_screen(q)).collect();
                painter.add(Shape::Path(PathShape { points: v, closed: true, fill: color(*col, th, selected, alpha), stroke: Default::default() }));
            }
            Prim::Text { p, s, size, bold, align, mid, col } => {
                let a = match (align, mid) {
                    (Align::Left, true) => Align2::LEFT_CENTER, (Align::Center, true) => Align2::CENTER_CENTER, (Align::Right, true) => Align2::RIGHT_CENTER,
                    (Align::Left, false) => Align2::LEFT_BOTTOM, (Align::Center, false) => Align2::CENTER_BOTTOM, (Align::Right, false) => Align2::RIGHT_BOTTOM,
                };
                // egui ships no bold face by default; bold labels use a slightly larger size instead.
                let fid = FontId::proportional(size * z * if *bold { 1.06 } else { 1.0 });
                painter.text(xf.to_screen(*p), a, s, fid, color(*col, th, selected, alpha));
            }
        }
    }
}

fn dashed(painter: &Painter, pts: Vec<Pos2>, stroke: Stroke, on: f32, off: f32) {
    painter.extend(Shape::dashed_line(&pts, stroke, on, off));
}

fn fault_color(l: Level) -> Color32 { if l == Level::Error { Color32::from_rgb(0xff, 0x3b, 0x30) } else { Color32::from_rgb(0xff, 0x9f, 0x0a) } }

/// Pulsing red (error) / orange (warning) highlights on wires and parts. While the simulation is not
/// running (or is paused) markers stay visible; while running they fade after their 3 s hold.
pub fn draw_faults(painter: &Painter, xf: &Xf, ed: &Editor, faults: &Faults, live: bool, now: f64) {
    let z = xf.view.z as f32;
    let pl = if live { (0.5 + 0.5 * (now / 170.0).sin()) as f32 } else { 1.0 };
    let vis = |f: &Fault| f.until > now || !live;
    for (&wi, f) in &faults.wires {
        let Some(w) = ed.proj.w.get(wi) else { continue };
        if !vis(f) { continue; }
        let col = fault_color(f.level).gamma_multiply(0.25 + 0.45 * pl);
        painter.add(Shape::line_segment([xf.to_screen((w.x1, w.y1)), xf.to_screen((w.x2, w.y2))], Stroke::new(6.0 * z, col)));
    }
    for (&ci, f) in &faults.comps {
        let Some(c) = ed.proj.c.get(ci) else { continue };
        if !vis(f) { continue; }
        let b = c.bbox(); let cl = fault_color(f.level);
        let r = Rect::from_two_pos(xf.to_screen((b[0] + 4.0, b[1] + 4.0)), xf.to_screen((b[2] - 4.0, b[3] - 4.0)));
        let stroke = Stroke::new(2.0 * z, cl.gamma_multiply(0.4 + 0.6 * pl));
        let pts = vec![r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
        dashed(painter, pts, stroke, 6.0 * z, 4.0 * z);
        if let Some(pin) = f.pin { if let Some(p) = c.pins().get(pin) { painter.circle_stroke(xf.to_screen(*p), 8.0 * z, stroke); } }
        let txt: String = f.msg.chars().take(44).collect();
        let s = format!("{} {}", if f.level == Level::Error { "✖" } else { "⚠" }, txt);
        let galley = painter.layout_no_wrap(s, FontId::proportional(10.0 * z), Color32::WHITE);
        let tl = xf.to_screen((b[2] + 2.0, b[1] - 1.0));
        painter.rect_filled(Rect::from_min_size(tl, egui::vec2(galley.size().x + 8.0 * z, 15.0 * z)), 0.0, cl);
        painter.galley(tl + egui::vec2(4.0 * z, 2.0 * z), galley, Color32::WHITE);
    }
}

pub fn paint(painter: &Painter, rect: Rect, view: &View, th: &Theme, ed: &Editor, sim: Option<&Sim>, faults: &Faults, live: bool, now: f64) {
    painter.rect_filled(rect, 0.0, th.bg);
    let xf = Xf { origin: rect.min, view: *view };
    let painter = &painter.with_clip_rect(rect);
    let proj = &ed.proj;
    let z = view.z as f32;
    // grid dots (every 2 cells when zoomed out)
    let st = if view.z < 0.7 { GRID * 2.0 } else { GRID };
    let (w0, w1) = (xf.to_world(rect.min), xf.to_world(rect.max));
    let mut gx = (w0.0 / st).floor() * st;
    while gx < w1.0 {
        let mut gy = (w0.1 / st).floor() * st;
        while gy < w1.1 { painter.circle_filled(xf.to_screen((gx, gy)), 0.75, th.grid); gy += st; }
        gx += st;
    }
    for (i, w) in proj.w.iter().enumerate() {
        let col = if ed.sw.get(i).copied().unwrap_or(false) { th.accent } else { th.wire };
        painter.add(Shape::line_segment([xf.to_screen((w.x1, w.y1)), xf.to_screen((w.x2, w.y2))], Stroke::new(2.0 * z, col)));
    }
    for j in junctions(proj) { painter.circle_filled(xf.to_screen(j), 4.5 * z, th.wire); }
    for (i, c) in proj.c.iter().enumerate() {
        let (st, rd) = match sim.and_then(|s| s.part(i)) {
            Some(p) => (SymState::run(c, p), Some(reading(c.t, p.av, p.sq, p.w, p.val))),
            None => (SymState::edit(c), None),
        };
        draw_prims(painter, &xf, th, &draw_component(c, &st, rd.as_deref(), false), ed.sc.get(i).copied().unwrap_or(false), 1.0);
    }
    draw_faults(painter, &xf, ed, faults, live, now);
    // overlays
    let (mx, my) = ed.mouse;
    let snapped = (snap(mx), snap(my));
    if ed.tool == Tool::Select && !ed.running && ed.drag.is_none() && ed.mouse_in {
        if let Some(h) = ed.pin_at(ed.mouse, view.z) { painter.circle_stroke(xf.to_screen(h), 7.0 * z, Stroke::new(1.5 * z, th.accent)); }
    }
    match (ed.tool, ed.wire_start) {
        (Tool::Wire, Some(a)) if ed.mouse_in => {
            let pts = vec![xf.to_screen(a), xf.to_screen((snapped.0, a.1)), xf.to_screen(snapped)];
            dashed(painter, pts, Stroke::new(2.0 * z, th.accent), 4.0 * z, 4.0 * z);
        }
        (Tool::Place(k), _) if ed.mouse_in => {
            let mut g = Component::new(k, snapped.0, snapped.1, ed.place_rot);
            g.m = if k == Kind::Os { 0 } else { ed.place_mir };
            draw_prims(painter, &xf, th, &draw_component(&g, &SymState::edit(&g), None, true), false, 0.5);
        }
        _ => {}
    }
    if let Some(Drag::Box { x0, y0, x1, y1 }) = &ed.drag {
        let win = x1 >= x0;
        let r = Rect::from_two_pos(xf.to_screen((*x0, *y0)), xf.to_screen((*x1, *y1)));
        let (fill, line) = if win { (Color32::from_rgba_unmultiplied(37, 99, 235, 38), Color32::from_rgb(37, 99, 235)) } else { (Color32::from_rgba_unmultiplied(16, 160, 70, 38), Color32::from_rgb(16, 160, 70)) };
        painter.rect_filled(r, 0.0, fill);
        if win { painter.rect_stroke(r, 0.0, Stroke::new(1.2, line), egui::StrokeKind::Middle); }
        else { let c = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()]; dashed(painter, c.to_vec(), Stroke::new(1.2, line), 6.0, 4.0); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use circuit_core::project::Project;
    use circuit_core::editor::Editor;

    /// Run one real egui frame through `paint` and check the tessellated output.
    fn frame_shapes(proj: &Project, view: View, sim: Option<&Sim>) -> usize {
        let ctx = egui::Context::default();
        let mut raw = egui::RawInput::default();
        raw.screen_rect = Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1200.0, 800.0)));
        let mut n = 0;
        let out = ctx.run(raw, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let (resp, painter) = ui.allocate_painter(ui.available_size(), egui::Sense::click_and_drag());
                let mut ed = Editor::new(proj.clone());
                ed.mouse = (100.0, 100.0); ed.mouse_in = true;
                ed.set_tool(Tool::Place(Kind::Npn)); ed.select_all();
                let mut faults = Faults::default();
                faults.comps.insert(0, Fault { level: Level::Error, msg: "overheating 8.65 W exceeds 250 mW".into(), pin: None, until: 5000.0 });
                faults.comps.insert(1, Fault { level: Level::Warn, msg: "pin 2 unconnected".into(), pin: Some(1), until: 5000.0 });
                faults.wires.insert(0, Fault { level: Level::Error, msg: String::new(), pin: None, until: 5000.0 });
                paint(&painter, resp.rect, &view, &Theme::from_visuals(true), &ed, sim, &faults, sim.is_some(), 1000.0);
            });
        });
        for s in &out.shapes { let _ = s; n += 1; }
        let tris = ctx.tessellate(out.shapes, 1.0);
        assert!(!tris.is_empty());
        n.max(tris.len())
    }

    #[test]
    fn demo_and_gallery_paint() {
        let demo = Project::from_json(include_str!("../../circuit_core/assets/demo.json")).unwrap();
        assert!(frame_shapes(&demo, demo.view.unwrap(), None) > 10);
        let g = Project::gallery();
        assert!(frame_shapes(&g, View { x: 20.0, y: 20.0, z: 0.45 }, None) > 10);
    }

    #[test]
    fn running_state_paints() {
        let demo = Project::from_json(include_str!("../../circuit_core/assets/demo.json")).unwrap();
        let mut sim = Sim::new(&demo, 1.0);
        sim.advance_to(0.05, None);
        assert!(frame_shapes(&demo, demo.view.unwrap(), Some(&sim)) > 10);
    }

    #[test]
    fn zoom_keeps_point_under_cursor() {
        let mut v = View { x: 60.0, y: 40.0, z: 1.0 };
        let s = (300.0, 200.0);
        let before = ((s.0 - v.x) / v.z, (s.1 - v.y) / v.z);
        zoom_at(&mut v, s, 1.1);
        let after = ((s.0 - v.x) / v.z, (s.1 - v.y) / v.z);
        assert!((before.0 - after.0).abs() < 1e-9 && (before.1 - after.1).abs() < 1e-9);
        for _ in 0..100 { zoom_at(&mut v, s, 1.1); }
        assert_eq!(v.z, 3.0);
        for _ in 0..200 { zoom_at(&mut v, s, 1.0 / 1.1); }
        assert_eq!(v.z, 0.4);
    }
}
