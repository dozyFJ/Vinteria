//! Step sequencer (channel rack) UI.
use super::browser::DragItem;
use super::library::Library;
use super::theme::*;
use crate::audio_engine::device::Shared;
use crate::audio_engine::Cmd;
use super::widgets::{bevel, led, lerp_color, mini_knob, route_box, section_label, selector_bar};
use crate::arrangement::TrackKind;
use crate::piano_roll::{note_name, Note, Track};
use crate::project::Project;
use crate::sequencer::*;
use egui::{pos2, vec2, Align2, FontId, Rect, Sense, Stroke, Ui};
use std::sync::atomic::Ordering;

/// Requests from the rack to the main window.
pub enum RackAction {
    OpenRoll(Track),
    OpenInstrument(Track),
    /// a Playlist track was opened in the rack (index into the active pattern's tracks)
    OpenTrack(usize),
    /// the sound window of a drum lane
    OpenSample(usize),
}

pub struct SeqState {
    pub lane: usize,
    paint: Option<bool>,
    /// the pitch a click on a step of each track writes (like the note of a channel in FL's rack)
    pub pitch: Vec<u8>,
    /// the seed of the Random button (changes every press)
    seed: u32,
}

impl SeqState {
    pub fn new() -> Self {
        SeqState { lane: 0, paint: None, pitch: vec![60; crate::session::MAX_LANES], seed: 1 }
    }
}

/// Step-row operations on the notes of one instrument in a pattern: every step holds one 1/16 note.
pub mod row {
    use super::*;

    fn steps(pat: &Pattern) -> u32 {
        pat.length_steps
    }

    pub fn clear(pat: &mut Pattern, track: Track) {
        pat.notes.retain(|n| n.track != track);
    }

    fn put(pat: &mut Pattern, track: Track, step: u32, pitch: u8, vel: u8) {
        let start = step * TICKS_PER_STEP;
        pat.notes.retain(|n| !(n.track == track && n.start == start));
        pat.notes.push(Note { start, len: TICKS_PER_STEP, pitch, vel, track, slide: false, mute: false, pan: 0, chance: 100 });
    }

    /// a hit on every `every`-th step starting at `offset` (FL's "fill each 2 / 4 steps")
    pub fn fill_every(pat: &mut Pattern, track: Track, every: u32, offset: u32, pitch: u8) {
        clear(pat, track);
        let mut s = offset;
        while s < steps(pat) {
            put(pat, track, s, pitch, 100);
            s += every.max(1);
        }
    }

    /// Euclidean rhythm: `hits` hits spread as evenly as possible over the steps
    pub fn euclid(pat: &mut Pattern, track: Track, hits: u32, pitch: u8) {
        clear(pat, track);
        let n = steps(pat);
        let hits = hits.min(n);
        for k in 0..hits {
            put(pat, track, k * n / hits.max(1), pitch, 100);
        }
    }

    /// move every step by `d` steps, wrapping around the pattern
    pub fn shift(pat: &mut Pattern, track: Track, d: i32) {
        let n = steps(pat) as i32;
        for note in pat.notes.iter_mut().filter(|x| x.track == track) {
            let s = (note.start / TICKS_PER_STEP) as i32;
            note.start = (((s + d).rem_euclid(n)) as u32) * TICKS_PER_STEP + note.start % TICKS_PER_STEP;
        }
    }

    pub fn reverse(pat: &mut Pattern, track: Track) {
        let n = steps(pat);
        for note in pat.notes.iter_mut().filter(|x| x.track == track) {
            let s = note.start / TICKS_PER_STEP;
            note.start = (n - 1 - s) * TICKS_PER_STEP;
        }
    }

    /// random steps (about `density` of them), with random accents
    pub fn random(pat: &mut Pattern, track: Track, density: f32, pitch: u8, seed: u32) {
        clear(pat, track);
        let mut x = seed.wrapping_mul(2654435761).wrapping_add(0x9E3779B9);
        let mut next = || {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            (x % 1000) as f32 / 1000.0
        };
        for s in 0..steps(pat) {
            if next() < density {
                let vel = if next() < 0.25 { 127 } else { 80 + (next() * 20.0) as u8 };
                put(pat, track, s, pitch, vel);
            }
        }
    }
}

/// The fixed drum lanes and instrument channels of the rack are hidden: the rack lists the Playlist's tracks instead.
const SHOW_BUILTIN_CHANNELS: bool = false;

