//! Extra built-in processors: the remaining Essentials FX and the Vinteria signature effects (ViHalla, ViGross).
//! Every processor allocates in `new()` only; `run()` is allocation-free and safe for the audio thread.
use crate::effects::{saturate, Biquad};
use std::f32::consts::{PI, TAU};

/// Parameters of one slot, flattened: `a[0..4]` = `FxSlot::p`, `a[4..12]` = `FxSlot::x`.
pub type Params = [f32; 12];

pub trait Stereo: Send {
    fn run(&mut self, l: f32, r: f32, a: &Params, sr: f32, bpm: f32) -> (f32, f32);
}

#[inline]
fn db(x: f32) -> f32 {
    10f32.powf(x / 20.0)
}
#[inline]
fn mixf(dry: f32, wet: f32, m: f32) -> f32 {
    dry + (wet - dry) * m.clamp(0.0, 1.0)
}
#[inline]
fn onepole(coef_hz: f32, sr: f32) -> f32 {
    1.0 - (-TAU * coef_hz / sr).exp()
}

// ------------------------------------------------------------------ dynamics

/// Brickwall limiter with a short look-ahead: the output never exceeds the ceiling.
pub struct Limit {
    buf: Vec<[f32; 2]>,
    pos: usize,
    gain: f32,
}
impl Limit {
    const LA: usize = 64;
    pub fn new() -> Self {
        Limit { buf: vec![[0.0; 2]; Self::LA], pos: 0, gain: 1.0 }
    }
}
impl Stereo for Limit {
    fn run(&mut self, l: f32, r: f32, a: &Params, sr: f32, _: f32) -> (f32, f32) {
        let ceil = db(a[0]);
        let g_in = db(a[1]);
        let (l, r) = (l * g_in, r * g_in);
        self.buf[self.pos] = [l, r];
        let mut peak = 0.0f32;
        for s in &self.buf {
            peak = peak.max(s[0].abs()).max(s[1].abs());
        }
        let target = if peak > ceil { ceil / peak } else { 1.0 };
        let rel = onepole(1000.0 / a[2].max(5.0) / 6.0, sr);
        if target < self.gain {
            self.gain += (target - self.gain) * 0.35;
        } else {
            self.gain += (target - self.gain) * rel;
        }
        self.pos = (self.pos + 1) % Self::LA;
        let d = self.buf[self.pos];
        let o = (d[0] * self.gain, d[1] * self.gain);
        let c = |x: f32| x.clamp(-ceil, ceil);
        (mixf(l, c(o.0), a[3]), mixf(r, c(o.1), a[3]))
    }
}

pub struct Clip;
impl Stereo for Clip {
    fn run(&mut self, l: f32, r: f32, a: &Params, _: f32, _: f32) -> (f32, f32) {
        let t = db(a[0]).max(1e-3);
        let shape = a[1].clamp(0.0, 1.0);
        let out = db(a[2]);
        let f = |x: f32| {
            let hard = x.clamp(-t, t);
            let soft = t * (x / t).tanh();
            (hard + (soft - hard) * shape) * out
        };
        (mixf(l, f(l), a[3]), mixf(r, f(r), a[3]))
    }
}

pub struct Gate {
    env: f32,
    g: f32,
}
impl Gate {
    pub fn new() -> Self {
        Gate { env: 0.0, g: 0.0 }
    }
}
impl Stereo for Gate {
    fn run(&mut self, l: f32, r: f32, a: &Params, sr: f32, _: f32) -> (f32, f32) {
        let lvl = l.abs().max(r.abs());
        self.env += (lvl - self.env) * if lvl > self.env { 0.2 } else { onepole(30.0, sr) };
        let open = self.env > db(a[0]);
        let floor = db(a[3]);
        let target = if open { 1.0 } else { floor };
        let t = if target > self.g { a[1].max(0.1) } else { a[2].max(1.0) };
        self.g += (target - self.g) * onepole(1000.0 / t / 3.0, sr);
        (l * self.g, r * self.g)
    }
}

