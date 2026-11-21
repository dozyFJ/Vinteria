//! Winteria design system, in the style of the AOS launcher.
//!
//! One look in two modes (AOS Night / AOS Day): cool graphite surfaces, a bright cyan accent,
//! big rounded pill controls, Segoe UI throughout. Everything in the UI reads its colours / radii from here, so a change in
//! this file restyles the whole program.
//!
//! Surface layers (back to front):  BG  <  SIDE  <  PANEL2 (cards, inputs)  <  PANEL3 (neutral controls)
//! Lines:                           border  <  MID (strong line)  <  LIGHT (faint text / icons)
//! Text:                            TEXT (primary)  >  DIM (secondary)
use egui::{Color32, Context, FontData, FontDefinitions, FontFamily, FontId, Rounding, Stroke, TextStyle};
use super::aos_themes::{AosTheme, AOS_THEMES, GROUPS};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

/// UI scale in percent (50..=150), applied as the egui zoom factor
static UI_SCALE: AtomicU8 = AtomicU8::new(100);

// ------------------------------------------------------------------ preferences (theme.txt next to the program)

static DARK_MODE: AtomicBool = AtomicBool::new(true);
static ACCENT: AtomicU8 = AtomicU8::new(0);
static AUTO: AtomicBool = AtomicBool::new(false);
/// 0 = the default AOS Night / AOS Day look, n = AOS_THEMES[n - 1]
static THEME: AtomicU8 = AtomicU8::new(0);

pub const ACCENTS: [&str; 6] = ["Cyan", "Mint", "Violet", "Blue", "Pink", "Orange"];

pub fn is_dark() -> bool {
    DARK_MODE.load(Ordering::Relaxed)
}
pub fn set_dark(on: bool) {
    DARK_MODE.store(on, Ordering::Relaxed);
}
pub fn accent() -> u8 {
    ACCENT.load(Ordering::Relaxed)
}
pub fn set_accent(a: u8) {
    ACCENT.store(a.min(ACCENTS.len() as u8 - 1), Ordering::Relaxed);
    THEME.store(0, Ordering::Relaxed);
    save_prefs();
}
pub fn theme_index() -> u8 {
    THEME.load(Ordering::Relaxed)
}
/// 0 = classic, n = the n-th AOS theme
pub fn set_theme(n: u8) {
    THEME.store(n.min(AOS_THEMES.len() as u8), Ordering::Relaxed);
    save_prefs();
}
fn aos() -> Option<&'static AosTheme> {
    AOS_THEMES.get((theme_index() as usize).wrapping_sub(1))
}
/// follow the time of day: dark from 19:00 to 07:00
pub fn auto() -> bool {
    AUTO.load(Ordering::Relaxed)
}
pub fn set_auto(on: bool) {
    AUTO.store(on, Ordering::Relaxed);
    save_prefs();
}

pub const SCALE_MIN: u8 = 50;
pub const SCALE_MAX: u8 = 150;

pub fn ui_scale() -> u8 {
    UI_SCALE.load(Ordering::Relaxed)
}
pub fn set_ui_scale(pct: u8) {
    UI_SCALE.store(pct.clamp(SCALE_MIN, SCALE_MAX), Ordering::Relaxed);
    save_prefs();
}
/// Push the stored scale to egui (call after `set_ui_scale` and at start-up).
pub fn apply_scale(ctx: &Context) {
    let z = ui_scale() as f32 / 100.0;
    if (ctx.zoom_factor() - z).abs() > 1e-3 {
        ctx.set_zoom_factor(z);
    }
}

pub fn theme_name() -> &'static str {
    if let Some(t) = aos() { return t.name; }
    if is_dark() { "AOS Night" } else { "AOS Day" }
}

pub fn save_prefs() {
    let _ = std::fs::write("theme.txt", format!("v3,{},{},{},{},{}", is_dark() as u8, accent(), auto() as u8, ui_scale(), theme_index()));
}

