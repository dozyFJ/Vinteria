//! Project Manager: the project data model, default project, save/load (.synthesia JSON), WAV sample loading.
use crate::arrangement::{Arrangement, Clip};
use crate::instruments::{AnalogParams, Bass808Params, SynthParams, WavetableParams};
use crate::mixer::{default_channels, Channel, NUM_INSERTS, NUM_STRIPS};
use crate::piano_roll::{Note, Track};
use crate::sequencer::*;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const FILE_VERSION: u32 = 2;

/// Decoded mono sample (shared, immutable).
pub struct SampleData {
    pub rate: u32,
    pub mono: Vec<f32>,
}

/// FL-style "precomputed effects" and playback range of a channel's sample. They are applied to the loaded sound itself
/// (never to the file on disk), so the engine just plays the processed copy.
#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SampleFx {
    /// start / end of the played part, 0..1 of the whole sound
    pub start: f32,
    pub end: f32,
    pub reverse: bool,
    pub normalize: bool,
    pub remove_dc: bool,
    /// fade in / out, 0..1 of the played part
    pub fade_in: f32,
    pub fade_out: f32,
}

impl Default for SampleFx {
    fn default() -> Self {
        SampleFx { start: 0.0, end: 1.0, reverse: false, normalize: false, remove_dc: false, fade_in: 0.0, fade_out: 0.0 }
    }
}

impl SampleFx {
    pub fn is_default(&self) -> bool {
        *self == SampleFx::default()
    }

    pub fn apply(&self, src: &SampleData) -> SampleData {
        let n = src.mono.len();
        let a = (self.start.clamp(0.0, 1.0) * n as f32) as usize;
        let b = ((self.end.clamp(0.0, 1.0) * n as f32) as usize).min(n);
        let (a, b) = if b > a + 1 { (a, b) } else { (a.min(n.saturating_sub(2)), (a + 2).min(n)) };
        let mut v: Vec<f32> = src.mono[a..b].to_vec();
        if self.remove_dc && !v.is_empty() {
            let mean = v.iter().sum::<f32>() / v.len() as f32;
            for x in v.iter_mut() {
                *x -= mean;
            }
        }
        if self.reverse {
            v.reverse();
        }
        let len = v.len();
        let fi = (self.fade_in.clamp(0.0, 1.0) * len as f32) as usize;
        for i in 0..fi.min(len) {
            v[i] *= i as f32 / fi as f32;
        }
        let fo = (self.fade_out.clamp(0.0, 1.0) * len as f32) as usize;
        for i in 0..fo.min(len) {
            v[len - 1 - i] *= i as f32 / fo as f32;
        }
        if self.normalize {
            let peak = v.iter().fold(0.0f32, |m, x| m.max(x.abs()));
            if peak > 1e-6 {
                for x in v.iter_mut() {
                    *x /= peak;
                }
            }
        }
        SampleData { rate: src.rate, mono: v }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct SampleRef {
    pub name: String,
    pub path: String,
    #[serde(skip)]
    pub data: Option<Arc<SampleData>>,
}

impl PartialEq for SampleRef {
    fn eq(&self, o: &Self) -> bool {
        self.name == o.name
            && self.path == o.path
            && match (&self.data, &o.data) {
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            }
    }
}

pub fn load_wav(path: &Path) -> Result<SampleData, String> {
    #[cfg(target_arch = "wasm32")]
    if let Some(bytes) = crate::web::sample_bytes(path) {
        return decode_wav(std::io::Cursor::new(bytes));
    }
    let input = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    decode_wav(std::io::BufReader::new(input))
}

pub(crate) fn decode_wav(input: impl std::io::Read) -> Result<SampleData, String> {
    decode_wav_with_limit(input, Some(60))
}

fn decode_wav_with_limit(input: impl std::io::Read, seconds: Option<usize>) -> Result<SampleData, String> {
    let mut r = hound::WavReader::new(input).map_err(|e| format!("invalid WAV: {e}"))?;
    let spec = r.spec();
    let ch = spec.channels.max(1) as usize;
    let max_samples = seconds.map_or(usize::MAX, |seconds| {
        (spec.sample_rate as usize).saturating_mul(seconds).saturating_mul(ch)
    });
    let raw: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => r.samples::<f32>().take(max_samples).collect::<Result<Vec<_>, _>>().map_err(|e| format!("invalid WAV data: {e}"))?,
        hound::SampleFormat::Int => {
            let scale = 1.0 / (1u64 << (spec.bits_per_sample - 1)) as f32;
            r.samples::<i32>().take(max_samples).map(|v| v.map(|v| v as f32 * scale)).collect::<Result<Vec<_>, _>>().map_err(|e| format!("invalid WAV data: {e}"))?
        }
    };
    if raw.iter().any(|v| !v.is_finite()) {
        return Err("invalid WAV data: non-finite sample".into());
    }
    let mono: Vec<f32> = raw.chunks_exact(ch).map(|f| f.iter().sum::<f32>() / ch as f32).collect();
    if mono.len() < 2 {
        return Err("sample is empty".into());
    }
    Ok(SampleData { rate: spec.sample_rate, mono })
}

/// Relative sample paths belong to the project, with a basename fallback for
/// projects moved together with their samples. Keep legacy working-directory paths.
fn resolve_sample_path(dir: &Path, stored: &str) -> PathBuf {
    let path = Path::new(stored);
    let relative = dir.join(path);
    if relative.is_file() {
        return relative;
    }
    let beside = dir.join(path.file_name().unwrap_or_default());
    if beside.is_file() {
        return beside;
    }
    path.to_path_buf()
}

/// Master bus: glue compressor and a two-band tone control, in front of the limiter.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MasterFx {
    pub comp_on: bool,
    pub threshold_db: f32,
    pub ratio: f32,
    pub makeup_db: f32,
    pub low_db: f32,
    pub high_db: f32,
}

impl Default for MasterFx {
    fn default() -> Self {
        MasterFx { comp_on: false, threshold_db: -12.0, ratio: 2.5, makeup_db: 0.0, low_db: 0.0, high_db: 0.0 }
    }
}

/// One pattern of the song: its own MIDI / audio tracks and the clips on them.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Scene {
    pub name: String,
    pub arrangement: Arrangement,
}

