//! 808 Sampler: plays one WAV (e.g. a sample from the 808 pack) chromatically across the keys, monophonic like an 808.
//! Slice mode (like FL's Slicex): the WAV is cut at `slices`; key `root + n` plays slice n once, at its original pitch.
use crate::project::SampleRef;
use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SamplerParams {
    pub sample: Option<SampleRef>,
    /// the MIDI note at which the sample plays at its original pitch
    pub root: u8,
    pub volume: f32,
    /// seconds
    pub release: f32,
    /// slicer: each key plays one piece of the sample instead of the whole sample transposed
    pub slice_mode: bool,
    /// start of every slice in sample frames, ascending; slice n runs to the start of slice n + 1 (the last one to the end)
    pub slices: Vec<u32>,
}

/// Most slices one sample can have (they sit on the keys from `root` up).
pub const MAX_SLICES: usize = 64;

impl Default for SamplerParams {
    fn default() -> Self {
        SamplerParams { sample: None, root: 36, volume: 0.9, release: 0.12, slice_mode: false, slices: vec![0] }
    }
}

#[derive(Clone, Copy)]
struct Voice {
    active: bool,
    pos: f64,
    ratio: f64,
    vel: f32,
    gain: f32,
    rel: bool,
    fresh: bool,
    pitch: u8,
    end_tick: u64,
    /// slice mode: where the slice stops (sample frames); f64::MAX plays to the end
    end: f64,
}

const VOICE: Voice = Voice { active: false, pos: 0.0, ratio: 1.0, vel: 0.0, gain: 0.0, rel: false, fresh: false, pitch: 0, end_tick: u64::MAX, end: f64::MAX };

pub struct Sampler {
    v: [Voice; 4],
    next: usize,
}

impl Sampler {
    pub fn new() -> Self {
        Sampler { v: [VOICE; 4], next: 0 }
    }

    pub fn note_on(&mut self, note: u8, vel: f32, end_tick: u64) {
        // monophonic: the previous note fades out
        for v in self.v.iter_mut() {
            v.rel = true;
        }
        let i = self.next;
        self.next = (self.next + 1) % self.v.len();
        self.v[i] = Voice { active: true, pos: 0.0, ratio: 1.0, vel, gain: 0.0, rel: false, fresh: true, pitch: note, end_tick, end: f64::MAX };
    }

    pub fn note_off(&mut self, note: u8) {
        for v in self.v.iter_mut() {
            if v.active && v.pitch == note && v.end_tick == u64::MAX {
                v.rel = true;
            }
        }
    }

    pub fn release_due(&mut self, abs: u64) {
        for v in self.v.iter_mut() {
            if v.active && v.end_tick <= abs {
                v.rel = true;
            }
        }
    }

    pub fn release_all(&mut self) {
        for v in self.v.iter_mut() {
            v.rel = true;
        }
    }

    #[inline]
    pub fn render_sample(&mut self, p: &SamplerParams, sr: f32) -> f32 {
        let Some(sd) = p.sample.as_ref().and_then(|s| s.data.as_deref()) else { return 0.0 };
        let mut out = 0.0;
        let step = sd.rate as f64 / sr as f64;
        let rel_step = 1.0 / (p.release.max(0.005) * sr);
        for v in self.v.iter_mut() {
            if !v.active {
                continue;
            }
            if v.fresh {
                if p.slice_mode {
                    let idx = v.pitch as i32 - p.root as i32;
                    let n = sd.mono.len();
                    let range = (idx >= 0).then(|| idx as usize).filter(|k| *k < p.slices.len()).map(|k| (p.slices[k] as usize, p.slices.get(k + 1).map_or(n, |e| *e as usize).min(n)));
                    match range {
                        Some((a, b)) if a < b => {
                            v.pos = a as f64;
                            v.end = b as f64;
                            v.ratio = 1.0;
                        }
                        _ => {
                            v.active = false;
                            continue;
                        }
                    }
                } else {
                    v.ratio = 2f64.powf((v.pitch as f64 - p.root as f64) / 12.0);
                    v.end = f64::MAX;
                }
                v.fresh = false;
            }
            let i = v.pos as usize;
            if i + 1 >= sd.mono.len() || v.pos >= v.end {
                v.active = false;
                continue;
            }
            if v.rel {
                v.gain -= rel_step;
                if v.gain <= 0.0 {
                    v.active = false;
                    continue;
                }
            } else if v.gain < 1.0 {
                v.gain = (v.gain + 1.0 / (0.002 * sr)).min(1.0);
            }
            let f = (v.pos - i as f64) as f32;
            // a slice stops with a 4 ms fade so the cut does not click
            let tail = if v.end < f64::MAX { ((v.end - v.pos) / (0.004 * sd.rate as f64)).min(1.0) as f32 } else { 1.0 };
            out += crate::audio_engine::dsp::hermite(&sd.mono, i, f) * v.gain * v.vel * tail;
            v.pos += step * v.ratio;
        }
        out * p.volume
    }
}

