//! Phone layout in the style of the Pulse messenger (the PC keeps its own AOS look).
//! Portrait UI with a translucent header, glass cards, and navigation above the content.
use super::*;
use crate::mixer::{NUM_CH, NUM_INSERTS};
use crate::sequencer::{DRUM_NAMES, NUM_DRUMS, TICKS_PER_STEP};
use egui::{Align2, FontId, Frame, Margin, Painter, Pos2, Rect, Response, RichText, Sense, Shape, Ui};

const TOUCH: f32 = 44.0;
/// written by the Android panic hook, shown on the next start
pub const CRASH_FILE: &str = "crash.txt";

// ───────────────────── Pulse palette ─────────────────────

fn argb(v: u32) -> Color32 {
    Color32::from_rgba_unmultiplied((v >> 16) as u8, (v >> 8) as u8, v as u8, (v >> 24) as u8)
}
fn pick(dark: u32, light: u32) -> Color32 {
    argb(if is_dark() { dark } else { light })
}
/// main text
fn ink() -> Color32 {
    pick(0xFFF1F3F5, 0xFF222A32)
}
/// secondary text
fn muted() -> Color32 {
    pick(0xFFBFC5CC, 0xFF4F5B68)
}
/// Pulse accent: a calm near-white (night) / slate (day) pill
fn p_accent() -> Color32 {
    pick(0xFFDCE5EE, 0xFF35424F)
}
fn on_accent() -> Color32 {
    pick(0xFF1A1B1D, 0xFFFFFFFF)
}
fn p_bg() -> Color32 {
    pick(0xFF1A1E23, 0xFFD9E1E9)
}
fn doodle() -> Color32 {
    pick(0x10FFFFFF, 0x18404855)
}
fn bar_fill() -> Color32 {
    pick(0xA022262B, 0xA0F1F5F9)
}
fn outline() -> Color32 {
    pick(0xFF555F69, 0x826C7A89)
}
fn glass_top() -> Color32 {
    pick(0x80404750, 0xA6FCFDFF)
}
fn glass_bottom() -> Color32 {
    pick(0x60272C32, 0x66EFF4F9)
}
fn glass_edge() -> Color32 {
    pick(0x48AAB7C4, 0xB3FFFFFF)
}
fn lerp_c(a: Color32, b: Color32, t: f32) -> Color32 {
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_premultiplied(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()), m(a.a(), b.a()))
}

// ───────────────────── drawing helpers ─────────────────────

fn rounded_points(r: Rect, rad: f32) -> Vec<Pos2> {
    let rad = rad.min(r.width() * 0.5).min(r.height() * 0.5).max(0.0);
    let corners = [
        (pos2(r.right() - rad, r.top() + rad), -90.0f32),
        (pos2(r.right() - rad, r.bottom() - rad), 0.0),
        (pos2(r.left() + rad, r.bottom() - rad), 90.0),
        (pos2(r.left() + rad, r.top() + rad), 180.0),
    ];
    let mut pts = Vec::with_capacity(32);
    for (c, start) in corners {
        for i in 0..=6 {
            let a = (start + i as f32 * 15.0).to_radians();
            pts.push(pos2(c.x + rad * a.cos(), c.y + rad * a.sin()));
        }
    }
    pts
}

/// Glassmorphism card: translucent top-to-bottom gradient with a light rim, like Pulse's `glass()`.
fn glass(p: &Painter, r: Rect, rad: f32) {
    glass_tinted(p, r, rad, None);
}

fn glass_tinted(p: &Painter, r: Rect, rad: f32, tint: Option<Color32>) {
    let pts = rounded_points(r, rad);
    let (top, bottom) = match tint {
        Some(t) => (t, lerp_c(t, Color32::TRANSPARENT, 0.25)),
        None => (glass_top(), glass_bottom()),
    };
    let col = |y: f32| lerp_c(top, bottom, ((y - r.top()) / r.height().max(1.0)).clamp(0.0, 1.0));
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(r.center(), col(r.center().y));
    for q in &pts {
        mesh.colored_vertex(*q, col(q.y));
    }
    let n = pts.len() as u32;
    for i in 0..n {
        mesh.add_triangle(0, 1 + i, 1 + (i + 1) % n);
    }
    p.add(Shape::mesh(mesh));
    p.add(Shape::closed_line(pts, Stroke::new(1.0, glass_edge())));
}

/// Glass on an opaque base, for editors where the doodles would get in the way (steps, piano roll, playlist, mixer).
fn panel(p: &Painter, r: Rect, rad: f32) {
    p.rect_filled(r, rad, p_bg());
    glass(p, r, rad);
}

fn pill(p: &Painter, r: Rect, fill: Color32) {
    p.rect_filled(r, r.height() * 0.5, fill);
}

/// Grey background with a faint pattern of music doodles (Pulse uses chat icons, like Telegram).
fn doodle_background(ctx: &egui::Context) {
    let rect = ctx.screen_rect();
    let p = ctx.layer_painter(egui::LayerId::background());
    p.rect_filled(rect, 0.0, p_bg());
    let col = doodle();
    let s = Stroke::new(1.4, col);
    let cell = 62.0;
    if !rect.is_finite() || rect.width() <= 0.0 {
        return;
    }
    let cols = ((rect.width() / cell).ceil() as i32 + 1).min(60);
    let rows = ((rect.height() / cell).ceil() as i32 + 1).min(80);
    for j in 0..rows {
        for i in 0..cols {
            let h = (i as u32).wrapping_mul(73856093) ^ (j as u32).wrapping_mul(19349663);
            let ox = (h % 17) as f32 - 8.0;
            let oy = ((h >> 5) % 17) as f32 - 8.0;
            let c = pos2(rect.left() + i as f32 * cell + cell * 0.5 + ox, rect.top() + j as f32 * cell + cell * 0.5 + oy + if i % 2 == 1 { cell * 0.5 } else { 0.0 });
            match (h >> 10) % 5 {
                0 => {
                    // eighth note
                    p.circle_stroke(c + vec2(-4.0, 6.0), 4.0, s);
                    p.line_segment([c + vec2(0.0, 6.0), c + vec2(0.0, -9.0)], s);
                    p.line_segment([c + vec2(0.0, -9.0), c + vec2(6.0, -4.0)], s);
                }
                1 => {
                    // beamed notes
                    p.circle_stroke(c + vec2(-7.0, 6.0), 3.5, s);
                    p.circle_stroke(c + vec2(5.0, 4.0), 3.5, s);
                    p.line_segment([c + vec2(-3.5, 6.0), c + vec2(-3.5, -8.0)], s);
                    p.line_segment([c + vec2(8.5, 4.0), c + vec2(8.5, -10.0)], s);
                    p.line_segment([c + vec2(-3.5, -8.0), c + vec2(8.5, -10.0)], s);
                }
                2 => {
                    // wave
                    let pts: Vec<Pos2> = (0..=16).map(|k| c + vec2(-11.0 + k as f32 * 1.4, (k as f32 * 0.8).sin() * 4.0)).collect();
                    p.add(Shape::line(pts, s));
                }
                3 => {
                    // knob
                    p.circle_stroke(c, 7.0, s);
                    p.line_segment([c, c + vec2(4.0, -4.0)], s);
                }
                _ => {
                    // little fader
                    p.line_segment([c + vec2(0.0, -9.0), c + vec2(0.0, 9.0)], s);
                    p.rect_stroke(Rect::from_center_size(c + vec2(0.0, 2.0), vec2(9.0, 4.0)), 1.0, s);
                }
            }
        }
    }
}

/// The app logo (yin-yang of night and day with a W), the same picture as the launcher icon.
fn logo(ui: &Ui, rect: Rect) {
    let id = egui::Id::new("winteria_logo_tex");
    let tex = match ui.ctx().data(|d| d.get_temp::<egui::TextureHandle>(id)) {
        Some(t) => t,
        None => {
            let img = egui::ColorImage::from_rgba_unmultiplied([128, 128], include_bytes!("../../assets/winteria_icon128.rgba"));
            let t = ui.ctx().load_texture("winteria_logo", img, egui::TextureOptions::LINEAR);
            ui.ctx().data_mut(|d| d.insert_temp(id, t.clone()));
            t
        }
    };
    ui.painter().image(tex.id(), rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
}

#[derive(Clone, Copy, PartialEq)]
enum Icon {
    Folder,
    Clock,
    Layout,
    Wave,
    Plug,
    Steps,
    Keys,
    Timeline,
    Mixer,
    Tracks,
    Exit,
    FullView,
    Orientation,
    Rewind,
    Play,
    Pause,
    Record,
    Stop,
    Plus,
    Session,
    Share,
}

fn icon(p: &Painter, kind: Icon, c: Pos2, col: Color32) {
    let s = Stroke::new(1.8, col);
    match kind {
        Icon::Folder => {
            let r = Rect::from_center_size(c + vec2(0.0, 1.5), vec2(18.0, 13.0));
            p.rect_stroke(r, 2.0, s);
            p.line_segment([r.left_top() + vec2(1.0, 0.0), r.left_top() + vec2(6.0, -3.0)], s);
            p.line_segment([r.left_top() + vec2(6.0, -3.0), r.left_top() + vec2(9.0, 0.0)], s);
        }
        Icon::Clock => {
            p.circle_stroke(c, 8.5, s);
            p.line_segment([c, c + vec2(0.0, -5.0)], s);
            p.line_segment([c, c + vec2(4.0, 2.0)], s);
        }
        Icon::Layout => {
            p.rect_stroke(Rect::from_center_size(c, vec2(17.0, 15.0)), 2.0, s);
            p.line_segment([c + vec2(-8.5, -2.0), c + vec2(8.5, -2.0)], s);
            p.line_segment([c + vec2(-1.0, -2.0), c + vec2(-1.0, 7.5)], s);
        }
        Icon::Wave => {
            for (k, h) in [3.0f32, 7.0, 10.0, 6.0, 8.0, 3.5].iter().enumerate() {
                let x = c.x - 7.5 + k as f32 * 3.0;
                p.line_segment([pos2(x, c.y - h), pos2(x, c.y + h)], s);
            }
        }
        Icon::Plug => {
            p.rect_stroke(Rect::from_center_size(c + vec2(0.0, 1.0), vec2(12.0, 9.0)), 3.0, s);
            p.line_segment([c + vec2(-3.0, -3.5), c + vec2(-3.0, -8.0)], s);
            p.line_segment([c + vec2(3.0, -3.5), c + vec2(3.0, -8.0)], s);
            p.line_segment([c + vec2(0.0, 5.5), c + vec2(0.0, 9.0)], s);
        }
        Icon::Steps => {
            for y in 0..3 {
                for x in 0..3 {
                    let r = Rect::from_center_size(c + vec2(-6.0 + x as f32 * 6.0, -6.0 + y as f32 * 6.0), vec2(4.2, 4.2));
                    if (x + y) % 2 == 0 {
                        p.rect_filled(r, 1.0, col);
                    } else {
                        p.rect_stroke(r, 1.0, Stroke::new(1.2, col));
                    }
                }
            }
        }
        Icon::Keys => {
            let r = Rect::from_center_size(c, vec2(18.0, 15.0));
            p.rect_stroke(r, 2.0, s);
            for k in 1..4 {
                let x = r.left() + k as f32 * 4.5;
                p.line_segment([pos2(x, r.top()), pos2(x, r.bottom())], Stroke::new(1.2, col));
            }
            for k in [1.0f32, 2.0] {
                p.rect_filled(Rect::from_min_size(pos2(r.left() + k * 4.5 + 1.2 + (k - 1.0) * 4.5 - 4.5, r.top()), vec2(3.0, 8.0)), 0.5, col);
            }
        }
        Icon::Timeline => {
            for (k, (x0, w)) in [(-8.0f32, 9.0f32), (-3.0, 11.0), (-8.0, 6.0)].iter().enumerate() {
                let y = c.y - 6.0 + k as f32 * 6.0;
                p.rect_filled(Rect::from_min_size(pos2(c.x + x0, y - 2.0), vec2(*w, 4.0)), 1.5, col);
            }
        }
        Icon::Mixer => {
            for (k, y) in [3.0f32, -4.0, 1.0].iter().enumerate() {
                let x = c.x - 6.0 + k as f32 * 6.0;
                p.line_segment([pos2(x, c.y - 9.0), pos2(x, c.y + 9.0)], Stroke::new(1.2, col));
                p.rect_filled(Rect::from_center_size(pos2(x, c.y + y), vec2(5.0, 3.0)), 1.0, col);
            }
        }
        Icon::Tracks => {
            for k in 0..3 {
                let y = c.y - 6.0 + k as f32 * 6.0;
                p.circle_filled(pos2(c.x - 7.0, y), 1.8, col);
                p.line_segment([pos2(c.x - 3.0, y), pos2(c.x + 9.0, y)], s);
            }
        }
        Icon::Exit => {
            p.rect_stroke(Rect::from_min_max(c + vec2(-2.0, -8.0), c + vec2(8.0, 8.0)), 2.0, s);
            p.line_segment([c + vec2(-9.0, 0.0), c + vec2(2.0, 0.0)], s);
            p.line_segment([c + vec2(-9.0, 0.0), c + vec2(-5.0, -4.0)], s);
            p.line_segment([c + vec2(-9.0, 0.0), c + vec2(-5.0, 4.0)], s);
        }
        Icon::FullView => {
            for x in [-1.0, 1.0] {
                for y in [-1.0, 1.0] {
                    let corner = c + vec2(x * 8.0, y * 8.0);
                    p.line_segment([corner, c + vec2(x * 3.0, y * 8.0)], s);
                    p.line_segment([corner, c + vec2(x * 8.0, y * 3.0)], s);
                }
            }
        }
        Icon::Orientation => {
            p.rect_stroke(Rect::from_center_size(c + vec2(-4.0, 0.0), vec2(8.0, 15.0)), 2.0, s);
            p.line_segment([c + vec2(2.0, -6.0), c + vec2(7.0, -6.0)], s);
            p.line_segment([c + vec2(7.0, -6.0), c + vec2(7.0, 3.0)], s);
            p.line_segment([c + vec2(7.0, 3.0), c + vec2(4.0, 0.0)], s);
            p.line_segment([c + vec2(7.0, 3.0), c + vec2(10.0, 0.0)], s);
        }
        Icon::Rewind => {
            p.line_segment([c + vec2(-8.0, -7.0), c + vec2(-8.0, 7.0)], s);
            p.add(Shape::convex_polygon(vec![c + vec2(5.0, -7.0), c + vec2(-5.0, 0.0), c + vec2(5.0, 7.0)], col, Stroke::NONE));
        }
        Icon::Pause => {
            for x in [-3.0, 3.0] {
                p.rect_filled(Rect::from_center_size(c + vec2(x, 0.0), vec2(3.0, 13.0)), 1.0, col);
            }
        }
        Icon::Record => {
            p.circle_filled(c, 6.0, col);
        }
        Icon::Play => {
            p.add(Shape::convex_polygon(vec![c + vec2(-4.0, -7.0), c + vec2(7.0, 0.0), c + vec2(-4.0, 7.0)], col, Stroke::NONE));
        }
        Icon::Stop => {
            p.rect_filled(Rect::from_center_size(c, vec2(11.0, 11.0)), 2.0, col);
        }
        Icon::Session => {
            for y in 0..2 {
                for x in 0..2 {
                    let r = Rect::from_center_size(c + vec2(-4.5 + x as f32 * 9.0, -4.5 + y as f32 * 9.0), vec2(7.0, 7.0));
                    p.rect_stroke(r, 1.5, s);
                }
            }
            p.add(Shape::convex_polygon(vec![c + vec2(-6.5, -7.0), c + vec2(-2.5, -4.5), c + vec2(-6.5, -2.0)], col, Stroke::NONE));
        }
        Icon::Share => {
            let a = c + vec2(6.0, -7.0);
            let b = c + vec2(-6.0, 0.0);
            let d = c + vec2(6.0, 7.0);
            p.line_segment([a, b], Stroke::new(1.6, col));
            p.line_segment([b, d], Stroke::new(1.6, col));
            for q in [a, b, d] {
                p.circle_filled(q, 3.0, col);
            }
        }
        Icon::Plus => {
            p.line_segment([c + vec2(-7.0, 0.0), c + vec2(7.0, 0.0)], Stroke::new(2.2, col));
            p.line_segment([c + vec2(0.0, -7.0), c + vec2(0.0, 7.0)], Stroke::new(2.2, col));
        }
    }
}

/// Section title inside the scrolling body.
fn heading(ui: &mut Ui, text: &str) {
    ui.add_space(4.0);
    ui.label(RichText::new(text.to_uppercase()).size(12.0).strong().color(muted()));
    ui.add_space(4.0);
}

/// Glass list row: coloured badge with a letter, title, subtitle and a right-hand note.
fn list_row(ui: &mut Ui, badge: Color32, letter: &str, title: &str, sub: &str, note: &str, selected: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 64.0), Sense::click());
    let p = ui.painter();
    if selected {
        glass_tinted(p, rect, 14.0, Some(p_accent().linear_multiply(0.30)));
    } else {
        glass(p, rect, 14.0);
    }
    let b = Rect::from_center_size(pos2(rect.left() + 32.0, rect.center().y), vec2(42.0, 42.0));
    p.circle_filled(b.center(), 21.0, badge.linear_multiply(0.85));
    p.text(b.center(), Align2::CENTER_CENTER, letter, FontId::proportional(18.0), Color32::WHITE);
    p.text(pos2(rect.left() + 64.0, rect.top() + 14.0), Align2::LEFT_TOP, title, FontId::proportional(16.0), ink());
    if !sub.is_empty() {
        p.text(pos2(rect.left() + 64.0, rect.top() + 36.0), Align2::LEFT_TOP, sub, FontId::proportional(12.0), muted());
    }
    if !note.is_empty() {
        p.text(pos2(rect.right() - 14.0, rect.top() + 16.0), Align2::RIGHT_TOP, note, FontId::proportional(11.0), muted());
    }
    ui.add_space(8.0);
    resp
}

