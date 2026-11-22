//! Audio Engine: sample-accurate sequencing + mixing. Independent of any UI or audio device code.
//! `Engine::render` performs no allocation and takes no locks, so it is safe in an audio callback.
pub mod device;
pub mod dsp;
pub mod fastmath;
pub mod export;

use crate::effects::{Biquad, Compressor, Delay, Limiter};
use crate::instruments::bass808::Bass808;
use crate::instruments::drums::DrumLane;
use crate::instruments::analog::AnalogSynth;
use crate::instruments::synth::Synth;
use crate::instruments::wavetable::WtSynth;
use crate::mixer::{effective_gains, NUM_CH, NUM_INSERTS, NUM_STRIPS};
use crate::piano_roll::Track;
use crate::project::{Project, SampleData};
use crate::sequencer::*;
use crate::session::{code, MAX_LANES};
use dsp::Rng;
use std::sync::Arc;

pub enum Cmd {
    Play,
    PatternLoop(bool),
    PatternRange(Option<(usize, u32, u32)>),
    /// halt without rewinding: the next Play goes on from the same place
    Pause,
    /// move the playhead: while playing the song jumps there, when stopped the next Play starts from there
    Seek(u32),
    Stop,
    NoteOn { track: Track, pitch: u8, vel: u8 },
    NoteOff { track: Track, pitch: u8 },
    DrumHit { lane: usize, vel: u8 },
    Preview(Arc<SampleData>),
    /// start the preview player at a point of the sound (0..1)
    PreviewFrom(Arc<SampleData>, f32),
    PreviewStop,
    /// volume of the Browser's sound preview (0..1)
    PreviewGain(f32),
    /// play the song from this tick (also while it is already playing: a jump)
    PlayFrom(u32),
    /// session view: start a clip on a track at the next bar (immediately if stopped)
    LaunchClip { lane: usize, pattern: usize, audio: bool },
    /// session view: stop the track's clip at the next bar
    StopLane { lane: usize },
    StopAllClips,
    /// the Playlist track whose sampler plays live notes (keyboard / piano roll preview)
    SetLiveLane(usize),
}

/// value of `Shared::session_queue` meaning "stop at next bar"
pub const QUEUED_STOP: u32 = u32::MAX;

#[derive(Clone, Copy)]
struct Run {
    pattern: usize,
    audio: bool,
    start: u32,
}

const AUDIO_VOICES: usize = 16;

/// One complete set of sound sources. Bank 0 feeds the instrument channels; bank `1 + i` holds the
/// sounds of arrangement tracks routed to mixer insert `i`, so they never share voices or levels
/// with the rest of the project.
struct Bank {
    synth: Synth,
    bass: Bass808,
    sampler: crate::instruments::sampler::Sampler,
    /// one sampler per Playlist track: a WAV dropped on a MIDI track plays on its own
    lanes: Vec<crate::instruments::sampler::Sampler>,
    wt: WtSynth,
    analog: AnalogSynth,
    drums: Vec<DrumLane>,
}

impl Bank {
    fn new() -> Self {
        Bank {
            synth: Synth::new(),
            bass: Bass808::new(),
            sampler: crate::instruments::sampler::Sampler::new(),
            lanes: (0..MAX_LANES).map(|_| crate::instruments::sampler::Sampler::new()).collect(),
            wt: WtSynth::new(),
            analog: AnalogSynth::new(),
            drums: (0..NUM_DRUMS).map(|_| DrumLane::new()).collect(),
        }
    }
    fn release_all(&mut self) {
        self.synth.release_all();
        self.bass.release_all();
        self.sampler.release_all();
        for s in self.lanes.iter_mut() {
            s.release_all();
        }
        self.wt.release_all();
        self.analog.release_all();
    }
    fn release_due(&mut self, abs: u64) {
        self.synth.release_due(abs);
        self.bass.release_due(abs);
        self.sampler.release_due(abs);
        for s in self.lanes.iter_mut() {
            s.release_due(abs);
        }
        self.wt.release_due(abs);
        self.analog.release_due(abs);
    }
    fn note_on(&mut self, track: Track, pitch: u8, vel: f32, slide: Option<(f32, f32)>, end: u64) {
        let sl = slide.is_some();
        match track {
            Track::Synth => {
                if !slide.map_or(false, |(g, sr)| self.synth.slide_to(pitch, vel, end, g, sr)) {
                    self.synth.note_on(pitch, vel, end)
                }
            }
            Track::Bass808 => self.bass.note_on(pitch, vel, sl, end),
            Track::Sampler => self.sampler.note_on(pitch, vel, end),
            Track::Wavetable => {
                if !slide.map_or(false, |(g, sr)| self.wt.slide_to(pitch, vel, end, g, sr)) {
                    self.wt.note_on(pitch, vel, end)
                }
            }
            Track::Analog => {
                if !slide.map_or(false, |(g, sr)| self.analog.slide_to(pitch, vel, end, g, sr)) {
                    self.analog.note_on(pitch, vel, end)
                }
            }
            Track::Drums => {
                let lane = (pitch.saturating_sub(crate::piano_roll::DRUM_BASE) as usize).min(NUM_DRUMS - 1);
                self.drums[lane].trigger(vel);
            }
        }
    }
}

/// The four built-in synths of ONE Playlist track that has its own settings (`ArrTrack::snd`).
struct LaneSyn {
    synth: Synth,
    bass: Bass808,
    wt: WtSynth,
    analog: AnalogSynth,
}

impl LaneSyn {
    fn new() -> Self {
        LaneSyn { synth: Synth::new(), bass: Bass808::new(), wt: WtSynth::new(), analog: AnalogSynth::new() }
    }
    fn release_all(&mut self) {
        self.synth.release_all();
        self.bass.release_all();
        self.wt.release_all();
        self.analog.release_all();
    }
    fn release_due(&mut self, abs: u64) {
        self.synth.release_due(abs);
        self.bass.release_due(abs);
        self.wt.release_due(abs);
        self.analog.release_due(abs);
    }
    fn note_on(&mut self, track: Track, pitch: u8, vel: f32, slide: Option<(f32, f32)>, end: u64) {
        let sl = slide.is_some();
        match track {
            Track::Synth => {
                if !slide.map_or(false, |(g, sr)| self.synth.slide_to(pitch, vel, end, g, sr)) {
                    self.synth.note_on(pitch, vel, end)
                }
            }
            Track::Bass808 => self.bass.note_on(pitch, vel, sl, end),
            Track::Wavetable => {
                if !slide.map_or(false, |(g, sr)| self.wt.slide_to(pitch, vel, end, g, sr)) {
                    self.wt.note_on(pitch, vel, end)
                }
            }
            Track::Analog => {
                if !slide.map_or(false, |(g, sr)| self.analog.slide_to(pitch, vel, end, g, sr)) {
                    self.analog.note_on(pitch, vel, end)
                }
            }
            _ => {}
        }
    }
    fn note_off(&mut self, track: Track, pitch: u8) {
        match track {
            Track::Synth => self.synth.note_off(pitch),
            Track::Bass808 => self.bass.note_off(),
            Track::Wavetable => self.wt.note_off(pitch),
            Track::Analog => self.analog.note_off(pitch),
            _ => {}
        }
    }
}

/// Playback of one audio-track clip.
#[derive(Clone, Copy, Default)]
struct AudioVoice {
    active: bool,
    pool: usize,
    lane: usize,
    pos: f64,
    end_abs: u64,
    fade: f32,
    /// playback-rate multiplier from the track's transpose (0 = unset = 1)
    mul: f64,
    /// volume of the clip
    gain: f32,
    /// Ableton-style clip settings: backwards playback, fades (engine samples), where the clip began and how long it lasts
    rev: bool,
    start_pos: f64,
    elapsed: f64,
    len_s: f64,
    fade_in_s: f64,
    fade_out_s: f64,
}

/// The per-clip settings an audio clip starts with.
#[derive(Clone, Copy, Default)]
struct ClipFx {
    reverse: bool,
    fade_in: u32,
    fade_out: u32,
    pitch: i8,
    len: u32,
    warp: f32,
}

impl Engine {
    /// how far the preview player is through its sound (0..1), None when it is not playing
    pub fn preview_progress(&self) -> Option<f32> {
        self.preview.as_ref().filter(|(_, _, a)| *a).map(|(s, pos, _)| (*pos as f32 / s.mono.len().max(1) as f32).clamp(0.0, 1.0))
    }
}

pub struct Engine {
    pub sr: f32,
    project: Arc<Project>,
    pub playing: bool,
    /// stopped by Pause: Play resumes instead of starting over
    paused: bool,
    countdown: f64,
    resume_audio_at_seek: bool,
    abs_tick: u64,
    song_tick: u32,
    /// position shown in the UI, in ticks (pattern-local in pattern mode)
    pub position: u32,
    banks: Vec<Bank>,
    /// one set of synths per Playlist track: tracks with their own settings play through these
    lane_syn: Vec<LaneSyn>,
    audio: Vec<AudioVoice>,
    session_on: bool,
    run: [Option<Run>; MAX_LANES],
    /// Some(Some(run)) = launch at next bar, Some(None) = stop at next bar
    queue: [Option<Option<Run>>; MAX_LANES],
    preview: Option<(Arc<SampleData>, f64, bool)>,
    preview_gain: f32,
    rng: Rng,
    /// Smart Step pan of the last hit per drum lane, added to the channel pan
    step_pan: [f32; NUM_CH],
    gain_s: [f32; NUM_STRIPS],
    master_s: f32,
    live_lane: usize,
    /// automation multipliers at the current song tick (1.0 without automation)
    lane_auto: [f32; MAX_LANES],
    master_auto: f32,
    duck_env: f32,
    /// the live player: a song that reaches its end starts again until the user presses Stop (export and tests stop at the end)
    pub live_loop: bool,
    pattern_loop: bool,
    pattern_range: Option<(usize, u32, u32)>,
    /// velocity multiplier of the MIDI clip being fired (its fade in / out)
    clip_fade: f32,
    fx_run: Vec<crate::fx::FxChain>,
    master_run: crate::fx::FxChain,
    /// metronome click: envelope, oscillator phase and pitch
    click_env: f32,
    click_ph: f32,
    click_hz: f32,
    delay: Delay,
    limiter: Limiter,
    comp: Compressor,
    eq_lo: [Biquad; 2],
    eq_hi: [Biquad; 2],
    /// largest compressor gain reduction (dB) since the last `take_gr`
    gr_db: f32,
    /// peak per strip (L,R) and master at index NUM_STRIPS since last `take_peaks`
    peaks: [[f32; 2]; NUM_STRIPS + 1],
}

