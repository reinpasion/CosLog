//! Backend-neutral vector symbol renderer (port of the JS `sym`, `osc`, `tml`, `pinLbl`, `lbl`, `junc`).
//! Everything is emitted as flattened polylines / polygons / text so it can be drawn by egui,
//! exported to SVG, or unit-tested. Symbol layers, captions, glow, LED colours, value suffixes and
//! reading formats all come from `spec/parts.json` via build.rs.
use crate::kind::Kind;
use crate::netlist::{Part, Wave, NS};
use crate::project::{Component, Project};
use crate::spec::{self, StateView};
use crate::units::{fm, pv};
use std::f64::consts::PI;

pub type Pt = (f64, f64);

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Col { Ink, Wire, Gray, Rgba(u8, u8, u8, u8) }
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Align { Left, Center, Right }

#[derive(Clone, Debug)]
pub enum Prim {
    Stroke { pts: Vec<Pt>, col: Col, w: f32 },
    Fill { pts: Vec<Pt>, col: Col },
    Text { p: Pt, s: String, size: f32, bold: bool, align: Align, mid: bool, col: Col },
}

// ---------------------------------------------------------------- path builder (canvas semantics)
#[derive(Default)]
struct Cv { prims: Vec<Prim>, subs: Vec<Vec<Pt>>, col: Option<Col>, w: f32 }

impl Cv {
    fn begin(&mut self) { self.subs.clear(); }
    fn mv(&mut self, x: f64, y: f64) { self.subs.push(vec![(x, y)]); }
    fn ln(&mut self, x: f64, y: f64) {
        match self.subs.last_mut() { Some(s) => s.push((x, y)), None => self.mv(x, y) }
    }
    fn pp(&mut self, a: &[f64]) { self.mv(a[0], a[1]); for c in a[2..].chunks(2) { self.ln(c[0], c[1]); } }
    fn cur(&self) -> Option<Pt> { self.subs.last().and_then(|s| s.last().copied()) }
    fn arc(&mut self, cx: f64, cy: f64, r: f64, a0: f64, a1: f64, ccw: bool) {
        let tau = 2.0 * PI;
        let mut sw = a1 - a0;
        if !ccw { if sw >= tau { sw = tau } else { while sw < 0.0 { sw += tau } } }
        else if -sw >= tau { sw = -tau } else { while sw > 0.0 { sw -= tau } }
        let n = ((sw.abs() / tau * 48.0 - 1e-3).ceil() as usize).max(4);
        for i in 0..=n {
            let a = a0 + sw * i as f64 / n as f64;
            let p = (cx + r * a.cos(), cy + r * a.sin());
            if i == 0 {
                match self.cur() {
                    Some(c) if (c.0 - p.0).abs() < 1e-3 && (c.1 - p.1).abs() < 1e-3 => {}
                    Some(_) => self.ln(p.0, p.1),
                    None => self.mv(p.0, p.1),
                }
            } else { self.ln(p.0, p.1); }
        }
    }
    fn circle(&mut self, x: f64, y: f64, r: f64) { self.mv(x + r, y); self.arc(x, y, r, 0.0, 2.0 * PI, false); }
    fn bez(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, x: f64, y: f64) {
        let (x0, y0) = self.cur().unwrap_or((x1, y1));
        for i in 1..=16 {
            let t = i as f64 / 16.0; let u = 1.0 - t;
            self.ln(u * u * u * x0 + 3.0 * u * u * t * x1 + 3.0 * u * t * t * x2 + t * t * t * x,
                    u * u * u * y0 + 3.0 * u * u * t * y1 + 3.0 * u * t * t * y2 + t * t * t * y);
        }
    }
    fn quad(&mut self, x1: f64, y1: f64, x: f64, y: f64) {
        let (x0, y0) = self.cur().unwrap_or((x1, y1));
        for i in 1..=16 {
            let t = i as f64 / 16.0; let u = 1.0 - t;
            self.ln(u * u * x0 + 2.0 * u * t * x1 + t * t * x, u * u * y0 + 2.0 * u * t * y1 + t * t * y);
        }
    }
    fn style(&mut self, col: Col, w: f32) { self.col = Some(col); self.w = w; }
    fn stroke(&mut self) {
        let col = self.col.unwrap_or(Col::Ink);
        for s in &self.subs { if s.len() > 1 { self.prims.push(Prim::Stroke { pts: s.clone(), col, w: self.w }); } }
    }
    fn fill(&mut self, col: Col) {
        for s in &self.subs { if s.len() > 2 { self.prims.push(Prim::Fill { pts: s.clone(), col }); } }
    }
    fn rect(&self, x: f64, y: f64, w: f64, h: f64) -> Vec<Pt> { vec![(x, y), (x + w, y), (x + w, y + h), (x, y + h), (x, y)] }
    fn fill_rect(&mut self, x: f64, y: f64, w: f64, h: f64, col: Col) { let r = self.rect(x, y, w, h); self.prims.push(Prim::Fill { pts: r, col }); }
    fn stroke_rect(&mut self, x: f64, y: f64, w: f64, h: f64, col: Col, lw: f32) { let r = self.rect(x, y, w, h); self.prims.push(Prim::Stroke { pts: r, col, w: lw }); }
    fn text(&mut self, x: f64, y: f64, s: &str, size: f32, bold: bool, align: Align, mid: bool, col: Col) {
        self.prims.push(Prim::Text { p: (x, y), s: s.into(), size, bold, align, mid, col });
    }
}

