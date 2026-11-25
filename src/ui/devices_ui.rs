//! Horizontal device chain for the selected track.
use super::{arrangement_ui::{ArrState, DetailCtx}, browser::DragItem, instruments_ui::Inst, library::Library, theme::*};
use crate::{arrangement::TrackKind, fx::{FxSlot, ALL}, mixer::{NUM_CH, NUM_INSERTS}, piano_roll::Track, project::Project};
use egui::{pos2, vec2, Rect, RichText, Sense, Stroke, Ui};

fn free_insert(p: &Project) -> Option<usize> {
    (0..NUM_INSERTS).find(|&i| p.insert_fx.get(i).map_or(true, |c| c.iter().all(|s| s.empty))
        && !p.arrangement.tracks.iter().any(|t| t.output == Some(i))
        && !p.scenes.iter().any(|s| s.arrangement.tracks.iter().any(|t| t.output == Some(i)))
        && !p.channels.iter().take(NUM_CH).any(|c| c.output == Some(i)))
}

/// Side panel the phone instrument actions open: (lane, 0 = instruments, 1 = WAV samples, 2 = presets).
pub(super) const INSTRUMENT_PANEL: &str = "m_instrument_panel";
/// Lane whose instrument editor is open full screen on the phone.
pub(super) const INSTRUMENT_EDITOR: &str = "m_instrument_editor";

/// What each instrument is for, shown in the phone's instrument list.
pub(super) fn instrument_blurb(t: Track) -> &'static str {
    match t {
        Track::Wavetable => "Morphing digital waves: pads, leads, plucks",
        Track::Synth => "Classic polyphonic synth for chords and leads",
        Track::Bass808 => "Deep 808 bass with glide, for trap and hip-hop",
        Track::Analog => "Warm analog-style synth for bass and leads",
        Track::Sampler => "Plays one WAV across the keyboard",
        Track::Drums => "Drum kit: edit the pads in Steps",
    }
}

/// The two quick knobs under an instrument: tone (filter) and level.
fn tone_level(ui: &mut Ui, p: &mut Project, lane: usize, t: Track, knobs: bool) {
    let tr = &mut p.arrangement.tracks[lane];
    let (tone, level) = match t {
        Track::Wavetable => (Some(&mut tr.snd.wt.cutoff), Some(&mut tr.snd.wt.volume)),
        Track::Synth => (Some(&mut tr.snd.synth.lp_cutoff), Some(&mut tr.snd.synth.volume)),
        Track::Analog => (Some(&mut tr.snd.analog.cutoff), Some(&mut tr.snd.analog.volume)),
        Track::Bass808 => (Some(&mut tr.snd.bass.tone), Some(&mut tr.snd.bass.volume)),
        Track::Sampler => (None, Some(&mut tr.sampler.volume)),
        Track::Drums => (None, None),
    };
    if knobs {
        ui.horizontal_wrapped(|ui| {
            if let Some(value) = tone {
                super::widgets::knob(ui, "Tone", value, 40.0, 18000.0, true,
                    |hz| format!("{hz:.0} Hz"));
            }
            if let Some(value) = level {
                super::widgets::knob(ui, "Output", value, 0.0, 3.0, false,
                    |gain| format!("{:+.1} dB", super::widgets::db(gain)));
            }
        });
    } else {
        if let Some(v) = tone { ui.add(egui::Slider::new(v, 40.0..=18000.0).logarithmic(true).text("Tone")); }
        if let Some(v) = level { ui.add(egui::Slider::new(v, 0.0..=3.0).text("Level")); }
    }
    if t == Track::Drums { ui.label("Edit sounds in the Channel Rack."); }
}

pub(super) use super::mobile::mobile_action;

fn card_ui(ui: &mut Ui, width: f32, content: impl FnOnce(&mut Ui)) -> egui::Response {
    egui::Frame::none().fill(PANEL2()).rounding(5.0).stroke(Stroke::new(1.0_f32, border())).inner_margin(10.0).show(ui, |ui| {
        ui.vertical(|ui| {
            ui.set_width(width.min(ui.available_width()).max(0.0)); ui.set_min_height(166.0);
            ui.spacing_mut().slider_width = (width - 96.0).max(60.0);
            content(ui);
        });
    }).response
}

pub(super) fn assign(p: &mut Project, lane: usize, track: Track, dc: &mut DetailCtx) {
    p.arrangement.tracks[lane].instrument = Some(track);
    p.arrangement.tracks[lane].blank = false;
    *dc.roll_track = track; *dc.live_track = track; *dc.inst = Inst::from_track(track);
    p.adopt_track_params();
}