impl Engine {
    pub fn new(sr: f32) -> Self {
        Engine {
            sr,
            project: Arc::new(Project::default()),
            playing: false,
            paused: false,
            countdown: 0.0,
            resume_audio_at_seek: false,
            abs_tick: 0,
            song_tick: 0,
            position: 0,
            banks: (0..=NUM_INSERTS).map(|_| Bank::new()).collect(),
            lane_syn: (0..MAX_LANES).map(|_| LaneSyn::new()).collect(),
            audio: (0..AUDIO_VOICES).map(|_| AudioVoice::default()).collect(),
            session_on: false,
            run: [None; MAX_LANES],
            queue: [None; MAX_LANES],
            preview: None,
            preview_gain: 1.0,
            rng: Rng::new(0x1234_5678),
            step_pan: [0.0; NUM_CH],
            gain_s: [0.0; NUM_STRIPS],
            master_s: 0.0,
            live_lane: 0,
            lane_auto: [1.0; MAX_LANES],
            master_auto: 1.0,
            duck_env: 0.0,
            live_loop: false,
            pattern_loop: true,
            pattern_range: None,
            clip_fade: 1.0,
            fx_run: (0..NUM_INSERTS).map(|_| crate::fx::FxChain::new()).collect(),
            master_run: crate::fx::FxChain::new(),
            click_env: 0.0,
            click_ph: 0.0,
            click_hz: 1000.0,
            delay: Delay::new(sr),
            limiter: Limiter::new(),
            comp: Compressor::new(),
            eq_lo: [Biquad::default(); 2],
            eq_hi: [Biquad::default(); 2],
            gr_db: 0.0,
            peaks: [[0.0; 2]; NUM_STRIPS + 1],
        }
    }

    /// Swap in a new project snapshot; returns the old one so the caller can drop it off the audio thread.
    pub fn set_project(&mut self, p: Arc<Project>) -> Arc<Project> {
        std::mem::replace(&mut self.project, p)
    }

    pub fn project_ptr(&self) -> *const Project {
        Arc::as_ptr(&self.project)
    }

    pub fn take_gr(&mut self) -> f32 {
        std::mem::replace(&mut self.gr_db, 0.0)
    }

    pub fn take_peaks(&mut self) -> [[f32; 2]; NUM_STRIPS + 1] {
        std::mem::replace(&mut self.peaks, [[0.0; 2]; NUM_STRIPS + 1])
    }

    pub fn apply(&mut self, c: Cmd) {
        match c {
            Cmd::Play => self.play(),
            Cmd::PatternLoop(looping) => self.pattern_loop = looping,
            Cmd::PatternRange(range) => self.pattern_range = range,
            Cmd::Stop => self.stop(),
            Cmd::Pause => {
                if self.playing {
                    self.playing = false;
                    self.paused = true;
                    self.release_everything();
                }
            }
            Cmd::SetLiveLane(l) => self.live_lane = l.min(MAX_LANES - 1),
            Cmd::NoteOn { track, pitch, vel } => {
                let l = self.live_lane;
                if self.lane_own(l, track) {
                    self.lane_syn[l].note_on(track, pitch, vel as f32 / 127.0, None, u64::MAX);
                } else if track == Track::Sampler && self.lane_has_sample(l) {
                    self.banks[0].lanes[l].note_on(pitch, vel as f32 / 127.0, u64::MAX);
                } else {
                    self.banks[0].note_on(track, pitch, vel as f32 / 127.0, None, u64::MAX);
                }
            }
            Cmd::NoteOff { track, pitch } => {
                let l = self.live_lane;
                if self.lane_own(l, track) {
                    self.lane_syn[l].note_off(track, pitch);
                }
                let b = &mut self.banks[0];
                match track {
                    Track::Synth => b.synth.note_off(pitch),
                    Track::Bass808 => b.bass.note_off(),
                    Track::Sampler => {
                        b.sampler.note_off(pitch);
                        b.lanes[self.live_lane].note_off(pitch);
                    }
                    Track::Wavetable => b.wt.note_off(pitch),
                    Track::Analog => b.analog.note_off(pitch),
                    Track::Drums => {}
                }
            }
            Cmd::DrumHit { lane, vel } => {
                if lane < NUM_DRUMS {
                    self.banks[0].drums[lane].trigger(vel as f32 / 127.0);
                }
            }
            Cmd::Preview(s) => self.preview = Some((s, 0.0, true)),
            Cmd::PreviewFrom(s, f) => {
                let pos = f.clamp(0.0, 1.0) as f64 * s.mono.len() as f64;
                self.preview = Some((s, pos, true));
            }
            Cmd::PreviewStop => self.preview = None,
            Cmd::PreviewGain(g) => self.preview_gain = g.clamp(0.0, 1.0),
            Cmd::Seek(t) => {
                self.resume_audio_at_seek = true;
                self.position = t;
                self.song_tick = t;
                self.countdown = 0.0;
                if self.playing {
                    self.release_everything();
                    self.stop_audio_clips();
                } else {
                    self.abs_tick = 0;
                    self.paused = true;
                    for voice in &mut self.audio {
                        voice.active = false;
                    }
                }
            }
            Cmd::PlayFrom(t) => {
                self.resume_audio_at_seek = true;
                self.paused = false;
                if !self.playing {
                    self.abs_tick = 0;
                    self.countdown = 0.0;
                    self.playing = true;
                }
                self.release_everything();
                self.stop_audio_clips();
                self.song_tick = t;
                self.position = t;
                self.countdown = 0.0;
            }
            Cmd::LaunchClip { lane, pattern, audio } => {
                if lane < MAX_LANES {
                    self.queue[lane] = Some(Some(Run { pattern, audio, start: 0 }));
                    self.session_on = true;
                    self.play();
                }
            }
            Cmd::StopLane { lane } => {
                if lane < MAX_LANES && (self.run[lane].is_some() || self.queue[lane].is_some()) {
                    self.queue[lane] = Some(None);
                }
            }
            Cmd::StopAllClips => {
                for lane in 0..MAX_LANES {
                    if self.run[lane].is_some() || self.queue[lane].is_some() {
                        self.queue[lane] = Some(None);
                    }
                }
            }
        }
    }

    /// Pattern mode while a Playlist MIDI clip is open in the piano roll: the clip's stretch of the song is looped with every
    /// other track playing along (start tick, end tick), so the part is heard in its context.
    fn context_range(proj: &Project) -> Option<(u32, u32)> {
        if proj.song_mode {
            return None;
        }
        let pat = proj.patterns.get(proj.current_pattern)?;
        if !pat.clip_only {
            return None;
        }
        proj.arrangement.clips.iter().find(|c| !c.audio && c.pattern == proj.current_pattern).map(|c| (c.start_tick(), c.end_tick()))
    }

    pub fn play(&mut self) {
        if !self.playing && self.paused {
            // resume where Pause stopped
            self.paused = false;
            self.countdown = 0.0;
            self.playing = true;
        } else if !self.playing {
            self.release_everything();
            self.abs_tick = 0;
            self.song_tick = if let Some((s, _)) = Self::context_range(&self.project) {
                s
            } else if self.project.song_mode && self.project.loop_on {
                self.resume_audio_at_seek = true;
                self.project.loop_start * TICKS_PER_BAR
            } else {
                0
            };
            self.countdown = 0.0;
            self.playing = true;
        }
    }

    /// (running code, queued code) of a session lane for the UI
    pub fn session_codes(&self, lane: usize) -> (u32, u32) {
        let r = self.run[lane].map(|r| code(r.pattern, r.audio)).unwrap_or(0);
        let q = match self.queue[lane] {
            None => 0,
            Some(None) => QUEUED_STOP,
            Some(Some(r)) => code(r.pattern, r.audio),
        };
        (r, q)
    }

    /// Apply queued launches/stops (called on bar boundaries) and loop audio clips that ended.
    fn session_bar(&mut self, proj: &Project) {
        let st = self.song_tick;
        for lane in 0..MAX_LANES {
            if let Some(q) = self.queue[lane].take() {
                if let Some(old) = self.run[lane].take() {
                    if old.audio {
                        self.end_audio(old.pattern);
                    }
                }
                if let Some(mut r) = q {
                    r.start = st;
                    if r.audio && !proj.arrangement.muted(lane) {
                        self.start_audio(r.pattern, lane, u64::MAX, 0, 1.0, ClipFx::default());
                    }
                    self.run[lane] = Some(r);
                }
            } else if let Some(r) = self.run[lane] {
                if r.audio && !proj.arrangement.muted(lane) && !self.audio.iter().any(|v| v.active && v.pool == r.pattern) {
                    self.start_audio(r.pattern, lane, u64::MAX, 0, 1.0, ClipFx::default());
                }
            }
        }
    }

    fn end_audio(&mut self, pool: usize) {
        for v in self.audio.iter_mut() {
            if v.active && v.pool == pool {
                v.end_abs = 0;
            }
        }
    }

    pub fn stop(&mut self) {
        self.paused = false;
        self.preview = None;
        self.lane_auto = [1.0; MAX_LANES];
        self.master_auto = 1.0;
        self.session_on = false;
        self.run = [None; MAX_LANES];
        self.queue = [None; MAX_LANES];
        self.playing = false;
        self.song_tick = 0;
        self.resume_audio_at_seek = false;
        self.position = 0;
        self.release_everything();
        self.stop_audio_clips();
    }

    fn release_everything(&mut self) {
        for b in self.banks.iter_mut() {
            b.release_all();
        }
        for s in self.lane_syn.iter_mut() {
            s.release_all();
        }
    }

    fn stop_audio_clips(&mut self) {
        for v in self.audio.iter_mut() {
            if v.active {
                v.end_abs = 0;
                // Stop/Pause do not advance ticks: begin the fade here so Stop
                // silences clips even when on_tick will no longer be called.
                v.fade = v.fade.min(0.999);
            }
        }
    }

    /// Bank that plays the sounds of arrangement track `lane`.
    fn lane_has_sample(&self, lane: usize) -> bool {
        self.project.arrangement.tracks.get(lane).map_or(false, |t| t.sampler.sample.as_ref().and_then(|s| s.data.as_ref()).is_some())
    }

    /// true when Playlist track `lane` plays built-in synth `t` with settings of its own
    fn lane_own(&self, lane: usize, t: Track) -> bool {
        lane < MAX_LANES && self.project.arrangement.tracks.get(lane).map_or(false, |x| x.snd.own && x.instrument == Some(t)) && matches!(t, Track::Synth | Track::Bass808 | Track::Wavetable | Track::Analog)
    }

    fn bank_of(proj: &Project, lane: usize) -> usize {
        proj.arrangement.tracks.get(lane).and_then(|t| t.output).map(|o| 1 + o.min(NUM_INSERTS - 1)).unwrap_or(0)
    }