// ---------------------------------------------------------------- symbol state
#[derive(Clone, Debug)]
pub struct SymState {
    pub on: bool, pub st: bool, pub bl: bool, pub cnt: u32,
    pub ang: f64, pub gl: f64, pub wiper: f64, pub wave: Wave, pub high: bool, pub rgb: [u8; 3],
    pub scope: Option<(Vec<f32>, Vec<f32>)>, pub tdiv: f64, pub vdiv: f64,
}

/// LED colour from the spec `ledColors` table; unknown or missing names fall back to red (the LED default).
pub fn led_rgb(p: Option<&str>) -> [u8; 3] {
    let find = |n: &str| spec::LED_RGB.iter().find(|e| e.0 == n).map(|e| e.1);
    p.and_then(find).or_else(|| find("red")).expect("spec ledColors defines red")
}
fn nz(x: f64) -> f64 { if x.is_nan() { 0.0 } else { x } }
fn wave_of(c: &Component) -> Wave { match c.k.as_deref() { Some("square") => Wave::Square, Some("triangle") => Wave::Triangle, Some("sawtooth") => Wave::Sawtooth, _ => Wave::Sine } }

impl SymState {
    /// State for edit mode (no simulation running).
    pub fn edit(c: &Component) -> SymState {
        let tdiv = pv(c.v.as_deref().unwrap_or(""));
        SymState { on: c.on != 0, st: false, bl: false, cnt: 0, ang: 0.0, gl: 0.0,
            wiper: nz(pv(c.f.as_deref().unwrap_or(""))), wave: wave_of(c), high: false, rgb: led_rgb(c.p.as_deref()),
            scope: if c.t == Kind::Os { Some((vec![0.0; NS], vec![0.0; NS])) } else { None },
            tdiv: if tdiv > 0.0 { tdiv } else { 1e-3 }, vdiv: pv(c.f.as_deref().unwrap_or("")) }
    }
    /// State while running, from the live part.
    pub fn run(c: &Component, p: &Part) -> SymState {
        let mut s = SymState::edit(c);
        s.on = p.on != 0; s.st = p.st; s.bl = p.bl; s.cnt = p.cnt; s.ang = p.ang; s.gl = p.gl; s.high = p.av > p.val;
        if c.t == Kind::Os { s.scope = Some((p.ta.clone(), p.tb.clone())); s.tdiv = p.val; }
        s
    }
}