fn show(ui: &mut Ui, p: &mut Project, st: &mut ArrState, dc: &mut DetailCtx, plugin_window: &mut bool, lib: &mut Library, playhead: u32, part: u8) {
    let lane = st.sel_lane;
    let Some(track) = p.arrangement.tracks.get(lane) else { return };
    let mobile_instrument = part == 3;
    let mobile_effects = part == 4;
    let effects_only = part == 2 || mobile_effects;
    let instrument_width = if mobile_instrument { (ui.available_width() - 22.0).max(0.0) } else { 286.0 };
    let (name, kind, out) = (track.name.clone(), track.kind, track.output.filter(|&i| i < NUM_INSERTS));
    p.insert_fx.resize_with(NUM_INSERTS, Vec::new);
    if !mobile_instrument && !mobile_effects {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Device chain").strong());
            ui.label(RichText::new(name).color(lane_col(lane)));
            if let Some(i) = out { ui.label(RichText::new(format!("→ {}", p.channels[NUM_CH + i].name)).small().color(DIM())); }
            ui.label(RichText::new("Signal flows left to right").small().color(DIM()));
        });
        ui.add_space(6.0);
    }
    let mut add_fx = None;
    let chain_content = |ui: &mut Ui| {
            if effects_only || mobile_instrument {} else if let Some(i) = st.sel_clip.filter(|&i| p.arrangement.clips.get(i).map_or(false, |c| c.lane == lane)) {
                ui.push_id(("device_clip_options", lane, i), |ui| {
                    card_ui(ui, 270.0, |ui| {
                        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                        egui::ScrollArea::vertical().id_salt("clip_options_scroll").max_height(166.0)
                            .auto_shrink([false, false]).show(ui, |ui| {
                                super::arrangement_ui::clip_options(ui, p, st, lib, i, playhead, false);
                            });
                    });
                });
            } else {
                card_ui(ui, 232.0, |ui| {
                    ui.label(RichText::new(if kind == TrackKind::Midi { "MIDI track" } else { "Audio track" }).strong());
                    ui.separator();
                    let tr = &mut p.arrangement.tracks[lane];
                    ui.horizontal(|ui| { ui.checkbox(&mut tr.mute, "Mute"); ui.checkbox(&mut tr.solo, "Solo"); });
                    super::mixer_ui::output_picker(ui, ("device_route", lane), &mut tr.output);
                    ui.add(egui::Slider::new(&mut tr.transpose, -24..=24).text("Transpose"));
                    ui.label(RichText::new("Select a clip to edit its options").small().color(DIM()));
                });
            }
            if !effects_only && !mobile_instrument { ui.label(RichText::new("→").color(DIM())); }
            if effects_only {} else if kind == TrackKind::Midi && mobile_instrument {
                let current = p.arrangement.tracks[lane].instrument;
                let ctx = ui.ctx().clone();
                let open_panel = |page: u8| {
                    ctx.data_mut(|d| d.insert_temp(egui::Id::new(INSTRUMENT_PANEL), (lane, page)));
                };
                let clip = st.sel_clip.filter(|&i| p.arrangement.clips.get(i)
                    .is_some_and(|clip| !clip.audio && clip.lane == lane));
                let response = track_settings_section(ui, "Instrument", |ui| {
                    ui.columns(2, |cols| {
                        let source = &mut cols[0];
                        let title = current.map_or("Choose instrument", |track| Inst::from_track(track).title());
                        let button = egui::Button::new(RichText::new(title).strong()).wrap_mode(egui::TextWrapMode::Wrap);
                        if source.add_sized([source.available_width(), 32.0], button).clicked() {
                            open_panel(0);
                        }
                        source.horizontal_wrapped(|ui| {
                            if ui.button("Load WAV").clicked() { open_panel(1); }
                            if ui.add_enabled(current.is_some(), egui::Button::new("Presets")).clicked() {
                                open_panel(2);
                            }
                            if ui.add_enabled(current.is_some(), egui::Button::new("Editor")).clicked() {
                                ctx.data_mut(|d| d.insert_temp(egui::Id::new(INSTRUMENT_EDITOR), lane));
                            }
                        });
                        if let Some(track) = current {
                            tone_level(source, p, lane, track, true);
                        }

                        let playback = &mut cols[1];
                        playback.label(RichText::new("Playback").small().strong().color(DIM()));
                        let track = &mut p.arrangement.tracks[lane];
                        level_row(playback, "Track level", &mut track.volume, 1.5);
                        pitch_row(playback, "Transpose", &mut track.transpose);
                        if let Some(i) = clip {
                            pitch_row(playback, "Clip transpose", &mut p.arrangement.clips[i].pitch);
                        }
                    });
                });
                if let Some(payload) = response.dnd_release_payload::<DragItem>() {
                    match &*payload {
                        DragItem::Instrument(t) => assign(p, lane, *t, dc),
                        DragItem::Sample(path) => { lib.assign_track_sampler(p, lane, path); assign(p, lane, Track::Sampler, dc); }
                        _ => {}
                    }
                }
            } else if kind == TrackKind::Midi {
                let mut picked = None;
                let current = p.arrangement.tracks[lane].instrument;
                let response = card_ui(ui, instrument_width, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new("Instrument").small().color(DIM()));
                        if current.is_some() && ui.small_button("Open").clicked() { *plugin_window = true; }
                        ui.menu_button("Choose", |ui| {
                            for t in [Track::Wavetable, Track::Synth, Track::Bass808, Track::Analog, Track::Sampler, Track::Drums] {
                                if ui.button(t.name()).clicked() { picked = Some(t); ui.close_menu(); }
                            }
                        });
                        ui.menu_button("WAV", |ui| {
                            if let Some(path) = super::arrangement_ui::sample_picker(ui, lib) {
                                lib.assign_track_sampler(p, lane, &path);
                                picked = Some(Track::Sampler);
                                ui.close_menu();
                            }
                        });
                    });
                    ui.separator();
                    if let Some(t) = current {
                        ui.label(RichText::new(Inst::from_track(t).title()).strong());
                        ui.menu_button("Preset", |ui| {
                            for preset in super::instruments_ui::presets_of(t) {
                                if ui.button(preset).clicked() { super::instruments_ui::apply_named_lane(p, lane, t, preset); ui.close_menu(); }
                            }
                        });
                        tone_level(ui, p, lane, t, false);
                    } else {
                        let (r, _) = ui.allocate_exact_size(vec2(instrument_width, 106.0), Sense::hover());
                        ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, "Drop an instrument or WAV\nor use Choose", egui::FontId::proportional(13.0), DIM());
                    }
                });
                if let Some(t) = picked { assign(p, lane, t, dc); }
                if let Some(payload) = response.dnd_release_payload::<DragItem>() {
                    match &*payload {
                        DragItem::Instrument(t) => assign(p, lane, *t, dc),
                        DragItem::Sample(path) => { lib.assign_track_sampler(p, lane, path); assign(p, lane, Track::Sampler, dc); }
                        _ => {}
                    }
                }
                if response.dnd_hover_payload::<DragItem>().is_some() { ui.painter().rect_stroke(response.rect, 5.0, Stroke::new(2.0_f32, TEAL())); }
            } else {
                card_ui(ui, 220.0, |ui| {
                    ui.label(RichText::new("Audio input").strong()); ui.separator();
                    ui.label(RichText::new("Audio clips on this track").small().color(DIM()));
                    let tr = &mut p.arrangement.tracks[lane];
                    ui.add(egui::Slider::new(&mut tr.volume, 0.0..=1.5).text("Gain"));
                    ui.add(egui::Slider::new(&mut tr.transpose, -24..=24).text("Transpose"));
                });
            }
            if let (Some(i), true) = (out, part != 1 && !mobile_instrument) {
                let chain = &mut p.insert_fx[i];
                let len = chain.len(); let mut clear = None; let mut swap = None;
                for k in 0..len {
                    if chain[k].empty { continue; }
                    if !mobile_effects {
                        ui.label(RichText::new("→").color(DIM()));
                    }
                    ui.push_id(("chain_device", lane, i, k), |ui| {
                        let slot = &mut chain[k];
                        // Ableton style: a click on the device's strip folds it into a narrow vertical bar, a click on the bar opens it again
                        let fold_id = egui::Id::new(("device_folded", lane, i, k));
                        let folded = ui.data(|d| d.get_temp::<bool>(fold_id)).unwrap_or(false);
                        let response = if mobile_effects {
                            card_ui(ui, (ui.available_width() - 22.0).max(0.0), |ui| {
                                super::plugin_ui::plugin_grid(ui, slot);
                            })
                        } else if folded {
                            let h = ui.available_height().max(150.0).min(240.0);
                            let (rect, r) = ui.allocate_exact_size(vec2(34.0, h), Sense::click());
                            let p = ui.painter();
                            p.rect_filled(rect, card(), if r.hovered() { PANEL3() } else { PANEL2() });
                            p.rect_stroke(rect, card(), Stroke::new(1.0, border()));
                            let galley = p.layout_no_wrap(slot.kind.name().to_string(), egui::FontId::proportional(13.0), TEXT());
                            let at = rect.center_bottom() - vec2(galley.size().y * 0.5, 10.0);
                            p.add(egui::epaint::TextShape::new(at, galley, TEXT()).with_angle(-std::f32::consts::FRAC_PI_2));
                            p.circle_filled(rect.center_top() + vec2(0.0, 14.0), 4.5, if slot.on { TEAL() } else { DIM() });
                            if r.clicked() { ui.data_mut(|d| d.insert_temp(fold_id, false)); }
                            r.clone().on_hover_text("Click to open the device");
                            r
                        } else {
                            let r = card_ui(ui, 420.0, |ui| {
                                super::plugin_ui::plugin_grid(ui, slot);
                            });
                            let tab = egui::Rect::from_min_size(r.rect.right_top() - vec2(30.0, -4.0), vec2(24.0, 18.0));
                            let t = ui.interact(tab, fold_id.with("btn"), Sense::click());
                            ui.painter().rect_filled(tab, 4.0, if t.hovered() { PANEL3() } else { PANEL2() });
                            ui.painter().text(tab.center(), egui::Align2::CENTER_CENTER, "<", egui::FontId::proportional(13.0), TEXT());
                            if t.on_hover_text("Fold the device into a narrow bar").clicked() { ui.data_mut(|d| d.insert_temp(fold_id, true)); }
                            r
                        };
                        response.context_menu(|ui| {
                            if ui.button("Clear device").clicked() { clear = Some(k); ui.close_menu(); }
                            if ui.add_enabled(k > 0, egui::Button::new(if mobile_effects { "Move up" } else { "Move left" })).clicked() { swap = Some((k - 1, k)); ui.close_menu(); }
                            if ui.add_enabled(k + 1 < len, egui::Button::new(if mobile_effects { "Move down" } else { "Move right" })).clicked() { swap = Some((k, k + 1)); ui.close_menu(); }
                        });
                    });
                }
                if let Some(k) = clear { chain[k] = FxSlot::empty(); }
                if let Some((a, b)) = swap { chain.swap(a, b); }
            }
            if part != 1 && !mobile_instrument {
            if !mobile_effects {
                ui.label(RichText::new("→").color(DIM()));
            }
            let width = if mobile_effects { (ui.available_width() - 22.0).max(0.0) } else { 184.0 };
            card_ui(ui, width, |ui| {
                ui.label(RichText::new("Audio effects").strong()); ui.separator();
                ui.label(RichText::new("Add the next device\nto this signal chain").small().color(DIM()));
                ui.add_space(10.0);
                ui.menu_button("+ Add effect", |ui| {
                    for kind in ALL { if ui.button(kind.name()).clicked() { add_fx = Some(kind); ui.close_menu(); } }
                });
            }); }
    };
    if mobile_instrument || mobile_effects {
        ui.vertical(chain_content);
    } else {
        egui::ScrollArea::horizontal().id_salt(("track_device_chain", lane)).auto_shrink([false, false]).show(ui, |ui| {
            ui.horizontal(chain_content);
        });
    }
    if let Some(kind) = add_fx {
        if let Some(i) = out.or_else(|| free_insert(p)) {
            p.arrangement.tracks[lane].output = Some(i);
            let chain = &mut p.insert_fx[i];
            if let Some(slot) = chain.iter_mut().find(|s| s.empty) { *slot = FxSlot::new(kind); } else { chain.push(FxSlot::new(kind)); }
        } else { lib.message = "No free mixer insert. Choose a channel in the track routing first.".into(); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_mixer_never_reuses_another_tracks_insert() {
        let mut p = Project::default();
        for i in 0..NUM_INSERTS {
            p.arrangement.add_track(TrackKind::Midi);
            p.arrangement.tracks[i].output = Some(i);
        }
        assert_eq!(free_insert(&p), None);
    }

    #[test]
    fn inactive_scenes_keep_their_routing_reserved() {
        let mut p = Project::default();
        p.arrangement.add_track(TrackKind::Midi);
        p.arrangement.tracks[0].output = Some(0);
        p.new_scene();
        assert_ne!(free_insert(&p), Some(0));
    }

    #[test]
    fn empty_positions_do_not_reserve_an_unused_insert() {
        let mut p = Project::default();
        p.insert_fx.resize_with(NUM_INSERTS, Vec::new);
        p.insert_fx[0] = vec![FxSlot::empty(); 10];
        assert_eq!(free_insert(&p), Some(0));
    }
}

/// Vertical pill button of the bottom dock.
fn pill(ui: &mut Ui, label: &str, on: bool) -> bool {
    let w = ui.painter().layout_no_wrap(label.to_string(), egui::FontId::proportional(12.5), TEXT()).size().x + 32.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(w.max(96.0), 28.0), Sense::click());
    let p = ui.painter();
    let fill = if on { TEAL() } else if resp.hovered() { PANEL3() } else { PANEL2() };
    gloss(p, rect, 14.0, fill);
    p.rect_stroke(rect, 14.0, Stroke::new(1.0, if on { TEAL() } else { border() }));
    p.text(rect.center(), egui::Align2::CENTER_CENTER, label, egui::FontId::proportional(12.5), if on { egui::Color32::WHITE } else { TEXT() });
    resp.clicked()
}

