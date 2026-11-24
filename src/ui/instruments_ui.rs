//! Instrument editors: Poly Synth, 808 Bass, Wavetable (Serum-style), Analog (Diva-style).
use super::theme::*;
use super::widgets::*;
use crate::instruments::wavetable::{lookup, tables};
use crate::instruments::*;
use crate::piano_roll::Track;
use crate::project::Project;
use egui::{pos2, vec2, Rect, Sense, Stroke, Ui};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Inst {
    Poly,
    Bass,
    Wavetable,
    Analog,
    Sampler,
}

impl Inst {
    pub fn title(self) -> &'static str {
        match self {
            Inst::Poly => "Poly Synth",
            Inst::Bass => "808 Bass",
            Inst::Wavetable => "Winteria Synth",
            Inst::Analog => "Analog",
            Inst::Sampler => "808 Sampler",
        }
    }
    pub fn subtitle(self) -> &'static str {
        match self {
            Inst::Poly => "subtractive, 16 voices",
            Inst::Bass => "mono sub bass, glide",
            Inst::Wavetable => "Serum-style wavetable synth",
            Inst::Analog => "Diva-style analog synth",
            Inst::Sampler => "plays an 808 sample from the pack on every key",
        }
    }
    pub fn from_track(t: Track) -> Inst {
        match t {
            Track::Synth => Inst::Poly,
            Track::Bass808 => Inst::Bass,
            Track::Wavetable => Inst::Wavetable,
            Track::Analog => Inst::Analog,
            Track::Drums => Inst::Poly,
            Track::Sampler => Inst::Sampler,
        }
    }
    pub fn track(self) -> Track {
        match self {
            Inst::Poly => Track::Synth,
            Inst::Bass => Track::Bass808,
            Inst::Wavetable => Track::Wavetable,
            Inst::Analog => Track::Analog,
            Inst::Sampler => Track::Sampler,
        }
    }
}

/// Phone layout flag, set by `show_mobile` while the engine panels draw.
fn is_mobile(ui: &Ui) -> bool {
    ui.data(|d| d.get_temp::<bool>(egui::Id::new("inst_mobile_layout"))).unwrap_or(false)
}

/// A row of controls: wraps onto the next line on a phone.
fn row<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> egui::InnerResponse<R> {
    if is_mobile(ui) { ui.horizontal_wrapped(add) } else { ui.horizontal(add) }
}

/// A row of boxes aligned at the top; on a phone the boxes stack one under another.
fn row_top<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> egui::InnerResponse<R> {
    if is_mobile(ui) {
        // boxes stack: frames take the rest of a row, so they would squeeze instead of wrapping
        ui.vertical(add)
    } else {
        ui.horizontal_top(add)
    }
}

/// A fixed panel width, no wider than the screen on a phone.
fn fit_w(ui: &Ui, w: f32) -> f32 {
    if is_mobile(ui) { w.min(ui.available_width().max(120.0)) } else { w }
}

/// The instrument editor on a phone: "Sounds" (the preset library) and "Edit" (the engine), one at a time.
pub fn show_mobile(ui: &mut Ui, project: &mut Project, inst: &mut Inst, lane: usize) -> Option<Track> {
    let tab_id = egui::Id::new("inst_mobile_tab");
    let mut tab = ui.data(|d| d.get_temp::<u8>(tab_id)).unwrap_or(1);
    ui.horizontal(|ui| {
        let w = (ui.available_width() - 4.0) * 0.5;
        for (i, label) in [(0u8, "Sounds"), (1u8, "Edit")] {
            if ui.add_sized([w, 40.0], egui::SelectableLabel::new(tab == i, egui::RichText::new(label).size(15.0))).clicked() {
                tab = i;
            }
        }
    });
    ui.data_mut(|d| d.insert_temp(tab_id, tab));
    ui.add_space(6.0);
    swap_lane_params(project, lane);
    let mut picked = None;
    if tab == 0 {
        let cat_id = egui::Id::new("preset_cat");
        let cur_id = egui::Id::new("preset_current");
        let mut cat: usize = ui.data(|d| d.get_temp::<usize>(cat_id)).unwrap_or(0);
        let current: String = ui.data(|d| d.get_temp::<String>(cur_id)).unwrap_or_default();
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
            for (i, c) in CATS.iter().enumerate() {
                if ui.add(egui::SelectableLabel::new(cat == i, egui::RichText::new(*c).size(13.0))).clicked() {
                    cat = i;
                }
            }
        });
        ui.add_space(6.0);
        egui::ScrollArea::vertical().id_salt("preset_list_mobile").auto_shrink([false, false]).show(ui, |ui| {
            for p in PRESETS.iter().filter(|p| cat == 0 || p.cat == CATS[cat]) {
                let on = current == p.name;
                let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 44.0), Sense::click());
                let pt = ui.painter();
                pt.rect_filled(rect.shrink2(vec2(0.0, 2.0)), 8.0, if on { TEAL().linear_multiply(0.35) } else { PANEL3() });
                if on { pt.rect_stroke(rect.shrink2(vec2(0.0, 2.0)), 8.0, Stroke::new(1.0, TEAL())); }
                pt.text(rect.left_center() + vec2(12.0, 0.0), egui::Align2::LEFT_CENTER, p.name, egui::FontId::proportional(15.0), TEXT());
                pt.text(rect.right_center() - vec2(12.0, 0.0), egui::Align2::RIGHT_CENTER, format!("{} · {}", p.cat, p.engine.name()), egui::FontId::proportional(11.0), DIM());
                if resp.clicked() {
                    apply_preset(project, p);
                    *inst = Inst::from_track(p.engine);
                    picked = Some(p.engine);
                    ui.data_mut(|d| d.insert_temp(cur_id, p.name.to_string()));
                }
            }
        });
        ui.data_mut(|d| d.insert_temp(cat_id, cat));
    } else {
        ui.data_mut(|d| d.insert_temp(egui::Id::new("inst_mobile_layout"), true));
        egui::ScrollArea::vertical().id_salt("inst_scroll_mobile").auto_shrink([false, false]).show(ui, |ui| {
            ui.set_width(ui.available_width());
            match *inst {
                Inst::Poly => synth_panel(ui, project),
                Inst::Bass => bass_panel(ui, project),
                Inst::Wavetable => wavetable_panel(ui, project),
                Inst::Analog => analog_panel(ui, project),
                Inst::Sampler => sampler_panel(ui, project, lane),
            }
        });
        ui.data_mut(|d| d.remove::<bool>(egui::Id::new("inst_mobile_layout")));
    }
    swap_lane_params(project, lane);
    picked
}

