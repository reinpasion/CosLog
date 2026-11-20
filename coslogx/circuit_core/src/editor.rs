//! Interactive editor logic (selection, placement, wiring, group transforms, clipboard, undo/redo).
//! UI-agnostic: a frontend feeds it world-space pointer events and key commands.
use crate::kind::Kind;
use crate::project::{Component, Project, Wire};
use crate::symbols::{label, value_text};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub type Pt = (f64, f64);
pub const G: f64 = 20.0;
/// JS `Math.round(v/G)*G` (rounds .5 toward +inf).
pub fn snap(v: f64) -> f64 { ((v / G + 0.5).floor()) * G }
fn key(p: Pt) -> (u64, u64) { ((p.0 + 0.0).to_bits(), (p.1 + 0.0).to_bits()) }

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Tool { Select, Wire, Place(Kind) }
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hit { Comp(usize), Wire(usize) }

#[derive(Clone, Debug)]
pub enum Drag {
    Box { x0: f64, y0: f64, x1: f64, y1: f64 },
    Move { lx: f64, ly: f64, ends: Vec<(usize, u8)>, moved: bool },
    Label { c: usize, name: bool, sx: f64, sy: f64, b0: [f64; 2] },
    Pot { c: usize },
    Btn { c: usize },
}
/// A live change to forward to a running simulation.
#[derive(Clone, Debug, PartialEq)]
pub struct Poke { pub comp: usize, pub on: Option<u8>, pub pp: Option<f64>, pub f: Option<String> }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Clip { pub c: Vec<Component>, pub w: Vec<Wire> }

pub struct Editor {
    pub proj: Project,
    pub sc: Vec<bool>, pub sw: Vec<bool>,
    pub tool: Tool, pub place_rot: u8, pub place_mir: u8,
    pub wire_start: Option<Pt>, pub auto_wire: bool,
    pub drag: Option<Drag>, pub mouse: Pt, pub mouse_in: bool,
    pub running: bool,
    pub recent: Vec<Kind>,
    pub pokes: Vec<Poke>,
    pub flash: String,
    pub rev: u64,
    clip: Option<Clip>, paste_n: u32,
    hist: Vec<String>, fut: Vec<String>, saved: String, edit_open: bool,
}

fn rot(mut x: f64, mut y: f64, r: u8) -> Pt { for _ in 0..r & 3 { let t = x; x = -y; y = t; } (x, y) }

impl Editor {
    pub fn new(proj: Project) -> Editor {
        let mut e = Editor { sc: vec![], sw: vec![], proj, tool: Tool::Select, place_rot: 0, place_mir: 0, wire_start: None, auto_wire: false, drag: None,
            mouse: (0., 0.), mouse_in: false, running: false, recent: vec![], pokes: vec![], flash: String::new(), rev: 0, clip: None, paste_n: 0,
            hist: vec![], fut: vec![], saved: String::new(), edit_open: false };
        e.resize(); e.saved = e.snap(); e
    }