/// vertical tab of the dock's side bar: flat row, brass bar and tint when open
fn side_tab(ui: &mut Ui, label: &str, on: bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::click());
    let p = ui.painter();
    if on {
        p.rect_filled(rect, corner(), tint(PANEL2(), TEAL(), if is_dark() { 0.30 } else { 0.22 }));
        p.rect_filled(egui::Rect::from_min_size(rect.left_top() + vec2(0.0, 7.0), vec2(3.0, rect.height() - 14.0)), 1.5, TEAL());
    } else if resp.hovered() {
        p.rect_filled(rect, corner(), PANEL3());
    }
    p.text(rect.left_center() + vec2(14.0, 0.0), egui::Align2::LEFT_CENTER, label, egui::FontId::proportional(13.0), if on { TEXT() } else { DIM() });
    resp.clicked()
}

/// flat outline tab for a horizontal row (works in light and dark)
fn pill_tab(ui: &mut Ui, label: &str, on: bool) -> bool {
    let w = ui.fonts(|f| f.layout_no_wrap(label.to_string(), egui::FontId::proportional(13.0), TEXT()).size().x) + 28.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 26.0), Sense::click());
    let p = ui.painter();
    if on { p.rect_filled(rect, 0.0, TEAL().linear_multiply(0.18)); } else if resp.hovered() { p.rect_filled(rect, 0.0, TEXT().linear_multiply(0.06)); }
    p.rect_stroke(rect, 0.0, Stroke::new(1.0, if on { TEAL() } else { TEXT().linear_multiply(0.35) }));
    p.text(rect.center(), egui::Align2::CENTER_CENTER, label, egui::FontId::proportional(13.0), if on { TEXT() } else { DIM() });
    resp.clicked()
}

