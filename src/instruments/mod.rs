//! Built-in instruments: polyphonic synth, 808 bass, drum machine voices.
pub mod bass808;
pub mod sampler;
pub mod params;
pub mod drums;
pub mod synth;
pub mod wavetable;

pub mod analog;
pub use params::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum Wave {
    Saw,
    Square,
    Sine,
    Triangle,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum LfoTarget {
    Pitch,
    Cutoff,
    Amp,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SynthParams {
    pub wave: Wave,
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
    pub lp_cutoff: f32,
    pub lp_res: f32,
    pub hp_cutoff: f32,
    pub lfo_rate: f32,
    pub lfo_depth: f32,
    pub lfo_target: LfoTarget,
    /// coarse tuning in semitones
    pub semitones: f32,
    /// fine tuning in cents
    pub fine: f32,
    /// second oscillator detune (cents) and mix
    pub osc2_detune: f32,
    pub osc2_mix: f32,
    pub volume: f32,
}

impl Default for SynthParams {
    fn default() -> Self {
        SynthParams {
            wave: Wave::Saw,
            attack: 0.01,
            decay: 0.3,
            sustain: 0.6,
            release: 0.25,
            lp_cutoff: 5000.0,
            lp_res: 0.2,
            hp_cutoff: 40.0,
            lfo_rate: 4.5,
            lfo_depth: 0.0,
            lfo_target: LfoTarget::Pitch,
            semitones: 0.0,
            fine: 0.0,
            osc2_detune: 8.0,
            osc2_mix: 0.5,
            volume: 0.7,
        }
    }
}

impl SynthParams {
    pub fn preset(name: &str) -> Self {
        let mut p = SynthParams::default();
        match name {
            "Pluck" => {
                p.wave = Wave::Square;
                p.attack = 0.002;
                p.decay = 0.18;
                p.sustain = 0.0;
                p.release = 0.12;
                p.lp_cutoff = 2800.0;
                p.lp_res = 0.35;
            }
            "Pad" => {
                p.wave = Wave::Saw;
                p.attack = 0.6;
                p.decay = 0.8;
                p.sustain = 0.8;
                p.release = 1.2;
                p.lp_cutoff = 1800.0;
                p.osc2_detune = 14.0;
                p.lfo_target = LfoTarget::Cutoff;
                p.lfo_depth = 0.25;
                p.lfo_rate = 0.4;
            }
            "Lead" => {
                p.wave = Wave::Saw;
                p.attack = 0.005;
                p.decay = 0.2;
                p.sustain = 0.75;
                p.release = 0.2;
                p.lp_cutoff = 7500.0;
                p.lp_res = 0.3;
                p.lfo_depth = 0.08;
                p.lfo_rate = 5.5;
            }
            "Bell" => {
                p.wave = Wave::Sine;
                p.attack = 0.001;
                p.decay = 1.2;
                p.sustain = 0.0;
                p.release = 0.8;
                p.osc2_detune = 1200.0;
                p.osc2_mix = 0.35;
                p.lp_cutoff = 12000.0;
            }
            _ => {}
        }
        p
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Bass808Params {
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
    /// glide time of slides in milliseconds
    pub glide_ms: f32,
    /// initial pitch drop in semitones and its time in milliseconds
    pub pitch_drop: f32,
    pub pitch_time_ms: f32,
    pub drive: f32,
    pub tone: f32,
    pub volume: f32,
}

impl Default for Bass808Params {
    fn default() -> Self {
        Bass808Params {
            attack: 0.002,
            decay: 0.9,
            sustain: 0.55,
            release: 0.18,
            glide_ms: 90.0,
            pitch_drop: 10.0,
            pitch_time_ms: 40.0,
            drive: 2.5,
            tone: 3500.0,
            volume: 0.9,
        }
    }
}