fn frame() -> egui::Frame {
    egui::Frame::none().fill(PANEL2()).rounding(card()).inner_margin(14.0).stroke(Stroke::new(border_w(), border()))
}

/// One entry of the preset library: category, name, which engine plays it and the engine's own preset key.
struct Preset {
    cat: &'static str,
    name: &'static str,
    engine: Track,
    key: &'static str,
}

const CATS: [&str; 13] = ["All", "Bass", "Lead", "Pad", "Keys", "Guitar", "Wind", "Hardbass", "Funk", "House", "Pluck", "Strings", "Bell"];

const PRESETS: &[Preset] = &[
    Preset { cat: "Guitar", name: "Acoustic Guitar", engine: Track::Wavetable, key: "Acoustic Guitar" },
    Preset { cat: "Guitar", name: "Nylon Guitar", engine: Track::Wavetable, key: "Nylon Guitar" },
    Preset { cat: "Guitar", name: "Clean Electric", engine: Track::Wavetable, key: "Clean Electric" },
    Preset { cat: "Guitar", name: "Overdrive Guitar", engine: Track::Wavetable, key: "Overdrive Guitar" },
    Preset { cat: "Guitar", name: "Muted Guitar", engine: Track::Wavetable, key: "Muted Guitar" },
    Preset { cat: "Guitar", name: "12 String", engine: Track::Wavetable, key: "12 String" },
    Preset { cat: "Hardbass", name: "Donk", engine: Track::Wavetable, key: "Donk" },
    Preset { cat: "Hardbass", name: "Hardbass Donk", engine: Track::Wavetable, key: "Hardbass Donk" },
    Preset { cat: "Hardbass", name: "Hard Sub", engine: Track::Wavetable, key: "Hard Sub" },
    Preset { cat: "Hardbass", name: "Reese Bass", engine: Track::Wavetable, key: "Reese Bass" },
    Preset { cat: "Hardbass", name: "Screech Lead", engine: Track::Wavetable, key: "Screech Lead" },
    Preset { cat: "Funk", name: "Donk Funk", engine: Track::Wavetable, key: "Donk Funk" },
    Preset { cat: "Funk", name: "Funk Bass", engine: Track::Wavetable, key: "Funk Bass" },
    Preset { cat: "Funk", name: "Funk Stab", engine: Track::Wavetable, key: "Funk Stab" },
    Preset { cat: "House", name: "Drift House Stab", engine: Track::Wavetable, key: "Drift House Stab" },
    Preset { cat: "House", name: "Deep House Chord", engine: Track::Wavetable, key: "Deep House Chord" },
    Preset { cat: "Keys", name: "Drawbar Organ", engine: Track::Wavetable, key: "Drawbar Organ" },
    Preset { cat: "Keys", name: "Rock Organ", engine: Track::Wavetable, key: "Rock Organ" },
    Preset { cat: "Keys", name: "Electric Piano", engine: Track::Wavetable, key: "Electric Piano" },
    Preset { cat: "Wind", name: "Flute", engine: Track::Wavetable, key: "Flute" },
    Preset { cat: "Wind", name: "Pan Flute", engine: Track::Wavetable, key: "Pan Flute" },
    Preset { cat: "Wind", name: "Accordion", engine: Track::Wavetable, key: "Accordion" },
    Preset { cat: "Wind", name: "Clarinet", engine: Track::Wavetable, key: "Clarinet" },
    Preset { cat: "Wind", name: "Brass Lead", engine: Track::Wavetable, key: "Brass Lead" },
    Preset { cat: "Lead", name: "Warm Lead", engine: Track::Wavetable, key: "Warm Lead" },
    Preset { cat: "Lead", name: "Square Lead", engine: Track::Wavetable, key: "Square Lead" },
    Preset { cat: "Lead", name: "Epic Lead", engine: Track::Wavetable, key: "Epic Lead" },
    Preset { cat: "Pad", name: "Choir Pad", engine: Track::Wavetable, key: "Choir Pad" },
    Preset { cat: "Strings", name: "String Ensemble", engine: Track::Wavetable, key: "String Ensemble" },
    Preset { cat: "Bass", name: "Growl Bass", engine: Track::Wavetable, key: "Growl Bass" },
    Preset { cat: "Bass", name: "Moog Bass", engine: Track::Analog, key: "Moog Bass" },
    Preset { cat: "Bass", name: "808 Sub", engine: Track::Bass808, key: "808 Sub" },
    Preset { cat: "Bass", name: "808 Dirty", engine: Track::Bass808, key: "808 Dirty" },
    Preset { cat: "Lead", name: "Saw Lead", engine: Track::Synth, key: "Lead" },
    Preset { cat: "Lead", name: "Sync Lead", engine: Track::Analog, key: "Sync Lead" },
    Preset { cat: "Lead", name: "Super Saw", engine: Track::Wavetable, key: "Super Saw" },
    Preset { cat: "Pad", name: "Soft Pad", engine: Track::Synth, key: "Pad" },
    Preset { cat: "Pad", name: "Vowel Pad", engine: Track::Wavetable, key: "Vowel Pad" },
    Preset { cat: "Keys", name: "Soft Keys", engine: Track::Analog, key: "Soft Keys" },
    Preset { cat: "Keys", name: "Init Keys", engine: Track::Synth, key: "Init" },
    Preset { cat: "Pluck", name: "Pluck", engine: Track::Synth, key: "Pluck" },
    Preset { cat: "Pluck", name: "Metal Pluck", engine: Track::Wavetable, key: "Metal Pluck" },
    Preset { cat: "Strings", name: "PWM Strings", engine: Track::Analog, key: "PWM Strings" },
    Preset { cat: "Bell", name: "Bell", engine: Track::Synth, key: "Bell" },
    Preset { cat: "Bell", name: "Init Wavetable", engine: Track::Wavetable, key: "Init" },
];

