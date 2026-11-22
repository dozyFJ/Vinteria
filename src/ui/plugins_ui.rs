//! Plugins page: every instrument and effect as a card in a grid, sorted into sections, with a search toolbar.
//! A click opens the plugin (its own GUI and presets) on the right; plugins are added to a mixer insert or to a track's effects.
use super::instruments_ui::Inst;
use super::theme::*;
use super::Tab;
use crate::arrangement::TrackKind;
use crate::fx::{FxKind, FxSlot};
use crate::mixer::{NUM_CH, NUM_INSERTS};
use crate::piano_roll::Track;
use crate::project::Project;
use egui::{pos2, vec2, Align2, Color32, FontId, RichText, Sense, Stroke, Ui};

const CARD_W: f32 = 158.0;
const CARD_H: f32 = 74.0;
pub(super) const MAX_SLOTS: usize = 10;

#[derive(Clone, PartialEq)]
enum Pick {
    Fx(FxKind),
    Inst(Track),
}

#[derive(Clone, Default)]
struct St {
    search: String,
    /// 0 = all, 1 = instruments, 2.. = effect categories
    cat: usize,
    pick: Option<Pick>,
    /// the plugin shown in the side GUI keeps its knob settings while another one is looked at
    slots: Vec<FxSlot>,
    insert: usize,
    msg: String,
}

impl Default for Pick {
    fn default() -> Self { Pick::Fx(FxKind::Filter) }
}

const CATS: [&str; 8] = ["All", "Instruments", "EQ / Filter", "Dynamics", "Space", "Modulation", "Colour", "Utility"];

fn cat_color(cat: &str) -> Color32 {
    match cat {
        "EQ / Filter" => lane_col(3),
        "Dynamics" => lane_col(0),
        "Space" => lane_col(5),
        "Modulation" => lane_col(6),
        "Colour" => lane_col(2),
        "Utility" => lane_col(8),
        _ => lane_col(7),
    }
}

/// put a copy of the plugin into the first free slot of the insert; false when the chain is full
pub fn add_to_insert(project: &mut Project, insert: usize, slot: &FxSlot) -> bool {
    project.insert_fx.resize_with(NUM_INSERTS, Vec::new);
    let chain = if insert == NUM_INSERTS { &mut project.master_chain } else if let Some(c) = project.insert_fx.get_mut(insert) { c } else { return false };
    if let Some(k) = chain.iter().position(|s| s.empty) {
        chain[k] = slot.clone();
    } else if chain.len() < MAX_SLOTS {
        chain.push(slot.clone());
    } else {
        return false;
    }
    true
}

/// add the plugin to the effects of a track: the track plays through an insert, a track without one gets a free insert
pub(super) fn add_to_track(project: &mut Project, lane: usize, slot: &FxSlot) -> Result<usize, &'static str> {
    project.insert_fx.resize_with(NUM_INSERTS, Vec::new);
    let used: Vec<usize> = project.arrangement.tracks.iter().filter_map(|t| t.output)
        .chain(project.channels.iter().take(NUM_CH).filter_map(|channel| channel.output))
        .chain(project.scenes.iter().flat_map(|scene| scene.arrangement.tracks.iter().filter_map(|track| track.output)))
        .collect();
    let out = match project.arrangement.tracks.get(lane).ok_or("No such track")?.output {
        Some(o) => o.min(NUM_INSERTS - 1),
        None => {
            let free = (0..NUM_INSERTS).find(|i| !used.contains(i) && project.insert_fx[*i].iter().all(|s| s.empty)).ok_or("No free mixer insert for this track")?;
            project.arrangement.tracks[lane].output = Some(free);
            free
        }
    };
    if add_to_insert(project, out, slot) { Ok(out) } else { Err("That insert has no free slot") }
}

fn matches_q(q: &str, name: &str, cat: &str) -> bool {
    q.is_empty() || name.to_lowercase().contains(q) || cat.to_lowercase().contains(q)
}

#[cfg(test)]
mod routing_tests {
    use super::*;

