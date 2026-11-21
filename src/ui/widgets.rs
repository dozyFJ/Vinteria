use super::theme::*;
use egui::{pos2, vec2, Align2, Color32, FontId, Pos2, Rect, Sense, Shape, Stroke, Ui};
use std::f32::consts::PI;

/// Rotary knob driven by vertical dragging (shift = fine, wheel = step). Returns true when changed.
pub fn knob(ui: &mut Ui, label: &str, v: &mut f32, min: f32, max: f32, log: bool, fmt: impl Fn(f32) -> String) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(62.0, 78.0), Sense::click_and_drag());
    let to_t = |x: f32| if log { (x / min).ln() / (max / min).ln() } else { (x - min) / (max - min) };
    let from_t = |t: f32| if log { min * (max / min).powf(t) } else { min + t * (max - min) };
    let mut t = to_t(*v).clamp(0.0, 1.0);
    let before = *v;
    if resp.dragged() {
        let speed = if ui.input(|i| i.modifiers.shift) { 0.0015 } else { 0.006 };
        t = (t - resp.drag_delta().y * speed).clamp(0.0, 1.0);
        *v = from_t(t);
    }
    if resp.hovered() {
        let sc = ui.input(|i| i.raw_scroll_delta.y);
        if sc != 0.0 {
            t = (t + sc * 0.0006).clamp(0.0, 1.0);
            *v = from_t(t);
        }
    }
    let p = ui.painter();
    let c = pos2(rect.center().x, rect.top() + 34.0);
    let r = 18.0;
    let a0 = 0.75 * PI;
    let sweep = 1.5 * PI;
    let arc = |t0: f32, t1: f32| -> Vec<Pos2> {
        let n = 32;
        (0..=n)
            .map(|i| {
                let a = a0 + sweep * (t0 + (t1 - t0) * i as f32 / n as f32);
                c + vec2(a.cos(), a.sin()) * r
            })
            .collect()
    };
    let hot = resp.hovered() || resp.dragged();
    if hot {
        p.circle_stroke(c, r + 7.0, Stroke::new(2.0, TEAL().linear_multiply(0.35)));
    }
    // value ring (groove + lit arc), then a raised cap with a pointer dot
    p.add(Shape::line(arc(0.0, 1.0), Stroke::new(4.0, MID().linear_multiply(0.55))));
    if t > 0.003 {
        p.add(Shape::line(arc(0.0, t), Stroke::new(4.0, if hot { TEAL_HI() } else { TEAL() })));
    }
    let cap = r - 6.0;
    p.circle_filled(c + vec2(0.0, 1.5), cap, Color32::BLACK.linear_multiply(0.18));
    p.circle_filled(c, cap, if is_dark() { PANEL3() } else { PANEL2() });
    p.circle_stroke(c, cap, Stroke::new(border_w(), MID()));
    let a = a0 + sweep * t;
    p.circle_filled(c + vec2(a.cos(), a.sin()) * (cap - 4.0), 2.2, TEXT());
    p.text(pos2(rect.center().x, rect.top() + 6.0), Align2::CENTER_CENTER, label, FontId::proportional(11.0), DIM());
    p.text(pos2(rect.center().x, rect.bottom() - 7.0), Align2::CENTER_CENTER, fmt(*v), FontId::monospace(10.5), TEXT());
    (*v - before).abs() > f32::EPSILON
}

pub fn lin_knob(ui: &mut Ui, label: &str, v: &mut f32, min: f32, max: f32, unit: &str) -> bool {
    knob(ui, label, v, min, max, false, |x| format!("{x:.2}{unit}"))
}

pub fn log_knob(ui: &mut Ui, label: &str, v: &mut f32, min: f32, max: f32) -> bool {
    knob(ui, label, v, min, max, true, |x| if x >= 1000.0 { format!("{:.1}k", x / 1000.0) } else { format!("{x:.0}Hz") })
}

pub fn time_knob(ui: &mut Ui, label: &str, v: &mut f32, min: f32, max: f32) -> bool {
    knob(ui, label, v, min, max, true, |x| if x < 1.0 { format!("{:.0}ms", x * 1000.0) } else { format!("{x:.2}s") })
}

pub fn db(v: f32) -> f32 {
    20.0 * v.max(1e-5).log10()
}

