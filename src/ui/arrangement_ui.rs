//! Playlist / arrangement UI (FL Studio style): track list on the left (MIDI and Audio tracks),
//! bar timeline on the right. Patterns (from the Browser) go onto MIDI tracks, audio files onto Audio tracks.
use super::browser::{dropped_wavs, resolve_lane, DragItem};
use super::library::Library;
use super::theme::*;
use super::widgets::{lerp_color, section_label};
use crate::arrangement::{Clip, TrackKind};
use crate::audio_engine::device::Shared;
use crate::project::Project;
use crate::sequencer::{Pattern, PPQ, TICKS_PER_BAR};
use egui::{pos2, vec2, Align2, Color32, FontId, Rect, Sense, Stroke, Ui};
use std::sync::atomic::Ordering;

/// height of one lane in px; changed with Alt+wheel over the lanes or the V slider of the Playlist bar
static LANE_H_BITS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0x4280_0000); // 64.0
fn lane_h() -> f32 {
    f32::from_bits(LANE_H_BITS.load(Ordering::Relaxed))
}
fn set_lane_h(v: f32) {
    LANE_H_BITS.store(v.clamp(28.0, 260.0).to_bits(), Ordering::Relaxed);
}
/// extra height of the expanded (accordion) track: strip + plugin + drop zone
const DETAIL_H: f32 = 270.0;
/// height of the automation lane that opens under the selected track (Ableton's automation view)
const AUTO_H: f32 = 120.0;

/// extra height under the selected track: the automation lane, or the detail card
fn extra_h(st: &ArrState, lanes: usize) -> f32 {
    if st.sel_lane >= lanes {
        0.0
    } else if st.auto_edit {
        // the curve is drawn right over the clips of the selected track: no lane of its own
        0.0
    } else if st.detail_open {
        DETAIL_H
    } else {
        0.0
    }
}
const RULER_H: f32 = 24.0;
const HEADER_W: f32 = 250.0;

/// A Playlist track dragged out of its card (dropped on a mixer insert it is routed to that insert).
#[derive(Clone, Copy)]
pub struct TrackDrag(pub usize);

enum Drag {
    /// move the selected clips together (`orig` = where they were when the drag began); `copies`: they are fresh copies
    Move { primary: usize, grab: i32, orig: Vec<(usize, Clip)>, copies: bool },
    /// stretch / trim from the right edge
    Resize { idx: usize },
    /// trim from the left edge (the clip keeps playing the same content)
    ResizeLeft { idx: usize },
    /// drag the round handle at the top corner of a clip: the fade in (false) or fade out (true)
    Fade { idx: usize, out: bool },
    /// rubber band on empty space; a plain click without moving still creates a clip
    Box { start: egui::Pos2, press_tick: i32, press_lane: usize, base: std::collections::BTreeSet<usize>, moved: bool },
}

pub struct ArrState {
    pub bar_w: f32,
    /// track shown in the Track / device panel
    pub sel_lane: usize,
    /// pool index of the audio clip last clicked
    pub sel_sample: Option<usize>,
    /// set when a clip is clicked: the detail dock should open for it
    pub open_detail: bool,
    /// set together with `open_detail` by a double-click on a MIDI clip: show the piano roll
    pub open_roll: bool,
    /// the selected-track panel under the track list is shown
    pub detail_open: bool,
    /// "Open editor" was pressed: show the instrument / sound window
    pub open_editor: bool,
    /// grid of the Playlist in ticks (384 = bar, 96 = beat, ...)
    pub snap: u32,
    /// 0 = select / move / stretch clips, 1 = cut
    pub tool: u8,
    pub sel_clip: Option<usize>,
    /// automation editing: the curve of the selected track (or of the master) is drawn over its lane and edited with the mouse
    pub auto_edit: bool,
    pub auto_master: bool,
    auto_drag: Option<usize>,
    /// the segment (index of its first point) whose bend handle is being dragged
    auto_bend: Option<usize>,
    /// every selected clip (Shift+click adds, a rubber band selects several, Ctrl+A all)
    pub sel: std::collections::BTreeSet<usize>,
    /// vertical scroll of the lanes, shared with the track column in the right sidebar
    pub scroll_y: f32,
    /// the clip pad (lane, scene) picked in the Channel Rack's launchpad: a MIDI clip opens its piano roll under the pads
    pub pad_sel: Option<(usize, usize)>,
    /// result of the last split, shown in the Playlist bar
    pub msg: String,
    drag: Option<Drag>,
    /// Ctrl+C / Ctrl+X: the clips waiting to be pasted (their notes are copied when they are pasted)
    clip_buf: Vec<Clip>,
}

/// What the selected-track panel needs from the rest of the app.
pub struct DetailCtx<'a> {
    pub roll_track: &'a mut crate::piano_roll::Track,
    pub live_track: &'a mut crate::piano_roll::Track,
    pub inst: &'a mut super::instruments_ui::Inst,
}

impl ArrState {
    pub fn is_sel(&self, i: usize) -> bool {
        self.sel.contains(&i) || self.sel_clip == Some(i)
    }

    /// the selected clips, sorted
    pub fn selection(&self) -> Vec<usize> {
        let mut v: Vec<usize> = self.sel.iter().copied().collect();
        if let Some(i) = self.sel_clip {
            if !v.contains(&i) {
                v.push(i);
            }
        }
        v.sort_unstable();
        v
    }

    pub fn new() -> Self {
        ArrState { bar_w: 44.0, sel_lane: 0, sel_sample: None, open_detail: false, open_roll: false, detail_open: false, open_editor: false, snap: 96, tool: 0, sel_clip: None, auto_edit: false, auto_master: false, auto_drag: None, auto_bend: None, sel: Default::default(), scroll_y: 0.0, pad_sel: None, msg: String::new(), drag: None, clip_buf: Vec::new() }
    }
}

fn pattern_color(i: usize) -> Color32 {
    lane_col((i + 3))
}

fn pattern_bars(p: &Pattern) -> u32 {
    ((p.len_ticks() + TICKS_PER_BAR - 1) / TICKS_PER_BAR).max(1)
}

fn audio_color() -> Color32 {
    lane_col(6)
}

/// A searchable list of the sounds of the Browser (every WAV of the library folders): returns the one that was clicked.
pub(super) fn sample_picker(ui: &mut Ui, lib: &mut Library) -> Option<std::path::PathBuf> {
    let id = egui::Id::new("clip_menu_sound_search");
    let mut q: String = ui.data(|d| d.get_temp(id)).unwrap_or_default();
    ui.add(egui::TextEdit::singleline(&mut q).hint_text("search the sounds").desired_width(230.0));
    ui.data_mut(|d| d.insert_temp(id, q.clone()));
    let low = q.trim().to_lowercase();
    let mut chosen = None;
    egui::ScrollArea::vertical().id_salt("clip_menu_sounds").max_height(300.0).show(ui, |ui| {
        let mut shown = 0;
        for f in lib.files.iter() {
            let name = f.file_stem().map(|x| x.to_string_lossy().to_string()).unwrap_or_default();
            if !low.is_empty() && !name.to_lowercase().contains(&low) {
                continue;
            }
            let kit = f.parent().and_then(|d| d.file_name()).map(|x| x.to_string_lossy().to_string()).unwrap_or_default();
            if ui.button(format!("{name}   ({kit})")).clicked() {
                chosen = Some(f.clone());
            }
            shown += 1;
            if shown >= 200 {
                ui.label(egui::RichText::new("... type to narrow the list").size(10.5).color(DIM()));
                break;
            }
        }
        if shown == 0 {
            ui.label(egui::RichText::new("no sounds found").color(DIM()));
        }
    });
    chosen
}

/// Ticks an audio file lasts at the current tempo.
fn audio_ticks(secs: f32, bpm: f32) -> u32 {
    ((secs * bpm / 60.0 * PPQ as f32).round() as u32).max(24)
}

/// snap steps of the Playlist grid: (ticks, name)
const SNAPS: [(u32, &str); 6] = [(384, "1 bar"), (192, "1/2 bar"), (96, "1/4 (beat)"), (48, "1/8"), (24, "1/16"), (6, "Off (fine)")];

/// Copy of clip `i` right after it (or at the next free spot on the same track).
pub(super) fn duplicate_clip(project: &mut Project, i: usize) -> Option<usize> {
    let c = *project.arrangement.clips.get(i)?;
    for k in 1..=64u32 {
        let mut n = Clip { start: c.start + c.len * k, ..c };
        if project.arrangement.can_place(&n, None) {
            // a MIDI clip's notes belong to the clip: the copy gets notes of its own (editing one must not change the other)
            if !c.audio && project.patterns.get(c.pattern).map_or(false, |p| p.clip_only) {
                let mut q = project.patterns[c.pattern].clone();
                q.name = format!("{} copy", q.name);
                project.patterns.push(q);
                n.pattern = project.patterns.len() - 1;
            }
            project.arrangement.clips.push(n);
            return Some(project.arrangement.clips.len() - 1);
        }
    }
    None
}

/// A copy of clip `c` that can be moved on its own: a MIDI clip's notes are copied too.
fn independent_copy(project: &mut Project, c: Clip) -> Clip {
    let mut n = c;
    if !c.audio && project.patterns.get(c.pattern).map_or(false, |p| p.clip_only) {
        let mut q = project.patterns[c.pattern].clone();
        q.name = format!("{} copy", q.name);
        project.patterns.push(q);
        n.pattern = project.patterns.len() - 1;
    }
    n
}

/// Copies of the clips `ids` placed one group-length later (the first free spot); returns their indices, empty when there is no room.
fn duplicate_group(project: &mut Project, ids: &[usize]) -> Vec<usize> {
    let clips: Vec<Clip> = ids.iter().filter_map(|&i| project.arrangement.clips.get(i).copied()).collect();
    if clips.is_empty() {
        return vec![];
    }
    let lo = clips.iter().map(|c| c.start).min().unwrap_or(0);
    let hi = clips.iter().map(|c| c.end_tick()).max().unwrap_or(0);
    let span = ((hi - lo) + TICKS_PER_BAR - 1) / TICKS_PER_BAR * TICKS_PER_BAR;
    for k in 1..=64u32 {
        let moved: Vec<Clip> = clips.iter().map(|c| Clip { start: c.start + span * k, ..*c }).collect();
        if moved.iter().all(|m| project.arrangement.can_place(m, None)) {
            let mut out = vec![];
            for m in moved {
                let n = independent_copy(project, m);
                project.arrangement.clips.push(n);
                out.push(project.arrangement.clips.len() - 1);
            }
            return out;
        }
    }
    vec![]
}

pub fn duplicate_selection(project: &mut Project, st: &mut ArrState) {
    let ids = st.selection();
    if ids.is_empty() {
        st.msg = "Select a clip first (click it, Shift+click for more, or drag a box)".into();
        return;
    }
    let new = duplicate_group(project, &ids);
    if new.is_empty() {
        st.msg = "No room for a copy".into();
    } else {
        st.sel = new.iter().copied().collect();
        st.sel_clip = new.last().copied();
    }
}

/// Ctrl+C: remember the selected clips. Returns how many.
pub fn copy_selection(project: &Project, st: &mut ArrState) -> usize {
    st.clip_buf = st.selection().iter().filter_map(|&i| project.arrangement.clips.get(i).copied()).collect();
    st.clip_buf.len()
}

/// Ctrl+V: put the remembered clips back with their layout kept, the earliest one at `at` (ticks); `lane` moves the group so its
/// top track lands on that track (only when every clip still meets a track of its own kind). Returns the number pasted.
pub fn paste_clips(project: &mut Project, st: &mut ArrState, at: u32, lane: Option<usize>) -> usize {
    if st.clip_buf.is_empty() {
        st.msg = "Nothing copied: select clips and press Ctrl+C".into();
        return 0;
    }
    let lo = st.clip_buf.iter().map(|c| c.start).min().unwrap_or(0);
    let top = st.clip_buf.iter().map(|c| c.lane).min().unwrap_or(0) as i32;
    let mut dl = lane.map_or(0, |l| l as i32 - top);
    let fits = |dl: i32, p: &Project, b: &[Clip]| b.iter().all(|c| {
        let l = c.lane as i32 + dl;
        l >= 0 && (l as usize) < p.arrangement.tracks.len() && p.arrangement.kind(l as usize) == p.arrangement.kind(c.lane)
    });
    if !fits(dl, project, &st.clip_buf) {
        dl = 0;
    }
    let moved: Vec<Clip> = st.clip_buf.iter().map(|c| Clip { start: c.start - lo + at, lane: (c.lane as i32 + dl) as usize, ..*c }).collect();
    let clash = moved.iter().enumerate().any(|(i, m)| project.arrangement.clips.iter().any(|o| m.overlaps(o)) || moved.iter().skip(i + 1).any(|o| m.overlaps(o)));
    if clash {
        st.msg = "No room there: paste on a free part of the grid".into();
        return 0;
    }
    let mut ids = vec![];
    for m in moved {
        let n = independent_copy(project, m);
        project.arrangement.clips.push(n);
        ids.push(project.arrangement.clips.len() - 1);
    }
    st.sel = ids.iter().copied().collect();
    st.sel_clip = ids.last().copied();
    ids.len()
}

/// FL-style track-card commands on every clip of one Playlist track.
#[derive(Clone, Copy, PartialEq)]
pub enum LaneOp { Cut, Copy, Paste, Fill(u32), Auto, Rotate(i32) }

pub fn lane_op(project: &mut Project, st: &mut ArrState, l: usize, op: LaneOp) {
    let mut ids: Vec<usize> = project.arrangement.clips.iter().enumerate().filter(|(_, c)| c.lane == l).map(|(i, _)| i).collect();
    ids.sort_by_key(|&i| project.arrangement.clips[i].start);
    let end_all = (project.arrangement.song_end_bar().max(1) * TICKS_PER_BAR as u32).max(TICKS_PER_BAR as u32 * 8);
    match op {
        LaneOp::Copy | LaneOp::Cut => {
            if ids.is_empty() { st.msg = "This track has no clips".into(); return; }
            st.clip_buf = ids.iter().map(|&i| project.arrangement.clips[i]).collect();
            if op == LaneOp::Cut {
                for &i in ids.iter().rev() { project.arrangement.clips.remove(i); }
                st.sel.clear();
                st.sel_clip = None;
            }
        }
        LaneOp::Paste => {
            if st.clip_buf.is_empty() { return; }
            let is_audio = project.arrangement.kind(l) == TrackKind::Audio;
            if st.clip_buf.iter().any(|c| c.audio != is_audio) { st.msg = "The copied clips are of the other track type".into(); return; }
            let lo = st.clip_buf.iter().map(|c| c.start).min().unwrap_or(0);
            let at = ids.iter().map(|&i| project.arrangement.clips[i].start + project.arrangement.clips[i].len).max().unwrap_or(0);
            let moved: Vec<Clip> = st.clip_buf.iter().map(|c| Clip { start: c.start - lo + at, lane: l, ..*c }).collect();
            for m in moved {
                let n = independent_copy(project, m);
                if project.arrangement.can_place(&n, None) { project.arrangement.clips.push(n); }
            }
        }
        LaneOp::Fill(_) | LaneOp::Auto => {
            let Some(&first) = ids.first() else { st.msg = "This track has no clips".into(); return; };
            let src = project.arrangement.clips[first];
            let period = match op { LaneOp::Fill(n) => (n * 24).max(24), _ => src.len.max(24) };
            let mut t = src.start + period;
            while t < end_all {
                let n = independent_copy(project, Clip { start: t, len: src.len.min(period), ..src });
                if project.arrangement.can_place(&n, None) { project.arrangement.clips.push(n); }
                t += period;
            }
        }
        LaneOp::Rotate(dir) => {
            if ids.is_empty() { return; }
            let before: Vec<u32> = ids.iter().map(|&i| project.arrangement.clips[i].start).collect();
            for &i in &ids {
                let c = &mut project.arrangement.clips[i];
                c.start = ((c.start as i64 + dir as i64 * 24).rem_euclid(end_all as i64)) as u32;
            }
            let bad = ids.iter().any(|&i| { let c = project.arrangement.clips[i]; c.start + c.len > end_all || project.arrangement.clips.iter().enumerate().any(|(j, o)| j != i && c.overlaps(o)) });
            if bad {
                for (k, &i) in ids.iter().enumerate() { project.arrangement.clips[i].start = before[k]; }
                st.msg = "Rotate: no room".into();
            }
        }
    }
}

pub fn delete_selection(project: &mut Project, st: &mut ArrState) {
    let mut ids = st.selection();
    ids.sort_unstable_by(|a, b| b.cmp(a));
    for i in ids {
        if i < project.arrangement.clips.len() {
            project.arrangement.clips.remove(i);
        }
    }
    st.sel.clear();
    st.sel_clip = None;
}

/// Cut every clip in `ids` into `parts` equal pieces (rounded to the snap grid). Returns how many cuts were made.
pub fn slice_into_parts(project: &mut Project, ids: &[usize], parts: u32, snap: u32) -> usize {
    if parts < 2 {
        return 0;
    }
    let mut cuts = 0;
    // from the back: cutting adds clips at the end of the list, so the indices in `ids` stay valid
    let mut ids = ids.to_vec();
    ids.sort_unstable();
    for &i in ids.iter().rev() {
        let Some(c) = project.arrangement.clips.get(i).copied() else { continue };
        let snap = snap.max(1);
        let mut at: Vec<u32> = (1..parts).map(|k| c.start + ((c.len as u64 * k as u64 / parts as u64) as u32 + snap / 2) / snap * snap).filter(|&t| t > c.start && t < c.end_tick()).collect();
        at.dedup();
        // cut the last piece first: the clip `i` always keeps the front part
        for t in at.into_iter().rev() {
            if split_clip(project, i, t) {
                cuts += 1;
            }
        }
    }
    cuts
}

/// Cut every clip in `ids` at each multiple of `step` ticks that falls inside it (a bar, a beat, a 1/8 ...).
pub fn slice_to_grid(project: &mut Project, ids: &[usize], step: u32) -> usize {
    let step = step.max(6);
    let mut cuts = 0;
    let mut ids = ids.to_vec();
    ids.sort_unstable();
    for &i in ids.iter().rev() {
        let Some(c) = project.arrangement.clips.get(i).copied() else { continue };
        let mut t = (c.start / step + 1) * step;
        let mut at = vec![];
        while t < c.end_tick() && at.len() < 256 {
            at.push(t);
            t += step;
        }
        for t in at.into_iter().rev() {
            if split_clip(project, i, t) {
                cuts += 1;
            }
        }
    }
    cuts
}

/// Cut the clips of `ids` that span tick `at` in two there (the playhead / the mouse).
pub fn slice_at(project: &mut Project, ids: &[usize], at: u32) -> usize {
    let mut n = 0;
    for &i in ids {
        if split_clip(project, i, at) {
            n += 1;
        }
    }
    n
}

/// Make the clips play `times` times as long: a MIDI clip loops its notes (it just gets longer), an audio clip is repeated as
/// independent copies after itself. Stops where the next clip begins. Returns how many clips grew or were copied.
pub fn repeat_clips(project: &mut Project, ids: &[usize], times: u32) -> usize {
    let mut done = 0;
    for &i in ids {
        let Some(c) = project.arrangement.clips.get(i).copied() else { continue };
        if c.audio {
            let mut placed = false;
            for k in 1..times {
                let n = Clip { start: c.start + c.len * k, offset: 0, ..c };
                if project.arrangement.can_place(&n, None) {
                    project.arrangement.clips.push(n);
                    placed = true;
                }
            }
            done += placed as usize;
        } else {
            let mut want = c.len * times.max(1);
            while want > c.len {
                let n = Clip { len: want, ..c };
                if project.arrangement.can_place(&n, Some(i)) {
                    project.arrangement.clips[i] = n;
                    done += 1;
                    break;
                }
                // not enough room: take whole copies of the clip as far as they fit
                want -= c.len;
            }
        }
    }
    done
}

/// Change the pitch of the clips in `ids` by `d` semitones (audio clips resample, MIDI clips transpose), limited to +-24.
/// `d == 0`... is not a move: use `reset_pitch`. Returns how many clips changed.
pub fn transpose_clips(project: &mut Project, ids: &[usize], d: i32) -> usize {
    let mut n = 0;
    for &i in ids {
        if let Some(c) = project.arrangement.clips.get_mut(i) {
            let np = (c.pitch as i32 + d).clamp(-24, 24) as i8;
            if np != c.pitch {
                c.pitch = np;
                n += 1;
            }
        }
    }
    n
}

