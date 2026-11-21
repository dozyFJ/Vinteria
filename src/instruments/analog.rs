//! Diva-style analog synth: 2 VCOs with PWM / hard sync / drift, sub, noise, 4-pole ladder filter with drive.
use super::{AnTarget, AnalogParams, AnalogWave, LfoShape};
use crate::audio_engine::dsp::{Adsr, Rng};
use crate::piano_roll::midi_to_freq;
use std::f32::consts::{PI, TAU};
use crate::audio_engine::fastmath::Fast;

const VOICES: usize = 10;

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

#[inline]
fn vco(w: AnalogWave, phase: f32, inc: f32, pw: f32) -> f32 {
    match w {
        AnalogWave::Sine => (phase * TAU).fsin(),
        AnalogWave::Tri => 1.0 - 4.0 * (phase - 0.5).abs(),
        AnalogWave::Saw => 2.0 * phase - 1.0 - polyblep(phase, inc),
        AnalogWave::Pulse => {
            let naive = if phase < pw { 1.0 } else { -1.0 };
            naive + polyblep(phase, inc) - polyblep((phase - pw + 1.0).ffract(), inc)
        }
    }
}

/// 4-pole (24 dB/oct) transistor-ladder style low-pass with tanh saturation in the feedback loop.
#[derive(Clone, Copy, Default)]
struct Ladder {
    y: [f32; 4],
    fc: f32,
    g: f32,
}

impl Ladder {
    #[inline]
    fn process(&mut self, x: f32, cutoff: f32, res: f32, drive: f32, sr: f32) -> f32 {
        let fc = cutoff.clamp(20.0, sr * 0.42);
        if (fc - self.fc).abs() > self.fc * 0.002 {
            self.fc = fc;
            self.g = 1.0 - (-TAU * fc / sr).fexp();
        }
        let g = self.g;
        let k = res.clamp(0.0, 1.0) * 3.9;
        let u = (x * drive - k * self.y[3]).ftanh();
        self.y[0] += g * (u - self.y[0]);
        self.y[1] += g * (self.y[0] - self.y[1]);
        self.y[2] += g * (self.y[1] - self.y[2]);
        self.y[3] += g * (self.y[2] - self.y[3]);
        self.y[3] / drive.max(1.0).sqrt().max(1.0)
    }
}

#[derive(Clone, Copy)]
struct Voice {
    active: bool,
    note: u8,
    vel: f32,
    p1: f32,
    p2: f32,
    sub: f32,
    freq: f32,
    drift: f32,
    amp: Adsr,
    fenv: Adsr,
    ladder: Ladder,
    end_tick: u64,
    age: u64,
    target: f32,
    /// slide: per-sample step towards `target` (0 = the glide time of the patch)
    gl: f32,
    tick: u32,
    cut: f32,
    drift_mul: f32,
}

pub struct AnalogSynth {
    voices: [Voice; VOICES],
    lfo: f32,
    counter: u64,
    rng: Rng,
}

impl AnalogSynth {
    pub fn new() -> Self {
        let v = Voice {
            active: false,
            note: 0,
            vel: 0.0,
            p1: 0.0,
            p2: 0.0,
            sub: 0.0,
            freq: 440.0,
            drift: 0.0,
            amp: Adsr::new(),
            fenv: Adsr::new(),
            ladder: Ladder::default(),
            end_tick: u64::MAX,
            age: 0,
            target: 440.0,
            gl: 0.0,
            tick: 0,
            cut: 1000.0,
            drift_mul: 1.0,
        };
        AnalogSynth { voices: [v; VOICES], lfo: 0.0, counter: 0, rng: Rng::new(4242) }
    }

    pub fn note_on(&mut self, note: u8, vel: f32, end_tick: u64) {
        self.counter += 1;
        let idx = self.voices.iter().position(|v| !v.active).unwrap_or_else(|| {
            self.voices.iter().enumerate().min_by_key(|(_, v)| v.age).map(|(i, _)| i).unwrap_or(0)
        });
        let was_active = self.voices[idx].active;
        let prev_freq = self.voices[idx].freq;
        let start_ph = if self.rng.noise() > 0.0 { 0.0 } else { 0.0 };
        let v = &mut self.voices[idx];
        v.active = true;
        v.note = note;
        v.vel = vel;
        v.p1 = start_ph;
        v.p2 = start_ph;
        v.sub = 0.0;
        v.freq = if was_active { prev_freq } else { midi_to_freq(note as f32) };
        v.drift = 0.0;
        v.amp = Adsr::new();
        v.amp.gate_on();
        v.fenv = Adsr::new();
        v.fenv.gate_on();
        v.ladder = Ladder::default();
        v.end_tick = end_tick;
        v.age = self.counter;
        v.target = midi_to_freq(note as f32);
        v.gl = 0.0;
        v.tick = 0;
    }

    /// FL-style slide: the newest sounding voice glides to `note` over the length of the slide note instead of a new voice starting.
    /// Returns false when nothing is sounding (the caller then plays the note normally).
    pub fn slide_to(&mut self, note: u8, vel: f32, end_tick: u64, glide_s: f32, sr: f32) -> bool {
        let Some(v) = self.voices.iter_mut().filter(|v| v.active && !v.amp.is_released()).max_by_key(|v| v.age) else { return false };
        v.note = note;
        v.vel = vel;
        v.end_tick = end_tick;
        v.target = midi_to_freq(note as f32);
        v.gl = 1.0 - (-1.0 / (glide_s.max(0.01) * 0.3 * sr)).exp();
        true
    }

