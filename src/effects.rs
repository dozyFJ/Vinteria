//! Effects & filters: state-variable filter, saturator, limiter, stereo delay.
use std::f32::consts::PI;
use crate::audio_engine::fastmath::Fast;

/// Cytomic state-variable filter.
#[derive(Clone, Copy, Default)]
pub struct Svf {
    ic1: f32,
    ic2: f32,
    fc: f32,
    g: f32,
}

impl Svf {
    /// returns (low, high). `res` in 0..1.
    #[inline]
    pub fn process(&mut self, x: f32, cutoff: f32, res: f32, sr: f32) -> (f32, f32) {
        let fc = cutoff.clamp(10.0, sr * 0.45);
        // tan() is expensive: only recompute when the cutoff moved by more than 0.2 %
        if (fc - self.fc).abs() > self.fc * 0.002 {
            self.fc = fc;
            self.g = (PI * fc / sr).ftan();
        }
        let g = self.g;
        let k = 2.0 - 1.95 * res.clamp(0.0, 1.0);
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        let a3 = g * a2;
        let v3 = x - self.ic2;
        let v1 = a1 * self.ic1 + a2 * v3;
        let v2 = self.ic2 + a2 * self.ic1 + a3 * v3;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        (v2, x - k * v1 - v2)
    }
    pub fn reset(&mut self) {
        *self = Svf::default();
    }
}

/// Normalised tanh saturation. `drive` >= 1.
#[inline]
pub fn saturate(x: f32, drive: f32) -> f32 {
    let d = drive.max(1.0);
    (x * d).ftanh() / d.ftanh()
}

/// Look-ahead peak limiter: the signal is delayed by ~2 ms while the gain is lowered smoothly *before* a
/// peak arrives, so there is no hard clipping and no gain "zipper" distortion.
pub struct Limiter {
    buf: Vec<[f32; 2]>,
    pos: usize,
    gain: f32,
    sr: f32,
    att: f32,
    rel: f32,
}

const LOOKAHEAD: usize = 96;

impl Limiter {
    pub fn new() -> Self {
        Limiter { buf: vec![[0.0; 2]; LOOKAHEAD], pos: 0, gain: 1.0, sr: 0.0, att: 0.0, rel: 0.0 }
    }
    #[inline]
    pub fn process(&mut self, l: f32, r: f32, sr: f32) -> (f32, f32) {
        let ceiling = 0.95;
        self.buf[self.pos] = [l, r];
        // loudest sample inside the look-ahead window
        let mut peak = 0.0f32;
        for s in self.buf.iter() {
            peak = peak.max(s[0].abs()).max(s[1].abs());
        }
        let target = if peak > ceiling { ceiling / peak } else { 1.0 };
        if sr != self.sr {
            self.sr = sr;
            self.att = 1.0 - (-1.0 / (0.0005 * sr)).exp();
            self.rel = 1.0 - (-1.0 / (0.12 * sr)).exp();
        }
        self.gain += (target - self.gain) * if target < self.gain { self.att } else { self.rel };
        self.pos = (self.pos + 1) % LOOKAHEAD;
        let d = self.buf[self.pos]; // oldest sample = input delayed by LOOKAHEAD-1 samples
        // safety net: smooth saturation instead of a hard clip if a peak still gets through
        let sat = |x: f32| if x.abs() > 0.97 { x.signum() * (0.97 + 0.03 * ((x.abs() - 0.97) * 8.0).ftanh()) } else { x };
        (sat(d[0] * self.gain), sat(d[1] * self.gain))
    }
}

/// Ping-pong stereo delay. Buffer is allocated once at construction.
pub struct Delay {
    buf: Vec<[f32; 2]>,
    w: usize,
}

impl Delay {
    pub fn new(sr: f32) -> Self {
        Delay { buf: vec![[0.0; 2]; (sr * 4.0) as usize + 8], w: 0 }
    }
    #[inline]
    pub fn process(&mut self, l: f32, r: f32, delay_samples: f32, feedback: f32) -> (f32, f32) {
        let len = self.buf.len();
        let d = (delay_samples as usize).clamp(1, len - 1);
        let rd = (self.w + len - d) % len;
        let [dl, dr] = self.buf[rd];
        self.buf[self.w] = [l + dr * feedback, r + dl * feedback];
        self.w = (self.w + 1) % len;
        (dl, dr)
    }
}