/// touch-sized pill (phone Track Settings tabs and toggles)
fn mobile_pill(ui: &mut Ui, label: &str, on: bool) -> bool {
    let font = egui::FontId::proportional(14.0);
    let w = ui.fonts(|f| f.layout_no_wrap(label.to_string(), font.clone(), TEXT()).size().x) + 30.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 36.0), Sense::click());
    let p = ui.painter();
    if on {
        p.rect_filled(rect, rect.height() * 0.5, tint(PANEL2(), TEAL(), if is_dark() { 0.45 } else { 0.30 }));
    } else {
        p.rect_filled(rect, rect.height() * 0.5, PANEL2());
        p.rect_stroke(rect, rect.height() * 0.5, Stroke::new(1.0, border()));
    }
    p.text(rect.center(), egui::Align2::CENTER_CENTER, label, font, if on { TEXT() } else { DIM() });
    resp.clicked()
}

fn section(ui: &mut Ui, title: &str, width: f32, content: impl FnOnce(&mut Ui)) {
    // an infinite width = fill the row (phone): the frame's margins and stroke come off first
    let width = if width.is_finite() { width } else { ui.available_width() - 22.0 };
    egui::Frame::none().fill(PANEL2()).rounding(card()).stroke(Stroke::new(1.0, border())).inner_margin(10.0).show(ui, |ui| {
        ui.vertical(|ui| {
            ui.set_width(width.min(ui.available_width()));
            ui.set_min_height(150.0);
            ui.label(RichText::new(title).strong());
            ui.add_space(4.0);
            content(ui);
        });
    });
}