// ------------------------------------------------------------------ colour

pub struct Saturator {
    lp: [f32; 2],
    dc: [(f32, f32); 2],
}
impl Saturator {
    pub fn new() -> Self {
        Saturator { lp: [0.0; 2], dc: [(0.0, 0.0); 2] }
    }
}
impl Stereo for Saturator {
    fn run(&mut self, l: f32, r: f32, a: &Params, sr: f32, _: f32) -> (f32, f32) {
        let drive = db(a[0]);
        let kind = a[1].round() as i32; // 0 tape, 1 tube, 2 fold
        let k = onepole(a[2], sr);
        let out = db(a[4]);
        let mut res = [l, r];
        for (ch, x) in [l, r].into_iter().enumerate() {
            let y = match kind {
                1 => {
                    let b = 0.25;
                    ((x * drive + b).tanh() - b.tanh()) * 0.9
                }
                2 => {
                    let v = x * drive;
                    (v * 1.2).sin() * 0.8 / drive.max(1.0).sqrt()
                }
                _ => saturate(x, drive) * 0.95,
            };
            // DC blocker (asymmetric tube curves create offset), then tone low-pass
            let (ref mut x1, ref mut y1) = self.dc[ch];
            let dcb = y - *x1 + 0.995 * *y1;
            *x1 = y;
            *y1 = dcb;
            self.lp[ch] += k * (dcb - self.lp[ch]);
            res[ch] = mixf(x, self.lp[ch] * out, a[3]);
        }
        (res[0], res[1])
    }
}

pub struct Crusher {
    hold: [f32; 2],
    cnt: f32,
}
impl Crusher {
    pub fn new() -> Self {
        Crusher { hold: [0.0; 2], cnt: 0.0 }
    }
}
impl Stereo for Crusher {
    fn run(&mut self, l: f32, r: f32, a: &Params, _: f32, _: f32) -> (f32, f32) {
        let levels = 2f32.powf(a[0].clamp(1.0, 16.0) - 1.0);
        self.cnt += 1.0;
        if self.cnt >= a[1].max(1.0) {
            self.cnt -= a[1].max(1.0);
            self.hold = [(l * levels).round() / levels, (r * levels).round() / levels];
        }
        let o = db(a[3]);
        (mixf(l, self.hold[0] * o, a[2]), mixf(r, self.hold[1] * o, a[2]))
    }
}

pub struct Exciter {
    lp: [f32; 2],
}
impl Exciter {
    pub fn new() -> Self {
        Exciter { lp: [0.0; 2] }
    }
}
impl Stereo for Exciter {
    fn run(&mut self, l: f32, r: f32, a: &Params, sr: f32, _: f32) -> (f32, f32) {
        let k = onepole(a[0], sr);
        let o = db(a[3]);
        let mut res = [l, r];
        for (ch, x) in [l, r].into_iter().enumerate() {
            self.lp[ch] += k * (x - self.lp[ch]);
            let high = x - self.lp[ch];
            let harm = (high * (2.0 + a[1] * 10.0)).tanh();
            res[ch] = (x + harm * a[1] * 0.5 * a[2].clamp(0.0, 1.0)) * o;
        }
        (res[0], res[1])
    }
}

// ------------------------------------------------------------------ modulation

fn lfo(ph: f32, shape: f32) -> f32 {
    let s = (ph * TAU).sin();
    let sq = (s * 6.0).tanh();
    s + (sq - s) * shape.clamp(0.0, 1.0)
}

