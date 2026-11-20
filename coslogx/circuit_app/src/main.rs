mod audio;
mod canvas;
use canvas::*;
use circuit_core::diagnostics::{self as diag, DynAcc, Faults, Level};
use circuit_core::editor::{Editor, Tool};
use circuit_core::examples::{self, Example};
use circuit_core::engine::Sim;
use circuit_core::kind::Kind;
use circuit_core::params::{self, GROUPS};
use circuit_core::project::{Project, View};
use eframe::egui::{self, Event, Key, PointerButton};



#[derive(Clone, Copy, PartialEq, Debug)]
enum Pending { OpenFile, LoadText, Example(usize) }

struct App {
    ed: Editor, view: View,
    sim: Option<Sim>, paused: bool, on0: Vec<u8>,
    speed_slider: f64, quality: u8, qs: f64,
    search: String, focus_search: bool,
    dlg: bool, dlg_text: String, dlg_msg: String, pending: Option<Pending>,
    ds: Vec<diag::Diag>, dd: Vec<diag::Diag>, faults: Faults, acc: DynAcc, tick: u32, ds_key: (u64, u64), dlg_diag: bool, audio: audio::Audio,
    press: bool, pan: Option<(PointerButton, egui::Pos2, f64, f64, f32)>,
}

impl App {
    fn new() -> App {
        let proj = examples::demo();
        let view = proj.view.unwrap_or_default();
        App { ed: Editor::new(proj), view, sim: None, paused: false, on0: vec![], speed_slider: 50.0, quality: 0, qs: 1.0,
            search: String::new(), focus_search: false, dlg: false, dlg_text: String::new(), dlg_msg: String::new(), pending: None,
            ds: vec![], dd: vec![], faults: Faults::default(), acc: DynAcc::default(), tick: 0, ds_key: (u64::MAX, 0), dlg_diag: false, audio: audio::Audio::default(),
            press: false, pan: None }
    }
    fn speed(&self) -> f64 { 10f64.powf((self.speed_slider - 50.0) / 40.0) }
    fn running(&self) -> bool { self.sim.is_some() }

    fn start_run(&mut self) {
        self.ed.set_tool(Tool::Select);
        self.on0 = self.ed.proj.c.iter().map(|c| c.on).collect();
        self.ed.running = true; self.paused = false;
        self.faults.clear(); self.acc = DynAcc::default(); self.tick = 0; self.dd.clear();
        self.sim = Some(Sim::new(&self.ed.proj, self.qs));
    }
    fn stop_run(&mut self) {
        if self.sim.take().is_some() { for (c, o) in self.ed.proj.c.iter_mut().zip(&self.on0) { c.on = *o; } }
        self.ed.running = false; self.ed.drag = None;
        self.dd.clear(); self.audio.set(None);
    }
    fn toggle_run(&mut self) { if !self.running() { self.start_run() } else { self.paused = !self.paused; if self.paused { self.audio.set(None); } } }
    fn reset(&mut self) { if self.running() { self.faults.clear(); self.acc = DynAcc::default(); self.sim = Some(Sim::new(&self.ed.proj, self.qs)); } }

    fn project_text(&self) -> String { let mut p = self.ed.proj.clone(); p.view = Some(self.view); p.to_json() }
    fn load_text(&mut self, t: &str) -> bool {
        match Project::load_json(t) {
            Ok(l) => { let p = l.project; self.stop_run(); self.view = p.view.unwrap_or(self.view); self.ed.load(p, true); self.ed.mark_saved(); self.ed.flash = l.warnings.join("; "); true }
            Err(_) => false,
        }
    }
    fn guard(&mut self, a: Pending) { if self.ed.is_dirty() { self.pending = Some(a) } else { self.run_pending(a) } }
    fn run_pending(&mut self, a: Pending) {
        match a {
            Pending::OpenFile => {
                if let Some(path) = rfd::FileDialog::new().add_filter("CircuitLite project", &["json"]).pick_file() {
                    self.dlg_msg = match std::fs::read_to_string(&path) { Ok(t) if self.load_text(&t) => { self.dlg = false; String::new() } Ok(_) => "Not a valid project file.".into(), Err(e) => e.to_string() };
                }
            }
            Pending::Example(i) => {
                let (_, ex) = examples::LIST[i];
                let p = ex.build();
                self.stop_run(); self.view = p.view.unwrap_or(self.view);
                self.ed.load(p, ex == Example::Demo); self.ed.mark_saved();
                if matches!(ex, Example::Stress(_)) { self.quality = 0; self.qs = 1.0; }
            }
            Pending::LoadText => { let t = self.dlg_text.clone(); if self.load_text(&t) { self.dlg = false; self.dlg_msg.clear(); } else { self.dlg_msg = "Not a valid project text.".into(); } }
        }
    }