/// Track Settings: General, Input / Output, Time & Pitch and Routing of the selected track.
/// `lead` draws extra cards in front of the track's own (the options of the selected clip), in the same flow.
fn track_settings(ui: &mut Ui, p: &mut Project, lane: usize, mobile: bool, mut extra: impl FnMut(&mut Ui, &mut Project, u8)) {
    let Some((kind, name)) = p.arrangement.tracks.get(lane).map(|t| (t.kind, t.name.clone())) else { return };
    let is_midi = kind == TrackKind::Midi;
    let pages: &[(&str, &[u8])] = if mobile {
        &[("Main", &[7, 0, 2, 4]), ("Effects", &[6])]
    } else if is_midi {
        &[("Main", &[1, 5]), ("Instrument", &[0]), ("Routing", &[2, 4])]
    } else {
        &[("Main", &[1, 3]), ("Sample", &[0]), ("Routing", &[2, 4])]
    };
    let tab_id = if mobile { egui::Id::new(("m_track_settings_page", lane)) } else { egui::Id::new(("ts_page", lane, mobile)) };
    let mut cur_tab = ui.data(|d| d.get_temp::<usize>(tab_id)).unwrap_or(0).min(pages.len() - 1);
    let body_rect = if mobile {
        ui.add_space(8.0);
        let mut body = ui.available_rect_before_wrap();
        let rail = Rect::from_min_size(body.min, vec2(76.0, body.height()));
        ui.painter().rect_filled(rail, card(), PANEL2());
        let mut tabs = ui.new_child(egui::UiBuilder::new().max_rect(rail.shrink(4.0)).layout(egui::Layout::top_down(egui::Align::LEFT)));
        for (i, (label, _)) in pages.iter().enumerate() {
            if tabs.add_sized([68.0, 44.0], egui::Button::new(*label).selected(cur_tab == i)).clicked() {
                cur_tab = i;
            }
        }
        body.min.x = rail.right() + 8.0;
        body
    } else {
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("Track Settings – {name}")).strong().size(14.0));
            ui.add_space(16.0);
            for (i, (label, _)) in pages.iter().enumerate() {
                if pill_tab(ui, label, cur_tab == i) { cur_tab = i; }
            }
        });
        ui.add_space(6.0);
        ui.available_rect_before_wrap()
    };
    ui.data_mut(|d| d.insert_temp(tab_id, cur_tab));
    let tids = pages[cur_tab].1;
    ui.allocate_new_ui(egui::UiBuilder::new().max_rect(body_rect).layout(egui::Layout::top_down(egui::Align::LEFT)), |ui| {
    if mobile {
        ui.set_clip_rect(body_rect.intersect(ui.clip_rect()));
    }
    // cards fill the width on the phone (section() clamps to the room there is)
    let k = if mobile { f32::INFINITY } else { 1.0f32 };
    let scroll = if mobile { egui::ScrollArea::vertical() } else { egui::ScrollArea::both() };
    scroll.id_salt(("track_settings", lane, cur_tab)).auto_shrink([false, false]).show(ui, |ui| {
        let layout = if mobile { egui::Layout::top_down_justified(egui::Align::LEFT) } else { egui::Layout::left_to_right(egui::Align::TOP).with_main_wrap(true) };
        ui.with_layout(layout, |ui| {
            ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
            for &tid in tids {
            if tid == 0 {
                extra(ui, p, 0);
                continue;
            }
            if tid == 5 || tid == 6 {
                extra(ui, p, if tid == 5 { 1 } else { 2 });
                continue;
            }
            let Some(tr) = p.arrangement.tracks.get_mut(lane) else { continue };
            match tid {
                7 => section(ui, if kind == TrackKind::Midi { "MIDI track" } else { "Audio track" }, k, |ui| {
                    ui.spacing_mut().slider_width = (ui.available_width() - 96.0).max(80.0);
                    ui.label(RichText::new("Name").small().color(DIM()));
                    ui.add(egui::TextEdit::singleline(&mut tr.name).desired_width(f32::INFINITY));
                    ui.add_space(8.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        let mute = tr.mute;
                        if mobile_pill(ui, "Mute", mute) { tr.mute = !mute; }
                        let solo = tr.solo;
                        if mobile_pill(ui, "Solo", solo) { tr.solo = !solo; }
                        let arm = tr.arm;
                        if mobile_pill(ui, "Rec", arm) { tr.arm = !arm; }
                    });
                    ui.add_space(10.0);
                    ui.label(RichText::new("Volume").small().color(DIM()));
                    ui.add(egui::Slider::new(&mut tr.volume, 0.0..=1.5)
                        .custom_formatter(|v, _| format!("{:+.1} dB", super::widgets::db(v as f32))));
                    ui.add_space(6.0);
                    ui.label(RichText::new(if kind == TrackKind::Midi { "Transpose (notes)" } else { "Pitch (resample)" }).small().color(DIM()));
                    ui.add(egui::Slider::new(&mut tr.transpose, -24..=24).suffix(" st"));
                }),
                1 => section(ui, "General", 260.0 * k, |ui| {
                    ui.label(RichText::new(if kind == TrackKind::Midi { "MIDI track" } else { "Audio track" }).small().strong().color(TEAL()));
                    ui.label(RichText::new("Track Name").small().color(DIM()));
                    ui.text_edit_singleline(&mut tr.name);
                    ui.add_space(4.0);
                    super::widgets::knob(ui, "Volume", &mut tr.volume, 0.0, 1.5, false, |v| format!("{:+.1} dB", super::widgets::db(v)));
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut tr.mute, "Mute");
                        ui.checkbox(&mut tr.solo, "Solo");
                        ui.checkbox(&mut tr.arm, "Rec");
                    });
                }),
                2 => section(ui, "Input / Output", 340.0 * k, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.vertical(|ui| {
                            ui.label(RichText::new("Input").small().color(DIM()));
                            let names = crate::arrangement::input_names(kind);
                            let cur = (tr.input as usize).min(names.len() - 1);
                            egui::ComboBox::from_id_salt(("ts_input", lane)).selected_text(names[cur]).width(120.0).show_ui(ui, |ui| {
                                for (i, n) in names.iter().enumerate() {
                                    if ui.selectable_label(cur == i, *n).clicked() { tr.input = i as u8; }
                                }
                            });
                            ui.label(RichText::new("Output").small().color(DIM()));
                            super::mixer_ui::output_picker(ui, ("ts_output", lane), &mut tr.output);
                        });
                        if !mobile {
                            ui.vertical(|ui| {
                                super::widgets::knob(ui, "Volume", &mut tr.volume, 0.0, 1.5, false, |v| format!("{:+.1} dB", super::widgets::db(v)));
                            });
                        }
                    });
                }),
                5 => section(ui, "Instrument", 300.0 * k, |ui| {
                    ui.label(RichText::new("Plays this track").small().color(DIM()));
                    let cur = tr.instrument;
                    let name = |t: Option<Track>| t.map_or("none".to_string(), |t| if t == Track::Drums { "Drums (MIDI)".to_string() } else { Inst::from_track(t).title().to_string() });
                    egui::ComboBox::from_id_salt(("ts_inst", lane)).selected_text(name(cur)).width(170.0).show_ui(ui, |ui| {
                        for t in [Track::Wavetable, Track::Synth, Track::Bass808, Track::Analog, Track::Sampler, Track::Drums] {
                            if ui.selectable_label(cur == Some(t), name(Some(t))).clicked() {
                                tr.instrument = Some(t);
                                tr.blank = false;
                            }
                        }
                    });
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        let mut t = tr.transpose as f32;
                        if super::widgets::knob(ui, "Transpose", &mut t, -24.0, 24.0, false, |v| format!("{:+.0} st", v)) {
                            tr.transpose = t.round() as i32;
                        }
                        super::widgets::knob(ui, "Volume", &mut tr.volume, 0.0, 1.5, false, |v| format!("{:+.1} dB", super::widgets::db(v)));
                    });
                }),
                3 => section(ui, "Time & Pitch", 260.0 * k, |ui| {
                    let mut t = tr.transpose as f32;
                    if super::widgets::knob(ui, "Transpose", &mut t, -24.0, 24.0, false, |v| format!("{:+.0} st", v)) {
                        tr.transpose = t.round() as i32;
                    }
                }),
                _ => section(ui, "Routing", 300.0 * k, |ui| {
                    ui.checkbox(&mut tr.sidechain_src, "Sidechain source").on_hover_text("This track's hits duck inserts that have a Duck amount");
                    ui.label(RichText::new("Insert sends and the track's plugin chain are in Track Effects.").small().color(DIM()));
                }),
            }
            }
        });
    });
        });
}
fn level_row(ui: &mut Ui, label: &str, value: &mut f32, max: f32) {
    ui.label(label);
    ui.add(egui::DragValue::new(value).range(0.0..=max).speed(0.01)
        .custom_formatter(|value, _| format!("{:+.1} dB", super::widgets::db(value as f32)))
        .custom_parser(|text| text.trim_end_matches(" dB").parse::<f64>().ok()
            .map(|db| 10.0_f64.powf(db / 20.0))));
}

