//! Arrangement model: MIDI tracks (pattern clips) and Audio tracks (sample clips) on a bar timeline.
use crate::piano_roll::Track;
use crate::sequencer::TICKS_PER_BAR;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum TrackKind {
    Midi,
    Audio,
}

/// The settings of the synths of one track. A track that has `own` set plays its instrument with these, so two tracks with
/// the same plugin are two independent plugins (before, every track of a kind shared one set of knobs).
#[derive(Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct LaneSynths {
    #[serde(default)]
    pub own: bool,
    #[serde(default)]
    pub synth: crate::instruments::SynthParams,
    #[serde(default)]
    pub bass: crate::instruments::Bass808Params,
    #[serde(default)]
    pub wt: crate::instruments::WavetableParams,
    #[serde(default)]
    pub analog: crate::instruments::AnalogParams,
}

/// One breakpoint of an automation curve: the value (a level multiplier, 1.0 = unchanged) at a tick of the song.
#[derive(Clone, Copy, PartialEq, Serialize, Deserialize, Debug)]
pub struct AutoPoint {
    pub tick: u32,
    pub v: f32,
    /// bend of the segment that starts at this point (-1..1): 0 straight, > 0 slow start / fast end, < 0 fast start / slow end
    #[serde(default)]
    pub curve: f32,
}

/// The shape of one segment: `t` 0..1 along it, `curve` -1..1 (0 = straight line).
pub fn bend(t: f32, curve: f32) -> f32 {
    if curve.abs() < 0.001 {
        t
    } else {
        t.clamp(0.0, 1.0).powf(2f32.powf(curve.clamp(-1.0, 1.0) * 2.0))
    }
}

/// The curve value a segment has at its middle (for drawing the bend handle).
pub fn curve_for_mid(v0: f32, v1: f32, mid: f32) -> f32 {
    let span = v1 - v0;
    if span.abs() < 1e-4 {
        return 0.0;
    }
    let f = ((mid - v0) / span).clamp(0.03, 0.97);
    // f = 0.5^p  ->  p = ln f / ln 0.5, curve = log2(p) / 2
    let p = f.ln() / 0.5f32.ln();
    (p.log2() / 2.0).clamp(-1.0, 1.0)
}

/// Value of the curve at `tick`: linear between points, held before the first and after the last. None without points.
pub fn auto_at(points: &[AutoPoint], tick: u32) -> Option<f32> {
    let first = points.first()?;
    if tick <= first.tick {
        return Some(first.v);
    }
    for w in points.windows(2) {
        if tick <= w[1].tick {
            let span = (w[1].tick - w[0].tick).max(1) as f32;
            let t = (tick - w[0].tick) as f32 / span;
            return Some(w[0].v + (w[1].v - w[0].v) * bend(t, w[0].curve));
        }
    }
    points.last().map(|p| p.v)
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct ArrTrack {
    pub name: String,
    pub kind: TrackKind,
    /// Sample pool entry used when tapping an empty cell on this audio track.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_brush: Option<usize>,
    /// An imported Playlist mix already contains the audio-channel mix.
    #[serde(default)]
    pub rendered_mix: bool,
    pub mute: bool,
    /// MIDI tracks: every note plays this instrument (None = the instrument stored in each note)
    #[serde(default)]
    pub instrument: Option<Track>,
    /// Mixer insert this track plays through (None = the instrument's own mixer channel)
    #[serde(default)]
    pub output: Option<usize>,
    /// semitones (MIDI: shifts notes; audio: resamples the sound)
    #[serde(default)]
    pub transpose: i32,
    /// a freshly added MIDI track has no sound: silent until a synth is dropped on it
    #[serde(default)]
    pub blank: bool,
    /// the sound of this track when its instrument is the 808 Sampler (drop a WAV on a MIDI track)
    #[serde(default)]
    pub sampler: crate::instruments::sampler::SamplerParams,
    /// only soloed tracks are heard while any track is soloed
    #[serde(default)]
    pub solo: bool,
    /// record-armed: recorded notes go to this track
    #[serde(default)]
    pub arm: bool,
    /// input source, an index into `input_names`
    #[serde(default)]
    pub input: u8,
    /// track level (1.0 = unity): scales note velocity / clip volume
    #[serde(default = "unit_gain")]
    pub volume: f32,
    /// the track's own synth settings (see `LaneSynths`)
    #[serde(default)]
    pub snd: LaneSynths,
    /// the hits of this track (its notes / audio clips starting) duck the inserts that have a Duck amount: sidechain without a plugin
    #[serde(default)]
    pub sidechain_src: bool,
    /// level automation of this track (empty = none)
    #[serde(default)]
    pub auto_vol: Vec<AutoPoint>,
    /// folder this track belongs to (0 = none, otherwise a `TrackGroup::id`)
    #[serde(default)]
    pub group: u16,
    #[serde(default)]
    pub playlist_locked: bool,
}

/// A folder of tracks in the Playlist: collapsed, only its first track shows (the others are hidden but still play).
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct TrackGroup {
    pub id: u16,
    pub name: String,
    #[serde(default)]
    pub collapsed: bool,
}

/// Input sources offered by a track's header.
pub fn input_names(kind: TrackKind) -> &'static [&'static str] {
    match kind {
        TrackKind::Midi => &["All Ins", "Keyboard", "MIDI device", "No Input"],
        TrackKind::Audio => &["No Input", "Ext. In 1/2"],
    }
}