/// Pulse button: accent pill (primary) or glass pill.
fn p_button(ui: &mut Ui, text: &str, primary: bool, size: egui::Vec2) -> Response {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let p = ui.painter();
    if primary {
        pill(p, rect, if resp.is_pointer_button_down_on() { p_accent().linear_multiply(0.8) } else { p_accent() });
    } else {
        glass(p, rect, rect.height() * 0.5);
    }
    p.text(rect.center(), Align2::CENTER_CENTER, text, FontId::proportional(15.0), if primary { on_accent() } else { ink() });
    resp
}

fn segmented(ui: &mut Ui, id_salt: &str, items: &[&str]) -> usize {
    let id = egui::Id::new(id_salt);
    let mut sel: usize = ui.data(|d| d.get_temp(id)).unwrap_or(0).min(items.len().saturating_sub(1));
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
        for (i, name) in items.iter().enumerate() {
            let w = 22.0 + name.chars().count() as f32 * 8.0;
            if p_button(ui, name, sel == i, vec2(w, 34.0)).clicked() {
                sel = i;
            }
        }
    });
    ui.data_mut(|d| d.insert_temp(id, sel));
    ui.add_space(8.0);
    sel
}

/// egui widgets (sliders, text fields, drag values) restyled to match the glass look.
fn mobile_scroll(area: egui::ScrollArea) -> egui::ScrollArea {
    area.scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
}

fn pulse_visuals(ui: &mut Ui) {
    // Shared desktop editors also inherit this style while embedded in a mobile sheet.
    let scroll = &mut ui.spacing_mut().scroll;
    scroll.bar_width = 0.0;
    scroll.floating_width = 0.0;
    scroll.floating_allocated_width = 0.0;
    scroll.bar_inner_margin = 0.0;
    scroll.bar_outer_margin = 0.0;
    let v = ui.visuals_mut();
    v.override_text_color = Some(ink());
    v.extreme_bg_color = glass_bottom();
    v.selection.bg_fill = p_accent().linear_multiply(0.6);
    v.selection.stroke = Stroke::new(1.0, p_accent());
    for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active] {
        w.bg_fill = glass_top();
        w.weak_bg_fill = glass_top();
        w.bg_stroke = Stroke::new(1.0, glass_edge());
        w.rounding = egui::Rounding::same(12.0);
        w.fg_stroke = Stroke::new(1.5, ink());
    }
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, ink());
    v.slider_trailing_fill = true;
}

const PROJECT_SECTIONS: [(&str, Icon); 4] = [("Projects", Icon::Folder), ("Recent", Icon::Clock), ("Templates", Icon::Layout), ("Sounds", Icon::Wave)];
const STUDIO_SECTIONS: [(&str, Icon); 6] = [
    ("Playlist", Icon::Timeline), ("Steps", Icon::Steps), ("Piano", Icon::Keys),
    ("Session", Icon::Session), ("Mixer", Icon::Mixer), ("Tracks", Icon::Tracks),
];
const SEC_PLAYLIST: usize = 0;
const SEC_PIANO: usize = 2;

/// Export & Share sheet of the phone layout: renders on a worker thread, then hands the WAV to Android's share sheet.
#[derive(Default)]
pub(super) struct MobileExport {
    open: bool,
    song: bool,
    share_after: bool,
    rx: Option<Receiver<Result<PathBuf, String>>>,
    status: String,
    last: Option<PathBuf>,
}

fn export_dir() -> PathBuf {
    #[cfg(target_os = "android")]
    {
        // shared Music folder: the file stays visible to players and other apps (Android 11+ lets an app add its own files here)
        let d = PathBuf::from("/storage/emulated/0/Music/Winteria");
        if std::fs::create_dir_all(&d).is_ok() {
            return d;
        }
    }
    let d = PathBuf::from("exports");
    let _ = std::fs::create_dir_all(&d);
    d
}

impl VinteriaApp {
    pub(super) fn is_mobile(&self) -> bool {
        cfg!(target_os = "android") || std::env::var_os("SYNTHESIA_MOBILE").is_some()
    }

