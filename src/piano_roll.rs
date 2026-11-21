//! Piano roll model: notes, tracks, chords.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum Track {
    Synth,
    Bass808,
    Wavetable,
    Analog,
    /// drum sounds as MIDI: pitch 36..40 = Kick, Snare, Clap, Hi-Hat, Perc
    Drums,
    /// plays one WAV (an 808 from the pack) across the keys
    Sampler,
}

/// first MIDI pitch of the drum map
pub const DRUM_BASE: u8 = 36;

impl Track {
    pub const ALL: [Track; 6] = [Track::Synth, Track::Bass808, Track::Wavetable, Track::Analog, Track::Drums, Track::Sampler];
    pub fn channel(self) -> usize {
        match self {
            Track::Synth => crate::mixer::CH_SYNTH,
            Track::Bass808 => crate::mixer::CH_BASS,
            Track::Wavetable => crate::mixer::CH_WT,
            Track::Analog => crate::mixer::CH_AN,
            Track::Drums => 0,
            Track::Sampler => crate::mixer::CH_BASS,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Track::Synth => "Poly Synth",
            Track::Bass808 => "808 Bass",
            Track::Wavetable => "Wavetable",
            Track::Analog => "Analog",
            Track::Drums => "Drums (MIDI)",
            Track::Sampler => "808 Sampler",
        }
    }
    pub fn id(self) -> u8 {
        Track::ALL.iter().position(|t| *t == self).unwrap_or(0) as u8
    }
    pub fn from_id(i: u8) -> Track {
        Track::ALL.get(i as usize).copied().unwrap_or(Track::Synth)
    }
}

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Note {
    /// start in ticks (96 per quarter note) inside the pattern
    pub start: u32,
    pub len: u32,
    pub pitch: u8,
    pub vel: u8,
    pub track: Track,
    /// 808 only: glide from the previous note instead of retriggering
    pub slide: bool,
    /// probability (0..100 %) that the note plays on each pass
    #[serde(default = "full_chance")]
    pub chance: u8,
    /// pan of this note, -100 (left) ..= 100 (right); applied to the instrument's channel when the note starts
    #[serde(default)]
    pub pan: i8,
    /// a muted note stays in the roll but does not play
    #[serde(default)]
    pub mute: bool,
}

fn full_chance() -> u8 {
    100
}

pub const CHORDS: [(&str, &[i32]); 8] = [
    ("Single note", &[0]),
    ("Major", &[0, 4, 7]),
    ("Minor", &[0, 3, 7]),
    ("Maj7", &[0, 4, 7, 11]),
    ("Min7", &[0, 3, 7, 10]),
    ("Dom7", &[0, 4, 7, 10]),
    ("Sus2", &[0, 2, 7]),
    ("Power (5th)", &[0, 7, 12]),
];

pub fn midi_to_freq(n: f32) -> f32 {
    440.0 * ((n - 69.0) / 12.0).exp2()
}

pub fn note_name(p: u8) -> String {
    const N: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
    // like FL Studio: middle C (MIDI 60) is C5
    format!("{}{}", N[(p % 12) as usize], p as i32 / 12)
}

pub fn is_black(p: u8) -> bool {
    matches!(p % 12, 1 | 3 | 6 | 8 | 10)
}