/// Vertical stereo level meter (-60..0 dB): green -> accent segments, amber / red clip zone, in a soft rounded dark well.
pub fn meter(ui: &mut Ui, size: egui::Vec2, lr: [f32; 2]) {
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, small() * 0.5, INSET());
    let w = (rect.width() - 3.0) / 2.0;
    for (i, v) in lr.iter().enumerate() {
        let frac = ((db(*v) + 60.0) / 60.0).clamp(0.0, 1.0);
        let x0 = rect.left() + 1.0 + i as f32 * (w + 1.0);
        let h = rect.height() * frac;
        let seg = 14;
        for s in 0..seg {
            let y0 = rect.bottom() - rect.height() * (s as f32 / seg as f32);
            let y1 = rect.bottom() - rect.height() * ((s + 1) as f32 / seg as f32);
            if rect.bottom() - y0 > h {
                break;
            }
            let t = s as f32 / seg as f32;
            let col = if t > 0.92 {
                RED
            } else if t > 0.78 {
                AMBER
            } else {
                lerp_color(GREEN, GOLD(), t / 0.78)
            };
            p.rect_filled(Rect::from_min_max(pos2(x0, y1 + 0.5), pos2(x0 + w, y0 - 0.5)), 1.0, col);
        }
    }
}

/// Meter ballistics: the bar rises at once and falls back smoothly; a peak tick stays for a moment and then sinks. `new` is the
/// level now (0..1); returns (bar, peak). The state lives in egui's memory under `id`.
pub fn meter_ballistics(ui: &Ui, id: egui::Id, new: f32) -> (f32, f32) {
    let now = ui.input(|i| i.time);
    let dt = ui.input(|i| i.stable_dt).clamp(0.0, 0.1);
    let (mut level, mut peak, mut held_at): (f32, f32, f64) = ui.data(|d| d.get_temp(id)).unwrap_or((0.0, 0.0, now));
    level = if new >= level { new } else { (level - dt * 1.1).max(new) };
    if new >= peak {
        peak = new;
        held_at = now;
    } else if now - held_at > 0.7 {
        peak = (peak - dt * 0.5).max(level);
    }
    ui.data_mut(|d| d.insert_temp(id, (level, peak, held_at)));
    (level, peak)
}

/// Smooth 0..1 transition towards `on` (hover, selected, pressed): the colours of a control glide instead of jumping.
pub fn fade(ui: &Ui, id: egui::Id, on: bool, secs: f32) -> f32 {
    ui.ctx().animate_bool_with_time(id, on, secs)
}

// ------------------------------------------------------------------ "liquid" fill of notes and clips while the song plays

static FILL_STYLE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(255);

/// 0 = off, 1 = pad (Launchpad flash), 2 = glow, 3 = ripple. Remembered in `fill_style.txt` next to the program.
pub fn fill_style() -> u8 {
    use std::sync::atomic::Ordering::Relaxed;
    let v = FILL_STYLE.load(Relaxed);
    if v != 255 {
        return v;
    }
    let read = std::fs::read_to_string("fill_style.txt").ok().and_then(|t| t.trim().parse::<u8>().ok()).filter(|v| *v <= 3).unwrap_or(1);
    FILL_STYLE.store(read, Relaxed);
    read
}

pub fn set_fill_style(v: u8) {
    FILL_STYLE.store(v.min(3), std::sync::atomic::Ordering::Relaxed);
    let _ = std::fs::write("fill_style.txt", v.min(3).to_string());
}

pub const FILL_NAMES: [&str; 4] = ["Off", "Pad", "Glow", "Ripple"];

/// How lit a pad is `age` ticks after its hit when the hit lasts `len` ticks: a bright flash right on the beat that settles to a
/// steady glow while the sound is held, and a quick fade after it ends.
pub fn pad_intensity(age: f32, len: f32) -> f32 {
    if age < 0.0 {
        return 0.0;
    }
    let flash = (1.0 - age / 56.0).clamp(0.0, 1.0);
    let held = if age <= len { 1.0 } else { (1.0 - (age - len) / 44.0).clamp(0.0, 1.0) };
    (0.38 + 0.62 * flash) * held
}