    pub(super) fn mobile_ui(&mut self, ctx: &egui::Context) {
        let crash_id = egui::Id::new("m_crash_report");
        if !ctx.data(|d| d.get_temp::<bool>(egui::Id::new("m_crash_checked")).unwrap_or(false)) {
            ctx.data_mut(|d| d.insert_temp(egui::Id::new("m_crash_checked"), true));
            if let Ok(text) = std::fs::read_to_string(CRASH_FILE) {
                ctx.data_mut(|d| d.insert_temp(crash_id, text));
            }
        }
        if let Some(report) = ctx.data(|d| d.get_temp::<String>(crash_id)) {
            doodle_background(ctx);
            self.m_crash_screen(ctx, &report, crash_id);
            return;
        }
        if crate::safe_mode() {
            doodle_background(ctx);
            self.m_safe_screen(ctx);
            return;
        }
        let first = ctx.cumulative_pass_nr() < 4;
        if first {
            crate::boot_log("phone screens");
        }
        // a bug in the phone screens shows its message instead of closing the app
        let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.mobile_screens(ctx)));
        if let Err(e) = run {
            let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_else(|| "unknown panic".into());
            let report = std::fs::read_to_string(CRASH_FILE).unwrap_or(msg);
            ctx.data_mut(|d| d.insert_temp(crash_id, report));
            ctx.request_repaint();
        }
    }

    /// Shown after a start that crashed without a Rust panic: the start-up steps of that run tell where it stopped.
    fn m_safe_screen(&mut self, ctx: &egui::Context) {
        let last = std::fs::read_to_string(crate::LAST_BOOT_FILE).unwrap_or_default();
        let crash = std::fs::read_to_string(CRASH_FILE).unwrap_or_default();
        let report = format!("Last start stopped after:\n{last}\n{crash}");
        egui::CentralPanel::default().frame(Frame::none().inner_margin(Margin::symmetric(14.0, 18.0))).show(ctx, |ui| {
            pulse_visuals(ui);
            ui.label(RichText::new("Safe mode").size(22.0).strong().color(ink()));
            ui.label(RichText::new("The last start crashed. Sound is off. Send a screenshot of this screen to the developer.").size(13.0).color(muted()));
            ui.add_space(10.0);
            if p_button(ui, "Open Winteria (no sound)", true, vec2(230.0, 40.0)).clicked() {
                let _ = std::fs::remove_file(crate::LAST_BOOT_FILE);
                let _ = std::fs::remove_file(CRASH_FILE);
                crate::leave_safe_mode();
            }
            ui.add_space(10.0);
            let (rect, _) = ui.allocate_exact_size(ui.available_size(), Sense::hover());
            panel(ui.painter(), rect, 14.0);
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink(10.0)));
            mobile_scroll(egui::ScrollArea::both()).auto_shrink([false, false]).show(&mut child, |ui| {
                ui.label(RichText::new(&report).monospace().size(11.0).color(ink()));
            });
        });
    }

    fn m_crash_screen(&mut self, ctx: &egui::Context, report: &str, crash_id: egui::Id) {
        egui::CentralPanel::default().frame(Frame::none().inner_margin(Margin::symmetric(14.0, 18.0))).show(ctx, |ui| {
            pulse_visuals(ui);
            ui.label(RichText::new("Winteria hit an error").size(22.0).strong().color(ink()));
            ui.label(RichText::new("Send a screenshot of this screen to the developer.").size(13.0).color(muted()));
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if p_button(ui, "Continue", true, vec2(130.0, 40.0)).clicked() {
                    let _ = std::fs::remove_file(CRASH_FILE);
                    ctx.data_mut(|d| d.remove::<String>(crash_id));
                }
                if p_button(ui, "Copy", false, vec2(100.0, 40.0)).clicked() {
                    ui.ctx().copy_text(report.to_string());
                }
            });
            ui.add_space(10.0);
            let (rect, _) = ui.allocate_exact_size(ui.available_size(), Sense::hover());
            panel(ui.painter(), rect, 14.0);
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink(10.0)));
            mobile_scroll(egui::ScrollArea::both()).auto_shrink([false, false]).show(&mut child, |ui| {
                ui.label(RichText::new(report).monospace().size(10.5).color(ink()));
            });
        });
    }

    fn mobile_screens(&mut self, ctx: &egui::Context) {
        let first = ctx.cumulative_pass_nr() < 4;
        let step = |s: &str| {
            if first {
                crate::boot_log(s);
            }
        };
        step("background");
        doodle_background(ctx);
        step("header");
        let screen = ctx.screen_rect();
        let portrait = screen.width() < screen.height();
        let full_view = self.m_tab == 1
            && ctx.data(|d| d.get_temp::<bool>(egui::Id::new("m_studio_full_view"))).unwrap_or(false);
        if self.m_tab == 1 {
            if full_view {
                egui::TopBottomPanel::top("m_studio_full_view_toolbar").show_separator_line(false)
                    .frame(Frame::none().fill(bar_fill()).inner_margin(4.0))
                    .show(ctx, |ui| {
                        if p_button(ui, "Exit full view", false, vec2(112.0, 28.0)).clicked() {
                            ctx.data_mut(|d| d.remove::<bool>(egui::Id::new("m_studio_full_view")));
                        }
                    });
            } else if portrait {
                self.m_studio_portrait_header(ctx);
                if self.m_tab == 1 {
                    self.m_studio_dock(ctx);
                }
            } else {
                self.m_studio_sidebar(ctx);
            }
        } else {
            self.mobile_header(ctx);
        }
        step("rail");
        if self.m_tab == 0 {
            self.mobile_rail(ctx, &PROJECT_SECTIONS);
        }
        if self.m_tab == 1 && self.m_sec == SEC_PLAYLIST {
            if !full_view {
                if portrait {
                    self.m_browser_bottom_sheet(ctx);
                } else {
                    self.m_browser_sidebar(ctx);
                }
            }
        } else {
            ctx.data_mut(|d| {
                d.remove::<bool>(egui::Id::new("m_browser_open"));
                d.remove::<(usize, u32)>(egui::Id::new("m_audio_clip_target"));
            });
        }
        step("page");
        let margin = if self.m_tab == 1 && self.m_sec == SEC_PLAYLIST {
            Margin { left: 2.0, right: 2.0, top: 0.0, bottom: 0.0 }
        } else {
            Margin { left: 14.0, right: 14.0, top: 10.0, bottom: 6.0 }
        };
        egui::CentralPanel::default().frame(Frame::none().inner_margin(margin)).show(ctx, |ui| {
            pulse_visuals(ui);
            match self.m_tab {
                0 => self.m_projects(ui),
                1 => self.m_studio(ui),
                2 => self.m_settings(ui, ctx),
                _ => self.m_profile(ui),
            }
        });
        if self.m_tab == 1 && self.m_sec == SEC_PLAYLIST {
            let dragging = egui::DragAndDrop::payload::<browser::DragItem>(ctx)
                .map(|item| item.label(&self.project))
                .or_else(|| egui::DragAndDrop::payload::<crate::fx::FxKind>(ctx).map(|kind| kind.name().to_owned()));
            if let (Some(label), Some(point)) = (dragging, ctx.pointer_interact_pos()) {
                let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("m_browser_drag")));
                let pos = (point + vec2(12.0, -22.0)).clamp(ctx.screen_rect().min, ctx.screen_rect().max);
                let rect = Rect::from_min_size(pos, vec2(118.0, 28.0));
                glass(&painter, rect, 9.0);
                let short: String = label.chars().take(15).collect();
                painter.text(rect.center(), Align2::CENTER_CENTER, short, FontId::proportional(12.0), ink());
            }
        }
        #[cfg(target_os = "android")]
        self.m_sync_orientation(ctx);
        let preview_id = egui::Id::new("m_piano_preview");
        if let Some((until, _, _, _)) = ctx.data(|d| d.get_temp::<(f64, usize, Track, Vec<u8>)>(preview_id)) {
            let remaining = until - ctx.input(|input| input.time);
            if remaining <= 0.0 {
                self.m_stop_note_preview(ctx);
            } else {
                ctx.request_repaint_after(std::time::Duration::from_secs_f64(remaining));
            }
        }
        let piano_visible = self.m_tab == 1 && (self.m_sec == SEC_PIANO
            || ctx.data(|d| d.get_temp::<usize>(egui::Id::new("m_clip_sheet_selection")))
                .and_then(|index| self.project.arrangement.clips.get(index)).is_some_and(|clip| !clip.audio));
        if !piano_visible || !ctx.input(|input| input.pointer.primary_down()) {
            let held_id = egui::Id::new("m_piano_held");
            let held = ctx.data_mut(|d| {
                let held = d.get_temp::<(Track, u8)>(held_id);
                d.remove::<(Track, u8)>(held_id);
                held
            });
            if let Some((track, pitch)) = held {
                self.shared.send(Cmd::NoteOff { track, pitch });
            }
        }
        step("page done");
        let repaint = if self.playing() { 33 } else { 250 };
        ctx.request_repaint_after(std::time::Duration::from_millis(repaint));
    }

    #[cfg(target_os = "android")]
    fn m_sync_orientation(&self, ctx: &egui::Context) {
        let id = egui::Id::new("m_requested_landscape");
        let previous = ctx.data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
        let landscape = self.m_tab == 1
            && ctx.data(|d| d.get_temp::<bool>(egui::Id::new("m_studio_landscape"))).unwrap_or(false);
        if previous == landscape {
            return;
        }
        if let Some(activity) = &self.android_updater {
            ctx.data_mut(|d| d.insert_temp(id, landscape));
            if let Err(error) = activity.request_landscape(landscape) {
                crate::android_log_error(&format!("Cannot change Studio orientation: {error}"));
            }
        }
    }

    // ───────────────────── shell: header and section navigation ─────────────────────

    fn mobile_header(&mut self, ctx: &egui::Context) {
        let resp = egui::TopBottomPanel::top("m_header").show_separator_line(false).frame(Frame::none().fill(bar_fill()).inner_margin(Margin::symmetric(14.0, 10.0))).show(ctx, |ui| {
            pulse_visuals(ui);
            let t = self.m_tab;
            ui.horizontal(|ui| {
                ui.set_min_height(TOUCH);
                if t != 0 && p_button(ui, "‹", false, vec2(34.0, 38.0)).on_hover_text("Back to Projects").clicked() {
                    self.m_tab = 0;
                    self.m_sec = 0;
                }
                let (title, sub) = match t {
                    0 => ("Winteria", format!("v{}  ·  your projects", env!("CARGO_PKG_VERSION"))),
                    2 => ("Settings", "theme, sync and about".to_string()),
                    _ => ("Profile", "Winteria Studio".to_string()),
                };
                ui.vertical(|ui| {
                    ui.label(RichText::new(title).size(22.0).strong().color(ink()));
                    ui.label(RichText::new(sub).size(12.0).color(muted()));
                });
                if t == 0 {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let (r, resp) = ui.allocate_exact_size(vec2(TOUCH, TOUCH), Sense::click());
                        pill(ui.painter(), r, p_accent());
                        icon(ui.painter(), Icon::Plus, r.center(), on_accent());
                        if resp.clicked() {
                            self.new_project();
                            self.m_open_studio(0);
                        }
                        ui.menu_button("⋯", |ui| {
                            for (tab, label) in [(3, "Profile"), (2, "Settings")] {
                                if ui.button(label).clicked() {
                                    self.m_tab = tab;
                                    self.m_sec = 0;
                                    ui.close_menu();
                                }
                            }
                        }).response.on_hover_text("Profile and settings");
                    });
                }
            });
        });
        let r = resp.response.rect;
        ctx.layer_painter(egui::LayerId::background()).line_segment([r.left_bottom(), r.right_bottom()], Stroke::new(1.0, outline()));
    }

    fn m_studio_sidebar(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("m_studio_sidebar").resizable(false).exact_width(52.0)
            .frame(Frame::none().fill(bar_fill()).inner_margin(Margin::symmetric(4.0, 5.0)))
            .show(ctx, |ui| {
                pulse_visuals(ui);
                let width = ui.available_width();
                if self.m_studio_exit(ui, ctx, width) {
                    return;
                }
                self.m_studio_full_view_button(ui, ctx, width);
                self.m_studio_orientation_button(ui, ctx, width);
                ui.separator();
                mobile_scroll(egui::ScrollArea::vertical()).id_salt("m_studio_controls")
                    .auto_shrink([false, false]).show(ui, |ui| self.m_studio_controls(ui, ctx, width, false));
            });
    }

    fn m_studio_portrait_header(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("m_studio_portrait_header").show_separator_line(false)
            .frame(Frame::none().fill(bar_fill()).inner_margin(Margin::symmetric(6.0, 4.0)))
            .show(ctx, |ui| {
                pulse_visuals(ui);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    let width = 32.0;
                    self.m_studio_full_view_button(ui, ctx, width);
                    let folder = self.m_sec == SEC_PLAYLIST;
                    let button_slot = width + ui.spacing().item_spacing.x;
                    let reserved = 45.0 + (2 + usize::from(folder)) as f32 * button_slot;
                    let title_width = (ui.available_width() - reserved).max(1.0);
                    ui.add_sized([title_width, 32.0], egui::Label::new(RichText::new(&self.project.name)
                        .strong().size(14.0).color(ink())).truncate());
                    self.m_studio_play_stop(ui, ctx, width);
                    self.m_studio_tempo(ui, 41.0);
                    if folder {
                        self.m_studio_browser_button(ui, ctx, width);
                    }
                });
            });
    }

    fn m_studio_dock(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("m_studio_port_dock")
            .show_separator_line(false)
            .frame(Frame::none().fill(bar_fill()).inner_margin(Margin { left: 7.0, right: 7.0, top: 6.0, bottom: 12.0 }))
            .show(ctx, |ui| {
                pulse_visuals(ui);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 5.0;
                    let width = 38.0;
                    if self.m_studio_exit(ui, ctx, width) {
                        return;
                    }
                    self.m_studio_orientation_button(ui, ctx, width);
                    ui.separator();
                    mobile_scroll(egui::ScrollArea::horizontal()).id_salt("m_studio_port_controls")
                        .auto_shrink([false, true]).show(ui, |ui| {
                            ui.horizontal(|ui| {
                                self.m_studio_controls(ui, ctx, width, true);
                            });
                        });
                });
            });
    }

    fn m_studio_exit(&mut self, ui: &mut Ui, ctx: &egui::Context, width: f32) -> bool {
        if !studio_icon(ui, width, Icon::Exit, false, "Leave project").clicked() {
            return false;
        }
        self.return_to_start(ctx, false);
        self.m_tab = 0;
        self.m_sec = 0;
        self.hub_cache = None;
        ctx.data_mut(|d| {
            d.remove::<usize>(egui::Id::new("m_clip_sheet_selection"));
            d.remove::<bool>(egui::Id::new("m_track_sheet_open"));
            d.remove::<bool>(egui::Id::new("m_studio_full_view"));
        });
        true
    }

    fn m_studio_full_view_button(&self, ui: &mut Ui, ctx: &egui::Context, width: f32) {
        if studio_icon(ui, width, Icon::FullView, false, "Full view").clicked() {
            ctx.data_mut(|d| d.insert_temp(egui::Id::new("m_studio_full_view"), true));
        }
    }

    fn m_studio_orientation_button(&self, _ui: &mut Ui, _ctx: &egui::Context, _width: f32) {
        #[cfg(target_os = "android")]
        {
            let id = egui::Id::new("m_studio_landscape");
            let landscape = _ctx.data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
            let label = if landscape { "Switch to portrait" } else { "Switch to landscape" };
            if studio_icon(_ui, _width, Icon::Orientation, landscape, label).clicked() {
                _ctx.data_mut(|d| d.insert_temp(id, !landscape));
            }
        }
    }

    fn m_studio_controls(&mut self, ui: &mut Ui, ctx: &egui::Context, width: f32, portrait: bool) {
        let playing = self.playing();
        if studio_icon(ui, width, Icon::Rewind, false, "Return to start").clicked() {
            self.return_to_start(ctx, playing);
        }
        if !portrait {
            self.m_studio_play_stop(ui, ctx, width);
        }
        if studio_icon(ui, width, Icon::Record, self.record, "Arm recording").clicked() {
            self.record = !self.record;
            if self.record && !playing {
                self.m_toggle_play(ctx);
            }
        }
        if !portrait {
            self.m_studio_tempo(ui, width);
        }
        if studio_icon(ui, width, Icon::Share, false, "Export or share").clicked() {
            self.m_export.open = !self.m_export.open;
            self.m_export.song = self.m_sec == SEC_PLAYLIST;
        }
        ui.separator();
        for (index, (title, icon)) in STUDIO_SECTIONS.into_iter().enumerate() {
            if studio_icon(ui, width, icon, self.m_sec == index, title).clicked() && self.m_sec != index {
                self.m_sec = index;
                ctx.data_mut(|d| {
                    d.remove::<usize>(egui::Id::new("m_clip_sheet_selection"));
                    d.remove::<bool>(egui::Id::new("m_track_sheet_open"));
                });
            }
        }
        if !portrait && self.m_sec == SEC_PLAYLIST {
            self.m_studio_browser_button(ui, ctx, width);
        }
    }

    fn m_studio_play_stop(&mut self, ui: &mut Ui, ctx: &egui::Context, width: f32) {
        let play = if self.playing() {
            studio_icon(ui, width, Icon::Pause, true, "Pause")
        } else {
            studio_icon(ui, width, Icon::Play, true, "Play")
        };
        if play.clicked() {
            self.m_toggle_play(ctx);
        }
        if studio_icon(ui, width, Icon::Stop, false, "Stop").clicked() {
            self.return_to_start(ctx, false);
        }
    }

    fn m_studio_tempo(&mut self, ui: &mut Ui, width: f32) {
        let tempo = egui::Button::new(RichText::new(format!("{:.0}", self.project.bpm)).size(12.0))
            .wrap_mode(egui::TextWrapMode::Extend).min_size(vec2(width, 28.0));
        ui.scope(|ui| {
            ui.spacing_mut().button_padding.x = 3.0;
            egui::menu::menu_custom_button(ui, tempo, |ui| {
                ui.add(egui::DragValue::new(&mut self.project.bpm)
                    .range(40.0..=300.0).speed(0.5).suffix(" BPM").max_decimals(1));
            }).response.on_hover_text(format!("Tempo in BPM · {}", self.project.name));
        });
    }

    fn m_studio_browser_button(&self, ui: &mut Ui, ctx: &egui::Context, width: f32) {
        let id = egui::Id::new("m_browser_open");
        let open = ctx.data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
        if studio_icon(ui, width, Icon::Folder, open, "Toggle browser").clicked() {
            ctx.data_mut(|d| {
                d.insert_temp(id, !open);
                if open {
                    d.remove::<(usize, u32)>(egui::Id::new("m_audio_clip_target"));
                }
            });
        }
    }

    /// Section navigation immediately below the header.
    fn mobile_rail(&mut self, ctx: &egui::Context, sections: &[(&str, Icon)]) {
        egui::TopBottomPanel::top("m_rail").show_separator_line(false).frame(Frame::none().inner_margin(Margin { left: 10.0, right: 10.0, top: 4.0, bottom: 6.0 })).show(ctx, |ui| {
            let w = ui.available_width();
            let (rect, _) = ui.allocate_exact_size(vec2(w, 56.0), Sense::hover());
            glass(ui.painter(), rect, 18.0);
            let n = sections.len() as f32;
            let slot = rect.width() / n;
            for (i, (name, ic)) in sections.iter().enumerate() {
                let cell = Rect::from_min_size(pos2(rect.left() + i as f32 * slot, rect.top()), vec2(slot, rect.height()));
                let resp = ui.interact(cell, ui.id().with(("rail", i)), Sense::click());
                let on = self.m_sec == i;
                let c = pos2(cell.center().x, cell.top() + 21.0);
                let p = ui.painter();
                if on {
                    p.circle_filled(c, 16.0, p_accent());
                }
                icon(p, *ic, c, if on { on_accent() } else { ink() });
                p.text(pos2(cell.center().x, cell.bottom() - 9.0), Align2::CENTER_CENTER, *name, FontId::proportional(10.5), if on { ink() } else { muted() });
                if resp.clicked() {
                    self.m_sec = i;
                }
            }
        });
    }

    fn m_open_studio(&mut self, section: usize) {
        self.m_tab = 1;
        self.m_sec = section;
    }

    fn m_toggle_play(&mut self, ctx: &egui::Context) {
        if self.playing() {
            self.shared.send(Cmd::Pause);
        } else {
            // Playlist plays the arrangement. Editors play the selected pattern.
            self.project.song_mode = self.m_sec == SEC_PLAYLIST;
            let looping = self.m_sec != SEC_PIANO
                || ctx.data(|d| d.get_temp::<bool>(egui::Id::new("m_piano_loop"))).unwrap_or(true);
            self.publish_edits();
            self.shared.send(Cmd::PatternLoop(looping));
            self.shared.send(Cmd::Play);
        }
    }

    // ───────────────────── Projects ─────────────────────

    fn m_projects(&mut self, ui: &mut Ui) {
        if self.hub_cache.is_none() {
            self.hub_cache = Some(hub_projects(&self.recent));
        }
        let list = self.hub_cache.clone().unwrap_or_default();
        let mut open = None;
        let mut tpl = None;
        mobile_scroll(egui::ScrollArea::vertical()).auto_shrink([false, false]).show(ui, |ui| {
            // the project in memory
            let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 128.0), Sense::hover());
            glass(ui.painter(), rect, 16.0);
            let p = ui.painter();
            p.text(rect.left_top() + vec2(16.0, 14.0), Align2::LEFT_TOP, "Current project", FontId::proportional(12.0), muted());
            p.text(rect.left_top() + vec2(16.0, 32.0), Align2::LEFT_TOP, &self.project.name, FontId::proportional(20.0), ink());
            p.text(
                rect.left_top() + vec2(16.0, 58.0),
                Align2::LEFT_TOP,
                format!("{:.0} BPM  ·  {} tracks  ·  {} clips", self.project.bpm, self.project.arrangement.tracks.len(), self.project.arrangement.clips.len()),
                FontId::proportional(12.0),
                muted(),
            );
            let br = Rect::from_min_size(rect.left_bottom() + vec2(16.0, -48.0), vec2(150.0, 38.0));
            pill(p, br, p_accent());
            p.text(br.center(), Align2::CENTER_CENTER, "Open in Studio", FontId::proportional(14.0), on_accent());
            if ui.interact(br, ui.id().with("open_studio"), Sense::click()).clicked() {
                self.m_open_studio(0);
            }
            ui.add_space(12.0);

            match self.m_sec {
                3 => {
                    heading(ui, "Sounds");
                    self.m_sounds(ui);
                }
                2 => {
                    heading(ui, "Templates");
                    for (i, (name, _)) in crate::templates::TEMPLATES.iter().enumerate() {
                        if name.starts_with("Empty") {
                            continue;
                        }
                        let letter = name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default();
                        if list_row(ui, lane_col(i), &letter, name, "Template", "", false).clicked() {
                            tpl = Some(i);
                        }
                    }
                }
                s => {
                    let recent = s == 1;
                    heading(ui, if recent { "Recently opened" } else { "All projects" });
                    let items: Vec<(PathBuf, u64)> = if recent {
                        self.recent.iter().filter_map(|p| std::fs::metadata(p).ok().map(|m| (p.clone(), m.len()))).collect()
                    } else {
                        list.iter().map(|(p, _, s)| (p.clone(), *s)).collect()
                    };
                    if items.is_empty() {
                        ui.label(RichText::new(if recent { "No recently opened projects." } else { "No saved projects yet. Tap + or start from a template." }).color(muted()));
                    }
                    for (i, (path, size)) in items.iter().enumerate() {
                        let name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                        let letter = name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default();
                        if list_row(ui, lane_col(i), &letter, &name, &format!("{} KB", size / 1024), "", false).clicked() {
                            open = Some(path.clone());
                        }
                    }
                }
            }
        });
        if let Some(i) = tpl {
            self.load_template(i);
            self.m_open_studio(0);
        }
        if let Some(path) = open {
            self.open_path(path);
            self.m_open_studio(0);
        }
    }

    fn m_sounds(&mut self, ui: &mut Ui) {
        fn walk(dir: &std::path::Path, out: &mut Vec<(String, String)>, depth: usize) {
            let Ok(rd) = std::fs::read_dir(dir) else { return };
            let mut items: Vec<_> = rd.flatten().collect();
            items.sort_by_key(|e| e.file_name());
            for e in items {
                let p = e.path();
                if p.is_dir() && depth < 4 {
                    walk(&p, out, depth + 1);
                } else if p.extension().map_or(false, |x| x.eq_ignore_ascii_case("wav")) && out.len() < 600 {
                    let kit = p.parent().and_then(|d| d.file_name()).map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                    out.push((kit, p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()));
                }
            }
        }
        let id = egui::Id::new("m_sounds_cache");
        let list: Vec<(String, String)> = match ui.data(|d| d.get_temp::<Vec<(String, String)>>(id)) {
            Some(l) => l,
            None => {
                let mut l = Vec::new();
                walk(std::path::Path::new("samples"), &mut l, 0);
                ui.data_mut(|d| d.insert_temp(id, l.clone()));
                l
            }
        };
        if list.is_empty() {
            ui.label(RichText::new("No sounds found yet.").color(muted()));
        }
        for (i, (kit, name)) in list.iter().enumerate() {
            let letter = name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default();
            list_row(ui, lane_col(i), &letter, name, kit, "WAV", false);
        }
    }

    // ───────────────────── Studio ─────────────────────

    fn m_studio(&mut self, ui: &mut Ui) {
        match self.m_sec {
            SEC_PLAYLIST => self.m_playlist(ui),
            1 => self.m_steps(ui),
            SEC_PIANO => self.m_piano(ui),
            3 => self.m_session(ui),
            4 => self.m_mixer(ui),
            _ => self.m_tracks(ui),
        }
        self.m_step_sheet(ui.ctx());
        self.m_export_sheet(ui.ctx());
        self.m_clip_sheet(ui.ctx());
        self.m_track_sheet(ui.ctx());
    }

    /// Pattern chips: tap to switch the pattern that the Steps and Piano screens edit.
    fn m_pattern_chips(&mut self, ui: &mut Ui) {
        let pats: Vec<(usize, String)> = self.project.patterns.iter().enumerate().filter(|(_, p)| !p.clip_only).map(|(i, p)| (i, p.name.clone())).collect();
        mobile_scroll(egui::ScrollArea::horizontal()).id_salt("m_pat_chips").auto_shrink([false, true]).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                for (i, name) in &pats {
                    let short: String = name.chars().take(14).collect();
                    let w = 24.0 + short.chars().count() as f32 * 8.0;
                    if p_button(ui, &short, self.project.current_pattern == *i, vec2(w, 34.0)).clicked() {
                        self.project.current_pattern = *i;
                    }
                }
            });
        });
        ui.add_space(8.0);
    }

    /// Drum step sequencer: one glass card per drum, two rows of 8 big steps per 16-step page.
    fn m_steps(&mut self, ui: &mut Ui) {
        self.m_pattern_chips(ui);
        let pi = self.project.current_pattern;
        if pi >= self.project.patterns.len() {
            ui.label(RichText::new("No pattern yet. Start from a template in Projects.").color(muted()));
            return;
        }
        let len = self.project.patterns[pi].length_steps.max(16) as usize;
        let pages = (len + 15) / 16;
        let page_id = egui::Id::new("m_steps_page");
        let mut page: usize = ui.data(|d| d.get_temp(page_id)).unwrap_or(0).min(pages - 1);
        if pages > 1 {
            let names: Vec<String> = (0..pages).map(|p| format!("{}-{}", p * 16 + 1, p * 16 + 16)).collect();
            ui.horizontal(|ui| {
                for (i, n) in names.iter().enumerate() {
                    if p_button(ui, n, page == i, vec2(70.0, 32.0)).clicked() {
                        page = i;
                    }
                }
            });
            ui.data_mut(|d| d.insert_temp(page_id, page));
            ui.add_space(6.0);
        }
        let playing = self.playing() && !self.project.song_mode;
        let play_step = (self.shared.position.load(Ordering::Relaxed) / TICKS_PER_STEP) as usize % len;
        mobile_scroll(egui::ScrollArea::vertical()).auto_shrink([false, false]).show(ui, |ui| {
            for lane in 0..NUM_DRUMS {
                let w = ui.available_width();
                let cell = ((w - 24.0 - 7.0 * 6.0 - 6.0) / 8.0).clamp(28.0, 64.0);
                let h = 34.0 + 2.0 * cell + 8.0 + 14.0;
                let (rect, _) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
                panel(ui.painter(), rect, 16.0);
                let col = lane_col(lane);
                ui.painter().circle_filled(rect.left_top() + vec2(20.0, 18.0), 5.0, col);
                ui.painter().text(rect.left_top() + vec2(32.0, 18.0), Align2::LEFT_CENTER, DRUM_NAMES[lane], FontId::proportional(15.0), ink());
                for k in 0..16 {
                    let s = page * 16 + k;
                    if s >= len {
                        break;
                    }
                    let (row, colm) = (k / 8, k % 8);
                    let x = rect.left() + 12.0 + colm as f32 * (cell + 6.0) + if colm >= 4 { 6.0 } else { 0.0 };
                    let y = rect.top() + 34.0 + row as f32 * (cell + 8.0);
                    let r = Rect::from_min_size(pos2(x, y), vec2(cell, cell));
                    let resp = ui.interact(r, ui.id().with(("step", lane, s)), Sense::click());
                    if resp.long_touched() || resp.secondary_clicked() {
                        ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("m_step_edit"), (pi, lane, s)));
                    }
                    let on = self.project.patterns[pi].drums[lane][s].on;
                    let p = ui.painter();
                    let st = self.project.patterns[pi].drums[lane][s];
                    if on {
                        p.rect_filled(r, 10.0, col.linear_multiply(0.45 + 0.55 * st.vel as f32 / 127.0));
                        if st.is_smart() {
                            p.circle_filled(r.right_top() + vec2(-7.0, 7.0), 3.0, Color32::WHITE);
                        }
                    } else {
                        glass(p, r, 10.0);
                    }
                    if playing && play_step == s {
                        p.rect_stroke(r.expand(2.0), 12.0, Stroke::new(2.0, p_accent()));
                    }
                    if resp.clicked() && !resp.long_touched() {
                        let st = &mut self.project.patterns[pi].drums[lane][s];
                        st.on = !st.on;
                        if st.on && st.vel == 0 {
                            st.vel = 100;
                        }
                    }
                }
                ui.add_space(10.0);
            }
        });
    }

    fn m_piano(&mut self, ui: &mut Ui) {
        self.m_pattern_chips(ui);
        self.m_piano_editor(ui, self.project.current_pattern, None);
    }

    fn m_piano_editor(&mut self, ui: &mut Ui, pi: usize, drawer: Option<(egui::Id, bool)>) -> bool {
        let oct_id = egui::Id::new("m_piano_oct");
        let chord_id = egui::Id::new("m_piano_chords");
        let loop_id = egui::Id::new("m_piano_loop");
        let mut oct = ui.data(|d| d.get_temp::<u8>(oct_id)).unwrap_or(3);
        let mut chords = ui.data(|d| d.get_temp::<bool>(chord_id)).unwrap_or(false);
        let mut looping = ui.data(|d| d.get_temp::<bool>(loop_id)).unwrap_or(true);
        let names = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
        let valid_pattern = pi < self.project.patterns.len();
        let screen = ui.ctx().screen_rect();
        let portrait = screen.width() < screen.height();
        let mut close = false;
        mobile_scroll(egui::ScrollArea::horizontal())
            .id_salt(("m_piano_toolbar", drawer.is_some(), portrait))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if let Some((full_id, full)) = drawer {
                        close = p_button(ui, "Done", true, vec2(60.0, 32.0)).clicked();
                        if p_button(ui, if full { "Restore" } else { "Full view" }, false, vec2(86.0, 32.0)).clicked() {
                            ui.data_mut(|d| d.insert_temp(full_id, !full));
                        }
                    }
                    if !valid_pattern {
                        return;
                    }
                    if p_button(ui, if self.playing() { "Pause" } else { "Play" }, true, vec2(62.0, 32.0)).clicked() {
                        if self.playing() && !self.project.song_mode {
                            self.shared.send(Cmd::Pause);
                        } else {
                            if self.playing() {
                                self.shared.send(Cmd::Stop);
                            }
                            self.project.song_mode = false;
                            self.project.current_pattern = pi;
                            self.publish_edits();
                            self.shared.send(Cmd::PatternLoop(looping));
                            self.shared.send(Cmd::Play);
                        }
                    }
                    if p_button(ui, "Stop", false, vec2(60.0, 32.0)).clicked() {
                        self.return_to_start(ui.ctx(), false);
                    }
                    if p_button(ui, "Oct −", false, vec2(62.0, 32.0)).clicked() {
                        oct = oct.saturating_sub(1);
                    }
                    if p_button(ui, "Oct +", false, vec2(62.0, 32.0)).clicked() {
                        oct = (oct + 1).min(7);
                    }
                    ui.menu_button(format!("Key {}", names[(self.project.key_root % 12) as usize]), |ui| {
                        for (index, name) in names.iter().enumerate() {
                            if ui.selectable_value(&mut self.project.key_root, index as u8, *name).clicked() {
                                ui.close_menu();
                            }
                        }
                    });
                    let scale = self.project.key_scale.min(crate::theory::CHROMATIC);
                    ui.menu_button(crate::theory::SCALES[scale].0, |ui| {
                        for (index, (name, _)) in crate::theory::SCALES.iter().enumerate() {
                            if ui.selectable_value(&mut self.project.key_scale, index, *name).clicked() {
                                ui.close_menu();
                            }
                        }
                    });
                    if p_button(ui, "Chords", chords, vec2(76.0, 32.0)).clicked() {
                        chords = !chords;
                    }
                    if p_button(ui, if looping { "Loop on" } else { "Loop off" }, looping, vec2(87.0, 32.0)).clicked() {
                        looping = !looping;
                        self.shared.send(Cmd::PatternLoop(looping));
                    }
                });
            });
        if !valid_pattern {
            ui.label(RichText::new("No pattern selected.").color(muted()));
            return close;
        }
        ui.data_mut(|d| {
            d.insert_temp(oct_id, oct);
            d.insert_temp(chord_id, chords);
            d.insert_temp(loop_id, looping);
        });
        ui.add_space(5.0);
        let (key_root, key_scale) = (self.project.key_root, self.project.key_scale);
        let base = (oct + 1) * 12;
        let keys = (128usize - base as usize).min(48);
        let kw = 34.0f32;
        let row_h = 28.0f32;
        let kb_h = (ui.available_height() * 0.22).clamp(34.0, 64.0);
        let steps = self.project.patterns[pi].length_steps.max(1) as usize;
        let track = self.project.arrangement.tracks.get(self.arr.sel_lane).and_then(|track| track.instrument)
            .or_else(|| self.project.patterns[pi].notes.first().map(|note| note.track)).unwrap_or(Track::Synth);
        let playing = self.playing() && !self.project.song_mode;
        let pos = self.shared.position.load(Ordering::Relaxed);
        let play_step = (pos / TICKS_PER_STEP) as usize % steps;
        let grid_h = (ui.available_height() - kb_h - 6.0).max(1.0);
        let out = mobile_scroll(egui::ScrollArea::both()).id_salt(("m_piano_grid", pi)).max_height(grid_h).auto_shrink([false, false]).show(ui, |ui| {
            let (rect, resp) = ui.allocate_exact_size(vec2(keys as f32 * kw, steps as f32 * row_h), Sense::click());
            let p = ui.painter_at(rect);
            panel(&p, rect, 12.0);
            for k in 0..keys {
                let pitch = base + k as u8;
                let col_r = Rect::from_min_size(pos2(rect.left() + k as f32 * kw, rect.top()), vec2(kw, rect.height()));
                if crate::theory::is_root(key_root, pitch) {
                    p.rect_filled(col_r, 0.0, p_accent().linear_multiply(0.10));
                } else if !crate::theory::in_scale(key_root, key_scale, pitch) {
                    p.rect_filled(col_r, 0.0, pick(0x30000000, 0x1E000000));
                } else if matches!(pitch % 12, 1 | 3 | 6 | 8 | 10) {
                    p.rect_filled(col_r, 0.0, pick(0x14000000, 0x0C000000));
                }
                let edge = if pitch % 12 == 0 { Stroke::new(1.0, outline()) } else { Stroke::new(0.4, glass_edge().linear_multiply(0.5)) };
                p.line_segment([col_r.left_top(), col_r.left_bottom()], edge);
            }
            // bar / beat numbers stay at the left edge of what is visible
            let vis_left = ui.clip_rect().left().max(rect.left());
            for s in 0..steps {
                let y = rect.top() + s as f32 * row_h;
                let stroke = if s % 16 == 0 { Stroke::new(1.0, outline()) } else if s % 4 == 0 { Stroke::new(0.7, glass_edge()) } else { Stroke::new(0.4, glass_edge().linear_multiply(0.5)) };
                p.line_segment([pos2(rect.left(), y), pos2(rect.right(), y)], stroke);
                if playing && play_step == s {
                    p.rect_filled(Rect::from_min_size(pos2(rect.left(), y), vec2(rect.width(), row_h)), 0.0, p_accent().linear_multiply(0.15));
                }
                if s % 4 == 0 {
                    p.text(pos2(vis_left + 4.0, y + 3.0), Align2::LEFT_TOP, format!("{}.{}", s / 16 + 1, (s % 16) / 4 + 1), FontId::proportional(10.0), muted());
                }
            }
            let mut remove = None;
            let mut resize = None;
            let pattern_end = steps as u32 * TICKS_PER_STEP;
            for (index, note) in self.project.patterns[pi].notes.iter().enumerate() {
                if note.pitch < base || note.pitch >= base + keys as u8 || note.start >= pattern_end {
                    continue;
                }
                let x = rect.left() + (note.pitch - base) as f32 * kw;
                let y = rect.top() + note.start as f32 / TICKS_PER_STEP as f32 * row_h;
                let body = |len: u32| {
                    let h = (len as f32 / TICKS_PER_STEP as f32 * row_h).max(6.0);
                    Rect::from_min_size(pos2(x + 2.0, y + 1.0), vec2(kw - 4.0, h - 2.0))
                };
                let original = body(note.len);
                let id = resp.id.with(("note", index));
                let note_response = ui.interact(original, id, Sense::click());
                let edge = Rect::from_min_max(
                    pos2(original.left(), original.bottom() - original.height().min(14.0)),
                    original.right_bottom());
                let handle = ui.interact(edge, id.with("length"), Sense::click_and_drag())
                    .on_hover_cursor(egui::CursorIcon::ResizeVertical);
                let max_len = pattern_end - note.start;
                let mut len = note.len;
                let grab_id = id.with("grab");
                if handle.drag_started() {
                    if let Some(origin) = ui.input(|input| input.pointer.press_origin()) {
                        ui.data_mut(|d| d.insert_temp(grab_id, origin.y));
                    }
                }
                if handle.dragged() || handle.drag_stopped() {
                    if let (Some(origin), Some(pointer)) = (
                        ui.data(|d| d.get_temp::<f32>(grab_id)), handle.interact_pointer_pos()
                    ) {
                        let delta = ((pointer.y - origin) / row_h).round() as i64 * i64::from(TICKS_PER_STEP);
                        len = (i64::from(note.len) + delta)
                            .clamp(i64::from(TICKS_PER_STEP.min(max_len)), i64::from(max_len)) as u32;
                    }
                    if handle.drag_stopped() {
                        ui.data_mut(|d| d.remove::<f32>(grab_id));
                        if len != note.len {
                            resize = Some((index, len));
                        }
                    }
                }
                let visual = body(len);
                let tint = lane_col(3);
                p.rect_filled(visual, 5.0, tint);
                let grip = Rect::from_center_size(visual.center_bottom() - vec2(0.0, 3.0), vec2(12.0, 2.0));
                p.rect_filled(grip, 1.0, Color32::WHITE.linear_multiply(0.8));
                if note_response.clicked() || handle.clicked() {
                    remove = Some(index);
                }
            }
            if let Some((index, len)) = resize {
                self.project.patterns[pi].notes[index].len = len;
            } else if let Some(index) = remove {
                self.project.patterns[pi].notes.remove(index);
            } else if resp.clicked() {
                if let Some(pt) = resp.interact_pointer_pos() {
                    let k = (((pt.x - rect.left()) / kw) as usize).min(keys - 1);
                    let s = (((pt.y - rect.top()) / row_h) as usize).min(steps - 1) as u32;
                    let pitch = base + k as u8;
                    let tick = s * TICKS_PER_STEP;
                    let mut pitches = if chords {
                        let tone = crate::theory::snap_to_scale(key_root, key_scale, pitch);
                        crate::theory::stack_chord(key_root, key_scale, tone, 3)
                    } else {
                        vec![pitch]
                    };
                    let len = (if chords { TICKS_PER_STEP * 4 } else { TICKS_PER_STEP }).min(pattern_end - tick);
                    let notes = &mut self.project.patterns[pi].notes;
                    pitches.retain(|&pitch| {
                        if notes.iter().any(|note| note.pitch == pitch
                            && note.start <= tick && tick < note.start.saturating_add(note.len))
                        {
                            return false;
                        }
                        let len = notes.iter().filter(|note| note.pitch == pitch && note.start > tick)
                            .fold(len, |len, note| len.min(note.start - tick));
                        notes.push(crate::piano_roll::Note { start: tick, len, pitch, vel: 100, track, slide: false, chance: 100, pan: 0, mute: false });
                        true
                    });
                    if !pitches.is_empty() {
                        self.m_preview_piano_notes(ui.ctx(), track, pitches);
                    }
                }
            }
        });
        let off_x = out.state.offset.x;
        ui.add_space(6.0);
        // ---- keyboard along the bottom: follows the grid sideways, press a key to hear it ----
        let held_id = egui::Id::new("m_piano_held");
        let mut held = ui.data(|d| d.get_temp::<(Track, u8)>(held_id));
        let (viewport, resp) = ui.allocate_exact_size(vec2(ui.available_width(), kb_h), Sense::click_and_drag());
        let rect = Rect::from_min_size(viewport.min - vec2(off_x, 0.0), vec2(keys as f32 * kw, kb_h));
        let p = ui.painter_at(viewport);
        p.rect_filled(rect, 10.0, pick(0xFFE9EDF1, 0xFFFFFFFF));
        let pressed = if resp.is_pointer_button_down_on() {
            resp.interact_pointer_pos().filter(|pt| rect.contains(*pt) && viewport.contains(*pt))
                .map(|pt| base + (((pt.x - rect.left()) / kw) as usize).min(keys - 1) as u8)
        } else {
            None
        };
        for k in 0..keys {
            let pitch = base + k as u8;
            let r = Rect::from_min_size(pos2(rect.left() + k as f32 * kw, rect.top()), vec2(kw, kb_h));
            let black = matches!(pitch % 12, 1 | 3 | 6 | 8 | 10);
            let down = pressed == Some(pitch);
            if black {
                let br = Rect::from_min_size(r.min + vec2(2.0, 0.0), vec2(kw - 4.0, kb_h * 0.72));
                p.rect_filled(br, 4.0, if down { lane_col(3) } else { pick(0xFF14171B, 0xFF2A3038) });
            } else if down {
                p.rect_filled(r.shrink(1.0), 4.0, lane_col(3).linear_multiply(0.8));
            }
            if !black {
                p.line_segment([r.left_top(), r.left_bottom()], Stroke::new(1.0, Color32::from_gray(170)));
            }
            if pitch % 12 == 0 {
                p.text(pos2(r.center().x, r.bottom() - 10.0), Align2::CENTER_CENTER, format!("C{}", pitch / 12 - 1), FontId::proportional(10.0), Color32::from_gray(60));
            }
        }
        let pressed = pressed.map(|pitch| (track, pitch));
        if pressed != held {
            if let Some((track, pitch)) = held {
                self.shared.send(Cmd::NoteOff { track, pitch });
            }
            if let Some((track, pitch)) = pressed {
                self.m_stop_note_preview(ui.ctx());
                self.shared.send(Cmd::SetLiveLane(self.arr.sel_lane));
                self.shared.send(Cmd::NoteOn { track, pitch, vel: 100 });
            }
            held = pressed;
        }
        ui.data_mut(|d| {
            if let Some(held) = held {
                d.insert_temp(held_id, held);
            } else {
                d.remove::<(Track, u8)>(held_id);
            }
        });
        close
    }

    fn m_preview_piano_notes(&self, ctx: &egui::Context, track: Track, pitches: Vec<u8>) {
        self.m_stop_note_preview(ctx);
        let lane = self.arr.sel_lane;
        self.shared.send(Cmd::SetLiveLane(lane));
        for &pitch in &pitches {
            self.shared.send(Cmd::NoteOn { track, pitch, vel: 100 });
        }
        let until = ctx.input(|input| input.time) + 0.17;
        ctx.data_mut(|d| d.insert_temp(egui::Id::new("m_piano_preview"), (until, lane, track, pitches)));
    }

    fn m_stop_note_preview(&self, ctx: &egui::Context) {
        let id = egui::Id::new("m_piano_preview");
        let preview = ctx.data_mut(|d| {
            let preview = d.get_temp::<(f64, usize, Track, Vec<u8>)>(id);
            d.remove::<(f64, usize, Track, Vec<u8>)>(id);
            preview
        });
        if let Some((_, lane, track, pitches)) = preview {
            self.shared.send(Cmd::SetLiveLane(lane));
            for pitch in pitches {
                self.shared.send(Cmd::NoteOff { track, pitch });
            }
        }
    }

    fn m_clip_sheet(&mut self, ctx: &egui::Context) {
        let id = egui::Id::new("m_clip_sheet_selection");
        let animation = egui::Id::new("m_piano_drawer_animation");
        let full = egui::Id::new("m_piano_drawer_full");
        let index = ctx.data(|d| d.get_temp::<usize>(id)).filter(|&i| self.arr.sel_clip == Some(i));
        let clip = index.and_then(|i| self.project.arrangement.clips.get(i).copied());
        let (Some(index), Some(clip)) = (index, clip) else {
            ctx.animate_bool(animation, false);
            ctx.data_mut(|d| {
                d.remove::<usize>(id);
                d.remove::<bool>(full);
            });
            return;
        };
        let done = if clip.audio {
            ctx.animate_bool(animation, false);
            mobile_editor_sheet(ctx, "m_clip_sheet_window", "Audio clip", |ui| {
                mobile_scroll(egui::ScrollArea::vertical()).id_salt("m_audio_clip_controls").show(ui, |ui| {
                    arrangement_ui::clip_options(ui, &mut self.project, &mut self.arr, &mut self.lib, index, clip.start);
                });
            })
        } else {
            self.m_piano_drawer(ctx, clip.pattern, animation, full)
        };
        if done {
            ctx.data_mut(|d| {
                d.remove::<usize>(id);
                d.remove::<bool>(full);
            });
            ctx.request_repaint();
        }
    }

    fn m_piano_drawer(&mut self, ctx: &egui::Context, pattern: usize, animation: egui::Id, full_id: egui::Id) -> bool {
        let shown = ctx.animate_bool(animation, true);
        let full = ctx.data(|d| d.get_temp::<bool>(full_id)).unwrap_or(false);
        let screen = ctx.screen_rect();
        let portrait = screen.width() < screen.height();
        let bottom_sheet = portrait && !full;
        let width = if full || portrait {
            screen.width()
        } else {
            (screen.width() * 0.74).max(screen.width().min(320.0))
        }.max(1.0);
        let height = (screen.height() * if bottom_sheet { 0.76 } else { 1.0 }).max(1.0);
        let open_at = if portrait {
            pos2(screen.left(), screen.bottom() - shown * height)
        } else {
            pos2(screen.left() - (1.0 - shown) * width, screen.top())
        };
        let mut close = false;
        egui::Area::new(egui::Id::new("m_piano_drawer"))
            .fixed_pos(open_at).constrain(false).movable(false).order(egui::Order::Foreground)
            .show(ctx, |ui| {
                let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
                ui.painter().rect_filled(rect, if bottom_sheet { 12.0 } else { 0.0 }, p_bg());
                if !portrait {
                    ui.painter().line_segment([rect.right_top(), rect.right_bottom()], Stroke::new(1.0, outline()));
                }
                let content = rect.shrink2(vec2(9.0, 7.0));
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(content));
                child.set_clip_rect(rect.intersect(screen));
                pulse_visuals(&mut child);
                if bottom_sheet {
                    let (handle, grip) = child.allocate_exact_size(vec2(child.available_width(), 14.0), Sense::drag());
                    let mark = Rect::from_center_size(handle.center(), vec2(34.0, 3.0));
                    child.painter().rect_filled(mark, 2.0, muted());
                    close = grip.drag_delta().y > 18.0;
                }
                close |= self.m_piano_editor(&mut child, pattern, Some((full_id, full)));
            });
        close || ctx.input(|input| input.key_pressed(egui::Key::Escape))
    }

    fn m_track_sheet(&mut self, ctx: &egui::Context) {
        let id = egui::Id::new("m_track_sheet_open");
        if !ctx.data(|d| d.get_temp::<bool>(id)).unwrap_or(false) {
            return;
        }
        let Some(track) = self.project.arrangement.tracks.get(self.arr.sel_lane) else {
            ctx.data_mut(|d| d.remove::<bool>(id));
            return;
        };
        let title = format!("Track Settings · {}", track.name);
        if mobile_editor_sheet(ctx, "m_track_sheet_window", &title, |ui| {
            let mut detail = arrangement_ui::DetailCtx {
                roll_track: &mut self.roll.track,
                live_track: &mut self.live_track,
                inst: &mut self.inst,
            };
            devices_ui::mobile_track_settings(ui, &mut self.project, &mut self.arr, &mut detail,
                &mut self.plugin_window, &mut self.lib, &self.shared);
        }) {
            ctx.data_mut(|d| d.remove::<bool>(id));
        }
    }

    /// Clip launcher in portrait: a row per scene, a column per track. Tap a clip to launch it on the next bar, tap a
    /// running clip to stop its track; tap an empty MIDI cell to put the current pattern there; hold a clip to remove it.
    fn m_session(&mut self, ui: &mut Ui) {
        use crate::audio_engine::QUEUED_STOP;
        use crate::session::MAX_LANES;
        let lanes = self.project.arrangement.tracks.len().min(MAX_LANES);
        ui.horizontal(|ui| {
            if p_button(ui, "Stop all", true, vec2(100.0, 36.0)).clicked() {
                self.shared.send(Cmd::StopAllClips);
            }
            if p_button(ui, "+ Scene", false, vec2(90.0, 36.0)).clicked() {
                self.project.session.add_scene();
            }
            if p_button(ui, "- Scene", false, vec2(90.0, 36.0)).clicked() {
                self.project.session.remove_scene();
            }
        });
        ui.label(RichText::new("Tap: launch / stop  ·  empty cell: put the current pattern  ·  hold: remove").size(11.0).color(muted()));
        ui.add_space(6.0);
        if lanes == 0 {
            ui.label(RichText::new("No tracks yet.").color(muted()));
            return;
        }
        let blink = ((ui.input(|i| i.time) * 3.0) as i64) % 2 == 0;
        let (cell_w, cell_h, head_w, head_h) = (86.0f32, 52.0f32, 48.0f32, 30.0f32);
        let scenes = self.project.session.scenes;
        mobile_scroll(egui::ScrollArea::both()).id_salt("m_session").auto_shrink([false, false]).show(ui, |ui| {
            let (rect, _) = ui.allocate_exact_size(vec2(head_w + lanes as f32 * (cell_w + 6.0), head_h + scenes as f32 * (cell_h + 6.0)), Sense::hover());
            // track names across the top
            for l in 0..lanes {
                let x = rect.left() + head_w + l as f32 * (cell_w + 6.0);
                let t = &self.project.arrangement.tracks[l];
                let name: String = t.name.chars().take(9).collect();
                ui.painter().circle_filled(pos2(x + 8.0, rect.top() + 12.0), 4.0, lane_col(l));
                ui.painter().text(pos2(x + 16.0, rect.top() + 12.0), Align2::LEFT_CENTER, name, FontId::proportional(12.0), ink());
            }
            for sc in 0..scenes {
                let y = rect.top() + head_h + sc as f32 * (cell_h + 6.0);
                // scene launcher
                let sr = Rect::from_min_size(pos2(rect.left(), y), vec2(head_w - 8.0, cell_h));
                let sresp = ui.interact(sr, ui.id().with(("scene", sc)), Sense::click());
                glass(ui.painter(), sr, 12.0);
                icon(ui.painter(), Icon::Play, sr.center() + vec2(1.0, -6.0), ink());
                ui.painter().text(sr.center() + vec2(0.0, 13.0), Align2::CENTER_CENTER, format!("{}", sc + 1), FontId::proportional(11.0), muted());
                if sresp.clicked() {
                    for c in self.project.session.clips.iter().filter(|c| c.scene == sc) {
                        self.shared.send(Cmd::LaunchClip { lane: c.lane, pattern: c.pattern, audio: c.audio });
                    }
                }
                for l in 0..lanes {
                    let r = Rect::from_min_size(pos2(rect.left() + head_w + l as f32 * (cell_w + 6.0), y), vec2(cell_w, cell_h));
                    let resp = ui.interact(r, ui.id().with(("slot", l, sc)), Sense::click());
                    let clip = self.project.session.get(l, sc).copied();
                    let p = ui.painter();
                    match clip {
                        Some(c) => {
                            let code = c.code();
                            let running = self.shared.session_run[l].load(Ordering::Relaxed) == code;
                            let queue = self.shared.session_queue[l].load(Ordering::Relaxed);
                            let queued = queue == code || (running && queue == QUEUED_STOP);
                            let col = lane_col(l);
                            p.rect_filled(r, 12.0, col.linear_multiply(if running { 0.95 } else { 0.55 }));
                            if running || (queued && blink) {
                                p.rect_stroke(r.expand(1.5), 13.0, Stroke::new(2.5, p_accent()));
                            }
                            let name = if c.audio { self.project.audio_pool.get(c.pattern).map(|s| s.name.clone()).unwrap_or_default() } else { self.project.patterns.get(c.pattern).map(|p| p.name.clone()).unwrap_or_default() };
                            let short: String = name.chars().take(10).collect();
                            icon(p, if running { Icon::Stop } else { Icon::Play }, r.left_top() + vec2(14.0, 16.0), Color32::WHITE);
                            p.text(pos2(r.left() + 8.0, r.bottom() - 13.0), Align2::LEFT_CENTER, short, FontId::proportional(11.5), Color32::WHITE);
                            if resp.long_touched() || resp.secondary_clicked() {
                                self.project.session.clear(l, sc);
                            } else if resp.clicked() {
                                if running {
                                    self.shared.send(Cmd::StopLane { lane: l });
                                } else {
                                    self.shared.send(Cmd::LaunchClip { lane: l, pattern: c.pattern, audio: c.audio });
                                }
                            }
                        }
                        None => {
                            p.rect_filled(r, 12.0, p_bg());
                            p.rect_stroke(r, 12.0, Stroke::new(1.0, glass_edge()));
                            let midi = self.project.arrangement.tracks[l].kind != crate::arrangement::TrackKind::Audio;
                            if midi {
                                icon(p, Icon::Plus, r.center(), muted().linear_multiply(0.6));
                                if resp.clicked() && self.project.current_pattern < self.project.patterns.len() {
                                    self.project.session.set(l, sc, self.project.current_pattern, false);
                                }
                            }
                        }
                    }
                }
            }
        });
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(150));
    }

    /// Smart Step editor: hold a step to open it (velocity, pitch, pan, chance, slide).
    fn m_step_sheet(&mut self, ctx: &egui::Context) {
        let id = egui::Id::new("m_step_edit");
        let Some((pi, lane, s)) = ctx.data(|d| d.get_temp::<(usize, usize, usize)>(id)) else { return };
        if pi >= self.project.patterns.len() || lane >= NUM_DRUMS {
            ctx.data_mut(|d| d.remove::<(usize, usize, usize)>(id));
            return;
        }
        let mut close = false;
        let w = (ctx.screen_rect().width() - 24.0).min(420.0);
        egui::Area::new(egui::Id::new("m_step_sheet")).order(egui::Order::Foreground).anchor(Align2::CENTER_BOTTOM, [0.0, -150.0]).show(ctx, |ui| {
            let (rect, _) = ui.allocate_exact_size(vec2(w, 372.0), Sense::click());
            ui.painter().rect_filled(rect, 20.0, p_bg());
            glass(ui.painter(), rect, 20.0);
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink(16.0)));
            pulse_visuals(&mut child);
            let ui = &mut child;
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{}  ·  step {}", DRUM_NAMES[lane], s + 1)).size(17.0).strong().color(ink()));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if p_button(ui, "Done", true, vec2(80.0, 34.0)).clicked() {
                        close = true;
                    }
                });
            });
            ui.add_space(8.0);
            ui.spacing_mut().slider_width = ui.available_width() - 90.0;
            let st = &mut self.project.patterns[pi].drums[lane][s];
            st.on = true;
            let mut vel = st.vel as i32;
            let mut pitch = st.pitch as i32;
            let mut pan = st.pan as i32;
            let mut prob = st.prob as i32;
            for (label, v, range) in [("Velocity", &mut vel, 1..=127), ("Pitch", &mut pitch, -24..=24), ("Pan", &mut pan, -100..=100), ("Chance %", &mut prob, 1..=100)] {
                ui.label(RichText::new(label).size(12.0).color(muted()));
                ui.add(egui::Slider::new(v, range));
                ui.add_space(4.0);
            }
            st.vel = vel as u8;
            st.pitch = pitch as i8;
            st.pan = pan as i8;
            st.prob = prob as u8;
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if p_button(ui, "Slide", st.slide, vec2(90.0, 34.0)).clicked() {
                    st.slide = !st.slide;
                }
                if p_button(ui, "Reset", false, vec2(90.0, 34.0)).clicked() {
                    *st = crate::sequencer::Step::new(true, 100);
                }
                if p_button(ui, "Clear", false, vec2(90.0, 34.0)).clicked() {
                    st.on = false;
                    close = true;
                }
            });
        });
        if close {
            ctx.data_mut(|d| d.remove::<(usize, usize, usize)>(id));
        }
    }

    /// Export & Share: render the song (Playlist) or the current pattern to WAV, then share it.
    fn m_export_sheet(&mut self, ctx: &egui::Context) {
        // collect a finished render
        if let Some(rx) = &self.m_export.rx {
            if let Ok(res) = rx.try_recv() {
                self.m_export.rx = None;
                match res {
                    Ok(path) => {
                        self.m_export.status = format!("Saved: {}", path.display());
                        self.m_export.last = Some(path.clone());
                        if self.m_export.share_after {
                            self.m_share(&path);
                        }
                    }
                    Err(e) if e.contains("silent") => self.m_export.status = "Nothing to export yet: this is silent. Add some steps or notes first.".into(),
                    Err(e) => self.m_export.status = format!("Export failed: {e}"),
                }
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
            }
        }
        if !self.m_export.open {
            return;
        }
        let w = (ctx.screen_rect().width() - 24.0).min(420.0);
        let busy = self.m_export.rx.is_some();
        egui::Area::new(egui::Id::new("m_export_sheet")).order(egui::Order::Foreground).anchor(Align2::CENTER_BOTTOM, [0.0, -150.0]).show(ctx, |ui| {
            let (rect, _) = ui.allocate_exact_size(vec2(w, 250.0), Sense::click());
            ui.painter().rect_filled(rect, 20.0, p_bg());
            glass(ui.painter(), rect, 20.0);
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink(16.0)));
            pulse_visuals(&mut child);
            let ui = &mut child;
            ui.horizontal(|ui| {
                ui.label(RichText::new("Export & share").size(18.0).strong().color(ink()));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if p_button(ui, "Close", false, vec2(80.0, 34.0)).clicked() {
                        self.m_export.open = false;
                    }
                });
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let half = (ui.available_width() - 8.0) / 2.0;
                if p_button(ui, "Whole song", self.m_export.song, vec2(half, 36.0)).clicked() {
                    self.m_export.song = true;
                }
                if p_button(ui, "This pattern", !self.m_export.song, vec2(half, 36.0)).clicked() {
                    self.m_export.song = false;
                }
            });
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                let half = (ui.available_width() - 8.0) / 2.0;
                if p_button(ui, if busy { "Rendering..." } else { "Export & share" }, true, vec2(half, 42.0)).clicked() && !busy {
                    self.m_start_export(true);
                }
                if p_button(ui, "Save WAV", false, vec2(half, 42.0)).clicked() && !busy {
                    self.m_start_export(false);
                }
            });
            ui.add_space(10.0);
            if busy {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new("Rendering the WAV...").size(12.0).color(muted()));
                });
            } else if !self.m_export.status.is_empty() {
                ui.label(RichText::new(&self.m_export.status).size(11.5).color(muted()));
                if let Some(path) = self.m_export.last.clone() {
                    if cfg!(target_os = "android") && p_button(ui, "Share again", false, vec2(130.0, 34.0)).clicked() {
                        self.m_share(&path);
                    }
                }
            }
        });
    }

    fn m_start_export(&mut self, share: bool) {
        let mut p = self.project.clone();
        p.song_mode = self.m_export.song;
        let settings = crate::audio_engine::export::ExportSettings {
            sample_rate: 44100,
            depth: crate::audio_engine::export::BitDepth::Pcm16,
            bars: crate::audio_engine::export::default_bars(&p),
            tail_secs: 2.0,
        };
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let clean: String = p.name.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
        let path = export_dir().join(format!("{}-{}.wav", if clean.is_empty() { "Winteria".to_string() } else { clean }, stamp % 1_000_000));
        let (tx, rx) = crossbeam_channel::bounded(1);
        std::thread::spawn(move || {
            let res = crate::audio_engine::export::export(&p, &settings, &path).map(|_| path);
            let _ = tx.send(res);
        });
        self.m_export.rx = Some(rx);
        self.m_export.share_after = share;
        self.m_export.status.clear();
    }

    fn m_share(&mut self, path: &std::path::Path) {
        #[cfg(target_os = "android")]
        {
            let res = match &self.android_updater {
                Some(u) => u.share_file(&path.to_string_lossy(), "audio/wav"),
                None => Err("no activity".into()),
            };
            if let Err(e) = res {
                self.m_export.status = format!("Saved: {}  ·  share failed: {e}", path.display());
            }
        }
        #[cfg(not(target_os = "android"))]
        {
            self.m_export.status = format!("Saved: {}", path.display());
        }
    }

    fn m_browser_bottom_sheet(&mut self, ctx: &egui::Context) {
        let id = egui::Id::new("m_browser_open");
        let open = ctx.data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
        let height = (ctx.screen_rect().height() * 0.39).min(360.0);
        egui::TopBottomPanel::bottom("m_browser_bottom_sheet")
            .show_separator_line(false)
            .exact_height(height)
            .frame(Frame::none().fill(p_bg()).stroke(Stroke::new(1.0, outline())).rounding(12.0).inner_margin(9.0))
            .show_animated(ctx, open, |ui| {
                pulse_visuals(ui);
                let (handle, grip) = ui.allocate_exact_size(vec2(ui.available_width(), 14.0), Sense::drag());
                let mark = Rect::from_center_size(handle.center(), vec2(34.0, 3.0));
                ui.painter().rect_filled(mark, 2.0, muted());
                let mut close = grip.drag_delta().y > 18.0;
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Browser").size(17.0).strong().color(ink()));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        close |= p_button(ui, "Close", false, vec2(62.0, 32.0)).clicked();
                    });
                });
                if close {
                    ctx.data_mut(|d| {
                        d.insert_temp(id, false);
                        d.remove::<(usize, u32)>(egui::Id::new("m_audio_clip_target"));
                    });
                } else {
                    self.m_browser_items(ui);
                }
            });
    }

    fn m_browser_sidebar(&mut self, ctx: &egui::Context) {
        let id = egui::Id::new("m_browser_open");
        let open = ctx.data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
        let width = (ctx.screen_rect().width() * 0.44).min(300.0);
        let mut next = open;
        egui::SidePanel::right("m_sample_browser").resizable(false)
            .exact_width(if open { width } else { 22.0 })
            .frame(Frame::none().fill(bar_fill()))
            .show(ctx, |ui| {
                if !open {
                    let (rect, response) = ui.allocate_exact_size(vec2(22.0, ui.available_height()), Sense::click_and_drag());
                    ui.painter().text(pos2(rect.center().x, rect.top() + 22.0), Align2::CENTER_CENTER, "‹", FontId::proportional(22.0), ink());
                    let response = response.on_hover_text("Browser");
                    if response.clicked() || response.drag_delta().x < -18.0 {
                        next = true;
                    }
                    return;
                }

                pulse_visuals(ui);
                Frame::none().inner_margin(8.0).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let response = ui.add(egui::Label::new(RichText::new("Browser").size(17.0).strong()).sense(Sense::drag()));
                        if response.drag_delta().x > 18.0 {
                            next = false;
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if p_button(ui, "×", false, vec2(28.0, 32.0)).on_hover_text("Close browser").clicked() {
                                next = false;
                            }
                        });
                    });
                    self.m_browser_items(ui);
                });
            });
        if next != open {
            ctx.data_mut(|d| {
                d.insert_temp(id, next);
                if !next {
                    d.remove::<(usize, u32)>(egui::Id::new("m_audio_clip_target"));
                }
            });
            ctx.request_repaint();
        }
    }

    fn m_browser_items(&mut self, ui: &mut Ui) {
        let tab_id = egui::Id::new("m_browser_plugins");
        let query_id = egui::Id::new("m_browser_query");
        let mut plugins = ui.data(|d| d.get_temp::<bool>(tab_id)).unwrap_or(false);
        let mut query = ui.data(|d| d.get_temp::<String>(query_id)).unwrap_or_default();
        ui.horizontal_wrapped(|ui| {
            ui.selectable_value(&mut plugins, false, "Sounds");
            ui.selectable_value(&mut plugins, true, "Plugins");
        });
        ui.add(egui::TextEdit::singleline(&mut query).hint_text("Search").desired_width(ui.available_width()));
        let filter = query.trim().to_lowercase();
        ui.data_mut(|d| {
            d.insert_temp(tab_id, plugins);
            d.insert_temp(query_id, query);
        });
        let mut sound = None;
        let mut instrument = None;
        let mut effect = None;
        mobile_scroll(egui::ScrollArea::vertical()).id_salt(("m_browser_list", plugins)).show(ui, |ui| {
            if plugins {
                for track in Track::ALL {
                    if track.name().to_lowercase().contains(&filter)
                        && browser_drag_row(ui, track.name(), browser::DragItem::Instrument(track))
                    {
                        instrument = Some(track);
                    }
                }
                ui.separator();
                for kind in crate::fx::ALL {
                    if kind.name().to_lowercase().contains(&filter)
                        && browser_drag_row(ui, kind.name(), kind)
                    {
                        effect = Some(kind);
                    }
                }
            } else {
                if self.lib.files.is_empty() {
                    ui.label("No WAV files in the sample library.");
                }
                for path in &self.lib.files {
                    let name = path.file_stem().unwrap_or_default().to_string_lossy();
                    if name.to_lowercase().contains(&filter)
                        && browser_drag_row(ui, &name, browser::DragItem::Sample(path.clone()))
                    {
                        sound = Some(path.clone());
                    }
                }
            }
        });
        if let Some(path) = sound {
            let target_id = egui::Id::new("m_audio_clip_target");
            if let Some((lane, tick)) = ui.data(|d| d.get_temp::<(usize, u32)>(target_id)) {
                if self.project.arrangement.tracks.get(lane).is_some_and(|t| t.kind == crate::arrangement::TrackKind::Audio) {
                    self.m_drop_browser_item(&browser::DragItem::Sample(path), lane, tick);
                    if self.arr.sel_clip.is_some() {
                        ui.data_mut(|d| {
                            d.remove::<(usize, u32)>(target_id);
                            d.insert_temp(egui::Id::new("m_browser_open"), false);
                        });
                    } else if !self.arr.msg.is_empty() {
                        ui.colored_label(RED, &self.arr.msg);
                    }
                } else {
                    ui.data_mut(|d| d.remove::<(usize, u32)>(target_id));
                    self.arr.msg = "Select an audio track again.".into();
                }
                return;
            }
            match self.lib.insert_audio_clip(&mut self.project, &path, Some(self.arr.sel_lane)) {
                Ok(index) => {
                    self.arr.sel_clip = Some(index);
                    self.arr.sel_lane = self.project.arrangement.clips[index].lane;
                    self.arr.msg.clear();
                    ui.data_mut(|d| d.insert_temp(egui::Id::new("m_browser_open"), false));
                }
                Err(error) => self.arr.msg = error,
            }
        }
        if let Some(track) = instrument {
            self.m_drop_browser_item(&browser::DragItem::Instrument(track), self.arr.sel_lane, 0);
        }
        if let Some(kind) = effect {
            self.m_drop_effect(kind, self.arr.sel_lane);
        }
    }

    fn m_drop_browser_item(&mut self, item: &browser::DragItem, lane: usize, tick: u32) {
        self.lib.message.clear();
        let bpm = self.project.bpm;
        let before = self.project.arrangement.clips.len();
        if let Some(lane) = arrangement_ui::drop_item(&mut self.project, &mut self.lib, item, lane, tick, bpm) {
            self.arr.sel_lane = lane;
            self.arr.sel_clip = (self.project.arrangement.clips.len() > before).then_some(before);
            self.arr.msg.clone_from(&self.lib.message);
        } else {
            self.arr.msg = "No available track for this item.".into();
        }
    }

    fn m_drop_effect(&mut self, kind: crate::fx::FxKind, lane: usize) {
        let slot = crate::fx::FxSlot::new(kind);
        match plugins_ui::add_to_track(&mut self.project, lane, &slot) {
            Ok(_) => {
                self.arr.sel_lane = lane;
                self.arr.msg.clear();
            }
            Err(error) => self.arr.msg = error.into(),
        }
    }

    /// Tap a track name or clip to open its editor sheet.
    fn m_playlist(&mut self, ui: &mut Ui) {
        if !self.arr.msg.is_empty() {
            ui.colored_label(RED, &self.arr.msg);
        }
        let (bar_w, ruler) = (64.0_f32, 30.0_f32);
        let height_id = egui::Id::new("m_playlist_lane_height");
        let gesture = ui.input(|input| input.multi_touch());
        let viewport = ui.available_rect_before_wrap().intersect(ui.clip_rect());
        let zoom = gesture.filter(|touch| viewport.contains(touch.start_pos));
        let lane_h = ui.data_mut(|d| {
            let height = d.get_temp::<f32>(height_id).unwrap_or(52.0);
            if let Some(touch) = zoom {
                let height = (height * touch.zoom_delta_2d.y).clamp(42.0, 140.0);
                d.insert_temp(height_id, height);
                height
            } else {
                height
            }
        });
        let tracks = self.project.arrangement.tracks.len();
        if tracks == 0 {
            ui.label(RichText::new("No tracks yet. Open a template in Projects.").color(muted()));
            return;
        }
        if self.arr.sel_lane >= tracks {
            self.arr.sel_lane = 0;
        }
        let bars = (self.project.arrangement.song_end_bar() + 8).max(16);
        let pos = self.shared.position.load(Ordering::Relaxed);
        let playing = self.playing() && self.project.song_mode;
        let add_lane = ui.data(|d| d.get_temp::<bool>(egui::Id::new("m_browser_open"))).unwrap_or(false);
        let full_h = ruler + (tracks + usize::from(add_lane)) as f32 * lane_h;
        let panel_id = egui::Id::new("m_playlist_track_names");
        let panel_open = ui.data(|d| d.get_temp::<bool>(panel_id)).unwrap_or(false);
        let panel_width: f32 = if panel_open { 106.0 } else { 28.0 };
        let (viewport, _) = ui.allocate_exact_size(ui.available_size(), Sense::hover());
        let mut grid = viewport;
        grid.min.x += panel_width.min((viewport.width() - 1.0).max(0.0));
        let mut grid_ui = ui.new_child(egui::UiBuilder::new().max_rect(grid));
        grid_ui.set_clip_rect(grid.intersect(ui.clip_rect()));
        let mut selection = None;
        let mut delete_clip = None;
        let mut move_clip = None;
        let mut dropped_item = None;
        let mut dropped_effect = None;
        let scroll = mobile_scroll(egui::ScrollArea::both()).id_salt("m_playlist")
            .drag_to_scroll(gesture.is_none()).max_height(grid.height()).auto_shrink([false, false])
            .show(&mut grid_ui, |ui| {
            let (rect, resp) = ui.allocate_exact_size(
                vec2((bars as f32 * bar_w).max(grid.width()), full_h.max(grid.height())), Sense::click());
            let p = ui.painter_at(rect);
            let gx = rect.left();
            let gy = rect.top() + ruler;
            p.rect_filled(rect, 0.0, pick(0xFF191E25, 0xFFE5EBF2));
            let time_header = Rect::from_min_size(rect.min, vec2(rect.width(), ruler));
            p.rect_filled(time_header, 0.0, pick(0xFF252D37, 0xFFD6E0EA));
            for (lane, track) in self.project.arrangement.tracks.iter().enumerate() {
                let y = gy + lane as f32 * lane_h;
                let r = Rect::from_min_size(pos2(rect.left(), y), vec2(rect.width(), lane_h));
                let audio = track.kind == crate::arrangement::TrackKind::Audio;
                let row_fill = if lane == self.arr.sel_lane {
                    pick(0xFF323D4C, 0xFFC9D9E7)
                } else if audio {
                    pick(0xFF202A30, 0xFFD9E9EB)
                } else {
                    pick(0xFF232A36, 0xFFDCE4EF)
                };
                p.rect_filled(r, 0.0, row_fill);
                if lane == self.arr.sel_lane {
                    p.rect_filled(Rect::from_min_size(r.min, vec2(3.0, r.height())), 0.0, lane_col(lane));
                }
                p.line_segment([r.left_top(), r.right_top()], Stroke::new(0.6, glass_edge()));
            }
            if add_lane {
                let y = gy + tracks as f32 * lane_h;
                p.line_segment([pos2(rect.left(), y), pos2(rect.right(), y)], Stroke::new(0.6, glass_edge()));
                p.text(pos2(rect.left() + 10.0, y + lane_h * 0.5), Align2::LEFT_CENTER,
                    "+ New track", FontId::proportional(12.0), muted());
            }
            for b in 0..=bars {
                let x = gx + b as f32 * bar_w;
                p.line_segment([pos2(x, rect.top() + ruler - 4.0), pos2(x, rect.bottom())], Stroke::new(if b % 4 == 0 { 0.9 } else { 0.4 }, glass_edge()));
                if b < bars {
                    p.text(pos2(x + 4.0, rect.top() + 6.0), Align2::LEFT_TOP, format!("{}", b + 1), FontId::proportional(11.0), muted());
                }
            }
            for (i, c) in self.project.arrangement.clips.iter().enumerate() {
                let x0 = gx + c.start as f32 / TICKS_PER_BAR as f32 * bar_w;
                let w = (c.len as f32 / TICKS_PER_BAR as f32 * bar_w).max(6.0);
                let hit = Rect::from_min_size(pos2(x0, gy + c.lane as f32 * lane_h), vec2(w, lane_h));
                let r = hit.shrink2(vec2(1.5, 5.0));
                let clip = ui.interact(hit, resp.id.with(("clip", i)), Sense::click_and_drag())
                    .on_hover_cursor(egui::CursorIcon::Grab);
                let col = lane_col(c.lane);
                p.rect_filled(r, 3.0, col.linear_multiply(0.75));
                if self.arr.sel_clip == Some(i) {
                    p.rect_stroke(r.expand(1.0), 9.0, Stroke::new(2.0, p_accent()));
                }
                let name = if c.audio {
                    self.project.audio_pool.get(c.pattern).map(|sample| sample.name.as_str())
                } else {
                    self.project.patterns.get(c.pattern).map(|pattern| pattern.name.as_str())
                };
                if let Some(name) = name.filter(|_| r.width() > 24.0) {
                    p.with_clip_rect(r.intersect(ui.clip_rect())).text(
                        r.left_center() + vec2(6.0, 0.0), Align2::LEFT_CENTER, name,
                        FontId::proportional(11.0), Color32::WHITE);
                }
                if gesture.is_some() {
                    continue;
                }

                if clip.clicked() {
                    let edit = clip.double_clicked() || self.arr.sel_clip == Some(i);
                    selection = Some((c.lane, Some(i), Some(c.start), edit));
                }
                clip.context_menu(|ui| {
                    if ui.button("Open clip").clicked() {
                        selection = Some((c.lane, Some(i), Some(c.start), true));
                        ui.close_menu();
                    }
                    if ui.button("Delete clip").clicked() {
                        delete_clip = Some(i);
                        ui.close_menu();
                    }
                });

                let grab_id = clip.id.with("grab_x");
                if clip.drag_started() {
                    if let Some(origin) = ui.input(|input| input.pointer.press_origin()) {
                        ui.data_mut(|d| d.insert_temp(grab_id, origin.x - x0));
                    }
                }
                if clip.dragged() || clip.drag_stopped() {
                    let grab = ui.data(|d| d.get_temp::<f32>(grab_id));
                    let point = clip.interact_pointer_pos().filter(|pt| grid.contains(*pt) && pt.y >= gy);
                    if let (Some(grab), Some(point)) = (grab, point) {
                        let lane = ((point.y - gy) / lane_h) as usize;
                        let snap = self.arr.snap.max(1);
                        let tick = (point.x - gx - grab).max(0.0) / bar_w * TICKS_PER_BAR as f32;
                        let start = (tick / snap as f32).round() as u32 * snap;
                        let candidate = crate::arrangement::Clip { lane, start, ..*c };
                        let valid = lane < tracks
                            && self.project.arrangement.tracks[lane].kind == self.project.arrangement.tracks[c.lane].kind
                            && !self.project.arrangement.clips.iter().enumerate()
                                .any(|(j, other)| i != j && candidate.overlaps(other));
                        let preview = Rect::from_min_size(
                            pos2(gx + start as f32 / TICKS_PER_BAR as f32 * bar_w, gy + lane as f32 * lane_h),
                            vec2(w, lane_h)).shrink2(vec2(1.5, 5.0));
                        p.rect_stroke(preview, 4.0, Stroke::new(2.0, if valid { p_accent() } else { RED }));
                        if valid && clip.drag_stopped() {
                            move_clip = Some((i, candidate));
                        }
                    }
                    if clip.drag_stopped() {
                        ui.data_mut(|d| d.remove::<f32>(grab_id));
                    }
                }

            }
            if playing {
                let x = gx + pos as f32 / TICKS_PER_BAR as f32 * bar_w;
                p.line_segment([pos2(x, rect.top()), pos2(x, rect.bottom())], Stroke::new(2.0, p_accent()));
            }
            if let Some(point) = ui.input(|input| input.pointer.interact_pos()).filter(|p| rect.contains(*p) && p.y >= gy && p.y < gy + (tracks + usize::from(add_lane)) as f32 * lane_h) {
                let lane = (((point.y - gy) / lane_h) as usize).min(tracks);
                let tick = ((point.x - gx).max(0.0) / bar_w * TICKS_PER_BAR as f32) as u32;
                let snap = self.arr.snap.max(1);
                if resp.dnd_hover_payload::<browser::DragItem>().is_some()
                    || (lane < tracks && resp.dnd_hover_payload::<crate::fx::FxKind>().is_some())
                {
                    let row = Rect::from_min_size(pos2(rect.left(), gy + lane as f32 * lane_h), vec2(rect.width(), lane_h));
                    p.rect_stroke(row.shrink(1.0), 6.0, Stroke::new(1.5, p_accent()));
                }
                if let Some(item) = resp.dnd_release_payload::<browser::DragItem>() {
                    dropped_item = Some((item, lane, tick / snap * snap));
                }
                if lane < tracks {
                    if let Some(kind) = resp.dnd_release_payload::<crate::fx::FxKind>() {
                        dropped_effect = Some((*kind, lane));
                    }
                }
            }
            if gesture.is_none() && resp.clicked() {
                if let Some(pt) = resp.interact_pointer_pos().filter(|pt| pt.y >= gy) {
                    let lane = ((pt.y - gy) / lane_h) as usize;
                    if lane < tracks {
                        let tick = ((pt.x - gx).max(0.0) / bar_w * TICKS_PER_BAR as f32) as u32;
                        selection = Some((lane, None, Some(tick), false));
                    }
                }
            }
        });
        let panel = Rect::from_min_max(viewport.min, pos2(grid.left(), viewport.bottom()));
        let painter = ui.painter_at(panel);
        painter.rect_filled(panel, 0.0, pick(0xFF252D37, 0xFFD6E0EA));
        let track_y = scroll.inner_rect.top() + ruler - scroll.state.offset.y;
        let names = Rect::from_min_max(pos2(panel.left(), panel.top() + ruler), panel.max);
        if panel_open {
            let labels = painter.with_clip_rect(names.intersect(ui.clip_rect()));
            for (lane, track) in self.project.arrangement.tracks.iter().enumerate() {
                let top = track_y + lane as f32 * lane_h;
                let row = Rect::from_min_size(pos2(panel.left(), top), vec2(panel.width(), lane_h));
                if !row.intersects(names) {
                    continue;
                }
                let selected = lane == self.arr.sel_lane;
                if selected {
                    labels.rect_filled(row, 0.0, pick(0xFF323D4C, 0xFFC9D9E7));
                }
                labels.line_segment([row.left_top(), row.right_top()], Stroke::new(0.6, glass_edge()));
                labels.circle_filled(row.left_center() + vec2(12.0, 0.0), 4.0, lane_col(lane));
                labels.with_clip_rect(row.intersect(names).intersect(ui.clip_rect())).text(
                    row.left_center() + vec2(23.0, 0.0), Align2::LEFT_CENTER, &track.name,
                    FontId::proportional(13.0), if track.mute { muted() } else { ink() });
                if ui.interact(row.intersect(names), panel_id.with(lane), Sense::click()).clicked() {
                    selection = Some((lane, None, None, false));
                }
            }
        }
        let toggle = Rect::from_min_size(panel.min, vec2(panel.width(), ruler));
        let toggle_response = ui.interact(toggle, panel_id, Sense::click())
            .on_hover_text(if panel_open { "Hide track names" } else { "Show track names" });
        painter.text(toggle.center(), Align2::CENTER_CENTER, if panel_open { "‹  Tracks" } else { "›" },
            FontId::proportional(if panel_open { 13.0 } else { 24.0 }), ink());
        if toggle_response.clicked() {
            ui.data_mut(|d| d.insert_temp(panel_id, !panel_open));
        }
        if let Some((index, moved)) = move_clip {
            self.project.arrangement.clips[index] = moved;
            self.arr.sel_lane = moved.lane;
            self.arr.sel_clip = Some(index);
            self.arr.sel.clear();
            self.arr.msg.clear();
            return;
        }
        if let Some(index) = delete_clip {
            self.project.arrangement.clips.remove(index);
            self.arr.sel_clip = None;
            self.arr.sel.clear();
            return;
        }
        if let Some((item, lane, tick)) = dropped_item {
            self.m_drop_browser_item(&item, lane, tick);
        }
        if let Some((kind, lane)) = dropped_effect {
            self.m_drop_effect(kind, lane);
            self.arr.sel_clip = None;
        }
        if let Some((lane, clip, tick, edit)) = selection {
            self.arr.sel_lane = lane;
            self.arr.sel_clip = clip;
            self.arr.msg.clear();
            ui.data_mut(|d| d.remove::<(usize, u32)>(egui::Id::new("m_audio_clip_target")));
            if tick.is_none() {
                ui.data_mut(|d| {
                    d.remove::<usize>(egui::Id::new("m_clip_sheet_selection"));
                    d.insert_temp(egui::Id::new("m_track_sheet_open"), true);
                });
            }
            if let Some(i) = clip {
                if let Some(c) = self.project.arrangement.clips.get(i).filter(|_| edit) {
                    if !c.audio && c.pattern < self.project.patterns.len() {
                        self.project.current_pattern = c.pattern;
                    }
                    ui.data_mut(|d| {
                        d.remove::<bool>(egui::Id::new("m_track_sheet_open"));
                        d.insert_temp(egui::Id::new("m_clip_sheet_selection"), i);
                    });
                }
            } else if let Some(tick) = tick {
                use crate::arrangement::TrackKind;
                let snap = self.arr.snap.max(1);
                let start = tick / snap * snap;
                match self.project.arrangement.kind(lane) {
                    TrackKind::Midi => {
                        if arrangement_ui::new_midi_clip_at(&mut self.project, &mut self.arr, lane, start, TICKS_PER_BAR) {
                            let track = &mut self.project.arrangement.tracks[lane];
                            if track.blank {
                                track.blank = false;
                                track.instrument = Some(Track::Synth);
                            }
                        } else {
                            self.arr.msg = "There is no room for a MIDI clip at this position.".into();
                        }
                    }
                    TrackKind::Audio => {
                        ui.data_mut(|d| {
                            d.remove::<bool>(egui::Id::new("m_studio_full_view"));
                            d.insert_temp(egui::Id::new("m_audio_clip_target"), (lane, start));
                            d.insert_temp(egui::Id::new("m_browser_plugins"), false);
                            d.insert_temp(egui::Id::new("m_browser_open"), true);
                        });
                    }
                }
            }
        }
    }

    fn m_tracks(&mut self, ui: &mut Ui) {
        let n = self.project.arrangement.tracks.len();
        mobile_scroll(egui::ScrollArea::vertical()).auto_shrink([false, false]).show(ui, |ui| {
            ui.horizontal(|ui| {
                let w = (ui.available_width() - 8.0) / 2.0;
                if p_button(ui, "+ MIDI track", true, vec2(w, 40.0)).clicked() {
                    self.project.arrangement.add_track(crate::arrangement::TrackKind::Midi);
                }
                if p_button(ui, "+ Audio track", false, vec2(w, 40.0)).clicked() {
                    self.project.arrangement.add_track(crate::arrangement::TrackKind::Audio);
                }
            });
            ui.add_space(10.0);
            for l in 0..n {
                let col = lane_col(l);
                let audio = self.project.arrangement.tracks[l].kind == crate::arrangement::TrackKind::Audio;
                // the glass goes under the card's widgets: reserve its slot first, fill it once the size is known
                let slot = ui.painter().add(Shape::Noop);
                let inner = Frame::none().inner_margin(12.0).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(vec2(10.0, 30.0), Sense::hover());
                        ui.painter().circle_filled(r.center(), 5.0, col);
                        let t = &mut self.project.arrangement.tracks[l];
                        ui.label(RichText::new(if audio { "AUDIO" } else { "MIDI" }).size(10.0).strong().color(muted()));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            for flag in [1u8, 0] {
                                let label = if flag == 0 { "M" } else { "S" };
                                let v = if flag == 0 { &mut t.mute } else { &mut t.solo };
                                let (r, resp) = ui.allocate_exact_size(vec2(42.0, 34.0), Sense::click());
                                if *v {
                                    pill(ui.painter(), r, if flag == 0 { AMBER } else { p_accent() });
                                } else {
                                    glass(ui.painter(), r, 17.0);
                                }
                                ui.painter().text(r.center(), Align2::CENTER_CENTER, label, FontId::proportional(14.0), if *v { on_accent() } else { ink() });
                                if resp.clicked() {
                                    *v = !*v;
                                }
                            }
                            ui.add(egui::TextEdit::singleline(&mut t.name).desired_width(ui.available_width()).font(FontId::proportional(15.0)));
                        });
                    });
                    let t = &mut self.project.arrangement.tracks[l];
                    ui.spacing_mut().slider_width = (ui.available_width() - 70.0).max(80.0);
                    ui.horizontal(|ui| {
                        ui.add(egui::Slider::new(&mut t.volume, 0.0..=1.5).show_value(false));
                        ui.label(RichText::new(format!("{:+.1} dB", db(t.volume))).size(11.0).color(muted()));
                    });
                    if let Some(inst) = t.instrument {
                        ui.label(RichText::new(instruments_ui::Inst::from_track(inst).title()).size(11.0).color(muted()));
                    }
                });
                let r = inner.response.rect;
                let pts = rounded_points(r, 16.0);
                let (top, bottom) = (glass_top(), glass_bottom());
                let col = |y: f32| lerp_c(top, bottom, ((y - r.top()) / r.height().max(1.0)).clamp(0.0, 1.0));
                let mut mesh = egui::Mesh::default();
                mesh.colored_vertex(r.center(), col(r.center().y));
                for q in &pts {
                    mesh.colored_vertex(*q, col(q.y));
                }
                let k = pts.len() as u32;
                for i in 0..k {
                    mesh.add_triangle(0, 1 + i, 1 + (i + 1) % k);
                }
                ui.painter().set(slot, Shape::Vec(vec![Shape::mesh(mesh), Shape::closed_line(pts, Stroke::new(1.0, glass_edge()))]));
                ui.add_space(8.0);
            }
        });
    }

    fn m_mixer(&mut self, ui: &mut Ui) {
        let id = egui::Id::new("m_mixer_channel");
        let mut selected = ui.data(|d| d.get_temp::<usize>(id)).unwrap_or(0).min(NUM_INSERTS);
        let height = ui.available_height().max(1.0);
        let rail_width = (ui.available_width() * 0.54).clamp(72.0, 190.0);
        let control_width = (ui.available_width() - rail_width - ui.spacing().item_spacing.x).max(1.0);
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(vec2(control_width, height), egui::Layout::top_down(egui::Align::Center), |ui| {
                self.m_strip(ui, selected.checked_sub(1), (height - 8.0).clamp(160.0, 520.0));
            });
            ui.allocate_ui_with_layout(vec2(rail_width, height), egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.set_width(rail_width);
                Frame::none().fill(bar_fill()).rounding(14.0).inner_margin(7.0).show(ui, |ui| {
                    mobile_scroll(egui::ScrollArea::vertical()).id_salt("m_mixer_right").max_height((height - 14.0).max(1.0)).show(ui, |ui| {
                        for channel in 0..=NUM_INSERTS {
                            let label: String = if channel == 0 { "Master".into() } else {
                                let name = &self.project.channels[NUM_CH + channel - 1].name;
                                let short: String = name.chars().take(12).collect();
                                format!("{channel}  {short}")
                            };
                            if p_button(ui, &label, channel == selected, vec2(ui.available_width(), 38.0)).clicked() {
                                selected = channel;
                            }
                        }
                    });
                });
            });
        });
        ui.data_mut(|d| d.insert_temp(id, selected));
    }

    fn m_strip(&mut self, ui: &mut Ui, insert: Option<usize>, h: f32) {
        let w = 72.0;
        let (rect, _) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
        if insert.is_none() {
            ui.painter().rect_filled(rect, 16.0, p_bg());
            glass_tinted(ui.painter(), rect, 16.0, Some(p_accent().linear_multiply(0.10)));
        } else {
            panel(ui.painter(), rect, 16.0);
        }
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink(8.0)).layout(egui::Layout::top_down(egui::Align::Center)));
        pulse_visuals(&mut child);
        child.spacing_mut().slider_width = (h - 130.0).max(80.0);
        match insert {
            None => {
                child.label(RichText::new("MASTER").size(11.0).strong().color(ink()));
                child.add_space(6.0);
                child.add(egui::Slider::new(&mut self.project.master_volume, 0.0..=1.2).vertical().show_value(false));
                child.label(RichText::new(format!("{:+.1}", db(self.project.master_volume))).size(11.0).color(muted()));
                let pk = self.shared.peak(0);
                meter(&child, pk);
            }
            Some(i) => {
                let ch = &mut self.project.channels[NUM_CH + i];
                child.label(RichText::new(format!("{}", i + 1)).size(14.0).strong().color(ink()));
                child.add(egui::TextEdit::singleline(&mut ch.name).desired_width(56.0).font(FontId::proportional(10.0)));
                child.add(egui::Slider::new(&mut ch.volume, 0.0..=1.5).vertical().show_value(false));
                child.label(RichText::new(format!("{:+.1}", db(ch.volume))).size(11.0).color(muted()));
                let (r, resp) = child.allocate_exact_size(vec2(48.0, 32.0), Sense::click());
                if ch.mute {
                    pill(child.painter(), r, AMBER);
                } else {
                    glass(child.painter(), r, 16.0);
                }
                child.painter().text(r.center(), Align2::CENTER_CENTER, "M", FontId::proportional(13.0), if ch.mute { on_accent() } else { ink() });
                if resp.clicked() {
                    ch.mute = !ch.mute;
                }
            }
        }
    }

    // ───────────────────── Settings / Profile ─────────────────────

    fn m_settings(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        mobile_scroll(egui::ScrollArea::vertical()).auto_shrink([false, false]).show(ui, |ui| {
            heading(ui, "Theme");
            ui.horizontal(|ui| {
                let w = (ui.available_width() - 8.0) / 2.0;
                if p_button(ui, "Night", is_dark(), vec2(w, 42.0)).clicked() {
                    set_dark(true);
                    save_prefs();
                    apply_visuals(ctx);
                }
                if p_button(ui, "Day", !is_dark(), vec2(w, 42.0)).clicked() {
                    set_dark(false);
                    save_prefs();
                    apply_visuals(ctx);
                }
            });
            ui.add_space(10.0);
            heading(ui, "Sound");
            let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 64.0), Sense::hover());
            glass(ui.painter(), rect, 14.0);
            let status = match &self.audio.error {
                Some(e) => e.clone(),
                None if self.audio.has_stream() => format!("{} Hz  ·  output is running", self.audio.sample_rate),
                None => "No output".to_string(),
            };
            ui.painter().text(rect.left_top() + vec2(16.0, 14.0), Align2::LEFT_TOP, "Audio output", FontId::proportional(15.0), ink());
            ui.painter().text(rect.left_top() + vec2(16.0, 36.0), Align2::LEFT_TOP, status, FontId::proportional(12.0), muted());
            ui.add_space(10.0);
            #[cfg(target_os = "android")]
            if let Some(updater) = &mut self.android_updater {
                heading(ui, "Updates");
                ui.horizontal_wrapped(|ui| updater.show(ui));
                ui.add_space(10.0);
            }
            heading(ui, "Sync with PC");
            ui.label(RichText::new(format!("Projects folder: {}", hub_dir().display())).size(12.0).color(ink()));
            ui.label(RichText::new("On the PC choose the same cloud folder in the Hub: a project saved on one device shows up on the other.").size(11.0).color(muted()));
            ui.add_space(10.0);
            heading(ui, "About");
            ui.label(RichText::new(format!("Winteria  v{}", env!("CARGO_PKG_VERSION"))).color(ink()));
            ui.label(RichText::new("Winteria Studio  ·  own audio engine, synths, sequencer, mixer").size(12.0).color(muted()));
        });
    }

    fn m_profile(&mut self, ui: &mut Ui) {
        mobile_scroll(egui::ScrollArea::vertical()).auto_shrink([false, false]).show(ui, |ui| {
            let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 180.0), Sense::hover());
            glass(ui.painter(), rect, 18.0);
            let c = pos2(rect.center().x, rect.top() + 64.0);
            let p = ui.painter();
            p.circle_stroke(c, 44.0, Stroke::new(3.0, p_accent()));
            logo(ui, Rect::from_center_size(c, vec2(84.0, 84.0)));
            let p = ui.painter();
            p.text(pos2(c.x, rect.top() + 124.0), Align2::CENTER_CENTER, "Winteria Studio", FontId::proportional(19.0), ink());
            p.text(pos2(c.x, rect.top() + 150.0), Align2::CENTER_CENTER, "Guest", FontId::proportional(12.0), muted());
            ui.add_space(12.0);
            let projects = self.hub_cache.as_ref().map(|l| l.len()).unwrap_or(0);
            list_row(ui, lane_col(0), "P", "Projects", &format!("{projects} saved"), "", false);
            list_row(ui, lane_col(4), "R", "Recent", &format!("{} opened", self.recent.len()), "", false);
        });
    }
}