/// Names of the presets that belong to one instrument (for the Channel Rack menu).
pub fn presets_of(t: Track) -> Vec<&'static str> {
    PRESETS.iter().filter(|p| p.engine == t).map(|p| p.name).collect()
}

/// Put the synth settings of track `lane` where the panels edit them (the project fields), or back: calling it twice is a no-op.
/// This is how every track gets an independent plugin without the panels knowing about tracks.
pub fn swap_lane_params(project: &mut Project, lane: usize) {
    let Project { synth, bass, wt, analog, arrangement, .. } = project;
    if let Some(t) = arrangement.tracks.get_mut(lane).filter(|t| t.snd.own) {
        std::mem::swap(synth, &mut t.snd.synth);
        std::mem::swap(bass, &mut t.snd.bass);
        std::mem::swap(wt, &mut t.snd.wt);
        std::mem::swap(analog, &mut t.snd.analog);
    }
}

/// Load preset `name` of instrument `t` onto the plugin of Playlist track `lane`.
pub fn apply_named_lane(project: &mut Project, lane: usize, t: Track, name: &str) {
    if let Some(p) = PRESETS.iter().find(|p| p.engine == t && p.name == name) {
        swap_lane_params(project, lane);
        apply_preset(project, p);
        swap_lane_params(project, lane);
    }
}

/// Load a preset of this instrument by name (onto the first track that plays this synth with settings of its own).
pub fn apply_named(project: &mut Project, t: Track, name: &str) {
    if let Some(p) = PRESETS.iter().find(|p| p.engine == t && p.name == name) {
        let lane = project.arrangement.tracks.iter().position(|x| x.instrument == Some(t) && x.snd.own);
        if let Some(l) = lane {
            swap_lane_params(project, l);
            apply_preset(project, p);
            swap_lane_params(project, l);
        } else {
            apply_preset(project, p);
        }
    }
}

fn apply_preset(project: &mut Project, p: &Preset) {
    match p.engine {
        Track::Synth => project.synth = SynthParams::preset(p.key),
        Track::Wavetable => project.wt = WavetableParams::preset(p.key),
        Track::Analog => project.analog = AnalogParams::preset(p.key),
        Track::Bass808 => {
            let dirty = p.key == "808 Dirty";
            project.bass = Bass808Params { drive: if dirty { 8.0 } else { 3.0 }, glide_ms: 40.0, decay: 1.2, sustain: 0.5, release: 0.2, pitch_drop: 8.0, tone: if dirty { 3500.0 } else { 2400.0 }, volume: 0.9, ..Bass808Params::default() };
        }
        _ => {}
    }
}

/// Shows the preset library on top of the engine editor. Returns the engine the user switched the track to, if any.
pub fn show(ui: &mut Ui, project: &mut Project, inst: &mut Inst, lane: usize) -> Option<Track> {
    swap_lane_params(project, lane);
    let picked = show_inner(ui, project, inst, lane);
    swap_lane_params(project, lane);
    picked
}

fn show_inner(ui: &mut Ui, project: &mut Project, inst: &mut Inst, lane: usize) -> Option<Track> {
    let mut picked = None;
    let cat_id = egui::Id::new("preset_cat");
    let cur_id = egui::Id::new("preset_current");
    let mut cat: usize = ui.data_mut(|d| *d.get_temp_mut_or_insert_with::<usize>(cat_id, || 0));
    let current: String = ui.data(|d| d.get_temp::<String>(cur_id)).unwrap_or_default();
    let mut chosen: Option<String> = None;
    // the sound library: a list of presets on the left, like the browser of any synth
    egui::SidePanel::left("preset_side").resizable(false).exact_width(188.0).frame(egui::Frame::none().fill(INSET()).stroke(Stroke::new(border_w(), border())).inner_margin(egui::Margin::symmetric(8.0, 8.0))).show_inside(ui, |ui| {
        section_label(ui, "SOUNDS");
        ui.add_space(2.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(3.0, 3.0);
            for (i, c) in CATS.iter().enumerate() {
                let on = cat == i;
                let b = egui::Button::new(egui::RichText::new(*c).size(10.5).color(if on { egui::Color32::WHITE } else { DIM() })).fill(if on { TEAL().linear_multiply(0.6) } else { PANEL3() }).rounding(2.0);
                if ui.add(b).clicked() {
                    cat = i;
                }
            }
        });
        ui.add_space(6.0);
        egui::ScrollArea::vertical().id_salt("preset_list").auto_shrink([false, false]).show(ui, |ui| {
            for p in PRESETS.iter().filter(|p| cat == 0 || p.cat == CATS[cat]) {
                let on = current == p.name;
                let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::click());
                let pt = ui.painter();
                if on {
                    pt.rect_filled(rect, corner(), TEAL().linear_multiply(0.35));
                    pt.rect_stroke(rect, corner(), Stroke::new(1.0, TEAL()));
                } else if resp.hovered() {
                    pt.rect_filled(rect, corner(), PANEL3());
                }
                pt.text(rect.left_center() + vec2(8.0, 0.0), egui::Align2::LEFT_CENTER, p.name, egui::FontId::proportional(12.5), TEXT());
                pt.text(rect.right_center() - vec2(8.0, 0.0), egui::Align2::RIGHT_CENTER, p.cat, egui::FontId::proportional(9.5), LIGHT());
                if resp.on_hover_text(format!("{} - engine: {}", p.cat, p.engine.name())).clicked() {
                    apply_preset(project, p);
                    *inst = Inst::from_track(p.engine);
                    picked = Some(p.engine);
                    chosen = Some(p.name.to_string());
                }
            }
        });
    });
    ui.data_mut(|d| d.insert_temp(cat_id, cat));
    if let Some(n) = chosen {
        ui.data_mut(|d| d.insert_temp(cur_id, n));
    }
    show_engine(ui, project, inst, lane);
    picked
}

