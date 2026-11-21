//! Browser panel (Modern Solid): header with pin / close, search with filters, quick sections
//! (Favorites, Recent, Samples, Instruments, Effects, VST3, Presets, Project Files, User Library) and the Preview player.
use super::browser::{row, DragItem};
use super::instruments_ui::{self, Inst};
use super::library::Library;
use super::theme::*;
use super::Tab;
use crate::audio_engine::device::Shared;
use crate::audio_engine::Cmd;
use crate::fx::{FxKind, FxSlot};
use crate::mixer::NUM_INSERTS;
use crate::piano_roll::Track;
use crate::project::Project;
#[cfg(target_os = "android")]
use crate::rfd;
use egui::{pos2, vec2, Align2, Color32, FontId, Painter, Pos2, Rect, RichText, Sense, Shape, Stroke, Ui};
use std::path::PathBuf;

pub const SECTIONS: [&str; 9] = ["Favorites", "Recent", "Samples", "Instruments", "Effects", "VST3", "Presets", "Project Files", "User Library"];
pub const SEC_SAMPLES: u8 = 2;
pub const SEC_INSTRUMENTS: u8 = 3;
pub const SEC_PROJECTS: u8 = 7;

pub struct Ctx<'a> {
    pub project: &'a mut Project,
    pub lib: &'a mut Library,
    pub shared: &'a Shared,
    pub lane: &'a mut usize,
    pub sel_lane: usize,
    pub tab: &'a mut Tab,
    pub inst: &'a mut Inst,
    pub live_track: &'a mut Track,
    pub roll_track: &'a mut Track,
    pub recent: &'a [PathBuf],
}

#[derive(Default)]
pub struct Out {
    pub close: bool,
    pub open_path: Option<PathBuf>,
}

#[derive(Clone, Copy, PartialEq)]
enum Ic { Star, Clock, Samples, Instruments, Effects, Vst, Presets, Folder, User, Pin, Close, Search, Filter, Tag, Heart, Dots, Play, Stop }

fn poly(p: &Painter, pts: Vec<Pos2>, col: Color32) {
    p.add(Shape::convex_polygon(pts, col, Stroke::NONE));
}