pub fn reset_pitch(project: &mut Project, ids: &[usize]) {
    for &i in ids {
        if let Some(c) = project.arrangement.clips.get_mut(i) {
            c.pitch = 0;
        }
    }
}

/// Select every clip (`kind` None), only the audio clips (Some(true)) or only the MIDI clips (Some(false)).
pub fn select_kind(project: &Project, st: &mut ArrState, kind: Option<bool>) {
    st.sel = project.arrangement.clips.iter().enumerate().filter(|(_, c)| kind.map_or(true, |a| c.audio == a)).map(|(i, _)| i).collect();
    st.sel_clip = None;
}

/// Move the start of every clip in `ids` to the nearest grid line.
pub fn quantize_clips(project: &mut Project, ids: &[usize], snap: u32) -> usize {
    let snap = snap.max(1);
    let mut n = 0;
    for &i in ids {
        let Some(c) = project.arrangement.clips.get(i).copied() else { continue };
        let ns = (c.start + snap / 2) / snap * snap;
        if ns != c.start && project.arrangement.can_place(&Clip { start: ns, ..c }, Some(i)) {
            project.arrangement.clips[i].start = ns;
            n += 1;
        }
    }
    n
}

/// Move the clips of `ids` together by `dt` ticks (all or none, never overlapping, never before the start).
pub fn nudge_clips(project: &mut Project, ids: &[usize], dt: i64) -> bool {
    let moved: Vec<(usize, Clip)> = ids.iter().filter_map(|&i| project.arrangement.clips.get(i).map(|c| (i, *c))).map(|(i, mut c)| {
        c.start = (c.start as i64 + dt).max(0) as u32;
        (i, c)
    }).collect();
    if moved.is_empty() || ids.iter().filter_map(|&i| project.arrangement.clips.get(i)).any(|c| (c.start as i64 + dt) < 0) {
        return false;
    }
    let ok = moved.iter().all(|(_, c)| project.arrangement.clips.iter().enumerate().all(|(k, o)| moved.iter().any(|(m, _)| *m == k) || !c.overlaps(o)));
    if ok {
        for (i, c) in moved {
            project.arrangement.clips[i] = c;
        }
    }
    ok
}

/// Cut clip `i` in two at tick `at`; the second half plays on where the first stopped.
fn split_clip(project: &mut Project, i: usize, at: u32) -> bool {
    let Some(c) = project.arrangement.clips.get(i).copied() else { return false };
    if at <= c.start || at >= c.end_tick() {
        return false;
    }
    project.arrangement.clips[i].len = at - c.start;
    project.arrangement.clips.push(Clip { start: at, len: c.end_tick() - at, offset: c.offset + (at - c.start), ..c });
    true
}

/// Height of the settings block (right sidebar) = height of the hint strip above the timeline,
/// so that track rows in the sidebar line up with the lanes.
const TOP_H: f32 = 0.0;

/// Shared clip controls, used by the context menu and the Device chain.
pub fn clip_options(ui: &mut Ui, project: &mut Project, st: &mut ArrState, lib: &mut Library, i: usize, at: u32, mobile: bool) {
    if i >= project.arrangement.clips.len() { return; }
                if !mobile { ui.set_min_width(250.0); }
                let c = project.arrangement.clips[i];
                let lane = c.lane;
                let tname = project.arrangement.tracks.get(lane).map(|t| t.name.clone()).unwrap_or_default();
                let cname = if c.audio { project.audio_pool.get(c.pattern).map(|s| s.name.clone()).unwrap_or_default() } else { project.patterns.get(c.pattern).map(|p| p.name.clone()).unwrap_or_default() };
                ui.label(egui::RichText::new(format!("{}  -  {}", if c.audio { "AUDIO CLIP" } else { "MIDI CLIP" }, cname)).strong().color(lane_col(lane)));
                ui.label(egui::RichText::new(format!("on track {tname}")).size(10.5).color(DIM()));
                ui.separator();

                // ---- the sound of the clip: which plugin / which sample plays it ----
                if mobile {
                    ui.heading(if c.audio { "Sample" } else { "Instrument" });
                }
                if c.audio {
                    ui.menu_button("Replace the sound", |ui| {
                        if let Some(path) = sample_picker(ui, lib) {
                            if let Ok(data) = lib.get(&path) {
                                let name = path.file_stem().map(|x| x.to_string_lossy().to_string()).unwrap_or_default();
                                let idx = project.pool_index(&name, &path.to_string_lossy(), data);
                                project.arrangement.clips[i].pattern = idx;
                                st.sel_sample = Some(idx);
                            }
                            ui.close_menu();
                        }
                    });
                } else {
                    let cur = project.arrangement.tracks.get(lane).and_then(|t| t.instrument);
                    ui.menu_button(format!("Plugin: {}", cur.map_or("none".to_string(), |t| super::instruments_ui::Inst::from_track(t).title().to_string())), |ui| {
                        ui.label(egui::RichText::new("which instrument plays this track").size(10.5).color(DIM()));
                        for t in [crate::piano_roll::Track::Wavetable, crate::piano_roll::Track::Synth, crate::piano_roll::Track::Bass808, crate::piano_roll::Track::Analog, crate::piano_roll::Track::Sampler, crate::piano_roll::Track::Drums] {
                            let name = if t == crate::piano_roll::Track::Drums { "Drums (MIDI)" } else { super::instruments_ui::Inst::from_track(t).title() };
                            if ui.selectable_label(cur == Some(t), name).clicked() {
                                if let Some(tr) = project.arrangement.tracks.get_mut(lane) {
                                    tr.instrument = Some(t);
                                    tr.blank = false;
                                }
                                ui.close_menu();
                            }
                        }
                    });
                    if let Some(t) = cur {
                        let presets = super::instruments_ui::presets_of(t);
                        if !presets.is_empty() {
                            ui.menu_button("Preset", |ui| {
                                for name in presets {
                                    if ui.button(name).clicked() {
                                        super::instruments_ui::apply_named_lane(project, lane, t, name);
                                        ui.close_menu();
                                    }
                                }
                            });
                        }
                        if t == crate::piano_roll::Track::Sampler {
                            let cs = project.arrangement.tracks.get(lane).and_then(|x| x.sampler.sample.as_ref().map(|s| s.name.clone()));
                            ui.menu_button(format!("Sound: {}", cs.unwrap_or_else(|| "none".into())), |ui| {
                                if let Some(path) = sample_picker(ui, lib) {
                                    lib.assign_track_sampler(project, lane, &path);
                                    ui.close_menu();
                                }
                            });
                        }
                        if ui.button("Open the plugin window").clicked() {
                            st.sel_lane = lane;
                            st.open_editor = true;
                            ui.close_menu();
                        }
                    } else {
                        ui.menu_button("Sound from a WAV", |ui| {
                            if let Some(path) = sample_picker(ui, lib) {
                                lib.assign_track_sampler(project, lane, &path);
                                ui.close_menu();
                            }
                        });
                    }
                    if ui.button(if mobile { "Open Piano Roll" } else { "Open in the Piano Roll   (double-click)" }).clicked() {
                        project.current_pattern = c.pattern;
                        st.sel_lane = lane;
                        st.open_detail = true;
                        ui.close_menu();
                    }
                }
                ui.separator();

                // ---- the clip's settings ----
                if mobile {
                    ui.heading("Playback");
                }
                if c.audio {
                    let mut g = c.gain;
                    if ui.add(egui::Slider::new(&mut g, 0.0..=2.0).text("Volume")).changed() {
                        project.arrangement.clips[i].gain = g;
                    }
                    ui.horizontal(|ui| {
                        let mut rev = c.reverse;
                        if ui.checkbox(&mut rev, "Reverse").on_hover_text("Play the clip backwards").changed() {
                            project.arrangement.clips[i].reverse = rev;
                        }
                        let mut mu = c.mute;
                        if ui.checkbox(&mut mu, if mobile { "Mute clip" } else { "Mute  Ctrl+M" }).changed() {
                            project.arrangement.clips[i].mute = mu;
                        }
                    });
                    let (mut fi, mut fo) = (c.fade_in as i32, c.fade_out as i32);
                    let maxf = (c.len / 2) as i32;
                    if ui.add(egui::Slider::new(&mut fi, 0..=maxf).text("Fade in").show_value(false)).changed() {
                        project.arrangement.clips[i].fade_in = fi as u32;
                    }
                    if ui.add(egui::Slider::new(&mut fo, 0..=maxf).text("Fade out").show_value(false)).changed() {
                        project.arrangement.clips[i].fade_out = fo as u32;
                    }
                } else {
                    let mut mu = c.mute;
                    if ui.checkbox(&mut mu, if mobile { "Mute clip" } else { "Mute clip   Ctrl+M" }).changed() {
                        project.arrangement.clips[i].mute = mu;
                    }
                }
                let mut pt = c.pitch as i32;
                if ui.add(egui::Slider::new(&mut pt, -24..=24).text(if c.audio { "Pitch (st)" } else { "Transpose (st)" })).on_hover_text(if c.audio { "Resamples the sound: pitch and speed move together" } else { "Moves every note of the clip" }).changed() {
                    project.arrangement.clips[i].pitch = pt as i8;
                }
                // tonality: say which key the clip is in and it is moved into the key of the project (like a pattern key in FL)
                {
                    let (kr, ks) = (project.key_root, project.key_scale);
                    let mut from: Option<u8> = None;
                    ui.horizontal(|ui| {
                        ui.label("Key");
                        egui::ComboBox::from_id_salt(("clip_key", i)).width(80.0).selected_text("clip is in...").show_ui(ui, |ui| {
                            for pc in 0..12u8 {
                                if ui.selectable_label(false, super::super::theory::pc_name(kr, ks, pc)).clicked() {
                                    from = Some(pc);
                                }
                            }
                        }).response.on_hover_text("Pick the key this clip is in: it is shifted into the project key");
                        ui.label(egui::RichText::new(format!("-> {}", super::super::theory::pc_name(kr, ks, kr))).color(DIM()));
                        if c.pitch != 0 && ui.small_button("Reset").clicked() {
                            project.arrangement.clips[i].pitch = 0;
                        }
                    });
                    if let Some(pc) = from {
                        // the shortest way (-6..+5 semitones) from the clip's key to the project key
                        let shift = (kr as i32 - pc as i32 + 18).rem_euclid(12) - 6;
                        project.arrangement.clips[i].pitch = shift as i8;
                    }
                }
                ui.separator();

                // ---- cut, copy, convert ----
                if mobile {
                    ui.heading("Clip actions");
                }
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Split here").on_hover_text("Cut the clip in two at this grid position").clicked() {
                        split_clip(project, i, at);
                        ui.close_menu();
                    }
                    if ui.button("Duplicate").on_hover_text("Ctrl+D").clicked() {
                        st.sel_clip = duplicate_clip(project, i);
                        ui.close_menu();
                    }
                    if ui.button("Delete").clicked() {
                        project.arrangement.clips.remove(i);
                        st.sel.clear();
                        st.sel_clip = None;
                        ui.close_menu();
                    }
                });
                if !mobile {
                    ui.label(egui::RichText::new("drag: move  -  Ctrl/Alt+drag: copy  -  Shift+click: several  -  Delete: remove").size(10.0).color(DIM()));
                }
                if !c.audio {
                    ui.menu_button("Convert", |ui| {
                        if ui.button("Split by channel -> MIDI tracks").clicked() {
                            let n = crate::bounce::split_clip_midi(project, i);
                            st.sel_clip = None;
                            st.msg = if n == 0 { "Nothing to split in that clip".into() } else { format!("Split into {n} MIDI tracks") };
                            ui.close_menu();
                        }
                        if ui.button("Split by channel -> Audio tracks").clicked() {
                            st.msg = match crate::bounce::split_clip_audio(project, i) {
                                Ok(0) => "Nothing audible in that clip".into(),
                                Ok(n) => format!("Rendered {n} channels to audio tracks"),
                                Err(e) => format!("Split failed: {e}"),
                            };
                            st.sel_clip = None;
                            ui.close_menu();
                        }
                    });
                }
}

pub fn show(ui: &mut Ui, project: &mut Project, st: &mut ArrState, shared: &Shared, lib: &mut Library, dc: &mut DetailCtx) {
    project.arrangement.ensure_tracks();
    // on a wide window the track headers live in the right sidebar; on a narrow one they stay next to the lanes
    let inline = false; // Track headers always live in the fixed column to the left.
    let total = if inline { ui.available_width() - 14.0 - HEADER_W - 8.0 } else { ui.available_width() - 14.0 };
    let height = TOP_H + RULER_H + project.arrangement.tracks.len() as f32 * lane_h() + lane_h() + 8.0 + extra_h(st, project.arrangement.tracks.len());
    let scrolled = egui::ScrollArea::vertical().id_salt("arr_vscroll").auto_shrink([false, false]).vertical_scroll_offset(st.scroll_y).show(ui, |ui| {
    ui.horizontal_top(|ui| {
        // timeline
        ui.allocate_ui_with_layout(vec2(total.max(200.0), height), egui::Layout::top_down(egui::Align::LEFT), |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            timeline(ui, project, st, shared, lib);
        });
        if inline {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.add_space(TOP_H);
                tracks_panel(ui, project, st, dc);
            });
        }
    });
    });
    st.scroll_y = scrolled.state.offset.y;
}

/// The track headers as a column of the right sidebar. Its rows line up with the lanes of the timeline: both start at the same
/// height and share one scroll position.
pub fn tracks_column(ui: &mut Ui, project: &mut Project, st: &mut ArrState, dc: &mut DetailCtx) {
    let out = egui::ScrollArea::vertical()
        .id_salt("tracks_col_scroll")
        .auto_shrink([false, false])
        .vertical_scroll_offset(st.scroll_y)
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            tracks_panel(ui, project, st, dc);
        });
    st.scroll_y = out.state.offset.y;
}

/// The track list of the active pattern (MIDI + audio tracks, new track, master) for the left panel.
pub fn tracks_panel(ui: &mut Ui, project: &mut Project, st: &mut ArrState, dc: &mut DetailCtx) {
    project.arrangement.ensure_tracks();
    let prev_sel = st.sel_lane;
    track_headers(ui, project, st, dc);
    if st.sel_lane != prev_sel {
        let lane = st.sel_lane;
        if let Some(c) = project.arrangement.clips.iter().find(|c| c.lane == lane && !c.audio) {
            if c.pattern < project.patterns.len() {
                project.current_pattern = c.pattern;
            }
        }
    }
}

/// Selected track under the track list: strip, plugin and a drop zone (the full editor opens in a window).
fn detail_column(ui: &mut Ui, project: &mut Project, st: &mut ArrState, dc: &mut DetailCtx) {
    let lane = st.sel_lane;
    if lane >= project.arrangement.tracks.len() {
        return;
    }
    super::track_ui::header_vertical(ui, project, lane);
    if project.arrangement.tracks[lane].kind == TrackKind::Midi {
        let cur = project.arrangement.tracks[lane].instrument;
        if let Some(t) = cur {
            ui.label(egui::RichText::new(t.name()).strong().color(TEAL()));
            if ui.button("Open editor").clicked() {
                st.open_editor = true;
            }
        }
        ui.add_space(4.0);
        let (rect, resp) = ui.allocate_exact_size(vec2(HEADER_W - 16.0, 56.0), Sense::hover());
        let over = resp.dnd_hover_payload::<DragItem>().is_some();
        let zone = rect.shrink(2.0);
        if over {
            ui.painter().rect_filled(zone, corner(), TEAL().linear_multiply(0.10));
            ui.painter().rect_stroke(zone, corner(), Stroke::new(2.0, TEAL()));
        } else {
            dashed_rect(ui.painter(), zone, ui.clip_rect(), corner(), Stroke::new(1.0, MID()));
        }
        let hint = if cur.is_some() { "Drop a plugin to replace" } else { "Empty - drop a plugin here" };
        ui.painter().text(rect.center(), Align2::CENTER_CENTER, hint, FontId::proportional(12.0), DIM());
        if let Some(pl) = resp.dnd_release_payload::<DragItem>() {
            if let DragItem::Instrument(t) = &*pl {
                project.arrangement.tracks[lane].instrument = Some(*t);
                *dc.roll_track = *t;
                *dc.live_track = *t;
                *dc.inst = super::instruments_ui::Inst::from_track(*t);
            }
        }
    } else if ui.button("Open sound view").clicked() {
        st.open_editor = true;
    }
}

/// The song's patterns (FL Studio style): one shared Playlist; patterns are picked here and dragged onto its tracks.
pub fn pattern_strip(ui: &mut Ui, project: &mut Project) {
    section_label(ui, "PATTERNS");
    ui.add_space(2.0);
    let mut pick: Option<usize> = None;
    let mut menu: Option<(usize, u8)> = None;
    for i in 0..project.patterns.len() {
        if project.patterns[i].clip_only {
            continue;
        }
        let clips = project.arrangement.clips.iter().filter(|c| !c.audio && c.pattern == i).count();
        let sel = i == project.current_pattern;
        let (rect, r) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::click_and_drag());
        let pp = ui.painter();
        pp.rect_filled(rect, corner(), if sel { TEAL().linear_multiply(0.22) } else if r.hovered() { PANEL3() } else { PANEL2() });
        if sel {
            pp.rect_stroke(rect, corner(), Stroke::new(1.0, TEAL().linear_multiply(0.7)));
        }
        pp.rect_filled(Rect::from_min_size(rect.min + vec2(5.0, 6.0), vec2(4.0, rect.height() - 12.0)), 2.0, pattern_color(i));
        pp.text(rect.left_center() + vec2(18.0, 0.0), Align2::LEFT_CENTER, &project.patterns[i].name, FontId::proportional(13.0), TEXT());
        pp.text(rect.right_center() - vec2(8.0, 0.0), Align2::RIGHT_CENTER, format!("{clips} clips"), FontId::proportional(10.0), DIM());
        r.dnd_set_drag_payload(super::browser::DragItem::Pattern(i));
        if r.clicked() {
            pick = Some(i);
        }
        let hover = r.clone();
        r.context_menu(|ui| {
            ui.label(egui::RichText::new("Rename").size(11.0).color(DIM()));
            ui.add(egui::TextEdit::singleline(&mut project.patterns[i].name).desired_width(150.0));
            ui.separator();
            for (n, label) in [(0u8, "Clone"), (1, "Delete"), (2, "Move up"), (3, "Move down"), (4, "Split by channel")] {
                if n == 2 || n == 4 {
                    ui.separator();
                }
                if ui.button(label).clicked() {
                    menu = Some((i, n));
                    ui.close_menu();
                }
            }
        });
        hover.on_hover_text("click: edit in the Channel Rack / Piano Roll - drag onto a MIDI track in the Playlist - right-click: menu");
        ui.add_space(2.0);
    }
    if let Some(i) = pick {
        project.current_pattern = i;
    }
    if let Some((i, n)) = menu {
        let visible = |p: &Project, k: usize| !p.patterns[k].clip_only;
        match n {
            0 => {
                project.current_pattern = i;
                project.duplicate_pattern();
            }
            1 => {
                if project.patterns.iter().filter(|p| !p.clip_only).count() > 1 {
                    project.current_pattern = i;
                    project.delete_pattern();
                }
            }
            2 => {
                if let Some(j) = (0..i).rev().find(|&k| visible(project, k)) {
                    swap_patterns(project, i, j);
                }
            }
            3 => {
                if let Some(j) = (i + 1..project.patterns.len()).find(|&k| visible(project, k)) {
                    swap_patterns(project, i, j);
                }
            }
            _ => {
                project.split_pattern_by_channel(i, true, 1);
            }
        }
    }
    ui.horizontal(|ui| {
        if ui.small_button("+ New").on_hover_text("A new empty pattern").clicked() {
            project.new_pattern();
        }
        if ui.small_button("Dup").on_hover_text("Copy this pattern with its notes").clicked() {
            project.duplicate_pattern();
        }
        if ui.add_enabled(project.patterns.iter().filter(|p| !p.clip_only).count() > 1 && !project.patterns[project.current_pattern.min(project.patterns.len() - 1)].clip_only, egui::Button::new("Del").small()).clicked() {
            project.delete_pattern();
        }
    });
}