pub fn load_prefs() {
    if let Ok(s) = std::fs::read_to_string("theme.txt") {
        let mut it = s.trim().split(',');
        // only the current format is read; files of the retired themes are ignored (defaults apply)
        let ver = it.next();
        if ver == Some("v2") || ver == Some("v3") {
            let v: Vec<u8> = it.filter_map(|x| x.parse().ok()).collect();
            if ver == Some("v3") && v.len() >= 4 {
                UI_SCALE.store(v[3].clamp(SCALE_MIN, SCALE_MAX), Ordering::Relaxed);
            }
            if ver == Some("v3") && v.len() >= 5 && (v[4] as usize) <= AOS_THEMES.len() {
                THEME.store(v[4], Ordering::Relaxed);
            }
            if v.len() >= 3 {
                DARK_MODE.store(v[0] == 1, Ordering::Relaxed);
                ACCENT.store(v[1].min(ACCENTS.len() as u8 - 1), Ordering::Relaxed);
                AUTO.store(v[2] == 1, Ordering::Relaxed);
            }
        }
    }
    if auto() {
        DARK_MODE.store(night_now(), Ordering::Relaxed);
    }
}

/// Local hour of the day (0..23).
pub fn local_hour() -> u32 {
    #[cfg(windows)]
    {
        #[repr(C)]
        struct St {
            y: u16,
            mo: u16,
            dow: u16,
            d: u16,
            h: u16,
            mi: u16,
            s: u16,
            ms: u16,
        }
        extern "system" {
            fn GetLocalTime(t: *mut St);
        }
        let mut st = St { y: 0, mo: 0, dow: 0, d: 0, h: 12, mi: 0, s: 0, ms: 0 };
        unsafe { GetLocalTime(&mut st) };
        return st.h as u32 % 24;
    }
    #[allow(unreachable_code)]
    12
}

/// true between 19:00 and 07:00
pub fn night_now() -> bool {
    let h = local_hour();
    h >= 19 || h < 7
}

// ------------------------------------------------------------------ geometry tokens

/// radius of buttons, inputs, rows and other controls
pub static FLAT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
fn flat() -> bool { FLAT.load(std::sync::atomic::Ordering::Relaxed) }
pub fn corner() -> f32 {
    if flat() { 0.0 } else { 14.0 }
}
/// radius of cards, panels inside panels
pub fn card() -> f32 {
    if flat() { 0.0 } else { 22.0 }
}
/// radius of notes, clips and step cells
pub fn small() -> f32 {
    0.0
}
/// panel / card outline width
pub fn border_w() -> f32 {
    1.0
}

// ------------------------------------------------------------------ colour tokens

fn rgb(c: u32) -> Color32 {
    Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8)
}
fn pk(light: u32, dark: u32) -> Color32 {
    rgb(if is_dark() { dark } else { light })
}

fn mobile_day() -> bool {
    cfg!(target_os = "android") && !is_dark()
}

fn surface(mobile: u32, light: u32, dark: u32) -> Color32 {
    if mobile_day() { rgb(mobile) } else { pk(light, dark) }
}

/// Base surface colour of the chosen AOS theme in the current mode (night scene when dark, day scene when light).
fn aos_base() -> Option<Color32> {
    let t = aos()?;
    Some(match (t.scene, is_dark()) {
        (Some(c), true) => lerp_rgb(rgb(c[3]), rgb(c[4]), 0.35),
        (Some(c), false) => lerp_rgb(lerp_rgb(rgb(c[0]), rgb(c[1]), 0.5), Color32::WHITE, 0.6),
        (None, true) => lerp_rgb(rgb(t.accent_dark), Color32::BLACK, 0.86),
        (None, false) => lerp_rgb(rgb(t.accent_light), Color32::WHITE, 0.88),
    })
}
/// A token of the AOS theme: the base mixed towards white (t > 0) or black (t < 0), one amount per mode.
fn aos_tok(dark_t: f32, light_t: f32) -> Option<Color32> {
    if mobile_day() { return None; }
    let b = aos_base()?;
    let t = if is_dark() { dark_t } else { light_t };
    Some(if t >= 0.0 { lerp_rgb(b, Color32::WHITE, t) } else { lerp_rgb(b, Color32::BLACK, -t) })
}