pub struct Tremolo {
    ph: f32,
}
impl Tremolo {
    pub fn new() -> Self {
        Tremolo { ph: 0.0 }
    }
}
impl Stereo for Tremolo {
    fn run(&mut self, l: f32, r: f32, a: &Params, sr: f32, _: f32) -> (f32, f32) {
        self.ph = (self.ph + a[0] / sr).fract();
        let d = a[1].clamp(0.0, 1.0);
        let gl = 1.0 - d * (0.5 + 0.5 * lfo(self.ph, a[2]));
        let gr = 1.0 - d * (0.5 + 0.5 * lfo(self.ph + a[3] * 0.5, a[2]));
        (l * gl, r * gr)
    }
}

pub struct AutoPan {
    ph: f32,
}
impl AutoPan {
    pub fn new() -> Self {
        AutoPan { ph: 0.0 }
    }
}
impl Stereo for AutoPan {
    fn run(&mut self, l: f32, r: f32, a: &Params, sr: f32, _: f32) -> (f32, f32) {
        self.ph = (self.ph + a[0] / sr).fract();
        let p = lfo(self.ph + a[3], a[2]) * a[1].clamp(0.0, 1.0); // -1..1
        let ang = (p * 0.5 + 0.5) * PI * 0.5;
        (l * ang.cos() * std::f32::consts::SQRT_2, r * ang.sin() * std::f32::consts::SQRT_2)
    }
}

pub struct Flanger {
    buf: [Vec<f32>; 2],
    w: usize,
    ph: f32,
    fb: [f32; 2],
}
impl Flanger {
    pub fn new(sr: f32) -> Self {
        let n = (sr * 0.02) as usize + 8;
        Flanger { buf: [vec![0.0; n], vec![0.0; n]], w: 0, ph: 0.0, fb: [0.0; 2] }
    }
}
impl Stereo for Flanger {
    fn run(&mut self, l: f32, r: f32, a: &Params, sr: f32, _: f32) -> (f32, f32) {
        let n = self.buf[0].len();
        self.ph = (self.ph + a[0] / sr).fract();
        let manual = a[4] * 0.001 * sr;
        let depth = a[1].clamp(0.0, 1.0) * manual * 0.9;
        let fb = a[2].clamp(-0.9, 0.9);
        let mut res = [l, r];
        for (ch, x) in [l, r].into_iter().enumerate() {
            let m = (self.ph + ch as f32 * 0.25).fract();
            let d = (manual + depth * (m * TAU).sin()).clamp(1.0, n as f32 - 2.0);
            self.buf[ch][self.w] = x + self.fb[ch] * fb;
            let pos = (self.w as f32 - d).rem_euclid(n as f32);
            let i = pos as usize;
            let f = pos - i as f32;
            let y = self.buf[ch][i % n] * (1.0 - f) + self.buf[ch][(i + 1) % n] * f;
            self.fb[ch] = y;
            res[ch] = x + y * a[3].clamp(0.0, 1.0);
        }
        self.w = (self.w + 1) % n;
        (res[0], res[1])
    }
}

pub struct Phaser {
    ph: f32,
    ap: [[(f32, f32); 6]; 2],
    fb: [f32; 2],
}
impl Phaser {
    pub fn new() -> Self {
        Phaser { ph: 0.0, ap: [[(0.0, 0.0); 6]; 2], fb: [0.0; 2] }
    }
}
impl Stereo for Phaser {
    fn run(&mut self, l: f32, r: f32, a: &Params, sr: f32, _: f32) -> (f32, f32) {
        self.ph = (self.ph + a[0] / sr).fract();
        let fb = a[2].clamp(-0.9, 0.9);
        let mut res = [l, r];
        for (ch, x) in [l, r].into_iter().enumerate() {
            let m = 0.5 + 0.5 * ((self.ph + ch as f32 * 0.1) * TAU).sin();
            let f = 200.0 * (4000.0f32 / 200.0).powf(m * a[1].clamp(0.0, 1.0) + (1.0 - a[1].clamp(0.0, 1.0)) * 0.5);
            let t = (PI * f / sr).tan();
            let c = (t - 1.0) / (t + 1.0);
            let mut s = x + self.fb[ch] * fb;
            for st in self.ap[ch].iter_mut() {
                let y = c * s + st.0 - c * st.1;
                st.0 = s;
                st.1 = y;
                s = y;
            }
            self.fb[ch] = s;
            res[ch] = mixf(x, (x + s) * 0.5, a[3]);
        }
        (res[0], res[1])
    }
}