/// A Launchpad pad: the rectangle lights up in its colour exactly when the sound hits, a halo breathes out around it and fades.
pub fn pad_light(p: &egui::Painter, r: Rect, col: Color32, k: f32) {
    if k <= 0.01 {
        return;
    }
    let hot = lerp_color(col, Color32::WHITE, 0.35 + 0.5 * k);
    p.rect_filled(r, small(), hot.gamma_multiply((0.25 + 0.65 * k).min(1.0)));
    // inner bright core and the halo rings (they grow as the flash fades, like a pad's light spilling out)
    let core = r.shrink2(vec2((r.width() * 0.18).min(6.0), (r.height() * 0.22).min(5.0)));
    if core.width() > 2.0 && core.height() > 2.0 {
        p.rect_filled(core, small(), Color32::from_white_alpha((40.0 + 150.0 * k * k) as u8));
    }
    for i in 1..=3 {
        let grow = i as f32 * (1.2 + (1.0 - k) * 2.5);
        p.rect_stroke(r.expand(grow), small() + grow, Stroke::new(1.2, col.linear_multiply(0.55 * k * (1.0 - i as f32 / 4.0))));
    }
    p.rect_stroke(r, small(), Stroke::new(1.5, Color32::from_white_alpha((90.0 + 140.0 * k) as u8)));
}

/// The playing-note effect, `age` ticks after the hit (see `pad_intensity`). Pad (default): the Launchpad flash; Glow: the whole
/// rect lights up and breathes; Ripple: soft bands run through it.
pub fn play_fill(p: &egui::Painter, r: Rect, col: Color32, age: f32, len: f32, time: f64) {
    let style = fill_style();
    if style == 0 || r.width() < 3.0 || r.height() < 3.0 {
        return;
    }
    let k = pad_intensity(age, len);
    let t = time as f32;
    let bright = lerp_color(col, Color32::WHITE, 0.55);
    match style {
        1 => pad_light(p, r, col, k),
        2 => {
            let breathe = 0.5 + 0.5 * (t * 6.0).sin();
            p.rect_filled(r, small(), bright.linear_multiply((0.25 + 0.25 * breathe) * k.max(0.3)));
            for i in 1..=3 {
                p.rect_stroke(r.expand(i as f32 * 1.4), small() + i as f32, Stroke::new(1.0, col.linear_multiply(0.30 * (1.0 - i as f32 / 4.0) * (0.6 + 0.4 * breathe))));
            }
        }
        _ => {
            let n = ((r.width() / 3.0).ceil() as usize).min(400);
            for i in 0..n {
                let x = r.left() + i as f32 * 3.0;
                let w = (3.0f32).min(r.right() - x);
                let a = 0.5 + 0.5 * (x * 0.12 - t * 7.0).sin();
                p.rect_filled(Rect::from_min_size(pos2(x, r.top()), vec2(w, r.height())), 0.0, bright.linear_multiply((0.05 + 0.32 * a * a) * k.max(0.3)));
            }
        }
    }
}

pub fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t) as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

#[derive(Clone, Copy, PartialEq)]
pub enum Icon {
    Play,
    Stop,
    Record,
}

/// Transport button with a vector icon.
pub fn icon_button(ui: &mut Ui, icon: Icon, active: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(40.0, 32.0), Sense::click());
    let hov_t = fade(ui, resp.id.with("h"), resp.hovered(), 0.12);
    let act_t = fade(ui, resp.id.with("a"), active, 0.16);
    let hov = resp.hovered();
    let col = match icon {
        Icon::Play => GREEN,
        Icon::Stop => DIM(),
        Icon::Record => RED,
    };
    let rest = lerp_color(PANEL3(), lerp_color(PANEL3(), TEXT(), 0.08), hov_t);
    let bg = lerp_color(rest, lerp_color(PANEL3(), col, 0.28), act_t);
    let edge = lerp_color(lerp_color(border(), MID(), hov_t), col, act_t);
    let _ = hov;
    let p = ui.painter();
    p.rect(rect, corner(), bg, Stroke::new(border_w(), edge));
    let c = rect.center();
    match icon {
        Icon::Play => p.add(Shape::convex_polygon(vec![c + vec2(-5.0, -8.0), c + vec2(-5.0, 8.0), c + vec2(8.0, 0.0)], col, Stroke::NONE)),
        Icon::Stop => p.add(Shape::rect_filled(Rect::from_center_size(c, vec2(12.0, 12.0)), small() * 0.5, col)),
        Icon::Record => p.add(Shape::circle_filled(c, 7.0, col)),
    };
    resp
}

pub fn section_label(ui: &mut Ui, text: &str) {
    ui.label(egui::RichText::new(text).color(TEXT()).strong().size(13.0));
}

