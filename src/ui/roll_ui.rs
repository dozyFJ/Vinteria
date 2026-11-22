//! Piano roll UI (FL Studio / Ableton style): draw & select tools, box / multi selection, group move & resize,
//! Ctrl/Alt-drag duplicate, copy / cut / paste, Ctrl+D duplicate, magnet snapping to neighbouring notes,
//! quantize, keyboard nudging, velocity lane, chords and 808 slides.
use super::theme::*;
use super::widgets::{play_fill, section_label};
use crate::audio_engine::device::Shared;
use crate::audio_engine::Cmd;
use crate::piano_roll::*;
use crate::project::Project;
use crate::sequencer::*;
use crate::theory;
use egui::{pos2, vec2, Align2, Color32, FontId, Key, Modifiers, Pos2, Rect, Sense, Stroke, Ui};
use std::collections::BTreeSet;
use std::sync::atomic::Ordering;

const PMIN: u8 = 0;
const PMAX: u8 = 127;
const KEYW: f32 = 58.0;

#[derive(Clone, Copy, PartialEq)]
pub enum Tool {
    /// click adds a note, Shift+drag box-selects
    Draw,
    /// drag box-selects, double-click adds a note
    Select,
    /// knife: click a note to split it at the snapped position
    Cut,
}

enum Drag {
    Move { orig: Vec<(usize, Note)>, anchor: usize, grab: i64, grab_pitch: i32, last_pitch: u8 },
    Resize { orig: Vec<(usize, Note)>, anchor: usize },
    Box { start: Pos2, base: BTreeSet<usize> },
}

pub struct RollState {
    /// shown inside the bottom dock: slim toolbar, the extra bars are opt-in
    pub compact: bool,
    pub show_more: bool,
    pub show_auto: bool,
    pub track: Track,
    pub snap: u32,
    pub note_len: u32,
    pub chord: usize,
    pub slide_new: bool,
    pub zoom: f32,
    /// row height in px (vertical zoom)
    pub row_h: f32,
    pub tool: Tool,
    pub magnet: bool,
    /// the Playlist track fixes the sound: hide the instrument chips
    pub lock_track: bool,
    /// the track has a plugin: only then clicking a note or a key makes a sound
    pub audible: bool,
    /// scale tools: tint rows by the project key, fold away foreign rows, snap pitches to the key, stack diatonic chords
    pub highlight: bool,
    pub fold: bool,
    pub snap_scale: bool,
    pub diatonic: bool,
    pub sevenths: bool,
    pub circle_open: bool,
    /// show a ghost where the next note of the melody could go (Tab accepts it)
    pub suggest: bool,
    /// translucent notes of the pattern / clip that was open before this one (a melody written there guides the new one)
    pub show_ghost: bool,
    ghost_pat: Option<usize>,
    /// automation lane under the grid: 0 = velocity, 1 = chance; mode 0 free draw, 1 line, 2 curve
    pub lane_param: u8,
    /// right panel: 0 Note, 1 Channel, 2 Track
    pub side_tab: u8,
    /// a click on a track tab asks the owner of the roll to switch to this Playlist track
    pub switch_to: Option<usize>,
    /// notes sound while they are drawn / clicked
    pub preview_on: bool,
    /// Playlist track being edited (for the Track tab)
    pub lane: usize,
    /// Quantize strength 0..1
    pub qstrength: f32,
    pub lane_mode: u8,
    lane_drag: Option<(f32, f32)>,
    ramp_from: f32,
    ramp_to: f32,
    scroll_pitch: Option<u8>,
    selected: BTreeSet<usize>,
    /// stretch window (FL style): open flag, factor and the notes as they were when it opened (index, start, len)
    stretch_open: bool,
    stretch_f: f32,
    stretch_snap: Vec<(usize, u32, u32)>,
    /// time range (start tick, end tick) dragged on the ruler: the note functions work on the notes inside it instead of the selection
    range: Option<(u32, u32)>,
    /// ruler drag in progress: 0 new range, 1 move (Ctrl), 2 stretch left edge, 3 stretch right edge; the range and the notes in it as they were
    ruler_drag: Option<(u8, (u32, u32), Vec<(usize, u32, u32)>)>,
    clip: Vec<Note>,
    cursor: Option<u32>,
    drag: Option<Drag>,
    preview: Vec<(Track, u8)>,
    scroll_to: Option<f32>,
    last_track: Option<Track>,
    last_pattern: Option<usize>,
    scroll_x: f32,
    /// horizontal offset to apply on the next frame (zoom keeps the point under the mouse)
    scroll_to_x: Option<f32>,
    /// the circular menu of a note (double click): where it is and which note it is about
    radial: Option<(Pos2, usize)>,
}

impl RollState {
    pub fn new() -> Self {
        RollState {
            compact: false,
            show_more: false,
            show_auto: false,
            track: Track::Synth,
            snap: 24,
            note_len: 24,
            chord: 0,
            slide_new: false,
            zoom: 44.0,
            row_h: 26.0,
            // Draw is on from the start: the first click puts a note
            tool: Tool::Draw,
            magnet: true,
            lock_track: false,
            audible: true,
            highlight: true,
            fold: false,
            snap_scale: false,
            diatonic: true,
            sevenths: false,
            circle_open: false,
            // the "next note" hint is off: it only gets in the way of a melody the user is writing
            suggest: false,
            show_ghost: true,
            ghost_pat: None,
            lane_param: 0,
            side_tab: 0,
            switch_to: None,
            preview_on: true,
            lane: usize::MAX,
            qstrength: 1.0,
            lane_mode: 0,
            lane_drag: None,
            ramp_from: 0.3,
            ramp_to: 1.0,
            scroll_pitch: None,
            selected: BTreeSet::new(),
            stretch_open: false,
            stretch_f: 1.0,
            stretch_snap: vec![],
            range: None,
            ruler_drag: None,
            clip: vec![],
            cursor: None,
            drag: None,
            preview: vec![],
            scroll_to: None,
            last_track: None,
            last_pattern: None,
            scroll_x: 0.0,
            scroll_to_x: None,
            radial: None,
        }
    }

    fn stop_preview(&mut self, shared: &Shared) {
        for (t, p) in self.preview.drain(..) {
            shared.send(Cmd::NoteOff { track: t, pitch: p });
        }
    }

    fn sel(&self, pat: &Pattern) -> Vec<usize> {
        self.selected.iter().copied().filter(|&i| i < pat.notes.len() && pat.notes[i].track == self.track).collect()
    }

    /// notes the functions (quantize, transpose, fit to scale...) act on: the notes starting inside the ruler range when one is set, else the selection
    fn work_sel(&self, pat: &Pattern) -> Vec<usize> {
        match self.range {
            Some((a, b)) => pat.notes.iter().enumerate().filter(|(_, n)| n.track == self.track && n.start >= a && n.start < b).map(|(i, _)| i).collect(),
            None => self.sel(pat),
        }
    }

    fn snap_unit(&self) -> u32 {
        self.snap.max(1)
    }

    fn select_all(&mut self, pat: &Pattern) {
        self.selected = pat.notes.iter().enumerate().filter(|(_, n)| n.track == self.track).map(|(i, _)| i).collect();
    }

    fn copy(&mut self, pat: &Pattern) {
        let idx = self.sel(pat);
        if idx.is_empty() {
            return;
        }
        let min = idx.iter().map(|&i| pat.notes[i].start).min().unwrap();
        self.clip = idx.iter().map(|&i| Note { start: pat.notes[i].start - min, ..pat.notes[i] }).collect();
    }

    fn delete_selected(&mut self, pat: &mut Pattern) {
        let mut idx = self.sel(pat);
        idx.sort_unstable_by(|a, b| b.cmp(a));
        for i in idx {
            pat.notes.remove(i);
        }
        self.selected.clear();
    }

    fn paste(&mut self, pat: &mut Pattern) {
        if self.clip.is_empty() {
            return;
        }
        let total = pat.len_ticks();
        let at = self.cursor.unwrap_or(0).min(total.saturating_sub(1));
        let mut extent = 0;
        self.selected.clear();
        for n in self.clip.clone() {
            let start = at + n.start;
            if start >= total {
                continue;
            }
            let len = n.len.min(total - start);
            extent = extent.max(n.start + n.len);
            pat.notes.push(Note { start, len, track: self.track, ..n });
            self.selected.insert(pat.notes.len() - 1);
        }
        self.cursor = Some((at + extent).min(total.saturating_sub(1)));
    }

    /// Ableton-style Ctrl+D: copy the selection right after itself and select the copies.
    fn duplicate(&mut self, pat: &mut Pattern) {
        let idx = self.sel(pat);
        if idx.is_empty() {
            return;
        }
        let min = idx.iter().map(|&i| pat.notes[i].start).min().unwrap();
        let max = idx.iter().map(|&i| pat.notes[i].start + pat.notes[i].len).max().unwrap();
        let unit = self.snap_unit();
        let shift = ((max - min + unit - 1) / unit * unit).max(unit);
        // copies that land past the end of the pattern open the next bar(s) instead of being lost
        let need = idx.iter().map(|&i| pat.notes[i].start + shift + pat.notes[i].len).max().unwrap();
        let need_steps = ((need + TICKS_PER_STEP - 1) / TICKS_PER_STEP + 15) / 16 * 16;
        if need_steps > pat.length_steps {
            pat.length_steps = need_steps.min(crate::sequencer::MAX_STEPS as u32);
        }
        let total = pat.len_ticks();
        let copies: Vec<Note> = idx.iter().map(|&i| pat.notes[i]).collect();
        self.selected.clear();
        for n in copies {
            let start = n.start + shift;
            if start < total {
                pat.notes.push(Note { start, len: n.len.min(total - start), ..n });
                self.selected.insert(pat.notes.len() - 1);
            }
        }
    }

    /// drop duplicate notes and trim a note where the same pitch starts again
    fn clean(&mut self, pat: &mut Pattern) {
        let tr = self.track;
        let mut v: Vec<Note> = pat.notes.iter().filter(|n| n.track == tr).copied().collect();
        v.sort_by_key(|n| (n.pitch, n.start, std::cmp::Reverse(n.len)));
        v.dedup_by(|b, a| a.pitch == b.pitch && a.start == b.start);
        for i in 0..v.len() {
            if let Some(nx) = v.get(i + 1).copied() {
                if nx.pitch == v[i].pitch && v[i].start + v[i].len > nx.start {
                    v[i].len = nx.start - v[i].start;
                }
            }
        }
        pat.notes.retain(|n| n.track != tr);
        pat.notes.extend(v.into_iter().filter(|n| n.len > 0));
        self.selected.clear();
    }

    /// Next-note hint: continues the melody (highest note per onset) at the same rhythm distance, repeating what followed the
    /// last interval earlier in the line - or stepping back toward the middle when it is new - and snapped to the key.
    fn suggest_next(&self, pat: &Pattern, kr: u8, ks: usize) -> Option<(u32, u32, u8)> {
        if self.track == Track::Drums {
            return None;
        }
        let mut v: Vec<&Note> = pat.notes.iter().filter(|n| n.track == self.track).collect();
        v.sort_by_key(|n| (n.start, n.pitch));
        let mut line: Vec<(u32, u32, u8)> = vec![];
        for n in v {
            if let Some(l) = line.last_mut() {
                if l.0 == n.start {
                    if n.pitch > l.2 {
                        l.2 = n.pitch;
                    }
                    continue;
                }
            }
            line.push((n.start, n.len, n.pitch));
        }
        if line.len() < 2 {
            return None;
        }
        let last = line[line.len() - 1];
        let prev = line[line.len() - 2];
        let ioi = (last.0 - prev.0).max(1);
        let start = last.0 + ioi;
        let total = pat.len_ticks();
        if start >= total {
            return None;
        }
        let iv: Vec<i32> = line.windows(2).map(|w| w[1].2 as i32 - w[0].2 as i32).collect();
        let last_iv = *iv.last()?;
        let mut pred = None;
        for k in (0..iv.len() - 1).rev() {
            if iv[k] == last_iv {
                pred = Some(iv[k + 1]);
                break;
            }
        }
        let step = pred.unwrap_or(-last_iv.signum() * last_iv.abs().min(2));
        let mut p = (last.2 as i32 + step).clamp(0, 127) as u8;
        if ks != theory::CHROMATIC {
            p = theory::snap_to_scale(kr, ks, p);
        }
        Some((start, last.1.min(total - start).max(1), p))
    }

    /// notes the tools work on: the selection, or every note of this track when nothing is selected
    fn targets(&self, pat: &Pattern) -> Vec<usize> {
        let s = self.work_sel(pat);
        if !s.is_empty() || self.range.is_some() {
            return s;
        }
        pat.notes.iter().enumerate().filter(|(_, n)| n.track == self.track).map(|(i, _)| i).collect()
    }

    /// FL "chop" / Ableton "split": every target note becomes `parts` equal notes in a row. With `ramp` the pieces get louder
    /// one after the other (a roll that builds up). The new notes join the selection.
    fn chop(&mut self, pat: &mut Pattern, parts: u32, ramp: bool) {
        if parts < 2 {
            return;
        }
        let mut idx = self.targets(pat);
        idx.sort_unstable();
        let mut fresh: Vec<Note> = Vec::new();
        for &i in &idx {
            let n = pat.notes[i];
            if n.len < parts {
                continue;
            }
            let piece = n.len / parts;
            let vel_at = |k: u32| -> u8 {
                if ramp {
                    let f = 0.45 + 0.55 * k as f32 / (parts - 1) as f32;
                    ((n.vel as f32 * f).round() as i32).clamp(1, 127) as u8
                } else {
                    n.vel
                }
            };
            pat.notes[i].len = piece;
            pat.notes[i].vel = vel_at(0);
            for k in 1..parts {
                let len = if k == parts - 1 { n.len - piece * (parts - 1) } else { piece };
                fresh.push(Note { start: n.start + piece * k, len, vel: vel_at(k), slide: false, ..n });
            }
        }
        let first = pat.notes.len();
        pat.notes.extend(fresh);
        for j in first..pat.notes.len() {
            self.selected.insert(j);
        }
    }

    /// Ableton "Reverse": mirror the notes in time
    fn reverse(&mut self, pat: &mut Pattern) {
        let idx = self.targets(pat);
        let lo = idx.iter().map(|&i| pat.notes[i].start).min().unwrap_or(0);
        let hi = idx.iter().map(|&i| pat.notes[i].start + pat.notes[i].len).max().unwrap_or(0);
        for i in idx {
            let n = &mut pat.notes[i];
            n.start = lo + hi - (n.start + n.len);
        }
    }

    /// Ableton "Invert" / FL "Flip": mirror the pitches
    fn flip(&mut self, pat: &mut Pattern) {
        let idx = self.targets(pat);
        let lo = idx.iter().map(|&i| pat.notes[i].pitch).min().unwrap_or(0) as i32;
        let hi = idx.iter().map(|&i| pat.notes[i].pitch).max().unwrap_or(0) as i32;
        for i in idx {
            pat.notes[i].pitch = (lo + hi - pat.notes[i].pitch as i32).clamp(0, 127) as u8;
        }
    }

    /// FL "Legato": every note lasts until the next one starts
    fn legato(&mut self, pat: &mut Pattern) {
        let mut idx = self.targets(pat);
        idx.sort_by_key(|&i| pat.notes[i].start);
        let total = pat.len_ticks();
        for k in 0..idx.len() {
            let s = pat.notes[idx[k]].start;
            let next = idx[k + 1..].iter().map(|&j| pat.notes[j].start).find(|&t| t > s).unwrap_or(total);
            pat.notes[idx[k]].len = (next - s).max(1);
        }
    }