    #[test]
    fn master_effects_do_not_land_on_the_last_insert() {
        let mut project = Project::default();
        let slot = FxSlot::new(FxKind::Filter);
        assert!(add_to_insert(&mut project, NUM_INSERTS, &slot));
        assert_eq!(project.master_chain.len(), 1);
        assert!(project.insert_fx.iter().all(Vec::is_empty));
        project.master_chain.resize(MAX_SLOTS, slot.clone());
        assert!(!add_to_insert(&mut project, NUM_INSERTS, &slot));
        project.master_chain[3] = FxSlot::empty();
        assert!(add_to_insert(&mut project, NUM_INSERTS, &slot));
        assert!(!project.master_chain[3].empty);
        assert!(!add_to_insert(&mut project, NUM_INSERTS + 1, &slot));
    }
}

/// a painted card; returns (clicked, double_clicked, right-click response for the menu)
fn card(ui: &mut Ui, name: &str, sub: &str, col: Color32, selected: bool, tag: Option<&str>) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(CARD_W, CARD_H), Sense::click());
    let p = ui.painter();
    let fill = if selected { tint(PANEL2(), TEAL(), 0.35) } else if resp.hovered() { PANEL3() } else { PANEL2() };
    p.rect_filled(rect, corner() + 2.0, fill);
    p.rect_stroke(rect, corner() + 2.0, Stroke::new(if selected { 2.0 } else { border_w() }, if selected { TEAL() } else { border() }));
    let strip = egui::Rect::from_min_size(rect.min, vec2(5.0, rect.height()));
    p.rect_filled(strip, corner(), col);
    // a tiny "knob" glyph so a card looks like a device
    let c = pos2(rect.left() + 28.0, rect.center().y);
    p.circle_stroke(c, 13.0, Stroke::new(2.0, col.linear_multiply(0.9)));
    let a = -1.2f32 + (name.len() % 7) as f32 * 0.35;
    p.line_segment([c, c + vec2(a.sin(), -a.cos()) * 9.0], Stroke::new(2.0, TEXT()));
    p.text(pos2(rect.left() + 50.0, rect.top() + 24.0), Align2::LEFT_CENTER, name, FontId::proportional(13.0), TEXT());
    p.text(pos2(rect.left() + 50.0, rect.top() + 42.0), Align2::LEFT_CENTER, sub, FontId::proportional(10.5), DIM());
    if let Some(t) = tag {
        p.text(pos2(rect.right() - 6.0, rect.bottom() - 9.0), Align2::RIGHT_CENTER, t, FontId::proportional(9.5), CYAN());
    }
    p.text(pos2(rect.left() + 12.0, rect.bottom() - 9.0), Align2::LEFT_CENTER, "double-click: add", FontId::proportional(9.0), DIM().linear_multiply(0.7));
    resp
}

/// the "add to" menu shared by the card's right-click and the side panel
fn add_menu(ui: &mut Ui, project: &mut Project, slot: &FxSlot, msg: &mut String) {
    ui.label(RichText::new("Mixer insert").small().color(DIM()));
    if ui.button("Master").clicked() {
        *msg = if add_to_insert(project, NUM_INSERTS, slot) { format!("{} added to the Master", slot.kind.name()) } else { "The master has no free slot".into() };
        ui.close_menu();
    }
    for i in 0..NUM_INSERTS {
        let name = project.channels.get(NUM_CH + i).map(|c| c.name.clone()).unwrap_or_default();
        if ui.button(format!("{} · {}", i + 1, name)).clicked() {
            *msg = if add_to_insert(project, i, slot) { format!("{} added to Insert {}", slot.kind.name(), i + 1) } else { "That insert has no free slot".into() };
            ui.close_menu();
        }
    }
    ui.separator();
    ui.label(RichText::new("Track effects (audio / MIDI)").small().color(DIM()));
    let lanes = project.arrangement.tracks.len();
    if lanes == 0 {
        ui.label(RichText::new("No tracks yet").color(DIM()));
    }
    for l in 0..lanes {
        let t = &project.arrangement.tracks[l];
        let label = format!("{}  {}", if t.kind == TrackKind::Audio { "AUDIO" } else { "MIDI" }, t.name);
        if ui.button(label).clicked() {
            *msg = match add_to_track(project, l, slot) {
                Ok(o) => format!("{} added to the track, Insert {}", slot.kind.name(), o + 1),
                Err(e) => e.to_string(),
            };
            ui.close_menu();
        }
    }
}