/// 8x8 pixel icons (same idea as the launcher's pixel icons).
pub const ICON_SEQ: [&str; 8] = ["##.##.##", "##.##.##", "........", "##.##.##", "##.##.##", "........", "##.##.##", "##.##.##"];
pub const ICON_ROLL: [&str; 8] = ["#.##.##.", "#.##.##.", "#.##.##.", "#.##.##.", "#..#..#.", "#..#..#.", "#..#..#.", "########"];
pub const ICON_ARR: [&str; 8] = ["########", "........", "###.####", "........", ".#####..", "........", "##..###.", "........"];
pub const ICON_MIX: [&str; 8] = ["..#..#..", "..#..#.#", "###.####", "..#..#.#", "..#..#.#", "#.#.##.#", "#.#.##.#", "#.#.##.#"];
pub const ICON_SYNTH: [&str; 8] = ["........", "..#.....", ".#.#..#.", "#...#.#.", "....###.", "........", "########", "........"];
pub const ICON_LIB: [&str; 8] = ["........", "###.....", "########", "#......#", "#......#", "#......#", "########", "........"];

pub fn pixel_icon(p: &egui::Painter, icon: &[&str; 8], origin: Pos2, scale: f32, color: Color32) {
    for (y, row) in icon.iter().enumerate() {
        for (x, ch) in row.bytes().enumerate() {
            if ch == b'#' {
                p.rect_filled(Rect::from_min_size(origin + vec2(x as f32 * scale, y as f32 * scale), vec2(scale, scale)), 0.0, color);
            }
        }
    }
}

/// Sidebar navigation button: soft rounded row, accent pill + tinted fill when selected.
pub fn nav_button(ui: &mut Ui, text: &str, icon: &[&str; 8], selected: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 42.0), Sense::click());
    let hover = resp.hovered();
    let hov_t = fade(ui, resp.id.with("h"), hover, 0.12);
    let sel_t = fade(ui, resp.id.with("s"), selected, 0.16);
    let p = ui.painter();
    let row = rect.shrink2(vec2(4.0, 3.0));
    p.rect_filled(row, corner(), PANEL3().linear_multiply(hov_t * (1.0 - sel_t)));
    p.rect_filled(row, corner(), TEAL().linear_multiply(sel_t * if is_dark() { 0.22 } else { 0.14 }));
    p.rect_filled(Rect::from_center_size(pos2(row.left() + 3.0, row.center().y), vec2(3.0, 18.0 * sel_t)), 1.5, TEAL());
    pixel_icon(p, icon, pos2(rect.left() + 16.0, rect.center().y - 8.0), 2.0, if selected { TEAL() } else if hover { TEXT() } else { DIM() });
    p.text(pos2(rect.left() + 46.0, rect.center().y), Align2::LEFT_CENTER, text, FontId::proportional(14.0), if selected || hover { TEXT() } else { DIM() });
    resp
}

/// Big primary button (the launcher's "play" button): flat accent fill, soft corners, no bevel.
pub fn gold_button(ui: &mut Ui, text: &str, size: egui::Vec2, enabled: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let hover = resp.hovered() && enabled;
    let down = resp.is_pointer_button_down_on() && enabled;
    let hov_t = fade(ui, resp.id.with("h"), hover, 0.12);
    let down_t = fade(ui, resp.id.with("d"), down, 0.06);
    let fill = if !enabled { PANEL3() } else { lerp_color(lerp_color(TEAL(), Color32::WHITE, 0.12 * hov_t), Color32::BLACK, 0.14 * down_t) };
    let p = ui.painter();
    p.rect_filled(rect, card(), fill);
    if !enabled {
        p.rect_stroke(rect, card(), Stroke::new(border_w(), border()));
    }
    p.text(rect.center() + vec2(0.0, if down { 1.0 } else { 0.0 }), Align2::CENTER_CENTER, text, FontId::proportional(20.0), if enabled { Color32::WHITE } else { DIM() });
    resp
}

/// Vertical gradient fill (brushed-metal look).
pub fn gradient_rect(p: &egui::Painter, rect: Rect, top: Color32, bottom: Color32) {
    let mut m = egui::Mesh::default();
    m.colored_vertex(rect.left_top(), top);
    m.colored_vertex(rect.right_top(), top);
    m.colored_vertex(rect.right_bottom(), bottom);
    m.colored_vertex(rect.left_bottom(), bottom);
    m.add_triangle(0, 1, 2);
    m.add_triangle(0, 2, 3);
    p.add(Shape::mesh(m));
}