fn pitch_row<T: egui::emath::Numeric>(ui: &mut Ui, label: &str, value: &mut T) {
    ui.label(label);
    ui.add(egui::DragValue::new(value).range(-24..=24).speed(0.2).suffix(" st"));
}

fn track_settings_section(ui: &mut Ui, title: &str, content: impl FnOnce(&mut Ui)) -> egui::Response {
    let width = (ui.available_width() - 22.0).max(0.0);
    let response = egui::Frame::none().fill(PANEL2()).rounding(12.0).stroke(Stroke::new(1.0, border()))
        .inner_margin(10.0).show(ui, |ui| {
            ui.set_width(width);
            ui.spacing_mut().slider_width = (width * 0.5).max(44.0);
            ui.label(RichText::new(title).small().strong().color(DIM()));
            ui.add_space(4.0);
            content(ui);
        }).response;
    ui.add_space(6.0);
    response
}

pub(super) fn mobile_track_settings(
    ui: &mut Ui,
    project: &mut Project,
    state: &mut ArrState,
    detail: &mut DetailCtx<'_>,
    plugin_window: &mut bool,
    library: &mut Library,
    shared: &crate::audio_engine::device::Shared,
) {
    let lane = state.sel_lane;
    let kind = project.arrangement.kind(lane);
    let page_id = egui::Id::new(("m_track_settings_page", lane));
    let mut page = ui.data(|d| d.get_temp::<usize>(page_id)).unwrap_or(0).min(2);
    let source_label = if kind == TrackKind::Audio { "Sample" } else { "Instrument" };
    ui.add_space(8.0);
    let screen = ui.ctx().screen_rect();
    let landscape = screen.width() >= screen.height();
    let mut tabs = |ui: &mut Ui, width: f32| {
        for (index, label) in [source_label, "Controls", "Effects"].iter().enumerate() {
            let response = if landscape {
                ui.add_sized([width, 44.0], egui::Button::new(*label).selected(page == index))
            } else {
                super::mobile::p_button(ui, label, page == index, egui::vec2(width, 44.0))
            };
            if response.clicked() {
                page = index;
            }
        }
    };
    let mut content;
    let ui = if landscape {
        let area = ui.available_rect_before_wrap();
        let sidebar_width = 116.0_f32.min(area.width() * 0.32);
        let left = area.right() - sidebar_width;
        let sidebar = egui::Rect::from_min_max(egui::pos2(left, area.top()), area.max);
        let mut nav = ui.new_child(egui::UiBuilder::new().id_salt("track_settings_sidebar").max_rect(sidebar));
        nav.set_clip_rect(sidebar.intersect(ui.clip_rect()));
        nav.set_width(sidebar_width);
        egui::ScrollArea::vertical().max_height(sidebar.height())
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .show(&mut nav, |ui| tabs(ui, sidebar_width));

        let body = egui::Rect::from_min_max(area.min, egui::pos2(left - 8.0, area.bottom()));
        content = ui.new_child(egui::UiBuilder::new().id_salt("track_settings_content").max_rect(body));
        content.set_clip_rect(body.intersect(ui.clip_rect()));
        &mut content
    } else {
        let area = ui.available_rect_before_wrap();
        let footer_top = (area.bottom() - 52.0).max(area.top());
        let footer = egui::Rect::from_min_max(egui::pos2(area.left(), footer_top), area.max);
        ui.painter().rect_filled(footer, 8.0, ui.visuals().window_fill());
        let mut nav = ui.new_child(egui::UiBuilder::new().id_salt("track_settings_footer")
            .max_rect(footer.shrink(4.0)));
        nav.set_clip_rect(footer.intersect(ui.clip_rect()));
        nav.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let width = ((ui.available_width() - 8.0) / 3.0).max(1.0);
            tabs(ui, width);
        });
        let body = egui::Rect::from_min_max(area.min, egui::pos2(area.right(), (footer_top - 7.0).max(area.top())));
        content = ui.new_child(egui::UiBuilder::new().id_salt("track_settings_content").max_rect(body));
        content.set_clip_rect(body.intersect(ui.clip_rect()));
        &mut content
    };
    ui.data_mut(|d| d.insert_temp(page_id, page));

    let playhead = shared.position.load(std::sync::atomic::Ordering::Relaxed);
    egui::ScrollArea::vertical().id_salt(("mobile_track_page", lane, page))
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
        .auto_shrink([false, false]).show(ui, |ui| match page {
            0 => {
                let clip = state.sel_clip.filter(|&index| {
                    project.arrangement.clips.get(index)
                        .is_some_and(|clip| clip.lane == lane && clip.audio == (kind == TrackKind::Audio))
                });
                if kind == TrackKind::Audio {
                    let clip = clip.or_else(|| project.arrangement.clips.iter()
                        .position(|clip| clip.audio && clip.lane == lane));
                    track_settings_section(ui, "Sample", |ui| {
                        ui.columns(2, |cols| {
                            let source = &mut cols[0];
                            let name = clip.and_then(|i| project.audio_pool.get(project.arrangement.clips[i].pattern))
                                .map_or("No audio clip", |sample| sample.name.as_str());
                            source.label(RichText::new(name).strong());
                            if let Some(i) = clip {
                                source.menu_button("Replace sample", |ui| {
                                    if let Some(path) = super::arrangement_ui::sample_picker(ui, library) {
                                        if let Ok(data) = library.get(&path) {
                                            let name = path.file_stem().map(|s| s.to_string_lossy()).unwrap_or_default();
                                            let sample = project.pool_index(&name, &path.to_string_lossy(), data);
                                            project.arrangement.clips[i].pattern = sample;
                                            state.sel_sample = Some(sample);
                                        }
                                        ui.close_menu();
                                    }
                                });
                                source.toggle_value(&mut project.arrangement.clips[i].reverse, "Reverse");
                            }
                            let playback = &mut cols[1];
                            playback.label(RichText::new("Playback").small().strong().color(DIM()));
                            let track = &mut project.arrangement.tracks[lane];
                            level_row(playback, "Track level", &mut track.volume, 1.5);
                            if let Some(i) = clip {
                                let clip = &mut project.arrangement.clips[i];
                                level_row(playback, "Clip gain", &mut clip.gain, 2.0);
                                pitch_row(playback, "Clip pitch", &mut clip.pitch);
                                let max_fade = clip.len / 2;
                                playback.spacing_mut().slider_width = (playback.available_width() - 4.0).max(16.0);
                                playback.label("Fade in");
                                playback.add(egui::Slider::new(&mut clip.fade_in, 0..=max_fade).show_value(false));
                                playback.label("Fade out");
                                playback.add(egui::Slider::new(&mut clip.fade_out, 0..=max_fade).show_value(false));
                            } else {
                                pitch_row(playback, "Pitch", &mut track.transpose);
                            }
                        });
                    });
                    if let Some(sample) = clip.and_then(|i| project.audio_pool.get(project.arrangement.clips[i].pattern)) {
                        track_settings_section(ui, "Waveform", |ui| {
                            super::track_ui::sample_waveform(ui, sample, project.bpm, shared);
                        });
                    }
                } else {
                    show(ui, project, state, detail, plugin_window, library, playhead, 3);
                    let track = &project.arrangement.tracks[lane];
                    if let Some(sample) = track.sampler.sample.as_ref().filter(|_| track.instrument == Some(Track::Sampler)) {
                        track_settings_section(ui, "Waveform", |ui| {
                            super::track_ui::sample_waveform(ui, sample, project.bpm, shared);
                        });
                    }
                }
            }
            1 => {
                let track = &mut project.arrangement.tracks[lane];
                ui.label("Track name");
                ui.add(egui::TextEdit::singleline(&mut track.name).desired_width(f32::INFINITY));
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    ui.toggle_value(&mut track.mute, "Mute");
                    ui.toggle_value(&mut track.solo, "Solo");
                    ui.toggle_value(&mut track.arm, "Arm");
                });
                ui.separator();
                ui.spacing_mut().slider_width = (ui.available_width() * 0.55).max(96.0);
                ui.label("Level");
                ui.add(egui::Slider::new(&mut track.volume, 0.0..=1.5)
                    .custom_formatter(|v, _| format!("{:+.1} dB", super::widgets::db(v as f32))));
                ui.label(if kind == TrackKind::Midi { "Transpose" } else { "Pitch" });
                ui.add(egui::Slider::new(&mut track.transpose, -24..=24).suffix(" st"));
                ui.separator();
                ui.label("Input");
                let inputs = crate::arrangement::input_names(kind);
                let input = (track.input as usize).min(inputs.len() - 1);
                egui::ComboBox::from_id_salt(("mobile_track_input", lane)).selected_text(inputs[input])
                    .show_ui(ui, |ui| {
                        for (index, name) in inputs.iter().enumerate() {
                            ui.selectable_value(&mut track.input, index as u8, *name);
                        }
                    });
                ui.label("Output");
                super::mixer_ui::output_picker_w(ui, ("mobile_track_output", lane), &mut track.output, 124.0);
                ui.checkbox(&mut track.sidechain_src, "Sidechain source");
            }
            _ => show(ui, project, state, detail, plugin_window, library, playhead, 4),
        });
}