/// A clip on a track. Positions are in ticks (96 per quarter note, 384 per bar), so clips can sit on any grid.
#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Clip {
    /// pattern index (MIDI track) or index into `Project::audio_pool` (audio track, `audio == true`)
    pub pattern: usize,
    pub lane: usize,
    /// start in ticks (files from before sub-bar clips stored whole bars here under the name `start_bar`)
    #[serde(alias = "start_bar")]
    pub start: u32,
    /// length in ticks (formerly `bars`)
    #[serde(alias = "bars")]
    pub len: u32,
    #[serde(default)]
    pub audio: bool,
    /// ticks of the clip's content that are skipped at its start (a clip cut in two plays on where the first half stopped)
    #[serde(default)]
    pub offset: u32,
    /// the clip does not play (FL's clip mute); its look stays
    #[serde(default)]
    pub mute: bool,
    /// audio clips: play the sound backwards (Ableton's Rev)
    #[serde(default)]
    pub reverse: bool,
    /// audio clips: fade in / out in ticks
    #[serde(default)]
    pub fade_in: u32,
    #[serde(default)]
    pub fade_out: u32,
    /// semitones: audio clips are resampled (pitch and speed move together, like FL), MIDI clips transpose their notes
    #[serde(default)]
    pub pitch: i8,
    /// audio clips: volume of this clip
    #[serde(default = "unit_gain")]
    pub gain: f32,
    /// audio clips: the project tempo the clip was laid out at; a different tempo now speeds the sound up or slows it down
    /// so that the clip keeps filling the same musical length (0 = not stamped yet)
    #[serde(default)]
    pub warp: f32,
}

fn unit_gain() -> f32 {
    1.0
}

impl Clip {
    pub fn new(pattern: usize, lane: usize, start: u32, len: u32, audio: bool) -> Clip {
        Clip { pattern, lane, start, len: len.max(1), audio, offset: 0, gain: 1.0, mute: false, reverse: false, fade_in: 0, fade_out: 0, pitch: 0, warp: 0.0 }
    }
    /// a clip `bars` bars long that starts on bar `start_bar`
    pub fn bars(pattern: usize, lane: usize, start_bar: u32, bars: u32, audio: bool) -> Clip {
        Clip::new(pattern, lane, start_bar * TICKS_PER_BAR, bars.max(1) * TICKS_PER_BAR, audio)
    }
    pub fn start_tick(&self) -> u32 {
        self.start
    }
    pub fn end_tick(&self) -> u32 {
        self.start + self.len
    }
    pub fn start_bar(&self) -> u32 {
        self.start / TICKS_PER_BAR
    }
    /// the bar in which the clip ends (rounded up)
    pub fn end_bar(&self) -> u32 {
        (self.end_tick() + TICKS_PER_BAR - 1) / TICKS_PER_BAR
    }
    pub fn overlaps(&self, other: &Clip) -> bool {
        self.lane == other.lane && self.start < other.end_tick() && other.start < self.end_tick()
    }
}

#[derive(Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Arrangement {
    pub clips: Vec<Clip>,
    pub tracks: Vec<ArrTrack>,
    pub groups: Vec<TrackGroup>,
}

impl Arrangement {
    /// What a new empty project starts with: 3 MIDI tracks (silent until a plugin is dropped on them) and 3 audio tracks.
    pub fn starter_tracks() -> Vec<ArrTrack> {
        let mut t = vec![];
        for i in 1..=3 {
            t.push(ArrTrack { name: format!("MIDI {i}"), kind: TrackKind::Midi, mute: false, instrument: None, output: None, transpose: 0, blank: true, sampler: Default::default(), snd: Default::default(), auto_vol: Vec::new(), sidechain_src: false, audio_brush: None, rendered_mix: false, solo: false, arm: false, input: 0, volume: 1.0, group: 0, playlist_locked: false });
        }
        for i in 1..=3 {
            t.push(ArrTrack { name: format!("Audio {i}"), kind: TrackKind::Audio, mute: false, instrument: None, output: None, transpose: 0, blank: false, sampler: Default::default(), snd: Default::default(), auto_vol: Vec::new(), sidechain_src: false, audio_brush: None, rendered_mix: false, solo: false, arm: false, input: 0, volume: 1.0, group: 0, playlist_locked: false });
        }
        t
    }

    pub fn default_tracks() -> Vec<ArrTrack> {
        let mut t = vec![];
        for i in 1..=4 {
            t.push(ArrTrack { name: format!("MIDI {i}"), kind: TrackKind::Midi, mute: false, instrument: None, output: None, transpose: 0, blank: false, sampler: Default::default(), snd: Default::default(), auto_vol: Vec::new(), sidechain_src: false, audio_brush: None, rendered_mix: false, solo: false, arm: false, input: 0, volume: 1.0, group: 0, playlist_locked: false });
        }
        for i in 1..=4 {
            t.push(ArrTrack { name: format!("Audio {i}"), kind: TrackKind::Audio, mute: false, instrument: None, output: None, transpose: 0, blank: false, sampler: Default::default(), snd: Default::default(), auto_vol: Vec::new(), sidechain_src: false, audio_brush: None, rendered_mix: false, solo: false, arm: false, input: 0, volume: 1.0, group: 0, playlist_locked: false });
        }
        t
    }