/// One note the user played, remembered for Retrospective Record: seconds since the program started, length in seconds.
#[derive(Clone, Copy, Debug)]
pub struct RetroNote {
    pub t: f64,
    pub len_s: f64,
    pub pitch: u8,
    pub vel: u8,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Project {
    pub name: String,
    /// Patterns: each one is its own arrangement (tracks + clips). The ACTIVE one lives in `arrangement`; the others are stored here.
    pub scenes: Vec<Scene>,
    pub active_scene: usize,
    pub bpm: f32,
    pub swing: f32,
    pub metronome: bool,
    pub master_volume: f32,
    /// automation of the master level (multiplier of `master_volume`)
    pub master_auto: Vec<crate::arrangement::AutoPoint>,
    /// per insert: how far (0..1) the sidechain sources push its level down on every hit
    pub duck: Vec<f32>,
    pub song_mode: bool,
    pub loop_bars: u32,
    /// first bar of the loop range in song mode (0-based)
    pub loop_start: u32,
    /// Song mode loops only the range loop_start..loop_start+loop_bars when on; off = the whole arrangement plays to its end
    pub loop_on: bool,
    pub current_pattern: usize,
    pub patterns: Vec<Pattern>,
    pub arrangement: Arrangement,
    pub session: crate::session::Session,
    /// musical key used by the piano roll (pitch class 0..11 and an index into `theory::SCALES`)
    pub key_root: u8,
    pub key_scale: usize,
    pub channels: Vec<Channel>,
    pub synth: SynthParams,
    pub bass: Bass808Params,
    pub wt: WavetableParams,
    pub analog: AnalogParams,
    pub sampler: crate::instruments::sampler::SamplerParams,
    /// audio files used by clips on audio tracks
    pub audio_pool: Vec<SampleRef>,
    pub delay_beats: f32,
    pub delay_feedback: f32,
    pub delay_return: f32,
    pub master_fx: MasterFx,
    pub drum_samples: Vec<Option<SampleRef>>,
    /// precomputed effects / range of each drum lane's sample
    pub drum_fx: Vec<SampleFx>,
    /// effect chain of every mixer insert
    pub insert_fx: Vec<Vec<crate::fx::FxSlot>>,
    /// effect chain of the master bus (after the master volume, before the limiter)
    pub master_chain: Vec<crate::fx::FxSlot>,
}

impl Default for Project {
    fn default() -> Self {
        Project {
            name: "Untitled".into(),
            scenes: vec![Scene { name: "Pattern 1".into(), arrangement: Arrangement::default() }],
            active_scene: 0,
            bpm: 140.0,
            swing: 0.0,
            metronome: false,
            master_volume: 0.65,
            master_auto: Vec::new(),
            duck: vec![0.0; crate::mixer::NUM_INSERTS],
            song_mode: false,
            loop_bars: 8,
            loop_start: 0,
            loop_on: false,
            current_pattern: 0,
            patterns: vec![Pattern::new("Pattern 1", 32)],
            arrangement: Arrangement::default(),
            session: Default::default(),
            key_root: 0,
            key_scale: crate::theory::CHROMATIC,
            channels: default_channels(),
            synth: SynthParams::default(),
            bass: Bass808Params::default(),
            wt: WavetableParams::default(),
            analog: AnalogParams::default(),
            sampler: Default::default(),
            audio_pool: vec![],
            delay_beats: 0.75,
            delay_feedback: 0.4,
            delay_return: 0.35,
            master_fx: MasterFx::default(),
            drum_samples: vec![None; NUM_DRUMS],
            drum_fx: vec![SampleFx::default(); NUM_DRUMS],
            insert_fx: vec![vec![]; crate::mixer::NUM_INSERTS],
            master_chain: vec![],
        }
    }
}

#[derive(Serialize, Deserialize)]
struct FileWrapper {
    app: String,
    version: u32,
    project: Project,
}

impl Project {
    /// Track-local effect automation needs a mixer insert dedicated to that track.
    pub fn track_automation_insert(&self, lane: usize) -> Option<usize> {
        let insert = self.arrangement.tracks.get(lane)?.output.filter(|&out| out < crate::mixer::NUM_INSERTS)?;
        let shared = self.arrangement.tracks.iter().enumerate()
            .any(|(index, track)| index != lane && track.output == Some(insert))
            || self.channels.iter().take(crate::mixer::NUM_CH).any(|channel| channel.output == Some(insert));
        (!shared).then_some(insert)
    }

    /// A ready-to-play trap beat so that Play makes sound immediately.
    pub fn demo() -> Self {
        let mut p = Project { name: "Demo Trap".into(), ..Default::default() };
        let mut a = Pattern::new("Trap Loop", 32);
        a.set_steps(0, &[0, 7, 10, 16, 23, 26], 112);
        a.set_steps(1, &[8, 24], 110);
        a.set_steps(2, &[8, 24], 90);
        let hats: Vec<usize> = (0..32).step_by(2).collect();
        a.set_steps(3, &hats, 96);
        a.set_steps(3, &[13, 15, 29, 30, 31], 62);
        a.set_steps(4, &[5, 20, 21], 80);
        let t = TICKS_PER_STEP;
        let n808 = |step: u32, len: u32, pitch: u8, slide: bool| Note {
            start: step * t,
            len: len * t,
            pitch,
            vel: 120,
            track: Track::Bass808,
            slide,
            chance: 100,
            pan: 0,
            mute: false,
        };
        a.notes.extend([
            n808(0, 6, 33, false),
            n808(7, 3, 33, false),
            n808(10, 4, 36, false),
            n808(16, 6, 31, false),
            n808(22, 2, 31, false),
            n808(23, 5, 34, true),
            n808(26, 4, 36, false),
        ]);
        let chord = |step: u32, len: u32, pitches: &[u8]| -> Vec<Note> {
            pitches
                .iter()
                .map(|&pitch| Note { start: step * t, len: len * t, pitch, vel: 88, track: Track::Synth, slide: false, mute: false, pan: 0, chance: 100 })
                .collect()
        };
        a.notes.extend(chord(0, 14, &[57, 60, 64]));
        a.notes.extend(chord(16, 14, &[53, 57, 60]));
        let mut b = Pattern::new("Break", 32);
        b.set_steps(3, &(0..32).step_by(4).collect::<Vec<_>>(), 70);
        b.notes.extend(chord(0, 30, &[57, 60, 64, 67]));
        for (i, &pitch) in [57u8, 60, 64, 67, 72, 67, 64, 60].iter().enumerate() {
            b.notes.push(Note { start: i as u32 * 96, len: 80, pitch, vel: 100, track: Track::Analog, slide: false, mute: false, pan: 0, chance: 100 });
            b.notes.push(Note { start: i as u32 * 96, len: 90, pitch: pitch - 24, vel: 95, track: Track::Wavetable, slide: false, mute: false, pan: 0, chance: 100 });
        }
        let c = Pattern::new("Empty", 16);
        p.patterns = vec![a, b, c];
        p.session = crate::session::Session::demo();
        p.arrangement.tracks = crate::arrangement::Arrangement::default_tracks();
        p.arrangement.clips = vec![
            Clip::bars(1, 0, 0, 2, false),
            Clip::bars(0, 0, 2, 2, false),
            Clip::bars(0, 0, 4, 2, false),
            Clip::bars(0, 0, 6, 2, false),
        ];
        p
    }

    /// Repair anything out-of-range (hand-edited or older files).
    /// The live arrangement belongs to the active pattern; the slot of the active pattern in `scenes` stays empty.
    fn stash_scene(&mut self) {
        let a = self.active_scene;
        if a < self.scenes.len() {
            self.scenes[a].arrangement = std::mem::take(&mut self.arrangement);
        }
    }

    fn activate_scene(&mut self, i: usize) {
        self.active_scene = i;
        self.arrangement = std::mem::take(&mut self.scenes[i].arrangement);
    }

    pub fn switch_scene(&mut self, i: usize) {
        if i >= self.scenes.len() || i == self.active_scene {
            return;
        }
        self.stash_scene();
        self.activate_scene(i);
    }

    /// a new, empty pattern with the starter tracks
    pub fn new_scene(&mut self) {
        self.stash_scene();
        let n = self.scenes.len() + 1;
        self.scenes.push(Scene { name: format!("Pattern {n}"), arrangement: Arrangement { tracks: Arrangement::starter_tracks(), clips: vec![], groups: vec![] } });
        self.activate_scene(self.scenes.len() - 1);
    }

    /// copy of the active pattern; the note data it plays is copied too, so the two can be edited independently
    pub fn duplicate_scene(&mut self) {
        let mut arr = self.arrangement.clone();
        let mut map: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
        for c in arr.clips.iter_mut().filter(|c| !c.audio) {
            if c.pattern >= self.patterns.len() {
                continue;
            }
            let ni = match map.get(&c.pattern) {
                Some(&i) => i,
                None => {
                    let mut q = self.patterns[c.pattern].clone();
                    q.name = format!("{} copy", q.name);
                    self.patterns.push(q);
                    map.insert(c.pattern, self.patterns.len() - 1);
                    self.patterns.len() - 1
                }
            };
            c.pattern = ni;
        }
        let name = format!("{} copy", self.scenes[self.active_scene].name);
        self.stash_scene();
        self.scenes.push(Scene { name, arrangement: arr });
        self.activate_scene(self.scenes.len() - 1);
    }