// ------------------------------------------------------------------ stereo / utility / EQ

pub struct Widener {
    lp: f32,
}
impl Widener {
    pub fn new() -> Self {
        Widener { lp: 0.0 }
    }
}
impl Stereo for Widener {
    fn run(&mut self, l: f32, r: f32, a: &Params, sr: f32, _: f32) -> (f32, f32) {
        let m = (l + r) * 0.5 * db(a[1]);
        let mut s = (l - r) * 0.5 * a[0].clamp(0.0, 2.0) * db(a[2]);
        // keep the side signal below the cutoff mono
        self.lp += onepole(a[3].max(20.0), sr) * (s - self.lp);
        s -= self.lp;
        (m + s, m - s)
    }
}

pub struct Utility;
impl Stereo for Utility {
    fn run(&mut self, l: f32, r: f32, a: &Params, _: f32, _: f32) -> (f32, f32) {
        let g = db(a[0]);
        let pan = a[1].clamp(-1.0, 1.0);
        let ang = (pan * 0.5 + 0.5) * PI * 0.5;
        let (mut l, mut r) = (l * g, r * g);
        if a[3] > 0.5 {
            let m = (l + r) * 0.5;
            l = m;
            r = m;
        }
        if a[2] > 0.5 {
            l = -l;
            r = -r;
        }
        (l * ang.cos() * std::f32::consts::SQRT_2, r * ang.sin() * std::f32::consts::SQRT_2)
    }
}

pub struct ParaEq {
    f: [[Biquad; 3]; 2],
    last: [f32; 7],
}
impl ParaEq {
    pub fn new() -> Self {
        ParaEq { f: [[Biquad::default(); 3]; 2], last: [f32::NAN; 7] }
    }
}
impl Stereo for ParaEq {
    fn run(&mut self, l: f32, r: f32, a: &Params, sr: f32, _: f32) -> (f32, f32) {
        let cur = [a[0], a[1], a[2], a[3], a[4], a[5], a[6]];
        if cur != self.last {
            for ch in 0..2 {
                for b in 0..3 {
                    self.f[ch][b].peak(cur[b * 2].clamp(20.0, sr * 0.45), cur[b * 2 + 1], cur[6].clamp(0.2, 10.0), sr);
                }
            }
            self.last = cur;
        }
        let mut o = [l, r];
        for ch in 0..2 {
            for b in 0..3 {
                o[ch] = self.f[ch][b].process(o[ch]);
            }
        }
        (o[0], o[1])
    }
}

// ------------------------------------------------------------------ ViHalla

struct Comb {
    buf: Vec<f32>,
    len: usize,
    idx: usize,
    store: f32,
}
impl Comb {
    fn new(max: usize) -> Self {
        Comb { buf: vec![0.0; max.max(8)], len: max.max(8), idx: 0, store: 0.0 }
    }
    fn set_len(&mut self, n: usize) {
        self.len = n.clamp(8, self.buf.len());
        if self.idx >= self.len {
            self.idx = 0;
        }
    }
    #[inline]
    fn run(&mut self, x: f32, fb: f32, damp: f32) -> f32 {
        let out = self.buf[self.idx];
        self.store = out * (1.0 - damp) + self.store * damp;
        self.buf[self.idx] = x + self.store * fb;
        self.idx += 1;
        if self.idx >= self.len {
            self.idx = 0;
        }
        out
    }
}
struct Ap {
    buf: Vec<f32>,
    idx: usize,
}
impl Ap {
    fn new(n: usize) -> Self {
        Ap { buf: vec![0.0; n.max(4)], idx: 0 }
    }
    #[inline]
    fn run(&mut self, x: f32) -> f32 {
        let b = self.buf[self.idx];
        let out = b - x;
        self.buf[self.idx] = x + b * 0.5;
        self.idx = (self.idx + 1) % self.buf.len();
        out
    }
}