/// The Playlist's tools: loop length, zoom, snap, tools, brush and splitting. Lives in the page bar next to the tabs.
pub fn toolbar(ui: &mut Ui, project: &mut Project, st: &mut ArrState) {
    ui.spacing_mut().item_spacing = vec2(7.0, 6.0);
    ui.spacing_mut().interact_size.y = 28.0;
    ui.spacing_mut().slider_width = 92.0;
    // tools: one segment group
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
                ui.selectable_value(&mut st.tool, 0, "Select").on_hover_text("1 - click: select / move - Shift+click: several - drag on empty space: box select - Ctrl/Alt+drag: copy - edges: stretch / trim");
                ui.selectable_value(&mut st.tool, 1, "Cut").on_hover_text("2 - click a clip to cut it in two at the grid");
                ui.toggle_value(&mut st.auto_edit, "Auto").on_hover_text("Level automation: draw a curve over the selected track's lane (volume goes up and down while the song plays)");
                if st.auto_edit {
                    ui.selectable_value(&mut st.auto_master, false, "track");
                    ui.selectable_value(&mut st.auto_master, true, "master");
                }
    });
    ui.separator();
    // one button with a drop-down: "Snap 1/4 (beat)"
    let snap_before = st.snap;
    egui::ComboBox::from_id_salt("pl_snap").width(150.0).selected_text(format!("Snap  {}", SNAPS.iter().find(|s| s.0 == st.snap).map_or("custom", |s| s.1))).show_ui(ui, |ui| {
        for (t, n) in SNAPS {
            ui.selectable_value(&mut st.snap, t, n);
        }
    });
    // a finer snap zooms in until its steps are wide enough to see and hit
    if st.snap != snap_before && st.snap > 0 && st.snap < 384 {
        let need = 9.0 * TICKS_PER_BAR as f32 / st.snap as f32;
        if st.bar_w < need {
            st.bar_w = need.min(640.0);
        }
    }
    ui.menu_button("View", |ui| {
        ui.set_min_width(220.0);
        ui.label(egui::RichText::new("Zoom").color(DIM()));
        ui.add(egui::Slider::new(&mut st.bar_w, 12.0..=640.0).logarithmic(true).show_value(false));
        let mut lh = lane_h();
        ui.label(egui::RichText::new("Track height").color(DIM()));
        if ui.add(egui::Slider::new(&mut lh, 28.0..=260.0).show_value(false)).on_hover_text("Alt+wheel or Ctrl+Shift+wheel over the lanes").changed() {
            set_lane_h(lh);
        }
    });
    ui.menu_button("Edit", |ui| {
        ui.set_min_width(150.0);
                ui.menu_button("All...", |ui| {
                    if ui.button("All clips   Ctrl+A").clicked() {
                        select_kind(project, st, None);
                        ui.close_menu();
                    }
                    if ui.button("All audio clips").clicked() {
                        select_kind(project, st, Some(true));
                        ui.close_menu();
                    }
                    if ui.button("All MIDI clips").clicked() {
                        select_kind(project, st, Some(false));
                        ui.close_menu();
                    }
                });
                ui.menu_button("Pitch", |ui| {
                    let ids = st.selection();
                    ui.label(egui::RichText::new("pitch of the selected clips - audio resamples (speed moves with it), MIDI transposes").size(10.5).color(DIM()));
                    ui.label(egui::RichText::new("Shift+Up / Down = 1 semitone, Ctrl+Shift+Up / Down = octave").size(10.5).color(DIM()));
                    let mut msg = None;
                    ui.horizontal(|ui| {
                        for (d, n) in [(-12, "-12"), (-7, "-7"), (-1, "-1"), (1, "+1"), (7, "+7"), (12, "+12")] {
                            if ui.button(n).clicked() {
                                msg = Some(transpose_clips(project, &ids, d));
                            }
                        }
                    });
                    if ui.button("Reset to 0").clicked() {
                        reset_pitch(project, &ids);
                        msg = Some(ids.len());
                    }
                    if ids.is_empty() {
                        ui.label(egui::RichText::new("Select clips first (Select menu, Ctrl+A, a box or Shift+click)").size(10.5).color(AMBER));
                    }
                    if let Some(c) = msg {
                        st.msg = format!("{c} clip(s) pitched");
                    }
                });
                ui.menu_button("Slice", |ui| {
                    let ids = st.selection();
                    ui.label(egui::RichText::new("cuts the selected clips (all clips when none is selected)").size(10.5).color(DIM()));
                    let ids = if ids.is_empty() { (0..project.arrangement.clips.len()).collect() } else { ids };
                    let mut msg = None;
                    ui.label(egui::RichText::new("into equal parts  (Alt+2 / 3 / 4 / 6 / 8)").size(10.5).color(DIM()));
                    ui.horizontal(|ui| {
                        for n in [2u32, 3, 4, 6, 8] {
                            if ui.button(format!("{n}")).clicked() {
                                msg = Some(slice_into_parts(project, &ids, n, st.snap));
                                ui.close_menu();
                            }
                        }
                    });
                    ui.label(egui::RichText::new("on the grid").size(10.5).color(DIM()));
                    for (t, n) in [(384u32, "every bar"), (192, "every 1/2 bar"), (96, "every beat"), (48, "every 1/8"), (24, "every 1/16")] {
                        if ui.button(n).clicked() {
                            msg = Some(slice_to_grid(project, &ids, t));
                            ui.close_menu();
                        }
                    }
                    if let Some(c) = msg {
                        st.msg = format!("{c} cut(s)");
                        st.sel.clear();
                    }
                });
                ui.menu_button("Repeat", |ui| {
                    let ids = st.selection();
                    ui.label(egui::RichText::new("the selected clips play longer: MIDI loops its notes, audio is copied (Ctrl+R = x2)").size(10.5).color(DIM()));
                    for n in [2u32, 3, 4, 8, 16] {
                        if ui.button(format!("x {n}")).clicked() {
                            let c = repeat_clips(project, &ids, n);
                            st.msg = if ids.is_empty() { "Select a clip first".into() } else { format!("{c} clip(s) repeated") };
                            ui.close_menu();
                        }
                    }
                    ui.separator();
                    if ui.button("Quantize starts to the grid  (Ctrl+Q)").clicked() {
                        let c = quantize_clips(project, &ids, st.snap);
                        st.msg = format!("{c} clip(s) quantized");
                        ui.close_menu();
                    }
                });
                if ui.button("Dup").on_hover_text("Duplicate the selected clips (Ctrl+D)").clicked() {
                    duplicate_selection(project, st);
                }
                if ui.button("Del").on_hover_text("Delete the selected clips (Delete)").clicked() {
                    delete_selection(project, st);
                }
                ui.separator();
                if ui.button("Clear all clips").on_hover_text("Remove every clip from the Playlist").clicked() {
                    project.arrangement.clips.clear();
                    ui.close_menu();
                }
    });
    ui.separator();
    ui.checkbox(&mut project.loop_on, "Loop").on_hover_text("Off: Song mode plays the whole arrangement. On: only the marked bars repeat (drag on the ruler to mark them).");
    if project.loop_on {
        ui.add(egui::DragValue::new(&mut project.loop_bars).range(1..=256).suffix(" bars"));
    }
    // a position slider: hold it and drag to wind the timeline (the Playlist playhead follows)
    {
        let pos_id = egui::Id::new("arr_play_pos");
        let cur: u32 = ui.ctx().data(|d| d.get_temp(pos_id)).unwrap_or(0);
        let mut bars_f = cur as f32 / TICKS_PER_BAR as f32;
        let end = (project.arrangement.song_end_bar().max(project.loop_start + project.loop_bars) + 2).max(8) as f32;
        ui.spacing_mut().slider_width = 130.0;
        let r = ui.add(egui::Slider::new(&mut bars_f, 0.0..=end).show_value(false)).on_hover_text("Timeline: hold and drag to wind the song (bars)");
        if r.changed() {
            let t = ((bars_f * TICKS_PER_BAR as f32) as u32) / 24 * 24;
            ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("arr_seek_req"), t));
        }
        ui.label(egui::RichText::new(format!("{:.1}", bars_f + 1.0)).monospace().color(DIM()).size(11.0));
    }
    ui.separator();
                let name = project.patterns[project.current_pattern].name.clone();
                ui.label(egui::RichText::new(format!("Brush: {name}")).color(TEAL()).strong().size(12.0));
                if !st.msg.is_empty() {
                    ui.label(egui::RichText::new(&st.msg).color(TEAL()).size(12.0));
                }
}

/// A new empty MIDI clip on the selected MIDI track (a new track when another kind is selected).
pub fn new_midi_clip(project: &mut Project, st: &mut ArrState) {
    let lane = if st.sel_lane < project.arrangement.tracks.len() && project.arrangement.kind(st.sel_lane) == TrackKind::Midi {
        st.sel_lane
    } else {
        project.arrangement.add_track(TrackKind::Midi);
        project.arrangement.tracks.len() - 1
    };
    let start = project.arrangement.clips.iter().filter(|c| c.lane == lane).map(|c| c.end_tick()).max().unwrap_or(0);
    new_midi_clip_at(project, st, lane, start, 2 * TICKS_PER_BAR);
}

pub(super) fn new_midi_clip_at(project: &mut Project, st: &mut ArrState, lane: usize, start: u32, duration: u32) -> bool {
    if !project.arrangement.tracks.get(lane).is_some_and(|track| track.kind == TrackKind::Midi) {
        return false;
    }
    let next = project.arrangement.clips.iter().filter(|c| c.lane == lane && c.start >= start)
        .map(|c| c.start - start).min().unwrap_or(duration);
    let len = duration.min(next);
    if len == 0 {
        return false;
    }
    let pi = project.patterns.len();
    let clip = Clip::new(pi, lane, start, len, false);
    if !project.arrangement.can_place(&clip, None) {
        return false;
    }
    let n = project.patterns.iter().filter(|p| p.clip_only).count() + 1;
    let steps = len.div_ceil(crate::sequencer::TICKS_PER_STEP);
    let mut pattern = crate::sequencer::Pattern::new(&format!("MIDI clip {n}"), steps);
    pattern.clip_only = true;
    project.patterns.push(pattern);
    project.arrangement.clips.push(clip);
    st.sel_clip = Some(project.arrangement.clips.len() - 1);
    st.sel_lane = lane;
    project.current_pattern = pi;
    true
}

/// Copy a clip right after itself (a MIDI clip with its own notes, a pattern clip keeps pointing at its pattern).
pub fn copy_clip_after(project: &mut Project, i: usize) {
    if i >= project.arrangement.clips.len() {
        return;
    }
    let c = project.arrangement.clips[i];
    let mut n = Clip { start: c.end_tick(), offset: 0, ..c };
    if project.arrangement.can_place(&n, None) {
        if !c.audio && project.patterns.get(c.pattern).map_or(false, |p| p.clip_only) {
            let mut q = project.patterns[c.pattern].clone();
            q.name = format!("{} copy", q.name);
            project.patterns.push(q);
            n.pattern = project.patterns.len() - 1;
        }
        project.arrangement.clips.push(n);
    }
}

/// Clips of the Playlist as a list (right column, above the player): click selects the clip and its track, the buttons
/// duplicate or delete it.
pub fn clip_list(ui: &mut Ui, project: &mut Project, st: &mut ArrState) {
    ui.horizontal(|ui| {
        section_label(ui, "CLIPS");
        if ui.small_button("+ MIDI clip").on_hover_text("A new empty MIDI clip on the selected MIDI track (its notes belong to the clip, not to a pattern) - Ctrl+Shift+M").clicked() {
            new_midi_clip(project, st);
        }
    });
    let mut act: Option<(usize, u8)> = None;
    egui::ScrollArea::vertical().id_salt("clip_list").max_height(150.0).auto_shrink([false, true]).show(ui, |ui| {
        if project.arrangement.clips.is_empty() {
            ui.label(egui::RichText::new("No clips yet - drag a pattern or a sound onto a track").color(DIM()).size(11.0));
        }
        for i in 0..project.arrangement.clips.len() {
            let c = project.arrangement.clips[i];
            let track = project.arrangement.tracks.get(c.lane).map(|t| t.name.clone()).unwrap_or_default();
            let what = if c.audio { project.audio_pool.get(c.pattern).map(|s| s.name.clone()).unwrap_or_default() } else { project.patterns.get(c.pattern).map(|p| p.name.clone()).unwrap_or_default() };
            ui.horizontal(|ui| {
                let sel = st.sel_clip == Some(i);
                let col = lane_col(c.lane);
                let kind = if c.audio { "audio" } else if project.patterns.get(c.pattern).map_or(false, |p| p.clip_only) { "MIDI clip" } else { "pattern" };
                let txt = format!("{} - {} [{}] bar {}, {}b", track, what, kind, c.start_bar() + 1, (c.len + TICKS_PER_BAR - 1) / TICKS_PER_BAR);
                // the Dup / x buttons are pinned to the right edge (same place in every row), the text takes what is left
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("x").on_hover_text("Delete this clip").clicked() {
                        act = Some((i, 1));
                    }
                    if ui.small_button("Dup").on_hover_text("Copy this clip right after itself").clicked() {
                        act = Some((i, 0));
                    }
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        let max_chars = ((ui.available_width() - 8.0) / 6.0).max(6.0) as usize;
                        let txt = if txt.chars().count() > max_chars { format!("{}...", txt.chars().take(max_chars.saturating_sub(3)).collect::<String>()) } else { txt.clone() };
                        if ui.selectable_label(sel, egui::RichText::new(txt).size(11.0).color(if sel { TEXT() } else { col })).clicked() {
                            st.sel_clip = Some(i);
                            st.sel_lane = c.lane;
                        }
                    });
                });
            });
        }
    });
    match act {
        Some((i, 0)) => copy_clip_after(project, i),
        Some((i, _)) => {
            project.arrangement.clips.remove(i);
            st.sel.clear();
            st.sel_clip = None;
        }
        None => {}
    }
}

/// Exchange two patterns in the list; every clip that plays one of them follows it.
fn swap_patterns(project: &mut Project, i: usize, j: usize) {
    project.patterns.swap(i, j);
    let map = |x: usize| if x == i { j } else if x == j { i } else { x };
    for c in project.arrangement.clips.iter_mut().filter(|c| !c.audio) {
        c.pattern = map(c.pattern);
    }
    for c in project.session.clips.iter_mut().filter(|c| !c.audio) {
        c.pattern = map(c.pattern);
    }
    project.current_pattern = map(project.current_pattern);
}

/// The master level, kept next to the pattern list.
pub fn master_card(ui: &mut Ui, project: &mut Project) {
    ui.add_space(14.0);
    egui::Frame::none().fill(PANEL3()).rounding(card()).inner_margin(egui::Margin::symmetric(10.0, 8.0)).stroke(Stroke::new(border_w(), border())).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(egui::RichText::new("MASTER").size(11.0).strong().color(TEAL_HI()));
        ui.spacing_mut().slider_width = ui.available_width() - 70.0;
        ui.horizontal(|ui| {
            ui.add(egui::Slider::new(&mut project.master_volume, 0.0..=1.2).show_value(false)).on_hover_text("Master volume");
            ui.label(egui::RichText::new(format!("{:+.1} dB", super::widgets::db(project.master_volume))).monospace().size(10.0).color(DIM()));
        });
    });
}

/// 16 ... 256 step length of the current pattern (lives in the main toolbar)
pub fn steps_selector(ui: &mut Ui, project: &mut Project) {
    let cur = project.current_pattern;
    let pat = &mut project.patterns[cur];
    ui.vertical(|ui| {
        ui.label(egui::RichText::new("STEPS").size(10.0).color(DIM()));
        ui.horizontal(|ui| {
            for st in [16u32, 32, 64, 128, 256] {
                ui.selectable_value(&mut pat.length_steps, st, st.to_string());
            }
        });
    });
    let max = pat.len_ticks();
    pat.notes.retain(|n| n.start < max);
}

/// Compact level control of a track header: click or drag to set, double-click for unity.
fn level_bar(ui: &mut Ui, v: &mut f32) {
    let (rect, resp) = ui.allocate_exact_size(vec2(62.0, 16.0), Sense::click_and_drag());
    if resp.dragged() || resp.clicked() {
        if let Some(p) = resp.interact_pointer_pos() {
            *v = ((p.x - rect.left()) / rect.width() * 1.5).clamp(0.0, 1.5);
        }
    }
    if resp.double_clicked() {
        *v = 1.0;
    }
    let bar = rect.shrink2(vec2(0.0, 5.0));
    let p = ui.painter();
    p.rect_filled(bar, 1.0, INSET());
    p.rect_filled(Rect::from_min_size(bar.min, vec2(bar.width() * (*v / 1.5), bar.height())), 1.0, TEAL());
    let unity = bar.left() + bar.width() / 1.5;
    p.line_segment([pos2(unity, bar.top() - 2.0), pos2(unity, bar.bottom() + 2.0)], Stroke::new(1.0, LIGHT()));
    resp.on_hover_text(format!("Track level {:+.1} dB (double-click: unity)", super::widgets::db(*v)));
}

/// A small square on / off button of a track header (mute, solo, arm).
fn flag(ui: &mut Ui, v: &mut bool, label: &str, on: egui::Color32, tip: &str) {
    // painted by hand: the global button padding would make these pills huge
    let (r, resp) = ui.allocate_exact_size(vec2(24.0, 22.0), Sense::click());
    let fill = if *v { on } else if resp.hovered() { PANEL3().linear_multiply(1.2) } else { PANEL3() };
    ui.painter().rect_filled(r, 2.0, fill);
    ui.painter().text(r.center(), Align2::CENTER_CENTER, label, egui::FontId::proportional(11.0), if *v { egui::Color32::WHITE } else { DIM() });
    if resp.on_hover_text(tip).clicked() {
        *v = !*v;
    }
}