/// Feed-forward stereo-linked compressor (peak detector, 5 ms attack / 140 ms release).
pub struct Compressor {
    env: f32,
    sr: f32,
    att: f32,
    rel: f32,
}

impl Compressor {
    pub fn new() -> Self {
        Compressor { env: 0.0, sr: 0.0, att: 0.0, rel: 0.0 }
    }
    /// Returns the processed pair and the current gain reduction in dB (>= 0).
    #[inline]
    pub fn process(&mut self, l: f32, r: f32, threshold_db: f32, ratio: f32, makeup_db: f32, sr: f32) -> (f32, f32, f32) {
        let lvl = l.abs().max(r.abs());
        if sr != self.sr {
            self.sr = sr;
            self.att = 1.0 - (-1.0 / (0.005 * sr)).exp();
            self.rel = 1.0 - (-1.0 / (0.14 * sr)).exp();
        }
        self.env += (lvl - self.env) * if lvl > self.env { self.att } else { self.rel };
        let db = 20.0 * self.env.max(1e-6).flog10();
        let over = db - threshold_db;
        let gr = if over > 0.0 { over * (1.0 - 1.0 / ratio.max(1.0)) } else { 0.0 };
        let g = ((makeup_db - gr) * (std::f32::consts::LN_10 / 20.0)).fexp();
        (l * g, r * g, gr)
    }
}

/// RBJ biquad (direct form I), used as low / high shelf.
#[derive(Clone, Copy, Default)]
pub struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl Biquad {
    fn set(&mut self, b0: f32, b1: f32, b2: f32, a0: f32, a1: f32, a2: f32) {
        self.b0 = b0 / a0;
        self.b1 = b1 / a0;
        self.b2 = b2 / a0;
        self.a1 = a1 / a0;
        self.a2 = a2 / a0;
    }
    pub fn low_shelf(&mut self, freq: f32, gain_db: f32, sr: f32) {
        let a = 10f32.powf(gain_db / 40.0);
        let w = std::f32::consts::TAU * freq / sr;
        let (s, c) = (w.sin(), w.cos());
        let alpha = s / 2.0 * std::f32::consts::SQRT_2;
        let t = 2.0 * a.sqrt() * alpha;
        self.set(
            a * ((a + 1.0) - (a - 1.0) * c + t),
            2.0 * a * ((a - 1.0) - (a + 1.0) * c),
            a * ((a + 1.0) - (a - 1.0) * c - t),
            (a + 1.0) + (a - 1.0) * c + t,
            -2.0 * ((a - 1.0) + (a + 1.0) * c),
            (a + 1.0) + (a - 1.0) * c - t,
        );
    }
    /// peaking EQ band
    pub fn peak(&mut self, freq: f32, gain_db: f32, q: f32, sr: f32) {
        let a = 10f32.powf(gain_db / 40.0);
        let w = std::f32::consts::TAU * freq / sr;
        let (s, c) = (w.sin(), w.cos());
        let alpha = s / (2.0 * q.max(0.1));
        self.set(1.0 + alpha * a, -2.0 * c, 1.0 - alpha * a, 1.0 + alpha / a, -2.0 * c, 1.0 - alpha / a);
    }
    pub fn high_shelf(&mut self, freq: f32, gain_db: f32, sr: f32) {
        let a = 10f32.powf(gain_db / 40.0);
        let w = std::f32::consts::TAU * freq / sr;
        let (s, c) = (w.sin(), w.cos());
        let alpha = s / 2.0 * std::f32::consts::SQRT_2;
        let t = 2.0 * a.sqrt() * alpha;
        self.set(
            a * ((a + 1.0) + (a - 1.0) * c + t),
            -2.0 * a * ((a - 1.0) + (a + 1.0) * c),
            a * ((a + 1.0) + (a - 1.0) * c - t),
            (a + 1.0) - (a - 1.0) * c + t,
            2.0 * ((a - 1.0) - (a + 1.0) * c),
            (a + 1.0) - (a - 1.0) * c - t,
        );
    }
    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2 - self.a1 * self.y1 - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}