    fn tick_metronome(&mut self, musical_tick: u32) {
        if self.project.metronome && musical_tick % PPQ == 0 {
            self.click_env = 1.0;
            self.click_ph = 0.0;
            self.click_hz = if musical_tick % TICKS_PER_BAR == 0 { 1600.0 } else { 1000.0 };
        }
    }

    fn on_tick(&mut self) {
        let proj = Arc::clone(&self.project);
        let abs = self.abs_tick;
        for b in self.banks.iter_mut() {
            b.release_due(abs);
        }
        for s in self.lane_syn.iter_mut() {
            s.release_due(abs);
        }
        for v in self.audio.iter_mut() {
            if v.active && v.end_abs <= abs && v.fade >= 1.0 {
                v.fade = 0.999; // start a short fade-out instead of cutting (no click)
            }
        }
        let swing = swing_ticks(proj.swing);
        if self.session_on {
            let st = self.song_tick;
            self.tick_metronome(st);
            if st % TICKS_PER_BAR == 0 {
                self.session_bar(&proj);
            }
            for lane in 0..MAX_LANES {
                let Some(r) = self.run[lane] else { continue };
                if r.audio || proj.arrangement.silent(lane) {
                    continue;
                }
                if let Some(pat) = proj.patterns.get(r.pattern) {
                    let local = st.wrapping_sub(r.start) % pat.len_ticks();
                    self.fire(Self::bank_of(&proj, lane), Some(lane), pat, local, abs, u64::MAX, swing, proj.arrangement.tracks.get(lane).and_then(|t| t.instrument), proj.arrangement.tracks.get(lane).map_or(0, |t| t.transpose));
                }
            }
            self.position = st % TICKS_PER_BAR;
            self.song_tick = st.wrapping_add(1);
        } else if proj.song_mode || Self::context_range(&proj).is_some() {
            let ctx_range = Self::context_range(&proj);
            // A switched-on loop repeats its range; a song without a loop stops at its end.
            let (loop_start, loop_ticks) = if let Some((s, e)) = ctx_range {
                (s, e)
            } else if proj.loop_on {
                (proj.loop_start * TICKS_PER_BAR, (proj.loop_start + proj.loop_bars.max(1)) * TICKS_PER_BAR)
            } else {
                (0, proj.arrangement.song_end_bar().max(1) * TICKS_PER_BAR)
            };
            let mut st = self.song_tick;
            // automation: the level curves at this tick (only the Playlist plays them)
            for (l, t) in proj.arrangement.tracks.iter().enumerate().take(MAX_LANES) {
                self.lane_auto[l] = crate::arrangement::auto_at(&t.auto_vol, st).unwrap_or(1.0);
            }
            self.master_auto = crate::arrangement::auto_at(&proj.master_auto, st).unwrap_or(1.0);
            if ctx_range.is_some() && (st < loop_start || st >= loop_ticks) {
                st = loop_start;
                self.song_tick = st;
            }
            self.tick_metronome(if ctx_range.is_some() { st - loop_start } else { st });
            let seek = std::mem::take(&mut self.resume_audio_at_seek);
            for clip in &proj.arrangement.clips {
                if clip.audio {
                    let start = clip.start_tick();
                    let end = clip.end_tick();
                    let inside = seek && st > start && st < end;
                    if (st == start || inside) && !proj.arrangement.muted(clip.lane) && !clip.mute {
                        let sample_rate = self.sr;
                        let voice = self.start_audio(clip.pattern, clip.lane, abs + (end - st) as u64, clip.offset, clip.gain, ClipFx { reverse: clip.reverse, fade_in: clip.fade_in, fade_out: clip.fade_out, pitch: clip.pitch, len: clip.len, warp: clip.warp });
                        if inside {
                            let rate = proj.audio_pool.get(clip.pattern).and_then(|sample| sample.data.as_ref())
                                .map_or(0.0, |sample| sample.rate as f64);
                            let secs = f64::from(st - start) * 60.0 / (f64::from(proj.bpm.max(20.0)) * f64::from(PPQ));
                            let skipped = secs * rate * voice.mul;
                            voice.pos += if voice.rev { -skipped } else { skipped };
                            voice.elapsed = secs * f64::from(sample_rate);
                        }
                    }
                    continue;
                }
                if st >= clip.start_tick() && st < clip.end_tick() && !proj.arrangement.silent(clip.lane) && !clip.mute {
                    if let Some(pat) = proj.patterns.get(clip.pattern) {
                        let local = (st - clip.start_tick() + clip.offset) % pat.len_ticks();
                        let remaining = (clip.end_tick() - st) as u64;
                        let pos = (st - clip.start_tick()) as f32;
                        let mut fade = 1.0f32;
                        if clip.fade_in > 0 {
                            fade = fade.min((pos / clip.fade_in as f32).min(1.0));
                        }
                        if clip.fade_out > 0 {
                            fade = fade.min(((clip.len as f32 - pos) / clip.fade_out as f32).clamp(0.0, 1.0));
                        }
                        self.clip_fade = fade.max(0.05);
                        self.fire(Self::bank_of(&proj, clip.lane), Some(clip.lane), pat, local, abs, remaining, swing, proj.arrangement.tracks.get(clip.lane).and_then(|t| t.instrument), proj.arrangement.tracks.get(clip.lane).map_or(0, |t| t.transpose) + clip.pitch as i32);
                        self.clip_fade = 1.0;
                    }
                }
            }
            // the piano roll shows the position inside the pattern it edits; the Playlist shows the song position
            self.position = match ctx_range.and_then(|_| proj.patterns.get(proj.current_pattern)) {
                Some(pat) => (st - loop_start) % pat.len_ticks().max(1),
                None => st,
            };
            self.song_tick += 1;
            if self.song_tick >= loop_ticks {
                if ctx_range.is_some() || proj.loop_on || self.live_loop {
                    self.song_tick = loop_start;
                    self.resume_audio_at_seek = true;
                    // cut notes at the loop point so they do not hang into the restart
                    self.release_everything();
                    self.stop_audio_clips();
                } else {
                    self.stop();
                }
            }
        } else if let Some(pat) = proj.patterns.get(proj.current_pattern) {
            let period = pat.len_ticks().max(1);
            let range = self.pattern_range.filter(|&(pattern, start, end)| {
                self.pattern_loop && pattern == proj.current_pattern && start < end && end <= period
            });
            let mut local = self.song_tick % period;
            if let Some((_, start, end)) = range {
                if !(start..end).contains(&local) {
                    self.song_tick = start;
                    local = start;
                }
            }
            self.tick_metronome(local);
            let remaining = range.map_or(u64::MAX, |(_, _, end)| u64::from(end - local));
            self.fire(0, None, pat, local, abs, remaining, swing, None, 0);
            self.position = local;
            self.song_tick = self.song_tick.wrapping_add(1);
            if let Some((_, start, end)) = range {
                if local + 1 >= end {
                    self.song_tick = start;
                    self.release_everything();
                }
            } else if !self.pattern_loop && local + 1 >= period {
                self.stop();
            }
        }
        self.abs_tick += 1;
    }

    fn start_audio(&mut self, pool: usize, lane: usize, end_abs: u64, offset_ticks: u32, gain: f32, fx: ClipFx) -> &mut AudioVoice {
        if self.project.arrangement.tracks.get(lane).map_or(false, |t| t.sidechain_src) {
            self.duck_env = 1.0;
        }
        let idx = self.audio.iter().position(|v| !v.active).unwrap_or(0);
        // a clip that was cut in two starts playing its sample where the first half ended
        let ref_bpm = if fx.warp > 0.0 { fx.warp as f64 } else { self.project.bpm.max(20.0) as f64 };
        // the project tempo does not change how fast or how high a recorded sound plays
        let warp_ratio = 1.0f64;
        let start = if offset_ticks > 0 {
            let rate = self.project.audio_pool.get(pool).and_then(|r| r.data.as_deref()).map_or(0.0, |d| d.rate as f64);
            offset_ticks as f64 / PPQ as f64 * 60.0 / ref_bpm * rate
        } else {
            0.0
        };
        let tr = self.project.arrangement.tracks.get(lane).map_or(0, |t| t.transpose) + fx.pitch as i32;
        let spt = 60.0 / (self.project.bpm.max(20.0) as f64 * PPQ as f64) * self.sr as f64;
        let rate = self.project.audio_pool.get(pool).and_then(|r| r.data.as_deref()).map_or(0.0, |d| d.rate as f64);
        let len_s = if fx.len > 0 { fx.len as f64 * spt } else { f64::MAX };
        // a reversed clip plays the stretch of the sound that the clip covers, from its end back to its start
        let (pos, rev) = if fx.reverse && fx.len > 0 {
            let secs = fx.len as f64 / PPQ as f64 * 60.0 / self.project.bpm.max(20.0) as f64;
            let end = (start + secs * rate).min(self.project.audio_pool.get(pool).and_then(|r| r.data.as_deref()).map_or(0.0, |d| d.mono.len() as f64 - 2.0));
            (end.max(start), true)
        } else {
            (start, false)
        };
        self.audio[idx] = AudioVoice { active: true, pool, lane, pos, end_abs, fade: 1.0, mul: 2f64.powf(tr as f64 / 12.0) * warp_ratio, gain: gain * self.project.arrangement.tracks.get(lane).map_or(1.0, |t| t.volume), rev, start_pos: start, elapsed: 0.0, len_s, fade_in_s: fx.fade_in as f64 * spt, fade_out_s: fx.fade_out as f64 * spt };
        &mut self.audio[idx]
    }