/// Track list: MIDI and Audio tracks with kind badge, name, mute and remove.
fn track_headers(ui: &mut Ui, project: &mut Project, st: &mut ArrState, dc: &mut DetailCtx) {
    let mut remove: Option<usize> = None;
    let mut render_req: Option<usize> = None;
    let mut clone_req: Option<usize> = None;
    let mut lane_req: Option<(usize, LaneOp)> = None;
    let can_paste = !st.clip_buf.is_empty();
    let mut midi_req: Option<usize> = None;
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        let (rect, _) = ui.allocate_exact_size(vec2(HEADER_W, RULER_H), Sense::hover());
        ui.painter().rect_filled(rect, 0.0, PANEL3());
        ui.painter().line_segment([rect.left_bottom(), rect.right_bottom()], Stroke::new(1.0, border()));
        // two labelled add buttons: the kind is spelled out, so a MIDI and an audio track are never mixed up
        let half = rect.width() * 0.5;
        for (k, (name, kind, tip)) in [("MIDI", TrackKind::Midi, "Add a MIDI track (pattern clips)"), ("AUDIO", TrackKind::Audio, "Add an audio track (WAV clips)")].into_iter().enumerate() {
            let cell = Rect::from_min_size(rect.min + vec2(half * k as f32, 0.0), vec2(half, rect.height())).shrink2(vec2(3.0, 2.0));
            let resp = ui.interact(cell, ui.id().with(("add_track", k)), Sense::click());
            let p = ui.painter();
            p.rect_filled(cell, corner(), if resp.hovered() { tint(PANEL2(), TEAL(), 0.25) } else { PANEL2() });
            p.rect_stroke(cell, corner(), Stroke::new(border_w(), if resp.hovered() { TEAL() } else { border() }));
            p.text(cell.left_center() + vec2(9.0, 0.0), Align2::LEFT_CENTER, name, FontId::proportional(11.5), TEXT());
            p.text(cell.right_center() - vec2(10.0, 0.0), Align2::CENTER_CENTER, "+", FontId::proportional(16.0), TEAL());
            if resp.on_hover_text(tip).clicked() {
                project.arrangement.add_track(kind);
                st.sel_lane = project.arrangement.tracks.len() - 1;
            }
        }
        for l in 0..project.arrangement.tracks.len() {
            if project.arrangement.hidden(l) {
                continue;
            }
            let (rect, hresp) = ui.allocate_exact_size(vec2(HEADER_W, lane_h()), Sense::click_and_drag());
            // drag the track card onto an insert of the mixer to route it there
            hresp.dnd_set_drag_payload(TrackDrag(l));
            hresp.context_menu(|ui| {
                if ui.button("Render this track to audio").on_hover_text("All its clips become one audio clip on a new track right below (the source is muted)").clicked() {
                    render_req = Some(l);
                    ui.close_menu();
                }
                ui.checkbox(&mut project.arrangement.tracks[l].sidechain_src, "Sidechain source").on_hover_text("Every hit of this track ducks the mixer inserts that have a Duck amount (kick -> 808 / bass pump)");
            });
            let kind = project.arrangement.tracks[l].kind;
            let muted = project.arrangement.tracks[l].mute;
            let col = lane_col(l);
            let p = ui.painter();
            // rounded card with a colour stripe on the left; the row keeps its full allocated size for hit-testing
            let card_r = rect.shrink2(vec2(1.0, 2.0));
            p.rect_filled(card_r, 2.0, if muted { SIDE() } else { PANEL2() });
            let sel_t = super::widgets::fade(ui, hresp.id.with("s"), st.sel_lane == l, 0.16);
            let hov_t = super::widgets::fade(ui, hresp.id.with("h"), hresp.hovered(), 0.12);
            p.rect_filled(card_r, 2.0, TEAL().gamma_multiply(0.04 * hov_t * (1.0 - sel_t) + 0.08 * sel_t));
            p.rect_stroke(card_r, 2.0, Stroke::new(1.0, border()));
            p.rect_filled(Rect::from_min_size(card_r.min + vec2(4.0, 6.0), vec2(3.0, card_r.height() - 12.0)), 1.5, if muted { MID() } else { col });
            // number, colour tab with the number again, name
            let name_s = project.arrangement.tracks[l].name.clone();
            let ink = if muted { DIM() } else { TEXT() };
            let idx_box = Rect::from_min_size(pos2(card_r.left() + 14.0, card_r.top() + 5.0), vec2(22.0, card_r.height() - 10.0));
            p.rect_filled(idx_box, 2.0, if muted { MID() } else { col });
            p.text(idx_box.center(), Align2::CENTER_CENTER, format!("{}", l + 1), FontId::proportional(12.0), Color32::WHITE);
            let name_rect = Rect::from_min_max(pos2(idx_box.right() + 10.0, card_r.top() + 8.0), pos2(card_r.right() - 118.0, card_r.bottom()));
            // folder: a stripe on the left of every member, a fold chip on the first one
            let gid = project.arrangement.tracks[l].group;
            let mut first_of_group = false;
            if gid != 0 {
                let gcol = lane_col(gid as usize + 2);
                p.rect_filled(Rect::from_min_size(rect.left_top() + vec2(0.0, 3.0), vec2(3.0, rect.height() - 6.0)), 1.0, gcol);
                first_of_group = project.arrangement.tracks.iter().position(|t| t.group == gid) == Some(l);
            }
            p.with_clip_rect(name_rect).text(name_rect.left_center() + vec2(0.0, if first_of_group { 6.0 } else { 0.0 }), Align2::LEFT_CENTER, &name_s, FontId::proportional(14.0), ink);
            if first_of_group {
                let (gname, coll) = project.arrangement.groups.iter().find(|g| g.id == gid).map(|g| (g.name.clone(), g.collapsed)).unwrap_or_default();
                let n = project.arrangement.group_size(gid);
                let chip = Rect::from_min_size(pos2(name_rect.left() - 2.0, card_r.top() + 2.0), vec2(name_rect.width() + 2.0, 14.0));
                let cresp = ui.interact(chip, hresp.id.with("fold"), Sense::click());
                let pp = ui.painter();
                pp.rect_filled(chip, 4.0, if cresp.hovered() { PANEL3() } else { Color32::TRANSPARENT });
                let c0 = pos2(chip.left() + 7.0, chip.center().y);
                let tri = if coll { vec![c0 + vec2(-2.5, -4.0), c0 + vec2(3.0, 0.0), c0 + vec2(-2.5, 4.0)] } else { vec![c0 + vec2(-4.0, -2.5), c0 + vec2(4.0, -2.5), c0 + vec2(0.0, 3.0)] };
                pp.add(egui::Shape::convex_polygon(tri, TEAL(), Stroke::NONE));
                pp.with_clip_rect(chip).text(pos2(chip.left() + 16.0, chip.center().y), Align2::LEFT_CENTER, format!("{gname}  ({n})"), FontId::proportional(10.5), TEAL_HI());
                if cresp.on_hover_text("Fold / unfold this folder").clicked() {
                    if let Some(g) = project.arrangement.groups.iter_mut().find(|g| g.id == gid) {
                        g.collapsed = !g.collapsed;
                    }
                }
            }
            p.text(pos2(card_r.right() - 118.0, card_r.top() + 6.0), Align2::RIGHT_TOP, if kind == TrackKind::Midi { "MIDI" } else { "AUDIO" }, FontId::proportional(8.5), LIGHT());
            if st.sel_lane == l {
                p.rect_stroke(card_r, 2.0, Stroke::new(1.0, TEAL()));
            }
            if hresp.clicked() {
                st.sel_lane = l;
            }
            // drag the number tab to move the track to another place
            {
                let drag_id = egui::Id::new("track_reorder_src");
                let tgt_id = egui::Id::new("track_reorder_tgt");
                let grab = ui.interact(idx_box, hresp.id.with("grab"), Sense::drag());
                let grab = grab.on_hover_cursor(egui::CursorIcon::Grab).on_hover_text("Drag to move the track");
                if grab.drag_started() {
                    ui.data_mut(|d| d.insert_temp(drag_id, l));
                }
                if let Some(src) = ui.data(|d| d.get_temp::<usize>(drag_id)) {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                    if ui.rect_contains_pointer(rect) {
                        ui.data_mut(|d| d.insert_temp(tgt_id, l));
                        let y = if src < l { card_r.bottom() } else { card_r.top() };
                        p.line_segment([pos2(card_r.left(), y), pos2(card_r.right(), y)], Stroke::new(2.5, TEAL_HI()));
                    }
                }
            }
            if let Some(pl) = hresp.dnd_hover_payload::<DragItem>() {
                let ok = matches!(&*pl, DragItem::Instrument(_)) && kind == TrackKind::Midi;
                p.rect_stroke(card_r, 2.0, Stroke::new(2.0, if ok { TEAL_HI() } else { RED }));
            }
            if let Some(pl) = hresp.dnd_release_payload::<DragItem>() {
                if let (DragItem::Instrument(t), TrackKind::Midi) = (&*pl, kind) {
                    project.arrangement.tracks[l].instrument = Some(*t);
                    st.sel_lane = l;
                }
            }
            // right side: record arm, mute, solo and the menu with everything else
            let ctl = Rect::from_min_size(pos2(card_r.right() - 112.0, card_r.center().y - 11.0), vec2(108.0, 22.0));
            let mut r1 = ui.child_ui(ctl, egui::Layout::left_to_right(egui::Align::Center), None);
            r1.spacing_mut().item_spacing.x = 3.0;
            {
                let t = &mut project.arrangement.tracks[l];
                let (ar, aresp) = r1.allocate_exact_size(vec2(22.0, 22.0), Sense::click());
                r1.painter().circle_filled(ar.center(), 6.5, if t.arm { RED } else { RED.linear_multiply(0.55) });
                if t.arm {
                    r1.painter().circle_stroke(ar.center(), 9.0, Stroke::new(1.5, RED));
                }
                if aresp.on_hover_text("Arm for recording: recorded notes go to this track").clicked() {
                    t.arm = !t.arm;
                }
                flag(&mut r1, &mut t.mute, "M", AMBER, "Mute");
                flag(&mut r1, &mut t.solo, "S", TEAL_HI(), "Solo: only soloed tracks are heard");
            }
            let (mr, mresp) = r1.allocate_exact_size(vec2(22.0, 22.0), Sense::click());
            for dy in [-5.0f32, 0.0, 5.0] {
                r1.painter().circle_filled(mr.center() + vec2(0.0, dy), 1.7, if mresp.hovered() { TEXT() } else { DIM() });
            }
            let pop = egui::Id::new(("trk_more", l));
            let mut is_open = ui.ctx().data(|d| d.get_temp::<bool>(pop)).unwrap_or(false);
            if mresp.clicked() {
                is_open = !is_open;
            }
            if is_open {
                // an own Area, not egui's single-slot popup: the Input / Output lists open their own popup and would close this one
                let mut close_now = false;
                let ar = egui::Area::new(pop.with("area")).order(egui::Order::Foreground).fixed_pos(mresp.rect.left_bottom() + vec2(-190.0, 4.0)).show(ui.ctx(), |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(230.0);
                    ui.set_max_width(230.0);
                    ui.spacing_mut().item_spacing.y = 6.0;
                    ui.label(egui::RichText::new("Name").small().color(DIM()));
                    ui.add(egui::TextEdit::singleline(&mut project.arrangement.tracks[l].name).desired_width(f32::INFINITY));
                    let names = crate::arrangement::input_names(kind);
                    let t = &mut project.arrangement.tracks[l];
                    t.input = t.input.min(names.len() as u8 - 1);
                    ui.label(egui::RichText::new("Input").small().color(DIM()));
                    egui::ComboBox::from_id_salt(("trk_in", l)).width(222.0).selected_text(names[t.input as usize]).show_ui(ui, |ui| {
                        for (i, n) in names.iter().enumerate() {
                            ui.selectable_value(&mut t.input, i as u8, *n);
                        }
                    });
                    ui.label(egui::RichText::new("Output").small().color(DIM()));
                    super::mixer_ui::output_picker_w(ui, ("trk_out", l), &mut t.output, 222.0);
                    ui.label(egui::RichText::new("Level").small().color(DIM()));
                    ui.spacing_mut().slider_width = 150.0;
                    ui.add(egui::Slider::new(&mut t.volume, 0.0..=1.5));
                    ui.checkbox(&mut t.sidechain_src, "Sidechain source").on_hover_text("Every hit of this track ducks the mixer inserts that have a Duck amount");
                    ui.separator();
                    if kind == TrackKind::Audio && ui.add_sized([222.0, 24.0], egui::Button::new("Convert to MIDI track")).on_hover_text("Its sounds become notes of a MIDI clip on a new track with the same sound").clicked() {
                        midi_req = Some(l);
                        close_now = true;
                    }
                    if ui.add_sized([222.0, 24.0], egui::Button::new("Render this track to audio")).clicked() {
                        render_req = Some(l);
                        close_now = true;
                    }
                    if ui.add_sized([222.0, 24.0], egui::Button::new(egui::RichText::new("Remove track").color(RED))).clicked() {
                        remove = Some(l);
                        close_now = true;
                    }
                    });
                });
                let combo_id = pop.with("combo");
                let combo_now = ui.ctx().memory(|m| m.any_popup_open());
                let combo_before = ui.ctx().data(|d| d.get_temp::<bool>(combo_id)).unwrap_or(false);
                ui.ctx().data_mut(|d| d.insert_temp(combo_id, combo_now));
                let clicked_out = ui.ctx().input(|i| i.pointer.any_pressed() && i.pointer.interact_pos().map_or(false, |p| !ar.response.rect.contains(p)));
                if close_now || (clicked_out && !combo_now && !combo_before && !mresp.hovered()) {
                    is_open = false;
                }
            }
            ui.ctx().data_mut(|d| d.insert_temp(pop, is_open));
            // right-click on the track card: the short FL-style menu
            hresp.context_menu(|ui| {
                for (n, op, en) in [("Cut", LaneOp::Cut, true), ("Copy", LaneOp::Copy, true), ("Paste", LaneOp::Paste, can_paste)] {
                    if ui.add_enabled(en, egui::Button::new(n)).clicked() { lane_req = Some((l, op)); ui.close_menu(); }
                }
                ui.separator();
                for (n, op) in [("Fill each 2 steps", LaneOp::Fill(2)), ("Fill each 4 steps", LaneOp::Fill(4)), ("Fill each 8 steps", LaneOp::Fill(8)), ("Auto fill", LaneOp::Auto), ("Rotate left", LaneOp::Rotate(-1)), ("Rotate right", LaneOp::Rotate(1))] {
                    if ui.button(n).clicked() { lane_req = Some((l, op)); ui.close_menu(); }
                }
                ui.separator();
                if ui.button("Rename...").clicked() {
                    is_open = true;
                    ui.ctx().data_mut(|d| d.insert_temp(pop, true));
                    ui.close_menu();
                }
                if ui.button("Clone track").clicked() {
                    clone_req = Some(l);
                    ui.close_menu();
                }
                if ui.button("Select all clips of this track").clicked() {
                    st.sel = project.arrangement.clips.iter().enumerate().filter(|(_, c)| c.lane == l).map(|(i, _)| i).collect();
                    st.sel_lane = l;
                    ui.close_menu();
                }
                if ui.button("Clear clips (keep the track)").clicked() {
                    project.arrangement.clips.retain(|c| c.lane != l);
                    st.sel.clear();
                    st.sel_clip = None;
                    ui.close_menu();
                }
                ui.menu_button("Insert track", |ui| {
                    if ui.button("MIDI track at the end").clicked() {
                        project.arrangement.add_track(TrackKind::Midi);
                        ui.close_menu();
                    }
                    if ui.button("Audio track at the end").clicked() {
                        project.arrangement.add_track(TrackKind::Audio);
                        ui.close_menu();
                    }
                });
                if kind == TrackKind::Audio && ui.button("Convert to MIDI track").on_hover_text("Every sound of this track becomes notes of a MIDI clip on a new MIDI track that plays the same sound (the audio track is muted)").clicked() {
                    midi_req = Some(l);
                    ui.close_menu();
                }
                ui.separator();
                let gid = project.arrangement.tracks[l].group;
                if gid == 0 {
                    if ui.button("New folder with this track").clicked() {
                        let id = project.arrangement.groups.iter().map(|g| g.id).max().unwrap_or(0) + 1;
                        project.arrangement.groups.push(crate::arrangement::TrackGroup { id, name: format!("Folder {id}"), collapsed: false });
                        project.arrangement.tracks[l].group = id;
                        ui.close_menu();
                    }
                    let list: Vec<(u16, String)> = project.arrangement.groups.iter().map(|g| (g.id, g.name.clone())).collect();
                    if !list.is_empty() {
                        ui.menu_button("Add to folder", |ui| {
                            for (id, name) in list {
                                if ui.button(name).clicked() {
                                    project.arrangement.tracks[l].group = id;
                                    ui.close_menu();
                                }
                            }
                        });
                    }
                } else {
                    if let Some(g) = project.arrangement.groups.iter_mut().find(|g| g.id == gid) {
                        ui.add(egui::TextEdit::singleline(&mut g.name).desired_width(150.0));
                        if ui.button(if g.collapsed { "Unfold folder" } else { "Fold folder" }).clicked() {
                            g.collapsed = !g.collapsed;
                            ui.close_menu();
                        }
                    }
                    if ui.button("Take out of folder").clicked() {
                        project.arrangement.tracks[l].group = 0;
                        ui.close_menu();
                    }
                }
                ui.separator();
                let t = &mut project.arrangement.tracks[l];
                ui.checkbox(&mut t.mute, "Mute");
                ui.checkbox(&mut t.solo, "Solo");
                ui.checkbox(&mut t.arm, "Arm for recording");
                ui.separator();
                if ui.button("Render this track to audio").clicked() {
                    render_req = Some(l);
                    ui.close_menu();
                }
                if ui.button(egui::RichText::new("Delete").color(RED)).clicked() {
                    remove = Some(l);
                    ui.close_menu();
                }
            });
            // accordion: the selected track opens its strip, plugin and drop zone right under its row
            if false && st.auto_edit && st.sel_lane == l {
                // the automation lane's card in the track column: what is automated
                let (r2, _) = ui.allocate_exact_size(vec2(HEADER_W, AUTO_H), Sense::hover());
                let dcard = r2.shrink2(vec2(1.0, 2.0));
                ui.painter().rect_filled(dcard, card(), RED.gamma_multiply(0.10));
                ui.painter().rect_stroke(dcard, card(), Stroke::new(1.0, RED.gamma_multiply(0.5)));
                ui.painter().text(dcard.left_top() + vec2(10.0, 8.0), Align2::LEFT_TOP, if st.auto_master { "Automation: Master level" } else { "Automation: track level" }, FontId::proportional(11.5), TEXT());
                ui.painter().text(dcard.left_top() + vec2(10.0, 26.0), Align2::LEFT_TOP, "click the lane to add a point\ndrag a point to move it\ndrag the diamond to bend\nright-click a point to remove", FontId::proportional(10.0), DIM());
            } else if !st.auto_edit && st.detail_open && st.sel_lane == l {
                let (r2, _) = ui.allocate_exact_size(vec2(HEADER_W, DETAIL_H), Sense::hover());
                let dcard = r2.shrink2(vec2(1.0, 2.0));
                ui.painter().rect_filled(dcard, card(), PANEL2());
                ui.painter().rect_stroke(dcard, card(), Stroke::new(1.0, border()));
                let mut cu = ui.child_ui(r2.shrink2(vec2(8.0, 6.0)), egui::Layout::top_down(egui::Align::LEFT), None);
                detail_column(&mut cu, project, st, dc);
            }
        }
        let (rect, _) = ui.allocate_exact_size(vec2(HEADER_W, lane_h()), Sense::hover());
        // quiet dashed "new track" slot, split in two: MIDI | Audio
        let slot = rect.shrink2(vec2(1.0, 2.0));
        dashed_rect(ui.painter(), slot, ui.clip_rect(), card(), Stroke::new(1.0, MID()));
        for (k, (name, kind)) in [("+ MIDI track", TrackKind::Midi), ("+ Audio track", TrackKind::Audio)].into_iter().enumerate() {
            let cell = Rect::from_min_size(slot.min + vec2(slot.width() * 0.5 * k as f32, 0.0), vec2(slot.width() * 0.5, slot.height()));
            let resp = ui.interact(cell, ui.id().with(("new_track", k)), Sense::click());
            if resp.hovered() {
                ui.painter().rect_filled(cell.shrink(2.0), card(), TEAL().linear_multiply(0.10));
            }
            ui.painter().text(cell.center(), Align2::CENTER_CENTER, name, FontId::proportional(12.0), if resp.hovered() { TEAL() } else { DIM() });
            if resp.clicked() {
                project.arrangement.add_track(kind);
                st.sel_lane = project.arrangement.tracks.len() - 1;
            }
        }
    });
    if let Some((l, op)) = lane_req { lane_op(project, st, l, op); }
    {
        let drag_id = egui::Id::new("track_reorder_src");
        let tgt_id = egui::Id::new("track_reorder_tgt");
        if let Some(from) = ui.data(|d| d.get_temp::<usize>(drag_id)) {
            if ui.input(|i| i.pointer.any_released() || !i.pointer.any_down()) {
                let to = ui.data(|d| d.get_temp::<usize>(tgt_id));
                ui.data_mut(|d| { d.remove::<usize>(drag_id); d.remove::<usize>(tgt_id); });
                if let Some(to) = to.filter(|&t| t != from && t < project.arrangement.tracks.len() && from < project.arrangement.tracks.len()) {
                    let t = project.arrangement.tracks.remove(from);
                    project.arrangement.tracks.insert(to, t);
                    for c in project.arrangement.clips.iter_mut() {
                        c.lane = if c.lane == from { to } else if from < to && c.lane > from && c.lane <= to { c.lane - 1 } else if from > to && c.lane >= to && c.lane < from { c.lane + 1 } else { c.lane };
                    }
                    st.sel_lane = to;
                    st.sel.clear();
                    st.sel_clip = None;
                }
            } else {
                ui.ctx().request_repaint();
            }
        }
    }
    if let Some(l) = clone_req {
        let mut t = project.arrangement.tracks[l].clone();
        t.name = format!("{} copy", t.name);
        project.arrangement.tracks.push(t);
        let nl = project.arrangement.tracks.len() - 1;
        let copies: Vec<_> = project.arrangement.clips.iter().filter(|c| c.lane == l).cloned().map(|mut c| { c.lane = nl; c }).collect();
        project.arrangement.clips.extend(copies);
        st.sel_lane = nl;
    }
    if let Some(l) = midi_req {
        if let Some(nl) = audio_lane_to_midi(project, l) {
            st.sel_lane = nl;
        }
    }
    if let Some(l) = remove {
        project.arrangement.remove_track(l);
        project.session.remove_lane(l);
    }
    if let Some(l) = render_req {
        // the render itself runs a frame later (see `render_overlay` in ui/mod.rs), after the "please wait" screen is up
        ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("render_job"), Some(l)));
        ui.ctx().request_repaint();
    }
}