// ---------------------------------------------------------------- auto slicing

/// `n` equal pieces (the first starts at 0).
pub fn grid_slices(len: usize, n: usize) -> Vec<u32> {
    let n = n.clamp(1, MAX_SLICES);
    (0..n).map(|k| (len as u64 * k as u64 / n as u64) as u32).collect()
}

/// A slice every `frames` sample frames (e.g. every beat at the project tempo).
pub fn step_slices(len: usize, frames: f64) -> Vec<u32> {
    if frames < 1.0 {
        return vec![0];
    }
    let mut v = Vec::new();
    let mut at = 0.0;
    while (at as usize) < len && v.len() < MAX_SLICES {
        v.push(at as u32);
        at += frames;
    }
    v
}

/// Find the hits of a loop: a slice starts where the short-term energy jumps above what came just before.
/// `sensitivity` 0..1 (higher finds more hits). The first slice always starts at 0.
pub fn detect_onsets(mono: &[f32], rate: u32, sensitivity: f32) -> Vec<u32> {
    let hop = (rate as usize / 200).max(32); // 5 ms
    let n = mono.len() / hop;
    let energy: Vec<f32> = (0..n).map(|k| mono[k * hop..(k + 1) * hop].iter().map(|x| x * x).sum::<f32>() / hop as f32).collect();
    let peak = energy.iter().cloned().fold(0.0f32, f32::max);
    let ratio = 1.5 + (1.0 - sensitivity.clamp(0.0, 1.0)) * 5.0;
    let min_gap = (0.06 * rate as f32) as usize / hop; // 60 ms between hits
    let mut out = vec![0u32];
    let mut last = 0usize;
    for k in 1..n {
        let from = k.saturating_sub(8);
        let before = energy[from..k].iter().sum::<f32>() / (k - from) as f32;
        if energy[k] > peak * 0.003 && energy[k] > before * ratio && k >= last + min_gap.max(1) {
            // step back to where the rise starts so the attack is not cut off
            let mut s = k;
            while s > last + 1 && energy[s - 1] < energy[s] {
                s -= 1;
            }
            out.push((s * hop) as u32);
            last = k;
            if out.len() >= MAX_SLICES {
                break;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::SampleData;
    use std::sync::Arc;

    fn params() -> SamplerParams {
        let mono: Vec<f32> = (0..24000).map(|i| (i as f32 * 0.05).sin() * 0.8).collect();
        SamplerParams { sample: Some(SampleRef { name: "t".into(), path: String::new(), data: Some(Arc::new(SampleData { rate: 48000, mono })) }), ..SamplerParams::default() }
    }

    fn length(note: u8) -> usize {
        let p = params();
        let mut s = Sampler::new();
        s.note_on(note, 1.0, u64::MAX);
        let mut n = 0;
        let mut peak = 0.0f32;
        while n < 200_000 {
            let v = s.render_sample(&p, 48000.0);
            peak = peak.max(v.abs());
            if s.v.iter().all(|x| !x.active) {
                break;
            }
            n += 1;
        }
        assert!(peak > 0.1, "silent");
        n
    }

    #[test]
    fn slice_plays_only_its_piece() {
        let mut p = params();
        p.slice_mode = true;
        p.slices = vec![0, 6000, 12000];
        let run = |note: u8| {
            let mut s = Sampler::new();
            s.note_on(note, 1.0, u64::MAX);
            let mut n = 0;
            while n < 100_000 {
                s.render_sample(&p, 48000.0);
                if s.v.iter().all(|x| !x.active) {
                    break;
                }
                n += 1;
            }
            n
        };
        assert!((run(36) as i32 - 6000).abs() < 4, "slice 0: {}", run(36));
        assert!((run(37) as i32 - 6000).abs() < 4);
        assert!((run(38) as i32 - 12000).abs() < 4, "last slice runs to the end: {}", run(38));
        assert_eq!(run(39), 0, "no slice on this key");
        assert_eq!(run(35), 0, "below the first key");
    }

    #[test]
    fn onsets_find_the_hits() {
        // three clicks 0.5 s apart in silence
        let mut m = vec![0.0f32; 48000 * 2];
        for k in 0..3 {
            for i in 0..2000 {
                m[k * 24000 + 100 + i] = (i as f32 * 0.3).sin() * (1.0 - i as f32 / 2000.0);
            }
        }
        let o = detect_onsets(&m, 48000, 0.6);
        assert_eq!(o.len(), 3 + 1 - 1, "{o:?}");
        assert_eq!(o[0], 0);
        assert!(grid_slices(48000, 4) == vec![0, 12000, 24000, 36000]);
    }

    #[test]
    fn sampler_plays_and_transposes() {
        let root = length(36);
        let octave_up = length(48);
        assert!(octave_up < root * 6 / 10, "octave up should play about twice as fast: {octave_up} vs {root}");
    }
}