    /// Ableton "x2 / :2": stretch positions and lengths around the first note
    fn stretch(&mut self, pat: &mut Pattern, f: f32) {
        let idx = self.targets(pat);
        let lo = idx.iter().map(|&i| pat.notes[i].start).min().unwrap_or(0);
        let total = pat.len_ticks();
        for i in idx {
            let n = &mut pat.notes[i];
            n.start = (lo + ((n.start - lo) as f32 * f).round() as u32).min(total.saturating_sub(1));
            n.len = ((n.len as f32 * f).round() as u32).clamp(1, total - n.start);
        }
    }

    /// open the stretch window on the selected notes (all notes of the track when nothing is selected)
    fn open_stretch(&mut self, pat: &Pattern) {
        self.stretch_snap = self.targets(pat).into_iter().map(|i| (i, pat.notes[i].start, pat.notes[i].len)).collect();
        self.stretch_f = 1.0;
        self.stretch_open = true;
    }

    /// set the notes of the window to `f` times their original timing (anchored at the first note)
    fn apply_stretch(&self, pat: &mut Pattern, f: f32) {
        let lo = self.stretch_snap.iter().map(|x| x.1).min().unwrap_or(0);
        let total = pat.len_ticks();
        for &(i, start, len) in &self.stretch_snap {
            if let Some(n) = pat.notes.get_mut(i) {
                n.start = (lo + ((start - lo) as f32 * f).round() as u32).min(total.saturating_sub(1));
                n.len = ((len as f32 * f).round() as u32).clamp(1, total - n.start);
            }
        }
    }

    /// Ableton "Double loop": the pattern gets twice as long and its notes repeat
    fn double_loop(&mut self, pat: &mut Pattern) {
        if pat.length_steps * 2 > crate::sequencer::MAX_STEPS as u32 {
            return;
        }
        let shift = pat.len_ticks();
        let copies: Vec<Note> = pat.notes.iter().map(|n| Note { start: n.start + shift, ..*n }).collect();
        pat.length_steps *= 2;
        pat.notes.extend(copies);
    }

    fn octave(&mut self, pat: &mut Pattern, d: i32) {
        for i in self.targets(pat) {
            pat.notes[i].pitch = (pat.notes[i].pitch as i32 + d).clamp(0, 127) as u8;
        }
    }

    /// FL "Randomize" velocity, deterministic per note so it does not flicker
    fn humanize(&mut self, pat: &mut Pattern) {
        for i in self.targets(pat) {
            let n = &mut pat.notes[i];
            let h = n.start.wrapping_mul(2654435761).wrapping_add(n.pitch as u32 * 40503) >> 7;
            let d = (h % 21) as i32 - 10;
            n.vel = (n.vel as i32 + d).clamp(20, 127) as u8;
        }
    }

    /// FL "Strum": chord notes that start together are spread out from the lowest to the highest
    fn strum(&mut self, pat: &mut Pattern) {
        let mut idx = self.targets(pat);
        idx.sort_by_key(|&i| (pat.notes[i].start, pat.notes[i].pitch));
        let total = pat.len_ticks();
        let mut k = 0;
        while k < idx.len() {
            let s = pat.notes[idx[k]].start;
            let mut e = k;
            while e < idx.len() && pat.notes[idx[e]].start == s {
                e += 1;
            }
            for (step, &i) in idx[k..e].iter().enumerate() {
                let n = &mut pat.notes[i];
                let off = (step as u32 * 6).min(n.len.saturating_sub(1));
                n.start = (s + off).min(total.saturating_sub(1));
                n.len -= off;
            }
            k = e;
        }
    }

    /// Alt+U "Chopper": every target note is cut into pieces of one grid step, each a little shorter than the step so it retriggers
    fn chop_snap(&mut self, pat: &mut Pattern) {
        let unit = self.snap_unit().max(6);
        let mut add: Vec<Note> = vec![];
        for i in self.targets(pat) {
            let n = pat.notes[i];
            let parts = n.len / unit;
            if parts < 2 {
                continue;
            }
            let gate = (unit * 3 / 4).max(1);
            pat.notes[i].len = gate;
            for k in 1..parts {
                add.push(Note { start: n.start + k * unit, len: gate, ..n });
            }
        }
        pat.notes.extend(add);
    }

    /// Alt+R "Randomizer": velocity and a little timing, different every time
    fn randomize(&mut self, pat: &mut Pattern) {
        let mut x = web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(1) | 1;
        let mut next = move || {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            x
        };
        let unit = self.snap_unit().max(6) as i64;
        let total = pat.len_ticks() as i64;
        for i in self.targets(pat) {
            let n = &mut pat.notes[i];
            n.vel = (50 + next() % 78) as u8;
            let jitter = (next() % (unit as u32 / 2 + 1)) as i64 - unit / 4;
            n.start = (n.start as i64 + jitter).clamp(0, total - 1) as u32;
        }
    }

    fn quantize(&mut self, pat: &mut Pattern) {
        let unit = if self.snap <= 1 { TICKS_PER_STEP } else { self.snap };
        let total = pat.len_ticks();
        let idx = self.targets(pat);
        for i in idx {
            let n = &mut pat.notes[i];
            let target = (((n.start + unit / 2) / unit) * unit).min(total.saturating_sub(1)) as f32;
            n.start = (n.start as f32 + (target - n.start as f32) * self.qstrength.clamp(0.0, 1.0)).round() as u32;
        }
    }

    fn nudge(&mut self, pat: &mut Pattern, dt: i64, dp: i32) {
        let total = pat.len_ticks() as i64;
        for i in self.work_sel(pat) {
            let n = &mut pat.notes[i];
            n.start = (n.start as i64 + dt).clamp(0, total - 1) as u32;
            n.pitch = (n.pitch as i32 + dp).clamp(0, 127) as u8;
        }
    }

    /// Put a chord at the cursor (advancing it by `len`) and select the new notes.
    fn insert_chord(&mut self, pat: &mut Pattern, pitches: &[u8], len: u32) {
        let total = pat.len_ticks();
        let at = self.cursor.unwrap_or(0).min(total.saturating_sub(1));
        let len = len.min(total - at).max(1);
        self.selected.clear();
        for &pitch in pitches {
            pat.notes.push(Note { start: at, len, pitch, vel: 100, track: self.track, slide: false, mute: false, pan: 0, chance: 100 });
            self.selected.insert(pat.notes.len() - 1);
        }
        self.cursor = Some(if at + len >= total { 0 } else { at + len });
    }

    /// Put a whole progression (one chord per bar) at the cursor, in the key of the project; the pattern grows to hold it.
    fn insert_progression(&mut self, pat: &mut Pattern, chords: &[(usize, usize)], root: u8, scale: usize) {
        let bar = crate::sequencer::TICKS_PER_BAR;
        let start = self.cursor.unwrap_or(0) / bar * bar;
        let need_steps = ((start / bar + chords.len() as u32) * 16).max(16);
        if let Some(l) = [16u32, 32, 64, 128, 256].into_iter().find(|l| *l >= need_steps) {
            pat.length_steps = pat.length_steps.max(l);
        }
        self.cursor = Some(start);
        self.selected.clear();
        for &(degree, count) in chords {
            let pitches = theory::degree_chord(root, scale, degree, count);
            let at = self.cursor.unwrap_or(0);
            if at + bar > pat.len_ticks() {
                break;
            }
            for &pitch in &pitches {
                pat.notes.push(Note { start: at, len: bar, pitch, vel: 100, track: self.track, slide: false, mute: false, pan: 0, chance: 100 });
                self.selected.insert(pat.notes.len() - 1);
            }
            self.cursor = Some(at + bar);
        }
    }

    /// Move the selection (or every note of the track) onto the nearest scale tones.
    fn fit_to_scale(&mut self, pat: &mut Pattern, root: u8, scale: usize) {
        let idx = self.targets(pat);
        for i in idx {
            pat.notes[i].pitch = theory::snap_to_scale(root, scale, pat.notes[i].pitch);
        }
    }

    fn toggle_slide(&mut self, pat: &mut Pattern) {
        let idx = self.work_sel(pat);
        let on = idx.iter().any(|&i| !pat.notes[i].slide);
        for i in idx {
            pat.notes[i].slide = on;
        }
    }
}

fn track_color(t: Track) -> Color32 {
    match t {
        Track::Synth => TEAL(),
        Track::Bass808 => CYAN(),
        Track::Wavetable => SKY,
        Track::Analog => lane_col(8),
        Track::Drums => lane_col(2),
        Track::Sampler => lane_col(6),
    }
}

/// Keys belong to the area the user clicked last: the piano roll and the Playlist can be on screen together, and a Ctrl+A / Delete
/// in the roll must not also select / delete clips.
const KB_AREA: &str = "kb_area_roll";

impl RollState {
    /// forget the selected notes (the Playlist has the keyboard now)
    pub fn clear_selection(&mut self) {
        self.selected.clear();
    }
}

pub fn roll_has_focus(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<bool>(egui::Id::new(KB_AREA))).unwrap_or(false)
}

pub fn give_focus_to_roll(ctx: &egui::Context, on: bool) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new(KB_AREA), on));
}