/// work area behind everything
pub fn BG() -> Color32 { aos_tok(-0.50, -0.05).unwrap_or_else(|| surface(0xB8C2C7, 0xE9EEF2, 0x14181C)) }
/// side panels, header, bars
pub fn SIDE() -> Color32 { aos_tok(-0.30, 0.30).unwrap_or_else(|| surface(0xC2CBCE, 0xF4F7FA, 0x1B2025)) }
pub fn PANEL() -> Color32 { SIDE() }
/// cards, inputs, raised surfaces
pub fn PANEL2() -> Color32 { aos_tok(0.07, 0.65).unwrap_or_else(|| surface(0xCBD2D4, 0xFFFFFF, 0x222830)) }
/// neutral controls (buttons, tracks of sliders)
pub fn PANEL3() -> Color32 { aos_tok(0.15, -0.07).unwrap_or_else(|| surface(0xB7C3C9, 0xDDE4EA, 0x2B323A)) }
/// the standard thin outline
pub fn border() -> Color32 { aos_tok(0.24, -0.17).unwrap_or_else(|| surface(0x74858D, 0xC5CED7, 0x343C45)) }
/// stronger line (disabled, off-state, grid beats)
pub fn MID() -> Color32 { aos_tok(0.40, -0.40).unwrap_or_else(|| pk(0x8B97A3, 0x56606B)) }
/// faint text, icons, hairlines on top of surfaces
pub fn LIGHT() -> Color32 { aos_tok(0.55, -0.50).unwrap_or_else(|| pk(0x6B7783, 0x7E8995)) }
pub fn TEXT() -> Color32 { aos_tok(0.92, -0.88).unwrap_or_else(|| pk(0x11181E, 0xEEF3F6)) }
pub fn DIM() -> Color32 { aos_tok(0.68, -0.62).unwrap_or_else(|| pk(0x4A5560, 0xA3AEB8)) }
/// "ink": outlines and marks that must contrast with the surfaces
pub fn DARK() -> Color32 { aos_tok(0.92, -0.80).unwrap_or_else(|| pk(0x11181E, 0xEEF3F6)) }
/// dark display used for meters and scopes (both modes)
pub fn INSET() -> Color32 { aos_tok(-0.70, -0.78).unwrap_or_else(|| pk(0x1B2127, 0x0C0F12)) }

/// accent colours: (light, dark) per accent set -> [primary, secondary, highlight / playhead]
fn acc(slot: usize) -> Color32 {
    const SETS: [[(u32, u32); 3]; 6] = [
        [(0x0E9FC4, 0x4FD8F5), (0x1FA97A, 0x3DDC97), (0xE59A00, 0xFFB020)],
        [(0x1FA97A, 0x3DDC97), (0x0E9FC4, 0x4FD8F5), (0xE59A00, 0xFFB020)],
        [(0x6D4AE0, 0x9A8CFF), (0x0E9FC4, 0x4FD8F5), (0xE59A00, 0xFFB020)],
        [(0x2563EB, 0x5B93FF), (0x0E9FC4, 0x4FD8F5), (0xE59A00, 0xFFB020)],
        [(0xD6336C, 0xFF6FA0), (0x0E9FC4, 0x4FD8F5), (0xE59A00, 0xFFB020)],
        [(0xE0702A, 0xFF9A5C), (0x0E9FC4, 0x4FD8F5), (0x1FA97A, 0x3DDC97)],
    ];
    if let Some(t) = aos() {
        let base = rgb(if is_dark() { t.accent_dark } else { t.accent_light });
        return match (slot, is_dark()) {
            (0, _) => base,
            (1, true) => lerp_rgb(base, Color32::WHITE, 0.30),
            (1, false) => lerp_rgb(base, Color32::BLACK, 0.15),
            (_, true) => lerp_rgb(base, Color32::WHITE, 0.55),
            (_, false) => lerp_rgb(base, Color32::BLACK, 0.30),
        };
    }
    let (l, d) = SETS[accent() as usize % SETS.len()][slot];
    rgb(if is_dark() { d } else { l })
}
/// primary accent
pub fn TEAL() -> Color32 { acc(0) }
/// secondary accent
pub fn TEAL_HI() -> Color32 { acc(1) }
pub fn GOLD() -> Color32 { TEAL() }
pub fn PURPLE() -> Color32 { TEAL_HI() }
/// highlight accent (playhead, active markers)
pub fn CYAN() -> Color32 { acc(2) }