    pub fn delete_scene(&mut self) {
        if self.scenes.len() <= 1 {
            return;
        }
        self.scenes.remove(self.active_scene);
        let n = self.active_scene.min(self.scenes.len() - 1);
        self.activate_scene(n);
    }

    /// Audio clips that have not met a tempo yet take the current one as the tempo they are laid out at.
    pub fn stamp_warp(&mut self) {
        let bpm = self.bpm;
        for c in self.arrangement.clips.iter_mut().filter(|c| c.audio && c.warp <= 0.0) {
            c.warp = bpm;
        }
    }

    pub fn sanitize(&mut self) {
        self.duck.resize(crate::mixer::NUM_INSERTS, 0.0);
        for d in self.duck.iter_mut() {
            *d = d.clamp(0.0, 1.0);
        }
        // projects from before "patterns own their tracks" have no scene list: the arrangement becomes the first pattern
        if self.scenes.is_empty() {
            self.scenes.push(Scene { name: "Pattern 1".into(), arrangement: Arrangement::default() });
            self.active_scene = 0;
        }
        self.active_scene = self.active_scene.min(self.scenes.len() - 1);
        self.scenes[self.active_scene].arrangement = Arrangement::default();
        self.bpm = self.bpm.clamp(40.0, 300.0);
        self.key_root %= 12;
        self.key_scale = self.key_scale.min(crate::theory::CHROMATIC);
        self.swing = self.swing.clamp(0.0, 1.0);
        self.loop_bars = self.loop_bars.clamp(1, 256);
        if self.patterns.is_empty() {
            self.patterns.push(Pattern::new("Pattern 1", 16));
        }
        for p in &mut self.patterns {
            p.length_steps = if p.length_steps == 0 { 16 } else { p.length_steps.min(MAX_STEPS as u32) };
            p.drums.resize(NUM_DRUMS, vec![]);
            for l in &mut p.drums {
                l.resize(MAX_STEPS, Step::default());
            }
            let max = p.len_ticks();
            p.notes.retain(|n| n.start < max && n.len > 0);
        }
        self.adopt_track_params();
        self.current_pattern = self.current_pattern.min(self.patterns.len() - 1);
        self.arrangement.clips.retain(|c| c.len > 0);
        if self.channels.len() != NUM_STRIPS {
            let defaults = default_channels();
            self.channels.truncate(NUM_STRIPS);
            while self.channels.len() < NUM_STRIPS {
                self.channels.push(defaults[self.channels.len()].clone());
            }
        }
        for c in self.channels.iter_mut() {
            c.output = c.output.filter(|&o| o < NUM_INSERTS);
        }
        self.drum_samples.resize(NUM_DRUMS, None);
        self.drum_fx.resize(NUM_DRUMS, SampleFx::default());
        for arr in std::iter::once(&mut self.arrangement).chain(self.scenes.iter_mut().map(|s| &mut s.arrangement)) {
            arr.ensure_tracks();
            for t in &mut arr.tracks {
                t.output = t.output.filter(|&o| o < NUM_INSERTS);
            }
            let (np, nt, na) = (self.patterns.len(), arr.tracks.len(), self.audio_pool.len());
            arr.clips.retain(|c| c.len > 0 && c.lane < nt && if c.audio { c.pattern < na } else { c.pattern < np });
        }
        let (np, nt, na) = (self.patterns.len(), self.arrangement.tracks.len(), self.audio_pool.len());
        self.arrangement.clips.retain(|c| c.lane < nt && if c.audio { c.pattern < na } else { c.pattern < np });
        self.session.sanitize(nt, np, na);
    }

    /// Keep every MIDI track above the audio tracks (the Playlist shows them as two groups).
    /// Returns `map[old_lane] = new_lane` when the order had to change; clips of the Playlist and of the
    /// Session move with their tracks.
    pub fn group_tracks(&mut self) -> Option<Vec<usize>> {
        use crate::arrangement::TrackKind;
        let kinds: Vec<TrackKind> = self.arrangement.tracks.iter().map(|t| t.kind).collect();
        let mut seen_audio = false;
        let mut mixed = false;
        for k in &kinds {
            match k {
                TrackKind::Audio => seen_audio = true,
                TrackKind::Midi if seen_audio => mixed = true,
                _ => {}
            }
        }
        if !mixed {
            return None;
        }
        let order: Vec<usize> = (0..kinds.len()).filter(|&i| kinds[i] == TrackKind::Midi).chain((0..kinds.len()).filter(|&i| kinds[i] == TrackKind::Audio)).collect();
        let mut map = vec![0usize; kinds.len()];
        for (new, &old) in order.iter().enumerate() {
            map[old] = new;
        }
        let mut old_tracks: Vec<Option<_>> = std::mem::take(&mut self.arrangement.tracks).into_iter().map(Some).collect();
        self.arrangement.tracks = order.iter().map(|&o| old_tracks[o].take().unwrap()).collect();
        for c in self.arrangement.clips.iter_mut() {
            if let Some(&n) = map.get(c.lane) {
                c.lane = n;
            }
        }
        for c in self.session.clips.iter_mut() {
            if let Some(&n) = map.get(c.lane) {
                c.lane = n;
            }
        }
        Some(map)
    }

    /// The pattern owned by a MIDI track: the one its first clip plays, or a fresh one named after the track.
    pub fn lane_pattern(&mut self, lane: usize) -> usize {
        if let Some(c) = self.arrangement.clips.iter().find(|c| c.lane == lane && !c.audio) {
            if c.pattern < self.patterns.len() {
                return c.pattern;
            }
        }
        let name = self.arrangement.tracks.get(lane).map(|t| t.name.clone()).unwrap_or_else(|| format!("Track {}", lane + 1));
        let mut p = Pattern::new(&name, 32);
        p.clip_only = true;
        self.patterns.push(p);
        self.patterns.len() - 1
    }

    /// The pattern the Channel Rack shows: the selected one, or the first real pattern while a Playlist MIDI clip's own notes
    /// are selected (those are edited in the Piano Roll, never in the rack).
    /// Every track with a built-in synth gets a copy of the project-level settings of that synth the first time it is seen, so
    /// from then on each track edits (and plays) its own plugin. Old projects and templates are converted here too.
    pub fn adopt_track_params(&mut self) {
        let (synth, bass, wt, analog) = (self.synth.clone(), self.bass.clone(), self.wt.clone(), self.analog.clone());
        for t in self.arrangement.tracks.iter_mut() {
            let engine = matches!(t.instrument, Some(Track::Synth | Track::Bass808 | Track::Wavetable | Track::Analog));
            if engine && !t.snd.own {
                t.snd.own = true;
                t.snd.synth = synth.clone();
                t.snd.bass = bass.clone();
                t.snd.wt = wt.clone();
                t.snd.analog = analog.clone();
            }
        }
    }