    // ------------------------------------------------------------ bookkeeping
    fn resize(&mut self) { self.sc.resize(self.proj.c.len(), false); self.sw.resize(self.proj.w.len(), false); }
    fn snap(&self) -> String { serde_json::to_string(&(&self.proj.c, &self.proj.w)).unwrap() }
    fn restore(&mut self, s: &str) {
        let (c, w): (Vec<Component>, Vec<Wire>) = serde_json::from_str(s).unwrap();
        self.proj.c = c; self.proj.w = w;
        self.sc = vec![false; self.proj.c.len()]; self.sw = vec![false; self.proj.w.len()]; self.mark();
    }
    pub fn mark(&mut self) { self.rev += 1; }
    pub fn push(&mut self) { let s = self.snap(); self.hist.push(s); if self.hist.len() > 80 { self.hist.remove(0); } self.fut.clear(); }
    pub fn undo(&mut self) { if !self.running { if let Some(h) = self.hist.pop() { let s = self.snap(); self.fut.push(s); self.restore(&h); } } }
    pub fn redo(&mut self) { if !self.running { if let Some(f) = self.fut.pop() { let s = self.snap(); self.hist.push(s); self.restore(&f); } } }
    pub fn can_undo(&self) -> bool { !self.hist.is_empty() }
    pub fn can_redo(&self) -> bool { !self.fut.is_empty() }
    pub fn is_dirty(&self) -> bool { self.snap() != self.saved }
    pub fn mark_saved(&mut self) { self.saved = self.snap(); }
    /// Replace the whole project (loading a file/example). `undoable` keeps history like the JS `loadProj`.
    pub fn load(&mut self, p: Project, undoable: bool) {
        if undoable { self.push(); } else { self.hist.clear(); self.fut.clear(); }
        self.proj = p; self.resize(); self.sc.iter_mut().for_each(|x| *x = false); self.sw.iter_mut().for_each(|x| *x = false);
        self.set_tool(Tool::Select); self.mark();
    }
    pub fn clear_selection(&mut self) { self.sc.iter_mut().for_each(|x| *x = false); self.sw.iter_mut().for_each(|x| *x = false); self.edit_open = false; }
    pub fn select_all(&mut self) { self.sc.iter_mut().for_each(|x| *x = true); self.sw.iter_mut().for_each(|x| *x = true); self.edit_open = false; }
    pub fn n_selected(&self) -> usize { self.sc.iter().filter(|x| **x).count() + self.sw.iter().filter(|x| **x).count() }
    /// The single selected component (and nothing else), for the property bar.
    pub fn single(&self) -> Option<usize> {
        let s: Vec<usize> = self.sc.iter().enumerate().filter(|(_, x)| **x).map(|(i, _)| i).collect();
        if s.len() == 1 && !self.sw.iter().any(|x| *x) { Some(s[0]) } else { None }
    }
    pub fn set_tool(&mut self, t: Tool) {
        if self.running && t != Tool::Select { return; }
        self.tool = t; self.auto_wire = false; self.wire_start = None; self.place_rot = 0; self.place_mir = 0;
    }
    pub fn toggle_wire(&mut self) { let t = if self.tool == Tool::Wire { Tool::Select } else { Tool::Wire }; self.set_tool(t); }

    /// Next free designator: prefix + smallest unused number.
    pub fn next_name(&self, k: Kind) -> Option<String> {
        let p = k.prefix()?;
        let used: HashSet<&str> = self.proj.c.iter().filter_map(|c| c.n.as_deref()).collect();
        (1..).map(|i| format!("{p}{i}")).find(|n| !used.contains(n.as_str()))
    }
    fn new_part(&self, k: Kind, x: f64, y: f64, r: u8, m: u8) -> Component {
        let mut c = Component::new(k, x, y, r);
        c.m = if k == Kind::Os { 0 } else { m };
        if c.n.is_none() { c.n = self.next_name(k); }
        c
    }

    // ------------------------------------------------------------ queries
    pub fn pin_at(&self, w: Pt, z: f64) -> Option<Pt> {
        let r = 8.0 / z;
        self.proj.c.iter().flat_map(|c| c.pins()).find(|q| ((w.0 - q.0).powi(2) + (w.1 - q.1).powi(2)).sqrt() < r)
    }
    pub fn is_conn(&self, q: Pt) -> bool {
        self.proj.c.iter().any(|c| c.pins().iter().any(|p| *p == q)) || self.proj.w.iter().any(|w| (w.x1, w.y1) == q || (w.x2, w.y2) == q || w.on_seg(q))
    }
    fn dseg(w: &Wire, x: f64, y: f64) -> f64 {
        let (dx, dy) = (w.x2 - w.x1, w.y2 - w.y1);
        let l = dx * dx + dy * dy; let l = if l == 0.0 { 1.0 } else { l };
        let t = (((x - w.x1) * dx + (y - w.y1) * dy) / l).clamp(0.0, 1.0);
        (x - w.x1 - t * dx).hypot(y - w.y1 - t * dy)
    }
    pub fn hit(&self, w: Pt, z: f64) -> Option<Hit> {
        for (i, c) in self.proj.c.iter().enumerate().rev() { let b = c.bbox(); if w.0 >= b[0] && w.0 <= b[2] && w.1 >= b[1] && w.1 <= b[3] { return Some(Hit::Comp(i)); } }
        self.proj.w.iter().position(|x| Self::dseg(x, w.0, w.1) < 6.0 / z).map(Hit::Wire)
    }
    /// Approximate hit rectangles of the name / value labels (frontend text metrics differ slightly).
    pub fn label_rects(c: &Component) -> [Option<[f64; 4]>; 2] {
        let b = c.bbox();
        let no = c.no.unwrap_or([0., 0.]); let vo = c.vo.unwrap_or([0., 0.]);
        let rect = |s: &str, x: f64, y: f64, bold: bool| if s.is_empty() { None } else {
            let w = s.chars().count() as f64 * if bold { 6.6 } else { 6.0 }; Some([x - w / 2. - 2., y - 11., x + w / 2. + 2., y + 3.]) };
        [rect(c.n.as_deref().unwrap_or(""), c.x + no[0], b[1] - 4. + no[1], true), rect(&value_text(c, None), c.x + vo[0], b[3] + 10. + vo[1], false)]
    }
    fn label_hit(&self, w: Pt) -> Option<(usize, bool)> {
        for (i, c) in self.proj.c.iter().enumerate().rev() {
            for (k, r) in Self::label_rects(c).iter().enumerate() {
                if let Some(r) = r { if w.0 >= r[0] && w.0 <= r[2] && w.1 >= r[1] && w.1 <= r[3] { return Some((i, k == 0)); } }
            }
        }
        None
    }
    pub fn value_label(c: &Component) -> String { label(c) }