    // ------------------------------------------------------------------ panels
    fn toolbar(&mut self, ui: &mut egui::Ui) {
        let running = self.running();
        ui.horizontal_wrapped(|ui| {
            if ui.selectable_label(running && !self.paused, if running { if self.paused { "Resume" } else { "Pause" } } else { "Run" }).clicked() { self.toggle_run(); }
            if ui.add_enabled(running, egui::Button::new("Stop")).clicked() { self.stop_run(); }
            if ui.add_enabled(running, egui::Button::new("Reset")).clicked() { self.reset(); }
            ui.separator();
            let sp = self.speed();
            ui.add(egui::Slider::new(&mut self.speed_slider, 1.0..=100.0).show_value(false).text(format!("Speed {sp:.2}×")));
            egui::ComboBox::from_id_salt("quality").selected_text(["Quality: Auto", "Quality: High", "Quality: Balanced", "Quality: Fast"][match self.quality { 0 => 0, 1 => 1, 2 => 2, _ => 3 }]).show_ui(ui, |ui| {
                for (v, n) in [(0u8, "Quality: Auto"), (1, "Quality: High"), (2, "Quality: Balanced"), (4, "Quality: Fast")] {
                    if ui.selectable_value(&mut self.quality, v, n).clicked() { self.qs = if v == 0 { 1.0 } else { v as f64 }; if let Some(s) = &self.sim { self.sim = Some(s.rebuild(&self.ed.proj, self.qs)); } }
                }
            });
            ui.separator();
            let (nprob, worst) = { let v = diag::merged(&self.ds, &self.dd); (diag::problem_count(&v), v.first().map(|d| d.level)) };
            let label = if nprob > 0 { format!("Diagnostics ({nprob})") } else { "Diagnostics".to_string() };
            let mut b = egui::Button::new(label);
            if nprob > 0 { b = b.stroke(egui::Stroke::new(1.5, if worst == Some(Level::Error) { egui::Color32::from_rgb(0xe5, 0x48, 0x4d) } else { egui::Color32::from_rgb(0xd9, 0x82, 0x2b) })); }
            if ui.add(b).clicked() { self.dlg_diag = !self.dlg_diag; }
            ui.separator();
            egui::ComboBox::from_id_salt("examples").selected_text("Examples...").show_ui(ui, |ui| {
                for (i, (name, _)) in examples::LIST.iter().enumerate() { if ui.add_enabled(!running, egui::Button::new(*name)).clicked() { self.guard(Pending::Example(i)); } }
            });
            if ui.button("Save").clicked() { self.dlg_text = self.project_text(); self.dlg_msg = "Copy or download to keep your project.".into(); self.dlg = true; }
            if ui.button("Open").clicked() { self.dlg_text.clear(); self.dlg_msg = "Open a file, or paste project text and press Load.".into(); self.dlg = true; }
        });
        ui.horizontal_wrapped(|ui| {
            ui.add_enabled_ui(!running, |ui| {
                if ui.add_enabled(self.ed.can_undo(), egui::Button::new("Undo")).clicked() { self.ed.undo(); }
                if ui.add_enabled(self.ed.can_redo(), egui::Button::new("Redo")).clicked() { self.ed.redo(); }
                ui.separator();
                if ui.selectable_label(self.ed.tool == Tool::Wire, "Wire (W)").clicked() { self.ed.toggle_wire(); }
                if ui.button("Rotate (R)").clicked() { self.ed.rotate_sel(false); }
                if ui.button("Mirror ↔").clicked() { self.ed.mirror_sel(false); }
                if ui.button("Mirror ↕").clicked() { self.ed.mirror_sel(true); }
                ui.separator();
                if ui.button("Copy").clicked() { if let Some(j) = self.ed.copy() { ui.ctx().copy_text(j); } }
                if ui.add_enabled(self.ed.has_clip(), egui::Button::new("Paste")).clicked() { self.ed.mouse_in = false; self.ed.paste(false); }
                if ui.button("Delete").clicked() { self.ed.delete(); }
            });
            if self.ed.is_dirty() { ui.colored_label(egui::Color32::from_rgb(0xd9, 0x82, 0x2b), "● Unsaved changes"); }
        });
    }