fn assign_instrument(project: &mut Project, lane: usize, t: Track) -> bool {
    if project.arrangement.tracks.get(lane).map(|x| x.kind) != Some(TrackKind::Midi) {
        return false;
    }
    project.arrangement.tracks[lane].instrument = Some(t);
    project.arrangement.tracks[lane].blank = false;
    project.adopt_track_params();
    true
}

/// the page; returns a tab to switch to (an instrument's editor)
pub fn show(ui: &mut Ui, project: &mut Project, live_track: &mut Track, roll_track: &mut Track, inst: &mut Inst, sel_lane: usize) -> Option<Tab> {
    let id = egui::Id::new("plugins_page_state");
    let mut st: St = ui.data(|d| d.get_temp::<St>(id)).unwrap_or_default();
    let mut go = None;
    project.insert_fx.resize_with(NUM_INSERTS, Vec::new);
    st.insert = st.insert.min(NUM_INSERTS);

    // toolbar: search + section chips
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("Search").small().color(DIM()));
        ui.add(egui::TextEdit::singleline(&mut st.search).hint_text("plugin, category...").desired_width(170.0));
        if !st.search.is_empty() && ui.small_button("x").clicked() {
            st.search.clear();
        }
    });
    ui.separator();

    // the categories as a menu on the left
    egui::SidePanel::left("plugins_cats").exact_width(168.0).resizable(false).frame(egui::Frame::none().fill(SIDE()).stroke(Stroke::new(border_w(), border())).inner_margin(egui::Margin::symmetric(8.0, 8.0))).show_inside(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        for (i, c) in CATS.iter().enumerate() {
            let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::click());
            let on = st.cat == i;
            let p = ui.painter();
            if on {
                p.rect_filled(r, 10.0, tint(PANEL2(), TEAL(), if is_dark() { 0.34 } else { 0.24 }));
            } else if resp.hovered() {
                p.rect_filled(r, 10.0, PANEL3());
            }
            let dot = if i >= 2 { cat_color(c) } else { TEAL() };
            p.circle_filled(pos2(r.left() + 16.0, r.center().y), 4.0, if on { TEAL() } else { dot.gamma_multiply(0.8) });
            p.text(pos2(r.left() + 30.0, r.center().y), Align2::LEFT_CENTER, *c, FontId::proportional(13.0), if on { TEXT() } else if resp.hovered() { TEXT() } else { DIM() });
            if resp.clicked() {
                st.cat = i;
            }
        }
    });

    let q = st.search.trim().to_lowercase();
    egui::SidePanel::right("plugins_detail").exact_width(430.0).frame(egui::Frame::none().fill(BG()).inner_margin(egui::Margin::symmetric(10.0, 6.0))).show_inside(ui, |ui| {
        detail(ui, project, &mut st, live_track, roll_track, inst, sel_lane, &mut go);
    });

    egui::CentralPanel::default().frame(egui::Frame::none().fill(BG())).show_inside(ui, |ui| {
        egui::ScrollArea::vertical().auto_shrink([false, false]).id_salt("plugins_grid").show(ui, |ui| {
            ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
            // instruments
            let insts: Vec<(Track, &str, &str, usize)> = vec![
                (Track::Drums, "Drums (MIDI)", "Kick, Snare, Clap, Hi-Hat, Perc", 0),
                (Track::Wavetable, "Winteria Synth", "one synth, many presets", 7),
                (Track::Sampler, "808 Sampler", "play any WAV on the keys", 9),
            ];
            let shown_i: Vec<_> = insts.iter().filter(|i| (st.cat == 0 || st.cat == 1) && matches_q(&q, i.1, "Instruments")).collect();
            if !shown_i.is_empty() {
                let n_i = shown_i.len();
                grid(ui, |ui| section_title(ui, "Instruments", n_i), |ui| {
                    for (t, n, sub, c) in shown_i {
                        let r = card(ui, n, sub, lane_col(*c), st.pick == Some(Pick::Inst(*t)), Some("INSTRUMENT"));
                        if r.clicked() {
                            st.pick = Some(Pick::Inst(*t));
                        }
                        if r.double_clicked() {
                            if assign_instrument(project, sel_lane, *t) {
                                *live_track = *t;
                                *roll_track = *t;
                                st.msg = format!("{n} set on the selected MIDI track");
                            } else {
                                st.msg = "Select a MIDI track in the Playlist first".into();
                            }
                        }
                    }
                });
            }
            // effects by category
            if st.cat != 1 {
                for cat in &CATS[2..] {
                    if st.cat >= 2 && CATS[st.cat] != *cat {
                        continue;
                    }
                    let kinds: Vec<FxKind> = crate::fx::ALL.into_iter().filter(|k| k.category() == *cat && matches_q(&q, k.name(), cat)).collect();
                    if kinds.is_empty() {
                        continue;
                    }
                    let n_k = kinds.len();
                    grid(ui, |ui| section_title(ui, cat, n_k), |ui| {
                        for k in kinds {
                            let r = card(ui, k.name(), cat, if k.is_signature() { CYAN() } else { cat_color(cat) }, st.pick == Some(Pick::Fx(k)), if k.is_signature() { Some("WINTERIA") } else { Some("EFFECT") });
                            if r.clicked() {
                                st.pick = Some(Pick::Fx(k));
                                if !st.slots.iter().any(|s| s.kind == k) {
                                    st.slots.push(FxSlot::new(k));
                                }
                            }
                            if r.double_clicked() {
                                let slot = st.slots.iter().find(|s| s.kind == k).cloned().unwrap_or_else(|| FxSlot::new(k));
                                let ins = st.insert;
                                st.msg = if add_to_insert(project, ins, &slot) { format!("{} added to Insert {}", k.name(), ins + 1) } else { "That insert has no free slot".into() };
                            }
                            r.context_menu(|ui| {
                                let slot = FxSlot::new(k);
                                add_menu(ui, project, &slot, &mut st.msg);
                            });
                        }
                    });
                }
            }
        });
    });
    ui.data_mut(|d| d.insert_temp(id, st));
    go
}