/// piano-roll surfaces
pub fn ROLL_WHITE() -> Color32 { aos_tok(-0.12, 0.45).unwrap_or_else(|| surface(0xC3CDD1, 0xF1F5F8, 0x191E23)) }
pub fn ROLL_BLACK() -> Color32 { aos_tok(-0.40, 0.25).unwrap_or_else(|| surface(0xB2BFC5, 0xE3E9EE, 0x12161A)) }
pub fn ROLL_LINE() -> Color32 { aos_tok(0.10, -0.12).unwrap_or_else(|| pk(0xCBD4DC, 0x2A3139)) }
pub fn KEY_WHITE() -> Color32 { surface(0xD2D8D8, 0xF4F7FA, 0xDDE4EA) }
pub fn KEY_BLACK() -> Color32 { pk(0x1B2127, 0x0C0F12) }

/// bar / panel gradients (kept almost flat on purpose)
pub fn bar_top() -> Color32 { SIDE() }
pub fn bar_bot() -> Color32 { SIDE() }
pub fn side_top() -> Color32 { SIDE() }
pub fn side_bot() -> Color32 { SIDE() }

pub const RED: Color32 = Color32::from_rgb(0xFF, 0x5C, 0x5C);
pub const GREEN: Color32 = Color32::from_rgb(0x3D, 0xDC, 0x97);
pub const AMBER: Color32 = Color32::from_rgb(0xFF, 0xB0, 0x20);
pub const SKY: Color32 = Color32::from_rgb(0x4F, 0xD8, 0xF5);

/// Track colour `i`: the fixed palette, pulled a third of the way towards the accent of an AOS theme so the tracks wear its palette.
pub fn lane_col(i: usize) -> Color32 {
    let base = LANE_COLORS[i % LANE_COLORS.len()];
    match aos() {
        Some(t) => lerp_rgb(base, rgb(if is_dark() { t.accent_dark } else { t.accent_light }), 0.33),
        None => base,
    }
}

pub const LANE_COLORS: [Color32; 10] = [
    Color32::from_rgb(0xFF, 0x6B, 0x6B),
    Color32::from_rgb(0xFF, 0xA9, 0x4D),
    Color32::from_rgb(0xFF, 0x7A, 0xB6),
    Color32::from_rgb(0x4F, 0xC3, 0xF7),
    Color32::from_rgb(0xA3, 0xE0, 0x4F),
    Color32::from_rgb(0x3D, 0xDC, 0x97),
    Color32::from_rgb(0xFF, 0x8A, 0x5C),
    Color32::from_rgb(0x9A, 0x8C, 0xFF),
    Color32::from_rgb(0xFF, 0xD4, 0x3B),
    Color32::from_rgb(0x8F, 0xA3, 0xB5),
];

// ------------------------------------------------------------------ applying it to egui

pub fn apply(ctx: &Context) {
    load_prefs();
    // Segoe UI for text and Consolas for numbers, read from the system font folder; egui's built-ins stay as fallback
    let mut fonts = FontDefinitions::default();
    for (key, path, fam) in [("ui", r"C:\Windows\Fonts\segoeui.ttf", FontFamily::Proportional), ("mono", r"C:\Windows\Fonts\consola.ttf", FontFamily::Monospace)] {
        if let Ok(bytes) = std::fs::read(path) {
            fonts.font_data.insert(key.into(), FontData::from_owned(bytes));
            fonts.families.entry(fam).or_default().insert(0, key.into());
        }
    }
    // title face: Georgia italic-style accent like the AOS app titles (falls back to the proportional face)
    let serif = std::fs::read(r"C:\Windows\Fonts\georgiai.ttf").ok();
    let mut chain = vec![];
    if let Some(b) = serif {
        fonts.font_data.insert("serif".into(), FontData::from_owned(b));
        chain.push("serif".to_string());
    }
    chain.extend(fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default());
    fonts.families.insert(FontFamily::Name("serif".into()), chain);
    ctx.set_fonts(fonts);
    apply_visuals(ctx);
}