/// id of the dock page (0 Track Settings, 1 Instruments, 2 Track Effects, 3 Editor); set it to 3 to open the clip editor
pub fn dock_page_id() -> egui::Id {
    egui::Id::new("bottom_dock_page")
}

/// the track whose clip was last clicked: only that track shows the Piano Roll / Sample Editor tab
pub fn dock_editor_id() -> egui::Id {
    egui::Id::new("bottom_dock_editor_lane")
}

/// Point the editor at another Playlist track (its pattern and instrument).
fn switch_lane(ui: &mut Ui, p: &mut Project, st: &mut ArrState, roll: &mut super::roll_ui::RollState, live_track: &mut Track, l: usize) {
    if l >= p.arrangement.tracks.len() {
        return;
    }
    st.sel_lane = l;
    ui.data_mut(|d| d.insert_temp(dock_editor_id(), l));
    if p.arrangement.tracks[l].kind == TrackKind::Midi {
        p.current_pattern = p.lane_pattern(l);
        let t = p.arrangement.tracks[l].instrument.unwrap_or(Track::Synth);
        roll.track = t;
        *live_track = t;
    }
}

/// Bottom dock: a row of pill tabs on top (Track Settings / Instruments / Track Effects / Editor) and the page of the chosen one.
/// The Editor page holds the piano roll of a MIDI clip or the sample view of an audio clip.
pub fn dock(
    ui: &mut Ui,
    p: &mut Project,
    st: &mut ArrState,
    roll: &mut super::roll_ui::RollState,
    live_track: &mut Track,
    inst: &mut Inst,
    plugin_window: &mut bool,
    lib: &mut Library,
    playhead: u32,
    shared: &crate::audio_engine::device::Shared,
) {
    let id = dock_page_id();
    let mut page = ui.data(|d| d.get_temp::<u8>(id)).unwrap_or(0);
    let lane = st.sel_lane;
    let audio = p.arrangement.tracks.get(lane).map_or(false, |t| t.kind == TrackKind::Audio);
    // the editor tab exists only after a clip of this track was clicked
    let editor_open = ui.data(|d| d.get_temp::<usize>(dock_editor_id())) == Some(lane);
    if (!editor_open || audio) && page == 3 { page = 0; }
    // the old Instruments page is the "Instrument" tab of Track (MIDI tracks)
    if page == 1 {
        page = 0;
        if !audio {
            ui.data_mut(|d| d.insert_temp(egui::Id::new("track_settings_tab"), 2usize));
        }
    }
    // a horizontal ui is only one line high, so the height of the whole dock is taken before it
    let mut switch_to: Option<usize> = None;
    let avail = ui.available_size();
    // a side bar of its own: vertical tabs on the right, the content of the open one on the left
    ui.allocate_ui_with_layout(avail, egui::Layout::right_to_left(egui::Align::TOP), |ui| {
        ui.allocate_ui_with_layout(vec2(132.0, avail.y), egui::Layout::top_down(egui::Align::LEFT), |ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            for (i, n) in ["Track", "", "Track Effects", if audio { "Sample Editor" } else { "Piano Roll" }].into_iter().enumerate() {
                if i == 1 || (i == 3 && (!editor_open || audio)) { continue; }
                if side_tab(ui, n, page == i as u8) { page = i as u8; }
            }
        });
        ui.add_space(4.0);
        ui.separator();
        ui.add_space(4.0);
        let size = vec2(ui.available_width(), avail.y);
        ui.allocate_ui_with_layout(size, egui::Layout::top_down(egui::Align::LEFT), |ui| {
            match page {
                0 => {
                    // one panel: the options of the selected clip first (when there is one), then the track's cards
                    let clip = st.sel_clip.filter(|&i| p.arrangement.clips.get(i).map_or(false, |c| c.lane == lane));
                    super::theme::FLAT.store(true, std::sync::atomic::Ordering::Relaxed);
                    {
                        let w = &mut ui.visuals_mut().widgets;
                        for v in [&mut w.inactive, &mut w.hovered, &mut w.active, &mut w.open, &mut w.noninteractive] { v.rounding = egui::Rounding::ZERO; }
                        ui.visuals_mut().menu_rounding = egui::Rounding::ZERO;
                    }
                    track_settings(ui, p, lane, false, |ui, p, what| {
                        if what == 1 {
                            let mut dc = DetailCtx { roll_track: &mut roll.track, live_track, inst };
                            show(ui, p, st, &mut dc, plugin_window, lib, playhead, 1);
                        } else {
                            if let Some(i) = clip {
                                egui::Frame::none().fill(PANEL2()).rounding(card()).stroke(Stroke::new(1.0, border())).inner_margin(10.0).show(ui, |ui| {
                                    ui.set_width(280.0);
                                    super::arrangement_ui::clip_options(ui, p, st, lib, i, playhead, false);
                                });
                            }
                            // the sample editor of an audio track lives in the Track tab now
                            if audio && p.arrangement.tracks.get(lane).is_some() {
                                ui.add_space(6.0);
                                ui.label(RichText::new(format!("Sample Editor – {}", p.arrangement.tracks[lane].name)).strong().size(14.0));
                                ui.add_space(4.0);
                                super::track_ui::sample_panel(ui, p, lane, st.sel_sample, true, shared);
                            }
                        }
                    });
                    super::theme::FLAT.store(false, std::sync::atomic::Ordering::Relaxed);
                }
                3 => {
                    if p.arrangement.tracks.get(lane).is_none() {
                        ui.label(RichText::new("Add a track in the Playlist first.").color(DIM()));
                    } else if audio {
                        ui.label(RichText::new(format!("Sample Editor – {}", p.arrangement.tracks[lane].name)).strong().size(14.0));
                        ui.add_space(4.0);
                        super::track_ui::sample_panel(ui, p, lane, st.sel_sample, true, shared);
                    } else {
                        let mut switch: Option<usize> = None;
                        if let Some(l) = switch.or(switch_to.take()) {
                            switch_lane(ui, p, st, roll, live_track, l);
                        }
                        ui.add_space(4.0);
                        roll.lane = lane;
                        roll.compact = true;
                        super::roll_ui::show(ui, p, roll, shared);
                        roll.compact = false;
                        if let Some(l) = roll.switch_to.take() {
                            switch_lane(ui, p, st, roll, live_track, l);
                        }
                    }
                }
                n => {
                    let mut dc = DetailCtx { roll_track: &mut roll.track, live_track, inst };
                    show(ui, p, st, &mut dc, plugin_window, lib, playhead, if n == 1 { 1 } else { 2 });
                }
            }
        });
    });
    ui.data_mut(|d| d.insert_temp(id, page));
}