fn icon(p: &Painter, ic: Ic, c: Pos2, col: Color32) {
    let s = Stroke::new(1.6, col);
    match ic {
        Ic::Star => {
            let pt = |i: usize, r: f32| {
                let a = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::TAU / 10.0;
                c + vec2(a.cos(), a.sin()) * r
            };
            poly(p, (0..5).map(|k| pt(k * 2 + 1, 4.2)).collect(), col);
            for k in 0..5 {
                poly(p, vec![pt(k * 2, 9.0), pt((k * 2 + 9) % 10, 4.2), pt(k * 2 + 1, 4.2)], col);
            }
        }
        Ic::Clock => {
            p.circle_stroke(c, 8.0, s);
            p.line_segment([c, c + vec2(0.0, -5.0)], s);
            p.line_segment([c, c + vec2(4.0, 2.0)], s);
        }
        Ic::Samples => {
            for (dx, dy, r) in [(-4.5, -4.5, 4.0), (4.5, -4.0, 3.5), (-3.5, 4.5, 3.5), (4.5, 4.5, 4.0)] {
                p.circle_filled(c + vec2(dx, dy), r, col);
            }
        }
        Ic::Instruments => {
            p.rect_stroke(Rect::from_center_size(c, vec2(18.0, 16.0)), 2.0, s);
            for k in 0..3 {
                p.line_segment([c + vec2(-3.0 + k as f32 * 4.5, -8.0), c + vec2(-3.0 + k as f32 * 4.5, 8.0)], s);
            }
            p.rect_filled(Rect::from_min_size(c + vec2(-5.5, -8.0), vec2(5.0, 9.0)), 1.0, col);
        }
        Ic::Effects => {
            for (dx, dy) in [(-5.0, -3.0), (5.0, -3.0), (0.0, 5.0)] {
                p.circle_stroke(c + vec2(dx, dy), 4.2, s);
            }
        }
        Ic::Vst => {
            p.rect_stroke(Rect::from_center_size(c, vec2(17.0, 17.0)), 4.0, s);
            p.line_segment([c + vec2(-4.0, -3.0), c + vec2(0.0, 4.0)], s);
            p.line_segment([c + vec2(0.0, 4.0), c + vec2(4.0, -3.0)], s);
        }
        Ic::Presets => {
            for k in 0..3 {
                let y = c.y - 6.0 + k as f32 * 6.0;
                p.line_segment([pos2(c.x - 8.0, y), pos2(c.x + 8.0, y)], s);
                p.circle_filled(pos2(c.x - 3.0 + k as f32 * 4.0, y), 2.4, col);
            }
        }
        Ic::Folder => {
            p.rect_filled(Rect::from_min_size(c + vec2(-9.0, -7.0), vec2(8.0, 4.0)), 1.5, col);
            p.rect_filled(Rect::from_min_size(c + vec2(-9.0, -4.5), vec2(18.0, 12.0)), 2.5, col);
        }
        Ic::User => {
            p.circle_filled(c + vec2(0.0, -4.0), 4.0, col);
            p.rect_filled(Rect::from_min_size(c + vec2(-8.0, 2.0), vec2(16.0, 8.0)), 4.0, col);
        }
        Ic::Pin => {
            p.line_segment([c + vec2(-3.0, 3.0), c + vec2(4.0, -4.0)], Stroke::new(2.4, col));
            p.line_segment([c + vec2(-6.0, -1.0), c + vec2(1.0, 6.0)], Stroke::new(2.4, col));
            p.line_segment([c + vec2(-4.0, 4.0), c + vec2(-8.0, 8.0)], s);
        }
        Ic::Close => {
            p.line_segment([c + vec2(-6.0, -6.0), c + vec2(6.0, 6.0)], Stroke::new(2.0, col));
            p.line_segment([c + vec2(-6.0, 6.0), c + vec2(6.0, -6.0)], Stroke::new(2.0, col));
        }
        Ic::Search => {
            p.circle_stroke(c + vec2(-1.5, -1.5), 6.0, s);
            p.line_segment([c + vec2(3.0, 3.0), c + vec2(8.0, 8.0)], Stroke::new(2.0, col));
        }
        Ic::Filter => {
            for (k, x) in [(-5.0f32, -3.0f32), (0.0, 4.0), (5.0, -1.0)] {
                p.line_segment([c + vec2(-8.0, k), c + vec2(8.0, k)], s);
                p.circle_filled(c + vec2(x, k), 2.6, col);
            }
        }
        Ic::Tag => {
            poly(p, vec![c + vec2(-9.0, -6.0), c + vec2(3.0, -6.0), c + vec2(9.0, 0.0), c + vec2(3.0, 6.0), c + vec2(-9.0, 6.0)], col);
            p.circle_filled(c + vec2(-4.5, 0.0), 1.8, BG());
        }
        Ic::Heart => {
            p.circle_filled(c + vec2(-3.6, -2.6), 4.4, col);
            p.circle_filled(c + vec2(3.6, -2.6), 4.4, col);
            poly(p, vec![c + vec2(-7.6, -0.6), c + vec2(7.6, -0.6), c + vec2(0.0, 8.0)], col);
        }
        Ic::Dots => {
            for k in [-6.0, 0.0, 6.0] {
                p.circle_filled(c + vec2(0.0, k), 1.9, col);
            }
        }
        Ic::Play => poly(p, vec![c + vec2(-5.0, -8.0), c + vec2(-5.0, 8.0), c + vec2(8.0, 0.0)], col),
        Ic::Stop => { p.rect_filled(Rect::from_center_size(c, vec2(14.0, 14.0)), 2.0, col); }
    }
}

/// painted square icon button
fn icon_button(ui: &mut Ui, ic: Ic, size: Vec2, active: bool, tip: &str) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let p = ui.painter();
    let fill = if active { tint(PANEL2(), TEAL(), 0.45) } else if resp.hovered() { PANEL3() } else { PANEL2() };
    gloss(p, rect, 0.0, fill);
    p.rect_stroke(rect, 0.0, Stroke::new(border_w(), if active { TEAL() } else { border() }));
    icon(p, ic, rect.center(), if active { TEAL() } else { TEXT() });
    resp.on_hover_text(tip)
}
type Vec2 = egui::Vec2;

