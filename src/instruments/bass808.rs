//! Monophonic 808 bass: sine body, pitch drop, glide (slides), saturation.
use super::Bass808Params;
use crate::audio_engine::dsp::Adsr;
use crate::effects::{saturate, Svf};
use crate::piano_roll::midi_to_freq;
use std::f32::consts::TAU;
use crate::audio_engine::fastmath::Fast;

pub struct Bass808 {
    active: bool,
    vel: f32,
    phase: f32,
    freq_cur: f32,
    freq_tgt: f32,
    pitch_t: f32,
    env: Adsr,
    lp: Svf,
    end_tick: u64,
}

impl Bass808 {
    pub fn new() -> Self {
        Bass808 {
            active: false,
            vel: 0.0,
            phase: 0.0,
            freq_cur: 55.0,
            freq_tgt: 55.0,
            pitch_t: 0.0,
            env: Adsr::new(),
            lp: Svf::default(),
            end_tick: u64::MAX,
        }
    }

    pub fn note_on(&mut self, note: u8, vel: f32, slide: bool, end_tick: u64) {
        let f = midi_to_freq(note as f32);
        if slide && self.active && !self.env.is_idle() {
            // legato slide: keep envelope and phase, glide the pitch
            self.freq_tgt = f;
            self.vel = vel;
            self.end_tick = end_tick;
            if self.env.is_released() {
                self.env.gate_on();
            }
        } else {
            self.active = true;
            self.freq_cur = f;
            self.freq_tgt = f;
            self.vel = vel;
            self.phase = 0.0;
            self.pitch_t = 0.0;
            self.env = Adsr::new();
            self.env.gate_on();
            self.end_tick = end_tick;
        }
    }

    pub fn note_off(&mut self) {
        if self.end_tick == u64::MAX {
            self.env.gate_off();
        }
    }

    pub fn release_due(&mut self, abs_tick: u64) {
        if self.active && self.end_tick <= abs_tick && !self.env.is_released() {
            self.env.gate_off();
        }
    }

    pub fn release_all(&mut self) {
        self.env.gate_off();
    }

    #[inline]
    pub fn render_sample(&mut self, p: &Bass808Params, sr: f32) -> f32 {
        if !self.active {
            return 0.0;
        }
        let coef = if p.glide_ms < 0.5 { 1.0 } else { 1.0 - (-1.0 / (p.glide_ms * 0.001 * sr)).fexp() };
        self.freq_cur += (self.freq_tgt - self.freq_cur) * coef;
        let drop = (p.pitch_drop / 12.0 * (-self.pitch_t / (p.pitch_time_ms * 0.001).max(0.001)).fexp()).fexp2();
        let f = self.freq_cur * drop;
        self.phase += f / sr;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        self.pitch_t += 1.0 / sr;
        let e = self.env.next(p.attack, p.decay, p.sustain, p.release, sr);
        let body = (self.phase * TAU).fsin();
        let s = saturate(body, p.drive) * e;
        let (lp, _) = self.lp.process(s, p.tone, 0.0, sr);
        if self.env.is_idle() {
            self.active = false;
        }
        lp * self.vel * p.volume
    }
}
