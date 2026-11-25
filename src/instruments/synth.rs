//! Polyphonic subtractive synth: 2 osc, ADSR, LP+HP filters, LFO.
use super::{LfoTarget, SynthParams};
use crate::audio_engine::dsp::{osc, Adsr};
use crate::effects::Svf;
use crate::piano_roll::midi_to_freq;
use std::f32::consts::TAU;
use crate::audio_engine::fastmath::Fast;

pub const VOICES: usize = 16;

#[derive(Clone, Copy)]
struct Voice {
    active: bool,
    note: u8,
    vel: f32,
    phase: f32,
    phase2: f32,
    env: Adsr,
    lp: Svf,
    hp: Svf,
    /// absolute engine tick at which the note is released (u64::MAX = held by a key)
    end_tick: u64,
    age: u64,
    freq: f32,
    /// slide: the pitch the voice is gliding to and its per-sample step (0 = not sliding)
    tgt: f32,
    gl: f32,
}

pub struct Synth {
    voices: [Voice; VOICES],
    lfo_phase: f32,
    counter: u64,
}

impl Synth {
    pub fn new() -> Self {
        let v = Voice {
            active: false,
            note: 0,
            vel: 0.0,
            phase: 0.0,
            phase2: 0.0,
            env: Adsr::new(),
            lp: Svf::default(),
            hp: Svf::default(),
            end_tick: u64::MAX,
            age: 0,
            freq: 440.0,
            tgt: 440.0,
            gl: 0.0,
        };
        Synth { voices: [v; VOICES], lfo_phase: 0.0, counter: 0 }
    }

    pub fn note_on(&mut self, note: u8, vel: f32, end_tick: u64) {
        self.counter += 1;
        let idx = self
            .voices
            .iter()
            .position(|v| !v.active)
            .unwrap_or_else(|| {
                self.voices
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, v)| v.age)
                    .map(|(i, _)| i)
                    .unwrap_or(0)
            });
        let v = &mut self.voices[idx];
        v.active = true;
        v.note = note;
        v.vel = vel;
        v.phase = 0.0;
        v.phase2 = 0.0;
        v.env = Adsr::new();
        v.env.gate_on();
        v.lp.reset();
        v.hp.reset();
        v.end_tick = end_tick;
        v.age = self.counter;
        v.freq = midi_to_freq(note as f32);
        v.tgt = v.freq;
        v.gl = 0.0;
    }

    /// FL-style slide: the newest sounding voice glides to `note` over the length of the slide note instead of a new voice starting.
    /// Returns false when nothing is sounding (the caller then plays the note normally).
    pub fn slide_to(&mut self, note: u8, vel: f32, end_tick: u64, glide_s: f32, sr: f32) -> bool {
        let Some(v) = self.voices.iter_mut().filter(|v| v.active && !v.env.is_released()).max_by_key(|v| v.age) else { return false };
        v.note = note;
        v.vel = vel;
        v.end_tick = end_tick;
        v.tgt = midi_to_freq(note as f32);
        v.gl = 1.0 - (-1.0 / (glide_s.max(0.01) * 0.3 * sr)).exp();
        true
    }

    /// Release voices held by a live key.
    pub fn note_off(&mut self, note: u8) {
        for v in self.voices.iter_mut() {
            if v.active && v.note == note && v.end_tick == u64::MAX && !v.env.is_released() {
                v.env.gate_off();
            }
        }
    }

    pub fn release_due(&mut self, abs_tick: u64) {
        for v in self.voices.iter_mut() {
            if v.active && v.end_tick <= abs_tick && !v.env.is_released() {
                v.env.gate_off();
            }
        }
    }

    pub fn release_all(&mut self) {
        for v in self.voices.iter_mut() {
            if v.active {
                v.env.gate_off();
            }
        }
    }

    #[inline]
    pub fn render_sample(&mut self, p: &SynthParams, sr: f32) -> f32 {
        if !self.voices.iter().any(|v| v.active) {
            return 0.0;
        }
        let lfo = (self.lfo_phase * TAU).fsin();
        self.lfo_phase += p.lfo_rate / sr;
        if self.lfo_phase >= 1.0 {
            self.lfo_phase -= 1.0;
        }
        let mut out = 0.0;
        let tune = p.semitones + p.fine / 100.0;
        let semis = if p.lfo_target == LfoTarget::Pitch { tune + lfo * p.lfo_depth * 2.0 } else { tune };
        let cut_mod = if p.lfo_target == LfoTarget::Cutoff { (lfo * p.lfo_depth * 3.0).fexp2() } else { 1.0 };
        let amp_mod = if p.lfo_target == LfoTarget::Amp { 1.0 - p.lfo_depth * (0.5 + 0.5 * lfo) } else { 1.0 };
        let cutoff = (p.lp_cutoff * cut_mod).clamp(20.0, 20000.0);
        let det = (p.osc2_detune / 1200.0).fexp2();
        let pitch_mul = (semis / 12.0).fexp2();
        for v in self.voices.iter_mut() {
            if !v.active {
                continue;
            }
            if v.gl > 0.0 {
                v.freq += (v.tgt - v.freq) * v.gl;
            }
            let f = v.freq * pitch_mul;
            let inc1 = (f / sr).min(0.45);
            let inc2 = (f * det / sr).min(0.45);
            let mut s = osc(p.wave, v.phase, inc1);
            if p.osc2_mix > 0.001 {
                s = (s + osc(p.wave, v.phase2, inc2) * p.osc2_mix) / (1.0 + p.osc2_mix);
            }
            v.phase += inc1;
            if v.phase >= 1.0 {
                v.phase -= 1.0;
            }
            v.phase2 += inc2;
            if v.phase2 >= 1.0 {
                v.phase2 -= 1.0;
            }
            let e = v.env.next(p.attack, p.decay, p.sustain, p.release, sr);
            let (lp, _) = v.lp.process(s, cutoff, p.lp_res, sr);
            let (_, hp) = v.hp.process(lp, p.hp_cutoff, 0.0, sr);
            out += hp * e * v.vel * amp_mod;
            if v.env.is_idle() {
                v.active = false;
            }
        }
        out * p.volume * 0.50
    }
}

#[cfg(test)]
mod slide_tests {
    use super::*;

    #[test]
    fn a_slide_reuses_the_sounding_voice_and_glides_to_the_new_pitch() {
        let mut s = Synth::new();
        let p = SynthParams::default();
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
        let mut e = Synth::new();
        assert!(!e.slide_to(60, 0.8, u64::MAX, 0.2, 48000.0));
    }
}
