//! Insert effects: a chain of simple, solid built-in plugins per mixer insert
//! (Filter, 3-band EQ, Compressor, Reverb, Delay, Chorus, Distortion).
use crate::effects::{saturate, Biquad, Compressor, Svf};
use crate::fx_ext::{self as ext, Stereo};
use serde::{Deserialize, Serialize};
use std::f32::consts::TAU;

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum FxKind {
    Filter,
    Eq3,
    Compressor,
    Reverb,
    Delay,
    Chorus,
    Distortion,
    Limiter,
    Clipper,
    Saturator,
    Bitcrusher,
    Flanger,
    Phaser,
    Tremolo,
    AutoPan,
    Gate,
    Widener,
    Utility,
    Exciter,
    ParaEq,
    ViHalla,
    ViGross,
}

pub const ALL: [FxKind; 22] = [
    FxKind::Filter, FxKind::Eq3, FxKind::Compressor, FxKind::Reverb, FxKind::Delay, FxKind::Chorus, FxKind::Distortion,
    FxKind::Limiter, FxKind::Clipper, FxKind::Saturator, FxKind::Bitcrusher, FxKind::Flanger, FxKind::Phaser, FxKind::Tremolo,
    FxKind::AutoPan, FxKind::Gate, FxKind::Widener, FxKind::Utility, FxKind::Exciter, FxKind::ParaEq,
    FxKind::ViHalla, FxKind::ViGross,
];

/// The 20 Essentials (everything except the Vinteria signature plugins).
pub fn essentials() -> impl Iterator<Item = FxKind> {
    ALL.into_iter().filter(|k| !k.is_signature())
}

/// number of parameters in a slot: `p` (4) + `x` (8)
pub const MAX_PARAMS: usize = 12;

