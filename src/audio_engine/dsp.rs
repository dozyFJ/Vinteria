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