pub fn show(ui: &mut Ui, project: &mut Project, st: &mut RollState, shared: &Shared) {
    // a press inside this panel gives it the keyboard, a press anywhere else takes it away; the full-window roll always has it
    if !st.compact {
        give_focus_to_roll(ui.ctx(), true);
    } else if let Some(pos) = ui.input(|i| if i.pointer.any_pressed() { i.pointer.interact_pos() } else { None }) {
        give_focus_to_roll(ui.ctx(), ui.max_rect().contains(pos));
    }
    let has_keys = roll_has_focus(ui.ctx());
    let cur = project.current_pattern;
    let (kr, ks) = (project.key_root, project.key_scale);
    if st.last_track != Some(st.track) {
        st.last_track = Some(st.track);
        st.selected.clear();
        st.scroll_pitch = Some(match st.track {
            Track::Bass808 => 45,
            Track::Drums => 40,
            _ => 72,
        });
    }
    if st.last_pattern != Some(cur) {
        st.ghost_pat = st.last_pattern.filter(|g| *g != cur && *g < project.patterns.len());
        st.last_pattern = Some(cur);
        st.zoom = ((ui.available_width() - KEYW - 40.0) / project.patterns[cur].length_steps.max(1) as f32).clamp(10.0, 160.0);
        st.selected.clear();
        st.cursor = None;
        st.drag = None;
    }

    // The notes of a Playlist clip belong to the track that plays it. If the plugin of the track was changed after the notes were
    // written, they would show greyed out and could not be touched: they are handed over to the track's current instrument.
    if st.lock_track && project.patterns[cur].clip_only {
        let t = st.track;
        for n in project.patterns[cur].notes.iter_mut() {
            if n.track != t {
                n.track = t;
            }
        }
    }

    // ---------------- toolbar ----------------
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let lane = st.lane;
        if lane < project.arrangement.tracks.len() {
            let clips = project.arrangement.clips.iter().filter(|c| c.lane == lane).count();
            let t = &mut project.arrangement.tracks[lane];
            let (ar, aresp) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::click());
            ui.painter().circle_filled(ar.center(), 6.5, if t.arm { RED } else { RED.linear_multiply(0.55) });
            if t.arm {
                ui.painter().circle_stroke(ar.center(), 9.5, Stroke::new(1.5, RED));
            }
            if aresp.on_hover_text("Arm for recording").clicked() {
                t.arm = !t.arm;
            }
            ui.toggle_value(&mut t.mute, "M").on_hover_text("Mute the track");
            ui.toggle_value(&mut t.solo, "S").on_hover_text("Solo the track");
            ui.label(RichText_dim("Out"));
            super::mixer_ui::output_picker(ui, "roll_out", &mut t.output);
            ui.label(RichText_dim(&format!("{clips} clip(s)")));
        } else {
            let is_clip = project.patterns[cur].clip_only;
            let (badge, bcol) = if is_clip { ("MIDI CLIP", lane_col(3)) } else { ("PATTERN", TEAL()) };
            ui.label(egui::RichText::new(format!(" {badge} ")).size(11.0).strong().color(Color32::WHITE).background_color(bcol));
            ui.label(egui::RichText::new(project.patterns[cur].name.clone()).strong().color(bcol));
        }
        if !st.lock_track {
            ui.label(RichText_dim("no plugin on this track - drop one from the Browser"));
        }
        ui.separator();
        // tools: select, draw, cut, magnet, note preview
        if tool_btn(ui, 0, st.tool == Tool::Select, "Select: drag on empty space box-selects, double-click adds a note (Ctrl+3)") { st.tool = Tool::Select; }
        if tool_btn(ui, 1, st.tool == Tool::Draw, "Draw: click adds notes, Shift+drag box-selects (Ctrl+1)") { st.tool = Tool::Draw; }
        if tool_btn(ui, 2, st.tool == Tool::Cut, "Cut: click a note to split it at the grid position (Ctrl+2)") { st.tool = Tool::Cut; }
        if tool_btn(ui, 3, st.magnet, "Magnet: snap to the edges of neighbouring notes (Ctrl+M)") { st.magnet = !st.magnet; }
        if tool_btn(ui, 4, st.preview_on, "Hear the notes while you draw and click them") { st.preview_on = !st.preview_on; }
        ui.separator();
        ui.label(RichText_dim("Snap"));
        egui::ComboBox::from_id_salt("snap").width(60.0).selected_text(snap_name(st.snap)).show_ui(ui, |ui| {
            for s in [96u32, 48, 24, 12, 6, 1] {
                ui.selectable_value(&mut st.snap, s, snap_name(s));
            }
        });
        ui.label(RichText_dim("Length"));
        egui::ComboBox::from_id_salt("nlen").width(60.0).selected_text(len_name(st.note_len)).show_ui(ui, |ui| {
            for s in [6u32, 12, 24, 48, 96, 192, 384, 768] {
                ui.selectable_value(&mut st.note_len, s, len_name(s));
            }
        });
        ui.label(RichText_dim("Chord"));
        egui::ComboBox::from_id_salt("chord").width(110.0).selected_text(CHORDS[st.chord].0).show_ui(ui, |ui| {
            for (i, (n, _)) in CHORDS.iter().enumerate() {
                ui.selectable_value(&mut st.chord, i, *n);
            }
        });
        // Slide mode (like FL's slide notes): notes written while it is on glide from the sounding note to their own pitch
        {
            let on = st.slide_new;
            if ui.add(egui::Button::new(egui::RichText::new("Slide").color(if on { Color32::WHITE } else { TEXT() })).fill(if on { TEAL() } else { PANEL3() })).on_hover_text("Slide mode: notes you draw now are slide notes - they glide from the note before them to their own pitch, over their own length. Works on every plugin. Double-click a note to turn its slide on or off.").clicked() {
                st.slide_new = !st.slide_new;
            }
        }
        ui.separator();
        ui.label(RichText_dim("V"));
        ui.add(egui::Slider::new(&mut st.row_h, 12.0..=56.0).show_value(false)).on_hover_text("Note height (Alt+wheel)");
        ui.label(RichText_dim("H"));
        ui.add(egui::Slider::new(&mut st.zoom, 10.0..=160.0).show_value(false)).on_hover_text("Horizontal zoom (Ctrl+wheel)");
        if ui.button("Fit").on_hover_text("Zoom so that the whole pattern fills the window").clicked() {
            st.zoom = ((ui.available_width() - KEYW - 40.0) / project.patterns[cur].length_steps.max(1) as f32).clamp(10.0, 160.0);
        }
        ui.menu_button("Notes", |ui| {
            let (kr, ks) = (project.key_root, project.key_scale);
            let cur_pat = &mut project.patterns[cur];
            ui.label(RichText_dim("applies to the selected notes (all notes when none is selected)"));
            if ui.button("Select all   Ctrl+A").clicked() {
                st.select_all(cur_pat);
                ui.close_menu();
            }
            if ui.button("Quantize   Ctrl+Q").on_hover_text("Snap the starts to the grid").clicked() {
                st.quantize(cur_pat);
                ui.close_menu();
            }
            if ui.button("Transpose +1 semitone").clicked() {
                st.nudge(cur_pat, 0, 1);
                ui.close_menu();
            }
            if ui.button("Transpose -1 semitone").clicked() {
                st.nudge(cur_pat, 0, -1);
                ui.close_menu();
            }
            if ui.button("Move later by one step").clicked() {
                st.nudge(cur_pat, TICKS_PER_STEP as i64, 0);
                ui.close_menu();
            }
            if ui.button("Move earlier by one step").clicked() {
                st.nudge(cur_pat, -(TICKS_PER_STEP as i64), 0);
                ui.close_menu();
            }
            if ui.button("Fit to the project scale").on_hover_text("Move every note to the nearest note of the key").clicked() {
                st.fit_to_scale(cur_pat, kr, ks);
                ui.close_menu();
            }
            if ui.button("Toggle slide (808)").clicked() {
                st.toggle_slide(cur_pat);
                ui.close_menu();
            }
            if ui.button("Duplicate   Ctrl+D").clicked() {
                st.duplicate(cur_pat);
                ui.close_menu();
            }
            ui.separator();
            ui.label(RichText_dim("chop: cut each note into equal parts (Alt+2 / 3 / 4 / 6 / 8)"));
            ui.horizontal(|ui| {
                for n in [2u32, 3, 4, 6, 8] {
                    if ui.button(format!("Chop {n}")).on_hover_text(format!("Alt+{n}")).clicked() {
                        st.chop(cur_pat, n, false);
                        ui.close_menu();
                    }
                }
            });
            ui.label(RichText_dim("roll: chop with a build-up in velocity (Alt+Shift+2 / 4 / 8)"));
            ui.horizontal(|ui| {
                for n in [4u32, 8, 16] {
                    if ui.button(format!("Roll x{n}")).clicked() {
                        st.chop(cur_pat, n, true);
                        ui.close_menu();
                    }
                }
            });
            if ui.button("Delete   Del").clicked() {
                st.delete_selected(cur_pat);
                ui.close_menu();
            }
            ui.separator();
            ui.checkbox(&mut st.show_ghost, "Ghost notes of the previous clip").on_hover_text("The notes of the clip you had open before show through, to write a new part against them");
        });
        ui.menu_button("Tools", |ui| {
            let cur_pat = &mut project.patterns[cur];
            if ui.button("Reverse").on_hover_text("Mirror the notes in time").clicked() {
                st.reverse(cur_pat);
                ui.close_menu();
            }
            if ui.button("Flip pitches").on_hover_text("Mirror the notes upside down").clicked() {
                st.flip(cur_pat);
                ui.close_menu();
            }
            if ui.button("Legato").on_hover_text("Each note lasts until the next one").clicked() {
                st.legato(cur_pat);
                ui.close_menu();
            }
            if ui.button("Stretch...").on_hover_text("Window with a slider: select the notes, then stretch them in time (x2, x3, x4 ...)").clicked() {
                st.open_stretch(cur_pat);
                ui.close_menu();
            }
            if ui.button("Stretch x2").clicked() {
                st.stretch(cur_pat, 2.0);
                ui.close_menu();
            }
            if ui.button("Stretch /2").clicked() {
                st.stretch(cur_pat, 0.5);
                ui.close_menu();
            }
            if ui.button("Double loop").on_hover_text("Pattern twice as long, notes repeated").clicked() {
                st.double_loop(cur_pat);
                ui.close_menu();
            }
            if ui.button("Octave up").clicked() {
                st.octave(cur_pat, 12);
                ui.close_menu();
            }
            if ui.button("Octave down").clicked() {
                st.octave(cur_pat, -12);
                ui.close_menu();
            }
            if ui.button("Strum chords").on_hover_text("Spread chord notes from low to high").clicked() {
                st.strum(cur_pat);
                ui.close_menu();
            }
            if ui.button("Humanize velocity").clicked() {
                st.humanize(cur_pat);
                ui.close_menu();
            }
        });
        if st.stretch_open {
            let mut open = true;
            let mut keep = false;
            let mut cancel = false;
            let n_notes = st.stretch_snap.len();
            egui::Window::new("Stretch").id(egui::Id::new("roll_stretch_window")).open(&mut open).collapsible(false).resizable(false).default_width(300.0).show(ui.ctx(), |ui| {
                ui.label(RichText_dim(&format!("{n_notes} notes: select them in the roll, then pick a factor")));
                ui.add_space(4.0);
                let mut f = st.stretch_f;
                ui.horizontal_wrapped(|ui| {
                    for (label, v) in [("x1/4", 0.25f32), ("x1/2", 0.5), ("x1", 1.0), ("x2", 2.0), ("x3", 3.0), ("x4", 4.0)] {
                        if ui.selectable_label((f - v).abs() < 1e-3, label).clicked() { f = v; }
                    }
                });
                ui.add(egui::Slider::new(&mut f, 0.25..=4.0).logarithmic(true).text("x").fixed_decimals(2));
                if (f - st.stretch_f).abs() > 1e-4 {
                    st.stretch_f = f;
                    st.apply_stretch(&mut project.patterns[cur], f);
                }
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if ui.button("Done").clicked() { keep = true; }
                    if ui.button("Cancel").clicked() { cancel = true; }
                });
            });
            if cancel {
                st.apply_stretch(&mut project.patterns[cur], 1.0);
            }
            if !open || keep || cancel {
                st.stretch_open = false;
                st.stretch_snap.clear();
            }
        }
        if ui.button("Clean").on_hover_text("Remove duplicate notes and trim overlaps of the same pitch").clicked() {
            st.clean(&mut project.patterns[cur]);
        }
        if st.compact {
            ui.separator();
            ui.toggle_value(&mut st.show_more, "Tools bar").on_hover_text("Select all, copy / paste, quantize, scale and chord helpers");
        }
        ui.separator();
        ui.toggle_value(&mut st.show_auto, "Panel").on_hover_text("The Note / Channel / Track window (a floating window you can move; closed by default)");
    });
    ui.add_space(4.0);
    if !st.compact || st.show_more {
    egui::TopBottomPanel::bottom("roll_bottom")
        .resizable(false)
        .frame(egui::Frame::none().fill(SIDE()).stroke(Stroke::new(border_w(), border())).inner_margin(egui::Margin::symmetric(10.0, 8.0)))
        .show_inside(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.checkbox(&mut st.suggest, "Suggest next note").on_hover_text("A ghost shows where the melody could continue. Tab places it.");
                ui.menu_button("Hotkeys", |ui| {
                    for line in [
                        "Ctrl+1 / 2 / 3  Draw / Cut / Select",
                        "Ctrl+M  magnet",
                        "Z S X D C V G B H N J M  piano keys (like FL), Q 2 W 3 E R 5 T 6 Y 7 U  the octave above",
                        "- / =  typing piano octave down / up",
                        "Tab  place suggested note",
                        "Ctrl+A / C / X / V / D  select, copy, cut, paste, duplicate",
                        "Alt+2 / 3 / 4 / 6 / 8  chop the notes into parts - Alt+Shift+2 / 3 / 4  roll with a build-up",
                        "Del  delete - Ctrl+Q  quantize",
                        "Arrows  nudge - Shift+Up/Down  octave",
                        "Ctrl+R  reverse - Ctrl+I  flip pitches",
                        "Ctrl+L  legato - Ctrl+K  strum",
                        "Ctrl+B  double loop",
                        "Ctrl+= / Ctrl+-  stretch x2 / :2",
                        "Ctrl+H  humanize velocity",
                        "Ctrl+wheel / Alt+wheel  zoom width / height",
                        "Ctrl+Z / Ctrl+Y  undo / redo",
                    ] {
                        ui.label(RichText_dim(line));
                    }
                });
                ui.separator();
                let pat = &mut project.patterns[cur];
                let n_sel = st.sel(pat).len();
                if ui.button("Select all").on_hover_text("Ctrl+A").clicked() {
                    st.select_all(pat);
                }
                if ui.add_enabled(n_sel > 0, egui::Button::new("Duplicate")).on_hover_text("Ctrl+D - or Ctrl/Alt+drag a note").clicked() {
                    st.duplicate(pat);
                }
                if ui.add_enabled(n_sel > 0, egui::Button::new("Copy")).on_hover_text("Ctrl+C").clicked() {
                    st.copy(pat);
                }
                if ui.add_enabled(!st.clip.is_empty(), egui::Button::new("Paste")).on_hover_text("Ctrl+V - pastes at the cursor line").clicked() {
                    st.paste(pat);
                }
                if ui.add_enabled(n_sel > 0, egui::Button::new("Delete")).on_hover_text("Delete / Backspace").clicked() {
                    st.delete_selected(pat);
                }
                if ui.button("Quantize").on_hover_text("Ctrl+Q - snap note starts to the grid (selection, or all notes)").clicked() {
                    st.quantize(pat);
                }
                if st.track == Track::Bass808 && ui.add_enabled(n_sel > 0, egui::Button::new("Toggle slide")).clicked() {
                    st.toggle_slide(pat);
                }
                if ui.button("Clear track").clicked() {
                    let t = st.track;
                    pat.notes.retain(|n| n.track != t);
                    st.selected.clear();
                }
                ui.label(RichText_dim(&format!("{n_sel} selected")));
                ui.separator();
                section_label(ui, "SCALE");
                egui::ComboBox::from_id_salt("key_root").width(52.0).selected_text(theory::pc_name(kr, ks, kr)).show_ui(ui, |ui| {
                    for r in 0..12u8 {
                        ui.selectable_value(&mut project.key_root, r, theory::pc_name(r, ks, r));
                    }
                });
                egui::ComboBox::from_id_salt("key_scale").width(130.0).selected_text(theory::SCALES[ks].0).show_ui(ui, |ui| {
                    for (i, (n, _)) in theory::SCALES.iter().enumerate() {
                        ui.selectable_value(&mut project.key_scale, i, *n);
                    }
                });
                ui.checkbox(&mut st.highlight, "Highlight").on_hover_text("Tint the rows of the key; foreign notes are dimmed");
                ui.checkbox(&mut st.fold, "Fold").on_hover_text("Show only the rows that belong to the key (rows that already hold notes stay)");
                ui.checkbox(&mut st.snap_scale, "Snap to scale").on_hover_text("New and moved notes jump to the nearest scale tone; Up/Down arrows step through the scale");
                ui.checkbox(&mut st.diatonic, "Diatonic chords").on_hover_text("With a chord selected above, stack thirds inside the key instead of fixed intervals");
                if ui.selectable_label(st.circle_open, "Circle of 5ths").on_hover_text("Pick a key, play and insert the chords of the key").clicked() {
                    st.circle_open = !st.circle_open;
                }
                let pat = &mut project.patterns[cur];
                ui.menu_button("Progressions", |ui| {
                    ui.label(RichText_dim("one chord per bar, in the key of the project, at the cursor"));
                    let mut genre = "";
                    for (g, name, chords) in theory::PROGRESSIONS {
                        if *g != genre {
                            genre = g;
                            ui.separator();
                            ui.label(egui::RichText::new(*g).strong().color(TEAL()));
                        }
                        if ui.button(*name).clicked() {
                            st.insert_progression(pat, chords, kr, ks);
                            ui.close_menu();
                        }
                    }
                });
                if ui.button("Fit to scale").on_hover_text("Snap the selection (or all notes) onto the key").clicked() {
                    st.fit_to_scale(pat, kr, ks);
                }
                let n_sel = st.work_sel(pat).len();
                for (label, d) in [("-12", -12), ("-1", -1), ("+1", 1), ("+12", 12)] {
                    if ui.add_enabled(n_sel > 0, egui::Button::new(label).small()).on_hover_text("Transpose the selection in semitones").clicked() {
                        st.nudge(pat, 0, d);
                    }
                }
                let pitches: Vec<u8> = st.work_sel(pat).iter().map(|&i| pat.notes[i].pitch).collect();
                if let Some(name) = theory::chord_name(&pitches, kr, ks) {
                    ui.label(egui::RichText::new(format!("Chord: {name}")).color(TEAL()).strong());
                }
            });
            ui.label(RichText_dim("click: add - click/drag note: select & move - Shift: add to selection - drag right edge: resize - Ctrl/Alt+drag: duplicate - right-click: delete - arrows: nudge - Ctrl+Q quantize - drag on the ruler: range for the functions (right-click the ruler clears it)"));
        });
    }
    // the Note / Channel / Track panel is a floating window now: it opens from the "Panel" button and does not take room from the grid
    if st.show_auto {
        let mut open = true;
        let right = ui.max_rect().right();
        egui::Window::new("Note / Channel / Track")
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(250.0)
            .default_height(420.0)
            .default_pos(pos2(right - 270.0, ui.max_rect().top() + 40.0))
            .show(ui.ctx(), |ui| {
                egui::ScrollArea::vertical().id_salt("roll_auto_scroll").auto_shrink([false, false]).show(ui, |ui| {
                    inspector(ui, st, project, cur);
                });
            });
        if !open {
            st.show_auto = false;
        }
    }

    // ---------------- hotkeys ----------------
    if has_keys && !ui.ctx().wants_keyboard_input() {
        let pat = &mut project.patterns[cur];
        let snap = st.snap_unit() as i64;
        let (c_a, c_c, c_x, c_v, c_d, del, bs, q, esc) = ui.input_mut(|i| {
            (
                i.consume_key(Modifiers::COMMAND, Key::A),
                i.consume_key(Modifiers::COMMAND, Key::C),
                i.consume_key(Modifiers::COMMAND, Key::X),
                i.consume_key(Modifiers::COMMAND, Key::V),
                i.consume_key(Modifiers::COMMAND, Key::D),
                i.consume_key(Modifiers::NONE, Key::Delete),
                i.consume_key(Modifiers::NONE, Key::Backspace),
                i.consume_key(Modifiers::COMMAND, Key::Q),
                i.consume_key(Modifiers::NONE, Key::Escape),
            )
        });
        if c_a {
            st.select_all(pat);
        }
        if c_c || c_x {
            st.copy(pat);
        }
        if c_x || del || bs {
            st.delete_selected(pat);
        }
        if c_v {
            st.paste(pat);
        }
        if c_d {
            st.duplicate(pat);
        }
        if q {
            st.quantize(pat);
        }
        if esc {
            st.selected.clear();
        }
        // tools and note operations on keys that are not piano keys: 1/2/3 tools, M magnet, Tab accepts the suggestion,
        // Ctrl+R reverse, Ctrl+L legato, Ctrl+I flip, Ctrl+B double loop, Ctrl+= / Ctrl+- stretch, Ctrl+H humanize, Ctrl+K strum
        let (k1, k2, k3, km, tab, c_r, c_l, c_i, c_b, c_pl, c_mi, c_h, c_k) = ui.input_mut(|i| {
            (
                i.consume_key(Modifiers::COMMAND, Key::Num1),
                i.consume_key(Modifiers::COMMAND, Key::Num2),
                i.consume_key(Modifiers::COMMAND, Key::Num3),
                i.consume_key(Modifiers::COMMAND, Key::M),
                i.consume_key(Modifiers::NONE, Key::Tab),
                i.consume_key(Modifiers::COMMAND, Key::R),
                i.consume_key(Modifiers::COMMAND, Key::L),
                i.consume_key(Modifiers::COMMAND, Key::I),
                i.consume_key(Modifiers::COMMAND, Key::B),
                i.consume_key(Modifiers::COMMAND, Key::Equals),
                i.consume_key(Modifiers::COMMAND, Key::Minus),
                i.consume_key(Modifiers::COMMAND, Key::H),
                i.consume_key(Modifiers::COMMAND, Key::K),
            )
        });
        if k1 {
            st.tool = Tool::Draw;
        }
        if k2 {
            st.tool = Tool::Cut;
        }
        if k3 {
            st.tool = Tool::Select;
        }
        if km {
            st.magnet = !st.magnet;
        }
        if c_r {
            st.reverse(pat);
        }
        if c_l {
            st.legato(pat);
        }
        if c_i {
            st.flip(pat);
        }
        if c_b {
            // Ctrl+B: duplicate the selection to the right (the whole pattern is doubled when nothing is selected)
            if st.sel(pat).is_empty() {
                st.double_loop(pat);
            } else {
                st.duplicate(pat);
            }
        }
        if c_pl {
            st.stretch(pat, 2.0);
        }
        if c_mi {
            st.stretch(pat, 0.5);
        }
        if c_h {
            st.humanize(pat);
        }
        if c_k {
            st.strum(pat);
        }
        // Alt+2 / 3 / 4 / 6 / 8 chop the selected notes, Alt+Shift+2 / 4 / 8 make a rolling build-up
        let mut chop_parts: Option<(u32, bool)> = None;
        ui.input_mut(|i| {
            for (key, n) in [(Key::Num2, 2u32), (Key::Num3, 3), (Key::Num4, 4), (Key::Num6, 6), (Key::Num8, 8)] {
                if i.consume_key(Modifiers::ALT | Modifiers::SHIFT, key) {
                    chop_parts = Some((n * 2, true));
                } else if i.consume_key(Modifiers::ALT, key) {
                    chop_parts = Some((n, false));
                }
            }
        });
        if let Some((n, ramp)) = chop_parts {
            st.chop(pat, n, ramp);
        }
        // Alt+U Chopper, Alt+R Randomizer, Alt+S Strum, Alt+Q Quantize
        let (a_u, a_r, a_s, a_q) = ui.input_mut(|i| {
            (i.consume_key(Modifiers::ALT, Key::U), i.consume_key(Modifiers::ALT, Key::R), i.consume_key(Modifiers::ALT, Key::S), i.consume_key(Modifiers::ALT, Key::Q))
        });
        if a_u {
            st.chop_snap(pat);
        }
        if a_r {
            st.randomize(pat);
        }
        if a_s {
            st.strum(pat);
        }
        if a_q {
            st.quantize(pat);
        }
        if tab {
            if let Some((s, l, p)) = st.suggest_next(pat, kr, ks) {
                pat.notes.push(Note { start: s, len: l, pitch: p, vel: 100, track: st.track, slide: st.slide_new, mute: false, pan: 0, chance: 100 });
                st.selected.clear();
                st.selected.insert(pat.notes.len() - 1);
                st.cursor = Some(s + l);
                if st.audible && st.preview_on { shared.send(Cmd::NoteOn { track: st.track, pitch: p, vel: 100 }); }
                st.preview.push((st.track, p));
            }
        }
        let (l, r, u, d, su, sd) = ui.input_mut(|i| {
            (
                i.consume_key(Modifiers::NONE, Key::ArrowLeft),
                i.consume_key(Modifiers::NONE, Key::ArrowRight),
                i.consume_key(Modifiers::NONE, Key::ArrowUp),
                i.consume_key(Modifiers::NONE, Key::ArrowDown),
                i.consume_key(Modifiers::SHIFT, Key::ArrowUp),
                i.consume_key(Modifiers::SHIFT, Key::ArrowDown),
            )
        });
        let dt = if l { -snap } else if r { snap } else { 0 };
        let dp: i32 = if su { 12 } else if sd { -12 } else if u { 1 } else if d { -1 } else { 0 };
        if st.snap_scale && dp.abs() == 1 {
            for i in st.sel(pat) {
                pat.notes[i].pitch = theory::step_in_scale(kr, ks, pat.notes[i].pitch, dp);
            }
            if dt != 0 {
                st.nudge(pat, dt, 0);
            }
        } else if dt != 0 || dp != 0 {
            st.nudge(pat, dt, dp);
        }
    }

    // ---------------- grid ----------------
    let total_ticks = project.patterns[cur].len_ticks();
    let ghost_notes: Vec<Note> = match st.ghost_pat {
        Some(g) if st.show_ghost && g < project.patterns.len() && g != cur => project.patterns[g].notes.clone(),
        _ => Vec::new(),
    };
    st.zoom = st.zoom.clamp(10.0, 160.0);
    st.row_h = st.row_h.clamp(12.0, 56.0);
    let rh = st.row_h;
    let tw = st.zoom / TICKS_PER_STEP as f32;
    let visible: Vec<u8> = {
        let notes = &project.patterns[cur].notes;
        if st.track == Track::Drums {
            (crate::piano_roll::DRUM_BASE..crate::piano_roll::DRUM_BASE + crate::sequencer::NUM_DRUMS as u8).rev().collect()
        } else {
        (PMIN..=PMAX).rev().filter(|&p| !st.fold || theory::in_scale(kr, ks, p) || notes.iter().any(|n| n.track == st.track && n.pitch == p)).collect()
        }
    };
    let mut ridx = [0usize; 128];
    for (i, &p) in visible.iter().enumerate() {
        ridx[p as usize] = i;
    }
    let rows = visible.len().max(1);
    if let Some(c) = st.scroll_pitch.take() {
        st.scroll_to = Some(ridx[c as usize] as f32 * rh - 120.0);
    }
    let grid_w = total_ticks as f32 * tw;
    // the roll fills the available width; whatever lies past the end of the pattern is shaded
    let full_w = (KEYW + grid_w).max(ui.available_width() - 4.0);
    let playing = shared.playing.load(Ordering::Relaxed) && !project.song_mode;
    let pos_tick = shared.position.load(Ordering::Relaxed);

    // bar / beat ruler (synced with the grid, click to move the cursor)
    {
        let steps = project.patterns[cur].length_steps;
        egui::ScrollArea::horizontal()
            .id_salt("ruler_scroll")
            .horizontal_scroll_offset(st.scroll_x)
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .drag_to_scroll(false)
            .show(ui, |ui| {
                let (rect, resp) = ui.allocate_exact_size(vec2(full_w, 20.0), Sense::click_and_drag());
                let gx = rect.left() + KEYW;
                let p = ui.painter_at(rect);
                p.rect_filled(rect, 0.0, PANEL2());
                // drag on the ruler: the range the note functions work on (like the loop range in the Playlist); right-click clears it
                let ctrl = ui.input(|i| i.modifiers.command);
                let edge_at = |x: f32, r: (u32, u32)| -> u8 {
                    let (x0, x1) = (gx + r.0 as f32 * tw, gx + r.1.min(total_ticks) as f32 * tw);
                    if (x - x0).abs() < 7.0 { 2 } else if (x - x1).abs() < 7.0 { 3 } else if ctrl && x > x0 && x < x1 { 1 } else { 0 }
                };
                if let (Some(r), Some(h)) = (st.range, resp.hover_pos()) {
                    match edge_at(h.x, r) {
                        2 | 3 => ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal),
                        1 => ui.ctx().set_cursor_icon(egui::CursorIcon::Grab),
                        _ => {}
                    }
                }
                if resp.drag_started() {
                    st.ruler_drag = None;
                    if let (Some(r), Some(o)) = (st.range, ui.input(|i| i.pointer.press_origin())) {
                        let mode = edge_at(o.x, r);
                        if mode != 0 {
                            let tr = st.track;
                            let orig = project.patterns[cur].notes.iter().enumerate().filter(|(_, n)| n.track == tr && n.start >= r.0 && n.start < r.1).map(|(i, n)| (i, n.start, n.len)).collect();
                            st.ruler_drag = Some((mode, r, orig));
                        }
                    }
                }
                if resp.drag_stopped() {
                    st.ruler_drag = None;
                }
                if resp.dragged() {
                    if let (Some(o), Some(c)) = (ui.input(|i| i.pointer.press_origin()), resp.interact_pointer_pos()) {
                        let u = st.snap_unit();
                        let tick = |x: f32| (((x - gx) / tw).max(0.0) as u32 / u * u).min(total_ticks);
                        if let Some((mode, (lo, hi), orig)) = st.ruler_drag.clone() {
                            let pat = &mut project.patterns[cur];
                            let (nlo, nhi) = match mode {
                                1 => {
                                    let d = tick(c.x) as i64 - tick(o.x) as i64;
                                    let nlo = (lo as i64 + d).clamp(0, (total_ticks - (hi - lo).min(total_ticks)) as i64) as u32;
                                    (nlo, nlo + (hi - lo))
                                }
                                2 => (tick(c.x).min(hi.saturating_sub(u)), hi),
                                _ => (lo, (tick(c.x) + u).max(lo + u).min(total_ticks.max(lo + u))),
                            };
                            let scale = (nhi - nlo) as f32 / (hi - lo).max(1) as f32;
                            for (i, s0, l0) in &orig {
                                if let Some(n) = pat.notes.get_mut(*i) {
                                    if mode == 1 {
                                        n.start = (*s0 as i64 + nlo as i64 - lo as i64).max(0) as u32;
                                    } else {
                                        let rel = if mode == 2 { (*s0 as f32 - hi as f32) * scale + nhi as f32 } else { (*s0 as f32 - lo as f32) * scale + nlo as f32 };
                                        n.start = rel.round().max(0.0) as u32;
                                        n.len = ((*l0 as f32 * scale).round() as u32).max(1);
                                    }
                                    n.start = n.start.min(total_ticks.saturating_sub(1));
                                    n.len = n.len.min(total_ticks - n.start).max(1);
                                }
                            }
                            st.range = Some((nlo, nhi));
                        } else {
                            let (a, b) = (tick(o.x), tick(c.x));
                            let (lo, hi) = (a.min(b), a.max(b) + u);
                            st.range = Some((lo, hi));
                            let tr = st.track;
                            st.selected = project.patterns[cur].notes.iter().enumerate().filter(|(_, n)| n.track == tr && n.start >= lo && n.start < hi).map(|(i, _)| i).collect();
                        }
                    }
                }
                if resp.secondary_clicked() {
                    st.range = None;
                }
                if let Some((a, b)) = st.range {
                    let (x0, x1) = (gx + a as f32 * tw, gx + b.min(total_ticks) as f32 * tw);
                    p.rect_filled(Rect::from_min_max(pos2(x0, rect.top()), pos2(x1, rect.bottom())), 0.0, TEAL().linear_multiply(0.28));
                    p.rect_filled(Rect::from_min_max(pos2(x0, rect.bottom() - 3.0), pos2(x1, rect.bottom())), 0.0, TEAL());
                }
                p.line_segment([pos2(rect.left(), rect.bottom() - 0.5), pos2(rect.right(), rect.bottom() - 0.5)], Stroke::new(1.0, border()));
                for s in 0..=steps {
                    let x = gx + s as f32 * st.zoom;
                    if s % 16 == 0 {
                        p.line_segment([pos2(x, rect.top()), pos2(x, rect.bottom())], Stroke::new(1.0, LIGHT()));
                        if s < steps {
                            p.text(pos2(x + 4.0, rect.center().y), Align2::LEFT_CENTER, format!("{}", s / 16 + 1), FontId::monospace(11.0), TEXT());
                        }
                    } else if s % 4 == 0 {
                        p.line_segment([pos2(x, rect.bottom() - 7.0), pos2(x, rect.bottom())], Stroke::new(1.0, MID()));
                    }
                }
                if let Some(c) = st.cursor {
                    let x = gx + c as f32 * tw;
                    p.add(egui::Shape::convex_polygon(vec![pos2(x - 5.0, rect.top()), pos2(x + 5.0, rect.top()), pos2(x, rect.top() + 9.0)], TEAL(), Stroke::NONE));
                }
                if playing {
                    let x = gx + (pos_tick % total_ticks) as f32 * tw;
                    p.add(egui::Shape::convex_polygon(vec![pos2(x - 5.0, rect.top()), pos2(x + 5.0, rect.top()), pos2(x, rect.top() + 9.0)], CYAN(), Stroke::NONE));
                }
                if resp.clicked() {
                    if let Some(pp) = resp.interact_pointer_pos() {
                        let unit = st.snap_unit() as f32 * tw;
                        let t = (((pp.x - gx) / unit).floor().max(0.0) as u32) * st.snap_unit();
                        st.cursor = Some(t.min(total_ticks.saturating_sub(1)));
                    }
                }
            });
    }
    // the note lane (Velocity / Pan / Chance): one thin bar per note, drag a bar to set its value
    egui::TopBottomPanel::bottom("roll_lane")
        .resizable(false)
        .exact_height(if st.compact { 104.0 } else { 120.0 })
        .frame(egui::Frame::none().fill(BG()).inner_margin(egui::Margin::symmetric(0.0, 4.0)))
        .show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                for (n, v) in [("Velocity", 0u8), ("Pan", 2), ("Chance", 1)] {
                    ui.selectable_value(&mut st.lane_param, v, n);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let (param, pat) = (st.lane_param, &mut project.patterns[cur]);
                    if ui.button("Reset").on_hover_text("Back to the default value").clicked() {
                        shape_values(st, pat, param, 5);
                    }
                    if ui.button("Randomize").clicked() {
                        shape_values(st, pat, param, 3);
                    }
                    if ui.button("Smooth").on_hover_text("Even out the values of neighbouring notes").clicked() {
                        shape_values(st, pat, param, 4);
                    }
                    if ui.button("Curve").on_hover_text("S-curve from the From to the To value of the Note panel").clicked() {
                        shape_values(st, pat, param, 1);
                    }
                    if ui.button("Line").on_hover_text("Straight ramp between the first and last note").clicked() {
                        shape_values(st, pat, param, 0);
                    }
                    ui.label(RichText_dim("Draw: drag a bar"));
                });
            });
            let param = st.lane_param;
            let lane_track = st.track;
            let sel_set = st.selected.clone();
            egui::ScrollArea::horizontal()
                .id_salt("lane_scroll")
                .horizontal_scroll_offset(st.scroll_x)
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .drag_to_scroll(false)
                .show(ui, |ui| {
                    let h = (ui.available_height() - 2.0).max(40.0);
                    let (rect, resp) = ui.allocate_exact_size(vec2(full_w, h), Sense::click_and_drag());
                    let gx = rect.left() + KEYW;
                    let p = ui.painter_at(rect);
                    p.rect_filled(Rect::from_min_max(pos2(gx, rect.top()), rect.right_bottom()), 0.0, PANEL2());
                    for k in 0..=2 {
                        let y = rect.top() + 4.0 + (h - 8.0) * k as f32 / 2.0;
                        p.line_segment([pos2(gx, y), pos2(rect.right(), y)], Stroke::new(1.0, ROLL_LINE()));
                    }
                    p.text(pos2(rect.left() + 8.0, rect.top() + 4.0), Align2::LEFT_TOP, match param { 0 => "127", 2 => "R", _ => "100%" }, FontId::monospace(10.0), DIM());
                    p.text(pos2(rect.left() + 8.0, rect.bottom() - 4.0), Align2::LEFT_BOTTOM, match param { 0 => "0", 2 => "L", _ => "0%" }, FontId::monospace(10.0), DIM());
                    let pat = &mut project.patterns[cur];
                    if resp.is_pointer_button_down_on() {
                        if let Some(pp) = resp.interact_pointer_pos() {
                            // the bar nearest to the pointer (within a few pixels) follows its height
                            let near = pat.notes.iter().enumerate().filter(|(_, n)| n.track == lane_track)
                                .map(|(i, n)| (i, (gx + n.start as f32 * tw - pp.x).abs())).filter(|(_, d)| *d < 7.0)
                                .min_by(|a, b| a.1.total_cmp(&b.1)).map(|(i, _)| i);
                            if let Some(i) = near {
                                let v = 1.0 - ((pp.y - rect.top() - 4.0) / (h - 8.0)).clamp(0.0, 1.0);
                                lane_put(&mut pat.notes[i], param, v);
                            }
                        }
                    }
                    for (i, n) in pat.notes.iter().enumerate().filter(|(_, n)| n.track == lane_track) {
                        let x = gx + n.start as f32 * tw;
                        let v = lane_get(n, param);
                        let y = rect.bottom() - 4.0 - (h - 8.0) * v;
                        let col = if sel_set.contains(&i) { Color32::WHITE } else { track_color(n.track) };
                        p.line_segment([pos2(x, rect.bottom() - 4.0), pos2(x, y)], Stroke::new(2.0, col));
                        p.circle_filled(pos2(x, y), 3.5, col);
                    }
                });
        });
    let avail_h = ui.available_height();
    let mut sa = egui::ScrollArea::both().auto_shrink([false, false]).drag_to_scroll(false).id_salt("roll_scroll").max_height((avail_h - 6.0).max(120.0));
    if let Some(y) = st.scroll_to.take() {
        sa = sa.vertical_scroll_offset(y.max(0.0));
    }
    if let Some(x) = st.scroll_to_x.take() {
        sa = sa.horizontal_scroll_offset(x);
    }
    let out = sa.show(ui, |ui| {
        let pat = &mut project.patterns[cur];
        let (rect, resp) = ui.allocate_exact_size(vec2(full_w, rows as f32 * rh), Sense::click_and_drag());
        let gx = rect.left() + KEYW;
        let row_y = |p: u8| rect.top() + ridx[p as usize] as f32 * rh;
        let note_rect = |n: &Note| Rect::from_min_size(pos2(gx + n.start as f32 * tw, row_y(n.pitch)), vec2((n.len as f32 * tw).max(12.0), rh));
        let painter = ui.painter_at(rect);
        let tinted = st.highlight && ks != theory::CHROMATIC;
        // keys light up in sync with the notes under the playhead (and with keys held live)
        let now = pos_tick % total_ticks.max(1);
        let sounding = |p: u8| -> bool {
            st.preview.iter().any(|(t, q)| *t == st.track && *q == p) || (playing && pat.notes.iter().any(|n| n.track == st.track && n.pitch == p && n.start <= now && now < n.start + n.len))
        };
        // backdrop of the keyboard column: shows through between the rounded key ends
        painter.rect_filled(Rect::from_min_size(rect.min, vec2(KEYW, rect.height())), 0.0, SIDE());
        for &p in &visible {
            let r = Rect::from_min_size(pos2(gx, row_y(p)), vec2(full_w - KEYW, rh));
            let ins = theory::in_scale(kr, ks, p);
            let is_r = theory::is_root(kr, p);
            painter.rect_filled(r, 0.0, if is_black(p) { ROLL_BLACK() } else { ROLL_WHITE() });
            if tinted {
                if is_r {
                    painter.rect_filled(r, 0.0, TEAL().linear_multiply(0.24));
                } else if ins {
                    painter.rect_filled(r, 0.0, TEAL().linear_multiply(0.07));
                } else {
                    painter.rect_filled(r, 0.0, BG().linear_multiply(0.55));
                }
            }
            painter.line_segment([r.left_top(), r.right_top()], Stroke::new(1.0, if p % 12 == 0 { MID() } else { ROLL_LINE() }));
            let key = Rect::from_min_size(pos2(rect.left(), row_y(p)), vec2(KEYW, rh));
            if tinted && is_r {
                painter.rect_filled(key, right_round(), TEAL());
            } else {
                painter.rect_filled(key, right_round(), if is_black(p) { KEY_BLACK() } else { KEY_WHITE() });
            }
            if sounding(p) {
                painter.rect_filled(key, right_round(), TEAL_HI());
                painter.rect_filled(Rect::from_min_size(pos2(gx, row_y(p)), vec2(full_w - KEYW, rh)), 0.0, TEAL().linear_multiply(0.07));
            }
            painter.line_segment([key.left_bottom(), key.right_bottom() - vec2(small(), 0.0)], Stroke::new(1.0, ROLL_LINE()));
            // key labels: dark ink on the light keys, light ink on the dark keys (both modes)
            let ink_on_white = KEY_BLACK().linear_multiply(0.8);
            let ink_on_black = KEY_WHITE().linear_multiply(0.85);
            let (txt, col) = if st.track == Track::Drums {
                (crate::sequencer::DRUM_NAMES[(p - crate::piano_roll::DRUM_BASE) as usize].to_string(), if is_black(p) { ink_on_black } else { ink_on_white })
            } else if tinted && is_r {
                (note_name_in(kr, ks, p), Color32::WHITE)
            } else if !is_black(p) {
                (note_name_in(kr, ks, p), ink_on_white)
            } else if tinted && ins {
                (theory::pc_name(kr, ks, p % 12).to_string(), ink_on_black)
            } else {
                (String::new(), Color32::WHITE)
            };
            if !txt.is_empty() {
                painter.text(key.right_center() - vec2(6.0, 0.0), Align2::RIGHT_CENTER, txt, FontId::monospace(10.0), col);
            }
        }
        if let Some((a, b)) = st.range {
            let (x0, x1) = (gx + a as f32 * tw, gx + b.min(total_ticks) as f32 * tw);
            painter.rect_filled(Rect::from_min_max(pos2(x0, rect.top()), pos2(x1, rect.bottom())), 0.0, TEAL().linear_multiply(0.10));
        }
        // the grid goes on past the pattern's end (dimmed below), so the roll never looks cut off
        let end_s = (((full_w - KEYW) / st.zoom.max(1.0)).ceil() as u32 + 1).min(4096).max(pat.length_steps);
        for s in 0..=end_s {
            let x = gx + s as f32 * st.zoom;
            // step < beat < bar, hairlines only: quiet but clearly separated in both themes
            let (w, c) = if s % 16 == 0 {
                (1.0, LIGHT().linear_multiply(0.75))
            } else if s % 4 == 0 {
                (1.0, MID())
            } else {
                (1.0, ROLL_LINE())
            };
            painter.line_segment([pos2(x, rect.top()), pos2(x, rect.bottom())], Stroke::new(w, c));
        }
        if full_w > KEYW + grid_w {
            painter.rect_filled(Rect::from_min_max(pos2(gx + grid_w, rect.top()), rect.max), 0.0, Color32::from_black_alpha(if is_dark() { 70 } else { 16 }));
            painter.line_segment([pos2(gx + grid_w, rect.top()), pos2(gx + grid_w, rect.bottom())], Stroke::new(1.0, MID()));
        }
        for n in &ghost_notes {
            if n.start >= total_ticks {
                continue;
            }
            let r = note_rect(n).shrink2(vec2(0.0, 1.0));
            painter.rect_filled(r, small(), track_color(n.track).linear_multiply(0.16));
            painter.rect_stroke(r, small(), Stroke::new(1.0, track_color(n.track).linear_multiply(0.45)));
        }
        for (i, n) in pat.notes.iter().enumerate() {
            let active = n.track == st.track;
            let selected = active && st.selected.contains(&i);
            let r = note_rect(n).shrink2(vec2(0.0, 1.0));
            let base = track_color(n.track);
            let a = if selected { 1.0 } else if active { 0.42 + 0.22 * n.vel as f32 / 127.0 } else { 0.22 };
            let a = if n.mute { a * 0.3 } else { a };
            painter.rect_filled(r, small(), base.linear_multiply(a));
            if selected {
                painter.rect_filled(r, small(), Color32::WHITE.linear_multiply(0.22));
            }
            if active {
                // lighter grip along the right edge = the resize handle
                let gw = (r.width() * 0.2).clamp(2.0, 4.0);
                let g = Rect::from_min_max(pos2((r.right() - gw).max(r.left()), r.top() + 2.0), pos2(r.right(), r.bottom() - 2.0));
                painter.rect_filled(g, 0.0, Color32::WHITE.linear_multiply(if selected { 0.45 } else { 0.18 }));
            }
            // the note flashes like a Launchpad pad on its hit (and a moment after it ends)
            if active && playing && now >= n.start && now < n.start + n.len + 44 {
                play_fill(&painter, r, base, (now - n.start) as f32, n.len as f32, ui.input(|i| i.time));
            }
            if active {
                painter.rect_stroke(r, small(), if selected { Stroke::new(2.0, Color32::WHITE) } else { Stroke::new(1.0, base.linear_multiply(0.7)) });
                if n.slide {
                    // a slide note: diagonal hatching, and a glide line up from the note it slides away from
                    let hp = painter.with_clip_rect(r);
                    let mut x = r.left() - r.height();
                    while x < r.right() {
                        hp.line_segment([pos2(x, r.bottom()), pos2(x + r.height(), r.top())], Stroke::new(1.0, Color32::WHITE.gamma_multiply(0.35)));
                        x += 7.0;
                    }
                    let from = pat.notes.iter().enumerate().filter(|(j, o)| *j != i && o.track == n.track && o.start < n.start && o.pitch != n.pitch).max_by_key(|(_, o)| o.start);
                    if let Some((_, o)) = from {
                        let ro = note_rect(o).shrink2(vec2(0.0, 1.0));
                        painter.line_segment([pos2(ro.right().min(r.left()), ro.center().y), pos2(r.left() + 2.0, r.center().y)], Stroke::new(1.5, Color32::WHITE.gamma_multiply(0.8)));
                    }
                }
            }
        }
        if st.suggest {
            if let Some((s, l, p)) = st.suggest_next(pat, kr, ks) {
                let sp = |q: u8| Rect::from_min_size(pos2(gx + s as f32 * tw, row_y(q)), vec2((l as f32 * tw).max(12.0), rh)).shrink2(vec2(0.0, 1.0));
                if visible.contains(&p) {
                    let g = sp(p);
                    painter.rect_filled(g, small(), TEAL().linear_multiply(0.14));
                    dashed_rect(&painter, g, Stroke::new(1.0, TEAL()), 4.0, 3.0);
                    painter.text(g.left_top() - vec2(0.0, 2.0), Align2::LEFT_BOTTOM, format!("Tab: {}", note_name_in(kr, ks, p)), FontId::monospace(10.0), TEAL());
                    for d in [1, -1] {
                        let alt = theory::step_in_scale(kr, ks, p, d);
                        if alt != p && visible.contains(&alt) {
                            painter.rect_stroke(sp(alt), small(), Stroke::new(1.0, TEAL().linear_multiply(0.5)));
                        }
                    }
                }
            }
        }
        // the insertion cursor is only marked in the ruler (triangle): a dashed line across the grid got in the way of drawing notes
        if playing {
            let x = gx + (pos_tick % total_ticks) as f32 * tw;
            painter.line_segment([pos2(x, rect.top()), pos2(x, rect.bottom())], Stroke::new(1.5, CYAN()));
        }

        // ---- interaction ----
        if resp.hovered() {
            let (zd, sd, alt) = ui.input(|i| (i.zoom_delta(), i.smooth_scroll_delta.y, i.modifiers.alt));
            if zd != 1.0 {
                let old = st.zoom;
                st.zoom = (st.zoom * zd).clamp(10.0, 160.0);
                // keep the point under the mouse where it is instead of jumping
                if let Some(p) = resp.hover_pos() {
                    let step = ((p.x - (rect.left() + KEYW)) / old).max(0.0);
                    st.scroll_to_x = Some((st.scroll_x + step * (st.zoom - old)).max(0.0));
                }
            }
            if alt && sd != 0.0 {
                st.row_h = (st.row_h + sd * 0.05).clamp(12.0, 56.0);
                // the wheel is used up here: the area must not scroll as well
                ui.input_mut(|i| {
                    i.raw_scroll_delta = egui::Vec2::ZERO;
                    i.smooth_scroll_delta = egui::Vec2::ZERO;
                });
            }
        }
        let hover = resp.hover_pos();
        let latest = ui.input(|i| i.pointer.latest_pos());
        let to_tick = |x: f32| -> i64 { ((x - gx) / tw).floor() as i64 };
        let to_pitch = |y: f32| -> u8 { visible[(((y - rect.top()) / rh).floor().max(0.0) as usize).min(visible.len() - 1)] };
        let snap_on = st.snap_scale;
        let snap_p = |p: u8| if snap_on { theory::snap_to_scale(kr, ks, p) } else { p };
        let unit = st.snap_unit() as i64;
        let magnet = st.magnet;
        // snapping: grid, or the nearest edge of a note that is not being edited
        let snap_tick = |raw: i64, round: bool, edges: &[i64]| -> i64 {
            if magnet {
                let thr = (8.0 / tw) as i64;
                if let Some(e) = edges.iter().copied().filter(|e| (e - raw).abs() <= thr).min_by_key(|e| (e - raw).abs()) {
                    return e;
                }
            }
            if round { (raw + unit / 2).div_euclid(unit) * unit } else { raw.div_euclid(unit) * unit }
        };
        let (mut pressed, down, sec_down, mods) = ui.input(|i| (i.pointer.primary_pressed(), i.pointer.primary_down(), i.pointer.secondary_down(), i.modifiers));
        // while the circular menu is open the grid below does not react
        if st.radial.is_some() {
            pressed = false;
        }
        if pressed && resp.hovered() {
            // a text field (search, pattern name) must not keep eating the roll's hotkeys
            if let Some(id) = ui.ctx().memory(|m| m.focused()) {
                ui.ctx().memory_mut(|m| m.surrender_focus(id));
            }
        }
        if !down {
            if st.drag.is_some() {
                st.drag = None;
            }
            st.stop_preview(shared);
        }
        let track = st.track;
        let hit_at = |pat: &Pattern, pos: Pos2| -> Option<usize> {
            pat.notes.iter().enumerate().rev().find(|(_, n)| n.track == track && note_rect(n).contains(pos)).map(|(i, _)| i)
        };
        let edges_excluding = |pat: &Pattern, excl: &BTreeSet<usize>| -> Vec<i64> {
            pat.notes
                .iter()
                .enumerate()
                .filter(|(i, n)| n.track == track && !excl.contains(i))
                .flat_map(|(_, n)| [n.start as i64, (n.start + n.len) as i64])
                .collect()
        };

        if let Some(pos) = hover {
            let hit = hit_at(pat, pos);
            if pos.x >= gx && st.drag.is_none() {
                // hover row + ghost note + pitch label
                let hp = snap_p(to_pitch(pos.y));
                painter.rect_filled(Rect::from_min_size(pos2(rect.left(), row_y(hp)), vec2(full_w, rh)), 0.0, TEAL().linear_multiply(0.10));
                if hit.is_none() && st.tool == Tool::Draw && !mods.shift {
                    let tick = snap_tick(to_tick(pos.x).max(0), false, &[]).clamp(0, total_ticks as i64 - 1) as u32;
                    let ghost = Rect::from_min_size(pos2(gx + tick as f32 * tw, row_y(hp)), vec2((st.note_len as f32 * tw).max(3.0), rh)).shrink2(vec2(0.0, 1.0));
                    painter.rect_filled(ghost, small(), track_color(st.track).linear_multiply(0.28));
                    painter.rect_stroke(ghost, small(), Stroke::new(1.0, track_color(st.track)));
                    painter.text(ghost.left_top() - vec2(0.0, 2.0), Align2::LEFT_BOTTOM, if st.track == Track::Drums { crate::sequencer::DRUM_NAMES[(hp.saturating_sub(crate::piano_roll::DRUM_BASE) as usize).min(4)].to_string() } else { note_name_in(kr, ks, hp) }, FontId::monospace(10.0), TEXT());
                }
            }
            if pressed {
                if pos.x > gx + grid_w {
                    // past the end of the pattern: a click makes the pattern long enough to reach there
                    let need = (to_tick(pos.x).max(0) as u32 / TICKS_PER_STEP + 1).min(crate::sequencer::MAX_STEPS as u32);
                    if let Some(l) = [16u32, 32, 64, 128, 256].into_iter().find(|l| *l >= need) {
                        pat.length_steps = l;
                    }
                } else if pos.x < gx {
                    let p = to_pitch(pos.y);
                    if st.audible && st.preview_on { shared.send(Cmd::NoteOn { track: st.track, pitch: p, vel: 100 }); }
                    st.preview.push((st.track, p));
                } else {
                    let tick_raw = to_tick(pos.x);
                    st.cursor = Some(snap_tick(tick_raw, false, &[]).clamp(0, total_ticks as i64 - 1) as u32);
                    match hit {
                        Some(i) if mods.shift => {
                            if !st.selected.remove(&i) {
                                st.selected.insert(i);
                            }
                        }
                        Some(i) if st.tool == Tool::Cut => {
                            let n = pat.notes[i];
                            let at = snap_tick(tick_raw, false, &[]).max(0) as u32;
                            if at > n.start && at < n.start + n.len {
                                pat.notes[i].len = at - n.start;
                                pat.notes.push(Note { start: at, len: n.start + n.len - at, slide: false, ..n });
                                st.selected.clear();
                            }
                        }
                        None if st.tool == Tool::Cut => {}
                        Some(i) => {
                            if !st.selected.contains(&i) {
                                st.selected.clear();
                                st.selected.insert(i);
                            }
                            let mut anchor = i;
                            if mods.command || mods.ctrl || mods.alt {
                                // duplicate-drag: leave the originals, drag copies
                                let src = st.sel(pat);
                                let base = pat.notes.len();
                                st.selected.clear();
                                for (k, &s) in src.iter().enumerate() {
                                    pat.notes.push(pat.notes[s]);
                                    st.selected.insert(base + k);
                                    if s == i {
                                        anchor = base + k;
                                    }
                                }
                            }
                            let n = pat.notes[anchor];
                            let orig: Vec<(usize, Note)> = st.sel(pat).into_iter().map(|k| (k, pat.notes[k])).collect();
                            let right = gx + (n.start + n.len) as f32 * tw;
                            let grip = (n.len as f32 * tw * 0.4).clamp(6.0, 14.0);
                            if pos.x > right - grip {
                                st.drag = Some(Drag::Resize { orig, anchor });
                            } else {
                                st.drag = Some(Drag::Move { orig, anchor, grab: tick_raw - n.start as i64, grab_pitch: to_pitch(pos.y) as i32, last_pitch: n.pitch });
                                if st.audible && st.preview_on { shared.send(Cmd::NoteOn { track: n.track, pitch: n.pitch, vel: n.vel }); }
                                st.preview.push((n.track, n.pitch));
                            }
                        }
                        None if mods.shift || st.tool == Tool::Select => {
                            let base = if mods.shift { st.selected.clone() } else { BTreeSet::new() };
                            if !mods.shift {
                                st.selected.clear();
                                st.range = None;
                            }
                            st.drag = Some(Drag::Box { start: pos, base });
                        }
                        None => {
                            // Draw tool: add a note (or chord); dragging sets its length
                            let edges = edges_excluding(pat, &BTreeSet::new());
                            let tick = snap_tick(tick_raw.max(0), false, &edges).clamp(0, total_ticks as i64 - 1) as u32;
                            let root = snap_p(to_pitch(pos.y));
                            let len = st.note_len.min(total_ticks - tick).max(1);
                            st.selected.clear();
                            let mut orig = vec![];
                            let ivs = CHORDS[st.chord].1;
                            let pitches: Vec<i32> = if st.diatonic && ks != theory::CHROMATIC && ivs.len() > 1 {
                                theory::stack_chord(kr, ks, root, ivs.len().min(4)).into_iter().map(|p| p as i32).collect()
                            } else {
                                ivs.iter().map(|iv| root as i32 + iv).collect()
                            };
                            for p in pitches {
                                if (0..=127).contains(&p) {
                                    let n = Note { start: tick, len, pitch: p as u8, vel: 100, track: st.track, slide: st.slide_new, mute: false, pan: 0, chance: 100 };
                                    pat.notes.push(n);
                                    orig.push((pat.notes.len() - 1, n));
                                    st.selected.insert(pat.notes.len() - 1);
                                    if st.audible && st.preview_on { shared.send(Cmd::NoteOn { track: st.track, pitch: p as u8, vel: 100 }); }
                                    st.preview.push((st.track, p as u8));
                                }
                            }
                            if let Some(&(anchor, _)) = orig.first() {
                                st.drag = Some(Drag::Resize { orig, anchor });
                            }
                        }
                    }
                }
            }
            // the wheel menu of a note opens on a triple click (a double click only edits, it no longer pops the wheel up)
            // (removed at the user's request: the wheel never opens)
            if resp.double_clicked() {
                match hit {
                    Some(_) => {}
                    None if st.tool == Tool::Select && pos.x >= gx => {
                        let tick = snap_tick(to_tick(pos.x).max(0), false, &[]).clamp(0, total_ticks as i64 - 1) as u32;
                        let len = st.note_len.min(total_ticks - tick).max(1);
                        pat.notes.push(Note { start: tick, len, pitch: snap_p(to_pitch(pos.y)), vel: 100, track: st.track, slide: st.slide_new, mute: false, pan: 0, chance: 100 });
                        st.selected.clear();
                        st.selected.insert(pat.notes.len() - 1);
                    }
                    _ => {}
                }
            }
            if sec_down && st.drag.is_none() {
                if let Some(i) = hit {
                    pat.notes.remove(i);
                    st.selected.clear();
                }
            }
        }

        if down {
            if let Some(pos) = latest {
                match st.drag.take() {
                    Some(Drag::Resize { orig, anchor }) => {
                        let moving: BTreeSet<usize> = orig.iter().map(|(i, _)| *i).collect();
                        let edges = edges_excluding(pat, &moving);
                        if let Some((_, a)) = orig.iter().find(|(i, _)| *i == anchor) {
                            let end = snap_tick(to_tick(pos.x), true, &edges).max(a.start as i64 + 1);
                            let delta = end - (a.start + a.len) as i64;
                            for (i, o) in &orig {
                                if *i < pat.notes.len() {
                                    let l = (o.len as i64 + delta).clamp(1, total_ticks as i64 - o.start as i64);
                                    pat.notes[*i].len = l as u32;
                                }
                            }
                            if let Some(n) = pat.notes.get(anchor) {
                                st.note_len = n.len;
                            }
                        }
                        st.drag = Some(Drag::Resize { orig, anchor });
                    }
                    Some(Drag::Move { orig, anchor, grab, grab_pitch, last_pitch }) => {
                        let moving: BTreeSet<usize> = orig.iter().map(|(i, _)| *i).collect();
                        let edges = edges_excluding(pat, &moving);
                        let mut last = last_pitch;
                        if let Some((_, a)) = orig.iter().find(|(i, _)| *i == anchor) {
                            let new_anchor = snap_tick(to_tick(pos.x) - grab, true, &edges);
                            let min_start = orig.iter().map(|(_, o)| o.start as i64).min().unwrap_or(0);
                            let max_start = orig.iter().map(|(_, o)| o.start as i64).max().unwrap_or(0);
                            let dt = (new_anchor - a.start as i64).clamp(-min_start, total_ticks as i64 - 1 - max_start);
                            let mut dp = to_pitch(pos.y) as i32 - grab_pitch;
                            if snap_on {
                                let t = theory::snap_to_scale(kr, ks, (a.pitch as i32 + dp).clamp(0, 127) as u8);
                                dp = t as i32 - a.pitch as i32;
                            }
                            for (i, o) in &orig {
                                if *i < pat.notes.len() {
                                    pat.notes[*i].start = (o.start as i64 + dt) as u32;
                                    pat.notes[*i].pitch = (o.pitch as i32 + dp).clamp(0, 127) as u8;
                                }
                            }
                            if let Some(n) = pat.notes.get(anchor) {
                                // octave hint while dragging
                                painter.text(pos + vec2(14.0, -16.0), Align2::LEFT_BOTTOM, crate::piano_roll::note_name(n.pitch), FontId::monospace(13.0), TEXT());
                            }
                            if let Some(n) = pat.notes.get(anchor).copied() {
                                if n.pitch != last_pitch {
                                    st.stop_preview(shared);
                                    if st.audible && st.preview_on { shared.send(Cmd::NoteOn { track: n.track, pitch: n.pitch, vel: n.vel }); }
                                    st.preview.push((n.track, n.pitch));
                                    last = n.pitch;
                                }
                            }
                        }
                        st.drag = Some(Drag::Move { orig, anchor, grab, grab_pitch, last_pitch: last });
                    }
                    Some(Drag::Box { start, base }) => {
                        let r = Rect::from_two_pos(start, pos);
                        painter.rect_filled(r, small(), TEAL().linear_multiply(0.18));
                        painter.rect_stroke(r, small(), Stroke::new(1.0, TEAL()));
                        st.selected = base.clone();
                        for (i, n) in pat.notes.iter().enumerate() {
                            if n.track == st.track && note_rect(n).intersects(r) {
                                st.selected.insert(i);
                            }
                        }
                        st.drag = Some(Drag::Box { start, base });
                    }
                    None => {}
                }
            }
        }
    });
    if (st.scroll_x - out.state.offset.x).abs() > 0.01 {
        st.scroll_x = out.state.offset.x;
        // the ruler above was drawn with the old offset: draw it again with the new one
        ui.ctx().request_repaint();
    }

    radial_menu(ui, project, st);
    if st.circle_open {
        circle_window(ui.ctx(), &mut project.key_root, &mut project.key_scale, &mut project.patterns[cur], st, shared);
    }
}