fn show_engine(ui: &mut Ui, project: &mut Project, inst: &mut Inst, lane: usize) {
    ui.horizontal(|ui| {
        section_label(ui, "INSTRUMENT");
        ui.label(egui::RichText::new(inst.title()).strong());
    });
    ui.add_space(4.0);
    egui::ScrollArea::both().auto_shrink([false, false]).id_salt("inst_scroll").show(ui, |ui| match *inst {
        Inst::Poly => synth_panel(ui, project),
        Inst::Bass => bass_panel(ui, project),
        Inst::Wavetable => wavetable_panel(ui, project),
        Inst::Analog => analog_panel(ui, project),
        Inst::Sampler => sampler_panel(ui, project, lane),
    });
}

fn sampler_panel(ui: &mut Ui, project: &mut Project, lane: usize) {
    // a Sampler track has its own sound; otherwise the shared default sample is edited
    let own = project.arrangement.tracks.get(lane).map_or(false, |t| t.instrument == Some(Track::Sampler));
    let bpm = project.bpm;
    let cur = project.current_pattern.min(project.patterns.len().saturating_sub(1));
    let p = if own { &mut project.arrangement.tracks[lane].sampler } else { &mut project.sampler };
    let mut chop = None;
    frame().show(ui, |ui| {
        ui.set_width(fit_w(ui, 640.0));
        ui.vertical(|ui| {
            ui.label(egui::RichText::new("808 SAMPLER").color(TEAL()).strong().size(15.0));
            match &p.sample {
                Some(s) => ui.label(format!("Sample: {}", s.name)),
                None => ui.label("No sound yet: drop a WAV (a loop or a bounced track) from the Browser onto this track (or onto the Devices panel)."),
            };
            row(ui, |ui| {
                ui.label(if p.slice_mode { "First slice key" } else { "Root note" });
                ui.add(egui::DragValue::new(&mut p.root).range(0..=127));
                ui.label(crate::piano_roll::note_name(p.root));
            });
            ui.add(egui::Slider::new(&mut p.volume, 0.0..=1.5).text("Volume"));
            ui.add(egui::Slider::new(&mut p.release, 0.01..=1.5).text("Release (s)"));
            chop = super::slicer_ui::show(ui, p, bpm);
            if ui.button("Remove sample").clicked() {
                p.sample = None;
                p.slices = vec![0];
                p.slice_mode = false;
            }
        });
    });
    if let (Some(notes), Some(pat)) = (chop, project.patterns.get_mut(cur)) {
        super::slicer_ui::write_chop(pat, &notes);
    }
}

fn synth_panel(ui: &mut Ui, project: &mut Project) {
    let p = &mut project.synth;
    frame().show(ui, |ui| {
        ui.set_width(fit_w(ui, 460.0));
        ui.vertical(|ui| {
            row(ui, |ui| {
                ui.label(egui::RichText::new("POLY SYNTH").color(TEAL()).strong().size(15.0));
                ui.label(egui::RichText::new("16 voices").color(DIM()));
                for name in ["Init", "Pluck", "Pad", "Lead", "Bell"] {
                    if ui.small_button(name).clicked() {
                        *p = SynthParams::preset(name);
                    }
                }
            });
            section_label(ui, "OSCILLATOR");
            row(ui, |ui| {
                for (w, n) in [(Wave::Saw, "Saw"), (Wave::Square, "Square"), (Wave::Sine, "Sine"), (Wave::Triangle, "Triangle")] {
                    ui.selectable_value(&mut p.wave, w, n);
                }
            });
            row(ui, |ui| {
                lin_knob(ui, "Semitone", &mut p.semitones, -24.0, 24.0, "");
                lin_knob(ui, "Fine", &mut p.fine, -100.0, 100.0, "c");
                lin_knob(ui, "Osc2 detune", &mut p.osc2_detune, 0.0, 1200.0, "c");
                lin_knob(ui, "Osc2 mix", &mut p.osc2_mix, 0.0, 1.0, "");
                lin_knob(ui, "Volume", &mut p.volume, 0.0, 1.5, "");
            });
            section_label(ui, "AMP ENVELOPE (ADSR)");
            row(ui, |ui| {
                time_knob(ui, "Attack", &mut p.attack, 0.001, 4.0);
                time_knob(ui, "Decay", &mut p.decay, 0.005, 4.0);
                lin_knob(ui, "Sustain", &mut p.sustain, 0.0, 1.0, "");
                time_knob(ui, "Release", &mut p.release, 0.005, 6.0);
            });
            section_label(ui, "FILTERS");
            row(ui, |ui| {
                log_knob(ui, "LP cutoff", &mut p.lp_cutoff, 40.0, 18000.0);
                lin_knob(ui, "Resonance", &mut p.lp_res, 0.0, 0.95, "");
                log_knob(ui, "HP cutoff", &mut p.hp_cutoff, 20.0, 5000.0);
            });
            section_label(ui, "LFO");
            row(ui, |ui| {
                lin_knob(ui, "Rate", &mut p.lfo_rate, 0.05, 20.0, "Hz");
                lin_knob(ui, "Depth", &mut p.lfo_depth, 0.0, 1.0, "");
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new("Target").size(10.0).color(DIM()));
                    for (t, n) in [(LfoTarget::Pitch, "Pitch"), (LfoTarget::Cutoff, "Cutoff"), (LfoTarget::Amp, "Amp")] {
                        ui.selectable_value(&mut p.lfo_target, t, n);
                    }
                });
            });
        });
    });
}

