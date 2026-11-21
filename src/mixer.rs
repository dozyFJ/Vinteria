//! Mixer model: one channel per instrument + master handled in Project.
use serde::{Deserialize, Serialize};

pub const NUM_CH: usize = 10;
/// Extra mixer channels that channel-rack sounds and arrangement tracks can be routed to.
pub const NUM_INSERTS: usize = 8;
/// Every mixer strip: the instrument channels followed by the inserts.
pub const NUM_STRIPS: usize = NUM_CH + NUM_INSERTS;
pub const CH_NAMES: [&str; NUM_CH] = ["Kick", "Snare", "Clap", "Hi-Hat", "Perc", "Synth", "808 Bass", "Wavetable", "Analog", "Audio"];
pub const CH_SYNTH: usize = 5;
pub const CH_BASS: usize = 6;
pub const CH_WT: usize = 7;
pub const CH_AN: usize = 8;
pub const CH_AUDIO: usize = 9;

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Channel {
    pub name: String,
    pub volume: f32,
    pub pan: f32,
    pub mute: bool,
    pub solo: bool,
    pub delay_send: f32,
    /// Instrument channels: `Some(i)` sends the channel into insert `i` instead of straight to the master.
    #[serde(default)]
    pub output: Option<usize>,
}

/// Name of a routing target for menus: "Master" or "Insert N".
pub fn output_label(o: Option<usize>) -> String {
    match o {
        None => "Master".to_string(),
        Some(i) => format!("Insert {}", i + 1),
    }
}

pub fn default_channels() -> Vec<Channel> {
    let vols = [0.95, 0.8, 0.7, 0.55, 0.6, 0.7, 0.9, 0.7, 0.7, 0.85];
    (0..NUM_STRIPS)
        .map(|i| Channel {
            name: if i < NUM_CH { CH_NAMES[i].to_string() } else { format!("Insert {}", i - NUM_CH + 1) },
            volume: if i < NUM_CH { vols[i] } else { 0.85 },
            pan: match i {
                3 => 0.15,
                4 => -0.2,
                _ => 0.0,
            },
            mute: false,
            solo: false,
            delay_send: if i == CH_SYNTH || i == CH_WT || i == CH_AN { 0.22 } else { 0.0 },
            output: None,
        })
        .collect()
}

/// Effective gain of each strip after mute/solo. Solo works inside a group: instrument channels
/// among themselves, inserts among themselves.
pub fn effective_gains(ch: &[Channel]) -> [f32; NUM_STRIPS] {
    let solo_src = ch.iter().take(NUM_CH).any(|c| c.solo);
    let solo_ins = ch.iter().skip(NUM_CH).take(NUM_INSERTS).any(|c| c.solo);
    let mut g = [0.0; NUM_STRIPS];
    for (i, c) in ch.iter().enumerate().take(NUM_STRIPS) {
        let any_solo = if i < NUM_CH { solo_src } else { solo_ins };
        g[i] = if c.mute || (any_solo && !c.solo) { 0.0 } else { c.volume };
    }
    g
}