/// Flat toolbar button with an optional hotkey hint (FL-style window buttons).
/// The synthwave picture behind the project hub: a night sky, a striped sun on the horizon and a perspective grid floor.
pub fn synthwave_backdrop(p: &egui::Painter, rect: Rect) {
    if !is_dark() {
        gradient_rect(p, rect, rgba_lerp(0xFBF3E2), rgba_lerp(0xE6C79A));
        return;
    }
    let horizon = rect.top() + rect.height() * 0.58;
    gradient_rect(p, Rect::from_min_max(rect.min, pos2(rect.right(), horizon)), Color32::from_rgb(0x14, 0x0F, 0x0B), Color32::from_rgb(0x4A, 0x2A, 0x1E));
    p.rect_filled(Rect::from_min_max(pos2(rect.left(), horizon), rect.max), 0.0, Color32::from_rgb(0x12, 0x0D, 0x0A));
    // sun
    let r = rect.height().min(rect.width()) * 0.19;
    let c = pos2(rect.center().x, horizon - r * 0.35);
    let (top_col, bot_col) = (Color32::from_rgb(0xF4, 0xD9, 0x9A), Color32::from_rgb(0xC9, 0x6A, 0x2E));
    let mut y = c.y - r;
    while y < horizon {
        let dy = (y + 1.0 - c.y).abs().min(r);
        let half = (r * r - dy * dy).max(0.0).sqrt();
        let t = ((y - (c.y - r)) / (2.0 * r)).clamp(0.0, 1.0);
        // dark stripes cut the lower part of the sun, wider towards the horizon
        let k = ((y - c.y) / r).max(0.0);
        let stripe = k > 0.12 && (((y - c.y) / (3.0 + 5.0 * k)) as i32) % 2 == 1 && k < 1.2;
        if !stripe && half > 0.5 {
            p.rect_filled(Rect::from_min_max(pos2(c.x - half, y), pos2(c.x + half, (y + 2.0).min(horizon))), 0.0, lerp_color(top_col, bot_col, t));
        }
        y += 2.0;
    }
    // grid floor: lines meeting at the vanishing point, horizontals spaced closer near the horizon
    let line = Color32::from_rgb(0x4F, 0x95, 0xFF).linear_multiply(0.5);
    let vp = pos2(rect.center().x, horizon);
    for i in -16..=16 {
        let x = rect.center().x + i as f32 * rect.width() * 0.09;
        p.line_segment([vp, pos2(vp.x + (x - vp.x) * 4.0, rect.bottom())], Stroke::new(1.0, line));
    }
    for k in 1..14 {
        let f = (k as f32 / 14.0).powf(2.2);
        let y = horizon + f * (rect.bottom() - horizon);
        p.line_segment([pos2(rect.left(), y), pos2(rect.right(), y)], Stroke::new(1.0, line.linear_multiply(0.5 + f)));
    }
    p.line_segment([pos2(rect.left(), horizon), pos2(rect.right(), horizon)], Stroke::new(2.0, Color32::from_rgb(0x27, 0xC6, 0xE6)));
}

