//! Browser sidebar (like FL Studio's): project patterns, instruments and samples.
//! Patterns and samples can be dragged onto the playlist (MIDI / audio tracks).
use super::instruments_ui::Inst;
use super::library::Library;
use super::theme::*;
use super::widgets::section_label;
use super::Tab;
use crate::arrangement::TrackKind;
use crate::audio_engine::device::Shared;
use crate::audio_engine::Cmd;
use crate::piano_roll::Track;
use crate::project::Project;
use crate::sequencer::DRUM_NAMES;
use crate::session::MAX_LANES;
use egui::{vec2, Align2, FontId, Sense, Ui};
use std::path::PathBuf;

/// What is being dragged out of the browser.
#[derive(Clone)]
pub enum DragItem {
    Pattern(usize),
    Sample(PathBuf),
    /// a synth: dropped on a MIDI track it becomes that track's instrument
    Instrument(Track),
}

impl DragItem {
    /// kind of track this item belongs on
    pub fn kind(&self) -> TrackKind {
        match self {
            DragItem::Pattern(_) => TrackKind::Midi,
            DragItem::Sample(_) => TrackKind::Audio,
            DragItem::Instrument(_) => TrackKind::Midi,
        }
    }

    pub fn label(&self, project: &Project) -> String {
        match self {
            DragItem::Pattern(i) => project.patterns.get(*i).map(|p| p.name.clone()).unwrap_or_default(),
            DragItem::Sample(p) => p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(),
            DragItem::Instrument(t) => t.name().to_string(),
        }
    }
}

/// Lane to drop an item on: the hovered track when its kind fits, otherwise a brand-new track
/// (named after the item) appended to the arrangement. None when the track limit is reached.
pub fn resolve_lane(project: &mut Project, lane: usize, want: TrackKind, name: &str) -> Option<usize> {
    if lane < project.arrangement.tracks.len() && project.arrangement.kind(lane) == want {
        return Some(lane);
    }
    if project.arrangement.tracks.len() >= MAX_LANES {
        return None;
    }
    project.arrangement.add_track(want);
    if !name.is_empty() {
        if let Some(t) = project.arrangement.tracks.last_mut() {
            t.name = name.to_string();
        }
    }
    Some(project.arrangement.tracks.len() - 1)
}

/// Load a WAV (cached) and register it in the project's audio pool; returns the pool index.
pub fn pool_for(project: &mut Project, lib: &mut Library, path: &std::path::Path) -> Option<usize> {
    match lib.get(path) {
        Ok(data) => {
            let name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            Some(project.pool_index(&name, &path.to_string_lossy(), data))
        }
        Err(e) => {
            lib.message = format!("Load failed: {e}");
            None
        }
    }
}

/// WAV files dropped from the OS file manager onto the window this frame.
pub fn dropped_wavs(ctx: &egui::Context) -> Vec<PathBuf> {
    ctx.input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).filter(|p| p.extension().map(|x| x.eq_ignore_ascii_case("wav")).unwrap_or(false)).collect())
}

pub struct Ctx<'a> {
    pub project: &'a mut Project,
    pub lib: &'a mut Library,
    pub shared: &'a Shared,
    pub lane: &'a mut usize,
    pub tab: &'a mut Tab,
    pub inst: &'a mut Inst,
    pub live_track: &'a mut Track,
    pub roll_track: &'a mut Track,
}

/// `text` cut with an ellipsis so that it fits `max_w` in the given font.
fn fit(ui: &Ui, text: &str, font: FontId, max_w: f32) -> String {
    let width = |t: &str| ui.painter().layout_no_wrap(t.to_string(), font.clone(), egui::Color32::WHITE).size().x;
    if width(text) <= max_w {
        return text.to_string();
    }
    let mut out: String = text.to_string();
    while !out.is_empty() && width(&format!("{out}...")) > max_w {
        out.pop();
    }
    format!("{}...", out.trim_end())
}