    // ------------------------------------------------------------ pointer events
    pub fn pointer_down(&mut self, w: Pt, shift: bool, z: f64) {
        self.mouse = w; self.edit_open = false;
        match self.tool {
            Tool::Wire => { self.wire_click(w); return; }
            Tool::Place(k) => {
                if self.running { return; }
                self.push();
                let c = self.new_part(k, snap(w.0), snap(w.1), self.place_rot, self.place_mir);
                self.proj.c.push(c); self.sc.push(false);
                self.recent.retain(|x| *x != k); self.recent.insert(0, k); self.recent.truncate(5);
                self.mark(); return;
            }
            Tool::Select => {}
        }
        if !self.running && !shift {
            if let Some(pp) = self.pin_at(w, z) { self.tool = Tool::Wire; self.auto_wire = true; self.wire_start = Some(pp); return; }
        }
        if let Some((c, name)) = self.label_hit(w) {
            self.push();
            let comp = &self.proj.c[c];
            let b0 = if name { comp.no } else { comp.vo }.unwrap_or([0., 0.]);
            self.drag = Some(Drag::Label { c, name, sx: w.0, sy: w.1, b0 }); return;
        }
        let h = self.hit(w, z);
        if self.running {
            if let Some(Hit::Comp(i)) = h {
                match self.proj.c[i].t {
                    Kind::Pot => { self.drag = Some(Drag::Pot { c: i }); self.pot_set(i, w); return; }
                    Kind::Bt | Kind::Nb => { self.proj.c[i].on = 1; self.pokes.push(Poke { comp: i, on: Some(1), pp: None, f: None }); self.drag = Some(Drag::Btn { c: i }); return; }
                    Kind::Sw | Kind::Sp => { let o = self.proj.c[i].on ^ 1; self.proj.c[i].on = o; self.pokes.push(Poke { comp: i, on: Some(o), pp: None, f: None }); return; }
                    _ => {}
                }
            }
        }
        if let Some(h) = h {
            let sel = match h { Hit::Comp(i) => &mut self.sc[i], Hit::Wire(i) => &mut self.sw[i] };
            if !*sel { if !shift { self.clear_selection(); } match h { Hit::Comp(i) => self.sc[i] = true, Hit::Wire(i) => self.sw[i] = true } }
            else if shift { *sel = false; }
            let (ends, _) = self.attached_ends();
            self.drag = Some(Drag::Move { lx: snap(w.0), ly: snap(w.1), ends, moved: false });
        } else {
            if !shift { self.clear_selection(); }
            self.drag = Some(Drag::Box { x0: w.0, y0: w.1, x1: w.0, y1: w.1 });
        }
    }

    /// Endpoints of unselected wires that touch a selected pin / selected wire end (they follow when moving).
    fn attached_ends(&self) -> (Vec<(usize, u8)>, HashSet<(u64, u64)>) {
        let mut pp = HashSet::new();
        for (c, s) in self.proj.c.iter().zip(&self.sc) { if *s { for p in c.pins() { pp.insert(key(p)); } } }
        for (w, s) in self.proj.w.iter().zip(&self.sw) { if *s { pp.insert(key((w.x1, w.y1))); pp.insert(key((w.x2, w.y2))); } }
        let mut ends = vec![];
        for (i, w) in self.proj.w.iter().enumerate() {
            if !self.sw[i] { if pp.contains(&key((w.x1, w.y1))) { ends.push((i, 1)); } if pp.contains(&key((w.x2, w.y2))) { ends.push((i, 2)); } }
        }
        (ends, pp)
    }

