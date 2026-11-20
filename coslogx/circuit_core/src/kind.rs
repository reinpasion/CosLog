use serde::{Deserialize, Serialize};

macro_rules! kinds {
    ($($v:ident = $s:literal),* $(,)?) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
        pub enum Kind { $(#[serde(rename = $s)] $v),* }
        impl Kind {
            pub const ALL: &'static [Kind] = &[$(Kind::$v),*];
            pub fn code(self) -> &'static str { match self { $(Kind::$v => $s),* } }
        }
    };
}
kinds! {
    Dc="dc", Ac="ac", Gnd="gnd", R="r", L="l", C="c", Tr="tr", Npn="npn", Pnp="pnp",
    Sw="sw", Bt="bt", Vm="vm", Am="am", Om="om", Lg="lg", D="d", Led="led", Zd="zd",
    Cs="cs", Pot="pot", Lamp="lamp", Op="op", Nm="nm", Pm="pm", Os="os", Sp="sp",
    And="and", Or="or", Not="not", Mo="mo", Sg="sg", Sd="sd", Fu="fu", Rl="rl",
    Nand="nand", Nor="nor", Xor="xor", Ldr="ldr", Ntc="ntc", Nb="nb", Dff="dff",
    Reg="reg", Br="br", Sol="sol", Ctr="ctr", Bz="bz", Tm="tm", Jk="jk", Srf="srf", Tf="tf",
}

type P = (f64, f64);
const P2: &[P] = &[(-40., 0.), (40., 0.)];
const SRC: &[P] = &[(0., -40.), (0., 40.)];
const BJT: &[P] = &[(-40., 0.), (20., -40.), (20., 40.)];
const GATE: &[P] = &[(-40., -20.), (-40., 20.), (40., 0.)];
const Q4: &[P] = &[(-40., -20.), (-40., 20.), (40., -20.), (40., 20.)];
const FF3: &[P] = &[(-40., -20.), (-40., 0.), (-40., 20.), (40., -20.), (40., 20.)];

impl Kind {
    pub fn name(self) -> &'static str {
        use Kind::*;
        match self {
            Dc=>"DC Source",Ac=>"AC Source",Gnd=>"Ground",R=>"Resistor",L=>"Inductor",C=>"Capacitor",
            Tr=>"Transformer",Npn=>"NPN",Pnp=>"PNP",Sw=>"Switch",Bt=>"Button",Vm=>"Voltmeter",
            Am=>"Ammeter",Om=>"Ohmmeter",Lg=>"Logic probe",D=>"Diode",Led=>"LED",Zd=>"Zener",
            Cs=>"Current src",Pot=>"Potentiometer",Lamp=>"Lamp",Op=>"Op-amp",Nm=>"NMOS",Pm=>"PMOS",
            Os=>"Oscilloscope",Sp=>"SPDT switch",And=>"AND gate",Or=>"OR gate",Not=>"NOT gate",
            Mo=>"DC Motor",Sg=>"Signal Gen",Sd=>"Schottky",Fu=>"Fuse",Rl=>"Relay",Nand=>"NAND gate",
            Nor=>"NOR gate",Xor=>"XOR gate",Ldr=>"LDR (light)",Ntc=>"NTC thermistor",Nb=>"Push-to-break",
            Dff=>"D flip-flop",Reg=>"Regulator",Br=>"Bridge rectifier",Sol=>"Solar cell",
            Ctr=>"4-bit counter",Bz=>"Buzzer",Tm=>"555 timer",Jk=>"JK flip-flop",Srf=>"SR flip-flop",Tf=>"T flip-flop",
        }
    }
    /// Local pin offsets (before mirror/rotation).
    pub fn pins(self) -> &'static [P] {
        use Kind::*;
        match self {
            Dc|Ac|Cs|Sol|Sg => SRC,
            Gnd|Lg => &[(0., 0.)],
            Tr|Rl|Dff|Br|Tf => Q4,
            Npn|Pnp|Nm|Pm => BJT,
            Pot => &[(-40., 0.), (40., 0.), (0., -40.)],
            Op|And|Or|Not|Nand|Nor|Xor => if self == Not { P2 } else { GATE },
            Os => &[(-40., -20.), (-40., 20.)],
            Sp => &[(-40., 0.), (40., -20.), (40., 20.)],
            Reg => &[(-40., -20.), (0., 40.), (40., -20.)],
            Ctr => &[(-40., -20.), (-40., 20.), (40., -40.), (40., -20.), (40., 20.), (40., 40.)],
            Tm => &[(0., -60.), (0., 60.), (-40., -20.), (-40., 20.), (40., 20.), (40., -20.), (-40., -40.), (-40., 40.)],
            Jk|Srf => FF3,
            _ => P2,
        }
    }
    /// Local bounding box (x0, y0, x1, y1).
    pub fn bbox(self) -> [f64; 4] {
        match self {
            Kind::Tm => [-54., -74., 54., 74.],
            Kind::Os => [-54., -64., 174., 64.],
            _ => {
                let p = self.pins();
                let f = |g: fn(&P) -> f64, m: fn(f64, f64) -> f64, s: f64| p.iter().map(g).fold(s, m);
                [f(|a| a.0, f64::min, f64::MAX) - 14., f(|a| a.1, f64::min, f64::MAX) - 14.,
                 f(|a| a.0, f64::max, f64::MIN) + 14., f(|a| a.1, f64::max, f64::MIN) + 14.]
            }
        }
    }
    /// Default (value, frequency/aux) strings, as in the JS table `T`.
    pub fn defaults(self) -> (Option<&'static str>, Option<&'static str>) {
        use Kind::*;
        match self {
            Dc=>(Some("9"),None),Ac=>(Some("12"),Some("50")),R=>(Some("1k"),None),L=>(Some("100m"),None),
            C=>(Some("10u"),None),Tr=>(Some("2"),None),Zd=>(Some("5.1"),None),Cs=>(Some("10m"),None),
            Pot=>(Some("10k"),Some("50")),Lamp=>(Some("100"),None),Op=>(Some("12"),None),
            Nm|Pm=>(Some("2"),None),Os=>(Some("5m"),Some("auto")),And|Or|Not|Nand|Nor|Xor=>(Some("5"),None),
            Mo=>(Some("12"),None),Sg=>(Some("5"),Some("1k")),Fu=>(Some("1"),None),Rl=>(Some("5"),None),
            Ldr=>(Some("100k"),Some("50")),Ntc=>(Some("10k"),Some("25")),Dff|Reg|Ctr|Jk|Srf|Tf=>(Some("5"),None),
            Sol=>(Some("6"),Some("80")),Bz=>(Some("100"),Some("2k")),Lg=>(Some("2.5"),None),_=>(None,None),
        }
    }
    /// Designator prefix (R1, Q2 ...).
    pub fn prefix(self) -> Option<&'static str> {
        use Kind::*;
        Some(match self {
            Dc|Ac=>"V",Cs=>"I",R=>"R",Pot=>"RV",L=>"L",C=>"C",Tr=>"T",Sw|Sp|Nb=>"SW",Bt=>"BTN",
            D|Sd=>"D",Led=>"LED",Zd=>"DZ",Npn|Pnp=>"Q",Nm|Pm=>"M",Op|And|Or|Not|Nand|Nor|Xor|Reg|Tm=>"U",
            Lamp=>"LP",Mo=>"MOT",Vm=>"VM",Am=>"AM",Om=>"OM",Lg=>"LG",Os=>"OSC",Sg=>"FG",Fu=>"F",Rl=>"K",
            Jk|Srf|Tf|Dff=>"FF",Br=>"BR",Sol=>"PV",Ctr=>"CTR",Bz=>"BZ",Ldr=>"LDR",Ntc=>"TH",
            _=>return None,
        })
    }
    pub fn nonlinear(self) -> bool {
        use Kind::*;
        matches!(self, Npn|Pnp|D|Sd|Led|Zd|Op|Nm|Pm|Br|Sol)
    }
}
