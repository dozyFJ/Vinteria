//! Session View (Ableton-style clip launcher): one column per track, one row per scene.
//! Clicking a clip queues it to start on the next bar; scene buttons launch a whole row.
use super::browser::{dropped_wavs, pool_for, resolve_lane, DragItem};
use super::library::Library;
use super::theme::*;
use super::widgets::*;
use crate::audio_engine::device::Shared;
use crate::audio_engine::{Cmd, QUEUED_STOP};
use crate::arrangement::TrackKind;
use crate::project::Project;
use crate::sequencer::TICKS_PER_BAR;
use crate::session::{code, MAX_LANES};
use egui::{pos2, vec2, Align2, FontId, Rect, Sense, Shape, Stroke, Ui};
use std::sync::atomic::Ordering;

const CELL_W: f32 = 132.0;
const CELL_H: f32 = 52.0;
const GAP: f32 = 3.0;
const HEAD_H: f32 = 44.0;
const CARD_W: f32 = 230.0;

fn play_icon(p: &egui::Painter, c: egui::Pos2, r: f32, col: egui::Color32) {
    p.add(Shape::convex_polygon(vec![pos2(c.x - r * 0.6, c.y - r), pos2(c.x + r, c.y), pos2(c.x - r * 0.6, c.y + r)], col, Stroke::NONE));
}

fn stop_icon(p: &egui::Painter, c: egui::Pos2, r: f32, col: egui::Color32) {
    p.rect_filled(Rect::from_center_size(c, vec2(r * 1.6, r * 1.6)), 0.0, col);
}

