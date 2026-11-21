//! Channel Rack · Smart Steps: per-step Velocity / Pitch / Pan / Slide / Probability for the drum lanes,
//! Step Paint, a live waveform of the lane sound, and Morph A/B between two stored step patterns.
//! Every value is stored in `sequencer::Step` and played back by the audio engine.
use super::theme::*;
use crate::project::Project;
use crate::sequencer::{Step, DRUM_NAMES, NUM_DRUMS};
use egui::{pos2, vec2, Align2, FontId, Rect, RichText, Sense, Stroke, Ui};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Param {
    Velocity,
    Pitch,
    Pan,
    Slide,
    Probability,
}

const PARAMS: [(Param, &str); 5] = [(Param::Velocity, "Velocity"), (Param::Pitch, "Pitch"), (Param::Pan, "Pan"), (Param::Slide, "Slide"), (Param::Probability, "Probability")];

/// Value of a step for display / editing, normalised to 0..=1 (Pitch and Pan are centred at 0.5).
pub fn norm(s: &Step, p: Param) -> f32 {
    match p {
        Param::Velocity => s.vel as f32 / 127.0,
        Param::Pitch => (s.pitch as f32 + 24.0) / 48.0,
        Param::Pan => (s.pan as f32 + 100.0) / 200.0,
        Param::Slide => s.slide as u8 as f32,
        Param::Probability => s.prob as f32 / 100.0,
    }
}

pub fn set_norm(s: &mut Step, p: Param, t: f32) {
    let t = t.clamp(0.0, 1.0);
    match p {
        Param::Velocity => s.vel = ((t * 127.0).round() as u8).clamp(1, 127),
        Param::Pitch => s.pitch = ((t * 48.0).round() as i32 - 24).clamp(-24, 24) as i8,
        Param::Pan => {
            let v = (t * 200.0).round() as i32 - 100;
            s.pan = if v.abs() <= 4 { 0 } else { v.clamp(-100, 100) as i8 };
        }
        Param::Slide => s.slide = t > 0.5,
        Param::Probability => s.prob = ((t * 100.0).round() as u8).clamp(1, 100),
    }
}

fn label(s: &Step, p: Param) -> String {
    match p {
        Param::Velocity => format!("{}", s.vel),
        Param::Pitch => format!("{:+}", s.pitch),
        Param::Pan => format!("{:+}", s.pan),
        Param::Slide => if s.slide { "~".into() } else { String::new() },
        Param::Probability => format!("{}%", s.prob),
    }
}

/// Morph two step patterns: `t` = 0 gives A, 1 gives B. A step switches over at its own threshold, values glide.
pub fn morph(a: &[Step], b: &[Step], t: f32, out: &mut [Step]) {
    for (i, o) in out.iter_mut().enumerate() {
        let (sa, sb) = (a.get(i).copied().unwrap_or_default(), b.get(i).copied().unwrap_or_default());
        // fixed pseudo-random threshold per step so the transition is gradual yet repeatable
        let th = ((i as u32).wrapping_mul(2654435761) >> 8) as f32 / 16_777_216.0;
        let lerp = |x: f32, y: f32| x + (y - x) * t;
        let src = if t > th { &sb } else { &sa };
        *o = Step { on: src.on, vel: lerp(sa.vel as f32, sb.vel as f32).round().clamp(1.0, 127.0) as u8, pitch: lerp(sa.pitch as f32, sb.pitch as f32).round() as i8, pan: lerp(sa.pan as f32, sb.pan as f32).round() as i8, slide: src.slide, prob: lerp(sa.prob as f32, sb.prob as f32).round().clamp(1.0, 100.0) as u8 };
    }
}