/// The circular menu of a note: the look of the playing effect, plus slide / duplicate / delete. A click on an item runs it, a click
/// elsewhere (or Esc) closes the menu.
fn radial_menu(ui: &mut Ui, project: &mut Project, st: &mut RollState) {
    let Some((c, ni)) = st.radial else { return };
    let cur = project.current_pattern;
    let ctx = ui.ctx().clone();
    let pt = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("radial_menu")));
    let items: [(&str, Color32); 6] = [
        ("Pad", TEAL()),
        ("Glow", CYAN()),
        ("Ripple", TEAL_HI()),
        ("Fill off", MID()),
        (if st.track == Track::Bass808 { "Slide" } else { "Copy" }, AMBER),
        ("Delete", RED),
    ];
    let (r0, r1) = (26.0f32, 84.0f32);
    pt.circle_filled(c, r1 + 6.0, Color32::from_black_alpha(90));
    let pointer = ctx.input(|i| i.pointer.latest_pos());
    let sector_of = |p: Pos2| -> Option<usize> {
        let d = p - c;
        let dist = d.length();
        if dist < r0 || dist > r1 + 6.0 {
            return None;
        }
        // first sector starts at the top and goes clockwise
        let a = (d.y.atan2(d.x) + std::f32::consts::FRAC_PI_2).rem_euclid(std::f32::consts::TAU);
        Some(((a / std::f32::consts::TAU * items.len() as f32) as usize).min(items.len() - 1))
    };
    let hover = pointer.and_then(sector_of);
    let n = items.len() as f32;
    for (k, (label, col)) in items.iter().enumerate() {
        let a0 = -std::f32::consts::FRAC_PI_2 + k as f32 / n * std::f32::consts::TAU;
        let a1 = -std::f32::consts::FRAC_PI_2 + (k as f32 + 1.0) / n * std::f32::consts::TAU;
        let on = hover == Some(k) || (k < 4 && super::widgets::fill_style() as usize == [1, 2, 3, 0][k]);
        let fill = if on { col.linear_multiply(0.85) } else { PANEL2() };
        sector(&pt, c, r0, r1, a0 + 0.02, a1 - 0.02, fill, col.linear_multiply(0.7));
        let am = (a0 + a1) * 0.5;
        pt.text(c + vec2(am.cos(), am.sin()) * (r0 + r1) * 0.5, Align2::CENTER_CENTER, *label, FontId::proportional(11.5), if on { Color32::WHITE } else { TEXT() });
    }
    pt.circle_filled(c, r0 - 3.0, PANEL3());
    pt.text(c, Align2::CENTER_CENTER, "x", FontId::proportional(12.0), DIM());
    ctx.request_repaint();
    let (clicked, esc) = ctx.input(|i| (i.pointer.primary_pressed(), i.key_pressed(Key::Escape)));
    if esc {
        st.radial = None;
    } else if clicked {
        st.radial = None;
        if let Some(k) = hover {
            match k {
                0 => super::widgets::set_fill_style(1),
                1 => super::widgets::set_fill_style(2),
                2 => super::widgets::set_fill_style(3),
                3 => super::widgets::set_fill_style(0),
                4 => {
                    if ni < project.patterns[cur].notes.len() {
                        st.selected.clear();
                        st.selected.insert(ni);
                        let pat = &mut project.patterns[cur];
                        if st.track == Track::Bass808 {
                            st.toggle_slide(pat);
                        } else {
                            st.duplicate(pat);
                        }
                    }
                }
                _ => {
                    if ni < project.patterns[cur].notes.len() {
                        project.patterns[cur].notes.remove(ni);
                        st.selected.clear();
                    }
                }
            }
        }
    }
}