pub(super) fn row(ui: &mut Ui, label: &str, sub: &str, chip: egui::Color32, selected: bool) -> egui::Response {
    let h = if sub.is_empty() { 26.0 } else { 40.0 };
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::click_and_drag());
    let hov_t = super::widgets::fade(ui, resp.id.with("h"), resp.hovered(), 0.12);
    let sel_t = super::widgets::fade(ui, resp.id.with("s"), selected, 0.16);
    let p = ui.painter();
    p.rect_filled(rect, corner(), PANEL3().linear_multiply(hov_t * (1.0 - sel_t)));
    p.rect_filled(rect, corner(), TEAL().gamma_multiply(sel_t * if is_dark() { 0.18 } else { 0.12 }));
    p.rect_stroke(rect, corner(), egui::Stroke::new(border_w(), TEAL().linear_multiply(sel_t)));
    // small rounded colour chip
    p.rect_filled(egui::Rect::from_min_size(rect.min + vec2(6.0, 5.0), vec2(6.0, rect.height() - 10.0)), 3.0, chip);
    let max_w = rect.width() - 30.0;
    let l = fit(ui, label, FontId::proportional(13.5), max_w);
    let p = ui.painter();
    if sub.is_empty() {
        p.text(rect.left_center() + vec2(22.0, 0.0), Align2::LEFT_CENTER, l, FontId::proportional(13.5), TEXT());
    } else {
        let sb = fit(ui, sub, FontId::proportional(10.5), max_w);
        let p = ui.painter();
        p.text(rect.left_center() + vec2(22.0, -8.0), Align2::LEFT_CENTER, l, FontId::proportional(13.5), TEXT());
        p.text(rect.left_center() + vec2(22.0, 9.0), Align2::LEFT_CENTER, sb, FontId::proportional(10.5), DIM());
    }
    resp
}

pub fn show(ui: &mut Ui, c: &mut Ctx) {
    let tab_id = egui::Id::new("browser_filter");
    let tab: u8 = ui.data_mut(|d| *d.get_temp_mut_or_insert_with::<u8>(tab_id, || 0));
    let (show_proj, show_inst, show_snd) = (tab <= 1, tab == 0 || tab == 2, tab == 0 || tab == 3);
    ui.add(egui::TextEdit::singleline(&mut c.lib.search).hint_text("Search sounds / plugins").margin(vec2(8.0, 5.0)).desired_width(f32::INFINITY));
    ui.add_space(6.0);
    let q = c.lib.search.trim().to_lowercase();
    // a plugin row shows when the search is empty or its name / description contains the text
    let hit = |title: &str, sub: &str| q.is_empty() || title.to_lowercase().contains(&q) || sub.to_lowercase().contains(&q);
    egui::ScrollArea::vertical().auto_shrink([false, false]).id_salt("browser_scroll").show(ui, |ui| {
        ui.set_width(ui.available_width());
        if show_proj {
        egui::CollapsingHeader::new(egui::RichText::new("Current project").strong().color(TEXT())).default_open(true).show(ui, |ui| {
            if !c.project.audio_pool.is_empty() {
                ui.label(egui::RichText::new("Audio in project").color(DIM()).size(10.5));
                for s in &c.project.audio_pool {
                    ui.label(egui::RichText::new(format!("  {}", s.name)).size(12.0));
                }
            }
        });
        }
        if show_inst {
        egui::CollapsingHeader::new(egui::RichText::new("Plugins").strong().color(TEXT())).default_open(true).show(ui, |ui| {
            ui.label(egui::RichText::new("click: editor  -  double-click: piano roll  -  drag onto a track").color(DIM()).size(10.5));
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
        });
        }
        if show_snd {
        egui::CollapsingHeader::new(egui::RichText::new("Samples").strong().color(TEXT())).default_open(true).show(ui, |ui| {
            c.lib.show(ui, c.project, c.lane, c.shared);
        });
        }
    });
    let _ = section_label;
}