    fn fire(&mut self, bank: usize, lane: Option<usize>, pat: &Pattern, local: u32, abs: u64, limit: u64, swing: u32, inst: Option<Track>, transpose: i32) {
        if let Some(step) = step_at(local, swing) {
            if step < pat.length_steps as usize {
                for lane in 0..NUM_DRUMS {
                    let s = pat.drums[lane][step];
                    if s.on && crate::sequencer::step_passes(s.prob, abs / pat.len_ticks().max(1) as u64, step, lane) {
                        self.banks[bank].drums[lane].trigger_ex(s.vel as f32 / 127.0, s.pitch as i32, s.slide);
                        self.step_pan[lane] = s.pan as f32 / 100.0;
                    }
                }
            }
        }
        let pat_remaining = (pat.len_ticks() - local) as u64;
        // the level of the Playlist track this pattern is played on
        let lane_vol = self.clip_fade * lane.and_then(|l| self.project.arrangement.tracks.get(l)).map_or(1.0, |t| t.volume) * lane.map_or(1.0, |l| self.lane_auto.get(l).copied().unwrap_or(1.0));
        for n in pat.notes.iter().filter(|n| n.start == local && !n.mute) {
            if lane.and_then(|l| self.project.arrangement.tracks.get(l)).map_or(false, |t| t.sidechain_src) {
                self.duck_env = 1.0;
            }
            if n.chance < 100 {
                // deterministic per pass through the pattern: a different roll on every loop
                let pass = abs / pat.len_ticks().max(1) as u64;
                let h = (pass.wrapping_mul(0x9E37_79B9) ^ (n.start as u64).wrapping_mul(40503) ^ (n.pitch as u64).wrapping_mul(2654435761)) >> 5;
                if (h % 100) as u8 >= n.chance {
                    continue;
                }
            }
            let len = (n.len as u64).min(pat_remaining).min(limit);
            let t = inst.unwrap_or(n.track);
            // the pan of a note moves the pan of its instrument channel (the latest note wins)
            let pan_ch = match t {
                Track::Synth => Some(crate::mixer::CH_SYNTH),
                Track::Bass808 | Track::Sampler => Some(crate::mixer::CH_BASS),
                Track::Wavetable => Some(crate::mixer::CH_WT),
                Track::Analog => Some(crate::mixer::CH_AN),
                Track::Drums => None,
            };
            if let Some(ch) = pan_ch {
                self.step_pan[ch] = n.pan as f32 / 100.0;
            }
            let pitch = if t == Track::Drums { n.pitch } else { (n.pitch as i32 + transpose).clamp(0, 127) as u8 };
            if t == Track::Sampler {
                if let Some(l) = lane.filter(|&l| l < MAX_LANES && self.lane_has_sample(l)) {
                    self.banks[bank].lanes[l].note_on(pitch, n.vel as f32 / 127.0 * lane_vol, abs + len);
                    continue;
                }
            }
            // a slide note glides the sounding voice over its own length (seconds, sample rate)
            let slide = if n.slide { Some((len as f32 / PPQ as f32 * 60.0 / self.project.bpm.max(20.0), self.sr)) } else { None };
            if let Some(l) = lane.filter(|&l| self.lane_own(l, t)) {
                self.lane_syn[l].note_on(t, pitch, (n.vel as f32 / 127.0 * lane_vol).min(1.0), slide, abs + len);
                continue;
            }
            self.banks[bank].note_on(t, pitch, (n.vel as f32 / 127.0 * lane_vol).min(1.0), slide, abs + len);
        }
    }

    /// Render interleaved stereo into `out`.
    fn update_fx_automation(proj: &Project, tick: u32, owners: &[Option<usize>; NUM_INSERTS],
        values: &mut [[[f32; crate::fx::MAX_PARAMS]; 10]; NUM_INSERTS])
    {
        for (insert, owner) in owners.iter().enumerate() {
            let Some(lane) = owner else { continue; };
            let Some(slots) = proj.insert_fx.get(insert) else { continue; };
            let values = &mut values[insert];
            values.fill([f32::NAN; crate::fx::MAX_PARAMS]);
            for clip in &proj.arrangement.tracks[*lane].fx_automation {
                if clip.effect == 0 || clip.param >= crate::fx::MAX_PARAMS {
                    continue;
                }
                let Some(value) = clip.value_at(tick) else { continue; };
                if let Some(slot) = slots.iter().take(values.len()).position(|slot| !slot.empty && slot.automation_key == clip.effect) {
                    values[slot][clip.param] = value;
                }
            }
        }
    }