const CW: f32 = 24.0;
const CH: f32 = 28.0;
const HDR_W: f32 = 320.0;

pub fn show(ui: &mut Ui, project: &mut Project, st: &mut SeqState, shared: &Shared, lib: &mut Library) -> Option<RackAction> {
    let mut action: Option<RackAction> = None;
    let cur = project.rack_pattern();
    let len = project.patterns[cur].length_steps as usize;
    let playing = shared.playing.load(Ordering::Relaxed) && !project.song_mode;
    let play_step = (shared.position.load(Ordering::Relaxed) / TICKS_PER_STEP) as usize;
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Channel rack").color(TEXT()).strong().size(13.0)).on_hover_text("drums: click or drag to paint steps - instruments: click a step to add / remove a note, the Roll button opens the piano roll");
    });
    egui::ScrollArea::both().auto_shrink([false, false]).id_salt("seq_scroll").show(ui, |ui| {
        let mut grid_left = 0.0f32;
        if SHOW_BUILTIN_CHANNELS {
        for lane in 0..NUM_DRUMS {
            let row = ui.horizontal(|ui| {
                ui.set_height(CH);
                let col = LANE_COLORS[lane];
                let hdr = ui.allocate_ui_with_layout(vec2(HDR_W, CH), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    let selected = st.lane == lane;
                    {
                        let ch = &mut project.channels[lane];
                        let mut on = !ch.mute;
                        if led(ui, &mut on) {
                            ch.mute = !on;
                        }
                        mini_knob(ui, &mut ch.volume, 0.0, 1.5, 0.8, "Volume");
                        mini_knob(ui, &mut ch.pan, -1.0, 1.0, 0.0, "Pan");
                    }
                    route_box(ui, &mut project.channels[lane].output, selected);
                    let sname = project.drum_samples[lane].as_ref().map(|s| s.name.clone());
                    let b = ui.add_sized(
                        [132.0, CH - 8.0],
                        egui::Button::new(egui::RichText::new(DRUM_NAMES[lane]).color(TEXT()).strong()).fill(PANEL3()),
                    );
                    if selector_bar(ui, selected).clicked() {
                        st.lane = lane;
                    }
                    if b.clicked() {
                        st.lane = lane;
                        shared.send(Cmd::DrumHit { lane, vel: 110 });
                    }
                    if b.double_clicked() {
                        action = Some(RackAction::OpenSample(lane));
                    }
                    let b = b.on_hover_text(format!("{} - double-click: sound settings, right-click: load sample, solo", sname.as_deref().unwrap_or("built-in sound")));
                    b.context_menu(|ui| {
                        if ui.button("Sound settings...").clicked() {
                            action = Some(RackAction::OpenSample(lane));
                            ui.close_menu();
                        }
                        if ui.button("Load WAV sample...").clicked() {
                            lib.import_dialog(project, lane);
                            ui.close_menu();
                        }
                        if ui.button("Use built-in sound").clicked() {
                            project.drum_samples[lane] = None;
                            ui.close_menu();
                        }
                        ui.toggle_value(&mut project.channels[lane].solo, "Solo");
                        ui.separator();
                        ui.label("Mixer output");
                        super::mixer_ui::output_picker(ui, ("rack_out", lane), &mut project.channels[lane].output);
                    });
                });
                if hdr.response.rect.width() < HDR_W {
                    ui.add_space(HDR_W - hdr.response.rect.width());
                }
                // step grid
                let (rect, resp) = ui.allocate_exact_size(vec2(CW * len as f32, CH), Sense::click_and_drag());
                grid_left = rect.left();
                let pat = &mut project.patterns[cur];
                let idx_at = |x: f32| -> Option<usize> {
                    let i = ((x - rect.left()) / CW).floor();
                    (i >= 0.0 && (i as usize) < len).then(|| i as usize)
                };
                if resp.is_pointer_button_down_on() {
                    if let Some(i) = resp.interact_pointer_pos().and_then(|p| idx_at(p.x)) {
                        let secondary = ui.input(|i| i.pointer.secondary_down());
                        if secondary {
                            pat.drums[lane][i].on = false;
                        } else {
                            let target = *st.paint.get_or_insert(!pat.drums[lane][i].on);
                            if pat.drums[lane][i].on != target {
                                pat.drums[lane][i].on = target;
                                if target {
                                    shared.send(Cmd::DrumHit { lane, vel: pat.drums[lane][i].vel });
                                }
                            }
                        }
                        st.lane = lane;
                    }
                }
                let p = ui.painter();
                for s in 0..len {
                    let r = Rect::from_min_size(pos2(rect.left() + s as f32 * CW, rect.top()), vec2(CW, CH)).shrink2(vec2(1.5, 3.0));
                    let step = pat.drums[lane][s];
                    let fill = if step.on {
                        col.linear_multiply(0.45 + 0.55 * step.vel as f32 / 127.0)
                    } else if (s / 4) % 2 == 0 {
                        PANEL3()
                    } else {
                        PANEL2().linear_multiply(0.8)
                    };
                    p.rect_filled(r, 2.0, fill);
                    bevel(p, r, !step.on);
                    if step.on {
                        // lit top edge like a pad
                        p.rect_filled(Rect::from_min_size(r.min + vec2(2.0, 2.0), vec2(r.width() - 4.0, 3.0)), 1.5, egui::Color32::WHITE.linear_multiply(0.35));
                    } else {
                        p.rect_stroke(r, 2.0, Stroke::new(1.0, MID().linear_multiply(0.45)));
                    }
                    if s % 4 == 0 && !step.on {
                        p.circle_filled(r.left_top() + vec2(5.0, 5.0), 1.5, MID());
                    }
                    if playing && s == play_step % len {
                        p.rect_stroke(r, 4.0, Stroke::new(2.0, CYAN()));
                    }
                }
            });
            // drop a sound from the Browser onto a drum lane: it becomes that lane's sample
            if matches!(row.response.dnd_hover_payload::<DragItem>().as_deref(), Some(DragItem::Sample(_))) {
                ui.painter().rect_stroke(row.response.rect.shrink(1.0), corner(), Stroke::new(2.0, TEAL()));
            }
            if let Some(pl) = row.response.dnd_release_payload::<DragItem>() {
                if let DragItem::Sample(path) = &*pl {
                    lib.assign(project, lane, path);
                    st.lane = lane;
                    shared.send(Cmd::DrumHit { lane, vel: 110 });
                }
            }
        }
        ui.add_space(12.0);
        ui.separator();
        ui.horizontal(|ui| {
            section_label(ui, "INSTRUMENTS");
            ui.label(egui::RichText::new("write melodies and bass here - Roll opens the piano roll - then Split by channel sends every channel to the Playlist").color(DIM()).size(11.0));
        });
        // the rack has its own instrument channels (independent of the Playlist's tracks): the whole beat is written here
        let rows: Vec<(usize, TrackKind, Track)> = [Track::Synth, Track::Bass808, Track::Wavetable, Track::Analog, Track::Sampler].into_iter().enumerate().map(|(i, t)| (i, TrackKind::Midi, t)).collect();
        for (lane_i, kind, track) in rows {
            if kind == TrackKind::Audio {
                ui.horizontal(|ui| {
                    ui.set_height(CH);
                    let hdr = ui.allocate_ui_with_layout(vec2(HDR_W, CH), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        {
                            let t = &mut project.arrangement.tracks[lane_i];
                            let mut on = !t.mute;
                            if led(ui, &mut on) {
                                t.mute = !on;
                            }
                            let ch = &mut project.channels[crate::mixer::CH_AUDIO];
                            mini_knob(ui, &mut ch.volume, 0.0, 1.5, 0.85, "Audio volume");
                            mini_knob(ui, &mut ch.pan, -1.0, 1.0, 0.0, "Audio pan");
                        }
                        route_box(ui, &mut project.arrangement.tracks[lane_i].output, false);
                        let name = project.arrangement.tracks[lane_i].name.clone();
                        let b = ui.add_sized([84.0, CH - 6.0], egui::Button::new(egui::RichText::new(name).color(TEXT()).strong()).fill(PANEL3()));
                        b.on_hover_text("right-click: mixer output").context_menu(|ui| {
                            ui.label("Mixer output");
                            super::mixer_ui::output_picker(ui, ("rack_aud_out", lane_i), &mut project.arrangement.tracks[lane_i].output);
                        });
                        selector_bar(ui, false);
                        let n = project.arrangement.clips.iter().filter(|c| c.lane == lane_i).count();
                        ui.label(egui::RichText::new(format!("{n} clip(s)")).size(11.0).color(DIM()));
                    });
                    if hdr.response.rect.width() < HDR_W {
                        ui.add_space(HDR_W - hdr.response.rect.width());
                    }
                    let (rect, _) = ui.allocate_exact_size(vec2(CW * len as f32, CH), Sense::hover());
                    let p = ui.painter();
                    p.rect_filled(rect.shrink2(vec2(1.5, 1.5)), 4.0, LANE_COLORS[crate::mixer::CH_AUDIO].linear_multiply(0.12));
                    p.rect_stroke(rect.shrink2(vec2(1.5, 1.5)), 4.0, Stroke::new(1.0, MID()));
                    p.text(rect.left_center() + vec2(10.0, 0.0), Align2::LEFT_CENTER, "audio track - its clips are arranged in the Playlist", FontId::proportional(11.0), DIM());
                });
                continue;
            }
            let row = ui.horizontal(|ui| {
                ui.set_height(CH);
                let ch_idx = track.channel();
                let col = LANE_COLORS[ch_idx];
                let row_name = track.name().to_string();
                let hdr = ui.allocate_ui_with_layout(vec2(HDR_W, CH), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    {
                        let ch = &mut project.channels[ch_idx];
                        let mut on = !ch.mute;
                        if led(ui, &mut on) {
                            ch.mute = !on;
                        }
                        mini_knob(ui, &mut ch.volume, 0.0, 1.5, 0.8, "Volume");
                        mini_knob(ui, &mut ch.pan, -1.0, 1.0, 0.0, "Pan");
                    }
                    route_box(ui, &mut project.channels[ch_idx].output, false);
                    let count = project.patterns[cur].notes.iter().filter(|n| n.track == track).count();
                    let b = ui.add_sized([84.0, CH - 6.0], egui::Button::new(egui::RichText::new(row_name.clone()).color(TEXT()).strong()).fill(PANEL3()));
                    selector_bar(ui, false);
                    if b.clicked() {
                        action = Some(RackAction::OpenInstrument(track));
                    }
                    let b = b.on_hover_text(format!("{} - {count} notes in this pattern - click: instrument editor, right-click: solo", track.name()));
                    b.context_menu(|ui| {
                        // the sound of this instrument: its presets
                        let presets = super::instruments_ui::presets_of(track);
                        if !presets.is_empty() {
                            ui.menu_button("Sound", |ui| {
                                for name in presets {
                                    if ui.button(name).clicked() {
                                        super::instruments_ui::apply_named(project, track, name);
                                        ui.close_menu();
                                    }
                                }
                            });
                            if ui.button("Edit sound...").clicked() {
                                action = Some(RackAction::OpenInstrument(track));
                                ui.close_menu();
                            }
                            ui.separator();
                        }
                        ui.toggle_value(&mut project.channels[ch_idx].solo, "Solo");
                        ui.separator();
                        ui.label("Mixer output");
                        super::mixer_ui::output_picker(ui, ("rack_trk_out", lane_i), &mut project.channels[ch_idx].output);
                    });
                    if ui.small_button("Roll").on_hover_text("Open the piano roll for this instrument").clicked() {
                        action = Some(RackAction::OpenRoll(track));
                    }
                });
                if hdr.response.rect.width() < HDR_W {
                    ui.add_space(HDR_W - hdr.response.rect.width());
                }
                let (rect, resp) = ui.allocate_exact_size(vec2(CW * len as f32, CH), Sense::click());
                grid_left = rect.left();
                let pat = &mut project.patterns[cur];
                if let Some(pos) = resp.interact_pointer_pos() {
                    let i = ((pos.x - rect.left()) / CW).floor();
                    if i >= 0.0 && (i as usize) < len {
                        let s = i as u32;
                        let starts_here = |n: &Note| n.track == track && n.start / TICKS_PER_STEP == s;
                        if resp.secondary_clicked() {
                            pat.notes.retain(|n| !starts_here(n));
                        } else if resp.clicked() {
                            if pat.notes.iter().any(|n| starts_here(n)) {
                                pat.notes.retain(|n| !starts_here(n));
                            } else {
                                let pitch = if matches!(track, Track::Bass808 | Track::Sampler) { 36 } else { 60 };
                                pat.notes.push(Note { start: s * TICKS_PER_STEP, len: TICKS_PER_STEP, pitch, vel: 100, track, slide: false, mute: false, pan: 0, chance: 100 });
                                shared.send(Cmd::NoteOn { track, pitch, vel: 100 });
                                shared.send(Cmd::NoteOff { track, pitch });
                            }
                        }
                    }
                }
                // 0 = empty, 1 = sustained, 2 = note start
                let mut cover = vec![(0u8, 0u8); len];
                for n in pat.notes.iter().filter(|n| n.track == track) {
                    let s0 = (n.start / TICKS_PER_STEP) as usize;
                    let s1 = ((n.start + n.len.max(1) - 1) / TICKS_PER_STEP) as usize;
                    for s in s0..=s1.min(len - 1) {
                        if s < len {
                            let v = if s == s0 { 2 } else { 1 };
                            if v > cover[s].0 {
                                cover[s] = (v, n.pitch);
                            }
                        }
                    }
                }
                let p = ui.painter();
                for s in 0..len {
                    let r = Rect::from_min_size(pos2(rect.left() + s as f32 * CW, rect.top()), vec2(CW, CH)).shrink2(vec2(1.5, 3.0));
                    let (kind, pitch) = cover[s];
                    let fill = match kind {
                        2 => col,
                        1 => col.linear_multiply(0.4),
                        _ if (s / 4) % 2 == 0 => PANEL3(),
                        _ => PANEL2(),
                    };
                    p.rect_filled(r, 2.0, fill);
                    bevel(p, r, kind != 2);
                    p.rect_stroke(r, 2.0, Stroke::new(1.0, if kind == 2 { DARK() } else { MID() }));
                    if kind == 2 {
                        p.text(r.center_bottom() - vec2(0.0, 2.0), Align2::CENTER_BOTTOM, note_name(pitch), FontId::monospace(8.5), DARK());
                    }
                    if playing && s == play_step % len {
                        p.rect_stroke(r, 4.0, Stroke::new(2.0, CYAN()));
                    }
                }
            });
            // a sound dropped on the 808 Sampler row becomes its sample
            if track == Track::Sampler {
                if matches!(row.response.dnd_hover_payload::<DragItem>().as_deref(), Some(DragItem::Sample(_))) {
                    ui.painter().rect_stroke(row.response.rect.shrink(1.0), corner(), Stroke::new(2.0, TEAL()));
                }
                if let Some(pl) = row.response.dnd_release_payload::<DragItem>() {
                    if let DragItem::Sample(path) = &*pl {
                        lib.assign_sampler(project, path);
                    }
                }
            }
        }
        }
        // the MIDI and audio tracks of the Playlist are the channels of the rack
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            section_label(ui, "TRACKS");
            ui.label(egui::RichText::new("the MIDI and audio tracks of the Playlist - MIDI rows: click steps to put notes into the current pattern, click a track's name to open it in the Piano Roll").color(DIM()).size(11.0));
        });
        let ntracks = project.arrangement.tracks.len();
        if ntracks == 0 {
            ui.label(egui::RichText::new("No tracks yet - add one in the Playlist").color(DIM()));
        }
        for l in 0..ntracks {
            let (kind, tname, inst, clips_n) = {
                let t = &project.arrangement.tracks[l];
                (t.kind, t.name.clone(), t.instrument, project.arrangement.clips.iter().filter(|c| c.lane == l).count())
            };
            let col = lane_col(l);
            let row = ui.horizontal(|ui| {
                ui.set_height(CH);
                let hdr = ui.allocate_ui_with_layout(vec2(HDR_W, CH), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    {
                        let t = &mut project.arrangement.tracks[l];
                        let mut on = !t.mute;
                        if led(ui, &mut on) {
                            t.mute = !on;
                        }
                    }
                    route_box(ui, &mut project.arrangement.tracks[l].output, false);
                    let b = ui.add_sized([132.0, CH - 8.0], egui::Button::new(egui::RichText::new(tname.clone()).color(TEXT()).strong()).fill(PANEL3()));
                    selector_bar(ui, false);
                    if b.on_hover_text(if kind == TrackKind::Audio { "Open this audio track (Playlist)" } else { "Click: open this track in the Piano Roll" }).clicked() {
                        action = Some(RackAction::OpenTrack(l));
                    }
                    // (a MIDI track opens in the Piano Roll by clicking its name; there is no separate Roll button)
                    if kind == TrackKind::Audio && ui.small_button("Open").on_hover_text("Open this audio track").clicked() {
                        action = Some(RackAction::OpenTrack(l));
                    }
                    if kind == TrackKind::Midi {
                        let track = inst.unwrap_or(Track::Synth);
                        let pi = &mut st.pitch[l.min(crate::session::MAX_LANES - 1)];
                        ui.add(egui::DragValue::new(pi).range(24..=108).custom_formatter(|v, _| note_name(v as u8)).speed(0.2)).on_hover_text("The note a click on a step of this row writes (drag, or scroll)");
                        let pitch = *pi;
                        let seed = st.seed;
                        let mut bump = false;
                        ui.menu_button("...", |ui| {
                            let pat = &mut project.patterns[cur];
                            ui.label(egui::RichText::new("row tools").size(10.5).color(DIM()));
                            if ui.button("Clear").clicked() {
                                row::clear(pat, track);
                                ui.close_menu();
                            }
                            for (n, name) in [(2u32, "Fill every 2 steps"), (4, "Fill every 4 steps (four on the floor)"), (8, "Fill every 8 steps")] {
                                if ui.button(name).clicked() {
                                    row::fill_every(pat, track, n, 0, pitch);
                                    ui.close_menu();
                                }
                            }
                            if ui.button("Off-beat (steps 2, 6, 10 ...)").clicked() {
                                row::fill_every(pat, track, 4, 2, pitch);
                                ui.close_menu();
                            }
                            ui.menu_button("Euclid", |ui| {
                                for k in [3u32, 5, 7, 9, 11, 13] {
                                    if ui.button(format!("{k} hits")).clicked() {
                                        row::euclid(pat, track, k, pitch);
                                        ui.close_menu();
                                    }
                                }
                            });
                            if ui.button("Random").on_hover_text("Random steps with accents - press again for another one").clicked() {
                                row::random(pat, track, 0.3, pitch, seed);
                                bump = true;
                                ui.close_menu();
                            }
                            ui.separator();
                            if ui.button("Shift left").clicked() {
                                row::shift(pat, track, -1);
                            }
                            if ui.button("Shift right").clicked() {
                                row::shift(pat, track, 1);
                            }
                            if ui.button("Reverse").clicked() {
                                row::reverse(pat, track);
                                ui.close_menu();
                            }
                        });
                        if bump {
                            st.seed = st.seed.wrapping_add(1);
                        }
                    }
                });
                if hdr.response.rect.width() < HDR_W {
                    ui.add_space(HDR_W - hdr.response.rect.width());
                }
                let (rect, resp) = ui.allocate_exact_size(vec2(CW * len as f32, CH), Sense::click());
                // step mode of a MIDI track: click a step to put a piano note (C5) there, click again or right-click to remove it
                if kind == TrackKind::Midi {
                    if let Some(pos) = resp.interact_pointer_pos() {
                        let i = ((pos.x - rect.left()) / CW).floor();
                        if (resp.clicked() || resp.secondary_clicked()) && i >= 0.0 && (i as usize) < len {
                            let s = i as u32;
                            let track = inst.unwrap_or(Track::Synth);
                            // every channel writes into the one current pattern; Split by channel later gives each its own part
                            let pat = &mut project.patterns[cur];
                            let here = |n: &Note| n.track == track && n.start / TICKS_PER_STEP == s;
                            if resp.secondary_clicked() || pat.notes.iter().any(|n| here(n)) {
                                pat.notes.retain(|n| !here(n));
                            } else {
                                // Shift+click = accent, Alt+click = ghost note (soft)
                                let (accent, ghost) = ui.input(|i| (i.modifiers.shift, i.modifiers.alt));
                                let vel = if accent { 127 } else if ghost { 55 } else { 100 };
                                let pitch = st.pitch[l.min(crate::session::MAX_LANES - 1)];
                                pat.notes.push(Note { start: s * TICKS_PER_STEP, len: TICKS_PER_STEP, pitch, vel, track, slide: false, mute: false, pan: 0, chance: 100 });
                                if inst.is_some() {
                                    shared.send(Cmd::NoteOn { track, pitch, vel });
                                    shared.send(Cmd::NoteOff { track, pitch });
                                }
                            }
                        }
                    }
                }
                let p = ui.painter();
                if kind == TrackKind::Audio {
                    p.rect_filled(rect.shrink2(vec2(1.5, 1.5)), small(), col.linear_multiply(0.12));
                    p.rect_stroke(rect.shrink2(vec2(1.5, 1.5)), small(), Stroke::new(1.0, MID()));
                    p.text(rect.left_center() + vec2(10.0, 0.0), Align2::LEFT_CENTER, format!("audio track - {clips_n} clip(s), arranged in the Playlist"), FontId::proportional(11.0), DIM());
                } else {
                    // preview of the track's own notes: a lit cell wherever a note starts
                    let ttrack = inst.unwrap_or(Track::Synth);
                    let pi: Option<usize> = Some(cur);
                    let mut vels = vec![0u8; len];
                    if let Some(pat) = pi.and_then(|i| project.patterns.get(i)) {
                        for n in pat.notes.iter().filter(|n| n.track == ttrack) {
                            let s = (n.start / TICKS_PER_STEP) as usize;
                            if s < len {
                                vels[s] = n.vel.max(1);
                            }
                        }
                    }
                    for s in 0..len {
                        let r = Rect::from_min_size(pos2(rect.left() + s as f32 * CW, rect.top()), vec2(CW, CH)).shrink2(vec2(1.5, 3.0));
                        let base = if (s / 4) % 2 == 0 { PANEL3() } else { PANEL2() };
                        // a lit step: the brighter, the louder (an accent is full colour, a ghost note is dim)
                        let fill = if vels[s] > 0 { lerp_color(base, col, 0.35 + 0.65 * vels[s] as f32 / 127.0) } else { base };
                        p.rect_filled(r, small(), fill);
                        bevel(p, r, vels[s] == 0);
                        if playing && s == play_step % len.max(1) {
                            p.rect_stroke(r.expand(0.5), small(), Stroke::new(1.5, CYAN()));
                            p.rect_filled(r, small(), egui::Color32::from_white_alpha(if vels[s] > 0 { 90 } else { 28 }));
                        }
                    }
                    if inst.is_none() {
                        p.text(rect.left_center() + vec2(8.0, 0.0), Align2::LEFT_CENTER, if vels.iter().any(|x| *x > 0) { "no plugin yet" } else { "empty" }, FontId::proportional(10.5), DIM());
                    }
                }
                if kind == TrackKind::Audio && resp.clicked() {
                    action = Some(RackAction::OpenTrack(l));
                }
            });
            // plugins go onto MIDI tracks, sounds onto audio tracks
            let want = if kind == TrackKind::Audio { matches!(row.response.dnd_hover_payload::<DragItem>().as_deref(), Some(DragItem::Sample(_))) } else { matches!(row.response.dnd_hover_payload::<DragItem>().as_deref(), Some(DragItem::Instrument(_)) | Some(DragItem::Sample(_))) };
            if want {
                ui.painter().rect_stroke(row.response.rect.shrink(1.0), corner(), Stroke::new(2.0, TEAL()));
            }
            if let Some(pl) = row.response.dnd_release_payload::<DragItem>() {
                match (&*pl, kind) {
                    (DragItem::Instrument(t), TrackKind::Midi) => {
                        project.arrangement.tracks[l].instrument = Some(*t);
                    }
                    (DragItem::Sample(path), TrackKind::Midi) => {
                        lib.assign_track_sampler(project, l, path);
                    }
                    (DragItem::Sample(path), TrackKind::Audio) => {
                        if let Ok(data) = lib.get(path) {
                            let secs = data.mono.len() as f32 / data.rate as f32;
                            let name = path.file_stem().map(|x| x.to_string_lossy().to_string()).unwrap_or_default();
                            let idx = project.pool_index(&name, &path.to_string_lossy(), data);
                            let start = project.arrangement.clips.iter().filter(|c| c.lane == l).map(|c| c.end_tick()).max().unwrap_or(0);
                            let ticks = ((secs * project.bpm / 60.0 * PPQ as f32).round() as u32).max(24);
                            let c = crate::arrangement::Clip::new(idx, l, start, ticks, true);
                            if project.arrangement.can_place(&c, None) {
                                project.arrangement.clips.push(c);
                                project.song_mode = true;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        if !ui.input(|i| i.pointer.primary_down()) {
            st.paint = None;
        }
        if SHOW_BUILTIN_CHANNELS {
        ui.add_space(8.0);
        // velocity editor for selected lane
        let lane = st.lane;
        ui.horizontal(|ui| {
            section_label(ui, &format!("VELOCITY - {}", DRUM_NAMES[lane]));
            if ui.button("Clear lane").clicked() {
                for s in project.patterns[cur].drums[lane].iter_mut() {
                    s.on = false;
                }
            }
            if ui.button("Randomize").clicked() {
                let mut seed = (shared.callbacks.load(Ordering::Relaxed) as u32).wrapping_mul(2654435761).wrapping_add(7);
                for s in project.patterns[cur].drums[lane].iter_mut().take(len) {
                    seed ^= seed << 13;
                    seed ^= seed >> 17;
                    seed ^= seed << 5;
                    s.on = seed % 100 < 28;
                    s.vel = 60 + (seed >> 8) as u8 % 67;
                }
            }
            if ui.button("Shift >").on_hover_text("Rotate lane one step right").clicked() {
                project.patterns[cur].drums[lane][..len].rotate_right(1);
            }
        });
        let offset = (grid_left - ui.cursor().left()).max(0.0);
        let (rect, resp) = ui.allocate_exact_size(vec2(offset + CW * len as f32, 90.0), Sense::click_and_drag());
        let x0 = rect.left() + offset;
        let area = Rect::from_min_size(pos2(x0, rect.top()), vec2(CW * len as f32, 90.0));
        let p = ui.painter();
        p.rect_filled(area, 0.0, BG());
        let pat = &mut project.patterns[cur];
        if resp.is_pointer_button_down_on() {
            if let Some(pp) = resp.interact_pointer_pos() {
                let i = ((pp.x - x0) / CW).floor();
                if i >= 0.0 && (i as usize) < len && pat.drums[lane][i as usize].on {
                    let v = ((area.bottom() - pp.y) / area.height() * 127.0).clamp(1.0, 127.0);
                    pat.drums[lane][i as usize].vel = v as u8;
                }
            }
        }
        let p = ui.painter();
        for s in 0..len {
            let step = pat.drums[lane][s];
            let h = area.height() * step.vel as f32 / 127.0;
            let r = Rect::from_min_max(pos2(x0 + s as f32 * CW + 4.0, area.bottom() - h), pos2(x0 + (s + 1) as f32 * CW - 4.0, area.bottom()));
            if step.on {
                p.rect_filled(r, 0.0, LANE_COLORS[lane]);
                p.text(pos2(r.center().x, r.top() - 2.0), Align2::CENTER_BOTTOM, format!("{}", step.vel), FontId::monospace(9.0), DIM());
            } else {
                p.rect_filled(Rect::from_min_max(pos2(r.left(), area.bottom() - 2.0), r.right_bottom()), 0.0, PANEL3());
            }
        }
        }
        ui.add_space(10.0);
        super::smart_steps::show(ui, project, cur, len, playing.then_some(play_step));
    });
    action
}


fn short_name(t: Track) -> &'static str {
    match t {
        Track::Synth => "Synth",
        Track::Bass808 => "808",
        Track::Wavetable => "Wavetbl",
        Track::Analog => "Analog",
        Track::Drums => "Drums",
        Track::Sampler => "808 Smp",
    }
}

#[cfg(test)]
mod row_tests {
    use super::*;

    fn starts(p: &Pattern, t: Track) -> Vec<u32> {
        let mut v: Vec<u32> = p.notes.iter().filter(|n| n.track == t).map(|n| n.start / TICKS_PER_STEP).collect();
        v.sort();
        v
    }

    #[test]
    fn row_generators_fill_shift_and_spread_steps() {
        let mut p = Pattern::new("t", 16);
        row::fill_every(&mut p, Track::Synth, 4, 0, 60);
        assert_eq!(starts(&p, Track::Synth), vec![0, 4, 8, 12]);
        row::shift(&mut p, Track::Synth, 1);
        assert_eq!(starts(&p, Track::Synth), vec![1, 5, 9, 13]);
        row::shift(&mut p, Track::Synth, -2);
        assert_eq!(starts(&p, Track::Synth), vec![3, 7, 11, 15]);
        row::reverse(&mut p, Track::Synth);
        assert_eq!(starts(&p, Track::Synth), vec![0, 4, 8, 12]);
        // another instrument is never touched
        p.notes.push(Note { start: 0, len: 24, pitch: 40, vel: 100, track: Track::Bass808, slide: false, mute: false, pan: 0, chance: 100 });
        row::euclid(&mut p, Track::Synth, 5, 60);
        assert_eq!(starts(&p, Track::Synth).len(), 5);
        assert_eq!(starts(&p, Track::Bass808), vec![0]);
        row::random(&mut p, Track::Synth, 0.5, 60, 7);
        let a = starts(&p, Track::Synth);
        row::random(&mut p, Track::Synth, 0.5, 60, 8);
        assert_ne!(a, starts(&p, Track::Synth), "another seed gives another pattern");
        row::clear(&mut p, Track::Synth);
        assert!(starts(&p, Track::Synth).is_empty());
        assert_eq!(starts(&p, Track::Bass808), vec![0]);
    }
}
