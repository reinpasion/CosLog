//! Per-part editable parameters (port of the JS `SX` table) and the part palette (`GR`, `KW`).
use crate::kind::Kind;

#[derive(Clone, Copy, Debug)]
pub struct Param {
    pub key: &'static str,           // "v" | "f" | "p" | "q" | "s" | "k"
    pub label: &'static str,
    pub unit: &'static str,
    pub def: Option<&'static str>,   // default applied when a part is created
    pub opts: Option<&'static [&'static str]>,
}
const fn p(key: &'static str, label: &'static str, unit: &'static str) -> Param { Param { key, label, unit, def: None, opts: None } }
const fn d(key: &'static str, label: &'static str, unit: &'static str, def: &'static str) -> Param { Param { key, label, unit, def: Some(def), opts: None } }
const GT: &[Param] = &[p("v", "High level", "V")];

pub fn params(k: Kind) -> Vec<Param> {
    use Kind::*;
    match k {
        Dc => vec![p("v", "Voltage", "V"), d("p", "Series R", "Ω", "0")],
        Ac => vec![p("v", "Amplitude", "V"), p("f", "Frequency", "Hz"), d("p", "Offset", "V", "0"), d("q", "Phase", "°", "0"), d("s", "Series R", "Ω", "0")],
        Cs => vec![p("v", "Current", "A")],
        R => vec![p("v", "Resistance", "Ω"), d("p", "Tolerance", "%", "0"), d("q", "Power rating", "W", "0.25")],
        C => vec![p("v", "Capacitance", "F"), d("p", "Tolerance", "%", "0"), d("q", "Initial V", "V", "0"), d("s", "Voltage rating", "V", "50")],
        L => vec![p("v", "Inductance", "H"), d("p", "Tolerance", "%", "0"), d("q", "Initial I", "A", "0")],
        Tr => vec![p("v", "Turns ratio", ":1"), d("p", "Coupling k", "", "0.995")],
        Pot => vec![p("v", "Resistance", "Ω"), p("f", "Wiper", "%")],
        Lamp => vec![p("v", "Resistance", "Ω"), d("p", "Full-bright power", "W", "0.5")],
        Zd => vec![p("v", "Zener voltage", "V")],
        Led => vec![Param { key: "p", label: "Color", unit: "", def: Some("red"), opts: Some(&["red", "yellow", "green", "blue", "white"]) }],
        Npn | Pnp => vec![d("p", "Gain (hFE)", "", "100")],
        Nm | Pm => vec![p("v", "Threshold", "V"), d("p", "Gain K", "A/V²", "0.01")],
        Op => vec![p("v", "Supply rail", "V"), d("p", "Open-loop gain", "", "10k")],
        And | Or | Not | Nand | Nor | Xor | Dff | Jk | Srf | Tf | Ctr => GT.to_vec(),
        Mo => vec![p("v", "Rated voltage", "V"), d("p", "Armature R", "Ω", "2")],
        Lg => vec![p("v", "Threshold", "V")],
        Os => vec![p("v", "Time/div", "s"), p("f", "Volts/div", "V")],
        Sg => vec![Param { key: "k", label: "Waveform", unit: "", def: Some("sine"), opts: Some(&["sine", "square", "triangle", "sawtooth"]) },
                p("v", "Amplitude", "V"), p("f", "Frequency", "Hz"), d("p", "Offset", "V", "0"), d("q", "Duty", "%", "50"), d("s", "Series R", "Ω", "0")],
        Fu => vec![p("v", "Rating", "A")],
        Rl => vec![p("v", "Pull-in voltage", "V"), d("p", "Coil R", "Ω", "100")],
        Ldr => vec![p("v", "Dark resistance", "Ω"), p("f", "Light", "%")],
        Ntc => vec![p("v", "R at 25°C", "Ω"), p("f", "Temperature", "°C")],
        Reg => vec![p("v", "Output voltage", "V")],
        Sol => vec![p("v", "Open-circuit V", "V"), d("p", "Short-circuit I", "A", "0.1"), p("f", "Light", "%")],
        Bz => vec![p("v", "Coil resistance", "Ω"), p("f", "Tone", "Hz")],
        _ => vec![],
    }
}
/// Parts whose `f` is a live slider (adjustable while the simulation runs).
pub fn live_slider(k: Kind) -> Option<(f64, f64)> {
    match k { Kind::Pot | Kind::Ldr | Kind::Sol => Some((0.0, 100.0)), Kind::Ntc => Some((-20.0, 150.0)), _ => None }
}

pub const GROUPS: &[(&str, &[Kind])] = {
    use Kind::*;
    &[("Sources", &[Dc, Ac, Sg, Cs, Sol, Gnd]), ("Passive", &[R, Pot, Ldr, Ntc, L, C, Tr, Fu]), ("Switches", &[Sw, Bt, Nb, Sp, Rl]),
      ("Diodes", &[D, Sd, Led, Zd, Br]), ("Transistors", &[Npn, Pnp, Nm, Pm]),
      ("ICs & logic", &[Op, Reg, And, Or, Not, Nand, Nor, Xor, Dff, Jk, Tf, Srf, Ctr, Tm]),
      ("Loads", &[Lamp, Mo, Bz]), ("Instruments", &[Vm, Am, Om, Lg, Os])]
};

pub fn keywords(k: Kind) -> &'static str {
    use Kind::*;
    match k {
        Dc => "battery cell supply power", Ac => "mains generator sine", Sg => "function generator clock pulse square triangle", Cs => "current source",
        Sol => "solar photovoltaic panel", Gnd => "ground earth reference 0v", R => "resistor ohm", Pot => "potentiometer variable trimmer volume",
        Ldr => "photoresistor light sensor", Ntc => "thermistor temperature sensor", L => "coil choke inductor henry", C => "capacitor farad",
        Tr => "transformer", Fu => "fuse protection", Sw => "switch toggle", Bt => "button push momentary", Nb => "normally closed push break",
        Sp => "spdt changeover", Rl => "relay coil contact", D => "diode rectifier", Sd => "schottky", Led => "light emitting indicator",
        Zd => "zener regulator reference", Br => "bridge rectifier full wave", Npn | Pnp => "transistor bjt", Nm | Pm => "mosfet fet transistor",
        Op => "opamp operational amplifier comparator", Reg => "voltage regulator 7805 ldo", And | Or | Not | Nand | Nor | Xor => "gate logic",
        Dff => "flip flop latch memory divider", Ctr => "counter binary digital", Lamp => "bulb light", Mo => "motor", Bz => "buzzer speaker sound beep",
        Vm => "voltmeter meter", Am => "ammeter meter current", Om => "ohmmeter resistance meter", Lg => "logic probe", Os => "scope oscilloscope waveform",
        Jk => "flip flop latch memory toggle", Tf => "flip flop toggle divider frequency", Srf => "flip flop latch set reset memory",
        Tm => "555 ne555 timer astable monostable oscillator pulse blink",
    }
}

/// Palette search: every whitespace-separated word must occur in name + code + keywords.
pub fn matches(k: Kind, q: &str) -> bool {
    let hay = format!("{} {} {}", k.name(), k.code(), keywords(k)).to_lowercase();
    q.to_lowercase().split_whitespace().all(|w| hay.contains(w))
}
pub fn search(q: &str) -> Vec<(&'static str, Vec<Kind>)> {
    GROUPS.iter().map(|(g, ks)| (*g, ks.iter().copied().filter(|k| matches(*k, q)).collect::<Vec<_>>())).filter(|(_, v)| !v.is_empty()).collect()
}