/// A section: its title and up to five cards per row, the whole block centred in the window.
fn grid(ui: &mut Ui, title: impl FnOnce(&mut Ui), cards: impl FnOnce(&mut Ui)) {
    let cols = 5.0;
    let w = cols * CARD_W + (cols - 1.0) * 8.0 + 2.0;
    let pad = ((ui.available_width() - w) / 2.0).max(0.0);
    ui.horizontal(|ui| {
        ui.add_space(pad);
        ui.vertical(|ui| {
            ui.set_max_width(w);
            title(ui);
            ui.horizontal_wrapped(cards);
        });
    });
}

fn section_title(ui: &mut Ui, name: &str, n: usize) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new(name).strong().size(14.0).color(TEXT()));
        ui.label(RichText::new(format!("{n}")).small().color(DIM()));
    });
}

fn detail(ui: &mut Ui, project: &mut Project, st: &mut St, live_track: &mut Track, roll_track: &mut Track, inst: &mut Inst, sel_lane: usize, go: &mut Option<Tab>) {
    match st.pick.clone() {
        None => {
            ui.add_space(20.0);
            ui.label(RichText::new("Click a plugin to open it here.\nDouble-click adds it to the chosen mixer insert,\nright-click offers every insert and track.").color(DIM()));
        }
        Some(Pick::Inst(t)) => {
            let i = Inst::from_track(t);
            ui.label(RichText::new(if t == Track::Drums { "Drums (MIDI)" } else { i.title() }).strong().size(16.0));
            ui.label(RichText::new(i.subtitle()).small().color(DIM()));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Open editor").on_hover_text("Presets and every knob").clicked() {
                    *live_track = t;
                    *roll_track = t;
                    *inst = i;
                    *go = Some(Tab::Instruments);
                }
                if ui.button("Use on the selected MIDI track").clicked() {
                    if assign_instrument(project, sel_lane, t) {
                        *live_track = t;
                        *roll_track = t;
                        *inst = i;
                        st.msg = "Instrument set on the selected track".into();
                    } else {
                        st.msg = "Select a MIDI track in the Playlist first".into();
                    }
                }
            });
            ui.add_space(6.0);
            ui.label(RichText::new("Set on a track:").small().color(DIM()));
            let lanes: Vec<(usize, String)> = project.arrangement.tracks.iter().enumerate().filter(|(_, t)| t.kind == TrackKind::Midi).map(|(l, t)| (l, t.name.clone())).collect();
            ui.horizontal_wrapped(|ui| {
                for (l, n) in lanes {
                    if ui.button(n).clicked() {
                        assign_instrument(project, l, t);
                        *live_track = t;
                        *roll_track = t;
                        st.msg = "Instrument set".into();
                    }
                }
            });
        }
        Some(Pick::Fx(k)) => {
            let idx = match st.slots.iter().position(|s| s.kind == k) {
                Some(i) => i,
                None => {
                    st.slots.push(FxSlot::new(k));
                    st.slots.len() - 1
                }
            };
            // add row
            ui.horizontal(|ui| {
                ui.label(RichText::new("Mixer insert").small().color(DIM()));
                egui::ComboBox::from_id_salt("plugins_insert").width(130.0).selected_text(if st.insert == NUM_INSERTS { "Master".to_string() } else { format!("{} · {}", st.insert + 1, project.channels[NUM_CH + st.insert].name) }).show_ui(ui, |ui| {
                    for i in 0..NUM_INSERTS {
                        ui.selectable_value(&mut st.insert, i, format!("{} · {}", i + 1, project.channels[NUM_CH + i].name));
                    }
                    ui.selectable_value(&mut st.insert, NUM_INSERTS, "Master");
                });
                if ui.button("Add").clicked() {
                    let ins = st.insert;
                    st.msg = if add_to_insert(project, ins, &st.slots[idx]) { format!("{} added to Insert {}", k.name(), ins + 1) } else { "That insert has no free slot".into() };
                }
                ui.menu_button("Add to...", |ui| {
                    let slot = st.slots[idx].clone();
                    add_menu(ui, project, &slot, &mut st.msg);
                });
            });
            if !st.msg.is_empty() {
                ui.label(RichText::new(st.msg.clone()).small().color(TEAL()));
            }
            ui.separator();
            egui::ScrollArea::vertical().auto_shrink([false, false]).id_salt("plugins_detail_scroll").show(ui, |ui| {
                super::plugin_ui::plugin_grid(ui, &mut st.slots[idx]);
            });
        }
    }
    if matches!(st.pick, Some(Pick::Inst(_))) && !st.msg.is_empty() {
        ui.add_space(6.0);
        ui.label(RichText::new(st.msg.clone()).small().color(TEAL()));
    }
}

/// Ctrl+L in the Playlist: every audio and MIDI track gets a mixer insert of its own (named after the track).
/// Tracks that already play through an insert keep it. Returns (newly routed, tracks left without an insert).
pub fn route_tracks_to_mixer(project: &mut Project) -> (usize, usize) {
    let mut used: Vec<usize> = project.arrangement.tracks.iter().filter_map(|t| t.output).collect();
    let (mut routed, mut left) = (0, 0);
    for l in 0..project.arrangement.tracks.len() {
        if project.arrangement.tracks[l].output.is_some() {
            continue;
        }
        match (0..NUM_INSERTS).find(|i| !used.contains(i)) {
            Some(i) => {
                used.push(i);
                project.arrangement.tracks[l].output = Some(i);
                let name = project.arrangement.tracks[l].name.clone();
                if let Some(ch) = project.channels.get_mut(NUM_CH + i) {
                    ch.name = name;
                }
                routed += 1;
            }
            None => left += 1,
        }
    }
    (routed, left)
}