fn timeline(ui: &mut Ui, project: &mut Project, st: &mut ArrState, shared: &Shared, lib: &mut Library) {
    let lanes = project.arrangement.tracks.len();
    // the grid always fills the visible width and runs well past the last clip
    let visible_bars = (ui.available_width() / st.bar_w.max(1.0)).ceil() as u32 + 2;
    let bars = (project.loop_bars + 8).max(project.arrangement.song_end_bar() + 16).max(64).max(visible_bars);
    let playing = shared.playing.load(Ordering::Relaxed);
    // a song that ran to its end stops and rewinds in the engine: bring the view back to the start as well
    let was_playing = ui.data_mut(|d| d.get_temp_mut_or_insert_with::<bool>(egui::Id::new("playlist_was_playing"), || false).clone());
    ui.data_mut(|d| d.insert_temp(egui::Id::new("playlist_was_playing"), playing));
    let ended = was_playing && !playing && shared.position.load(Ordering::Relaxed) == 0;
    let return_to_start = ended || ui.data_mut(|d| {
        let id = egui::Id::new("playlist_return_to_start");
        let requested = d.get_temp::<bool>(id).unwrap_or(false);
        d.remove::<bool>(id);
        requested
    });
    let pos = if return_to_start { 0 } else { shared.position.load(Ordering::Relaxed) };
    // the toolbar's timeline slider: publish the position, take its seek request
    ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("arr_play_pos"), pos as u32));
    if let Some(t) = ui.ctx().data_mut(|d| d.remove_temp::<u32>(egui::Id::new("arr_seek_req"))) {
        shared.send(crate::audio_engine::Cmd::Seek(t));
    }
    let bpm = project.bpm;
    let mut scroll = egui::ScrollArea::horizontal().auto_shrink([false, false]).drag_to_scroll(false).id_salt("arr_scroll");
    if return_to_start { scroll = scroll.horizontal_scroll_offset(0.0); }
    scroll.show(ui, |ui| {
        // tracks inside a folded folder take no room
        let hid: Vec<bool> = (0..lanes).map(|l| project.arrangement.hidden(l)).collect();
        let ex = if st.sel_lane < lanes && hid[st.sel_lane] { 0.0 } else { extra_h(st, lanes) };
        let sel_l = st.sel_lane;
        let mut ys: Vec<f32> = Vec::with_capacity(lanes + 1);
        let mut acc = 0.0f32;
        for l in 0..lanes {
            ys.push(acc);
            if !hid[l] {
                acc += lane_h();
            }
        }
        ys.push(acc);
        let y_of = |l: usize| ys[l.min(lanes)] + if ex > 0.0 && l > sel_l { ex } else { 0.0 };
        let lane_at = |dy: f32| -> usize {
            for l in 0..lanes {
                if !hid[l] && dy < y_of(l) + lane_h() + if l == sel_l { ex } else { 0.0 } {
                    return l;
                }
            }
            lanes
        };
        let size = vec2(bars as f32 * st.bar_w, RULER_H + ys[lanes] + lane_h() + ex);
        let (rect, resp) = ui.allocate_exact_size(size, Sense::click_and_drag());
        // Alt + wheel changes the height of the lanes
        if resp.hovered() {
            let (alt, wheel) = ui.input(|i| (i.modifiers.alt || (i.modifiers.command && i.modifiers.shift), i.raw_scroll_delta.y));
            if alt && wheel != 0.0 {
                set_lane_h(lane_h() + wheel * 0.12);
                ui.input_mut(|i| {
                    i.raw_scroll_delta = egui::Vec2::ZERO;
                    i.smooth_scroll_delta = egui::Vec2::ZERO;
                });
            }
        }
        // Ctrl + wheel (or a pinch) zooms the bars
        if resp.hovered() {
            let z = ui.input(|i| if i.modifiers.command { i.zoom_delta() } else { 1.0 });
            if z != 1.0 {
                st.bar_w = (st.bar_w * z).clamp(12.0, 640.0);
            }
        }
        let gx = rect.left();
        let gy = rect.top() + RULER_H;
        let p = ui.painter_at(rect);
        p.rect_filled(rect, 0.0, BG());
        p.rect_filled(Rect::from_min_size(rect.min, vec2(size.x, RULER_H)), 0.0, PANEL2());
        for l in 0..lanes {
            if hid[l] {
                continue;
            }
            let r = Rect::from_min_size(pos2(rect.left(), gy + y_of(l)), vec2(size.x, lane_h() - 1.0 + if l == sel_l { ex } else { 0.0 }));
            let base = if l % 2 == 0 { SIDE() } else { BG() };
            p.rect_filled(r, 0.0, if project.arrangement.tracks[l].mute { tint(base, MID(), 0.12) } else { base });
            p.line_segment([r.left_bottom(), r.right_bottom()], Stroke::new(1.0, border()));
        }
        // quiet bottom hairline under the ruler
        p.line_segment([pos2(rect.left(), gy), pos2(rect.right(), gy)], Stroke::new(1.0, border()));
        // FL-style alternating 4-bar blocks
        for g in (4..bars).step_by(8) {
            let x0 = gx + g as f32 * st.bar_w;
            let w = (4.min(bars - g)) as f32 * st.bar_w;
            p.rect_filled(Rect::from_min_size(pos2(x0, gy), vec2(w, ys[lanes] + ex)), 0.0, TEAL().gamma_multiply(0.06));
        }
        // "new track" drop row under the last track: quiet dashed slot, accent while something is being dragged
        let dnd_active = egui::DragAndDrop::has_payload_of_type::<DragItem>(ui.ctx()) || ui.ctx().input(|i| !i.raw.hovered_files.is_empty());
        {
            let r = Rect::from_min_size(pos2(rect.left(), gy + y_of(lanes)), vec2(size.x, lane_h() - 1.0));
            let slot = r.shrink2(vec2(0.0, 2.0));
            if dnd_active {
                p.rect_filled(slot, corner(), TEAL().linear_multiply(0.10));
            }
            dashed_rect(&p, slot, ui.clip_rect(), corner(), Stroke::new(1.0, if dnd_active { TEAL() } else { MID() }));
            let tx = ui.clip_rect().left().max(rect.left()) + 14.0;
            p.text(pos2(tx, r.center().y), Align2::LEFT_CENTER, "+  Click here to add a MIDI track (right-click: audio track) - or drop a pattern / sample / instrument", FontId::proportional(13.0), if dnd_active { TEAL() } else { LIGHT() });
        }
        // drag on the ruler to select the loop range (like Ableton's loop brace)
        let ruler = Rect::from_min_size(rect.min, vec2(size.x, RULER_H));
        let rr = ui.interact(ruler, ui.id().with("loop_ruler"), Sense::click_and_drag()).on_hover_text("Click: move the playhead (Play starts from there)   -   drag: set the loop   -   right-click: loop off");
        // grab the playhead marker (or hold Alt) and drag: playback jumps with the pointer, also while the track plays
        let seek_id = ui.id().with("seek_drag");
        if rr.drag_started() || rr.clicked() {
            let near = ui.input(|i| i.pointer.press_origin()).map_or(false, |o| (o.x - (gx + pos as f32 / TICKS_PER_BAR as f32 * st.bar_w)).abs() <= 10.0);
            let alt = ui.input(|i| i.modifiers.alt);
            ui.data_mut(|d| d.insert_temp(seek_id, (near && playing) || alt));
        }
        let seeking: bool = ui.data(|d| d.get_temp(seek_id)).unwrap_or(false);
        if seeking && (rr.dragged() || rr.clicked()) {
            if let Some(c) = rr.interact_pointer_pos() {
                let t = (((c.x - gx) / st.bar_w).max(0.0) * TICKS_PER_BAR as f32) as u32;
                let t = t / 96 * 96;
                if t != pos as u32 / 96 * 96 {
                    shared.send(crate::audio_engine::Cmd::Seek(t));
                }
            }
        } else if rr.clicked() {
            // a plain click on the ruler: the playhead moves there (it only plays when the song is already playing) (drag to set a loop instead, right-click clears the loop)
            if let Some(c) = rr.interact_pointer_pos() {
                let t = (((c.x - gx) / st.bar_w).max(0.0) * TICKS_PER_BAR as f32) as u32 / 96 * 96;
                shared.send(crate::audio_engine::Cmd::Seek(t));
            }
        } else if rr.dragged() {
            if let (Some(o), Some(c)) = (ui.input(|i| i.pointer.press_origin()), rr.interact_pointer_pos()) {
                let (a, b) = (((o.x - gx) / st.bar_w).floor().max(0.0) as u32, ((c.x - gx) / st.bar_w).floor().max(0.0) as u32);
                project.loop_start = a.min(b);
                project.loop_bars = (a.max(b) - a.min(b) + 1).clamp(1, 256);
                project.loop_on = true;
            }
        }
        if rr.secondary_clicked() {
            project.loop_on = false;
        }
        // loop range: soft accent wash on the ruler, a faint tint down the lanes and a thin brace under the ruler
        let loop_l = gx + project.loop_start as f32 * st.bar_w;
        let loop_w = project.loop_bars as f32 * st.bar_w;
        if project.loop_on {
            p.rect_filled(Rect::from_min_size(pos2(loop_l, rect.top()), vec2(loop_w, RULER_H)), 0.0, TEAL().linear_multiply(0.20));
            p.rect_filled(Rect::from_min_size(pos2(loop_l, gy), vec2(loop_w, lanes as f32 * lane_h() + ex)), 0.0, TEAL().gamma_multiply(0.03));
            p.rect_filled(Rect::from_min_size(pos2(loop_l, gy - 3.0), vec2(loop_w, 3.0)), 1.5, TEAL());
        }
        for b in 0..=bars {
            let x = gx + b as f32 * st.bar_w;
            // every bar is visible; every 4th bar is brighter and thicker
            let stroke = if b % 4 == 0 { Stroke::new(1.6_f32, MID().linear_multiply(1.25)) } else { Stroke::new(1.0_f32, MID().linear_multiply(0.55)) };
            p.line_segment([pos2(x, gy), pos2(x, rect.bottom())], stroke);
            if b < bars {
                p.text(pos2(x + 4.0, rect.top() + 5.0), Align2::LEFT_TOP, format!("{}", b + 1), FontId::monospace(11.0), if b % 4 == 0 { TEXT() } else { DIM() });
            }
        }
        // beat lines whenever a beat is wide enough, then the snap steps (fainter) between them
        {
            let beat_px = 96.0 / TICKS_PER_BAR as f32 * st.bar_w;
            if beat_px >= 9.0 {
                for b in 0..bars {
                    for k in 1..(TICKS_PER_BAR / 96) {
                        let x = gx + (b as f32 + (k * 96) as f32 / TICKS_PER_BAR as f32) * st.bar_w;
                        p.line_segment([pos2(x, gy), pos2(x, rect.bottom())], Stroke::new(1.0_f32, MID().linear_multiply(0.5)));
                    }
                }
            }
            if st.snap < 96 && st.snap as f32 / TICKS_PER_BAR as f32 * st.bar_w >= 6.0 {
                let per = TICKS_PER_BAR / st.snap;
                for b in 0..bars {
                    for k in 1..per {
                        if (k * st.snap) % 96 == 0 {
                            continue;
                        }
                        let x = gx + (b as f32 + k as f32 / per as f32) * st.bar_w;
                        p.line_segment([pos2(x, gy), pos2(x, rect.bottom())], Stroke::new(1.0_f32, MID().linear_multiply(0.34)));
                    }
                }
            }
        }
        let loop_x = gx + (project.loop_start + project.loop_bars) as f32 * st.bar_w;
        if project.loop_on {
            p.line_segment([pos2(loop_l, rect.top()), pos2(loop_l, rect.bottom())], Stroke::new(1.0, TEAL().linear_multiply(0.6)));
            p.line_segment([pos2(loop_x, rect.top()), pos2(loop_x, rect.bottom())], Stroke::new(1.0, TEAL()));
        }
        for (ci, c) in project.arrangement.clips.iter().enumerate() {
            if hid.get(c.lane).copied().unwrap_or(false) {
                continue;
            }
            let px_t = st.bar_w / TICKS_PER_BAR as f32;
            let r = Rect::from_min_size(pos2(gx + c.start as f32 * px_t, gy + y_of(c.lane)), vec2(c.len as f32 * px_t, lane_h())).shrink2(vec2(1.0, 3.0));
            let muted = project.arrangement.muted(c.lane) || c.mute;
            let a = if muted { if is_dark() { 0.10 } else { 0.22 } } else if is_dark() { 0.22 } else { 0.55 };
            let title_col = if muted { DIM() } else { TEXT() };
            let clip_line = |col: Color32| Stroke::new(1.0, col.linear_multiply(if muted { 0.35 } else { 0.62 }));
            if c.audio {
                let col = lane_col(c.lane);
                p.rect_filled(r, small(), col.linear_multiply(a));
                clip_header(&p, r, col, muted);
                p.rect_stroke(r, small(), clip_line(col));
                if st.is_sel(ci) {
                    p.rect_stroke(r, small(), Stroke::new(2.0, TEAL()));
                }
                // fades (Ableton): a dark wedge at the clip edges, and small tags for the clip's settings
                {
                    let px_t = st.bar_w / TICKS_PER_BAR as f32;
                    let (fi, fo) = (c.fade_in as f32 * px_t, c.fade_out as f32 * px_t);
                    let body = Rect::from_min_max(pos2(r.left(), r.top() + 15.0), pos2(r.right(), r.bottom()));
                    if fi > 2.0 {
                        p.add(egui::Shape::convex_polygon(vec![body.left_top(), pos2(body.left() + fi.min(body.width()), body.top()), body.left_bottom()], Color32::from_black_alpha(110), Stroke::NONE));
                    }
                    if fo > 2.0 {
                        p.add(egui::Shape::convex_polygon(vec![body.right_top(), pos2(body.right() - fo.min(body.width()), body.top()), body.right_bottom()], Color32::from_black_alpha(110), Stroke::NONE));
                    }
                    if body.width() > 28.0 {
                        let hot = st.is_sel(ci);
                        let col = if hot { TEAL() } else { Color32::from_white_alpha(170) };
                        for hx in [body.left() + 10.0 + fi.min(body.width() / 2.0), body.right() - 10.0 - fo.min(body.width() / 2.0)] {
                            p.circle_filled(pos2(hx, body.top()), 3.5, Color32::from_black_alpha(140));
                            p.circle_stroke(pos2(hx, body.top()), 3.5, Stroke::new(1.2, col));
                        }
                    }
                    let mut tags = String::new();
                    if c.reverse {
                        tags.push_str("REV ");
                    }
                    if c.pitch != 0 {
                        tags.push_str(&format!("{:+}st ", c.pitch));
                    }
                    if c.mute {
                        tags.push_str("MUTE");
                    }
                    if !tags.is_empty() {
                        p.with_clip_rect(r).text(r.right_top() + vec2(-4.0, 1.0), Align2::RIGHT_TOP, tags.trim(), FontId::proportional(9.5), Color32::WHITE);
                    }
                }
                if let Some(s) = project.audio_pool.get(c.pattern) {
                    p.with_clip_rect(r).text(r.left_top() + vec2(5.0, 1.0), Align2::LEFT_TOP, &s.name, FontId::proportional(11.0), title_col);
                    if let Some(d) = &s.data {
                        // waveform preview: peak per 2px column over the part of the file that plays inside the clip
                        let body = Rect::from_min_max(pos2(r.left(), r.top() + 15.0), pos2(r.right(), r.bottom() - 1.0));
                        // the picture shows the sound as laid out: at the tempo the clip was stamped with
                        let cbpm = if c.warp > 0.0 { c.warp } else { bpm };
                        let secs_per_px = 60.0 / bpm * 4.0 / st.bar_w;
                        let off_secs = c.offset as f32 / PPQ as f32 * 60.0 / cbpm;
                        let mut x = body.left();
                        while x < body.right() {
                            let clip_secs = c.len as f32 / PPQ as f32 * 60.0 / bpm;
                            let (ta, tb) = if c.reverse { (off_secs + clip_secs - (x - body.left() + 2.0) * secs_per_px, off_secs + clip_secs - (x - body.left()) * secs_per_px) } else { (off_secs + (x - body.left()) * secs_per_px, off_secs + (x - body.left() + 2.0) * secs_per_px) };
                            let a0 = (ta.max(0.0) * d.rate as f32) as usize;
                            let a1 = (tb.max(0.0) * d.rate as f32) as usize;
                            if a0 >= d.mono.len() {
                                break;
                            }
                            let a1 = a1.min(d.mono.len()).max(a0 + 1);
                            let stride = ((a1 - a0) / 24).max(1);
                            let mut pk = 0.0f32;
                            let mut i = a0;
                            while i < a1 {
                                pk = pk.max(d.mono[i].abs());
                                i += stride;
                            }
                            let h = pk.min(1.0) * body.height() * 0.5;
                            p.line_segment([pos2(x, body.center().y - h), pos2(x, body.center().y + h)], Stroke::new(1.5, DARK().linear_multiply(0.85)));
                            x += 2.0;
                        }
                    }
                }
                continue;
            }
            // a clip wears the colour of its own track (the same as the track card)
            let col = lane_col(c.lane);
            p.rect_filled(r, small(), col.linear_multiply(a));
            clip_header(&p, r, col, muted);
            // only selected clips get the accent ring; the clips of the pattern being edited get a thin white one
            p.rect_stroke(r, small(), if st.is_sel(ci) { Stroke::new(2.0, TEAL()) } else if project.current_pattern == c.pattern { Stroke::new(1.0, Color32::WHITE.linear_multiply(0.55)) } else { clip_line(col) });
            if let Some(pat) = project.patterns.get(c.pattern) {
                p.with_clip_rect(r).text(r.left_top() + vec2(5.0, 1.0), Align2::LEFT_TOP, &pat.name, FontId::proportional(11.0), title_col);
                let plen = pat.len_ticks() as f32;
                let px_per_tick = st.bar_w / TICKS_PER_BAR as f32;
                let body = Rect::from_min_max(pos2(r.left(), r.top() + 16.0), pos2(r.right(), r.bottom() - 2.0));
                let (lo, hi) = pat.notes.iter().fold((127u8, 0u8), |(lo, hi), n| (lo.min(n.pitch), hi.max(n.pitch)));
                let span = (hi.saturating_sub(lo)).max(7) as f32;
                let total = c.len as f32 + c.offset as f32;
                let mut rep = -((c.offset as f32) % plen);
                while rep < total {
                    for n in &pat.notes {
                        let x0 = r.left() + (rep + n.start as f32) * px_per_tick;
                        if x0 >= r.right() {
                            continue;
                        }
                        let x1 = (x0 + (n.len as f32 * px_per_tick).max(1.5)).min(r.right());
                        if x1 <= r.left() {
                            continue;
                        }
                        let x0 = x0.max(r.left());
                        let y = body.bottom() - (n.pitch.saturating_sub(lo)) as f32 / span * (body.height() - 3.0) - 2.0;
                        p.line_segment([pos2(x0, y), pos2(x1, y)], Stroke::new(1.5, DARK().linear_multiply(0.85)));
                    }
                    rep += plen;
                }
                // drum hits as small ticks along the bottom edge
                for lane in 0..pat.drums.len() {
                    let mut rep = -((c.offset as f32) % plen);
                    while rep < total {
                        for (s, step) in pat.drums[lane].iter().take(pat.length_steps as usize).enumerate() {
                            if step.on {
                                let x = r.left() + (rep + (s as u32 * (PPQ / 4)) as f32) * px_per_tick;
                                if x < r.right() && x >= r.left() {
                                    p.line_segment([pos2(x, r.bottom() - 3.0 - lane as f32), pos2(x + 2.0, r.bottom() - 3.0 - lane as f32)], Stroke::new(1.0, DARK().linear_multiply(0.5)));
                                }
                            }
                        }
                        rep += plen;
                    }
                }
            }
        }
        // fades of MIDI clips (they fade the velocity of the notes): wedges and round handles, drawn on top of the notes
        for (ci, c) in project.arrangement.clips.iter().enumerate().filter(|(_, c)| !c.audio && !hid.get(c.lane).copied().unwrap_or(false)) {
            let pxt = st.bar_w / TICKS_PER_BAR as f32;
            let r = Rect::from_min_size(pos2(gx + c.start as f32 * pxt, gy + y_of(c.lane)), vec2(c.len as f32 * pxt, lane_h())).shrink2(vec2(1.0, 3.0));
            let (fi, fo) = (c.fade_in as f32 * pxt, c.fade_out as f32 * pxt);
            let body = Rect::from_min_max(pos2(r.left(), r.top() + 15.0), pos2(r.right(), r.bottom()));
            if fi > 2.0 {
                p.add(egui::Shape::convex_polygon(vec![body.left_top(), pos2(body.left() + fi.min(body.width()), body.top()), body.left_bottom()], Color32::from_black_alpha(90), Stroke::NONE));
            }
            if fo > 2.0 {
                p.add(egui::Shape::convex_polygon(vec![body.right_top(), pos2(body.right() - fo.min(body.width()), body.top()), body.right_bottom()], Color32::from_black_alpha(90), Stroke::NONE));
            }
            if body.width() > 28.0 {
                let col = if st.is_sel(ci) { TEAL() } else { Color32::from_white_alpha(200) };
                for hx in [body.left() + 10.0 + fi.min(body.width() / 2.0), body.right() - 10.0 - fo.min(body.width() / 2.0)] {
                    p.circle_filled(pos2(hx, body.top()), 3.5, Color32::from_black_alpha(160));
                    p.circle_stroke(pos2(hx, body.top()), 3.5, Stroke::new(1.2, col));
                }
            }
        }
        // lane names ride on the lanes (the track list itself lives in the Patterns panel on the left)
        for l in 0..lanes {
            if hid[l] {
                continue;
            }
            let t = &project.arrangement.tracks[l];
            let col = lane_col(l);
            let lx = ui.clip_rect().left().max(rect.left()) + 4.0;
            let chip = Rect::from_min_size(pos2(lx, gy + y_of(l) + 4.0), vec2(8.0 + t.name.chars().count() as f32 * 7.0 + 8.0, 16.0));
            p.rect_filled(chip, small(), BG().linear_multiply(0.85));
            p.rect_filled(Rect::from_min_size(chip.min + vec2(3.0, 3.0), vec2(3.0, 10.0)), 1.0, col);
            p.text(chip.left_center() + vec2(10.0, 0.0), Align2::LEFT_CENTER, &t.name, FontId::proportional(11.0), TEXT());
        }
        if playing && project.song_mode {
            // Launchpad: every hit lights its pad exactly when the sound happens (the notes inside a MIDI clip, the start of an audio clip)
            let px_t = st.bar_w / TICKS_PER_BAR as f32;
            let time = ui.input(|i| i.time);
            let style = super::widgets::fill_style();
            for c in project.arrangement.clips.iter() {
                if !(pos >= c.start && pos < c.end_tick() + 44) || project.arrangement.muted(c.lane) {
                    continue;
                }
                let col = lane_col(c.lane);
                let r = Rect::from_min_size(pos2(gx + c.start as f32 * px_t, gy + y_of(c.lane)), vec2(c.len as f32 * px_t, lane_h())).shrink2(vec2(1.0, 3.0));
                if style != 1 {
                    if pos < c.end_tick() {
                        super::widgets::play_fill(&p, r, col, (pos - c.start) as f32, c.len as f32, time);
                    }
                    continue;
                }
                if c.audio {
                    super::widgets::pad_light(&p, r, col, super::widgets::pad_intensity((pos - c.start) as f32, c.len as f32) * 0.8);
                    continue;
                }
                let Some(pat) = project.patterns.get(c.pattern) else { continue };
                if pos >= c.end_tick() {
                    continue;
                }
                let plen = pat.len_ticks().max(1);
                let local = (pos - c.start + c.offset) % plen;
                let (lo, hi) = pat.notes.iter().fold((127u8, 0u8), |(lo, hi), n| (lo.min(n.pitch), hi.max(n.pitch)));
                let span = (hi.saturating_sub(lo)).max(7) as f32;
                let body = Rect::from_min_max(pos2(r.left(), r.top() + 16.0), pos2(r.right(), r.bottom() - 2.0));
                let mut kmax = 0.0f32;
                for n in &pat.notes {
                    let age = (local + plen - n.start % plen) % plen;
                    if (age as f32) >= n.len as f32 + 44.0 {
                        continue;
                    }
                    let k = super::widgets::pad_intensity(age as f32, n.len as f32);
                    kmax = kmax.max(k);
                    let y = body.bottom() - (n.pitch.saturating_sub(lo)) as f32 / span * (body.height() - 3.0) - 2.0;
                    let x0 = (gx + (pos - age.min(pos - c.start)) as f32 * px_t).max(r.left());
                    let w = (n.len as f32 * px_t).clamp(5.0, (r.right() - x0).max(5.0));
                    super::widgets::pad_light(&p.with_clip_rect(r.expand(6.0)), Rect::from_min_size(pos2(x0, y - 3.0), vec2(w, 6.0)), col, k);
                }
                // drum steps (the Kick / Snare / Hat tracks of the templates hold drum hits, not notes) light their pad as well
                for (lane_i, row) in pat.drums.iter().enumerate() {
                    for (step, st_) in row.iter().take(pat.length_steps as usize).enumerate() {
                        if !st_.on {
                            continue;
                        }
                        let start = step as u32 * (PPQ / 4);
                        let age = (local + plen - start % plen) % plen;
                        if (age as f32) >= 24.0 + 44.0 {
                            continue;
                        }
                        let k = super::widgets::pad_intensity(age as f32, 24.0) * (0.55 + 0.45 * st_.vel as f32 / 127.0);
                        kmax = kmax.max(k);
                        let x0 = (gx + (pos - age.min(pos - c.start)) as f32 * px_t).max(r.left());
                        let y = (r.bottom() - 5.0 - lane_i as f32 * 3.0).max(r.top() + 16.0);
                        super::widgets::pad_light(&p.with_clip_rect(r.expand(6.0)), Rect::from_min_size(pos2(x0, y - 3.0), vec2((24.0 * px_t).clamp(5.0, 14.0), 6.0)), col, k);
                    }
                }
                if kmax > 0.0 {
                    p.rect_filled(r, small(), Color32::from_white_alpha((kmax * 38.0) as u8));
                    p.rect_stroke(r, small(), Stroke::new(1.2, lerp_color(col, Color32::WHITE, 0.6).linear_multiply(0.35 + 0.65 * kmax)));
                }
            }
        }
        {
            // playback position: a vertical line running down through every track + a marker in the ruler
            let x = gx + pos as f32 / TICKS_PER_BAR as f32 * st.bar_w;
            // a fading trail behind the playhead
            for i in 0..if playing { 14 } else { 0 } {
                let w = 3.0;
                let a = (1.0 - i as f32 / 14.0).powi(2) * 0.16;
                p.rect_filled(Rect::from_min_max(pos2(x - (i as f32 + 1.0) * w, gy), pos2(x - i as f32 * w, rect.bottom())), 0.0, CYAN().gamma_multiply(a));
            }
            p.line_segment([pos2(x, rect.top()), pos2(x, rect.bottom())], Stroke::new(4.0, alpha_c(CYAN(), 0.12)));
            p.line_segment([pos2(x, rect.top()), pos2(x, rect.bottom())], Stroke::new(1.5, CYAN()));
            p.add(egui::Shape::convex_polygon(vec![pos2(x - 7.0, rect.top()), pos2(x + 7.0, rect.top()), pos2(x, rect.top() + 11.0)], CYAN(), Stroke::NONE));
        }

        // ---- interaction ----
        let (pressed, down, sec_pressed) = ui.input(|i| (i.pointer.primary_pressed(), i.pointer.primary_down(), i.pointer.secondary_pressed()));
        let snap = st.snap.clamp(6, TICKS_PER_BAR) as i32;
        let bar_w = st.bar_w;
        let tick_at = move |x: f32| -> i32 { ((x - gx) / bar_w * TICKS_PER_BAR as f32).floor() as i32 };
        let snap_near = move |t: i32| -> i32 { (((t as f32 / snap as f32).round() as i32) * snap).max(0) };
        let snap_floor = move |t: i32| -> i32 { (t.div_euclid(snap) * snap).max(0) };
        let to_x = move |t: u32| -> f32 { gx + t as f32 / TICKS_PER_BAR as f32 * bar_w };
        let cell = |pos: egui::Pos2| -> Option<(i32, usize)> {
            if pos.y < gy || !rect.contains(pos) {
                return None;
            }
            Some((tick_at(pos.x), lane_at(pos.y - gy).min(lanes)))
        };
        let clip_at = |clips: &[Clip], lane: usize, tick: i32| clips.iter().position(|c| c.lane == lane && tick >= c.start as i32 && tick < c.end_tick() as i32);

        // ---- automation (Ableton's view): a lane of its own opens under the selected track with the curve in red.
        // click in the lane adds a point, drag moves it, the diamond in the middle of a segment bends it, right-click removes a point ----
        let mut auto_busy = false;
        if st.auto_edit && st.sel_lane < lanes {
            let ly = gy + y_of(st.sel_lane) + lane_h();
            let lr = Rect::from_min_size(pos2(rect.left(), ly - lane_h()), vec2(size.x, lane_h()));
            let (maxv, unity) = (1.5f32, 1.0f32);
            let ypos = |v: f32| lr.bottom() - 6.0 - (v / maxv).clamp(0.0, 1.0) * (lr.height() - 12.0);
            let val_at = |y: f32| (((lr.bottom() - 6.0 - y) / (lr.height() - 12.0)).clamp(0.0, 1.0) * maxv * 100.0).round() / 100.0;
            let col = RED;
            p.line_segment([pos2(lr.left(), ypos(unity)), pos2(lr.right(), ypos(unity))], Stroke::new(1.0, RED.gamma_multiply(0.35)));
            let pts_ref: &mut Vec<crate::arrangement::AutoPoint> = if st.auto_master { &mut project.master_auto } else { &mut project.arrangement.tracks[st.sel_lane].auto_vol };
            pts_ref.sort_by_key(|q| q.tick);
            let to_x = |t: u32| gx + t as f32 / TICKS_PER_BAR as f32 * bar_w;
            let seg_pt = |q0: &crate::arrangement::AutoPoint, q1: &crate::arrangement::AutoPoint, t: f32| pos2(to_x(q0.tick) + (to_x(q1.tick) - to_x(q0.tick)) * t, ypos(q0.v + (q1.v - q0.v) * crate::arrangement::bend(t, q0.curve)));
            // the curve (flat before the first point and after the last one)
            if let Some(f) = pts_ref.first() {
                let mut line = vec![pos2(lr.left(), ypos(f.v))];
                for w in pts_ref.windows(2) {
                    for k in 0..=24 {
                        line.push(seg_pt(&w[0], &w[1], k as f32 / 24.0));
                    }
                }
                if let Some(l) = pts_ref.last() {
                    line.push(pos2(to_x(l.tick), ypos(l.v)));
                    line.push(pos2(lr.right(), ypos(l.v)));
                }
                // a soft fill under the curve, as in Ableton
                for w in line.windows(2) {
                    p.rect_filled(Rect::from_min_max(pos2(w[0].x, w[0].y.min(w[1].y)), pos2(w[1].x.max(w[0].x + 0.5), lr.bottom())), 0.0, RED.gamma_multiply(0.06));
                }
                p.add(egui::Shape::line(line, Stroke::new(2.0, col)));
            }
            for w in pts_ref.windows(2) {
                let m = seg_pt(&w[0], &w[1], 0.5);
                if (to_x(w[1].tick) - to_x(w[0].tick)) > 26.0 {
                    p.add(egui::Shape::convex_polygon(vec![m + vec2(0.0, -4.5), m + vec2(4.5, 0.0), m + vec2(0.0, 4.5), m + vec2(-4.5, 0.0)], RED.gamma_multiply(0.45), Stroke::new(1.0, RED.gamma_multiply(0.8))));
                }
            }
            for q in pts_ref.iter() {
                p.circle_filled(pos2(to_x(q.tick), ypos(q.v)), 4.5, col);
                p.circle_stroke(pos2(to_x(q.tick), ypos(q.v)), 4.5, Stroke::new(1.0, Color32::WHITE));
            }
            let pos_now = resp.hover_pos();
            let snapt = st.snap.clamp(6, TICKS_PER_BAR);
            let near = |pos: egui::Pos2, pts: &Vec<crate::arrangement::AutoPoint>| pts.iter().position(|q| (pos2(to_x(q.tick), ypos(q.v)) - pos).length() < 9.0);
            let near_mid = |pos: egui::Pos2, pts: &Vec<crate::arrangement::AutoPoint>| (0..pts.len().saturating_sub(1)).find(|&k| (to_x(pts[k + 1].tick) - to_x(pts[k].tick)) > 26.0 && (seg_pt(&pts[k], &pts[k + 1], 0.5) - pos).length() < 8.0);
            let (pressed_a, down_a, sec_a) = ui.input(|i| (i.pointer.primary_pressed(), i.pointer.primary_down(), i.pointer.secondary_pressed()));
            let inside = st.drag.is_none() && pos_now.map_or(false, |q| lr.contains(q));
            if inside {
                auto_busy = true;
                let pos = pos_now.unwrap_or(lr.center());
                if sec_a {
                    if let Some(k) = near(pos, pts_ref) {
                        pts_ref.remove(k);
                    }
                } else if pressed_a {
                    if let Some(k) = near(pos, pts_ref) {
                        st.auto_drag = Some(k);
                    } else if let Some(k) = near_mid(pos, pts_ref) {
                        st.auto_bend = Some(k);
                    } else {
                        let t = (((pos.x - gx) / bar_w * TICKS_PER_BAR as f32).max(0.0) as u32 + snapt / 2) / snapt * snapt;
                        if !pts_ref.iter().any(|q| q.tick == t) {
                            pts_ref.push(crate::arrangement::AutoPoint { tick: t, v: val_at(pos.y), curve: 0.0 });
                            pts_ref.sort_by_key(|q| q.tick);
                            st.auto_drag = pts_ref.iter().position(|q| q.tick == t);
                        }
                    }
                }
            }
            if down_a {
                if let Some(pos) = ui.input(|i| i.pointer.latest_pos()) {
                    if let Some(k) = st.auto_drag {
                        auto_busy = true;
                        if k < pts_ref.len() {
                            let lo = if k == 0 { 0 } else { pts_ref[k - 1].tick + 1 };
                            let hi = pts_ref.get(k + 1).map_or(u32::MAX, |q| q.tick - 1);
                            let t = ((((pos.x - gx) / bar_w * TICKS_PER_BAR as f32).max(0.0) as u32 + snapt / 2) / snapt * snapt).clamp(lo, hi.max(lo));
                            pts_ref[k].tick = t;
                            pts_ref[k].v = val_at(pos.y);
                        }
                    } else if let Some(k) = st.auto_bend {
                        auto_busy = true;
                        if k + 1 < pts_ref.len() {
                            let (v0, v1) = (pts_ref[k].v, pts_ref[k + 1].v);
                            pts_ref[k].curve = crate::arrangement::curve_for_mid(v0, v1, val_at(pos.y));
                        }
                    }
                }
            } else {
                st.auto_drag = None;
                st.auto_bend = None;
            }
        }
        // while the automation lane is being worked on the clips under the pointer are left alone
        if auto_busy && st.drag.is_none() {
            return;
        }
        // keys: 1 select, 2 cut, Ctrl+D duplicate the selected clips, Delete removes them, Ctrl+A selects all, Esc clears
        if !ui.ctx().wants_keyboard_input() && !super::roll_ui::roll_has_focus(ui.ctx()) {
            let (k1, k2, kd, kdel, ka, kesc) = ui.input_mut(|i| {
                (
                    i.consume_key(egui::Modifiers::NONE, egui::Key::Num1),
                    i.consume_key(egui::Modifiers::NONE, egui::Key::Num2),
                    i.consume_key(egui::Modifiers::COMMAND, egui::Key::D),
                    i.consume_key(egui::Modifiers::NONE, egui::Key::Delete),
                    i.consume_key(egui::Modifiers::COMMAND, egui::Key::A),
                    i.consume_key(egui::Modifiers::NONE, egui::Key::Escape),
                )
            });
            if k1 {
                st.tool = 0;
            }
            if k2 {
                st.tool = 1;
            }
            let _ = (kd, kdel, ka); // Ctrl+D / Delete / Ctrl+A are handled once, in the app's key handler
            // Ctrl+C / Ctrl+X / Ctrl+V reach egui as copy / cut / paste events
            let (copy, cut, paste) = ui.input(|i| (i.events.iter().any(|e| matches!(e, egui::Event::Copy)), i.events.iter().any(|e| matches!(e, egui::Event::Cut)), i.events.iter().any(|e| matches!(e, egui::Event::Paste(_)))));
            if copy || cut {
                let n = copy_selection(project, st);
                st.msg = if n == 0 { "Select clips first".into() } else { format!("Copied {n} clip(s) - Ctrl+V pastes") };
                if cut && n > 0 {
                    delete_selection(project, st);
                }
            }
            if paste {
                // at the pointer when it is over the grid, else at the playhead
                let hover = resp.hover_pos().and_then(cell);
                let (at, lane) = match hover {
                    Some((t, l)) => (snap_floor(t) as u32, if l < lanes { Some(l) } else { None }),
                    None => (pos / snap.max(1) as u32 * snap.max(1) as u32, None),
                };
                let n = paste_clips(project, st, at, lane);
                if n > 0 {
                    st.msg = format!("Pasted {n} clip(s)");
                }
            }
            if kesc {
                st.sel.clear();
                st.sel_clip = None;
            }
        }
        let nclips = project.arrangement.clips.len();
        st.sel.retain(|&i| i < nclips);
        if st.sel_clip.map_or(false, |i| i >= nclips) {
            st.sel_clip = None;
        }

        // right-click on a clip: delete it
        if resp.hovered() && sec_pressed {
            if let Some((t, l)) = resp.hover_pos().and_then(cell) {
                let hit = clip_at(&project.arrangement.clips, l, t);
                // FL style: right-click on a clip deletes it (no menu)
                if let Some(i) = hit {
                    project.arrangement.clips.remove(i);
                    st.sel.clear();
                    st.sel_clip = None;
                    st.drag = None;
                }
            }
        }

        // drag & drop from the Browser: ghost while hovering, place on release
        if let (Some(pl), Some(pos)) = (resp.dnd_hover_payload::<DragItem>(), resp.hover_pos()) {
            if let Some((tick, lane)) = cell(pos) {
                // a fitting track takes the clip, anything else becomes a new track below the last one
                let sample_on_midi = matches!(&*pl, DragItem::Sample(_)) && lane < lanes && project.arrangement.kind(lane) == TrackKind::Midi;
                let target = if lane < lanes && (project.arrangement.kind(lane) == pl.kind() || sample_on_midi) { lane } else { lanes };
                let nt = match &*pl {
                    DragItem::Pattern(i) => project.patterns.get(*i).map(|p| pattern_bars(p) * TICKS_PER_BAR),
                    DragItem::Sample(path) => lib.get(path).ok().map(|d| audio_ticks(d.mono.len() as f32 / d.rate as f32, bpm)),
                    DragItem::Instrument(_) => Some(TICKS_PER_BAR),
                };
                let ok = nt.is_some();
                let mut ghost = Rect::from_min_size(pos2(to_x(snap_floor(tick) as u32), gy + y_of(target)), vec2(nt.unwrap_or(TICKS_PER_BAR) as f32 / TICKS_PER_BAR as f32 * bar_w, lane_h())).shrink2(vec2(1.0, 3.0));
                let mut ghost_r = small();
                if matches!(&*pl, DragItem::Instrument(_)) {
                    ghost = Rect::from_min_size(pos2(rect.left(), gy + y_of(target)), vec2(size.x, lane_h())).shrink2(vec2(1.0, 2.0));
                    ghost_r = corner();
                }
                p.rect_filled(ghost, ghost_r, (if ok { TEAL() } else { RED }).linear_multiply(0.25));
                p.rect_stroke(ghost, ghost_r, Stroke::new(1.0, if ok { TEAL() } else { RED }));
            }
        }
        if let Some(pl) = resp.dnd_release_payload::<DragItem>() {
            if let Some((tick, lane)) = resp.hover_pos().and_then(cell) {
                if let Some(l) = drop_item(project, lib, &pl, lane, snap_floor(tick) as u32, bpm) {
                    st.sel_lane = l;
                }
            }
            return;
        }
        // WAV files dragged in from the OS file manager
        let files = dropped_wavs(ui.ctx());
        if !files.is_empty() {
            let at = ui.ctx().input(|i| i.pointer.hover_pos().or(i.pointer.latest_pos()));
            if let Some((tick, lane)) = at.filter(|p| rect.contains(*p)).and_then(cell) {
                for (k, f) in files.into_iter().enumerate() {
                    drop_item(project, lib, &DragItem::Sample(f), if k == 0 { lane } else { usize::MAX }, snap_floor(tick) as u32, bpm);
                }
            }
        }
        let mods = ui.input(|i| i.modifiers);
        if !down {
            // the drag ended
            match st.drag.take() {
                Some(Drag::Move { copies: true, orig, .. }) => {
                    // a copy that was never carried off its original is dropped again
                    let ids: Vec<usize> = orig.iter().map(|(i, _)| *i).collect();
                    let stuck: Vec<usize> = ids.iter().copied().filter(|&j| j < project.arrangement.clips.len() && project.arrangement.clips.iter().enumerate().any(|(k, o)| !ids.contains(&k) && project.arrangement.clips[j].overlaps(o))).collect();
                    if !stuck.is_empty() {
                        for j in stuck.into_iter().rev() {
                            project.arrangement.clips.remove(j);
                        }
                        st.sel.clear();
                        st.sel_clip = None;
                    }
                }
                Some(Drag::Box { moved: false, press_tick, press_lane, .. }) => {
                    if st.tool == 0 && press_tick >= 0 && press_lane < lanes && !mods.shift {
                        match project.arrangement.kind(press_lane) {
                            TrackKind::Midi => {
                                let pi = project.lane_pattern(press_lane);
                                project.current_pattern = pi;
                                let pat = &project.patterns[pi];
                                let mut c = Clip::new(pi, press_lane, snap_floor(press_tick) as u32, pattern_bars(pat) * TICKS_PER_BAR, false);
                                if let Some(next) = project.arrangement.clips.iter().filter(|o| o.lane == press_lane && o.start >= c.start).map(|o| o.start).min() {
                                    c.len = c.len.min(next - c.start);
                                }
                                if c.len >= 24 && project.arrangement.can_place(&c, None) {
                                    project.arrangement.clips.push(c);
                                    st.sel_clip = Some(project.arrangement.clips.len() - 1);
                                }
                            }
                            TrackKind::Audio => {
                                let margin = (10.0 / bar_w.max(1.0) * TICKS_PER_BAR as f32) as i32;
                                let near = project.arrangement.clips.iter().any(|o| o.lane == press_lane && press_tick > o.start as i32 - margin && press_tick < o.end_tick() as i32 + margin);
                                if let Some(path) = lib.selected.clone().filter(|_| !near) {
                                    place_audio(project, lib, &path, press_lane, snap_floor(press_tick) as u32, bpm);
                                }
                            }
                        }
                    } else {
                        st.sel.clear();
                        st.sel_clip = None;
                    }
                }
                _ => {}
            }
        }
        let Some(pos) = resp.hover_pos() else { return };
        let Some((tick, lane)) = cell(pos) else { return };
        if lane >= lanes {
            // click on the empty row under the last track creates a track (left: MIDI, right: audio)
            if pressed {
                project.arrangement.add_track(TrackKind::Midi);
                st.sel_lane = lanes;
            } else if sec_pressed {
                project.arrangement.add_track(TrackKind::Audio);
                st.sel_lane = lanes;
            }
            return;
        }
        if pressed {
            // with the automation lane open, the lanes below the selected one shift when it changes: pick the track by its header then
            if !st.auto_edit {
                st.sel_lane = lane;
            }
            // a very quick click presses and releases inside one frame: draw another so the release is handled at once
            ui.ctx().request_repaint();
        }
        // double-click a clip: open it in the detail dock (piano roll for MIDI clips, sample view for audio)
        if resp.double_clicked() {
            let on_clip = clip_at(&project.arrangement.clips, lane, tick).map(|i| project.arrangement.clips[i]);
            if let Some(c) = on_clip {
                if !c.audio {
                    project.current_pattern = c.pattern;
                    st.open_roll = true;
                } else {
                    st.sel_sample = Some(c.pattern);
                }
                st.open_detail = true;
            }
        }
        let hit = clip_at(&project.arrangement.clips, lane, tick);
        // the pointer tells what a press would do: stretch at the edges, carry on the body, cut with the cut tool
        if st.drag.is_none() || down {
            if let Some(i) = hit {
                let c = project.arrangement.clips[i];
                let icon = if st.tool == 1 {
                    egui::CursorIcon::Crosshair
                } else if pos.x > to_x(c.end_tick()) - 7.0 || (pos.x < to_x(c.start) + 7.0 && c.len > 24) {
                    egui::CursorIcon::ResizeHorizontal
                } else if down {
                    egui::CursorIcon::Grabbing
                } else {
                    egui::CursorIcon::Grab
                };
                ui.ctx().set_cursor_icon(icon);
            }
        }
        // cut tool: the exact place of the cut is lit up on the clip under the pointer (audio and MIDI clips alike)
        if st.tool == 1 && st.drag.is_none() {
            if let Some(i) = hit {
                let c = project.arrangement.clips[i];
                let cut = snap_near(tick).max(0) as u32;
                if cut > c.start && cut < c.end_tick() {
                    let x = to_x(cut);
                    let top = gy + y_of(c.lane) + 3.0;
                    let bot = gy + y_of(c.lane) + lane_h() - 3.0;
                    p.rect_filled(Rect::from_min_max(pos2(to_x(c.start), top), pos2(x, bot)), 4.0, AMBER.gamma_multiply(0.18));
                    p.line_segment([pos2(x, top - 3.0), pos2(x, bot + 3.0)], Stroke::new(2.0, AMBER));
                    let beat = (cut % TICKS_PER_BAR) / 96 + 1;
                    p.text(pos2(x + 4.0, top - 1.0), Align2::LEFT_BOTTOM, format!("cut {}.{}", cut / TICKS_PER_BAR + 1, beat), FontId::proportional(10.5), AMBER);
                    ui.ctx().request_repaint();
                }
            }
        }
        if pressed {
            if let Some(i) = hit {
                let c = project.arrangement.clips[i];
                if st.tool == 1 {
                    // cut tool
                    split_clip(project, i, snap_near(tick) as u32);
                } else if mods.shift {
                    // (Shift+drag on a clip copies it, like in FL Studio; the selection is extended with the rubber band)
                    if !st.sel.remove(&i) {
                        st.sel.insert(i);
                    }
                    st.sel_clip = Some(i);
                } else {
                    if !st.is_sel(i) {
                        st.sel.clear();
                        st.sel.insert(i);
                    }
                    st.sel_clip = Some(i);
                    if !c.audio {
                        project.current_pattern = c.pattern;
                    } else {
                        st.sel_sample = Some(c.pattern);
                    }
                    if !st.auto_edit {
                        st.sel_lane = lane;
                    }
                    let right = to_x(c.end_tick());
                    let left = to_x(c.start);
                    let px_t = st.bar_w / TICKS_PER_BAR as f32;
                    let hy = gy + y_of(c.lane) + 3.0 + 15.0;
                    let near = |hx: f32| (pos.x - hx).abs() < 7.0 && (pos.y - hy).abs() < 5.0;
                    if near(left + 10.0 + c.fade_in as f32 * px_t) {
                        st.drag = Some(Drag::Fade { idx: i, out: false });
                    } else if near(right - 10.0 - c.fade_out as f32 * px_t) {
                        st.drag = Some(Drag::Fade { idx: i, out: true });
                    } else if pos.x > right - 7.0 {
                        st.drag = Some(Drag::Resize { idx: i });
                    } else if pos.x < left + 7.0 && c.len > 24 {
                        st.drag = Some(Drag::ResizeLeft { idx: i });
                    } else {
                        let ids = st.selection();
                        if mods.command || mods.alt || mods.shift {
                            // Shift / Ctrl / Alt + drag: copies (their own notes too) travel, the originals stay
                            let mut new_ids = vec![];
                            let mut primary = i;
                            for &j in &ids {
                                let src = project.arrangement.clips[j];
                                let n = independent_copy(project, src);
                                project.arrangement.clips.push(n);
                                new_ids.push(project.arrangement.clips.len() - 1);
                                if j == i {
                                    primary = project.arrangement.clips.len() - 1;
                                }
                            }
                            let orig: Vec<(usize, Clip)> = new_ids.iter().map(|&j| (j, project.arrangement.clips[j])).collect();
                            st.sel = new_ids.iter().copied().collect();
                            st.sel_clip = Some(primary);
                            st.drag = Some(Drag::Move { primary, grab: tick - c.start as i32, orig, copies: true });
                        } else {
                            let orig: Vec<(usize, Clip)> = ids.iter().map(|&j| (j, project.arrangement.clips[j])).collect();
                            st.drag = Some(Drag::Move { primary: i, grab: tick - c.start as i32, orig, copies: false });
                        }
                    }
                }
            } else if st.tool == 0 {
                if !mods.shift {
                    st.sel.clear();
                    st.sel_clip = None;
                }
                st.drag = Some(Drag::Box { start: pos, press_tick: tick, press_lane: lane, base: st.sel.clone(), moved: false });
            }
        }
        if down {
            if let Some(mut d) = st.drag.take() {
                match &mut d {
                    Drag::Move { primary, grab, orig, .. } => {
                        if let Some(po) = orig.iter().find(|(i, _)| i == primary).map(|(_, c)| *c) {
                            let mut dt = snap_near(tick - *grab) as i32 - po.start as i32;
                            // the earliest clip of the group may not go before the start of the song
                            let min_start = orig.iter().map(|(_, c)| c.start as i32).min().unwrap_or(0);
                            dt = dt.max(-min_start);
                            let dl = lane as i32 - po.lane as i32;
                            let moved: Vec<(usize, Clip)> = orig
                                .iter()
                                .map(|(i, oc)| {
                                    let mut c = *oc;
                                    c.start = (oc.start as i32 + dt) as u32;
                                    let tl = (oc.lane as i32 + dl).clamp(0, lanes as i32 - 1) as usize;
                                    if project.arrangement.kind(tl) == project.arrangement.kind(oc.lane) {
                                        c.lane = tl;
                                    }
                                    (*i, c)
                                })
                                .collect();
                            let ok = moved.iter().all(|(_, c)| project.arrangement.clips.iter().enumerate().all(|(k, o)| moved.iter().any(|(m, _)| *m == k) || !c.overlaps(o)));
                            if ok {
                                for (i, c) in moved {
                                    if i < project.arrangement.clips.len() {
                                        project.arrangement.clips[i] = c;
                                    }
                                }
                            }
                        }
                    }
                    Drag::Resize { idx } => {
                        if *idx < project.arrangement.clips.len() {
                            let mut c = project.arrangement.clips[*idx];
                            c.len = (snap_near(tick) - c.start as i32).max(snap.max(24)) as u32;
                            if project.arrangement.can_place(&c, Some(*idx)) {
                                project.arrangement.clips[*idx] = c;
                            }
                        }
                    }
                    Drag::Fade { idx, out } => {
                        if *idx < project.arrangement.clips.len() {
                            let c = &mut project.arrangement.clips[*idx];
                            let maxf = (c.len / 2) as i32;
                            let inset = (10.0 / (st.bar_w / TICKS_PER_BAR as f32)) as i32;
                            let t = if *out { c.end_tick() as i32 - tick - inset } else { tick - c.start as i32 - inset };
                            let v = t.clamp(0, maxf) as u32;
                            if *out { c.fade_out = v } else { c.fade_in = v }
                        }
                    }
                    Drag::ResizeLeft { idx } => {
                        if *idx < project.arrangement.clips.len() {
                            let mut c = project.arrangement.clips[*idx];
                            let end = c.end_tick() as i32;
                            let new_start = snap_near(tick).min(end - snap.max(24)).max(0);
                            // trimming the front skips content; extending it can only give back what was trimmed before
                            let delta = (new_start - c.start as i32).max(-(c.offset as i32));
                            let ns = c.start as i32 + delta;
                            c.start = ns as u32;
                            c.len = (end - ns) as u32;
                            c.offset = (c.offset as i32 + delta) as u32;
                            if c.len > 0 && project.arrangement.can_place(&c, Some(*idx)) {
                                project.arrangement.clips[*idx] = c;
                            }
                        }
                    }
                    Drag::Box { start, base, moved, .. } => {
                        if pos.distance(*start) > 7.0 {
                            *moved = true;
                        }
                        if *moved {
                            let band = Rect::from_two_pos(*start, pos);
                            p.rect_filled(band, 0.0, TEAL().linear_multiply(0.12));
                            p.rect_stroke(band, 0.0, Stroke::new(1.0, TEAL()));
                            let px_t = st.bar_w / TICKS_PER_BAR as f32;
                            let mut sel = base.clone();
                            for (i, c) in project.arrangement.clips.iter().enumerate() {
                                let r = Rect::from_min_size(pos2(gx + c.start as f32 * px_t, gy + y_of(c.lane)), vec2(c.len as f32 * px_t, lane_h()));
                                if band.intersects(r) {
                                    sel.insert(i);
                                }
                            }
                            st.sel = sel;
                            st.sel_clip = None;
                        }
                    }
                }
                st.drag = Some(d);
            }
        }
    });
}