    fn wire_click(&mut self, w: Pt) {
        let q = (snap(w.0), snap(w.1));
        let cn = self.auto_wire && self.is_conn(q);
        let mut made = false;
        if let Some(a) = self.wire_start {
            if a != q {
                self.push();
                let mk = |x1, y1, x2, y2| Wire { x1, y1, x2, y2 };
                if a.0 != q.0 && a.1 != q.1 { self.proj.w.push(mk(a.0, a.1, q.0, a.1)); self.proj.w.push(mk(q.0, a.1, q.0, q.1)); }
                else { self.proj.w.push(mk(a.0, a.1, q.0, q.1)); }
                self.resize(); self.mark(); made = true;
            }
        }
        if cn && made { self.set_tool(Tool::Select); } else { self.wire_start = Some(q); }
    }

    fn pot_set(&mut self, i: usize, w: Pt) {
        let c = &mut self.proj.c[i];
        let (lx, _) = rot(w.0 - c.x, w.1 - c.y, (4 - c.r) & 3);
        let p = ((lx + 30.0) / 60.0).clamp(0.0, 1.0) * 100.0;
        let p = (p + 0.5).floor();
        c.f = Some(format!("{p}"));
        self.pokes.push(Poke { comp: i, on: None, pp: Some(p / 100.0), f: None });
    }

    pub fn pointer_move(&mut self, w: Pt) {
        self.mouse = w; self.mouse_in = true;
        let running = self.running;
        let Some(d) = self.drag.take() else { return };
        let nd = match d {
            Drag::Pot { c } => { self.pot_set(c, w); Drag::Pot { c } }
            Drag::Label { c, name, sx, sy, b0 } => { let v = Some([b0[0] + w.0 - sx, b0[1] + w.1 - sy]); if name { self.proj.c[c].no = v } else { self.proj.c[c].vo = v } Drag::Label { c, name, sx, sy, b0 } }
            Drag::Box { x0, y0, .. } => Drag::Box { x0, y0, x1: w.0, y1: w.1 },
            Drag::Move { lx, ly, ends, mut moved } if !running => {
                let (qx, qy) = (snap(w.0), snap(w.1)); let (dx, dy) = (qx - lx, qy - ly);
                if dx != 0.0 || dy != 0.0 {
                    if !moved { self.push(); moved = true; }
                    for (c, s) in self.proj.c.iter_mut().zip(&self.sc) { if *s { c.x += dx; c.y += dy; } }
                    for (x, s) in self.proj.w.iter_mut().zip(&self.sw) { if *s { x.x1 += dx; x.x2 += dx; x.y1 += dy; x.y2 += dy; } }
                    for &(i, e) in &ends { let x = &mut self.proj.w[i]; if e == 1 { x.x1 += dx; x.y1 += dy } else { x.x2 += dx; x.y2 += dy } }
                    self.mark();
                    return self.drag = Some(Drag::Move { lx: qx, ly: qy, ends, moved });
                }
                Drag::Move { lx, ly, ends, moved }
            }
            other => other,
        };
        self.drag = Some(nd);
    }