    fn props(&mut self, ui: &mut egui::Ui) {
        let running = self.running();
        let Some(i) = self.ed.single() else {
            ui.label("Select one element to edit its name, value, unit and settings. Drag a name or value label to move it. Turn the potentiometer wiper by dragging it while running.");
            return;
        };
        let kind = self.ed.proj.c[i].t;
        ui.horizontal_wrapped(|ui| {
            ui.add_enabled_ui(!running, |ui| {
                let mut n = self.ed.proj.c[i].n.clone().unwrap_or_default();
                ui.label("Name"); if ui.add(egui::TextEdit::singleline(&mut n).desired_width(70.0)).changed() { self.ed.edit_field(i, "n", n); }
                if self.ed.proj.c[i].v.is_some() {
                    let mut u = self.ed.proj.c[i].u.clone().unwrap_or_else(|| circuit_core::symbols::unit(kind).to_string());
                    ui.label("Unit"); if ui.add(egui::TextEdit::singleline(&mut u).desired_width(50.0)).changed() { self.ed.edit_field(i, "u", u); }
                }
            });
            for p in params::params(kind) {
                let live = p.key == "f" && params::live_slider(kind).is_some();
                ui.add_enabled_ui(!running || live, |ui| {
                    ui.label(p.label);
                    let cur = self.ed.proj.c[i].get(p.key).unwrap_or("").to_string();
                    if let Some(opts) = p.opts {
                        let mut sel = cur.clone();
                        egui::ComboBox::from_id_salt((i, p.key)).selected_text(&sel).show_ui(ui, |ui| { for o in opts { ui.selectable_value(&mut sel, o.to_string(), *o); } });
                        if sel != cur { self.ed.edit_field(i, p.key, sel); }
                    } else if live {
                        let (lo, hi) = params::live_slider(kind).unwrap();
                        let mut v: f64 = circuit_core::units::pv(&cur); if v.is_nan() { v = 0.0; }
                        if ui.add(egui::Slider::new(&mut v, lo..=hi).integer()).changed() {
                            if running { self.ed.live_field(i, v) } else { self.ed.edit_field(i, "f", format!("{}", v)); }
                        }
                    } else {
                        let mut s = cur.clone();
                        if ui.add(egui::TextEdit::singleline(&mut s).desired_width(78.0)).changed() { self.ed.edit_field(i, p.key, s); }
                    }
                    if !p.unit.is_empty() { ui.weak(p.unit); }
                });
            }
        });
    }