/// Coloured clip title bar: a flat soft fill whose top corners follow the clip's rounding.
fn clip_header(p: &egui::Painter, r: Rect, col: Color32, muted: bool) {
    let hdr = Rect::from_min_size(r.min, vec2(r.width(), 14.0));
    let top = egui::Rounding { nw: small(), ne: small(), sw: 0.0, se: 0.0 };
    p.rect_filled(hdr, top, col.linear_multiply(if muted { 0.20 } else { 0.55 }));
}

/// Quiet dashed outline with rounded-looking corners (dashes stop one radius before each corner).
/// `vis` is the visible part of the surface, so very wide rows only emit the dashes that can be seen.
fn dashed_rect(p: &egui::Painter, r: Rect, vis: Rect, radius: f32, stroke: Stroke) {
    let (dash, gap) = (6.0f32, 5.0f32);
    let step = dash + gap;
    let (xa, xb) = (r.left() + radius, r.right() - radius);
    let (ya, yb) = (r.top() + radius, r.bottom() - radius);
    // top and bottom edges: start at the first dash that can be visible
    let k0 = ((vis.left() - xa) / step).floor().max(0.0);
    let mut x = xa + k0 * step;
    while x < xb && x < vis.right() {
        let x1 = (x + dash).min(xb);
        p.line_segment([pos2(x, r.top()), pos2(x1, r.top())], stroke);
        p.line_segment([pos2(x, r.bottom()), pos2(x1, r.bottom())], stroke);
        x += step;
    }
    // left and right edges
    let mut y = ya;
    while y < yb {
        let y1 = (y + dash).min(yb);
        p.line_segment([pos2(r.left(), y), pos2(r.left(), y1)], stroke);
        p.line_segment([pos2(r.right(), y), pos2(r.right(), y1)], stroke);
        y += step;
    }
}