fn section_icon(i: usize) -> Ic {
    [Ic::Star, Ic::Clock, Ic::Samples, Ic::Instruments, Ic::Effects, Ic::Vst, Ic::Presets, Ic::Folder, Ic::User][i]
}

pub fn is_pinned(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<bool>(egui::Id::new("browser_pin"))).unwrap_or(false)
}

/// Pin and Close now live in the search row (see `search_row`), so the panel has no header of its own.
pub fn header(_ui: &mut Ui, _out: &mut Out) {}

fn search_row(ui: &mut Ui, lib: &mut Library, project: &mut Project, out: &mut Out) {
    let pin = is_pinned(ui.ctx());
    let open_id = egui::Id::new("browser_search_open");
    let focus_id = egui::Id::new("browser_search_focus");
    let mut open = ui.ctx().data(|d| d.get_temp::<bool>(open_id)).unwrap_or(false) || !lib.search.is_empty();
    // one slim line: "Browser" on the left; on the right the magnifier (opens the search field), filters, pin and close
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if icon_button(ui, Ic::Close, vec2(26.0, 26.0), false, "Hide the Browser").clicked() {
                ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("browser_pin"), false));
                out.close = true;
            }
            if icon_button(ui, Ic::Pin, vec2(26.0, 26.0), pin, if pin { "Pinned: the panel stays open" } else { "Pin the panel so it stays open" }).clicked() {
                ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("browser_pin"), !pin));
            }
            // three dots: Import WAV / + Folder / Rescan
            let mresp = icon_button(ui, Ic::Dots, vec2(26.0, 26.0), false, "Import WAV, add a folder, rescan");
            egui::popup::popup_below_widget(ui, egui::Id::new("browser_more"), &mresp, egui::PopupCloseBehavior::CloseOnClick, |ui| {
                ui.set_min_width(170.0);
                lib.actions_menu(ui, project);
            });
            if mresp.clicked() {
                ui.memory_mut(|m| m.toggle_popup(egui::Id::new("browser_more")));
            }
            let active = lib.fav_only || !lib.auto_preview;
            let fresp = icon_button(ui, Ic::Filter, vec2(26.0, 26.0), active, "Filters");
            egui::popup::popup_below_widget(ui, egui::Id::new("browser_filters"), &fresp, egui::PopupCloseBehavior::CloseOnClickOutside, |ui| {
                ui.set_min_width(190.0);
                ui.checkbox(&mut lib.fav_only, "Favorites only");
                ui.checkbox(&mut lib.auto_preview, "Preview on click");
                ui.separator();
                if ui.button("Rescan folders").clicked() {
                    lib.rescan();
                }
            });
            if fresp.clicked() {
                ui.memory_mut(|m| m.toggle_popup(egui::Id::new("browser_filters")));
            }
            if icon_button(ui, Ic::Search, vec2(26.0, 26.0), open, "Search").clicked() {
                if open && lib.search.is_empty() {
                    open = false;
                } else if open {
                    lib.search.clear();
                    open = false;
                } else {
                    open = true;
                    ui.ctx().data_mut(|d| d.insert_temp(focus_id, true));
                }
            }
        });
    });
    if open {
        ui.add_space(2.0);
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::hover());
        let p = ui.painter();
        p.rect_filled(rect, 0.0, PANEL2());
        p.rect_stroke(rect, 0.0, Stroke::new(border_w(), TEAL()));
        icon(p, Ic::Search, rect.left_center() + vec2(12.0, 0.0), DIM());
        let mut te = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_max(rect.min + vec2(23.0, 1.0), rect.max - vec2(3.0, 1.0))));
        let r = te.add(egui::TextEdit::singleline(&mut lib.search).id(egui::Id::new("browser_search_field")).hint_text("Search...").frame(false).desired_width(f32::INFINITY).margin(vec2(2.0, 5.0)));
        if ui.ctx().data_mut(|d| d.remove_temp::<bool>(focus_id)).unwrap_or(false) {
            r.request_focus();
        } else if r.lost_focus() && lib.search.is_empty() {
            open = false;
        }
        if r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            lib.search.clear();
            open = false;
        }
    }
    ui.ctx().data_mut(|d| d.insert_temp(open_id, open));
}