    pub fn note_off(&mut self, note: u8) {
        for v in self.voices.iter_mut() {
            if v.active && v.note == note && v.end_tick == u64::MAX && !v.amp.is_released() {
                v.amp.gate_off();
                v.fenv.gate_off();
            }
        }
    }

    pub fn release_due(&mut self, abs_tick: u64) {
        for v in self.voices.iter_mut() {
            if v.active && v.end_tick <= abs_tick && !v.amp.is_released() {
                v.amp.gate_off();
                v.fenv.gate_off();
            }
        }
    }

    pub fn release_all(&mut self) {
        for v in self.voices.iter_mut() {
            v.amp.gate_off();
            v.fenv.gate_off();
        }
    }

    #[inline]
    pub fn render_sample(&mut self, p: &AnalogParams, sr: f32) -> f32 {
        if !self.voices.iter().any(|v| v.active) {
            return 0.0;
        }
        let ph = self.lfo;
        let lfo = match p.lfo_shape {
            LfoShape::Sine => (ph * TAU).fsin(),
            LfoShape::Tri => 1.0 - 4.0 * (ph - 0.5).abs(),
            LfoShape::Square => if ph < 0.5 { 1.0 } else { -1.0 },
        } * p.lfo_depth;
        self.lfo = (self.lfo + p.lfo_rate / sr).ffract();
        let glide = if p.glide_ms < 1.0 { 1.0 } else { 1.0 - (-1.0 / (p.glide_ms * 0.001 * sr)).fexp() };
        let m1 = (p.semi1 / 12.0).fexp2();
        let m2 = (p.semi2 / 12.0 + p.detune2 / 1200.0).fexp2();
        let pitch_mod = if p.lfo_target == AnTarget::Pitch { (lfo * 2.0 / 12.0).fexp2() } else { 1.0 };
        let mut out = 0.0;
        for v in self.voices.iter_mut() {
            if !v.active {
                continue;
            }
            let fe = v.fenv.next(p.f_attack, p.f_decay, p.f_sustain, p.f_release, sr);
            let ae = v.amp.next(p.attack, p.decay, p.sustain, p.release, sr);
            v.freq += (v.target - v.freq) * if v.gl > 0.0 { v.gl } else { glide };
            // slow random walk = analog pitch drift (+-~12 cents at drift 1)
            v.drift += (self.rng.noise() * 0.0004 - v.drift * 0.0002) * p.drift;
            if v.tick % 8 == 0 {
                v.drift_mul = (v.drift * 12.0 / 1200.0).fexp2();
                let kt = (v.note as f32 - 60.0) / 12.0 * p.keytrack;
                v.cut = p.cutoff * (p.env_amt * fe + kt + if p.lfo_target == AnTarget::Cutoff { lfo * 2.0 } else { 0.0 }).fexp2();
            }
            v.tick = v.tick.wrapping_add(1);
            let f1 = v.freq * m1 * v.drift_mul * pitch_mod;
            let f2 = v.freq * m2 * v.drift_mul * pitch_mod;
            let pw = (p.pw + if p.lfo_target == AnTarget::Pwm { lfo * 0.4 } else { 0.0 }).clamp(0.08, 0.92);
            let (i1, i2) = ((f1 / sr).min(0.45), (f2 / sr).min(0.45));
            let o1 = vco(p.wave1, v.p1, i1, pw);
            let o2 = vco(p.wave2, v.p2, i2, pw);
            v.p1 += i1;
            let wrapped = v.p1 >= 1.0;
            if wrapped {
                v.p1 -= 1.0;
                if p.sync {
                    v.p2 = 0.0;
                }
            }
            v.p2 += i2;
            if v.p2 >= 1.0 {
                v.p2 -= 1.0;
            }
            let mut s = o1 * (1.0 - p.mix) + o2 * p.mix;
            if p.sub > 0.001 {
                v.sub = (v.sub + v.freq * 0.5 / sr).ffract();
                s += (v.sub * TAU).fsin() * p.sub;
            }
            if p.noise > 0.001 {
                s += self.rng.noise() * p.noise * 0.5;
            }
            let y = v.ladder.process(s * 0.6, v.cut, p.res, p.drive, sr);
            out += y * ae * v.vel;
            if v.amp.is_idle() {
                v.active = false;
            }
        }
        let _ = PI;
        out * p.volume * 0.7
    }
}

#[cfg(test)]
mod slide_tests {
    use super::*;

    #[test]
    fn a_slide_reuses_the_sounding_voice_and_glides_to_the_new_pitch() {
        let mut s = AnalogSynth::new();
        let p = AnalogParams::default();
        s.note_on(57, 0.8, u64::MAX);
        for _ in 0..2000 {
            s.render_sample(&p, 48000.0);
        }
        assert!(s.slide_to(69, 0.8, u64::MAX, 0.2, 48000.0));
        assert_eq!(s.voices.iter().filter(|v| v.active).count(), 1, "no second voice starts");
        for _ in 0..48000 {
            s.render_sample(&p, 48000.0);
        }
        let v = s.voices.iter().find(|v| v.active).unwrap();
        let target = midi_to_freq(69.0);
        assert!((v.freq - target).abs() < target * 0.02, "the pitch arrived: {} vs {}", v.freq, target);
        // nothing sounding: the slide asks for a normal note
        let mut e = AnalogSynth::new();
        assert!(!e.slide_to(60, 0.8, u64::MAX, 0.2, 48000.0));
    }
}
