//! "Split by channel" for Playlist clips: a clip that holds the whole beat (everything written in the Channel Rack) becomes
//! one MIDI track per channel, or one audio track per channel (each channel is rendered to a WAV and placed as audio clips).
use crate::arrangement::{ArrTrack, Clip, TrackKind};
use crate::audio_engine::export::{self, BitDepth, ExportSettings};
use crate::piano_roll::Track;
use crate::project::{Project, SampleData};
use crate::sequencer::*;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// every drum lane and every instrument that has something in the pattern, as a pattern of its own
fn channel_parts(src: &Pattern) -> Vec<(String, Pattern, Option<Track>)> {
    let mut parts = Vec::new();
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
    parts
}

/// the track called `name` of that kind, created when missing
fn lane_named(project: &mut Project, name: &str, kind: TrackKind, inst: Option<Track>) -> usize {
    if let Some(l) = project.arrangement.tracks.iter().position(|t| t.kind == kind && t.name == name) {
        return l;
    }
    project.arrangement.add_track(kind);
    let l = project.arrangement.tracks.len() - 1;
    let t = &mut project.arrangement.tracks[l];
    t.name = name.to_string();
    t.instrument = inst;
    t.blank = false;
    l
}

/// drop the track the beat was sent to when nothing is left on it
fn drop_if_empty(project: &mut Project, lane: usize, beat_name: &str) {
    let empty = project.arrangement.tracks.get(lane).map_or(false, |t| t.kind == TrackKind::Midi && t.instrument.is_none() && t.name == beat_name)
        && !project.arrangement.clips.iter().any(|c| c.lane == lane);
    if empty && project.arrangement.tracks.len() > 1 {
        project.arrangement.remove_track(lane);
        project.session.remove_lane(lane);
    }
}

/// Split clip `ci` into one MIDI track per channel. Returns the number of channels.
pub fn split_clip_midi(project: &mut Project, ci: usize) -> usize {
    let Some(c) = project.arrangement.clips.get(ci).copied() else { return 0 };
    if c.audio || c.pattern >= project.patterns.len() {
        return 0;
    }
    let src = project.patterns[c.pattern].clone();
    let parts = channel_parts(&src);
    if parts.is_empty() {
        return 0;
    }
    project.arrangement.clips.remove(ci);
    let mut count = 0;
    for (name, pat, inst) in parts {
        project.patterns.push(pat);
        let pi = project.patterns.len() - 1;
        let lane = lane_named(project, &name, TrackKind::Midi, inst);
        let clip = Clip { pattern: pi, lane, start: c.start, len: c.len, audio: false, offset: c.offset, gain: 1.0, mute: false, reverse: false, fade_in: 0, fade_out: 0, pitch: 0, warp: 0.0 };
        if project.arrangement.can_place(&clip, None) {
            project.arrangement.clips.push(clip);
            count += 1;
        }
    }
    drop_if_empty(project, c.lane, &src.name);
    project.order_tracks();
    count
}

/// Split clip `ci` into one audio track per channel: each channel is rendered (through the mixer and master chain) to a
/// WAV in the `bounces` folder and placed as audio clips. Returns the number of channels.
pub fn split_clip_audio(project: &mut Project, ci: usize) -> Result<usize, String> {
    split_clip_audio_in(project, ci, Path::new("bounces"))
}