fn note_name_in(root: u8, scale: usize, p: u8) -> String {
    format!("{}{}", theory::pc_name(root, scale, p % 12), p as i32 / 12)
}

/// Filled ring sector (works for any angle, drawn as a triangle strip).
fn sector(p: &egui::Painter, c: Pos2, r0: f32, r1: f32, a0: f32, a1: f32, fill: Color32, edge: Color32) {
    const N: usize = 8;
    let at = |r: f32, a: f32| pos2(c.x + r * a.cos(), c.y + r * a.sin());
    let mut m = egui::Mesh::default();
    for i in 0..=N {
        let a = a0 + (a1 - a0) * i as f32 / N as f32;
        m.colored_vertex(at(r0, a), fill);
        m.colored_vertex(at(r1, a), fill);
    }
    for i in 0..N as u32 {
        let b = i * 2;
        m.add_triangle(b, b + 1, b + 2);
        m.add_triangle(b + 1, b + 3, b + 2);
    }
    p.add(egui::Shape::mesh(m));
    for (r, _) in [(r0, 0), (r1, 1)] {
        let pts: Vec<Pos2> = (0..=N).map(|i| at(r, a0 + (a1 - a0) * i as f32 / N as f32)).collect();
        p.add(egui::Shape::line(pts, Stroke::new(1.0, edge)));
    }
    p.line_segment([at(r0, a0), at(r1, a0)], Stroke::new(1.0, edge));
    p.line_segment([at(r0, a1), at(r1, a1)], Stroke::new(1.0, edge));
}

