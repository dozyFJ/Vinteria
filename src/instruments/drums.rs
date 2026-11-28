//! Drum machine voices: synthesized defaults or WAV sample playback.
use crate::audio_engine::dsp::Rng;
use crate::project::SampleData;
use std::f32::consts::TAU;
use crate::audio_engine::fastmath::Fast;

pub const DRUM_VOICES: usize = 8;

#[derive(Clone, Copy, Default)]
pub struct DrumVoice {
    pub active: bool,
    t: f32,
    vel: f32,
    pos: f64,
    phase: f32,
    lp: f32,
    lp2: f32,
    /// playback / pitch multiplier of this hit (1.0 = original)
    rate: f32,
    /// slide: extra pitch multiplier that decays to 1.0
    bend: f32,
}

impl DrumVoice {
    pub fn trigger(&mut self, vel: f32) {
        self.trigger_ex(vel, 0, false);
    }

    /// Hit with a Smart Step pitch (semitones) and optional slide.
    pub fn trigger_ex(&mut self, vel: f32, semis: i32, slide: bool) {
        *self = DrumVoice { active: true, vel, rate: 2f32.powf(semis as f32 / 12.0), bend: if slide { 2.0 } else { 1.0 }, ..Default::default() };
    }

    /// Render one sample. `kind`: 0 kick, 1 snare, 2 clap, 3 hat, 4 perc.
    #[inline]
    pub fn render(&mut self, kind: usize, sample: Option<&SampleData>, sr: f32, rng: &mut Rng) -> f32 {
        if !self.active {
            return 0.0;
        }
        self.bend += (1.0 - self.bend) * (1.0 - (-1.0 / (0.06 * sr)).exp());
        let pm = if self.rate == 0.0 { 1.0 } else { self.rate } * self.bend;
        if let Some(sd) = sample {
            let i = self.pos as usize;
            if i + 1 >= sd.mono.len() {
                self.active = false;
                return 0.0;
            }
            let fr = (self.pos - i as f64) as f32;
            let s = crate::audio_engine::dsp::hermite(&sd.mono, i, fr);
            self.pos += sd.rate as f64 / sr as f64 * pm as f64;
            return s * self.vel;
        }
        let t = self.t;
        self.t += 1.0 / sr;
        let out = match kind {
            0 => {
                let f = (42.0 + 120.0 * (-t / 0.028).fexp()) * pm;
                self.phase += f / sr;
                let body = (self.phase * TAU).fsin() * (-t / 0.22).fexp();
                let click = rng.noise() * (-t / 0.003).fexp() * 0.25;
                if t > 0.7 {
                    self.active = false;
                }
                (body * 1.25 + click).clamp(-1.0, 1.0) * 0.95
            }
            1 => {
                self.phase += (190.0 + 80.0 * (-t / 0.02).fexp()) * pm / sr;
                let tone = (self.phase * TAU).fsin() * (-t / 0.07).fexp() * 0.6;
                let n = rng.noise();
                self.lp += 0.45 * (n - self.lp);
                let noise = (n - self.lp) * (-t / 0.11).fexp();
                if t > 0.5 {
                    self.active = false;
                }
                (tone + noise * 0.7) * 0.9
            }
            2 => {
                let n = rng.noise();
                self.lp += 0.35 * (n - self.lp);
                let hp = n - self.lp;
                let mut env = 0.0;
                for k in 0..3 {
                    let tk = t - k as f32 * 0.011;
                    if tk >= 0.0 {
                        env += (-tk / 0.006).fexp();
                    }
                }
                if t > 0.033 {
                    env += (-(t - 0.033) / 0.09).fexp() * 0.9;
                }
                if t > 0.45 {
                    self.active = false;
                }
                hp * env.min(1.2) * 0.8
            }
            3 => {
                let n = rng.noise();
                self.lp += 0.6 * (n - self.lp);
                let hp1 = n - self.lp;
                self.lp2 += 0.6 * (hp1 - self.lp2);
                let hp2 = hp1 - self.lp2;
                // metallic ring: inharmonic squares
                self.phase += pm / sr;
                let m = [4180.0f32, 5320.0, 6930.0]
                    .iter()
                    .map(|f| if ((self.phase * f).ffract()) < 0.5 { 1.0 } else { -1.0 })
                    .sum::<f32>()
                    * 0.12;
                if t > 0.2 {
                    self.active = false;
                }
                (hp2 * 1.4 + m) * (-t / 0.035).fexp() * 0.6
            }
            _ => {
                self.phase += (360.0 - 140.0 * (t / 0.1).min(1.0)) * pm / sr;
                let tone = (self.phase * TAU).fsin() * (-t / 0.07).fexp();
                let click = rng.noise() * (-t / 0.004).fexp() * 0.4;
                if t > 0.3 {
                    self.active = false;
                }
                (tone * 0.8 + click) * 0.8
            }
        };
        out * self.vel
    }
}

pub struct DrumLane {
    pub voices: [DrumVoice; DRUM_VOICES],
    next: usize,
}

impl DrumLane {
    pub fn new() -> Self {
        DrumLane { voices: [DrumVoice::default(); DRUM_VOICES], next: 0 }
    }
    pub fn trigger(&mut self, vel: f32) {
        self.trigger_ex(vel, 0, false);
    }
    pub fn trigger_ex(&mut self, vel: f32, semis: i32, slide: bool) {
        let idx = self.voices.iter().position(|v| !v.active).unwrap_or_else(|| {
            self.next = (self.next + 1) % DRUM_VOICES;
            self.next
        });
        self.voices[idx].trigger_ex(vel, semis, slide);
    }
    #[inline]
    pub fn render(&mut self, kind: usize, sample: Option<&SampleData>, sr: f32, rng: &mut Rng) -> f32 {
        let mut o = 0.0;
        for v in self.voices.iter_mut() {
            o += v.render(kind, sample, sr, rng);
        }
        o
    }
}

/// Render the built-in drum voice of `kind` to a mono buffer (used to create factory samples).
pub fn render_factory(kind: usize, sr: u32) -> Vec<f32> {
    let mut v = DrumVoice::default();
    v.trigger(1.0);
    let mut rng = Rng::new(12345);
    let mut out = Vec::new();
    while v.active && out.len() < sr as usize * 2 {
        out.push(v.render(kind, None, sr as f32, &mut rng));
    }
    out
}