/// (comb length scale, base decay, damping, predelay ms, early-diffusion allpass scale, modulation depth)
pub const HALLA_MODES: [(&str, f32, f32, f32, f32); 12] = [
    ("Room", 0.45, 0.80, 0.35, 0.5),
    ("Hall", 1.20, 0.90, 0.25, 1.0),
    ("Plate", 0.80, 0.92, 0.10, 1.2),
    ("Chamber", 0.70, 0.86, 0.30, 0.8),
    ("Cathedral", 2.00, 0.95, 0.30, 1.0),
    ("Spring", 0.55, 0.85, 0.15, 1.6),
    ("Ambience", 0.30, 0.70, 0.40, 0.4),
    ("Bloom", 1.50, 0.93, 0.15, 1.4),
    ("Nonlinear", 0.90, 0.88, 0.05, 0.6),
    ("Shimmer Hall", 1.40, 0.93, 0.20, 1.0),
    ("Dark", 1.00, 0.91, 0.65, 0.9),
    ("Space", 2.20, 0.96, 0.20, 1.1),
];

struct Verb {
    cl: Vec<Comb>,
    cr: Vec<Comb>,
    al: Vec<Ap>,
    ar: Vec<Ap>,
    mode: i32,
}
const TUNE: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const APT: [usize; 4] = [556, 441, 341, 225];
impl Verb {
    fn new(sr: f32) -> Self {
        let k = sr / 44100.0 * 2.3;
        let sz = |n: usize| (n as f32 * k) as usize;
        Verb {
            cl: TUNE.iter().map(|&n| Comb::new(sz(n))).collect(),
            cr: TUNE.iter().map(|&n| Comb::new(sz(n + 23))).collect(),
            al: APT.iter().map(|&n| Ap::new(sz(n))).collect(),
            ar: APT.iter().map(|&n| Ap::new(sz(n + 23))).collect(),
            mode: -1,
        }
    }
    fn set_mode(&mut self, m: i32, sr: f32) {
        if m == self.mode {
            return;
        }
        self.mode = m;
        let scale = HALLA_MODES[m as usize].1.min(2.2);
        let k = sr / 44100.0 * scale;
        for (i, c) in self.cl.iter_mut().enumerate() {
            c.set_len((TUNE[i] as f32 * k) as usize);
        }
        for (i, c) in self.cr.iter_mut().enumerate() {
            c.set_len(((TUNE[i] + 23) as f32 * k) as usize);
        }
    }
    #[inline]
    fn run(&mut self, x: f32, fb: f32, damp: f32, width: f32) -> (f32, f32) {
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
        let w = width.clamp(0.0, 1.0);
        let (p, q) = (0.5 + w * 0.5, 0.5 - w * 0.5);
        ((wl * p + wr * q) * 0.35, (wr * p + wl * q) * 0.35)
    }
}

/// Octave-up pitch shifter (two crossfaded read heads) used as the shimmer path.
struct Shifter {
    buf: Vec<f32>,
    w: usize,
    ph: f32,
}
impl Shifter {
    fn new(sr: f32) -> Self {
        Shifter { buf: vec![0.0; (sr * 0.12) as usize + 8], w: 0, ph: 0.0 }
    }
    #[inline]
    fn run(&mut self, x: f32, sr: f32) -> f32 {
        let n = self.buf.len();
        self.buf[self.w] = x;
        let win = sr * 0.05;
        self.ph += 1.0 / win; // read head moves at 2x -> relative speed 1
        if self.ph >= 1.0 {
            self.ph -= 1.0;
        }
        let tap = |ph: f32| {
            let d = ph * win;
            let pos = (self.w as f32 - d).rem_euclid(n as f32);
            let i = pos as usize;
            let f = pos - i as f32;
            self.buf[i % n] * (1.0 - f) + self.buf[(i + 1) % n] * f
        };
        let p2 = (self.ph + 0.5).fract();
        let g1 = (self.ph * PI).sin();
        let g2 = (p2 * PI).sin();
        let y = tap(self.ph) * g1 * g1 + tap(p2) * g2 * g2;
        self.w = (self.w + 1) % n;
        y
    }
}