pub fn split_clip_audio_in(project: &mut Project, ci: usize, dir: &Path) -> Result<usize, String> {
    let Some(c) = project.arrangement.clips.get(ci).copied() else { return Ok(0) };
    if c.audio || c.pattern >= project.patterns.len() {
        return Ok(0);
    }
    let src = project.patterns[c.pattern].clone();
    let parts = channel_parts(&src);
    if parts.is_empty() {
        return Ok(0);
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let bars = ((src.len_ticks() + TICKS_PER_BAR - 1) / TICKS_PER_BAR).max(1);
    let one = bars * TICKS_PER_BAR;
    let settings = ExportSettings { sample_rate: 48000, depth: BitDepth::Pcm16, bars, tail_secs: 0.0 };
    let clean = |s: &str| -> String { s.chars().map(|ch| if ch.is_alphanumeric() { ch } else { '_' }).collect() };
    let mut rendered: Vec<(String, usize)> = Vec::new();
    for (name, pat, _) in parts {
        // render this channel alone: the same project, but only this part of the pattern
        let mut q = project.clone();
        q.song_mode = false;
        q.metronome = false;
        q.patterns = vec![pat];
        q.current_pattern = 0;
        q.arrangement.clips.clear();
        let data = export::render(&q, &settings);
        if data.iter().fold(0.0f32, |m, v| m.max(v.abs())) < 1e-4 {
            continue;
        }
        let file: PathBuf = dir.join(format!("{}_{}_{}.wav", clean(&project.name), clean(&src.name), clean(&name)));
        export::write_wav(&file, &data, &settings)?;
        let mono: Vec<f32> = data.chunks(2).map(|f| (f[0] + f.get(1).copied().unwrap_or(f[0])) * 0.5).collect();
        let idx = project.pool_index(&format!("{} - {}", src.name, name), &file.to_string_lossy(), Arc::new(SampleData { rate: settings.sample_rate, mono }));
        rendered.push((name, idx));
    }
    if rendered.is_empty() {
        return Ok(0);
    }
    project.arrangement.clips.remove(ci);
    for (name, idx) in &rendered {
        let lane = lane_named(project, &format!("{name} (audio)"), TrackKind::Audio, None);
        let mut start = c.start;
        while start < c.end_tick() {
            let len = one.min(c.end_tick() - start);
            let clip = Clip::new(*idx, lane, start, len, true);
            if project.arrangement.can_place(&clip, None) {
                project.arrangement.clips.push(clip);
            }
            start += one;
        }
    }
    drop_if_empty(project, c.lane, &src.name);
    project.order_tracks();
    Ok(rendered.len())
}

/// Render one Playlist track (all its clips, its own synth settings, its insert effects) to a WAV and put it at once on a new
/// audio track under the others: "<name> (render)". The source track is muted so the render replaces it (unmute it to compare).
/// Returns the index of the new audio track, or Ok(None) when the track has nothing audible.
pub fn render_track_to_audio(project: &mut Project, lane: usize) -> Result<Option<usize>, String> {
    render_track_to_audio_in(project, lane, Path::new("bounces"))
}

pub fn render_track_to_audio_in(project: &mut Project, lane: usize, dir: &Path) -> Result<Option<usize>, String> {
    render_to_audio_in(project, Some(lane), dir)
}

pub fn render_mix_to_audio(project: &mut Project) -> Result<Option<usize>, String> {
    render_to_audio_in(project, None, Path::new("bounces"))
}

fn render_to_audio_in(project: &mut Project, lane: Option<usize>, dir: &Path) -> Result<Option<usize>, String> {
    let name = match lane {
        Some(lane) => match project.arrangement.tracks.get(lane) {
            Some(track) => track.name.clone(),
            None => return Ok(None),
        },
        None => "Playlist mix".to_owned(),
    };
    let mut q = project.clone();
    q.song_mode = true;
    q.metronome = false;
    q.loop_on = false;
    if let Some(lane) = lane {
        q.arrangement.clips.retain(|clip| clip.lane == lane);
        for track in &mut q.arrangement.tracks {
            track.solo = false;
        }
        q.arrangement.tracks[lane].mute = false;
    }
    if q.arrangement.clips.is_empty() {
        return Ok(None);
    }
    q.arrangement.clips.retain(|clip| !clip.mute && (lane.is_some() || !project.arrangement.muted(clip.lane)));
    if q.arrangement.clips.is_empty() {
        return Ok(None);
    }
    let bars = export::default_bars(&q).max(1);
    let settings = ExportSettings { sample_rate: 48000, depth: BitDepth::Pcm16, bars, tail_secs: 1.5 };
    // The imported track runs through the project's master chain on playback.
    q.master_fx = crate::project::MasterFx::default();
    q.master_volume = 1.0;
    q.master_chain.clear();
    q.master_auto.clear();
    let data = export::render(&q, &settings);
    if data.iter().fold(0.0f32, |m, v| m.max(v.abs())) < 1e-4 {
        return Ok(None);
    }
    let new_lane = store_render(project, dir, &name, &data, &settings, 0, None)?;
    if let Some(lane) = lane {
        project.arrangement.tracks[lane].mute = true;
        if project.arrangement.tracks[lane].solo {
            project.arrangement.tracks[lane].solo = false;
            project.arrangement.tracks[new_lane].solo = true;
        }
    } else {
        project.arrangement.tracks[new_lane].rendered_mix = true;
        for track in &mut project.arrangement.tracks[..new_lane] {
            track.mute = true;
            track.solo = false;
        }
    }
    project.song_mode = true;
    Ok(Some(new_lane))
}

fn store_render(project: &mut Project, dir: &Path, name: &str, data: &[f32], settings: &ExportSettings,
    start: u32, duration: Option<u32>) -> Result<usize, String>
{
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let clean = |s: &str| -> String { s.chars().map(|ch| if ch.is_alphanumeric() { ch } else { '_' }).collect() };
    let base = format!("{}_{}_render", clean(&project.name), clean(name));
    let (file, writer) = (1_u32..).find_map(|take| {
        let name = if take == 1 { base.clone() } else { format!("{base}_{take}") };
        let path = dir.join(format!("{name}.wav"));
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(writer) => Some(Ok((path, writer))),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => None,
            Err(error) => Some(Err(format!("{}: {error}", path.display()))),
        }
    }).ok_or("Could not reserve a render file")??;
    if let Err(error) = export::write_wav_to(std::io::BufWriter::new(writer), data, settings) {
        let _ = std::fs::remove_file(&file);
        return Err(error);
    }
    let mono: Vec<f32> = data.chunks(2).map(|f| (f[0] + f.get(1).copied().unwrap_or(f[0])) * 0.5).collect();
    let secs = mono.len() as f32 / settings.sample_rate as f32;
    let idx = project.pool_index(&format!("{name} (render)"), &file.to_string_lossy(), Arc::new(SampleData { rate: settings.sample_rate, mono }));
    project.arrangement.add_track(TrackKind::Audio);
    let new_lane = project.arrangement.tracks.len() - 1;
    project.arrangement.tracks[new_lane].name = format!("{name} (render)");
    let len = duration.unwrap_or_else(|| ((secs * project.bpm / 60.0 * PPQ as f32).floor() as u32).max(24));
    project.arrangement.clips.push(Clip::new(idx, new_lane, start, len, true));
    project.arrangement.tracks[new_lane].audio_brush = Some(idx);
    Ok(new_lane)
}