/// (name, min, max, default)
pub type ParamSpec = (&'static str, f32, f32, f32);

impl FxKind {
    pub fn name(self) -> &'static str {
        match self {
            FxKind::Filter => "Filter",
            FxKind::Eq3 => "EQ Three",
            FxKind::Compressor => "Compressor",
            FxKind::Reverb => "Reverb",
            FxKind::Delay => "Delay",
            FxKind::Chorus => "Chorus",
            FxKind::Distortion => "Distortion",
            FxKind::Limiter => "Limiter",
            FxKind::Clipper => "Clipper",
            FxKind::Saturator => "Saturator",
            FxKind::Bitcrusher => "Bitcrusher",
            FxKind::Flanger => "Flanger",
            FxKind::Phaser => "Phaser",
            FxKind::Tremolo => "Tremolo",
            FxKind::AutoPan => "Auto Pan",
            FxKind::Gate => "Gate",
            FxKind::Widener => "Stereo Widener",
            FxKind::Utility => "Utility",
            FxKind::Exciter => "Exciter",
            FxKind::ParaEq => "Parametric EQ",
            FxKind::ViHalla => "ViHalla",
            FxKind::ViGross => "ViGross",
        }
    }

    pub fn is_signature(self) -> bool {
        matches!(self, FxKind::ViHalla | FxKind::ViGross)
    }

    /// Short category label shown in the plugin header.
    pub fn category(self) -> &'static str {
        match self {
            FxKind::Filter | FxKind::Eq3 | FxKind::ParaEq => "EQ / Filter",
            FxKind::Compressor | FxKind::Limiter | FxKind::Gate => "Dynamics",
            FxKind::Reverb | FxKind::Delay | FxKind::ViHalla => "Space",
            FxKind::Chorus | FxKind::Flanger | FxKind::Phaser | FxKind::Tremolo | FxKind::AutoPan => "Modulation",
            FxKind::Distortion | FxKind::Clipper | FxKind::Saturator | FxKind::Bitcrusher | FxKind::Exciter => "Colour",
            FxKind::Widener | FxKind::Utility => "Utility",
            FxKind::ViGross => "Time / Volume",
        }
    }

    /// Named factory presets: `(name, full parameter set)`.
    pub fn presets(self) -> Vec<(&'static str, [f32; MAX_PARAMS])> {
        let base = FxSlot::new(self).all();
        let with = |set: &[(usize, f32)]| {
            let mut v = base;
            for &(i, x) in set {
                v[i] = x;
            }
            v
        };
        let mut out = vec![("Default", base)];
        match self {
            FxKind::ViHalla => out.extend([
                ("Small Room", with(&[(0, 0.0), (1, 0.3), (4, 0.2)])),
                ("Big Hall", with(&[(0, 1.0), (1, 0.8), (4, 0.3)])),
                ("Dream Shimmer", with(&[(0, 9.0), (1, 0.85), (6, 0.7), (4, 0.4)])),
                ("Frozen Space", with(&[(0, 11.0), (7, 1.0), (4, 0.5)])),
                ("Ducked Plate", with(&[(0, 2.0), (8, 0.8), (4, 0.35)])),
                ("Dual Cathedral", with(&[(0, 4.0), (9, 10.0), (10, 0.5), (4, 0.35)])),
            ]),
            FxKind::ViGross => out.extend([
                ("Stutter Cut", with(&[(2, 0.6), (3, 0.12), (4, 0.3)])),
                ("Reverse Wash", with(&[(1, 0.7), (4, 0.0), (7, 0.8)])),
                ("Trap Gate", with(&[(0, 0.5), (4, 1.0), (5, 1.0)])),
            ]),
            FxKind::Limiter => out.extend([("Loud", with(&[(1, 9.0), (0, -0.5)])), ("Gentle", with(&[(1, 3.0), (2, 200.0)]))]),
            FxKind::Saturator => out.extend([("Warm Tape", with(&[(0, 6.0), (1, 0.0)])), ("Hot Tube", with(&[(0, 14.0), (1, 1.0)]))]),
            FxKind::Reverb => out.extend([("Tight", with(&[(0, 0.3), (2, 0.15)])), ("Wash", with(&[(0, 0.95), (2, 0.5)]))]),
            FxKind::Delay => out.extend([("Slap", with(&[(0, 0.125), (1, 0.15)])), ("Dotted Echo", with(&[(0, 0.75), (1, 0.55)]))]),
            FxKind::Filter => out.extend([("Low Pass Dark", with(&[(0, 0.0), (1, 800.0), (2, 0.2)])), ("High Pass Thin", with(&[(0, 1.0), (1, 300.0), (2, 0.15)])), ("Band Radio", with(&[(0, 2.0), (1, 1800.0), (2, 0.6)]))]),
            FxKind::Eq3 => out.extend([("Smile", with(&[(0, 4.0), (1, -3.0), (2, 4.0)])), ("Bass Boost", with(&[(0, 7.0)])), ("Vocal Air", with(&[(0, -3.0), (1, 2.0), (2, 5.0), (3, 2500.0)]))]),
            FxKind::Compressor => out.extend([("Glue", with(&[(0, -18.0), (1, 2.0), (2, 2.0)])), ("Punch", with(&[(0, -20.0), (1, 6.0), (2, 5.0)])), ("Squash", with(&[(0, -30.0), (1, 12.0), (2, 9.0)]))]),
            FxKind::Chorus => out.extend([("Subtle", with(&[(1, 0.25), (2, 0.25)])), ("Lush", with(&[(0, 0.5), (1, 0.8), (2, 0.55), (3, 1.0)]))]),
            FxKind::Distortion => out.extend([("Warm Drive", with(&[(0, 0.25), (1, 4000.0)])), ("Fuzz", with(&[(0, 0.9), (1, 3000.0), (3, -4.0)]))]),
            FxKind::Clipper => out.extend([("Hard", with(&[(0, -10.0), (1, 0.0)])), ("Soft", with(&[(0, -6.0), (1, 0.8)]))]),
            FxKind::Bitcrusher => out.extend([("Lo-Fi", with(&[(0, 10.0), (1, 3.0), (2, 0.5)])), ("8-Bit", with(&[(0, 6.0), (1, 8.0), (2, 1.0)]))]),
            FxKind::Flanger => out.extend([("Jet", with(&[(0, 0.15), (1, 1.0), (2, 0.8)])), ("Soft Sweep", with(&[(0, 0.5), (1, 0.4), (2, 0.2)]))]),
            FxKind::Phaser => out.extend([("Slow Swirl", with(&[(0, 0.2), (1, 0.9), (2, 0.6)])), ("Fast Phase", with(&[(0, 2.5), (1, 0.6)]))]),
            FxKind::Tremolo => out.extend([("Vintage", with(&[(0, 4.5), (1, 0.5)])), ("Chopper", with(&[(0, 10.0), (1, 1.0), (2, 1.0)]))]),
            FxKind::AutoPan => out.extend([("Slow Sweep", with(&[(0, 0.2), (1, 1.0)])), ("Square Ping", with(&[(0, 2.0), (2, 1.0)]))]),
            FxKind::Gate => out.extend([("Tight", with(&[(0, -30.0), (2, 60.0)])), ("Drum Gate", with(&[(0, -24.0), (1, 0.5), (2, 90.0), (3, -80.0)]))]),
            FxKind::Widener => out.extend([("Wide", with(&[(0, 1.6)])), ("Mono Bass", with(&[(0, 1.3), (3, 250.0)])), ("Narrow", with(&[(0, 0.6)]))]),
            FxKind::Utility => out.extend([("Mono", with(&[(3, 1.0)])), ("-6 dB", with(&[(0, -6.0)]))]),
            FxKind::Exciter => out.extend([("Presence", with(&[(0, 3500.0), (1, 0.4)])), ("Sparkle", with(&[(0, 8000.0), (1, 0.7)]))]),
            FxKind::ParaEq => out.extend([("Mud Cut", with(&[(2, 300.0), (3, -4.0)])), ("Air Lift", with(&[(4, 10000.0), (5, 4.0)])), ("Bass Weight", with(&[(0, 80.0), (1, 5.0)]))]),
            _ => {}
        }
        out
    }

    /// the first four parameters (`FxSlot::p`)
    pub fn specs(self) -> [ParamSpec; 4] {
        let f = self.specs_full();
        [f[0], f[1], f[2], f[3]]
    }

    /// every parameter of the plugin (4..=12)
    pub fn specs_full(self) -> Vec<ParamSpec> {
        match self {
            FxKind::Limiter => vec![("Ceiling dB", -12.0, 0.0, -0.3), ("Gain dB", 0.0, 18.0, 0.0), ("Release ms", 10.0, 500.0, 80.0), ("Mix", 0.0, 1.0, 1.0)],
            FxKind::Clipper => vec![("Threshold dB", -48.0, 0.0, -6.0), ("Softness", 0.0, 1.0, 0.3), ("Out dB", -12.0, 6.0, 0.0), ("Mix", 0.0, 1.0, 1.0)],
            FxKind::Saturator => vec![("Drive dB", 0.0, 30.0, 8.0), ("Type Tape/Tube/Fold", 0.0, 2.0, 0.0), ("Tone Hz", 800.0, 18000.0, 9000.0), ("Mix", 0.0, 1.0, 1.0), ("Out dB", -12.0, 6.0, 0.0)],
            FxKind::Bitcrusher => vec![("Bits", 1.0, 16.0, 8.0), ("Downsample", 1.0, 40.0, 4.0), ("Mix", 0.0, 1.0, 0.5), ("Out dB", -12.0, 6.0, 0.0)],
            FxKind::Flanger => vec![("Rate Hz", 0.05, 5.0, 0.3), ("Depth", 0.0, 1.0, 0.7), ("Feedback", -0.9, 0.9, 0.5), ("Mix", 0.0, 1.0, 0.5), ("Delay ms", 0.5, 8.0, 2.5)],
            FxKind::Phaser => vec![("Rate Hz", 0.05, 5.0, 0.4), ("Depth", 0.0, 1.0, 0.8), ("Feedback", -0.9, 0.9, 0.4), ("Mix", 0.0, 1.0, 0.5)],
            FxKind::Tremolo => vec![("Rate Hz", 0.1, 20.0, 5.0), ("Depth", 0.0, 1.0, 0.6), ("Shape Sin/Sq", 0.0, 1.0, 0.0), ("Stereo", 0.0, 1.0, 0.0)],
            FxKind::AutoPan => vec![("Rate Hz", 0.05, 10.0, 0.5), ("Depth", 0.0, 1.0, 0.8), ("Shape Sin/Sq", 0.0, 1.0, 0.0), ("Phase", 0.0, 1.0, 0.0)],
            FxKind::Gate => vec![("Threshold dB", -70.0, 0.0, -40.0), ("Attack ms", 0.1, 50.0, 1.0), ("Release ms", 5.0, 800.0, 120.0), ("Floor dB", -90.0, 0.0, -60.0)],
            FxKind::Widener => vec![("Width", 0.0, 2.0, 1.2), ("Mid dB", -12.0, 12.0, 0.0), ("Side dB", -12.0, 12.0, 0.0), ("Mono below Hz", 20.0, 500.0, 120.0)],
            FxKind::Utility => vec![("Gain dB", -36.0, 24.0, 0.0), ("Pan", -1.0, 1.0, 0.0), ("Invert Phase", 0.0, 1.0, 0.0), ("Mono", 0.0, 1.0, 0.0)],
            FxKind::Exciter => vec![("Freq Hz", 1000.0, 12000.0, 4000.0), ("Amount", 0.0, 1.0, 0.5), ("Mix", 0.0, 1.0, 1.0), ("Out dB", -12.0, 6.0, 0.0)],
            FxKind::ParaEq => vec![("Low Hz", 30.0, 400.0, 120.0), ("Low dB", -15.0, 15.0, 0.0), ("Mid Hz", 200.0, 6000.0, 1200.0), ("Mid dB", -15.0, 15.0, 0.0), ("High Hz", 2000.0, 18000.0, 8000.0), ("High dB", -15.0, 15.0, 0.0), ("Q", 0.3, 6.0, 1.0)],
            FxKind::ViHalla => vec![
                ("Mode A", 0.0, 11.0, 1.0), ("Decay", 0.0, 1.0, 0.6), ("Damping", 0.0, 1.0, 0.4), ("Pre-delay ms", 0.0, 200.0, 20.0),
                ("Mix", 0.0, 1.0, 0.3), ("Width", 0.0, 1.0, 1.0), ("Shimmer", 0.0, 1.0, 0.0), ("Freeze", 0.0, 1.0, 0.0),
                ("Ducking", 0.0, 1.0, 0.0), ("Mode B", 0.0, 11.0, 4.0), ("Dual A/B", 0.0, 1.0, 0.0), ("Hi-cut Hz", 1000.0, 20000.0, 14000.0),
            ],
            FxKind::ViGross => vec![
                ("Slice beats", 0.125, 2.0, 0.5), ("Reverse", 0.0, 1.0, 0.0), ("Stutter", 0.0, 1.0, 0.0), ("Stutter len", 0.02, 0.5, 0.125),
                ("Vol depth", 0.0, 1.0, 0.0), ("Pattern Morph A>B", 0.0, 1.0, 0.0), ("Smooth ms", 0.5, 50.0, 5.0), ("Mix", 0.0, 1.0, 1.0),
            ],
            _ => self.legacy_specs().to_vec(),
        }
    }

    fn legacy_specs(self) -> [ParamSpec; 4] {
        match self {
            FxKind::Filter => [("Type LP/HP/BP", 0.0, 2.0, 0.0), ("Cutoff Hz", 40.0, 16000.0, 2000.0), ("Resonance", 0.0, 1.0, 0.3), ("Mix", 0.0, 1.0, 1.0)],
            FxKind::Eq3 => [("Low dB", -15.0, 15.0, 0.0), ("Mid dB", -15.0, 15.0, 0.0), ("High dB", -15.0, 15.0, 0.0), ("Mid Hz", 200.0, 5000.0, 1000.0)],
            FxKind::Compressor => [("Threshold dB", -40.0, 0.0, -14.0), ("Ratio", 1.0, 20.0, 4.0), ("Makeup dB", 0.0, 18.0, 3.0), ("Mix", 0.0, 1.0, 1.0)],
            FxKind::Reverb => [("Size", 0.0, 1.0, 0.6), ("Damping", 0.0, 1.0, 0.5), ("Mix", 0.0, 1.0, 0.25), ("Width", 0.0, 1.0, 1.0)],
            FxKind::Delay => [("Beats", 0.0625, 2.0, 0.75), ("Feedback", 0.0, 0.95, 0.4), ("Mix", 0.0, 1.0, 0.3), ("Ping-pong", 0.0, 1.0, 0.5)],
            FxKind::Chorus => [("Rate Hz", 0.1, 5.0, 0.8), ("Depth", 0.0, 1.0, 0.5), ("Mix", 0.0, 1.0, 0.4), ("Spread", 0.0, 1.0, 0.7)],
            FxKind::Distortion => [("Drive", 0.0, 1.0, 0.4), ("Tone Hz", 500.0, 12000.0, 6000.0), ("Mix", 0.0, 1.0, 1.0), ("Out dB", -12.0, 6.0, 0.0)],
            _ => unreachable!("handled by specs_full"),
        }
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct FxSlot {
    #[serde(default)]
    pub empty: bool,
    #[serde(default = "full_wet")]
    pub wet: f32,
    pub kind: FxKind,
    pub on: bool,
    pub p: [f32; 4],
    /// parameters 5..12 of the larger plugins (older projects have none)
    #[serde(default)]
    pub x: [f32; 8],
}

fn full_wet() -> f32 { 1.0 }

impl FxSlot {
    pub fn new(kind: FxKind) -> Self {
        let s = kind.specs_full();
        let mut v = [0.0f32; MAX_PARAMS];
        for (i, sp) in s.iter().enumerate() {
            v[i] = sp.3;
        }
        FxSlot { empty: false, wet: 1.0, kind, on: true, p: [v[0], v[1], v[2], v[3]], x: [v[4], v[5], v[6], v[7], v[8], v[9], v[10], v[11]] }
    }

    /// all parameters as one array
    pub fn all(&self) -> [f32; MAX_PARAMS] {
        let mut a = [0.0; MAX_PARAMS];
        a[..4].copy_from_slice(&self.p);
        a[4..].copy_from_slice(&self.x);
        a
    }

    pub fn set_all(&mut self, a: &[f32; MAX_PARAMS]) {
        self.p.copy_from_slice(&a[..4]);
        self.x.copy_from_slice(&a[4..]);
    }

    pub fn get(&self, i: usize) -> f32 {
        if i < 4 { self.p[i] } else { self.x[i - 4] }
    }

    pub fn set(&mut self, i: usize, v: f32) {
        if i < 4 { self.p[i] = v } else { self.x[i - 4] = v }
    }

    pub fn empty() -> Self {
        Self { empty: true, on: false, ..Self::new(FxKind::Filter) }
    }
}

// ------------------------------------------------------------------ runtime state

struct Comb {
    buf: Vec<f32>,
    idx: usize,
    store: f32,
}

impl Comb {
    fn new(n: usize) -> Self {
        Comb { buf: vec![0.0; n.max(2)], idx: 0, store: 0.0 }
    }
    #[inline]
    fn run(&mut self, x: f32, fb: f32, damp: f32) -> f32 {
        let out = self.buf[self.idx];
        self.store = out * (1.0 - damp) + self.store * damp;
        self.buf[self.idx] = x + self.store * fb;
        self.idx += 1;
        if self.idx >= self.buf.len() {
            self.idx = 0;
        }
        out
    }
}

struct Allpass {
    buf: Vec<f32>,
    idx: usize,
}

impl Allpass {
    fn new(n: usize) -> Self {
        Allpass { buf: vec![0.0; n.max(2)], idx: 0 }
    }
    #[inline]
    fn run(&mut self, x: f32) -> f32 {
        let b = self.buf[self.idx];
        let out = b - x;
        self.buf[self.idx] = x + b * 0.5;
        self.idx += 1;
        if self.idx >= self.buf.len() {
            self.idx = 0;
        }
        out
    }
}

/// Freeverb-style reverb.
pub struct Reverb {
    cl: Vec<Comb>,
    cr: Vec<Comb>,
    al: Vec<Allpass>,
    ar: Vec<Allpass>,
}

impl Reverb {
    fn new(sr: f32) -> Self {
        let k = sr / 44100.0;
        let sz = |n: usize| (n as f32 * k) as usize;
        let combs = [1116usize, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
        let aps = [556usize, 441, 341, 225];
        Reverb {
            cl: combs.iter().map(|&n| Comb::new(sz(n))).collect(),
            cr: combs.iter().map(|&n| Comb::new(sz(n + 23))).collect(),
            al: aps.iter().map(|&n| Allpass::new(sz(n))).collect(),
            ar: aps.iter().map(|&n| Allpass::new(sz(n + 23))).collect(),
        }
    }
    #[inline]
    fn run(&mut self, l: f32, r: f32, p: &[f32; 4]) -> (f32, f32) {
        let fb = 0.7 + p[0].clamp(0.0, 1.0) * 0.28;
        let damp = p[1].clamp(0.0, 1.0) * 0.4;
        let x = (l + r) * 0.5 * 0.015;
        let (mut wl, mut wr) = (0.0, 0.0);
        for c in self.cl.iter_mut() {
            wl += c.run(x, fb, damp);
        }
        for c in self.cr.iter_mut() {
            wr += c.run(x, fb, damp);
        }
        for a in self.al.iter_mut() {
            wl = a.run(wl);
        }
        for a in self.ar.iter_mut() {
            wr = a.run(wr);
        }
        let w = p[3].clamp(0.0, 1.0);
        let (a, b) = (0.5 + w * 0.5, 0.5 - w * 0.5);
        let (ol, or) = ((wl * a + wr * b) * 3.0, (wr * a + wl * b) * 3.0);
        let m = p[2].clamp(0.0, 1.0);
        (l * (1.0 - m) + ol * m, r * (1.0 - m) + or * m)
    }
}

pub struct StereoDelay {
    l: Vec<f32>,
    r: Vec<f32>,
    w: usize,
}

impl StereoDelay {
    fn new(sr: f32) -> Self {
        let n = (sr * 2.2) as usize;
        StereoDelay { l: vec![0.0; n], r: vec![0.0; n], w: 0 }
    }
    #[inline]
    fn run(&mut self, l: f32, r: f32, p: &[f32; 4], bpm: f32, sr: f32) -> (f32, f32) {
        let n = self.l.len();
        let d = ((p[0] * 60.0 / bpm * sr) as usize).clamp(1, n - 1);
        let rp = (self.w + n - d) % n;
        let (dl, dr) = (self.l[rp], self.r[rp]);
        let (fb, pp) = (p[1].clamp(0.0, 0.95), p[3].clamp(0.0, 1.0));
        self.l[self.w] = l + fb * (dl * (1.0 - pp) + dr * pp);
        self.r[self.w] = r + fb * (dr * (1.0 - pp) + dl * pp);
        self.w = (self.w + 1) % n;
        let m = p[2].clamp(0.0, 1.0);
        (l + dl * m, r + dr * m)
    }
}

pub struct ChorusFx {
    l: Vec<f32>,
    r: Vec<f32>,
    w: usize,
    ph: f32,
}

impl ChorusFx {
    fn new(sr: f32) -> Self {
        let n = (sr * 0.08) as usize + 4;
        ChorusFx { l: vec![0.0; n], r: vec![0.0; n], w: 0, ph: 0.0 }
    }
    #[inline]
    fn tap(buf: &[f32], w: usize, delay: f32) -> f32 {
        let n = buf.len();
        let pos = (w as f32 - delay).rem_euclid(n as f32);
        let i = pos as usize % n;
        let f = pos - pos.floor();
        buf[i] * (1.0 - f) + buf[(i + 1) % n] * f
    }
    #[inline]
    fn run(&mut self, l: f32, r: f32, p: &[f32; 4], sr: f32) -> (f32, f32) {
        let n = self.l.len();
        self.l[self.w] = l;
        self.r[self.w] = r;
        self.ph = (self.ph + p[0] / sr).fract();
        let depth = p[1].clamp(0.0, 1.0) * 0.006 * sr;
        let base = 0.010 * sr;
        let off = p[3].clamp(0.0, 1.0) * 0.5;
        let dl = base + depth * (self.ph * TAU).sin();
        let dr = base + depth * ((self.ph + off) * TAU).sin();
        let (wl, wr) = (Self::tap(&self.l, self.w, dl), Self::tap(&self.r, self.w, dr));
        self.w = (self.w + 1) % n;
        let m = p[2].clamp(0.0, 1.0);
        (l * (1.0 - m * 0.5) + wl * m, r * (1.0 - m * 0.5) + wr * m)
    }
}

pub enum FxRun {
    Filter([Svf; 2]),
    Eq { lo: [Biquad; 2], mid: [Biquad; 2], hi: [Biquad; 2], last: [f32; 4] },
    Comp(Compressor),
    Reverb(Box<Reverb>),
    Delay(Box<StereoDelay>),
    Chorus(Box<ChorusFx>),
    Dist([f32; 2]),
    Dyn(Box<dyn Stereo>),
}

impl FxRun {
    fn new(kind: FxKind, sr: f32) -> FxRun {
        match kind {
            FxKind::Filter => FxRun::Filter([Svf::default(); 2]),
            FxKind::Eq3 => FxRun::Eq { lo: [Biquad::default(); 2], mid: [Biquad::default(); 2], hi: [Biquad::default(); 2], last: [f32::NAN; 4] },
            FxKind::Compressor => FxRun::Comp(Compressor::new()),
            FxKind::Reverb => FxRun::Reverb(Box::new(Reverb::new(sr))),
            FxKind::Delay => FxRun::Delay(Box::new(StereoDelay::new(sr))),
            FxKind::Chorus => FxRun::Chorus(Box::new(ChorusFx::new(sr))),
            FxKind::Distortion => FxRun::Dist([0.0; 2]),
            FxKind::Limiter => FxRun::Dyn(Box::new(ext::Limit::new())),
            FxKind::Clipper => FxRun::Dyn(Box::new(ext::Clip)),
            FxKind::Saturator => FxRun::Dyn(Box::new(ext::Saturator::new())),
            FxKind::Bitcrusher => FxRun::Dyn(Box::new(ext::Crusher::new())),
            FxKind::Flanger => FxRun::Dyn(Box::new(ext::Flanger::new(sr))),
            FxKind::Phaser => FxRun::Dyn(Box::new(ext::Phaser::new())),
            FxKind::Tremolo => FxRun::Dyn(Box::new(ext::Tremolo::new())),
            FxKind::AutoPan => FxRun::Dyn(Box::new(ext::AutoPan::new())),
            FxKind::Gate => FxRun::Dyn(Box::new(ext::Gate::new())),
            FxKind::Widener => FxRun::Dyn(Box::new(ext::Widener::new())),
            FxKind::Utility => FxRun::Dyn(Box::new(ext::Utility)),
            FxKind::Exciter => FxRun::Dyn(Box::new(ext::Exciter::new())),
            FxKind::ParaEq => FxRun::Dyn(Box::new(ext::ParaEq::new())),
            FxKind::ViHalla => FxRun::Dyn(Box::new(ext::ViHalla::new(sr))),
            FxKind::ViGross => FxRun::Dyn(Box::new(ext::ViGross::new(sr))),
        }
    }
}

/// The effect chain of one insert.
pub struct FxChain {
    runs: Vec<(FxKind, FxRun)>,
}

impl FxChain {
    pub fn new() -> Self {
        FxChain { runs: vec![] }
    }

    /// Rebuild the processors when the chain was edited (kinds or order changed).
    pub fn sync(&mut self, slots: &[FxSlot], sr: f32) {
        if self.runs.len() == slots.len() && self.runs.iter().zip(slots).all(|((k, _), s)| *k == s.kind) {
            return;
        }
        self.runs = slots.iter().map(|s| (s.kind, FxRun::new(s.kind, sr))).collect();
    }

    #[inline]
    pub fn process(&mut self, slots: &[FxSlot], mut l: f32, mut r: f32, bpm: f32, sr: f32) -> (f32, f32) {
        for ((_, run), slot) in self.runs.iter_mut().zip(slots) {
            if slot.empty || !slot.on {
                continue;
            }
            let p = &slot.p;
            let (dl, dr) = (l, r);
            match run {
                FxRun::Filter(f) => {
                    let mix = p[3].clamp(0.0, 1.0);
                    let ty = p[0].round() as i32;
                    let mut out = [l, r];
                    for (ch, x) in [l, r].into_iter().enumerate() {
                        let (low, high) = f[ch].process(x, p[1], p[2], sr);
                        out[ch] = match ty {
                            1 => high,
                            2 => x - low - high,
                            _ => low,
                        };
                    }
                    l = dl * (1.0 - mix) + out[0] * mix;
                    r = dr * (1.0 - mix) + out[1] * mix;
                }
                FxRun::Eq { lo, mid, hi, last } => {
                    if *last != *p {
                        for ch in 0..2 {
                            lo[ch].low_shelf(200.0, p[0], sr);
                            mid[ch].peak(p[3], p[1], 0.9, sr);
                            hi[ch].high_shelf(5000.0, p[2], sr);
                        }
                        *last = *p;
                    }
                    l = hi[0].process(mid[0].process(lo[0].process(l)));
                    r = hi[1].process(mid[1].process(lo[1].process(r)));
                }
                FxRun::Comp(c) => {
                    let (cl, cr, _) = c.process(l, r, p[0], p[1], p[2], sr);
                    let mix = p[3].clamp(0.0, 1.0);
                    l = dl * (1.0 - mix) + cl * mix;
                    r = dr * (1.0 - mix) + cr * mix;
                }
                FxRun::Reverb(rv) => {
                    let (a, b) = rv.run(l, r, p);
                    l = a;
                    r = b;
                }
                FxRun::Delay(d) => {
                    let (a, b) = d.run(l, r, p, bpm, sr);
                    l = a;
                    r = b;
                }
                FxRun::Chorus(c) => {
                    let (a, b) = c.run(l, r, p, sr);
                    l = a;
                    r = b;
                }
                FxRun::Dyn(d) => {
                    let mut a = [0.0f32; MAX_PARAMS];
                    a[..4].copy_from_slice(&slot.p);
                    a[4..].copy_from_slice(&slot.x);
                    let (x, y) = d.run(l, r, &a, sr, bpm);
                    l = x;
                    r = y;
                }
                FxRun::Dist(lp) => {
                    let drive = 1.0 + p[0].clamp(0.0, 1.0) * 29.0;
                    let a = 1.0 - (-TAU * p[1] / sr).exp();
                    let mix = p[2].clamp(0.0, 1.0);
                    let gain = 10f32.powf(p[3] / 20.0);
                    let mut out = [l, r];
                    for (ch, x) in [l, r].into_iter().enumerate() {
                        let y = saturate(x, drive);
                        lp[ch] += a * (y - lp[ch]);
                        out[ch] = (x * (1.0 - mix) + lp[ch] * mix) * gain;
                    }
                    l = out[0];
                    r = out[1];
                }
            }
            let wet = slot.wet.clamp(0.0, 1.0);
            l = dl + (l - dl) * wet;
            r = dr + (r - dr) * wet;
        }
        (l, r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_slots_and_zero_wet_leave_audio_unchanged() {
        let mut zero_wet = FxSlot::new(FxKind::Distortion);
        zero_wet.wet = 0.0;
        let mut empty = FxSlot::empty();
        empty.on = true;
        let slots = [empty, zero_wet];
        let mut chain = FxChain::new();
        chain.sync(&slots, 48000.0);
        for i in 0..128 {
            let x = (i as f32 * 0.1).sin() * 0.25;
            assert_eq!(chain.process(&slots, x, -x, 120.0, 48000.0), (x, -x));
        }
        let restored: Vec<FxSlot> = serde_json::from_str(&serde_json::to_string(&slots).unwrap()).unwrap();
        assert!(restored[0].empty);
        assert_eq!(restored[1].wet, 0.0);
    }

    #[test]
    fn legacy_effects_load_as_occupied_and_full_wet() {
        let slot: FxSlot = serde_json::from_str(r#"{"kind":"Filter","on":true,"p":[0.0,2000.0,0.3,1.0]}"#).unwrap();
        assert!(!slot.empty);
        assert_eq!(slot.wet, 1.0);
    }

    #[test]
    fn every_effect_is_finite_and_audible() {
        let sr = 48000.0;
        for kind in ALL {
            let slots = vec![FxSlot::new(kind)];
            let mut ch = FxChain::new();
            ch.sync(&slots, sr);
            let mut energy = 0.0f32;
            for i in 0..sr as usize {
                let x = (i as f32 * 440.0 * TAU / sr).sin() * 0.5;
                let (l, r) = ch.process(&slots, x, x, 120.0, sr);
                assert!(l.is_finite() && r.is_finite() && l.abs() < 8.0, "{kind:?}");
                energy += l * l + r * r;
            }
            assert!(energy > 1.0, "{kind:?} silent");
        }
    }

    #[test]
    fn reverb_has_a_tail() {
        let sr = 48000.0;
        let mut slots = vec![FxSlot::new(FxKind::Reverb)];
        slots[0].p[2] = 1.0;
        let mut ch = FxChain::new();
        ch.sync(&slots, sr);
        for i in 0..2000 {
            ch.process(&slots, if i < 100 { 0.8 } else { 0.0 }, 0.0, 120.0, sr);
        }
        let mut tail = 0.0f32;
        for _ in 0..20000 {
            let (l, _) = ch.process(&slots, 0.0, 0.0, 120.0, sr);
            tail += l.abs();
        }
        assert!(tail > 0.05, "no reverb tail: {tail}");
    }

    fn run_sine(slot: FxSlot, secs: f32) -> Vec<(f32, f32)> {
        let sr = 48000.0;
        let slots = vec![slot];
        let mut ch = FxChain::new();
        ch.sync(&slots, sr);
        (0..(sr * secs) as usize)
            .map(|i| {
                let x = (i as f32 * 220.0 * TAU / sr).sin() * 0.6;
                ch.process(&slots, x, x, 120.0, sr)
            })
            .collect()
    }

    #[test]
    fn twenty_essentials_plus_two_signature_plugins() {
        assert_eq!(essentials().count(), 20);
        assert_eq!(ALL.iter().filter(|k| k.is_signature()).count(), 2);
        for k in ALL {
            assert!((4..=MAX_PARAMS).contains(&k.specs_full().len()), "{k:?}");
            assert!(!k.presets().is_empty());
        }
    }

    #[test]
    fn limiter_never_exceeds_its_ceiling() {
        let mut s = FxSlot::new(FxKind::Limiter);
        s.p[0] = -6.0;
        s.p[1] = 12.0;
        let ceil = 10f32.powf(-6.0 / 20.0);
        for (l, r) in run_sine(s, 0.5) {
            assert!(l.abs() <= ceil + 1e-4 && r.abs() <= ceil + 1e-4);
        }
    }

    #[test]
    fn gate_silences_quiet_signal_and_utility_inverts() {
        let mut g = FxSlot::new(FxKind::Gate);
        g.p[0] = -3.0; // threshold above the signal level
        let out = run_sine(g, 0.5);
        assert!(out.iter().skip(24000).all(|(l, _)| l.abs() < 0.01));
        let mut u = FxSlot::new(FxKind::Utility);
        u.p[2] = 1.0;
        let out = run_sine(u, 0.1);
        let dry = run_sine({ let mut z = FxSlot::new(FxKind::Utility); z.on = false; z }, 0.1);
        assert!((out[500].0 + dry[500].0).abs() < 1e-3);
    }

    #[test]
    fn vigross_reverse_and_stutter_change_the_signal() {
        let dry = run_sine({ let mut s = FxSlot::new(FxKind::ViGross); s.on = false; s }, 1.0);
        for set in [[(1usize, 1.0f32), (2, 0.0)], [(1, 0.0), (2, 1.0)], [(4, 1.0), (1, 0.0)]] {
            let mut s = FxSlot::new(FxKind::ViGross);
            for (i, v) in set {
                s.set(i, v);
            }
            let out = run_sine(s, 1.0);
            let diff: f32 = out.iter().zip(&dry).map(|(a, b)| (a.0 - b.0).abs()).sum();
            assert!(diff > 50.0, "{set:?} diff {diff}");
            assert!(out.iter().all(|(l, r)| l.is_finite() && r.is_finite()));
        }
    }

    #[test]
    fn vihalla_modes_freeze_and_ducking_work() {
        let sr = 48000.0;
        for mode in 0..12 {
            let mut s = FxSlot::new(FxKind::ViHalla);
            s.set(0, mode as f32);
            s.set(4, 1.0);
            let slots = vec![s];
            let mut ch = FxChain::new();
            ch.sync(&slots, sr);
            let mut tail = 0.0;
            for i in 0..60000 {
                let x = if i < 200 { 0.8 } else { 0.0 };
                let (l, r) = ch.process(&slots, x, x, 120.0, sr);
                assert!(l.is_finite() && r.is_finite() && l.abs() < 8.0, "mode {mode}");
                if i > 30000 { tail += l.abs(); }
            }
            assert!(tail > 0.01, "mode {mode} has no tail");
        }
        // freeze holds the tail much longer than a normal decay
        let tail_after = |freeze: f32| {
            let mut s = FxSlot::new(FxKind::ViHalla);
            s.set(1, 0.2);
            s.set(4, 1.0);
            s.set(7, freeze);
            let slots = vec![s];
            let mut ch = FxChain::new();
            ch.sync(&slots, sr);
            let mut t = 0.0;
            for i in 0..200000 {
                let x = if i < 4000 { (i as f32 * 0.05).sin() * 0.8 } else { 0.0 };
                let (l, _) = ch.process(&slots, x, x, 120.0, sr);
                if i > 190000 { t += l.abs(); }
            }
            t
        };
        assert!(tail_after(1.0) > tail_after(0.0) * 5.0);
        // ducking lowers the wet level while the input plays
        let wet_level = |duck: f32| {
            let mut s = FxSlot::new(FxKind::ViHalla);
            s.set(4, 1.0);
            s.set(8, duck);
            let out = run_sine(s, 1.0);
            out.iter().enumerate().skip(20000).map(|(i, (l, _))| (l - 0.5 * (i as f32 * 220.0 * TAU / 48000.0).sin() * 0.6).abs()).sum::<f32>()
        };
        assert!(wet_level(1.0) < wet_level(0.0) * 0.8);
    }
}