    fn palette(&mut self, ui: &mut egui::Ui) {
        let running = self.running();
        let r = ui.add(egui::TextEdit::singleline(&mut self.search).hint_text("Search parts  ( / )").desired_width(f32::INFINITY));
        if self.focus_search { r.request_focus(); self.focus_search = false; }
        let mut pick: Option<Kind> = None;
        if r.has_focus() && ui.input(|i| i.key_pressed(Key::Enter)) { pick = params::search(&self.search).first().and_then(|g| g.1.first().copied()); }
        if r.has_focus() && ui.input(|i| i.key_pressed(Key::Escape)) { self.search.clear(); r.surrender_focus(); }
        let q = self.search.trim().to_string();
        let mut groups: Vec<(String, Vec<Kind>)> = vec![];
        if q.is_empty() && !self.ed.recent.is_empty() { groups.push(("Recent".into(), self.ed.recent.clone())); }
        if q.is_empty() { groups.extend(GROUPS.iter().map(|(g, k)| (g.to_string(), k.to_vec()))); }
        else { groups.extend(params::search(&q).into_iter().map(|(g, k)| (g.to_string(), k))); }
        if groups.is_empty() { ui.label(format!("No parts match \"{q}\"")); }
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (g, ks) in &groups {
                egui::CollapsingHeader::new(format!("{} ({})", g.to_uppercase(), ks.len())).id_salt(g).default_open(true).open(if q.is_empty() { None } else { Some(true) }).show(ui, |ui| {
                    for &k in ks {
                        let on = self.ed.tool == Tool::Place(k);
                        if ui.add_enabled(!running, egui::Button::new(k.name()).selected(on).min_size(egui::vec2(ui.available_width(), 0.0))).clicked() { pick = Some(k); }
                    }
                });
            }
        });
        if let Some(k) = pick { self.ed.set_tool(Tool::Place(k)); }
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        if self.dlg {
            let mut open = true;
            egui::Window::new("Project").open(&mut open).collapsible(false).default_width(520.0).show(ctx, |ui| {
                ui.weak(&self.dlg_msg);
                ui.add(egui::TextEdit::multiline(&mut self.dlg_text).font(egui::TextStyle::Monospace).desired_rows(10).desired_width(f32::INFINITY));
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Copy").clicked() { ctx.copy_text(self.dlg_text.clone()); self.dlg_msg = "Copied.".into(); }
                    if ui.button("Download .json").clicked() {
                        let t = if self.dlg_text.is_empty() { self.project_text() } else { self.dlg_text.clone() };
                        if let Some(p) = rfd::FileDialog::new().set_file_name("circuit.json").add_filter("CircuitLite project", &["json"]).save_file() {
                            self.dlg_msg = match std::fs::write(&p, t) { Ok(_) => { self.ed.mark_saved(); "Saved.".into() } Err(e) => format!("Could not save: {e}") };
                        } else { self.dlg_msg = "Cancelled.".into(); }
                    }
                    if ui.button("Open file…").clicked() { self.guard(Pending::OpenFile); }
                    if ui.button("Load from text").clicked() { self.guard(Pending::LoadText); }
                });
            });
            if !open { self.dlg = false; }
        }
        if self.dlg_diag {
            let mut open = true;
            egui::Window::new("Simulation diagnostics").open(&mut open).anchor(egui::Align2::RIGHT_BOTTOM, [-8.0, -36.0]).default_width(380.0).show(ctx, |ui| {
                let all = diag::merged(&self.ds, &self.dd);
                if diag::problem_count(&all) == 0 { ui.colored_label(egui::Color32::from_rgb(0x25, 0x63, 0xeb), "✓ No problems detected."); }
                egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                    for d in &all {
                        let (ico, col) = match d.level { Level::Error => ("✖ ", egui::Color32::from_rgb(0xe5, 0x48, 0x4d)), Level::Warn => ("⚠ ", egui::Color32::from_rgb(0xd9, 0x82, 0x2b)), Level::Info => ("ℹ ", egui::Color32::from_rgb(0x25, 0x63, 0xeb)) };
                        ui.colored_label(col, format!("{ico}{}", d.msg));
                    }
                });
            });
            if !open { self.dlg_diag = false; }
        }
        if let Some(a) = self.pending {
            egui::Window::new("Unsaved changes").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
                ui.label("This circuit has changes that were not saved to a file. Continue and lose them?");
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() { self.pending = None; }
                    if ui.button("Save first").clicked() { self.pending = None; self.dlg_text = self.project_text(); self.dlg_msg = "Download to save, then try again.".into(); self.dlg = true; }
                    if ui.button("Discard").clicked() { self.pending = None; self.run_pending(a); }
                });
            });
        }
    }

    // ------------------------------------------------------------------ input
    fn keys(&mut self, ctx: &egui::Context) {
        if ctx.wants_keyboard_input() { return; }
        let events = ctx.input(|i| i.events.clone());
        for e in events {
            match e {
                Event::Copy => { if let Some(j) = self.ed.copy() { ctx.copy_text(j); } }
                Event::Cut => { if let Some(j) = self.ed.cut() { ctx.copy_text(j); } }
                Event::Paste(t) => { self.ed.set_clip_json(&t); self.ed.paste(false); }
                Event::Key { key, pressed: true, modifiers: m, repeat: false, .. } => {
                    if m.command {
                        match key {
                            Key::Z => if m.shift { self.ed.redo() } else { self.ed.undo() },
                            Key::Y => self.ed.redo(),
                            Key::D => { if !self.running() { self.ed.duplicate() } }
                            Key::A => self.ed.select_all(),
                            _ => {}
                        }
                        continue;
                    }
                    match key {
                        Key::Slash => self.focus_search = true,
                        Key::Delete | Key::Backspace => self.ed.delete(),
                        Key::R => self.ed.rotate_sel(m.shift),
                        Key::M => self.ed.mirror_sel(m.shift),
                        Key::W => self.ed.toggle_wire(),
                        Key::Escape => self.ed.escape(),
                        Key::Space => self.toggle_run(),
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    }

    fn canvas(&mut self, ui: &mut egui::Ui, th: &Theme) {
        let ctx = ui.ctx().clone();
        let (resp, painter) = ui.allocate_painter(ui.available_size(), egui::Sense::click_and_drag());
        let rect = resp.rect;
        let (pos, shift) = ctx.input(|i| (i.pointer.latest_pos(), i.modifiers.shift));
        let (p1, r1, p2, r2, p3, r3) = ctx.input(|i| (i.pointer.button_pressed(PointerButton::Primary), i.pointer.button_released(PointerButton::Primary),
            i.pointer.button_pressed(PointerButton::Secondary), i.pointer.button_released(PointerButton::Secondary),
            i.pointer.button_pressed(PointerButton::Middle), i.pointer.button_released(PointerButton::Middle)));
        let hovered = resp.hovered();
        // pan (middle / right button)
        for (btn, pr, re) in [(PointerButton::Secondary, p2, r2), (PointerButton::Middle, p3, r3)] {
            if pr && hovered { if let Some(p) = pos { self.pan = Some((btn, p, self.view.x, self.view.y, 0.0)); } }
            if let (Some((b, s, vx, vy, mv)), Some(p)) = (self.pan, pos) {
                if b == btn {
                    let d = p - s; self.view.x = vx + d.x as f64; self.view.y = vy + d.y as f64;
                    self.pan = Some((b, s, vx, vy, mv.max(d.x.abs() + d.y.abs())));
                    if re { if b == PointerButton::Secondary && mv < 4.0 { self.ed.right_click(); } self.pan = None; }
                }
            } else if re { self.pan = None; }
        }
        // primary: editor pointer events
        let xf2 = Xf { origin: rect.min, view: self.view };
        if let Some(p) = pos {
            let w = xf2.to_world(p);
            if p1 && hovered { self.press = true; self.ed.pointer_down(w, shift, self.view.z); }
            if self.press || hovered { self.ed.pointer_move(w); }
            if !hovered && !self.press { self.ed.mouse_in = false; }
        }
        if r1 && self.press { self.press = false; self.ed.pointer_up(); }
        if resp.double_clicked_by(PointerButton::Primary) { self.ed.double_click(); }
        if hovered {
            let dy = ctx.input(|i| i.raw_scroll_delta.y);
            if dy != 0.0 { if let Some(m) = pos { zoom_at(&mut self.view, ((m.x - rect.min.x) as f64, (m.y - rect.min.y) as f64), if dy > 0.0 { 1.1 } else { 1.0 / 1.1 }); } }
            if self.ed.tool != Tool::Select { ctx.set_cursor_icon(egui::CursorIcon::Crosshair); }
        }
        let live = self.sim.is_some() && !self.paused;
        let now = ctx.input(|i| i.time) * 1000.0;
        paint(&painter, rect, &self.view, th, &self.ed, self.sim.as_ref(), &self.faults, live, now);
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _f: &mut eframe::Frame) { self.ui_update(ctx) }
}

impl App {
    fn ui_update(&mut self, ctx: &egui::Context) {
        let th = Theme::from_visuals(ctx.style().visuals.dark_mode);
        let now = ctx.input(|i| i.time) * 1000.0;
        // static diagnostics are recomputed whenever the circuit or quality changes
        if self.ds_key != (self.ed.rev, self.qs.to_bits()) {
            if self.ds_key.0 != self.ed.rev { self.faults.clear(); }
            self.ds = diag::statics(&self.ed.proj, self.qs); self.ds_key = (self.ed.rev, self.qs.to_bits());
        }
        // simulation step + live pokes
        if self.sim.is_some() {
            let pokes = self.ed.take_pokes();
            let speed = self.speed();
            let (qa, mut coarsen) = (self.quality == 0, false);
            if let Some(sim) = self.sim.as_mut() {
                for p in pokes { sim.poke(p.comp, p.on, p.pp, p.f.as_deref()); }
                if !self.paused {
                    let dt = ctx.input(|i| i.stable_dt) as f64;
                    let st = sim.frame(dt, speed, 22.0);
                    self.acc.add(&st); self.tick += 1;
                    if self.tick % 15 == 0 {
                        // like the JS `dyn()`: auto-quality first, then the diagnostic pass over the last ~15 frames
                        if qa && self.acc.lag > 5 && self.qs < 4.0 { self.qs *= 2.0; coarsen = true; }
                        self.dd = diag::dynamic(sim, &self.ed.proj, &mut self.faults, &self.acc, qa, self.qs, now);
                        self.audio.set(sim.buzzer_tone());
                        self.acc = DynAcc::default();
                        if coarsen { *sim = sim.rebuild(&self.ed.proj, self.qs); }
                    }
                }
            }
            ctx.request_repaint();
        } else { self.ed.take_pokes(); }
        self.keys(ctx);

        egui::TopBottomPanel::top("bar").show(ctx, |ui| self.toolbar(ui));
        egui::TopBottomPanel::top("props").show(ctx, |ui| self.props(ui));
        egui::SidePanel::left("palette").exact_width(176.0).show(ctx, |ui| self.palette(ui));
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            let t = self.sim.as_ref().map(|s| s.time()).unwrap_or(0.0);
            let mode = if !self.running() { "■ STOPPED - edit mode" } else if self.paused { "⏸ PAUSED" } else { "▶ RUNNING - editing locked" };
            let g = if self.ed.proj.c.iter().any(|c| c.t == Kind::Gnd) || self.ed.proj.c.is_empty() { "" } else { "  ⚠ Add a Ground." };
            let fl = if self.ed.flash.is_empty() { String::new() } else { format!("  |  {}", self.ed.flash) };
            ui.label(format!("{mode}{g}  |  t = {}  |  {}{fl}", circuit_core::units::fm(t, "s"), self.ed.status()));
        });
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx, |ui| self.canvas(ui, &th));
        self.dialogs(ctx);
    }
}

