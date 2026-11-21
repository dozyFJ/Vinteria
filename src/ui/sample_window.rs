//! Sound settings of a drum channel, laid out like FL Studio's Sampler channel window: channel strip on top, file,
//! playback range, precomputed effects and the waveform. Only what the engine really does is offered.
use super::library::Library;
use super::theme::*;
use super::widgets::{led, lerp_color, lin_knob, mini_knob};
use crate::audio_engine::device::Shared;
use crate::audio_engine::Cmd;
use crate::project::{Project, SampleData};
use crate::sequencer::DRUM_NAMES;
use egui::{pos2, vec2, Rect, Sense, Stroke, Ui};
use std::sync::Arc;

fn group(ui: &mut Ui, title: &str, add: impl FnOnce(&mut Ui)) {
    egui::Frame::none().fill(PANEL2()).rounding(6.0).inner_margin(egui::Margin::symmetric(10.0, 8.0)).stroke(Stroke::new(border_w(), border())).show(ui, |ui| {
        ui.vertical(|ui| {
            ui.label(egui::RichText::new(title).strong().size(12.0).color(TEAL_HI()));
            add(ui);
        });
    });
}

fn wave(ui: &mut Ui, orig: &SampleData, fx: &crate::project::SampleFx) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 130.0), Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 4.0, INSET());
    let n = orig.mono.len().max(1);
    let w = rect.width().max(1.0) as usize;
    let mid = rect.center().y;
    let col = lerp_color(TEAL(), egui::Color32::WHITE, 0.6);
    for x in 0..w {
        let a0 = x * n / w;
        let a1 = (((x + 1) * n / w).max(a0 + 1)).min(n);
        let stride = ((a1 - a0) / 32).max(1);
        let (mut pk, mut i) = (0.0f32, a0);
        while i < a1 {
            pk = pk.max(orig.mono[i].abs());
            i += stride;
        }
        let h = pk.min(1.0) * (rect.height() * 0.5 - 4.0);
        let fx_pos = x as f32 / w as f32;
        let inside = fx_pos >= fx.start && fx_pos <= fx.end;
        p.line_segment([pos2(rect.left() + x as f32, mid - h), pos2(rect.left() + x as f32, mid + h)], Stroke::new(1.0, if inside { col } else { col.linear_multiply(0.25) }));
    }
    for v in [fx.start, fx.end] {
        let x = rect.left() + v * rect.width();
        p.line_segment([pos2(x, rect.top()), pos2(x, rect.bottom())], Stroke::new(1.5, CYAN()));
    }
    p.text(rect.right_top() + vec2(-8.0, 6.0), egui::Align2::RIGHT_TOP, "Sampler", egui::FontId::proportional(20.0), LIGHT().linear_multiply(0.6));
}

/// Returns nothing; closes itself through `open`.
pub fn show(ctx: &egui::Context, project: &mut Project, lib: &mut Library, shared: &Shared, lane: usize, open: &mut bool) {
    if lane >= project.drum_samples.len() {
        *open = false;
        return;
    }
    project.drum_fx.resize(project.drum_samples.len(), Default::default());
    let sname = project.drum_samples[lane].as_ref().map(|s| s.name.clone());
    let title = format!("{} ({}) - Sampler", DRUM_NAMES[lane], sname.clone().unwrap_or_else(|| "built-in sound".into()));
    egui::Window::new(title).id(egui::Id::new("sample_window")).open(open).default_width(560.0).resizable(true).show(ctx, |ui| {
        // channel strip: on, pan, volume, mixer output
        ui.horizontal(|ui| {
            let ch = &mut project.channels[lane];
            let mut on = !ch.mute;
            if led(ui, &mut on) {
                ch.mute = !on;
            }
            ui.label(egui::RichText::new("ON").size(10.0).color(DIM()));
            mini_knob(ui, &mut ch.pan, -1.0, 1.0, 0.0, "Pan");
            ui.label(egui::RichText::new("PAN").size(10.0).color(DIM()));
            mini_knob(ui, &mut ch.volume, 0.0, 1.5, 0.8, "Volume");
            ui.label(egui::RichText::new("VOL").size(10.0).color(DIM()));
            ui.label(egui::RichText::new("TRACK").size(10.0).color(DIM()));
            super::mixer_ui::output_picker(ui, ("smp_win_out", lane), &mut ch.output);
            if ui.button("Play").on_hover_text("Hear the sound").clicked() {
                shared.send(Cmd::DrumHit { lane, vel: 110 });
            }
        });
        ui.add_space(6.0);
        group(ui, "File", |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(sname.clone().unwrap_or_else(|| "built-in sound".into())).strong());
                if ui.button("Load...").clicked() {
                    lib.import_dialog(project, lane);
                }
                if ui.add_enabled(sname.is_some(), egui::Button::new("x")).on_hover_text("Back to the built-in sound").clicked() {
                    project.drum_samples[lane] = None;
                    project.drum_fx[lane] = Default::default();
                }
            });
        });
        ui.add_space(6.0);
        let before = project.drum_fx[lane];
        let mut fx = before;
        ui.horizontal_top(|ui| {
            group(ui, "Playback", |ui| {
                ui.horizontal(|ui| {
                    lin_knob(ui, "START", &mut fx.start, 0.0, 0.98, "");
                    lin_knob(ui, "END", &mut fx.end, 0.02, 1.0, "");
                });
            });
            group(ui, "Precomputed effects", |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.checkbox(&mut fx.remove_dc, "Remove DC offset");
                        ui.checkbox(&mut fx.normalize, "Normalize");
                        ui.checkbox(&mut fx.reverse, "Reverse");
                    });
                    lin_knob(ui, "IN", &mut fx.fade_in, 0.0, 1.0, "");
                    lin_knob(ui, "OUT", &mut fx.fade_out, 0.0, 1.0, "");
                });
            });
        });
        if fx.start >= fx.end - 0.01 {
            fx.start = (fx.end - 0.02).max(0.0);
        }
        ui.add_space(6.0);
        let orig: Option<Arc<SampleData>> = project.drum_samples[lane].as_ref().and_then(|s| lib.get(std::path::Path::new(&s.path)).ok());
        match &orig {
            Some(o) => wave(ui, o, &fx),
            None => {
                ui.label(egui::RichText::new("Built-in synthesized sound - load a WAV to edit it here").color(DIM()));
            }
        }
        if fx != before {
            project.drum_fx[lane] = fx;
            if let (Some(o), Some(r)) = (orig, project.drum_samples[lane].as_mut()) {
                r.data = Some(Arc::new(fx.apply(&o)));
            }
        }
        let _ = Rect::NOTHING;
    });
}