/// one square tab of the category strip: only the icon, the name shows in the title line below
fn section_tab(ui: &mut Ui, i: usize, size: Vec2, selected: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let p = ui.painter();
    if selected {
        p.rect_filled(rect, 0.0, tint(PANEL2(), TEAL(), if is_dark() { 0.30 } else { 0.22 }));
        p.rect_filled(Rect::from_min_size(rect.left_bottom() - vec2(0.0, 2.5), vec2(rect.width(), 2.5)), 1.0, TEAL());
    } else if resp.hovered() {
        p.rect_filled(rect, 0.0, PANEL3());
    }
    icon(p, section_icon(i), rect.center() - vec2(0.0, 1.0), if selected { TEAL() } else if resp.hovered() { TEXT() } else { DIM() });
    resp.on_hover_text(SECTIONS[i])
}

fn hint(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).size(11.0).color(DIM()));
}

fn instruments(ui: &mut Ui, c: &mut Ctx, q: &str) {
    let hit = |title: &str, sub: &str| q.is_empty() || title.to_lowercase().contains(q) || sub.to_lowercase().contains(q);
    hint(ui, "click: editor  -  double-click: piano roll  -  drag onto a track");
    if hit("Drums (MIDI)", "Kick, Snare, Clap, Hi-Hat, Perc") {
        let r = row(ui, "Drums (MIDI)", "Kick, Snare, Clap, Hi-Hat, Perc", lane_col(0), *c.live_track == Track::Drums);
        r.dnd_set_drag_payload(DragItem::Instrument(Track::Drums));
        if r.clicked() || r.double_clicked() {
            *c.roll_track = Track::Drums;
            *c.live_track = Track::Drums;
        }
        if r.double_clicked() {
            *c.tab = Tab::PianoRoll;
        }
    }
    for (k, inst) in [Inst::Wavetable, Inst::Sampler].into_iter().enumerate() {
        let (title, sub) = match inst {
            Inst::Wavetable => ("Vinteria Synth", "one synth, many presets"),
            _ => (inst.title(), inst.subtitle()),
        };
        if !hit(title, sub) {
            continue;
        }
        let r = row(ui, title, sub, LANE_COLORS[[7, 9][k]], *c.inst == inst);
        r.dnd_set_drag_payload(DragItem::Instrument(inst.track()));
        if r.clicked() {
            *c.tab = Tab::Instruments;
            *c.inst = inst;
            *c.live_track = inst.track();
        }
        if r.double_clicked() {
            *c.roll_track = inst.track();
            *c.live_track = inst.track();
            *c.tab = Tab::PianoRoll;
        }
    }
}

/// Effects: a click puts the effect into the first free slot of the insert selected in the Mixer.
fn effects(ui: &mut Ui, c: &mut Ctx, q: &str) {
    let sel = ui.data(|d| d.get_temp::<usize>(egui::Id::new("mix_fx_sel"))).unwrap_or(0).min(NUM_INSERTS);
    let target = if sel == NUM_INSERTS { "Master".to_string() } else { format!("Insert {}", sel + 1) };
    hint(ui, &format!("click: add to {target} (selected in the Mixer)"));
    c.project.insert_fx.resize_with(NUM_INSERTS, Vec::new);
    for (title, sig) in [("Essentials", false), ("Vinteria", true)] {
        let kinds: Vec<FxKind> = crate::fx::ALL.into_iter().filter(|k| k.is_signature() == sig && (q.is_empty() || k.name().to_lowercase().contains(q) || k.category().to_lowercase().contains(q))).collect();
        if kinds.is_empty() {
            continue;
        }
        ui.label(RichText::new(title).small().strong().color(DIM()));
        for kind in kinds {
            let r = row(ui, kind.name(), kind.category(), if sig { CYAN() } else { lane_col(3) }, false);
            if r.clicked() {
                if !super::plugins_ui::add_to_insert(c.project, sel, &FxSlot::new(kind)) {
                    c.lib.message = format!("{target} has no free slot");
                    continue;
                }
                c.lib.message = format!("{} added to {target}", kind.name());
            }
        }
    }
    if !c.lib.message.is_empty() {
        hint(ui, &c.lib.message.clone());
    }
}