// ---------------------------------------------------------------- symbols (local coordinates)
fn nice(x: f64) -> f64 { let e = 10f64.powf(x.log10().floor()); let f = x / e; (if f <= 1.0 { 1.0 } else if f <= 2.0 { 2.0 } else if f <= 5.0 { 5.0 } else { 10.0 }) * e }

impl StateView for SymState {
    fn state(&self, key: &str) -> f64 {
        match key { "on" => self.on as u8 as f64, "st" => self.st as u8 as f64, "bl" => self.bl as u8 as f64, "cnt" => self.cnt as f64, "hi" => self.high as u8 as f64,
            "gl" => self.gl, "ang" => self.ang, "w" => self.wiper, _ => 0.0 }
    }
    fn value(&self) -> f64 { 0.0 }
    fn param(&self, key: &str) -> Option<&str> {
        if key != "k" { return None; }
        Some(match self.wave { Wave::Sine => "sine", Wave::Square => "square", Wave::Triangle => "triangle", Wave::Sawtooth => "sawtooth" })
    }
}

/// One number of a spec path: a literal, or a wiper expression `{w}`, `{w-4}`, `{w+4}`.
fn arg(t: &str, wx: f64) -> f64 {
    match t.strip_prefix('{').and_then(|r| r.strip_suffix('}')) {
        Some(e) => { let rest = e.strip_prefix('w').unwrap_or_else(|| panic!("bad expression {t}")); if rest.is_empty() { wx } else { wx + rest.parse::<f64>().unwrap() } }
        None => t.parse().unwrap_or_else(|_| panic!("bad number {t}")),
    }
}

/// Replays a spec path string (`M L A C Q`, see `pathSyntax` in parts.json) onto the canvas.
fn run_path(v: &mut Cv, d: &str, wx: f64) {
    let mut cmd = ' '; let mut rest = d;
    while !rest.is_empty() {
        cmd = rest.chars().next().filter(|c| c.is_ascii_alphabetic()).unwrap_or(cmd);
        let body = &rest[1..];
        let (mut depth, mut end) = (0, body.len());
        for (i, ch) in body.char_indices() { match ch { '{' => depth += 1, '}' => depth -= 1, c if depth == 0 && c.is_ascii_alphabetic() => { end = i; break; } _ => {} } }
        let a: Vec<f64> = body[..end].split_whitespace().map(|t| arg(t, wx)).collect();
        match cmd {
            'M' => v.mv(a[0], a[1]), 'L' => v.ln(a[0], a[1]),
            'A' => v.arc(a[0], a[1], a[2], a[3], a[4], a[5] != 0.0),
            'C' => v.bez(a[0], a[1], a[2], a[3], a[4], a[5]), 'Q' => v.quad(a[0], a[1], a[2], a[3]),
            other => panic!("unknown path command {other}"),
        }
        rest = &body[end..];
    }
}

fn hex(s: &str) -> Col {
    let h = &s[1..];
    let n = if h.len() == 3 { h.chars().map(|c| { let d = c.to_digit(16).unwrap(); d * 17 }).fold(0, |a, d| a << 8 | d) } else { u32::from_str_radix(h, 16).unwrap() };
    Col::Rgba((n >> 16) as u8, (n >> 8) as u8, n as u8, 255)
}