pub fn show(ui: &mut Ui, project: &mut Project, shared: &Shared, lib: &mut Library, arr: &mut super::arrangement_ui::ArrState) {
    let lanes = project.arrangement.tracks.len().min(MAX_LANES);
    let project_patterns = project.patterns.len();
    let playing = shared.playing.load(Ordering::Relaxed);
    let bar_pos = shared.position.load(Ordering::Relaxed) as f32 / TICKS_PER_BAR as f32;
    let blink = ((ui.input(|i| i.time) * 4.0) as i64 % 2) == 0;

    ui.horizontal(|ui| {
        section_label(ui, "SESSION");
        ui.label(egui::RichText::new("Click a clip to launch it on the next bar. Right-click for options.").color(DIM()).size(12.0));
        ui.separator();
        if ui.button("+ Scene").clicked() {
            project.session.add_scene();
        }
        if ui.button("- Scene").clicked() {
            project.session.remove_scene();
        }
        if ui.button("Stop all").clicked() {
            shared.send(Cmd::StopAllClips);
        }
        if ui.button("+ MIDI track").clicked() && lanes < MAX_LANES {
            project.arrangement.add_track(TrackKind::Midi);
        }
        if ui.button("+ Audio track").clicked() && lanes < MAX_LANES {
            project.arrangement.add_track(TrackKind::Audio);
        }
    });
    ui.add_space(4.0);
    let files = dropped_wavs(ui.ctx());
    let pointer = ui.ctx().input(|i| i.pointer.hover_pos().or(i.pointer.latest_pos()));
    let dnd_active = egui::DragAndDrop::has_payload_of_type::<DragItem>(ui.ctx()) || ui.ctx().input(|i| !i.raw.hovered_files.is_empty());

    egui::ScrollArea::both().auto_shrink([false, false]).id_salt("session_scroll").show(ui, |ui| {
        ui.spacing_mut().item_spacing = vec2(GAP, GAP);
        // ---- scene launchers across the top (the session is turned over: a card per track going down, the scenes across) ----
        ui.horizontal(|ui| {
            ui.add_space(CARD_W + GAP);
            for scene in 0..project.session.scenes {
                let (srect, sresp) = ui.allocate_exact_size(vec2(CELL_W, 30.0), Sense::click());
                let p = ui.painter();
                let hot = sresp.hovered();
                gradient_rect(p, srect, if hot { TEAL_HI() } else { PANEL3() }, if hot { TEAL() } else { PANEL2() });
                p.rect_stroke(srect, 0.0, Stroke::new(1.0, if hot { TEAL() } else { LIGHT() }));
                play_icon(p, srect.left_center() + vec2(14.0, 0.0), 6.0, if hot { egui::Color32::WHITE } else { TEXT() });
                p.text(srect.left_center() + vec2(30.0, 0.0), Align2::LEFT_CENTER, format!("Scene {}", scene + 1), FontId::proportional(13.0), if hot { egui::Color32::WHITE } else { TEXT() });
                if sresp.on_hover_text("Launch every clip of this scene").clicked() {
                    for c in project.session.clips.iter().filter(|c| c.scene == scene) {
                        shared.send(Cmd::LaunchClip { lane: c.lane, pattern: c.pattern, audio: c.audio });
                    }
                }
            }
        });
        // ---- one row per track: a card (name, mute, stop, volume slider) and the clip slots of every scene ----
        let rows = if lanes < MAX_LANES { lanes + 1 } else { lanes };
        for lane in 0..rows {
            ui.horizontal(|ui| {
                let new_col = lane >= lanes;
                if new_col {
                    let (rect, _) = ui.allocate_exact_size(vec2(CARD_W, CELL_H), Sense::hover());
                    let p = ui.painter();
                    p.rect_filled(rect, card(), if dnd_active { TEAL().linear_multiply(0.18) } else { PANEL2().linear_multiply(0.5) });
                    p.rect_stroke(rect, card(), Stroke::new(1.0, if dnd_active { TEAL() } else { MID() }));
                    p.text(rect.center(), Align2::CENTER_CENTER, "+ NEW TRACK (drop here)", FontId::proportional(12.0), if dnd_active { TEAL() } else { MID() });
                } else {
                    session_card(ui, project, shared, arr, lane);
                }
                for scene in 0..project.session.scenes {
                    let new_col = lane >= lanes;
                    let clip = if new_col { None } else { project.session.get(lane, scene).copied() };
                    let (rect, resp) = ui.allocate_exact_size(vec2(CELL_W, CELL_H), Sense::click());
                    let run = shared.session_run[lane].load(Ordering::Relaxed);
                    let queued = shared.session_queue[lane].load(Ordering::Relaxed);
                    let p = ui.painter();
                    let col = lane_col(lane);
                    match clip {
                        Some(c) => {
                            let is_run = playing && run == c.code();
                            let is_q = queued == c.code();
                            let missing = if c.audio { c.pattern >= project.audio_pool.len() } else { c.pattern >= project.patterns.len() };
                            let name = if missing {
                                "(missing)".to_string()
                            } else if c.audio {
                                project.audio_pool[c.pattern].name.clone()
                            } else {
                                project.patterns[c.pattern].name.clone()
                            };
                            if is_run {
                                gradient_rect(p, rect, TEAL_HI(), TEAL());
                                // bar progress along the bottom edge
                                p.rect_filled(Rect::from_min_size(pos2(rect.left(), rect.bottom() - 4.0), vec2(rect.width() * bar_pos.clamp(0.0, 1.0), 4.0)), 0.0, egui::Color32::from_white_alpha(200));
                            } else {
                                p.rect_filled(rect, 0.0, col.linear_multiply(if is_dark() { 0.55 } else { 0.8 }));
                            }
                            p.rect_stroke(rect, 0.0, Stroke::new(if is_q && blink { 3.0 } else { 1.0 }, if is_q { CYAN() } else { DARK() }));
                            let fg = egui::Color32::WHITE;
                            if is_run && (queued != QUEUED_STOP || blink) {
                                stop_icon(p, rect.left_center() + vec2(14.0, 0.0), 5.0, fg);
                            } else {
                                play_icon(p, rect.left_center() + vec2(14.0, 0.0), 6.0, fg);
                            }
                            p.text(rect.left_center() + vec2(30.0, 0.0), Align2::LEFT_CENTER, name, FontId::proportional(13.0), fg);
                            if resp.double_clicked() {
                                arr.sel_lane = lane;
                                arr.open_detail = true;
                            }
                            if resp.clicked() {
                                arr.sel_lane = lane;
                                arr.pad_sel = Some((lane, scene));
                                if c.audio {
                                    arr.sel_sample = Some(c.pattern);
                                } else if c.pattern < project_patterns {
                                    project.current_pattern = c.pattern;
                                }
                                shared.send(Cmd::LaunchClip { lane, pattern: c.pattern, audio: c.audio });
                            }
                        }
                        None if new_col => {
                            p.rect_filled(rect, 0.0, if dnd_active { TEAL().linear_multiply(0.12) } else { PANEL2().linear_multiply(0.4) });
                            p.rect_stroke(rect, 0.0, Stroke::new(1.0, if dnd_active { TEAL() } else { MID().linear_multiply(0.6) }));
                        }
                        None => {
                            p.rect_filled(rect, 0.0, PANEL2());
                            p.rect_stroke(rect, 0.0, Stroke::new(1.0, MID()));
                            p.text(rect.center(), Align2::CENTER_CENTER, "+", FontId::proportional(16.0), if resp.hovered() { TEXT() } else { MID() });
                        }
                    }
                    // drag & drop from the Browser (patterns -> MIDI tracks, samples -> audio tracks)
                    if let Some(pl) = resp.dnd_hover_payload::<DragItem>() {
                        let ok = new_col || project.arrangement.kind(lane) == pl.kind();
                        ui.painter().rect_stroke(rect, 0.0, Stroke::new(2.5, if ok { TEAL() } else { RED }));
                    }
                    if let Some(pl) = resp.dnd_release_payload::<DragItem>() {
                        if new_col || project.arrangement.kind(lane) == pl.kind() {
                            let name = pl.label(project);
                            if let Some(l) = resolve_lane(project, if new_col { usize::MAX } else { lane }, pl.kind(), &name) {
                                match &*pl {
                                    DragItem::Pattern(i) if *i < project.patterns.len() => project.session.set(l, scene, *i, false),
                                    DragItem::Instrument(t) => project.arrangement.tracks[l].instrument = Some(*t),
                                    DragItem::Sample(path) => {
                                        if let Some(idx) = pool_for(project, lib, path) {
                                            project.session.set(l, scene, idx, true);
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                    // WAV files dragged in from the OS file manager (several files fill consecutive scenes)
                    if !files.is_empty() && pointer.map_or(false, |p| rect.contains(p)) {
                        if let Some(l) = resolve_lane(project, if new_col { usize::MAX } else { lane }, TrackKind::Audio, &files[0].file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()) {
                            for (k, f) in files.iter().enumerate() {
                                if scene + k < project.session.scenes {
                                    if let Some(idx) = pool_for(project, lib, f) {
                                        project.session.set(l, scene + k, idx, true);
                                    }
                                }
                            }
                        }
                    }
                    if new_col {
                        continue;
                    }
                    let is_audio = project.arrangement.kind(lane) == TrackKind::Audio;
                    let popup_id = ui.id().with(("assign", lane, scene));
                    if clip.is_none() && resp.clicked() {
                        ui.memory_mut(|m| m.toggle_popup(popup_id));
                    }
                    let mut chosen: Option<(usize, bool)> = None;
                    let mut delete = false;
                    egui::popup_below_widget(ui, popup_id, &resp, egui::PopupCloseBehavior::CloseOnClick, |ui| {
                        ui.set_min_width(170.0);
                        pick_list(ui, project, is_audio, &mut chosen);
                    });
                    resp.context_menu(|ui| {
                        ui.set_min_width(170.0);
                        pick_list(ui, project, is_audio, &mut chosen);
                        if clip.is_some() {
                            ui.separator();
                            if ui.button("Delete clip").clicked() {
                                delete = true;
                                ui.close_menu();
                            }
                        }
                    });
                    if let Some((pat, audio)) = chosen {
                        project.session.set(lane, scene, pat, audio);
                    }
                    if delete {
                        project.session.clear(lane, scene);
                    }
                }
            });
        }
    });
    let _ = code;
}

/// The card of one track in the turned-over session: colour stripe, name, kind, mute, stop and the track's volume slider inside it.
fn session_card(ui: &mut Ui, project: &mut Project, shared: &Shared, arr: &mut super::arrangement_ui::ArrState, lane: usize) {
    let (rect, resp) = ui.allocate_exact_size(vec2(CARD_W, CELL_H), Sense::click());
    let col = lane_col(lane);
    {
        let p = ui.painter();
        p.rect_filled(rect, card(), lerp_color(PANEL2(), col, 0.10));
        p.rect_stroke(rect, card(), Stroke::new(if arr.sel_lane == lane { 2.0 } else { 1.0 }, if arr.sel_lane == lane { TEAL() } else { border() }));
        p.rect_filled(Rect::from_min_size(rect.min + vec2(0.0, 4.0), vec2(4.0, rect.height() - 8.0)), 2.0, col);
        let tr = &project.arrangement.tracks[lane];
        let kind = if tr.kind == TrackKind::Audio { "AUDIO" } else { "MIDI" };
        let inst = tr.instrument.map(|t| format!("{kind}  {}", t.name())).unwrap_or_else(|| kind.to_string());
        p.text(rect.left_top() + vec2(12.0, 6.0), Align2::LEFT_TOP, &tr.name, FontId::proportional(13.0), if tr.mute { DIM() } else { TEXT() });
        p.text(rect.left_top() + vec2(12.0, 23.0), Align2::LEFT_TOP, inst, FontId::proportional(9.5), DIM());
    }
    if resp.clicked() {
        arr.sel_lane = lane;
    }
    // mute and stop buttons in the corner
    let muted = project.arrangement.tracks[lane].mute;
    let mrect = Rect::from_center_size(rect.right_top() + vec2(-40.0, 13.0), vec2(22.0, 20.0));
    let mresp = ui.interact(mrect, ui.id().with(("smute", lane)), Sense::click());
    ui.painter().rect_filled(mrect, corner(), if muted { AMBER } else if mresp.hovered() { MID() } else { PANEL3() });
    ui.painter().text(mrect.center(), Align2::CENTER_CENTER, "M", FontId::proportional(11.0), if muted { egui::Color32::WHITE } else { TEXT() });
    if mresp.on_hover_text("Mute / unmute the track").clicked() {
        project.arrangement.tracks[lane].mute = !muted;
    }
    let srect = Rect::from_center_size(rect.right_top() + vec2(-14.0, 13.0), vec2(22.0, 20.0));
    let sresp = ui.interact(srect, ui.id().with(("sstop", lane)), Sense::click());
    ui.painter().rect_filled(srect, corner(), if sresp.hovered() { MID() } else { PANEL3() });
    stop_icon(ui.painter(), srect.center(), 4.5, TEXT());
    if sresp.on_hover_text("Stop this track at the next bar").clicked() {
        shared.send(Cmd::StopLane { lane });
    }
    // the volume slider lives inside the card
    let vrect = Rect::from_min_max(rect.left_bottom() + vec2(12.0, -19.0), rect.right_bottom() + vec2(-12.0, -5.0));
    let mut vol = project.arrangement.tracks[lane].volume;
    let mut child = ui.child_ui(vrect, egui::Layout::left_to_right(egui::Align::Center), None);
    child.spacing_mut().slider_width = vrect.width() - 46.0;
    if child.add(egui::Slider::new(&mut vol, 0.0..=1.5).show_value(false)).on_hover_text("Track level").changed() {
        project.arrangement.tracks[lane].volume = vol;
    }
    child.label(egui::RichText::new(format!("{:+.1}", db(vol))).monospace().size(9.5).color(DIM()));
}

fn pick_list(ui: &mut Ui, project: &mut Project, audio: bool, chosen: &mut Option<(usize, bool)>) {
    if audio {
        if project.audio_pool.is_empty() {
            ui.label("No audio files yet - add one in the Playlist.");
        }
        for (i, s) in project.audio_pool.iter().enumerate() {
            if ui.button(&s.name).clicked() {
                *chosen = Some((i, true));
                ui.close_menu();
            }
        }
    } else {
        for i in 0..project.patterns.len() {
            if project.patterns[i].clip_only {
                continue;
            }
            let name = project.patterns[i].name.clone();
            if ui.button(name).clicked() {
                *chosen = Some((i, false));
                ui.close_menu();
            }
        }
        ui.separator();
        if ui.button("+ New pattern").clicked() {
            project.new_pattern();
            *chosen = Some((project.patterns.len() - 1, false));
            ui.close_menu();
        }
    }
}