pub struct ViHalla {
    a: Verb,
    b: Verb,
    pre: Vec<f32>,
    pw: usize,
    shl: Shifter,
    shr: Shifter,
    fbl: f32,
    fbr: f32,
    env: f32,
    duck: f32,
    freeze: f32,
    hicut: [f32; 2],
}
impl ViHalla {
    pub fn new(sr: f32) -> Self {
        ViHalla {
            a: Verb::new(sr),
            b: Verb::new(sr),
            pre: vec![0.0; (sr * 0.25) as usize + 8],
            pw: 0,
            shl: Shifter::new(sr),
            shr: Shifter::new(sr),
            fbl: 0.0,
            fbr: 0.0,
            env: 0.0,
            duck: 1.0,
            freeze: 0.0,
            hicut: [0.0; 2],
        }
    }
}
impl Stereo for ViHalla {
    // params: 0 mode, 1 decay, 2 damping, 3 predelay ms, 4 mix, 5 width, 6 shimmer, 7 freeze, 8 duck, 9 mode B, 10 A/B blend, 11 hi-cut
    fn run(&mut self, l: f32, r: f32, a: &Params, sr: f32, _: f32) -> (f32, f32) {
        let ma = (a[0].round() as i32).clamp(0, 11);
        let mb = (a[9].round() as i32).clamp(0, 11);
        self.a.set_mode(ma, sr);
        self.b.set_mode(mb, sr);
        // freeze fades smoothly so the tail locks without a click
        self.freeze += (if a[7] > 0.5 { 1.0 } else { 0.0 } - self.freeze) * 0.001;
        let fr = self.freeze;
        let n = self.pre.len();
        let mono = (l + r) * 0.5;
        // pre-delay
        self.pre[self.pw] = mono;
        let d = ((a[3] * 0.001 * sr) as usize).clamp(1, n - 1);
        let pd = self.pre[(self.pw + n - d) % n];
        self.pw = (self.pw + 1) % n;
        // envelope follower for ducking
        let lvl = l.abs().max(r.abs());
        self.env += (lvl - self.env) * if lvl > self.env { 0.05 } else { 0.0004 };
        let target = 1.0 - (self.env * 3.0).min(1.0) * a[8].clamp(0.0, 1.0);
        self.duck += (target - self.duck) * if target < self.duck { 0.01 } else { 0.0005 };

        let shim = if ma == 9 { a[6].max(0.35) } else { a[6] }.clamp(0.0, 1.0);
        let input = (pd + (self.fbl + self.fbr) * 0.5 * shim * 0.6) * (1.0 - fr);
        let decay = a[1].clamp(0.0, 1.0);
        let fbk = |m: i32| {
            let base = HALLA_MODES[m as usize].2;
            (0.55 + decay * (base - 0.55)).min(0.985) * (1.0 - fr) + 0.9999 * fr
        };
        let dmp = |m: i32| (a[2].clamp(0.0, 1.0) * 0.5 + HALLA_MODES[m as usize].3 * 0.3).min(0.8) * (1.0 - fr);
        let (al, ar) = self.a.run(input * 0.03, fbk(ma), dmp(ma), a[5]);
        let (bl, br) = self.b.run(input * 0.03, fbk(mb), dmp(mb), a[5]);
        let blend = a[10].clamp(0.0, 1.0);
        let (mut wl, mut wr) = (al * (1.0 - blend) + bl * blend, ar * (1.0 - blend) + br * blend);
        let k = onepole(a[11].clamp(1000.0, 20000.0), sr);
        self.hicut[0] += k * (wl - self.hicut[0]);
        self.hicut[1] += k * (wr - self.hicut[1]);
        wl = self.hicut[0];
        wr = self.hicut[1];
        // shimmer feedback: octave-up copy of the tail
        self.fbl = self.shl.run(wl, sr);
        self.fbr = self.shr.run(wr, sr);
        let m = a[4].clamp(0.0, 1.0);
        (l * (1.0 - m * 0.5) + wl * m * 2.0 * self.duck, r * (1.0 - m * 0.5) + wr * m * 2.0 * self.duck)
    }
}