    /// A new project starts without tracks. Only files / tests that have clips but no track list get the
    /// tracks those clips refer to.
    pub fn ensure_tracks(&mut self) {
        if self.tracks.is_empty() && !self.clips.is_empty() {
            let n = self.clips.iter().map(|c| c.lane + 1).max().unwrap_or(0);
            for l in 0..n {
                let audio = self.clips.iter().any(|c| c.lane == l && c.audio);
                self.add_track(if audio { TrackKind::Audio } else { TrackKind::Midi });
            }
        }
    }

    pub fn add_track(&mut self, kind: TrackKind) {
        let n = self.tracks.iter().filter(|t| t.kind == kind).count() + 1;
        let name = match kind {
            TrackKind::Midi => format!("MIDI {n}"),
            TrackKind::Audio => format!("Audio {n}"),
        };
        self.tracks.push(ArrTrack { name, kind, mute: false, instrument: None, output: None, transpose: 0, blank: kind == TrackKind::Midi, sampler: Default::default(), snd: Default::default(), auto_vol: Vec::new(), sidechain_src: false, audio_brush: None, rendered_mix: false, solo: false, arm: false, input: 0, volume: 1.0, group: 0, playlist_locked: false });
    }

    /// a track inside a collapsed folder that is not the folder's first track: not drawn in the Playlist
    pub fn hidden(&self, lane: usize) -> bool {
        let g = self.tracks.get(lane).map_or(0, |t| t.group);
        g != 0 && self.groups.iter().any(|x| x.id == g && x.collapsed) && self.tracks.iter().position(|t| t.group == g) != Some(lane)
    }

    /// how many tracks a folder holds
    pub fn group_size(&self, g: u16) -> usize {
        self.tracks.iter().filter(|t| t.group == g).count()
    }

    pub fn group_playlist_track(&mut self, source: usize, target: Option<usize>, folder: Option<u16>) {
        if source >= self.tracks.len() || target == Some(source) {
            return;
        }
        let id = if let Some(target) = target.filter(|&lane| lane < self.tracks.len()) {
            let existing = self.tracks[target].group;
            if existing != 0 && self.groups.iter().any(|group| group.id == existing) {
                existing
            } else {
                let Some(id) = (1..=u16::MAX).find(|id| !self.groups.iter().any(|group| group.id == *id)) else { return; };
                self.groups.push(TrackGroup { id, name: format!("Group {id}"), collapsed: false });
                self.tracks[target].group = id;
                id
            }
        } else if let Some(id) = folder.filter(|id| *id != 0 && self.groups.iter().any(|group| group.id == *id)) {
            id
        } else {
            return;
        };
        self.tracks[source].group = id;
        self.prune_empty_groups();
    }

    pub fn ungroup_playlist_track(&mut self, lane: usize) {
        if let Some(track) = self.tracks.get_mut(lane) {
            track.group = 0;
        }
        self.prune_empty_groups();
    }

    fn prune_empty_groups(&mut self) {
        self.groups.retain(|group| self.tracks.iter().any(|track| track.group == group.id));
    }

    pub fn remove_track(&mut self, lane: usize) {
        if lane >= self.tracks.len() || self.tracks.len() <= 1 {
            return;
        }
        self.tracks.remove(lane);
        self.prune_empty_groups();
        self.clips.retain(|c| c.lane != lane);
        for c in self.clips.iter_mut() {
            if c.lane > lane {
                c.lane -= 1;
            }
        }
    }

    /// muted, or a MIDI track that has no sound yet
    pub fn silent(&self, lane: usize) -> bool {
        self.tracks.get(lane).map(|t| t.mute || self.solo_blocks(lane) || (t.blank && t.instrument.is_none())).unwrap_or(false)
    }

    pub fn muted(&self, lane: usize) -> bool {
        self.tracks.get(lane).map(|t| t.mute || self.solo_blocks(lane)).unwrap_or(false)
    }

    /// another track is soloed and this one is not
    fn solo_blocks(&self, lane: usize) -> bool {
        self.tracks.iter().any(|t| t.solo) && !self.tracks.get(lane).map_or(false, |t| t.solo)
    }

    pub fn kind(&self, lane: usize) -> TrackKind {
        self.tracks.get(lane).map(|t| t.kind).unwrap_or(TrackKind::Midi)
    }

    pub fn song_end_bar(&self) -> u32 {
        self.clips.iter().map(|c| c.end_bar()).max().unwrap_or(0)
    }

    pub fn can_place(&self, c: &Clip, ignore: Option<usize>) -> bool {
        self.clips.iter().enumerate().all(|(i, o)| Some(i) == ignore || !c.overlaps(o))
    }
}