fn bass_panel(ui: &mut Ui, project: &mut Project) {
    let p = &mut project.bass;
    frame().show(ui, |ui| {
        ui.set_width(fit_w(ui, 400.0));
        ui.vertical(|ui| {
            row(ui, |ui| {
                ui.label(egui::RichText::new("808 BASS").color(TEAL()).strong().size(15.0));
                ui.label(egui::RichText::new("mono - glide - saturation").color(DIM()));
                if ui.small_button("Reset").clicked() {
                    *p = Default::default();
                }
            });
            section_label(ui, "ENVELOPE");
            row(ui, |ui| {
                time_knob(ui, "Attack", &mut p.attack, 0.001, 0.5);
                time_knob(ui, "Decay", &mut p.decay, 0.02, 4.0);
                lin_knob(ui, "Sustain", &mut p.sustain, 0.0, 1.0, "");
                time_knob(ui, "Release", &mut p.release, 0.01, 3.0);
            });
            section_label(ui, "PITCH");
            row(ui, |ui| {
                knob(ui, "Glide", &mut p.glide_ms, 1.0, 600.0, true, |x| format!("{x:.0}ms"));
                lin_knob(ui, "Pitch drop", &mut p.pitch_drop, 0.0, 36.0, "st");
                knob(ui, "Drop time", &mut p.pitch_time_ms, 5.0, 400.0, true, |x| format!("{x:.0}ms"));
            });
            section_label(ui, "TONE");
            row(ui, |ui| {
                lin_knob(ui, "Saturation", &mut p.drive, 1.0, 20.0, "x");
                log_knob(ui, "Tone LP", &mut p.tone, 120.0, 12000.0);
                lin_knob(ui, "Volume", &mut p.volume, 0.0, 1.5, "");
            });
            ui.label(egui::RichText::new("Slides: in the Piano Roll enable 'Slide' for the 808 track (or double-click a note).").color(DIM()).size(11.0));
        });
    });
}

/// Draw one wavetable frame (morph position `pos`) as a waveform.
fn wave_view(ui: &mut Ui, table: usize, pos: f32, size: egui::Vec2) {
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 0.0, INSET());
    p.line_segment([rect.left_center(), rect.right_center()], Stroke::new(1.0, MID()));
    let t = &tables()[table.min(tables().len() - 1)];
    let n = 160;
    let pts: Vec<_> = (0..=n)
        .map(|i| {
            let ph = i as f32 / n as f32;
            pos2(rect.left() + ph * rect.width(), rect.center().y - lookup(t, 0, pos, ph.min(0.9999)) * rect.height() * 0.42)
        })
        .collect();
    p.add(egui::Shape::line(pts, Stroke::new(1.6, TEAL_HI())));
}

/// The frequency response of the filter (a sketch: cutoff position and resonance peak).
fn filter_curve(ui: &mut Ui, kind: FilterType, cutoff: f32, res: f32, w: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(w, 46.0), Sense::hover());
    let pt = ui.painter_at(rect);
    pt.rect_filled(rect, 0.0, INSET());
    let x_of = |hz: f32| rect.left() + ((hz.max(20.0) / 20.0).ln() / (20000.0f32 / 20.0).ln()).clamp(0.0, 1.0) * rect.width();
    let cx = x_of(cutoff);
    let pts: Vec<_> = (0..=64)
        .map(|i| {
            let t = i as f32 / 64.0;
            let hz = 20.0 * (1000.0f32).powf(t);
            let r = hz / cutoff.max(20.0);
            let lp = 1.0 / (1.0 + r.powi(4)).sqrt();
            let hp = 1.0 / (1.0 + (1.0 / r).powi(4)).sqrt();
            let bump = 1.0 + res * 1.4 * (-((r.ln()) * 3.0).powi(2)).exp();
            let g = match kind {
                FilterType::Lp => lp * bump,
                FilterType::Hp => hp * bump,
                FilterType::Bp => (lp * hp * 2.0) * bump,
            };
            pos2(rect.left() + t * rect.width(), rect.bottom() - 3.0 - (g / 2.4).clamp(0.0, 1.0) * (rect.height() - 8.0))
        })
        .collect();
    pt.line_segment([pos2(cx, rect.top()), pos2(cx, rect.bottom())], Stroke::new(1.0, TEAL().linear_multiply(0.4)));
    pt.add(egui::Shape::line(pts, Stroke::new(1.6, TEAL_HI())));
}

/// The shape of an ADSR envelope (time on the x axis is compressed so long releases still fit).
fn env_shape(ui: &mut Ui, a: f32, d: f32, s: f32, r: f32, w: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(w, 36.0), Sense::hover());
    let pt = ui.painter_at(rect);
    pt.rect_filled(rect, 0.0, INSET());
    let t = |x: f32| x.max(0.001).sqrt();
    let total = t(a) + t(d) + 0.6 + t(r);
    let px = |v: f32| rect.left() + 3.0 + v / total * (rect.width() - 6.0);
    let (y0, y1) = (rect.bottom() - 3.0, rect.top() + 4.0);
    let ys = y0 + (y1 - y0) * s.clamp(0.0, 1.0);
    let x1 = px(t(a));
    let x2 = px(t(a) + t(d));
    let x3 = px(t(a) + t(d) + 0.6);
    let x4 = px(total);
    let line = vec![pos2(px(0.0), y0), pos2(x1, y1), pos2(x2, ys), pos2(x3, ys), pos2(x4, y0)];
    let mut fill = line.clone();
    fill.push(pos2(px(0.0), y0));
    pt.add(egui::Shape::convex_polygon(vec![pos2(px(0.0), y0), pos2(x1, y1), pos2(x2, ys), pos2(x3, ys), pos2(x4, y0)], TEAL().linear_multiply(0.12), Stroke::NONE));
    pt.add(egui::Shape::line(line, Stroke::new(1.5, TEAL_HI())));
}