pub fn paint_glass_background(ctx: &Context) {
    let rect = ctx.screen_rect();
    let base = BG();
    let accent = TEAL();
    let violet = if is_dark() { Color32::from_rgb(90, 62, 155) } else { Color32::from_rgb(165, 144, 225) };
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.left_top(), lerp_rgb(base, accent, if is_dark() { 0.23 } else { 0.18 }));
    mesh.colored_vertex(rect.right_top(), lerp_rgb(base, violet, 0.37));
    mesh.colored_vertex(rect.right_bottom(), lerp_rgb(base, accent, if is_dark() { 0.10 } else { 0.12 }));
    mesh.colored_vertex(rect.left_bottom(), base);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    ctx.layer_painter(egui::LayerId::background()).add(egui::Shape::mesh(mesh));
}

pub fn glass_frame() -> egui::Frame {
    let surface = SIDE();
    let line = if is_dark() {
        Color32::from_white_alpha(50)
    } else if mobile_day() {
        border()
    } else {
        Color32::from_white_alpha(170)
    };
    egui::Frame::none()
        .fill(Color32::from_rgba_unmultiplied(surface.r(), surface.g(), surface.b(), 192))
        .stroke(Stroke::new(1.0, line))
        .rounding(12.0)
}

pub fn apply_visuals(ctx: &Context) {
    apply_scale(ctx);
    let dark = is_dark();
    let mut v = if dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    v.panel_fill = BG();
    // windows and menus stay opaque: glass over other windows would show their content through
    v.window_fill = PANEL2();
    v.window_stroke = Stroke::new(border_w(), border());
    v.window_rounding = Rounding::same(card() + 2.0);
    v.menu_rounding = Rounding::same(corner() + 2.0);
    v.extreme_bg_color = aos_tok(-0.65, -0.12).unwrap_or_else(|| surface(0xACB9BF, 0xDCE3E9, 0x0F1316));
    v.faint_bg_color = PANEL3().linear_multiply(0.5);
    v.override_text_color = Some(TEXT());
    v.selection.bg_fill = if dark { TEAL() } else { lerp_rgb(TEAL(), Color32::BLACK, 0.12) };
    v.selection.stroke = Stroke::new(1.0, Color32::WHITE);
    v.hyperlink_color = TEAL();
    v.slider_trailing_fill = true;
    let shadow = |blur: f32, alpha: u8| egui::epaint::Shadow { offset: egui::vec2(0.0, 6.0), blur, spread: 0.0, color: Color32::from_black_alpha(alpha) };
    v.popup_shadow = shadow(10.0, if dark { 150 } else { 40 });
    v.window_shadow = shadow(16.0, if dark { 170 } else { 50 });

    let w = &mut v.widgets;
    for x in [&mut w.noninteractive, &mut w.inactive, &mut w.hovered, &mut w.active, &mut w.open] {
        x.rounding = Rounding::same(corner());
        x.expansion = 0.0;
    }
    w.noninteractive.bg_fill = SIDE();
    w.noninteractive.weak_bg_fill = SIDE();
    w.noninteractive.bg_stroke = Stroke::new(1.0, border());
    w.noninteractive.fg_stroke = Stroke::new(1.0, DIM());
    w.inactive.bg_fill = PANEL3();
    w.inactive.weak_bg_fill = PANEL3();
    w.inactive.bg_stroke = Stroke::new(1.0, border());
    w.inactive.fg_stroke = Stroke::new(1.0, TEXT());
    // hover picks up a hint of the accent, so the whole program glows slightly in its colour
    let hov = lerp_rgb(PANEL3(), TEAL(), if dark { 0.16 } else { 0.10 });
    w.hovered.bg_fill = hov;
    w.hovered.weak_bg_fill = hov;
    w.hovered.bg_stroke = Stroke::new(1.0, TEAL());
    w.hovered.fg_stroke = Stroke::new(1.0, TEXT());
    w.active.bg_fill = lerp_rgb(TEAL(), Color32::BLACK, if dark { 0.18 } else { 0.10 });
    w.active.weak_bg_fill = w.active.bg_fill;
    w.active.bg_stroke = Stroke::new(1.0, TEAL());
    // `strong()` text uses this colour: it has to stay readable on every surface
    w.active.fg_stroke = Stroke::new(1.0, TEXT());
    w.open.bg_fill = PANEL3();
    w.open.weak_bg_fill = PANEL3();
    w.open.bg_stroke = Stroke::new(1.0, border());
    ctx.set_visuals(v);

    let mut s = (*ctx.style()).clone();
    // solid scroll bars that take their own lane: they never float over (or flicker against) the content
    s.spacing.scroll = egui::style::ScrollStyle { bar_width: 9.0, handle_min_length: 28.0, ..egui::style::ScrollStyle::solid() };
    s.animation_time = 0.12; // hover / open / close transitions of the standard widgets
    s.spacing.item_spacing = egui::vec2(8.0, 6.0);
    s.spacing.button_padding = egui::vec2(14.0, 6.0);
    s.spacing.interact_size.y = 32.0;
    s.spacing.menu_margin = egui::Margin::same(10.0);
    s.spacing.window_margin = egui::Margin::same(14.0);
    s.text_styles.insert(TextStyle::Body, FontId::proportional(14.5));
    s.text_styles.insert(TextStyle::Button, FontId::proportional(14.5));
    s.text_styles.insert(TextStyle::Small, FontId::proportional(12.0));
    s.text_styles.insert(TextStyle::Heading, FontId::proportional(21.0));
    s.text_styles.insert(TextStyle::Monospace, FontId::monospace(12.5));
    ctx.set_style(s);
}