fn rgba_lerp(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

/// A thin synthwave "horizon" line along the bottom of a bar: pink -> purple -> blue, with a soft glow above it.
pub fn neon_line(p: &egui::Painter, r: Rect) {
    if !is_dark() {
        return;
    }
    let (a, b, c) = (Color32::from_rgb(0xFF, 0x4F, 0xC8), Color32::from_rgb(0xB1, 0x6B, 0xFF), Color32::from_rgb(0x4D, 0xA3, 0xFF));
    let n = ((r.width() / 6.0) as usize).max(2);
    for i in 0..n {
        let t = i as f32 / (n - 1) as f32;
        let col = if t < 0.5 { lerp_color(a, b, t * 2.0) } else { lerp_color(b, c, (t - 0.5) * 2.0) };
        let x0 = r.left() + t * r.width();
        let w = r.width() / n as f32 + 1.0;
        p.rect_filled(Rect::from_min_size(egui::pos2(x0, r.bottom() - 2.0), vec2(w, 2.0)), 0.0, col);
        p.rect_filled(Rect::from_min_size(egui::pos2(x0, r.bottom() - 7.0), vec2(w, 5.0)), 0.0, col.linear_multiply(0.10));
    }
}

/// Soft 3D edge: a light line on the top / left and a dark one on the bottom / right (swapped when pressed).
pub fn bevel(p: &egui::Painter, r: Rect, raised: bool) {
    let (hi, lo) = (Color32::from_white_alpha(if is_dark() { 46 } else { 130 }), Color32::from_black_alpha(if is_dark() { 90 } else { 40 }));
    let (a, b) = if raised { (hi, lo) } else { (lo, hi) };
    p.line_segment([r.left_top() + vec2(0.0, 0.5), r.right_top() + vec2(0.0, 0.5)], Stroke::new(1.0, a));
    p.line_segment([r.left_top() + vec2(0.5, 0.0), r.left_bottom() + vec2(0.5, 0.0)], Stroke::new(1.0, a));
    p.line_segment([r.left_bottom() - vec2(0.0, 0.5), r.right_bottom() - vec2(0.0, 0.5)], Stroke::new(1.0, b));
    p.line_segment([r.right_top() - vec2(0.5, 0.0), r.right_bottom() - vec2(0.5, 0.0)], Stroke::new(1.0, b));
}

pub fn toolbar_button(ui: &mut Ui, label: &str, key: &str, selected: bool) -> egui::Response {
    let text_w = |s: &str, size: f32| ui.fonts(|f| f.layout_no_wrap(s.to_string(), FontId::proportional(size), TEXT()).size().x);
    let lw = text_w(label, 13.0);
    let kw = if key.is_empty() { 0.0 } else { text_w(key, 10.0) + 6.0 };
    let (rect, resp) = ui.allocate_exact_size(vec2(lw + kw + 20.0, 28.0), Sense::click());
    let hov_t = fade(ui, resp.id.with("h"), resp.hovered(), 0.12);
    let sel_t = fade(ui, resp.id.with("s"), selected, 0.16);
    let down_t = fade(ui, resp.id.with("d"), resp.is_pointer_button_down_on(), 0.06);
    let p = ui.painter();
    // the colours glide between the resting, hovered and selected look
    // flat outline: no fill at rest, a thin edge in the text colour, text centred
    let edge_rest = TEXT().linear_multiply(0.38);
    let edge = lerp_color(lerp_color(edge_rest, TEXT().linear_multiply(0.7), hov_t), TEAL(), sel_t);
    let fill = lerp_color(lerp_color(Color32::TRANSPARENT, TEXT().linear_multiply(0.07), hov_t), TEAL().linear_multiply(0.20), sel_t);
    let _ = down_t;
    p.rect_filled(rect, 0.0, fill);
    p.rect_stroke(rect, 0.0, Stroke::new(border_w().max(1.0), edge));
    let fg = TEXT();
    let kfont = FontId::proportional(10.0);
    let gap = if key.is_empty() { 0.0 } else { 6.0 };
    let total = lw + if key.is_empty() { 0.0 } else { kw - 6.0 + gap };
    let x0 = rect.center().x - total / 2.0;
    p.text(pos2(x0, rect.center().y), Align2::LEFT_CENTER, label, FontId::proportional(13.0), fg);
    if !key.is_empty() {
        p.text(pos2(x0 + lw + gap, rect.center().y + 1.0), Align2::LEFT_CENTER, key, kfont, DIM());
    }
    resp
}

/// Round on/off LED (FL channel rack style). Returns true when clicked.
pub fn led(ui: &mut Ui, on: &mut bool) -> bool {
    led_tip(ui, on, "Mute / unmute")
}

/// `led` with its own tooltip.
pub fn led_tip(ui: &mut Ui, on: &mut bool, tip: &str) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(16.0, 22.0), Sense::click());
    let c = rect.center();
    let on_t = fade(ui, resp.id.with("on"), *on, 0.14);
    let hov_t = fade(ui, resp.id.with("h"), resp.hovered(), 0.10);
    let p = ui.painter();
    p.circle_filled(c, 6.0, PANEL3());
    p.circle_stroke(c, 6.0, Stroke::new(border_w(), lerp_color(MID(), TEAL(), hov_t)));
    if on_t > 0.01 {
        p.circle_stroke(c, 7.0, Stroke::new(1.0, TEAL().linear_multiply(0.3 * on_t)));
        p.circle_filled(c, 4.5 * on_t, TEAL());
        p.circle_filled(c - vec2(1.0, 1.0), 1.6 * on_t, Color32::from_white_alpha((120.0 * on_t) as u8));
    }
    let clicked = resp.on_hover_text(tip).clicked();
    if clicked {
        *on = !*on;
    }
    clicked
}