fn osc_block(ui: &mut Ui, title: &str, id: &str, table: &mut usize, pos: &mut f32, oct: &mut f32, level: &mut f32) {
    ui.vertical(|ui| {
        section_label(ui, title);
        let names: Vec<&str> = tables().iter().map(|t| t.name).collect();
        egui::ComboBox::from_id_salt(id).width(150.0).selected_text(names[(*table).min(names.len() - 1)]).show_ui(ui, |ui| {
            for (i, n) in names.iter().enumerate() {
                ui.selectable_value(table, i, *n);
            }
        });
        wave_view(ui, *table, *pos, vec2(200.0, 70.0));
        row(ui, |ui| {
            lin_knob(ui, "WT pos", pos, 0.0, 1.0, "");
            lin_knob(ui, "Pitch", oct, -24.0, 24.0, "st");
            lin_knob(ui, "Level", level, 0.0, 1.0, "");
        });
    });
}

/// A dark, titled group box of the Morphine-style layout.
fn mbox(ui: &mut Ui, title: &str, add: impl FnOnce(&mut Ui)) {
    egui::Frame::none().fill(INSET()).rounding(6.0).inner_margin(egui::Margin::symmetric(8.0, 6.0)).stroke(Stroke::new(border_w(), border())).show(ui, |ui| {
        ui.vertical(|ui| {
            ui.label(egui::RichText::new(title).size(10.0).strong().color(TEAL_HI()));
            add(ui);
        });
    });
}

