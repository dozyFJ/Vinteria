//! Slicer (like FL's Slicex): the waveform of a dropped loop / bounced track with slice markers.
//! Click the wave to add a marker, drag a marker to move it, right-click it to delete it; Ctrl+wheel zooms, the wheel scrolls.
//! Key `root + n` plays slice n; "Chop to piano roll" writes the slices as notes in their original order.
use super::theme::*;
use crate::instruments::sampler::{detect_onsets, grid_slices, step_slices, SamplerParams, MAX_SLICES};
use crate::piano_roll::{Note, Track};
use crate::sequencer::{PPQ, TICKS_PER_STEP};
use egui::{pos2, vec2, Align2, Color32, FontId, Sense, Stroke, Ui};
use std::sync::Arc;

/// The visible part of the wave, as fractions of the whole sample.
#[derive(Clone, Copy)]
struct View {
    v0: f32,
    v1: f32,
}

type PeakKey = (usize, u32, u32, usize);

/// One (min, max) pair per pixel column of the visible range; cached because a 3-minute track is millions of frames.
fn peaks(ui: &Ui, mono: &Arc<crate::project::SampleData>, view: View, cols: usize) -> Arc<Vec<(f32, f32)>> {
    let id = egui::Id::new("slicer_peaks");
    let key: PeakKey = (Arc::as_ptr(mono) as usize, view.v0.to_bits(), view.v1.to_bits(), cols);
    if let Some((k, v)) = ui.data(|d| d.get_temp::<(PeakKey, Arc<Vec<(f32, f32)>>)>(id)) {
        if k == key {
            return v;
        }
    }
    let n = mono.mono.len() as f64;
    let a = view.v0 as f64 * n;
    let b = view.v1 as f64 * n;
    let out: Vec<(f32, f32)> = (0..cols)
        .map(|c| {
            let s = (a + (b - a) * c as f64 / cols as f64) as usize;
            let e = ((a + (b - a) * (c + 1) as f64 / cols as f64) as usize).max(s + 1).min(mono.mono.len());
            let s = s.min(e.saturating_sub(1));
            // very long columns are sampled, not scanned: the picture stays the same, the cost stays small
            let stride = ((e - s) / 256).max(1);
            let (mut lo, mut hi) = (0.0f32, 0.0f32);
            let mut i = s;
            while i < e {
                let x = mono.mono[i];
                lo = lo.min(x);
                hi = hi.max(x);
                i += stride;
            }
            (lo, hi)
        })
        .collect();
    let out = Arc::new(out);
    ui.data_mut(|d| d.insert_temp(id, (key, Arc::clone(&out))));
    out
}