/// Put a browser item on the timeline; creates a new track when the hovered one does not fit.
pub(super) fn drop_item(project: &mut Project, lib: &mut Library, item: &DragItem, lane: usize, start: u32, bpm: f32) -> Option<usize> {
    let name = item.label(project);
    match item {
        DragItem::Pattern(i) if *i < project.patterns.len() => {
            let lane = resolve_lane(project, lane, TrackKind::Midi, &name)?;
            let c = Clip::new(*i, lane, start, pattern_bars(&project.patterns[*i]) * TICKS_PER_BAR, false);
            if project.arrangement.can_place(&c, None) {
                project.arrangement.clips.push(c);
                project.song_mode = true;
            }
            Some(lane)
        }
        DragItem::Instrument(t) => {
            let lane = resolve_lane(project, lane, TrackKind::Midi, &name)?;
            project.arrangement.tracks[lane].instrument = Some(*t);
            project.arrangement.tracks[lane].blank = false;
            Some(lane)
        }
        DragItem::Sample(path) => {
            // a sound dropped on a MIDI track becomes that track's instrument; on anything else it is an audio clip
            if lane < project.arrangement.tracks.len() && project.arrangement.kind(lane) == TrackKind::Midi {
                lib.assign_track_sampler(project, lane, path);
                return Some(lane);
            }
            let lane = resolve_lane(project, lane, TrackKind::Audio, &name)?;
            place_audio(project, lib, path, lane, start, bpm);
            Some(lane)
        }
        _ => None,
    }
}

/// The sounds of an audio track become notes: one new MIDI track per distinct sound (it plays that sound through the sampler),
/// with MIDI clips of up to 16 bars whose notes sit where the audio clips were. The audio track is muted. Returns the first new track.
fn audio_lane_to_midi(project: &mut Project, lane: usize) -> Option<usize> {
    use crate::sequencer::{Pattern, TICKS_PER_STEP};
    let clips: Vec<Clip> = project.arrangement.clips.iter().filter(|c| c.lane == lane && c.audio).cloned().collect();
    let mut pools: Vec<usize> = clips.iter().map(|c| c.pattern).collect();
    pools.sort_unstable();
    pools.dedup();
    let mut first = None;
    for pool in pools {
        let Some(sample) = project.audio_pool.get(pool).cloned() else { continue };
        project.arrangement.add_track(TrackKind::Midi);
        let nl = project.arrangement.tracks.len() - 1;
        let name = sample.name.clone();
        {
            let t = &mut project.arrangement.tracks[nl];
            t.name = format!("{name} MIDI");
            t.sampler.sample = Some(sample);
            t.instrument = Some(crate::piano_roll::Track::Sampler);
            t.blank = false;
        }
        let chunk = 16 * TICKS_PER_BAR;
        let mut mine: Vec<&Clip> = clips.iter().filter(|c| c.pattern == pool).collect();
        mine.sort_by_key(|c| c.start);
        let mut k = 0u32;
        while let Some(&c0) = mine.iter().find(|c| c.start / chunk >= k) {
            k = c0.start / chunk;
            let base = k * chunk;
            let group: Vec<&&Clip> = mine.iter().filter(|c| c.start / chunk == k).collect();
            let end = group.iter().map(|c| c.start - base + c.len).max().unwrap_or(TICKS_PER_BAR);
            let bars = ((end + TICKS_PER_BAR - 1) / TICKS_PER_BAR).clamp(1, 16);
            let mut pat = Pattern::new(&format!("{name} {}", k + 1), bars * (TICKS_PER_BAR / TICKS_PER_STEP));
            pat.clip_only = true;
            for c in &group {
                pat.notes.push(crate::piano_roll::Note { start: c.start - base, len: c.len.max(6), pitch: (project.arrangement.tracks[nl].sampler.root as i32 + c.pitch as i32).clamp(0, 127) as u8, vel: 100, track: crate::piano_roll::Track::Sampler, slide: false, mute: false, pan: 0, chance: 100 });
            }
            project.patterns.push(pat);
            let pi = project.patterns.len() - 1;
            project.arrangement.clips.push(Clip::new(pi, nl, base, bars * TICKS_PER_BAR, false));
            k += 1;
        }
        first.get_or_insert(nl);
    }
    if first.is_some() {
        project.arrangement.tracks[lane].mute = true;
        project.song_mode = true;
    }
    first
}

fn place_audio(project: &mut Project, lib: &mut Library, path: &std::path::Path, lane: usize, start: u32, bpm: f32) {
    let data = match lib.get(path) {
        Ok(data) => data,
        Err(error) => {
            lib.message = format!("Load failed: {error}");
            return;
        }
    };
    let name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let sample = project.pool_index(&name, &path.to_string_lossy(), data);
    match place_audio_from_pool(project, sample, lane, start, bpm) {
        Ok(_) => project.arrangement.tracks[lane].audio_brush = Some(sample),
        Err(error) => lib.message = error.into(),
    }
}