fn studio_icon(ui: &mut Ui, width: f32, symbol: Icon, active: bool, label: &str) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(width, 32.0), Sense::click());
    if active {
        pill(ui.painter(), rect, p_accent());
    } else if response.hovered() {
        glass(ui.painter(), rect, 9.0);
    }
    icon(ui.painter(), symbol, rect.center(), if active { on_accent() } else { ink() });
    response.on_hover_text(label)
}

fn mobile_editor_sheet(ctx: &egui::Context, id: &'static str, title: &str, body: impl FnOnce(&mut Ui)) -> bool {
    let screen = ctx.screen_rect();
    let size = vec2((screen.width() - 22.0).max(1.0), (screen.height() * 0.76).max(96.0));
    let mut done = false;
    egui::Window::new(title).id(egui::Id::new(id)).title_bar(false).resizable(false).movable(false)
        .anchor(Align2::CENTER_BOTTOM, [0.0, -8.0]).fixed_size(size)
        .frame(Frame::none().fill(p_bg()).stroke(Stroke::new(1.0, outline())).rounding(16.0).inner_margin(10.0))
        .show(ctx, |ui| {
            pulse_visuals(ui);
            ui.horizontal(|ui| {
                let title_width = (ui.available_width() - 76.0).max(1.0);
                ui.add_sized([title_width, 32.0], egui::Label::new(RichText::new(title).size(17.0).strong().color(ink())).truncate());
                done = p_button(ui, "Done", true, vec2(68.0, 32.0)).clicked();
            });
            body(ui);
        });
    done || ctx.input(|input| input.key_pressed(egui::Key::Escape))
}

fn browser_drag_row<T: std::any::Any + Send + Sync>(ui: &mut Ui, label: &str, item: T) -> bool {
    ui.horizontal(|ui| {
        let (rect, grip) = ui.allocate_exact_size(vec2(24.0, 38.0), Sense::drag());
        grip.dnd_set_drag_payload(item);
        ui.painter().text(rect.center(), Align2::CENTER_CENTER, "⋮⋮", FontId::proportional(13.0), muted());
        ui.add_sized([ui.available_width(), 38.0], egui::Button::new(label).truncate()).clicked()
    }).inner
}

fn meter(ui: &Ui, pk: [f32; 2]) {
    let r = ui.max_rect();
    let p = ui.painter();
    for (k, v) in pk.iter().enumerate() {
        let x = r.right() - 4.0 - k as f32 * 5.0;
        let h = (v.clamp(0.0, 1.2) / 1.2) * (r.height() - 60.0);
        let bottom = r.bottom() - 30.0;
        p.line_segment([pos2(x, bottom), pos2(x, bottom - h)], Stroke::new(3.0, p_accent()));
    }
}