/// The slicer section of the sampler panel. Returns the notes to write when "Chop to piano roll" was pressed:
/// (start tick, length in ticks, pitch).
pub fn show(ui: &mut Ui, p: &mut SamplerParams, bpm: f32) -> Option<Vec<(u32, u32, u8)>> {
    let mut chop = None;
    let Some(sd) = p.sample.as_ref().and_then(|s| s.data.clone()) else { return None };
    let len = sd.mono.len().max(1);
    let secs = len as f32 / sd.rate.max(1) as f32;

    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("SLICER").color(TEAL()).strong().size(15.0));
        if ui.checkbox(&mut p.slice_mode, "Slice mode").on_hover_text("Each key plays one slice of the sample (from the root key up) instead of the whole sample transposed").changed() && p.slice_mode && p.slices.is_empty() {
            p.slices = vec![0];
        }
        ui.label(egui::RichText::new(format!("{:.2} s, {} slices", secs, p.slices.len())).color(DIM()).size(11.0));
    });

    // ---- auto slicing
    let beat_frames = sd.rate as f64 * 60.0 / bpm.max(20.0) as f64;
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new("Auto:").color(DIM()).size(11.0));
        for n in [4usize, 8, 16, 32] {
            if ui.small_button(format!("{n} equal")).on_hover_text("Cut the sample into equal pieces").clicked() {
                p.slices = grid_slices(len, n);
                p.slice_mode = true;
            }
        }
        for (name, beats) in [("1 beat", 1.0), ("1/2 beat", 0.5), ("1/4 beat", 0.25)] {
            if ui.small_button(name).on_hover_text("A slice every so many beats at the project tempo (the loop must start on the beat)").clicked() {
                p.slices = step_slices(len, beat_frames * beats);
                p.slice_mode = true;
            }
        }
    });
    ui.horizontal(|ui| {
        let sens_id = egui::Id::new("slicer_sens");
        let mut sens: f32 = ui.data(|d| d.get_temp(sens_id)).unwrap_or(0.5);
        if ui.button("Detect hits").on_hover_text("Cut at the beginning of every drum hit / note it finds").clicked() {
            p.slices = detect_onsets(&sd.mono, sd.rate, sens);
            p.slice_mode = true;
        }
        ui.add(egui::Slider::new(&mut sens, 0.0..=1.0).text("sensitivity").show_value(false));
        ui.data_mut(|d| d.insert_temp(sens_id, sens));
        if ui.button("Clear").on_hover_text("Back to one slice").clicked() {
            p.slices = vec![0];
        }
    });

    // ---- the waveform
    let view_id = egui::Id::new("slicer_view");
    let drag_id = egui::Id::new("slicer_drag");
    let mut view: View = ui.data(|d| d.get_temp::<(f32, f32)>(view_id)).map(|(a, b)| View { v0: a, v1: b }).unwrap_or(View { v0: 0.0, v1: 1.0 });
    let w = ui.available_width().max(200.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 150.0), Sense::click_and_drag());
    let span = (view.v1 - view.v0).max(1e-5);

    if resp.hovered() {
        let (zd, scroll) = ui.input(|i| (i.zoom_delta(), i.smooth_scroll_delta));
        if zd != 1.0 {
            // zoom around the mouse
            let at = resp.hover_pos().map_or(0.5, |m| (m.x - rect.left()) / rect.width());
            let focus = view.v0 + span * at;
            let ns = (span / zd).clamp(0.002, 1.0);
            view.v0 = (focus - ns * at).clamp(0.0, 1.0 - ns);
            view.v1 = view.v0 + ns;
        } else if scroll.y != 0.0 || scroll.x != 0.0 {
            let d = -(scroll.y + scroll.x) / rect.width() * span;
            view.v0 = (view.v0 + d).clamp(0.0, 1.0 - span);
            view.v1 = view.v0 + span;
        }
    }
    let span = (view.v1 - view.v0).max(1e-5);
    let v0 = view.v0;
    let x_of = |frame: f32| rect.left() + (frame / len as f32 - v0) / span * rect.width();
    let frame_of = |x: f32| (((x - rect.left()) / rect.width() * span + v0) * len as f32).clamp(0.0, len as f32 - 1.0);
    ui.data_mut(|d| d.insert_temp(view_id, (view.v0, view.v1)));

    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, card(), INSET());
    // slice bodies, alternating tint
    let marks: Vec<f32> = p.slices.iter().map(|s| *s as f32).collect();
    for (k, s) in marks.iter().enumerate() {
        let end = marks.get(k + 1).copied().unwrap_or(len as f32);
        let (x0, x1) = (x_of(*s).max(rect.left()), x_of(end).min(rect.right()));
        if x1 > x0 {
            let tint = if k % 2 == 0 { TEAL().linear_multiply(0.14) } else { TEAL_HI().linear_multiply(0.07) };
            painter.rect_filled(egui::Rect::from_min_max(pos2(x0, rect.top()), pos2(x1, rect.bottom())), 0.0, if p.slice_mode { tint } else { Color32::TRANSPARENT });
        }
    }
    let cols = rect.width() as usize;
    let pk = peaks(ui, &sd, view, cols.max(1));
    let mid = rect.center().y;
    let amp = rect.height() * 0.46;
    for (c, (lo, hi)) in pk.iter().enumerate() {
        let x = rect.left() + c as f32 + 0.5;
        painter.line_segment([pos2(x, mid - hi.min(1.0) * amp), pos2(x, mid - lo.max(-1.0) * amp - 0.0)], Stroke::new(1.0, LIGHT().linear_multiply(0.95)));
    }
    painter.line_segment([pos2(rect.left(), mid), pos2(rect.right(), mid)], Stroke::new(1.0, MID().linear_multiply(0.5)));

    // ---- markers: hit test, drag, add, delete
    let hit = |sl: &[u32], x: f32| sl.iter().enumerate().skip(1).find(|(_, s)| (x_of(**s as f32) - x).abs() <= 5.0).map(|(k, _)| k);
    let hover_m = resp.hover_pos().and_then(|m| hit(&p.slices, m.x));
    let dragging: Option<usize> = ui.data(|d| d.get_temp(drag_id));
    if resp.drag_started_by(egui::PointerButton::Primary) {
        if let Some(m) = resp.interact_pointer_pos().and_then(|m| hit(&p.slices, m.x)) {
            ui.data_mut(|d| d.insert_temp(drag_id, m));
        }
    }
    if let (Some(k), Some(m)) = (dragging, resp.interact_pointer_pos()) {
        if resp.dragged() && k < p.slices.len() {
            let lo = p.slices[k - 1] + 1;
            let hi = p.slices.get(k + 1).map_or(len as u32 - 1, |n| n - 1);
            p.slices[k] = (frame_of(m.x) as u32).clamp(lo.min(hi), hi.max(lo));
        }
    }
    if resp.drag_stopped() {
        ui.data_mut(|d| d.remove::<usize>(drag_id));
    }
    if resp.clicked_by(egui::PointerButton::Primary) && dragging.is_none() {
        if let Some(m) = resp.interact_pointer_pos() {
            if hit(&p.slices, m.x).is_none() && p.slices.len() < MAX_SLICES {
                let f = frame_of(m.x) as u32;
                if !p.slices.contains(&f) {
                    p.slices.push(f);
                    p.slices.sort_unstable();
                    p.slice_mode = true;
                }
            }
        }
    }
    if resp.secondary_clicked() {
        if let Some(k) = resp.interact_pointer_pos().and_then(|m| hit(&p.slices, m.x)) {
            p.slices.remove(k);
        }
    }
    for (k, s) in p.slices.iter().enumerate() {
        let x = x_of(*s as f32);
        if x < rect.left() - 1.0 || x > rect.right() + 1.0 {
            continue;
        }
        let hot = hover_m == Some(k) || dragging == Some(k);
        let col = if k == 0 { MID() } else if hot { CYAN() } else { TEAL() };
        painter.line_segment([pos2(x, rect.top()), pos2(x, rect.bottom())], Stroke::new(if hot { 2.0 } else { 1.2 }, col));
        if p.slice_mode {
            painter.text(pos2(x + 3.0, rect.top() + 2.0), Align2::LEFT_TOP, format!("{}", k + 1), FontId::monospace(10.0), TEXT());
            let key = p.root as usize + k;
            if key < 128 {
                painter.text(pos2(x + 3.0, rect.bottom() - 2.0), Align2::LEFT_BOTTOM, crate::piano_roll::note_name(key as u8), FontId::monospace(9.0), DIM());
            }
        }
    }
    if hover_m.is_some() || dragging.is_some() {
        ui.output_mut(|o| o.cursor_icon = egui::CursorIcon::ResizeHorizontal);
    }
    resp.on_hover_text("Click: add a slice - drag a marker: move it - right-click a marker: delete it - Ctrl+wheel: zoom - wheel: scroll");

    // ---- chop to the piano roll
    ui.horizontal(|ui| {
        if span < 0.999 && ui.small_button("Show all").clicked() {
            ui.data_mut(|d| d.insert_temp(view_id, (0.0f32, 1.0f32)));
        }
        let has = p.slices.len() > 1 || p.slice_mode;
        if ui.add_enabled(has, egui::Button::new("Chop to piano roll")).on_hover_text("Writes the slices as notes in the original order and timing (at the project tempo), on the keys from the root note up. Replaces the sampler notes of this pattern.").clicked() {
            let ticks_per_frame = bpm.max(20.0) as f64 / 60.0 * PPQ as f64 / sd.rate.max(1) as f64;
            let notes: Vec<(u32, u32, u8)> = p
                .slices
                .iter()
                .enumerate()
                .take_while(|(k, _)| p.root as usize + k < 128)
                .map(|(k, s)| {
                    let end = p.slices.get(k + 1).map_or(len as u32, |e| *e);
                    let start = (*s as f64 * ticks_per_frame).round() as u32;
                    let stop = (end as f64 * ticks_per_frame).round() as u32;
                    (start, stop.saturating_sub(start).max(1), p.root + k as u8)
                })
                .collect();
            p.slice_mode = true;
            chop = Some(notes);
        }
    });
    chop
}

/// Replace the sampler notes of the pattern by the chopped slices and make the pattern long enough to hold them.
pub fn write_chop(pat: &mut crate::sequencer::Pattern, notes: &[(u32, u32, u8)]) {
    pat.notes.retain(|n| n.track != Track::Sampler);
    let mut end = 0;
    for (start, len, pitch) in notes {
        pat.notes.push(Note { start: *start, len: *len, pitch: *pitch, vel: 100, track: Track::Sampler, slide: false, mute: false, pan: 0, chance: 100 });
        end = end.max(start + len);
    }
    let steps = end.div_ceil(TICKS_PER_STEP).max(16);
    let need = [16u32, 32, 64, 128, 256].into_iter().find(|l| *l >= steps).unwrap_or(crate::sequencer::MAX_STEPS as u32);
    pat.length_steps = pat.length_steps.max(need);
}