/// Small rotary knob (22px): drag vertically, double-click resets. Returns true when changed.
pub fn mini_knob(ui: &mut Ui, v: &mut f32, min: f32, max: f32, reset: f32, tip: &str) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::click_and_drag());
    let before = *v;
    let mut t = ((*v - min) / (max - min)).clamp(0.0, 1.0);
    if resp.dragged() {
        t = (t - resp.drag_delta().y * 0.008).clamp(0.0, 1.0);
        *v = min + t * (max - min);
    }
    if resp.double_clicked() {
        *v = reset;
        t = ((*v - min) / (max - min)).clamp(0.0, 1.0);
    }
    let c = rect.center();
    let a = 0.75 * PI + 1.5 * PI * t;
    let p = ui.painter();
    p.circle_filled(c, 9.0, PANEL3());
    p.circle_stroke(c, 9.0, Stroke::new(border_w(), if resp.hovered() { TEAL() } else { MID() }));
    p.line_segment([c + vec2(a.cos(), a.sin()) * 2.5, c + vec2(a.cos(), a.sin()) * 8.0], Stroke::new(2.0, TEXT()));
    resp.on_hover_text(format!("{tip}: {:.2}", *v));
    (*v - before).abs() > f32::EPSILON
}

/// Channel number box.
pub fn number_box(ui: &mut Ui, n: usize, selected: bool) {
    let (rect, _) = ui.allocate_exact_size(vec2(28.0, CH_BOX_H), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, corner(), if selected { TEAL() } else { PANEL3() });
    p.rect_stroke(rect, corner(), Stroke::new(border_w(), if selected { TEAL() } else { border() }));
    p.text(rect.center(), Align2::CENTER_CENTER, n.to_string(), FontId::proportional(14.0), if selected { Color32::WHITE } else { DIM() });
}

const CH_BOX_H: f32 = 26.0;

/// FL-style channel selector: a slim vertical bar right of the name button, lit when the channel is selected.
pub fn selector_bar(ui: &mut Ui, selected: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(8.0, 24.0), Sense::click());
    let p = ui.painter();
    p.rect_filled(rect, small(), if selected { TEAL() } else { PANEL3() });
    p.rect_stroke(rect, small(), Stroke::new(border_w(), if selected { TEAL() } else { border() }));
    resp
}

/// Routing box: shows the mixer insert this channel goes to (M = master). Drag or scroll to change, right-click = master.
pub fn route_box(ui: &mut Ui, output: &mut Option<usize>, selected: bool) {
    let mut v = output.map_or(0, |i| i as i32 + 1);
    let r = ui.add_sized(
        vec2(32.0, CH_BOX_H),
        egui::DragValue::new(&mut v)
            .range(0..=crate::mixer::NUM_INSERTS as i32)
            .speed(0.08)
            .custom_formatter(|n, _| if n < 0.5 { "M".to_string() } else { format!("{}", n.round() as i32) })
            .custom_parser(|s| if s.trim().eq_ignore_ascii_case("m") { Some(0.0) } else { s.trim().parse::<f64>().ok() }),
    );
    let _ = selected;
    if r.secondary_clicked() {
        v = 0;
    }
    r.on_hover_text("Mixer insert this channel plays through (M = master). Drag / scroll to change, right-click: master");
    *output = if v <= 0 { None } else { Some(v as usize - 1) };
}

#[cfg(test)]
mod pad_tests {
    use super::pad_intensity;

    #[test]
    fn a_pad_flashes_on_the_hit_holds_and_fades_after_the_sound() {
        let len = 200.0;
        assert_eq!(pad_intensity(-1.0, len), 0.0, "nothing before the hit");
        let hit = pad_intensity(0.0, len);
        let held = pad_intensity(100.0, len);
        assert!(hit > 0.99, "full flash on the beat: {hit}");
        assert!(held > 0.3 && held < hit, "settles to a glow while held: {held}");
        assert!(pad_intensity(len + 22.0, len) < held, "fades after the end");
        assert_eq!(pad_intensity(len + 60.0, len), 0.0, "dark again");
    }
}
