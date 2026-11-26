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
            "Velvet Sub" => {
                p.wave = Wave::Sine;
                p.semitones = -12.0;
                p.attack = 0.015;
                p.decay = 0.42;
                p.sustain = 0.82;
                p.release = 0.16;
                p.hp_cutoff = 20.0;
                p.lp_cutoff = 460.0;
                p.osc2_mix = 0.0;
            }
            "Rubber Bass" => {
                p.wave = Wave::Square;
                p.attack = 0.003;
                p.decay = 0.19;
                p.sustain = 0.45;
                p.release = 0.09;
                p.lp_cutoff = 630.0;
                p.lp_res = 0.57;
                p.lfo_target = LfoTarget::Cutoff;
                p.lfo_depth = 0.28;
                p.lfo_rate = 3.2;
                p.osc2_mix = 0.16;
            }
            "Twin Saw Bass" => {
                p.wave = Wave::Saw;
                p.attack = 0.009;
                p.decay = 0.32;
                p.sustain = 0.77;
                p.release = 0.13;
                p.lp_cutoff = 1100.0;
                p.lp_res = 0.22;
                p.osc2_detune = 23.0;
                p.osc2_mix = 0.92;
                p.volume = 0.58;
            }
            "Pulse Runner" => {
                p.wave = Wave::Square;
                p.attack = 0.007;
                p.decay = 0.24;
                p.sustain = 0.73;
                p.release = 0.17;
                p.lp_cutoff = 4300.0;
                p.hp_cutoff = 155.0;
                p.osc2_mix = 0.23;
                p.lfo_depth = 0.055;
                p.lfo_rate = 5.9;
            }
            "Octave Signal" => {
                p.wave = Wave::Triangle;
                p.attack = 0.021;
                p.decay = 0.35;
                p.sustain = 0.7;
                p.release = 0.28;
                p.lp_cutoff = 6400.0;
                p.hp_cutoff = 125.0;
                p.osc2_detune = 1200.0;
                p.osc2_mix = 0.43;
                p.lfo_depth = 0.07;
                p.lfo_rate = 4.1;
            }
            "Polar Haze" => {
                p.wave = Wave::Saw;
                p.attack = 0.87;
                p.decay = 0.61;
                p.sustain = 0.82;
                p.release = 1.75;
                p.lp_cutoff = 1350.0;
                p.hp_cutoff = 175.0;
                p.osc2_detune = 17.0;
                p.osc2_mix = 0.78;
                p.lfo_target = LfoTarget::Cutoff;
                p.lfo_depth = 0.28;
                p.lfo_rate = 0.31;
                p.volume = 0.48;
            }
            "Tidal Pad" => {
                p.wave = Wave::Triangle;
                p.attack = 0.36;
                p.decay = 0.86;
                p.sustain = 0.68;
                p.release = 1.3;
                p.lp_cutoff = 2700.0;
                p.osc2_detune = 1200.0;
                p.osc2_mix = 0.22;
                p.lfo_target = LfoTarget::Amp;
                p.lfo_depth = 0.38;
                p.lfo_rate = 1.65;
                p.volume = 0.54;
            }
            "Late Night Keys" => {
                p.wave = Wave::Triangle;
                p.attack = 0.012;
                p.decay = 0.62;
                p.sustain = 0.24;
                p.release = 0.33;
                p.lp_cutoff = 2450.0;
                p.hp_cutoff = 95.0;
                p.osc2_detune = 5.0;
                p.osc2_mix = 0.2;
            }
            "Soft Organ" => {
                p.wave = Wave::Sine;
                p.attack = 0.016;
                p.decay = 0.15;
                p.sustain = 0.91;
                p.release = 0.13;
                p.lp_cutoff = 5200.0;
                p.osc2_detune = 1200.0;
                p.osc2_mix = 0.55;
                p.lfo_target = LfoTarget::Amp;
                p.lfo_depth = 0.17;
                p.lfo_rate = 4.4;
            }
            "Glass Pluck" => {
                p.wave = Wave::Triangle;
                p.attack = 0.002;
                p.decay = 0.26;
                p.sustain = 0.03;
                p.release = 0.22;
                p.lp_cutoff = 7900.0;
                p.hp_cutoff = 180.0;
                p.osc2_detune = 1200.0;
                p.osc2_mix = 0.57;
            }
            "Club Stab" => {
                p.wave = Wave::Saw;
                p.attack = 0.003;
                p.decay = 0.31;
                p.sustain = 0.09;
                p.release = 0.15;
                p.lp_cutoff = 3100.0;
                p.lp_res = 0.48;
                p.hp_cutoff = 205.0;
                p.osc2_detune = 12.0;
                p.osc2_mix = 0.7;
                p.volume = 0.62;
            }
            "Ice Chime" => {
                p.wave = Wave::Sine;
                p.attack = 0.002;
                p.decay = 1.05;
                p.sustain = 0.015;
                p.release = 0.95;
                p.lp_cutoff = 9600.0;
                p.hp_cutoff = 240.0;
                p.osc2_detune = 700.0;
                p.osc2_mix = 0.72;
                p.lfo_depth = 0.018;
                p.lfo_rate = 3.9;
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

impl Bass808Params {
    pub fn preset(name: &str) -> Self {
        let base = Self { glide_ms: 40.0, decay: 1.2, sustain: 0.5, release: 0.2,
            pitch_drop: 8.0, tone: 2400.0, drive: 3.0, ..Self::default() };
        match name {
            "808 Dirty" => Self { drive: 8.0, tone: 3500.0, ..base },
            "808 Drift" => Self {
                drive: 5.2, glide_ms: 120.0, decay: 1.0,
                sustain: 0.64, release: 0.29, pitch_drop: 13.0,
                pitch_time_ms: 27.0, tone: 2400.0, volume: 0.56,
                ..Self::default()
            },
            "808 Soft Floor" => Self {
                drive: 1.1, glide_ms: 80.0, decay: 1.8,
                sustain: 0.78, release: 0.35, pitch_drop: 1.5,
                pitch_time_ms: 22.0, tone: 1300.0, volume: 0.62,
                ..Self::default()
            },
            "808 Short Knock" => Self {
                drive: 2.0, glide_ms: 28.0, decay: 0.25,
                sustain: 0.08, release: 0.075, pitch_drop: 18.0,
                pitch_time_ms: 34.0, tone: 3000.0, volume: 0.64,
                ..Self::default()
            },
            "808 Tape Long" => Self {
                drive: 3.1, glide_ms: 145.0, decay: 2.4,
                sustain: 0.82, release: 0.54, pitch_drop: 4.0,
                pitch_time_ms: 31.0, tone: 1850.0, volume: 0.56,
                ..Self::default()
            },
            "808 Brick" => Self {
                drive: 9.0, glide_ms: 67.0, decay: 0.76,
                sustain: 0.56, release: 0.16, pitch_drop: 17.0,
                pitch_time_ms: 49.0, tone: 3600.0, volume: 0.54,
                ..Self::default()
            },
            "808 Glide Bow" => Self {
                attack: 0.052, drive: 1.8, glide_ms: 205.0,
                decay: 1.5, sustain: 0.72, release: 0.43,
                pitch_drop: 0.0, tone: 1550.0, volume: 0.60,
                ..Self::default()
            },
            _ => base,
        }
    }
}