fn scan_vst3() -> Vec<PathBuf> {
    fn walk(dir: &std::path::Path, depth: u32, out: &mut Vec<PathBuf>) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let p = e.path();
            let is_plugin = p.extension().map_or(false, |x| x.eq_ignore_ascii_case("vst3"));
            if is_plugin {
                out.push(p);
            } else if p.is_dir() && depth < 2 {
                walk(&p, depth + 1, out);
            }
        }
    }
    let mut v = vec![];
    for d in [r"C:\Program Files\Common Files\VST3", r"C:\Program Files (x86)\Common Files\VST3"] {
        walk(std::path::Path::new(d), 0, &mut v);
    }
    v.sort();
    v
}

fn vst3(ui: &mut Ui, q: &str) {
    let id = egui::Id::new("vst3_list");
    let list = ui.data(|d| d.get_temp::<Vec<PathBuf>>(id)).unwrap_or_else(|| {
        let v = scan_vst3();
        ui.data_mut(|d| d.insert_temp(id, v.clone()));
        v
    });
    ui.horizontal(|ui| {
        hint(ui, &format!("{} plugin(s) found", list.len()));
        if ui.small_button("Rescan").clicked() {
            ui.data_mut(|d| d.remove::<Vec<PathBuf>>(id));
        }
    });
    hint(ui, "Hosting VST3 plugins is not available in this build: they are listed only.");
    for p in list.iter().filter(|p| q.is_empty() || p.to_string_lossy().to_lowercase().contains(q)) {
        let name = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        row(ui, &name, "VST3", lane_col(8), false).on_hover_text(p.to_string_lossy());
    }
}

fn presets(ui: &mut Ui, c: &mut Ctx, q: &str) {
    hint(ui, "click: load on the selected MIDI track");
    for t in [Track::Wavetable, Track::Synth, Track::Bass808, Track::Analog, Track::Sampler] {
        let names: Vec<&'static str> = instruments_ui::presets_of(t).into_iter().filter(|n| q.is_empty() || n.to_lowercase().contains(q)).collect();
        if names.is_empty() {
            continue;
        }
        egui::CollapsingHeader::new(RichText::new(Inst::from_track(t).title()).strong()).id_salt(("preset_group", t as u8)).default_open(!q.is_empty()).show(ui, |ui| {
            for n in names {
                if row(ui, n, "", lane_col((t as usize)), false).clicked() {
                    let lane = c.sel_lane;
                    if c.project.arrangement.tracks.get(lane).map(|x| x.kind) == Some(crate::arrangement::TrackKind::Midi) {
                        c.project.arrangement.tracks[lane].instrument = Some(t);
                        c.project.arrangement.tracks[lane].blank = false;
                        c.project.adopt_track_params();
                        instruments_ui::apply_named_lane(c.project, lane, t, n);
                        *c.live_track = t;
                        *c.roll_track = t;
                        *c.inst = Inst::from_track(t);
                        c.lib.message = format!("{n} loaded on {}", c.project.arrangement.tracks[lane].name);
                    } else {
                        c.lib.message = "Select a MIDI track in the Playlist first".into();
                    }
                }
            }
        });
    }
    if !c.lib.message.is_empty() {
        hint(ui, &c.lib.message.clone());
    }
}

fn project_files(ui: &mut Ui, c: &mut Ctx, out: &mut Out, q: &str) {
    hint(ui, "click: open the project");
    if c.recent.is_empty() {
        hint(ui, "No projects yet - save one with Ctrl+S.");
    }
    for p in c.recent.iter().filter(|p| q.is_empty() || p.to_string_lossy().to_lowercase().contains(q)) {
        let name = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let dir = p.parent().map(|d| d.to_string_lossy().to_string()).unwrap_or_default();
        if row(ui, &name, &dir, lane_col(5), false).on_hover_text(p.to_string_lossy()).clicked() {
            out.open_path = Some(p.clone());
        }
    }
}