/// Circle of fifths: click a key, hear and insert the diatonic chords of the key.
fn circle_window(ctx: &egui::Context, key_root: &mut u8, key_scale: &mut usize, pat: &mut Pattern, st: &mut RollState, shared: &Shared) {
    let mut open = st.circle_open;
    egui::Window::new("Circle of Fifths").open(&mut open).resizable(false).default_width(300.0).show(ctx, |ui| {
        let size = 270.0;
        let (rect, resp) = ui.allocate_exact_size(vec2(size, size), Sense::click());
        let c = rect.center();
        let (r_out, r_mid, r_in) = (size * 0.5 - 2.0, size * 0.35, size * 0.2);
        let minor_like = theory::is_minor_like(*key_scale);
        let home = theory::circle_pos(if minor_like { (*key_root + 3) % 12 } else { *key_root });
        let step = std::f32::consts::TAU / 12.0;
        let p = ui.painter_at(rect);
        for pos in 0..12usize {
            let a0 = -std::f32::consts::FRAC_PI_2 + (pos as f32 - 0.5) * step;
            let dist = (pos + 12 - home) % 12;
            let near = dist == 1 || dist == 11;
            for (ring, (ra, rb)) in [(0, (r_mid, r_out)), (1, (r_in, r_mid))] {
                let is_home = pos == home && (ring == 1) == minor_like;
                let fill = if is_home {
                    TEAL()
                } else if pos == home {
                    TEAL().linear_multiply(0.45)
                } else if near {
                    TEAL_HI().linear_multiply(0.35)
                } else if ring == 0 {
                    PANEL3()
                } else {
                    PANEL2()
                };
                sector(&p, c, ra, rb, a0, a0 + step, fill, DARK().linear_multiply(0.6));
                let am = a0 + step * 0.5;
                let rm = (ra + rb) * 0.5;
                let (name, size_t) = if ring == 0 { (theory::CIRCLE_MAJOR[pos], 15.0) } else { (theory::CIRCLE_MINOR[pos], 11.0) };
                p.text(pos2(c.x + rm * am.cos(), c.y + rm * am.sin()), Align2::CENTER_CENTER, name, FontId::proportional(size_t), if is_home { Color32::WHITE } else { TEXT() });
            }
        }
        p.circle_filled(c, r_in - 4.0, PANEL2());
        p.circle_stroke(c, r_in - 4.0, Stroke::new(1.0, DARK().linear_multiply(0.6)));
        p.text(c - vec2(0.0, 8.0), Align2::CENTER_CENTER, theory::pc_name(*key_root, *key_scale, *key_root), FontId::proportional(22.0), TEAL());
        p.text(c + vec2(0.0, 12.0), Align2::CENTER_CENTER, theory::SCALES[*key_scale].0, FontId::proportional(10.0), DIM());
        if resp.clicked() {
            if let Some(pp) = resp.interact_pointer_pos() {
                let d = pp - c;
                let dist = d.length();
                let ang = d.y.atan2(d.x) + std::f32::consts::FRAC_PI_2;
                let pos = ((ang / step).round().rem_euclid(12.0)) as usize % 12;
                if dist > r_mid && dist <= r_out {
                    *key_root = theory::circle_root(pos);
                    if minor_like || *key_scale == theory::CHROMATIC {
                        *key_scale = 0;
                    }
                } else if dist > r_in - 4.0 && dist <= r_mid {
                    *key_root = (theory::circle_root(pos) + 9) % 12;
                    if !minor_like {
                        *key_scale = 1;
                    }
                }
            }
        }
        let (kr, ks) = (*key_root, *key_scale);
        let tones: Vec<&str> = theory::SCALES[ks].1.iter().map(|s| theory::pc_name(kr, ks, kr + s)).collect();
        ui.label(egui::RichText::new(format!("Notes: {}", tones.join("  "))).color(DIM()));
        ui.separator();
        ui.horizontal(|ui| {
            section_label(ui, "CHORDS OF THE KEY");
            ui.checkbox(&mut st.sevenths, "7ths");
        });
        ui.label(RichText_dim("hold to hear - click to insert at the cursor line"));
        let n = theory::SCALES[ks].1.len().min(7);
        ui.horizontal_wrapped(|ui| {
            for deg in 0..n {
                let notes = theory::diatonic_chord(kr, ks, deg, st.sevenths);
                let name = theory::chord_name(&notes, kr, ks).unwrap_or_default();
                let label = format!("{}\n{}", theory::roman(kr, ks, deg), name);
                let r = ui.add_sized([56.0, 38.0], egui::Button::new(label).sense(Sense::click_and_drag()));
                if r.hovered() && ui.input(|i| i.pointer.primary_pressed()) {
                    for &pi in &notes {
                        if st.audible && st.preview_on { shared.send(Cmd::NoteOn { track: st.track, pitch: pi, vel: 100 }); }
                        st.preview.push((st.track, pi));
                    }
                }
                if r.clicked() {
                    st.insert_chord(pat, &notes, st.note_len.max(96));
                }
            }
        });
        if n == 7 {
            ui.label(RichText_dim("progressions (1 bar per chord)"));
            let progs: [(&str, [usize; 4]); 3] = if minor_like {
                [("i VI III VII", [0, 5, 2, 6]), ("i iv v i", [0, 3, 4, 0]), ("i VII VI VII", [0, 6, 5, 6])]
            } else {
                [("I V vi IV", [0, 4, 5, 3]), ("vi IV I V", [5, 3, 0, 4]), ("I vi IV V", [0, 5, 3, 4])]
            };
            ui.horizontal_wrapped(|ui| {
                for (name, degs) in progs {
                    if ui.button(name).clicked() {
                        for d in degs {
                            let notes = theory::diatonic_chord(kr, ks, d, st.sevenths);
                            st.insert_chord(pat, &notes, TICKS_PER_BAR);
                        }
                    }
                }
            });
        }
    });
    st.circle_open = open;
}