fn waveform(ui: &mut Ui, project: &Project, lane: usize, playhead: Option<f32>) {
    let w = ui.available_width().max(160.0);
    let (rect, _) = ui.allocate_exact_size(vec2(w, 44.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, card(), INSET());
    p.rect_stroke(rect, card(), Stroke::new(border_w(), border()));
    p.line_segment([pos2(rect.left() + 4.0, rect.center().y), pos2(rect.right() - 4.0, rect.center().y)], Stroke::new(1.0, MID().linear_multiply(0.3)));
    // the sound loaded in the lane, or the built-in voice
    let owned;
    let data: &[f32] = match project.drum_samples[lane].as_ref().and_then(|r| r.data.as_deref()) {
        Some(d) => &d.mono,
        None => {
            owned = crate::instruments::drums::render_factory(lane, 44100);
            &owned
        }
    };
    if data.is_empty() {
        return;
    }
    let cols = (rect.width() as usize - 8).max(8);
    let per = (data.len() / cols).max(1);
    let inner = rect.shrink2(vec2(4.0, 4.0));
    for c in 0..cols {
        let seg = &data[(c * per).min(data.len())..((c + 1) * per).min(data.len())];
        let peak = seg.iter().fold(0.0f32, |m, x| m.max(x.abs())).min(1.0);
        let x = inner.left() + c as f32;
        p.line_segment([pos2(x, inner.center().y - peak * inner.height() * 0.5), pos2(x, inner.center().y + peak * inner.height() * 0.5)], Stroke::new(1.0, lane_col(lane)));
    }
    if let Some(ph) = playhead {
        let x = inner.left() + ph.clamp(0.0, 1.0) * inner.width();
        p.line_segment([pos2(x, rect.top() + 2.0), pos2(x, rect.bottom() - 2.0)], Stroke::new(1.5, CYAN()));
    }
}

/// The panel. `play_step` is the current step while the pattern plays.
pub fn show(ui: &mut Ui, project: &mut Project, cur: usize, len: usize, play_step: Option<usize>) {
    let id = egui::Id::new("smart_steps_state");
    let (mut lane, mut param): (usize, usize) = ui.data(|d| d.get_temp(id)).unwrap_or((0, 0));
    let mem_a = egui::Id::new(("morph_a", cur, lane));
    let mem_b = egui::Id::new(("morph_b", cur, lane));
    egui::Frame::none().fill(PANEL2()).rounding(card()).stroke(Stroke::new(border_w(), border())).inner_margin(10.0).show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Smart Steps").strong().size(14.0));
            ui.separator();
            for l in 0..NUM_DRUMS {
                if ui.selectable_label(lane == l, DRUM_NAMES[l]).clicked() {
                    lane = l;
                }
            }
            ui.separator();
            for (i, (_, name)) in PARAMS.iter().enumerate() {
                if ui.selectable_label(param == i, *name).clicked() {
                    param = i;
                }
            }
        });
        ui.add_space(6.0);
        let ph = play_step.map(|s| (s % len.max(1)) as f32 / len.max(1) as f32);
        waveform(ui, project, lane, ph);
        ui.add_space(6.0);

        // step grid: paint on/off on the top row, edit the chosen value on the bars below (drag across steps)
        let (p_kind, _) = PARAMS[param];
        let cw = ((ui.available_width() - 4.0) / len.max(1) as f32).clamp(14.0, 30.0);
        let h = 86.0;
        let (rect, resp) = ui.allocate_exact_size(vec2(cw * len as f32, h + 26.0), Sense::click_and_drag());
        let steps = &mut project.patterns[cur].drums[lane];
        let pos = resp.interact_pointer_pos().filter(|_| resp.is_pointer_button_down_on());
        let paint_id = id.with("paint");
        if let Some(pp) = pos {
            let i = ((pp.x - rect.left()) / cw).floor();
            if i >= 0.0 && (i as usize) < len {
                let i = i as usize;
                if pp.y < rect.top() + 24.0 {
                    // Step Paint: the first touched step decides whether the drag paints or erases
                    let target = ui.data_mut(|d| *d.get_temp_mut_or_insert_with(paint_id, || !steps[i].on));
                    steps[i].on = target;
                } else if steps[i].on {
                    let t = 1.0 - (pp.y - (rect.top() + 26.0)) / h;
                    if p_kind == Param::Slide {
                        if resp.drag_started() || resp.clicked() {
                            steps[i].slide = !steps[i].slide;
                        }
                    } else {
                        set_norm(&mut steps[i], p_kind, t);
                    }
                }
            }
        } else {
            ui.data_mut(|d| d.remove::<bool>(paint_id));
        }
        let p = ui.painter();
        let col = lane_col(lane);
        for s in 0..len {
            let st = steps[s];
            let x0 = rect.left() + s as f32 * cw;
            let cell = Rect::from_min_size(pos2(x0 + 1.0, rect.top()), vec2(cw - 2.0, 22.0));
            let base = if (s / 4) % 2 == 0 { PANEL3() } else { PANEL3().linear_multiply(0.8) };
            p.rect_filled(cell, 6.0, if st.on { col } else { base });
            if st.is_smart() && st.on {
                p.circle_filled(cell.right_top() + vec2(-4.0, 4.0), 2.0, egui::Color32::WHITE);
            }
            if play_step.map_or(false, |ps| ps % len == s) {
                p.rect_stroke(cell, 6.0, Stroke::new(2.0, CYAN()));
            }
            let bar_area = Rect::from_min_size(pos2(x0 + 2.0, rect.top() + 26.0), vec2(cw - 4.0, h));
            p.rect_filled(bar_area, 4.0, INSET().linear_multiply(0.6));
            if st.on {
                let t = norm(&st, p_kind);
                let bar = if matches!(p_kind, Param::Pitch | Param::Pan) {
                    let mid = bar_area.bottom() - h * 0.5;
                    let y = bar_area.bottom() - t * h;
                    Rect::from_min_max(pos2(bar_area.left(), y.min(mid)), pos2(bar_area.right(), y.max(mid)))
                } else {
                    Rect::from_min_max(pos2(bar_area.left(), bar_area.bottom() - t * h), bar_area.right_bottom())
                };
                p.rect_filled(bar, 4.0, col.linear_multiply(0.85));
                if cw >= 20.0 {
                    p.text(pos2(bar_area.center().x, bar_area.top() + 8.0), Align2::CENTER_CENTER, label(&st, p_kind), FontId::monospace(8.5), TEXT());
                }
            }
        }
        ui.add_space(6.0);

        // Morph A/B: store the lane in A and B, then blend with the slider (applies to the real lane immediately)
        ui.horizontal(|ui| {
            ui.label(RichText::new("Morph").small().color(DIM()));
            if ui.button("Set A").on_hover_text("Store this lane as A").clicked() {
                let snap = project.patterns[cur].drums[lane][..len].to_vec();
                ui.data_mut(|d| d.insert_temp(mem_a, snap));
            }
            if ui.button("Set B").on_hover_text("Store this lane as B").clicked() {
                let snap = project.patterns[cur].drums[lane][..len].to_vec();
                ui.data_mut(|d| d.insert_temp(mem_b, snap));
            }
            let a = ui.data(|d| d.get_temp::<Vec<Step>>(mem_a));
            let b = ui.data(|d| d.get_temp::<Vec<Step>>(mem_b));
            let mut t = ui.data(|d| d.get_temp::<f32>(id.with("t"))).unwrap_or(0.0);
            let ready = a.is_some() && b.is_some();
            if ui.add_enabled(ready, egui::Slider::new(&mut t, 0.0..=1.0).text("A > B")).changed() {
                if let (Some(a), Some(b)) = (&a, &b) {
                    morph(a, b, t, &mut project.patterns[cur].drums[lane][..len]);
                }
            }
            ui.data_mut(|d| d.insert_temp(id.with("t"), t));
            if ui.button("Clear lane").clicked() {
                for s in project.patterns[cur].drums[lane].iter_mut() {
                    *s = Step::default();
                }
            }
            if ui.button("Reset smart values").clicked() {
                for s in project.patterns[cur].drums[lane].iter_mut() {
                    let (on, vel) = (s.on, s.vel);
                    *s = Step::new(on, vel);
                }
            }
        });
    });
    ui.data_mut(|d| d.insert_temp(id, (lane, param)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smart_values_round_trip_and_morph() {
        let mut s = Step::new(true, 100);
        for p in [Param::Velocity, Param::Pitch, Param::Pan, Param::Probability] {
            set_norm(&mut s, p, 0.75);
            assert!((norm(&s, p) - 0.75).abs() < 0.02, "{:?}", p as u8);
        }
        set_norm(&mut s, Param::Slide, 1.0);
        assert!(s.slide && s.is_smart());
        let a = vec![Step::new(true, 100); 16];
        let mut b = vec![Step::new(false, 40); 16];
        b[3] = Step::new(true, 40);
        let mut out = vec![Step::default(); 16];
        morph(&a, &b, 0.0, &mut out);
        assert!(out.iter().all(|s| s.on));
        morph(&a, &b, 1.0, &mut out);
        assert_eq!(out.iter().filter(|s| s.on).count(), 1);
        assert_eq!(out[3].vel, 40);
    }

    #[test]
    fn probability_gates_hits_deterministically() {
        use crate::sequencer::step_passes;
        let hits = (0..1000u64).filter(|&pass| step_passes(50, pass, 4, 0)).count();
        assert!((300..700).contains(&hits), "{hits}");
        assert!((0..100u64).all(|p| step_passes(100, p, 1, 1)));
        assert_eq!((0..100u64).filter(|&p| step_passes(50, p, 2, 1)).count(), (0..100u64).filter(|&p| step_passes(50, p, 2, 1)).count());
    }
}
