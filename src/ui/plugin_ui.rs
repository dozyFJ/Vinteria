//! Plugin Grid Template: one adaptive layout for every built-in plugin (Dark / Light):
//! header (power, name, category, preset browser) · graph / indicator strip · wrapped grid of knobs · dry/wet row.
use super::{theme::*, widgets::knob};
use crate::fx::{FxKind, FxSlot, ParamSpec};
use crate::fx_ext::HALLA_MODES;
use egui::{pos2, vec2, Align2, FontId, Pos2, RichText, Sense, Shape, Stroke, Ui};

fn is_toggle(kind: FxKind, i: usize, spec: &ParamSpec) -> bool {
    spec.1 == 0.0 && spec.2 == 1.0 && matches!((kind, spec.0), (_, "Freeze") | (_, "Invert Phase") | (_, "Mono"))
        || (kind == FxKind::ViHalla && i == 7)
}

fn fmt_value(kind: FxKind, i: usize, spec: &ParamSpec, v: f32) -> String {
    if kind == FxKind::ViHalla && (i == 0 || i == 9) {
        return HALLA_MODES[(v.round() as usize).min(11)].0.to_string();
    }
    if spec.0.starts_with("Type") {
        return ["Tape", "Tube", "Fold"][(v.round() as usize).min(2)].to_string();
    }
    let unit = if spec.0.ends_with("Hz") {
        " Hz"
    } else if spec.0.ends_with("dB") {
        " dB"
    } else if spec.0.ends_with("ms") {
        " ms"
    } else {
        ""
    };
    if v.abs() >= 1000.0 && unit == " Hz" {
        format!("{:.1}k", v / 1000.0)
    } else if v.abs() >= 100.0 {
        format!("{v:.0}{unit}")
    } else if v.abs() >= 10.0 {
        format!("{v:.1}{unit}")
    } else {
        format!("{v:.2}{unit}")
    }
}

/// short label without the unit suffix (knob labels are tiny)
fn short(label: &str) -> String {
    let l = label.trim_end_matches(" Hz").trim_end_matches(" dB").trim_end_matches(" ms");
    let l = l.split(' ').next().filter(|_| l.len() > 9).unwrap_or(l);
    l.to_string()
}

fn graph_frame(ui: &mut Ui, h: f32) -> (egui::Rect, egui::Painter) {
    let w = ui.available_width().max(120.0);
    let (rect, _) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
    let p = ui.painter().clone();
    p.rect_filled(rect, card(), INSET());
    p.rect_stroke(rect, card(), Stroke::new(border_w(), border()));
    for i in 1..4 {
        let y = rect.top() + rect.height() * i as f32 / 4.0;
        p.line_segment([pos2(rect.left() + 4.0, y), pos2(rect.right() - 4.0, y)], Stroke::new(1.0, MID().linear_multiply(0.25)));
    }
    (rect, p)
}

fn curve(p: &egui::Painter, pts: Vec<Pos2>, col: egui::Color32) {
    p.add(Shape::line(pts, Stroke::new(2.0, col)));
}

fn biquad_peak_db(f: f32, f0: f32, gain: f32, q: f32) -> f32 {
    // smooth bell approximation in log-frequency (display only)
    let x = (f / f0).log2() * q.max(0.2) * 1.4;
    gain * (-x * x).exp()
}