#[allow(non_snake_case)]
fn RichText_dim(s: &str) -> egui::RichText {
    egui::RichText::new(s).color(DIM()).size(11.0)
}

fn snap_name(s: u32) -> &'static str {
    match s {
        96 => "1/4",
        48 => "1/8",
        24 => "1/16",
        12 => "1/32",
        6 => "1/64",
        _ => "Off",
    }
}

fn len_name(s: u32) -> &'static str {
    match s {
        6 => "1/64",
        12 => "1/32",
        24 => "1/16",
        48 => "1/8",
        96 => "1/4",
        192 => "1/2",
        384 => "1 bar",
        768 => "2 bars",
        _ => "custom",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(start: u32, len: u32, pitch: u8) -> Note {
        Note { start, len, pitch, vel: 100, track: Track::Synth, slide: false, mute: false, pan: 0, chance: 100 }
    }

    #[test]
    fn duplicate_copy_paste_quantize_delete() {
        let mut pat = Pattern::new("t", 32);
        pat.notes = vec![note(0, 24, 60), note(30, 24, 64), note(100, 48, 67)];
        let mut st = RollState::new();
        st.select_all(&pat);
        st.duplicate(&mut pat); // copies appended after the block (len 148 -> rounded to grid 168)
        assert_eq!(pat.notes.len(), 6);
        assert_eq!(pat.notes[3].start, 168);
        assert_eq!(st.sel(&pat).len(), 3); // selection moved to the copies
        st.copy(&pat);
        st.cursor = Some(400);
        st.paste(&mut pat);
        assert_eq!(pat.notes.len(), 9);
        assert_eq!(pat.notes[6].start, 400);
        // quantize: 30 -> 24 (snap 1/16 = 24)
        st.selected.clear();
        st.quantize(&mut pat);
        assert!(pat.notes.iter().all(|n| n.start % 24 == 0));
        st.select_all(&pat);
        st.delete_selected(&mut pat);
        assert!(pat.notes.is_empty());
    }

    #[test]
    fn chop_cuts_notes_into_equal_parts_and_a_roll_builds_up() {
        let mut st = RollState::new();
        let mut pat = Pattern::new("t", 16);
        pat.notes.push(note(0, 96, 60));
        st.selected.insert(0);
        st.chop(&mut pat, 4, false);
        assert_eq!(pat.notes.len(), 4);
        assert!(pat.notes.iter().all(|n| n.len == 24));
        let mut starts: Vec<u32> = pat.notes.iter().map(|n| n.start).collect();
        starts.sort();
        assert_eq!(starts, vec![0, 24, 48, 72]);
        let mut pat = Pattern::new("t", 16);
        pat.notes.push(note(0, 96, 60));
        st.selected.clear();
        st.selected.insert(0);
        st.chop(&mut pat, 4, true);
        let mut by_start = pat.notes.clone();
        by_start.sort_by_key(|n| n.start);
        assert!(by_start[0].vel < by_start[3].vel, "a roll gets louder");
    }

    #[test]
    fn nudge_clamps() {
        let mut pat = Pattern::new("t", 16);
        pat.notes = vec![note(0, 24, 0)];
        let mut st = RollState::new();
        st.select_all(&pat);
        st.nudge(&mut pat, -100, -5);
        assert_eq!((pat.notes[0].start, pat.notes[0].pitch), (0, 0));
        st.nudge(&mut pat, 10_000, 200);
        assert_eq!((pat.notes[0].start, pat.notes[0].pitch), (pat.len_ticks() - 1, 127));
    }
}

/// A card in the tools panel.
/// value of a note for the lane / panel parameter: 0 velocity, 1 chance, 2 pan, as 0..1
/// Shape the lane values of the target notes: 0 line, 1 curve, 3 random, 4 smooth, 5 reset
fn shape_values(st: &mut RollState, pat: &mut Pattern, param: u8, kind: u8) {
    let mut idx = st.targets(pat);
    idx.sort_by_key(|&i| pat.notes[i].start);
    if idx.is_empty() {
        return;
    }
    let (a, b) = (st.ramp_from, st.ramp_to);
    let n = idx.len();
    let old: Vec<f32> = idx.iter().map(|&i| lane_get(&pat.notes[i], param)).collect();
    let mut seed = web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(1) | 1;
    for (k, &i) in idx.iter().enumerate() {
        let u = if n < 2 { 0.0 } else { k as f32 / (n - 1) as f32 };
        let v = match kind {
            0 => a + (b - a) * u,
            1 => a + (b - a) * u * u * (3.0 - 2.0 * u),
            3 => {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                a.min(b) + (a - b).abs() * ((seed % 1000) as f32 / 1000.0)
            }
            4 => {
                let prev = old[k.saturating_sub(1)];
                let next = old[(k + 1).min(n - 1)];
                (prev + 2.0 * old[k] + next) / 4.0
            }
            _ => lane_default(param),
        };
        lane_put(&mut pat.notes[i], param, v);
    }
}

/// a square icon button of the roll toolbar: 0 select, 1 draw, 2 cut, 3 magnet, 4 speaker
fn tool_btn(ui: &mut Ui, icon: u8, active: bool, tip: &str) -> bool {
    let (r, resp) = ui.allocate_exact_size(vec2(32.0, 28.0), Sense::click());
    let p = ui.painter();
    gloss(p, r, 9.0, if active { TEAL() } else if resp.hovered() { PANEL3() } else { PANEL2() });
    p.rect_stroke(r, 9.0, Stroke::new(border_w(), if active { TEAL() } else { border() }));
    let c = r.center();
    let ink = if active { Color32::WHITE } else { TEXT() };
    let s = Stroke::new(1.7, ink);
    match icon {
        0 => {
            p.add(egui::Shape::convex_polygon(vec![c + vec2(-4.0, -7.0), c + vec2(-4.0, 6.0), c + vec2(0.0, 2.5), c + vec2(6.0, 2.5)], ink, Stroke::NONE));
        }
        1 => {
            p.line_segment([c + vec2(-6.0, 6.0), c + vec2(5.0, -5.0)], Stroke::new(3.0, ink));
            p.add(egui::Shape::convex_polygon(vec![c + vec2(-7.0, 7.0), c + vec2(-7.0, 3.0), c + vec2(-3.0, 7.0)], ink, Stroke::NONE));
        }
        2 => {
            p.line_segment([c + vec2(-5.5, -5.5), c + vec2(5.5, 5.5)], s);
            p.line_segment([c + vec2(-5.5, 5.5), c + vec2(5.5, -5.5)], s);
        }
        3 => {
            p.line_segment([c + vec2(-5.0, -6.0), c + vec2(-5.0, 1.0)], Stroke::new(2.4, ink));
            p.line_segment([c + vec2(5.0, -6.0), c + vec2(5.0, 1.0)], Stroke::new(2.4, ink));
            p.add(egui::Shape::line((0..=12).map(|k| { let a = std::f32::consts::PI * k as f32 / 12.0; c + vec2(-5.0 * a.cos(), 1.0 + 5.0 * a.sin()) }).collect(), s));
        }
        _ => {
            p.add(egui::Shape::convex_polygon(vec![c + vec2(-7.0, -2.5), c + vec2(-3.0, -2.5), c + vec2(1.0, -6.0), c + vec2(1.0, 6.0), c + vec2(-3.0, 2.5), c + vec2(-7.0, 2.5)], ink, Stroke::NONE));
            p.add(egui::Shape::line((0..=8).map(|k| { let a = -0.9 + 1.8 * k as f32 / 8.0; c + vec2(2.0 + 4.0 * a.cos(), 4.5 * a.sin()) }).collect(), s));
        }
    }
    resp.on_hover_text(tip).clicked()
}

fn lane_get(n: &Note, p: u8) -> f32 {
    match p {
        0 => n.vel as f32 / 127.0,
        2 => (n.pan as f32 + 100.0) / 200.0,
        _ => n.chance as f32 / 100.0,
    }
}

fn lane_put(n: &mut Note, p: u8, v: f32) {
    let v = v.clamp(0.0, 1.0);
    match p {
        0 => n.vel = ((v * 127.0).round() as i32).clamp(1, 127) as u8,
        2 => {
            let x = (v * 200.0 - 100.0).round() as i32;
            n.pan = if x.abs() <= 3 { 0 } else { x.clamp(-100, 100) as i8 };
        }
        _ => n.chance = ((v * 100.0).round() as u8).min(100),
    }
}

fn lane_fmt(p: u8, x: f32) -> String {
    match p {
        0 => format!("{}", (x * 127.0).round() as i32),
        2 => format!("{:+}", (x * 200.0 - 100.0).round() as i32),
        _ => format!("{} %", (x * 100.0).round() as i32),
    }
}

fn lane_default(p: u8) -> f32 {
    match p {
        0 => 100.0 / 127.0,
        2 => 0.5,
        _ => 1.0,
    }
}

/// Right panel: Note / Channel / Track.
fn inspector(ui: &mut Ui, st: &mut RollState, project: &mut Project, cur: usize) {
    ui.horizontal(|ui| {
        for (i, n) in ["Note", "Channel", "Track"].into_iter().enumerate() {
            ui.selectable_value(&mut st.side_tab, i as u8, n);
        }
    });
    ui.add_space(6.0);
    match st.side_tab {
        0 => note_tab(ui, st, project, cur),
        1 => channel_tab(ui, st, project),
        _ => track_tab(ui, st, project),
    }
}

fn pos_fmt(t: u32) -> String {
    format!("{}:{}:{}", t / 384 + 1, t % 384 / 96 + 1, t % 96)
}

fn pos_parse(s: &str) -> Option<f64> {
    let v: Vec<u32> = s.trim().split(':').filter_map(|x| x.trim().parse().ok()).collect();
    match v.as_slice() {
        [b, q, t] => Some(((b.saturating_sub(1)) * 384 + q.saturating_sub(1) * 96 + t) as f64),
        [t] => Some(*t as f64),
        _ => None,
    }
}