    /// Retrospective record: the notes that were played (not recorded) become a MIDI clip on `lane`, after the clip already there,
    /// quantized to 1/16. Returns the index of the new clip.
    pub fn retro_to_clip(&mut self, lane: usize, track: Track, notes: &[RetroNote]) -> Option<usize> {
        if notes.is_empty() || self.arrangement.kind(lane) != crate::arrangement::TrackKind::Midi || lane >= self.arrangement.tracks.len() {
            return None;
        }
        let t0 = notes.iter().map(|n| n.t).fold(f64::MAX, f64::min);
        let tick_s = 60.0 / self.bpm.max(20.0) as f64 / PPQ as f64;
        let q = |ticks: f64| ((ticks / 24.0).round() as u32) * 24;
        let mut out: Vec<Note> = notes.iter().map(|n| Note { start: q((n.t - t0) / tick_s), len: q(n.len_s / tick_s).max(24), pitch: n.pitch, vel: n.vel, track, slide: false, mute: false, pan: 0, chance: 100 }).collect();
        let end = out.iter().map(|n| n.start + n.len).max().unwrap_or(24);
        let bars = ((end + TICKS_PER_BAR - 1) / TICKS_PER_BAR).max(1);
        let steps = [16u32, 32, 64, 128, 256].into_iter().find(|s| s / 16 >= bars)?;
        let mut pat = Pattern::new("Retro take", steps);
        pat.clip_only = true;
        let max = pat.len_ticks();
        out.retain(|n| n.start < max);
        for n in out.iter_mut() {
            n.len = n.len.min(max - n.start);
        }
        pat.notes = out;
        self.patterns.push(pat);
        let pi = self.patterns.len() - 1;
        let first = self.arrangement.clips.iter().filter(|c| c.lane == lane).map(|c| c.end_bar()).max().unwrap_or(0);
        self.arrangement.clips.push(Clip::bars(pi, lane, first, steps / 16, false));
        Some(self.arrangement.clips.len() - 1)
    }

    /// For a render of chosen tracks: every Playlist track that is not ticked in `picked` goes silent and the solos are cleared,
    /// so exactly the ticked MIDI and audio tracks are heard (a missing entry counts as not picked).
    pub fn render_only_tracks(&mut self, picked: &[bool]) {
        for (l, t) in self.arrangement.tracks.iter_mut().enumerate() {
            t.solo = false;
            if !picked.get(l).copied().unwrap_or(false) {
                t.mute = true;
            }
        }
    }

    pub fn rack_pattern(&self) -> usize {
        // the rack and the roll show whatever is selected, also the notes of a Playlist clip: they must stay reachable
        self.current_pattern.min(self.patterns.len().saturating_sub(1))
    }

    /// "Send to Playlist" of the Channel Rack: the notes every MIDI track's instrument has in pattern `idx` become a clip on that
    /// very track (after what the track already holds), `repeat` times in a row. Returns how many tracks got clips.
    pub fn send_rack_to_tracks(&mut self, idx: usize, repeat: u32) -> usize {
        // the notes of a clip already play in the Playlist: sending them again would only double the clip
        if idx >= self.patterns.len() || self.patterns[idx].clip_only {
            return 0;
        }
        let src = self.patterns[idx].clone();
        let bars = ((src.len_ticks() + TICKS_PER_BAR - 1) / TICKS_PER_BAR).max(1);
        let mut sent = 0;
        for lane in 0..self.arrangement.tracks.len() {
            if self.arrangement.tracks[lane].kind != crate::arrangement::TrackKind::Midi {
                continue;
            }
            let inst = self.arrangement.tracks[lane].instrument.unwrap_or(Track::Synth);
            let notes: Vec<Note> = src.notes.iter().filter(|n| n.track == inst).copied().collect();
            if notes.is_empty() {
                continue;
            }
            let mut part = Pattern::new(&format!("{} - {}", src.name, self.arrangement.tracks[lane].name), src.length_steps);
            part.notes = notes;
            part.clip_only = true;
            self.patterns.push(part);
            let pi = self.patterns.len() - 1;
            let first_bar = self.arrangement.clips.iter().filter(|c| c.lane == lane).map(|c| c.end_bar()).max().unwrap_or(0);
            for r in 0..repeat.max(1) {
                let c = Clip::bars(pi, lane, first_bar + r * bars, bars, false);
                if self.arrangement.can_place(&c, None) {
                    self.arrangement.clips.push(c);
                }
            }
            sent += 1;
        }
        self.loop_bars = self.loop_bars.max(self.arrangement.song_end_bar().min(256));
        sent
    }

    pub fn new_pattern(&mut self) {
        let n = self.patterns.len() + 1;
        self.patterns.push(Pattern::new(&format!("Pattern {n}"), 16));
        self.current_pattern = n - 1;
    }

    pub fn duplicate_pattern(&mut self) {
        let mut p = self.patterns[self.current_pattern].clone();
        p.name = format!("{} copy", p.name);
        self.patterns.push(p);
        self.current_pattern = self.patterns.len() - 1;
    }

    /// FL "split by channel": one new pattern per used channel (drum lane / instrument).
    /// With `place`, each part is dropped on its own Playlist track after the current end of the song.
    /// Split a pattern (the whole beat written in the Channel Rack) by channel: every drum lane and every instrument becomes
    /// its own MIDI track with a clip, `repeat` times in a row. Returns how many channels were split off.
    pub fn split_pattern_by_channel(&mut self, idx: usize, place: bool, repeat: u32) -> usize {
        let src = self.patterns[idx].clone();
        let mut parts: Vec<(String, Pattern, Option<Track>)> = Vec::new();
        for lane in 0..NUM_DRUMS {
            if src.drums[lane].iter().any(|s| s.on) {
                let mut p = Pattern::new(&format!("{} - {}", src.name, DRUM_NAMES[lane]), src.length_steps);
                p.clip_only = true;
                p.drums[lane] = src.drums[lane].clone();
                parts.push((DRUM_NAMES[lane].to_string(), p, None));
            }
        }
        for t in Track::ALL {
            if src.notes.iter().any(|n| n.track == t) {
                let mut p = Pattern::new(&format!("{} - {}", src.name, t.name()), src.length_steps);
                p.clip_only = true;
                p.notes = src.notes.iter().filter(|n| n.track == t).copied().collect();
                parts.push((t.name().to_string(), p, Some(t)));
            }
        }
        if parts.is_empty() {
            return 0;
        }
        self.arrangement.ensure_tracks();
        // a fresh project still has its empty starter MIDI tracks: they make room for the channels
        if place && self.arrangement.clips.is_empty() {
            let keep: Vec<crate::arrangement::ArrTrack> = self.arrangement.tracks.iter().filter(|t| !(t.kind == crate::arrangement::TrackKind::Midi && t.instrument.is_none() && t.name.starts_with("MIDI "))).cloned().collect();
            self.arrangement.tracks = keep;
        }
        let start_bar = self.arrangement.song_end_bar();
        let count = parts.len();
        let repeat = repeat.clamp(1, 64);
        for (chan, pat, inst) in parts {
            let bars = ((pat.len_ticks() + TICKS_PER_BAR - 1) / TICKS_PER_BAR).max(1);
            self.patterns.push(pat);
            let pi = self.patterns.len() - 1;
            if !place {
                continue;
            }
            let lane = match self.arrangement.tracks.iter().position(|t| t.kind == crate::arrangement::TrackKind::Midi && t.name == chan) {
                Some(l) => l,
                None => {
                    self.arrangement.add_track(crate::arrangement::TrackKind::Midi);
                    let l = self.arrangement.tracks.len() - 1;
                    self.arrangement.tracks[l].name = chan.clone();
                    self.arrangement.tracks[l].instrument = inst;
                    self.arrangement.tracks[l].blank = false;
                    l
                }
            };
            for k in 0..repeat {
                let c = Clip::bars(pi, lane, start_bar + k * bars, bars, false);
                if self.arrangement.can_place(&c, None) {
                    self.arrangement.clips.push(c);
                }
            }
        }
        if place {
            self.order_tracks();
            self.loop_bars = self.loop_bars.max(self.arrangement.song_end_bar().min(256));
        }
        count
    }