/// Draws a part's symbol from the spec layer list: conditional layers, wiper expressions, spinning layers, fills,
/// widgets and in-body captions (spec `pinText`).
fn sym(k: Kind, st: &SymState, v: &mut Cv) {
    v.begin();
    let ps = spec::part(k.code()).expect("part has a spec");
    let wx = -30.0 + 60.0 * (st.wiper / 100.0).clamp(0.0, 1.0);
    let mut fills: Vec<(usize, usize, Col)> = vec![];
    for l in ps.symbol {
        if l.widget == Some("scope") { osc(st, v); continue; }
        if l.when.map_or(false, |w| !w.holds(st)) { continue; }
        let start = v.subs.len();
        run_path(v, l.d, wx);
        if l.spin {
            let (c, s) = (st.ang.cos(), st.ang.sin());
            for sub in &mut v.subs[start..] { for p in sub.iter_mut() { *p = (p.0 * c - p.1 * s, p.0 * s + p.1 * c); } }
        }
        if l.fill.is_some() {
            let col = l.fill_when.iter().find(|(w, _)| w.holds(st)).map(|(_, c)| hex(c)).unwrap_or(Col::Gray);
            fills.push((start, v.subs.len(), col));
        }
    }
    v.stroke();
    for (a, b, col) in fills { for sub in v.subs[a..b].to_vec() { if sub.len() > 2 { v.prims.push(Prim::Fill { pts: sub, col }); } } }
    if let Some((_, items)) = spec::PIN_TEXT.iter().find(|e| e.0 == k.code()) {
        v.begin();
        for &(t, x, y, size, bold) in items.iter() { v.text(x, y, t, size, bold, Align::Center, true, Col::Ink); }
    }
}

fn osc(st: &SymState, v: &mut Cv) {
    let rgb = |s: &str| { let n = u32::from_str_radix(&s[1..], 16).unwrap(); Col::Rgba((n >> 16) as u8, (n >> 8) as u8, n as u8, 255) };
    v.fill_rect(-30., -60., 200., 120., rgb("#08140e"));
    v.begin();
    for i in 1..10 { let x = -30. + i as f64 * 20.; v.pp(&[x, -60., x, 60.]); }
    for j in 1..6 { let y = -60. + j as f64 * 20.; v.pp(&[-30., y, 170., y]); }
    v.style(rgb("#1f3b2c"), 1.0); v.stroke();
    v.begin(); v.pp(&[-30., 0., 170., 0.]); v.pp(&[70., -60., 70., 60.]); v.style(rgb("#3e6b52"), 1.0); v.stroke();
    v.stroke_rect(-30., -60., 200., 120., Col::Ink, 2.0);
    v.begin(); v.pp(&[-40., -20., -30., -20.]); v.pp(&[-40., 20., -30., 20.]); v.style(Col::Ink, 2.0); v.stroke(); v.begin();
    let Some((ta, tb)) = &st.scope else { return };
    let mm = |a: &[f32]| a.iter().fold((f64::MAX, f64::MIN), |(m, x), &p| (m.min(p as f64), x.max(p as f64)));
    let ((m1, x1), (m2, x2)) = (mm(ta), mm(tb));
    let vd = if st.vdiv > 0.0 { st.vdiv } else { nice(m1.abs().max(x1.abs()).max(m2.abs()).max(x2.abs()).max(1e-3) / 2.6) };
    for (a, cl) in [(ta, "#f5d020"), (tb, "#35c7ff")] {
        v.begin();
        for i in 0..NS.min(a.len()) {
            let x = -30. + i as f64 * 200. / NS as f64; let y = (-(a[i] as f64) / vd * 20.).clamp(-60., 60.);
            if i == 0 { v.mv(x, y) } else { v.ln(x, y) }
        }
        v.style(rgb(cl), 1.4); v.stroke();
    }
    v.text(-27., 56., &format!("CH1 {}", fm(x1 - m1, "Vpp")), 9., false, Align::Left, false, rgb("#f5d020"));
    v.text(50., 56., &format!("CH2 {}", fm(x2 - m2, "Vpp")), 9., false, Align::Left, false, rgb("#35c7ff"));
    v.text(-27., -51., &format!("{}/div{}", fm(vd, "V"), if st.vdiv > 0.0 { "" } else { " auto" }), 9., false, Align::Left, false, rgb("#9aabbb"));
    v.text(167., -51., &format!("{}/div", fm(st.tdiv, "s")), 9., false, Align::Right, false, rgb("#9aabbb"));
    v.style(Col::Ink, 2.0);
}

// ---------------------------------------------------------------- world-space component drawing
fn xf(c: &Component, p: Pt) -> Pt {
    let (mut x, mut y) = (if c.m != 0 { -p.0 } else { p.0 }, p.1);
    for _ in 0..c.r & 3 { (x, y) = (-y, x); }
    (c.x + x, c.y + y)
}