/// one labelled row of the right panel
fn prop_row(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(vec2(74.0, 22.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.label(RichText_dim(label));
        });
        add(ui);
    });
}

/// card with a title and a reset button on the right; true when reset was clicked
fn card_titled(ui: &mut Ui, title: &str, body: impl FnOnce(&mut Ui)) -> bool {
    let mut reset = false;
    side_card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(title).strong().size(13.0));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("Reset").on_hover_text("Back to the defaults").clicked() {
                    reset = true;
                }
            });
        });
        ui.add_space(2.0);
        body(ui);
    });
    reset
}

fn note_tab(ui: &mut Ui, st: &mut RollState, project: &mut Project, cur: usize) {
    let (kr, ks) = (project.key_root, project.key_scale);
    let sel = st.sel(&project.patterns[cur]);
    // ---- Note
    side_card(ui, |ui| {
        let pat = &mut project.patterns[cur];
        let Some(&first_i) = sel.first() else {
            ui.label(egui::RichText::new("Note").strong().size(13.0));
            ui.label(RichText_dim("select a note to edit its values"));
            return;
        };
        let first = pat.notes[first_i];
        let total = pat.len_ticks();
        ui.label(egui::RichText::new(if sel.len() > 1 { format!("{} notes", sel.len()) } else { "Note".to_string() }).strong().size(13.0));
        let mut pitch = first.pitch as i32;
        prop_row(ui, "Pitch", |ui| {
            if ui.add(egui::DragValue::new(&mut pitch).range(0..=127).custom_formatter(move |v, _| note_name_in(kr, ks, v as u8))).changed() {
                let d = pitch - first.pitch as i32;
                for &i in &sel {
                    pat.notes[i].pitch = (pat.notes[i].pitch as i32 + d).clamp(0, 127) as u8;
                }
            }
        });
        let mut vel = first.vel as i32;
        prop_row(ui, "Velocity", |ui| {
            ui.spacing_mut().slider_width = 90.0;
            let a = ui.add(egui::Slider::new(&mut vel, 1..=127).show_value(false)).changed();
            let b = ui.add(egui::DragValue::new(&mut vel).range(1..=127)).changed();
            if a || b {
                for &i in &sel {
                    pat.notes[i].vel = vel as u8;
                }
            }
        });
        let mut len = first.len as i32;
        prop_row(ui, "Length", |ui| {
            let r = ui.add(
                egui::DragValue::new(&mut len)
                    .range(1..=total as i32)
                    .custom_formatter(|v, _| { let n = v as u32; if len_name(n) != "custom" { len_name(n).to_string() } else { format!("{n} t") } })
                    .custom_parser(|t| match t.trim() { "1/64" => Some(6.0), "1/32" => Some(12.0), "1/16" => Some(24.0), "1/8" => Some(48.0), "1/4" => Some(96.0), "1/2" => Some(192.0), "1 bar" => Some(384.0), x => x.trim_end_matches('t').trim().parse().ok() }),
            );
            if r.changed() {
                for &i in &sel {
                    pat.notes[i].len = (len as u32).min(total - pat.notes[i].start).max(1);
                }
            }
        });
        let mut start = first.start as i32;
        prop_row(ui, "Start", |ui| {
            if ui.add(egui::DragValue::new(&mut start).range(0..=(total as i32 - 1)).custom_formatter(|v, _| pos_fmt(v as u32)).custom_parser(pos_parse)).changed() {
                let d = start - first.start as i32;
                for &i in &sel {
                    pat.notes[i].start = (pat.notes[i].start as i32 + d).clamp(0, total as i32 - 1) as u32;
                }
            }
        });
        let mut end = (first.start + first.len) as i32;
        prop_row(ui, "End", |ui| {
            if ui.add(egui::DragValue::new(&mut end).range(1..=total as i32).custom_formatter(|v, _| pos_fmt(v as u32)).custom_parser(pos_parse)).changed() {
                for &i in &sel {
                    let s0 = pat.notes[i].start as i32;
                    pat.notes[i].len = (end - s0).max(1) as u32;
                }
            }
        });
    });
    // ---- Note Properties
    if !sel.is_empty() {
        let pat = &mut project.patterns[cur];
        let first = pat.notes[sel[0]];
        let reset = card_titled(ui, "Note Properties", |ui| {
            let mut mute = first.mute;
            prop_row(ui, "Mute", |ui| {
                if ui.checkbox(&mut mute, "").changed() {
                    for &i in &sel {
                        pat.notes[i].mute = mute;
                    }
                }
            });
            let mut tr = 0i32;
            prop_row(ui, "Transpose", |ui| {
                if ui.add(egui::DragValue::new(&mut tr).range(-24..=24).suffix(" st")).changed() {
                    for &i in &sel {
                        pat.notes[i].pitch = (pat.notes[i].pitch as i32 + tr).clamp(0, 127) as u8;
                    }
                }
            });
            let mut pan = first.pan as i32;
            prop_row(ui, "Pan", |ui| {
                ui.spacing_mut().slider_width = 90.0;
                if ui.add(egui::Slider::new(&mut pan, -100..=100)).changed() {
                    for &i in &sel {
                        pat.notes[i].pan = pan as i8;
                    }
                }
            });
            let mut chance = first.chance as i32;
            prop_row(ui, "Chance", |ui| {
                ui.spacing_mut().slider_width = 90.0;
                if ui.add(egui::Slider::new(&mut chance, 0..=100).suffix(" %")).changed() {
                    for &i in &sel {
                        pat.notes[i].chance = chance as u8;
                    }
                }
            });
            let mut slide = first.slide;
            prop_row(ui, "Slide", |ui| {
                if ui.checkbox(&mut slide, "").on_hover_text("Glide from the previous note (808 and compatible instruments)").changed() {
                    for &i in &sel {
                        pat.notes[i].slide = slide;
                    }
                }
            });
        });
        if reset {
            for &i in &sel {
                let n = &mut pat.notes[i];
                n.mute = false;
                n.pan = 0;
                n.chance = 100;
                n.slide = false;
            }
        }
    }
    // ---- Quantize
    let reset = card_titled(ui, "Quantize", |ui| {
        prop_row(ui, "Grid", |ui| {
            egui::ComboBox::from_id_salt("insp_snap").width(90.0).selected_text(snap_name(st.snap)).show_ui(ui, |ui| {
                for s in [96u32, 48, 24, 12, 6] {
                    ui.selectable_value(&mut st.snap, s, snap_name(s));
                }
            });
        });
        prop_row(ui, "Strength", |ui| {
            ui.spacing_mut().slider_width = 90.0;
            ui.add(egui::Slider::new(&mut st.qstrength, 0.0..=1.0).custom_formatter(|v, _| format!("{:.0}%", v * 100.0)));
        });
        if ui.button("Quantize  (Alt+Q)").clicked() {
            st.quantize(&mut project.patterns[cur]);
        }
    });
    if reset {
        st.snap = 24;
        st.qstrength = 1.0;
    }
    // ---- Scale
    let reset = card_titled(ui, "Scale", |ui| {
        prop_row(ui, "Key", |ui| {
            egui::ComboBox::from_id_salt("insp_root").width(90.0).selected_text(theory::pc_name(kr, ks, kr)).show_ui(ui, |ui| {
                for r in 0..12u8 {
                    ui.selectable_value(&mut project.key_root, r, theory::pc_name(r, ks, r));
                }
            });
        });
        prop_row(ui, "Scale", |ui| {
            egui::ComboBox::from_id_salt("insp_scale").width(110.0).selected_text(theory::SCALES[ks].0).show_ui(ui, |ui| {
                for (i, (n, _)) in theory::SCALES.iter().enumerate() {
                    ui.selectable_value(&mut project.key_scale, i, *n);
                }
            });
        });
    });
    if reset {
        project.key_root = 0;
        project.key_scale = theory::CHROMATIC;
    }
    // ---- Chord
    let reset = card_titled(ui, "Chord", |ui| {
        prop_row(ui, "Type", |ui| {
            egui::ComboBox::from_id_salt("insp_chord").width(110.0).selected_text(CHORDS[st.chord.min(CHORDS.len() - 1)].0).show_ui(ui, |ui| {
                for (i, (n, _)) in CHORDS.iter().enumerate() {
                    ui.selectable_value(&mut st.chord, i, *n);
                }
            });
        });
    });
    if reset {
        st.chord = 0;
    }
}

fn channel_tab(ui: &mut Ui, st: &mut RollState, project: &mut Project) {
    let ch = match st.track {
        Track::Synth => crate::mixer::CH_SYNTH,
        Track::Bass808 | Track::Sampler => crate::mixer::CH_BASS,
        Track::Wavetable => crate::mixer::CH_WT,
        Track::Analog => crate::mixer::CH_AN,
        Track::Drums => 0,
    };
    side_card(ui, |ui| {
        section_label(ui, "INSTRUMENT CHANNEL");
        ui.label(egui::RichText::new(st.track.name()).strong().color(track_color(st.track)));
        let c = &mut project.channels[ch];
        ui.add(egui::Slider::new(&mut c.volume, 0.0..=1.5).text("Volume"));
        ui.add(egui::Slider::new(&mut c.pan, -1.0..=1.0).text("Pan"));
        ui.add(egui::Slider::new(&mut c.delay_send, 0.0..=1.0).text("Delay send"));
        ui.horizontal(|ui| {
            ui.toggle_value(&mut c.mute, "Mute");
            ui.toggle_value(&mut c.solo, "Solo");
            super::mixer_ui::output_picker(ui, "insp_ch_out", &mut c.output);
        });
    });
}

fn track_tab(ui: &mut Ui, st: &mut RollState, project: &mut Project) {
    side_card(ui, |ui| {
        section_label(ui, "PLAYLIST TRACK");
        let Some(t) = project.arrangement.tracks.get_mut(st.lane) else {
            ui.label(RichText_dim("open the editor from a Playlist clip"));
            return;
        };
        ui.text_edit_singleline(&mut t.name);
        ui.horizontal(|ui| {
            ui.toggle_value(&mut t.mute, "Mute");
            ui.toggle_value(&mut t.solo, "Solo");
            ui.toggle_value(&mut t.arm, "Rec");
        });
        ui.add(egui::Slider::new(&mut t.transpose, -24..=24).text("Transpose"));
        ui.add(egui::Slider::new(&mut t.volume, 0.0..=1.5).text("Level"));
        ui.horizontal(|ui| {
            ui.label("Output");
            super::mixer_ui::output_picker(ui, "insp_trk_out", &mut t.output);
        });
        ui.checkbox(&mut t.sidechain_src, "Sidechain source");
    });
}

fn side_card(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    egui::Frame::none().fill(PANEL2()).rounding(card()).stroke(Stroke::new(border_w(), border())).inner_margin(egui::Margin::symmetric(10.0, 8.0)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        add(ui);
    });
    ui.add_space(6.0);
}

/// Piano keys are rounded on the side that faces the grid.
fn right_round() -> egui::Rounding {
    egui::Rounding { nw: 0.0, ne: small(), sw: 0.0, se: small() }
}

/// Dashed outline (used for the suggested next note).
fn dashed_rect(p: &egui::Painter, r: Rect, stroke: Stroke, _radius: f32, dash: f32) {
    let seg = |a: Pos2, b: Pos2| {
        let len = (b - a).length();
        let n = (len / (dash * 2.0)).max(1.0) as usize;
        for i in 0..n {
            let t0 = (i as f32 * dash * 2.0) / len;
            let t1 = ((i as f32 * dash * 2.0) + dash).min(len) / len;
            p.line_segment([a + (b - a) * t0, a + (b - a) * t1], stroke);
        }
    };
    seg(r.left_top(), r.right_top());
    seg(r.right_top(), r.right_bottom());
    seg(r.right_bottom(), r.left_bottom());
    seg(r.left_bottom(), r.left_top());
}

/// Right panel: edit the velocity or the chance of the selected notes (or of every note of the track).
fn automation_panel(ui: &mut Ui, st: &mut RollState, pat: &mut Pattern) {
    let param = st.lane_param;
    let mut idx = st.targets(pat);
    idx.sort_by_key(|&i| pat.notes[i].start);
    let scope = if st.selected.is_empty() { "all notes of the track".to_string() } else { format!("{} selected notes", idx.len()) };
    let get = |n: &Note| lane_get(n, param);
    let put = |n: &mut Note, v: f32| lane_put(n, param, v);
    side_card(ui, |ui| {
        section_label(ui, "NOTE AUTOMATION");
        ui.horizontal(|ui| {
            for (n, v) in [("Velocity", 0u8), ("Pan", 2), ("Chance", 1)] {
                ui.selectable_value(&mut st.lane_param, v, n);
            }
        });
        ui.label(RichText_dim(match param { 0 => "how hard each note plays", 2 => "left / right position of each note", _ => "how often each note plays (100 % = always)" }));
        ui.label(RichText_dim(&scope));
    });
    side_card(ui, |ui| {
        section_label(ui, "VALUE");
        if idx.is_empty() {
            ui.label(RichText_dim("no notes on this track yet"));
            return;
        }
        let mut v = idx.iter().map(|&i| get(&pat.notes[i])).sum::<f32>() / idx.len() as f32;
        let r = ui.add(egui::Slider::new(&mut v, 0.0..=1.0).custom_formatter(move |x, _| lane_fmt(param, x as f32)));
        if r.changed() {
            for &i in &idx {
                put(&mut pat.notes[i], v);
            }
        }
    });
    side_card(ui, |ui| {
        section_label(ui, "SHAPE OVER TIME");
        ui.add(egui::Slider::new(&mut st.ramp_from, 0.0..=1.0).text("From").show_value(false));
        ui.add(egui::Slider::new(&mut st.ramp_to, 0.0..=1.0).text("To").show_value(false));
        if idx.len() < 2 {
            ui.label(RichText_dim("select two or more notes"));
            return;
        }
        let (a, b) = (st.ramp_from, st.ramp_to);
        let t0 = pat.notes[idx[0]].start as f32;
        let t1 = pat.notes[*idx.last().unwrap()].start as f32;
        let at = |n: &Note| if t1 - t0 < 1.0 { 0.0 } else { (n.start as f32 - t0) / (t1 - t0) };
        let mut apply = |f: &dyn Fn(f32, &Note) -> f32| {
            for &i in &idx {
                let u = at(&pat.notes[i]);
                let v = f(u, &pat.notes[i]);
                put(&mut pat.notes[i], v);
            }
        };
        ui.horizontal_wrapped(|ui| {
            if ui.button("Line").on_hover_text("Straight ramp from -> to").clicked() {
                apply(&|u, _| a + (b - a) * u);
            }
            if ui.button("Curve").on_hover_text("Smooth S-curve from -> to").clicked() {
                apply(&|u, _| a + (b - a) * u * u * (3.0 - 2.0 * u));
            }
            if ui.button("Swell").on_hover_text("From -> to -> from, like a breath").clicked() {
                apply(&|u, _| a + (b - a) * (u * std::f32::consts::PI).sin());
            }
            if ui.button("Random").on_hover_text("Random values between from and to").clicked() {
                apply(&|_, n| {
                    let h = (n.start.wrapping_mul(2654435761) ^ (n.pitch as u32).wrapping_mul(40503)) >> 8;
                    a.min(b) + (a - b).abs() * ((h % 1000) as f32 / 1000.0)
                });
            }
            if ui.button("Reset").on_hover_text("Velocity 100 / chance 100 %").clicked() {
                apply(&|_, _| lane_default(param));
            }
        });
    });
}