fn lerp_rgb(a: Color32, b: Color32, t: f32) -> Color32 {
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()))
}

/// Solid control surface: a flat fill with a crisp darker edge (no gradients, no transparency).
pub fn gloss(p: &egui::Painter, r: egui::Rect, rounding: f32, fill: Color32) {
    p.rect_filled(r, rounding, fill);
    p.rect_stroke(r, rounding, Stroke::new(1.0, lerp_rgb(fill, Color32::BLACK, if is_dark() { 0.45 } else { 0.22 })));
}

/// Opaque surface tint: preserves text contrast in both appearances.
pub fn tint(base: Color32, accent: Color32, amount: f32) -> Color32 {
    lerp_rgb(base, accent, amount.clamp(0.0, 1.0))
}

/// A bordered surface: the default container for cards and grouped controls.
pub fn bevel_frame() -> egui::Frame {
    egui::Frame::none().fill(PANEL2()).rounding(Rounding::same(card())).stroke(Stroke::new(border_w(), border())).inner_margin(16.0)
}

/// Theme chooser: the classic look and the 44 AOS launcher themes by group. Applies the choice at once.
pub fn theme_picker(ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    ui.label(egui::RichText::new("Theme").small().color(DIM()));
    if ui.selectable_label(theme_index() == 0, "AOS Default - Night / Day").clicked() {
        set_theme(0);
        apply_visuals(&ctx);
    }
    egui::ScrollArea::vertical().id_salt("aos_theme_scroll").max_height(260.0).auto_shrink([false, true]).show(ui, |ui| {
        for (g, group) in GROUPS.iter().enumerate() {
            ui.label(egui::RichText::new(*group).small().strong().color(DIM()));
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
                for (i, t) in AOS_THEMES.iter().enumerate().filter(|(_, t)| t.group as usize == g) {
                    let on = theme_index() as usize == i + 1;
                    let (r, resp) = ui.allocate_exact_size(egui::vec2(104.0, 24.0), egui::Sense::click());
                    let p = ui.painter();
                    p.rect_filled(r, corner(), if on { tint(PANEL2(), TEAL(), 0.3) } else if resp.hovered() { PANEL3() } else { PANEL2() });
                    p.rect_stroke(r, corner(), Stroke::new(border_w(), if on { TEAL() } else { border() }));
                    p.circle_filled(r.left_center() + egui::vec2(12.0, 0.0), 6.0, rgb(if is_dark() { t.accent_dark } else { t.accent_light }));
                    p.text(r.left_center() + egui::vec2(24.0, 0.0), egui::Align2::LEFT_CENTER, t.name, FontId::proportional(11.5), TEXT());
                    if resp.clicked() {
                        set_theme(i as u8 + 1);
                        apply_visuals(&ctx);
                    }
                }
            });
        }
    });
}