pub fn render_selection_to_audio(project: &mut Project, indices: &[usize]) -> Result<Option<usize>, String> {
    let clips = &project.arrangement.clips;
    let mut audible = Vec::new();
    for &index in indices {
        let Some(clip) = clips.get(index) else { continue; };
        let Some(track) = project.arrangement.tracks.get(clip.lane) else { continue; };
        if track.playlist_locked {
            return Err("Unlock the selected tracks before rendering.".into());
        }
        if !clip.mute && (if clip.audio { !project.arrangement.muted(clip.lane) }
            else { !project.arrangement.silent(clip.lane) })
        {
            audible.push(index);
        }
    }
    let Some(start) = audible.iter().map(|&index| clips[index].start).min() else { return Ok(None); };
    let end = audible.iter().map(|&index| clips[index].end_tick()).max().unwrap_or(start);
    let mut source = project.clone();
    source.arrangement.clips = audible.iter().map(|&index| clips[index]).collect();
    source.master_fx = crate::project::MasterFx::default();
    source.master_volume = 1.0;
    source.master_chain.clear();
    source.master_auto.clear();
    let settings = ExportSettings { sample_rate: 48000, depth: BitDepth::Pcm16, bars: 1, tail_secs: 0.0 };
    let data = export::render_range(&source, &settings, start, end)?;
    if data.iter().all(|value| value.abs() < 1e-4) {
        return Ok(None);
    }
    let lane = store_render(project, Path::new("bounces"), "Rendered selection", &data, &settings, start, Some(end - start))?;
    let solo = project.arrangement.tracks.iter().any(|track| track.solo);
    let rendered = &mut project.arrangement.tracks[lane];
    rendered.rendered_mix = true;
    rendered.solo = solo;
    for index in audible {
        project.arrangement.clips[index].mute = true;
    }
    project.song_mode = true;
    Ok(Some(lane))
}

