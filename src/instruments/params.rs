//! Parameter sets of the Wavetable (Serum-style) and Analog (Diva-style) synths.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum FilterType {
    Lp,
    Hp,
    Bp,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum WtTarget {
    Position,
    Cutoff,
    Pitch,
    Amp,
}

/// Two wavetable oscillators with morph position, sub, noise, unison, multimode filter,
/// amp + filter envelopes and an LFO.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WavetableParams {
    pub table_a: usize,
    pub pos_a: f32,
    pub oct_a: f32,
    pub level_a: f32,
    pub table_b: usize,
    pub pos_b: f32,
    pub oct_b: f32,
    pub level_b: f32,
    pub sub: f32,
    pub noise: f32,
    pub unison: u8,
    pub uni_detune: f32,
    pub filter: FilterType,
    pub cutoff: f32,
    pub res: f32,
    pub env_amt: f32,
    pub pos_env: f32,
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
    pub f_attack: f32,
    pub f_decay: f32,
    pub f_sustain: f32,
    pub f_release: f32,
    pub lfo_rate: f32,
    pub lfo_depth: f32,
    pub lfo_target: WtTarget,
    pub volume: f32,
}

impl Default for WavetableParams {
    fn default() -> Self {
        WavetableParams {
            table_a: 0,
            pos_a: 0.35,
            oct_a: 0.0,
            level_a: 0.8,
            table_b: 1,
            pos_b: 0.2,
            oct_b: 0.0,
            level_b: 0.0,
            sub: 0.0,
            noise: 0.0,
            unison: 3,
            uni_detune: 14.0,
            filter: FilterType::Lp,
            cutoff: 7000.0,
            res: 0.15,
            env_amt: 0.0,
            pos_env: 0.0,
            attack: 0.005,
            decay: 0.4,
            sustain: 0.7,
            release: 0.3,
            f_attack: 0.005,
            f_decay: 0.4,
            f_sustain: 0.3,
            f_release: 0.3,
            lfo_rate: 3.0,
            lfo_depth: 0.0,
            lfo_target: WtTarget::Position,
            volume: 0.7,
        }
    }
}