/// Graph strip that shows what the plugin is doing from its current settings.
fn graph(ui: &mut Ui, slot: &FxSlot) {
    let h = 78.0;
    let (r, p) = graph_frame(ui, h);
    let inner = r.shrink(6.0);
    let n = 96;
    let xs = |i: usize| inner.left() + inner.width() * i as f32 / n as f32;
    let a = slot.all();
    let col = TEAL();
    match slot.kind {
        FxKind::Eq3 | FxKind::ParaEq | FxKind::Filter => {
            let pts: Vec<Pos2> = (0..=n)
                .map(|i| {
                    let f = 20.0 * (1000.0f32).powf(i as f32 / n as f32);
                    let db = match slot.kind {
                        FxKind::Eq3 => {
                            let lo = a[0] / (1.0 + (f / 200.0).powi(2));
                            let hi = a[2] / (1.0 + (5000.0 / f).powi(2));
                            lo + hi + biquad_peak_db(f, a[3], a[1], 0.9)
                        }
                        FxKind::ParaEq => (0..3).map(|b| biquad_peak_db(f, a[b * 2], a[b * 2 + 1], a[6])).sum(),
                        _ => {
                            let fc = a[1];
                            let r = 1.0 + a[2] * 6.0;
                            let x = f / fc;
                            let mag = match a[0].round() as i32 {
                                1 => x * x / ((1.0 - x * x).powi(2) + (x / r).powi(2)).sqrt(),
                                2 => (x / r) / ((1.0 - x * x).powi(2) + (x / r).powi(2)).sqrt(),
                                _ => 1.0 / ((1.0 - x * x).powi(2) + (x / r).powi(2)).sqrt(),
                            };
                            (20.0 * mag.max(1e-3).log10()).clamp(-30.0, 15.0) * 0.5
                        }
                    };
                    pos2(xs(i), inner.center().y - db.clamp(-20.0, 20.0) / 20.0 * inner.height() * 0.5)
                })
                .collect();
            curve(&p, pts, col);
        }
        FxKind::Compressor | FxKind::Limiter | FxKind::Clipper | FxKind::Gate | FxKind::Saturator | FxKind::Distortion | FxKind::Bitcrusher | FxKind::Exciter => {
            // transfer curve: input level on x, output level on y
            let pts: Vec<Pos2> = (0..=n)
                .map(|i| {
                    let x = i as f32 / n as f32;
                    let y = match slot.kind {
                        FxKind::Compressor => {
                            let db_in = -48.0 + 48.0 * x;
                            let over = (db_in - a[0]).max(0.0);
                            ((db_in - over * (1.0 - 1.0 / a[1].max(1.0)) + a[2]) + 48.0) / 48.0
                        }
                        FxKind::Limiter => {
                            let db_in = -48.0 + 48.0 * x + a[1];
                            (db_in.min(a[0]) + 48.0) / 48.0
                        }
                        FxKind::Gate => {
                            let db_in = -90.0 + 90.0 * x;
                            (if db_in < a[0] { db_in + a[3].max(-90.0) * 0.0 + (a[3] - 0.0).max(-90.0) * 0.0 - 20.0 } else { db_in } + 90.0) / 90.0
                        }
                        FxKind::Clipper => {
                            let v = (x * 2.0 - 1.0) * 2.0;
                            let t = 10f32.powf(a[0] / 20.0);
                            let hard = v.clamp(-t, t);
                            let soft = t * (v / t).tanh();
                            0.5 + (hard + (soft - hard) * a[1]) * 0.25
                        }
                        FxKind::Saturator => 0.5 + ((x * 2.0 - 1.0) * 10f32.powf(a[0] / 20.0)).tanh() * 0.45,
                        FxKind::Distortion => 0.5 + ((x * 2.0 - 1.0) * (1.0 + a[0] * 12.0)).tanh() * 0.45,
                        FxKind::Bitcrusher => {
                            let l = 2f32.powf(a[0].clamp(1.0, 8.0) - 1.0);
                            0.5 + (((x * 2.0 - 1.0) * l).round() / l) * 0.45
                        }
                        _ => 0.5 + ((x * 2.0 - 1.0) * (1.0 + a[1] * 4.0)).tanh() * 0.45,
                    };
                    pos2(xs(i), inner.bottom() - y.clamp(0.0, 1.0) * inner.height())
                })
                .collect();
            p.line_segment([inner.left_bottom(), inner.right_top()], Stroke::new(1.0, MID().linear_multiply(0.4)));
            curve(&p, pts, col);
        }
        FxKind::ViHalla => {
            // decay envelope of the selected mode(s) + level the freeze holds
            let m = HALLA_MODES[(a[0].round() as usize).min(11)];
            let dec = (0.55 + a[1] * (m.2 - 0.55)).min(0.985);
            let t_end = 1.0 / (1.0 - dec).max(0.01);
            let frozen = a[7] > 0.5;
            let pts: Vec<Pos2> = (0..=n)
                .map(|i| {
                    let x = i as f32 / n as f32;
                    let e = if frozen { 0.85 } else { (-x * t_end * 0.35 * m.1.recip().max(0.3)).exp() };
                    pos2(xs(i), inner.bottom() - e * inner.height())
                })
                .collect();
            curve(&p, pts, col);
            p.text(r.left_top() + vec2(8.0, 6.0), Align2::LEFT_TOP, format!("{}{}", m.0, if a[10] > 0.01 { format!(" + {}", HALLA_MODES[(a[9].round() as usize).min(11)].0) } else { String::new() }), FontId::proportional(11.0), DIM());
            if a[6] > 0.01 {
                p.text(r.right_top() + vec2(-8.0, 6.0), Align2::RIGHT_TOP, "SHIMMER", FontId::proportional(10.0), CYAN());
            }
        }
        FxKind::ViGross => {
            let w = inner.width() / 8.0;
            for s in 0..8 {
                let pa = [1.0f32, 0.2, 1.0, 0.2, 1.0, 0.2, 1.0, 0.2][s];
                let pb = [1.0f32, 1.0, 0.1, 1.0, 0.1, 0.1, 1.0, 0.3][s];
                let pat = pa * (1.0 - a[5]) + pb * a[5];
                let v = 1.0 - a[4] * (1.0 - pat);
                let bar = egui::Rect::from_min_max(pos2(inner.left() + w * s as f32 + 2.0, inner.bottom() - v * inner.height()), pos2(inner.left() + w * (s + 1) as f32 - 2.0, inner.bottom()));
                p.rect_filled(bar, 2.0, col.linear_multiply(0.35 + 0.65 * v));
            }
            p.text(r.left_top() + vec2(8.0, 4.0), Align2::LEFT_TOP, format!("rev {:.0}%  stutter {:.0}%", a[1] * 100.0, a[2] * 100.0), FontId::proportional(10.0), DIM());
        }
        FxKind::Widener | FxKind::Utility | FxKind::AutoPan | FxKind::Tremolo | FxKind::Chorus | FxKind::Flanger | FxKind::Phaser | FxKind::Reverb | FxKind::Delay => {
            let pts: Vec<Pos2> = (0..=n)
                .map(|i| {
                    let x = i as f32 / n as f32;
                    let y = match slot.kind {
                        FxKind::Tremolo | FxKind::AutoPan | FxKind::Chorus | FxKind::Flanger | FxKind::Phaser => {
                            let cycles = 2.0;
                            let s = (x * cycles * std::f32::consts::TAU).sin();
                            0.5 + s * 0.4 * a[1].clamp(0.0, 1.0)
                        }
                        FxKind::Reverb => (-x * (4.0 - a[0] * 3.0)).exp(),
                        FxKind::Delay => {
                            let tap = (x / (a[0] / 2.0).max(0.03)).fract();
                            let k = (x / (a[0] / 2.0).max(0.03)).floor();
                            if tap < 0.04 { a[1].powf(k) } else { 0.02 }
                        }
                        _ => 0.5 + (x * std::f32::consts::TAU).sin() * 0.3 * a[0].min(2.0) * 0.5,
                    };
                    pos2(xs(i), inner.bottom() - y.clamp(0.0, 1.0) * inner.height())
                })
                .collect();
            curve(&p, pts, col);
        }
    }
}