    /// channels first (drums, then instruments), other MIDI tracks next, audio tracks last; clips follow their tracks
    pub fn order_tracks(&mut self) {
        use crate::arrangement::TrackKind;
        let mut canon: Vec<String> = DRUM_NAMES.iter().map(|s| s.to_string()).collect();
        canon.extend(Track::ALL.iter().map(|t| t.name().to_string()));
        let key = |i: usize, t: &crate::arrangement::ArrTrack| -> (u8, usize) {
            match (t.kind, canon.iter().position(|c| *c == t.name)) {
                (TrackKind::Midi, Some(p)) => (0, p),
                (TrackKind::Midi, None) => (1, i),
                _ => (2, i),
            }
        };
        let mut order: Vec<usize> = (0..self.arrangement.tracks.len()).collect();
        order.sort_by_key(|&i| key(i, &self.arrangement.tracks[i]));
        let mut new_of_old = vec![0usize; order.len()];
        for (new, &old) in order.iter().enumerate() {
            new_of_old[old] = new;
        }
        let old_tracks = self.arrangement.tracks.clone();
        self.arrangement.tracks = order.iter().map(|&o| old_tracks[o].clone()).collect();
        for c in self.arrangement.clips.iter_mut() {
            if c.lane < new_of_old.len() {
                c.lane = new_of_old[c.lane];
            }
        }
    }

    /// The whole beat written in the Channel Rack goes to the Playlist as ordinary clips of that pattern (`repeat` times in a
    /// row) on a track named after the pattern. The Playlist can then split them by channel (see `bounce`).
    pub fn send_pattern_to_playlist(&mut self, idx: usize, repeat: u32) {
        use crate::arrangement::TrackKind;
        if idx >= self.patterns.len() {
            return;
        }
        self.arrangement.ensure_tracks();
        let name = self.patterns[idx].name.clone();
        let lane = match self.arrangement.tracks.iter().position(|t| t.kind == TrackKind::Midi && t.instrument.is_none() && t.name == name) {
            Some(l) => l,
            None => {
                // a new Beat track goes to the top; everything below moves down one lane
                self.arrangement.tracks.insert(0, crate::bounce::beat_track(&name));
                for c in self.arrangement.clips.iter_mut() {
                    c.lane += 1;
                }
                0
            }
        };
        let len = ((self.patterns[idx].len_ticks() + TICKS_PER_BAR - 1) / TICKS_PER_BAR).max(1) * TICKS_PER_BAR;
        let start = self.arrangement.clips.iter().filter(|c| c.lane == lane).map(|c| c.end_tick()).max().unwrap_or(0);
        for k in 0..repeat.clamp(1, 64) {
            let c = Clip::new(idx, lane, start + k * len, len, false);
            if self.arrangement.can_place(&c, None) {
                self.arrangement.clips.push(c);
            }
        }
        self.song_mode = true;
        self.loop_bars = self.loop_bars.max(self.arrangement.song_end_bar().min(256));
    }

    /// Turn "one whole-pattern clip" arrangements into a real multitrack one: every channel (Kick, Snare, ..., each instrument)
    /// gets its own MIDI track holding only its own part of every pattern the clips used.
    pub fn spread_clips_over_tracks(&mut self) {
        use crate::arrangement::TrackKind;
        let old: Vec<Clip> = std::mem::take(&mut self.arrangement.clips);
        let mut order: Vec<String> = DRUM_NAMES.iter().map(|s| s.to_string()).collect();
        order.extend(Track::ALL.iter().map(|t| t.name().to_string()));
        let mut parts: std::collections::HashMap<(usize, String), usize> = std::collections::HashMap::new();
        let mut lanes: std::collections::HashMap<String, (usize, Option<Track>)> = std::collections::HashMap::new();
        let mut placed: Vec<(String, Clip)> = vec![];
        for clip in old {
            if clip.audio || clip.pattern >= self.patterns.len() {
                continue;
            }
            let src = self.patterns[clip.pattern].clone();
            let mut found: Vec<(String, Pattern, Option<Track>)> = vec![];
            for lane in 0..NUM_DRUMS {
                if src.drums[lane].iter().any(|s| s.on) {
                    let mut q = Pattern::new(&format!("{} - {}", src.name, DRUM_NAMES[lane]), src.length_steps);
                q.clip_only = true;
                    q.drums[lane] = src.drums[lane].clone();
                    found.push((DRUM_NAMES[lane].to_string(), q, None));
                }
            }
            for t in Track::ALL {
                if src.notes.iter().any(|n| n.track == t) {
                    let mut q = Pattern::new(&format!("{} - {}", src.name, t.name()), src.length_steps);
                q.clip_only = true;
                    q.notes = src.notes.iter().filter(|n| n.track == t).copied().collect();
                    found.push((t.name().to_string(), q, Some(t)));
                }
            }
            for (chan, q, inst) in found {
                let key = (clip.pattern, chan.clone());
                let pi = match parts.get(&key) {
                    Some(&i) => i,
                    None => {
                        self.patterns.push(q);
                        parts.insert(key, self.patterns.len() - 1);
                        self.patterns.len() - 1
                    }
                };
                lanes.entry(chan.clone()).or_insert((0, inst));
                placed.push((chan, Clip { pattern: pi, lane: 0, audio: false, ..clip }));
            }
        }
        // tracks in a fixed, readable order: drums first, then the instruments
        self.arrangement.tracks.clear();
        for name in &order {
            if let Some((_, inst)) = lanes.get(name).copied() {
                self.arrangement.add_track(TrackKind::Midi);
                let l = self.arrangement.tracks.len() - 1;
                let t = &mut self.arrangement.tracks[l];
                t.name = name.clone();
                t.instrument = inst;
                t.blank = false;
                lanes.insert(name.clone(), (l, inst));
            }
        }
        for (chan, mut c) in placed {
            if let Some(&(l, _)) = lanes.get(&chan) {
                c.lane = l;
                self.arrangement.clips.push(c);
            }
        }
    }

    /// Playlist MIDI clips that were deleted (or whose track was deleted) leave their notes behind in `patterns`; drop what no
    /// clip refers to any more (the pattern open in the piano roll stays) and renumber the clips. Returns how many were dropped.
    pub fn prune_orphan_patterns(&mut self) -> usize {
        let mut used: std::collections::HashSet<usize> = self.arrangement.clips.iter().filter(|c| !c.audio).map(|c| c.pattern).collect();
        for s in &self.scenes {
            used.extend(s.arrangement.clips.iter().filter(|c| !c.audio).map(|c| c.pattern));
        }
        let keep: Vec<bool> = (0..self.patterns.len()).map(|i| !self.patterns[i].clip_only || used.contains(&i) || i == self.current_pattern || self.session_uses(i)).collect();
        let dropped = keep.iter().filter(|k| !**k).count();
        if dropped == 0 {
            return 0;
        }
        let mut map = vec![0usize; self.patterns.len()];
        let mut next = 0;
        for (i, k) in keep.iter().enumerate() {
            if *k {
                map[i] = next;
                next += 1;
            }
        }
        let mut i = 0;
        self.patterns.retain(|_| {
            let k = keep[i];
            i += 1;
            k
        });
        for c in self.arrangement.clips.iter_mut().filter(|c| !c.audio) {
            c.pattern = map[c.pattern];
        }
        for s in self.scenes.iter_mut() {
            for c in s.arrangement.clips.iter_mut().filter(|c| !c.audio) {
                c.pattern = map.get(c.pattern).copied().unwrap_or(0);
            }
        }
        self.current_pattern = map[self.current_pattern.min(map.len() - 1)];
        self.session.remap_patterns(&map);
        dropped
    }

    fn session_uses(&self, pattern: usize) -> bool {
        self.session.uses_pattern(pattern)
    }

    pub fn delete_pattern(&mut self) {
        if self.patterns.len() <= 1 {
            return;
        }
        let d = self.current_pattern;
        self.patterns.remove(d);
        self.session.remove_pattern(d);
        for arr in std::iter::once(&mut self.arrangement).chain(self.scenes.iter_mut().map(|s| &mut s.arrangement)) {
            arr.clips.retain(|c| c.audio || c.pattern != d);
            for c in &mut arr.clips {
                if !c.audio && c.pattern > d {
                    c.pattern -= 1;
                }
            }
        }
        self.current_pattern = d.min(self.patterns.len() - 1);
    }