/// quick action of a section row (the mini button): rescan the folders / plugins
fn mini_tip(i: usize) -> Option<&'static str> {
    match i {
        // (the sound folders' Rescan sits in the row of Import WAV / + Folder)
        5 => Some("Rescan VST3 plugins"),
        _ => None,
    }
}

/// The browser: search, one row of icon tabs, the open section's title with its quick action,
/// the contents filling the rest of the panel and (for sounds) the Preview player pinned at the bottom.
/// A search shows matching results of every section.
pub fn body(ui: &mut Ui, c: &mut Ctx, out: &mut Out) {
    search_row(ui, c.lib, c.project, out);
    ui.add_space(1.0);
    let sec_id = egui::Id::new("browser_filter");
    let mut sec: u8 = ui.data_mut(|d| *d.get_temp_mut_or_insert_with::<u8>(sec_id, || SEC_SAMPLES)).min(8);
    let q = c.lib.search.trim().to_lowercase();
    let sounds = matches!(sec, 0 | 1 | 2 | 8);
    if sounds || !q.is_empty() {
        egui::TopBottomPanel::bottom("browser_preview_dock").resizable(false).frame(egui::Frame::none().inner_margin(egui::Margin { left: 0.0, right: 0.0, top: 4.0, bottom: 2.0 })).show_inside(ui, |ui| {
            ui.separator();
            preview(ui, c);
        });
    }
    if !q.is_empty() {
        egui::ScrollArea::vertical().auto_shrink([false, false]).id_salt("browser_scroll_q").show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.label(RichText::new("Results").strong());
            c.lib.show(ui, c.project, c.lane, c.shared);
            instruments(ui, c, &q);
            effects(ui, c, &q);
            presets(ui, c, &q);
        });
        return;
    }
    // two levels of text tabs: three pages on top, the sections of the open page below them
    const PAGES: [(&str, &[u8]); 3] = [("Sounds", &[2, 0, 1, 8]), ("Plugins", &[3, 4, 6, 5]), ("Projects", &[7])];
    let page = PAGES.iter().position(|(_, secs)| secs.contains(&sec)).unwrap_or(0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let w = ui.available_width() / PAGES.len() as f32;
        for (i, (name, secs)) in PAGES.iter().enumerate() {
            let (rect, resp) = ui.allocate_exact_size(vec2(w, 30.0), Sense::click());
            let p = ui.painter();
            if resp.hovered() && i != page {
                p.rect_filled(rect, 0.0, PANEL3());
            }
            let on = i == page;
            p.text(rect.center() - vec2(0.0, 1.0), Align2::CENTER_CENTER, *name, FontId::proportional(13.5), if on { TEXT() } else { DIM() });
            if on {
                p.rect_filled(Rect::from_min_size(rect.left_bottom() - vec2(0.0, 2.5), vec2(rect.width(), 2.5)), 1.0, TEAL());
            }
            if resp.clicked() && !on {
                sec = secs[0];
            }
        }
    });
    // Import WAV / + Folder / Rescan sit right under the page tabs, the section switch (Samples / Favorites ...) below them
    let page_secs = PAGES[page].1;
    if page_secs.len() > 1 {
        ui.add_space(6.0);
        // segmented control: equal cells in one row
        let total = ui.available_width();
        let (bar, _) = ui.allocate_exact_size(vec2(total, 26.0), Sense::hover());
        let cell = total / page_secs.len() as f32;
        ui.painter().rect_filled(bar, 0.0, PANEL2());
        ui.painter().rect_stroke(bar, 0.0, Stroke::new(border_w(), border()));
        for (k, &i) in page_secs.iter().enumerate() {
            let rect = Rect::from_min_size(bar.min + vec2(cell * k as f32, 0.0), vec2(cell, bar.height())).shrink(2.0);
            let resp = ui.interact(rect, ui.id().with(("browser_seg", i)), Sense::click());
            let on = sec == i;
            let p = ui.painter();
            if on {
                p.rect_filled(rect, 0.0, tint(PANEL2(), TEAL(), if is_dark() { 0.40 } else { 0.28 }));
            } else if resp.hovered() {
                p.rect_filled(rect, 0.0, PANEL3());
            }
            let name = if i == 8 { "User" } else { SECTIONS[i as usize] };
            p.text(rect.center(), Align2::CENTER_CENTER, name, FontId::proportional(11.5), if on { TEXT() } else { DIM() });
            if resp.clicked() {
                sec = i;
            }
        }
    }
    ui.add_space(4.0);
    if let Some(tip) = mini_tip(sec as usize) {
        ui.horizontal(|ui| {
            // (no section title here: the segmented control above already names it)
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("Rescan").on_hover_text(tip).clicked() {
                    match sec {
                        5 => ui.data_mut(|d| d.remove::<Vec<PathBuf>>(egui::Id::new("vst3_list"))),
                        _ => c.lib.rescan(),
                    }
                }
            });
        });
    }
    ui.add_space(6.0);
    // the content sits in its own rounded card, apart from the header with the tabs
    egui::Frame::none().fill(BG()).stroke(Stroke::new(border_w(), border())).rounding(0.0).inner_margin(egui::Margin::symmetric(8.0, 8.0)).show(ui, |ui| {
    ui.set_min_height(ui.available_height());
    egui::ScrollArea::vertical().auto_shrink([false, false]).id_salt(("browser_scroll", sec)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = 2.0;
        match sec {
            0 => {
                let files = c.lib.favorite_files();
                if files.is_empty() {
                    hint(ui, "Press the heart in Preview to add a sound here.");
                }
                c.lib.show_files(ui, &files, c.project, c.lane, c.shared);
            }
            1 => {
                let files = c.lib.recent.clone();
                if files.is_empty() {
                    hint(ui, "Sounds you use appear here.");
                }
                c.lib.show_files(ui, &files, c.project, c.lane, c.shared);
            }
            2 => c.lib.show_samples(ui, c.project, c.lane, c.shared),
            3 => instruments(ui, c, ""),
            4 => effects(ui, c, ""),
            5 => vst3(ui, ""),
            6 => presets(ui, c, ""),
            7 => project_files(ui, c, out, ""),
            _ => c.lib.show_user(ui, c.project, c.lane, c.shared),
        }
    });
    });
    ui.data_mut(|d| d.insert_temp(sec_id, sec));
}