pub(super) fn place_audio_from_pool(project: &mut Project, sample: usize, lane: usize, start: u32, bpm: f32) -> Result<usize, &'static str> {
    let data = project.audio_pool.get(sample).and_then(|sample| sample.data.as_ref())
        .ok_or("This sample could not be loaded.")?;
    let secs = data.mono.len() as f32 / data.rate as f32;
    let mut clip = Clip::new(sample, lane, start, audio_ticks(secs, bpm), true);
    if let Some(next) = project.arrangement.clips.iter()
        .filter(|other| other.lane == lane && other.start >= start).map(|other| other.start).min()
    {
        clip.len = clip.len.min(next - start);
    }
    if clip.len < 6 || !project.arrangement.can_place(&clip, None) {
        return Err("No room for the sound here: it would overlap another clip");
    }
    project.arrangement.clips.push(clip);
    project.song_mode = true;
    Ok(project.arrangement.clips.len() - 1)
}

fn alpha_c(c: Color32, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (a.clamp(0.0, 1.0) * 255.0) as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proj_with_clip(bars: u32) -> Project {
        let mut p = Project::default();
        p.arrangement.add_track(TrackKind::Midi);
        let mut pat = crate::sequencer::Pattern::new("c", 16);
        pat.clip_only = true;
        p.patterns.push(pat);
        let pi = p.patterns.len() - 1;
        p.arrangement.clips.push(Clip::bars(pi, 0, 0, bars, false));
        p
    }

    // ---- headless mouse tests of the real timeline ----
    use egui::{Event, PointerButton, Pos2};

    struct Rig {
        ctx: egui::Context,
        shared: std::sync::Arc<Shared>,
        lib: Library,
        st: ArrState,
        t: f64,
        mods: egui::Modifiers,
    }

    impl Rig {
        fn new() -> Rig {
            let ctx = egui::Context::default();
            Rig { ctx, shared: Shared::new(std::sync::Arc::new(Project::default())), lib: Library::new(), st: ArrState { snap: 384, ..ArrState::new() }, t: 0.0, mods: egui::Modifiers::NONE }
        }
        fn frame(&mut self, p: &mut Project, events: Vec<Event>) {
            self.t += 0.016;
            let mut raw = egui::RawInput { time: Some(self.t), screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1400.0, 800.0))), events, ..Default::default() };
            raw.modifiers = self.mods;
            let (st, lib, shared) = (&mut self.st, &mut self.lib, &self.shared);
            let _ = self.ctx.run(raw, |ctx| {
                egui::CentralPanel::default().frame(egui::Frame::none()).show(ctx, |ui| timeline(ui, p, st, shared, lib));
            });
        }
        fn pt(&self, bar: f32, lane: usize) -> Pos2 {
            pos2(bar * self.st.bar_w, RULER_H + lane as f32 * lane_h() + lane_h() * 0.5)
        }
        fn mv(&mut self, p: &mut Project, at: Pos2) {
            self.frame(p, vec![Event::PointerMoved(at)]);
        }
        fn down(&mut self, p: &mut Project, at: Pos2) {
            self.frame(p, vec![Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE }]);
        }
        fn up(&mut self, p: &mut Project, at: Pos2) {
            self.frame(p, vec![Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE }]);
            self.frame(p, vec![]);
        }
        fn click(&mut self, p: &mut Project, at: Pos2) {
            self.mv(p, at);
            self.down(p, at);
            self.up(p, at);
        }
        fn drag(&mut self, p: &mut Project, a: Pos2, b: Pos2) {
            self.mv(p, a);
            self.down(p, a);
            for k in 1..=8 {
                let f = k as f32 / 8.0;
                self.mv(p, pos2(a.x + (b.x - a.x) * f, a.y + (b.y - a.y) * f));
            }
            self.up(p, b);
        }
    }

    fn blank_project() -> Project {
        let mut p = Project::default();
        p.arrangement.tracks.clear();
        p.arrangement.add_track(TrackKind::Midi);
        p.arrangement.add_track(TrackKind::Midi);
        p
    }

    #[test]
    fn clicking_an_empty_lane_makes_a_clip_dragging_moves_it_and_the_edge_resizes_it() {
        let mut rig = Rig::new();
        let mut p = blank_project();
        rig.frame(&mut p, vec![]);
        rig.click(&mut p, rig.pt(2.2, 0));
        assert_eq!(p.arrangement.clips.len(), 1, "a click creates a clip");
        let c = p.arrangement.clips[0];
        assert_eq!((c.lane, c.start), (0, 2 * 384));
        // move it three bars to the right and one lane down
        let a = rig.pt(2.5, 0);
        let b = pos2(a.x + 3.0 * rig.st.bar_w, a.y + lane_h());
        rig.drag(&mut p, a, b);
        let c = p.arrangement.clips[0];
        assert_eq!((c.lane, c.start), (1, 5 * 384), "the clip moved");
        // stretch the right edge one bar
        let end = c.end_tick() as f32 / 384.0 * rig.st.bar_w - 3.0;
        let a = pos2(end, rig.pt(0.0, 1).y);
        let len0 = c.len;
        rig.drag(&mut p, a, pos2(a.x + rig.st.bar_w, a.y));
        assert_eq!(p.arrangement.clips[0].len, len0 + 384, "the clip got longer");
        // a second click right next to it makes another clip; clicking on a clip does not
        rig.click(&mut p, rig.pt(1.2, 1));
        assert_eq!(p.arrangement.clips.len(), 2);
        let at = rig.pt(5.5, 1);
        rig.click(&mut p, at);
        assert_eq!(p.arrangement.clips.len(), 2, "a click on a clip selects it, no new clip");
        assert!(rig.st.sel_clip.is_some());
    }

    #[test]
    fn rubber_band_selects_and_delete_dup_work_on_the_selection() {
        let mut rig = Rig::new();
        let mut p = blank_project();
        rig.frame(&mut p, vec![]);
        rig.click(&mut p, rig.pt(0.2, 0));
        rig.click(&mut p, rig.pt(3.2, 1));
        assert_eq!(p.arrangement.clips.len(), 2);
        // a band over both
        let a = pos2(rig.pt(0.0, 0).x + 150.0 * 0.0 + 600.0, rig.pt(0.0, 0).y);
        let _ = a;
        let from = pos2(rig.pt(9.0, 0).x, rig.pt(0.0, 1).y + 10.0);
        let to = pos2(2.0, RULER_H + 2.0);
        rig.drag(&mut p, from, to);
        assert_eq!(rig.st.sel.len(), 2, "both clips are in the band");
        duplicate_selection(&mut p, &mut rig.st);
        assert_eq!(p.arrangement.clips.len(), 4);
        delete_selection(&mut p, &mut rig.st);
        assert_eq!(p.arrangement.clips.len(), 2);
    }

    #[test]
    fn quick_click_shift_ctrl_drag_and_cut_tool() {
        let mut rig = Rig::new();
        let mut p = blank_project();
        rig.frame(&mut p, vec![]);
        // press and release inside one frame
        let at = rig.pt(1.3, 0);
        rig.mv(&mut p, at);
        rig.frame(&mut p, vec![
            Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE },
            Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE },
        ]);
        rig.frame(&mut p, vec![]);
        assert_eq!(p.arrangement.clips.len(), 1, "a very quick click still makes a clip");
        // Ctrl + drag copies
        let a = rig.pt(1.5, 0);
        rig.mv(&mut p, a);
        let ctrl = egui::Modifiers { command: true, ctrl: true, ..Default::default() };
        rig.mods = ctrl;
        rig.frame(&mut p, vec![Event::PointerButton { pos: a, button: PointerButton::Primary, pressed: true, modifiers: ctrl }]);
        let b = pos2(a.x + 4.0 * rig.st.bar_w, a.y);
        for k in 1..=6 {
            let f = k as f32 / 6.0;
            rig.frame(&mut p, vec![Event::PointerMoved(pos2(a.x + (b.x - a.x) * f, a.y))]);
        }
        rig.up(&mut p, b);
        rig.mods = egui::Modifiers::NONE;
        assert_eq!(p.arrangement.clips.len(), 2, "the copy stays where it was dropped, the original stays too");
        // cut tool splits a clip
        rig.st.tool = 1;
        let n0 = p.arrangement.clips.len();
        let c = p.arrangement.clips[0];
        let cut_at = pos2((c.start as f32 + c.len as f32 * 0.5) / 384.0 * rig.st.bar_w, rig.pt(0.0, 0).y);
        rig.click(&mut p, cut_at);
        assert_eq!(p.arrangement.clips.len(), n0 + 1, "the cut tool splits");
    }

    #[test]
    fn automation_lane_takes_its_own_clicks_and_other_lanes_still_make_clips() {
        let mut rig = Rig::new();
        let mut p = blank_project();
        rig.st.auto_edit = true;
        rig.st.sel_lane = 0;
        rig.frame(&mut p, vec![]);
        // the curve is drawn over the clips of lane 0 itself: a click there is a point, no clip
        let y = RULER_H + lane_h() * 0.5;
        rig.click(&mut p, pos2(5.0 * rig.st.bar_w, y));
        assert_eq!(p.arrangement.clips.len(), 0);
        assert!(!p.arrangement.tracks[0].auto_vol.is_empty(), "the click added an automation point");
        // the track below still takes clips
        let lane1 = RULER_H + lane_h() + lane_h() * 0.5;
        rig.click(&mut p, pos2(8.2 * rig.st.bar_w, lane1));
        assert_eq!(p.arrangement.clips.len(), 1);
        assert_eq!(p.arrangement.clips[0].lane, 1);
    }

    #[test]
    fn a_selected_group_moves_together_to_another_part_of_the_grid() {
        let mut rig = Rig::new();
        let mut p = blank_project();
        rig.frame(&mut p, vec![]);
        rig.click(&mut p, rig.pt(0.2, 0));
        rig.click(&mut p, rig.pt(2.2, 1));
        // band over both, then grab one of them and carry the group 6 bars right, one lane down for the first
        rig.drag(&mut p, pos2(9.0 * rig.st.bar_w, rig.pt(0.0, 1).y + 10.0), pos2(2.0, RULER_H + 2.0));
        assert_eq!(rig.st.sel.len(), 2);
        let a = rig.pt(0.5, 0);
        rig.drag(&mut p, a, pos2(a.x + 6.0 * rig.st.bar_w, a.y));
        let mut starts: Vec<(usize, u32)> = p.arrangement.clips.iter().map(|c| (c.lane, c.start)).collect();
        starts.sort();
        assert_eq!(starts, vec![(0, 6 * 384), (1, 8 * 384)], "both clips moved by the same distance");
        assert_eq!(rig.st.sel.len(), 2, "the selection stays");
    }

    #[test]
    fn copy_cut_and_paste_keep_the_layout() {
        let mut p = blank_project();
        let mut st = ArrState::new();
        p.arrangement.clips.push(Clip::bars(0, 0, 0, 1, false));
        p.arrangement.clips.push(Clip::bars(0, 1, 1, 1, false));
        st.sel = [0usize, 1].into_iter().collect();
        assert_eq!(copy_selection(&p, &mut st), 2);
        // free spot: pasted 8 bars later, same lanes, same distance
        assert_eq!(paste_clips(&mut p, &mut st, 8 * 384, None), 2);
        let mut v: Vec<(usize, u32)> = p.arrangement.clips.iter().map(|c| (c.lane, c.start)).collect();
        v.sort();
        assert_eq!(v, vec![(0, 0), (0, 8 * 384), (1, 384), (1, 9 * 384)]);
        // on top of existing clips: refused, nothing changes
        assert_eq!(paste_clips(&mut p, &mut st, 0, None), 0);
        assert_eq!(p.arrangement.clips.len(), 4);
        // a single clip follows the track under the pointer
        let mut q = blank_project();
        let mut st2 = ArrState::new();
        q.arrangement.clips.push(Clip::bars(0, 0, 0, 1, false));
        st2.sel = [0usize].into_iter().collect();
        copy_selection(&q, &mut st2);
        assert_eq!(paste_clips(&mut q, &mut st2, 4 * 384, Some(1)), 1);
        assert_eq!((q.arrangement.clips[1].lane, q.arrangement.clips[1].start), (1, 4 * 384));
        // a group that would leave the grid stays on its own tracks
        assert_eq!(paste_clips(&mut p, &mut st, 20 * 384, Some(1)), 2);
    }

    #[test]
    fn edge_cases_of_moving_and_resizing() {
        let mut rig = Rig::new();
        let mut p = blank_project();
        p.arrangement.add_track(TrackKind::Audio);
        rig.frame(&mut p, vec![]);
        rig.click(&mut p, rig.pt(2.2, 0));
        rig.click(&mut p, rig.pt(5.2, 0));
        assert_eq!(p.arrangement.clips.len(), 2);
        // dropping a clip onto another: no overlap ever
        let a = rig.pt(2.5, 0);
        rig.drag(&mut p, a, pos2(a.x + 3.0 * rig.st.bar_w, a.y));
        let (c0, c1) = (p.arrangement.clips[0], p.arrangement.clips[1]);
        assert!(!c0.overlaps(&c1), "clips never overlap");
        // left edge trims, the end stays
        let c = p.arrangement.clips[1];
        let end = c.end_tick();
        let x = c.start as f32 / 384.0 * rig.st.bar_w + 3.0;
        let a = pos2(x, rig.pt(0.0, 0).y);
        rig.drag(&mut p, a, pos2(a.x + rig.st.bar_w * 0.5, a.y));
        let t = p.arrangement.clips[1];
        assert_eq!(t.end_tick(), end, "left trim keeps the end");
        assert!(t.start > c.start && t.offset > 0);
        // a MIDI clip does not jump onto the audio lane
        let c = p.arrangement.clips[0];
        let a = pos2(c.start as f32 / 384.0 * rig.st.bar_w + 20.0, rig.pt(0.0, 0).y);
        rig.drag(&mut p, a, pos2(a.x, rig.pt(0.0, 2).y));
        assert!(p.arrangement.clips.iter().all(|c| c.lane < 2), "a MIDI clip stays on MIDI lanes");
        // dragging far left stops at the start of the song
        let c = p.arrangement.clips[0];
        let a = pos2(c.start as f32 / 384.0 * rig.st.bar_w + 20.0, rig.pt(0.0, c.lane).y);
        rig.drag(&mut p, a, pos2(-300.0, a.y));
        assert!(p.arrangement.clips.iter().all(|c| c.start as i64 >= 0));
        assert_eq!(p.arrangement.clips.iter().filter(|c| c.lane == 0).map(|c| c.start).min(), Some(0));
    }

    #[test]
    fn slicing_cuts_clips_into_equal_parts_and_on_the_grid() {
        let mut p = proj_with_clip(4);
        let n = slice_into_parts(&mut p, &[0], 4, 384);
        assert_eq!(n, 3);
        let mut starts: Vec<u32> = p.arrangement.clips.iter().map(|c| c.start).collect();
        starts.sort();
        assert_eq!(starts, vec![0, 384, 768, 1152]);
        assert_eq!(p.arrangement.clips.iter().map(|c| c.len).sum::<u32>(), 4 * 384, "no length is lost");
        // pieces keep playing the same content: offsets follow their start
        assert!(p.arrangement.clips.iter().all(|c| c.offset == c.start));
        let mut q = proj_with_clip(2);
        assert_eq!(slice_to_grid(&mut q, &[0], 96), 7);
        assert_eq!(q.arrangement.clips.len(), 8);
    }

    #[test]
    fn repeat_quantize_and_nudge_move_clips_safely() {
        let mut p = proj_with_clip(1);
        assert_eq!(repeat_clips(&mut p, &[0], 4), 1);
        assert_eq!(p.arrangement.clips[0].len, 4 * 384);
        // a blocker after the clip limits the repeat to what fits
        let mut q = proj_with_clip(1);
        q.arrangement.clips.push(Clip::bars(0, 0, 3, 1, false));
        repeat_clips(&mut q, &[0], 4);
        assert_eq!(q.arrangement.clips[0].len, 3 * 384);
        // quantize
        let mut r = proj_with_clip(1);
        r.arrangement.clips[0].start = 400;
        assert_eq!(quantize_clips(&mut r, &[0], 384), 1);
        assert_eq!(r.arrangement.clips[0].start, 384);
        // nudge: blocked moves change nothing
        assert!(nudge_clips(&mut r, &[0], 384));
        assert_eq!(r.arrangement.clips[0].start, 768);
        assert!(!nudge_clips(&mut r, &[0], -5000));
        assert_eq!(r.arrangement.clips[0].start, 768);
    }

    #[test]
    fn selecting_by_kind_and_pitching_the_selection() {
        let mut p = proj_with_clip(1);
        p.arrangement.add_track(TrackKind::Audio);
        p.audio_pool.push(crate::project::SampleRef { name: "a".into(), path: String::new(), data: None });
        p.arrangement.clips.push(Clip::new(0, 1, 0, 96, true));
        let mut st = ArrState::new();
        select_kind(&p, &mut st, Some(true));
        assert_eq!(st.selection(), vec![1]);
        select_kind(&p, &mut st, Some(false));
        assert_eq!(st.selection(), vec![0]);
        select_kind(&p, &mut st, None);
        let ids = st.selection();
        assert_eq!(ids.len(), 2);
        assert_eq!(transpose_clips(&mut p, &ids, 7), 2);
        assert!(p.arrangement.clips.iter().all(|c| c.pitch == 7));
        transpose_clips(&mut p, &ids, 30);
        assert!(p.arrangement.clips.iter().all(|c| c.pitch == 24), "limited to two octaves");
        reset_pitch(&mut p, &ids);
        assert!(p.arrangement.clips.iter().all(|c| c.pitch == 0));
    }

    #[test]
    fn group_duplicate_keeps_the_layout_and_delete_removes_the_selection() {
        let mut p = Project::default();
        p.arrangement.add_track(TrackKind::Midi);
        p.arrangement.add_track(TrackKind::Midi);
        for (lane, bar) in [(0usize, 0u32), (1, 1)] {
            let mut pat = crate::sequencer::Pattern::new("c", 16);
            pat.clip_only = true;
            p.patterns.push(pat);
            let pi = p.patterns.len() - 1;
            p.arrangement.clips.push(Clip::bars(pi, lane, bar, 1, false));
        }
        let new = duplicate_group(&mut p, &[0, 1]);
        assert_eq!(new.len(), 2);
        let (a, b) = (p.arrangement.clips[new[0]], p.arrangement.clips[new[1]]);
        // same lanes, same distance between the two clips, one group length (2 bars) later
        assert_eq!((a.lane, b.lane), (0, 1));
        assert_eq!(b.start - a.start, TICKS_PER_BAR);
        assert_eq!(a.start, 2 * TICKS_PER_BAR);
        assert!(new.iter().all(|&i| p.arrangement.clips.iter().enumerate().all(|(k, o)| k == i || !p.arrangement.clips[i].overlaps(o))));
        let mut st = ArrState::new();
        st.sel = [0usize, 2].into_iter().collect();
        delete_selection(&mut p, &mut st);
        assert_eq!(p.arrangement.clips.len(), 2);
        assert!(st.sel.is_empty());
    }

    #[test]
    fn duplicating_a_midi_clip_copies_its_notes_instead_of_sharing_them() {
        let mut p = Project::default();
        p.arrangement.add_track(TrackKind::Midi);
        let mut pat = crate::sequencer::Pattern::new("clip", 16);
        pat.clip_only = true;
        pat.notes.push(crate::piano_roll::Note { start: 0, len: 24, pitch: 60, vel: 100, track: crate::piano_roll::Track::Synth, slide: false, mute: false, pan: 0, chance: 100 });
        p.patterns.push(pat);
        let pi = p.patterns.len() - 1;
        p.arrangement.clips.push(Clip::bars(pi, 0, 0, 1, false));
        let n = duplicate_clip(&mut p, 0).expect("room for a copy");
        assert_ne!(p.arrangement.clips[n].pattern, p.arrangement.clips[0].pattern, "the copy shares the notes of the original");
        // editing the copy leaves the original alone
        let copy_pat = p.arrangement.clips[n].pattern;
        p.patterns[copy_pat].notes[0].pitch = 72;
        assert_eq!(p.patterns[pi].notes[0].pitch, 60);
    }
}