impl WavetableParams {
    pub fn preset(name: &str) -> Self {
        let mut p = WavetableParams::default();
        match name {
            "Growl Bass" => {
                p.table_a = 1;
                p.pos_a = 0.5;
                p.unison = 2;
                p.uni_detune = 8.0;
                p.sub = 0.5;
                p.cutoff = 900.0;
                p.res = 0.35;
                p.env_amt = 2.5;
                p.f_decay = 0.35;
                p.f_sustain = 0.0;
                p.pos_env = 0.4;
                p.lfo_depth = 0.35;
                p.lfo_rate = 2.0;
            }
            "Vowel Pad" => {
                p.table_a = 2;
                p.pos_a = 0.3;
                p.attack = 0.5;
                p.release = 1.2;
                p.unison = 5;
                p.uni_detune = 18.0;
                p.cutoff = 5000.0;
                p.lfo_depth = 0.5;
                p.lfo_rate = 0.25;
            }
            "Metal Pluck" => {
                p.table_a = 3;
                p.pos_a = 0.6;
                p.attack = 0.001;
                p.decay = 0.25;
                p.sustain = 0.0;
                p.release = 0.2;
                p.unison = 1;
                p.cutoff = 3000.0;
                p.env_amt = 1.5;
                p.f_decay = 0.2;
                p.f_sustain = 0.0;
            }
            "Super Saw" => {
                p.table_a = 0;
                p.pos_a = 0.66;
                p.unison = 7;
                p.uni_detune = 22.0;
                p.cutoff = 9000.0;
                p.attack = 0.01;
            }
            "Acoustic Guitar" => {
                p.table_a = 9;
                p.unison = 1;
                p.attack = 0.001;
                p.pos_a = 0.2;
                p.pos_env = 0.5;
                p.decay = 1.6;
                p.sustain = 0.0;
                p.release = 0.3;
                p.cutoff = 2200.0;
                p.env_amt = 2.2;
                p.f_decay = 0.5;
                p.f_sustain = 0.1;
                p.res = 0.05;
                p.volume = 0.8;
            }
            "Nylon Guitar" => {
                p.table_a = 9;
                p.unison = 1;
                p.attack = 0.001;
                p.pos_a = 0.45;
                p.pos_env = 0.2;
                p.decay = 1.3;
                p.sustain = 0.0;
                p.release = 0.3;
                p.cutoff = 1500.0;
                p.env_amt = 1.8;
                p.f_decay = 0.4;
                p.f_sustain = 0.1;
                p.res = 0.05;
                p.volume = 0.85;
            }
            "Clean Electric" => {
                p.table_a = 9;
                p.unison = 1;
                p.attack = 0.001;
                p.pos_a = 0.3;
                p.table_b = 9;
                p.pos_b = 0.7;
                p.level_b = 0.4;
                p.decay = 2.0;
                p.sustain = 0.1;
                p.release = 0.35;
                p.cutoff = 3600.0;
                p.env_amt = 1.2;
                p.f_decay = 0.8;
                p.f_sustain = 0.3;
                p.uni_detune = 4.0;
                p.unison = 2;
            }
            "Overdrive Guitar" => {
                p.table_a = 9;
                p.pos_a = 0.1;
                p.table_b = 0;
                p.pos_b = 0.66;
                p.level_b = 0.45;
                p.unison = 3;
                p.uni_detune = 6.0;
                p.attack = 0.002;
                p.decay = 1.0;
                p.sustain = 0.7;
                p.release = 0.2;
                p.cutoff = 3800.0;
                p.res = 0.25;
                p.env_amt = 1.0;
                p.f_decay = 0.6;
                p.f_sustain = 0.5;
                p.volume = 0.65;
            }
            "Muted Guitar" => {
                p.table_a = 9;
                p.unison = 1;
                p.attack = 0.001;
                p.pos_a = 0.1;
                p.decay = 0.22;
                p.sustain = 0.0;
                p.release = 0.08;
                p.cutoff = 1400.0;
                p.env_amt = 1.2;
                p.f_decay = 0.15;
                p.f_sustain = 0.0;
                p.res = 0.1;
                p.volume = 0.9;
            }
            "12 String" => {
                p.table_a = 9;
                p.unison = 1;
                p.attack = 0.001;
                p.pos_a = 0.25;
                p.table_b = 9;
                p.pos_b = 0.25;
                p.oct_b = 1.0;
                p.level_b = 0.35;
                p.unison = 3;
                p.uni_detune = 9.0;
                p.decay = 1.8;
                p.sustain = 0.0;
                p.release = 0.35;
                p.cutoff = 4200.0;
                p.env_amt = 1.5;
                p.f_decay = 0.6;
                p.f_sustain = 0.15;
            }
            "Donk" => {
                p.table_a = 0;
                p.pos_a = 0.9;
                p.unison = 1;
                p.sub = 0.3;
                p.attack = 0.001;
                p.decay = 0.22;
                p.sustain = 0.0;
                p.release = 0.1;
                p.cutoff = 320.0;
                p.res = 0.72;
                p.env_amt = 3.2;
                p.f_attack = 0.001;
                p.f_decay = 0.09;
                p.f_sustain = 0.0;
                p.f_release = 0.1;
                p.volume = 0.85;
            }
            "Hardbass Donk" => {
                p.table_a = 0;
                p.pos_a = 0.95;
                p.table_b = 0;
                p.pos_b = 0.66;
                p.level_b = 0.35;
                p.unison = 2;
                p.uni_detune = 6.0;
                p.sub = 0.45;
                p.attack = 0.001;
                p.decay = 0.18;
                p.sustain = 0.0;
                p.release = 0.08;
                p.cutoff = 260.0;
                p.res = 0.8;
                p.env_amt = 3.6;
                p.f_attack = 0.001;
                p.f_decay = 0.07;
                p.f_sustain = 0.0;
                p.f_release = 0.08;
                p.volume = 0.9;
            }
            "Hard Sub" => {
                p.table_a = 0;
                p.pos_a = 0.0;
                p.unison = 1;
                p.sub = 0.5;
                p.attack = 0.002;
                p.decay = 0.3;
                p.sustain = 0.9;
                p.release = 0.1;
                p.cutoff = 420.0;
                p.res = 0.1;
                p.volume = 0.9;
            }
            "Reese Bass" => {
                p.table_a = 0;
                p.pos_a = 0.66;
                p.table_b = 0;
                p.pos_b = 0.66;
                p.level_b = 0.7;
                p.oct_b = -12.0;
                p.unison = 2;
                p.uni_detune = 24.0;
                p.sub = 0.3;
                p.attack = 0.005;
                p.sustain = 0.9;
                p.release = 0.15;
                p.cutoff = 650.0;
                p.res = 0.3;
                p.lfo_target = WtTarget::Cutoff;
                p.lfo_rate = 0.6;
                p.lfo_depth = 0.5;
                p.volume = 0.8;
            }
            "Screech Lead" => {
                p.table_a = 0;
                p.pos_a = 0.66;
                p.table_b = 7;
                p.pos_b = 0.9;
                p.level_b = 0.4;
                p.unison = 3;
                p.uni_detune = 12.0;
                p.attack = 0.003;
                p.decay = 0.2;
                p.sustain = 0.85;
                p.release = 0.12;
                p.cutoff = 5200.0;
                p.res = 0.5;
                p.env_amt = 1.2;
                p.f_decay = 0.2;
                p.f_sustain = 0.5;
                p.volume = 0.75;
            }
            "Donk Funk" => {
                p.table_a = 0;
                p.pos_a = 0.8;
                p.unison = 1;
                p.sub = 0.25;
                p.attack = 0.001;
                p.decay = 0.28;
                p.sustain = 0.0;
                p.release = 0.12;
                p.cutoff = 380.0;
                p.res = 0.6;
                p.env_amt = 2.8;
                p.f_attack = 0.001;
                p.f_decay = 0.12;
                p.f_sustain = 0.0;
                p.f_release = 0.1;
                p.volume = 0.85;
            }
            "Funk Bass" => {
                p.table_a = 0;
                p.pos_a = 0.66;
                p.unison = 1;
                p.sub = 0.3;
                p.attack = 0.002;
                p.decay = 0.25;
                p.sustain = 0.3;
                p.release = 0.1;
                p.cutoff = 480.0;
                p.res = 0.35;
                p.env_amt = 2.6;
                p.f_decay = 0.16;
                p.f_sustain = 0.1;
                p.volume = 0.85;
            }
            "Funk Stab" => {
                p.table_a = 0;
                p.pos_a = 0.7;
                p.unison = 3;
                p.uni_detune = 8.0;
                p.attack = 0.002;
                p.decay = 0.15;
                p.sustain = 0.0;
                p.release = 0.1;
                p.cutoff = 3000.0;
                p.res = 0.15;
                p.env_amt = 1.2;
                p.f_decay = 0.12;
                p.f_sustain = 0.0;
                p.volume = 0.8;
            }
            "Drift House Stab" => {
                p.table_a = 8;
                p.pos_a = 0.8;
                p.unison = 5;
                p.uni_detune = 14.0;
                p.attack = 0.003;
                p.decay = 0.25;
                p.sustain = 0.0;
                p.release = 0.2;
                p.cutoff = 4200.0;
                p.res = 0.12;
                p.env_amt = 1.2;
                p.f_decay = 0.2;
                p.f_sustain = 0.0;
                p.volume = 0.75;
            }
            "Deep House Chord" => {
                p.table_a = 8;
                p.pos_a = 0.5;
                p.table_b = 5;
                p.pos_b = 0.5;
                p.level_b = 0.4;
                p.unison = 3;
                p.uni_detune = 9.0;
                p.attack = 0.01;
                p.decay = 0.5;
                p.sustain = 0.35;
                p.release = 0.35;
                p.cutoff = 2600.0;
                p.res = 0.15;
                p.volume = 0.8;
            }
            "Drawbar Organ" => {
                p.table_a = 4;
                p.pos_a = 0.85;
                p.unison = 1;
                p.attack = 0.004;
                p.decay = 0.1;
                p.sustain = 1.0;
                p.release = 0.05;
                p.cutoff = 9500.0;
                p.res = 0.05;
                p.lfo_target = WtTarget::Pitch;
                p.lfo_rate = 6.0;
                p.lfo_depth = 0.04;
            }
            "Rock Organ" => {
                p.table_a = 4;
                p.pos_a = 1.0;
                p.table_b = 4;
                p.pos_b = 0.4;
                p.oct_b = 1.0;
                p.level_b = 0.35;
                p.unison = 2;
                p.uni_detune = 6.0;
                p.attack = 0.003;
                p.sustain = 1.0;
                p.release = 0.08;
                p.cutoff = 7000.0;
                p.lfo_target = WtTarget::Amp;
                p.lfo_rate = 6.5;
                p.lfo_depth = 0.12;
            }
            "Flute" => {
                p.table_a = 5;
                p.pos_a = 0.35;
                p.unison = 1;
                p.noise = 0.04;
                p.attack = 0.06;
                p.decay = 0.2;
                p.sustain = 0.85;
                p.release = 0.18;
                p.cutoff = 6500.0;
                p.res = 0.05;
                p.lfo_target = WtTarget::Pitch;
                p.lfo_rate = 5.2;
                p.lfo_depth = 0.08;
                p.volume = 0.8;
            }
            "Pan Flute" => {
                p.table_a = 5;
                p.pos_a = 0.15;
                p.unison = 1;
                p.noise = 0.12;
                p.attack = 0.04;
                p.sustain = 0.7;
                p.release = 0.25;
                p.cutoff = 4800.0;
                p.volume = 0.85;
            }
            "Accordion" => {
                p.table_a = 6;
                p.pos_a = 0.7;
                p.table_b = 6;
                p.pos_b = 0.7;
                p.level_b = 0.7;
                p.unison = 3;
                p.uni_detune = 9.0;
                p.attack = 0.05;
                p.decay = 0.15;
                p.sustain = 0.9;
                p.release = 0.12;
                p.cutoff = 4500.0;
                p.res = 0.1;
                p.lfo_target = WtTarget::Amp;
                p.lfo_rate = 5.5;
                p.lfo_depth = 0.1;
            }
            "Clarinet" => {
                p.table_a = 6;
                p.pos_a = 0.0;
                p.unison = 1;
                p.attack = 0.04;
                p.sustain = 0.85;
                p.release = 0.12;
                p.cutoff = 3800.0;
                p.lfo_target = WtTarget::Pitch;
                p.lfo_rate = 5.0;
                p.lfo_depth = 0.05;
            }
            "Brass Lead" => {
                p.table_a = 7;
                p.pos_a = 0.3;
                p.unison = 2;
                p.uni_detune = 5.0;
                p.attack = 0.04;
                p.decay = 0.3;
                p.sustain = 0.8;
                p.release = 0.15;
                p.cutoff = 3000.0;
                p.env_amt = 2.0;
                p.pos_env = 0.6;
                p.f_attack = 0.06;
                p.f_decay = 0.4;
                p.f_sustain = 0.6;
            }
            "Warm Lead" => {
                p.table_a = 8;
                p.pos_a = 0.7;
                p.unison = 2;
                p.uni_detune = 7.0;
                p.sub = 0.15;
                p.attack = 0.01;
                p.decay = 0.3;
                p.sustain = 0.85;
                p.release = 0.2;
                p.cutoff = 5500.0;
                p.res = 0.12;
                p.lfo_target = WtTarget::Pitch;
                p.lfo_rate = 5.5;
                p.lfo_depth = 0.1;
            }
            "Square Lead" => {
                p.table_a = 0;
                p.pos_a = 1.0;
                p.unison = 3;
                p.uni_detune = 10.0;
                p.attack = 0.005;
                p.sustain = 0.8;
                p.release = 0.12;
                p.cutoff = 4200.0;
                p.res = 0.2;
                p.env_amt = 1.0;
                p.f_decay = 0.25;
            }
            "Epic Lead" => {
                p.table_a = 8;
                p.pos_a = 0.9;
                p.table_b = 7;
                p.pos_b = 0.5;
                p.level_b = 0.35;
                p.unison = 5;
                p.uni_detune = 14.0;
                p.sub = 0.2;
                p.attack = 0.02;
                p.decay = 0.4;
                p.sustain = 0.8;
                p.release = 0.35;
                p.cutoff = 7000.0;
            }
            "Choir Pad" => {
                p.table_a = 2;
                p.pos_a = 0.5;
                p.table_b = 5;
                p.pos_b = 0.4;
                p.level_b = 0.5;
                p.unison = 5;
                p.uni_detune = 12.0;
                p.attack = 0.7;
                p.release = 1.5;
                p.cutoff = 4200.0;
                p.lfo_target = WtTarget::Position;
                p.lfo_rate = 0.3;
                p.lfo_depth = 0.3;
            }
            "String Ensemble" => {
                p.table_a = 8;
                p.pos_a = 0.45;
                p.unison = 7;
                p.uni_detune = 16.0;
                p.attack = 0.35;
                p.decay = 0.4;
                p.sustain = 0.9;
                p.release = 0.8;
                p.cutoff = 3800.0;
                p.lfo_target = WtTarget::Pitch;
                p.lfo_rate = 4.8;
                p.lfo_depth = 0.05;
            }
            "Electric Piano" => {
                p.table_a = 5;
                p.pos_a = 0.9;
                p.table_b = 3;
                p.pos_b = 0.3;
                p.level_b = 0.12;
                p.oct_b = 2.0;
                p.unison = 1;
                p.attack = 0.002;
                p.decay = 1.2;
                p.sustain = 0.2;
                p.release = 0.4;
                p.cutoff = 4800.0;
                p.env_amt = 1.0;
                p.f_decay = 0.5;
            }
            "Glass Current" => {
                p.table_a = 3;
                p.pos_a = 0.68;
                p.level_b = 0.16;
                p.pos_env = 0.4;
                p.cutoff = 4700.0;
                p.decay = 0.32;
                p.sustain = 0.04;
                p.release = 0.27;
                p.volume = 0.49;
            }
            "Aurora Choir" => {
                p.table_a = 2;
                p.pos_a = 0.46;
                p.table_b = 2;
                p.pos_b = 0.79;
                p.level_b = 0.32;
                p.unison = 5;
                p.uni_detune = 14.0;
                p.attack = 0.62;
                p.release = 1.6;
                p.cutoff = 3200.0;
                p.lfo_rate = 0.32;
                p.lfo_depth = 0.38;
                p.volume = 0.50;
            }
            "Drift Cowbell" => {
                p.table_a = 3;
                p.pos_a = 0.36;
                p.table_b = 3;
                p.pos_b = 0.78;
                p.oct_b = 1.0;
                p.level_b = 0.28;
                p.attack = 0.001;
                p.decay = 0.24;
                p.sustain = 0.0;
                p.release = 0.15;
                p.filter = FilterType::Bp;
                p.cutoff = 2150.0;
                p.res = 0.36;
                p.volume = 0.59;
            }
            "Neon Reese" => {
                p.table_a = 1;
                p.pos_a = 0.73;
                p.level_b = 0.21;
                p.sub = 0.34;
                p.unison = 3;
                p.uni_detune = 21.0;
                p.cutoff = 1260.0;
                p.env_amt = 0.7;
                p.lfo_target = WtTarget::Cutoff;
                p.lfo_depth = 0.26;
                p.lfo_rate = 0.84;
                p.volume = 0.54;
            }
            "Orbital Lead" => {
                p.table_a = 0;
                p.pos_a = 0.71;
                p.table_b = 1;
                p.pos_b = 0.57;
                p.oct_b = 1.0;
                p.level_b = 0.23;
                p.unison = 3;
                p.uni_detune = 9.0;
                p.cutoff = 4300.0;
                p.env_amt = 0.8;
                p.lfo_target = WtTarget::Pitch;
                p.lfo_depth = 0.09;
                p.volume = 0.49;
            }
            "Afterhours Chord" => {
                p.table_a = 0;
                p.pos_a = 0.48;
                p.table_b = 2;
                p.pos_b = 0.36;
                p.level_b = 0.30;
                p.cutoff = 2100.0;
                p.env_amt = 1.8;
                p.f_decay = 0.21;
                p.f_sustain = 0.1;
                p.decay = 0.36;
                p.sustain = 0.26;
                p.release = 0.34;
                p.volume = 0.50;
            }
            _ => {}
        }
        p
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum AnalogWave {
    Saw,
    Pulse,
    Tri,
    Sine,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum AnTarget {
    Pitch,
    Pwm,
    Cutoff,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum LfoShape {
    Sine,
    Tri,
    Square,
}

/// 2 VCOs (PWM, hard sync, drift), sub, noise, 24 dB ladder filter with drive,
/// amp + filter envelopes, key tracking, LFO.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AnalogParams {
    pub wave1: AnalogWave,
    pub wave2: AnalogWave,
    pub semi1: f32,
    pub semi2: f32,
    pub detune2: f32,
    pub pw: f32,
    pub sync: bool,
    pub mix: f32,
    pub sub: f32,
    pub noise: f32,
    pub cutoff: f32,
    pub res: f32,
    pub drive: f32,
    pub env_amt: f32,
    pub keytrack: f32,
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
    pub f_attack: f32,
    pub f_decay: f32,
    pub f_sustain: f32,
    pub f_release: f32,
    pub lfo_rate: f32,
    pub lfo_depth: f32,
    pub lfo_shape: LfoShape,
    pub lfo_target: AnTarget,
    pub drift: f32,
    pub glide_ms: f32,
    pub volume: f32,
}

impl Default for AnalogParams {
    fn default() -> Self {
        AnalogParams {
            wave1: AnalogWave::Saw,
            wave2: AnalogWave::Saw,
            semi1: 0.0,
            semi2: 0.0,
            detune2: 7.0,
            pw: 0.5,
            sync: false,
            mix: 0.5,
            sub: 0.0,
            noise: 0.0,
            cutoff: 1800.0,
            res: 0.3,
            drive: 1.5,
            env_amt: 1.5,
            keytrack: 0.5,
            attack: 0.005,
            decay: 0.5,
            sustain: 0.6,
            release: 0.25,
            f_attack: 0.003,
            f_decay: 0.35,
            f_sustain: 0.2,
            f_release: 0.25,
            lfo_rate: 4.0,
            lfo_depth: 0.0,
            lfo_shape: LfoShape::Sine,
            lfo_target: AnTarget::Pwm,
            drift: 0.25,
            glide_ms: 0.0,
            volume: 0.7,
        }
    }
}

impl AnalogParams {
    pub fn preset(name: &str) -> Self {
        let mut p = AnalogParams::default();
        match name {
            "Moog Bass" => {
                p.wave2 = AnalogWave::Pulse;
                p.semi2 = -12.0;
                p.detune2 = 4.0;
                p.cutoff = 380.0;
                p.res = 0.45;
                p.env_amt = 3.0;
                p.f_decay = 0.3;
                p.f_sustain = 0.1;
                p.sustain = 0.8;
                p.drive = 2.5;
                p.keytrack = 0.3;
            }
            "PWM Strings" => {
                p.wave1 = AnalogWave::Pulse;
                p.wave2 = AnalogWave::Pulse;
                p.detune2 = 12.0;
                p.attack = 0.4;
                p.release = 0.8;
                p.lfo_depth = 0.6;
                p.lfo_rate = 0.7;
                p.cutoff = 3500.0;
                p.env_amt = 0.0;
            }
            "Sync Lead" => {
                p.sync = true;
                p.semi2 = 7.0;
                p.cutoff = 5000.0;
                p.res = 0.2;
                p.lfo_target = AnTarget::Pitch;
                p.lfo_depth = 0.12;
                p.glide_ms = 60.0;
            }
            "Soft Keys" => {
                p.wave1 = AnalogWave::Tri;
                p.wave2 = AnalogWave::Sine;
                p.semi2 = 12.0;
                p.mix = 0.3;
                p.attack = 0.002;
                p.decay = 0.9;
                p.sustain = 0.2;
                p.cutoff = 2600.0;
                p.env_amt = 1.0;
            }
            "Tape Keys" => {
                p.wave1 = AnalogWave::Tri;
                p.wave2 = AnalogWave::Sine;
                p.semi2 = 12.0;
                p.mix = 0.17;
                p.drift = 0.48;
                p.cutoff = 1920.0;
                p.decay = 0.82;
                p.sustain = 0.19;
                p.release = 0.48;
                p.volume = 0.58;
            }
            "Basement Acid" => {
                p.wave1 = AnalogWave::Saw;
                p.mix = 0.0;
                p.cutoff = 520.0;
                p.res = 0.64;
                p.drive = 1.9;
                p.env_amt = 3.0;
                p.f_decay = 0.19;
                p.f_sustain = 0.04;
                p.sustain = 0.44;
                p.release = 0.12;
                p.volume = 0.49;
            }
            "Silk Brass" => {
                p.wave2 = AnalogWave::Pulse;
                p.detune2 = 6.0;
                p.cutoff = 2300.0;
                p.env_amt = 1.15;
                p.f_attack = 0.13;
                p.attack = 0.075;
                p.f_decay = 0.36;
                p.sustain = 0.73;
                p.release = 0.28;
                p.volume = 0.50;
            }
            "Velvet Strings" => {
                p.wave1 = AnalogWave::Pulse;
                p.wave2 = AnalogWave::Saw;
                p.pw = 0.38;
                p.detune2 = -15.0;
                p.cutoff = 1750.0;
                p.attack = 0.48;
                p.release = 1.15;
                p.lfo_shape = LfoShape::Tri;
                p.lfo_rate = 0.56;
                p.lfo_depth = 0.35;
                p.volume = 0.49;
            }
            "Circuit Pulse" => {
                p.wave1 = AnalogWave::Pulse;
                p.wave2 = AnalogWave::Tri;
                p.pw = 0.25;
                p.mix = 0.30;
                p.cutoff = 2750.0;
                p.res = 0.28;
                p.attack = 0.002;
                p.decay = 0.16;
                p.sustain = 0.03;
                p.release = 0.16;
                p.env_amt = 1.2;
                p.volume = 0.53;
            }
            "Low Orbit" => {
                p.wave1 = AnalogWave::Tri;
                p.wave2 = AnalogWave::Pulse;
                p.mix = 0.28;
                p.sub = 0.32;
                p.cutoff = 440.0;
                p.env_amt = 1.65;
                p.f_decay = 0.29;
                p.glide_ms = 85.0;
                p.drive = 1.7;
                p.volume = 0.56;
            }
            _ => {}
        }
        p
    }
}