/// Unit shown after a part's value (spec `labelUnit`).
pub fn unit(k: Kind) -> &'static str { k.label_unit() }

/// Spec `valueFormat` of a part: value | meter | ohm | motor | logic | none.
fn value_format(k: Kind) -> &'static str {
    spec::VALUE_FORMAT.iter().find(|e| e.0 == k.code()).map(|e| e.1).unwrap_or_else(|| panic!("no valueFormat for {k:?}"))
}
/// Spec `centerText`: (text, size, bold, dy).
fn center_text(k: Kind) -> Option<(&'static str, f32, bool, f64)> {
    spec::CENTER_TEXT.iter().find(|e| e.0 == k.code()).map(|e| (e.1, e.2, e.3, e.4))
}

/// Value label text (JS `lab`): value + unit, then the spec `valueSuffix` entries.
pub fn label(c: &Component) -> String {
    let Some(v) = &c.v else { return String::new() };
    let mut s = format!("{}{}", v, c.u.as_deref().unwrap_or(unit(c.t)));
    if let Some((_, sfx)) = spec::VALUE_SUFFIX.iter().find(|e| e.0 == c.t.code()) {
        for (key, u) in sfx.iter() { s += &format!(" {}{}", c.get(key).unwrap_or(""), u); }
    }
    s
}
/// Meter / probe / motor reading (JS `rd`), by spec `valueFormat`. Parts with a plain value label return "".
pub fn reading(k: Kind, av: f64, sq: f64, w: f64, val: f64) -> String {
    match value_format(k) {
        "motor" => { let r = (w * 9.549).round(); format!("{} rpm", if r == 0.0 { 0.0 } else { r }) }
        "logic" => if av > val { "HIGH".into() } else { "LOW".into() },
        "ohm" => if av > 1e8 { "OL".into() } else { fm(av, "Ω") },
        "meter" => {
            let rm = sq.sqrt(); let ac = (sq - av * av).max(0.0).sqrt();
            let u = center_text(k).map_or("", |t| t.0); // the meter letter is its unit: V or A
            if ac > 0.02 * rm && ac > 1e-6 { format!("{} rms", fm(rm, u)) } else { fm(av, u) }
        }
        "value" | "none" => String::new(),
        other => panic!("{k:?}: unknown valueFormat {other:?}"),
    }
}

/// Text shown under a part: live reading for meters/probes/motors, otherwise the value label.
pub fn value_text(c: &Component, reading_txt: Option<&str>) -> String {
    match value_format(c.t) {
        "value" => label(c),
        "none" => String::new(),
        _ => reading_txt.map(String::from).unwrap_or_else(|| reading(c.t, 0., 0., 0., 1.)),
    }
}