/// A fresh Beat track for a pattern: first in the list, plays whatever the pattern holds (drums and per-note instruments).
pub fn beat_track(name: &str) -> ArrTrack {
    ArrTrack { name: name.to_string(), kind: TrackKind::Midi, mute: false, instrument: None, output: None, transpose: 0, blank: false, sampler: Default::default(), snd: Default::default(), auto_vol: Vec::new(), sidechain_src: false, audio_brush: None, rendered_mix: false, solo: false, arm: false, input: 0, volume: 1.0, group: 0, playlist_locked: false, fx_automation: Vec::new() }
}

#[cfg(test)]
mod render_tests {
    use super::*;
    use crate::arrangement::Arrangement;
    use crate::piano_roll::Note;

    #[test]
    fn rendering_a_track_puts_audio_on_a_new_track_at_once() {
        let mut p = Project::default();
        p.arrangement.tracks = Arrangement::default_tracks();
        p.arrangement.tracks[0].instrument = Some(Track::Wavetable);
        let mut pat = Pattern::new("x", 16);
        pat.clip_only = true;
        pat.notes.push(Note { start: 0, len: 96, pitch: 60, vel: 110, track: Track::Wavetable, slide: false, mute: false, pan: 0, chance: 100 });
        p.patterns = vec![pat];
        p.arrangement.clips.push(Clip::bars(0, 0, 0, 1, false));
        p.adopt_track_params();
        let before = p.arrangement.tracks.len();
        let dir = std::env::temp_dir().join("wynteria_render_test");
        let lane = render_track_to_audio_in(&mut p, 0, &dir).unwrap().expect("something audible");
        assert_eq!(p.arrangement.tracks.len(), before + 1);
        assert_eq!(p.arrangement.tracks[lane].kind, TrackKind::Audio);
        assert!(p.arrangement.clips.iter().any(|c| c.audio && c.lane == lane && c.len > 0), "no audio clip on the new track");
        assert!(p.arrangement.tracks[0].mute, "the source is muted so the render replaces it");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arrangement::Arrangement;
    use crate::piano_roll::Note;

    fn beat() -> Project {
        let mut p = Project::default();
        p.arrangement.tracks = Arrangement::starter_tracks();
        let pat = &mut p.patterns[0];
        pat.length_steps = 32;
        pat.set_steps(0, &[0, 8, 16, 24], 120);
        pat.set_steps(3, &[2, 6, 10, 14], 90);
        pat.notes.push(Note { start: 0, len: 96, pitch: 60, vel: 100, track: Track::Synth, slide: false, mute: false, pan: 0, chance: 100 });
        p.send_pattern_to_playlist(0, 2);
        p
    }

    #[test]
    fn whole_beat_goes_to_the_playlist_then_splits_into_midi_tracks() {
        let mut p = beat();
        assert_eq!(p.arrangement.tracks[0].name, "Pattern 1");
        assert_eq!(p.arrangement.clips.len(), 2, "the whole pattern, twice, on one track");
        let n = split_clip_midi(&mut p, 0);
        assert_eq!(n, 3);
        assert!(p.arrangement.tracks.iter().any(|t| t.name == "Kick"));
        // one whole-beat clip is left (the second repeat) on the Beat track, the first one became channel clips
        assert!(p.arrangement.clips.len() >= 4);
        let whole = p.arrangement.clips.iter().position(|c| p.patterns[c.pattern].name == "Pattern 1").unwrap();
        let n2 = split_clip_midi(&mut p, whole);
        assert_eq!(n2, 3);
        assert!(!p.arrangement.tracks.iter().any(|t| t.name == "Pattern 1"), "the empty Beat track disappears");
    }

    #[test]
    fn whole_beat_splits_into_audio_tracks() {
        let mut p = beat();
        let dir = std::env::temp_dir().join("synthesia_bounce_test");
        let n = split_clip_audio_in(&mut p, 0, &dir).unwrap();
        assert!(n >= 2, "at least kick and hats are audible: {n}");
        let audio: Vec<_> = p.arrangement.tracks.iter().filter(|t| t.name.ends_with("(audio)")).collect();
        assert_eq!(audio.len(), n);
        assert!(p.arrangement.clips.iter().any(|c| c.audio));
        assert!(std::fs::read_dir(&dir).unwrap().count() >= n);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