    pub fn pointer_up(&mut self) {
        let Some(d) = self.drag.take() else { return };
        match d {
            Drag::Btn { c } => { self.proj.c[c].on = 0; self.pokes.push(Poke { comp: c, on: Some(0), pp: None, f: None }); }
            Drag::Box { x0, y0, x1, y1 } => {
                let (a, b, c, d) = (x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1).max(y1));
                if c - a > 3.0 || d - b > 3.0 {
                    let win = x1 >= x0;
                    let t = |r: [f64; 4]| if win { r[0] >= a && r[2] <= c && r[1] >= b && r[3] <= d } else { !(r[0] > c || r[2] < a || r[1] > d || r[3] < b) };
                    for (i, o) in self.proj.c.iter().enumerate() { if t(o.bbox()) { self.sc[i] = true; } }
                    for (i, w) in self.proj.w.iter().enumerate() { if t([w.x1.min(w.x2), w.y1.min(w.y2), w.x1.max(w.x2), w.y1.max(w.y2)]) { self.sw[i] = true; } }
                }
            }
            Drag::Move { moved, .. } => { if moved { self.mark(); } }
            Drag::Label { .. } => self.mark(),
            Drag::Pot { .. } => self.mark(),
        }
    }
    /// Right click without drag: cancel a manual wire chain, otherwise return to the select tool.
    pub fn right_click(&mut self) { if self.wire_start.is_some() && !self.auto_wire { self.wire_start = None } else { self.set_tool(Tool::Select) } }
    pub fn escape(&mut self) { self.right_click() }
    pub fn double_click(&mut self) { self.wire_start = None; if self.auto_wire { self.set_tool(Tool::Select) } }

    // ------------------------------------------------------------ commands
    pub fn delete(&mut self) {
        if self.running || self.n_selected() == 0 { return; }
        self.push();
        let (sc, sw) = (self.sc.clone(), self.sw.clone());
        let mut i = 0; self.proj.c.retain(|_| { i += 1; !sc[i - 1] });
        let mut i = 0; self.proj.w.retain(|_| { i += 1; !sw[i - 1] });
        self.sc = vec![false; self.proj.c.len()]; self.sw = vec![false; self.proj.w.len()]; self.mark();
    }
    pub fn rotate_sel(&mut self, left: bool) {
        if self.running { return; }
        if let Tool::Place(_) = self.tool { self.place_rot = (self.place_rot + 1) & 3; } else { self.group(if left { 'l' } else { 'r' }); }
    }
    pub fn mirror_sel(&mut self, vertical: bool) {
        if self.running { return; }
        if let Tool::Place(_) = self.tool { self.place_mir ^= 1; } else { self.group(if vertical { 'v' } else { 'h' }); }
    }
    /// Group rotate ('r' cw, 'l' ccw) / mirror ('h' about vertical axis, 'v' about horizontal axis).
    pub fn group(&mut self, k: char) {
        if self.running { return; }
        let n = self.n_selected(); if n == 0 { return; }
        if self.sc.iter().zip(&self.proj.c).any(|(s, c)| *s && c.t == Kind::Os) && (n > 1 || k != 'r') {
            self.flash = "Oscilloscope cannot be rotated or mirrored with other parts".into(); return;
        }
        let mut pts: Vec<Pt> = vec![];
        for (c, s) in self.proj.c.iter().zip(&self.sc) { if *s { pts.push((c.x, c.y)); } }
        for (w, s) in self.proj.w.iter().zip(&self.sw) { if *s { pts.push((w.x1, w.y1)); pts.push((w.x2, w.y2)); } }
        let fx = |f: fn(Pt) -> f64, m: fn(f64, f64) -> f64, s: f64| pts.iter().map(|p| f(*p)).fold(s, m);
        let x = snap((fx(|p| p.0, f64::min, f64::MAX) + fx(|p| p.0, f64::max, f64::MIN)) / 2.0);
        let y = snap((fx(|p| p.1, f64::min, f64::MAX) + fx(|p| p.1, f64::max, f64::MIN)) / 2.0);
        let f = move |p: Pt| -> Pt { match k { 'r' => (x - (p.1 - y), y + (p.0 - x)), 'l' => (x + (p.1 - y), y - (p.0 - x)), 'h' => (2. * x - p.0, p.1), _ => (p.0, 2. * y - p.1) } };
        let (ends, _) = self.attached_ends();
        self.push();
        for (i, e) in ends { let w = &mut self.proj.w[i]; if e == 1 { let q = f((w.x1, w.y1)); w.x1 = q.0; w.y1 = q.1 } else { let q = f((w.x2, w.y2)); w.x2 = q.0; w.y2 = q.1 } }
        for (c, s) in self.proj.c.iter_mut().zip(&self.sc) {
            if !*s { continue; }
            let q = f((c.x, c.y)); c.x = q.0; c.y = q.1;
            match k { 'r' => c.r = (c.r + 1) & 3, 'l' => c.r = (c.r + 3) & 3, 'h' => { c.r = (4 - c.r) & 3; c.m ^= 1 } _ => { c.r = (6 - c.r) & 3; c.m ^= 1 } }
        }
        for (w, s) in self.proj.w.iter_mut().zip(&self.sw) {
            if *s { let a = f((w.x1, w.y1)); let b = f((w.x2, w.y2)); w.x1 = a.0; w.y1 = a.1; w.x2 = b.0; w.y2 = b.1; }
        }
        self.mark();
    }

    // ------------------------------------------------------------ clipboard
    /// Copy the selection; returns JSON for the system clipboard.
    pub fn copy(&mut self) -> Option<String> {
        let c: Vec<Component> = self.proj.c.iter().zip(&self.sc).filter(|(_, s)| **s).map(|(c, _)| c.clone()).collect();
        let w: Vec<Wire> = self.proj.w.iter().zip(&self.sw).filter(|(_, s)| **s).map(|(w, _)| *w).collect();
        if c.is_empty() && w.is_empty() { self.flash = "Nothing selected".into(); return None; }
        self.flash = format!("Copied {} part(s)", c.len());
        self.paste_n = 0;
        let clip = Clip { c, w };
        let js = serde_json::to_string(&serde_json::json!({ "app": "CircuitLite-clip", "c": clip.c, "w": clip.w })).ok();
        self.clip = Some(clip); js
    }
    pub fn cut(&mut self) -> Option<String> { let j = self.copy(); self.delete(); j }
    /// Accept clipboard text from another instance.
    pub fn set_clip_json(&mut self, s: &str) -> bool {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(s) else { return false };
        if v["app"] != "CircuitLite-clip" { return false; }
        match (serde_json::from_value(v["c"].clone()), serde_json::from_value(v["w"].clone())) {
            (Ok(c), Ok(w)) => { self.clip = Some(Clip { c, w }); self.paste_n = 0; true } _ => false }
    }
    pub fn has_clip(&self) -> bool { self.clip.is_some() }
    /// Paste at the pointer (or offset 40px per paste when `dup` or the pointer is outside the canvas).
    pub fn paste(&mut self, dup: bool) {
        if self.running { return; }
        let Some(clip) = self.clip.clone() else { return };
        let mut pts: Vec<Pt> = clip.c.iter().map(|c| (c.x, c.y)).collect();
        for w in &clip.w { pts.push((w.x1, w.y1)); pts.push((w.x2, w.y2)); }
        if pts.is_empty() { return; }
        self.paste_n += 1;
        let mut d = (40.0 * self.paste_n as f64, 40.0 * self.paste_n as f64);
        if self.mouse_in && !dup {
            let (x0, x1) = pts.iter().fold((f64::MAX, f64::MIN), |a, p| (a.0.min(p.0), a.1.max(p.0)));
            let (y0, y1) = pts.iter().fold((f64::MAX, f64::MIN), |a, p| (a.0.min(p.1), a.1.max(p.1)));
            d = (snap(self.mouse.0 - (x0 + x1) / 2.0), snap(self.mouse.1 - (y0 + y1) / 2.0));
        }
        self.push(); self.clear_selection();
        for mut c in clip.c { c.n = None; c.x += d.0; c.y += d.1; if c.n.is_none() { c.n = self.next_name(c.t); } self.proj.c.push(c); self.sc.push(true); }
        for w in clip.w { self.proj.w.push(Wire { x1: w.x1 + d.0, y1: w.y1 + d.1, x2: w.x2 + d.0, y2: w.y2 + d.1 }); self.sw.push(true); }
        self.mark();
    }
    pub fn duplicate(&mut self) { if self.copy().is_some() { self.paste(true); } }

    // ------------------------------------------------------------ property edits
    /// Edit one field of a part. One undo step per editing session (reset when the selection changes).
    pub fn edit_field(&mut self, i: usize, key: &str, val: String) {
        if i >= self.proj.c.len() || self.proj.c[i].get(key) == Some(val.as_str()) { return; }
        if !self.edit_open { self.push(); self.edit_open = true; }
        self.proj.c[i].set(key, val);
        if key != "n" && key != "u" { self.mark(); }
    }
    /// Live slider while the simulation runs (pot wiper / LDR light / NTC temperature / solar light).
    pub fn live_field(&mut self, i: usize, val: f64) {
        let c = &mut self.proj.c[i];
        let f = format!("{}", val);
        c.f = Some(f.clone());
        let pp = if matches!(c.t, Kind::Pot | Kind::Sol) { Some(val / 100.0) } else { None };
        self.pokes.push(Poke { comp: i, on: None, pp, f: Some(f) });
    }
    pub fn take_pokes(&mut self) -> Vec<Poke> { std::mem::take(&mut self.pokes) }

    pub fn status(&self) -> String {
        format!("selected: {}  |  Drag left→right = window select, right→left = crossing select (Shift adds)", self.n_selected())
    }
}
