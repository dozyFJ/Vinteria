//! Session (clip launcher) model: a grid of clips, one column per arrangement track, one row per scene.
use serde::{Deserialize, Serialize};

/// Highest track index the launcher can play (engine uses fixed-size state, no allocation on the audio thread).
pub const MAX_LANES: usize = 32;

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SlotClip {
    pub lane: usize,
    pub scene: usize,
    /// pattern index (MIDI track) or `Project::audio_pool` index (audio track, `audio == true`)
    pub pattern: usize,
    pub audio: bool,
}

impl SlotClip {
    /// non-zero code shared with the UI through atomics: pattern + 1, bit 31 = audio
    pub fn code(&self) -> u32 {
        code(self.pattern, self.audio)
    }
}

pub fn code(pattern: usize, audio: bool) -> u32 {
    (pattern as u32 + 1) | if audio { 1 << 31 } else { 0 }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Session {
    pub scenes: usize,
    pub clips: Vec<SlotClip>,
}

impl Default for Session {
    fn default() -> Self {
        Session { scenes: 8, clips: vec![] }
    }
}

impl Session {
    pub fn get(&self, lane: usize, scene: usize) -> Option<&SlotClip> {
        self.clips.iter().find(|c| c.lane == lane && c.scene == scene)
    }

    pub fn set(&mut self, lane: usize, scene: usize, pattern: usize, audio: bool) {
        self.clear(lane, scene);
        self.clips.push(SlotClip { lane, scene, pattern, audio });
    }

    pub fn clear(&mut self, lane: usize, scene: usize) {
        self.clips.retain(|c| !(c.lane == lane && c.scene == scene));
    }

    pub fn add_scene(&mut self) {
        self.scenes = (self.scenes + 1).min(64);
    }

    pub fn remove_scene(&mut self) {
        if self.scenes > 1 {
            self.scenes -= 1;
            let n = self.scenes;
            self.clips.retain(|c| c.scene < n);
        }
    }

    pub fn remove_lane(&mut self, lane: usize) {
        self.clips.retain(|c| c.lane != lane);
        for c in self.clips.iter_mut() {
            if c.lane > lane {
                c.lane -= 1;
            }
        }
    }

    pub fn uses_pattern(&self, pattern: usize) -> bool {
        self.clips.iter().any(|c| !c.audio && c.pattern == pattern)
    }

    /// Renumber the pattern indices after patterns were dropped (`map[old] = new`).
    pub fn remap_patterns(&mut self, map: &[usize]) {
        for c in self.clips.iter_mut().filter(|c| !c.audio) {
            c.pattern = map.get(c.pattern).copied().unwrap_or(0);
        }
    }

    pub fn remove_pattern(&mut self, d: usize) {
        self.clips.retain(|c| c.audio || c.pattern != d);
        for c in self.clips.iter_mut() {
            if !c.audio && c.pattern > d {
                c.pattern -= 1;
            }
        }
    }

    /// Drop clips that point at missing tracks / patterns / samples.
    pub fn sanitize(&mut self, lanes: usize, patterns: usize, samples: usize) {
        self.scenes = self.scenes.clamp(1, 64);
        let n = self.scenes;
        self.clips.retain(|c| c.lane < lanes.min(MAX_LANES) && c.scene < n && if c.audio { c.pattern < samples } else { c.pattern < patterns });
    }

    /// A few ready-made scenes for the demo project.
    pub fn demo() -> Self {
        let mut s = Session::default();
        s.set(0, 0, 0, false);
        s.set(0, 1, 1, false);
        s.set(0, 2, 0, false);
        s.set(1, 1, 2, false);
        s
    }
}