/// Small peak indicator next to the header (driven by the slot's wet level setting, not the audio thread).
fn header(ui: &mut Ui, slot: &mut FxSlot) {
    let compact = ui.available_width() < 350.0;
    ui.horizontal_wrapped(|ui| {
        super::widgets::led_tip(ui, &mut slot.on, "Enable / bypass");
        ui.label(RichText::new(slot.kind.name()).strong().size(15.0));
        if !compact {
            ui.label(RichText::new(slot.kind.category()).small().color(DIM()));
            if slot.kind.is_signature() {
                ui.label(RichText::new("WINTERIA").small().color(CYAN()));
            }
        }
        let presets = slot.kind.presets();
        let width = if compact { 72.0 } else { 120.0 };
        egui::ComboBox::from_id_salt(("preset", slot.kind as u8)).selected_text("Presets").width(width).show_ui(ui, |ui| {
            for (name, vals) in &presets {
                if ui.selectable_label(false, *name).clicked() {
                    slot.set_all(vals);
                }
            }
        });
    });
}

/// The template. Draws header, graph, knobs and the dry/wet row for one slot.
pub fn plugin_grid(ui: &mut Ui, slot: &mut FxSlot) {
    header(ui, slot);
    ui.add_space(4.0);
    graph(ui, slot);
    ui.add_space(6.0);
    let specs = slot.kind.specs_full();
    let kind = slot.kind;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
        for (i, spec) in specs.iter().enumerate() {
            let mut v = slot.get(i);
            if is_toggle(kind, i, spec) {
                ui.vertical(|ui| {
                    ui.add_space(22.0);
                    let on = v > 0.5;
                    if ui.add_sized([62.0, 28.0], egui::SelectableLabel::new(on, short(spec.0))).clicked() {
                        v = if on { 0.0 } else { 1.0 };
                    }
                });
            } else {
                let (lo, hi) = (spec.1, spec.2);
                let log = lo > 0.0 && hi / lo >= 40.0;
                let stepped = matches!(spec.0, "Mode A" | "Mode B") || spec.0.starts_with("Type");
                let spec_c = *spec;
                knob(ui, &short(spec.0), &mut v, lo, hi, log, |x| fmt_value(kind, i, &spec_c, x));
                if stepped {
                    v = v.round();
                }
            }
            slot.set(i, v.clamp(spec.1, spec.2));
        }
    });
    ui.horizontal(|ui| {
        ui.label(RichText::new("Dry / Wet").small().color(DIM()));
        ui.add(egui::Slider::new(&mut slot.wet, 0.0..=1.0).show_value(true));
    });
}