/// The wavetable synth laid out like FL Studio's Morphine: program bar and master section on top, a column of page
/// buttons on the left (modulation, morph / mix, generator A, generator B), the display in the middle and the
/// filter / unison boxes on the right.
fn wavetable_panel(ui: &mut Ui, project: &mut Project) {
    let tab_id = egui::Id::new("wt_page");
    let mut tab: usize = ui.data(|d| d.get_temp::<usize>(tab_id)).unwrap_or(2);
    let name_id = egui::Id::new("wt_program");
    let mut prog: String = ui.data(|d| d.get_temp::<String>(name_id)).unwrap_or_else(|| "Init".to_string());
    let p = &mut project.wt;
    egui::Frame::none().fill(PANEL2()).rounding(card()).inner_margin(12.0).stroke(Stroke::new(border_w(), border())).show(ui, |ui| {
        // the synth fills the window: the middle grows, the right column keeps its width and sits flush right
        let mobile = is_mobile(ui);
        let total_w = if mobile { ui.available_width() } else { ui.available_width().max(640.0) };
        ui.set_width(fit_w(ui, total_w));
        // on a phone the display takes the full width and the side boxes wrap under it
        let center_w = if mobile { total_w - 16.0 } else { (total_w - 24.0 - 150.0 - 24.0 - 16.0).max(300.0) };
        ui.vertical(|ui| {
            // ---- top bar: program, logo, master envelope, master
            row_top(ui, |ui| {
                mbox(ui, "PROGRAM NAME", |ui| {
                    row(ui, |ui| {
                        ui.label(egui::RichText::new(prog.clone()).strong().size(13.0));
                        ui.menu_button("OPTIONS", |ui| {
                            for name in ["Init", "Growl Bass", "Vowel Pad", "Metal Pluck", "Super Saw"] {
                                if ui.button(name).clicked() {
                                    *p = WavetableParams::preset(name);
                                    prog = name.to_string();
                                    ui.close_menu();
                                }
                            }
                        });
                    });
                    row(ui, |ui| {
                        let mut u = p.unison as f32;
                        if knob(ui, "POLY", &mut u, 1.0, 7.0, false, |x| format!("{}", x.round() as i32)) {
                            p.unison = u.round().clamp(1.0, 7.0) as u8;
                        }
                        lin_knob(ui, "TRANSP", &mut p.oct_a, -24.0, 24.0, "st");
                    });
                });
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    // a fixed-width logo block: the name must not push the master boxes out of the panel
                    ui.set_width(fit_w(ui, 150.0));
                    ui.add_space(10.0);
                    ui.label(egui::RichText::new("Winteria DAW").size(24.0).strong().color(TEAL()));
                    ui.label(egui::RichText::new("WINTERIA WAVETABLE").size(10.0).color(DIM()));
                });
                ui.add_space(8.0);
                mbox(ui, "MASTER ENVELOPE", |ui| {
                    env_shape(ui, p.attack, p.decay, p.sustain, p.release, 280.0);
                    row(ui, |ui| {
                        time_knob(ui, "ATTACK", &mut p.attack, 0.001, 4.0);
                        time_knob(ui, "DECAY", &mut p.decay, 0.005, 4.0);
                        lin_knob(ui, "SUSTAIN", &mut p.sustain, 0.0, 1.0, "");
                        time_knob(ui, "RELEASE", &mut p.release, 0.005, 6.0);
                    });
                });
                mbox(ui, "MASTER", |ui| {
                    lin_knob(ui, "LEVEL", &mut p.volume, 0.0, 1.5, "");
                });
            });
            ui.add_space(6.0);
            // ---- page buttons | display | filter and unison
            row_top(ui, |ui| {
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    ui.set_width(fit_w(ui, center_w));
                    match tab {
                        2 | 3 => {
                            let (table, pos, oct, level, id, title) = if tab == 2 {
                                (&mut p.table_a, &mut p.pos_a, &mut p.oct_a, &mut p.level_a, "wt_a", "GENERATOR A")
                            } else {
                                (&mut p.table_b, &mut p.pos_b, &mut p.oct_b, &mut p.level_b, "wt_b", "GENERATOR B")
                            };
                            mbox(ui, title, |ui| {
                                let names: Vec<&str> = tables().iter().map(|t| t.name).collect();
                                egui::ComboBox::from_id_salt(id).width(180.0).selected_text(names[(*table).min(names.len() - 1)]).show_ui(ui, |ui| {
                                    for (i, n) in names.iter().enumerate() {
                                        ui.selectable_value(table, i, *n);
                                    }
                                });
                                wave_view(ui, *table, *pos, vec2(center_w - 20.0, 150.0));
                                row(ui, |ui| {
                                    lin_knob(ui, "WT POS", pos, 0.0, 1.0, "");
                                    lin_knob(ui, "PITCH", oct, -24.0, 24.0, "st");
                                    lin_knob(ui, "LEVEL", level, 0.0, 1.0, "");
                                });
                            });
                        }
                        1 => {
                            mbox(ui, "MORPH / MIX", |ui| {
                                row(ui, |ui| {
                                    ui.vertical(|ui| {
                                        ui.label(egui::RichText::new("A").size(10.0).color(DIM()));
                                        wave_view(ui, p.table_a, p.pos_a, vec2((center_w - 40.0) / 2.0, 90.0));
                                    });
                                    ui.vertical(|ui| {
                                        ui.label(egui::RichText::new("B").size(10.0).color(DIM()));
                                        wave_view(ui, p.table_b, p.pos_b, vec2((center_w - 40.0) / 2.0, 90.0));
                                    });
                                });
                                row(ui, |ui| {
                                    lin_knob(ui, "A LEVEL", &mut p.level_a, 0.0, 1.0, "");
                                    lin_knob(ui, "B LEVEL", &mut p.level_b, 0.0, 1.0, "");
                                    lin_knob(ui, "SUB", &mut p.sub, 0.0, 1.0, "");
                                    lin_knob(ui, "NOISE", &mut p.noise, 0.0, 1.0, "");
                                    lin_knob(ui, "ENV>POS", &mut p.pos_env, -1.0, 1.0, "");
                                });
                            });
                        }
                        _ => {
                            mbox(ui, "MODULATION ENVELOPE", |ui| {
                                row(ui, |ui| {
                                    time_knob(ui, "ATTACK", &mut p.f_attack, 0.001, 4.0);
                                    time_knob(ui, "DECAY", &mut p.f_decay, 0.005, 4.0);
                                    lin_knob(ui, "SUSTAIN", &mut p.f_sustain, 0.0, 1.0, "");
                                    time_knob(ui, "RELEASE", &mut p.f_release, 0.005, 6.0);
                                });
                            });
                            ui.add_space(4.0);
                            mbox(ui, "LFO", |ui| {
                                row(ui, |ui| {
                                    lin_knob(ui, "RATE", &mut p.lfo_rate, 0.05, 20.0, "Hz");
                                    lin_knob(ui, "DEPTH", &mut p.lfo_depth, 0.0, 1.0, "");
                                    ui.vertical(|ui| {
                                        for (t, n) in [(WtTarget::Position, "WT pos"), (WtTarget::Cutoff, "Cutoff"), (WtTarget::Pitch, "Pitch"), (WtTarget::Amp, "Amp")] {
                                            ui.selectable_value(&mut p.lfo_target, t, n);
                                        }
                                    });
                                });
                            });
                        }
                    }
                });
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    ui.set_width(fit_w(ui, 150.0));
                    mbox(ui, "UNISON", |ui| {
                        row(ui, |ui| {
                            let mut u = p.unison as f32;
                            if knob(ui, "VOICES", &mut u, 1.0, 7.0, false, |x| format!("{}", x.round() as i32)) {
                                p.unison = u.round().clamp(1.0, 7.0) as u8;
                            }
                            lin_knob(ui, "DETUNE", &mut p.uni_detune, 0.0, 60.0, "c");
                        });
                    });
                    ui.add_space(4.0);
                    mbox(ui, "FILTER", |ui| {
                        row(ui, |ui| {
                            for (t, n) in [(FilterType::Lp, "LP"), (FilterType::Hp, "HP"), (FilterType::Bp, "BP")] {
                                ui.selectable_value(&mut p.filter, t, n);
                            }
                        });
                        row(ui, |ui| {
                            log_knob(ui, "CUTOFF", &mut p.cutoff, 40.0, 18000.0);
                            lin_knob(ui, "RES", &mut p.res, 0.0, 0.95, "");
                        });
                        lin_knob(ui, "ENV AMT", &mut p.env_amt, -4.0, 4.0, "oct");
                        filter_curve(ui, p.filter, p.cutoff, p.res, 128.0);
                    });
                });
            });
            ui.add_space(6.0);
            // pages of the synth: a bar of buttons along the bottom
            row(ui, |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for (i, n) in ["MODULATION", "MORPH / MIX", "GENERATOR A", "GENERATOR B"].into_iter().enumerate() {
                    let on = tab == i;
                    let r = ui.add_sized([120.0, 26.0], egui::Button::new(egui::RichText::new(n).size(10.5).strong().color(if on { egui::Color32::WHITE } else { DIM() })).fill(if on { TEAL().linear_multiply(0.55) } else { PANEL3() }).stroke(Stroke::new(1.0, if on { TEAL() } else { MID() })));
                    if r.clicked() {
                        tab = i;
                    }
                }
            });
        });
    });
    ui.data_mut(|d| {
        d.insert_temp(tab_id, tab);
        d.insert_temp(name_id, prog);
    });
}

fn wave_pick(ui: &mut Ui, title: &str, w: &mut AnalogWave) {
    ui.vertical(|ui| {
        ui.label(egui::RichText::new(title).size(10.0).color(DIM()));
        for (v, n) in [(AnalogWave::Saw, "Saw"), (AnalogWave::Pulse, "Pulse"), (AnalogWave::Tri, "Tri"), (AnalogWave::Sine, "Sine")] {
            ui.selectable_value(w, v, n);
        }
    });
}

