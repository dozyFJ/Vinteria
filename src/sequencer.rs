//! Step sequencer model: patterns with drum lanes, plus the pattern-level note list.
use crate::piano_roll::Note;
use serde::{Deserialize, Serialize};

pub const PPQ: u32 = 96;
pub const TICKS_PER_STEP: u32 = PPQ / 4;
pub const TICKS_PER_BAR: u32 = PPQ * 4;
pub const MAX_STEPS: usize = 256;
pub const NUM_DRUMS: usize = 5;
pub const DRUM_NAMES: [&str; NUM_DRUMS] = ["Kick", "Snare", "Clap", "Hi-Hat", "Perc"];

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Step {
    pub on: bool,
    pub vel: u8,
    /// Smart Step: pitch offset in semitones (-24..=24)
    #[serde(default)]
    pub pitch: i8,
    /// Smart Step: pan -100 (left) ..= 100 (right)
    #[serde(default)]
    pub pan: i8,
    /// Smart Step: glide - the note starts an octave up and slides into its pitch
    #[serde(default)]
    pub slide: bool,
    /// Smart Step: probability that the step plays, 1..=100 (%)
    #[serde(default = "full_prob")]
    pub prob: u8,
}

fn full_prob() -> u8 {
    100
}

impl Step {
    pub fn new(on: bool, vel: u8) -> Self {
        Step { on, vel, pitch: 0, pan: 0, slide: false, prob: 100 }
    }
    /// true when any Smart Step value differs from the plain step
    pub fn is_smart(&self) -> bool {
        self.pitch != 0 || self.pan != 0 || self.slide || self.prob < 100
    }
}

impl Default for Step {
    fn default() -> Self {
        Step::new(false, 100)
    }
}

/// Deterministic per-pass dice for step probability: a different roll on every loop of the pattern.
pub fn step_passes(prob: u8, pass: u64, step: usize, lane: usize) -> bool {
    if prob >= 100 {
        return true;
    }
    let h = (pass.wrapping_mul(0x9E37_79B9) ^ (step as u64).wrapping_mul(40503) ^ (lane as u64).wrapping_mul(2654435761)) >> 5;
    (h % 100) as u8 <= prob.saturating_sub(1)
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Pattern {
    pub name: String,
    /// 16, 32 or 64 sixteenth-note steps.
    pub length_steps: u32,
    pub drums: Vec<Vec<Step>>,
    pub notes: Vec<Note>,
    /// content of a MIDI clip of the Playlist: it belongs to that clip alone and is not listed among the patterns
    #[serde(default)]
    pub clip_only: bool,
}

impl Pattern {
    pub fn new(name: &str, length_steps: u32) -> Self {
        Pattern {
            name: name.to_string(),
            length_steps,
            drums: vec![vec![Step::default(); MAX_STEPS]; NUM_DRUMS],
            notes: Vec::new(),
            clip_only: false,
        }
    }
    pub fn len_ticks(&self) -> u32 {
        self.length_steps * TICKS_PER_STEP
    }
    pub fn set_steps(&mut self, lane: usize, steps: &[usize], vel: u8) {
        for &s in steps {
            self.drums[lane][s] = Step::new(true, vel);
        }
    }
}

/// Which step starts exactly at `local` tick, taking swing into account.
/// Odd (off-beat) 16th steps are delayed by `swing_ticks`.
pub fn step_at(local: u32, swing_ticks: u32) -> Option<usize> {
    if swing_ticks == 0 {
        return (local % TICKS_PER_STEP == 0).then(|| (local / TICKS_PER_STEP) as usize);
    }
    if local % TICKS_PER_STEP == 0 && (local / TICKS_PER_STEP) % 2 == 0 {
        return Some((local / TICKS_PER_STEP) as usize);
    }
    if local >= swing_ticks {
        let l2 = local - swing_ticks;
        if l2 % TICKS_PER_STEP == 0 && (l2 / TICKS_PER_STEP) % 2 == 1 {
            return Some((l2 / TICKS_PER_STEP) as usize);
        }
    }
    None
}

/// swing 0..1 -> delay in ticks of odd steps (max 12 ticks = triplet-ish feel)
pub fn swing_ticks(swing: f32) -> u32 {
    (swing.clamp(0.0, 1.0) * (TICKS_PER_STEP as f32 / 2.0)).round() as u32
}