/// Preview player: waveform, name, format, volume, play / stop and the quick actions.
pub fn preview(ui: &mut Ui, c: &mut Ctx) {
    // nothing selected: one thin line instead of the whole player
    if c.lib.selected.is_none() {
        egui::Frame::none().inner_margin(egui::Margin::symmetric(6.0, 5.0)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new("Preview  ·  click a sound to listen").size(11.5).color(DIM()));
        });
        return;
    }
    let open_id = egui::Id::new("browser_preview_open");
    let mut open = ui.data(|d| d.get_temp::<bool>(open_id)).unwrap_or(true);
    egui::Frame::none().inner_margin(egui::Margin::symmetric(4.0, 2.0)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        let (hr, hresp) = ui.allocate_exact_size(vec2(ui.available_width(), 18.0), Sense::click());
        ui.painter().text(hr.left_center(), Align2::LEFT_CENTER, "Preview", FontId::proportional(13.5), TEXT());
        let cv = hr.right_center() - vec2(10.0, 0.0);
        let chev = if open { vec![cv + vec2(-5.0, 2.5), cv + vec2(0.0, -2.5), cv + vec2(5.0, 2.5)] } else { vec![cv + vec2(-5.0, -2.5), cv + vec2(0.0, 2.5), cv + vec2(5.0, -2.5)] };
        ui.painter().add(Shape::line(chev, Stroke::new(1.8, DIM())));
        if hresp.clicked() {
            open = !open;
        }
        if !open {
            return;
        }
        ui.add_space(4.0);
        c.lib.preview_wave(ui, c.shared, 36.0);
        let sel = c.lib.selected.clone();
        let (name, info) = match &sel {
            Some(p) => (p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(), c.lib.format_info(p)),
            None => ("No sound selected".to_string(), String::new()),
        };
        ui.add_space(4.0);
        ui.add(egui::Label::new(RichText::new(name).size(12.5).strong()).truncate());
        ui.label(RichText::new(info).size(10.0).color(DIM()));
        ui.add_space(4.0);
        let progress = f32::from_bits(c.shared.preview_pos.load(std::sync::atomic::Ordering::Relaxed));
        let playing = progress <= 1.0;
        ui.horizontal(|ui| {
            ui.spacing_mut().slider_width = (ui.available_width() - 44.0).max(24.0);
            if ui.add(egui::Slider::new(&mut c.lib.preview_vol, 0.0..=1.0).show_value(false)).on_hover_text("Preview volume").changed() {
                c.shared.send(Cmd::PreviewGain(c.lib.preview_vol));
            }
            let (r, resp) = ui.allocate_exact_size(vec2(30.0, 26.0), Sense::click());
            let p = ui.painter();
            gloss(p, r, 0.0, if resp.hovered() { PANEL3() } else { PANEL2() });
            p.rect_stroke(r, 0.0, Stroke::new(1.5, TEAL()));
            icon(p, if playing { Ic::Stop } else { Ic::Play }, r.center(), TEXT());
            if resp.on_hover_text(if playing { "Stop" } else { "Play" }).clicked() {
                if playing {
                    c.shared.send(Cmd::PreviewStop);
                } else if let Some(d) = sel.as_ref().and_then(|f| c.lib.get(f).ok()) {
                    c.shared.send(Cmd::Preview(d));
                }
            }
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let b = vec2(24.0, 24.0);
            let tag_resp = icon_button(ui, Ic::Tag, b, sel.as_ref().map_or(false, |p| c.lib.tags.contains_key(p)), "Tag");
            egui::popup::popup_below_widget(ui, egui::Id::new("browser_tag"), &tag_resp, egui::PopupCloseBehavior::CloseOnClickOutside, |ui| {
                ui.set_min_width(170.0);
                if let Some(p) = sel.clone() {
                    let mut t = c.lib.tags.get(&p).cloned().unwrap_or_default();
                    ui.label("Tag (searchable)");
                    if ui.text_edit_singleline(&mut t).changed() {
                        c.lib.set_tag(&p, t);
                    }
                } else {
                    ui.label("Select a sound first");
                }
            });
            if tag_resp.clicked() {
                ui.memory_mut(|m| m.toggle_popup(egui::Id::new("browser_tag")));
            }
            let fav = sel.as_ref().map_or(false, |p| c.lib.is_fav(p));
            if icon_button(ui, Ic::Heart, b, fav, "Favorite").clicked() {
                if let Some(p) = &sel {
                    c.lib.toggle_fav(p);
                }
            }
            if icon_button(ui, Ic::Filter, b, c.lib.auto_preview, "Preview on click").clicked() {
                c.lib.auto_preview = !c.lib.auto_preview;
            }
            let more = icon_button(ui, Ic::Dots, b, false, "More");
            egui::popup::popup_below_widget(ui, egui::Id::new("browser_more"), &more, egui::PopupCloseBehavior::CloseOnClick, |ui| {
                ui.set_min_width(210.0);
                if let Some(p) = sel.clone() {
                    if ui.button("Send to the selected drum lane").clicked() {
                        c.lib.assign(c.project, *c.lane, &p);
                    }
                    if ui.button("Use as 808 Sampler sound").clicked() {
                        c.lib.assign_sampler(c.project, &p);
                    }
                    if ui.button("Send to the Playlist").clicked() {
                        c.lib.send_to_playlist_pub(c.project, &p, false);
                    }
                    if ui.button("Send to a new audio track").clicked() {
                        c.lib.send_to_playlist_pub(c.project, &p, true);
                    }
                    if ui.button("Locate file").clicked() {
                        let _ = std::process::Command::new("explorer").arg(format!("/select,{}", p.display())).spawn();
                    }
                } else {
                    ui.label("Select a sound first");
                }
                if ui.button("Open WAV...").clicked() {
                    if let Some(path) = rfd::FileDialog::new().add_filter("WAV audio", &["wav"]).pick_file() {
                        if c.lib.get(&path).is_ok() {
                            c.lib.selected = Some(path);
                        }
                    }
                }
            });
            if more.clicked() {
                ui.memory_mut(|m| m.toggle_popup(egui::Id::new("browser_more")));
            }
        });
    });
    ui.data_mut(|d| d.insert_temp(open_id, open));
}