// ------------------------------------------------------------------ ViGross

pub struct ViGross {
    buf: Vec<[f32; 2]>,
    w: usize,
    phase: f32,
    slice_start: usize,
    rev: bool,
    stut: bool,
    stut_len: usize,
    gain: f32,
    rng: u32,
}
impl ViGross {
    pub fn new(sr: f32) -> Self {
        ViGross { buf: vec![[0.0; 2]; (sr * 6.0) as usize], w: 0, phase: 0.0, slice_start: 0, rev: false, stut: false, stut_len: 1, gain: 1.0, rng: 0x9E37_79B9 }
    }
    fn rand(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        (self.rng >> 8) as f32 / 16_777_216.0
    }
}
/// volume shapes: A = steady 16th gate, B = trap stutter
const SHAPE_A: [f32; 8] = [1.0, 0.2, 1.0, 0.2, 1.0, 0.2, 1.0, 0.2];
const SHAPE_B: [f32; 8] = [1.0, 1.0, 0.1, 1.0, 0.1, 0.1, 1.0, 0.3];
impl Stereo for ViGross {
    // params: 0 slice (beats), 1 reverse prob, 2 stutter prob, 3 stutter length (fraction of slice), 4 depth, 5 morph A->B, 6 smooth ms, 7 mix
    fn run(&mut self, l: f32, r: f32, a: &Params, sr: f32, bpm: f32) -> (f32, f32) {
        let n = self.buf.len();
        let slice = ((a[0].max(0.0625) * 60.0 / bpm.max(20.0) * sr) as usize).clamp(64, n / 2);
        self.buf[self.w] = [l, r];
        let t = self.phase as usize;
        if t == 0 {
            self.slice_start = self.w;
            self.rev = self.rand() < a[1];
            self.stut = !self.rev && self.rand() < a[2];
            self.stut_len = ((slice as f32 * a[3].clamp(0.02, 0.5)) as usize).max(16);
        }
        let mut out = [l, r];
        if self.rev {
            // play the previous slice backwards
            let back = self.slice_start + n - 1 - t.min(slice - 1);
            out = self.buf[back % n];
        } else if self.stut && t >= self.stut_len {
            let off = t % self.stut_len;
            out = self.buf[(self.slice_start + off) % n];
        }
        // volume pattern with smooth morph
        let step = (t * 8 / slice).min(7);
        let pat = SHAPE_A[step] * (1.0 - a[5].clamp(0.0, 1.0)) + SHAPE_B[step] * a[5].clamp(0.0, 1.0);
        let tgt = 1.0 - a[4].clamp(0.0, 1.0) * (1.0 - pat);
        self.gain += (tgt - self.gain) * onepole(1000.0 / a[6].max(0.5) / 3.0, sr);
        self.phase += 1.0;
        if self.phase as usize >= slice {
            self.phase = 0.0;
        }
        self.w = (self.w + 1) % n;
        let m = a[7].clamp(0.0, 1.0);
        (mixf(l, out[0] * self.gain, m), mixf(r, out[1] * self.gain, m))
    }
}