fn main() -> eframe::Result {
    let opts = eframe::NativeOptions { viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 800.0]).with_title("CircuitLite"), ..Default::default() };
    eframe::run_native("CircuitLite", opts, Box::new(|_cc| Ok(Box::new(App::new()))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use circuit_core::editor::Editor;
    use egui::{pos2, vec2, Modifiers, RawInput};

    struct Rig { ctx: egui::Context, app: App }
    impl Rig {
        fn new(p: Project) -> Rig {
            let mut app = App::new(); app.ed = Editor::new(p); app.view = View { x: 0.0, y: 0.0, z: 1.0 };
            let mut r = Rig { ctx: egui::Context::default(), app }; r.frame(vec![]); r.frame(vec![]); r
        }
        fn frame(&mut self, events: Vec<Event>) {
            let mut raw = RawInput::default();
            raw.screen_rect = Some(egui::Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 800.0)));
            raw.events = events;
            let app = &mut self.app;
            let _ = self.ctx.run(raw, |c| app.ui_update(c));
        }
        fn mv(&mut self, p: egui::Pos2) { self.frame(vec![Event::PointerMoved(p)]); }
        fn btn(&mut self, p: egui::Pos2, b: PointerButton, down: bool) { self.frame(vec![Event::PointerButton { pos: p, button: b, pressed: down, modifiers: Modifiers::NONE }]); }
        fn click(&mut self, p: egui::Pos2) { self.mv(p); self.btn(p, PointerButton::Primary, true); self.btn(p, PointerButton::Primary, false); }
        fn key(&mut self, k: Key, m: Modifiers) {
            self.frame(vec![Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers: m }]);
            self.frame(vec![Event::Key { key: k, physical_key: None, pressed: false, repeat: false, modifiers: m }]);
        }
    }

    fn two_parts() -> Project { let mut p = Project::empty(); let mut r = circuit_core::project::Component::new(Kind::R, 400.0, 300.0, 0); r.n = Some("R1".into()); p.c.push(r); p }

    /// The canvas origin depends on panel sizes; find it by clicking a known part.
    fn calibrate(r: &mut Rig) -> egui::Pos2 {
        // the central panel starts right of the 176px palette and below the toolbars
        for y in (60..260).step_by(2) { r.mv(pos2(700.0, y as f32)); r.btn(pos2(700.0, y as f32), PointerButton::Primary, true); r.btn(pos2(700.0, y as f32), PointerButton::Primary, false); if r.app.ed.mouse_in { return pos2(0.0, y as f32); } }
        pos2(0.0, 0.0)
    }

    #[test]
    fn click_places_a_part_and_selection_commands_work() {
        let mut r = Rig::new(two_parts());
        let _ = calibrate(&mut r);
        r.app.ed.proj.c.truncate(1); r.app.ed.sc = vec![false];
        r.app.ed.set_tool(Tool::Place(Kind::C));
        r.click(pos2(800.0, 400.0));
        assert_eq!(r.app.ed.proj.c.len(), 2, "click should place a part");
        assert_eq!(r.app.ed.proj.c[1].t, Kind::C);
        // select all, then delete via keyboard, then undo via Ctrl+Z
        r.app.ed.set_tool(Tool::Select);
        r.key(Key::A, Modifiers::COMMAND);
        assert_eq!(r.app.ed.n_selected(), 2);
        r.key(Key::Delete, Modifiers::NONE);
        assert_eq!(r.app.ed.proj.c.len(), 0);
        r.key(Key::Z, Modifiers::COMMAND);
        assert_eq!(r.app.ed.proj.c.len(), 2);
        r.key(Key::Y, Modifiers::COMMAND);
        assert_eq!(r.app.ed.proj.c.len(), 0);
    }

    #[test]
    fn keyboard_shortcuts_pick_tools_and_transform() {
        let mut r = Rig::new(two_parts());
        r.key(Key::W, Modifiers::NONE); assert_eq!(r.app.ed.tool, Tool::Wire);
        r.key(Key::Escape, Modifiers::NONE); assert_eq!(r.app.ed.tool, Tool::Select);
        r.key(Key::A, Modifiers::COMMAND);
        let r0 = r.app.ed.proj.c[0].r;
        r.key(Key::R, Modifiers::NONE); assert_eq!(r.app.ed.proj.c[0].r, (r0 + 1) & 3);
        r.key(Key::R, Modifiers::SHIFT); assert_eq!(r.app.ed.proj.c[0].r, r0);
        r.key(Key::M, Modifiers::NONE); assert_eq!(r.app.ed.proj.c[0].m, 1);
        r.key(Key::Slash, Modifiers::NONE); assert!(r.app.focus_search || r.ctx.wants_keyboard_input());
    }

    #[test]
    fn right_drag_pans_and_wheel_zooms() {
        let mut r = Rig::new(two_parts());
        let start = pos2(700.0, 500.0);
        r.mv(start);
        r.btn(start, PointerButton::Secondary, true);
        r.mv(pos2(760.0, 540.0));
        r.btn(pos2(760.0, 540.0), PointerButton::Secondary, false);
        assert!((r.app.view.x - 60.0).abs() < 1.0 && (r.app.view.y - 40.0).abs() < 1.0, "view {:?}", (r.app.view.x, r.app.view.y));
        let z0 = r.app.view.z;
        r.frame(vec![Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta: vec2(0.0, 50.0), modifiers: Modifiers::NONE }]);
        assert!(r.app.view.z > z0);
    }

    #[test]
    fn space_runs_and_stops_and_switch_toggles_live() {
        let mut p = Project::empty();
        for (k, x) in [(Kind::Dc, 200.0), (Kind::Sw, 400.0), (Kind::R, 600.0), (Kind::Gnd, 800.0)] { p.c.push(circuit_core::project::Component::new(k, x, 300.0, 0)); }
        let mut r = Rig::new(p);
        r.key(Key::Space, Modifiers::NONE);
        assert!(r.app.running() && r.app.ed.running);
        let _ = calibrate(&mut r);
        // palette buttons and editing are locked while running
        r.app.ed.set_tool(Tool::Place(Kind::R)); assert_eq!(r.app.ed.tool, Tool::Select);
        // toggle the switch with a click on it: find it on screen
        let before = r.app.ed.proj.c[1].on;
        let o = { let mut found = None; for y in 40..200 { r.mv(pos2(1000.0, y as f32)); if r.app.ed.mouse_in { found = Some(y); break; } } found.unwrap_or(100) as f32 };
        let sw = pos2(176.0 + 400.0 + r.app.view.x as f32, o + 300.0 + r.app.view.y as f32);
        r.click(sw);
        assert_ne!(r.app.ed.proj.c[1].on, before, "switch should toggle while running");
        r.key(Key::Space, Modifiers::NONE); assert!(r.app.paused);
        r.app.stop_run();
        assert_eq!(r.app.ed.proj.c[1].on, before, "switch state is restored on stop");
        assert!(!r.app.ed.running);
    }

    fn short_loop() -> Project {
        let mut p = Project::empty();
        for (k, x, n) in [(Kind::Dc, 200.0, "V1"), (Kind::Am, 400.0, "AM1"), (Kind::Gnd, 600.0, "")] {
            let mut c = circuit_core::project::Component::new(k, x, 300.0, 0); if !n.is_empty() { c.n = Some(n.into()); } p.c.push(c);
        }
        let pin = |i: usize, j: usize| p.c[i].pins()[j];
        let w = |a: (f64, f64), b: (f64, f64)| circuit_core::project::Wire { x1: a.0, y1: a.1, x2: b.0, y2: b.1 };
        let ws = vec![w(pin(0, 0), pin(1, 0)), w(pin(1, 1), pin(2, 0)), w(pin(0, 1), pin(2, 0))];
        p.w = ws; p
    }

    #[test]
    fn running_a_short_circuit_fills_diagnostics_and_fault_markers() {
        let mut r = Rig::new(short_loop());
        assert!(diag::problem_count(&r.app.ds) == 0, "static checks pass: {:?}", r.app.ds);
        r.key(Key::Space, Modifiers::NONE);
        for _ in 0..40 { r.frame(vec![]); }
        assert!(r.app.dd.iter().any(|d| d.level == Level::Error && d.msg.contains("short-circuited")), "{:?}", r.app.dd);
        assert!(!r.app.faults.comps.is_empty() && r.app.faults.wires.len() == 3);
        // stop: runtime messages clear, markers stay visible for inspection until the circuit is edited
        r.app.stop_run();
        assert!(r.app.dd.is_empty()); assert!(!r.app.faults.comps.is_empty());
        r.frame(vec![]);
        r.app.ed.set_tool(Tool::Place(Kind::R)); r.click(pos2(900.0, 500.0));
        r.frame(vec![]);
        assert!(r.app.faults.comps.is_empty(), "editing clears markers");
    }

    #[test]
    fn static_diagnostics_update_when_the_circuit_changes() {
        let mut p = Project::empty(); p.c.push(circuit_core::project::Component::new(Kind::R, 400.0, 300.0, 0));
        let mut r = Rig::new(p);
        assert!(r.app.ds.iter().any(|d| d.msg.contains("No Ground")), "{:?}", r.app.ds);
        r.app.ed.set_tool(Tool::Place(Kind::Gnd)); r.click(pos2(900.0, 500.0));
        r.frame(vec![]);
        assert!(!r.app.ds.iter().any(|d| d.msg.contains("No Ground")), "{:?}", r.app.ds);
    }

    #[test]
    fn examples_load_directly_or_through_the_unsaved_guard() {
        let mut r = Rig::new(two_parts());
        r.app.guard(Pending::Example(1));                 // clean project: loads immediately
        assert!(r.app.pending.is_none());
        assert!(r.app.ed.proj.c.iter().any(|c| c.t == Kind::Tm), "555 example loaded");
        assert!(!r.app.ed.is_dirty() && !r.app.ed.can_undo());
        r.app.ed.set_tool(Tool::Place(Kind::R)); r.click(pos2(900.0, 500.0));
        assert!(r.app.ed.is_dirty());
        r.app.guard(Pending::Example(0));                 // dirty: asks first
        assert_eq!(r.app.pending, Some(Pending::Example(0)));
        assert!(r.app.ed.proj.c.iter().any(|c| c.t == Kind::Tm), "nothing changed yet");
        r.app.run_pending(Pending::Example(0)); r.app.pending = None;
        assert!(r.app.ed.proj.c.iter().any(|c| c.t == Kind::Npn));
        assert!(r.app.ed.can_undo(), "the first example stays undoable like the JS loadProj");
    }

    #[test]
    fn stress_example_resets_quality_to_auto() {
        let mut r = Rig::new(two_parts());
        r.app.quality = 4; r.app.qs = 4.0;
        r.app.run_pending(Pending::Example(2));
        assert_eq!((r.app.quality, r.app.qs), (0, 1.0));
        assert_eq!(r.app.ed.proj.c.len(), 48);
        r.key(Key::Space, Modifiers::NONE);
        for _ in 0..20 { r.frame(vec![]); }
        assert_eq!(r.app.sim.as_ref().unwrap().islands.len(), 6);
    }
}
