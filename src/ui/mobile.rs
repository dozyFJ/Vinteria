//! Phone layout in the style of the Pulse messenger (the PC keeps its own AOS look).
//! Phone navigation and surfaces shared across the mobile screens.
use super::*;
use crate::mixer::{NUM_CH, NUM_INSERTS};
use crate::sequencer::{DRUM_NAMES, NUM_DRUMS, TICKS_PER_STEP};
use egui::{Align2, FontId, Frame, Margin, Painter, Pos2, Rect, Response, RichText, Sense, Shape, Ui};

const TOUCH: f32 = 44.0;
const GRID_VALUES: [(u32, &str); 6] = [
    (TICKS_PER_BAR, "Bar"),
    (TICKS_PER_BAR / 2, "1/2"),
    (TICKS_PER_BAR / 4, "1/4"),
    (TICKS_PER_BAR / 8, "1/8"),
    (TICKS_PER_STEP, "1/16"),
    (TICKS_PER_STEP / 2, "1/32"),
];

fn grid_label(ticks: u32) -> &'static str {
    GRID_VALUES.iter().find(|&&(step, _)| step == ticks)
        .map_or(if ticks == 1 { "Off" } else { "Fine" }, |&(_, label)| label)
}

/// At low zoom, draw a subset of the snap lines while retaining the editing resolution.
fn visible_grid_step(snap: u32, pixels_per_tick: f32) -> u32 {
    let mut step = snap.max(1);
    while step < TICKS_PER_BAR && step as f32 * pixels_per_tick < 7.0 {
        step *= 2;
    }
    step
}
/// written by the Android panic hook, shown on the next start
pub const CRASH_FILE: &str = "crash.txt";

// ───────────────────── Pulse palette ─────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum MobileTheme {
    Winteria,
    Graphite,
    Day,
    DarkGraphite,
    ToxicDark,
}

impl MobileTheme {
    const ALL: [Self; 5] = [Self::Winteria, Self::Day, Self::DarkGraphite, Self::ToxicDark, Self::Graphite];

    #[cfg(target_arch = "wasm32")]
    const KEY: &'static str = "winteria:mobile-theme";
    #[cfg(not(target_arch = "wasm32"))]
    const FILE: &'static str = "mobile-theme.txt";

    fn name(self) -> &'static str {
        match self {
            Self::Winteria => "Winteria",
            Self::Graphite => "Graphite Silver",
            Self::Day => "Winteria Light",
            Self::DarkGraphite => "Dark Graphite",
            Self::ToxicDark => "Toxic Dark",
        }
    }

    fn stored() -> &'static std::sync::atomic::AtomicU8 {
        static THEME: std::sync::OnceLock<std::sync::atomic::AtomicU8> = std::sync::OnceLock::new();
        THEME.get_or_init(|| {
            #[cfg(target_arch = "wasm32")]
            let saved = Self::browser_storage().and_then(|storage| storage.get_item(Self::KEY).ok().flatten());
            #[cfg(not(target_arch = "wasm32"))]
            let saved = std::fs::read_to_string(Self::FILE).ok();
            let theme = Self::ALL.into_iter()
                .find(|theme| saved.as_deref().map(str::trim).map(|s| if s == "Day" { "Winteria Light" } else { s }) == Some(theme.name()))
                .unwrap_or(if saved.is_none() && !is_dark() { Self::Day } else { Self::Winteria });
            std::sync::atomic::AtomicU8::new(theme as u8)
        })
    }

    fn current() -> Self {
        let value = Self::stored().load(Ordering::Relaxed);
        Self::ALL.into_iter().find(|theme| *theme as u8 == value).unwrap_or(Self::Winteria)
    }

    fn save(self) {
        Self::stored().store(self as u8, Ordering::Relaxed);
        #[cfg(target_arch = "wasm32")]
        if let Some(storage) = Self::browser_storage() {
            let _ = storage.set_item(Self::KEY, self.name());
        }
        #[cfg(not(target_arch = "wasm32"))]
        let _ = std::fs::write(Self::FILE, self.name());
    }

    #[cfg(target_arch = "wasm32")]
    fn browser_storage() -> Option<web_sys::Storage> {
        web_sys::window()?.local_storage().ok().flatten()
    }
}

/// Track / clip colours of the phone UI, one palette per theme (the PC keeps theme::lane_col). Shadows the glob import.
fn lane_col(i: usize) -> Color32 {
    const WINTERIA: [u32; 10] = [0xB9B3FE, 0xFF9ABE, 0x73CAF3, 0xD0A9FF, 0x61D6B6, 0xF5AA81, 0xA6BBFE, 0xE99DF5, 0x7ED67E, 0xE3B75E];
    const LIGHT: [u32; 10] = [0x7D73E7, 0xD25383, 0x1690CA, 0xA066D6, 0x069D7D, 0xC7692D, 0x5F7EE4, 0xB45DC2, 0x489C48, 0xAF7B05];
    const SILVER: [u32; 10] = [0x9EBEE0, 0xD4B491, 0xCDAED2, 0xA5C5A1, 0xDEADA1, 0x8EC5D2, 0xB7B6E0, 0xBFBE91, 0xDCABBA, 0x91C8BA];
    const GRAPHITE: [u32; 10] = [0xCEA98B, 0x8FB7D4, 0xA3BA92, 0xBEA7CD, 0x83BDC3, 0xD5A39F, 0xBCB286, 0xA6AED8, 0x8CBEAA, 0xCFA2B8];
    const TOXIC: [u32; 10] = [0xAFD46A, 0xD5B2FF, 0x11DFE3, 0xFEAAAD, 0xADC2FE, 0xE9BF4A, 0x62E0A6, 0xFFA0DE, 0x77D0FF, 0xFEAF7D];
    let palette = match MobileTheme::current() {
        MobileTheme::Winteria => &WINTERIA,
        MobileTheme::Day => &LIGHT,
        MobileTheme::Graphite => &SILVER,
        MobileTheme::DarkGraphite => &GRAPHITE,
        MobileTheme::ToxicDark => &TOXIC,
    };
    argb(0xFF00_0000 | palette[i % palette.len()])
}

fn argb(v: u32) -> Color32 {
    Color32::from_rgba_unmultiplied((v >> 16) as u8, (v >> 8) as u8, v as u8, (v >> 24) as u8)
}
fn pick(dark: u32, light: u32) -> Color32 {
    argb(if MobileTheme::current() == MobileTheme::Day { light } else { dark })
}
fn mobile_color(winteria: u32, silver: u32, day: u32, graphite: u32, toxic: u32) -> Color32 {
    match MobileTheme::current() {
        MobileTheme::Winteria => argb(winteria),
        MobileTheme::Graphite => argb(silver),
        MobileTheme::Day => argb(day),
        MobileTheme::DarkGraphite => argb(graphite),
        MobileTheme::ToxicDark => argb(toxic),
    }
}

/// main text
fn ink() -> Color32 {
    mobile_color(0xFFE7ECFF, 0xFFF6F8FB, 0xFF241E3D, 0xFFDDDAD4, 0xFFE6ECDD)
}
/// secondary text
fn muted() -> Color32 {
    mobile_color(0xFFBFC1DC, 0xFFD0D6DE, 0xFF625A80, 0xFFA8A6A0, 0xFFA5B497)
}
/// Silver accent from Chat Pulse in dark mode.
fn p_accent() -> Color32 {
    mobile_color(0xFFA79CFF, 0xFF9FC3E8, 0xFF5B4BD6, 0xFFD9A066, 0xFFA8D861)
}
fn on_accent() -> Color32 {
    mobile_color(0xFF17122C, 0xFF14202D, 0xFFFFFFFF, 0xFF1D1D20, 0xFF15200B)
}
fn p_bg() -> Color32 {
    mobile_color(0xFF111320, 0xFF191A1C, 0xFFF0ECFA, 0xFF161618, 0xFF12170F)
}
fn bar_fill() -> Color32 {
    mobile_color(0xFF1C1A2A, 0xFF2B3036, 0xFFE3DCF6, 0xFF202023, 0xFF1B2317)
}
fn surface_fill() -> Color32 {
    mobile_color(0xFF2D2648, 0xFF3C444E, 0xFFFAF8FF, 0xFF2E2E31, 0xFF27341F)
}

fn surface_edge() -> Color32 {
    mobile_color(0xFF75669D, 0xFF929FAA, 0xFF9E92D6, 0xFF686763, 0xFF59734A)
}

fn piano_note_fill() -> Color32 {
    mobile_color(0xFFC9C0FF, 0xFFDDE4EC, 0xFFB4A8F5, 0xFFBCB8AF, 0xFFA5CB69)
}

fn outline() -> Color32 {
    mobile_color(0xFF605382, 0xFF67737F, 0xFF8F83C2, 0xFF4D4B48, 0xFF41543A)
}
fn card_fill() -> Color32 {
    mobile_color(0xFF252139, 0xFF323941, 0xFFFFFFFF, 0xFF29292C, 0xFF242D1D)
}
fn selected_fill() -> Color32 {
    mobile_color(0xFF463A80, 0xFF5A6878, 0xFFD4C9F7, 0xFF4A4845, 0xFF44592F)
}
fn grid_line() -> Color32 {
    mobile_color(0xFF443A60, 0xFF525E6B, 0xFFC3BAE3, 0xFF383838, 0xFF374431)
}

fn panel(p: &Painter, rect: Rect, radius: f32) {
    p.rect_filled(rect, radius, card_fill());
    p.rect_stroke(rect, radius, Stroke::new(1.0, grid_line()));
}

fn pill(p: &Painter, rect: Rect, fill: Color32) {
    p.rect_filled(rect, rect.height() * 0.5, fill);
}

fn mobile_background(ctx: &egui::Context) {
    ctx.layer_painter(egui::LayerId::background()).rect_filled(ctx.screen_rect(), 0.0, p_bg());
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
    Forward,
    Repeat,
    Play,
    Pause,
    Record,
    Mic,
    Stop,
    Plus,
    Session,
    Share,
    Undo,
    Redo,
    Menu,
    Settings,
    Pencil,
    SelectBox,
    CloneClips,
    Minus,
    Trash,
}

fn icon(p: &Painter, kind: Icon, c: Pos2, col: Color32) {
    let s = Stroke::new(1.8, col);
    match kind {
        Icon::Repeat => {
            p.add(Shape::line(vec![
                c + vec2(-9.0, 0.0), c + vec2(-9.0, -5.0), c + vec2(9.0, -5.0),
            ], s));
            p.add(Shape::line(vec![
                c + vec2(5.0, -9.0), c + vec2(9.0, -5.0), c + vec2(5.0, -1.0),
            ], s));
            p.add(Shape::line(vec![
                c + vec2(9.0, 0.0), c + vec2(9.0, 5.0), c + vec2(-9.0, 5.0),
            ], s));
            p.add(Shape::line(vec![
                c + vec2(-5.0, 1.0), c + vec2(-9.0, 5.0), c + vec2(-5.0, 9.0),
            ], s));
        }
        Icon::Mic => {
            p.rect_stroke(Rect::from_center_size(c - vec2(0.0, 3.0), vec2(7.0, 12.0)), 3.5, s);
            p.add(Shape::line(vec![
                c + vec2(-7.0, -1.0), c + vec2(-6.0, 3.0), c + vec2(-3.0, 6.0),
                c + vec2(3.0, 6.0), c + vec2(6.0, 3.0), c + vec2(7.0, -1.0),
            ], s));
            p.line_segment([c + vec2(0.0, 6.0), c + vec2(0.0, 10.0)], s);
            p.line_segment([c + vec2(-4.0, 10.0), c + vec2(4.0, 10.0)], s);
        }
        Icon::Minus => {
            p.line_segment([c - vec2(8.0, 0.0), c + vec2(8.0, 0.0)], s);
        }
        Icon::Trash => {
            let bin = Rect::from_center_size(c + vec2(0.0, 2.0), vec2(11.0, 13.0));
            p.rect_stroke(bin, 2.0, s);
            p.line_segment([c + vec2(-8.0, -5.0), c + vec2(8.0, -5.0)], s);
            p.line_segment([c + vec2(-3.0, -8.0), c + vec2(3.0, -8.0)], s);
        }
        Icon::SelectBox => {
            for (x, y) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                let corner = c + vec2(x * 9.0, y * 9.0);
                p.line_segment([corner, corner - vec2(x * 5.0, 0.0)], s);
                p.line_segment([corner, corner - vec2(0.0, y * 5.0)], s);
            }
        }
        Icon::CloneClips => {
            let back = Rect::from_center_size(c + vec2(3.0, -3.0), vec2(12.0, 13.0));
            let front = Rect::from_center_size(c + vec2(-3.0, 3.0), vec2(12.0, 13.0));
            p.line_segment([back.left_top(), back.right_top()], s);
            p.line_segment([back.right_top(), back.right_bottom()], s);
            p.rect_stroke(front, 2.0, s);
        }
        Icon::Undo | Icon::Redo => {
            // a curved arrow; Redo is the mirror image of Undo
            let dir = if kind == Icon::Undo { 1.0 } else { -1.0 };
            let pts: Vec<Pos2> = (0..=10)
                .map(|i| {
                    let a = std::f32::consts::PI * (0.15 + 0.85 * i as f32 / 10.0);
                    c + vec2(-dir * 7.0 * a.cos(), 2.0 - 7.0 * a.sin())
                })
                .collect();
            let tip = pts[0];
            p.add(Shape::line(pts, s));
            p.line_segment([tip, tip + vec2(dir * 5.0, -1.0)], s);
            p.line_segment([tip, tip + vec2(dir * 1.0, -5.0)], s);
        }
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
        Icon::Pencil => {
            let pts = [vec2(-8.0, 8.0), vec2(-6.5, 2.5), vec2(4.5, -8.5), vec2(8.5, -4.5), vec2(-2.5, 6.5)];
            let line: Vec<_> = pts.into_iter().map(|point| c + point).collect();
            p.add(Shape::closed_line(line, s));
            p.line_segment([c + vec2(2.0, -6.0), c + vec2(6.0, -2.0)], s);
        }
        Icon::Settings => {
            p.circle_stroke(c, 6.0, s);
            p.circle_stroke(c, 2.0, s);
            for tooth in 0..8 {
                let angle = tooth as f32 * std::f32::consts::FRAC_PI_4;
                let direction = vec2(angle.cos(), angle.sin());
                p.line_segment([c + direction * 6.0, c + direction * 9.0], s);
            }
        }
        Icon::Rewind | Icon::Forward => {
            let d = if kind == Icon::Rewind { 1.0 } else { -1.0 };
            p.line_segment([c + vec2(-8.0 * d, -7.0), c + vec2(-8.0 * d, 7.0)], s);
            p.add(Shape::convex_polygon(vec![c + vec2(5.0 * d, -7.0), c + vec2(-5.0 * d, 0.0), c + vec2(5.0 * d, 7.0)], col, Stroke::NONE));
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
        Icon::Menu => {
            for dy in [-6.0, 0.0, 6.0] {
                p.line_segment([c + vec2(-9.0, dy), c + vec2(9.0, dy)], Stroke::new(2.0, col));
            }
        }
    }
}

/// Section title inside the scrolling body.
fn home_section_title(ui: &mut Ui, title: &str) {
    ui.label(RichText::new(title).size(17.0).strong().color(ink()));
}

fn heading(ui: &mut Ui, text: &str) {
    ui.add_space(4.0);
    ui.label(RichText::new(text.to_uppercase()).size(12.0).strong().color(muted()));
    ui.add_space(4.0);
}

/// List row: coloured badge with a letter, title, subtitle and a right-hand note.
fn list_row(ui: &mut Ui, badge: Color32, letter: &str, title: &str, sub: &str, note: &str, selected: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 64.0), Sense::click());
    let p = ui.painter();
    if selected {
        p.rect_filled(rect, 14.0, selected_fill());
    } else {
        panel(p, rect, 14.0);
    }
    let b = Rect::from_center_size(pos2(rect.left() + 32.0, rect.center().y), vec2(42.0, 42.0));
    p.circle_filled(b.center(), 21.0, badge.linear_multiply(0.85));
    p.text(b.center(), Align2::CENTER_CENTER, letter, FontId::proportional(18.0), pick(0xFFFFFFFF, 0xFF241E3D));
    p.text(pos2(rect.left() + 64.0, rect.top() + 14.0), Align2::LEFT_TOP, title, FontId::proportional(16.0), ink());
    if !sub.is_empty() {
        p.text(pos2(rect.left() + 64.0, rect.top() + 36.0), Align2::LEFT_TOP, sub, FontId::proportional(12.0), muted());
    }
    if note == "Add" {
        // an action, drawn as a button rather than a caption
        let pill = Rect::from_min_size(pos2(rect.right() - 66.0, rect.center().y - 16.0), vec2(52.0, 32.0));
        p.rect_filled(pill, 16.0, if resp.is_pointer_button_down_on() { p_accent() } else { p_accent().gamma_multiply(0.22) });
        p.rect_stroke(pill, 16.0, Stroke::new(1.0, p_accent()));
        p.text(pill.center(), Align2::CENTER_CENTER, note, FontId::proportional(13.0),
            if resp.is_pointer_button_down_on() { on_accent() } else { ink() });
    } else if !note.is_empty() {
        p.text(pos2(rect.right() - 14.0, rect.top() + 16.0), Align2::RIGHT_TOP, note, FontId::proportional(11.0), muted());
    }
    ui.add_space(8.0);
    resp
}

/// Up to two letters for a badge: "Poly Synth" -> "PS", "808 Bass" -> "8B", "Filter" -> "Fi".
fn initials(name: &str) -> String {
    let words: Vec<&str> = name.split(|c: char| c.is_whitespace() || c == '(' || c == '-').filter(|w| !w.is_empty()).collect();
    match words.as_slice() {
        [one] => one.chars().take(2).collect(),
        [a, b, ..] => a.chars().take(1).chain(b.chars().take(1)).collect::<String>().to_uppercase(),
        [] => "?".into(),
    }
}

fn control_state(p: &Painter, rect: Rect, radius: f32, response: &Response, foreground: Color32) {
    let opacity = if response.is_pointer_button_down_on() {
        0.14
    } else if response.hovered() || response.has_focus() {
        0.08
    } else {
        0.0
    };
    if opacity > 0.0 {
        p.rect_filled(rect, radius, foreground.gamma_multiply(opacity));
    }
}

/// Filled primary action or outlined tonal button.
pub(super) fn p_button(ui: &mut Ui, text: &str, primary: bool, size: egui::Vec2) -> Response {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let p = ui.painter();
    let radius = rect.height() * 0.5;
    let foreground = if primary { on_accent() } else { ink() };
    p.rect_filled(rect, radius, if primary { p_accent() } else { card_fill() });
    control_state(p, rect, radius, &resp, foreground);
    if !primary || resp.has_focus() {
        let edge = if resp.has_focus() { p_accent() } else { outline() };
        p.rect_stroke(rect.shrink(0.5), radius, Stroke::new(1.0, edge));
    }
    p.text(rect.center(), Align2::CENTER_CENTER, text, FontId::proportional(14.0), foreground);
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

/// One-finger scrolling of a zoomable grid with its own fling. egui's drag-to-scroll keeps its fling speed while a pinch
/// switches it off and replays it when the fingers lift, so the view flew away after every zoom; this one is cancelled
/// by a pinch. `blocked(press)` says the press grabbed something (a note, the selected clip) instead of the view.
#[derive(Clone, Copy, Default)]
struct TouchPan {
    origin: Pos2,
    start: egui::Vec2,
    vel: egui::Vec2,
    active: bool,
    moved: bool,
}

fn touch_pan(ui: &Ui, id: egui::Id, grid: Rect, content: egui::Vec2, offset: egui::Vec2, enabled: bool,
    blocked: impl Fn(Pos2) -> bool) -> egui::Vec2
{
    let mut pan = ui.data(|d| d.get_temp::<TouchPan>(id)).unwrap_or_default();
    let max = (content - grid.size()).max(egui::Vec2::ZERO);
    let (pressed, down, origin, pos, velocity, dt) = ui.input(|i| (i.pointer.any_pressed(), i.pointer.primary_down(),
        i.pointer.press_origin(), i.pointer.interact_pos(), i.pointer.velocity(), i.stable_dt.min(0.1)));
    let mut offset = offset;
    if !enabled {
        pan = TouchPan::default();
    } else if pressed {
        pan = match origin.filter(|o| grid.contains(*o) && !blocked(*o)) {
            Some(origin) => TouchPan { origin, start: offset, active: true, ..Default::default() },
            None => TouchPan::default(),
        };
    } else if pan.active && down {
        if let Some(pos) = pos {
            let moved = pos - pan.origin;
            pan.moved |= moved.length() > 6.0;
            if pan.moved {
                offset = pan.start - moved;
                pan.vel = velocity;
            }
        }
    } else if pan.active {
        pan.active = false;
        if !pan.moved || pan.vel.length() < 60.0 {
            pan.vel = egui::Vec2::ZERO;
        }
    } else if pan.vel != egui::Vec2::ZERO {
        offset -= pan.vel * dt;
        let speed = pan.vel.length();
        let slower = (speed - 2400.0 * dt).max(0.0);
        pan.vel = if slower < 30.0 { egui::Vec2::ZERO } else { pan.vel * (slower / speed) };
        ui.ctx().request_repaint();
    }
    let clamped = offset.clamp(egui::Vec2::ZERO, max);
    if clamped != offset {
        // hit an edge: stop the fling on that axis
        if clamped.x != offset.x { pan.vel.x = 0.0; }
        if clamped.y != offset.y { pan.vel.y = 0.0; }
    }
    ui.data_mut(|d| d.insert_temp(id, pan));
    clamped
}

/// egui widgets (sliders, text fields, drag values) restyled with solid fills.
#[derive(Clone, Default)]
struct GridPinch {
    touches: Vec<(egui::TouchId, Pos2)>,
    axis: Option<usize>,
    baseline: Option<egui::Vec2>,
    previous: egui::Vec2,
    suppressed: bool,
}

fn grid_pinch(ui: &Ui, id: egui::Id, grid: Rect) -> (bool, Option<(usize, f32, Pos2)>) {
    let mut state = ui.data_mut(|d| d.remove_temp::<GridPinch>(id)).unwrap_or_default();
    let (pressed, ended) = ui.input(|input| {
        let mut ended = false;
        for event in &input.events {
            if let egui::Event::Touch { id, phase, pos, .. } = event {
                match phase {
                    egui::TouchPhase::Start if grid.contains(*pos) => state.touches.push((*id, *pos)),
                    egui::TouchPhase::Move => {
                        if let Some((_, active)) = state.touches.iter_mut().find(|(active, _)| active == id) {
                            *active = *pos;
                        }
                    }
                    egui::TouchPhase::End | egui::TouchPhase::Cancel => {
                        if let Some(index) = state.touches.iter().position(|(active, _)| active == id) {
                            state.touches.remove(index);
                            ended = true;
                        }
                    }
                    _ => {}
                }
            }
        }
        (input.pointer.primary_down(), ended)
    });
    let mut zoom = None;
    if let [(_, a), (_, b)] = state.touches.as_slice() {
        state.suppressed = true;
        let span = (*a - *b).abs();
        if ended {
            state.baseline = None;
            state.axis = None;
        }
        let baseline = *state.baseline.get_or_insert(span);
        let delta = (span - baseline).abs();
        if state.axis.is_none() && delta.max_elem() >= 16.0 {
            state.axis = Some(usize::from(delta.y > delta.x));
            state.previous = baseline;
        }
        if let Some(axis) = state.axis {
            let factor = span[axis].max(12.0) / state.previous[axis].max(12.0);
            zoom = Some((axis, factor, a.lerp(*b, 0.5)));
        }
        state.previous = span;
    } else {
        state.baseline = None;
        state.axis = None;
    }
    let suppress = state.suppressed;
    if pressed || !state.touches.is_empty() {
        ui.data_mut(|d| d.insert_temp(id, state));
    }
    (suppress, zoom)
}

#[derive(Clone, Copy)]
enum PlaylistRow {
    Track(usize),
    Group(u16),
    FxAutomation(usize, usize),
}

#[derive(Clone, Copy)]
struct PlaylistTrackDrag(usize);

fn mobile_scroll(area: egui::ScrollArea) -> egui::ScrollArea {
    area.scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
}

fn mobile_visuals(ui: &mut Ui) {
    // Shared desktop editors also inherit this style while embedded in a mobile sheet.
    let scroll = &mut ui.spacing_mut().scroll;
    scroll.bar_width = 0.0;
    scroll.floating_width = 0.0;
    scroll.floating_allocated_width = 0.0;
    scroll.bar_inner_margin = 0.0;
    scroll.bar_outer_margin = 0.0;
    let v = ui.visuals_mut();
    v.dark_mode = MobileTheme::current() != MobileTheme::Day;
    v.override_text_color = Some(ink());
    v.window_fill = card_fill();
    v.panel_fill = p_bg();
    v.extreme_bg_color = mobile_color(0xFF171523, 0xFF212327, 0xFFD9D2F3, 0xFF19191B, 0xFF171D13);
    v.selection.bg_fill = selected_fill();
    v.selection.stroke = Stroke::new(1.0, p_accent());
    for (widget, fill) in [
        (&mut v.widgets.inactive, card_fill()),
        (&mut v.widgets.hovered, mobile_color(0xFF524485, 0xFF414B57, 0xFFD6CEF8, 0xFF333234, 0xFF303F26)),
        (&mut v.widgets.active, selected_fill()),
        (&mut v.widgets.open, bar_fill()),
    ] {
        widget.bg_fill = fill;
        widget.weak_bg_fill = fill;
        widget.bg_stroke = Stroke::new(1.0, outline());
        widget.rounding = egui::Rounding::same(10.0);
        widget.fg_stroke = Stroke::new(1.5, ink());
    }
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, ink());
    v.slider_trailing_fill = true;
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum MobileMixerChannel {
    Track(usize),
    Insert(usize),
    Master,
}

#[derive(Clone, Copy, PartialEq)]
enum MobileMixerPage {
    Effects,
    Master,
}

const MIXER_CHANNEL: &str = "m_mixer_channel";
const MIXER_PAGE: &str = "m_mixer_page";

const STUDIO_SECTIONS: [(&str, Icon); 6] = [
    ("Playlist", Icon::Timeline), ("Steps", Icon::Steps), ("Piano", Icon::Keys),
    ("Launchpad", Icon::Session), ("Mixer", Icon::Mixer), ("Tracks", Icon::Tracks),
];
const SEC_PLAYLIST: usize = 0;
const CONTROLS_DRAWER: &str = "m_landscape_controls_drawer";
/// set by the press that closed the clip action row, so the same tap does not also act on an empty Playlist cell
const CLIP_MENU_DISMISSED: &str = "m_playlist_clip_menu_dismissed";
/// pass number of the frame in which the landscape Playlist dock was drawn
const PLAYLIST_DOCK_PASS: &str = "m_playlist_dock_pass";
const MIXER_SHEET: &str = "m_mixer_sheet_open";

/// one line of the navigation drawer: icon + label, the active one on a filled pill
fn option_row(ui: &mut Ui, label: &str, value: &str) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 37.0), Sense::click());
    let painter = ui.painter();
    painter.rect_filled(rect, 8.0, if response.is_pointer_button_down_on() { selected_fill() } else { card_fill() });
    painter.text(rect.left_center() + vec2(10.0, 0.0), Align2::LEFT_CENTER, label, FontId::proportional(14.0), ink());
    painter.text(rect.right_center() - vec2(10.0, 0.0), Align2::RIGHT_CENTER, value, FontId::proportional(13.0), p_accent());
    response
}

fn drawer_row(ui: &mut Ui, symbol: Icon, label: &str, active: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 48.0), Sense::click());
    let p = ui.painter();
    if active {
        p.rect_filled(rect, 12.0, selected_fill());
    } else if resp.is_pointer_button_down_on() {
        p.rect_filled(rect, 12.0, card_fill());
    }
    let color = if matches!(symbol, Icon::Trash) { RED } else { ink() };
    icon(p, symbol, rect.left_center() + vec2(26.0, 0.0), color);
    p.text(rect.left_center() + vec2(52.0, 0.0), Align2::LEFT_CENTER, label, FontId::proportional(16.0), color);
    resp
}
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
        cfg!(any(target_os = "android", target_arch = "wasm32")) || std::env::var_os("SYNTHESIA_MOBILE").is_some()
    }

    pub(super) fn mobile_ui(&mut self, ctx: &egui::Context) {
        super::theme::set_mobile_accent(Some(p_accent()));
        let crash_id = egui::Id::new("m_crash_report");
        if !ctx.data(|d| d.get_temp::<bool>(egui::Id::new("m_crash_checked")).unwrap_or(false)) {
            ctx.data_mut(|d| d.insert_temp(egui::Id::new("m_crash_checked"), true));
            if let Ok(text) = std::fs::read_to_string(CRASH_FILE) {
                ctx.data_mut(|d| d.insert_temp(crash_id, text));
            }
        }
        if let Some(report) = ctx.data(|d| d.get_temp::<String>(crash_id)) {
            mobile_background(ctx);
            self.m_crash_screen(ctx, &report, crash_id);
            return;
        }
        if crate::safe_mode() {
            mobile_background(ctx);
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
            mobile_visuals(ui);
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
            mobile_visuals(ui);
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
        #[cfg(target_os = "android")]
        {
            let pending = egui::Id::new("m_sound_import_pending");
            // also polled without a pending flag (twice a second): after Android restarted the app behind the
            // file picker the import still finishes, and its result must reach the library
            let poll = egui::Id::new("m_sound_import_poll");
            let now = ctx.input(|i| i.time);
            let due = now - ctx.data(|d| d.get_temp::<f64>(poll)).unwrap_or(f64::MIN) > 0.5;
            if due {
                if let Some(result) = self.android_updater.as_ref().and_then(|android| android.take_project_import_result()) {
                    let status = match result.split_once('\n') {
                        Some((name, path)) => self.m_finish_project_import(name, std::path::Path::new(path)),
                        None if result == "Project import canceled." => Ok(result),
                        None => Err(result),
                    };
                    self.hub_cache = None;
                    ctx.data_mut(|d| d.insert_temp(egui::Id::new("m_project_import_status"), status));
                }
            }
            if ctx.data(|d| d.get_temp::<bool>(pending)).unwrap_or(false) || due {
                ctx.data_mut(|d| d.insert_temp(poll, now));
                if let Some(status) = self.android_updater.as_ref().and_then(|android| android.take_sound_import_status()) {
                    self.lib.rescan();
                    ctx.data_mut(|d| {
                        d.remove::<bool>(pending);
                        d.remove::<Vec<(String, String)>>(egui::Id::new("m_sounds_cache"));
                        d.insert_temp(egui::Id::new("m_sound_import_status"), status);
                    });
                }
            }
        }
        step("background");
        #[cfg(target_arch = "wasm32")]
        ctx.style_mut(|style| style.animation_time = 0.12);
        mobile_background(ctx);
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
                if self.m_tab == 1 {
                    self.m_studio_transport_dock(ctx);
                    self.m_studio_portrait_sidebar(ctx);
                    self.m_mixer_sheet(ctx);
                    self.m_studio_project_drawer(ctx);
                }
            } else if self.m_sec != SEC_PIANO {
                self.m_studio_sidebar(ctx);
                if self.m_sec == SEC_PLAYLIST {
                    self.m_landscape_playlist_dock(ctx);
                }
                if self.m_tab == 1 {
                    self.m_landscape_controls_drawer(ctx);
                }
                self.m_mixer_sheet(ctx);
            }
        } else {
            self.mobile_header(ctx);
            self.m_hub_navigation(ctx);
        }
        if self.m_tab == 1 && self.m_sec == SEC_PLAYLIST {
            if !full_view && !self.m_clip_piano_open(ctx) {
                self.m_browser_sidebar(ctx);
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
            mobile_visuals(ui);
            match self.m_tab {
                0 => self.m_projects(ui),
                1 => self.m_studio(ui),
                2 => self.m_settings(ui, ctx),
                3 => self.m_profile(ui),
                4 => self.m_home(ui),
                _ => self.m_plugin_store(ui),
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
                panel(&painter, rect, 9.0);
                let short: String = label.chars().take(15).collect();
                painter.text(rect.center(), Align2::CENTER_CENTER, short, FontId::proportional(12.0), ink());
            }
        }
        if self.m_tab == 1 && portrait && !full_view {
            self.m_studio_menu_overlay(ctx);
        } else {
            ctx.data_mut(|d| d.remove::<bool>(egui::Id::new("m_studio_menu_overlay")));
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
        let piano_visible = self.m_tab == 1 && (self.m_sec == SEC_PIANO || self.m_clip_piano_open(ctx));
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
        // ~60 fps while playing so the playhead glides
        let repaint = if self.playing() { 16 } else { 250 };
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
        let resp = egui::TopBottomPanel::top("m_header").show_separator_line(false).frame(Frame::none().fill(surface_fill()).inner_margin(Margin::symmetric(14.0, 10.0))).show(ctx, |ui| {
            mobile_visuals(ui);
            let t = self.m_tab;
            ui.horizontal(|ui| {
                ui.set_min_height(TOUCH);
                if matches!(t, 2 | 3) && p_button(ui, "‹", false, vec2(34.0, 38.0)).on_hover_text("Back to Home").clicked() {
                    self.m_tab = 4;
                    self.m_sec = 0;
                }
                let title = match (t, self.m_sec) {
                    (0, 1) => "Recent",
                    (0, 2) => "Templates",
                    (0, 3) => "Sounds",
                    (0, 4) => "Backups",
                    (0, _) => "Projects",
                    (2, _) => "Settings",
                    (4, _) => "Home",
                    (5, _) => "Plugin Store",
                    _ => "Profile",
                };
                ui.label(RichText::new(title).size(22.0).strong().color(ink()));
                if matches!(t, 0 | 4) {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let (r, resp) = ui.allocate_exact_size(vec2(TOUCH, TOUCH), Sense::click());
                        pill(ui.painter(), r, p_accent());
                        icon(ui.painter(), Icon::Plus, r.center(), on_accent());
                        if resp.on_hover_text("New project").clicked() && self.m_save_pending_project(ctx) {
                            self.new_project();
                            if self.m_save_project_to_list(ctx) {
                                self.m_open_studio(0);
                            }
                        }
                        ui.menu_button("More", |ui| {
                            if ui.button("Backups").clicked() {
                                self.m_tab = 0;
                                self.m_sec = 4;
                                ui.close_menu();
                            }
                            ui.separator();
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
            .frame(Frame::none().fill(surface_fill()).inner_margin(Margin::symmetric(4.0, 5.0)))
            .show(ctx, |ui| {
                mobile_visuals(ui);
                let width = ui.available_width();
                if self.m_studio_exit(ui, ctx, width) {
                    return;
                }
                self.m_studio_full_view_button(ui, ctx, width);
                self.m_studio_orientation_button(ui, ctx, width);
                if self.m_sec == SEC_PLAYLIST {
                    self.m_studio_browser_button(ui, ctx, width);
                } else {
                    self.m_studio_undo_redo(ui, width, false);
                }
            });
    }

    /// Select, Clone, Undo and Redo controls shared by both Playlist docks.
    fn m_playlist_tool_buttons(&mut self, ui: &mut Ui, captions: bool) {
        let select_id = egui::Id::new("m_playlist_select");
        let mut select_mode = ui.data(|d| d.get_temp::<bool>(select_id)).unwrap_or(false) && !self.arr.auto_edit;
        ui.spacing_mut().item_spacing.x = 2.0;
        if studio_icon(ui, 44.0, Icon::SelectBox, select_mode, "Select clips").clicked() {
            select_mode = !select_mode;
            self.arr.auto_edit = false;
            self.arr.sel_clip = None;
            self.arr.sel.clear();
        }
        if captions { sidebar_caption(ui, "Select"); }
        let selection = self.arr.selection();
        let can_clone = !selection.is_empty() && selection.iter().all(|&index| {
            self.project.arrangement.clips.get(index).is_some_and(|clip|
                self.project.arrangement.tracks.get(clip.lane).is_some_and(|track| !track.playlist_locked))
        });
        ui.add_enabled_ui(can_clone, |ui| {
            if studio_icon(ui, 44.0, Icon::CloneClips, false, "Clone clips").clicked() {
                self.m_playlist_undo_step();
                self.arr.msg.clear();
                arrangement_ui::duplicate_selection(&mut self.project, &mut self.arr);
                if let Some(clip) = self.arr.sel_clip.and_then(|index| self.project.arrangement.clips.get(index)) {
                    ui.data_mut(|d| d.insert_temp(egui::Id::new("m_playlist_reveal_tick"), clip.start));
                }
            }
        });
        if captions { sidebar_caption(ui, "Clone"); }
        if self.m_studio_undo_redo(ui, 44.0, captions) {
            self.arr.sel_clip = None;
            self.arr.sel.clear();
            self.arr.msg.clear();
        }
        ui.data_mut(|d| d.insert_temp(select_id, select_mode));
    }

    /// Landscape Playlist: one bottom bar with the edit tools and every studio control (transport, tempo,
    /// automation, render, export, sections), so the Playlist needs no right-hand drawer.
    fn m_landscape_playlist_dock(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("m_playlist_dock").show_separator_line(false).exact_height(58.0)
            .frame(Frame::none().fill(surface_fill()).stroke(Stroke::new(1.0, surface_edge()))
                .inner_margin(Margin { left: 6.0, right: 6.0, top: 4.0, bottom: 4.0 }))
            .show(ctx, |ui| {
                mobile_visuals(ui);
                mobile_scroll(egui::ScrollArea::horizontal()).id_salt("m_playlist_dock_scroll")
                    .auto_shrink([false, true]).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            Frame::none().fill(card_fill()).rounding(16.0)
                                .inner_margin(Margin::symmetric(4.0, 2.0)).show(ui, |ui| {
                                    ui.horizontal(|ui| self.m_playlist_tool_buttons(ui, false));
                                });
                            ui.separator();
                            self.m_studio_controls(ui, ctx, 44.0);
                        });
                    });
            });
        let pass = ctx.cumulative_pass_nr();
        ctx.data_mut(|d| d.insert_temp(egui::Id::new(PLAYLIST_DOCK_PASS), pass));
    }

    fn m_landscape_controls_drawer(&mut self, ctx: &egui::Context) {
        let id = egui::Id::new(CONTROLS_DRAWER);
        let open = ctx.data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
        let width = 48.0;
        let screen = ctx.screen_rect();
        // the landscape Playlist shows all of these controls in its bottom dock instead
        if ctx.data(|d| d.get_temp::<u64>(egui::Id::new(PLAYLIST_DOCK_PASS))) == Some(ctx.cumulative_pass_nr()) {
            return;
        }
        let bottom = screen.bottom() - 6.0;
        let height = (screen.height() - 68.0).min(420.0).max(1.0);
        let drawer_height = 54.0 + if open { height + 2.0 } else { 0.0 };
        let position = pos2(screen.right() - width - 10.0, bottom - drawer_height);
        let rounding = egui::Rounding { nw: 18.0, sw: 18.0, ne: 0.0, se: 0.0 };
        egui::Area::new(id.with("area")).order(egui::Order::Foreground)
            .fixed_pos(position).movable(false)
            .show(ctx, |ui| {
                mobile_visuals(ui);
                Frame::none().fill(bar_fill()).rounding(rounding)
                    .stroke(Stroke::new(1.0, outline())).inner_margin(4.0)
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        if open {
                            mobile_scroll(egui::ScrollArea::vertical()).max_height(height)
                                .id_salt("m_studio_controls").auto_shrink([true, false])
                                .show(ui, |ui| self.m_studio_controls(ui, ctx, width));
                        }
                        if drawer_chevron(ui, width, open).clicked() {
                            ctx.data_mut(|d| d.insert_temp(id, !open));
                        }
                    });
            });
    }

    fn m_hub_navigation(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("m_hub_nav").show_separator_line(false)
            .frame(Frame::none().fill(surface_fill()).inner_margin(Margin { left: 8.0, right: 8.0, top: 6.0, bottom: 10.0 }))
            .show(ctx, |ui| {
                mobile_visuals(ui);
                ui.horizontal(|ui| {
                    let width = (ui.available_width() - 2.0 * ui.spacing().item_spacing.x) / 3.0;
                    let current = if matches!(self.m_tab, 0 | 5) { self.m_tab } else { 4 };
                    let destinations = [
                        (4, "Home", Icon::Layout),
                        (0, "Projects", Icon::Folder),
                        (5, "Plugin Store", Icon::Plug),
                    ];
                    for (tab, title, symbol) in destinations {
                        let (rect, response) = ui.allocate_exact_size(vec2(width, 56.0), Sense::click());
                        let selected = current == tab;
                        let icon_center = pos2(rect.center().x, rect.top() + 17.0);
                        let indicator = Rect::from_center_size(icon_center, vec2(width.min(60.0), 30.0));
                        let p = ui.painter();
                        if selected {
                            pill(p, indicator, selected_fill());
                        }
                        control_state(p, indicator, 15.0, &response, ink());
                        icon(p, symbol, icon_center, if selected { ink() } else { muted() });
                        p.text(rect.center_bottom() - vec2(0.0, 10.0), Align2::CENTER_CENTER,
                            title, FontId::proportional(12.0), if selected { ink() } else { muted() });
                        if response.on_hover_text(title).clicked() {
                            self.m_tab = tab;
                            self.m_sec = 0;
                        }
                    }
                });
            });
    }

    /// Empty project list: say so and offer the two ways to start.
    fn m_empty_projects(&mut self, ui: &mut Ui) {
        ui.label(RichText::new("No projects yet. Start one:").color(muted()));
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let width = ((ui.available_width() - 8.0) / 2.0).max(80.0);
            if p_button(ui, "+ New project", true, vec2(width, 44.0)).clicked() {
                let ctx = ui.ctx().clone();
                if self.m_save_pending_project(&ctx) {
                    self.new_project();
                    if self.m_save_project_to_list(&ctx) {
                        self.m_open_studio(0);
                    }
                }
            }
            if p_button(ui, "From template", false, vec2(width, 44.0)).clicked() {
                self.m_tab = 0;
                self.m_sec = 2;
            }
        });
    }

    fn m_home(&mut self, ui: &mut Ui) {
        mobile_scroll(egui::ScrollArea::vertical()).id_salt("m_home_scroll")
            .auto_shrink([false, false]).show(ui, |ui| {
                home_section_title(ui, "Library");
                let gap = ui.spacing().item_spacing;
                let width = (ui.available_width() - gap.x).max(2.0);
                let sounds_width = (width * 0.55).floor();
                let side_width = width - sounds_width;
                ui.horizontal_top(|ui| {
                    if Self::m_navigation_tile(ui, vec2(sounds_width, 128.0 + gap.y), Icon::Wave,
                        "Sounds", p_accent()).clicked()
                    {
                        self.m_tab = 0;
                        self.m_sec = 3;
                    }
                    ui.vertical(|ui| {
                        if Self::m_navigation_tile(ui, vec2(side_width, 64.0), Icon::Layout,
                            "Templates", lane_col(2)).clicked()
                        {
                            self.m_tab = 0;
                            self.m_sec = 2;
                        }
                        if Self::m_navigation_tile(ui, vec2(side_width, 64.0), Icon::Plug,
                            "Plugin Store", lane_col(3)).clicked()
                        {
                            self.m_tab = 5;
                            self.m_sec = 0;
                        }
                    });
                });

                ui.add_space(16.0);
                let mut open = None;
                ui.horizontal(|ui| {
                    home_section_title(ui, "Recent");
                    if !self.recent.is_empty() {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if p_button(ui, "See all", false, vec2(72.0, 30.0)).clicked() {
                                self.m_tab = 0;
                                self.m_sec = 1;
                            }
                        });
                    }
                });

                if self.recent.is_empty() {
                    ui.label(RichText::new("No recent projects.").color(muted()));
                } else {
                    let gap = ui.spacing().item_spacing.x;
                    let tile_width = ((ui.available_width() - 2.0 * gap - 18.0) / 2.0).clamp(92.0, 188.0);
                    mobile_scroll(egui::ScrollArea::horizontal()).id_salt("m_home_recent_tiles")
                        .auto_shrink([false, true]).show(ui, |ui| {
                            ui.horizontal_top(|ui| {
                                for (column, paths) in self.recent.chunks(2).enumerate() {
                                    ui.vertical(|ui| {
                                        for (row, path) in paths.iter().enumerate() {
                                            let name = path.file_stem().and_then(|name| name.to_str()).unwrap_or("Project");
                                            if Self::m_navigation_tile(ui, vec2(tile_width, 68.0), Icon::Folder,
                                                name, lane_col(column * 2 + row)).clicked()
                                            {
                                                open = Some(path.clone());
                                            }
                                        }
                                    });
                                }
                            });
                        });
                }

                ui.add_space(16.0);
                home_section_title(ui, "Projects");
                let error = ui.data(|d| d.get_temp::<String>(egui::Id::new("m_project_save_error")));
                if let Some(message) = &error {
                    ui.colored_label(RED, message);
                }
                if self.m_needs_project_save() || error.is_some() {
                    self.m_unsaved_project_actions(ui);
                }
                if self.hub_cache.get_or_insert_with(|| hub_projects(&self.recent)).is_empty() {
                    self.m_empty_projects(ui);
                }
                let projects = self.hub_cache.get_or_insert_with(|| hub_projects(&self.recent));
                let width = ui.available_width();
                for (index, (path, _, _)) in projects.iter().enumerate() {
                    let name = path.file_stem().and_then(|name| name.to_str()).unwrap_or("Project");
                    if Self::m_project_list_card(ui, width, name, lane_col(index)).clicked() {
                        open = Some(path.clone());
                    }
                }

                if let Some(path) = open {
                    if self.m_save_pending_project(ui.ctx()) && self.m_open_project_file(ui.ctx(), path, false) {
                        self.m_open_studio(SEC_PLAYLIST);
                    }
                }
            });
    }

    fn m_navigation_tile(ui: &mut Ui, size: egui::Vec2, symbol: Icon, title: &str, accent: Color32) -> egui::Response {
        let (rect, response) = ui.allocate_exact_size(size, Sense::click());
        let painter = ui.painter_at(rect);
        let featured = matches!(symbol, Icon::Wave);
        let surface = tint(card_fill(), accent, if featured { 0.34 } else { 0.24 });
        painter.rect_filled(rect, 13.0, surface);
        painter.rect_stroke(rect, 13.0, Stroke::new(1.0, tint(outline(), accent, 0.50)));
        control_state(&painter, rect, 13.0, &response, ink());
        let badge_size = if featured { 44.0 } else { 32.0 };
        let icon_center = rect.left_top() + vec2(11.0 + badge_size * 0.5, 10.0 + badge_size * 0.5);
        painter.rect_filled(Rect::from_center_size(icon_center, vec2(badge_size, badge_size)),
            9.0, if featured { p_accent() } else { tint(surface, accent, 0.45) });
        icon(&painter, symbol, icon_center, if featured { on_accent() } else { ink() });
        fit_label(&painter, rect.shrink2(vec2(11.0, 0.0)), rect.bottom() - 16.0,
            title, ink(), if featured { 17.0 } else { 13.0 });
        response.on_hover_text(title)
    }

    fn m_project_list_card(ui: &mut Ui, width: f32, title: &str, accent: Color32) -> Response {
        let (rect, response) = ui.allocate_exact_size(vec2(width, 49.0), Sense::click());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 11.0, tint(card_fill(), accent, 0.12));
        painter.rect_stroke(rect, 11.0, Stroke::new(1.0, tint(outline(), accent, 0.32)));
        control_state(&painter, rect, 11.0, &response, ink());
        let badge = Rect::from_center_size(rect.left_center() + vec2(25.0, 0.0), vec2(31.0, 31.0));
        painter.rect_filled(badge, 8.0, tint(card_fill(), accent, 0.40));
        icon(&painter, Icon::Folder, badge.center(), ink());
        let text = Rect::from_min_max(rect.left_top() + vec2(48.0, 0.0), rect.right_bottom() - vec2(30.0, 0.0));
        fit_label(&painter, text, rect.center().y, title, ink(), 14.0);
        painter.text(rect.right_center() - vec2(12.0, 0.0), Align2::RIGHT_CENTER, "›",
            FontId::proportional(20.0), muted());
        response.on_hover_text(title)
    }

    fn m_plugin_store(&mut self, ui: &mut Ui) {
        let destination_id = egui::Id::new("m_plugin_store_destination");
        let mut destination = ui.data(|d| d.get_temp::<usize>(destination_id))
            .filter(|&lane| lane < self.project.arrangement.tracks.len());
        let mut instrument = None;
        let mut effect = None;
        mobile_scroll(egui::ScrollArea::vertical()).id_salt("m_plugin_store_scroll")
            .auto_shrink([false, false]).show(ui, |ui| {
                let label = destination.map_or("New MIDI track", |lane| self.project.arrangement.tracks[lane].name.as_str());
                egui::ComboBox::from_id_salt("m_plugin_store_track").selected_text(label).show_ui(ui, |ui| {
                    let new_track_label = RichText::new("New MIDI track").color(if destination.is_none() { on_accent() } else { ink() });
                    ui.selectable_value(&mut destination, None, new_track_label);
                    for (lane, track) in self.project.arrangement.tracks.iter().enumerate() {
                        let text = RichText::new(&track.name).color(if destination == Some(lane) { on_accent() } else { ink() });
                        ui.selectable_value(&mut destination, Some(lane), text);
                    }
                });
                if !self.arr.msg.is_empty() {
                    ui.label(RichText::new(&self.arr.msg).color(RED));
                }
                heading(ui, "Built-in instruments");
                let accepts_instrument = destination.is_none_or(|lane| {
                    let track = &self.project.arrangement.tracks[lane];
                    track.kind == crate::arrangement::TrackKind::Midi && !track.playlist_locked
                });
                ui.add_enabled_ui(accepts_instrument, |ui| {
                    for (index, track) in Track::ALL.into_iter().enumerate() {
                        if list_row(ui, lane_col(index), &initials(track.name()), track.name(), "Instrument", "Add", false).clicked() {
                            instrument = Some(track);
                        }
                    }
                });
                ui.add_space(12.0);
                heading(ui, "Built-in effects");
                if destination.is_none() {
                    ui.label("Select an existing track to add an effect.");
                }
                ui.add_enabled_ui(destination.is_some(), |ui| {
                    for (index, kind) in crate::fx::ALL.into_iter().enumerate() {
                        if list_row(ui, lane_col(index), &initials(kind.name()), kind.name(), kind.category(), "Add", false).clicked() {
                            effect = Some(kind);
                        }
                    }
                });
            });
        ui.data_mut(|d| {
            if let Some(lane) = destination {
                d.insert_temp(destination_id, lane);
            } else {
                d.remove::<usize>(destination_id);
            }
        });
        let (lane, page): (usize, usize) = if let Some(track) = instrument {
            let lane = destination.unwrap_or_else(|| {
                self.project.arrangement.add_track(crate::arrangement::TrackKind::Midi);
                let lane = self.project.arrangement.tracks.len() - 1;
                self.project.arrangement.tracks[lane].name = track.name().into();
                lane
            });
            let mut detail = arrangement_ui::DetailCtx {
                roll_track: &mut self.roll.track,
                live_track: &mut self.live_track,
                inst: &mut self.inst,
            };
            devices_ui::assign(&mut self.project, lane, track, &mut detail);
            self.arr.msg.clear();
            (lane, 0)
        } else if let (Some(kind), Some(lane)) = (effect, destination) {
            self.m_drop_effect(kind, lane);
            if !self.arr.msg.is_empty() {
                return;
            }
            (lane, 2)
        } else {
            return;
        };

        self.arr.sel_lane = lane;
        self.arr.sel_clip = None;
        self.m_playlist_undo_step();
        self.m_open_studio(SEC_PLAYLIST);
        ui.data_mut(|d| {
            d.remove::<usize>(destination_id);
            d.insert_temp(egui::Id::new("m_playlist_reveal_track"), lane);
            d.insert_temp(egui::Id::new(("m_track_settings_page", lane)), page);
            d.insert_temp(egui::Id::new("m_track_sheet_open"), true);
        });
        ui.ctx().request_repaint();
    }

    /// Undo / Redo for the phone (the PC has Ctrl+Z / Ctrl+Y). A finished drag or edit is one step.
    fn m_studio_undo_redo(&mut self, ui: &mut Ui, width: f32, captions: bool) -> bool {
        let (can_undo, can_redo) = (!self.undo.is_empty(), !self.redo.is_empty());
        let undo = round_icon(ui, width, Icon::Undo, can_undo, "Undo");
        let did_undo = undo.clicked() && can_undo;
        if did_undo {
            self.undo();
        }
        if captions { sidebar_caption(ui, "Undo"); }
        let redo = round_icon(ui, width, Icon::Redo, can_redo, "Redo");
        let did_redo = redo.clicked() && can_redo;
        if did_redo {
            self.redo();
        }
        if captions { sidebar_caption(ui, "Redo"); }
        did_undo || did_redo
    }

    fn m_studio_quick_controls(&mut self, ui: &mut Ui, ctx: &egui::Context, width: f32) {
        self.m_studio_tempo(ui, width, false, false);
        sidebar_caption(ui, "BPM");
        let (rect, metronome) = ui.allocate_exact_size(vec2(width, 44.0), Sense::click());
        if metronome.on_hover_text("Metronome").clicked() {
            self.project.metronome = !self.project.metronome;
        }
        let clock_color = if self.project.metronome { super::theme::TEAL() } else { ink() };
        icon(ui.painter(), Icon::Clock, rect.center(), clock_color);
        sidebar_caption(ui, "Click");
        self.m_automation_button(ui, ctx, width);
        sidebar_caption(ui, "Auto.");
        if studio_icon(ui, width, Icon::Mic, self.rec_mode == 1 && self.record,
            "Record microphone to audio track").clicked()
        {
            self.m_voice_record();
        }
        sidebar_caption(ui, "Mic");
        if studio_icon(ui, width, Icon::Keys, self.rec_mode == 0 && self.record,
            "Record notes in Piano Roll").clicked()
        {
            if self.record && self.rec_mode == 1 {
                self.m_voice_record();
            }
            self.rec_mode = 0;
            self.record = !self.record;
            if self.record {
                self.m_select_studio_section(ctx, SEC_PIANO);
                ctx.data_mut(|d| d.insert_temp(egui::Id::new(("m_piano_record", self.project.current_pattern)), 0_usize));
            } else {
                ctx.data_mut(|d| d.remove::<usize>(egui::Id::new(("m_piano_record", self.project.current_pattern))));
            }
        }
        sidebar_caption(ui, "MIDI");
    }

    fn m_automation_button(&mut self, ui: &mut Ui, ctx: &egui::Context, width: f32) {
        let active = self.m_sec == SEC_PLAYLIST && self.arr.auto_edit;
        if studio_icon(ui, width, Icon::Pencil, active, "Draw volume automation").clicked() {
            self.m_select_studio_section(ctx, SEC_PLAYLIST);
            self.arr.auto_edit = !active;
            self.arr.sel_clip = None;
            ctx.data_mut(|data| {
                data.remove::<usize>(egui::Id::new("m_clip_sheet_selection"));
                data.remove::<bool>(egui::Id::new("m_track_sheet_open"));
                data.remove::<bool>(egui::Id::new("m_browser_open"));
                data.insert_temp(egui::Id::new("m_playlist_reveal_track"), self.arr.sel_lane);
            });
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn m_render_track_button(&mut self, ui: &mut Ui, ctx: &egui::Context, width: f32) {
        if studio_icon(ui, width, Icon::Wave, false, "Render Playlist mix to audio").clicked() {
            self.m_render_playlist_mix(ctx);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn m_render_playlist_mix(&mut self, ctx: &egui::Context) {
        if self.project.arrangement.clips.is_empty() {
            self.status = "Add clips to the Playlist before rendering.".into();
            self.arr.msg.clone_from(&self.status);
            return;
        }
        self.m_stop(ctx);
        self.m_select_studio_section(ctx, SEC_PLAYLIST);
        ctx.data_mut(|d| {
            d.remove::<bool>(egui::Id::new("m_browser_open"));
            d.insert_temp(egui::Id::new("render_job"), None::<usize>);
        });
        ctx.request_repaint();
    }

    fn m_playlist_track_strip(&mut self, ui: &mut Ui) {
        use crate::arrangement::TrackKind;

        let ctx = ui.ctx().clone();
        let options_id = egui::Id::new("m_playlist_track_names").with("track_modal");
        let mut selected = None;
        let mut settings = None;
        let mut open_track = None;
        let mut effect = None;
        let mut instrument = None;
        let mut group_drop = None;
        let mut group_toggle = None;
        let mut add_track = None;
        let mut master = false;
        let strip_id = egui::Id::new("m_playlist_track_strip");
        let last_lane_id = strip_id.with("selected_lane");
        let reveal = ui.data(|d| d.get_temp::<usize>(last_lane_id)) != Some(self.arr.sel_lane);
        ui.data_mut(|d| d.insert_temp(last_lane_id, self.arr.sel_lane));
        let offset_id = strip_id.with("requested_offset");
        let mut scroll = mobile_scroll(egui::ScrollArea::horizontal().id_salt(strip_id))
            .auto_shrink([false, true]);
        if let Some(offset) = ui.data_mut(|d| d.remove_temp::<f32>(offset_id)) {
            scroll = scroll.horizontal_scroll_offset(offset);
        }
        let out = scroll.show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add_space(5.0);
                for (lane, track) in self.project.arrangement.tracks.iter().enumerate() {
                    let (card, hit) = ui.allocate_exact_size(vec2(122.0, 48.0), Sense::click());
                    if reveal && lane == self.arr.sel_lane {
                        ui.scroll_to_rect(card.expand(4.0), None);
                    }
                    let on = self.arr.sel_lane == lane && !self.arr.auto_master;
                    let p = ui.painter();
                    let color = lane_col(lane);
                    p.rect_filled(card, 11.0, if on { selected_fill() } else { card_fill() });
                    p.rect_stroke(card, 11.0, Stroke::new(1.0, if on { color } else { surface_edge() }));
                    let grip = Rect::from_min_size(card.min, vec2(19.0, card.height()));
                    p.rect_filled(Rect::from_min_size(card.min + vec2(5.0, 10.0), vec2(4.0, 28.0)), 2.0, color);
                    let name = Rect::from_min_max(card.min + vec2(16.0, 3.0), card.max - vec2(28.0, 20.0));
                    fit_label(p, name, name.center().y, &track.name, if track.mute { muted() } else { ink() }, 12.0);
                    p.text(card.left_bottom() + vec2(16.0, -11.0), Align2::LEFT_CENTER,
                        if track.playlist_locked { "Locked" } else if track.kind == TrackKind::Audio { "Audio" } else { "MIDI" },
                        FontId::proportional(10.0), muted());
                    let menu = Rect::from_min_size(pos2(card.right() - 28.0, card.top()), vec2(28.0, card.height()));
                    let menu_hit = ui.interact(menu, hit.id.with("options"), Sense::click()).on_hover_text("Track options");
                    for offset in [-5.0, 0.0, 5.0] {
                        ui.painter().circle_filled(menu.center() + vec2(offset, 0.0), 1.3, muted());
                    }
                    if menu_hit.clicked() || hit.secondary_clicked() || hit.long_touched() {
                        settings = Some(lane);
                    } else if hit.double_clicked() {
                        open_track = Some(lane);
                    } else if hit.clicked() {
                        selected = Some(lane);
                    }
                    if track.playlist_locked {
                        continue;
                    }
                    ui.interact(grip, hit.id.with("group"), Sense::drag())
                        .dnd_set_drag_payload(PlaylistTrackDrag(lane));
                    let sound = hit.dnd_hover_payload::<browser::DragItem>()
                        .filter(|item| track.kind == TrackKind::Midi
                            && matches!(item.as_ref(), browser::DragItem::Instrument(_)));
                    if hit.dnd_hover_payload::<crate::fx::FxKind>().is_some() || sound.is_some() {
                        ui.painter().rect_stroke(card.shrink(1.0), 10.0, Stroke::new(2.0, p_accent()));
                        if let Some(kind) = hit.dnd_release_payload::<crate::fx::FxKind>() {
                            effect = Some((*kind, lane));
                        } else if sound.is_some() {
                            instrument = hit.dnd_release_payload::<browser::DragItem>().map(|item| (item, lane));
                        }
                    }
                    if let Some(source) = hit.dnd_release_payload::<PlaylistTrackDrag>() {
                        if source.0 != lane {
                            group_drop = Some((source.0, Some(lane), None));
                        }
                    }
                }
                for group in &self.project.arrangement.groups {
                    let (card, hit) = ui.allocate_exact_size(vec2(112.0, 48.0), Sense::click());
                    panel(ui.painter(), card, 10.0);
                    let label = format!("{} {}", if group.collapsed { "›" } else { "⌄" }, group.name);
                    fit_label(ui.painter(), card.shrink(8.0), card.center().y, &label, ink(), 12.0);
                    if hit.clicked() {
                        group_toggle = Some(group.id);
                    }
                    if let Some(source) = hit.dnd_release_payload::<PlaylistTrackDrag>() {
                        group_drop = Some((source.0, None, Some(group.id)));
                    }
                }
                let (card, hit) = ui.allocate_exact_size(vec2(82.0, 48.0), Sense::click());
                let p = ui.painter();
                p.rect_filled(card, 11.0, if self.arr.auto_master { selected_fill() } else { card_fill() });
                p.rect_stroke(card, 11.0, Stroke::new(1.0, surface_edge()));
                p.text(card.center() - vec2(0.0, 7.0), Align2::CENTER_CENTER,
                    "Master", FontId::proportional(12.0), ink());
                let meter = Rect::from_center_size(card.center() + vec2(0.0, 11.0), vec2(61.0, 4.0));
                p.rect_filled(meter, 2.0, p_bg());
                let [left, right] = self.shared.peak(crate::mixer::NUM_STRIPS);
                let level = left.max(right).clamp(0.0, 1.0).sqrt();
                p.rect_filled(Rect::from_min_size(meter.min, vec2(meter.width() * level, 4.0)), 2.0, p_accent());
                master = hit.on_hover_text("Master mixer or automation").clicked();
                for (kind, label) in [(TrackKind::Midi, "+ MIDI"), (TrackKind::Audio, "+ Audio")] {
                    if p_button(ui, label, false, vec2(78.0, 48.0)).clicked() {
                        add_track = Some(kind);
                    }
                }
                ui.add_space(5.0);
            });
        });
        let width = out.inner_rect.width();
        let max_offset = (out.content_size.x - width).max(0.0);
        if max_offset > 0.0 {
            let (rect, scrub) = ui.allocate_exact_size(vec2(width, 12.0), Sense::click_and_drag());
            let track = Rect::from_center_size(rect.center(), vec2((width - 16.0).max(1.0), 3.0));
            let thumb_width = (track.width() * width / out.content_size.x).clamp(1.0, track.width()).max(14.0_f32.min(track.width()));
            let travel = (track.width() - thumb_width).max(1.0);
            let fraction = (out.state.offset.x / max_offset).clamp(0.0, 1.0);
            let thumb = Rect::from_min_size(track.min + vec2(travel * fraction, 0.0), vec2(thumb_width, 3.0));
            ui.painter().rect_filled(track, 1.5, outline().gamma_multiply(0.55));
            ui.painter().rect_filled(thumb, 1.5, p_accent().gamma_multiply(0.76));
            if let Some(point) = scrub.interact_pointer_pos().filter(|_| scrub.clicked() || scrub.dragged()) {
                let offset = ((point.x - track.left() - thumb_width * 0.5) / travel).clamp(0.0, 1.0) * max_offset;
                ui.data_mut(|d| d.insert_temp(offset_id, offset));
                ui.ctx().request_repaint();
            }
        }
        if let Some(lane) = selected {
            if self.arr.sel_lane != lane {
                self.arr.sel_clip = None;
                self.arr.sel.clear();
            }
            self.arr.sel_lane = lane;
            self.arr.auto_master = false;
            ctx.data_mut(|d| d.insert_temp(egui::Id::new("m_playlist_reveal_track"), lane));
        }
        if let Some(lane) = open_track {
            self.arr.sel_lane = lane;
            self.arr.sel_clip = None;
            self.arr.sel.clear();
            self.arr.auto_master = false;
            ctx.data_mut(|d| {
                d.remove::<usize>(options_id);
                d.remove::<usize>(egui::Id::new("m_clip_sheet_selection"));
                d.insert_temp(egui::Id::new(("m_track_settings_page", lane)), 0_usize);
                d.insert_temp(egui::Id::new("m_track_sheet_open"), true);
            });
        } else if let Some(lane) = settings {
            ctx.data_mut(|d| d.insert_temp(options_id, lane));
        }
        if let Some((kind, lane)) = effect {
            self.m_drop_effect(kind, lane);
        }
        if let Some((item, lane)) = instrument {
            self.m_drop_browser_item(&item, lane, 0);
        }
        if let Some((source, lane, group)) = group_drop {
            self.m_playlist_undo_step();
            self.project.arrangement.group_playlist_track(source, lane, group);
        }
        if let Some(group) = group_toggle.and_then(|id| self.project.arrangement.groups.iter_mut().find(|g| g.id == id)) {
            group.collapsed = !group.collapsed;
            self.m_playlist_undo_step();
        }
        if let Some(kind) = add_track {
            self.m_playlist_undo_step();
            self.project.arrangement.add_track(kind);
            self.arr.sel_lane = self.project.arrangement.tracks.len() - 1;
            self.arr.sel_clip = None;
            self.arr.sel.clear();
            ctx.data_mut(|d| d.insert_temp(egui::Id::new("m_playlist_reveal_track"), self.arr.sel_lane));
        }
        if master {
            if self.arr.auto_edit {
                self.arr.auto_master = !self.arr.auto_master;
                self.arr.sel_clip = None;
                self.arr.sel.clear();
            } else {
                ctx.data_mut(|d| d.insert_temp(egui::Id::new(MIXER_CHANNEL), MobileMixerChannel::Master));
                self.m_select_studio_section(&ctx, 4);
            }
        }
        if self.arr.auto_edit && self.arr.auto_master {
            let bars = self.project.arrangement.song_end_bar().max(1);
            let (plot, _) = ui.allocate_exact_size(vec2(ui.available_width(), 66.0), Sense::hover());
            self.m_playlist_automation(ui, plot, plot.width() / bars as f32, bars, false, true);
        }
    }

    fn m_studio_transport_dock(&mut self, ctx: &egui::Context) {
        let playlist = self.m_sec == SEC_PLAYLIST;
        let dock = egui::TopBottomPanel::bottom("m_studio_transport_dock").show_separator_line(false)
            .frame(Frame::none().fill(surface_fill()).stroke(Stroke::new(1.0, surface_edge()))
                .inner_margin(Margin { left: 8.0, right: 8.0, top: 4.0, bottom: 10.0 }))
            .show(ctx, |ui| {
                mobile_visuals(ui);
                let vertical_playlist = playlist && portrait_view(ui) && !self.arr.auto_edit
                    && ui.data(|d| d.get_temp::<bool>(egui::Id::new("m_playlist_vertical")).unwrap_or(true));
                if playlist && !vertical_playlist {
                    self.m_playlist_track_strip(ui);
                    ui.separator();
                }
                let tick = self.shared.position.load(Ordering::Relaxed);
                let bar = tick / TICKS_PER_BAR;
                let bars = self.m_studio_transport_bars();
                let end = bars * TICKS_PER_BAR;
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    let playing = self.m_transport_playing(ctx);
                    if dock_icon(ui, if playing { Icon::Pause } else { Icon::Play }, true,
                        if playing { "Pause" } else { "Play" }).clicked()
                    {
                        self.m_toggle_play(ctx);
                    }
                    if portrait_view(ui) {
                        self.m_dock_transport_buttons(ui, ctx);
                    }
                    let space = ui.available_width().max(12.0);
                    let (rect, scrub) = ui.allocate_exact_size(vec2(space, 44.0), Sense::click_and_drag());
                    let track = Rect::from_center_size(rect.center() + vec2(0.0, 7.0), vec2((rect.width() - 14.0).max(1.0), 5.0));
                    let head = track.left() + track.width() * (tick as f32 / end as f32).clamp(0.0, 1.0);
                    let p = ui.painter();
                    let caption_y = rect.top() + 9.0;
                    if track.width() > 150.0 {
                        p.text(pos2(track.left(), caption_y), Align2::LEFT_CENTER,
                            "Position", FontId::proportional(11.0), muted());
                    }
                    p.text(pos2(track.right(), caption_y), Align2::RIGHT_CENTER,
                        format!("Bar {} / {bars}", bar.saturating_add(1).min(bars)),
                        FontId::proportional(11.0), ink());
                    p.rect_filled(track, 2.5, outline());
                    let head_pos = pos2(head, track.center().y);
                    p.line_segment([track.left_center(), head_pos], Stroke::new(5.0, p_accent()));
                    p.circle_filled(head_pos, 6.0, p_accent());
                    if let Some(point) = scrub.interact_pointer_pos().filter(|_| scrub.clicked() || scrub.dragged()) {
                        let fraction = ((point.x - track.left()) / track.width()).clamp(0.0, 1.0);
                        self.m_seek(ctx, ((fraction * end as f32) as u32).min(end - 1));
                    }
                    scrub.on_hover_text(format!("Bar {} of {bars}", bar.saturating_add(1).min(bars)));
                });
            });
        ctx.data_mut(|d| d.insert_temp(egui::Id::new("m_studio_transport_rect"), dock.response.rect));
        if playlist {
            let pass = ctx.cumulative_pass_nr();
            ctx.data_mut(|d| d.insert_temp(egui::Id::new(PLAYLIST_DOCK_PASS), pass));
        }
    }

    /// Playlist: the mixer as a right side bar, full height. A swipe from the right edge pulls it in; a swipe to the
    /// right on it or a tap on the dimmed rest closes it.
    fn m_mixer_sheet(&mut self, ctx: &egui::Context) {
        let open_id = egui::Id::new(MIXER_SHEET);
        let screen = ctx.screen_rect();
        let mut open = ctx.data(|d| d.get_temp::<bool>(open_id)).unwrap_or(false) && self.m_sec == SEC_PLAYLIST;
        if !open && self.m_sec == SEC_PLAYLIST && !egui::DragAndDrop::has_any_payload(ctx) {
            let dock = ctx.data(|d| d.get_temp::<Rect>(egui::Id::new("m_studio_transport_rect")));
            let edge = ctx.input(|i| i.pointer.press_origin().zip(i.pointer.interact_pos())
                .is_some_and(|(a, b)| a.x > screen.right() - 22.0 && a.x - b.x > 36.0
                    && (a.y - b.y).abs() < a.x - b.x
                    && !dock.is_some_and(|rect| rect.contains(a))));
            if edge {
                open = true;
                ctx.data_mut(|d| d.insert_temp(open_id, true));
            }
        }
        let t = ctx.animate_bool_with_time(open_id.with("anim"), open, 0.18);
        if t <= 0.0 {
            return;
        }
        let width = (screen.width() * 0.86).min(360.0);
        let mut close = false;
        egui::Area::new(open_id.with("scrim")).order(egui::Order::Middle).fixed_pos(screen.min)
            .show(ctx, |ui| {
                let (rect, resp) = ui.allocate_exact_size(screen.size(), Sense::click());
                ui.painter().rect_filled(rect, 0.0, Color32::from_black_alpha((120.0 * t) as u8));
                close |= resp.clicked();
            });
        egui::Area::new(open_id.with("panel")).order(egui::Order::Foreground)
            .fixed_pos(pos2(screen.right() - width * t, screen.top()))
            .show(ctx, |ui| {
                let rect = Rect::from_min_size(ui.max_rect().min, vec2(width, screen.height()));
                let back = ui.allocate_rect(rect, Sense::drag());
                ui.painter().rect_filled(rect, egui::Rounding { nw: 16.0, sw: 16.0, ..Default::default() }, bar_fill());
                if back.drag_stopped() && back.drag_delta().x > 0.0 || back.dragged() && ui.input(|i| {
                    i.pointer.press_origin().zip(i.pointer.interact_pos()).is_some_and(|(a, b)| b.x - a.x > 50.0)
                }) {
                    close = true;
                }
                let head = Rect::from_min_size(rect.min + vec2(14.0, 10.0), vec2(rect.width() - 28.0, 36.0));
                ui.painter().text(head.left_center(), Align2::LEFT_CENTER, "Mixer", FontId::proportional(17.0), ink());
                let close_rect = Rect::from_center_size(pos2(head.right() - 16.0, head.center().y), vec2(36.0, 36.0));
                let mut head_ui = ui.new_child(egui::UiBuilder::new().max_rect(close_rect));
                mobile_visuals(&mut head_ui);
                if p_button(&mut head_ui, "×", false, vec2(36.0, 36.0)).clicked() {
                    close = true;
                }
                let body = Rect::from_min_max(pos2(rect.left() + 8.0, head.bottom() + 8.0), pos2(rect.right() - 8.0, rect.bottom() - 12.0));
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(body));
                child.set_clip_rect(body.intersect(screen));
                mobile_visuals(&mut child);
                self.m_mixer(&mut child);
            });
        if close {
            ctx.data_mut(|d| d.insert_temp(open_id, false));
        }
        ctx.request_repaint();
    }

    fn m_studio_transport_bars(&self) -> u32 {
        if self.m_sec == SEC_PLAYLIST {
            self.project.arrangement.song_end_bar().max(1)
        } else {
            self.project.patterns.get(self.project.current_pattern)
                .map_or(1, |p| u32::from(p.length_steps).div_ceil(16).max(1))
        }
    }

    /// Stop / previous bar / next bar beside Play in the bottom dock, so the whole transport sits in one place.
    fn m_dock_transport_buttons(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        if dock_icon(ui, Icon::Stop, false, "Stop").clicked() {
            self.m_stop(ctx);
        }
        let tick = self.shared.position.load(Ordering::Relaxed);
        let bar = tick / TICKS_PER_BAR;
        if dock_icon(ui, Icon::Rewind, false, "Previous bar").clicked() {
            let previous = if tick % TICKS_PER_BAR == 0 { bar.saturating_sub(1) } else { bar };
            self.m_seek(ctx, previous * TICKS_PER_BAR);
        }
        if dock_icon(ui, Icon::Forward, false, "Next bar").clicked() {
            self.m_seek(ctx, (bar + 1).min(self.m_studio_transport_bars() - 1) * TICKS_PER_BAR);
        }
    }

    fn m_studio_sidebar_transport(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let (name, symbol) = STUDIO_SECTIONS.get(self.m_sec).copied().unwrap_or(STUDIO_SECTIONS[0]);
        let views = dock_icon(ui, symbol, true, name);
        let sheet = egui::Id::new("m_studio_menu_overlay");
        if views.clicked() {
            ctx.data_mut(|d| d.insert_temp(sheet, false));
        }
        sidebar_caption(ui, "Views");
        if self.m_sec == SEC_PLAYLIST {
            if dock_icon(ui, Icon::Settings, false, "Playlist options").clicked() {
                ctx.data_mut(|d| d.insert_temp(sheet, true));
            }
            sidebar_caption(ui, "Options");
        }
    }

    fn m_studio_menu_overlay(&mut self, ctx: &egui::Context) {
        let id = egui::Id::new("m_studio_menu_overlay");
        let Some(options) = ctx.data(|d| d.get_temp::<bool>(id)) else { return };
        let close = if options {
            let mut action = false;
            let closed = mobile_side_editor(ctx, id.with("options"), "Playlist options", egui::Align::Min, 0.86, |ui| {
                ui.spacing_mut().interact_size.y = 37.0;
                ui.style_mut().override_font_id = Some(FontId::proportional(14.0));
                action = self.m_playlist_options_content(ui, true);
            });
            if action {
                ctx.animate_bool(id.with("options").with("animation"), false);
            }
            closed || action
        } else {
            let drawer = id.with("views");
            let mut selected = None;
            let close = mobile_side_editor(ctx, drawer, "Views", egui::Align::Min, 0.78, |ui| {
                mobile_scroll(egui::ScrollArea::vertical()).show(ui, |ui| {
                    for (index, (label, symbol)) in STUDIO_SECTIONS.into_iter().enumerate() {
                        if index == SEC_PIANO || matches!(symbol, Icon::Tracks) {
                            continue;
                        }
                        let label = if label == "Steps" { "Drum Rack" } else { label };
                        if drawer_row(ui, symbol, label, index == self.m_sec).clicked() {
                            selected = Some(index);
                        }
                    }
                });
            });
            if let Some(index) = selected {
                self.m_select_studio_section(ctx, index);
                ctx.animate_bool(drawer.with("animation"), false);
            }
            close || selected.is_some()
        };
        if close {
            ctx.data_mut(|d| d.remove::<bool>(id));
            ctx.request_repaint();
        }
    }

    fn m_studio_portrait_sidebar(&mut self, ctx: &egui::Context) {
        let drawer = egui::Id::new("m_studio_project_drawer");
        egui::SidePanel::right("m_studio_portrait_sidebar")
            .resizable(false).exact_width(TOUCH + 6.0).show_separator_line(false)
            .frame(Frame::none().fill(surface_fill()).stroke(Stroke::new(1.0, surface_edge()))
                .inner_margin(Margin::symmetric(1.0, 4.0)))
            .show(ctx, |ui| {
                mobile_visuals(ui);
                mobile_scroll(egui::ScrollArea::vertical()).id_salt("m_studio_sidebar_tools")
                    .auto_shrink([false, false]).show(ui, |ui| {
                        ui.vertical_centered(|ui| {
                            ui.spacing_mut().item_spacing.y = 1.0;
                            let open = ctx.data(|d| d.get_temp::<bool>(drawer)).unwrap_or(false);
                            if dock_icon(ui, Icon::Menu, open, "Project actions").clicked() {
                                ctx.data_mut(|d| {
                                    d.insert_temp(drawer, !open);
                                    d.remove::<bool>(egui::Id::new("m_browser_open"));
                                });
                            }
                            sidebar_caption(ui, "Project");
                            self.m_studio_sidebar_transport(ui, ctx);
                            ui.separator();
                            if self.m_sec == SEC_PLAYLIST {
                                self.m_studio_browser_button(ui, ctx, TOUCH);
                                sidebar_caption(ui, "Browser");
                                if studio_icon(ui, TOUCH, Icon::Repeat, self.project.loop_on,
                                    "Auto Play · Repeat Playlist").clicked()
                                {
                                    self.project.loop_on = !self.project.loop_on;
                                    if self.project.loop_on {
                                        self.project.loop_start = 0;
                                        self.project.loop_bars = self.project.arrangement.song_end_bar().max(1);
                                    }
                                    if self.project.loop_on && !self.m_transport_playing(ctx) {
                                        self.m_start_playback(ctx, true, self.project.current_pattern, true);
                                    } else {
                                        self.publish_edits();
                                    }
                                }
                                sidebar_caption_color(ui, "Auto Play", if self.project.loop_on { p_accent() } else { muted() });
                                ui.separator();
                                self.m_playlist_tool_buttons(ui, true);
                            } else {
                                if dock_icon(ui, Icon::Folder, false, "Browser").clicked() {
                                    self.m_select_studio_section(ctx, SEC_PLAYLIST);
                                    ctx.data_mut(|d| d.insert_temp(egui::Id::new("m_browser_open"), true));
                                }
                                sidebar_caption(ui, "Browser");
                            }
                            ui.separator();
                            self.m_studio_quick_controls(ui, ctx, TOUCH);
                        });
                    });
            });
    }

    fn m_studio_project_drawer(&mut self, ctx: &egui::Context) {
        let id = egui::Id::new("m_studio_project_drawer");
        let open = ctx.data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
        let shown = ctx.animate_bool_with_time(id.with("slide"), open, 0.16);
        if shown <= 0.0 {
            return;
        }
        let screen = ctx.screen_rect();
        let width = (screen.width() * 0.80).min(320.0);
        let mut close = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        egui::Area::new(id.with("scrim")).order(egui::Order::Middle).fixed_pos(screen.min).movable(false)
            .show(ctx, |ui| {
                let (rect, hit) = ui.allocate_exact_size(screen.size(), Sense::click());
                ui.painter().rect_filled(rect, 0.0, Color32::from_black_alpha((145.0 * shown) as u8));
                close |= hit.clicked();
            });
        egui::Area::new(id.with("area")).order(egui::Order::Foreground).movable(false).constrain(false)
            .fixed_pos(pos2(screen.left() - (1.0 - shown) * width, screen.top()))
            .show(ctx, |ui| {
                let corners = egui::Rounding { ne: 18.0, se: 18.0, ..Default::default() };
                Frame::none().fill(surface_fill()).stroke(Stroke::new(1.0, surface_edge()))
                    .rounding(corners).inner_margin(Margin::symmetric(10.0, 10.0)).show(ui, |ui| {
                        mobile_visuals(ui);
                        let content_width = (width - 22.0).max(1.0);
                        ui.set_width(content_width);
                        ui.set_min_height((screen.height() - 22.0).max(1.0));
                        ui.horizontal(|ui| {
                            ui.add_sized([(content_width - 52.0).max(1.0), TOUCH],
                                egui::Label::new(RichText::new(&self.project.name).size(19.0).strong()).truncate());
                            close |= p_button(ui, "×", false, vec2(TOUCH, TOUCH)).clicked();
                        });
                        ui.separator();
                        close |= self.m_studio_project_actions(ui, ctx);
                    });
            });
        if close {
            ctx.data_mut(|d| d.insert_temp(id, false));
            ctx.request_repaint();
        }
    }

    fn m_studio_project_actions(&mut self, ui: &mut Ui, ctx: &egui::Context) -> bool {
        let mut close = false;
        mobile_scroll(egui::ScrollArea::vertical()).max_height(ui.available_height().max(1.0))
            .auto_shrink([false, true]).show(ui, |ui| {
                let rec = if self.record { "Disarm recording" } else { "Arm recording" };
                let items = [
                    (Icon::Rewind, "Return to start", 0),
                    (Icon::Record, rec, 1),
                    (Icon::Folder, "Save project", 2),
                    (Icon::Share, "Export or share", 3),
                    #[cfg(not(target_arch = "wasm32"))]
                    (Icon::Wave, "Render mix to audio", 8),
                    (Icon::FullView, "Full view", 4),
                    (Icon::Orientation, "Switch to landscape", 5),
                    (Icon::Settings, "Settings", 7),
                    (Icon::Exit, "Leave project", 6),
                ];
                for (symbol, label, id) in items {
                    if id == 5 && !cfg!(target_os = "android") {
                        continue;
                    }
                    if drawer_row(ui, symbol, label, id == 1 && self.record).clicked() {
                        match id {
                            0 => self.return_to_start(ctx, self.playing()),
                            1 => self.m_toggle_studio_record(ctx),
                            2 => { self.m_save_project_to_list(ctx); }
                            3 => {
                                self.m_export.open = true;
                                self.m_export.song = self.m_sec == SEC_PLAYLIST;
                            }
                            #[cfg(not(target_arch = "wasm32"))]
                            8 => self.m_render_playlist_mix(ctx),
                            4 => ctx.data_mut(|d| d.insert_temp(egui::Id::new("m_studio_full_view"), true)),
                            5 => ctx.data_mut(|d| d.insert_temp(egui::Id::new("m_studio_landscape"), true)),
                            7 => {
                                if self.m_save_pending_project(ctx) {
                                    self.m_tab = 2;
                                    self.m_sec = 0;
                                }
                            }
                            _ => self.m_leave_studio(ctx),
                        }
                        close = true;
                        break;
                    }
                }
            });
        close
    }

    #[allow(dead_code)]
    fn m_playlist_track_nav(&mut self, ctx: &egui::Context) {
        let mut select = None;
        egui::TopBottomPanel::top("m_playlist_track_nav")
            .show_separator_line(false)
            .frame(Frame::none().fill(bar_fill()).inner_margin(Margin { left: 7.0, right: 7.0, top: 0.0, bottom: 5.0 }))
            .show(ctx, |ui| {
                mobile_visuals(ui);
                mobile_scroll(egui::ScrollArea::horizontal()).id_salt("m_playlist_track_tabs")
                    .auto_shrink([false, true]).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            for (lane, track) in self.project.arrangement.tracks.iter().enumerate() {
                                let selected = self.arr.sel_lane == lane;
                                let label = RichText::new(&track.name).size(13.0)
                                    .color(if selected { on_accent() } else { lane_col(lane) });
                                let mut button = egui::Button::new(label)
                                    .min_size(vec2(60.0, 36.0)).wrap_mode(egui::TextWrapMode::Extend);
                                if selected {
                                    button = button.fill(p_accent());
                                }
                                let response = ui.add(button).on_hover_text(if selected { "Track settings" } else { "Select track" });
                                if response.clicked() || response.secondary_clicked() {
                                    select = Some((lane, selected || response.secondary_clicked()));
                                }
                            }
                        });
                    });
            });
        if let Some((lane, open_settings)) = select {
            self.arr.sel_lane = lane;
            self.arr.sel_clip = None;
            self.arr.sel.clear();
            ctx.data_mut(|d| {
                d.insert_temp(egui::Id::new("m_playlist_reveal_track"), lane);
                d.remove::<usize>(egui::Id::new("m_clip_sheet_selection"));
                d.remove::<(usize, u32)>(egui::Id::new("m_audio_clip_target"));
                if open_settings {
                    d.insert_temp(egui::Id::new("m_track_sheet_open"), true);
                }
            });
        }
    }

    fn m_studio_exit(&mut self, ui: &mut Ui, ctx: &egui::Context, width: f32) -> bool {
        if !studio_icon(ui, width, Icon::Exit, false, "Leave project").clicked() {
            return false;
        }
        self.m_leave_studio(ctx);
        true
    }

    fn m_leave_studio(&mut self, ctx: &egui::Context) {
        self.return_to_start(ctx, false);
        self.m_save_pending_project(ctx);
        self.m_tab = 4;
        self.m_sec = 0;
        self.hub_cache = None;
        ctx.data_mut(|d| {
            d.remove::<usize>(egui::Id::new("m_clip_sheet_selection"));
            d.remove::<bool>(egui::Id::new("m_track_sheet_open"));
            d.remove::<bool>(egui::Id::new("m_studio_full_view"));
        });
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

    fn m_studio_controls(&mut self, ui: &mut Ui, ctx: &egui::Context, width: f32) {
        if studio_icon(ui, width, Icon::Rewind, false, "Return to start").clicked() {
            self.return_to_start(ctx, self.playing());
        }
        self.m_studio_play_stop(ui, ctx, width);
        if studio_icon(ui, width, Icon::Record, self.record, "Arm recording").clicked() {
            self.m_toggle_studio_record(ctx);
        }
        self.m_studio_tempo(ui, width, false, true);
        self.m_automation_button(ui, ctx, width);
        #[cfg(not(target_arch = "wasm32"))]
        self.m_render_track_button(ui, ctx, width);
        if studio_icon(ui, width, Icon::Share, false, "Export or share").clicked() {
            self.m_export.open = !self.m_export.open;
            self.m_export.song = self.m_sec == SEC_PLAYLIST;
        }
        ui.separator();
        for (index, (title, icon)) in STUDIO_SECTIONS.into_iter().enumerate().filter(|(i, _)| *i != SEC_PIANO) {
            if studio_icon(ui, width, icon, self.m_sec == index, title).clicked() {
                self.m_select_studio_section(ctx, index);
            }
        }
    }

    fn m_seek(&mut self, ctx: &egui::Context, tick: u32) {
        let song = self.m_sec == SEC_PLAYLIST;
        self.project.song_mode = song;
        self.publish_edits();
        let pattern = if song { usize::MAX } else { self.project.current_pattern };
        if !self.m_transport_playing(ctx) {
            ctx.data_mut(|d| d.insert_temp(egui::Id::new(Self::M_PAUSED_AT), (song, pattern)));
        }
        self.shared.send(Cmd::Seek(tick));
    }

    fn m_voice_record(&mut self) {
        if self.record && self.rec_mode == 1 {
            self.record = false;
            #[cfg(target_arch = "wasm32")]
            crate::web::record_voice(false);
            return;
        }
        self.rec_mode = 1;
        self.record = true;
        #[cfg(target_os = "android")]
        if let Err(error) = self.android_updater.as_ref().ok_or_else(|| "Android activity unavailable".to_owned())
            .and_then(|android| android.request_microphone_permission())
        {
            self.record = false;
            self.status = error;
        }
        #[cfg(target_arch = "wasm32")]
        {
            self.rec_from = self.shared.position.load(Ordering::Relaxed);
            self.voice_lane = self.arr.sel_lane;
            crate::web::record_voice(true);
        }
    }

    fn m_toggle_studio_record(&mut self, ctx: &egui::Context) {
        self.record = !self.record;
        if self.record && !self.playing() {
            self.m_toggle_play(ctx);
        }
    }

    fn m_select_studio_section(&mut self, ctx: &egui::Context, index: usize) {
        if self.m_sec != index {
            self.m_sec = index;
            ctx.data_mut(|d| {
                d.remove::<usize>(egui::Id::new("m_clip_sheet_selection"));
                d.remove::<bool>(egui::Id::new("m_track_sheet_open"));
            });
        }
    }

    fn m_studio_play_stop(&mut self, ui: &mut Ui, ctx: &egui::Context, width: f32) {
        let play = if self.m_transport_playing(ctx) {
            studio_icon(ui, width, Icon::Pause, true, "Pause")
        } else {
            studio_icon(ui, width, Icon::Play, true, "Play")
        };
        if play.clicked() {
            self.m_toggle_play(ctx);
        }
        if studio_icon(ui, width, Icon::Stop, false, "Stop").clicked() {
            self.m_stop(ctx);
        }
    }

    fn m_studio_tempo(&mut self, ui: &mut Ui, width: f32, show_unit: bool, framed: bool) {
        let unit = if show_unit { " BPM" } else { "" };
        let tempo = egui::Button::new(RichText::new(format!("{:.0}{unit}", self.project.bpm)).size(12.0))
            .frame(framed).wrap_mode(egui::TextWrapMode::Extend).min_size(vec2(width, 44.0));
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

    fn m_needs_project_save(&self) -> bool {
        self.dirty() || (self.path.is_none()
            && (!self.project.arrangement.clips.is_empty()
                || self.project.patterns.iter().any(|pattern| !pattern.notes.is_empty())))
    }

    fn m_save_project_to_list(&mut self, ctx: &egui::Context) -> bool {
        let saved = self.save_to_hub();
        let id = egui::Id::new("m_project_save_error");
        ctx.data_mut(|d| {
            d.remove::<String>(id);
            if !saved {
                d.insert_temp(id, self.status.clone());
            }
        });
        saved
    }

    fn m_save_pending_project(&mut self, ctx: &egui::Context) -> bool {
        !self.m_needs_project_save() || self.m_save_project_to_list(ctx)
    }

    fn m_open_studio(&mut self, section: usize) {
        self.m_tab = 1;
        self.m_sec = section;
    }

    fn m_toggle_play(&mut self, ctx: &egui::Context) {
        if self.m_transport_playing(ctx) {
            self.m_pause(ctx);
        } else {
            // Playlist plays the arrangement. Editors play the selected pattern.
            let song = self.m_sec == SEC_PLAYLIST;
            let looping = self.m_sec != SEC_PIANO
                || ctx.data(|d| d.get_temp::<bool>(egui::Id::new("m_piano_loop"))).unwrap_or(true);
            self.m_start_playback(ctx, song, self.project.current_pattern, looping);
        }
    }

    // One transport for every phone button. Pause remembers what was playing (song, or which pattern): Play resumes
    // only that; anything else starts from the beginning instead of continuing at the old position.
    // The button shows the state asked for at once, because the audio thread confirms it a few milliseconds later.
    const M_PAUSED_AT: &'static str = "m_transport_paused_at";
    const M_EXPECT: &'static str = "m_transport_expect";

    fn m_transport_playing(&self, ctx: &egui::Context) -> bool {
        let now = ctx.input(|i| i.time);
        match ctx.data(|d| d.get_temp::<(bool, f64)>(egui::Id::new(Self::M_EXPECT))) {
            Some((expect, at)) if now - at < 0.3 => expect,
            _ => self.playing(),
        }
    }

    fn m_expect(&self, ctx: &egui::Context, playing: bool) {
        let now = ctx.input(|i| i.time);
        ctx.data_mut(|d| d.insert_temp(egui::Id::new(Self::M_EXPECT), (playing, now)));
        ctx.request_repaint_after(std::time::Duration::from_millis(320));
    }

    fn m_start_playback(&mut self, ctx: &egui::Context, song: bool, pattern: usize, looping: bool) {
        let range = (!song).then(|| ctx.data(|d| d.get_temp::<(u32, u32)>(egui::Id::new(("m_piano_loop_range", (false, pattern)))))).flatten();
        self.m_start_playback_in_range(ctx, song, pattern, looping, range);
    }

    fn m_start_playback_in_range(&mut self, ctx: &egui::Context, song: bool, pattern: usize, looping: bool, range: Option<(u32, u32)>) {
        let key = (song, if song { usize::MAX } else { pattern });
        let paused = ctx.data_mut(|d| d.remove_temp::<(bool, usize)>(egui::Id::new(Self::M_PAUSED_AT)));
        if self.playing() || paused != Some(key) {
            // a different thing (or nothing) was paused: start fresh from the beginning
            self.shared.send(Cmd::Stop);
        }
        self.project.song_mode = song;
        if !song && pattern < self.project.patterns.len() {
            self.project.current_pattern = pattern;
        }
        self.publish_edits();
        self.shared.send(Cmd::PatternLoop(looping));
        self.shared.send(Cmd::PatternRange(range.map(|(start, end)| (pattern, start, end))));
        self.shared.send(Cmd::Play);
        self.m_expect(ctx, true);
    }

    fn m_pause(&mut self, ctx: &egui::Context) {
        self.shared.send(Cmd::Pause);
        let key = (self.project.song_mode, if self.project.song_mode { usize::MAX } else { self.project.current_pattern });
        ctx.data_mut(|d| d.insert_temp(egui::Id::new(Self::M_PAUSED_AT), key));
        self.m_expect(ctx, false);
    }

    fn m_stop(&mut self, ctx: &egui::Context) {
        ctx.data_mut(|d| d.remove::<(bool, usize)>(egui::Id::new(Self::M_PAUSED_AT)));
        if self.record && self.rec_mode == 1 {
            self.m_voice_record();
        }
        self.return_to_start(ctx, false);
        self.m_expect(ctx, false);
    }

    // ───────────────────── Projects ─────────────────────

    fn m_unsaved_project_actions(&mut self, ui: &mut Ui) {
        ui.add(egui::Label::new(RichText::new(&self.project.name).strong()).truncate());
        let w = ui.available_width();
        ui.horizontal(|ui| {
            if p_button(ui, "Continue", true, vec2((w - 84.0).max(80.0), TOUCH)).clicked() {
                self.m_open_studio(0);
            }
            if p_button(ui, "Save", false, vec2(72.0, TOUCH)).clicked() {
                self.m_save_project_to_list(ui.ctx());
            }
        });
        ui.add_space(8.0);
    }

    #[cfg(target_os = "android")]
    fn m_finish_project_import(&mut self, name: &str, staged: &std::path::Path) -> Result<String, String> {
        use std::io;

        let result = (|| {
            let (_, warnings) = Project::load(staged)?;
            let dir = staged.parent().ok_or("Missing project folder")?;
            let name = std::path::Path::new(name);
            let stem = name.file_stem().and_then(|stem| stem.to_str()).ok_or("Invalid project name")?;
            let ext = name.extension().and_then(|ext| ext.to_str()).ok_or("Missing project extension")?;
            let mut input = std::fs::File::open(staged).map_err(|error| error.to_string())?;
            for index in 1.. {
                let filename = if index == 1 { format!("{stem}.{ext}") } else { format!("{stem} {index}.{ext}") };
                let dest = dir.join(&filename);
                let output = std::fs::OpenOptions::new().write(true).create_new(true).open(&dest);
                let mut output = match output {
                    Ok(file) => file,
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                    Err(error) => return Err(error.to_string()),
                };
                if let Err(error) = io::copy(&mut input, &mut output).and_then(|_| output.sync_all()) {
                    drop(output);
                    let _ = std::fs::remove_file(&dest);
                    return Err(error.to_string());
                }
                return Ok(if warnings.is_empty() {
                    format!("Imported {filename}")
                } else {
                    format!("Imported {filename}. {} audio reference(s) could not be loaded.", warnings.len())
                });
            }
            unreachable!()
        })();
        let _ = std::fs::remove_file(staged);
        result.map_err(|error| format!("Import failed: {error}"))
    }

    fn m_projects(&mut self, ui: &mut Ui) {
        let mut open = None;
        let mut template = None;
        let backups = self.m_sec == 4;
        mobile_scroll(egui::ScrollArea::vertical()).auto_shrink([false, false]).show(ui, |ui| {
            let error_id = egui::Id::new("m_project_save_error");
            let project_error = ui.data(|d| d.get_temp::<String>(error_id));
            if let Some(error) = &project_error {
                ui.colored_label(RED, error);
            }
            match self.m_sec {
                3 => self.m_sounds(ui),
                2 => {
                    for (i, (name, _)) in crate::templates::TEMPLATES.iter().enumerate() {
                        if !name.starts_with("Empty") && ui.add_sized([ui.available_width(), TOUCH], egui::Button::new(*name)).clicked() {
                            template = Some(i);
                        }
                    }
                }
                section => {
                    #[cfg(target_os = "android")]
                    if section == 0 {
                        if p_button(ui, "Import project", false, vec2(ui.available_width(), TOUCH)).clicked() {
                            let result = self.android_updater.as_ref().ok_or_else(|| "Android file picker is unavailable.".to_owned())
                                .and_then(|android| {
                                    let dir = hub_dir().canonicalize().map_err(|error| error.to_string())?;
                                    android.choose_project(&dir.to_string_lossy())
                                });
                            ui.data_mut(|d| d.insert_temp(egui::Id::new("m_project_import_status"), result.map(|_| "Choose a project file…".to_owned())));
                        }
                        if let Some(status) = ui.data(|d| d.get_temp::<Result<String, String>>(egui::Id::new("m_project_import_status"))) {
                            match status {
                                Ok(text) => ui.label(RichText::new(text).color(muted())),
                                Err(text) => ui.colored_label(RED, text),
                            };
                        }
                        ui.add_space(8.0);
                    }
                    if section == 0 && (self.m_needs_project_save() || project_error.is_some()) {
                        self.m_unsaved_project_actions(ui);
                    }
                    if self.hub_cache.is_none() {
                        self.hub_cache = Some(hub_projects(&self.recent));
                    }
                    let items: Vec<PathBuf> = if backups {
                        let mut files: Vec<_> = std::fs::read_dir(backup_dir()).into_iter().flatten().flatten()
                            .filter_map(|entry| {
                                let path = entry.path();
                                let modified = entry.metadata().ok()?.modified().ok()?;
                                (path.extension().and_then(|ext| ext.to_str()) == Some("wd")).then_some((path, modified))
                            }).collect();
                        files.sort_by_key(|(_, modified)| std::cmp::Reverse(*modified));
                        files.into_iter().map(|(path, _)| path).collect()
                    } else if section == 1 {
                        self.recent.iter().filter(|path| {
                            #[cfg(target_arch = "wasm32")]
                            { self.hub_cache.as_deref().unwrap_or_default().iter().any(|(saved, _, _)| saved == *path) }
                            #[cfg(not(target_arch = "wasm32"))]
                            { path.is_file() }
                        }).cloned().collect()
                    } else {
                        self.hub_cache.as_deref().unwrap_or_default().iter().map(|(path, _, _)| path.clone()).collect()
                    };
                    if items.is_empty() {
                        if backups {
                            ui.label(RichText::new("No backups yet.").color(muted()));
                        } else {
                            self.m_empty_projects(ui);
                        }
                    }
                    if section == 0 {
                        let width = ((ui.available_width() - ui.spacing().item_spacing.x) * 0.5).max(1.0);
                        for (row, paths) in items.chunks(2).enumerate() {
                            ui.horizontal(|ui| {
                                for (column, path) in paths.iter().enumerate() {
                                    let name = path.file_stem().and_then(|name| name.to_str()).unwrap_or("Project");
                                    if Self::m_navigation_tile(ui, vec2(width, 72.0), Icon::Folder,
                                        name, lane_col(row * 2 + column)).clicked()
                                    {
                                        open = Some(path.clone());
                                    }
                                }
                            });
                        }
                    } else {
                        for path in items {
                            let name = path.file_stem().and_then(|name| name.to_str()).unwrap_or("Project");
                            let name = if backups && name == "_autosave" { "Autosave" } else { name };
                            let button = egui::Button::new(name).truncate();
                            if ui.add_sized([ui.available_width(), TOUCH], button).clicked() {
                                open = Some(path);
                            }
                        }
                    }
                }
            }
        });
        if (template.is_some() || open.is_some()) && !self.m_save_pending_project(ui.ctx()) {
            return;
        }
        if let Some(i) = template {
            self.load_template(i);
            if self.m_save_project_to_list(ui.ctx()) {
                self.m_open_studio(0);
            }
        }
        if let Some(path) = open {
            let recovering = backups;
            if self.m_open_project_file(ui.ctx(), path, recovering) {
                self.m_open_studio(0);
            }
        }
    }

    fn m_open_project_file(&mut self, ctx: &egui::Context, path: PathBuf, recovering: bool) -> bool {
        if !self.open_path(path.clone()) {
            ctx.data_mut(|d| d.insert_temp(egui::Id::new("m_project_save_error"), self.status.clone()));
            return false;
        }
        if recovering {
            self.recent.retain(|recent| recent != &path);
            self.path = None;
            return self.m_save_project_to_list(ctx);
        }
        self.hub_cache = None;
        self.arr = arrangement_ui::ArrState::new();
        if self.status.starts_with("Opened with warnings:") {
            self.arr.msg.clone_from(&self.status);
        }
        ctx.data_mut(|d| d.remove::<String>(egui::Id::new("m_project_save_error")));
        true
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
            3 => self.m_launchpad(ui),
            4 => self.m_mixer(ui),
            _ => self.m_tracks(ui),
        }
        self.m_step_sheet(ui.ctx());
        self.m_drum_sound_sheet(ui.ctx());
        self.m_export_sheet(ui.ctx());
        self.m_clip_sheet(ui.ctx());
        self.m_track_sheet(ui.ctx());
        self.m_instrument_editor(ui.ctx());
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

    fn m_drum_rack_clip(&self) -> Option<usize> {
        let clips = &self.project.arrangement.clips;
        let editable = |clip: &crate::arrangement::Clip| {
            !clip.audio && clip.offset == 0 && self.project.patterns.get(clip.pattern).is_some()
                && self.project.arrangement.tracks.get(clip.lane).is_some_and(|track| !track.playlist_locked)
        };
        self.arr.sel_clip.filter(|&index| clips.get(index).is_some_and(editable))
            .or_else(|| clips.iter().position(|clip| editable(clip)
                && clip.pattern == self.project.current_pattern
                && self.project.arrangement.tracks[clip.lane].name == "Drum Rack"))
    }

    fn m_create_drum_rack_clip(&mut self, source: usize) -> Option<usize> {
        let mut pattern = crate::sequencer::Pattern::new("Drum Rack", self.project.patterns[source].length_steps.clamp(16, crate::sequencer::MAX_STEPS as u32));
        pattern.clip_only = true;
        pattern.drums.clone_from(&self.project.patterns[source].drums);
        let start = self.shared.position.load(Ordering::Relaxed) / TICKS_PER_BAR * TICKS_PER_BAR;
        let duration = pattern.len_ticks();
        let lane = self.arr.sel_lane;
        let can_use_lane = self.project.arrangement.tracks.get(lane).is_some_and(|track|
            track.kind == crate::arrangement::TrackKind::Midi && !track.playlist_locked)
            && self.project.arrangement.can_place(
                &crate::arrangement::Clip::new(self.project.patterns.len(), lane, start, duration, false), None);
        let target = if can_use_lane { lane } else {
            self.project.arrangement.add_track(crate::arrangement::TrackKind::Midi);
            let lane = self.project.arrangement.tracks.len() - 1;
            self.project.arrangement.tracks[lane].name = "Drum Rack".into();
            self.project.arrangement.tracks[lane].instrument = Some(Track::Drums);
            self.project.arrangement.tracks[lane].blank = false;
            lane
        };
        let index = self.project.arrangement.clips.len();
        let clip = crate::arrangement::Clip::new(self.project.patterns.len(), target, start, duration, false);
        self.project.patterns.push(pattern);
        self.project.arrangement.clips.push(clip);
        Some(index)
    }

    /// Drum step sequencer: one card per drum, two rows of 8 big steps per 16-step page.
    /// Step sequencer laid out like FL Studio Mobile: one row per drum (name on the left, selected one highlighted,
    /// hold it for the sound menu), the steps in a line on the right; the Mixer and Sample tabs swap the steps for
    /// that row's knobs; the graph editor under the rows edits velocity, pan, pitch or chance per step.
    fn m_steps(&mut self, ui: &mut Ui) {
        let active_clip = self.m_drum_rack_clip();
        let pi = active_clip.map_or(self.project.current_pattern, |index| self.project.arrangement.clips[index].pattern);
        if pi >= self.project.patterns.len() {
            ui.label(RichText::new("No pattern yet. Start from a template in Projects.").color(muted()));
            return;
        }
        self.project.current_pattern = pi;
        let tab_id = egui::Id::new("m_steps_tab");
        let sel_id = egui::Id::new("m_steps_selected");
        let graph_id = egui::Id::new("m_steps_graph");
        let graph_open_id = egui::Id::new("m_steps_graph_open");
        let mut tab = ui.data(|d| d.get_temp::<u8>(tab_id)).unwrap_or(0);
        let mut selected = ui.data(|d| d.get_temp::<usize>(sel_id)).unwrap_or(0).min(NUM_DRUMS - 1);
        let mut graph = ui.data(|d| d.get_temp::<u8>(graph_id)).unwrap_or(0);
        let mut graph_open = ui.data(|d| d.get_temp::<bool>(graph_open_id)).unwrap_or(true);
        let grid_id = egui::Id::new("m_steps_grid");
        let mut snap = ui.data(|d| d.get_temp::<u32>(grid_id)).unwrap_or(TICKS_PER_STEP);
        ui.horizontal(|ui| {
            ui.spacing_mut().interact_size.y = 38.0;
            egui::ComboBox::from_id_salt("m_steps_grid_choice").width(88.0)
                .selected_text(format!("Grid {}", grid_label(snap))).show_ui(ui, |ui| {
                    for (ticks, label) in GRID_VALUES.into_iter().filter(|&(ticks, _)| ticks >= TICKS_PER_STEP) {
                        ui.selectable_value(&mut snap, ticks, label);
                    }
                });
            #[cfg(not(target_arch = "wasm32"))]
            if let Some(index) = active_clip {
                if p_button(ui, "To Audio", false, vec2(78.0, 38.0)).clicked() {
                    self.m_stop(ui.ctx());
                    self.m_select_studio_section(ui.ctx(), SEC_PLAYLIST);
                    ui.data_mut(|d| d.insert_temp(egui::Id::new("render_selection_job"), vec![index]));
                    return;
                }
            }
        });
        ui.data_mut(|d| d.insert_temp(grid_id, snap));
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let (rect, resp) = ui.allocate_exact_size(vec2(92.0, 36.0), Sense::click_and_drag());
            panel(ui.painter(), rect, 8.0);
            let knob = Rect::from_center_size(rect.right_center() - vec2(18.0, 0.0), vec2(28.0, 28.0));
            fl_knob(ui.painter(), knob, self.project.swing, false);
            ui.painter().text(rect.left_center() + vec2(10.0, 0.0), Align2::LEFT_CENTER, "SWING", FontId::proportional(12.0), p_accent());
            if resp.dragged() {
                self.project.swing = (self.project.swing - resp.drag_delta().y * 0.005 + resp.drag_delta().x * 0.005).clamp(0.0, 1.0);
            }
            if resp.double_clicked() {
                self.project.swing = 0.0;
            }
            let w = ((ui.available_width() - 8.0) / 3.0).max(40.0);
            for (index, label) in ["Steps", "Mixer", "Sample"].into_iter().enumerate() {
                if p_button(ui, label, tab == index as u8, vec2(w, 36.0)).clicked() {
                    tab = index as u8;
                }
            }
        });
        ui.add_space(4.0);
        let pattern = &self.project.patterns[pi];
        let len = active_clip.map_or(pattern.length_steps.max(16), |index| {
            self.project.arrangement.clips[index].len.div_ceil(TICKS_PER_STEP).min(pattern.length_steps)
        }).clamp(1, crate::sequencer::MAX_STEPS as u32) as usize;
        let stride = (snap / TICKS_PER_STEP).max(1) as usize;
        let playing = self.playing() && !self.project.song_mode;
        let play_step = (self.shared.position.load(Ordering::Relaxed) / TICKS_PER_STEP) as usize % len;
        let name_w = 104.0;
        let row_h = 54.0;
        let step_w = 34.0;
        let graph_h = if graph_open { 150.0 } else { 0.0 };
        let mut sound_menu = None;
        let mut step_edit = None;
        let mut hit_lane = None;
        let mut toggle = None;
        if tab == 0 && portrait_view(ui) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 3.0;
                let width = ((ui.available_width() - 12.0) / 5.0).max(27.0);
                if p_button(ui, "Graph", graph_open, vec2(width, 30.0))
                    .on_hover_text("Step parameters").clicked()
                {
                    graph_open = !graph_open;
                }
                for (index, label) in ["Vel", "Pan", "Pitch", "Chance"].into_iter().enumerate() {
                    if p_button(ui, label, graph_open && graph == index as u8, vec2(width, 30.0)).clicked() {
                        graph = index as u8;
                        graph_open = true;
                    }
                }
            });
            let width = ui.available_width();
            let ruler_width = 27.0;
            let graph_width = if graph_open { (width * 0.16).clamp(32.0, 52.0) } else { 0.0 };
            let cell_width = ((width - ruler_width - graph_width) / NUM_DRUMS as f32).max(1.0);
            let cell_x = |left: f32, lane: usize| left + ruler_width + lane as f32 * cell_width;
            let (header, _) = ui.allocate_exact_size(vec2(width, 34.0), Sense::hover());
            let p = ui.painter_at(header);
            p.rect_filled(header, 0.0, bar_fill());
            p.text(pos2(header.left() + ruler_width * 0.5, header.center().y),
                Align2::CENTER_CENTER, "#", FontId::proportional(11.0), muted());
            for (lane, &name) in DRUM_NAMES.iter().enumerate() {
                let rect = Rect::from_min_size(pos2(cell_x(header.left(), lane), header.top()), vec2(cell_width, header.height()));
                let color = lane_col(lane);
                if lane == selected {
                    p.rect_filled(rect, 0.0, selected_fill());
                }
                p.vline(rect.left(), rect.y_range(), Stroke::new(1.0, outline()));
                p.hline(rect.x_range(), rect.bottom() - 1.0, Stroke::new(if lane == selected { 2.0 } else { 1.0 }, color));
                fit_label(&p, rect.shrink2(vec2(3.0, 0.0)), rect.center().y, name, ink(), 10.0);
                let sample = self.project.drum_samples[lane].as_ref().map_or("Built-in", |sample| sample.name.as_str());
                let hit = ui.interact(rect, ui.id().with(("vertical_drum_header", lane)), Sense::click())
                    .on_hover_text(format!("{name} · {sample}"));
                if hit.long_touched() || hit.secondary_clicked() {
                    sound_menu = Some(lane);
                } else if hit.clicked() {
                    selected = lane;
                    self.shared.send(Cmd::DrumHit { lane, vel: 112 });
                }
            }
            if graph_open {
                let graph_header = Rect::from_min_max(
                    pos2(cell_x(header.left(), NUM_DRUMS), header.top()), header.max);
                p.vline(graph_header.left(), graph_header.y_range(), Stroke::new(1.0, outline()));
                fit_label(&p, graph_header.shrink2(vec2(3.0, 0.0)), graph_header.top() + 10.0,
                    DRUM_NAMES[selected], lane_col(selected), 10.0);
                fit_label(&p, graph_header.shrink2(vec2(3.0, 0.0)), graph_header.top() + 23.0,
                    ["Vel", "Pan", "Pitch", "Chance"][usize::from(graph)], muted(), 9.0);
            }
            let row_height = 36.0;
            ui.scope(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                mobile_scroll(egui::ScrollArea::vertical()).id_salt("vertical_drum_steps")
                    .auto_shrink([false, false]).show_rows(ui, row_height, len.div_ceil(stride), |ui, rows| {
                        for row in rows {
                            let step = row * stride;
                            let (rect, _) = ui.allocate_exact_size(vec2(width, row_height), Sense::hover());
                            let p = ui.painter_at(rect);
                            let bar_start = step % 16 == 0;
                            let beat_start = step % 4 == 0;
                            let running = playing && play_step / stride == row;
                            let shade = if running { selected_fill() } else if (step / 4) % 2 == 0 { card_fill() } else { bar_fill() };
                            p.rect_filled(rect, 0.0, shade);
                            let ruler = Rect::from_min_size(rect.min, vec2(ruler_width, row_height));
                            let ruler_fill = if running || bar_start { selected_fill() }
                                else if beat_start { card_fill() } else { bar_fill() };
                            p.rect_filled(ruler, 0.0, ruler_fill);
                            if bar_start {
                                p.text(ruler.center() - vec2(0.0, 7.0), Align2::CENTER_CENTER,
                                    format!("B{}", step / 16 + 1), FontId::proportional(9.0), p_accent());
                            }
                            p.text(ruler.center() + vec2(0.0, if bar_start { 7.0 } else { 0.0 }),
                                Align2::CENTER_CENTER, step + 1, FontId::proportional(10.0),
                                if running || beat_start { ink() } else { muted() });
                            let divider = if bar_start { Stroke::new(2.0, p_accent().gamma_multiply(0.68)) }
                                else if beat_start { Stroke::new(1.4, outline()) }
                                else { Stroke::new(0.65, grid_line()) };
                            p.hline(rect.x_range(), rect.top(), divider);
                            for lane in 0..NUM_DRUMS {
                                let cell = Rect::from_min_size(pos2(cell_x(rect.left(), lane), rect.top()), vec2(cell_width, row_height));
                                p.vline(cell.left(), cell.y_range(), Stroke::new(1.0, grid_line()));
                                let st = self.project.patterns[pi].drums[lane][step];
                                if st.on {
                                    let note = cell.shrink2(vec2(3.0, 5.0));
                                    let color = lane_col(lane).linear_multiply(0.50 + 0.50 * f32::from(st.vel) / 127.0);
                                    // soft glow around a lit step in its drum's colour
                                    for (grow, alpha) in [(5.0, 0.10), (2.5, 0.18)] {
                                        p.rect_filled(note.expand(grow), 5.0 + grow, lane_col(lane).gamma_multiply(alpha));
                                    }
                                    p.rect_filled(note, 5.0, color);
                                    clip_shine(&p, note, lane_col(lane), false);
                                    if st.is_smart() {
                                        p.circle_filled(note.right_center() - vec2(5.0, 0.0), 2.0, Color32::from_black_alpha(160));
                                    }
                                }
                                let hit = ui.interact(cell.intersect(ui.clip_rect()),
                                    ui.id().with(("vertical_drum_step", lane, step)), Sense::click());
                                if hit.secondary_clicked() {
                                    step_edit = Some((pi, lane, step));
                                } else if hit.clicked() {
                                    toggle = Some((lane, step));
                                }
                            }
                            if graph_open {
                                let plot = Rect::from_min_max(pos2(cell_x(rect.left(), NUM_DRUMS), rect.top()), rect.max);
                                p.vline(plot.left(), plot.y_range(), Stroke::new(1.0, outline()));
                                let st = &mut self.project.patterns[pi].drums[selected][step];
                                let sense = if st.on { Sense::click_and_drag() } else { Sense::hover() };
                                let hit = ui.interact(plot.intersect(ui.clip_rect()), ui.id().with(("vertical_drum_value", step)), sense);
                                let inner = plot.shrink2(vec2(4.0, 10.0));
                                if hit.double_clicked() {
                                    set_drum_step_graph_value(st, graph, if graph == 0 { 100.0 / 127.0 } else if graph == 3 { 1.0 } else { 0.5 });
                                } else if let Some(point) = hit.interact_pointer_pos().filter(|_| hit.dragged() || hit.clicked()) {
                                    set_drum_step_graph_value(st, graph, ((point.x - inner.left()) / inner.width()).clamp(0.0, 1.0));
                                }
                                let (value, bipolar) = drum_step_graph_value(st, graph);
                                let start = if bipolar { inner.center().x } else { inner.left() };
                                let end = inner.left() + inner.width() * value;
                                let bar = Rect::from_min_max(pos2(start.min(end), inner.top()), pos2(start.max(end) + 1.0, inner.bottom()));
                                let color = lane_col(selected);
                                if st.on {
                                    p.rect_filled(bar, 2.0, color);
                                }
                                if let Some(point) = hit.interact_pointer_pos().filter(|_| hit.dragged()) {
                                    p.rect_stroke(rect.shrink(0.5), 2.0, Stroke::new(1.2, color));
                                    let value = match graph {
                                        0 => format!("Velocity {}", st.vel),
                                        1 => format!("Pan {:+}", st.pan),
                                        2 => format!("Pitch {:+} st", st.pitch),
                                        _ => format!("Chance {}%", st.prob),
                                    };
                                    let ctx = ui.ctx();
                                    let tooltip = ctx.layer_painter(egui::LayerId::new(
                                        egui::Order::Tooltip, ui.id().with("drum_graph_value")));
                                    let label = tooltip.layout_no_wrap(value, FontId::proportional(12.0), ink());
                                    let size = label.size() + vec2(16.0, 10.0);
                                    let screen = ctx.screen_rect();
                                    let max_x = (screen.right() - size.x - 4.0).max(screen.left() + 4.0);
                                    let x = (point.x - size.x - 12.0).clamp(screen.left() + 4.0, max_x);
                                    let y = (point.y - size.y - 7.0).max(screen.top() + 4.0);
                                    let tip = Rect::from_min_size(pos2(x, y), size);
                                    tooltip.rect_filled(tip, 7.0, bar_fill());
                                    tooltip.rect_stroke(tip, 7.0, Stroke::new(1.0, color));
                                    tooltip.galley(tip.min + vec2(8.0, 5.0), label, ink());
                                }
                            }
                            if running {
                                p.hline(rect.x_range(), rect.bottom() - 1.0, Stroke::new(2.0, p_accent()));
                            }
                        }
                    });
            });
        } else {
            // own layout instead of a ScrollArea: a sideways swipe on the steps scrolls them, a tap hits exactly the
            // cell under the finger (the ScrollArea's own drag shifted taps by a step)
            let total_h = NUM_DRUMS as f32 * row_h + 36.0 + graph_h + 12.0;
            let (view, _) = ui.allocate_exact_size(vec2(ui.available_width(), total_h), Sense::hover());
            let origin = view.min;
            let offset_id = egui::Id::new(("m_steps_offset", pi));
            let visible_w = (view.width() - name_w).max(step_w);
            let max_offset = (len.div_ceil(stride) as f32 * step_w - visible_w).max(0.0);
            let mut offset = ui.data(|d| d.get_temp::<f32>(offset_id)).unwrap_or(0.0).clamp(0.0, max_offset);
            let steps_area = Rect::from_min_max(pos2(view.left() + name_w, view.top()),
                pos2(view.right(), view.top() + NUM_DRUMS as f32 * row_h));
            let graph_area = Rect::from_min_max(pos2(view.left() + name_w, view.top() + NUM_DRUMS as f32 * row_h + 42.0), view.max);
            if tab == 0 {
                // a sideways swipe anywhere over the steps or the graph scrolls them together
                let swipe = ui.interact(steps_area, ui.id().with("fl_steps_swipe"), Sense::drag());
                if swipe.dragged() && swipe.drag_delta().x.abs() > swipe.drag_delta().y.abs() {
                    offset = (offset - swipe.drag_delta().x).clamp(0.0, max_offset);
                }
                let _ = graph_area;
            }
            ui.data_mut(|d| d.insert_temp(offset_id, offset));
            let p = ui.painter_at(view);
            let steps_left = view.left() + name_w - offset;
            for lane in 0..NUM_DRUMS {
                let y = origin.y + lane as f32 * row_h;
                let row = Rect::from_min_size(pos2(view.left(), y), vec2(view.width(), row_h));
                p.line_segment([row.left_bottom(), row.right_bottom()], Stroke::new(1.0, p_bg()));
                let col = lane_col(lane);
                match tab {
                    0 => {
                        for s in (0..len).step_by(stride) {
                            let x = steps_left + (s / stride) as f32 * step_w;
                            let cell = Rect::from_min_size(pos2(x, y), vec2(step_w, row_h)).shrink2(vec2(1.5, 3.0));
                            if cell.right() < view.left() + name_w || cell.left() > view.right() {
                                continue;
                            }
                            // groups of four steps alternate their shade, like FL
                            let shade = if (s / 4) % 2 == 0 { card_fill() } else { card_fill().gamma_multiply(0.7) };
                            let st = self.project.patterns[pi].drums[lane][s];
                            if st.on {
                                p.rect_filled(cell, 3.0, col.linear_multiply(0.45 + 0.55 * st.vel as f32 / 127.0));
                                p.line_segment([cell.left_top() + vec2(5.0, 6.0), cell.right_top() + vec2(-5.0, 6.0)], Stroke::new(2.0, Color32::from_black_alpha(140)));
                                if st.is_smart() {
                                    p.circle_filled(cell.right_bottom() - vec2(6.0, 6.0), 2.5, Color32::WHITE);
                                }
                            } else {
                                p.rect_filled(cell, 3.0, shade);
                            }
                            if playing && play_step / stride == s / stride {
                                p.rect_stroke(cell.expand(1.0), 3.0, Stroke::new(2.0, if st.on { Color32::WHITE } else { p_accent().gamma_multiply(0.6) }));
                            }
                            let resp = ui.interact(cell.intersect(steps_area), ui.id().with(("fl_step", lane, s)), Sense::click());
                            if resp.long_touched() || resp.secondary_clicked() {
                                step_edit = Some((pi, lane, s));
                            } else if resp.clicked() {
                                toggle = Some((lane, s));
                            }
                        }
                    }
                    1 => {
                        // Mixer: level, pan, mute
                        let ch = &mut self.project.channels[lane];
                        let mut x = view.left() + name_w + 10.0;
                        for (label, value, range, bipolar) in [("Level", &mut ch.volume, (0.0f32, 1.0f32), false), ("Pan", &mut ch.pan, (-1.0, 1.0), true)] {
                            p.text(pos2(x, y + row_h * 0.5), Align2::LEFT_CENTER, label, FontId::proportional(12.0), p_accent());
                            let knob = Rect::from_center_size(pos2(x + 58.0, y + row_h * 0.5), vec2(36.0, 36.0));
                            let hit = ui.interact(knob.expand(4.0), ui.id().with(("fl_knob", label, lane)), Sense::click_and_drag());
                            if hit.dragged() {
                                *value = (*value + (hit.drag_delta().x - hit.drag_delta().y) * 0.005 * (range.1 - range.0)).clamp(range.0, range.1);
                            }
                            if hit.double_clicked() {
                                *value = if bipolar { 0.0 } else { 0.8 };
                            }
                            fl_knob(&p, knob, (*value - range.0) / (range.1 - range.0), bipolar);
                            x += 96.0;
                        }
                        let mute = Rect::from_center_size(pos2(x + 16.0, y + row_h * 0.5), vec2(34.0, 30.0));
                        p.rect_filled(mute, 6.0, if ch.mute { RED.gamma_multiply(0.7) } else { card_fill() });
                        p.text(mute.center(), Align2::CENTER_CENTER, "M", FontId::proportional(13.0), ink());
                        if ui.interact(mute, ui.id().with(("fl_mute", lane)), Sense::click()).clicked() {
                            ch.mute = !ch.mute;
                        }
                    }
                    _ => {
                        // Sample: reverse, start, fade out (the processed copy is rebuilt from the original)
                        let before = self.project.drum_fx[lane];
                        let mut fx = before;
                        let mut x = view.left() + name_w + 8.0;
                        let rev = Rect::from_min_size(pos2(x, y + row_h * 0.5 - 15.0), vec2(42.0, 30.0));
                        p.rect_filled(rev, 6.0, if fx.reverse { p_accent().gamma_multiply(0.6) } else { card_fill() });
                        p.text(rev.center(), Align2::CENTER_CENTER, "REV", FontId::proportional(12.0), ink());
                        if ui.interact(rev, ui.id().with(("fl_rev", lane)), Sense::click()).clicked() {
                            fx.reverse = !fx.reverse;
                        }
                        x = rev.right() + 10.0;
                        for (label, value, max) in [("Start", &mut fx.start, 0.9f32), ("Fade", &mut fx.fade_out, 1.0)] {
                            p.text(pos2(x, y + row_h * 0.5), Align2::LEFT_CENTER, label, FontId::proportional(12.0), p_accent());
                            let knob = Rect::from_center_size(pos2(x + 56.0, y + row_h * 0.5), vec2(36.0, 36.0));
                            let hit = ui.interact(knob.expand(4.0), ui.id().with(("fl_fx", label, lane)), Sense::click_and_drag());
                            if hit.dragged() {
                                *value = (*value + (hit.drag_delta().x - hit.drag_delta().y) * 0.004).clamp(0.0, max);
                            }
                            if hit.double_clicked() {
                                *value = 0.0;
                            }
                            fl_knob(&p, knob, *value / max, false);
                            x += 90.0;
                        }
                        if fx.start >= fx.end - 0.01 {
                            fx.start = (fx.end - 0.02).max(0.0);
                        }
                        if fx != before {
                            self.project.drum_fx[lane] = fx;
                            let original = self.project.drum_samples[lane].as_ref()
                                .and_then(|sample| self.lib.get(std::path::Path::new(&sample.path)).ok());
                            if let (Some(original), Some(sample)) = (original, self.project.drum_samples[lane].as_mut()) {
                                sample.data = Some(std::sync::Arc::new(fx.apply(&original)));
                            }
                        }
                    }
                }
                // the name column stays at the left edge while the steps scroll
                let name = Rect::from_min_size(pos2(view.left(), y), vec2(name_w, row_h)).shrink2(vec2(0.0, 1.5));
                let active = lane == selected;
                p.rect_filled(name, 4.0, if active { col.gamma_multiply(0.85) } else { bar_fill() });
                p.rect_filled(Rect::from_min_size(name.min, vec2(4.0, name.height())), 2.0, col);
                let sample = self.project.drum_samples.get(lane).and_then(Option::as_ref).map_or("Built-in".to_owned(), |s| s.name.clone());
                let text = name.shrink2(vec2(10.0, 0.0));
                fit_label(&p, text, name.top() + 20.0, DRUM_NAMES[lane], if active { Color32::BLACK } else { ink() }, 15.0);
                fit_label(&p, text, name.top() + 38.0, &sample, if active { Color32::from_black_alpha(170) } else { muted() }, 10.0);
                let hit = ui.interact(name, ui.id().with(("fl_name", lane)), Sense::click());
                if hit.long_touched() || hit.secondary_clicked() {
                    sound_menu = Some(lane);
                } else if hit.clicked() {
                    selected = lane;
                    self.shared.send(Cmd::DrumHit { lane, vel: 112 });
                }
            }
            // graph editor: one bar per step of the selected row
            let top = origin.y + NUM_DRUMS as f32 * row_h + 6.0;
            let tabs = Rect::from_min_size(pos2(view.left(), top), vec2(view.width(), 30.0));
            let chevron = Rect::from_min_size(tabs.min, vec2(44.0, 30.0));
            p.rect_filled(chevron, 6.0, card_fill());
            p.text(chevron.center(), Align2::CENTER_CENTER, if graph_open { "v" } else { "^" }, FontId::proportional(15.0), ink());
            if ui.interact(chevron, ui.id().with("fl_graph_toggle"), Sense::click()).on_hover_text("Graph editor").clicked() {
                graph_open = !graph_open;
            }
            let labels = ["Velocity", "Pan", "Pitch", "Chance"];
            let tab_w = ((tabs.width() - 50.0) / 4.0).max(40.0);
            for (index, label) in labels.iter().enumerate() {
                let r = Rect::from_min_size(pos2(chevron.right() + 6.0 + index as f32 * tab_w, top), vec2(tab_w - 4.0, 30.0));
                p.rect_filled(r, 6.0, if graph == index as u8 && graph_open { p_accent().gamma_multiply(0.55) } else { card_fill() });
                p.text(r.center(), Align2::CENTER_CENTER, *label, FontId::proportional(12.0), ink());
                if ui.interact(r, ui.id().with(("fl_graph_tab", index)), Sense::click()).clicked() {
                    graph = index as u8;
                    graph_open = true;
                }
            }
            if graph_open && tab == 0 {
                let plot_top = top + 36.0;
                let plot_h = graph_h - 40.0;
                let label = Rect::from_min_size(pos2(view.left(), plot_top), vec2(name_w, plot_h));
                p.rect_filled(label, 4.0, bar_fill());
                p.text(label.center(), Align2::CENTER_CENTER, format!("{}\n{}", DRUM_NAMES[selected], labels[graph as usize]),
                    FontId::proportional(12.0), muted());
                let col = lane_col(selected);
                for s in (0..len).step_by(stride) {
                    let x = steps_left + (s / stride) as f32 * step_w;
                    let cell = Rect::from_min_size(pos2(x, plot_top), vec2(step_w, plot_h)).shrink2(vec2(2.0, 0.0));
                    if cell.right() < view.left() + name_w || cell.left() > view.right() {
                        continue;
                    }
                    p.rect_filled(cell, 2.0, if (s / 4) % 2 == 0 { card_fill().gamma_multiply(0.6) } else { card_fill().gamma_multiply(0.4) });
                    let st = &mut self.project.patterns[pi].drums[selected][s];
                    // value as 0..1 and whether it is drawn from the middle line
                    let (t, mid) = drum_step_graph_value(st, graph);
                    let fill = if st.on { col } else { muted().gamma_multiply(0.35) };
                    let y_at = |t: f32| cell.bottom() - t.clamp(0.0, 1.0) * cell.height();
                    let bar = if mid {
                        let (a, b) = (y_at(0.5), y_at(t));
                        Rect::from_min_max(pos2(cell.left() + 3.0, a.min(b)), pos2(cell.right() - 3.0, a.max(b) + 1.0))
                    } else {
                        Rect::from_min_max(pos2(cell.left() + 3.0, y_at(t)), pos2(cell.right() - 3.0, cell.bottom()))
                    };
                    p.rect_filled(bar, 2.0, fill);
                    let hit = ui.interact(cell, ui.id().with(("fl_graph", s)), Sense::click_and_drag());
                    if (hit.dragged() || hit.clicked()) && st.on {
                        if let Some(point) = hit.interact_pointer_pos() {
                            let t = ((cell.bottom() - point.y) / cell.height()).clamp(0.0, 1.0);
                            set_drum_step_graph_value(st, graph, t);
                        }
                    }
                    if hit.double_clicked() && st.on {
                        match graph { 0 => st.vel = 100, 1 => st.pan = 0, 2 => st.pitch = 0, _ => st.prob = 100 }
                    }
                }
            }
        }
        if let Some((lane, step)) = toggle {
            let target = active_clip.map_or_else(|| self.m_create_drum_rack_clip(pi), Some);
            if let Some(index) = target {
                let clip = self.project.arrangement.clips[index];
                let cell = &mut self.project.patterns[clip.pattern].drums[lane][step];
                cell.on = !cell.on;
                if cell.on && cell.vel == 0 {
                    cell.vel = 100;
                }
                self.arr.sel_lane = clip.lane;
                self.arr.sel_clip = Some(index);
                self.project.current_pattern = clip.pattern;
                self.m_playlist_undo_step();
                self.publish_edits();
                hit_lane = Some(lane);
            }
        }
        if let Some(lane) = hit_lane {
            selected = lane;
        }
        if let Some(lane) = sound_menu {
            selected = lane;
            ui.data_mut(|d| d.insert_temp(egui::Id::new("m_drum_sound_sheet"), lane));
        }
        if let Some(edit) = step_edit {
            ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("m_step_edit"), edit));
        }
        ui.data_mut(|d| {
            d.insert_temp(tab_id, tab);
            d.insert_temp(sel_id, selected);
            d.insert_temp(graph_id, graph);
            d.insert_temp(graph_open_id, graph_open);
        });
    }

    fn m_piano(&mut self, ui: &mut Ui) {
        self.m_pattern_chips(ui);
        self.m_piano_editor(ui, self.project.current_pattern, None);
    }

    fn m_piano_editor(&mut self, ui: &mut Ui, pi: usize, drawer: Option<(egui::Id, bool)>) -> bool {
        let selected_clip = self.arr.sel_clip.filter(|&index| {
            self.project.arrangement.clips.get(index).is_some_and(|clip| !clip.audio && clip.pattern == pi)
        });
        let clip_index = drawer.and(selected_clip);
        let clip = clip_index.map(|index| self.project.arrangement.clips[index]);
        let clip_start = clip.map_or(0, |clip| clip.offset);
        let editor = (clip_index.is_some(), clip_index.unwrap_or(pi));
        let oct_id = egui::Id::new(("m_piano_oct", editor));
        let chord_id = egui::Id::new("m_piano_chords");
        let loop_id = egui::Id::new("m_piano_loop");
        let default_oct = self.project.patterns.get(pi).and_then(|pattern| pattern.notes.first())
            .map_or(4, |note| (note.pitch / 12).min(9));
        let mut oct = ui.data(|d| d.get_temp::<u8>(oct_id)).unwrap_or(default_oct);
        let previous_oct = oct;
        let mut chords = ui.data(|d| d.get_temp::<bool>(chord_id)).unwrap_or(false);
        let mut looping = ui.data(|d| d.get_temp::<bool>(loop_id)).unwrap_or(true);
        let names = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
        let mut record_id = egui::Id::new(("m_piano_record", pi));
        let mut record_step = ui.data(|d| d.get_temp::<usize>(record_id));
        let screen = ui.ctx().screen_rect();
        let portrait = screen.width() < screen.height();
        let ruler_width = if portrait { 50.0 } else { 58.0 };
        let toolbar_width = 96.0;
        let pattern = self.project.patterns.get(pi);
        let valid_pattern = pattern.is_some();
        let steps = clip.map_or_else(
            || pattern.map_or(1, |pattern| pattern.length_steps.max(1) as usize),
            |clip| clip.len.max(1).div_ceil(TICKS_PER_STEP) as usize,
        );
        let zoom_id = egui::Id::new(("m_piano_step_w", portrait, drawer.is_some()));
        let default_step_w = if portrait {
            ((ui.available_width() - ruler_width) / 16.0).clamp(14.0, 36.0)
        } else {
            ((ui.available_width() - ruler_width - toolbar_width - 8.0) / steps.min(16) as f32).clamp(32.0, 72.0)
        };
        let mut step_w = ui.data(|d| d.get_temp::<f32>(zoom_id)).unwrap_or(default_step_w);
        let snap_id = egui::Id::new("m_piano_snap");
        let mut snap: u32 = ui.data(|d| d.get_temp::<u32>(snap_id)).unwrap_or(TICKS_PER_STEP);
        let mut zoom_changed = false;
        let velocity_id = egui::Id::new(("m_piano_velocity", portrait));
        let slide_id = egui::Id::new(("m_piano_slide", editor));
        let mut slide_mode = ui.data(|d| d.get_temp::<bool>(slide_id)).unwrap_or(false);
        let mut show_velocity = ui.data(|d| d.get_temp::<bool>(velocity_id)).unwrap_or(portrait);
        let mut close = false;
        let tools_id = egui::Id::new(("m_piano_tools", drawer.is_some(), portrait));
        let mut tools_open = ui.data(|d| d.get_temp::<bool>(tools_id)).unwrap_or(false);
        let range_id = egui::Id::new(("m_piano_loop_range", editor));
        let length_error_id = egui::Id::new(("m_piano_length_error", editor));
        let mut loop_range = ui.data(|d| d.get_temp::<(u32, u32)>(range_id));
        let copy_id = egui::Id::new(("m_piano_beat_selection", editor));
        let mut selecting = ui.data(|d| d.get_temp::<bool>(copy_id)).unwrap_or(false);
        let mut selection = ui.data(|d| d.get_temp::<(u32, u32)>(copy_id.with("range")));
        let mut copy = false;
        let mut extend = false;
        let mut history_changed = false;
        // part 0 = the transport (Done, Play, Stop, Full view, Tools), 1 = the rest, 2 = both
        let mut toolbar = |ui: &mut Ui, part: u8| {
            if part != 1 { piano_toolbar_row(ui, portrait, |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                if drawer.is_some() {
                    close = p_button(ui, "Done", true, vec2(60.0, 44.0)).clicked();
                } else if !portrait {
                    let ctx = ui.ctx().clone();
                    if p_button(ui, "Playlist", false, vec2(66.0, 44.0)).clicked() {
                        self.m_select_studio_section(&ctx, SEC_PLAYLIST);
                    }
                    self.m_studio_orientation_button(ui, &ctx, 44.0);
                }
                if !valid_pattern {
                    return;
                }
                let pattern_playing = self.m_transport_playing(ui.ctx()) && !self.project.song_mode && self.project.current_pattern == pi;
                let transport = if pattern_playing { Icon::Pause } else { Icon::Play };
                if studio_icon(ui, 44.0, transport, true, if pattern_playing { "Pause pattern" } else { "Play pattern" }).clicked() {
                    if pattern_playing {
                        self.m_pause(ui.ctx());
                    } else {
                        let range = if let Some(clip) = clip {
                        let end = clip.offset.saturating_add(clip.len.max(1));
                        loop_range.map(|(a, b)| (a.max(clip.offset), b.min(end)))
                            .filter(|(a, b)| a < b).or(Some((clip.offset, end)))
                    } else {
                        loop_range
                    };
                        self.m_start_playback_in_range(ui.ctx(), false, pi, looping, range);
                    }
                }
                if round_icon(ui, 44.0, Icon::Stop, true, "Stop").clicked() {
                    self.m_stop(ui.ctx());
                }
                if !portrait {
                    if let Some((full_id, full)) = drawer {
                        if studio_icon(ui, 44.0, Icon::FullView, full, if full { "Restore view" } else { "Full view" }).clicked() {
                            ui.data_mut(|d| d.insert_temp(full_id, !full));
                        }
                    }
                }
                if p_button(ui, "Tools", tools_open, vec2(62.0, 44.0)).clicked() {
                    tools_open = !tools_open;
                    ui.data_mut(|d| d.insert_temp(tools_id, tools_open));
                }
                if p_button(ui, "Select", selecting, vec2(64.0, 44.0))
                    .on_hover_text("Switch ruler mode").clicked()
                {
                    selecting = !selecting;
                }
                if selecting && selection.is_some() {
                    copy = p_button(ui, "Copy →", true, vec2(72.0, 44.0)).clicked();
                }
            }); }
            if part == 0 {
                return;
            }
            if valid_pattern && tools_open {
                let mut tools = |ui: &mut Ui| {
                        let row = |ui: &mut Ui, add: &mut dyn FnMut(&mut Ui)| if portrait {
                            // one scrolling row in portrait (inside the horizontal ScrollArea below)
                            ui.horizontal(|ui| add(ui));
                        } else {
                            piano_toolbar_row(ui, false, |ui| add(ui));
                        };
                        row(ui, &mut |ui: &mut Ui| {
                            ui.spacing_mut().interact_size.y = 44.0;
                            if p_button(ui, "Rec", record_step.is_some(), vec2(44.0, 44.0)).clicked() {
                                record_step = if record_step.is_some() { None } else { Some(0) };
                            }
                            if let Some((full_id, full)) = drawer.filter(|_| portrait) {
                                if studio_icon(ui, 44.0, Icon::FullView, full, if full { "Restore view" } else { "Full view" }).clicked() {
                                    ui.data_mut(|d| d.insert_temp(full_id, !full));
                                }
                            }
                            if p_button(ui, "Slide", slide_mode, vec2(64.0, 44.0))
                                .on_hover_text("Draw slide notes or tap a note to toggle its slide").clicked()
                            {
                                slide_mode = !slide_mode;
                            }
                            // the octave in view: − / + jump the grid a whole octave down / up and flash it
                            // (read the time first: ui.input inside ui.data_mut would lock the context twice)
                            let jump = |ui: &mut Ui| {
                                let now = ui.input(|i| i.time);
                                ui.data_mut(|d| d.insert_temp(egui::Id::new("m_piano_oct_flash"), now));
                            };
                            if p_button(ui, "Oct −", false, vec2(62.0, 44.0)).on_hover_text("Show one octave lower").clicked() && oct > 0 {
                                oct -= 1;
                                jump(ui);
                            }
                            ui.add_sized([46.0, 44.0], egui::Label::new(RichText::new(format!("C{}", i16::from(oct) - 1))
                                .size(15.0).strong().color(p_accent())));
                            if p_button(ui, "Oct +", false, vec2(62.0, 44.0)).on_hover_text("Show one octave higher").clicked() && oct < 9 {
                                oct += 1;
                                jump(ui);
                            }
                            egui::ComboBox::from_id_salt("m_piano_grid_choice").width(78.0)
                                .selected_text(format!("Grid {}", grid_label(snap)))
                                .show_ui(ui, |ui| {
                                    for (ticks, label) in GRID_VALUES {
                                        ui.selectable_value(&mut snap, ticks, label);
                                    }
                                });
                            if p_button(ui, "Zoom −", false, vec2(70.0, 44.0)).clicked() {
                                step_w = (step_w / 1.25).max(8.0);
                                zoom_changed = true;
                            }
                            if p_button(ui, "Zoom +", false, vec2(70.0, 44.0)).clicked() {
                                step_w = (step_w * 1.25).min(72.0);
                                zoom_changed = true;
                            }
                            if p_button(ui, "Velocity", show_velocity, vec2(76.0, 44.0)).clicked() {
                                show_velocity = !show_velocity;
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
                            if p_button(ui, "Chords", chords, vec2(76.0, 44.0)).clicked() {
                                chords = !chords;
                            }
                            if p_button(ui, if looping { "Loop on" } else { "Loop off" }, looping, vec2(87.0, 44.0)).clicked() {
                                looping = !looping;
                                self.shared.send(Cmd::PatternLoop(looping));
                            }
                        });
                };
                tools(ui);
            }
            if valid_pattern {
                piano_toolbar_row(ui, portrait, |ui| {
                    extend = p_button(ui, "+1 beat", false, vec2(72.0, 40.0))
                        .on_hover_text("Extend the clip by one beat").clicked();
                    if loop_range.is_some() && p_button(ui, "Clear loop", false, vec2(92.0, 40.0)).clicked() {
                        loop_range = None;
                        self.shared.send(Cmd::PatternRange(clip.map(|clip| (pi, clip.offset, clip.offset.saturating_add(clip.len.max(1))))));
                    }
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        history_changed = self.m_studio_undo_redo(ui, 43.0, false);
                    });
                });
            }
        };
        let mut grid_ui;
        let ui = if portrait {
            // two rows: the transport, then the tools and +1 beat / Undo / Redo (they used to hide off the right edge)
            mobile_scroll(egui::ScrollArea::horizontal()).id_salt("m_piano_toolbar_scroll")
                .auto_shrink([false, true]).show(ui, |ui| {
                    ui.horizontal(|ui| toolbar(ui, 0));
                });
            mobile_scroll(egui::ScrollArea::horizontal()).id_salt("m_piano_toolbar_scroll_tools")
                .auto_shrink([false, true]).show(ui, |ui| {
                    ui.horizontal(|ui| toolbar(ui, 1));
                });
            ui
        } else {
            let area = ui.available_rect_before_wrap();
            let sidebar = Rect::from_min_max(pos2(area.right() - toolbar_width, area.top()), area.max);
            ui.painter().rect_filled(sidebar, 10.0, surface_fill());
            ui.painter().rect_stroke(sidebar, 10.0, Stroke::new(1.0, surface_edge()));
            let mut side = ui.new_child(egui::UiBuilder::new().id_salt("piano_toolbar").max_rect(sidebar.shrink(2.0)));
            side.set_clip_rect(sidebar.intersect(ui.clip_rect()));
            side.set_width(sidebar.width() - 4.0);
            // the transport stays put at the top; only the tools below it scroll
            toolbar(&mut side, 0);
            side.add_space(4.0);
            let side_height = side.available_height();
            mobile_scroll(egui::ScrollArea::vertical()).max_height(side_height)
                .auto_shrink([false, false]).show(&mut side, |ui| toolbar(ui, 1));
            let content = Rect::from_min_max(area.min, pos2(sidebar.left() - 8.0, area.bottom()));
            grid_ui = ui.new_child(egui::UiBuilder::new().id_salt("piano_content").max_rect(content));
            grid_ui.set_clip_rect(content.intersect(ui.clip_rect()));
            &mut grid_ui
        };
        if !valid_pattern {
            ui.label(RichText::new("No pattern selected.").color(muted()));
            return close;
        }
        ui.data_mut(|d| {
            d.insert_temp(oct_id, oct);
            d.insert_temp(chord_id, chords);
            d.insert_temp(loop_id, looping);
            if zoom_changed {
                d.insert_temp(zoom_id, step_w);
            }
            d.insert_temp(snap_id, snap);
            d.insert_temp(velocity_id, show_velocity);
            d.insert_temp(slide_id, slide_mode);
        });
        ui.data_mut(|d| d.insert_temp(copy_id, selecting));
        if let Some(range) = selection.filter(|_| copy) {
            match self.m_copy_piano_beats(pi, clip_index, range) {
                Ok(next) => {
                    ui.data_mut(|d| {
                        d.insert_temp(copy_id.with("range"), next);
                        d.remove::<String>(length_error_id);
                    });
                    ui.ctx().request_repaint();
                    return close;
                }
                Err(error) => ui.data_mut(|d| d.insert_temp(length_error_id, error.to_owned())),
            }
        }
        let mut pi = pi;
        if history_changed {
            ui.data_mut(|d| {
                d.remove::<String>(length_error_id);
                d.remove::<Vec<usize>>(copy_id.with(("notes", pi)));
            });
            ui.ctx().request_repaint();
            return close;
        }
        ui.label(RichText::new(if selecting { "Drag empty grid to select · Drag a selected note to move" } else if slide_mode { "Tap a note to toggle slide" } else { "Double-tap to select all · Hold a note to delete" })
            .size(11.0).color(muted()));
        if extend {
            match self.m_extend_piano(pi, clip_index) {
                Ok(()) => {
                    ui.data_mut(|d| d.remove::<String>(length_error_id));
                    let edited_pattern = self.project.current_pattern;
                    if edited_pattern != pi {
                        ui.data_mut(|d| d.remove::<usize>(record_id));
                        record_id = egui::Id::new(("m_piano_record", edited_pattern));
                        pi = edited_pattern;
                        self.shared.send(Cmd::PatternRange(loop_range.map(|(start, end)| (pi, start, end))));
                    }
                    ui.ctx().request_repaint();
                }
                Err(error) => ui.data_mut(|d| d.insert_temp(length_error_id, error.to_owned())),
            }
        }
        if let Some(error) = ui.data(|d| d.get_temp::<String>(length_error_id)) {
            ui.colored_label(RED, error);
        }
        let clip = clip_index.map(|index| self.project.arrangement.clips[index]);
        let duration = clip.map_or_else(|| self.project.patterns[pi].len_ticks(), |clip| clip.len).max(1);
        let pattern_end = clip_start.saturating_add(duration);
        let editable_end = pattern_end.min(crate::sequencer::MAX_STEPS as u32 * TICKS_PER_STEP);
        let steps = duration.div_ceil(TICKS_PER_STEP) as usize;
        loop_range = loop_range.map(|(start, end)| (start.max(clip_start), end.min(pattern_end)))
            .filter(|(start, end)| start < end);
        let (key_root, key_scale) = (self.project.key_root, self.project.key_scale);
        let base = oct * 12;
        let high_pitch = 127_u8;
        let rows = usize::from(high_pitch) + 1;
        let row_id = zoom_id.with("row_height");
        let mut row_h = ui.data(|d| d.get_temp::<f32>(row_id)).unwrap_or(24.0);
        // event editor height: dragged with the splitter above it, like FL's
        let event_h_id = egui::Id::new(("m_piano_event_h", portrait));
        let velocity_h = if show_velocity {
            ui.data(|d| d.get_temp::<f32>(event_h_id)).unwrap_or((ui.available_height() * 0.22).clamp(54.0, 105.0))
                .clamp(44.0, (ui.available_height() * 0.6).max(44.0))
        } else { 0.0 };
        let track = self.project.arrangement.tracks.get(self.arr.sel_lane).and_then(|track| track.instrument)
            .or_else(|| self.project.patterns[pi].notes.first().map(|note| note.track)).unwrap_or(Track::Synth);
        let held_id = egui::Id::new("m_piano_held");
        let mut held = ui.data(|d| d.get_temp::<(Track, u8)>(held_id));
        let playing = self.playing() && !self.project.song_mode && self.project.current_pattern == pi;
        let pos = self.shared.position.load(Ordering::Relaxed);
        let play_step = (pos.saturating_sub(clip_start).min(duration - 1) / TICKS_PER_STEP) as usize;
        let cursor = if playing { Some(play_step) } else { record_step.map(|step| step % steps) };
        let timeline_height = 23.0;
        // FL-style zoom bar over the ruler: drag the middle to scroll, an end to zoom in time
        let (zoom_bar, _) = ui.allocate_exact_size(vec2(ui.available_width(), 16.0), Sense::hover());
        let mut bar_offset: Option<f32> = None;
        let mut bar_step: Option<f32> = None;
        {
            let track = Rect::from_min_max(pos2(zoom_bar.left() + ruler_width, zoom_bar.top() + 2.0), zoom_bar.max - vec2(0.0, 2.0));
            let content_w = (steps as f32 * step_w).max(1.0);
            let grid_w = track.width().max(1.0);
            let offset = ui.data(|d| d.get_temp::<egui::Vec2>(zoom_id.with("scroll_offset"))).unwrap_or_default().x;
            let frac = (grid_w / content_w).min(1.0);
            let thumb_w = (track.width() * frac).max(24.0);
            let thumb_x = track.left() + (offset / content_w) * track.width();
            let thumb = Rect::from_min_size(pos2(thumb_x, track.top()), vec2(thumb_w, track.height()));
            let p = ui.painter();
            p.rect_filled(track, 6.0, bar_fill());
            p.rect_filled(thumb, 6.0, muted().gamma_multiply(0.6));
            for edge in [thumb.left() + 5.0, thumb.right() - 5.0] {
                p.line_segment([pos2(edge, thumb.top() + 3.0), pos2(edge, thumb.bottom() - 3.0)], Stroke::new(1.5, ink()));
            }
            let id = ui.id().with(("m_piano_zoom_bar", editor));
            let left = ui.interact(Rect::from_min_max(thumb.min - vec2(6.0, 0.0), pos2(thumb.left() + 10.0, thumb.bottom())), id.with("l"), Sense::drag());
            let right = ui.interact(Rect::from_min_max(pos2(thumb.right() - 10.0, thumb.top()), thumb.max + vec2(6.0, 0.0)), id.with("r"), Sense::drag());
            let body = ui.interact(thumb.shrink2(vec2(10.0, 0.0)), id.with("b"), Sense::drag());
            let px_to_content = content_w / track.width();
            if body.dragged() {
                bar_offset = Some((offset + body.drag_delta().x * px_to_content).clamp(0.0, (content_w - grid_w).max(0.0)));
            } else if right.dragged() || left.dragged() {
                let dx = if right.dragged() { right.drag_delta().x } else { -left.drag_delta().x };
                let new_w = (thumb_w + dx).clamp(24.0, track.width());
                let new_step = (grid_w / (new_w / track.width() * steps as f32)).clamp(8.0, 72.0);
                bar_step = Some(new_step);
                // keep the opposite end where it is
                let new_content = steps as f32 * new_step;
                bar_offset = Some(if left.dragged() {
                    let right_px = thumb.right() - track.left();
                    ((right_px - new_w) / track.width() * new_content).max(0.0)
                } else {
                    (offset / content_w * new_content).max(0.0)
                });
            }
        }
        let view_h = (ui.available_height() - velocity_h - if show_velocity { 12.0 } else { 4.0 }).max(timeline_height + 1.0);
        let (viewport, _) = ui.allocate_exact_size(vec2(ui.available_width(), view_h), Sense::hover());
        let grid = Rect::from_min_max(viewport.min + vec2(ruler_width, timeline_height), viewport.max);
        let (pinching, zoom) = grid_pinch(ui, zoom_id.with("pinch"), grid);
        let offset_id = zoom_id.with("scroll_offset");
        let mut zoom_offset = None;
        if let Some(width) = bar_step {
            step_w = width;
            ui.data_mut(|d| d.insert_temp(zoom_id, step_w));
        }
        if let Some(offset) = bar_offset {
            zoom_offset = Some((0usize, offset));
        }
        if let Some((axis, factor, center)) = zoom {
            let previous = vec2(step_w, row_h);
            step_w = if axis == 0 { (step_w * factor).clamp(8.0, 72.0) } else { step_w };
            row_h = if axis == 1 { (row_h * factor).clamp(14.0, 54.0) } else { row_h };
            let scale = vec2(step_w, row_h)[axis] / previous[axis];
            if scale != 1.0 {
                let anchor = (center - grid.min)[axis];
                let scroll_offset = ui.data(|d| d.get_temp::<egui::Vec2>(offset_id)).unwrap_or_default();
                zoom_offset = Some((axis, ((scroll_offset[axis] + anchor) * scale - anchor).max(0.0)));
                ui.data_mut(|d| {
                    d.insert_temp(zoom_id, step_w);
                    d.insert_temp(row_id, row_h);
                });
            }
        }
        let mut grid_ui = ui.new_child(egui::UiBuilder::new().max_rect(grid));
        grid_ui.set_clip_rect(grid.intersect(ui.clip_rect()));
        let grid_id = ("m_piano_grid_pc", editor);
        let initialized = egui::Id::new((grid_id, "initialized"));
        // one-finger scrolling is ours (see touch_pan); a press on a note edits the note instead
        let stored = ui.data(|d| d.get_temp::<egui::Vec2>(offset_id)).unwrap_or_default();
        let content = vec2(duration as f32 / TICKS_PER_STEP as f32 * step_w, rows as f32 * row_h);
        let panned = {
            let notes = &self.project.patterns[pi].notes;
            let origin = grid.min - stored;
            touch_pan(ui, zoom_id.with("pan"), grid, content, stored, !pinching && !selecting, |press| {
                notes.iter().any(|note| {
                    if note.start.saturating_add(note.len) <= clip_start || note.start >= pattern_end { return false; }
                    let x = origin.x + note.start.saturating_sub(clip_start) as f32 / TICKS_PER_STEP as f32 * step_w;
                    let y = origin.y + f32::from(high_pitch - note.pitch.min(high_pitch)) * row_h;
                    let w = (note.len as f32 / TICKS_PER_STEP as f32 * step_w).max(6.0);
                    Rect::from_min_size(pos2(x, y), vec2(w, row_h)).expand(2.0).contains(press)
                })
            })
        };
        let mut scroll = mobile_scroll(egui::ScrollArea::both()).id_salt(grid_id)
            .drag_to_scroll(false).max_height(grid.height()).auto_shrink([false, false])
            .horizontal_scroll_offset(panned.x).vertical_scroll_offset(panned.y);
        // no auto-follow: the view stays where the user scrolled it, only the playhead line moves
        let first_view = !ui.data(|d| d.get_temp::<bool>(initialized)).unwrap_or(false);
        if first_view || oct != previous_oct {
            scroll = scroll.vertical_scroll_offset(f32::from(high_pitch - base - 11) * row_h);
            ui.data_mut(|d| d.insert_temp(initialized, true));
        }
        if let Some((axis, offset)) = zoom_offset {
            scroll = if axis == 0 { scroll.horizontal_scroll_offset(offset) } else { scroll.vertical_scroll_offset(offset) };
        }
        let out = scroll.show(&mut grid_ui, |ui| {
            let grid_sense = if selecting && !pinching { Sense::click_and_drag() } else { Sense::click() };
            let (rect, resp) = ui.allocate_exact_size(vec2(duration as f32 / TICKS_PER_STEP as f32 * step_w, rows as f32 * row_h), grid_sense);
            let p = ui.painter_at(rect);
            panel(&p, rect, 4.0);
            // after Oct − / +: the octave that came into view glows for a moment
            let flash = ui.data(|d| d.get_temp::<f64>(egui::Id::new("m_piano_oct_flash")))
                .map(|at| (1.0 - (ui.input(|i| i.time) - at) / 0.9).clamp(0.0, 1.0) as f32).filter(|t| *t > 0.0);
            if flash.is_some() {
                ui.ctx().request_repaint();
            }
            for row in 0..rows {
                let pitch = high_pitch - row as u8;
                let y = rect.top() + row as f32 * row_h;
                let lane = Rect::from_min_size(pos2(rect.left(), y), vec2(rect.width(), row_h));
                if let Some(t) = flash.filter(|_| (base..base.saturating_add(12)).contains(&pitch)) {
                    p.rect_filled(lane, 0.0, p_accent().gamma_multiply(0.28 * t));
                }
                if held.is_some_and(|(_, key)| key == pitch) {
                    p.rect_filled(lane, 0.0, p_accent().linear_multiply(0.30));
                } else if crate::theory::is_root(key_root, pitch) {
                    p.rect_filled(lane, 0.0, p_accent().gamma_multiply(0.07));
                } else if !crate::theory::in_scale(key_root, key_scale, pitch) || matches!(pitch % 12, 1 | 3 | 6 | 8 | 10) {
                    p.rect_filled(lane, 0.0, pick(0x28000000, 0x18000000));
                }
                let edge = if pitch % 12 == 0 { outline() } else { grid_line().linear_multiply(0.34) };
                p.line_segment([lane.left_bottom(), lane.right_bottom()], Stroke::new(0.7, edge));
            }
            let pixels_per_tick = step_w / TICKS_PER_STEP as f32;
            if let Some((start, end)) = selection.filter(|_| selecting) {
                let x = |tick: u32| rect.left() + tick.saturating_sub(clip_start) as f32 * pixels_per_tick;
                let band = Rect::from_min_max(pos2(x(start), rect.top()), pos2(x(end), rect.bottom())).intersect(rect);
                if band.is_positive() {
                    p.rect_filled(band, 0.0, piano_note_fill().gamma_multiply(0.16));
                }
            }
            let beat = crate::sequencer::PPQ;
            let division = if snap == 1 { beat } else { visible_grid_step(snap, pixels_per_tick).min(beat) };
            let first_tick = clip_start + ((p.clip_rect().left() - rect.left()).max(0.0) / pixels_per_tick) as u32;
            let last_tick = clip_start + (((p.clip_rect().right() - rect.left()).max(0.0) / pixels_per_tick).ceil() as u32)
                .min(duration);
            // `tick` is a pattern tick: a clip trimmed from the left keeps the lines on the pattern's beats
            for tick in (first_tick.div_ceil(division) * division..=last_tick).step_by(division as usize) {
                let x = rect.left() + (tick - clip_start) as f32 * pixels_per_tick;
                let stroke = if tick % TICKS_PER_BAR == 0 {
                    Stroke::new(1.5, surface_edge())
                } else if tick % beat == 0 {
                    Stroke::new(1.0, outline())
                } else {
                    Stroke::new(0.6, grid_line().gamma_multiply(0.62))
                };
                p.line_segment([pos2(x, rect.top()), pos2(x, rect.bottom())], stroke);
            }
            if let Some(step) = cursor {
                // while playing the line glides with the exact tick (it used to jump from cell to cell);
                // the record cursor stays on its step
                let x = if playing {
                    let local = pos.saturating_sub(clip_start).min(duration);
                    rect.left() + local as f32 / TICKS_PER_STEP as f32 * step_w
                } else {
                    rect.left() + step as f32 * step_w
                };
                let color = if record_step.is_some() { RED } else { p_accent() };
                p.line_segment([pos2(x, rect.top()), pos2(x, rect.bottom())], Stroke::new(2.0, color));
            }
            let body = |len: u32, pitch: u8, start: u32| {
                let visible_start = start.max(clip_start);
                let visible_len = start.saturating_add(len).min(pattern_end).saturating_sub(visible_start);
                let x = rect.left() + visible_start.saturating_sub(clip_start) as f32 / TICKS_PER_STEP as f32 * step_w;
                let y = rect.top() + (high_pitch - pitch) as f32 * row_h;
                let width = (visible_len as f32 / TICKS_PER_STEP as f32 * step_w).max(6.0);
                Rect::from_min_size(pos2(x + 1.0, y + 1.0), vec2(width - 2.0, row_h - 2.0))
            };
            let selected_id = copy_id.with(("notes", pi));
            let drag_id = selected_id.with("drag");
            let notes = &self.project.patterns[pi].notes;
            let mut selected = ui.data(|d| d.get_temp::<Vec<usize>>(selected_id)).unwrap_or_default();
            let editable = |note: &crate::piano_roll::Note| {
                note.start >= clip_start && note.start.saturating_add(note.len) <= editable_end
                    && note.pitch <= high_pitch
            };
            selected.retain(|&index| notes.get(index).is_some_and(editable));
            let now = ui.input(|input| input.time);
            let pending_id = selected_id.with("pending_empty_tap");
            let tap_wait = std::time::Duration::from_millis(320);
            let mut pending = ui.data_mut(|d| d.remove_temp::<(f64, u32, u8)>(pending_id));
            let mut place_note = None;
            let key_input = !ui.ctx().wants_keyboard_input();
            let select_shortcut = key_input && ui.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::A));
            let mut select_all = !pinching && (select_shortcut || (!slide_mode && resp.double_clicked()));
            let mut group_delta = (0i64, 0i32);
            let mut end_group_move = false;
            if !selecting || pinching {
                ui.data_mut(|d| {
                    d.remove::<(Pos2, Pos2, bool, bool)>(drag_id);
                    if !selecting {
                        d.remove::<Vec<usize>>(selected_id);
                    }
                });
            } else {
                let mut drag = ui.data(|d| d.get_temp::<(Pos2, Pos2, bool, bool)>(drag_id));
                let note_at = |point: Pos2| notes.iter().enumerate().rev().find(|(_, note)| {
                    editable(note) && body(note.len, note.pitch, note.start).expand(1.0).contains(point)
                }).map(|(index, _)| index);
                let point = resp.interact_pointer_pos();
                let (raw_point, released, cancelled) = ui.input(|input| (
                    input.pointer.interact_pos(),
                    input.pointer.primary_released(),
                    input.key_pressed(egui::Key::Escape) || input.events.iter().any(|event| {
                        matches!(event, egui::Event::Touch { phase: egui::TouchPhase::Cancel, .. })
                    }),
                ));
                if cancelled {
                    drag = None;
                } else if resp.long_touched() {
                    // egui clears its drag candidate on long touch. Keep the selected note's grab
                    // until this finger moves or releases so a deliberate hold can start a move.
                    if let Some(origin) = ui.input(|input| input.pointer.press_origin()).or(point) {
                        if let Some(index) = note_at(origin) {
                            if !selected.contains(&index) {
                                selected = vec![index];
                            }
                            drag = Some((origin, origin, true, true));
                        }
                    }
                } else if resp.drag_started() {
                    if let Some(origin) = ui.input(|input| input.pointer.press_origin()).or(point) {
                        let moving = if let Some(index) = note_at(origin) {
                            if !selected.contains(&index) {
                                selected = vec![index];
                            }
                            true
                        } else {
                            false
                        };
                        drag = Some((origin, origin, moving, false));
                    }
                }
                if let Some((origin, last, moving, held)) = drag.filter(|&(_, _, _, held)| {
                    held || resp.dragged() || resp.drag_stopped()
                }) {
                    let stopped = if held { released } else { resp.drag_stopped() };
                    let point = point.or(raw_point).unwrap_or(last).clamp(p.clip_rect().min, p.clip_rect().max);
                    if moving && !selected.is_empty() {
                        let units = ((point.x - origin.x) / pixels_per_tick / snap as f32).round() as i64;
                        let semitones = ((origin.y - point.y) / row_h).round() as i32;
                        group_delta = (units, semitones);
                        end_group_move = stopped;
                    } else {
                        let area = Rect::from_two_pos(origin, point);
                        selected = notes.iter().enumerate().filter(|(_, note)| {
                            editable(note) && area.intersects(body(note.len, note.pitch, note.start))
                        }).map(|(index, _)| index).collect();
                        p.rect_filled(area, 3.0, p_accent().gamma_multiply(0.18));
                        p.rect_stroke(area, 3.0, Stroke::new(1.2, p_accent()));
                    }
                    drag = (!stopped && ui.input(|input| input.pointer.primary_down()))
                        .then_some((origin, point, moving, held));
                } else if resp.clicked() && !select_all {
                    selected = point.and_then(note_at).into_iter().collect();
                    drag = None;
                } else if !ui.input(|input| input.pointer.primary_down()) {
                    drag = None;
                }
                ui.data_mut(|d| {
                    d.insert_temp(selected_id, selected.clone());
                    if let Some(drag) = drag {
                        d.insert_temp(drag_id, drag);
                    } else {
                        d.remove::<(Pos2, Pos2, bool, bool)>(drag_id);
                    }
                });
            }
            if key_input && selecting && !pinching && !selected.is_empty() && group_delta == (0, 0) {
                let (x, y) = ui.input_mut(|input| {
                    let key = egui::Modifiers::NONE;
                    let x = i64::from(input.consume_key(key, egui::Key::ArrowRight))
                        - i64::from(input.consume_key(key, egui::Key::ArrowLeft));
                    let y = i32::from(input.consume_key(key, egui::Key::ArrowUp))
                        - i32::from(input.consume_key(key, egui::Key::ArrowDown));
                    (x, y)
                });
                if (x, y) != (0, 0) {
                    group_delta = (x, y);
                    end_group_move = true;
                }
            }
            if group_delta != (0, 0) {
                let mut min_start = editable_end;
                let mut max_end = clip_start;
                let mut min_pitch = high_pitch;
                let mut max_pitch = 0;
                for index in &selected {
                    let note = notes[*index];
                    min_start = min_start.min(note.start);
                    max_end = max_end.max(note.start + note.len);
                    min_pitch = min_pitch.min(note.pitch);
                    max_pitch = max_pitch.max(note.pitch);
                }
                let step = i64::from(snap);
                let left = i64::from(min_start - clip_start) / step;
                let right = i64::from(editable_end - max_end) / step;
                group_delta = (group_delta.0.clamp(-left, right) * step,
                    group_delta.1.clamp(-i32::from(min_pitch), i32::from(high_pitch - max_pitch)));
            }
            let commit_group = end_group_move && group_delta != (0, 0);
            let tap_id = resp.id.with("last_piano_note_tap");
            let mut remove = None;
            let mut toggle_slide = None;
            let mut audition = None;
            let mut edit = None;
            // ghost notes: the other MIDI clips that sound at the same time (any track), faded and not editable
            if let Some(this) = clip {
                for (other_index, other) in self.project.arrangement.clips.iter().enumerate() {
                    if Some(other_index) == clip_index || other.audio || other.pattern == pi {
                        continue;
                    }
                    let Some(pattern) = self.project.patterns.get(other.pattern) else { continue };
                    for ghost in &pattern.notes {
                        if ghost.start < other.offset || ghost.start >= other.offset.saturating_add(other.len) || ghost.pitch > high_pitch {
                            continue;
                        }
                        let song = other.start + (ghost.start - other.offset);
                        if song < this.start || song >= this.start.saturating_add(this.len) {
                            continue;
                        }
                        let local = song - this.start;
                        let x = rect.left() + local as f32 / TICKS_PER_STEP as f32 * step_w;
                        let y = rect.top() + (high_pitch - ghost.pitch) as f32 * row_h;
                        let w = (ghost.len.min(this.len - local) as f32 / TICKS_PER_STEP as f32 * step_w).max(4.0);
                        let r = Rect::from_min_size(pos2(x + 1.0, y + 2.0), vec2(w - 2.0, row_h - 4.0));
                        p.rect_filled(r, 4.0, muted().gamma_multiply(0.28));
                        p.rect_stroke(r, 4.0, Stroke::new(1.0, lane_col(other.lane).gamma_multiply(0.35)));
                    }
                }
            }
            for (index, note) in self.project.patterns[pi].notes.iter().enumerate() {
                if note.pitch > high_pitch || note.start >= editable_end || note.start.saturating_add(note.len) <= clip_start {
                    continue;
                }
                let original = body(note.len, note.pitch, note.start);
                let id = resp.id.with(("note", index));
                let note_response = ui.interact(original.expand(1.0), id, if selecting || pinching { Sense::hover() } else { Sense::click_and_drag() })
                    .on_hover_cursor(egui::CursorIcon::Grab);
                let edge = Rect::from_min_max(
                    pos2(original.right() - (original.width() * 0.35).min(13.0), original.top()),
                    original.right_bottom());
                let handle = ui.interact(edge, id.with("length"), if selecting || pinching { Sense::hover() } else { Sense::click_and_drag() })
                    .on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
                let max_len = editable_end - note.start;
                let mut len = note.len;
                let mut pitch = note.pitch;
                let mut start = note.start;
                let grab_id = id.with("grab");
                if pinching {
                    ui.data_mut(|d| d.remove::<(Pos2, bool)>(grab_id));
                } else if handle.drag_started() || note_response.drag_started() {
                    ui.data_mut(|d| d.remove::<(usize, u32, u8, f64)>(tap_id));
                    if let (Some(origin), Some(pointer)) = (
                        ui.input(|input| input.pointer.press_origin()),
                        handle.interact_pointer_pos().or(note_response.interact_pointer_pos()),
                    ) {
                        let delta = pointer - origin;
                        let resize = handle.drag_started() && delta.x.abs() >= delta.y.abs();
                        ui.data_mut(|d| d.insert_temp(grab_id, (origin, resize)));
                    }
                }
                let stopped = handle.drag_stopped() || note_response.drag_stopped();
                if !pinching && (handle.dragged() || note_response.dragged() || stopped) {
                    if let (Some((origin, resize)), Some(pointer)) = (
                        ui.data(|d| d.get_temp::<(egui::Pos2, bool)>(grab_id)),
                        handle.interact_pointer_pos().or(note_response.interact_pointer_pos()),
                    ) {
                        let ticks = (pointer.x - origin.x) / step_w * TICKS_PER_STEP as f32;
                        let snapped = (ticks / snap as f32).round() as i64 * i64::from(snap);
                        if resize {
                            len = (i64::from(note.len) + snapped)
                                .clamp(i64::from(snap.min(max_len)), i64::from(max_len)) as u32;
                        } else {
                            // the body moves both ways: up/down = pitch, left/right = time (snapped to the grid)
                            let semitones = ((origin.y - pointer.y) / row_h).round() as i32;
                            pitch = (i32::from(note.pitch) + semitones).clamp(0, i32::from(high_pitch)) as u8;
                            let latest = i64::from(editable_end.saturating_sub(note.len).max(clip_start));
                            if snapped != 0 {
                                start = (i64::from(note.start) + snapped).clamp(i64::from(clip_start), latest) as u32;
                            }
                        }
                    }
                    if stopped {
                        ui.data_mut(|d| d.remove::<(egui::Pos2, bool)>(grab_id));
                        if len != note.len || pitch != note.pitch || start != note.start {
                            edit = Some((index, len, pitch, start));
                        }
                    }
                }
                let highlighted = selecting && selected.contains(&index);
                if highlighted {
                    start = (i64::from(start) + group_delta.0) as u32;
                    pitch = (i32::from(pitch) + group_delta.1) as u8;
                }
                let visual = body(len, pitch, start);
                p.rect_filled(visual, 5.0, if highlighted { mobile_color(0xFFE7ECFF, 0xFFDCEAFF, 0xFFD8D0FF, 0xFFE0DBCF, 0xFFD8FBA1) } else { piano_note_fill() });
                if highlighted {
                    p.rect_stroke(visual, 5.0, Stroke::new(1.4, mobile_color(0xFFA79CFF, 0xFF89B3FF, 0xFF5B4BD6, 0xFFD9A066, 0xFFA8D861)));
                }
                if note.slide { piano_slide_mark(&p, visual); }
                let grip = Rect::from_center_size(visual.right_center() - vec2(3.0, 0.0), vec2(2.0, 10.0));
                p.rect_filled(grip, 1.0, Color32::WHITE.linear_multiply(0.8));
                if visual.width() >= 16.0 && visual.height() >= 10.0 {
                    let size = (visual.height() * 0.55).clamp(9.0, 13.0);
                    p.with_clip_rect(visual.intersect(ui.clip_rect())).text(
                        visual.left_center() + vec2(3.0, 0.0), Align2::LEFT_CENTER,
                        format!("{}{}", names[usize::from(pitch % 12)], i16::from(pitch) / 12 - 1),
                        FontId::proportional(size), Color32::from_gray(18));
                }
                if !pinching && (note_response.secondary_clicked() || handle.secondary_clicked()
                    || note_response.long_touched() || handle.long_touched()) {
                    remove = Some(index);
                } else if !pinching && (note_response.clicked() || handle.clicked()) {
                    if slide_mode {
                        toggle_slide = Some(index);
                        continue;
                    }
                    let repeat = ui.data(|d| d.get_temp::<(usize, u32, u8, f64)>(tap_id))
                        .is_some_and(|(last_index, start, pitch, time)| {
                            (last_index, start, pitch) == (index, note.start, note.pitch)
                                && (0.0..=0.42).contains(&(now - time))
                        });
                    if repeat || note_response.double_clicked() || handle.double_clicked() {
                        select_all = true;
                    } else {
                        ui.data_mut(|d| d.insert_temp(tap_id, (index, note.start, note.pitch, now)));
                        audition = Some((note.track, note.pitch, note.vel));
                    }
                }
            }
            if pinching || selecting || select_all {
                pending = None;
            } else if resp.clicked() {
                if let Some(pt) = resp.interact_pointer_pos() {
                    let row = (((pt.y - rect.top()) / row_h) as usize).min(rows - 1);
                    let pitch = high_pitch - row as u8;
                    let raw = ((pt.x - rect.left()) / pixels_per_tick).max(0.0) as u32;
                    let tick = ((clip_start + raw.min(duration - 1)) / snap * snap).max(clip_start);
                    if tick < editable_end {
                        if slide_mode {
                            place_note = Some((tick, pitch));
                        } else {
                            place_note = pending.map(|(_, tick, pitch)| (tick, pitch));
                            pending = Some((now, tick, pitch));
                        }
                    }
                }
            } else if let Some((at, tick, pitch)) = pending {
                if now - at >= tap_wait.as_secs_f64() {
                    place_note = Some((tick, pitch));
                    pending = None;
                }
            }
            if let Some(tap) = pending {
                ui.data_mut(|d| d.insert_temp(pending_id, tap));
                ui.ctx().request_repaint_after(tap_wait.saturating_sub(
                    std::time::Duration::from_secs_f64((now - tap.0).max(0.0))));
            }
            if select_all {
                selecting = true;
                let all: Vec<_> = notes.iter().enumerate()
                    .filter(|(_, note)| editable(note)).map(|(index, _)| index).collect();
                ui.data_mut(|d| {
                    d.insert_temp(copy_id, true);
                    d.insert_temp(selected_id, all);
                    d.remove::<(usize, u32, u8, f64)>(tap_id);
                });
                ui.ctx().request_repaint();
            } else if commit_group {
                self.m_playlist_undo_step();
                let pattern = &mut self.project.patterns[pi];
                for index in selected {
                    let note = &mut pattern.notes[index];
                    note.start = (i64::from(note.start) + group_delta.0) as u32;
                    note.pitch = (i32::from(note.pitch) + group_delta.1) as u8;
                    pattern.length_steps = pattern.length_steps.max((note.start + note.len).div_ceil(TICKS_PER_STEP));
                }
            } else if pinching || selecting {
                ui.data_mut(|d| d.remove::<(usize, u32, u8, f64)>(tap_id));
            } else if let Some((index, len, pitch, start)) = edit {
                self.m_playlist_undo_step();
                let pattern = &mut self.project.patterns[pi];
                pattern.length_steps = pattern.length_steps.max(
                    start.saturating_add(len).min(editable_end).div_ceil(TICKS_PER_STEP),
                );
                let note = &mut pattern.notes[index];
                let moved = note.pitch != pitch;
                let track = note.track;
                let velocity = note.vel;
                note.len = len;
                note.pitch = pitch;
                note.start = start;
                if moved {
                    self.m_preview_piano_notes(ui.ctx(), track, vec![pitch], velocity);
                }
            } else if let Some(index) = toggle_slide {
                self.m_playlist_undo_step();
                let note = &mut self.project.patterns[pi].notes[index];
                note.slide = !note.slide;
                ui.data_mut(|d| d.remove::<(usize, u32, u8, f64)>(tap_id));
            } else if let Some(index) = remove {
                ui.data_mut(|d| d.remove::<(usize, u32, u8, f64)>(tap_id));
                self.m_playlist_undo_step();
                self.project.patterns[pi].notes.remove(index);
            } else if let Some((track, pitch, velocity)) = audition {
                self.m_preview_piano_notes(ui.ctx(), track, vec![pitch], velocity);
            } else if let Some((tick, pitch)) = place_note.filter(|(tick, _)| (clip_start..editable_end).contains(tick)) {
                ui.data_mut(|d| d.remove::<(usize, u32, u8, f64)>(tap_id));
                let mut pitches = if chords {
                    let tone = crate::theory::snap_to_scale(key_root, key_scale, pitch);
                    crate::theory::stack_chord(key_root, key_scale, tone, 3)
                } else {
                    vec![pitch]
                };
                let len = (if chords { snap.max(TICKS_PER_STEP) * 4 } else { snap }).min(editable_end - tick);
                let notes = &mut self.project.patterns[pi].notes;
                pitches.retain(|&pitch| {
                    if notes.iter().any(|note| note.pitch == pitch
                        && note.start <= tick && tick < note.start.saturating_add(note.len))
                    {
                        return false;
                    }
                    let len = notes.iter().filter(|note| note.pitch == pitch && note.start > tick)
                        .fold(len, |len, note| len.min(note.start - tick));
                    notes.push(crate::piano_roll::Note { start: tick, len, pitch, vel: 100, track, slide: slide_mode, chance: 100, pan: 0, mute: false });
                    true
                });
                if !pitches.is_empty() {
                    let pattern = &mut self.project.patterns[pi];
                    pattern.length_steps = pattern.length_steps.max((tick + len).div_ceil(TICKS_PER_STEP));
                    self.m_playlist_undo_step();
                    self.m_preview_piano_notes(ui.ctx(), track, pitches, 100);
                }
            }
            rect.min
        });
        ui.data_mut(|d| d.insert_temp(offset_id, out.state.offset));
        // keys down the left side, one per row of the grid (like the PC piano roll): press one to hear it (and record it)
        let ruler = Rect::from_min_max(pos2(viewport.left(), grid.top()), pos2(grid.left() - 2.0, grid.bottom()));
        let side = ui.interact(ruler, ui.id().with(("m_piano_side_keys", pi)), Sense::click_and_drag());
        let side_pressed = if !pinching && side.is_pointer_button_down_on() {
            side.interact_pointer_pos().filter(|pt| ruler.contains(*pt)).map(|pt| {
                let row = (((pt.y - out.inner.y) / row_h).max(0.0) as usize).min(rows - 1);
                high_pitch - row as u8
            })
        } else {
            None
        };
        let painter = ui.painter_at(ruler);
        painter.rect_filled(ruler, 5.0, mobile_color(0xFFD6D2EE, 0xFFD3D9E0, 0xFFE6E1F8, 0xFFD5D2CC, 0xFFD2DCC8));
        for row in 0..rows {
            let pitch = high_pitch - row as u8;
            let y = out.inner.y + row as f32 * row_h;
            if y + row_h <= ruler.top() || y >= ruler.bottom() {
                continue;
            }
            let lane = Rect::from_min_size(pos2(ruler.left(), y), vec2(ruler.width(), row_h));
            let black = matches!(pitch % 12, 1 | 3 | 6 | 8 | 10);
            let down = side_pressed == Some(pitch) || held.is_some_and(|(_, key)| key == pitch);
            if black {
                let key = Rect::from_min_size(lane.min, vec2(lane.width() * 0.62, row_h));
                painter.rect_filled(key.shrink2(vec2(0.0, 1.0)), 2.0, if down { lane_col(3) } else { mobile_color(0xFF241F38, 0xFF242B38, 0xFF4A4170, 0xFF2A2927, 0xFF1F2A1A) });
            } else {
                if down {
                    painter.rect_filled(lane.shrink(0.5), 2.0, lane_col(3));
                }
                if matches!(pitch % 12, 0 | 5) {
                    painter.line_segment([lane.left_bottom(), lane.right_bottom()], Stroke::new(1.0, Color32::from_gray(150)));
                }
            }
            if pitch % 12 == 0 || !black {
                let label = format!("{}{}", names[usize::from(pitch % 12)], i16::from(pitch) / 12 - 1);
                painter.text(pos2(lane.right() - 4.0, lane.center().y), Align2::RIGHT_CENTER, label,
                    FontId::proportional(if pitch % 12 == 0 { 10.5 } else { 9.0 }),
                    if pitch % 12 == 0 { Color32::from_gray(25) } else { Color32::from_gray(110) });
            }
        }
        let timeline = Rect::from_min_max(pos2(grid.left(), viewport.top()), pos2(grid.right(), grid.top()));
        let painter = ui.painter_at(timeline);
        painter.rect_filled(timeline, 3.0, bar_fill());
        let beat = TICKS_PER_BAR / 4;
        let ruler_hit = ui.interact(timeline, range_id.with("ruler"), Sense::click_and_drag())
            .on_hover_text(if selecting { "Tap or drag to select beats" } else { "Tap to seek · Drag to loop" });
        let tick_at = |x: f32| {
            let x = x.clamp(timeline.left(), timeline.right());
            let ticks = ((x - out.inner.x).max(0.0) / step_w * TICKS_PER_STEP as f32) as u32;
            clip_start + ticks.min(duration - 1)
        };
        if selecting {
            if !pinching && (ruler_hit.secondary_clicked() || ruler_hit.long_touched()) {
                selection = None;
            } else if !pinching && (ruler_hit.clicked() || ruler_hit.dragged()) {
                if let Some(point) = ruler_hit.interact_pointer_pos() {
                    let origin = if ruler_hit.clicked() { point } else {
                        ui.input(|input| input.pointer.press_origin()).unwrap_or(point)
                    };
                    let beat_at = |x| (tick_at(x) / beat * beat).max(clip_start);
                    let (a, b) = (beat_at(origin.x), beat_at(point.x));
                    selection = Some((a.min(b), (a.max(b) + beat).min(pattern_end)));
                }
            }
            ui.data_mut(|d| {
                if let Some(range) = selection {
                    d.insert_temp(copy_id.with("range"), range);
                } else {
                    d.remove::<(u32, u32)>(copy_id.with("range"));
                }
            });
        } else if !pinching && ruler_hit.dragged() {
            if let Some((origin, point)) = ui.input(|input| input.pointer.press_origin()).zip(ruler_hit.interact_pointer_pos()) {
                let a = (tick_at(origin.x) / beat * beat).max(clip_start);
                let b = (tick_at(point.x) / beat * beat).max(clip_start);
                let range = (a.min(b), (a.max(b) + beat).min(pattern_end));
                if loop_range != Some(range) {
                    loop_range = Some(range);
                    ui.data_mut(|d| d.insert_temp(loop_id, true));
                    self.shared.send(Cmd::PatternLoop(true));
                    self.shared.send(Cmd::PatternRange(Some((pi, range.0, range.1))));
                }
            }
        } else if !pinching && (ruler_hit.secondary_clicked() || ruler_hit.long_touched()) {
            loop_range = None;
            self.shared.send(Cmd::PatternRange(clip.map(|_| (pi, clip_start, pattern_end))));
        } else if !pinching && (ruler_hit.clicked() || ruler_hit.drag_stopped()) {
            let target = if ruler_hit.drag_stopped() {
                loop_range.map(|(start, _)| start)
            } else {
                ruler_hit.interact_pointer_pos().map(|point| tick_at(point.x))
            };
            if let Some(tick) = target {
                self.project.song_mode = false;
                self.project.current_pattern = pi;
                self.publish_edits();
                if !playing {
                    ui.data_mut(|d| d.insert_temp(egui::Id::new(Self::M_PAUSED_AT), (false, pi)));
                }
                self.shared.send(Cmd::Seek(tick));
            }
        }
        let ruler_range = if selecting { selection } else { loop_range };
        if let Some((start, end)) = ruler_range {
            let x = |tick: u32| out.inner.x + tick.saturating_sub(clip_start) as f32 / TICKS_PER_STEP as f32 * step_w;
            let mark = Rect::from_min_max(pos2(x(start), timeline.top()), pos2(x(end), timeline.bottom())).intersect(timeline);
            if mark.is_positive() {
                let color = if selecting { piano_note_fill() } else { p_accent() };
                painter.rect_filled(mark, 3.0, color.gamma_multiply(0.32));
                painter.line_segment([mark.left_bottom(), mark.right_bottom()], Stroke::new(2.0, color));
            }
        }
        ui.data_mut(|d| {
            let id = range_id;
            if let Some(range) = loop_range {
                d.insert_temp(id, range);
            } else {
                d.remove::<(u32, u32)>(id);
            }
        });
        let first_step = (clip_start / TICKS_PER_STEP) as usize;
        for step in (first_step.div_ceil(4) * 4..first_step + steps).step_by(4) {
            let x = out.inner.x + (step - first_step) as f32 * step_w;
            if x + 36.0 <= timeline.left() || x >= timeline.right() {
                continue;
            }
            painter.text(pos2(x + 4.0, timeline.center().y), Align2::LEFT_CENTER,
                format!("{}.{}", step / 16 + 1, (step % 16) / 4 + 1), FontId::proportional(11.0),
                if step % 16 == 0 { ink() } else { muted() });
        }
        if show_velocity {
            // splitter handle: drag up / down to resize the event editor
            let (bar, split) = ui.allocate_exact_size(vec2(ui.available_width(), 12.0), Sense::drag());
            ui.painter().rect_filled(Rect::from_center_size(bar.center(), vec2(44.0, 4.0)), 2.0,
                if split.dragged() { p_accent() } else { muted() });
            if split.dragged() {
                let h = (velocity_h - split.drag_delta().y).clamp(44.0, 320.0);
                ui.data_mut(|d| d.insert_temp(event_h_id, h));
            }
            self.m_piano_velocity(ui, pi, out.inner.x, grid.left(), step_w, velocity_h - 8.0, clip_start..pattern_end);
        }
        let pressed = side_pressed.map(|pitch| (track, pitch));
        if pressed != held {
            if let Some((track, pitch)) = held {
                self.shared.send(Cmd::NoteOff { track, pitch });
            }
            if let Some((track, pitch)) = pressed {
                self.m_stop_note_preview(ui.ctx());
                self.shared.send(Cmd::SetLiveLane(self.arr.sel_lane));
                self.shared.send(Cmd::NoteOn { track, pitch, vel: 100 });
                if let Some(step) = record_step {
                    let step = if playing { play_step } else { step % steps };
                    let start = clip_start + step as u32 * TICKS_PER_STEP;
                    let len = TICKS_PER_STEP.min(editable_end.saturating_sub(start));
                    let pattern = &mut self.project.patterns[pi];
                    let notes = &mut pattern.notes;
                    if len > 0 && !notes.iter().any(|note| note.pitch == pitch && note.start < start + len
                        && start < note.start.saturating_add(note.len))
                    {
                        notes.push(crate::piano_roll::Note { start, len, pitch, vel: 100, track, slide: false, chance: 100, pan: 0, mute: false });
                        pattern.length_steps = pattern.length_steps.max((start + len).div_ceil(TICKS_PER_STEP));
                    }
                    record_step = Some(if playing { step } else { (step + 1) % steps });
                }
            }
            held = pressed;
        }
        ui.data_mut(|d| {
            if let Some(step) = record_step {
                d.insert_temp(record_id, step);
            } else {
                d.remove::<usize>(record_id);
            }
            if let Some(held) = held {
                d.insert_temp(held_id, held);
            } else {
                d.remove::<(Track, u8)>(held_id);
            }
        });
        close
    }

    /// FL-style event editor under the Piano Roll: one bar per note for the chosen target (tap the label on the left
    /// to switch Velocity → Pan → Chance). Pan is drawn from the middle line.
    fn m_piano_velocity(&mut self, ui: &mut Ui, pattern: usize, origin_x: f32, grid_left: f32, step_w: f32, height: f32, ticks: std::ops::Range<u32>) {
        let target_id = egui::Id::new("m_piano_event_target");
        let mut target = ui.data(|d| d.get_temp::<u8>(target_id)).unwrap_or(0) % 3;
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
        let ruler = Rect::from_min_max(rect.min, pos2(grid_left, rect.bottom()));
        let painter = ui.painter_at(ruler);
        painter.rect_filled(ruler, 3.0, bar_fill());
        let name = ["Vel", "Pan", "Chance"][target as usize];
        painter.text(ruler.center() - vec2(0.0, 8.0), Align2::CENTER_CENTER, name, FontId::proportional(12.0), ink());
        painter.text(ruler.center() + vec2(0.0, 9.0), Align2::CENTER_CENTER, "›", FontId::proportional(14.0), muted());
        if ui.interact(ruler, ui.id().with("piano_event_target"), Sense::click()).on_hover_text("Switch: Velocity / Pan / Chance").clicked() {
            target = (target + 1) % 3;
            ui.data_mut(|d| d.insert_temp(target_id, target));
        }
        let plot = Rect::from_min_max(pos2(grid_left, rect.top()), rect.max);
        let painter = ui.painter_at(plot);
        painter.rect_filled(plot, 3.0, p_bg());
        let range = plot.shrink2(vec2(0.0, 4.0));
        for t in [0.25f32, 0.5, 0.75] {
            let y = range.bottom() - t * range.height();
            painter.hline(plot.x_range(), y, Stroke::new(if target == 1 && t == 0.5 { 1.0 } else { 0.5 }, grid_line()));
        }
        // note value <-> 0..1
        let get = |note: &crate::piano_roll::Note| match target {
            0 => f32::from(note.vel) / 127.0,
            1 => (f32::from(note.pan) + 100.0) / 200.0,
            _ => f32::from(note.chance) / 100.0,
        };
        let notes = &mut self.project.patterns[pattern].notes;
        let mut edited = false;
        let mut columns = std::collections::BTreeMap::<u32, Vec<usize>>::new();
        for (index, note) in notes.iter().enumerate() {
            if ticks.contains(&note.start) {
                columns.entry((note.start - ticks.start) / TICKS_PER_STEP).or_default().push(index);
            }
        }
        for (step, indices) in columns {
            let left = origin_x + step as f32 * step_w;
            if left + step_w <= plot.left() || left >= plot.right() {
                continue;
            }
            let width = step_w / indices.len() as f32;
            for (offset, index) in indices.into_iter().enumerate() {
                let note = &mut notes[index];
                let x = left + offset as f32 * width;
                let hit = Rect::from_min_size(pos2(x, plot.top()), vec2(width, plot.height())).intersect(plot);
                if !hit.is_positive() {
                    continue;
                }
                let response = ui.interact(hit, ui.id().with(("piano_event", pattern, index)), Sense::click_and_drag())
                    .on_hover_cursor(egui::CursorIcon::ResizeVertical);
                let mut value = get(note);
                if response.dragged() || response.drag_stopped() || response.clicked() {
                    if let Some(point) = response.interact_pointer_pos() {
                        value = ((range.bottom() - point.y) / range.height()).clamp(0.0, 1.0);
                        if response.drag_stopped() || response.clicked() {
                            let before = (note.vel, note.pan, note.chance);
                            match target {
                                0 => note.vel = (value * 127.0).round().clamp(1.0, 127.0) as u8,
                                1 => note.pan = (value * 200.0 - 100.0).round().clamp(-100.0, 100.0) as i8,
                                _ => note.chance = (value * 100.0).round().clamp(1.0, 100.0) as u8,
                            }
                            edited |= before != (note.vel, note.pan, note.chance);
                        }
                    }
                }
                if response.double_clicked() {
                    match target { 0 => note.vel = 100, 1 => note.pan = 0, _ => note.chance = 100 }
                    edited = true;
                }
                let y = range.bottom() - value * range.height();
                let base = if target == 1 { range.center().y } else { range.bottom() };
                let inset = (width * 0.16).min(2.0);
                let bar = Rect::from_min_max(pos2(x + inset, y.min(base)), pos2(x + width - inset, y.max(base) + 1.0));
                let color = if response.hovered() || response.dragged() { p_accent() } else { piano_note_fill() };
                painter.rect_filled(bar, 2.0, color);
                painter.circle_filled(pos2(x + width * 0.5, y), 3.0, color);
                let label = match target { 0 => format!("Velocity {}", note.vel), 1 => format!("Pan {}", note.pan), _ => format!("Chance {}%", note.chance) };
                response.on_hover_text(label);
            }
        }
        if edited {
            self.m_playlist_undo_step();
        }
    }

    fn m_preview_piano_notes(&self, ctx: &egui::Context, track: Track, pitches: Vec<u8>, vel: u8) {
        self.m_stop_note_preview(ctx);
        let lane = self.arr.sel_lane;
        self.shared.send(Cmd::SetLiveLane(lane));
        for &pitch in &pitches {
            self.shared.send(Cmd::NoteOn { track, pitch, vel });
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

    fn m_piano_length_limit(&self, clip: Option<crate::arrangement::Clip>) -> u32 {
        let offset = clip.map_or(0, |clip| clip.offset);
        let max_ticks = crate::sequencer::MAX_STEPS as u32 * TICKS_PER_STEP;
        let mut limit = max_ticks.saturating_sub(offset);
        if let Some(clip) = clip {
            if let Some(next) = self.project.arrangement.clips.iter()
                .filter(|other| other.lane == clip.lane && other.start >= clip.end_tick())
                .map(|other| other.start).min()
            {
                limit = limit.min(next - clip.start);
            }
        }
        limit
    }

    fn m_piano_pattern_shared(&self, pattern: usize, index: usize) -> bool {
        !self.project.patterns[pattern].clip_only || self.project.arrangement.clips.iter().enumerate()
            .any(|(other, clip)| other != index && !clip.audio && clip.pattern == pattern)
            || self.project.scenes.iter().enumerate().any(|(scene, entry)| {
                scene != self.project.active_scene && entry.arrangement.clips.iter()
                    .any(|clip| !clip.audio && clip.pattern == pattern)
            })
    }

    fn m_copy_piano_beats(
        &mut self, pattern: usize, clip_index: Option<usize>, range: (u32, u32),
    ) -> Result<(u32, u32), &'static str> {
        let source = &self.project.patterns[pattern];
        let clip = clip_index.map(|index| self.project.arrangement.clips[index]);
        let offset = clip.map_or(0, |clip| clip.offset);
        let duration = clip.map_or(source.len_ticks(), |clip| clip.len);
        let (start, end) = range;
        if start < offset || start >= end || end > offset.saturating_add(duration) {
            return Err("Select beats inside the current clip.");
        }

        let beat = TICKS_PER_BAR / 4;
        let shift = (end - start).div_ceil(beat) * beat;
        let mut notes = Vec::new();
        for note in source.notes.iter().filter(|note| (start..end).contains(&note.start)) {
            let target = note.start + shift;
            // Keep a note within the audible portion of a trimmed source clip.
            let len = note.len.min(offset.saturating_add(duration) - note.start);
            notes.push(crate::piano_roll::Note { start: target, len, ..*note });
        }
        let copy_end = notes.iter().map(|note| note.start + note.len).max()
            .ok_or("No notes in the selected beats.")?;
        let next_end = (end + shift).max(copy_end);
        if next_end - offset > self.m_piano_length_limit(clip) {
            return Err("No room to copy here. Move the next clip first.");
        }

        let edit_pattern = if let Some(index) = clip_index.filter(|&index| self.m_piano_pattern_shared(pattern, index)) {
            let mut copy = source.clone();
            copy.clip_only = true;
            let next = self.project.patterns.len();
            self.project.patterns.push(copy);
            self.project.arrangement.clips[index].pattern = next;
            next
        } else {
            pattern
        };
        let edited = &mut self.project.patterns[edit_pattern];
        edited.notes.extend(notes);
        edited.length_steps = edited.length_steps.max(next_end.div_ceil(TICKS_PER_STEP));
        if let Some(index) = clip_index {
            let clip = &mut self.project.arrangement.clips[index];
            clip.len = clip.len.max(next_end - offset);
        }
        self.project.current_pattern = edit_pattern;
        self.m_playlist_undo_step();
        Ok((start + shift, end + shift))
    }

    fn m_extend_piano(&mut self, pattern: usize, clip_index: Option<usize>) -> Result<(), &'static str> {
        let source = &self.project.patterns[pattern];
        let clip = clip_index.map(|index| self.project.arrangement.clips[index]);
        let offset = clip.map_or(0, |clip| clip.offset);
        let current = clip.map_or(source.len_ticks(), |clip| clip.len);
        let limit = self.m_piano_length_limit(clip);
        let next = current.saturating_add(TICKS_PER_BAR / 4).min(limit);
        if next <= current {
            return Err("No room to extend here. Move the next clip or shorten this one.");
        }
        let needed = offset.saturating_add(next).div_ceil(TICKS_PER_STEP);
        let mut edit_pattern = pattern;
        if let Some(index) = clip_index {
            if self.m_piano_pattern_shared(pattern, index) && needed > source.length_steps {
                let mut copy = source.clone();
                copy.clip_only = true;
                edit_pattern = self.project.patterns.len();
                self.project.patterns.push(copy);
            }
            let clip = &mut self.project.arrangement.clips[index];
            clip.pattern = edit_pattern;
            clip.len = next;
        }
        let edited = &mut self.project.patterns[edit_pattern];
        edited.length_steps = edited.length_steps.max(needed);
        self.project.current_pattern = edit_pattern;
        self.m_playlist_undo_step();
        Ok(())
    }

    fn m_clip_piano_open(&self, ctx: &egui::Context) -> bool {
        ctx.data(|d| d.get_temp::<usize>(egui::Id::new("m_clip_sheet_selection")))
            .filter(|&index| self.arr.sel_clip == Some(index))
            .and_then(|index| self.project.arrangement.clips.get(index))
            .is_some_and(|clip| !clip.audio)
    }

    fn m_clip_sheet(&mut self, ctx: &egui::Context) {
        let id = egui::Id::new("m_clip_sheet_selection");
        let animation = egui::Id::new("m_piano_drawer_animation");
        let full = egui::Id::new("m_piano_drawer_full");
        let index = ctx.data(|d| d.get_temp::<usize>(id)).filter(|&i| self.arr.sel_clip == Some(i));
        let settings = egui::Id::new("m_midi_clip_settings");
        let midi_settings = index.is_some() && ctx.data(|d| d.get_temp::<usize>(settings)) == index;
        let clip = index.and_then(|i| self.project.arrangement.clips.get(i).copied());
        let (Some(index), Some(clip)) = (index, clip) else {
            ctx.animate_bool(animation, false);
            ctx.data_mut(|d| {
                d.remove::<usize>(id);
                d.remove::<bool>(full);
                d.remove::<usize>(settings);
            });
            return;
        };
        let done = if clip.audio || midi_settings {
            ctx.animate_bool(animation, false);
            let title = if clip.audio { "Audio clip settings" } else { "MIDI clip settings" };
            let controls = |ui: &mut Ui| {
                mobile_scroll(egui::ScrollArea::vertical()).id_salt(("m_clip_controls", index))
                    .auto_shrink([false, false]).show(ui, |ui| {
                        ui.spacing_mut().interact_size.y = 38.0;
                        ui.spacing_mut().slider_width = (ui.available_width() * 0.45).min(220.0);
                        arrangement_ui::clip_options(ui, &mut self.project, &mut self.arr, &mut self.lib, index, clip.start, true);
                        if clip.audio {
                            if let Some(sample) = self.project.arrangement.clips.get(index)
                                .filter(|_| self.arr.sel_clip == Some(index))
                                .and_then(|clip| self.project.audio_pool.get(clip.pattern))
                            {
                                ui.separator();
                                track_ui::sample_waveform(ui, sample, self.project.bpm, &self.shared);
                            }
                        }
                    });
            };
            let closed = if clip.audio && ctx.screen_rect().width() < ctx.screen_rect().height() {
                mobile_side_editor(ctx, egui::Id::new("m_audio_clip_drawer"), title, egui::Align::Min, 0.90, controls)
            } else {
                mobile_editor_sheet(ctx, "m_clip_sheet_window", title, controls)
            };
            if std::mem::take(&mut self.arr.open_detail) {
                ctx.data_mut(|d| d.remove::<usize>(settings));
                ctx.request_repaint();
                false
            } else if std::mem::take(&mut self.arr.open_editor) {
                ctx.data_mut(|d| d.insert_temp(egui::Id::new(devices_ui::INSTRUMENT_EDITOR), clip.lane));
                true
            } else {
                closed
            }
        } else {
            self.m_piano_drawer(ctx, clip.pattern, animation, full)
        };
        if done {
            ctx.data_mut(|d| {
                d.remove::<usize>(id);
                d.remove::<bool>(full);
                d.remove::<usize>(settings);
            });
            ctx.request_repaint();
        }
    }

    fn m_piano_drawer(&mut self, ctx: &egui::Context, pattern: usize, animation: egui::Id, full_id: egui::Id) -> bool {
        let shown = ctx.animate_bool(animation, true);
        let full = ctx.data(|d| d.get_temp::<bool>(full_id)).unwrap_or(false);
        let screen = ctx.screen_rect();
        let portrait = screen.width() < screen.height();
        let landscape_sheet = !portrait && !full;
        let width = screen.width().max(1.0);
        let height = if landscape_sheet { screen.height() * 0.82 } else { screen.height() }.max(1.0);
        let open_at = if landscape_sheet {
            pos2(screen.left(), screen.bottom() - shown * height)
        } else if portrait {
            pos2(screen.left() - (1.0 - shown) * width, screen.top())
        } else {
            screen.min
        };
        let mut close = false;
        if landscape_sheet {
            egui::Area::new(animation.with("scrim")).order(egui::Order::Middle)
                .fixed_pos(screen.min).movable(false).show(ctx, |ui| {
                    let (rect, hit) = ui.allocate_exact_size(screen.size(), Sense::click());
                    ui.painter().rect_filled(rect, 0.0, Color32::from_black_alpha((105.0 * shown) as u8));
                    close |= hit.clicked();
                });
        }
        egui::Area::new(egui::Id::new("m_piano_drawer"))
            .fixed_pos(open_at).constrain(false).movable(false).order(egui::Order::Foreground)
            .show(ctx, |ui| {
                let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::click());
                let mut content = rect.shrink2(vec2(9.0, 7.0));
                if landscape_sheet {
                    let rounding = egui::Rounding { nw: 16.0, ne: 16.0, ..Default::default() };
                    ui.painter().rect_filled(rect, rounding, p_bg());
                    ui.painter().rect_stroke(rect, rounding, Stroke::new(1.0, outline()));
                    let grab = Rect::from_center_size(pos2(rect.center().x, rect.top() + 9.0), vec2(70.0, 18.0));
                    let hit = ui.interact(grab, animation.with("handle"), Sense::drag());
                    let handle = Rect::from_center_size(grab.center(), vec2(42.0, 4.0));
                    ui.painter().rect_filled(handle, 2.0, muted());
                    if hit.dragged() && ui.input(|input| input.pointer.press_origin()
                        .zip(input.pointer.interact_pos()).is_some_and(|(start, end)| end.y - start.y > 37.0))
                    {
                        close = true;
                    }
                    content.min.y += 11.0;
                } else {
                    ui.painter().rect_filled(rect, 0.0, p_bg());
                    if !full {
                        ui.painter().line_segment([rect.right_top(), rect.right_bottom()], Stroke::new(1.0, outline()));
                    }
                }
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(content));
                child.set_clip_rect(rect.intersect(screen));
                mobile_visuals(&mut child);
                close |= self.m_piano_editor(&mut child, pattern, Some((full_id, full)));
            });
        close || ctx.input(|input| input.key_pressed(egui::Key::Escape))
    }

    fn m_drum_sound_sheet(&mut self, ctx: &egui::Context) {
        let id = egui::Id::new("m_drum_sound_sheet");
        let picker = id.with("picker");
        let lane = ctx.data(|d| d.get_temp::<usize>(id));
        let Some(lane) = lane.filter(|&lane| self.m_sec == 1 && lane < NUM_DRUMS) else {
            if lane.is_some() {
                ctx.data_mut(|d| { d.remove::<usize>(id); d.remove::<bool>(picker); });
            }
            return;
        };
        let screen = ctx.screen_rect();
        let portrait = screen.width() < screen.height();
        if portrait && ctx.data(|d| d.get_temp::<bool>(picker)).unwrap_or(false) {
            let error_id = picker.with("error");
            let mut chosen = None;
            let done = mobile_side_editor(ctx, picker, "Choose WAV", egui::Align::Max, 0.86, |ui| {
                if let Some(error) = ui.data(|d| d.get_temp::<String>(error_id)) {
                    ui.colored_label(RED, error);
                }
                chosen = arrangement_ui::sample_picker_list(ui, &self.lib, picker.with("search"), true);
            });
            let assigned = chosen.is_some_and(|path| {
                let assigned = self.lib.assign(&mut self.project, lane, &path);
                self.status.clone_from(&self.lib.message);
                if assigned {
                    self.m_playlist_undo_step();
                    self.publish_edits();
                    ctx.animate_bool(picker.with("animation"), false);
                } else {
                    ctx.data_mut(|d| d.insert_temp(error_id, self.lib.message.clone()));
                }
                assigned
            });
            if done || assigned {
                ctx.data_mut(|d| {
                    d.remove::<bool>(picker);
                    d.remove::<String>(error_id);
                });
                ctx.request_repaint();
            }
            return;
        }
        let drawer = id.with("controls");
        let mut choose = false;
        let controls = |ui: &mut Ui| {
            ui.add_space(8.0);
            if let Some(sample) = self.project.drum_samples.get(lane).and_then(Option::as_ref) {
                track_ui::sample_waveform(ui, sample, self.project.bpm, &self.shared);
            } else {
                ui.label("Built-in drum sound");
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if p_button(ui, "Preview", true, vec2(96.0, 40.0)).clicked() {
                    self.shared.send(Cmd::DrumHit { lane, vel: 112 });
                }
                if portrait {
                    choose = p_button(ui, "Choose WAV", false, vec2(110.0, 40.0)).clicked();
                    return;
                }
                ui.menu_button("Choose WAV", |ui| {
                    if let Some(path) = arrangement_ui::sample_picker(ui, &mut self.lib) {
                        self.lib.assign(&mut self.project, lane, &path);
                        self.status.clone_from(&self.lib.message);
                        ui.close_menu();
                    }
                });
            });
            ui.separator();
            let channel = &mut self.project.channels[lane];
            ui.label("Level");
            ui.add(egui::Slider::new(&mut channel.volume, 0.0..=1.0)
                .custom_formatter(|v, _| format!("{:+.1} dB", widgets::db(v as f32))));
            ui.label("Pan");
            ui.add(egui::Slider::new(&mut channel.pan, -1.0..=1.0));
        };
        let done = if portrait {
            mobile_side_editor(ctx, drawer, DRUM_NAMES[lane], egui::Align::Min, 0.82, |ui| {
                mobile_scroll(egui::ScrollArea::vertical()).show(ui, controls);
            })
        } else {
            mobile_editor_sheet(ctx, "m_drum_sound", DRUM_NAMES[lane], controls)
        };
        if choose {
            self.shared.send(Cmd::PreviewStop);
            ctx.data_mut(|d| {
                d.remove::<String>(picker.with("error"));
                d.insert_temp(picker, true);
            });
            ctx.request_repaint();
        }
        if done {
            self.shared.send(Cmd::PreviewStop);
            ctx.data_mut(|d| { d.remove::<usize>(id); d.remove::<bool>(picker); });
        }
    }

    fn m_track_sheet(&mut self, ctx: &egui::Context) {
        let id = egui::Id::new("m_track_sheet_open");
        let animation = id.with("animation");
        let presets = egui::Id::new(devices_ui::EFFECT_PRESETS);
        let open = ctx.data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
        let shown = ctx.animate_bool(animation, open);
        if !open {
            return;
        }
        let Some(track) = self.project.arrangement.tracks.get(self.arr.sel_lane) else {
            ctx.data_mut(|d| d.remove::<bool>(id));
            return;
        };
        let title = track.name.clone();
        let screen = ctx.screen_rect();
        let portrait = screen.width() < screen.height();
        let width = if portrait { (screen.width() * 0.90).min(440.0) } else { screen.width() };
        let kind = if track.kind == crate::arrangement::TrackKind::Audio { "Audio track" } else { "MIDI track" };
        let sheet_h = if portrait { screen.height() }
            else { (screen.height() * 0.78).min(screen.height() - 40.0) };
        let origin = if portrait { screen.left_top() - vec2(width * (1.0 - shown), 0.0) }
            else { pos2(screen.left(), screen.bottom() - sheet_h * shown) };
        let mut done = false;
        egui::Area::new(id.with("scrim")).order(egui::Order::Middle).fixed_pos(screen.min)
                .show(ctx, |ui| {
                    let (rect, resp) = ui.allocate_exact_size(screen.size(), Sense::click());
                    ui.painter().rect_filled(rect, 0.0, Color32::from_black_alpha((120.0 * shown) as u8));
                    done |= resp.clicked();
                });
        egui::Area::new(id.with("drawer"))
            .fixed_pos(origin).constrain(false).movable(false).order(egui::Order::Foreground)
            .show(ctx, |ui| {
                let size = vec2(width, sheet_h);
                let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
                let mut inner = rect.shrink(9.0);
                let corners = if portrait { egui::Rounding { ne: 18.0, se: 18.0, ..Default::default() } }
                    else { egui::Rounding { nw: 20.0, ne: 20.0, ..Default::default() } };
                ui.painter().rect_filled(rect, corners, p_bg());
                ui.painter().rect_stroke(rect, corners, Stroke::new(1.0, outline()));
                if portrait {
                    let header = Rect::from_min_size(rect.min, vec2(rect.width(), 62.0));
                    let header_corners = egui::Rounding { ne: 18.0, ..Default::default() };
                    ui.painter().rect_filled(header, header_corners, surface_fill());
                    ui.painter().hline(header.x_range(), header.bottom(), Stroke::new(1.0, surface_edge()));
                    let mut head = ui.new_child(egui::UiBuilder::new().id_salt("track_drawer_header")
                        .max_rect(header.shrink2(vec2(9.0, 8.0))));
                    head.set_clip_rect(header.intersect(screen));
                    mobile_visuals(&mut head);
                    head.horizontal(|ui| {
                        done |= p_button(ui, "Done", true, vec2(57.0, 36.0)).clicked();
                        ui.vertical(|ui| {
                            ui.add(egui::Label::new(RichText::new(&title).size(17.0).strong()).truncate());
                            ui.label(RichText::new(kind).size(11.0).color(muted()));
                        });
                    });
                    inner.min.y = header.bottom() + 8.0;
                } else {
                    let head = Rect::from_min_size(rect.min, vec2(rect.width(), 22.0));
                    let grip = ui.interact(head, id.with("grip"), Sense::click_and_drag());
                    ui.painter().rect_filled(Rect::from_center_size(pos2(head.center().x, head.top() + 10.0),
                        vec2(40.0, 5.0)), 2.5, outline());
                    if grip.clicked() || grip.drag_stopped() && grip.drag_delta().y >= 0.0 || grip.dragged() && ui.input(|i| {
                        i.pointer.press_origin().zip(i.pointer.interact_pos()).is_some_and(|(a, b)| b.y - a.y > 40.0)
                    }) {
                        done = true;
                    }
                    inner.min.y = head.bottom();
                }
                let mut content = ui.new_child(egui::UiBuilder::new().max_rect(inner));
                content.set_clip_rect(inner.intersect(screen));
                mobile_visuals(&mut content);
                if !portrait {
                    content.horizontal(|ui| {
                        done |= p_button(ui, "Done", true, vec2(60.0, 32.0)).clicked();
                        ui.add(egui::Label::new(RichText::new(&title).size(15.0).strong()).truncate());
                    });
                }
                let mut detail = arrangement_ui::DetailCtx {
                    roll_track: &mut self.roll.track,
                    live_track: &mut self.live_track,
                    inst: &mut self.inst,
                };
                devices_ui::mobile_track_settings(&mut content, &mut self.project, &mut self.arr,
                    &mut detail, &mut self.plugin_window, &mut self.lib, &self.shared);
            });
        let picker = egui::Id::new(devices_ui::EFFECT_PICKER);
        let target = ctx.data(|d| d.get_temp::<devices_ui::EffectPresetTarget>(presets))
            .filter(|target| target.0 == self.arr.sel_lane);
        let picking = target.is_some() || ctx.data(|d| d.get_temp::<usize>(picker)) == Some(self.arr.sel_lane);
        if done || (!picking && ctx.input(|input| input.key_pressed(egui::Key::Escape))) {
            ctx.data_mut(|d| {
                d.remove::<bool>(id);
                d.remove::<(usize, u8)>(egui::Id::new(devices_ui::INSTRUMENT_PANEL));
                d.remove::<usize>(egui::Id::new(devices_ui::INSTRUMENT_EDITOR));
                d.remove::<usize>(picker);
                d.remove::<devices_ui::EffectPresetTarget>(presets);
            });
        } else if let Some(target) = target {
            self.m_track_effect_presets(ctx, presets, target);
        } else if picking {
            self.m_track_effect_picker(ctx, picker, self.arr.sel_lane);
        } else {
            ctx.data_mut(|d| d.remove::<usize>(picker));
            self.m_instrument_panel(ctx);
        }
    }

    fn m_track_effect_picker(&mut self, ctx: &egui::Context, id: egui::Id, lane: usize) {
        let mut picked = None;
        let error_id = id.with("error");
        let mut close = mobile_effect_sheet(ctx, id, "Add effect", |content| {
            if let Some(error) = content.data(|d| d.get_temp::<String>(error_id)) {
                content.colored_label(RED, error);
            }
            mobile_scroll(egui::ScrollArea::vertical()).id_salt(id.with("effects"))
                .auto_shrink([false, false]).show(content, |ui| {
                    for kind in crate::fx::ALL {
                        if option_row(ui, kind.name(), kind.category()).clicked() {
                            picked = Some(kind);
                        }
                    }
                });
        });
        if let Some(kind) = picked {
            match devices_ui::add_effect(&mut self.project, lane, kind) {
                Ok(()) => {
                    self.m_playlist_undo_step();
                    self.publish_edits();
                    close = true;
                }
                Err(error) => ctx.data_mut(|d| d.insert_temp(error_id, error.to_owned())),
            }
        }
        if close {
            ctx.data_mut(|d| {
                d.remove::<usize>(id);
                d.remove::<String>(error_id);
            });
            ctx.request_repaint();
        }
    }

    fn m_track_effect_presets(&mut self, ctx: &egui::Context, id: egui::Id, target: devices_ui::EffectPresetTarget) {
        let (lane, insert, index, kind) = target;
        let slot = self.project.arrangement.tracks.get(lane).filter(|track| track.output == Some(insert))
            .and_then(|_| self.project.insert_fx.get_mut(insert))
            .and_then(|chain| chain.get_mut(index))
            .filter(|slot| !slot.empty && slot.kind == kind);
        let Some(slot) = slot else {
            ctx.data_mut(|d| d.remove::<devices_ui::EffectPresetTarget>(id));
            return;
        };
        let title = format!("{} presets", kind.name());
        let mut picked = None;
        let close = mobile_effect_sheet(ctx, id, &title, |ui| {
            let current = slot.all();
            mobile_scroll(egui::ScrollArea::vertical()).auto_shrink([false, false]).show(ui, |ui| {
                for (name, values) in kind.presets() {
                    let active = values == current;
                    if p_button(ui, name, active, vec2(ui.available_width(), 40.0)).clicked() {
                        picked = Some(values);
                    }
                }
            });
        });
        if let Some(values) = picked {
            slot.set_all(&values);
            self.m_playlist_undo_step();
            self.publish_edits();
        }
        if close || picked.is_some() {
            ctx.data_mut(|d| d.remove::<devices_ui::EffectPresetTarget>(id));
            ctx.request_repaint();
        }
    }

    /// The instrument's full editor (the PC plugin window), full screen with Done.
    fn m_instrument_editor(&mut self, ctx: &egui::Context) {
        let id = egui::Id::new(devices_ui::INSTRUMENT_EDITOR);
        let Some(lane) = ctx.data(|d| d.get_temp::<usize>(id)) else { return };
        let Some((name, track)) = self.project.arrangement.tracks.get(lane).and_then(|t| t.instrument.map(|i| (t.name.clone(), i))) else {
            ctx.data_mut(|d| d.remove::<usize>(id));
            return;
        };
        self.inst = instruments_ui::Inst::from_track(track);
        let screen = ctx.screen_rect();
        let mut done = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        let mut picked = None;
        ctx.memory_mut(|m| m.areas_mut().move_to_top(egui::LayerId::new(egui::Order::Foreground, id.with("area"))));
        egui::Area::new(id.with("area")).order(egui::Order::Foreground).fixed_pos(screen.min).movable(false).constrain(false)
            .show(ctx, |ui| {
                let (rect, _) = ui.allocate_exact_size(screen.size(), Sense::click());
                ui.painter().rect_filled(rect, 0.0, p_bg());
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink(8.0)));
                mobile_visuals(&mut child);
                child.horizontal(|ui| {
                    done |= p_button(ui, "Done", true, vec2(64.0, 40.0)).clicked();
                    ui.add(egui::Label::new(RichText::new(format!("{name} · {}", self.inst.title())).size(16.0).strong()).truncate());
                });
                child.add_space(6.0);
                picked = instruments_ui::show_mobile(&mut child, &mut self.project, &mut self.inst, lane);
            });
        if let Some(pt) = picked {
            // a preset can live on another engine: the track follows it
            self.project.arrangement.tracks[lane].instrument = Some(pt);
            self.roll.track = pt;
            self.live_track = pt;
        }
        if done {
            ctx.data_mut(|d| d.remove::<usize>(id));
        }
    }

    /// Left side panel for the phone instrument actions: instruments (with what each is for), WAV samples, presets.
    fn m_instrument_panel(&mut self, ctx: &egui::Context) {
        let id = egui::Id::new(devices_ui::INSTRUMENT_PANEL);
        let Some((lane, kind)) = ctx.data(|d| d.get_temp::<(usize, u8)>(id)) else { return };
        if lane >= self.project.arrangement.tracks.len() {
            ctx.data_mut(|d| d.remove::<(usize, u8)>(id));
            return;
        }
        let screen = ctx.screen_rect();
        let width = (screen.width() * 0.82).min(360.0);
        let shown = ctx.animate_bool_with_time(id.with("slide"), true, 0.14);
        let rect = Rect::from_min_size(pos2(screen.left() - width * (1.0 - shown), screen.top()), vec2(width, screen.height()));
        let current = self.project.arrangement.tracks[lane].instrument;
        let title = match kind {
            0 => "Choose instrument".to_owned(),
            1 => "Load WAV".to_owned(),
            _ => format!("Presets · {}", current.map_or("", |t| t.name())),
        };
        let mut close = ctx.input(|i| i.pointer.any_pressed() && i.pointer.press_origin().is_some_and(|at| !rect.contains(at)));
        let mut pick_instrument = None;
        let mut pick_sample = None;
        let mut pick_preset = None;
        let preview_id = egui::Id::new("m_browser_preview");
        let mut preview = ctx.data(|d| d.get_temp::<PathBuf>(preview_id));
        let mut preview_toggle = None;
        // always above the track sheet it was opened from
        ctx.memory_mut(|m| m.areas_mut().move_to_top(egui::LayerId::new(egui::Order::Foreground, id.with("panel"))));
        egui::Area::new(id.with("panel")).order(egui::Order::Foreground).fixed_pos(rect.min).constrain(false).movable(false)
            .show(ctx, |ui| {
                Frame::none().fill(bar_fill()).rounding(egui::Rounding { ne: 16.0, se: 16.0, ..Default::default() })
                    .stroke(Stroke::new(1.0, outline())).inner_margin(Margin::symmetric(12.0, 12.0)).show(ui, |ui| {
                        mobile_visuals(ui);
                        ui.set_min_size(rect.size() - vec2(24.0, 24.0));
                        ui.set_max_size(rect.size() - vec2(24.0, 24.0));
                        ui.horizontal(|ui| {
                            ui.add(egui::Label::new(RichText::new(&title).size(19.0).strong()).truncate());
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                close |= p_button(ui, "×", false, vec2(40.0, 40.0)).clicked();
                            });
                        });
                        let hint = match kind {
                            0 => "The instrument is the sound engine that plays this track's notes.",
                            1 => "The sample becomes a Sampler instrument. The triangle plays it first.",
                            _ => "Tap to try a preset; the panel stays open so you can compare.",
                        };
                        ui.label(RichText::new(hint).size(11.0).color(muted()));
                        ui.add_space(6.0);
                        match kind {
                            0 => { mobile_scroll(egui::ScrollArea::vertical()).id_salt("m_inst_list").show(ui, |ui| {
                                for t in [Track::Synth, Track::Wavetable, Track::Analog, Track::Bass808, Track::Sampler, Track::Drums] {
                                    let name = if current == Some(t) { format!("{}  ✓", t.name()) } else { t.name().to_owned() };
                                    if devices_ui::mobile_action(ui, &name, devices_ui::instrument_blurb(t)).clicked() {
                                        pick_instrument = Some(t);
                                    }
                                }
                            }); }
                            1 => {
                                let query_id = id.with("query");
                                let mut query = ui.data(|d| d.get_temp::<String>(query_id)).unwrap_or_default();
                                ui.add_sized([ui.available_width(), 34.0], egui::TextEdit::singleline(&mut query).hint_text("Search samples"));
                                let filter = query.trim().to_lowercase();
                                ui.data_mut(|d| d.insert_temp(query_id, query));
                                mobile_scroll(egui::ScrollArea::vertical()).id_salt("m_inst_samples").show(ui, |ui| {
                                    if self.lib.files.is_empty() {
                                        ui.label(RichText::new("No WAV files yet. Import them in the Browser (+).").size(12.0).color(muted()));
                                    }
                                    for path in &self.lib.files {
                                        let name = path.file_stem().unwrap_or_default().to_string_lossy();
                                        if !filter.is_empty() && !name.to_lowercase().contains(&filter) {
                                            continue;
                                        }
                                        let playing = preview.as_ref() == Some(path);
                                        let (row, play) = browser_list_row(ui, &name, Some(playing), browser::DragItem::Sample(path.clone()));
                                        if play {
                                            preview_toggle = Some(path.clone());
                                        } else if row.clicked() {
                                            pick_sample = Some(path.clone());
                                        }
                                    }
                                });
                            }
                            _ => { mobile_scroll(egui::ScrollArea::vertical()).id_salt("m_inst_presets").show(ui, |ui| {
                                match current {
                                    Some(t) => {
                                        let last = ui.data(|d| d.get_temp::<String>(id.with(("preset", lane))));
                                        for preset in instruments_ui::presets_of(t) {
                                            let active = last.as_deref() == Some(preset);
                                            if p_button(ui, preset, active, vec2(ui.available_width(), 44.0)).clicked() {
                                                pick_preset = Some((t, preset));
                                            }
                                        }
                                    }
                                    None => { ui.label(RichText::new("Choose an instrument first.").color(muted())); }
                                }
                            }); }
                        }
                    });
            });
        if let Some(path) = preview_toggle {
            if preview.as_ref() == Some(&path) {
                self.shared.send(Cmd::PreviewStop);
                preview = None;
            } else if let Ok(data) = crate::project::load_wav(&path) {
                self.shared.send(Cmd::Preview(std::sync::Arc::new(data)));
                preview = Some(path);
            }
            ctx.data_mut(|d| match &preview {
                Some(path) => d.insert_temp(preview_id, path.clone()),
                None => d.remove::<PathBuf>(preview_id),
            });
        }
        let mut detail = arrangement_ui::DetailCtx { roll_track: &mut self.roll.track, live_track: &mut self.live_track, inst: &mut self.inst };
        if let Some(t) = pick_instrument {
            devices_ui::assign(&mut self.project, lane, t, &mut detail);
            close = true;
        }
        if let Some(path) = pick_sample {
            self.lib.assign_track_sampler(&mut self.project, lane, &path);
            devices_ui::assign(&mut self.project, lane, Track::Sampler, &mut detail);
            self.shared.send(Cmd::PreviewStop);
            ctx.data_mut(|d| d.remove::<PathBuf>(preview_id));
            close = true;
        }
        if let Some((t, preset)) = pick_preset {
            instruments_ui::apply_named_lane(&mut self.project, lane, t, preset);
            self.m_playlist_undo_step();
            self.publish_edits();
            if let Some(sample) = instruments_ui::preset_preview(t, preset) {
                self.shared.send(Cmd::Preview(std::sync::Arc::new(sample)));
            }
            ctx.data_mut(|d| d.insert_temp(id.with(("preset", lane)), preset.to_owned()));
        }
        if close || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            if kind == 2 {
                self.shared.send(Cmd::PreviewStop);
            }
            ctx.data_mut(|d| d.remove::<(usize, u8)>(id));
        }
    }

    /// Clip launcher in portrait: a row per scene, a column per track. Tap a clip to launch it on the next bar, tap a
    /// running clip to stop its track; tap an empty MIDI cell to put the current pattern there; hold a clip to remove it.
    fn m_launchpad(&mut self, ui: &mut Ui) {
        use crate::arrangement::TrackKind;
        use crate::audio_engine::QUEUED_STOP;
        use crate::session::MAX_LANES;

        const BANK: usize = 4;
        let lanes = self.project.arrangement.tracks.len().min(MAX_LANES);
        if lanes == 0 {
            ui.label(RichText::new("Add a track in Playlist to launch clips.").color(muted()));
            return;
        }
        let track_id = egui::Id::new("m_session_track_bank");
        let scene_id = egui::Id::new("m_session_scene_bank");
        let track_banks = lanes.div_ceil(BANK);
        let scene_banks = self.project.session.scenes.max(1).div_ceil(BANK);
        let mut track_bank = ui.data(|d| d.get_temp::<usize>(track_id)).unwrap_or(0).min(track_banks - 1);
        let mut scene_bank = ui.data(|d| d.get_temp::<usize>(scene_id)).unwrap_or(0).min(scene_banks - 1);
        let mut add = false;
        let mut remove = false;
        Frame::none().fill(surface_fill()).stroke(Stroke::new(1.0, surface_edge())).rounding(13.0)
            .inner_margin(Margin::symmetric(6.0, 4.0)).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    if dock_icon(ui, Icon::Stop, false, "Stop all clips").clicked() {
                        self.shared.send(Cmd::StopAllClips);
                    }
                    egui::ComboBox::from_id_salt(track_id).width(64.0)
                        .selected_text(format!("Tracks {}–{}", track_bank * BANK + 1, ((track_bank + 1) * BANK).min(lanes)))
                        .show_ui(ui, |ui| {
                            for bank in 0..track_banks {
                                let label = format!("Tracks {}–{}", bank * BANK + 1, ((bank + 1) * BANK).min(lanes));
                                ui.selectable_value(&mut track_bank, bank, label);
                            }
                        });
                    egui::ComboBox::from_id_salt(scene_id).width(64.0)
                        .selected_text(format!("Scenes {}–{}", scene_bank * BANK + 1,
                            ((scene_bank + 1) * BANK).min(self.project.session.scenes)))
                        .show_ui(ui, |ui| {
                            for bank in 0..scene_banks {
                                let label = format!("Scenes {}–{}", bank * BANK + 1,
                                    ((bank + 1) * BANK).min(self.project.session.scenes));
                                ui.selectable_value(&mut scene_bank, bank, label);
                            }
                        });
                    dock_menu(ui, Icon::Menu, "Scene options", false, |ui| {
                        if ui.add_enabled(self.project.session.scenes < 64, egui::Button::new("Add scene")).clicked() {
                            add = true;
                            ui.close_menu();
                        }
                        if ui.add_enabled(self.project.session.scenes > 1, egui::Button::new("Remove last scene")).clicked() {
                            remove = true;
                            ui.close_menu();
                        }
                    });
                });
            });
        if add || remove {
            self.m_playlist_undo_step();
            if add {
                self.project.session.add_scene();
                scene_bank = self.project.session.scenes.saturating_sub(1) / BANK;
            } else {
                self.project.session.remove_scene();
                scene_bank = scene_bank.min(self.project.session.scenes.saturating_sub(1) / BANK);
            }
        }
        ui.data_mut(|d| {
            d.insert_temp(track_id, track_bank);
            d.insert_temp(scene_id, scene_bank);
        });
        ui.add_space(6.0);

        let first_track = track_bank * BANK;
        let first_scene = scene_bank * BANK;
        let track_end = (first_track + BANK).min(lanes);
        let scene_end = (first_scene + BANK).min(self.project.session.scenes);
        let gap = 4.0;
        let ruler_width = 34.0;
        let header = 34.0;
        let columns = BANK as f32;
        let width_limit = (ui.available_width() - ruler_width - 12.0) / columns - gap;
        let height_limit = (ui.available_height() - header) / columns - gap;
        let pad_size = width_limit.min(height_limit).clamp(44.0, 74.0);
        let beat = TICKS_PER_BAR / 4;
        let phase = (self.shared.position.load(Ordering::Relaxed) % beat) as f32 / beat as f32;
        let mut animate = false;
        let source = self.arr.sel_clip.and_then(|index| self.project.arrangement.clips.get(index)).copied();
        mobile_scroll(egui::ScrollArea::both()).id_salt("m_session_pad_matrix").auto_shrink([false, false]).show(ui, |ui| {
            let matrix_width = ruler_width + (pad_size + gap) * BANK as f32;
            let matrix_height = header + (pad_size + gap) * (scene_end - first_scene) as f32;
            let (area, _) = ui.allocate_exact_size(vec2(matrix_width, matrix_height), Sense::hover());
            let p = ui.painter_at(area);
            p.rect_filled(area, 12.0, p_bg());
            let scene_left = area.left() + BANK as f32 * (pad_size + gap);
            let x_at = |lane: usize| area.left() + (lane - first_track) as f32 * (pad_size + gap);
            for lane in first_track..track_end {
                let track = &self.project.arrangement.tracks[lane];
                let rect = Rect::from_min_size(pos2(x_at(lane), area.top()), vec2(pad_size, header - gap));
                let color = lane_col(lane);
                p.rect_filled(rect, 9.0, if lane == self.arr.sel_lane { selected_fill() } else { surface_fill() });
                let stop = Rect::from_min_size(rect.right_top() - vec2(20.0, 0.0), vec2(20.0, rect.height()));
                let active = self.project.session.clips.iter().filter(|clip| clip.lane == lane).any(|clip| {
                    let code = clip.code();
                    self.shared.session_run[lane].load(Ordering::Relaxed) == code
                        || self.shared.session_queue[lane].load(Ordering::Relaxed) == code
                });
                animate |= active;
                let label = Rect::from_min_max(rect.min + vec2(5.0, 0.0), pos2(stop.left(), rect.bottom()));
                playlist_label(&p, label, rect.center().y, &track.name, ink());
                p.rect_filled(Rect::from_min_size(rect.left_bottom() + vec2(5.0, -4.0),
                    vec2(rect.width() - 10.0, 2.0)), 1.0, color);
                if ui.interact(label, ui.id().with(("session_track", lane)), Sense::click()).clicked() {
                    self.arr.sel_lane = lane;
                }
                let stop_hit = ui.interact(stop, ui.id().with(("session_track_stop", lane)),
                    if active { Sense::click() } else { Sense::hover() });
                p.rect_filled(Rect::from_center_size(stop.center(), vec2(7.0, 7.0)),
                    2.0, if active { color } else { muted().gamma_multiply(0.45) });
                if stop_hit.on_hover_text("Stop track").clicked() {
                    self.shared.send(Cmd::StopLane { lane });
                }
            }

            for scene in first_scene..scene_end {
                let y = area.top() + header + (scene - first_scene) as f32 * (pad_size + gap);
                let scene_pad = Rect::from_min_size(pos2(scene_left, y), vec2(ruler_width, pad_size));
                let scene_has_clips = self.project.session.clips.iter().any(|clip| clip.scene == scene);
                let hit = ui.interact(scene_pad, ui.id().with(("session_scene", scene)),
                    if scene_has_clips { Sense::click() } else { Sense::hover() });
                p.rect_filled(scene_pad, 12.0, if scene_has_clips {
                    tint(bar_fill(), p_accent(), 0.16)
                } else { bar_fill() });
                p.rect_stroke(scene_pad, 12.0, Stroke::new(1.0, outline()));
                control_state(&p, scene_pad, 9.0, &hit, ink());
                icon(&p, Icon::Play, scene_pad.center() - vec2(0.0, 8.0),
                    if scene_has_clips { p_accent() } else { muted() });
                p.text(scene_pad.center() + vec2(0.0, 9.0), Align2::CENTER_CENTER, scene + 1,
                    FontId::proportional(10.0), ink());
                if hit.on_hover_text("Launch scene").clicked() {
                    for clip in self.project.session.clips.iter().filter(|clip| clip.scene == scene) {
                        self.shared.send(Cmd::LaunchClip { lane: clip.lane, pattern: clip.pattern, audio: clip.audio });
                    }
                }
                for lane in first_track..track_end {
                    let pad = Rect::from_min_size(pos2(x_at(lane), y), vec2(pad_size, pad_size));
                    let hit = ui.interact(pad, ui.id().with(("session_pad", lane, scene)), Sense::click());
                    let clip = self.project.session.get(lane, scene).copied();
                    let audio_lane = self.project.arrangement.tracks[lane].kind == TrackKind::Audio;
                    if let Some(clip) = clip {
                        let code = clip.code();
                        let running = self.shared.session_run[lane].load(Ordering::Relaxed) == code;
                        let queue = self.shared.session_queue[lane].load(Ordering::Relaxed);
                        let pending_stop = running && queue == QUEUED_STOP;
                        let queued = queue == code || pending_stop;
                        animate |= running || queued;
                        let lane_color = lane_col(lane);
                        let pad_color = tint(card_fill(), lane_color, if running { 0.38 } else { 0.20 });
                        let signal = if queued { argb(0xFFC58B48) } else { lane_color };
                        let text = ink();
                        p.rect_filled(pad, 12.0, pad_color);
                        control_state(&p, pad, 12.0, &hit, text);
                        p.rect_stroke(pad.shrink(1.0), 11.0, Stroke::new(
                            if running || queued { 2.0 } else { 1.0 },
                            if running || queued { signal } else { tint(outline(), lane_color, 0.35) },
                        ));
                        icon(&p, if running { Icon::Stop } else { Icon::Play }, pad.left_top() + vec2(14.0, 15.0), text);
                        let led = pad.right_top() + vec2(-10.0, 11.0);
                        p.circle_filled(led, 3.0, if running || queued { signal } else { lane_color.gamma_multiply(0.48) });
                        let name = if clip.audio { self.project.audio_pool.get(clip.pattern).map(|s| s.name.as_str()) }
                            else { self.project.patterns.get(clip.pattern).map(|p| p.name.as_str()) };
                        playlist_label(&p, pad.shrink2(vec2(6.0, 0.0)), pad.center().y + 3.0,
                            name.unwrap_or("Clip"), text);
                        if running {
                            let progress = Rect::from_min_size(pad.left_bottom() + vec2(6.0, -7.0),
                                vec2((pad.width() - 12.0) * phase, 2.0));
                            p.rect_filled(progress, 1.0, lane_color);
                        }
                        let hint = if pending_stop { "Stop queued" } else if queued { "Launch queued" }
                            else if running { "Playing · tap to stop" } else { "Tap to launch · hold to clear" };
                        let hit = hit.on_hover_text(hint);
                        if hit.long_touched() || hit.secondary_clicked() {
                            if running || queued { self.shared.send(Cmd::StopLane { lane }); }
                            self.m_playlist_undo_step();
                            self.project.session.clear(lane, scene);
                        } else if hit.clicked() {
                            if running || queued {
                                self.shared.send(Cmd::StopLane { lane });
                            } else {
                                self.shared.send(Cmd::LaunchClip { lane, pattern: clip.pattern, audio: clip.audio });
                            }
                        }
                    } else {
                        let assign = source.filter(|clip| clip.audio == audio_lane).map(|clip| clip.pattern)
                            .or_else(|| if audio_lane {
                                self.project.arrangement.tracks[lane].audio_brush
                                    .filter(|&sample| sample < self.project.audio_pool.len())
                            } else { Some(self.project.current_pattern).filter(|&pattern| pattern < self.project.patterns.len()) });
                        p.rect_filled(pad, 12.0, surface_fill());
                        p.rect_stroke(pad, 12.0, Stroke::new(1.0, grid_line()));
                        if assign.is_some() {
                            control_state(&p, pad, 9.0, &hit, ink());
                        }
                        icon(&p, Icon::Plus, pad.center(), if assign.is_some() { muted() } else { muted().gamma_multiply(0.45) });
                        let hint = if assign.is_some() { "Assign clip" } else { "Select an Audio clip in Playlist first" };
                        if hit.on_hover_text(hint).clicked() {
                            if let Some(pattern) = assign {
                                self.m_playlist_undo_step();
                                self.project.session.set(lane, scene, pattern, audio_lane);
                            }
                        }
                    }
                }
            }
        });
        if animate {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(45));
        }
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
            panel(ui.painter(), rect, 20.0);
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink(16.0)));
            mobile_visuals(&mut child);
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
            panel(ui.painter(), rect, 20.0);
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink(16.0)));
            mobile_visuals(&mut child);
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
        self.m_start_export_range(share, None);
    }

    fn m_start_export_range(&mut self, share: bool, range: Option<(u32, u32)>) {
        if self.m_export.rx.is_some() {
            return;
        }
        let mut p = self.project.clone();
        p.song_mode = range.is_some() || self.m_export.song;
        let settings = crate::audio_engine::export::ExportSettings {
            sample_rate: 44100,
            depth: crate::audio_engine::export::BitDepth::Pcm16,
            bars: crate::audio_engine::export::default_bars(&p),
            tail_secs: 2.0,
        };
        let stamp = web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let clean: String = p.name.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
        let label = if range.is_some() { "-loop" } else { "" };
        let name = format!("{}{}-{}.wav", if clean.is_empty() { "Winteria".to_string() } else { clean }, label, stamp % 1_000_000);
        #[cfg(target_arch = "wasm32")]
        {
            let _ = share;
            let result = if let Some((start, end)) = range {
                crate::web::export_wav_range(&p, &settings, &name, start, end)
            } else {
                crate::web::export_wav(&p, &settings, &name)
            };
            self.m_export.status = match result {
                Ok(()) => format!("Downloaded: {name}"),
                Err(error) => error,
            };
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let path = export_dir().join(name);
            let (tx, rx) = crossbeam_channel::bounded(1);
            std::thread::spawn(move || {
                use crate::audio_engine::export;
                let res = if let Some((start, end)) = range {
                    export::export_range(&p, &settings, start, end, &path)
                } else {
                    export::export(&p, &settings, &path)
                }.map(|_| path);
                let _ = tx.send(res);
            });
            self.m_export.rx = Some(rx);
            self.m_export.share_after = share;
            self.m_export.status.clear();
        }

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

    fn m_browser_sidebar(&mut self, ctx: &egui::Context) {
        let id = egui::Id::new("m_browser_open");
        let open = ctx.data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
        let screen = ctx.screen_rect();
        // wide enough for the two panes (categories / items)
        let width = if screen.width() < screen.height() {
            (screen.width() * 0.96).min(520.0)
        } else {
            (screen.width() * 0.62).clamp(320.0, 600.0)
        };
        let dragging = egui::DragAndDrop::has_any_payload(ctx);
        let drag_id = id.with("dragging");
        let was_dragging = ctx.data(|d| d.get_temp::<bool>(drag_id)).unwrap_or(false);
        ctx.data_mut(|d| {
            if was_dragging && !dragging {
                d.insert_temp(id, false);
            }
            d.insert_temp(drag_id, dragging && open);
        });
        if dragging {
            return;
        }
        let open = open && !was_dragging;
        let shown = ctx.animate_bool_with_time(id.with("slide"), open, 0.14);
        if shown <= 0.0 {
            return;
        }

        let mut close = false;
        egui::Area::new(id.with("scrim")).order(egui::Order::Middle).fixed_pos(screen.min)
            .show(ctx, |ui| {
                let (rect, response) = ui.allocate_exact_size(screen.size(), Sense::click());
                ui.painter().rect_filled(rect, 0.0, Color32::from_black_alpha((150.0 * shown) as u8));
                close = response.clicked();
            });

        egui::Area::new(id.with("drawer")).order(egui::Order::Foreground)
            .fixed_pos(pos2(screen.left() - width * (1.0 - shown), screen.top()))
            .constrain(false)
            .show(ctx, |ui| {
                Frame::none().fill(bar_fill())
                    .rounding(egui::Rounding { ne: 16.0, se: 16.0, ..Default::default() })
                    .inner_margin(Margin::symmetric(12.0, 14.0))
                    .show(ui, |ui| {
                        mobile_visuals(ui);
                        ui.set_min_size(vec2(width - 24.0, screen.height() - 28.0));
                        ui.set_max_size(vec2(width - 24.0, screen.height() - 28.0));
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Browser").size(24.0).strong().color(ink()));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                close |= p_button(ui, "×", false, vec2(42.0, 42.0))
                                    .on_hover_text("Close Browser").clicked();
                            });
                        });
                        ui.add_space(10.0);
                        if open && !close {
                            self.m_browser_items(ui);
                        }
                    });
            });

        if close || ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            ctx.data_mut(|d| {
                d.insert_temp(id, false);
                d.remove::<(usize, u32)>(egui::Id::new("m_audio_clip_target"));
            });
            ctx.request_repaint();
        }
    }

    /// Phone browser laid out like Ableton's: search on top, Categories and Places on the left, the items of the
    /// selected category or pack on the right (▷ previews a sample), the tap destination at the bottom.
    fn m_browser_items(&mut self, ui: &mut Ui) {
        let query_id = egui::Id::new("m_browser_query");
        let cat_id = egui::Id::new("m_browser_category");
        let place_id = egui::Id::new("m_browser_place");
        let preview_id = egui::Id::new("m_browser_preview");
        let mut query = ui.data(|d| d.get_temp::<String>(query_id)).unwrap_or_default();
        // 0 Sounds, 1 Drums, 2 Instruments, 3 Effects
        let mut category = ui.data(|d| d.get_temp::<u8>(cat_id)).unwrap_or(0);
        let mut place = ui.data(|d| d.get_temp::<String>(place_id)).unwrap_or_default();
        // other screens open the browser on Sounds or on Plugins
        match ui.data_mut(|d| d.remove_temp::<bool>(egui::Id::new("m_browser_plugins"))) {
            Some(true) if category < 2 => category = 2,
            Some(false) if category >= 2 => category = 0,
            _ => {}
        }
        #[cfg(target_arch = "wasm32")]
        if let Some(files) = crate::web::take_sample_list() {
            self.lib.files = files;
        }
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let import_w = if cfg!(any(target_os = "android", target_arch = "wasm32")) { 44.0 } else { 0.0 };
            ui.add_sized([(ui.available_width() - import_w - 4.0).max(60.0), 34.0],
                egui::TextEdit::singleline(&mut query).hint_text("Search").vertical_align(egui::Align::Center));
            #[cfg(target_arch = "wasm32")]
            ui.menu_button(RichText::new("+").size(18.0), |ui| {
                if ui.button("Import WAV").clicked() { crate::web::choose_sounds(false); ui.close_menu(); }
                if ui.button("Import folder").clicked() { crate::web::choose_sounds(true); ui.close_menu(); }
            });
            #[cfg(target_os = "android")]
            {
                let pending = egui::Id::new("m_sound_import_pending");
                let status = egui::Id::new("m_sound_import_status");
                let busy = ui.data(|d| d.get_temp::<bool>(pending)).unwrap_or(false);
                let mut pick = None;
                ui.add_enabled_ui(!busy, |ui| {
                    let button = egui::Button::new(RichText::new("+").size(18.0)).min_size(vec2(40.0, 34.0));
                    egui::menu::menu_custom_button(ui, button, |ui| {
                        ui.set_min_width(170.0);
                        for (kind, label) in [(0, "WAV files"), (1, "Pack (.zip)"), (2, "Pack folder")] {
                            if ui.add(egui::Button::new(label).min_size(vec2(170.0, 40.0))).clicked() {
                                pick = Some(kind);
                                ui.close_menu();
                            }
                        }
                    });
                });
                if let Some(kind) = pick {
                    let result = self.android_updater.as_ref()
                        .ok_or_else(|| "Android file picker is unavailable.".to_owned())
                        .and_then(|android| if kind == 0 { android.choose_sounds() } else { android.choose_pack(kind == 2) });
                    let hint = match kind {
                        0 => "Select one or more WAV files.",
                        1 => "Select a ZIP with WAV files.",
                        _ => "Select a folder with WAV files.",
                    };
                    ui.data_mut(|d| match result {
                        Ok(()) => {
                            d.insert_temp(pending, true);
                            d.insert_temp(status, hint.to_owned());
                        }
                        Err(error) => d.insert_temp(status, error),
                    });
                }
            }
        });
        #[cfg(target_os = "android")]
        if let Some(status) = ui.data(|d| d.get_temp::<String>(egui::Id::new("m_sound_import_status"))) {
            ui.label(RichText::new(status).size(11.0).color(muted()));
        }
        let filter = query.trim().to_lowercase();
        let plugins = category >= 2;
        // packs = first folder under a library root
        #[cfg(target_arch = "wasm32")]
        let browser_root = std::path::Path::new("Browser samples");
        let pack_of = |path: &std::path::Path| -> String {
            #[cfg(target_arch = "wasm32")]
            let roots = self.lib.dirs.iter().map(PathBuf::as_path).chain(std::iter::once(browser_root));
            #[cfg(not(target_arch = "wasm32"))]
            let roots = self.lib.dirs.iter().map(PathBuf::as_path);
            roots.into_iter().find_map(|root| path.strip_prefix(root).ok())
                .filter(|rel| rel.components().count() > 1)
                .and_then(|rel| rel.components().next())
                .map(|c| c.as_os_str().to_string_lossy().to_string())
                .unwrap_or_default()
        };
        let mut packs: Vec<(String, usize)> = vec![];
        for path in &self.lib.files {
            let pack = pack_of(path);
            match packs.iter_mut().find(|(name, _)| *name == pack) {
                Some((_, count)) => *count += 1,
                None => packs.push((pack, 1)),
            }
        }
        packs.sort_by_key(|(name, _)| (name.is_empty(), name.to_lowercase()));
        const DRUM_WORDS: [&str; 13] = ["kick", "snare", "clap", "hat", "perc", "drum", "tom", "rim", "cymbal", "crash", "ride", "808", "shaker"];
        let files: Vec<PathBuf> = if plugins { vec![] } else {
            self.lib.files.iter().filter(|path| {
                let name = path.to_string_lossy().to_lowercase();
                (filter.is_empty() || path.file_stem().unwrap_or_default().to_string_lossy().to_lowercase().contains(&filter))
                    && (category != 1 || DRUM_WORDS.iter().any(|word| name.contains(word)))
                    // "" = everything, "/" = loose files outside any pack, otherwise one pack
                    && (place.is_empty() || pack_of(path) == if place == "/" { "" } else { place.as_str() })
            }).cloned().collect()
        };
        let picking_clip = ui.data(|d| d.get_temp::<(usize, u32)>(egui::Id::new("m_audio_clip_target"))).is_some();
        let midi_available = self.project.arrangement.tracks.get(self.arr.sel_lane)
            .is_some_and(|track| track.kind == crate::arrangement::TrackKind::Midi);
        let midi_destination = DRUM_NAMES.len();
        let drum_id = egui::Id::new("m_browser_drum_destination");
        let mut drum_destination = ui.data(|d| d.get_temp::<usize>(drum_id))
            .filter(|&lane| lane < DRUM_NAMES.len() || (lane == midi_destination && midi_available));
        let mut sound = None;
        let mut drum = None;
        let mut instrument = None;
        let mut effect = None;
        let mut preview = ui.data(|d| d.get_temp::<PathBuf>(preview_id));
        let mut preview_toggle: Option<PathBuf> = None;

        let bottom_h = if plugins || picking_clip { 0.0 } else { 38.0 };
        let body = ui.available_rect_before_wrap();
        let body = Rect::from_min_max(body.min + vec2(0.0, 4.0), pos2(body.right(), body.bottom() - bottom_h));
        let left_w = (body.width() * 0.36).clamp(96.0, 160.0);
        let left = Rect::from_min_max(body.min, pos2(body.left() + left_w, body.bottom()));
        let right = Rect::from_min_max(pos2(left.right() + 6.0, body.top()), body.max);
        ui.allocate_rect(body, Sense::hover());
        ui.painter().rect_filled(left, 8.0, card_fill().gamma_multiply(0.7));
        ui.painter().rect_filled(right, 8.0, card_fill().gamma_multiply(0.45));
        // left: categories and places
        let mut side = ui.new_child(egui::UiBuilder::new().id_salt("m_browser_side").max_rect(left.shrink(4.0)));
        side.set_clip_rect(left.intersect(ui.clip_rect()));
        mobile_scroll(egui::ScrollArea::vertical()).id_salt("m_browser_side_scroll").auto_shrink([false, false])
            .show(&mut side, |ui| {
                let entry = |ui: &mut Ui, label: &str, active: bool| -> bool {
                    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
                    if active {
                        ui.painter().rect_filled(rect, 6.0, selected_fill());
                        ui.painter().rect_filled(Rect::from_min_size(rect.min, vec2(3.0, rect.height())), 1.5, p_accent());
                    }
                    fit_label(ui.painter(), rect.shrink2(vec2(8.0, 0.0)), rect.center().y, label, if active { ink() } else { muted() }, 13.0);
                    response.clicked()
                };
                ui.label(RichText::new("CATEGORIES").size(10.0).color(muted()));
                for (index, label) in ["Sounds", "Drums", "Instruments", "Effects"].into_iter().enumerate() {
                    if entry(ui, label, category == index as u8 && (index >= 2 || place.is_empty())) {
                        category = index as u8;
                        place.clear();
                    }
                }
                ui.add_space(6.0);
                ui.label(RichText::new("PLACES").size(10.0).color(muted()));
                for (pack, count) in &packs {
                    let label = if pack.is_empty() { format!("Library ({count})") } else { format!("{pack} ({count})") };
                    let key = if pack.is_empty() { "/".to_owned() } else { pack.clone() };
                    if entry(ui, &label, category == 0 && place == key) {
                        category = 0;
                        place = key;
                    }
                }
                if packs.is_empty() {
                    ui.label(RichText::new("No sample packs yet. Use + to import.").size(10.0).color(muted()));
                }
            });
        // right: the items
        let mut list = ui.new_child(egui::UiBuilder::new().id_salt("m_browser_list_ui").max_rect(right.shrink(4.0)));
        list.set_clip_rect(right.intersect(ui.clip_rect()));
        list.horizontal(|ui| {
            ui.label(RichText::new("Name").size(11.0).color(muted()));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let count = match category { 2 => Track::ALL.len(), 3 => crate::fx::ALL.len(), _ => files.len() };
                ui.label(RichText::new(count.to_string()).size(11.0).color(muted()));
            });
        });
        mobile_scroll(egui::ScrollArea::vertical()).id_salt(("m_browser_list", category, place.clone()))
            .auto_shrink([false, false]).show(&mut list, |ui| {
                match category {
                    2 => for track in Track::ALL {
                        if track.name().to_lowercase().contains(&filter)
                            && browser_list_row(ui, track.name(), None, browser::DragItem::Instrument(track)).0.clicked()
                        {
                            instrument = Some(track);
                        }
                    },
                    3 => for kind in crate::fx::ALL {
                        if kind.name().to_lowercase().contains(&filter) && browser_list_row(ui, kind.name(), None, kind).0.clicked() {
                            effect = Some(kind);
                        }
                    },
                    _ => {
                        if files.is_empty() {
                            ui.label(RichText::new(if self.lib.files.is_empty() { "No WAV files in the sample library." } else { "Nothing matches." })
                                .size(12.0).color(muted()));
                        }
                        for path in &files {
                            let name = path.file_stem().unwrap_or_default().to_string_lossy();
                            let playing = preview.as_ref() == Some(path);
                            let (response, play) = browser_list_row(ui, &name, Some(playing), browser::DragItem::Sample(path.clone()));
                            let response = response.on_hover_text("Tap to use · ▷ to listen · hold for drum pads");
                            response.context_menu(|ui| {
                                ui.label("Assign to drum pad");
                                for (lane, label) in DRUM_NAMES.iter().enumerate() {
                                    if ui.button(*label).clicked() {
                                        drum = Some((lane, path.clone()));
                                        ui.close_menu();
                                    }
                                }
                            });
                            if play {
                                preview_toggle = Some(path.clone());
                            } else if response.clicked() {
                                if let Some(lane) = drum_destination.filter(|_| !picking_clip) {
                                    drum = Some((lane, path.clone()));
                                } else {
                                    sound = Some(path.clone());
                                }
                            }
                        }
                    }
                }
            });
        // bottom: where a tapped sample goes
        if bottom_h > 0.0 {
            let destination = match drum_destination {
                Some(lane) => DRUM_NAMES.get(lane).copied().unwrap_or("Selected MIDI track"),
                None => self.project.arrangement.tracks.get(self.arr.sel_lane)
                    .filter(|track| track.kind == crate::arrangement::TrackKind::Audio)
                    .map_or("New audio track", |track| track.name.as_str()),
            };
            let dest_button = egui::Button::new(RichText::new(format!("Tap to: {destination}")).size(12.0).color(ink()))
                .min_size(vec2(ui.available_width(), 32.0)).truncate();
            egui::menu::menu_custom_button(ui, dest_button, |ui| {
                if ui.selectable_value(&mut drum_destination, None, "Audio clip").clicked() {
                    ui.close_menu();
                }
                if ui.add_enabled(midi_available, egui::Button::new("Selected MIDI track")).clicked() {
                    drum_destination = Some(midi_destination);
                    ui.close_menu();
                }
                for (lane, label) in DRUM_NAMES.iter().enumerate() {
                    if ui.selectable_value(&mut drum_destination, Some(lane), *label).clicked() {
                        ui.close_menu();
                    }
                }
            }).response.on_hover_text("Tap a sample to put it here");
        }
        if let Some(path) = preview_toggle {
            if preview.as_ref() == Some(&path) {
                self.shared.send(Cmd::PreviewStop);
                preview = None;
            } else {
                match crate::project::load_wav(&path) {
                    Ok(data) => {
                        self.shared.send(Cmd::Preview(std::sync::Arc::new(data)));
                        preview = Some(path);
                    }
                    Err(error) => self.arr.msg = error,
                }
            }
        }
        ui.data_mut(|d| {
            d.insert_temp(query_id, query);
            d.insert_temp(cat_id, category);
            d.insert_temp(place_id, place);
            match preview {
                Some(path) => d.insert_temp(preview_id, path),
                None => d.remove::<PathBuf>(preview_id),
            }
            match drum_destination {
                Some(lane) => d.insert_temp(drum_id, lane),
                None => d.remove::<usize>(drum_id),
            }
        });
        if let Some((lane, path)) = drum {
            if lane == midi_destination {
                self.m_assign_midi_sample(&path, self.arr.sel_lane, None);
                if self.arr.msg.is_empty() {
                    ui.data_mut(|d| d.insert_temp(egui::Id::new("m_browser_open"), false));
                }
            } else {
                self.lib.assign(&mut self.project, lane, &path);
                self.arr.msg.clone_from(&self.lib.message);
            }
        }
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
            match self.lib.insert_audio_clip(&mut self.project, &path, Some(self.arr.sel_lane), self.arr.snap) {
                Ok(index) => {
                    self.m_fit_audio_clip_to_grid(index);
                    self.arr.sel_clip = Some(index);
                    let clip = self.project.arrangement.clips[index];
                    self.arr.sel_lane = clip.lane;
                    self.project.arrangement.tracks[clip.lane].audio_brush = Some(clip.pattern);
                    self.arr.msg.clear();
                    ui.data_mut(|d| {
                        d.insert_temp(egui::Id::new("m_browser_open"), false);
                        d.insert_temp(egui::Id::new("m_playlist_reveal_tick"), clip.start);
                        d.insert_temp(egui::Id::new("m_playlist_reveal_track"), clip.lane);
                    });
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
        if self.project.arrangement.tracks.get(lane).is_some_and(|track| track.playlist_locked) {
            return;
        }
        if let browser::DragItem::Sample(path) = item {
            if self.project.arrangement.tracks.get(lane).is_some_and(|track| track.kind == crate::arrangement::TrackKind::Midi) {
                self.m_assign_midi_sample(path, lane, Some(tick));
                return;
            }
        }
        self.lib.message.clear();
        let bpm = self.project.bpm;
        let before = self.project.arrangement.clips.len();
        if let Some(lane) = arrangement_ui::drop_item(&mut self.project, &mut self.lib, item, lane, tick, bpm) {
            self.arr.sel_lane = lane;
            self.arr.sel_clip = (self.project.arrangement.clips.len() > before).then_some(before);
            if let Some(index) = self.arr.sel_clip {
                self.m_fit_audio_clip_to_grid(index);
            }
            self.arr.msg.clone_from(&self.lib.message);
        } else {
            self.arr.msg = "No available track for this item.".into();
        }
    }

    fn m_assign_midi_sample(&mut self, path: &std::path::Path, lane: usize, tick: Option<u32>) {
        if !self.lib.assign_track_sampler(&mut self.project, lane, path) {
            self.arr.msg.clone_from(&self.lib.message);
            return;
        }
        self.arr.sel_lane = lane;
        self.arr.msg.clear();
        let Some(start) = tick else { return };
        if let Some(index) = self.project.arrangement.clips.iter().position(|clip| {
            clip.lane == lane && (clip.start..clip.end_tick()).contains(&start)
        }) {
            self.arr.sel_clip = Some(index);
            return;
        }
        let sampler = &self.project.arrangement.tracks[lane].sampler;
        let pitch = sampler.root;
        let duration = sampler.sample.as_ref().and_then(|sample| sample.data.as_ref()).map_or(TICKS_PER_STEP, |sample| {
            (sample.mono.len() as f64 / f64::from(sample.rate) * f64::from(self.project.bpm)
                * f64::from(TICKS_PER_BAR) / 240.0).ceil().clamp(1.0, f64::from(TICKS_PER_BAR * 16)) as u32
        });
        if arrangement_ui::new_midi_clip_at(&mut self.project, &mut self.arr, lane, start, duration) {
            if let Some(clip) = self.arr.sel_clip.and_then(|index| self.project.arrangement.clips.get(index)) {
                self.project.patterns[clip.pattern].notes.push(crate::piano_roll::Note {
                    start: 0, len: clip.len, pitch, vel: 100, track: Track::Sampler,
                    slide: false, chance: 100, pan: 0, mute: false,
                });
            }
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

    fn m_playlist_options(&mut self, ui: &mut Ui, rect: Rect) {
        let mut trigger = ui.new_child(egui::UiBuilder::new().id_salt("playlist_options").max_rect(rect));
        trigger.set_clip_rect(rect.intersect(ui.clip_rect()));
        let button = egui::Button::new("").frame(false).min_size(rect.size());
        let menu = egui::menu::menu_custom_button(&mut trigger, button, |ui| {
            self.m_playlist_options_content(ui, false);
        });
        let center = rect.center();
        let painter = trigger.painter();
        painter.circle_filled(center, 16.0, if menu.response.hovered() { selected_fill() } else { card_fill() });
        painter.circle_stroke(center, 16.0, Stroke::new(1.0, outline()));
        for x in [-6.0, 0.0, 6.0] {
            painter.circle_filled(center + vec2(x, 0.0), 1.4, ink());
        }
        menu.response.on_hover_text("Playlist options");
    }

    fn m_playlist_options_content(&mut self, ui: &mut Ui, sheet: bool) -> bool {
        let mut close = false;
        let selected = self.arr.sel_clip.filter(|&index| index < self.project.arrangement.clips.len());
        let button = |ui: &Ui, label| {
            let width = if sheet { ui.available_width() } else { 0.0 };
            egui::Button::new(label).min_size(vec2(width, 0.0))
        };
        ui.set_min_width(176.0);
        let max_height = if sheet {
            ui.available_height()
        } else {
            ui.available_height().min((ui.ctx().screen_rect().height() * 0.50).min(330.0))
        };
        mobile_scroll(egui::ScrollArea::vertical()).max_height(max_height)
            .show(ui, |ui| {
                if ui.add_enabled(!self.undo.is_empty(), button(ui, "Undo")).clicked() {
                    self.undo();
                    self.arr.sel_clip = None;
                    self.arr.sel.clear();
                    close = true;
                    ui.close_menu();
                    return;
                }
                egui::CollapsingHeader::new("Playlist").default_open(true).show(ui, |ui| {
                    if portrait_view(ui) {
                        let id = egui::Id::new("m_playlist_vertical");
                        let mut vertical = ui.data(|d| d.get_temp::<bool>(id)).unwrap_or(true);
                        let changed = if sheet {
                            let state = if vertical { "On" } else { "Off" };
                            if option_row(ui, "Vertical timeline", state).clicked() {
                                vertical = !vertical;
                                true
                            } else {
                                false
                            }
                        } else {
                            ui.checkbox(&mut vertical, "Vertical timeline").changed()
                        };
                        if changed {
                            ui.data_mut(|d| d.insert_temp(id, vertical));
                        }
                    }
                    let snap = grid_label(self.arr.snap);
                    let mut grid_picker = |ui: &mut Ui, selected: &str| {
                        egui::ComboBox::from_id_salt("playlist_grid").selected_text(selected)
                            .show_ui(ui, |ui| {
                                for (step, name) in GRID_VALUES.into_iter().chain([(1, "Off")]) {
                                    let color = if self.arr.snap == step { on_accent() } else { ink() };
                                    ui.selectable_value(&mut self.arr.snap, step, RichText::new(name).color(color));
                                }
                            });
                    };
                    if sheet {
                        ui.horizontal(|ui| {
                            ui.label("Grid");
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| grid_picker(ui, snap));
                        });
                    } else {
                        grid_picker(ui, &format!("Grid · {snap}"));
                    }
                    if self.project.loop_on && ui.add(button(ui, "Export loop · WAV")).clicked() {
                        let start = self.project.loop_start.saturating_mul(TICKS_PER_BAR);
                        let end = start.saturating_add(self.project.loop_bars.max(1).saturating_mul(TICKS_PER_BAR));
                        self.m_export.open = true;
                        self.m_start_export_range(false, Some((start, end)));
                        close = true;
                        ui.close_menu();
                    }
                    if self.project.loop_on && ui.add(button(ui, "Clear loop")).clicked() {
                        self.project.loop_on = false;
                        self.publish_edits();
                        close = true;
                        ui.close_menu();
                    }
                    for (mute, label) in [(false, "Enable all tracks"), (true, "Disable all tracks")] {
                        if ui.add(button(ui, label)).clicked() {
                            self.m_playlist_undo_step();
                            for track in &mut self.project.arrangement.tracks {
                                track.mute = mute;
                                track.solo = false;
                            }
                            self.publish_edits();
                            close = true;
                            ui.close_menu();
                        }
                    }
                });
                egui::CollapsingHeader::new("Clips").show(ui, |ui| {
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        let selection = self.arr.selection();
                        if ui.add_enabled(!selection.is_empty(), button(ui, "Render selection")).clicked() {
                            ui.data_mut(|d| d.insert_temp(egui::Id::new("render_selection_job"), selection));
                            close = true;
                            ui.close_menu();
                        }
                    }
                    let Some(index) = selected else {
                        ui.label("Select a clip on the playlist.");
                        return;
                    };
                    if ui.add(button(ui, "Open clip")).clicked() {
                        self.m_open_playlist_clip(ui.ctx(), index);
                        close = true;
                        ui.close_menu();
                    }
                    if ui.add(button(ui, "Duplicate")).clicked() {
                        if let Some(copy) = arrangement_ui::duplicate_clip(&mut self.project, index) {
                            self.arr.sel_clip = Some(copy);
                            self.arr.sel.clear();
                        } else {
                            self.arr.msg = "No room to duplicate this clip.".into();
                        }
                        close = true;
                        ui.close_menu();
                    }
                    let clip = self.project.arrangement.clips[index];
                    let step = self.arr.snap.max(1);
                    for (label, len) in [
                        ("Extend by grid step", clip.len.saturating_add(step)),
                        ("Shorten by grid step", clip.len.saturating_sub(step).max(1)),
                    ] {
                        let resized = crate::arrangement::Clip { len, ..clip };
                        let valid = len != clip.len && self.project.arrangement.can_place(&resized, Some(index));
                        if ui.add_enabled(valid, button(ui, label)).clicked() {
                            self.project.arrangement.clips[index] = resized;
                            close = true;
                            ui.close_menu();
                        }
                    }
                });
                egui::CollapsingHeader::new("Audio").show(ui, |ui| {
                    if ui.add(button(ui, "Sounds & import")).clicked() {
                        ui.data_mut(|d| {
                            d.remove::<(usize, u32)>(egui::Id::new("m_audio_clip_target"));
                            d.insert_temp(egui::Id::new("m_browser_plugins"), false);
                            d.insert_temp(egui::Id::new("m_browser_open"), true);
                        });
                        close = true;
                        ui.close_menu();
                    }
                    if let Some(index) = selected.filter(|&index| self.project.arrangement.clips[index].audio) {
                        if ui.add(button(ui, "Audio clip settings")).clicked() {
                            self.m_open_playlist_clip(ui.ctx(), index);
                            close = true;
                            ui.close_menu();
                        }
                    }
                    if ui.add(button(ui, "Export WAV")).clicked() {
                        self.m_export.open = true;
                        self.m_export.song = true;
                        close = true;
                        ui.close_menu();
                    }
                });
            });
        close
    }

    fn m_open_playlist_clip(&mut self, ctx: &egui::Context, index: usize) {
        let clip = &self.project.arrangement.clips[index];
        self.arr.sel_lane = clip.lane;
        self.arr.sel_clip = Some(index);
        if !clip.audio && clip.pattern < self.project.patterns.len() {
            self.project.current_pattern = clip.pattern;
        }
        ctx.data_mut(|d| {
            d.remove::<bool>(egui::Id::new("m_track_sheet_open"));
            d.remove::<usize>(egui::Id::new("m_midi_clip_settings"));
            d.insert_temp(egui::Id::new("m_clip_sheet_selection"), index);
        });
    }

    /// Tap a track name or clip to open its editor sheet.
    fn m_playlist(&mut self, ui: &mut Ui) {
        if !self.arr.msg.is_empty() {
            ui.colored_label(RED, &self.arr.msg);
        }
        // Full view keeps the edit tools accessible when the regular dock is hidden.
        let docked = ui.data(|d| d.get_temp::<u64>(egui::Id::new(PLAYLIST_DOCK_PASS)))
            == Some(ui.ctx().cumulative_pass_nr());
        if !docked {
            let viewport = ui.available_rect_before_wrap();
            egui::Area::new(egui::Id::new("m_playlist_floating_tools"))
                .order(egui::Order::Foreground).movable(false)
                .fixed_pos(pos2(viewport.left() + 8.0, (viewport.bottom() - 58.0).max(viewport.top())))
                .show(ui.ctx(), |ui| {
                    mobile_visuals(ui);
                    Frame::none().fill(surface_fill()).stroke(Stroke::new(1.0, surface_edge()))
                        .rounding(14.0).inner_margin(Margin::symmetric(5.0, 3.0)).show(ui, |ui| {
                            ui.horizontal(|ui| self.m_playlist_tool_buttons(ui, false));
                        });
                });
        }
        if ui.input(|input| !input.pointer.any_down() && !input.pointer.any_released()) {
            ui.data_mut(|d| d.remove::<bool>(egui::Id::new(CLIP_MENU_DISMISSED)));
        }
        let select_id = egui::Id::new("m_playlist_select");
        let mut select_mode = ui.data(|d| d.get_temp::<bool>(select_id)).unwrap_or(false);
        if self.arr.auto_edit {
            select_mode = false;
        }
        ui.data_mut(|d| d.insert_temp(select_id, select_mode));
        if portrait_view(ui) && !self.arr.auto_edit && ui.data(|d| {
            d.get_temp::<bool>(egui::Id::new("m_playlist_vertical")).unwrap_or(true)
        }) {
            self.m_vertical_playlist(ui, select_mode, docked);
            return;
        }
        let ruler = TOUCH;
        let automation = self.arr.auto_edit;
        let width_id = egui::Id::new("m_playlist_bar_width");
        let previous_bar_w = ui.data(|d| d.get_temp::<f32>(width_id)).unwrap_or(64.0);
        let height_id = egui::Id::new("m_playlist_lane_height");
        let tracks = self.project.arrangement.tracks.len();
        if tracks == 0 {
            ui.label(RichText::new("No tracks yet. Open a template in Projects.").color(muted()));
            return;
        }
        if self.arr.sel_lane >= tracks {
            self.arr.sel_lane = 0;
        }
        let automation_end = self.project.arrangement.tracks.iter()
            .flat_map(|track| &track.fx_automation).map(|clip| clip.end()).max().unwrap_or(0);
        let bars = self.project.arrangement.song_end_bar().max(automation_end.div_ceil(TICKS_PER_BAR)).max(
            if self.project.loop_on { self.project.loop_start.saturating_add(self.project.loop_bars) } else { 0 },
        ).saturating_add(8).max(16);
        let pos = self.shared.position.load(Ordering::Relaxed);
        let playing = self.playing() && self.project.song_mode;
        let mut active_lanes = vec![None; tracks];
        if playing {
            for clip in &self.project.arrangement.clips {
                if let Some(progress) = playlist_clip_playback(&self.project, clip, pos) {
                    if let Some(lane) = active_lanes.get_mut(clip.lane) {
                        *lane = Some(progress);
                    }
                }
            }
        }
        let add_lane = egui::DragAndDrop::has_payload_of_type::<browser::DragItem>(ui.ctx());
        let panel_id = egui::Id::new("m_playlist_track_names");
        let portrait = portrait_view(ui);
        let panel_open = ui.data(|d| d.get_temp::<bool>(panel_id)).unwrap_or(true);
        let panel_width: f32 = if portrait { 0.0 } else if panel_open { 116.0 } else { 28.0 };
        let show_names = !portrait && panel_open;
        let show_master = show_names && !add_lane;
        let extra_h = if show_master { 92.0 } else { 0.0 };
        let reveal_id = egui::Id::new("m_playlist_reveal_track");
        let reveal_lane = ui.data_mut(|d| d.remove_temp::<usize>(reveal_id));
        let (viewport, _) = ui.allocate_exact_size(ui.available_size(), Sense::hover());
        let mut grid = viewport;
        grid.max.x -= panel_width.min((viewport.width() - 1.0).max(0.0));
        grid.min.y += ruler.min((viewport.height() - 1.0).max(0.0));
        let (pinching, zoom) = grid_pinch(ui, egui::Id::new("m_playlist_pinch"), grid);
        let previous_height = ui.data(|d| d.get_temp::<f32>(height_id)).unwrap_or(52.0);
        let height = zoom.filter(|(axis, _, _)| *axis == 1)
            .map_or(previous_height, |(_, factor, _)| (previous_height * factor).clamp(42.0, 140.0));
        let bar_w = zoom.filter(|(axis, _, _)| *axis == 0)
            .map_or(previous_bar_w, |(_, factor, _)| (previous_bar_w * factor).clamp(28.0, 240.0));
        if zoom.is_some() {
            ui.data_mut(|d| {
                d.insert_temp(height_id, height);
                d.insert_temp(width_id, bar_w);
            });
        }
        let lane_h = if automation { height.max(84.0) } else { height };
        let previous_lane_h = if automation { previous_height.max(84.0) } else { previous_height };
        let arrangement = &self.project.arrangement;
        let mut rows = Vec::new();
        let mut track_rows = vec![None; tracks];
        for (lane, track) in arrangement.tracks.iter().enumerate() {
            if let Some(group) = arrangement.groups.iter().find(|group| group.id != 0 && group.id == track.group) {
                if arrangement.tracks.iter().position(|track| track.group == group.id) != Some(lane) {
                    continue;
                }
                rows.push(PlaylistRow::Group(group.id));
                if !group.collapsed {
                    for (member, track) in arrangement.tracks.iter().enumerate().skip(lane) {
                        if track.group == group.id {
                            track_rows[member] = Some(rows.len());
                            rows.push(PlaylistRow::Track(member));
                            rows.extend((0..track.fx_automation.len()).map(|index| PlaylistRow::FxAutomation(member, index)));
                        }
                    }
                }
            } else {
                track_rows[lane] = Some(rows.len());
                rows.push(PlaylistRow::Track(lane));
                rows.extend((0..track.fx_automation.len()).map(|index| PlaylistRow::FxAutomation(lane, index)));
            }
        }
        let row_count = rows.len();
        let full_h = (row_count + usize::from(add_lane || show_master)) as f32 * lane_h + extra_h;
        let mut grid_ui = ui.new_child(egui::UiBuilder::new().max_rect(grid));
        grid_ui.set_clip_rect(grid.intersect(ui.clip_rect()));
        let mut selection = None;
        let mut delete_clip = None;
        let mut duplicate: Option<usize> = None;
        let mut move_clip = None;
        let mut fade_clip = None;
        let mut remove_fx_automation = None;
        let clip_menu_id = egui::Id::new("m_playlist_clip_actions");
        let mut select_master = false;
        let mut open_mixer = false;
        let mut dropped_item = None;
        let mut dropped_effect = None;
        let scroll = mobile_scroll(egui::ScrollArea::both()).id_salt("m_playlist")
            .drag_to_scroll(!pinching && !select_mode).max_height(grid.height()).auto_shrink([false, false]);
        let scroll = if let Some(row) = reveal_lane.and_then(|lane| track_rows.get(lane).copied().flatten()) {
            scroll.vertical_scroll_offset((row as f32 * lane_h - grid.height() * 0.35).max(0.0))
        } else {
            scroll
        };
        let follow_id = egui::Id::new("m_playlist_offset_x");
        let shown_x = ui.data(|d| d.get_temp::<f32>(follow_id)).unwrap_or(0.0);
        let (rewind, reveal_tick) = ui.data_mut(|d| (
            d.remove_temp::<bool>(egui::Id::new("playlist_return_to_start")).unwrap_or(false),
            d.remove_temp::<u32>(egui::Id::new("m_playlist_reveal_tick")),
        ));
        let scroll = if rewind {
            scroll.horizontal_scroll_offset(0.0)
        } else if let Some(tick) = reveal_tick {
            let x = tick as f32 / TICKS_PER_BAR as f32 * bar_w;
            if x < shown_x || x > shown_x + grid.width() * 0.85 {
                scroll.horizontal_scroll_offset((x - grid.width() * 0.15).max(0.0))
            } else {
                scroll
            }
        } else if let Some((_, _, center)) = zoom.filter(|_| bar_w != previous_bar_w) {
            let anchor = center.x - grid.left();
            let offset = (shown_x + anchor) * bar_w / previous_bar_w - anchor;
            scroll.horizontal_scroll_offset(offset.max(0.0))
        } else {
            scroll
        };
        // a swipe on the right column (track names, + MIDI / + Audio, Master) scrolls the lanes too
        let names_scroll_id = egui::Id::new("m_playlist_names_scroll");
        let scroll = match ui.data_mut(|d| d.remove_temp::<f32>(names_scroll_id)) {
            Some(y) => scroll.vertical_scroll_offset(y.clamp(0.0, (full_h - grid.height()).max(0.0))),
            None => scroll,
        };
        let zoom_y_id = follow_id.with("vertical");
        let scroll = if let Some((_, _, center)) = zoom.filter(|_| lane_h != previous_lane_h) {
            let shown_y = ui.data(|d| d.get_temp::<f32>(zoom_y_id)).unwrap_or_default();
            let anchor = center.y - grid.top();
            scroll.vertical_scroll_offset(((shown_y + anchor) * lane_h / previous_lane_h - anchor).max(0.0))
        } else {
            scroll
        };
        let scroll = scroll.show(&mut grid_ui, |ui| {
            let (rect, resp) = ui.allocate_exact_size(
                vec2((bars as f32 * bar_w).max(grid.width()), full_h.max(grid.height())),
                if select_mode && !pinching { Sense::click_and_drag() } else { Sense::click() });
            let p = ui.painter_at(rect);
            let gx = rect.left();
            let gy = rect.top();
            let box_id = resp.id.with("clip_marquee");
            if select_mode && !pinching && resp.drag_started() {
                if let Some(origin) = ui.input(|input| input.pointer.press_origin()) {
                    ui.data_mut(|d| d.insert_temp(box_id, origin));
                }
            }
            let box_area = if select_mode && !pinching && (resp.dragged() || resp.drag_stopped()) {
                let origin = ui.data(|d| d.get_temp::<Pos2>(box_id));
                origin.zip(resp.interact_pointer_pos())
                    .map(|(start, end)| Rect::from_two_pos(start, end).intersect(grid))
            } else {
                None
            };
            let mut box_hits = Vec::new();
            let mut selected_tap = None;
            p.rect_filled(rect, 0.0, p_bg());
            for (index, item) in rows.iter().enumerate() {
                let y = gy + index as f32 * lane_h;
                let r = Rect::from_min_size(pos2(rect.left(), y), vec2(rect.width(), lane_h));
                let PlaylistRow::Track(lane) = *item else {
                    p.rect_filled(r, 0.0, card_fill());
                    p.line_segment([r.left_bottom(), r.right_bottom()], Stroke::new(1.0, outline()));
                    continue;
                };
                let track = &self.project.arrangement.tracks[lane];
                let audio = track.kind == crate::arrangement::TrackKind::Audio;
                let base_fill = if audio {
                    mobile_color(0xFF1B1A2B, 0xFF232A2B, 0xFFE7E2F7, 0xFF222225, 0xFF1B2416)
                } else {
                    mobile_color(0xFF191725, 0xFF25282D, 0xFFEEEAFB, 0xFF1C1C1F, 0xFF181F14)
                };
                let row_fill = if lane == self.arr.sel_lane {
                    tint(base_fill, selected_fill(), 0.24)
                } else {
                    base_fill
                };
                p.rect_filled(r, 0.0, row_fill);
                if lane == self.arr.sel_lane {
                    p.rect_filled(Rect::from_min_size(r.min, vec2(3.0, r.height())), 0.0, lane_col(lane));
                }
                p.line_segment([r.left_top(), r.right_top()], Stroke::new(1.0, pick(0xFF535E68, 0xFFA79CD8)));
            }
            for (index, item) in rows.iter().enumerate() {
                if let PlaylistRow::FxAutomation(lane, clip) = *item {
                    let y = gy + index as f32 * lane_h;
                    let area = Rect::from_min_size(pos2(gx, y), vec2(rect.width(), lane_h));
                    if self.m_fx_automation_clip(ui, area, bar_w, lane, clip, pinching) {
                        remove_fx_automation = Some((lane, clip));
                    }
                }
            }
            if add_lane {
                let y = gy + row_count as f32 * lane_h;
                p.line_segment([pos2(rect.left(), y), pos2(rect.right(), y)], Stroke::new(0.6, grid_line()));
                p.text(pos2(rect.left() + 10.0, y + lane_h * 0.5), Align2::LEFT_CENTER,
                    "+ New track", FontId::proportional(12.0), muted());
            }
            let snap = self.arr.snap.max(1);
            if snap > 1 && snap < TICKS_PER_BAR {
                let pixels_per_tick = bar_w / TICKS_PER_BAR as f32;
                let division_ticks = visible_grid_step(snap, pixels_per_tick * 0.65);
                let gap = division_ticks as f32 * pixels_per_tick;
                if division_ticks < TICKS_PER_BAR {
                    let first = ((ui.clip_rect().left() - gx) / gap).ceil().max(0.0) as u32;
                    let last = ((ui.clip_rect().right().min(rect.right()) - gx) / gap).floor().max(0.0) as u32;
                    for division in first..=last {
                        if (division * division_ticks) % TICKS_PER_BAR == 0 {
                            continue;
                        }
                        let x = gx + division as f32 * gap;
                        p.line_segment([pos2(x, gy), pos2(x, gy + full_h)],
                            Stroke::new(0.65, pick(0xFF383F47, 0xFFCBC3EA)));
                    }
                }
            }
            for b in 0..=bars {
                let x = gx + b as f32 * bar_w;
                let strong = b % 4 == 0;
                p.line_segment([pos2(x, gy), pos2(x, gy + full_h)],
                    Stroke::new(if strong { 1.6 } else { 1.15 },
                        if strong { pick(0xFF98A6B3, 0xFF6E62A3) } else { pick(0xFF677681, 0xFF9388C4) }));
            }
            for (i, c) in self.project.arrangement.clips.iter().enumerate() {
                let Some(row) = track_rows.get(c.lane).copied().flatten() else { continue; };
                let x0 = gx + c.start as f32 / TICKS_PER_BAR as f32 * bar_w;
                let w = (c.len as f32 / TICKS_PER_BAR as f32 * bar_w).max(6.0);
                let hit = Rect::from_min_size(pos2(x0, gy + row as f32 * lane_h), vec2(w, lane_h));
                let r = hit.shrink2(vec2(1.5, 5.0));
                // the right edge is a handle of its own: drag it to make the clip longer or shorter
                let edge_w = (w * 0.36).clamp(5.0, 18.0).min(w * 0.6);
                let edge_rect = Rect::from_min_max(pos2(hit.right() - edge_w, hit.top()), hit.right_bottom());
                let locked = self.project.arrangement.tracks[c.lane].playlist_locked;
                let clip = ui.interact(hit, resp.id.with(("clip", i)), if automation || locked { Sense::hover() }
                    else if select_mode { Sense::click() } else { Sense::click_and_drag() })
                    .on_hover_cursor(egui::CursorIcon::Grab);
                let resize_id = resp.id.with(("clip_resize", i));
                let fade_id = resp.id.with(("clip_fade", i));
                let grab_id = clip.id.with("grab_x");
                if pinching || automation || clip.drag_started() {
                    ui.data_mut(|d| {
                        d.remove::<f32>(resize_id);
                        d.remove::<(f32, bool)>(fade_id);
                        d.remove::<f32>(grab_id);
                    });
                }
                let show_fades = !select_mode && !locked && self.arr.sel_clip == Some(i) && w >= 10.0;
                if !pinching && !automation && clip.drag_started() {
                    if let Some(origin) = ui.input(|input| input.pointer.press_origin()) {
                        let fade_zone = (w * 0.5).min(27.0);
                        if show_fades && origin.y <= hit.top() + hit.height().min(25.0)
                            && (origin.x <= hit.left() + fade_zone || origin.x >= hit.right() - fade_zone)
                        {
                            let out = origin.x > hit.center().x;
                            ui.data_mut(|d| d.insert_temp(fade_id, (origin.x, out)));
                        } else if edge_rect.contains(origin) {
                            ui.data_mut(|d| d.insert_temp(resize_id, origin.x));
                        }
                    }
                }
                let mut preview_fade = *c;
                let fade_drag = ui.data(|d| d.get_temp::<(f32, bool)>(fade_id));
                if let Some((origin, out)) = fade_drag {
                    if let Some(point) = clip.interact_pointer_pos() {
                        let delta = if out { origin - point.x } else { point.x - origin };
                        let field = if out { &mut preview_fade.fade_out } else { &mut preview_fade.fade_in };
                        let ticks = delta / bar_w * TICKS_PER_BAR as f32;
                        *field = (*field as f32 + ticks).round().clamp(0.0, (c.len / 2) as f32) as u32;
                        if clip.drag_stopped() && preview_fade != *c {
                            fade_clip = Some((i, preview_fade));
                        }
                    }
                    if clip.drag_stopped() {
                        ui.data_mut(|d| d.remove::<(f32, bool)>(fade_id));
                    }
                }
                let resize_from = ui.data(|d| d.get_temp::<f32>(resize_id));
                let resizing = resize_from.is_some();
                let col = lane_col(c.lane);
                let pastel = if c.audio { argb(0xFFD6EEF5) } else { argb(0xFFDED1F3) };
                let fill = tint(col, pastel, if MobileTheme::current() == MobileTheme::Day { 0.42 } else { 0.38 });
                let fill = if c.mute { tint(fill, card_fill(), 0.45) } else { fill };
                p.rect_filled(r, 5.0, fill);
                if let Some(progress) = playing.then(|| playlist_clip_playback(&self.project, c, pos)).flatten() {
                    paint_playlist_playback(&p, r, col, progress, pos, false);
                }
                if r.is_positive() {
                    let fade_y = r.top() + 2.0;
                    let fade_height = (r.height() * 0.45).min(20.0);
                    let tint = argb(0xFF253242);
                    for (out, length) in [(false, preview_fade.fade_in), (true, preview_fade.fade_out)] {
                        let edge = if out { r.right() } else { r.left() };
                        let direction = if out { -1.0 } else { 1.0 };
                        if length > 0 {
                            let inner = edge + direction * r.width() * (length.min(c.len) as f32 / c.len.max(1) as f32);
                            let corner = pos2(edge, fade_y + fade_height);
                            let top = pos2(inner, fade_y);
                            p.add(Shape::convex_polygon(vec![pos2(edge, fade_y), corner, top],
                                tint.gamma_multiply(0.35), Stroke::NONE));
                            p.line_segment([corner, top], Stroke::new(1.2, tint));
                        }
                        if show_fades {
                            let x = edge + direction * (r.width() * 0.2).min(7.0);
                            p.circle_filled(pos2(x, fade_y + 4.0), 3.2, tint);
                        }
                    }
                }
                let in_box = !locked && box_area.is_some_and(|area| area.intersects(r));
                if in_box {
                    box_hits.push(i);
                }
                if in_box || (box_area.is_none() && self.arr.is_sel(i)) {
                    p.rect_stroke(r.shrink(1.0), 5.0, Stroke::new(1.8, pick(0xFFF2F6FF, 0xFF3B2F7A)));
                }
                let pattern = (!c.audio).then(|| self.project.patterns.get(c.pattern)).flatten();
                let notes_visible = pattern.is_some_and(|pattern| !pattern.notes.is_empty()) && r.height() >= 32.0;
                let name = if c.audio {
                    self.project.audio_pool.get(c.pattern).map(|sample| sample.name.as_str())
                } else {
                    pattern.map(|pattern| pattern.name.as_str())
                };
                let content = p.with_clip_rect(r.intersect(ui.clip_rect()));
                if let Some(name) = name {
                    let y = if notes_visible { r.top() + 14.0 } else { r.center().y };
                    let visible = r.shrink2(vec2(5.0, 0.0)).intersect(ui.clip_rect());
                    playlist_label(&content, visible, y, name, argb(0xFF172227));
                }
                if let Some(pattern) = pattern.filter(|_| notes_visible) {
                    let notes_rect = Rect::from_min_max(r.min + vec2(3.0, 24.0), r.max - vec2(5.0, 3.0));
                    if notes_rect.is_positive() {
                        playlist_midi_preview(&content, notes_rect, pattern, c.offset, c.len);
                    }
                }
                // grip on the right edge of the selected clip
                if r.width() > 8.0 {
                    let grip = Rect::from_center_size(pos2(r.right() - 4.0, r.center().y), vec2(3.0, (r.height() * 0.5).max(8.0)));
                    p.rect_filled(grip, 1.5, argb(0xFF253242).gamma_multiply(0.75));
                }
                if select_mode {
                    if !pinching && !locked && clip.clicked() {
                        selected_tap = Some(i);
                    }
                    continue;
                }
                if pinching || automation || locked || fade_drag.is_some() {
                    continue;
                }
                if resizing && (clip.dragged() || clip.drag_stopped()) {
                    if let (Some(origin_x), Some(point)) = (resize_from, clip.interact_pointer_pos()) {
                        let snap = self.arr.snap.max(1);
                        let delta = (point.x - origin_x) / bar_w * TICKS_PER_BAR as f32;
                        let target = (c.end_tick() as f64 + f64::from(delta)).max(0.0);
                        let end = ((target / f64::from(snap)).round() as u32)
                            .max(c.start / snap + 1).saturating_mul(snap);
                        let len = end.saturating_sub(c.start).max(1);
                        let candidate = crate::arrangement::Clip { len, ..*c };
                        let valid = !self.project.arrangement.clips.iter().enumerate()
                            .any(|(j, other)| i != j && candidate.overlaps(other));
                        let preview = Rect::from_min_size(pos2(x0, gy + row as f32 * lane_h),
                            vec2(len as f32 / TICKS_PER_BAR as f32 * bar_w, lane_h)).shrink2(vec2(1.5, 5.0));
                        p.rect_stroke(preview, 4.0, Stroke::new(2.0, if valid { p_accent() } else { RED }));
                        if valid && clip.drag_stopped() {
                            move_clip = Some((i, candidate));
                        }
                    }
                    if clip.drag_stopped() {
                        ui.data_mut(|d| d.remove::<f32>(resize_id));
                    }
                    continue;
                }

                let open_editor = if portrait { clip.double_clicked() } else { clip.long_touched() && !c.audio };
                let open_menu = if portrait {
                    clip.long_touched() || clip.secondary_clicked()
                } else {
                    clip.clicked() || clip.secondary_clicked()
                };
                if open_editor && !(portrait && open_menu) {
                    ui.data_mut(|d| d.remove::<(usize, Pos2)>(clip_menu_id));
                    selection = Some((c.lane, Some(i), Some(c.start), true));
                } else if open_menu {
                    let pointer = clip.interact_pointer_pos().unwrap_or(hit.intersect(ui.clip_rect()).center());
                    ui.data_mut(|d| d.insert_temp(clip_menu_id, (i, pointer)));
                    selection = Some((c.lane, Some(i), Some(c.start), false));
                } else if clip.clicked() {
                    selection = Some((c.lane, Some(i), Some(c.start), false));
                }

                if clip.drag_started() {
                    if let Some(origin) = ui.input(|input| input.pointer.press_origin()) {
                        ui.data_mut(|d| d.insert_temp(grab_id, origin.x - x0));
                    }
                }
                if clip.dragged() || clip.drag_stopped() {
                    let grab = ui.data(|d| d.get_temp::<f32>(grab_id));
                    let point = clip.interact_pointer_pos().filter(|pt| grid.contains(*pt) && pt.y >= gy);
                    if let (Some(grab), Some(point)) = (grab, point) {
                        let target_row = ((point.y - gy) / lane_h) as usize;
                        let Some(PlaylistRow::Track(lane)) = rows.get(target_row).copied() else {
                            if clip.drag_stopped() {
                                ui.data_mut(|d| d.remove::<f32>(grab_id));
                            }
                            continue;
                        };
                        let snap = self.arr.snap.max(1);
                        let tick = (point.x - gx - grab).max(0.0) / bar_w * TICKS_PER_BAR as f32;
                        let start = (tick / snap as f32).round() as u32 * snap;
                        let candidate = crate::arrangement::Clip { lane, start, ..*c };
                        let valid = lane < tracks
                            && !self.project.arrangement.tracks[lane].playlist_locked
                            && self.project.arrangement.tracks[lane].kind == self.project.arrangement.tracks[c.lane].kind
                            && !self.project.arrangement.clips.iter().enumerate()
                                .any(|(j, other)| i != j && candidate.overlaps(other));
                        let preview = Rect::from_min_size(
                            pos2(gx + start as f32 / TICKS_PER_BAR as f32 * bar_w, gy + target_row as f32 * lane_h),
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
            if let Some(row) = track_rows[self.arr.sel_lane].filter(|_| automation && !self.arr.auto_master
                && !self.project.arrangement.tracks[self.arr.sel_lane].playlist_locked)
            {
                let area = Rect::from_min_size(pos2(gx, gy + row as f32 * lane_h), vec2(rect.width(), lane_h));
                self.m_playlist_automation(ui, area, bar_w, bars, pinching, false);
            }
            if playing || pos != 0 {
                let x = gx + pos as f32 / TICKS_PER_BAR as f32 * bar_w;
                p.line_segment([pos2(x, gy), pos2(x, gy + full_h)], Stroke::new(2.0, p_accent()));
            }
            if let Some(point) = ui.input(|input| input.pointer.interact_pos()).filter(|p| rect.contains(*p) && p.y >= gy && p.y < gy + (row_count + usize::from(add_lane)) as f32 * lane_h) {
                let index = ((point.y - gy) / lane_h) as usize;
                let lane = match rows.get(index) {
                    Some(PlaylistRow::Track(lane)) => Some(*lane),
                    None if add_lane && index == row_count => Some(tracks),
                    _ => None,
                };
                if let Some(lane) = lane.filter(|&lane| lane == tracks || !self.project.arrangement.tracks[lane].playlist_locked) {
                    let tick = ((point.x - gx).max(0.0) / bar_w * TICKS_PER_BAR as f32) as u32;
                    let snap = self.arr.snap.max(1);
                    let sample = egui::DragAndDrop::payload::<browser::DragItem>(ui.ctx()).filter(|item| {
                        !matches!(item.as_ref(), browser::DragItem::Instrument(_))
                            || lane == tracks
                            || self.project.arrangement.tracks[lane].kind == crate::arrangement::TrackKind::Midi
                    });
                    let effect = (lane < tracks).then(|| egui::DragAndDrop::payload::<crate::fx::FxKind>(ui.ctx())).flatten();
                    if ui.rect_contains_pointer(rect) && (sample.is_some() || effect.is_some()) {
                        let row = Rect::from_min_size(pos2(rect.left(), gy + index as f32 * lane_h), vec2(rect.width(), lane_h));
                        p.rect_stroke(row.shrink(1.0), 6.0, Stroke::new(1.5, p_accent()));
                        if ui.input(|input| input.pointer.primary_released()) {
                            if sample.is_some() {
                                dropped_item = egui::DragAndDrop::take_payload::<browser::DragItem>(ui.ctx())
                                    .map(|item| (item, lane, tick / snap * snap));
                            } else if effect.is_some() {
                                dropped_effect = egui::DragAndDrop::take_payload::<crate::fx::FxKind>(ui.ctx()).map(|kind| (*kind, lane));
                            }
                        }
                    }
                }
            }
            if let Some(area) = box_area {
                p.rect_filled(area, 2.0, p_accent().gamma_multiply(0.16));
                p.rect_stroke(area, 2.0, Stroke::new(1.2, p_accent()));
                if resp.drag_stopped() {
                    self.arr.sel = box_hits.into_iter().collect();
                    self.arr.sel_clip = None;
                }
            } else if let Some(index) = selected_tap {
                if self.arr.is_sel(index) {
                    self.arr.sel.remove(&index);
                    if self.arr.sel_clip == Some(index) {
                        self.arr.sel_clip = None;
                    }
                } else {
                    self.arr.sel.insert(index);
                }
            } else if select_mode && !pinching && resp.clicked() {
                self.arr.sel.clear();
                self.arr.sel_clip = None;
            }
            if pinching || resp.drag_stopped() {
                ui.data_mut(|d| d.remove::<Pos2>(box_id));
            }
            // the tap that closed the clip action row only closes it: it must not also make a clip or open the browser
            let dismissed = resp.clicked() && ui.data_mut(|d| d.remove_temp::<bool>(egui::Id::new(CLIP_MENU_DISMISSED))).is_some();
            if !select_mode && !pinching && !automation && resp.clicked() && !dismissed {
                if let Some(pt) = resp.interact_pointer_pos().filter(|pt| pt.y >= gy) {
                    let row = ((pt.y - gy) / lane_h) as usize;
                    if let Some(PlaylistRow::Track(lane)) = rows.get(row).copied()
                        .filter(|item| matches!(item, PlaylistRow::Track(lane) if !self.project.arrangement.tracks[*lane].playlist_locked))
                    {
                        let tick = ((pt.x - gx).max(0.0) / bar_w * TICKS_PER_BAR as f32) as u32;
                        selection = Some((lane, None, Some(tick), false));
                    }
                }
            }
            rect.min
        });
        ui.data_mut(|d| {
            d.insert_temp(follow_id, scroll.state.offset.x);
            d.insert_temp(zoom_y_id, scroll.state.offset.y);
        });
        let offset_y = scroll.state.offset.y;
        let timeline = Rect::from_min_max(pos2(grid.left(), viewport.top()), pos2(grid.right(), grid.top()));
        let painter = ui.painter_at(timeline);
        painter.rect_filled(timeline, 0.0, surface_fill());
        painter.line_segment([timeline.left_bottom(), timeline.right_bottom()], Stroke::new(1.0, surface_edge()));
        let ruler_response = ui.interact(timeline, egui::Id::new("m_playlist_seek_ruler"), Sense::click_and_drag())
            .on_hover_text("Tap to seek · Drag to loop");
        let tick_at = |x: f32| {
            let x = x.clamp(timeline.left(), timeline.right());
            let tick = ((x - scroll.inner.x).max(0.0) / bar_w * TICKS_PER_BAR as f32) as u32;
            tick.min(bars.saturating_mul(TICKS_PER_BAR).saturating_sub(1))
        };
        let seek_id = ruler_response.id.with("move_playhead");
        if ruler_response.drag_started() {
            let marker = scroll.inner.x + pos as f32 / TICKS_PER_BAR as f32 * bar_w;
            let seek = playing && ui.input(|input| input.pointer.press_origin())
                .is_some_and(|origin| (origin.x - marker).abs() < 10.0);
            ui.data_mut(|d| d.insert_temp(seek_id, seek));
        }
        let seek_drag = ui.data(|d| d.get_temp::<bool>(seek_id)).unwrap_or(false);
        let seek_tick = ruler_response.interact_pointer_pos()
            .filter(|_| !pinching && ((ruler_response.clicked() && !ruler_response.long_touched())
                || (seek_drag && ruler_response.dragged())))
            .map(|point| tick_at(point.x));
        if !pinching && ruler_response.dragged() && !seek_drag {
            if let Some((origin, current)) = ui.input(|input| input.pointer.press_origin())
                .zip(ruler_response.interact_pointer_pos())
            {
                let a = tick_at(origin.x) / TICKS_PER_BAR;
                let b = tick_at(current.x) / TICKS_PER_BAR;
                let start = a.min(b);
                let length = (a.max(b) - start + 1).min(256);
                if !self.project.loop_on || self.project.loop_start != start || self.project.loop_bars != length {
                    self.project.loop_on = true;
                    self.project.loop_start = start;
                    self.project.loop_bars = length;
                    self.publish_edits();
                }
            }
        }
        if !pinching && ruler_response.drag_stopped() && !seek_drag && self.project.loop_on {
            self.m_seek(ui.ctx(), self.project.loop_start * TICKS_PER_BAR);
        }
        if !pinching && (ruler_response.secondary_clicked() || ruler_response.long_touched()) && self.project.loop_on {
            self.project.loop_on = false;
            self.publish_edits();
        }
        if self.project.loop_on {
            let left = scroll.inner.x + self.project.loop_start as f32 * bar_w;
            let right = left + self.project.loop_bars as f32 * bar_w;
            let range = Rect::from_min_max(pos2(left, timeline.top()), pos2(right, timeline.bottom())).intersect(timeline);
            if range.is_positive() {
                painter.rect_filled(range, 4.0, p_accent().gamma_multiply(0.25));
                painter.line_segment([range.left_bottom(), range.right_bottom()], Stroke::new(3.0, p_accent()));
            }
        }
        for bar in 0..bars {
            let x = scroll.inner.x + bar as f32 * bar_w;
            if !timeline.x_range().contains(x) {
                continue;
            }
            let strong = bar % 4 == 0;
            painter.line_segment([pos2(x, timeline.bottom() - 5.0), pos2(x, timeline.bottom())],
                Stroke::new(1.0, if strong { ink() } else { outline() }));
            let name = painter.layout_no_wrap((bar + 1).to_string(),
                FontId::proportional(if strong { 13.0 } else { 11.0 }), if strong { ink() } else { muted() });
            if x + 5.0 + name.size().x <= (x + bar_w - 2.0).min(timeline.right()) {
                let at = pos2(x + 5.0, timeline.center().y - name.size().y * 0.5);
                painter.galley(at, name, ink());
            }
        }
        if let Some(tick) = seek_tick.filter(|&tick| tick != pos) {
            self.m_seek(ui.ctx(), tick);
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(16));
        }
        let marker = seek_tick.unwrap_or(pos);
        if playing || marker != 0 || seek_tick.is_some() {
            let x = scroll.inner.x + marker as f32 / TICKS_PER_BAR as f32 * bar_w;
            if timeline.x_range().contains(x) {
                painter.add(Shape::convex_polygon(
                    vec![pos2(x - 4.0, timeline.bottom() - 8.0), pos2(x + 4.0, timeline.bottom() - 8.0), pos2(x, timeline.bottom())],
                    p_accent(), Stroke::NONE));
            }
        }
        let panel = Rect::from_min_max(pos2(grid.right(), viewport.top()), viewport.max);
        let painter = ui.painter_at(panel);
        painter.rect_filled(panel, 0.0, bar_fill());
        let names = Rect::from_min_max(pos2(panel.left(), grid.top()), panel.max);
        let options_id = panel_id.with("track_modal");
        let mut group_drop = None;
        let (drag_pos, released) = ui.input(|input| {
            let cancelled = input.events.iter().any(|event| matches!(event,
                egui::Event::Touch { phase: egui::TouchPhase::Cancel, .. }));
            (input.pointer.interact_pos(), input.pointer.primary_released() && !cancelled)
        });
        let track_drag = egui::DragAndDrop::payload::<PlaylistTrackDrag>(ui.ctx());
        let mut group_toggle = None;
        let mut name_pick: Option<(usize, bool)> = None;
        if show_names {
            let swipe = ui.interact(names.intersect(ui.clip_rect()), egui::Id::new("m_playlist_names_swipe"), Sense::drag());
            if track_drag.is_none() && swipe.dragged() && swipe.drag_delta().y != 0.0 {
                let y = offset_y - swipe.drag_delta().y;
                ui.data_mut(|d| d.insert_temp(names_scroll_id, y));
                ui.ctx().request_repaint();
            }
        }
        if show_names {
            let labels = painter.with_clip_rect(names.intersect(ui.clip_rect()));
            for (index, item) in rows.iter().copied().enumerate() {
                let top = scroll.inner.y + index as f32 * lane_h;
                let row = Rect::from_min_size(pos2(panel.left(), top), vec2(panel.width(), lane_h));
                let visible = row.intersect(names).intersect(ui.clip_rect());
                if !visible.is_positive() {
                    continue;
                }
                let lane = match item {
                    PlaylistRow::Track(lane) => lane,
                    PlaylistRow::FxAutomation(lane, clip) => {
                        let automation = &self.project.arrangement.tracks[lane].fx_automation[clip];
                        let id = egui::Id::new(("m_fx_automation_draw", lane, clip));
                        let drawing = ui.data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
                        let card = ui.interact(visible, id.with("card"), Sense::click())
                            .on_hover_text(&automation.label);
                        labels.rect_filled(row.shrink(2.0), 8.0, if drawing { selected_fill() } else { card_fill() });
                        labels.with_clip_rect(visible).text(row.left_top() + vec2(7.0, lane_h * 0.32),
                            Align2::LEFT_CENTER, &automation.label, FontId::proportional(10.0), ink());
                        labels.text(row.left_top() + vec2(7.0, lane_h * 0.72), Align2::LEFT_CENTER,
                            if drawing { "Done drawing" } else { "Draw curve" }, FontId::proportional(10.0), muted());
                        if card.clicked() && !self.project.arrangement.tracks[lane].playlist_locked {
                            ui.data_mut(|d| d.insert_temp(id, !drawing));
                        }
                        continue;
                    }
                    PlaylistRow::Group(id) => {
                        let group = self.project.arrangement.groups.iter().find(|group| group.id == id).unwrap();
                        let card = ui.interact(visible, panel_id.with(("folder", id)), Sense::click());
                        labels.rect_filled(row.shrink(2.0), 8.0, selected_fill());
                        labels.text(row.left_top() + vec2(8.0, lane_h * 0.33), Align2::LEFT_CENTER,
                            format!("{} {}", if group.collapsed { "›" } else { "⌄" }, group.name),
                            FontId::proportional(12.0), ink());
                        labels.text(row.left_top() + vec2(17.0, lane_h * 0.70), Align2::LEFT_CENTER,
                            format!("{} tracks", self.project.arrangement.group_size(id)),
                            FontId::proportional(10.0), muted());
                        if card.clicked() {
                            group_toggle = Some(id);
                        }
                        if let Some(source) = track_drag.as_ref().filter(|_| drag_pos.is_some_and(|pos| visible.contains(pos))) {
                            labels.rect_stroke(row.shrink(2.0), 8.0, Stroke::new(2.0, p_accent()));
                            if released {
                                group_drop = Some((source.0, item));
                            }
                        }
                        continue;
                    }
                };
                let track = &self.project.arrangement.tracks[lane];
                let card = row.shrink2(vec2(3.0, 2.0));
                if lane == self.arr.sel_lane {
                    labels.rect_filled(card, 7.0, selected_fill());
                } else if lane % 2 == 0 {
                    labels.rect_filled(card, 7.0, card_fill().gamma_multiply(0.62));
                }
                labels.line_segment([row.left_top(), row.right_top()], Stroke::new(0.6, grid_line()));
                if !track.playlist_locked && ui.rect_contains_pointer(visible) {
                    let effect = egui::DragAndDrop::payload::<crate::fx::FxKind>(ui.ctx());
                    let instrument = egui::DragAndDrop::payload::<browser::DragItem>(ui.ctx())
                        .filter(|item| matches!(item.as_ref(), browser::DragItem::Instrument(_)))
                        .filter(|_| track.kind == crate::arrangement::TrackKind::Midi);
                    if effect.is_some() || instrument.is_some() {
                        labels.rect_stroke(card, 7.0, Stroke::new(2.0, p_accent()));
                        if released {
                            if effect.is_some() {
                                dropped_effect = egui::DragAndDrop::take_payload::<crate::fx::FxKind>(ui.ctx())
                                    .map(|kind| (*kind, lane));
                            } else {
                                dropped_item = egui::DragAndDrop::take_payload::<browser::DragItem>(ui.ctx())
                                    .map(|item| (item, lane, 0));
                            }
                        }
                    }
                }
                if let Some(progress) = active_lanes[lane] {
                    let beat = TICKS_PER_BAR / 4;
                    let phase = (pos % beat) as f32 / beat as f32;
                    let color = lane_col(lane);
                    let card = row.shrink2(vec2(2.0, 2.0));
                    labels.rect_filled(card, 6.0, color.gamma_multiply(0.18 + 0.14 * (1.0 - phase)));
                    let width = (card.width() - 10.0).max(0.0);
                    let bar = Rect::from_min_size(card.left_bottom() + vec2(5.0, -6.0), vec2(width, 3.0));
                    labels.rect_filled(bar, 1.5, color.gamma_multiply(0.28));
                    labels.rect_filled(Rect::from_min_size(bar.min, vec2(width * progress, 3.0)), 1.5, color);
                }
                labels.rect_filled(Rect::from_min_size(row.min + vec2(2.0, 3.0), vec2(4.0, row.height() - 6.0)), 2.0, lane_col(lane));
                if add_lane {
                    continue;
                }
                let mut label_rect = visible;
                label_rect.min.x = label_rect.min.x.max(row.left() + 19.0);
                if panel.width() >= 84.0 {
                    label_rect.max.x -= 29.0;
                    let button_rect = Rect::from_min_max(pos2(label_rect.right(), visible.top()), visible.max);
                    let button = ui.interact(button_rect, panel_id.with(("options", lane)), Sense::click())
                        .on_hover_text("Track options");
                    if button.clicked() {
                        ui.data_mut(|d| d.insert_temp(options_id, lane));
                    }
                    if track.playlist_locked {
                        labels.text(button_rect.center(), Align2::CENTER_CENTER, "L", FontId::proportional(12.0), muted());
                    }
                    let dot_color = if track.solo { super::theme::TEAL() } else { muted() };
                    if !track.playlist_locked {
                        for offset in [-5.0, 0.0, 5.0] {
                            labels.circle_filled(button_rect.center() + vec2(offset, 0.0), 1.4, dot_color);
                        }
                    }
                }
                playlist_label(&labels, label_rect.shrink2(vec2(1.0, 0.0)), row.center().y,
                    &track.name, if track.mute { muted() } else { ink() });
                for offset in [-4.0, 0.0, 4.0] {
                    labels.circle_filled(pos2(row.left() + 12.0, row.center().y + offset), 1.25, muted());
                }
                let handle = Rect::from_min_max(visible.min, label_rect.max);
                let hit = ui.interact(handle, panel_id.with(lane), Sense::click_and_drag())
                    .on_hover_text("Swipe for actions or drag onto a track");
                let gesture = hit.id.with("track_gesture");
                if hit.drag_started() {
                    if let Some(origin) = ui.input(|input| input.pointer.press_origin()) {
                        ui.data_mut(|d| d.insert_temp(gesture, (origin, None::<bool>)));
                    }
                }
                let motion = ui.data(|d| d.get_temp::<(Pos2, Option<bool>)>(gesture));
                if let Some(((origin, direction), pointer)) = motion.zip(drag_pos) {
                    let delta = pointer - origin;
                    if hit.dragged() && direction.is_none() && delta.abs().max_elem() >= 12.0 {
                        let sideways = delta.x.abs() > delta.y.abs();
                        ui.data_mut(|d| d.insert_temp(gesture, (origin, Some(sideways))));
                        if !sideways && !track.playlist_locked {
                            egui::DragAndDrop::set_payload(ui.ctx(), PlaylistTrackDrag(lane));
                        }
                    }
                    if hit.drag_stopped() {
                        ui.data_mut(|d| d.remove::<(Pos2, Option<bool>)>(gesture));
                        if released && direction == Some(true) && delta.x.abs() >= 25.0 {
                            ui.data_mut(|d| d.insert_temp(options_id, lane));
                        }
                    }
                }
                if let Some(source) = track_drag.as_ref().filter(|source| {
                    source.0 != lane && !track.playlist_locked && drag_pos.is_some_and(|pos| visible.contains(pos))
                }) {
                    labels.rect_stroke(row.shrink(2.0), 7.0, Stroke::new(2.0, p_accent()));
                    if released {
                        group_drop = Some((source.0, item));
                    }
                }
                if hit.clicked() || hit.secondary_clicked() {
                    name_pick = Some((lane, lane == self.arr.sel_lane || hit.secondary_clicked()));
                }
            }
        }
        if let Some((source, pointer)) = track_drag.as_ref().zip(drag_pos).filter(|_| !released) {
            if let Some(track) = self.project.arrangement.tracks.get(source.0) {
                let ghost = ui.painter().with_clip_rect(viewport.intersect(ui.clip_rect()));
                let width = panel.width().min(104.0);
                let left = (pointer.x - width - 10.0).clamp(viewport.left(), (viewport.right() - width).max(viewport.left()));
                let top = (pointer.y - 16.0).clamp(viewport.top(), (viewport.bottom() - 32.0).max(viewport.top()));
                let badge = Rect::from_min_size(pos2(left, top), vec2(width, 32.0));
                ghost.rect_filled(badge, 8.0, card_fill());
                ghost.rect_stroke(badge, 8.0, Stroke::new(1.5, lane_col(source.0)));
                ghost.with_clip_rect(badge.intersect(viewport)).text(badge.center(), Align2::CENTER_CENTER,
                    &track.name, FontId::proportional(11.0), ink());
            }
        }
        if group_drop.is_some() {
            egui::DragAndDrop::take_payload::<PlaylistTrackDrag>(ui.ctx());
        }
        if show_master {
            let clip = names.intersect(ui.clip_rect());
            let master_top = scroll.inner.y + row_count as f32 * lane_h;
            let mut y = master_top + lane_h + 6.0;
            let mut add = None;
            for (kind, label) in [(crate::arrangement::TrackKind::Midi, "+ MIDI"), (crate::arrangement::TrackKind::Audio, "+ Audio")] {
                let r = Rect::from_min_size(pos2(panel.left() + 4.0, y), vec2(panel.width() - 8.0, 34.0));
                y += 40.0;
                if !r.intersects(clip) {
                    continue;
                }
                let resp = ui.interact(r.intersect(clip), panel_id.with(label), Sense::click());
                let p = ui.painter().with_clip_rect(clip);
                p.rect_filled(r, 10.0, if resp.is_pointer_button_down_on() { selected_fill() } else { card_fill() });
                p.rect_stroke(r, 10.0, Stroke::new(1.0, outline()));
                p.text(r.center(), Align2::CENTER_CENTER, label, FontId::proportional(12.5), ink());
                if resp.clicked() {
                    add = Some(kind);
                }
            }
            let master = Rect::from_min_size(pos2(panel.left() + 4.0, master_top + 3.0), vec2(panel.width() - 8.0, lane_h - 6.0));
            if master.intersects(clip) {
                let editing = automation && self.arr.auto_master;
                let header = if editing { Rect::from_min_size(master.min, vec2(master.width(), 24.0)) } else { master };
                let resp = ui.interact(header.intersect(clip), panel_id.with("master"), Sense::click())
                    .on_hover_text(if automation { "Edit Master volume automation" } else { "Open Master mixer" });
                let p = ui.painter().with_clip_rect(clip);
                p.rect_filled(master, 10.0, if self.arr.auto_master { selected_fill() } else { card_fill() });
                p.rect_stroke(master, 10.0, Stroke::new(1.0, outline()));
                let title_y = if editing { 12.0 } else { 17.0 };
                p.text(master.left_top() + vec2(7.0, title_y), Align2::LEFT_CENTER, "Master", FontId::proportional(12.0), ink());
                p.text(master.right_top() + vec2(-7.0, title_y), Align2::RIGHT_CENTER,
                    format!("{:+.1}", widgets::db(self.project.master_volume)), FontId::proportional(10.0), muted());
                if editing {
                    let graph = Rect::from_min_max(master.min + vec2(5.0, 25.0), master.max - vec2(5.0, 5.0));
                    let visible = graph.intersect(clip);
                    if visible.is_positive() {
                        let scale = graph.width() / grid.width().max(1.0);
                        let bar_width = bar_w * scale;
                        let origin = graph.left() - scroll.state.offset.x * scale;
                        let area = Rect::from_min_size(pos2(origin, graph.top()),
                            vec2(grid.width().max(bars as f32 * bar_w) * scale, graph.height()));
                        let mut plot = ui.new_child(egui::UiBuilder::new().max_rect(graph));
                        plot.set_clip_rect(visible);
                        self.m_playlist_automation(&mut plot, area, bar_width, bars, pinching, true);
                    }
                } else {
                    let meter = Rect::from_min_size(master.left_bottom() + vec2(7.0, -21.0),
                        vec2(master.width() - 14.0, 9.0));
                    p.rect_filled(meter, 4.5, p_bg());
                    let [left, right] = self.shared.peak(crate::mixer::NUM_STRIPS);
                    let level = left.max(right).clamp(0.0, 1.0).sqrt();
                    if level > 0.0 {
                        let fill = Rect::from_min_size(meter.min, vec2(meter.width() * level, meter.height()));
                        p.rect_filled(fill, 4.5, p_accent());
                    }
                    if level > 0.0 || self.playing() {
                        ui.ctx().request_repaint_after(std::time::Duration::from_millis(16));
                    }
                    let slider = master.intersect(clip).intersect(Rect::from_min_max(
                        pos2(master.left(), meter.top() - 5.0), master.right_bottom()));
                    if !automation && slider.is_positive() {
                        let gain = ui.interact(slider, panel_id.with("master_gain"), Sense::click_and_drag())
                            .on_hover_text("Drag to adjust Master volume, tap to open the Mixer");
                        open_mixer |= gain.clicked() && !automation;
                        if gain.drag_started() {
                            self.m_playlist_undo_step();
                        }
                        if (gain.dragged() || gain.drag_stopped()) && !pinching {
                            if let Some(pointer) = gain.interact_pointer_pos() {
                                let fraction = ((pointer.x - meter.left()) / meter.width()).clamp(0.0, 1.0);
                                self.project.master_volume = fraction * 1.2;
                            }
                        }
                    }
                }
                if resp.clicked() && automation {
                    self.arr.auto_master = !editing;
                    select_master = !editing;
                } else if resp.clicked() {
                    open_mixer = true;
                }
            }
            if let Some(kind) = add {
                self.project.arrangement.add_track(kind);
                self.arr.sel_lane = self.project.arrangement.tracks.len() - 1;
            }
            if open_mixer {
                // the Master card opens the full Mixer page (the one in the main menu) with Master selected
                ui.data_mut(|d| d.insert_temp(egui::Id::new(MIXER_CHANNEL), MobileMixerChannel::Master));
                let ctx = ui.ctx().clone();
                self.m_select_studio_section(&ctx, 4);
            }
        }
        if !portrait {
            let width = if panel_open { (panel.width() - TOUCH - 3.0).max(1.0) } else { panel.width() };
            let toggle = Rect::from_min_size(panel.min, vec2(width, ruler));
            let toggle_response = ui.interact(toggle, panel_id, Sense::click())
                .on_hover_text(if panel_open { "Hide track names" } else { "Show track names" });
            painter.text(toggle.center(), Align2::CENTER_CENTER, if panel_open { "Tracks ›" } else { "‹" },
                FontId::proportional(if panel_open { 13.0 } else { 24.0 }), ink());
            if toggle_response.clicked() {
                ui.data_mut(|d| d.insert_temp(panel_id, !panel_open));
            }
        }
        if !add_lane && (!portrait || !docked) {
            let x = if show_names && !portrait { panel.right() - 3.0 } else { timeline.right() - 3.0 };
            let options = Rect::from_min_size(pos2(x - TOUCH, viewport.top()), vec2(TOUCH, ruler));
            self.m_playlist_options(ui, options);
        }
        if let Some((index, anchor)) = ui.data(|d| d.get_temp::<(usize, Pos2)>(clip_menu_id)) {
            if let Some(c) = self.project.arrangement.clips.get(index).copied().filter(|c| {
                !pinching && !select_mode && !automation && !self.project.arrangement.tracks[c.lane].playlist_locked
            }) {
                let step = self.arr.snap.max(1);
                let longer = crate::arrangement::Clip { len: c.len.saturating_add(step), ..c };
                let shorter = crate::arrangement::Clip { len: c.len.saturating_sub(step).max(1), ..c };
                let actions = [
                    (if c.audio { Icon::Wave } else { Icon::Keys }, "Open clip", true),
                    (Icon::Plus, "Extend by grid step", self.project.arrangement.can_place(&longer, Some(index))),
                    (Icon::Minus, "Shorten by grid step", shorter.len != c.len
                        && self.project.arrangement.can_place(&shorter, Some(index))),
                    (Icon::CloneClips, "Clone clip", true),
                    (Icon::Trash, "Delete clip", true),
                    (Icon::Settings, "MIDI clip settings", true),
                ];
                let actions = if c.audio { &actions[..5] } else { &actions[..] };
                if let Some(action) = playlist_clip_menu(ui.ctx(), clip_menu_id, anchor, actions, portrait) {
                    ui.data_mut(|d| d.remove::<(usize, Pos2)>(clip_menu_id));
                    match action {
                        0 => selection = Some((c.lane, Some(index), Some(c.start), true)),
                        1 | 2 => move_clip = Some((index, if action == 1 { longer } else { shorter })),
                        3 => duplicate = Some(index),
                        4 => delete_clip = Some(index),
                        5 => {
                            self.m_open_playlist_clip(ui.ctx(), index);
                            ui.data_mut(|d| d.insert_temp(egui::Id::new("m_midi_clip_settings"), index));
                        }
                        _ => {}
                    }
                }
            } else {
                ui.data_mut(|d| d.remove::<(usize, Pos2)>(clip_menu_id));
            }
        }
        if let Some(id) = group_toggle {
            self.m_playlist_undo_step();
            if let Some(group) = self.project.arrangement.groups.iter_mut().find(|group| group.id == id) {
                group.collapsed = !group.collapsed;
            }
            ui.ctx().request_repaint();
        }
        if let Some((source, target)) = group_drop {
            let (track, folder) = match target {
                PlaylistRow::Track(target) => (Some(target), None),
                PlaylistRow::Group(id) => (None, Some(id)),
                PlaylistRow::FxAutomation(_, _) => (None, None),
            };
            self.m_playlist_undo_step();
            self.project.arrangement.group_playlist_track(source, track, folder);
            ui.ctx().request_repaint();
        }
        if self.m_clip_piano_open(ui.ctx()) {
            ui.data_mut(|d| {
                d.remove::<usize>(options_id);
                d.remove::<bool>(options_id.with("confirm_delete"));
            });
        } else if let Some(lane) = ui.data(|d| d.get_temp::<usize>(options_id)).filter(|&lane| lane < tracks) {
            if self.m_playlist_track_modal(ui.ctx(), lane, options_id) {
                name_pick = Some((lane, true));
            }
        }
        if select_master {
            self.arr.auto_master = true;
            self.arr.sel_clip = None;
            self.arr.sel.clear();
            ui.ctx().request_repaint();
        }
        if let Some((lane, open_settings)) = name_pick {
            self.arr.auto_master = false;
            self.arr.sel_lane = lane;
            self.arr.sel_clip = None;
            self.arr.sel.clear();
            if open_settings {
                ui.data_mut(|d| {
                    d.remove::<usize>(egui::Id::new("m_clip_sheet_selection"));
                    d.insert_temp(egui::Id::new("m_track_sheet_open"), true);
                });
            }
        }
        if let Some((lane, index)) = remove_fx_automation {
            let clips = &mut self.project.arrangement.tracks[lane].fx_automation;
            clips.remove(index);
            ui.data_mut(|d| {
                for i in index..=clips.len() {
                    d.remove::<bool>(egui::Id::new(("m_fx_automation_draw", lane, i)));
                }
            });
        }
        if let Some((index, edited)) = fade_clip {
            self.m_playlist_undo_step();
            self.project.arrangement.clips[index] = edited;
        }
        if let Some((index, moved)) = move_clip {
            self.project.arrangement.clips[index] = moved;
            self.arr.sel_lane = moved.lane;
            self.arr.sel_clip = Some(index);
            self.arr.sel.clear();
            self.arr.msg.clear();
            return;
        }
        if let Some(index) = duplicate {
            match arrangement_ui::duplicate_clip(&mut self.project, index) {
                Some(copy) => {
                    self.m_playlist_undo_step();
                    self.arr.sel_clip = Some(copy);
                    self.arr.sel.clear();
                }
                None => self.arr.msg = "No room for a copy on this track.".into(),
            }
            return;
        }
        if let Some(index) = delete_clip {
            self.m_playlist_undo_step();
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
            self.arr.auto_master = false;
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
                if let Some(clip) = self.project.arrangement.clips.get(i).filter(|clip| clip.audio) {
                    self.project.arrangement.tracks[lane].audio_brush = Some(clip.pattern);
                }
                if let Some(c) = self.project.arrangement.clips.get(i).filter(|_| edit) {
                    if !c.audio && c.pattern < self.project.patterns.len() {
                        self.project.current_pattern = c.pattern;
                    }
                    ui.data_mut(|d| {
                        d.remove::<bool>(egui::Id::new("m_track_sheet_open"));
                        d.remove::<usize>(egui::Id::new("m_midi_clip_settings"));
                        d.insert_temp(egui::Id::new("m_clip_sheet_selection"), i);
                    });
                }
            } else if let Some(tick) = tick {
                self.m_insert_playlist_clip(ui.ctx(), lane, tick);
            }
        }
    }

    fn m_vertical_playlist(&mut self, ui: &mut Ui, select: bool, docked: bool) {
        use crate::arrangement::{Clip, TrackKind};

        let ctx = ui.ctx().clone();
        let id = egui::Id::new("m_playlist_vertical_view");
        let menu_id = egui::Id::new("m_playlist_clip_actions");
        let lanes: Vec<_> = self.project.arrangement.tracks.iter().enumerate()
            .filter(|(_, track)| !self.project.arrangement.groups.iter().any(|g| g.id == track.group && g.collapsed))
            .map(|(lane, _)| lane).collect();
        let mut columns = vec![None; self.project.arrangement.tracks.len()];
        for (column, &lane) in lanes.iter().enumerate() {
            columns[lane] = Some(column);
        }
        let (viewport, _) = ui.allocate_exact_size(ui.available_size(), Sense::hover());
        let header_height = 42.0;
        let grid = Rect::from_min_max((viewport.min + vec2(32.0, header_height)).min(viewport.max), viewport.max);
        if !grid.is_positive() { return; }
        let corner = Rect::from_min_max(viewport.min, grid.min);
        let mut trigger = ui.new_child(egui::UiBuilder::new().id_salt("vertical_track_menu").max_rect(corner));
        trigger.set_clip_rect(corner.intersect(ui.clip_rect()));
        let button = egui::Button::new("").frame(false).min_size(corner.size());
        let menu = egui::menu::menu_custom_button(&mut trigger, button, |ui| {
            ui.set_width((ctx.screen_rect().width() - 40.0).clamp(120.0, 280.0));
            self.m_playlist_track_strip(ui);
            if !docked {
                ui.separator();
                self.m_playlist_options_content(ui, false);
            }
        });
        trigger.painter().rect_filled(corner, 0.0, if menu.response.hovered() { selected_fill() } else { bar_fill() });
        icon(trigger.painter(), Icon::Tracks, corner.center(), ink());
        menu.response.on_hover_text("Tracks");
        if lanes.is_empty() {
            ui.painter().text(grid.center(), Align2::CENTER_CENTER, "Add a track or expand a group",
                FontId::proportional(12.0), muted());
            return;
        }
        let mut size = ui.data(|d| d.get_temp::<egui::Vec2>(id.with("size")))
            .unwrap_or(vec2((grid.width() / 3.0).clamp(66.0, 112.0), 48.0));
        // zoomed out, the columns stretch to the screen instead of leaving an empty strip on the right
        let min_column = (grid.width() / lanes.len() as f32).clamp(42.0, 200.0);
        size.x = size.x.clamp(min_column, 200.0);
        let (pinching, zoom) = grid_pinch(ui, id.with("pinch"), grid);
        let mut pinch_offset = None;
        if let Some((axis, factor, center)) = zoom {
            // zoom around the fingers: the cell under the pinch centre stays under it
            let before = size[axis];
            size[axis] = (size[axis] * factor).clamp(if axis == 0 { min_column } else { 42.0 }, 200.0);
            let applied = size[axis] / before;
            if (applied - 1.0).abs() > f32::EPSILON {
                let anchor = (center[axis] - grid.min[axis]).max(0.0);
                let offset = ui.data(|d| d.get_temp::<egui::Vec2>(id.with("offset"))).unwrap_or_default()[axis];
                pinch_offset = Some((axis, ((offset + anchor) * applied - anchor).max(0.0)));
            }
            ui.data_mut(|d| d.insert_temp(id.with("size"), size));
        }
        let column = size.x;
        let bar_height = size.y;
        let scale = bar_height / TICKS_PER_BAR as f32;
        let bars = self.project.arrangement.song_end_bar()
            .max(if self.project.loop_on { self.project.loop_start.saturating_add(self.project.loop_bars) } else { 0 })
            .saturating_add(8).max(16)
            // zoomed out, enough bars to fill the screen
            .max((grid.height() / bar_height).ceil() as u32 + 1);
        let snap = self.arr.snap.max(1);
        let pos = self.shared.position.load(Ordering::Relaxed);
        let playing = self.playing() && self.project.song_mode;
        let (reveal_lane, reveal_tick, rewind) = ui.data_mut(|d| (
            d.remove_temp::<usize>(egui::Id::new("m_playlist_reveal_track")),
            d.remove_temp::<u32>(egui::Id::new("m_playlist_reveal_tick")),
            d.remove_temp::<bool>(egui::Id::new("playlist_return_to_start")).unwrap_or(false),
        ));
        let old = ui.data(|d| d.get_temp::<egui::Vec2>(id.with("offset"))).unwrap_or_default();
        // one-finger scrolling is ours (see touch_pan); a press on the selected clip moves the clip instead
        let content = vec2(column * lanes.len() as f32, bar_height * bars as f32);
        let selected_rect = self.arr.sel_clip.and_then(|index| self.project.arrangement.clips.get(index)).and_then(|clip| {
            let i = columns.get(clip.lane).copied().flatten()?;
            Some(Rect::from_min_size(grid.min - old + vec2(i as f32 * column, clip.start as f32 * scale),
                vec2(column, (clip.len as f32 * scale).max(5.0))))
        });
        let panned = touch_pan(ui, id.with("pan"), grid, content, old, !pinching && !select,
            |press| selected_rect.is_some_and(|rect| rect.contains(press)));
        let mut scroll = mobile_scroll(egui::ScrollArea::both().id_salt(id))
            .drag_to_scroll(false).auto_shrink([false, false])
            .horizontal_scroll_offset(panned.x).vertical_scroll_offset(panned.y);
        if let Some(index) = reveal_lane.and_then(|lane| lanes.iter().position(|&v| v == lane)) {
            let x = index as f32 * column;
            if x < old.x || x + column > old.x + grid.width() {
                scroll = scroll.horizontal_scroll_offset(x);
            }
        }
        if rewind {
            scroll = scroll.vertical_scroll_offset(0.0);
        } else if let Some(tick) = reveal_tick {
            let y = tick as f32 * scale;
            if y < old.y || y > old.y + grid.height() - 16.0 {
                scroll = scroll.vertical_scroll_offset(y);
            }
        }
        match pinch_offset {
            Some((0, x)) => scroll = scroll.horizontal_scroll_offset(x),
            Some((_, y)) => scroll = scroll.vertical_scroll_offset(y),
            None => {}
        }

        let mut picked_clip = None;
        let mut open = None;
        let mut move_clip = None;
        let mut create = None;
        let mut item_drop = None;
        let mut fx_drop = None;
        let mut child = ui.new_child(egui::UiBuilder::new().id_salt(id).max_rect(grid));
        child.set_clip_rect(grid.intersect(ui.clip_rect()));
        let out = scroll.show(&mut child, |ui| {
            let (area, background) = ui.allocate_exact_size(vec2(column * lanes.len() as f32, bar_height * bars as f32),
                if select { Sense::click_and_drag() } else { Sense::click() });
            let p = ui.painter_at(area);
            let visible = p.clip_rect();
            let tick_at = |y: f32| ((y - area.top()).max(0.0) / scale) as u32;
            let y_at = |tick: u32| area.top() + tick as f32 * scale;
            let lane_at = |x: f32| lanes.get(((x - area.left()) / column).floor() as usize).copied();
            let bounds = |i: usize, clip: Clip| Rect::from_min_size(
                pos2(area.left() + i as f32 * column, y_at(clip.start)),
                vec2(column, (clip.len as f32 * scale).max(5.0)));
            for (i, &lane) in lanes.iter().enumerate() {
                let rect = Rect::from_min_size(pos2(area.left() + i as f32 * column, area.top()), vec2(column, area.height()));
                if !rect.intersects(visible) { continue; }
                let base = if self.project.arrangement.kind(lane) == TrackKind::Audio {
                    mobile_color(0xFF1B1A2B, 0xFF232A2B, 0xFFE7E2F7, 0xFF222225, 0xFF1B2416)
                } else { mobile_color(0xFF191725, 0xFF25282D, 0xFFEEEAFB, 0xFF1C1C1F, 0xFF181F14) };
                let fill = if self.arr.sel_lane == lane { tint(base, selected_fill(), 0.28) } else { base };
                p.rect_filled(rect, 0.0, fill);
                p.vline(rect.left(), rect.y_range(), Stroke::new(1.0, grid_line()));
            }
            let step = if snap == 1 { TICKS_PER_BAR } else { visible_grid_step(snap, scale) };
            let first = tick_at(visible.top()).div_ceil(step) * step;
            let last = tick_at(visible.bottom()).min(bars.saturating_mul(TICKS_PER_BAR));
            for tick in (first..=last).step_by(step as usize) {
                let major = tick % TICKS_PER_BAR == 0;
                p.hline(area.x_range(), y_at(tick), Stroke::new(if major { 1.25 } else { 0.65 },
                    if major { outline() } else { grid_line().gamma_multiply(0.72) }));
            }
            if self.project.loop_on {
                let start = self.project.loop_start.saturating_mul(TICKS_PER_BAR);
                let end = self.project.loop_start.saturating_add(self.project.loop_bars).saturating_mul(TICKS_PER_BAR);
                p.rect_filled(Rect::from_min_max(pos2(area.left(), y_at(start)), pos2(area.right(), y_at(end))),
                    0.0, p_accent().gamma_multiply(0.1));
            }
            let box_id = id.with("box_origin");
            if select && !pinching && background.drag_started() {
                if let Some(start) = ui.input(|i| i.pointer.press_origin()) {
                    ui.data_mut(|d| d.insert_temp(box_id, start));
                }
            }
            let selection_box = if select && !pinching && (background.dragged() || background.drag_stopped()) {
                ui.data(|d| d.get_temp::<Pos2>(box_id)).zip(background.interact_pointer_pos())
                    .map(|(a,b)| Rect::from_two_pos(a,b).intersect(area))
            } else { None };
            let mut selected = Vec::new();
            for (index, &clip) in self.project.arrangement.clips.iter().enumerate() {
                let Some(i) = columns.get(clip.lane).copied().flatten() else { continue; };
                let bounds_at = bounds(i, clip);
                let clip_id = id.with(("clip", index));
                if !bounds_at.intersects(visible) && !ctx.is_being_dragged(clip_id) { continue; }
                let locked = self.project.arrangement.tracks[clip.lane].playlist_locked;
                // tap selects, then the selected clip can be dragged; a swipe that starts on any other clip scrolls
                let movable = self.arr.sel_clip == Some(index) || ctx.is_being_dragged(clip_id);
                let hit = ui.interact(bounds_at, clip_id, if locked || pinching { Sense::hover() }
                    else if select || !movable { Sense::click() } else { Sense::click_and_drag() });
                let rect = bounds_at.shrink2(vec2(3.0, 1.0));
                let track_color = lane_col(clip.lane);
                let fill = tint(card_fill(), track_color, 0.34);
                let marks_color = tint(track_color, ink(), 0.52);
                let fill = if clip.mute { tint(fill, card_fill(), 0.45) } else { fill };
                p.rect_filled(rect, 6.0, fill);
                clip_shine(&p, rect, track_color, clip.mute);
                if let Some(progress) = playing.then(|| playlist_clip_playback(&self.project, &clip, pos)).flatten() {
                    paint_playlist_playback(&p, rect, track_color, progress, pos, true);
                }
                if !locked && selection_box.is_some_and(|area| area.intersects(rect)) { selected.push(index); }
                if self.arr.is_sel(index) || selected.last() == Some(&index) {
                    p.rect_stroke(rect, 6.0, Stroke::new(1.7, p_accent()));
                }
                let pattern = (!clip.audio).then(|| self.project.patterns.get(clip.pattern)).flatten();
                let name = if clip.audio { self.project.audio_pool.get(clip.pattern).map(|s| s.name.as_str()) }
                    else { pattern.map(|p| p.name.as_str()) };
                if let Some(name) = name.filter(|_| rect.height() >= 14.0) {
                    let short = (rect.width() < 87.0)
                        .then(|| name.strip_prefix("MIDI clip ").filter(|n| n.parse::<usize>().is_ok()))
                        .flatten().map(|n| format!("MIDI {n}"));
                    fit_label(&p, rect.shrink2(vec2(3.0, 0.0)), rect.top() + 8.0,
                        short.as_deref().unwrap_or(name), ink(), if rect.width() < 60.0 { 9.5 } else { 11.0 });
                }
                if let Some(pattern) = pattern.filter(|_| rect.height() > 27.0) {
                    // mini notes: time runs down the clip, pitch across it, each note as long as it plays
                    let inset = Rect::from_min_max(rect.min + vec2(7.0, 17.0), rect.max - vec2(6.0, 5.0));
                    let visible_marks = inset.intersect(visible);
                    if visible_marks.is_positive() {
                        let span = clip.offset..clip.offset.saturating_add(clip.len);
                        let (lo, hi) = pattern.notes.iter().filter(|n| !n.mute && span.contains(&n.start))
                            .fold((u8::MAX, 0u8), |(lo, hi), n| (lo.min(n.pitch), hi.max(n.pitch)));
                        let range = f32::from(hi.saturating_sub(lo)).max(11.0);
                        let marks = p.with_clip_rect(visible_marks);
                        let ink_note = marks_color;
                        let y_of = |tick: u32| inset.top() + tick.saturating_sub(clip.offset) as f32 / clip.len.max(1) as f32 * inset.height();
                        let max_marks = (visible_marks.height().ceil() as usize).saturating_mul(3);
                        let notes = pattern.notes.iter().filter(|n| !n.mute && span.contains(&n.start)
                            && y_of(n.start) <= visible_marks.bottom() && y_of(n.start.saturating_add(n.len)) >= visible_marks.top());
                        for note in notes.take(max_marks) {
                            let x = inset.left() + f32::from(note.pitch.saturating_sub(lo.min(note.pitch))) / range * (inset.width() - 4.0);
                            let y0 = y_of(note.start);
                            let y1 = y_of(note.start.saturating_add(note.len).min(span.end)).max(y0 + 2.5);
                            marks.rect_filled(Rect::from_min_max(pos2(x, y0 + 0.5), pos2(x + 4.0, y1 - 0.5)), 1.5, ink_note);
                        }
                        if lo == u8::MAX {
                            // a drum clip: its lit steps as short bars, one row per step
                            let first = clip.offset / TICKS_PER_STEP;
                            let last = span.end.div_ceil(TICKS_PER_STEP);
                            let rows = pattern.drums.len().max(1) as f32;
                            for step in first..last {
                                let y = y_of(step * TICKS_PER_STEP);
                                if y > visible_marks.bottom() { break; }
                                if y < visible_marks.top() - 4.0 { continue; }
                                for (row, lane_steps) in pattern.drums.iter().enumerate() {
                                    if lane_steps.get(step as usize).is_some_and(|st| st.on) {
                                        let w = (inset.width() - 4.0) / rows;
                                        let x = inset.left() + row as f32 * w;
                                        marks.rect_filled(Rect::from_min_size(pos2(x, y), vec2((w - 2.0).max(3.0), 3.0)), 1.5, ink_note);
                                    }
                                }
                            }
                        }
                    }
                }
                if clip.audio && rect.height() > 24.0 {
                    // mini waveform down the clip: the part of the file that plays inside it
                    if let Some(data) = self.project.audio_pool.get(clip.pattern).and_then(|s| s.data.as_ref()) {
                        let body = Rect::from_min_max(rect.min + vec2(7.0, 17.0), rect.max - vec2(5.0, 5.0));
                        let shown = body.intersect(visible);
                        if shown.is_positive() && !data.mono.is_empty() {
                            let bpm = self.project.bpm.max(1.0);
                            let clip_bpm = if clip.warp > 0.0 { clip.warp } else { bpm };
                            let clip_secs = clip.len as f32 / crate::sequencer::PPQ as f32 * 60.0 / bpm;
                            let secs_per_px = clip_secs / body.height().max(1.0);
                            let offset_secs = clip.offset as f32 / crate::sequencer::PPQ as f32 * 60.0 / clip_bpm;
                            let wave = marks_color;
                            let painter = p.with_clip_rect(shown);
                            let mut y = shown.top();
                            while y < shown.bottom() {
                                let t = (y - body.top()) * secs_per_px;
                                let (ta, tb) = if clip.reverse { (offset_secs + clip_secs - t - 2.0 * secs_per_px, offset_secs + clip_secs - t) }
                                    else { (offset_secs + t, offset_secs + t + 2.0 * secs_per_px) };
                                let a0 = (ta.max(0.0) * data.rate as f32) as usize;
                                if a0 >= data.mono.len() { break; }
                                let a1 = ((tb.max(0.0) * data.rate as f32) as usize).min(data.mono.len()).max(a0 + 1);
                                let stride = ((a1 - a0) / 24).max(1);
                                let peak = (a0..a1).step_by(stride).fold(0.0f32, |m, i| m.max(data.mono[i].abs()));
                                let half = peak.min(1.0) * body.width() * 0.48;
                                painter.line_segment([pos2(body.center().x - half, y), pos2(body.center().x + half, y)], Stroke::new(1.5, wave));
                                y += 2.0;
                            }
                        }
                    }
                }
                if rect.height() > 10.0 {
                    p.rect_filled(Rect::from_center_size(pos2(rect.center().x, rect.bottom() - 3.0), vec2(16.0, 2.0)),
                        1.0, tint(fill, ink(), 0.46));
                }
                if locked || pinching {
                    ui.data_mut(|d| d.remove::<(Pos2, Pos2, bool)>(clip_id.with("drag")));
                    continue;
                }
                if select {
                    if hit.clicked() { picked_clip = Some(index); }
                    continue;
                }
                if hit.long_touched() || hit.secondary_clicked() {
                    ui.data_mut(|d| d.insert_temp(menu_id, (index, hit.interact_pointer_pos().unwrap_or(rect.center()))));
                    picked_clip = Some(index);
                } else if hit.double_clicked() {
                    open = Some(index);
                } else if hit.clicked() {
                    picked_clip = Some(index);
                }
                let drag_id = clip_id.with("drag");
                if hit.drag_started() {
                    if let Some(origin) = ui.input(|i| i.pointer.press_origin()) {
                        let delta = hit.interact_pointer_pos().unwrap_or(origin) - origin;
                        let grip_height = (bounds_at.height() * 0.30).min(14.0);
                        let resize = bounds_at.height() > 10.0
                            && origin.y >= bounds_at.bottom() - grip_height
                            && delta.y.abs() >= delta.x.abs();
                        ui.data_mut(|d| d.insert_temp(drag_id, (origin, origin, resize)));
                    }
                }
                if hit.dragged() || hit.drag_stopped() {
                    if let Some((origin, last, resize)) = ui.data(|d| d.get_temp::<(Pos2, Pos2, bool)>(drag_id)) {
                        let point = hit.interact_pointer_pos().unwrap_or(last);
                        let delta = (point.y - origin.y) / scale;
                        let snap_tick = |tick: f32| ((tick.max(0.0) / snap as f32).round() as u32).saturating_mul(snap);
                        let mut candidate = clip;
                        if resize {
                            candidate.len = snap_tick(clip.end_tick() as f32 + delta).saturating_sub(clip.start).max(1);
                            candidate.fade_in = candidate.fade_in.min(candidate.len / 2);
                            candidate.fade_out = candidate.fade_out.min(candidate.len / 2);
                        } else {
                            candidate.lane = lane_at(point.x).unwrap_or(clip.lane);
                            candidate.start = snap_tick(clip.start as f32 + delta);
                        }
                        let valid = visible.contains(point) && (resize || lane_at(point.x).is_some())
                            && !self.project.arrangement.tracks[candidate.lane].playlist_locked
                            && self.project.arrangement.kind(candidate.lane) == self.project.arrangement.kind(clip.lane)
                            && self.project.arrangement.can_place(&candidate, Some(index));
                        if let Some(target) = columns[candidate.lane] {
                            p.rect_stroke(bounds(target, candidate).shrink(2.0), 6.0,
                                Stroke::new(2.0, if valid { p_accent() } else { RED }));
                        }
                        if hit.drag_stopped() {
                            if valid && candidate != clip { move_clip = Some((index, candidate)); }
                            ui.data_mut(|d| d.remove::<(Pos2, Pos2, bool)>(drag_id));
                        } else {
                            ui.data_mut(|d| d.insert_temp(drag_id, (origin, point, resize)));
                        }
                    }
                }
            }
            if let Some(rect) = selection_box.filter(|rect| rect.is_positive()) {
                p.rect_stroke(rect, 3.0, Stroke::new(1.2, p_accent()));
                if background.drag_stopped() {
                    self.arr.sel = selected.into_iter().collect();
                    self.arr.sel_clip = None;
                    ui.data_mut(|d| d.remove::<Pos2>(box_id));
                }
            } else if background.double_clicked() && !pinching && !select
                && !ui.data(|d| d.get_temp::<bool>(egui::Id::new(CLIP_MENU_DISMISSED))).unwrap_or(false)
            {
                // a double tap creates a clip; a single tap (often the end of a scroll) never does
                if let Some(point) = background.interact_pointer_pos() {
                    create = lane_at(point.x).map(|lane| (lane, tick_at(point.y)));
                }
            } else if background.clicked() && !pinching && !select {
                self.arr.sel_clip = None;
                self.arr.sel.clear();
            }
            if ui.rect_contains_pointer(area) && ui.input(|i| i.pointer.primary_released()) {
                if let Some(point) = ctx.pointer_interact_pos() {
                    if let Some(lane) = lane_at(point.x).filter(|&lane| !self.project.arrangement.tracks[lane].playlist_locked) {
                        if let Some(kind) = egui::DragAndDrop::take_payload::<crate::fx::FxKind>(&ctx) {
                            fx_drop = Some((*kind, lane));
                        } else if let Some(item) = egui::DragAndDrop::take_payload::<browser::DragItem>(&ctx) {
                            item_drop = Some((item, lane, tick_at(point.y) / snap * snap));
                        }
                    }
                }
            }
            p.hline(area.x_range(), y_at(pos), Stroke::new(2.0, p_accent()));
            area.min
        });
        ui.data_mut(|d| d.insert_temp(id.with("offset"), out.state.offset));

        let ruler = Rect::from_min_max(pos2(viewport.left(), grid.top()), pos2(grid.left(), grid.bottom()));
        let p = ui.painter_at(ruler);
        p.rect_filled(ruler, 0.0, bar_fill());
        let first = ((ruler.top() - out.inner.y).max(0.0) / bar_height) as u32;
        let last = (((ruler.bottom() - out.inner.y).max(0.0) / bar_height).ceil() as u32).min(bars);
        for bar in first..last {
            let y = out.inner.y + bar as f32 * bar_height;
            p.hline(ruler.x_range(), y, Stroke::new(1.0, outline()));
            p.text(pos2(ruler.center().x, y + 12.0), Align2::CENTER_CENTER, bar + 1, FontId::proportional(12.0), ink());
        }
        let head = out.inner.y + pos as f32 * scale;
        if ruler.y_range().contains(head) {
            p.add(Shape::convex_polygon(vec![pos2(ruler.right(), head), pos2(ruler.right() - 9.0, head - 5.0),
                pos2(ruler.right() - 9.0, head + 5.0)], p_accent(), Stroke::NONE));
        }
        let seek = ui.interact(ruler, id.with("ruler"), Sense::click_and_drag()).on_hover_text("Tap to seek · Drag to loop");
        if let Some(point) = seek.interact_pointer_pos().filter(|_| !pinching) {
            let bar_at = |y: f32| (((y - out.inner.y).max(0.0) / bar_height) as u32).min(bars - 1);
            if seek.dragged() {
                if let Some(origin) = ui.input(|input| input.pointer.press_origin()) {
                    let (a, b) = (bar_at(origin.y), bar_at(point.y));
                    let start = a.min(b);
                    let len = a.abs_diff(b) + 1;
                    if !self.project.loop_on || self.project.loop_start != start || self.project.loop_bars != len {
                        self.project.loop_on = true;
                        self.project.loop_start = start;
                        self.project.loop_bars = len;
                        self.publish_edits();
                    }
                }
            } else if seek.clicked() {
                self.m_seek(&ctx, ((point.y - out.inner.y).max(0.0) / scale) as u32);
            }
        }
        let header = Rect::from_min_max(pos2(grid.left(), viewport.top()), pos2(grid.right(), grid.top()));
        let painter = ui.painter_at(header).with_clip_rect(header.intersect(ui.clip_rect()));
        painter.rect_filled(header, 0.0, bar_fill());
        let options = egui::Id::new("m_playlist_track_names").with("track_modal");
        for (index, &lane) in lanes.iter().enumerate() {
            let left = out.inner.x + index as f32 * column;
            let cell = Rect::from_min_size(pos2(left, header.top()), vec2(column, header_height));
            let hit_rect = cell.intersect(header);
            if !hit_rect.is_positive() { continue; }

            let track = &self.project.arrangement.tracks[lane];
            let color = lane_col(lane);
            let selected = self.arr.sel_lane == lane && !self.arr.auto_master;
            painter.rect_filled(cell, 0.0, tint(bar_fill(), color, if selected { 0.30 } else { 0.14 }));
            painter.vline(cell.right(), cell.y_range(), Stroke::new(1.0, outline()));
            painter.hline(cell.x_range(), cell.bottom() - 1.5, Stroke::new(if selected { 3.0 } else { 2.0 }, color));

            let mut n = index + 1;
            let mut letters = Vec::new();
            while n > 0 {
                n -= 1;
                letters.push(char::from(b'A' + (n % 26) as u8));
                n /= 26;
            }
            let label: String = letters.into_iter().rev().collect();
            painter.text(pos2(cell.center().x, cell.top() + 11.0), Align2::CENTER_CENTER,
                label, FontId::proportional(12.0), ink());
            let shown = vertical_track_label(&self.project, lane);
            fit_label(&painter, cell.shrink2(vec2(4.0, 0.0)).intersect(header), cell.top() + 28.0,
                &shown, if track.mute { muted() } else { ink() }, if column < 64.0 { 10.0 } else { 11.0 });

            let hit = ui.interact(hit_rect, id.with(("column_header", lane)), Sense::click())
                .on_hover_text(format!("{} · {}", track.name, if track.kind == TrackKind::Audio { "Audio" } else { "MIDI" }));
            if hit.secondary_clicked() || hit.long_touched() {
                ctx.data_mut(|d| d.insert_temp(options, lane));
            } else if hit.clicked() {
                let open_settings = hit.double_clicked();
                if self.arr.sel_lane != lane || open_settings {
                    self.arr.sel_clip = None;
                    self.arr.sel.clear();
                }
                self.arr.sel_lane = lane;
                self.arr.auto_master = false;
                if open_settings {
                    ctx.data_mut(|d| {
                        d.remove::<usize>(options);
                        d.remove::<usize>(egui::Id::new("m_clip_sheet_selection"));
                        d.insert_temp(egui::Id::new(("m_track_settings_page", lane)), 0_usize);
                        d.insert_temp(egui::Id::new("m_track_sheet_open"), true);
                    });
                }
            }
        }
        if let Some(index) = picked_clip {
            let clip = self.project.arrangement.clips[index];
            self.arr.sel_lane = clip.lane;
            if select && self.arr.is_sel(index) {
                self.arr.sel.remove(&index);
                if self.arr.sel_clip == Some(index) { self.arr.sel_clip = None; }
            } else {
                if select { self.arr.sel.insert(index); } else { self.arr.sel.clear(); }
                self.arr.sel_clip = Some(index);
                if clip.audio { self.project.arrangement.tracks[clip.lane].audio_brush = Some(clip.pattern); }
            }
        }
        if let Some(index) = open { self.m_open_playlist_clip(&ctx, index); }
        if let Some((index, clip)) = move_clip {
            self.m_playlist_undo_step();
            self.project.arrangement.clips[index] = clip;
            self.arr.sel_lane = clip.lane;
            self.arr.sel_clip = Some(index);
            self.arr.sel.clear();
        }
        if let Some((lane, tick)) = create.filter(|&(lane, _)| !self.project.arrangement.tracks[lane].playlist_locked) {
            self.arr.sel_lane = lane;
            self.arr.sel_clip = None;
            self.arr.sel.clear();
            self.m_insert_playlist_clip(&ctx, lane, tick);
        }
        if let Some((item, lane, tick)) = item_drop { self.m_drop_browser_item(&item, lane, tick); }
        if let Some((kind, lane)) = fx_drop { self.m_drop_effect(kind, lane); }
        self.m_vertical_clip_actions(ui, menu_id, snap, select || pinching);
        let options = egui::Id::new("m_playlist_track_names").with("track_modal");
        if let Some(lane) = ui.data(|d| d.get_temp::<usize>(options)).filter(|&lane| lane < self.project.arrangement.tracks.len()) {
            if !self.m_clip_piano_open(&ctx) && self.m_playlist_track_modal(&ctx, lane, options) {
                self.arr.sel_lane = lane;
                self.arr.sel_clip = None;
                self.arr.sel.clear();
                ctx.data_mut(|d| d.insert_temp(egui::Id::new("m_track_sheet_open"), true));
            }
        }
    }

    fn m_vertical_clip_actions(&mut self, ui: &mut Ui, id: egui::Id, snap: u32, disabled: bool) {
        use crate::arrangement::Clip;

        let Some((index, anchor)) = ui.data(|d| d.get_temp::<(usize, Pos2)>(id)) else { return; };
        let Some(clip) = self.project.arrangement.clips.get(index).copied().filter(|clip|
            !disabled && !self.project.arrangement.tracks[clip.lane].playlist_locked)
        else {
            ui.data_mut(|d| d.remove::<(usize, Pos2)>(id));
            return;
        };
        let longer = Clip { len: clip.len.saturating_add(snap), ..clip };
        let shorter = Clip { len: clip.len.saturating_sub(snap).max(1), ..clip };
        let actions = [
            (if clip.audio { Icon::Wave } else { Icon::Keys }, "Open clip", true),
            (Icon::Plus, "Extend by grid step", self.project.arrangement.can_place(&longer, Some(index))),
            (Icon::Minus, "Shorten by grid step", shorter.len < clip.len),
            (Icon::CloneClips, "Clone clip", true),
            (Icon::Trash, "Delete clip", true),
            (Icon::Settings, "MIDI clip settings", true),
        ];
        let actions = if clip.audio { &actions[..5] } else { &actions[..] };
        let ctx = ui.ctx().clone();
        if let Some(action) = playlist_clip_menu(&ctx, id, anchor, actions, true) {
            ui.data_mut(|d| d.remove::<(usize, Pos2)>(id));
            match action {
                0 => self.m_open_playlist_clip(&ctx, index),
                1 | 2 => {
                    self.m_playlist_undo_step();
                    self.project.arrangement.clips[index] = if action == 1 { longer } else { shorter };
                }
                3 => match arrangement_ui::duplicate_clip(&mut self.project, index) {
                    Some(copy) => {
                        self.m_playlist_undo_step();
                        self.arr.sel_clip = Some(copy);
                        self.arr.sel.clear();
                        let start = self.project.arrangement.clips[copy].start;
                        ui.data_mut(|d| d.insert_temp(egui::Id::new("m_playlist_reveal_tick"), start));
                    }
                    None => self.arr.msg = "No room for a copy on this track.".into(),
                },
                4 => {
                    self.m_playlist_undo_step();
                    self.project.arrangement.clips.remove(index);
                    self.arr.sel_clip = None;
                    self.arr.sel.clear();
                }
                5 => {
                    self.m_open_playlist_clip(&ctx, index);
                    ui.data_mut(|d| d.insert_temp(egui::Id::new("m_midi_clip_settings"), index));
                }
                _ => {}
            }
        }
    }

    fn m_insert_playlist_clip(&mut self, ctx: &egui::Context, lane: usize, tick: u32) {
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
                let sample = self.project.arrangement.tracks[lane].audio_brush.or_else(|| {
                    self.project.arrangement.clips.iter().rev()
                        .find(|clip| clip.audio && clip.lane == lane)
                        .map(|clip| clip.pattern)
                });
                if let Some(sample) = sample.filter(|&sample| {
                    self.project.audio_pool.get(sample).is_some_and(|entry| entry.data.is_some())
                }) {
                    let bpm = self.project.bpm;
                    match arrangement_ui::place_audio_from_pool(&mut self.project, sample, lane, start, bpm) {
                        Ok(index) => {
                            self.m_fit_audio_clip_to_grid(index);
                            self.arr.sel_clip = Some(index);
                            self.project.arrangement.tracks[lane].audio_brush = Some(sample);
                        }
                        Err(error) => self.arr.msg = error.into(),
                    }
                    return;
                }
                ctx.data_mut(|d| {
                    d.remove::<bool>(egui::Id::new("m_studio_full_view"));
                    d.insert_temp(egui::Id::new("m_audio_clip_target"), (lane, start));
                    d.insert_temp(egui::Id::new("m_browser_plugins"), false);
                    d.insert_temp(egui::Id::new("m_browser_open"), true);
                });
            }
        }
    }

    fn m_fx_automation_clip(&mut self, ui: &mut Ui, row: Rect, bar_width: f32, lane: usize, index: usize, pinching: bool) -> bool {
        let track = &self.project.arrangement.tracks[lane];
        let clip = &track.fx_automation[index];
        let id = egui::Id::new(("m_fx_automation_draw", lane, index));
        let drawing = ui.data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
        let editable = !track.playlist_locked && !pinching;
        let pixels_per_tick = bar_width / TICKS_PER_BAR as f32;
        let rect_at = |start: u32, len: u32| Rect::from_min_size(
            pos2(row.left() + start as f32 * pixels_per_tick, row.top() + 4.0),
            vec2((len as f32 * pixels_per_tick).max(2.0), row.height() - 8.0),
        );
        let rect = rect_at(clip.start, clip.len);
        if !rect.intersects(ui.clip_rect()) && !ui.ctx().is_being_dragged(id) {
            return false;
        }
        let hit = ui.interact(rect, id, if editable { Sense::click_and_drag() } else { Sense::hover() });
        let origin_id = id.with("drag");
        if !editable {
            ui.data_mut(|d| d.remove::<(f32, bool)>(origin_id));
        }
        if editable && !drawing && hit.drag_started() {
            if let Some(origin) = ui.input(|input| input.pointer.press_origin()) {
                let resize = origin.x >= rect.right() - (rect.width() * 0.4).min(18.0);
                ui.data_mut(|d| d.insert_temp(origin_id, (origin.x, resize)));
            }
        }
        let mut change = None;
        let mut preview = None;
        if editable && !drawing && (hit.dragged() || hit.drag_stopped()) {
            if let Some(((origin, resize), pointer)) = ui.data(|d| d.get_temp::<(f32, bool)>(origin_id)).zip(hit.interact_pointer_pos()) {
                let snap = self.arr.snap.max(1);
                let delta = ((pointer.x - origin) / pixels_per_tick / snap as f32).round() as i64 * i64::from(snap);
                let (start, len) = if resize {
                    (clip.start, (i64::from(clip.len) + delta).clamp(1, i64::from(TICKS_PER_BAR * 256)) as u32)
                } else {
                    ((i64::from(clip.start) + delta).clamp(0, i64::from(u32::MAX - clip.len)) as u32, clip.len)
                };
                let valid = track.fx_automation.iter().enumerate().all(|(i, other)| i == index
                    || other.effect != clip.effect || other.param != clip.param
                    || start >= other.end() || start.saturating_add(len) <= other.start);
                preview = Some((start, len, valid));
                if valid && hit.drag_stopped() && (start != clip.start || len != clip.len) {
                    change = Some((start, len));
                }
            }
        }
        if hit.drag_stopped() {
            ui.data_mut(|d| d.remove::<(f32, bool)>(origin_id));
        }
        let mut point = None;
        if editable && drawing && (hit.clicked() || hit.dragged()) {
            if let Some(pointer) = hit.interact_pointer_pos() {
                let tick = ((pointer.x - rect.left()) / pixels_per_tick).round().clamp(0.0, clip.len as f32) as u32;
                let value = ((rect.bottom() - 4.0 - pointer.y) / (rect.height() - 8.0).max(1.0)).clamp(0.0, 1.0);
                point = Some(crate::arrangement::AutoPoint { tick, v: value, curve: 0.0 });
            }
        }
        let (start, len, valid) = preview.unwrap_or((clip.start, clip.len, true));
        let shown = rect_at(start, len);
        let paint = ui.painter_at(shown);
        paint.rect_filled(shown, 6.0, pick(0xFF573C61, 0xFFD9BEDB));
        paint.rect_stroke(shown, 6.0, Stroke::new(if drawing { 2.0 } else { 1.0 }, if valid { p_accent() } else { RED }));
        let line = (0..=32).map(|step| {
            let tick = clip.len.saturating_mul(step) / 32;
            let level = crate::arrangement::auto_at(&clip.points, tick).unwrap_or(0.5).clamp(0.0, 1.0);
            pos2(shown.left() + shown.width() * step as f32 / 32.0,
                shown.bottom() - 4.0 - level * (shown.height() - 8.0))
        }).collect();
        paint.add(Shape::line(line, Stroke::new(1.8, p_accent())));
        if shown.width() >= 28.0 {
            paint.text(shown.left_top() + vec2(5.0, 3.0), Align2::LEFT_TOP, &clip.label, FontId::proportional(10.0), ink());
        }
        let mut clone = false;
        let mut remove = false;
        if editable {
            if hit.double_clicked() && !drawing {
                ui.data_mut(|d| d.insert_temp(id, true));
            }
            hit.context_menu(|ui| {
                if ui.button(if drawing { "Done drawing" } else { "Draw curve" }).clicked() {
                    ui.data_mut(|d| d.insert_temp(id, !drawing));
                    ui.close_menu();
                }
                if ui.button("Clone").clicked() { clone = true; ui.close_menu(); }
                if ui.button("Delete automation clip").clicked() { remove = true; ui.close_menu(); }
            });
        }
        if change.is_some() || clone || remove || (point.is_some() && (hit.drag_started() || hit.clicked())) {
            self.m_playlist_undo_step();
        }
        let clips = &mut self.project.arrangement.tracks[lane].fx_automation;
        if let Some((start, len)) = change {
            let clip = &mut clips[index];
            let old_len = u64::from(clip.len.max(1));
            for point in &mut clip.points {
                point.tick = (u64::from(point.tick) * u64::from(len) / old_len).min(u64::from(len)) as u32;
            }
            clip.points.dedup_by_key(|point| point.tick);
            clip.start = start;
            clip.len = len;
        }
        if let Some(point) = point {
            let points = &mut clips[index].points;
            match points.binary_search_by_key(&point.tick, |point| point.tick) {
                Ok(index) => points[index] = point,
                Err(index) => points.insert(index, point),
            }
        }
        if clone {
            let mut copy = clips[index].clone();
            while let Some(end) = clips.iter().filter(|other| other.overlaps(&copy)).map(|other| other.end()).max() {
                copy.start = end;
            }
            if copy.end() > copy.start {
                clips.push(copy);
            }
        }
        remove
    }

    fn m_playlist_track_modal(&mut self, ctx: &egui::Context, lane: usize, id: egui::Id) -> bool {
        let track = &self.project.arrangement.tracks[lane];
        let title = track.name.clone();
        let (muted_track, solo, grouped, locked) = (track.mute, track.solo, track.group != 0, track.playlist_locked);
        let can_delete = !locked && self.project.arrangement.tracks.len() > 1;
        let confirm_id = id.with("confirm_delete");
        let mut confirm = ctx.data(|d| d.get_temp::<bool>(confirm_id)).unwrap_or(false) && can_delete;
        let screen = ctx.screen_rect();
        let landscape = screen.width() >= screen.height();
        let width = if landscape {
            (screen.width() - 48.0).clamp(1.0, 208.0)
        } else {
            (screen.width() - 28.0).clamp(1.0, 460.0)
        };
        let mut close = ctx.input(|input| input.key_pressed(egui::Key::Escape));
        egui::Area::new(id.with("scrim")).order(egui::Order::Middle).fixed_pos(screen.min).movable(false)
            .show(ctx, |ui| {
                let (rect, hit) = ui.allocate_exact_size(screen.size(), Sense::click());
                ui.painter().rect_filled(rect, 0.0, Color32::from_black_alpha(145));
                close |= hit.clicked();
            });
        let mut main = false;
        let mut effects = false;
        let mut toggle_mute = false;
        let mut toggle_solo = false;
        let mut ungroup = false;
        let mut toggle_lock = false;
        let mut delete = false;
        let area = egui::Area::new(id.with("dialog")).order(egui::Order::Foreground).movable(false);
        let area = if landscape {
            area.fixed_pos(screen.min + vec2(4.0, 4.0))
        } else {
            area.anchor(Align2::CENTER_BOTTOM, [0.0, -8.0])
        };
        area.show(ctx, |ui| {
                let fill = if landscape { p_bg() } else { surface_fill() };
                let edge = if landscape { outline() } else { surface_edge() };
                Frame::none().fill(fill).stroke(Stroke::new(1.0, edge)).rounding(if landscape { 16.0 } else { 22.0 })
                    .inner_margin(12.0).show(ui, |ui| {
                        mobile_visuals(ui);
                        ui.set_width(width);
                        if !landscape {
                            let (grip, hit) = ui.allocate_exact_size(vec2(width, 14.0), Sense::drag());
                            let handle = Rect::from_center_size(grip.center(), vec2(34.0, 3.0));
                            ui.painter().rect_filled(handle, 1.5, muted());
                            if hit.dragged() && ui.input(|input| {
                                input.pointer.press_origin().zip(input.pointer.interact_pos())
                                    .is_some_and(|(start, end)| end.y - start.y > 30.0)
                            }) {
                                close = true;
                            }
                        }
                        ui.horizontal(|ui| {
                            let label = egui::Label::new(RichText::new(title).size(17.0).strong()).truncate();
                            ui.allocate_ui_with_layout(vec2((width - 52.0).max(1.0), 44.0),
                                egui::Layout::left_to_right(egui::Align::Center), |ui| ui.add(label));
                            close |= p_button(ui, "×", false, vec2(44.0, 44.0)).clicked();
                        });
                        mobile_scroll(egui::ScrollArea::vertical()).max_height(
                            if landscape { (screen.height() - 84.0).max(1.0) } else { (screen.height() * 0.58).max(1.0) })
                            .auto_shrink([false, true]).show(ui, |ui| {
                                let size = vec2(width, TOUCH);
                                if confirm {
                                    ui.label("Delete this track and its clips?");
                                    delete = p_button(ui, "Delete track", true, size).clicked();
                                    if p_button(ui, "Cancel", false, size).clicked() { confirm = false; }
                                    return;
                                }
                                main = p_button(ui, "Track settings", false, size).clicked();
                                effects = p_button(ui, "Effects", false, size).clicked();
                                toggle_mute = p_button(ui, if muted_track { "Enable track" } else { "Disable track" }, muted_track, size).clicked();
                                toggle_solo = p_button(ui, if solo { "Turn solo off" } else { "Solo" }, solo, size).clicked();
                                toggle_lock = p_button(ui, if locked { "Unlock track" } else { "Lock track" }, locked, size).clicked();
                                if can_delete {
                                    confirm = p_button(ui, "Delete track…", false, size).clicked();
                                }
                                if grouped && !locked {
                                    ungroup = p_button(ui, "Remove from group", false, size).clicked();
                                }
                            });
                    });
            });
        if toggle_mute || toggle_solo || ungroup || toggle_lock {
            self.m_playlist_undo_step();
            self.project.arrangement.tracks[lane].mute ^= toggle_mute;
            self.project.arrangement.tracks[lane].solo ^= toggle_solo;
            self.project.arrangement.tracks[lane].playlist_locked ^= toggle_lock;
            if toggle_lock {
                self.arr.sel_clip = None;
                self.arr.sel.clear();
                ctx.data_mut(|d| d.remove::<usize>(egui::Id::new("m_clip_sheet_selection")));
            }
            if ungroup {
                self.project.arrangement.ungroup_playlist_track(lane);
            }
            ctx.request_repaint();
        }
        if delete {
            self.m_playlist_undo_step();
            self.project.arrangement.remove_track(lane);
            self.project.session.remove_lane(lane);
            self.arr.sel_lane = lane.min(self.project.arrangement.tracks.len() - 1);
            self.arr.sel_clip = None;
            self.arr.sel.clear();
            ctx.data_mut(|d| d.remove::<usize>(egui::Id::new("m_clip_sheet_selection")));
            ctx.request_repaint();
        }
        if effects {
            ctx.data_mut(|d| {
                d.insert_temp(egui::Id::new(MIXER_CHANNEL), MobileMixerChannel::Track(lane));
                d.insert_temp(egui::Id::new(MIXER_PAGE), MobileMixerPage::Effects);
                d.insert_temp(egui::Id::new(MIXER_SHEET), true);
            });
        }
        if close || main || effects || ungroup || delete {
            ctx.data_mut(|d| {
                d.remove::<usize>(id);
                d.remove::<bool>(confirm_id);
            });
        } else {
            ctx.data_mut(|d| d.insert_temp(confirm_id, confirm));
        }
        if main {
            ctx.data_mut(|d| d.insert_temp(egui::Id::new(("m_track_settings_page", lane)), 0_usize));
        }
        main
    }

    fn m_playlist_automation(&mut self, ui: &mut Ui, area: Rect, bar_width: f32, bars: u32, pinching: bool, master: bool) {
        let visible = area.intersect(ui.clip_rect());
        if !visible.is_positive() {
            return;
        }
        let lane = self.arr.sel_lane;
        let response = ui.interact(visible, egui::Id::new(("playlist_volume_automation", lane, master)), Sense::click_and_drag());
        let stroke_id = response.id.with("last_tick");
        let snap = self.arr.snap.max(1);
        let last_tick = bars.saturating_mul(TICKS_PER_BAR);
        let plot = area.shrink2(vec2(0.0, 9.0));
        let to_x = |tick: u32| area.left() + tick as f32 / TICKS_PER_BAR as f32 * bar_width;
        let to_y = |level: f32| plot.bottom() - (level / 1.5).clamp(0.0, 1.0) * plot.height();
        let point = response.interact_pointer_pos().filter(|_| !pinching).map(|pos| {
            let tick = ((pos.x - area.left()).max(0.0) / bar_width * TICKS_PER_BAR as f32 / snap as f32).round() as u32;
            let value = ((plot.bottom() - pos.y) / plot.height()).clamp(0.0, 1.0) * 1.5;
            (tick.saturating_mul(snap).min(last_tick), (value * 100.0).round() / 100.0)
        });
        let previous = ui.data(|data| data.get_temp::<u32>(stroke_id));
        if let Some((tick, value)) = point.filter(|_| response.clicked() || response.dragged() || response.secondary_clicked()) {
            if previous.is_none() {
                self.m_playlist_undo_step();
            }
            let points = if master { &mut self.project.master_auto } else { &mut self.project.arrangement.tracks[lane].auto_vol };
            if response.secondary_clicked() {
                let hit = points.iter().enumerate().min_by_key(|(_, point)| point.tick.abs_diff(tick));
                if let Some((index, point)) = hit.filter(|(_, point)| (to_x(point.tick) - to_x(tick)).abs() <= 18.0) {
                    let _ = point;
                    points.remove(index);
                }
            } else {
                if let Some(from) = previous {
                    points.retain(|point| point.tick <= from.min(tick) || point.tick >= from.max(tick));
                }
                if points.is_empty() && tick > 0 {
                    points.push(crate::arrangement::AutoPoint { tick: 0, v: 1.0, curve: 0.0 });
                }
                match points.binary_search_by_key(&tick, |point| point.tick) {
                    Ok(index) => points[index].v = value,
                    Err(index) => points.insert(index, crate::arrangement::AutoPoint { tick, v: value, curve: 0.0 }),
                }
                if response.dragged() {
                    ui.data_mut(|data| data.insert_temp(stroke_id, tick));
                }
            }
        }
        if response.drag_stopped() || pinching {
            ui.data_mut(|data| data.remove::<u32>(stroke_id));
        }
        let points = if master { &self.project.master_auto } else { &self.project.arrangement.tracks[lane].auto_vol };
        let painter = ui.painter_at(visible);
        painter.rect_filled(visible, 0.0, p_bg().gamma_multiply(0.80));
        let accent = pick(0xFFFFD6A5, 0xFF803E29);
        let first = ((visible.left() - area.left()).max(0.0) / bar_width * TICKS_PER_BAR as f32) as u32;
        let last = ((visible.right() - area.left()).max(0.0) / bar_width * TICKS_PER_BAR as f32) as u32;
        let line = (0..=64).map(|index| {
            let tick = first + (last - first) * index / 64;
            pos2(to_x(tick), to_y(crate::arrangement::auto_at(points, tick).unwrap_or(1.0)))
        }).collect();
        painter.add(Shape::line(line, Stroke::new(2.0, accent)));
        for point in points.iter().filter(|point| point.tick >= first && point.tick <= last) {
            painter.circle_filled(pos2(to_x(point.tick), to_y(point.v)), 3.5, accent);
        }
        if !master {
            painter.text(visible.left_bottom() + vec2(7.0, -4.0), Align2::LEFT_BOTTOM,
                "Volume automation", FontId::proportional(11.0), ink());
        }
    }

    fn m_fit_audio_clip_to_grid(&mut self, index: usize) {
        let step = self.arr.snap.max(1);
        let clip = self.project.arrangement.clips[index];
        if clip.audio && step > 1 {
            let end = clip.end_tick().div_ceil(step).saturating_mul(step);
            let fitted = crate::arrangement::Clip { len: end - clip.start, ..clip };
            if self.project.arrangement.can_place(&fitted, Some(index)) {
                self.project.arrangement.clips[index] = fitted;
            }
        }
    }

    pub(super) fn m_playlist_undo_step(&mut self) {
        let now = web_time::Instant::now();
        self.last_undo = now.checked_sub(std::time::Duration::from_secs(1)).unwrap_or(now);
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
                Frame::none().fill(card_fill()).rounding(16.0).stroke(Stroke::new(1.0, grid_line()))
                    .inner_margin(12.0).show(ui, |ui| {
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
                                    panel(ui.painter(), r, 17.0);
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
                ui.add_space(8.0);
            }
        });
    }

    /// FL-style mixer. Header with options; left: the channel strips (Master + insert buses) with LED mute, pan knob,
    /// meter and fader; right: the dock of the selected strip (effect slots, routing, Master EQ).
    fn m_mixer(&mut self, ui: &mut Ui) {
        let channel_id = egui::Id::new(MIXER_CHANNEL);
        let dock_id = egui::Id::new("m_mixer_dock");
        let mut channel = ui.data(|d| d.get_temp::<MobileMixerChannel>(channel_id)).unwrap_or(MobileMixerChannel::Master);
        // a track opens the bus it plays through
        channel = match channel {
            MobileMixerChannel::Track(lane) => self.project.arrangement.tracks.get(lane).and_then(|track| track.output)
                .filter(|&insert| insert < NUM_INSERTS).map_or(MobileMixerChannel::Master, MobileMixerChannel::Insert),
            MobileMixerChannel::Insert(insert) if insert >= NUM_INSERTS => MobileMixerChannel::Master,
            other => other,
        };
        let mut dock = ui.data(|d| d.get_temp::<u8>(dock_id)).unwrap_or(0);
        if dock == 2 && channel != MobileMixerChannel::Master {
            dock = 0;
        }
        self.project.insert_fx.resize_with(NUM_INSERTS, Vec::new);
        let name = self.m_mixer_name(channel);
        // header: what is selected and which dock page shows
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.label(RichText::new("Mixer").size(16.0).strong());
            ui.add(egui::Label::new(RichText::new(format!("· {name}")).size(13.0).color(p_accent())).truncate());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 3.0;
                if channel == MobileMixerChannel::Master && p_button(ui, "EQ", dock == 2, vec2(44.0, 34.0)).clicked() {
                    dock = 2;
                }
                if p_button(ui, "Route", dock == 1, vec2(60.0, 34.0)).clicked() {
                    dock = 1;
                }
                if p_button(ui, "FX", dock == 0, vec2(44.0, 34.0)).clicked() {
                    dock = 0;
                }
            });
        });
        ui.add_space(4.0);
        let body = ui.available_rect_before_wrap();
        let split = (body.width() * 0.5).clamp(150.0, 420.0);
        let left = Rect::from_min_max(body.min, pos2(body.left() + split, body.bottom()));
        let right = Rect::from_min_max(pos2(left.right() + 6.0, body.top()), body.max);
        ui.allocate_rect(body, Sense::hover());
        let mut strips = ui.new_child(egui::UiBuilder::new().id_salt("m_mixer_strips").max_rect(left));
        strips.set_clip_rect(left.intersect(ui.clip_rect()));
        mobile_scroll(egui::ScrollArea::horizontal()).id_salt("m_mixer_strip_scroll").auto_shrink([false, false])
            .show(&mut strips, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    let height = left.height() - 4.0;
                    for strip in std::iter::once(MobileMixerChannel::Master).chain((0..NUM_INSERTS).map(MobileMixerChannel::Insert)) {
                        if self.m_mixer_strip(ui, strip, channel == strip, height) {
                            channel = strip;
                        }
                    }
                });
            });
        ui.painter().rect_filled(right, 10.0, card_fill());
        let mut panel = ui.new_child(egui::UiBuilder::new().id_salt("m_mixer_dock_ui").max_rect(right.shrink(6.0)));
        panel.set_clip_rect(right.intersect(ui.clip_rect()));
        mobile_scroll(egui::ScrollArea::vertical()).id_salt(("m_mixer_dock_scroll", dock)).auto_shrink([false, false])
            .show(&mut panel, |ui| {
                match dock {
                    1 => self.m_mixer_routing(ui, channel),
                    2 => self.m_mixer_master(ui),
                    _ => self.m_mixer_slots(ui, channel),
                }
                ui.add_space(16.0);
            });
        ui.data_mut(|d| {
            d.insert_temp(channel_id, channel);
            d.insert_temp(dock_id, dock);
        });
    }

    fn m_mixer_name(&self, channel: MobileMixerChannel) -> String {
        match channel {
            MobileMixerChannel::Insert(insert) => self.project.channels[NUM_CH + insert].name.clone(),
            _ => "Master".to_owned(),
        }
    }

    /// One FL-style strip; returns true when it was tapped.
    fn m_mixer_strip(&mut self, ui: &mut Ui, strip: MobileMixerChannel, selected: bool, height: f32) -> bool {
        let width = 40.0;
        let (rect, response) = ui.allocate_exact_size(vec2(width, height.max(160.0)), Sense::click());
        let id = ui.id().with(("m_mixer_strip", strip));
        let insert = match strip { MobileMixerChannel::Insert(insert) => Some(insert), _ => None };
        let p = ui.painter().clone();
        p.rect_filled(rect, 6.0, if selected { selected_fill() } else { card_fill().gamma_multiply(0.8) });
        if selected {
            p.rect_stroke(rect, 6.0, Stroke::new(1.5, p_accent()));
        }
        let mut tapped = response.clicked();
        // number box
        let number = Rect::from_min_size(rect.min + vec2(3.0, 3.0), vec2(width - 6.0, 18.0));
        p.rect_filled(number, 4.0, if insert.is_none() { p_accent().gamma_multiply(0.55) } else { bar_fill() });
        p.text(number.center(), Align2::CENTER_CENTER, insert.map_or("M".to_owned(), |i| (i + 1).to_string()),
            FontId::proportional(11.0), ink());
        // vertical name
        let name_h = (rect.height() * 0.2).clamp(40.0, 90.0);
        let name_rect = Rect::from_min_size(pos2(rect.left(), number.bottom() + 4.0), vec2(width, name_h));
        let mut job = egui::text::LayoutJob::single_section(self.m_mixer_name(strip),
            egui::TextFormat::simple(FontId::proportional(11.0), if selected { ink() } else { muted() }));
        job.wrap = egui::text::TextWrapping::truncate_at_width(name_h - 4.0);
        let galley = p.layout_job(job);
        let at = pos2(name_rect.center().x - galley.size().y * 0.5, name_rect.bottom() - 2.0);
        p.add(egui::epaint::TextShape::new(at, galley, ink()).with_angle(-std::f32::consts::FRAC_PI_2));
        // LED mute (green = playing)
        let mut y = name_rect.bottom() + 4.0;
        let led = Rect::from_center_size(pos2(rect.center().x, y + 9.0), vec2(22.0, 18.0));
        let muted_now = insert.is_some_and(|insert| self.project.channels[NUM_CH + insert].mute);
        p.circle_filled(led.center(), 5.0, if muted_now { argb(0xFF2A3440) } else { argb(0xFF5BE37A) });
        p.circle_stroke(led.center(), 5.0, Stroke::new(1.0, outline()));
        if let Some(insert) = insert {
            if ui.interact(led, id.with("led"), Sense::click()).on_hover_text("Mute").clicked() {
                let ch = &mut self.project.channels[NUM_CH + insert];
                ch.mute = !ch.mute;
                tapped = true;
            }
        }
        y = led.bottom() + 4.0;
        // pan knob (inserts)
        let knob = Rect::from_center_size(pos2(rect.center().x, y + 13.0), vec2(26.0, 26.0));
        if let Some(insert) = insert {
            let hit = ui.interact(knob, id.with("pan"), Sense::click_and_drag()).on_hover_text("Pan (double-tap: centre)");
            let pan = &mut self.project.channels[NUM_CH + insert].pan;
            if hit.dragged() {
                *pan = (*pan - hit.drag_delta().y * 0.01 + hit.drag_delta().x * 0.01).clamp(-1.0, 1.0);
                tapped = true;
            }
            if hit.double_clicked() {
                *pan = 0.0;
            }
            fl_knob(&p, knob, *pan * 0.5 + 0.5, true);
        }
        y = knob.bottom() + 6.0;
        // meter + fader
        let bottom_pad = 22.0;
        let lane = Rect::from_min_max(pos2(rect.left() + 3.0, y), pos2(rect.right() - 3.0, rect.bottom() - bottom_pad));
        if lane.height() > 30.0 {
            let peak = self.shared.peak(insert.map_or(crate::mixer::NUM_STRIPS, |insert| NUM_CH + insert));
            for (index, level) in peak.iter().enumerate() {
                let bar = Rect::from_min_size(pos2(lane.left() + index as f32 * 4.0, lane.top()), vec2(3.0, lane.height()));
                p.rect_filled(bar, 1.0, argb(0xFF10151C));
                let h = (level.clamp(0.0, 1.2) / 1.2) * bar.height();
                let fill = Rect::from_min_max(pos2(bar.left(), bar.bottom() - h), bar.max);
                p.rect_filled(fill, 1.0, if *level > 0.9 { argb(0xFFF2C14E) } else { argb(0xFF5BE37A) });
            }
            let track = Rect::from_min_max(pos2(lane.left() + 10.0, lane.top()), lane.max);
            let max = if insert.is_none() { 1.2 } else { 1.5 };
            let value = match insert {
                Some(insert) => &mut self.project.channels[NUM_CH + insert].volume,
                None => &mut self.project.master_volume,
            };
            let hit = ui.interact(track, id.with("fader"), Sense::click_and_drag()).on_hover_text("Volume");
            let travel = (track.height() - 14.0).max(1.0);
            if hit.dragged() || hit.clicked() {
                if let Some(pointer) = hit.interact_pointer_pos() {
                    *value = (((track.bottom() - 7.0 - pointer.y) / travel).clamp(0.0, 1.0)) * max;
                }
                tapped = true;
            }
            if hit.double_clicked() {
                *value = if insert.is_none() { 0.8 } else { 1.0 };
            }
            let t = (*value / max).clamp(0.0, 1.0);
            let cx = track.center().x;
            p.line_segment([pos2(cx, track.top() + 4.0), pos2(cx, track.bottom() - 4.0)], Stroke::new(2.0, argb(0xFF10151C)));
            let handle = Rect::from_center_size(pos2(cx, track.bottom() - 7.0 - t * travel), vec2(track.width() - 2.0, 12.0));
            p.rect_filled(handle, 2.0, if selected { p_accent() } else { argb(0xFFB9C2CF) });
            p.line_segment([handle.left_center(), handle.right_center()], Stroke::new(1.0, argb(0xFF2A3440)));
        }
        // dots: tracks feeding this strip
        let dots = Rect::from_min_max(pos2(rect.left(), rect.bottom() - bottom_pad), rect.max);
        let feeders: Vec<usize> = self.project.arrangement.tracks.iter().enumerate()
            .filter(|(_, track)| track.output.filter(|&o| o < NUM_INSERTS) == insert).map(|(lane, _)| lane).collect();
        for (index, lane) in feeders.iter().take(6).enumerate() {
            let at = pos2(dots.left() + 9.0 + (index % 3) as f32 * 11.0, dots.top() + 6.0 + (index / 3) as f32 * 9.0);
            p.circle_filled(at, 3.2, lane_col(*lane));
        }
        tapped
    }

    /// Right dock: the effect slots of the selected strip, FL style (name, wet knob, on/off LED).
    fn m_mixer_slots(&mut self, ui: &mut Ui, channel: MobileMixerChannel) {
        let insert = match channel { MobileMixerChannel::Insert(insert) => insert, _ => NUM_INSERTS };
        let open_id = egui::Id::new(("m_mixer_slot_open", insert));
        let mut open = ui.data(|d| d.get_temp::<usize>(open_id));
        let automation_lane = (insert < NUM_INSERTS).then(|| (0..self.project.arrangement.tracks.len()).find(|&lane|
            self.project.track_automation_insert(lane) == Some(insert) && !self.project.arrangement.tracks[lane].playlist_locked))
            .flatten();
        let mut create = None;
        let chain = if insert == NUM_INSERTS { &mut self.project.master_chain } else { &mut self.project.insert_fx[insert] };
        if chain.len() < plugins_ui::MAX_SLOTS {
            chain.resize_with(plugins_ui::MAX_SLOTS, crate::fx::FxSlot::empty);
        }
        for index in 0..plugins_ui::MAX_SLOTS {
            ui.push_id(("m_mixer_slot", insert, index), |ui| {
                let slot = &mut chain[index];
                let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 36.0), Sense::hover());
                let p = ui.painter();
                p.rect_filled(row.shrink2(vec2(0.0, 1.0)), 6.0, if open == Some(index) { selected_fill() } else { bar_fill() });
                let led = Rect::from_center_size(pos2(row.right() - 12.0, row.center().y), vec2(24.0, 30.0));
                let knob = Rect::from_center_size(pos2(row.right() - 40.0, row.center().y), vec2(26.0, 26.0));
                let label = Rect::from_min_max(row.min + vec2(8.0, 0.0), pos2(knob.left() - 4.0, row.bottom()));
                if slot.empty {
                    playlist_label(p, label, row.center().y, &format!("Slot {}", index + 1), muted());
                    let pick = ui.interact(label, ui.id().with("pick"), Sense::click());
                    egui::popup::popup_below_widget(ui, ui.id().with("menu"), &pick, egui::PopupCloseBehavior::CloseOnClick, |ui| {
                        ui.set_min_width(160.0);
                        mobile_scroll(egui::ScrollArea::vertical()).max_height(280.0).show(ui, |ui| {
                            for kind in crate::fx::ALL {
                                if ui.button(kind.name()).clicked() {
                                    *slot = crate::fx::FxSlot::new(kind);
                                    open = Some(index);
                                }
                            }
                        });
                    });
                    if pick.clicked() {
                        ui.memory_mut(|m| m.toggle_popup(ui.id().with("menu")));
                    }
                    return;
                }
                playlist_label(p, label, row.center().y, slot.kind.name(), ink());
                if ui.interact(label, ui.id().with("open"), Sense::click()).clicked() {
                    open = if open == Some(index) { None } else { Some(index) };
                }
                let wet = ui.interact(knob, ui.id().with("wet"), Sense::click_and_drag()).on_hover_text("Wet mix");
                if wet.dragged() {
                    slot.wet = (slot.wet - wet.drag_delta().y * 0.01 + wet.drag_delta().x * 0.01).clamp(0.0, 1.0);
                }
                if wet.double_clicked() {
                    slot.wet = 1.0;
                }
                fl_knob(ui.painter(), knob, slot.wet, false);
                if ui.interact(led, ui.id().with("on"), Sense::click()).on_hover_text("Enable effect").clicked() {
                    slot.on = !slot.on;
                }
                ui.painter().circle_filled(led.center(), 5.0, if slot.on { argb(0xFF5BE37A) } else { argb(0xFF2A3440) });
                ui.painter().circle_stroke(led.center(), 5.0, Stroke::new(1.0, outline()));
                if open == Some(index) {
                    Frame::none().inner_margin(Margin::symmetric(4.0, 4.0)).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            if ui.button("Remove").clicked() {
                                *slot = crate::fx::FxSlot::empty();
                                open = None;
                            }
                            if automation_lane.is_some() {
                                ui.menu_button("Automate", |ui| {
                                    for (param, (name, min, max, _)) in slot.kind.specs_full().into_iter().enumerate() {
                                        if (name == "Mix" || name.starts_with("Cutoff") || name == "Feedback") && ui.button(name).clicked() {
                                            create = Some((index, param, name, min, max));
                                            ui.close_menu();
                                        }
                                    }
                                });
                            }
                        });
                        if !slot.empty {
                            plugin_ui::plugin_grid(ui, slot);
                        }
                    });
                }
            });
        }
        // trailing empty slots are not saved
        while chain.last().is_some_and(|slot| slot.empty) {
            chain.pop();
        }
        ui.data_mut(|d| match open {
            Some(index) => d.insert_temp(open_id, index),
            None => d.remove::<usize>(open_id),
        });
        if let Some((lane, (index, param, name, min, max))) = automation_lane.zip(create) {
            self.m_create_fx_automation(ui.ctx(), lane, insert, index, param, name, min, max);
        }
    }

    /// Right dock, Route page: which tracks play through the selected strip, and moving tracks onto it.
    fn m_mixer_routing(&mut self, ui: &mut Ui, channel: MobileMixerChannel) {
        let insert = match channel { MobileMixerChannel::Insert(insert) => Some(insert), _ => None };
        heading(ui, "Tracks into this channel");
        let mut any = false;
        for lane in 0..self.project.arrangement.tracks.len() {
            let track = &mut self.project.arrangement.tracks[lane];
            if track.output.filter(|&o| o < NUM_INSERTS) != insert {
                continue;
            }
            any = true;
            ui.push_id(("m_mixer_feeder", lane), |ui| {
                ui.horizontal(|ui| {
                    let (dot, _) = ui.allocate_exact_size(vec2(10.0, 24.0), Sense::hover());
                    ui.painter().circle_filled(dot.center(), 4.0, lane_col(lane));
                    ui.add_sized([(ui.available_width() - 70.0).max(40.0), 24.0],
                        egui::Label::new(RichText::new(&track.name).size(12.0)).truncate());
                    mixer_ui::output_picker_w(ui, ("m_mixer_route", lane), &mut track.output, 64.0);
                });
                ui.horizontal(|ui| {
                    ui.add_space(14.0);
                    ui.spacing_mut().slider_width = (ui.available_width() - 44.0).max(40.0);
                    ui.add(egui::Slider::new(&mut track.volume, 0.0..=1.5).show_value(false));
                    if p_button(ui, "M", track.mute, vec2(30.0, 26.0)).clicked() {
                        track.mute = !track.mute;
                    }
                });
            });
        }
        if !any {
            ui.label(RichText::new("No tracks play through here yet.").size(11.0).color(muted()));
        }
        if let Some(target) = insert {
            let others: Vec<(usize, String)> = self.project.arrangement.tracks.iter().enumerate()
                .filter(|(_, track)| track.output != Some(target)).map(|(lane, track)| (lane, track.name.clone())).collect();
            if !others.is_empty() {
                ui.menu_button("+ Route a track here", |ui| {
                    for (lane, name) in others {
                        if ui.button(name).clicked() {
                            self.project.arrangement.tracks[lane].output = Some(target);
                            ui.close_menu();
                        }
                    }
                });
            }
            ui.add_space(8.0);
            let ch = &mut self.project.channels[NUM_CH + target];
            ui.label(RichText::new("Name").size(11.0).color(muted()));
            ui.add(egui::TextEdit::singleline(&mut ch.name).desired_width(ui.available_width()));
        }
    }

    fn m_create_fx_automation(&mut self, ctx: &egui::Context, lane: usize, insert: usize, index: usize,
        param: usize, name: &str, min: f32, max: f32)
    {
        let current_key = self.project.insert_fx[insert][index].automation_key;
        let effect = if current_key == 0 {
            let highest = self.project.insert_fx.iter().flatten().map(|slot| slot.automation_key)
                .chain(self.project.arrangement.tracks.iter().chain(
                    self.project.scenes.iter().flat_map(|scene| &scene.arrangement.tracks))
                    .flat_map(|track| &track.fx_automation).map(|clip| clip.effect))
                .max().unwrap_or(0);
            highest.saturating_add(1)
        } else {
            current_key
        };
        let selected = self.arr.sel_clip.and_then(|index| self.project.arrangement.clips.get(index)).filter(|clip| clip.lane == lane);
        let position = self.shared.position.load(Ordering::Relaxed);
        let (start, len) = selected.map_or_else(|| {
            if self.project.loop_on {
                (self.project.loop_start * TICKS_PER_BAR, self.project.loop_bars.max(1) * TICKS_PER_BAR)
            } else {
                (position / TICKS_PER_BAR * TICKS_PER_BAR, TICKS_PER_BAR)
            }
        }, |clip| (clip.start, clip.len));
        self.m_playlist_undo_step();
        let slot = &mut self.project.insert_fx[insert][index];
        slot.automation_key = effect;
        let v = ((slot.get(param) - min) / (max - min)).clamp(0.0, 1.0);
        let track = &mut self.project.arrangement.tracks[lane];
        let group = track.group;
        let mut clip = crate::arrangement::FxAutomationClip {
            effect, param, label: format!("{} · {name}", slot.kind.name()), start, len, min, max,
            points: vec![
                crate::arrangement::AutoPoint { tick: 0, v, curve: 0.0 },
                crate::arrangement::AutoPoint { tick: len, v, curve: 0.0 },
            ],
        };
        while let Some(end) = track.fx_automation.iter().filter(|other| other.overlaps(&clip)).map(|other| other.end()).max() {
            clip.start = end;
        }
        if clip.end() <= clip.start { return; }
        let start = clip.start;
        let index = track.fx_automation.len();
        track.fx_automation.push(clip);
        if let Some(folder) = self.project.arrangement.groups.iter_mut().find(|folder| folder.id == group) {
            folder.collapsed = false;
        }
        self.arr.sel_lane = lane;
        self.arr.auto_edit = false;
        self.m_select_studio_section(ctx, SEC_PLAYLIST);
        ctx.data_mut(|d| {
            d.remove::<bool>(egui::Id::new(MIXER_SHEET));
            d.insert_temp(egui::Id::new(("m_fx_automation_draw", lane, index)), true);
            d.insert_temp(egui::Id::new("m_playlist_reveal_track"), lane);
            d.insert_temp(egui::Id::new("m_playlist_reveal_tick"), start);
        });
        self.publish_edits();
        ctx.request_repaint();
    }

    fn m_mixer_effect_picker(ui: &mut Ui, label: &str) -> Option<crate::fx::FxKind> {
        let mut picked = None;
        let button = egui::Button::new(label).min_size(vec2(ui.available_width(), TOUCH));
        egui::menu::menu_custom_button(ui, button, |ui| {
            mobile_scroll(egui::ScrollArea::vertical()).max_height(280.0).show(ui, |ui| {
                for kind in crate::fx::ALL {
                    if ui.button(kind.name()).clicked() {
                        picked = Some(kind);
                        ui.close_menu();
                    }
                }
            });
        });
        picked
    }

    fn m_mixer_master(&mut self, ui: &mut Ui) {
        Frame::none().fill(card_fill()).rounding(12.0).inner_margin(10.0).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                widgets::meter(ui, vec2(24.0, 54.0), self.shared.peak(crate::mixer::NUM_STRIPS));
                ui.vertical(|ui| {
                    ui.label(RichText::new("Output level").strong());
                    ui.label(RichText::new(format!("{:+.1} dB", widgets::db(self.project.master_volume))).color(muted()));
                });
            });
        });
        let volume = &mut self.project.master_volume;
        let fx = &mut self.project.master_fx;
        let slider = |ui: &mut Ui, label, value: &mut f32, range, suffix| {
            ui.label(label);
            let width = ui.available_width();
            ui.add_sized([width, 30.0], egui::Slider::new(value, range).suffix(suffix));
        };
        slider(ui, "Volume", volume, 0.0..=1.2, "");
        slider(ui, "Low EQ", &mut fx.low_db, -18.0..=18.0, " dB");
        slider(ui, "High EQ", &mut fx.high_db, -18.0..=18.0, " dB");
        if p_button(ui, "Compressor", fx.comp_on, vec2(ui.available_width(), TOUCH)).clicked() {
            fx.comp_on = !fx.comp_on;
        }
        if fx.comp_on {
            slider(ui, "Threshold", &mut fx.threshold_db, -48.0..=0.0, " dB");
            slider(ui, "Ratio", &mut fx.ratio, 1.0..=20.0, "");
            slider(ui, "Makeup", &mut fx.makeup_db, 0.0..=24.0, " dB");
        }
    }

    // ───────────────────── Settings / Profile ─────────────────────

    fn m_settings(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        mobile_scroll(egui::ScrollArea::vertical()).auto_shrink([false, false]).show(ui, |ui| {
            heading(ui, "Theme");
            for theme in MobileTheme::ALL {
                if p_button(ui, theme.name(), MobileTheme::current() == theme, vec2(ui.available_width(), 42.0)).clicked() {
                    theme.save();
                    set_dark(theme != MobileTheme::Day);
                    save_prefs();
                    apply_visuals(ctx);
                    ctx.request_repaint();
                }
            }
            ui.add_space(10.0);
            heading(ui, "Sound");
            let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 64.0), Sense::hover());
            panel(ui.painter(), rect, 14.0);
            let status = match &self.audio.error {
                Some(e) => e.clone(),
                None if self.audio.has_stream() => format!("{} Hz  ·  output is running", self.audio.sample_rate),
                None => "No output".to_string(),
            };
            ui.painter().text(rect.left_top() + vec2(16.0, 14.0), Align2::LEFT_TOP, "Audio output", FontId::proportional(15.0), ink());
            ui.painter().text(rect.left_top() + vec2(16.0, 36.0), Align2::LEFT_TOP, status, FontId::proportional(12.0), muted());
            ui.add_space(6.0);
            // buffer: smaller = less delay, bigger = no crackles on a busy phone
            let latency = |frames: u32| frames as f32 / self.audio.sample_rate.max(1) as f32 * 1000.0;
            ui.label(RichText::new("Audio buffer (crackles? pick a bigger one)").size(12.0).color(muted()));
            let mut pick = None;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let w = (ui.available_width() - 8.0) / 3.0;
                for (frames, label) in [(512u32, "Fast"), (1024, "Stable"), (2048, "Extra safe")] {
                    let text = format!("{label}\n{:.0} ms", latency(frames));
                    if p_button(ui, &text, self.audio.buffer_pref == frames, vec2(w, 48.0)).clicked() {
                        pick = Some(frames);
                    }
                }
            });
            if p_button(ui, "Restart audio", false, vec2(ui.available_width(), TOUCH)).clicked() {
                pick = Some(self.audio.buffer_pref.max(256));
            }
            if let Some(frames) = pick {
                self.audio.reopen_now(frames);
            }
            ui.label(RichText::new(format!("Running buffers: {} · reconnects: {}",
                self.shared.callbacks.load(Ordering::Relaxed), self.audio.reopens)).size(11.0).color(muted()));
            ui.add_space(10.0);
            // master Punch: psychoacoustic bass + mono low end + soft clip, saved with the project and in exports
            ui.label(RichText::new("Punch (808 you can hear on earbuds and phone speakers)").size(12.0).color(muted()));
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let w = (ui.available_width() - 8.0) / 3.0;
                for (amount, label) in [(0.0f32, "Off"), (0.6, "On"), (1.0, "Max")] {
                    let on = (self.project.punch - amount).abs() < 0.05;
                    if p_button(ui, label, on, vec2(w, 44.0)).clicked() && !on {
                        self.m_playlist_undo_step();
                        self.project.punch = amount;
                        self.publish_edits();
                    }
                }
            });
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
            panel(ui.painter(), rect, 18.0);
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

/// Clip actions as one row of icons right above the tapped point (below it when there is no room above).
fn playlist_clip_menu(
    ctx: &egui::Context, id: egui::Id, anchor: Pos2, actions: &[(Icon, &str, bool)], portrait: bool,
) -> Option<usize> {
    let cancel = usize::MAX;
    if portrait {
        let mut action = None;
        let drawer = id.with("left_drawer");
        let close = mobile_side_editor(ctx, drawer, "Clip actions", egui::Align::Min, 0.78, |ui| {
            mobile_scroll(egui::ScrollArea::vertical()).show(ui, |ui| {
                for (index, &(symbol, label, enabled)) in actions.iter().enumerate().filter(|(index, _)| *index != 4).chain(actions.get(4).map(|action| (4, action))) {
                    let label = match index {
                        0 if actions.len() == 6 => "Piano Roll",
                        0 => "Audio clip settings",
                        _ => label,
                    };
                    if matches!(symbol, Icon::Trash) {
                        ui.separator();
                    }
                    if ui.add_enabled_ui(enabled, |ui| drawer_row(ui, symbol, label, false)).inner.clicked() {
                        action = Some(index);
                    }
                }
            });
        });
        if action.is_some() {
            ctx.animate_bool(drawer.with("animation"), false);
        }
        return action.or_else(|| close.then_some(cancel));
    }

    let screen = ctx.screen_rect();
    let gap = 2.0;
    let pad = 4.0;
    let count = (actions.len() + 1) as f32;
    let button_width = ((screen.width() - 8.0 - (count - 1.0) * gap - pad * 2.0) / count).clamp(1.0, 44.0);
    let button_height = 44.0;
    let size = vec2(count * button_width + (count - 1.0) * gap + pad * 2.0, button_height + pad * 2.0);
    let above = anchor.y - size.y - 14.0;
    let top = if above >= screen.top() + 4.0 { above } else { anchor.y + 14.0 };
    let min = pos2(anchor.x - size.x * 0.5, top)
        .clamp(screen.min + vec2(4.0, 4.0), (screen.max - size - vec2(4.0, 4.0)).max(screen.min + vec2(4.0, 4.0)));
    let bounds = Rect::from_min_size(min, size);
    let mut action = ctx.input(|input| input.key_pressed(egui::Key::Escape).then_some(cancel));
    // no full-screen catcher: a press outside the bar closes it and still reaches whatever is under the finger
    // (an invisible catcher in the same layer order could end up above the bar and swallow every tap)
    if ctx.input(|input| input.pointer.any_pressed() && input.pointer.press_origin().is_some_and(|at| !bounds.contains(at))) {
        action = Some(cancel);
        ctx.data_mut(|d| d.insert_temp(egui::Id::new(CLIP_MENU_DISMISSED), true));
    }
    egui::Area::new(id.with("bar")).order(egui::Order::Foreground).fixed_pos(bounds.min)
        .movable(false).show(ctx, |ui| {
            let (area, _) = ui.allocate_exact_size(bounds.size(), Sense::click());
            let p = ui.painter();
            p.rect_filled(area, 14.0, surface_fill());
            p.rect_stroke(area, 14.0, Stroke::new(1.0, surface_edge()));
            let slot = |index: usize| Rect::from_min_size(
                pos2(area.left() + pad + index as f32 * (button_width + gap), area.top() + pad),
                vec2(button_width, button_height));
            for (index, &(symbol, label, enabled)) in actions.iter().enumerate() {
                let rect = slot(index);
                let hit = ui.interact(rect, id.with(index), if enabled { Sense::click() } else { Sense::hover() })
                    .on_hover_text(label);
                if hit.hovered() && enabled {
                    p.rect_filled(rect, 9.0, selected_fill());
                }
                let color = if !enabled { muted().gamma_multiply(0.55) } else if index == 4 { RED } else { ink() };
                icon(p, symbol, rect.center(), color);
                if hit.clicked() && enabled {
                    action = Some(index);
                }
            }
            let close = slot(actions.len());
            let x = close.left() - gap * 0.5 - 0.5;
            p.line_segment([pos2(x, close.top() + 8.0), pos2(x, close.bottom() - 8.0)], Stroke::new(1.0, outline()));
            if ui.interact(close, id.with("close"), Sense::click()).on_hover_text("Close clip actions").clicked() {
                action = Some(cancel);
            }
            p.text(close.center(), Align2::CENTER_CENTER, "×", FontId::proportional(21.0), ink());
        });
    action
}

/// One line of text cut to `rect`, vertically centred on `y`.
fn fit_label(painter: &egui::Painter, rect: Rect, y: f32, text: &str, color: Color32, size: f32) {
    if rect.width() < 15.0 || !rect.is_positive() {
        return;
    }
    let mut job = egui::text::LayoutJob::single_section(text.to_owned(), egui::TextFormat::simple(FontId::proportional(size), color));
    job.wrap = egui::text::TextWrapping::truncate_at_width(rect.width());
    let galley = painter.layout_job(job);
    painter.with_clip_rect(rect).galley(pos2(rect.left(), y - galley.size().y * 0.5), galley, color);
}

fn playlist_label(painter: &egui::Painter, rect: Rect, y: f32, text: &str, color: Color32) {
    if rect.width() < 15.0 || !rect.is_positive() {
        return;
    }
    let mut job = egui::text::LayoutJob::single_section(text.to_owned(),
        egui::TextFormat::simple(FontId::proportional(11.0), color));
    job.wrap = egui::text::TextWrapping::truncate_at_width(rect.width());
    let galley = painter.layout_job(job);
    let at = pos2(rect.left(), y - galley.size().y * 0.5);
    painter.with_clip_rect(rect).galley(at, galley, color);
}

fn playlist_midi_preview(painter: &egui::Painter, rect: Rect, pattern: &crate::sequencer::Pattern, offset: u32, length: u32) {
    let visible = rect.intersect(painter.clip_rect());
    if !visible.is_positive() || length == 0 {
        return;
    }
    let (low, high) = pattern.notes.iter().fold((127, 0), |(lo, hi), note| (lo.min(note.pitch), hi.max(note.pitch)));
    let pitch_span = high.saturating_sub(low).max(6) as f32;
    let origin = u64::from(offset);
    let end = origin + u64::from(length);
    let period = u64::from(pattern.len_ticks().max(1));
    let to_tick = |x: f32| origin + ((x - rect.left()) / rect.width() * length as f32) as u64;
    let left = to_tick(visible.left());
    let right = to_tick(visible.right()).saturating_add(1).min(end);
    let x_at = |tick: u64| rect.left() + (tick - origin) as f32 / length as f32 * rect.width();
    let preview = painter.with_clip_rect(visible);
    for cycle in left / period..=right / period {
        let base = cycle * period;
        for note in pattern.notes.iter().filter(|note| !note.mute && u64::from(note.start) < period) {
            let start = base + u64::from(note.start);
            let stop = (start + u64::from(note.len)).min(base + period);
            if stop <= left || start >= right {
                continue;
            }
            let y = rect.bottom() - 1.0 - f32::from(note.pitch - low) / pitch_span * (rect.height() - 2.0).max(0.0);
            let x = x_at(start.max(origin));
            let x_end = x_at(stop.min(end)).max(x + 2.0);
            preview.line_segment([pos2(x, y), pos2(x_end, y)], Stroke::new(1.8, Color32::from_black_alpha(150)));
        }
    }
}

fn piano_slide_mark(painter: &egui::Painter, note: Rect) {
    let size = (note.width() - 5.0).min(note.height() - 4.0).clamp(0.0, 6.0);
    if size >= 3.0 {
        let anchor = note.left_bottom() + vec2(2.0, -2.0);
        painter.add(Shape::convex_polygon(
            vec![anchor, anchor + vec2(size, 0.0), anchor + vec2(size, -size)],
            Color32::from_gray(24), Stroke::NONE,
        ));
    }
}

/// One Piano Roll toolbar group: a wrapping row in portrait, a column in the landscape sidebar.
/// (A `left_to_right(Center)` layout in portrait would take the whole remaining height, push the note grid off the
/// screen and leave the buttons floating in the middle.)
fn piano_toolbar_row(ui: &mut Ui, portrait: bool, add: impl FnOnce(&mut Ui)) {
    if portrait {
        ui.horizontal(add);
    } else {
        ui.with_layout(egui::Layout::top_down(egui::Align::Center), add);
    }
}

/// A full-width phone action: a title and one line on what it does.
pub(super) fn mobile_action(ui: &mut Ui, title: &str, what: &str) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 56.0), Sense::click());
    let p = ui.painter();
    let card = rect.shrink2(vec2(0.0, 2.0));
    p.rect_filled(card, 10.0, if response.is_pointer_button_down_on() { selected_fill() } else { card_fill() });
    p.rect_stroke(card, 10.0, Stroke::new(1.0, outline()));
    let text = Rect::from_min_max(card.min + vec2(12.0, 0.0), pos2(card.right() - 26.0, card.bottom()));
    fit_label(p, text, card.top() + 18.0, title, ink(), 15.0);
    fit_label(p, text, card.top() + 37.0, what, muted(), 11.0);
    p.text(card.right_center() - vec2(12.0, 0.0), Align2::RIGHT_CENTER, "›", FontId::proportional(20.0), muted());
    response
}

fn playlist_clip_playback(project: &Project, clip: &crate::arrangement::Clip, tick: u32) -> Option<f32> {
    if clip.mute || clip.gain <= 0.0 || !(clip.start..clip.end_tick()).contains(&tick) {
        return None;
    }
    let track = project.arrangement.tracks.get(clip.lane)?;
    if track.volume <= 0.0 {
        return None;
    }

    let active = if clip.audio {
        !project.arrangement.muted(clip.lane)
            && project.audio_pool.get(clip.pattern).is_some_and(|sample| sample.data.is_some())
    } else {
        !project.arrangement.silent(clip.lane) && project.patterns.get(clip.pattern).is_some_and(|pattern| {
            let local = (tick - clip.start).saturating_add(clip.offset) % pattern.len_ticks().max(1);
            let step = (local / TICKS_PER_STEP) as usize;
            pattern.notes.iter().any(|note| !note.mute && note.vel > 0 && note.chance > 0
                && (note.start..note.start.saturating_add(note.len)).contains(&local))
                || pattern.drums.iter().any(|lane| lane.get(step).is_some_and(|cell| cell.on && cell.vel > 0 && cell.prob > 0))
        })
    };
    active.then(|| (tick - clip.start) as f32 / clip.len.max(1) as f32)
}

/// Beat-synced highlight and clip progress, painted under the clip contents.
fn paint_playlist_playback(painter: &Painter, rect: Rect, color: Color32, progress: f32, tick: u32, vertical: bool) {
    if !rect.is_positive() {
        return;
    }
    let beat = crate::sequencer::PPQ;
    let phase = (tick % beat) as f32 / beat as f32;
    let pulse = (1.0 - phase).powi(2);
    let signal = tint(color, ink(), 0.38);
    let alpha = (18.0 + pulse * 28.0).round() as u8;
    painter.rect_filled(rect, 5.0, signal.gamma_multiply(f32::from(alpha) / 255.0));
    painter.rect_stroke(rect.shrink(0.6), 5.0, Stroke::new(1.4 + pulse * 0.8, signal));
    let progress_color = signal.gamma_multiply(0.66);
    if vertical {
        let y = egui::lerp(rect.y_range(), progress);
        painter.hline(rect.shrink(4.0).x_range(), y, Stroke::new(1.5, progress_color));
    } else {
        let x = egui::lerp(rect.x_range(), progress);
        painter.vline(x, rect.shrink(4.0).y_range(), Stroke::new(1.5, progress_color));
    }
}

/// Clip / step body polish: a light-to-dark sheen down the body and a track-colour edge on the left.
fn clip_shine(p: &Painter, rect: Rect, color: Color32, muted_clip: bool) {
    let body = rect.shrink2(vec2(1.0, 2.0));
    if body.is_positive() {
        let mut mesh = egui::Mesh::default();
        let top = Color32::from_white_alpha(if muted_clip { 3 } else { 9 });
        let bottom = Color32::from_black_alpha(if muted_clip { 8 } else { 26 });
        mesh.colored_vertex(body.left_top(), top);
        mesh.colored_vertex(body.right_top(), top);
        mesh.colored_vertex(body.right_bottom(), bottom);
        mesh.colored_vertex(body.left_bottom(), bottom);
        mesh.add_triangle(0, 1, 2);
        mesh.add_triangle(0, 2, 3);
        p.add(Shape::mesh(mesh));
    }
    if rect.width() > 10.0 && rect.height() > 8.0 {
        let edge = Rect::from_min_size(rect.min, vec2(3.0, rect.height()));
        p.rect_filled(edge, egui::Rounding { nw: 6.0, sw: 6.0, ne: 0.0, se: 0.0 }, if muted_clip { color.gamma_multiply(0.4) } else { color });
    }
}

/// FL-style knob: dark disc with an indicator; `t` in 0..1 (pan: 0.5 is centre, drawn from the top).
fn fl_knob(p: &Painter, rect: Rect, t: f32, bipolar: bool) {
    let c = rect.center();
    let r = rect.width().min(rect.height()) * 0.5 - 2.0;
    p.circle_filled(c, r, argb(0xFF2A3440));
    p.circle_stroke(c, r, Stroke::new(1.0, outline()));
    let angle = |t: f32| std::f32::consts::PI * (0.75 + 1.5 * t.clamp(0.0, 1.0));
    let from = if bipolar { 0.5 } else { 0.0 };
    let steps = 16;
    let arc: Vec<Pos2> = (0..=steps).map(|i| {
        let a = angle(from + (t - from) * i as f32 / steps as f32);
        c + vec2(a.cos(), a.sin()) * (r + 1.0)
    }).collect();
    p.add(Shape::line(arc, Stroke::new(2.0, p_accent())));
    let a = angle(t);
    p.line_segment([c, c + vec2(a.cos(), a.sin()) * (r - 2.0)], Stroke::new(2.0, ink()));
}

/// The chevron that opens (points up) or closes (points down) the landscape studio controls.
fn drawer_chevron(ui: &mut Ui, width: f32, open: bool) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(width, 44.0), Sense::click());
    let center = rect.center();
    let direction = if open { 1.0 } else { -1.0 };
    ui.painter().add(Shape::line(
        vec![center + vec2(-7.0, -direction * 4.0), center + vec2(0.0, direction * 3.0), center + vec2(7.0, -direction * 4.0)],
        Stroke::new(2.0, ink()),
    ));
    response.on_hover_text(if open { "Hide studio controls" } else { "Show studio controls" })
}

fn paint_dock_icon(ui: &Ui, response: &Response, symbol: Icon, active: bool) {
    let rect = response.rect.shrink(2.0);
    let p = ui.painter();
    let color = if active { on_accent() } else { ink() };
    let radius = rect.height() * 0.5;
    p.rect_filled(rect, radius, if active { p_accent() } else { card_fill() });
    control_state(p, rect, radius, response, color);
    icon(p, symbol, rect.center(), color);
}

fn drum_step_graph_value(step: &crate::sequencer::Step, graph: u8) -> (f32, bool) {
    match graph {
        0 => (f32::from(step.vel) / 127.0, false),
        1 => ((f32::from(step.pan) + 100.0) / 200.0, true),
        2 => ((f32::from(step.pitch) + 24.0) / 48.0, true),
        _ => (f32::from(step.prob) / 100.0, false),
    }
}

fn set_drum_step_graph_value(step: &mut crate::sequencer::Step, graph: u8, value: f32) {
    match graph {
        0 => step.vel = (value * 127.0).round().max(1.0) as u8,
        1 => step.pan = (value * 200.0 - 100.0).round() as i8,
        2 => step.pitch = (value * 48.0 - 24.0).round() as i8,
        _ => step.prob = (value * 100.0).round().max(1.0) as u8,
    }
}

fn sidebar_caption(ui: &mut Ui, label: &str) {
    sidebar_caption_color(ui, label, muted());
}

fn sidebar_caption_color(ui: &mut Ui, label: &str, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(TOUCH + 4.0, 12.0), Sense::hover());
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, label, FontId::proportional(8.5), color);
}

fn dock_icon(ui: &mut Ui, symbol: Icon, active: bool, label: &str) -> Response {
    let (_, response) = ui.allocate_exact_size(vec2(TOUCH, TOUCH), Sense::click());
    paint_dock_icon(ui, &response, symbol, active);
    response.on_hover_text(label)
}

fn dock_menu(ui: &mut Ui, symbol: Icon, label: &str, active: bool, body: impl FnOnce(&mut Ui)) {
    let button = egui::Button::new("").frame(false).min_size(vec2(TOUCH, TOUCH));
    let menu = egui::menu::menu_custom_button(ui, button, body);
    paint_dock_icon(ui, &menu.response, symbol, active);
    menu.response.on_hover_text(label);
}

/// A round button like the dock icons; a disabled one is drawn dimmed instead of with a dark square over it.
fn round_icon(ui: &mut Ui, width: f32, symbol: Icon, enabled: bool, label: &str) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(width.max(TOUCH), TOUCH), Sense::click());
    let circle = Rect::from_center_size(rect.center(), vec2(TOUCH, TOUCH)).shrink(2.0);
    let p = ui.painter();
    let radius = circle.height() * 0.5;
    p.rect_filled(circle, radius, card_fill());
    let color = if enabled { ink() } else { muted().gamma_multiply(0.55) };
    if enabled {
        control_state(p, circle, radius, &response, color);
    }
    icon(p, symbol, circle.center(), color);
    response.on_hover_text(label)
}

/// The name a phone Playlist column shows: generic names ("MIDI 3", "Audio 1") give way to the instrument or the first
/// clip on the track, so narrow columns show "808 Bass" or "Kick" instead of "MI…".
fn vertical_track_label(project: &crate::project::Project, lane: usize) -> String {
    let Some(track) = project.arrangement.tracks.get(lane) else { return String::new() };
    let generic = ["MIDI ", "Audio ", "Track "].iter().any(|prefix| track.name.strip_prefix(prefix)
        .is_some_and(|n| n.trim().parse::<u32>().is_ok()));
    if !generic {
        return track.name.clone();
    }
    if let Some(instrument) = track.instrument {
        return instrument.name().to_string();
    }
    project.arrangement.clips.iter().filter(|c| c.lane == lane)
        .find_map(|c| if c.audio { project.audio_pool.get(c.pattern).map(|s| s.name.clone()) }
            else { project.patterns.get(c.pattern).map(|p| p.name.clone()) })
        .filter(|n| !n.starts_with("MIDI clip ") && !n.is_empty())
        .unwrap_or_else(|| track.name.clone())
}

fn studio_icon(ui: &mut Ui, width: f32, symbol: Icon, active: bool, label: &str) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(width, 44.0), Sense::click());
    let p = ui.painter();
    let foreground = if active { on_accent() } else { ink() };
    if active {
        pill(p, rect, p_accent());
    }
    control_state(p, rect, rect.height() * 0.5, &response, foreground);
    icon(p, symbol, rect.center(), foreground);
    response.on_hover_text(label)
}

fn mobile_side_editor(ctx: &egui::Context, id: egui::Id, title: &str, side: egui::Align, width_fraction: f32, body: impl FnOnce(&mut Ui)) -> bool {
    let screen = ctx.screen_rect();
    let width = (screen.width() * width_fraction).min(440.0);
    let shown = ctx.animate_bool(id.with("animation"), true);
    let right = side == egui::Align::Max;
    let x = if right { screen.right() - width * shown } else { screen.left() - width * (1.0 - shown) };
    let mut done = false;
    egui::Area::new(id.with("scrim")).order(egui::Order::Middle).fixed_pos(screen.min)
        .movable(false).show(ctx, |ui| {
            let (rect, response) = ui.allocate_exact_size(screen.size(), Sense::click());
            ui.painter().rect_filled(rect, 0.0, Color32::from_black_alpha((120.0 * shown) as u8));
            done |= response.clicked();
        });
    egui::Area::new(id).order(egui::Order::Foreground).constrain(false).movable(false)
        .fixed_pos(pos2(x, screen.top())).show(ctx, |ui| {
            let (rect, _) = ui.allocate_exact_size(vec2(width, screen.height()), Sense::click());
            let corners = if right { egui::Rounding { nw: 18.0, sw: 18.0, ..Default::default() } }
                else { egui::Rounding { ne: 18.0, se: 18.0, ..Default::default() } };
            ui.painter().rect_filled(rect, corners, mobile_color(0xFF191628, 0xFF272D35, 0xFFF1EEFB, 0xFF1B1B1E, 0xFF171F13));
            ui.painter().rect_stroke(rect, corners, Stroke::new(1.0, surface_edge().gamma_multiply(0.7)));
            let header = Rect::from_min_size(rect.min, vec2(width, 54.0));
            let header_corners = egui::Rounding { sw: 0.0, se: 0.0, ..corners };
            ui.painter().rect_filled(header, header_corners, surface_fill());
            ui.painter().hline(header.x_range(), header.bottom(), Stroke::new(1.0, surface_edge()));
            let mut head = ui.new_child(egui::UiBuilder::new().id_salt("header")
                .max_rect(header.shrink2(vec2(9.0, 8.0))));
            head.set_clip_rect(header.intersect(screen));
            mobile_visuals(&mut head);
            head.horizontal(|ui| {
                done |= p_button(ui, "Done", true, vec2(57.0, 36.0)).clicked();
                ui.add(egui::Label::new(RichText::new(title).size(17.0).strong()).truncate());
            });
            let mut inner = rect.shrink(9.0);
            inner.min.y = header.bottom() + 8.0;
            let mut content = ui.new_child(egui::UiBuilder::new().id_salt("content").max_rect(inner));
            content.set_clip_rect(inner.intersect(screen));
            mobile_visuals(&mut content);
            body(&mut content);
        });
    done |= ctx.input(|input| input.key_pressed(egui::Key::Escape));
    if done {
        ctx.animate_bool(id.with("animation"), false);
    }
    done
}

fn mobile_effect_sheet(ctx: &egui::Context, id: egui::Id, title: &str, body: impl FnOnce(&mut Ui)) -> bool {
    let screen = ctx.screen_rect();
    let height = (screen.height() * 0.72).min(560.0);
    let mut close = ctx.input(|input| input.key_pressed(egui::Key::Escape));
    let layer = egui::LayerId::new(egui::Order::Foreground, id);
    ctx.memory_mut(|m| m.areas_mut().move_to_top(layer));
    egui::Area::new(id).order(layer.order).fixed_pos(screen.min).movable(false).constrain(false)
        .show(ctx, |ui| {
            let (rect, _) = ui.allocate_exact_size(screen.size(), Sense::hover());
            let top = rect.bottom() - height;
            let backdrop = Rect::from_min_max(rect.min, pos2(rect.right(), top));
            close |= ui.interact(backdrop, id.with("backdrop"), Sense::click()).clicked();
            ui.painter().rect_filled(rect, 0.0, Color32::from_black_alpha(125));
            let sheet = Rect::from_min_max(pos2(rect.left(), top), rect.max);
            let corners = egui::Rounding { nw: 18.0, ne: 18.0, ..Default::default() };
            ui.painter().rect_filled(sheet, corners, p_bg());
            ui.painter().rect_stroke(sheet, corners, Stroke::new(1.0, surface_edge()));
            ui.interact(sheet, id.with("surface"), Sense::click());
            let mut content = ui.new_child(egui::UiBuilder::new().max_rect(sheet.shrink(12.0)));
            content.set_clip_rect(sheet.intersect(screen));
            mobile_visuals(&mut content);
            content.horizontal(|ui| {
                close |= p_button(ui, "Done", true, vec2(57.0, 36.0)).clicked();
                ui.label(RichText::new(title).size(18.0).strong());
            });
            content.separator();
            body(&mut content);
        });
    close
}

fn mobile_editor_sheet(ctx: &egui::Context, id: &'static str, title: &str, body: impl FnOnce(&mut Ui)) -> bool {
    let screen = ctx.screen_rect();
    let size = vec2((screen.width() - 22.0).max(1.0), (screen.height() * 0.76).max(96.0));
    let mut done = false;
    egui::Window::new(title).id(egui::Id::new(id)).title_bar(false).resizable(false).movable(false)
        .anchor(Align2::CENTER_BOTTOM, [0.0, -8.0]).fixed_size(size)
        .frame(Frame::none().fill(p_bg()).stroke(Stroke::new(1.0, outline())).rounding(16.0).inner_margin(10.0))
        .show(ctx, |ui| {
            mobile_visuals(ui);
            ui.horizontal(|ui| {
                let title_width = (ui.available_width() - 76.0).max(1.0);
                ui.add_sized([title_width, 32.0], egui::Label::new(RichText::new(title).size(17.0).strong().color(ink())).truncate());
                done = p_button(ui, "Done", true, vec2(68.0, 32.0)).clicked();
            });
            body(ui);
        });
    done || ctx.input(|input| input.key_pressed(egui::Key::Escape))
}

/// one sample / plugin row of the phone browser: full width, drag the dots on the left to drop it on the Playlist
fn portrait_view(ui: &Ui) -> bool {
    let screen = ui.ctx().screen_rect();
    screen.width() < screen.height()
}

/// A browser list row: ▷ (samples only, filled while previewing), the name, and a drag grip on the right.
/// Returns the row response and whether ▷ was tapped.
fn browser_list_row<T: std::any::Any + Send + Sync>(ui: &mut Ui, label: &str, preview: Option<bool>, item: T) -> (Response, bool) {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(vec2(width, 36.0), Sense::click_and_drag());
    let grip_rect = Rect::from_min_max(pos2(rect.right() - 26.0, rect.top()), rect.max);
    let grip = ui.interact(grip_rect, response.id.with("grip"), Sense::drag());
    let mut item = Some(item);
    if grip.dragged() {
        if let Some(item) = item.take() {
            grip.dnd_set_drag_payload(item);
        }
    }
    // the whole row: a sideways drag picks the item up (drop it on a Playlist track), an up/down drag scrolls the list
    let gesture = response.id.with("gesture");
    if response.drag_started() {
        ui.data_mut(|d| d.remove::<bool>(gesture));
    }
    if response.dragged() {
        let mut sideways = ui.data(|d| d.get_temp::<bool>(gesture));
        if sideways.is_none() {
            let moved = ui.input(|i| i.pointer.press_origin().zip(i.pointer.interact_pos()).map(|(a, b)| b - a));
            if let Some(moved) = moved.filter(|m| m.length() >= 8.0) {
                sideways = Some(moved.x.abs() > moved.y.abs());
                ui.data_mut(|d| d.insert_temp(gesture, moved.x.abs() > moved.y.abs()));
            }
        }
        match sideways {
            Some(true) => if let Some(item) = item.take() {
                egui::DragAndDrop::set_payload(ui.ctx(), item);
            },
            Some(false) => ui.scroll_with_delta(vec2(0.0, response.drag_delta().y)),
            None => {}
        }
    }
    let p = ui.painter();
    if response.is_pointer_button_down_on() {
        p.rect_filled(rect.shrink2(vec2(0.0, 1.0)), 6.0, selected_fill());
    }
    p.line_segment([rect.left_bottom(), rect.right_bottom()], Stroke::new(0.5, grid_line()));
    let mut text_left = rect.left() + 6.0;
    let mut played = false;
    if let Some(playing) = preview {
        let play_rect = Rect::from_min_size(rect.min, vec2(30.0, rect.height()));
        let c = play_rect.center();
        let tri = vec![c + vec2(-4.0, -6.0), c + vec2(6.0, 0.0), c + vec2(-4.0, 6.0)];
        if playing {
            p.add(Shape::convex_polygon(tri, p_accent(), Stroke::NONE));
        } else {
            p.add(Shape::closed_line(tri, Stroke::new(1.3, muted())));
        }
        played = ui.interact(play_rect, response.id.with("play"), Sense::click()).on_hover_text("Listen").clicked();
        text_left = play_rect.right();
    }
    for col in [-3.0, 3.0] {
        for row in [-6.0, 0.0, 6.0] {
            p.circle_filled(grip_rect.center() + vec2(col, row), 1.4, muted());
        }
    }
    let text = Rect::from_min_max(pos2(text_left, rect.top()), pos2(grip_rect.left() - 2.0, rect.bottom()));
    fit_label(p, text, rect.center().y, label, if preview == Some(true) { p_accent() } else { ink() }, 13.0);
    (response.union(grip), played)
}