/// Draw a component in world coordinates. `reading` overrides the value text for meters/probes/motors.
/// `ghost` returns only the (half-transparent) symbol, for placement previews.
pub fn draw_component(c: &Component, st: &SymState, reading_txt: Option<&str>, ghost: bool) -> Vec<Prim> {
    let mut v = Cv::default();
    v.style(Col::Ink, 2.0);
    let mut local = Cv::default();
    local.style(Col::Ink, 2.0);
    if st.gl > 0.03 {
        // symbols_golden pins the hand-written behaviour: every kind glows when `gl` is set, so parts without
        // a spec `glow` entry use the lamp's.
        let gl = spec::GLOW.iter().find(|e| e.0 == c.t.code()).or_else(|| spec::GLOW.iter().find(|e| e.0 == "lamp")).expect("spec defines a lamp glow");
        let [r, g, b] = gl.2.unwrap_or(st.rgb);
        local.begin(); local.circle(0., 0., gl.1);
        local.fill(Col::Rgba(r, g, b, (st.gl.min(1.0) * 0.85 * 255.0) as u8));
    }
    sym(c.t, st, &mut local);
    for p in local.prims {
        v.prims.push(match p {
            Prim::Stroke { pts, col, w } => Prim::Stroke { pts: pts.into_iter().map(|q| xf(c, q)).collect(), col, w },
            Prim::Fill { pts, col } => Prim::Fill { pts: pts.into_iter().map(|q| xf(c, q)).collect(), col },
            Prim::Text { p, s, size, bold, align, mid, col } => Prim::Text { p: xf(c, p), s, size, bold, align, mid, col },
        });
    }
    if ghost { return v.prims; }
    // pin labels
    let sp = spec::part(c.t.code()).expect("part has a spec");
    if sp.pins.iter().any(|p| p.label.is_some()) {
        for (i, (lp, wp)) in c.t.pins().iter().zip(c.pins()).enumerate() {
            let ld = if lp.0.abs() >= 40.0 { (if lp.0 > 0.0 { -1.0 } else { 1.0 }, 0.0) } else { (0.0, if lp.1 > 0.0 { -1.0 } else { 1.0 }) };
            let (mut sx, sy) = (if c.m != 0 { -ld.0 } else { ld.0 }, ld.1);
            let mut sy_ = sy;
            for _ in 0..c.r & 3 { let t = sx; sx = -sy_; sy_ = t; }
            let (mut x, mut y) = (wp.0 + sx * 9.0, wp.1 + sy_ * 9.0);
            if sy_ == 0.0 { y -= 6.0 } else { x += 8.0 }
            v.text(x, y, sp.pins[i].label.unwrap_or(""), 8., false, Align::Center, true, Col::Ink);
        }
    }
    if let Some((t, size, bold, dy)) = center_text(c.t) { v.text(c.x, c.y + dy, t, size, bold, Align::Center, false, Col::Ink); }
    let b = c.bbox();
    let no = c.no.unwrap_or([0., 0.]); let vo = c.vo.unwrap_or([0., 0.]);
    if let Some(n) = &c.n { v.text(c.x + no[0], b[1] - 4. + no[1], n, 11., true, Align::Center, false, Col::Ink); }
    let vs = value_text(c, reading_txt);
    if !vs.is_empty() { v.text(c.x + vo[0], b[3] + 10. + vo[1], &vs, 11., false, Align::Center, false, Col::Ink); }
    for p in c.pins() { v.begin(); v.circle(p.0, p.1, 2.5); v.fill(Col::Wire); }
    v.prims
}

/// Solder dots: points where more than two wire ends / pins meet, or a wire end touches a wire's interior.
pub fn junctions(p: &Project) -> Vec<Pt> {
    use std::collections::HashMap;
    let mut d: HashMap<(u64, u64), (Pt, u32)> = HashMap::new();
    let mut order: Vec<(u64, u64)> = vec![];
    fn add(d: &mut HashMap<(u64, u64), (Pt, u32)>, order: &mut Vec<(u64, u64)>, q: Pt, n: u32) {
        let k = (((q.0 + 0.0).to_bits()), ((q.1 + 0.0).to_bits()));
        let e = d.entry(k).or_insert_with(|| { order.push(k); (q, 0) }); e.1 += n;
    }
    let ws: Vec<_> = p.w.iter().filter(|w| w.x1 != w.x2 || w.y1 != w.y2).collect();
    for w in &ws { add(&mut d, &mut order, (w.x1, w.y1), 1); add(&mut d, &mut order, (w.x2, w.y2), 1); }
    for c in &p.c { for q in c.pins() { add(&mut d, &mut order, q, 1); } }
    let pts: Vec<Pt> = order.iter().map(|k| d[k].0).collect();
    for w in &ws { for &q in &pts { if w.on_seg(q) && !((q.0 == w.x1 && q.1 == w.y1) || (q.0 == w.x2 && q.1 == w.y2)) { add(&mut d, &mut order, q, 2); } } }
    order.iter().filter(|k| d[*k].1 > 2).map(|k| d[k].0).collect()
}