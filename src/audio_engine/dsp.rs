//! Small DSP building blocks used by the instruments (all allocation-free).
use crate::instruments::Wave;
use std::f32::consts::TAU;
use crate::audio_engine::fastmath::Fast;

#[derive(Clone, Copy, PartialEq)]
enum Stage {
    Idle,
    Attack,
    Decay,
    Sustain,
    Release,
}

#[derive(Clone, Copy)]
pub struct Adsr {
    stage: Stage,
    level: f32,
    dt: f32,
    dcoef: f32,
    rt: f32,
    rcoef: f32,
}

impl Adsr {
    pub fn new() -> Self {
        Adsr { stage: Stage::Idle, level: 0.0, dt: -1.0, dcoef: 0.0, rt: -1.0, rcoef: 0.0 }
    }
    pub fn gate_on(&mut self) {
        self.stage = Stage::Attack;
    }
    pub fn gate_off(&mut self) {
        if self.stage != Stage::Idle {
            self.stage = Stage::Release;
        }
    }
    pub fn is_idle(&self) -> bool {
        self.stage == Stage::Idle
    }
    pub fn is_released(&self) -> bool {
        matches!(self.stage, Stage::Release | Stage::Idle)
    }
    pub fn kill(&mut self) {
        self.stage = Stage::Idle;
        self.level = 0.0;
    }
    #[inline]
    pub fn next(&mut self, a: f32, d: f32, s: f32, r: f32, sr: f32) -> f32 {
        match self.stage {
            Stage::Idle => self.level = 0.0,
            Stage::Attack => {
                self.level += 1.0 / (a.max(0.0005) * sr);
                if self.level >= 1.0 {
                    self.level = 1.0;
                    self.stage = Stage::Decay;
                }
            }
            Stage::Decay => {
                if d != self.dt {
                    self.dt = d;
                    self.dcoef = 1.0 - (-4.6 / (d.max(0.001) * sr)).exp();
                }
                self.level += (s - self.level) * self.dcoef;
                if (self.level - s).abs() < 1e-4 {
                    self.level = s;
                    self.stage = Stage::Sustain;
                }
            }
            Stage::Sustain => self.level = s,
            Stage::Release => {
                if r != self.rt {
                    self.rt = r;
                    self.rcoef = 1.0 - (-4.6 / (r.max(0.001) * sr)).exp();
                }
                self.level -= self.level * self.rcoef;
                if self.level < 1e-4 {
                    self.level = 0.0;
                    self.stage = Stage::Idle;
                }
            }
        }
        self.level
    }
}

#[inline]
fn polyblep(t: f32, dt: f32) -> f32 {
    if t < dt {
        let t = t / dt;
        2.0 * t - t * t - 1.0
    } else if t > 1.0 - dt {
        let t = (t - 1.0) / dt;
        t * t + 2.0 * t + 1.0
    } else {
        0.0
    }
}

/// Band-limited oscillator sample. `phase` in 0..1, `inc` = freq/sr.
#[inline]
pub fn osc(w: Wave, phase: f32, inc: f32) -> f32 {
    match w {
        Wave::Sine => (phase * TAU).fsin(),
        Wave::Triangle => 1.0 - 4.0 * (phase - 0.5).abs(),
        Wave::Saw => 2.0 * phase - 1.0 - polyblep(phase, inc),
        Wave::Square => {
            let naive = if phase < 0.5 { 1.0 } else { -1.0 };
            naive + polyblep(phase, inc) - polyblep((phase + 0.5).ffract(), inc)
        }
    }
}

pub struct Rng(u32);

impl Rng {
    pub fn new(seed: u32) -> Self {
        Rng(seed.max(1))
    }
    /// white noise in -1..1
    #[inline]
    pub fn noise(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        (x as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

/// 4-point Hermite interpolation of `d` at `i + f` (`f` in 0..1): cleaner highs than linear interpolation when a
/// sample plays at another rate (pitched hats, 44.1 kHz files on a 48 kHz device). Edges repeat the end samples.
#[inline]
pub fn hermite(d: &[f32], i: usize, f: f32) -> f32 {
    let n = d.len();
    if n == 0 {
        return 0.0;
    }
    let at = |k: usize| d[k.min(n - 1)];
    let xm1 = if i == 0 { at(0) } else { at(i - 1) };
    let (x0, x1, x2) = (at(i), at(i + 1), at(i + 2));
    let c = (x1 - xm1) * 0.5;
    let v = x0 - x1;
    let w = c + v;
    let a = w + v + (x2 - x0) * 0.5;
    let b = w + a;
    ((a * f - b) * f + c) * f + x0
}

#[cfg(test)]
mod hermite_tests {
    #[test]
    fn hermite_hits_the_samples_and_is_smooth_between() {
        let d = [0.0, 1.0, 0.0, -1.0, 0.0];
        for i in 0..4 {
            assert!((super::hermite(&d, i, 0.0) - d[i]).abs() < 1e-6);
        }
        let mid = super::hermite(&d, 1, 0.5);
        assert!(mid > 0.5 && mid < 1.0, "{mid}");
    }
}