    pub fn render(&mut self, out: &mut [f32]) {
        let proj = Arc::clone(&self.project);
        let sr = self.sr;
        let gains = effective_gains(&proj.channels);
        let mut pan_l = [0.0f32; NUM_CH];
        let mut pan_r = [0.0f32; NUM_CH];
        for i in 0..NUM_CH {
            let sp = self.step_pan[i];
            let a = ((proj.channels[i].pan + sp).clamp(-1.0, 1.0) + 1.0) * std::f32::consts::FRAC_PI_4;
            pan_l[i] = a.cos();
            pan_r[i] = a.sin();
        }
        // where each instrument channel goes: its own pan law, then master or an insert
        let mut route = [None::<usize>; NUM_CH];
        for c in 0..NUM_CH {
            route[c] = proj.channels[c].output.map(|o| o.min(NUM_INSERTS - 1));
        }
        let spt = 60.0 / (proj.bpm.max(20.0) as f64 * PPQ as f64) * sr as f64;
        let delay_samples = proj.delay_beats * 60.0 / proj.bpm.max(20.0) * sr;
        let master_target = proj.master_volume * self.master_auto;
        let fx = proj.master_fx.clone();
        for ch in 0..2 {
            self.eq_lo[ch].low_shelf(110.0, fx.low_db, sr);
            self.eq_hi[ch].high_shelf(7500.0, fx.high_db, sr);
        }
        let bpm = proj.bpm.max(20.0);
        for i in 0..NUM_INSERTS {
            let slots: &[crate::fx::FxSlot] = proj.insert_fx.get(i).map(|v| v.as_slice()).unwrap_or(&[]);
            self.fx_run[i].sync(slots, sr);
        }
        self.master_run.sync(&proj.master_chain, sr);
        let mut owners = [None; NUM_INSERTS];
        if proj.song_mode && self.playing {
            for (lane, _) in proj.arrangement.tracks.iter().enumerate().filter(|(_, track)| !track.fx_automation.is_empty()) {
                if let Some(insert) = proj.track_automation_insert(lane) {
                    owners[insert] = Some(lane);
                }
            }
        }
        let mut values = [[[f32::NAN; crate::fx::MAX_PARAMS]; 10]; NUM_INSERTS];
        let automated = owners.iter().any(Option::is_some);
        if automated {
            Self::update_fx_automation(&proj, self.position, &owners, &mut values);
        }
        for frame in out.chunks_exact_mut(2) {
            if self.playing {
                if self.countdown <= 0.0 {
                    self.on_tick();
                    if automated && self.playing {
                        Self::update_fx_automation(&proj, self.position, &owners, &mut values);
                    }
                    self.countdown += spt;
                }
                self.countdown -= 1.0;
            }
            let mut raw = [0.0f32; NUM_CH];
            {
                let b = &mut self.banks[0];
                for lane in 0..NUM_DRUMS {
                    let sample = proj.drum_samples[lane].as_ref().and_then(|r| r.data.as_deref());
                    raw[lane] = b.drums[lane].render(lane, sample, sr, &mut self.rng);
                }
                raw[crate::mixer::CH_SYNTH] = b.synth.render_sample(&proj.synth, sr);
                let mut lane_snd = 0.0;
                for (l, tr) in proj.arrangement.tracks.iter().enumerate().take(MAX_LANES) {
                    lane_snd += b.lanes[l].render_sample(&tr.sampler, sr);
                }
                raw[crate::mixer::CH_BASS] = b.bass.render_sample(&proj.bass, sr) + b.sampler.render_sample(&proj.sampler, sr) + lane_snd;
                raw[crate::mixer::CH_WT] = b.wt.render_sample(&proj.wt, sr);
                raw[crate::mixer::CH_AN] = b.analog.render_sample(&proj.analog, sr);
            }
            // tracks with their own synth settings: summed into the channel of their synth (or their insert)
            let mut lane_ins = [0.0f32; NUM_INSERTS];
            for (l, tr) in proj.arrangement.tracks.iter().enumerate().take(MAX_LANES) {
                if !tr.snd.own {
                    continue;
                }
                let ls = &mut self.lane_syn[l];
                let (s, ch) = match tr.instrument {
                    Some(Track::Synth) => (ls.synth.render_sample(&tr.snd.synth, sr), crate::mixer::CH_SYNTH),
                    Some(Track::Bass808) => (ls.bass.render_sample(&tr.snd.bass, sr), crate::mixer::CH_BASS),
                    Some(Track::Wavetable) => (ls.wt.render_sample(&tr.snd.wt, sr), crate::mixer::CH_WT),
                    Some(Track::Analog) => (ls.analog.render_sample(&tr.snd.analog, sr), crate::mixer::CH_AN),
                    _ => continue,
                };
                if s == 0.0 {
                    continue;
                }
                match tr.output {
                    None => raw[ch] += s,
                    Some(o) => lane_ins[o.min(NUM_INSERTS - 1)] += s * proj.channels[ch].volume,
                }
            }
            // sounds that bypass the instrument channels and play through an insert (mono, centred)
            let mut ins_mono = [0.0f32; NUM_INSERTS];
            for i in 0..NUM_INSERTS {
                let b = &mut self.banks[1 + i];
                let mut m = 0.0;
                for lane in 0..NUM_DRUMS {
                    let sample = proj.drum_samples[lane].as_ref().and_then(|r| r.data.as_deref());
                    m += b.drums[lane].render(lane, sample, sr, &mut self.rng) * proj.channels[lane].volume;
                }
                m += b.synth.render_sample(&proj.synth, sr) * proj.channels[crate::mixer::CH_SYNTH].volume;
                let mut lane_snd = 0.0;
                for (l, tr) in proj.arrangement.tracks.iter().enumerate().take(MAX_LANES) {
                    lane_snd += b.lanes[l].render_sample(&tr.sampler, sr);
                }
                m += (b.bass.render_sample(&proj.bass, sr) + b.sampler.render_sample(&proj.sampler, sr) + lane_snd) * proj.channels[crate::mixer::CH_BASS].volume;
                m += b.wt.render_sample(&proj.wt, sr) * proj.channels[crate::mixer::CH_WT].volume;
                m += b.analog.render_sample(&proj.analog, sr) * proj.channels[crate::mixer::CH_AN].volume;
                ins_mono[i] = m + lane_ins[i];
            }
            let mut au = 0.0;
            let mut rendered_mix = 0.0f32;
            for v in self.audio.iter_mut() {
                if !v.active || self.paused {
                    continue;
                }
                let Some(sd) = proj.audio_pool.get(v.pool).and_then(|r| r.data.as_deref()) else {
                    v.active = false;
                    continue;
                };
                let i = v.pos as usize;
                if i + 1 >= sd.mono.len() || (v.rev && v.pos <= v.start_pos) {
                    v.active = false;
                    continue;
                }
                let fr = (v.pos - i as f64) as f32;
                // Ableton-style clip fades
                let mut cg = 1.0f32;
                if v.fade_in_s > 0.0 {
                    cg = cg.min((v.elapsed / v.fade_in_s).min(1.0) as f32);
                }
                if v.fade_out_s > 0.0 && v.len_s < f64::MAX {
                    cg = cg.min(((v.len_s - v.elapsed) / v.fade_out_s).clamp(0.0, 1.0) as f32);
                }
                v.elapsed += 1.0;
                let x = (sd.mono[i] * (1.0 - fr) + sd.mono[i + 1] * fr) * v.fade * v.gain * cg * self.lane_auto.get(v.lane).copied().unwrap_or(1.0);
                let track = proj.arrangement.tracks.get(v.lane);
                match track.and_then(|track| track.output) {
                    Some(o) => ins_mono[o.min(NUM_INSERTS - 1)] += x * proj.channels[crate::mixer::CH_AUDIO].volume,
                    None if track.is_some_and(|track| track.rendered_mix) => rendered_mix += x,
                    None => au += x,
                }
                let step = sd.rate as f64 / sr as f64 * if v.mul > 0.0 { v.mul } else { 1.0 };
                if v.rev {
                    v.pos -= step;
                } else {
                    v.pos += step;
                }
                if v.fade < 1.0 {
                    v.fade -= 1.0 / (0.006 * sr);
                    if v.fade <= 0.0 {
                        v.active = false;
                    }
                }
            }
            raw[crate::mixer::CH_AUDIO] = au;
            let (mut ml, mut mr, mut sl, mut sr_) = (rendered_mix, rendered_mix, 0.0f32, 0.0f32);
            // the envelope falls back in about 0.22 s
            self.duck_env *= 1.0 - 4.5 / sr;
            let mut ins_l = [0.0f32; NUM_INSERTS];
            let mut ins_r = [0.0f32; NUM_INSERTS];
            for i in 0..NUM_INSERTS {
                ins_l[i] = ins_mono[i] * std::f32::consts::FRAC_1_SQRT_2;
                ins_r[i] = ins_mono[i] * std::f32::consts::FRAC_1_SQRT_2;
            }
            for c in 0..NUM_CH {
                self.gain_s[c] += (gains[c] - self.gain_s[c]) * 0.003;
                let s = raw[c] * self.gain_s[c];
                let (l, r) = (s * pan_l[c], s * pan_r[c]);
                match route[c] {
                    Some(i) => {
                        ins_l[i] += l;
                        ins_r[i] += r;
                    }
                    None => {
                        ml += l;
                        mr += r;
                    }
                }
                sl += l * proj.channels[c].delay_send;
                sr_ += r * proj.channels[c].delay_send;
                self.peaks[c][0] = self.peaks[c][0].max(l.abs());
                self.peaks[c][1] = self.peaks[c][1].max(r.abs());
            }
            // insert effect chains
            for i in 0..NUM_INSERTS {
                if let Some(slots) = proj.insert_fx.get(i) {
                    if !slots.is_empty() {
                        let overrides: &[_] = if self.playing && owners[i].is_some() { &values[i] } else { &[] };
                        let (a, b) = self.fx_run[i].process_automated(slots, overrides, ins_l[i], ins_r[i], bpm, sr);
                        ins_l[i] = a;
                        ins_r[i] = b;
                    }
                }
            }
            // insert strips: volume, balance, delay send -> master
            for i in 0..NUM_INSERTS {
                let c = NUM_CH + i;
                self.gain_s[c] += (gains[c] - self.gain_s[c]) * 0.003;
                let pan = proj.channels[c].pan.clamp(-1.0, 1.0);
                // sidechain: this insert ducks while the envelope of the source tracks is up
                let duck = 1.0 - proj.duck.get(i).copied().unwrap_or(0.0) * self.duck_env;
                let l = ins_l[i] * self.gain_s[c] * (1.0 - pan.max(0.0)) * duck;
                let r = ins_r[i] * self.gain_s[c] * (1.0 + pan.min(0.0)) * duck;
                ml += l;
                mr += r;
                sl += l * proj.channels[c].delay_send;
                sr_ += r * proj.channels[c].delay_send;
                self.peaks[c][0] = self.peaks[c][0].max(l.abs());
                self.peaks[c][1] = self.peaks[c][1].max(r.abs());
            }
            if let Some((s, pos, active)) = self.preview.as_mut() {
                if *active {
                    let i = *pos as usize;
                    if i + 1 < s.mono.len() {
                        let v = s.mono[i] * 0.8 * self.preview_gain;
                        ml += v;
                        mr += v;
                        *pos += s.rate as f64 / sr as f64;
                    } else {
                        *active = false;
                    }
                }
            }
            if self.click_env > 0.001 {
                let v = (self.click_ph * std::f32::consts::TAU).sin() * self.click_env * 0.35;
                ml += v;
                mr += v;
                self.click_ph = (self.click_ph + self.click_hz / sr).fract();
                self.click_env *= 1.0 - 90.0 / sr;
            }
            let (dl, dr) = self.delay.process(sl, sr_, delay_samples, proj.delay_feedback);
            self.master_s += (proj.master_volume * self.master_auto - self.master_s) * 0.003;
            let l = (ml + dl * proj.delay_return) * self.master_s;
            let r = (mr + dr * proj.delay_return) * self.master_s;
            let (mut l, mut r) = (l, r);
            if fx.low_db != 0.0 || fx.high_db != 0.0 {
                l = self.eq_hi[0].process(self.eq_lo[0].process(l));
                r = self.eq_hi[1].process(self.eq_lo[1].process(r));
            }
            if fx.comp_on {
                let (cl, cr, gr) = self.comp.process(l, r, fx.threshold_db, fx.ratio, fx.makeup_db, sr);
                l = cl;
                r = cr;
                self.gr_db = self.gr_db.max(gr);
            }
            let (l, r) = if proj.master_chain.is_empty() { (l, r) } else { self.master_run.process(&proj.master_chain, l, r, bpm, sr) };
            let (l, r) = self.limiter.process(l, r, sr);
            let l = if l.is_finite() { l } else { 0.0 };
            let r = if r.is_finite() { r } else { 0.0 };
            self.peaks[NUM_STRIPS][0] = self.peaks[NUM_STRIPS][0].max(l.abs());
            self.peaks[NUM_STRIPS][1] = self.peaks[NUM_STRIPS][1].max(r.abs());
            frame[0] = l;
            frame[1] = r;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn song_end_stops_unless_loop_is_enabled() {
        for loop_on in [false, true] {
            let mut project = Project::demo();
            project.song_mode = true;
            project.loop_on = loop_on;
            project.loop_start = 1;
            project.loop_bars = 2;
            let end = if loop_on { 3 * TICKS_PER_BAR } else { project.arrangement.song_end_bar().max(1) * TICKS_PER_BAR };
            let mut engine = Engine::new(48000.0);
            engine.set_project(Arc::new(project));
            engine.apply(Cmd::PlayFrom(end - 1));
            engine.on_tick();
            assert_eq!(engine.playing, loop_on);
            assert_eq!(engine.song_tick, if loop_on { TICKS_PER_BAR } else { 0 });
            if !loop_on {
                engine.apply(Cmd::Play);
                assert_eq!(engine.song_tick, 0);
            }
        }
    }

    #[test]
    fn pause_keeps_the_place_and_stop_rewinds() {
        let mut e = Engine::new(48000.0);
        e.set_project(Arc::new(Project::demo()));
        e.play();
        let mut buf = vec![0.0f32; 48000 * 2];
        e.render(&mut buf);
        let at = e.song_tick;
        assert!(at > 0);
        e.apply(Cmd::Pause);
        assert!(!e.playing);
        e.render(&mut buf);
        assert_eq!(e.song_tick, at, "a pause does not move the song");
        e.apply(Cmd::Play);
        assert!(e.playing && e.song_tick == at, "play resumes from the same place");
        e.apply(Cmd::PlayFrom(0));
        assert_eq!(e.position, 0, "rewind updates the visible position immediately");
        assert_eq!(e.song_tick, 0);
        e.apply(Cmd::Stop);
        e.apply(Cmd::Play);
        e.render(&mut buf[..2]);
        assert!(e.song_tick < at, "stop rewinds");
    }

    #[test]
    fn pause_preserves_audio_clip_position_for_resume() {
        let mut project = Project::default();
        project.song_mode = true;
        project.audio_pool.push(crate::project::SampleRef {
            name: "pause test".into(), path: "pause-test.wav".into(),
            data: Some(Arc::new(crate::project::SampleData { rate: 48000, mono: vec![0.25; 48000] })),
        });
        let mut engine = Engine::new(48000.0);
        engine.set_project(Arc::new(project));
        engine.play();
        engine.start_audio(0, 0, u64::MAX, 0, 1.0, ClipFx::default());
        let mut out = [0.0; 512];
        engine.render(&mut out);
        let at = engine.audio[0].pos;
        assert!(at > 0.0);
        engine.apply(Cmd::Pause);
        engine.render(&mut out);
        assert!(engine.audio[0].active);
        assert_eq!(engine.audio[0].pos, at);
        engine.apply(Cmd::Play);
        engine.render(&mut out);
        assert!(engine.audio[0].pos > at);
        engine.apply(Cmd::Preview(Arc::new(crate::project::SampleData { rate: 48000, mono: vec![0.25; 48000] })));
        engine.apply(Cmd::Stop);
        assert!(engine.preview.is_none(), "Stop also silences the browser preview");
        for _ in 0..4 { engine.render(&mut out); }
        assert!(!engine.audio[0].active, "Stop must silence the audio without another tick");
        assert_eq!(engine.song_tick, 0);
    }

    #[test]
    fn default_project_makes_sound() {
        let mut e = Engine::new(48000.0);
        e.set_project(Arc::new(Project::demo()));
        e.play();
        let mut buf = vec![0.0f32; 48000 * 2 * 4];
        for chunk in buf.chunks_mut(512) {
            e.render(chunk);
        }
        let peak = buf.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(buf.iter().all(|v| v.is_finite()));
        assert!(peak > 0.1 && peak <= 1.0, "peak {peak}");
        let peaks = e.take_peaks();
        for c in 0..7 {
            assert!(peaks[c][0] + peaks[c][1] > 0.0, "channel {c} silent");
        }
    }

    #[test]
    fn sound_preview_plays_while_stopped_and_reports_progress() {
        let mut e = Engine::new(48000.0);
        e.set_project(Arc::new(Project::default()));
        let mono: Vec<f32> = (0..48000).map(|i| (i as f32 * 0.05).sin() * 0.8).collect();
        let data = Arc::new(crate::project::SampleData { rate: 48000, mono });
        assert!(e.preview_progress().is_none());
        e.apply(Cmd::Preview(data.clone()));
        let mut buf = vec![0.0f32; 4800 * 2];
        e.render(&mut buf);
        let peak = buf.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(peak > 0.2, "the preview is silent: {peak}");
        let p = e.preview_progress().expect("preview should be running");
        assert!(p > 0.05 && p < 0.2, "progress {p}");
        e.apply(Cmd::PreviewStop);
        assert!(e.preview_progress().is_none());
    }

    #[test]
    fn play_from_jumps_into_the_song_and_the_whole_song_plays_without_loop() {
        let mut proj = crate::templates::dark_trap();
        proj.song_mode = true;
        proj.loop_on = false;
        let end = proj.arrangement.song_end_bar();
        assert!(end > 16, "the template arrangement should be longer than the old 8 bar loop: {end}");
        let mut e = Engine::new(48000.0);
        e.set_project(Arc::new(proj));
        e.apply(Cmd::PlayFrom(12 * TICKS_PER_BAR));
        assert!(e.playing);
        let mut buf = vec![0.0f32; 48000 * 2];
        e.render(&mut buf);
        assert!(e.position >= 12 * TICKS_PER_BAR, "position {}", e.position);
        assert!(buf.iter().fold(0.0f32, |m, v| m.max(v.abs())) > 0.05, "silent after the jump");
    }

    #[test]
    fn session_clip_launches_on_bar() {
        let mut e = Engine::new(48000.0);
        e.set_project(Arc::new(Project::demo()));
        e.apply(Cmd::LaunchClip { lane: 0, pattern: 0, audio: false });
        let mut buf = vec![0.0f32; 48000 * 2 * 2];
        for chunk in buf.chunks_mut(512) {
            e.render(chunk);
        }
        assert!(e.playing);
        assert!(buf.iter().fold(0.0f32, |m, v| m.max(v.abs())) > 0.1, "session clip silent");
        assert_eq!(e.session_codes(0).0, code(0, false));
        e.apply(Cmd::StopLane { lane: 0 });
        assert_eq!(e.session_codes(0).1, QUEUED_STOP);
        e.stop();
        assert_eq!(e.session_codes(0), (0, 0));
    }

    #[test]
    fn slide_notes_glide_the_sounding_voice_on_every_synth() {
        use crate::arrangement::Clip;
        use crate::piano_roll::Note;
        // zero-crossing pitch (Hz) of the left channel between two times
        let freq = |buf: &[f32], t0: f32, t1: f32| {
            let (a, b) = ((t0 * 48000.0) as usize, (t1 * 48000.0) as usize);
            let mut z = 0;
            for i in a + 1..b {
                if (buf[(i - 1) * 2] < 0.0) != (buf[i * 2] < 0.0) {
                    z += 1;
                }
            }
            z as f32 / 2.0 / (t1 - t0)
        };
        for t in [Track::Synth, Track::Wavetable, Track::Analog] {
            let mut p = Project::default();
            let mut pat = Pattern::new("s", 16);
            pat.notes.push(Note { start: 0, len: 384, pitch: 57, vel: 110, track: t, slide: false, mute: false, pan: 0, chance: 100 });
            // 220 Hz, then a slide note an octave up at beat 2 that lasts one beat: ends around 1.29 s at 140 bpm
            pat.notes.push(Note { start: 192, len: 96, pitch: 69, vel: 110, track: t, slide: true, mute: false, pan: 0, chance: 100 });
            p.patterns = vec![pat];
            p.arrangement.tracks = crate::arrangement::Arrangement::default_tracks();
            p.arrangement.clips = vec![Clip::bars(0, 0, 0, 1, false)];
            p.song_mode = true;
            p.bpm = 140.0;
            let (buf, _) = render_secs(p, 1.6);
            let (before, after) = (freq(&buf, 0.5, 0.8), freq(&buf, 1.15, 1.27));
            assert!(before > 100.0, "{t:?}: sounds before the slide, got {before}");
            assert!(after / before > 1.6 && after / before < 2.4, "{t:?}: glided an octave up: {before} -> {after}");
        }
    }

    #[test]
    fn track_instrument_overrides_note_instrument() {
        use crate::arrangement::Clip;
        use crate::piano_roll::Note;
        let mut p = Project::default();
        let mut pat = Pattern::new("x", 16);
        pat.notes.push(Note { start: 0, len: 96, pitch: 60, vel: 110, track: Track::Synth, slide: false, mute: false, pan: 0, chance: 100 });
        p.patterns = vec![pat];
        p.arrangement.tracks = crate::arrangement::Arrangement::default_tracks();
        p.arrangement.tracks[0].instrument = Some(Track::Wavetable);
        p.arrangement.clips = vec![Clip::bars(0, 0, 0, 1, false)];
        p.song_mode = true;
        let (_, peaks) = render_secs(p, 1.0);
        assert!(peaks[crate::mixer::CH_WT][0] > 0.01, "wavetable silent");
        assert_eq!(peaks[crate::mixer::CH_SYNTH][0], 0.0, "synth should not play");
    }

    #[test]
    fn two_tracks_with_the_same_synth_have_independent_settings() {
        use crate::arrangement::Clip;
        use crate::piano_roll::Note;
        let make = |lane: usize| {
            let mut p = Project::default();
            let mut pat = Pattern::new("x", 16);
            pat.notes.push(Note { start: 0, len: 96, pitch: 60, vel: 110, track: Track::Synth, slide: false, mute: false, pan: 0, chance: 100 });
            p.patterns = vec![pat];
            p.arrangement.tracks = crate::arrangement::Arrangement::default_tracks();
            p.arrangement.tracks[0].instrument = Some(Track::Wavetable);
            p.arrangement.tracks[1].instrument = Some(Track::Wavetable);
            p.adopt_track_params();
            // track 0 is turned all the way down, track 1 keeps the default sound
            p.arrangement.tracks[0].snd.wt.volume = 0.0;
            p.arrangement.clips = vec![Clip::bars(0, lane, 0, 1, false)];
            p.song_mode = true;
            p
        };
        let (_, quiet) = render_secs(make(0), 1.0);
        let (_, loud) = render_secs(make(1), 1.0);
        assert!(quiet[crate::mixer::CH_WT][0] < 0.0005, "track 0 should be silent: {}", quiet[crate::mixer::CH_WT][0]);
        assert!(loud[crate::mixer::CH_WT][0] > 0.01, "track 1 should play: {}", loud[crate::mixer::CH_WT][0]);
    }

    #[test]
    fn piano_roll_on_a_clip_plays_the_other_tracks_with_it() {
        use crate::arrangement::Clip;
        use crate::piano_roll::Note;
        let note = |t| Note { start: 0, len: 96, pitch: 60, vel: 110, track: t, slide: false, mute: false, pan: 0, chance: 100 };
        let mut a = Pattern::new("a", 16);
        a.notes.push(note(Track::Wavetable));
        a.clip_only = true;
        let mut b = Pattern::new("b", 16);
        b.notes.push(note(Track::Analog));
        b.clip_only = true;
        let mut p = Project::default();
        p.patterns = vec![a, b];
        p.arrangement.tracks = crate::arrangement::Arrangement::default_tracks();
        p.arrangement.tracks[0].instrument = Some(Track::Wavetable);
        p.arrangement.tracks[1].instrument = Some(Track::Analog);
        p.arrangement.clips = vec![Clip::bars(0, 0, 0, 1, false), Clip::bars(1, 1, 0, 1, false)];
        // pattern mode with the first clip open in the piano roll
        p.song_mode = false;
        p.current_pattern = 0;
        let (_, peaks) = render_secs(p, 1.0);
        assert!(peaks[crate::mixer::CH_WT][0] > 0.01, "the open clip is silent");
        assert!(peaks[crate::mixer::CH_AN][0] > 0.01, "the other track does not play along");
    }

    /// One MIDI track (wavetable) with a one bar clip, routed to insert `ins`.
    fn routed_project(ins: Option<usize>) -> Project {
        use crate::arrangement::Clip;
        use crate::piano_roll::Note;
        let mut pat = Pattern::new("x", 16);
        pat.notes.push(Note { start: 0, len: 192, pitch: 60, vel: 110, track: Track::Wavetable, slide: false, mute: false, pan: 0, chance: 100 });
        pat.clip_only = true;
        let mut p = Project::default();
        p.patterns = vec![pat];
        p.arrangement.tracks = crate::arrangement::Arrangement::default_tracks();
        p.arrangement.tracks[0].instrument = Some(Track::Wavetable);
        p.arrangement.tracks[0].output = ins;
        p.adopt_track_params();
        p.arrangement.clips = vec![Clip::bars(0, 0, 0, 1, false)];
        p.song_mode = true;
        p
    }

    fn peak_of(p: Project) -> f32 {
        let (buf, _) = render_secs(p, 1.0);
        buf.iter().fold(0.0f32, |m, v| m.max(v.abs()))
    }

    #[test]
    fn mixer_mute_solo_volume_and_master_really_change_the_sound() {
        use crate::mixer::NUM_CH;
        let base = peak_of(routed_project(Some(0)));
        assert!(base > 0.01, "the routed track is silent: {base}");
        // insert volume
        let mut p = routed_project(Some(0));
        p.channels[NUM_CH].volume *= 0.25;
        let quiet = peak_of(p);
        assert!(quiet < base * 0.6, "insert volume has no effect: {quiet} vs {base}");
        // insert mute
        let mut p = routed_project(Some(0));
        p.channels[NUM_CH].mute = true;
        assert!(peak_of(p) < 0.0005, "a muted insert still sounds");
        // another insert soloed: this one is silent
        let mut p = routed_project(Some(0));
        p.channels[NUM_CH + 1].solo = true;
        assert!(peak_of(p) < 0.0005, "solo on another insert does not silence this one");
        // soloing the insert itself keeps it
        let mut p = routed_project(Some(0));
        p.channels[NUM_CH].solo = true;
        assert!(peak_of(p) > 0.01, "the soloed insert is silent");
        // master volume
        let mut p = routed_project(Some(0));
        p.master_volume = 0.0;
        assert!(peak_of(p) < 0.0005, "master at zero is not silent");
        // a track routed to another insert is not heard through this insert's mute
        let mut p = routed_project(Some(1));
        p.channels[NUM_CH].mute = true;
        assert!(peak_of(p) > 0.01, "muting insert 1 silenced a track routed to insert 2");
    }

    #[test]
    fn insert_pan_moves_the_sound_between_the_speakers() {
        use crate::mixer::NUM_CH;
        let peaks = |pan: f32| {
            let mut p = routed_project(Some(0));
            p.channels[NUM_CH].pan = pan;
            let (buf, _) = render_secs(p, 1.0);
            let l = buf.iter().step_by(2).fold(0.0f32, |m, v| m.max(v.abs()));
            let r = buf.iter().skip(1).step_by(2).fold(0.0f32, |m, v| m.max(v.abs()));
            (l, r)
        };
        let (l, r) = peaks(1.0);
        assert!(r > 0.01 && l < r * 0.1, "pan right: left {l} right {r}");
        let (l, r) = peaks(-1.0);
        assert!(l > 0.01 && r < l * 0.1, "pan left: left {l} right {r}");
    }

    #[test]
    fn track_mute_and_solo_work_in_the_playlist() {
        let mut p = routed_project(None);
        p.arrangement.tracks[0].mute = true;
        assert!(peak_of(p) < 0.0005, "a muted track still sounds");
        let mut p = routed_project(None);
        p.arrangement.tracks[1].solo = true;
        assert!(peak_of(p) < 0.0005, "soloing another track does not silence this one");
        let mut p = routed_project(None);
        p.arrangement.tracks[0].solo = true;
        assert!(peak_of(p) > 0.01, "the soloed track is silent");
    }

    /// what the "+ Add effect" button of the Track Effects page does: the track goes to a free insert that gets the plugin.
    #[test]
    fn adding_a_track_effect_keeps_the_track_audible() {
        use crate::fx::{FxKind, FxSlot};
        let builders: [(&str, fn() -> Project); 4] = [("phonk_house", crate::templates::phonk_house), ("mafia_808", crate::templates::mafia_808), ("uk_drill", crate::templates::uk_drill), ("bbt_phonk", crate::templates::bbt_phonk)];
        for (name, build) in builders {
            let dry_p = build();
            let dry = peak_of(dry_p.clone());
            assert!(dry > 0.01, "{name} is silent without effects");
            for lane in 0..dry_p.arrangement.tracks.len().min(6) {
                let mut p = build();
                let used: Vec<usize> = p.arrangement.tracks.iter().filter_map(|t| t.output).collect();
                let Some(i) = (0..crate::mixer::NUM_INSERTS).find(|i| !used.contains(i) && p.insert_fx.get(*i).map_or(true, |c| c.iter().all(|s| s.empty))) else { continue };
                p.insert_fx.resize_with(crate::mixer::NUM_INSERTS, Vec::new);
                p.arrangement.tracks[lane].output = Some(i);
                p.insert_fx[i].push(FxSlot::new(FxKind::Reverb));
                let wet = peak_of(p);
                assert!(wet > dry * 0.2, "{name}: adding an effect to track {lane} made the song nearly silent ({wet} vs {dry})");
            }
        }
    }

    /// the effect is added WHILE the song plays (the project is swapped under the running engine): every plugin keeps the track audible
    #[test]
    fn adding_an_effect_while_playing_keeps_the_sound() {
        use crate::fx::FxSlot;
        let base = crate::templates::phonk_house();
        for kind in crate::fx::ALL {
            let mut e = Engine::new(48000.0);
            e.set_project(Arc::new(base.clone()));
            e.play();
            let mut buf = vec![0.0f32; 48000 * 2];
            for chunk in buf.chunks_mut(512) { e.render(chunk); }
            let before = buf.iter().fold(0.0f32, |m, v| m.max(v.abs()));
            let mut p = base.clone();
            let used: Vec<usize> = p.arrangement.tracks.iter().filter_map(|t| t.output).collect();
            let i = (0..crate::mixer::NUM_INSERTS).find(|i| !used.contains(i) && p.insert_fx.get(*i).map_or(true, |c| c.iter().all(|s| s.empty))).unwrap_or(7);
            p.insert_fx.resize_with(crate::mixer::NUM_INSERTS, Vec::new);
            p.arrangement.tracks[0].output = Some(i);
            p.insert_fx[i].push(FxSlot::new(kind));
            e.set_project(Arc::new(p));
            let mut after = 0.0f32;
            let mut finite = true;
            for _ in 0..200 {
                let mut chunk = vec![0.0f32; 1024];
                e.render(&mut chunk);
                finite &= chunk.iter().all(|v| v.is_finite());
                after = after.max(chunk.iter().fold(0.0f32, |m, v| m.max(v.abs())));
            }
            assert!(finite, "{kind:?}: non-finite output after adding it live");
            assert!(after > before * 0.2, "{kind:?}: the sound vanished after adding it live ({after} vs {before})");
        }
    }

    #[test]
    fn insert_effects_change_the_sound() {
        use crate::fx::{FxKind, FxSlot};
        let render_with = |kind: Option<FxKind>| {
            let mut p = routed_project(Some(0));
            if let Some(k) = kind {
                p.insert_fx.resize_with(crate::mixer::NUM_INSERTS, Vec::new);
                let mut slot = FxSlot::new(k);
                // the default EQ is flat on purpose: turn its first band all the way up to hear a change
                if k == FxKind::Eq3 {
                    slot.p[0] = k.specs()[0].2;
                }
                // flat / threshold-based plugins need a setting that actually acts on this quiet test signal
                if k == FxKind::ParaEq {
                    slot.p[1] = 12.0;
                    slot.p[3] = 12.0;
                }
                if k == FxKind::Clipper {
                    slot.p[0] = -48.0;
                    slot.p[1] = 0.0;
                }
                p.insert_fx[0].push(slot);
            }
            render_secs(p, 1.0).0
        };
        let dry = render_with(None);
        for kind in crate::fx::ALL {
            if matches!(kind, FxKind::Gate | FxKind::Utility | FxKind::ViGross | FxKind::Widener | FxKind::Limiter) { continue; } // neutral at defaults: covered by fx::tests
            let wet = render_with(Some(kind));
            assert!(wet.iter().all(|v| v.is_finite()), "{kind:?} produced a non-finite sample");
            let diff: f32 = dry.iter().zip(wet.iter()).map(|(a, b)| (a - b).abs()).sum();
            assert!(diff > 0.5, "{kind:?} does not change the sound (difference {diff})");
        }
    }

    #[test]
    fn level_automation_changes_the_sound_while_the_song_plays() {
        use crate::arrangement::AutoPoint;
        let base = peak_of(routed_project(None));
        assert!(base > 0.01);
        // track level curve down to zero from the very first tick
        let mut p = routed_project(None);
        p.arrangement.tracks[0].auto_vol = vec![AutoPoint { tick: 0, v: 0.0, curve: 0.0 }];
        assert!(peak_of(p) < 0.0005, "a track automated to zero is still heard");
        // master automated to zero
        let mut p = routed_project(None);
        p.master_auto = vec![AutoPoint { tick: 0, v: 0.0, curve: 0.0 }];
        assert!(peak_of(p) < 0.0005, "the automated master is still heard");
        // interpolation
        let pts = [AutoPoint { tick: 0, v: 0.0, curve: 0.0 }, AutoPoint { tick: 100, v: 1.0, curve: 0.0 }];
        assert_eq!(crate::arrangement::auto_at(&pts, 50), Some(0.5));
        assert_eq!(crate::arrangement::auto_at(&pts, 500), Some(1.0));
        assert_eq!(crate::arrangement::auto_at(&[], 5), None);
        // a bent segment: the middle sits below / above the straight line, the ends stay
        let bent = [AutoPoint { tick: 0, v: 0.0, curve: 0.8 }, AutoPoint { tick: 100, v: 1.0, curve: 0.0 }];
        let mid = crate::arrangement::auto_at(&bent, 50).unwrap();
        assert!(mid < 0.35, "a positive bend starts slowly: {mid}");
        assert_eq!(crate::arrangement::auto_at(&bent, 100), Some(1.0));
        // dragging the middle handle to a value gives the curve that passes through it
        let c = crate::arrangement::curve_for_mid(0.0, 1.0, 0.2);
        let back = crate::arrangement::auto_at(&[AutoPoint { tick: 0, v: 0.0, curve: c }, AutoPoint { tick: 100, v: 1.0, curve: 0.0 }], 50).unwrap();
        assert!((back - 0.2).abs() < 0.03, "bend handle: {back}");
    }

    #[test]
    fn sidechain_ducking_pushes_an_insert_down_on_the_source_hits() {
        use crate::arrangement::Clip;
        use crate::piano_roll::Note;
        let make = |duck: f32| {
            // track 1: a long wavetable note on insert 0; track 0: a source whose hit is at the very start
            let mut p = routed_project(Some(0));
            let mut src = Pattern::new("s", 16);
            src.clip_only = true;
            src.notes.push(Note { start: 0, len: 24, pitch: 60, vel: 100, track: Track::Synth, slide: false, mute: false, pan: 0, chance: 100 });
            p.patterns.push(src);
            p.arrangement.tracks[1].instrument = Some(Track::Sampler); // no sample loaded: silent itself, it only triggers the ducking
            p.arrangement.tracks[1].blank = false;
            p.arrangement.tracks[1].sidechain_src = true;
            p.arrangement.clips.push(Clip::bars(1, 1, 0, 1, false));
            p.duck[0] = duck;
            p
        };
        // energy of the first 80 ms, right after the hit
        let early = |p: Project| {
            let (buf, _) = render_secs(p, 0.5);
            buf[..(48000.0 * 0.08) as usize * 2].iter().map(|v| v * v).sum::<f32>()
        };
        let (loud, ducked) = (early(make(0.0)), early(make(1.0)));
        assert!(ducked < loud * 0.5, "ducking has no effect: {ducked} vs {loud}");
    }

    /// One audio track with a one bar clip of a sound that is loud at its start and quiet at its end.
    fn audio_clip_project(edit: impl Fn(&mut crate::arrangement::Clip)) -> Project {
        use crate::arrangement::{Arrangement, Clip};
        use crate::project::{SampleData, SampleRef};
        let n = 48000usize;
        // a decaying tone: strong first, weak last
        let mono: Vec<f32> = (0..n).map(|i| (i as f32 * 0.05).sin() * 0.9 * (1.0 - i as f32 / n as f32)).collect();
        let mut p = Project::default();
        p.arrangement.tracks = Arrangement::default_tracks();
        p.audio_pool = vec![SampleRef { name: "t".into(), path: String::new(), data: Some(std::sync::Arc::new(SampleData { rate: 48000, mono })) }];
        let lane = p.arrangement.tracks.iter().position(|t| t.kind == crate::arrangement::TrackKind::Audio).unwrap();
        let mut c = Clip::new(0, lane, 0, 96, true); // a quarter bar: 0.43 s at 140 bpm
        edit(&mut c);
        p.arrangement.clips = vec![c];
        p.song_mode = true;
        p
    }

    fn window_energy(p: Project, from_s: f32, to_s: f32) -> f32 {
        let (buf, _) = render_secs(p, 0.6);
        let a = (from_s * 48000.0) as usize * 2;
        let b = (to_s * 48000.0) as usize * 2;
        buf[a..b.min(buf.len())].iter().map(|v| v * v).sum::<f32>()
    }

    #[test]
    fn audio_clip_reverse_fades_pitch_and_mute_change_what_is_heard() {
        // normal: the start is louder than the end of the clip; reversed: the other way round
        let (n_start, n_end) = (window_energy(audio_clip_project(|_| {}), 0.0, 0.1), window_energy(audio_clip_project(|_| {}), 0.3, 0.4));
        assert!(n_start > n_end * 1.5, "normal clip should start loud: {n_start} vs {n_end}");
        let (r_start, r_end) = (window_energy(audio_clip_project(|c| c.reverse = true), 0.0, 0.1), window_energy(audio_clip_project(|c| c.reverse = true), 0.3, 0.4));
        assert!(r_end > r_start * 1.5, "reversed clip should start quiet: {r_start} vs {r_end}");
        // a fade in takes the loud start away
        let fade = window_energy(audio_clip_project(|c| c.fade_in = 90), 0.0, 0.1);
        assert!(fade < n_start * 0.5, "fade in has no effect: {fade} vs {n_start}");
        // an octave up plays twice as fast: the clip is over after half the time
        let late_normal = window_energy(audio_clip_project(|_| {}), 0.2, 0.4);
        let late_up = window_energy(audio_clip_project(|c| c.pitch = 12), 0.2, 0.4);
        assert!(late_up < late_normal, "an octave up should finish sooner: {late_up} vs {late_normal}");
        // a muted clip is silent
        assert!(window_energy(audio_clip_project(|c| c.mute = true), 0.0, 0.4) < 1e-6, "a muted clip is heard");
    }

    #[test]
    fn midi_clip_transpose_and_mute() {
        use crate::arrangement::Clip;
        let peak_with = |edit: &dyn Fn(&mut Clip)| {
            let mut p = routed_project(None);
            edit(&mut p.arrangement.clips[0]);
            peak_of(p)
        };
        assert!(peak_with(&|_| {}) > 0.01);
        assert!(peak_with(&|c| c.mute = true) < 0.0005, "a muted MIDI clip is heard");
        assert!(peak_with(&|c| c.pitch = 7) > 0.01, "a transposed clip still plays");
    }

    #[test]
    fn silent_when_stopped() {
        let mut e = Engine::new(44100.0);
        e.set_project(Arc::new(Project::demo()));
        let mut buf = vec![0.0f32; 4096];
        e.render(&mut buf);
        assert!(buf.iter().all(|v| *v == 0.0));
    }

    #[test]
    fn swing_steps() {
        assert_eq!(step_at(24, 0), Some(1));
        assert_eq!(step_at(24, 6), None);
        assert_eq!(step_at(30, 6), Some(1));
        assert_eq!(step_at(48, 6), Some(2));
    }

    #[test]
    fn song_mode_plays_clips() {
        let mut p = Project::demo();
        p.song_mode = true;
        let mut e = Engine::new(44100.0);
        e.set_project(Arc::new(p));
        e.play();
        let mut buf = vec![0.0f32; 44100 * 2 * 3];
        e.render(&mut buf);
        assert!(buf.iter().fold(0.0f32, |m, v| m.max(v.abs())) > 0.1);
    }

    fn render_secs(p: Project, secs: f32) -> (Vec<f32>, [[f32; 2]; NUM_STRIPS + 1]) {
        let mut e = Engine::new(48000.0);
        e.set_project(Arc::new(p));
        e.play();
        let mut buf = vec![0.0f32; (48000.0 * secs) as usize * 2];
        for chunk in buf.chunks_mut(512) {
            e.render(chunk);
        }
        (buf, e.take_peaks())
    }

    #[test]
    fn wav_on_a_midi_track_plays_in_song_mode() {
        use crate::arrangement::{Arrangement, Clip};
        use crate::piano_roll::Note;
        use crate::project::{SampleData, SampleRef};
        let mono: Vec<f32> = (0..24000).map(|i| (i as f32 * 0.05).sin() * 0.8).collect();
        let mut p = Project::default();
        p.arrangement.tracks = Arrangement::starter_tracks();
        {
            let t = &mut p.arrangement.tracks[0];
            t.instrument = Some(Track::Sampler);
            t.blank = false;
            t.sampler.sample = Some(SampleRef { name: "t".into(), path: String::new(), data: Some(std::sync::Arc::new(SampleData { rate: 48000, mono })) });
        }
        assert!(p.sampler.sample.is_none(), "the shared sample is empty: the sound must come from the track");
        let mut pat = Pattern::new("x", 16);
        pat.notes.push(Note { start: 0, len: 96, pitch: 36, vel: 110, track: Track::Sampler, slide: false, mute: false, pan: 0, chance: 100 });
        p.patterns = vec![pat];
        p.arrangement.clips.push(Clip::bars(0, 0, 0, 2, false));
        p.song_mode = true;
        let (_, peaks) = render_secs(p, 2.0);
        let ch = crate::mixer::CH_BASS;
        assert!(peaks[ch][0] + peaks[ch][1] > 0.0, "the track's own sample must be audible");
    }

    #[test]
    fn note_chance_zero_is_silent_and_full_plays() {
        use crate::piano_roll::Note;
        let make = |chance: u8| {
            let mut p = Project::default();
            let mut pat = Pattern::new("x", 16);
            pat.notes.push(Note { start: 0, len: 96, pitch: 60, vel: 110, track: Track::Synth, slide: false, mute: false, pan: 0, chance });
            p.patterns = vec![pat];
            p
        };
        let (_, silent) = render_secs(make(0), 2.0);
        let (_, loud) = render_secs(make(100), 2.0);
        let ch = crate::mixer::CH_SYNTH;
        assert!(silent[ch][0] + silent[ch][1] == 0.0, "chance 0 must never play");
        assert!(loud[ch][0] + loud[ch][1] > 0.0, "chance 100 must play");
    }

    #[test]
    fn wavetable_and_analog_play() {
        use crate::piano_roll::Note;
        let mut p = Project::default();
        let mut pat = Pattern::new("x", 16);
        for (i, t) in [Track::Wavetable, Track::Analog].into_iter().enumerate() {
            pat.notes.push(Note { start: 0, len: 96, pitch: 48 + i as u8 * 12, vel: 110, track: t, slide: false, mute: false, pan: 0, chance: 100 });
            pat.notes.push(Note { start: 96, len: 96, pitch: 55, vel: 110, track: t, slide: false, mute: false, pan: 0, chance: 100 });
        }
        p.patterns = vec![pat];
        let (buf, peaks) = render_secs(p, 2.0);
        assert!(buf.iter().all(|v| v.is_finite()));
        assert!(peaks[crate::mixer::CH_WT][0] > 0.01, "wavetable silent");
        assert!(peaks[crate::mixer::CH_AN][0] > 0.01, "analog silent");
    }

    #[test]
    fn audio_clip_plays_in_song_mode() {
        use crate::arrangement::Clip;
        use crate::project::{SampleData, SampleRef};
        let mut p = Project::default();
        let mono: Vec<f32> = (0..96000).map(|i| (i as f32 * 0.05).sin() * 0.8).collect();
        p.audio_pool.push(SampleRef { name: "tone".into(), path: "tone.wav".into(), data: Some(Arc::new(SampleData { rate: 48000, mono })) });
        p.arrangement.tracks = crate::arrangement::Arrangement::default_tracks();
        p.arrangement.clips.push(Clip::bars(0, 4, 0, 1, true));
        p.song_mode = true;
        let (buf, peaks) = render_secs(p, 1.5);
        assert!(buf.iter().all(|v| v.is_finite()));
        assert!(peaks[crate::mixer::CH_AUDIO][0] > 0.05, "audio clip silent");
    }

    #[test]
    fn muted_audio_track_is_silent() {
        use crate::arrangement::Clip;
        use crate::project::{SampleData, SampleRef};
        let mut p = Project::default();
        p.audio_pool.push(SampleRef { name: "tone".into(), path: "t.wav".into(), data: Some(Arc::new(SampleData { rate: 48000, mono: vec![0.5; 96000] })) });
        p.arrangement.tracks = crate::arrangement::Arrangement::default_tracks();
        p.arrangement.tracks[4].mute = true;
        p.arrangement.clips.push(Clip::bars(0, 4, 0, 1, true));
        p.song_mode = true;
        let (_, peaks) = render_secs(p, 1.0);
        assert_eq!(peaks[crate::mixer::CH_AUDIO][0], 0.0);
    }

    #[test]
    fn midi_track_routed_to_insert() {
        use crate::arrangement::Clip;
        use crate::piano_roll::Note;
        let mut p = Project::default();
        let mut pat = Pattern::new("x", 16);
        pat.notes.push(Note { start: 0, len: 96, pitch: 60, vel: 110, track: Track::Synth, slide: false, mute: false, pan: 0, chance: 100 });
        pat.set_steps(0, &[0], 110);
        p.patterns = vec![pat];
        p.arrangement.tracks = crate::arrangement::Arrangement::default_tracks();
        p.arrangement.tracks[0].output = Some(2);
        p.arrangement.clips = vec![Clip::bars(0, 0, 0, 1, false)];
        p.song_mode = true;
        let (_, peaks) = render_secs(p, 1.0);
        assert!(peaks[NUM_CH + 2][0] > 0.01, "insert 3 silent");
        assert_eq!(peaks[crate::mixer::CH_SYNTH][0], 0.0, "synth channel should be bypassed");
        assert_eq!(peaks[0][0], 0.0, "kick channel should be bypassed");
        assert_eq!(peaks[NUM_CH][0], 0.0, "insert 1 must stay silent");
    }

    #[test]
    fn audio_track_and_channel_routed_to_insert() {
        use crate::arrangement::Clip;
        use crate::project::{SampleData, SampleRef};
        let mut p = Project::default();
        p.audio_pool.push(SampleRef { name: "t".into(), path: "t.wav".into(), data: Some(Arc::new(SampleData { rate: 48000, mono: vec![0.5; 96000] })) });
        p.arrangement.tracks = crate::arrangement::Arrangement::default_tracks();
        p.arrangement.tracks[4].output = Some(0);
        p.arrangement.clips.push(Clip::bars(0, 4, 0, 1, true));
        p.song_mode = true;
        let (_, peaks) = render_secs(p, 1.0);
        assert!(peaks[NUM_CH][0] > 0.05, "routed audio silent");
        assert_eq!(peaks[crate::mixer::CH_AUDIO][0], 0.0);

        // instrument channel -> insert: pattern drums land on the insert, mute of the insert silences them
        let mut p = Project::default();
        let mut pat = Pattern::new("x", 16);
        pat.set_steps(0, &[0, 4, 8, 12], 120);
        p.patterns = vec![pat];
        p.channels[0].output = Some(1);
        let (_, peaks) = render_secs(p.clone(), 1.0);
        assert!(peaks[NUM_CH + 1][0] > 0.01, "kick did not reach insert 2");
        assert!(peaks[0][0] > 0.01, "source meter should still show the kick");
        p.channels[NUM_CH + 1].mute = true;
        let (buf, _) = render_secs(p, 1.0);
        assert!(buf.iter().all(|v| v.abs() < 1e-4), "muted insert should silence its channels");
    }
}