fn analog_panel(ui: &mut Ui, project: &mut Project) {
    let p = &mut project.analog;
    frame().show(ui, |ui| {
        ui.set_width(fit_w(ui, 760.0));
        ui.vertical(|ui| {
            row(ui, |ui| {
                ui.label(egui::RichText::new("ANALOG").color(TEAL()).strong().size(15.0));
                ui.label(egui::RichText::new("Diva-style: 2 VCOs with PWM / sync / drift, 24 dB ladder filter").color(DIM()));
            });
            row(ui, |ui| {
                ui.label("Presets:");
                for name in ["Init", "Moog Bass", "PWM Strings", "Sync Lead", "Soft Keys"] {
                    if ui.small_button(name).clicked() {
                        *p = AnalogParams::preset(name);
                    }
                }
            });
            section_label(ui, "OSCILLATORS");
            row_top(ui, |ui| {
                wave_pick(ui, "VCO 1", &mut p.wave1);
                lin_knob(ui, "Semi", &mut p.semi1, -24.0, 24.0, "");
                ui.add_space(10.0);
                wave_pick(ui, "VCO 2", &mut p.wave2);
                lin_knob(ui, "Semi", &mut p.semi2, -24.0, 24.0, "");
                lin_knob(ui, "Detune", &mut p.detune2, -50.0, 50.0, "c");
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new("Sync").size(10.0).color(DIM()));
                    ui.toggle_value(&mut p.sync, "VCO1 > 2").on_hover_text("Hard-sync VCO 2 to VCO 1");
                });
                ui.add_space(10.0);
                lin_knob(ui, "Mix", &mut p.mix, 0.0, 1.0, "");
                lin_knob(ui, "PW", &mut p.pw, 0.08, 0.92, "");
                lin_knob(ui, "Sub", &mut p.sub, 0.0, 1.0, "");
                lin_knob(ui, "Noise", &mut p.noise, 0.0, 1.0, "");
            });
            section_label(ui, "FILTER (LADDER 24 dB)");
            row(ui, |ui| {
                log_knob(ui, "Cutoff", &mut p.cutoff, 40.0, 16000.0);
                lin_knob(ui, "Res", &mut p.res, 0.0, 1.0, "");
                lin_knob(ui, "Drive", &mut p.drive, 1.0, 8.0, "x");
                lin_knob(ui, "Env amt", &mut p.env_amt, -4.0, 4.0, "oct");
                lin_knob(ui, "Key track", &mut p.keytrack, 0.0, 1.0, "");
            });
            row_top(ui, |ui| {
                ui.vertical(|ui| {
                    section_label(ui, "AMP ENV");
                    row(ui, |ui| {
                        time_knob(ui, "A", &mut p.attack, 0.001, 4.0);
                        time_knob(ui, "D", &mut p.decay, 0.005, 4.0);
                        lin_knob(ui, "S", &mut p.sustain, 0.0, 1.0, "");
                        time_knob(ui, "R", &mut p.release, 0.005, 6.0);
                    });
                });
                ui.add_space(14.0);
                ui.vertical(|ui| {
                    section_label(ui, "FILTER ENV");
                    row(ui, |ui| {
                        time_knob(ui, "A", &mut p.f_attack, 0.001, 4.0);
                        time_knob(ui, "D", &mut p.f_decay, 0.005, 4.0);
                        lin_knob(ui, "S", &mut p.f_sustain, 0.0, 1.0, "");
                        time_knob(ui, "R", &mut p.f_release, 0.005, 6.0);
                    });
                });
            });
            section_label(ui, "LFO / ANALOG");
            row_top(ui, |ui| {
                lin_knob(ui, "Rate", &mut p.lfo_rate, 0.05, 20.0, "Hz");
                lin_knob(ui, "Depth", &mut p.lfo_depth, 0.0, 1.0, "");
                ui.vertical(|ui| {
                    for (s, n) in [(LfoShape::Sine, "Sine"), (LfoShape::Tri, "Tri"), (LfoShape::Square, "Square")] {
                        ui.selectable_value(&mut p.lfo_shape, s, n);
                    }
                });
                ui.vertical(|ui| {
                    for (t, n) in [(AnTarget::Pitch, "Pitch"), (AnTarget::Pwm, "PWM"), (AnTarget::Cutoff, "Cutoff")] {
                        ui.selectable_value(&mut p.lfo_target, t, n);
                    }
                });
                ui.add_space(10.0);
                lin_knob(ui, "Drift", &mut p.drift, 0.0, 1.0, "");
                knob(ui, "Glide", &mut p.glide_ms, 0.0, 500.0, false, |x| format!("{x:.0}ms"));
                lin_knob(ui, "Volume", &mut p.volume, 0.0, 1.5, "");
            });
        });
    });
    let _ = Rect::NOTHING;
}

#[cfg(test)]
mod preset_tests {
    use super::*;

    #[test]
    fn a_preset_lands_on_the_chosen_track_only() {
        use crate::arrangement::TrackKind;
        let mut p = Project::default();
        for _ in 0..2 {
            p.arrangement.add_track(TrackKind::Midi);
        }
        p.arrangement.tracks[0].instrument = Some(Track::Wavetable);
        p.arrangement.tracks[1].instrument = Some(Track::Wavetable);
        p.adopt_track_params();
        let before1 = p.arrangement.tracks[1].snd.wt.clone();
        let name = presets_of(Track::Wavetable).into_iter().find(|n| *n == "Growl Bass").expect("preset exists");
        apply_named_lane(&mut p, 0, Track::Wavetable, name);
        assert!(p.arrangement.tracks[0].snd.wt != before1, "track 0 did not get the preset");
        assert!(p.arrangement.tracks[1].snd.wt == before1, "track 1 was changed too");
    }
}