    /// Index of `path` in the audio pool, adding it if needed.
    pub fn pool_index(&mut self, name: &str, path: &str, data: Arc<SampleData>) -> usize {
        if let Some(i) = self.audio_pool.iter().position(|s| s.path == path) {
            if self.audio_pool[i].data.is_none() {
                self.audio_pool[i].data = Some(data);
            }
            return i;
        }
        self.audio_pool.push(SampleRef { name: name.to_string(), path: path.to_string(), data: Some(data) });
        self.audio_pool.len() - 1
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let w = FileWrapper { app: "SYNTHESIA".into(), version: FILE_VERSION, project: self.clone() };
        let json = serde_json::to_string_pretty(&w).map_err(|e| e.to_string())?;
        #[cfg(target_arch = "wasm32")]
        return crate::web::save_project(path, &json);
        #[cfg(not(target_arch = "wasm32"))]
        {
        // write beside the file and swap it in: a crash or a full disk in the middle never destroys the old project
        let mut tmp = path.as_os_str().to_owned();
        tmp.push(".tmp");
        let tmp = PathBuf::from(tmp);
        std::fs::write(&tmp, json).map_err(|e| format!("{}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, path).map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            format!("{}: {e}", path.display())
        })
        }
    }

    /// Returns the project and a list of warnings (e.g. missing sample files).
    pub fn load(path: &Path) -> Result<(Project, Vec<String>), String> {
        #[cfg(target_arch = "wasm32")]
        let text = crate::web::read_project(path)?;
        #[cfg(not(target_arch = "wasm32"))]
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let w: FileWrapper = serde_json::from_str(&text).map_err(|e| format!("invalid project file: {e}"))?;
        if w.app != "SYNTHESIA" {
            return Err("not a SYNTHESIA project".into());
        }
        if w.version > FILE_VERSION {
            return Err(format!("project version {} is newer than supported {}", w.version, FILE_VERSION));
        }
        // version 1 stored clip positions in whole bars; version 2 stores ticks
        let from_bars = w.version < 2;
        let mut p = w.project;
        p.group_tracks();
        if from_bars {
            let bars_to_ticks = |arr: &mut Arrangement| {
                for c in arr.clips.iter_mut() {
                    c.start *= TICKS_PER_BAR;
                    c.len *= TICKS_PER_BAR;
                }
            };
            bars_to_ticks(&mut p.arrangement);
            for s in p.scenes.iter_mut() {
                bars_to_ticks(&mut s.arrangement);
            }
        }
        p.sanitize();
        let dir: PathBuf = path.parent().map(|d| d.to_path_buf()).unwrap_or_default();
        let mut warnings = vec![];
        {
            let fix = |arr: &mut Arrangement, warnings: &mut Vec<String>| {
                for t in arr.tracks.iter_mut() {
                    let Some((path, name)) = t.sampler.sample.as_ref().map(|r| (r.path.clone(), r.name.clone())) else { continue };
                    let cand = resolve_sample_path(&dir, &path);
                    match load_wav(&cand) {
                        Ok(d) => {
                            if let Some(r) = t.sampler.sample.as_mut() {
                                r.data = Some(Arc::new(d));
                            }
                        }
                        Err(e) => {
                            warnings.push(format!("sample '{name}' of track '{}' not loaded: {e}", t.name));
                        }
                    }
                }
            };
            fix(&mut p.arrangement, &mut warnings);
            for s in p.scenes.iter_mut() {
                fix(&mut s.arrangement, &mut warnings);
            }
        }
        if let Some(r) = p.sampler.sample.as_mut() {
            let cand = resolve_sample_path(&dir, &r.path);
            match load_wav(&cand) {
                Ok(d) => r.data = Some(Arc::new(d)),
                Err(e) => {
                    warnings.push(format!("808 sample '{}' not loaded: {e}", r.name));
                }
            }
        }
        for (li, s) in p.drum_samples.iter_mut().enumerate() {
            if let Some(r) = s {
                let cand = resolve_sample_path(&dir, &r.path);
                match load_wav(&cand) {
                    Ok(d) => {
                        let d = match p.drum_fx.get(li) {
                            Some(fx) if !fx.is_default() => fx.apply(&d),
                            _ => d,
                        };
                        r.data = Some(Arc::new(d));
                    }
                    Err(e) => {
                        warnings.push(format!("sample '{}' not loaded: {e}", r.name));
                    }
                }
            }
        }
        for r in p.audio_pool.iter_mut() {
            let cand = resolve_sample_path(&dir, &r.path);
            let stored = Path::new(&r.path);
            let bounced = stored.parent() == Some(Path::new("bounces"));
            let audio = if bounced {
                std::fs::File::open(&cand).map_err(|error| error.to_string())
                    .and_then(|file| decode_wav_with_limit(std::io::BufReader::new(file), None))
            } else {
                load_wav(&cand)
            };
            match audio {
                Ok(d) => r.data = Some(Arc::new(d)),
                Err(e) => {
                    warnings.push(format!("audio '{}' not loaded: {e}", r.name));
                }
            }
        }
        // Missing media stays referenced so saving cannot silently erase the
        // arrangement or prevent recovery when the files become available again.
        Ok((p, warnings))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_only_picked_tracks() {
        let mut p = Project::default();
        p.arrangement.tracks.clear();
        for k in [crate::arrangement::TrackKind::Midi, crate::arrangement::TrackKind::Audio, crate::arrangement::TrackKind::Midi] {
            p.arrangement.add_track(k);
        }
        p.arrangement.tracks[2].solo = true;
        p.render_only_tracks(&[true, true]);
        assert!(!p.arrangement.muted(0) && !p.arrangement.muted(1), "the picked MIDI and audio tracks play");
        assert!(p.arrangement.muted(2), "a track that is not in the list is silent");
        assert!(p.arrangement.tracks.iter().all(|t| !t.solo), "solos are cleared");
    }

    #[test]
    fn save_load_roundtrip() {
        let mut p = Project::demo();
        p.bpm = 133.5;
        p.swing = 0.3;
        p.synth.lp_cutoff = 1234.0;
        p.bass.glide_ms = 77.0;
        let path = std::env::temp_dir().join("synthesia_roundtrip_test.synthesia");
        p.save(&path).unwrap();
        let (q, warns) = Project::load(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert!(warns.is_empty());
        assert!(p == q, "loaded project differs from saved project");
    }

    #[test]
    fn rejects_garbage() {
        let path = std::env::temp_dir().join("synthesia_garbage_test.synthesia");
        std::fs::write(&path, "{ not a project").unwrap();
        assert!(Project::load(&path).is_err());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn wav_import_rejects_non_finite_audio_and_limits_decoding() {
        let path = std::env::temp_dir().join(format!("vinteria_wav_import_{}.wav", std::process::id()));
        let spec = hound::WavSpec { channels: 2, sample_rate: 10, bits_per_sample: 32, sample_format: hound::SampleFormat::Float };
        let mut writer = hound::WavWriter::create(&path, spec).unwrap();
        for value in [0.0, 0.0, f32::NAN, 0.0] {
            writer.write_sample(value).unwrap();
        }
        writer.finalize().unwrap();
        assert!(load_wav(&path).err().unwrap().contains("non-finite"));

        let mut writer = hound::WavWriter::create(&path, spec).unwrap();
        for _ in 0..600 {
            writer.write_sample(0.25f32).unwrap();
            writer.write_sample(0.75f32).unwrap();
        }
        // Content after the supported minute is never decoded.
        writer.write_sample(f32::NAN).unwrap();
        writer.write_sample(f32::INFINITY).unwrap();
        writer.finalize().unwrap();
        let sample = load_wav(&path).unwrap();
        assert_eq!(sample.mono.len(), 600);
        assert!(sample.mono.iter().all(|&v| v == 0.5));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn missing_samples_survive_save_and_recover_when_restored() {
        let dir = std::env::temp_dir().join(format!("vinteria_media_recovery_{}", std::process::id()));
        let media = dir.join("media");
        std::fs::create_dir_all(&media).unwrap();
        let path = dir.join("recovery.synthesia");
        let mut p = Project::default();
        p.arrangement.add_track(crate::arrangement::TrackKind::Audio);
        p.arrangement.add_track(crate::arrangement::TrackKind::Midi);
        let sample = SampleRef { name: "Recover me".into(), path: "media/recover.wav".into(), data: None };
        p.audio_pool.push(sample.clone());
        p.drum_samples[0] = Some(sample.clone());
        p.sampler.sample = Some(sample.clone());
        p.arrangement.tracks[1].sampler.sample = Some(sample);
        p.arrangement.clips.push(Clip::bars(0, 0, 0, 1, true));
        p.save(&path).unwrap();
        let (q, warnings) = Project::load(&path).unwrap();
        assert_eq!(warnings.len(), 4);
        assert_eq!(q.arrangement.clips.len(), 1);
        assert!(q.drum_samples[0].is_some());
        assert!(q.sampler.sample.is_some());
        assert!(q.arrangement.tracks[0].sampler.sample.is_some());
        q.save(&path).unwrap();

        let wav = media.join("recover.wav");
        let spec = hound::WavSpec { channels: 1, sample_rate: 44100, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
        let mut writer = hound::WavWriter::create(&wav, spec).unwrap();
        for value in [0i16, 12000, -12000, 0] {
            writer.write_sample(value).unwrap();
        }
        writer.finalize().unwrap();
        let (restored, warnings) = Project::load(&path).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(restored.arrangement.clips.len(), 1);
        assert!(restored.audio_pool[0].data.is_some());
        assert!(restored.drum_samples[0].as_ref().unwrap().data.is_some());
        assert!(restored.sampler.sample.as_ref().unwrap().data.is_some());
        assert!(restored.arrangement.tracks[0].sampler.sample.as_ref().unwrap().data.is_some());
        std::fs::remove_file(wav).unwrap();
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(media).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }

    #[test]
    fn new_project_has_no_tracks_until_added() {
        use crate::arrangement::TrackKind;
        let mut p = Project::default();
        p.sanitize();
        assert!(p.arrangement.tracks.is_empty(), "a new project must start without tracks");
        p.arrangement.add_track(TrackKind::Midi);
        p.arrangement.add_track(TrackKind::Audio);
        assert_eq!(p.arrangement.tracks.len(), 2);
        assert_eq!(p.arrangement.kind(1), TrackKind::Audio);
        p.sanitize();
        assert_eq!(p.arrangement.tracks.len(), 2);
    }
}

#[cfg(test)]
mod scene_tests {
    use super::*;
    use crate::arrangement::Arrangement;

    #[test]
    fn deleting_pattern_updates_inactive_scenes_and_session() {
        let mut p = Project::default();
        p.patterns.push(Pattern::new("Delete", 16));
        p.patterns.push(Pattern::new("Keep", 16));
        p.arrangement.tracks = Arrangement::starter_tracks();
        p.arrangement.clips = vec![Clip::bars(1, 0, 0, 1, false), Clip::bars(2, 0, 1, 1, false), Clip::bars(1, 0, 2, 1, true)];
        p.new_scene();
        p.arrangement.clips.push(Clip::bars(2, 0, 0, 1, false));
        p.session.set(0, 0, 2, false);
        p.current_pattern = 1;
        p.delete_pattern();
        assert_eq!(p.arrangement.clips[0].pattern, 1);
        assert_eq!(p.session.get(0, 0).unwrap().pattern, 1);
        p.switch_scene(0);
        assert_eq!(p.arrangement.clips.len(), 2);
        assert_eq!(p.arrangement.clips[0].pattern, 1);
        assert!(p.arrangement.clips[1].audio);
        assert_eq!(p.arrangement.clips[1].pattern, 1);
    }

    #[test]
    fn sanitize_repairs_inactive_scenes_before_switching() {
        let mut p = Project::default();
        p.arrangement.tracks = Arrangement::starter_tracks();
        p.arrangement.tracks[0].output = Some(NUM_INSERTS + 5);
        p.arrangement.clips = vec![Clip::bars(0, 0, 0, 1, false), Clip::bars(999, 0, 0, 1, false), Clip::bars(0, 999, 0, 1, false), Clip::bars(0, 0, 0, 0, false)];
        // The bars constructor enforces one bar; simulate invalid serialized data explicitly.
        p.arrangement.clips.last_mut().unwrap().len = 0;
        p.new_scene();
        p.sanitize();
        p.switch_scene(0);
        assert_eq!(p.arrangement.clips.len(), 1);
        assert_eq!(p.arrangement.tracks[0].output, None);
    }

    #[test]
    fn patterns_own_their_tracks_and_survive_save_load() {
        let mut p = Project::default();
        p.arrangement.tracks = Arrangement::starter_tracks();
        p.arrangement.clips.push(Clip::bars(0, 0, 0, 2, false));
        p.new_scene();
        assert_eq!(p.scenes.len(), 2);
        assert!(p.arrangement.clips.is_empty(), "a new pattern starts empty");
        assert_eq!(p.arrangement.tracks.len(), 6);
        p.switch_scene(0);
        assert_eq!(p.arrangement.clips.len(), 1, "the first pattern kept its clip");
        p.duplicate_scene();
        assert_eq!(p.scenes.len(), 3);
        assert_eq!(p.arrangement.clips.len(), 1);
        assert_ne!(p.arrangement.clips[0].pattern, 0, "the copy plays its own copy of the notes");
        let path = std::env::temp_dir().join("synthesia_scenes_test.synthesia");
        p.save(&path).unwrap();
        let (q, w) = Project::load(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert!(w.is_empty());
        assert_eq!(q.scenes.len(), 3);
        assert_eq!(q.active_scene, 2);
        assert_eq!(q.arrangement.clips.len(), 1);
        let mut q = q;
        q.switch_scene(1);
        assert!(q.arrangement.clips.is_empty());
        q.delete_scene();
        assert_eq!(q.scenes.len(), 2);
    }

    #[test]
    fn old_projects_without_scenes_load_as_one_pattern() {
        let mut p = Project::default();
        p.arrangement.tracks = Arrangement::starter_tracks();
        p.scenes.clear();
        p.sanitize();
        assert_eq!(p.scenes.len(), 1);
        assert_eq!(p.arrangement.tracks.len(), 6);
    }

    #[test]
    fn rack_beat_splits_into_channel_tracks() {
        use crate::piano_roll::Note;
        let mut p = Project::default();
        p.arrangement.tracks = Arrangement::starter_tracks();
        let pat = &mut p.patterns[0];
        pat.length_steps = 32;
        pat.set_steps(0, &[0, 8, 16, 24], 120); // kick
        pat.set_steps(3, &[2, 6], 90); // hat
        pat.notes.push(Note { start: 0, len: 48, pitch: 60, vel: 100, track: Track::Wavetable, slide: false, mute: false, pan: 0, chance: 100 });
        let n = p.split_pattern_by_channel(0, true, 3);
        assert_eq!(n, 3);
        let names: Vec<&str> = p.arrangement.tracks.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names[..3], ["Kick", "Hi-Hat", "Wavetable"], "drums first, then instruments: {names:?}");
        assert!(names[3..].iter().all(|n| n.starts_with("Audio")), "starter MIDI tracks made room, audio stays last: {names:?}");
        assert_eq!(p.arrangement.clips.len(), 9, "three channels x three repeats");
        assert!(p.arrangement.tracks.iter().take(3).all(|t| !t.blank), "split tracks are not silent");
        let kick = p.arrangement.clips.iter().filter(|c| c.lane == 0).count();
        assert_eq!(kick, 3);
    }

    #[test]
    fn version_one_files_store_clips_in_bars() {
        let mut p = Project::default();
        p.arrangement.tracks = Arrangement::starter_tracks();
        p.arrangement.clips.push(Clip::bars(0, 0, 2, 3, false));
        let path = std::env::temp_dir().join("synthesia_v1_clip_test.synthesia");
        p.save(&path).unwrap();
        // make it look like a file written before clips were stored in ticks
        let text = std::fs::read_to_string(&path).unwrap();
        let old = text.replace("\"version\": 2", "\"version\": 1").replace("\"start\": 768", "\"start_bar\": 2").replace("\"len\": 1152", "\"bars\": 3");
        assert_ne!(old, text);
        std::fs::write(&path, old).unwrap();
        let (q, _) = Project::load(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        let c = q.arrangement.clips[0];
        assert_eq!((c.start_tick(), c.end_tick()), (768, 768 + 1152));
        // and a clip can now sit off the bar grid
        let d = Clip::new(0, 1, 96, 48, false);
        assert_eq!((d.start_bar(), d.end_bar()), (0, 1));
    }
}

#[cfg(test)]
mod midi_clip_tests {
    use super::*;

    #[test]
    fn the_rack_shows_the_selected_clip_notes_and_never_resends_them() {
        use crate::arrangement::TrackKind;
        let mut p = Project::default();
        p.arrangement.add_track(TrackKind::Midi);
        let mut pat = Pattern::new("clip", 16);
        pat.clip_only = true;
        pat.notes.push(Note { start: 0, len: 24, pitch: 60, vel: 100, track: Track::Synth, slide: false, mute: false, pan: 0, chance: 100 });
        p.patterns.push(pat);
        p.current_pattern = p.patterns.len() - 1;
        // the clip's own pattern stays the one that is shown (before, the rack jumped to an empty "Pattern 1")
        assert_eq!(p.rack_pattern(), p.patterns.len() - 1);
        assert_eq!(p.send_rack_to_tracks(p.rack_pattern(), 2), 0, "a clip's notes are not sent again");
    }

    #[test]
    fn retrospective_record_turns_played_notes_into_a_quantized_clip() {
        use crate::arrangement::TrackKind;
        let mut p = Project::default();
        p.bpm = 120.0; // a beat = 0.5 s
        p.arrangement.add_track(TrackKind::Midi);
        // three notes played a beat apart, slightly off the grid, held a bit
        let notes = [
            RetroNote { t: 10.02, len_s: 0.24, pitch: 60, vel: 100 },
            RetroNote { t: 10.53, len_s: 0.26, pitch: 64, vel: 90 },
            RetroNote { t: 11.02, len_s: 0.49, pitch: 67, vel: 80 },
        ];
        let ci = p.retro_to_clip(0, Track::Synth, &notes).expect("a clip");
        let c = p.arrangement.clips[ci];
        let pat = &p.patterns[c.pattern];
        assert!(pat.clip_only);
        let mut starts: Vec<u32> = pat.notes.iter().map(|n| n.start).collect();
        starts.sort();
        assert_eq!(starts, vec![0, 96, 192], "notes land on the beat grid");
        assert_eq!(pat.notes.len(), 3);
        assert!(p.retro_to_clip(0, Track::Synth, &[]).is_none());
        assert!(p.retro_to_clip(5, Track::Synth, &notes).is_none(), "no such track");
    }

    #[test]
    fn deleted_clips_leave_no_patterns_behind_and_survive_a_save() {
        use crate::arrangement::{Clip, TrackKind};
        let mut p = Project::default();
        p.arrangement.add_track(TrackKind::Midi);
        p.arrangement.tracks[0].instrument = Some(Track::Synth);
        for k in 0..3u8 {
            let mut pat = Pattern::new(&format!("clip {k}"), 16);
            pat.notes.push(Note { start: 0, len: 24, pitch: 60 + k, vel: 100, track: Track::Synth, slide: false, mute: false, pan: 0, chance: 100 });
            pat.clip_only = true;
            p.patterns.push(pat);
            p.arrangement.clips.push(Clip::bars(p.patterns.len() - 1, 0, k as u32, 1, false));
        }
        // the middle clip is deleted
        p.arrangement.clips.remove(1);
        let before = p.patterns.len();
        let dropped = p.prune_orphan_patterns();
        assert_eq!(dropped, 1);
        assert_eq!(p.patterns.len(), before - 1);
        // the remaining clips still point at their own notes
        let pitches: Vec<u8> = p.arrangement.clips.iter().map(|c| p.patterns[c.pattern].notes[0].pitch).collect();
        assert_eq!(pitches, vec![60, 62]);
        // saving leaves no temp file and the file loads back equal
        let path = std::env::temp_dir().join("wynteria_save_test.synthesia");
        p.adopt_track_params();
        p.save(&path).unwrap();
        assert!(!std::path::Path::new(&format!("{}.tmp", path.display())).exists());
        let (q, w) = Project::load(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert!(w.is_empty());
        assert!(p == q);
    }

    #[test]
    fn rack_notes_go_to_clips_on_their_own_tracks() {
        use crate::arrangement::TrackKind;
        let mut p = Project::default();
        p.arrangement.add_track(TrackKind::Midi);
        p.arrangement.add_track(TrackKind::Midi);
        p.arrangement.tracks[0].instrument = Some(Track::Synth);
        p.arrangement.tracks[1].instrument = Some(Track::Bass808);
        let pat = &mut p.patterns[0];
        pat.notes.push(Note { start: 0, len: 24, pitch: 60, vel: 100, track: Track::Synth, slide: false, mute: false, pan: 0, chance: 100 });
        pat.notes.push(Note { start: 48, len: 24, pitch: 36, vel: 100, track: Track::Bass808, slide: false, mute: false, pan: 0, chance: 100 });
        let sent = p.send_rack_to_tracks(0, 2);
        assert_eq!(sent, 2);
        assert_eq!(p.arrangement.clips.iter().filter(|c| c.lane == 0).count(), 2, "repeat 2 on the first track");
        assert_eq!(p.arrangement.clips.iter().filter(|c| c.lane == 1).count(), 2);
        // the clips hold only that track's notes
        let c0 = p.arrangement.clips.iter().find(|c| c.lane == 0).unwrap();
        assert!(p.patterns[c0.pattern].notes.iter().all(|n| n.track == Track::Synth));
        assert!(p.patterns[c0.pattern].clip_only);
        // a second send goes after the first
        assert_eq!(p.send_rack_to_tracks(0, 1), 2);
        assert_eq!(p.arrangement.clips.iter().filter(|c| c.lane == 0).count(), 3);
    }

    #[test]
    fn midi_clip_content_is_not_a_pattern_and_survives_save_load() {
        let mut p = Project::default();
        p.arrangement.add_track(crate::arrangement::TrackKind::Midi);
        let lane_pat = p.lane_pattern(0);
        assert!(p.patterns[lane_pat].clip_only, "a track's own content is clip content");
        assert!(!p.patterns[0].clip_only, "a real pattern stays listed");
        p.arrangement.clips.push(Clip::bars(lane_pat, 0, 0, 2, false));
        let path = std::env::temp_dir().join("synthesia_midi_clip_test.synthesia");
        p.save(&path).unwrap();
        let (q, _) = Project::load(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert!(q.patterns[lane_pat].clip_only);
        assert!(!q.patterns[0].clip_only);
    }
}
