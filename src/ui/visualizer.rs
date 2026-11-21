//! Audio visualizer (like Avee Player): live spectrum / waveform analysis of the engine output with several
//! visual templates and colour themes. Works in a window or full screen.
use super::theme::*;
use super::widgets::{gradient_rect, lerp_color};
use crate::audio_engine::device::Shared;
use egui::{pos2, vec2, Align2, Color32, FontId, Key, Pos2, Rect, Shape, Stroke, Ui};
use std::f32::consts::{PI, TAU};

const BANDS: usize = 64;
const FFT_N: usize = 2048;
pub const TEMPLATES: [&str; 6] = ["Spectrum Bars", "Circle Pulse", "Waveform Ring", "Synthwave Grid", "Particles", "Phonk Speed"];
pub const THEMES: [&str; 4] = ["Neon", "Phonk Red", "Ice", "Sunset"];

struct Particle {
    pos: Pos2,
    vel: egui::Vec2,
    life: f32,
    hue: f32,
    size: f32,
}

pub struct VizState {
    pub open: bool,
    pub fullscreen: bool,
    pub template: usize,
    pub theme: usize,
    pub sens: f32,
    pub smooth: f32,
    bands: Vec<f32>,
    peaks: Vec<f32>,
    wave: Vec<f32>,
    bass: f32,
    avg_bass: f32,
    beat: f32,
    cooldown: f32,
    energy: f32,
    t: f32,
    particles: Vec<Particle>,
    seed: u32,
    last_move: f64,
}

impl VizState {
    pub fn new() -> Self {
        VizState {
            open: false,
            fullscreen: false,
            template: 0,
            theme: 0,
            sens: 1.0,
            smooth: 0.5,
            bands: vec![0.0; BANDS],
            peaks: vec![0.0; BANDS],
            wave: vec![0.0; 512],
            bass: 0.0,
            avg_bass: 0.0,
            beat: 0.0,
            cooldown: 0.0,
            energy: 0.0,
            t: 0.0,
            particles: vec![],
            seed: 12345,
            last_move: 0.0,
        }
    }

    fn rnd(&mut self) -> f32 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 17;
        self.seed ^= self.seed << 5;
        (self.seed & 0xffffff) as f32 / 0xffffff as f32
    }

    /// FFT of the latest samples -> log-spaced bands, waveform, bass energy and beat detection.
    fn analyze(&mut self, samples: &[f32], sr: f32, dt: f32) {
        if samples.len() < FFT_N {
            return;
        }
        let s = &samples[samples.len() - FFT_N..];
        let mut re = vec![0.0f32; FFT_N];
        let mut im = vec![0.0f32; FFT_N];
        let mut wsum = 0.0;
        for i in 0..FFT_N {
            let w = 0.5 - 0.5 * (TAU * i as f32 / (FFT_N - 1) as f32).cos();
            re[i] = s[i] * w;
            wsum += w;
        }
        fft(&mut re, &mut im);
        let mag: Vec<f32> = (0..FFT_N / 2).map(|i| (re[i] * re[i] + im[i] * im[i]).sqrt() * 2.0 / wsum).collect();
        let rise = 0.75;
        let fall = 0.04 + 0.35 * (1.0 - self.smooth);
        for b in 0..BANDS {
            let f0 = 35.0 * (16000.0f32 / 35.0).powf(b as f32 / BANDS as f32);
            let f1 = 35.0 * (16000.0f32 / 35.0).powf((b + 1) as f32 / BANDS as f32);
            let lo = ((f0 * FFT_N as f32 / sr) as usize).clamp(1, FFT_N / 2 - 1);
            let hi = ((f1 * FFT_N as f32 / sr) as usize).clamp(lo + 1, FFT_N / 2);
            let v = mag[lo..hi].iter().fold(0.0f32, |m, x| m.max(*x));
            let db = 20.0 * (v * self.sens + 1e-7).log10();
            let tilt = b as f32 / BANDS as f32 * 10.0; // pink-ish compensation so highs are visible
            let level = ((db + tilt + 72.0) / 72.0).clamp(0.0, 1.0);
            let cur = self.bands[b];
            self.bands[b] = if level > cur { cur + (level - cur) * rise } else { cur + (level - cur) * fall };
            self.peaks[b] = (self.peaks[b] - dt * 0.45).max(self.bands[b]);
        }
        for (i, w) in self.wave.iter_mut().enumerate() {
            *w = (s[FFT_N - 512 + i] * self.sens * 1.8).clamp(-1.0, 1.0);
        }
        self.bass = self.bands[..8].iter().sum::<f32>() / 8.0;
        self.energy = self.bands.iter().sum::<f32>() / BANDS as f32;
        self.avg_bass += (self.bass - self.avg_bass) * 0.06;
        self.cooldown -= dt;
        if self.bass > self.avg_bass * 1.22 + 0.04 && self.bass > 0.25 && self.cooldown <= 0.0 {
            self.beat = 1.0;
            self.cooldown = 0.16;
        }
        self.beat = (self.beat - dt * 3.2).max(0.0);
    }
}

/// In-place iterative radix-2 FFT.
fn fft(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j ^= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = -TAU / len as f32;
        let (wr, wi) = (ang.cos(), ang.sin());
        for start in (0..n).step_by(len) {
            let (mut cr, mut ci) = (1.0f32, 0.0f32);
            for k in 0..len / 2 {
                let (a, b) = (start + k, start + k + len / 2);
                let (tr, ti) = (re[b] * cr - im[b] * ci, re[b] * ci + im[b] * cr);
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
                let ncr = cr * wr - ci * wi;
                ci = cr * wi + ci * wr;
                cr = ncr;
            }
        }
        len <<= 1;
    }
}

struct Pal {
    bg1: Color32,
    bg2: Color32,
    c: [Color32; 3],
}

fn rgb(h: u32) -> Color32 {
    Color32::from_rgb((h >> 16) as u8, (h >> 8) as u8, h as u8)
}

fn palette(i: usize) -> Pal {
    match i {
        1 => Pal { bg1: rgb(0x0A0000), bg2: rgb(0x2A0008), c: [rgb(0xFF1744), rgb(0xFF6D00), rgb(0xFFFFFF)] },
        2 => Pal { bg1: rgb(0x06121F), bg2: rgb(0x0B2A44), c: [rgb(0x38BDF8), rgb(0xE0F2FE), rgb(0xA78BFA)] },
        3 => Pal { bg1: rgb(0x1A0B2E), bg2: rgb(0x4A1508), c: [rgb(0xFB7185), rgb(0xFDBA74), rgb(0xFDE047)] },
        _ => Pal { bg1: rgb(0x080818), bg2: rgb(0x1E0B38), c: [rgb(0x22D3EE), rgb(0xE879F9), rgb(0xFACC15)] },
    }
}

fn col(p: &Pal, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        lerp_color(p.c[0], p.c[1], t * 2.0)
    } else {
        lerp_color(p.c[1], p.c[2], (t - 0.5) * 2.0)
    }
}

fn hash(i: u32) -> f32 {
    let mut x = i.wrapping_mul(2654435761);
    x ^= x >> 15;
    x = x.wrapping_mul(2246822519);
    x ^= x >> 13;
    (x & 0xffff) as f32 / 65535.0
}

fn alpha(c: Color32, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (a.clamp(0.0, 1.0) * 255.0) as u8)
}

pub fn show(ui: &mut Ui, st: &mut VizState, shared: &Shared, sr: f32, title: &str, bpm: f32, playing: bool) {
    let dt = ui.input(|i| i.stable_dt).clamp(0.001, 0.05);
    st.t += dt;
    let samples = shared.wave_snapshot(FFT_N);
    st.analyze(&samples, sr, dt);
    let now = ui.input(|i| i.time);
    if ui.input(|i| i.pointer.delta() != egui::Vec2::ZERO) {
        st.last_move = now;
    }
    let show_controls = !st.fullscreen || now - st.last_move < 2.5;

    if show_controls {
        ui.horizontal_wrapped(|ui| {
            egui::ComboBox::from_id_salt("viz_t").selected_text(TEMPLATES[st.template]).show_ui(ui, |ui| {
                for (i, n) in TEMPLATES.iter().enumerate() {
                    ui.selectable_value(&mut st.template, i, *n);
                }
            });
            egui::ComboBox::from_id_salt("viz_c").selected_text(THEMES[st.theme]).show_ui(ui, |ui| {
                for (i, n) in THEMES.iter().enumerate() {
                    ui.selectable_value(&mut st.theme, i, *n);
                }
            });
            ui.add(egui::Slider::new(&mut st.sens, 0.3..=6.0).text("Gain").logarithmic(true));
            ui.add(egui::Slider::new(&mut st.smooth, 0.0..=1.0).text("Smooth"));
            if ui.button(if st.fullscreen { "Exit full screen (Esc)" } else { "Full screen" }).clicked() {
                set_fullscreen(ui.ctx(), st, !st.fullscreen);
            }
            if !playing {
                ui.label(egui::RichText::new("press Play to see the music").color(DIM()));
            }
        });
    }

    let size = ui.available_size().max(vec2(200.0, 160.0));
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
    if resp.double_clicked() {
        set_fullscreen(ui.ctx(), st, !st.fullscreen);
    }
    if st.fullscreen && ui.input(|i| i.key_pressed(Key::Escape)) {
        set_fullscreen(ui.ctx(), st, false);
    }
    let pal = palette(st.theme);
    let p = ui.painter_at(rect);
    gradient_rect(&p, rect, pal.bg1, pal.bg2);
    match st.template {
        0 => bars(&p, rect, st, &pal),
        1 => circle(&p, rect, st, &pal),
        2 => ring(&p, rect, st, &pal),
        3 => synthwave(&p, rect, st, &pal),
        4 => particles(&p, rect, st, &pal, dt),
        _ => phonk(&p, rect, st, &pal, title, bpm),
    }
    ui.ctx().request_repaint();
}

fn set_fullscreen(ctx: &egui::Context, st: &mut VizState, on: bool) {
    st.fullscreen = on;
    ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(on));
}

fn bars(p: &egui::Painter, r: Rect, st: &VizState, pal: &Pal) {
    let n = BANDS;
    let base_y = r.bottom() - r.height() * 0.22;
    let w = r.width() / n as f32;
    for i in 0..n {
        let v = st.bands[i].powf(1.35);
        let h = v * r.height() * 0.62;
        let x = r.left() + i as f32 * w;
        let c = col(pal, i as f32 / n as f32 * 0.6 + v * 0.4);
        let bar = Rect::from_min_max(pos2(x + w * 0.12, base_y - h), pos2(x + w * 0.88, base_y));
        p.rect_filled(bar, 0.0, c);
        // reflection
        let refl = Rect::from_min_max(pos2(x + w * 0.12, base_y + 3.0), pos2(x + w * 0.88, base_y + 3.0 + h * 0.45));
        p.rect_filled(refl, 0.0, alpha(c, 0.22));
        // peak cap
        let py = base_y - st.peaks[i].powf(1.35) * r.height() * 0.62 - 5.0;
        p.rect_filled(Rect::from_min_size(pos2(x + w * 0.12, py), vec2(w * 0.76, 3.0)), 0.0, alpha(Color32::WHITE, 0.85));
    }
    p.rect_filled(Rect::from_min_max(pos2(r.left(), base_y), pos2(r.right(), base_y + 2.0)), 0.0, alpha(col(pal, 0.5), 0.6));
    p.rect_filled(r, 0.0, alpha(col(pal, 0.2), st.beat * 0.08));
}

fn circle(p: &egui::Painter, r: Rect, st: &VizState, pal: &Pal) {
    let c = r.center();
    let m = r.width().min(r.height());
    let r0 = m * 0.17 * (1.0 + st.beat * 0.16);
    let n = BANDS * 2;
    for i in 0..n {
        let b = if i < BANDS { i } else { n - 1 - i };
        let v = st.bands[b].powf(1.3);
        let a = i as f32 / n as f32 * TAU - PI / 2.0 + st.t * 0.12;
        let len = (0.02 + v) * m * 0.3;
        let d = vec2(a.cos(), a.sin());
        let cc = col(pal, b as f32 / BANDS as f32);
        p.line_segment([c + d * r0, c + d * (r0 + len)], Stroke::new((m / 190.0).max(2.0), cc));
        p.circle_filled(c + d * (r0 + len + 4.0), 1.8, alpha(Color32::WHITE, 0.8));
    }
    p.circle_stroke(c, r0 - 3.0, Stroke::new(3.0, alpha(col(pal, 0.1), 0.9)));
    p.circle_filled(c, r0 * (0.55 + st.bass * 0.35), alpha(col(pal, 0.6), 0.25 + st.beat * 0.4));
}

fn ring(p: &egui::Painter, r: Rect, st: &VizState, pal: &Pal) {
    let c = r.center();
    let m = r.width().min(r.height());
    let rad = m * 0.24 * (1.0 + st.bass * 0.18);
    let n = st.wave.len();
    for (layer, (wid, a)) in [(9.0, 0.10), (5.0, 0.22), (2.2, 1.0)].into_iter().enumerate() {
        let mut pts: Vec<Pos2> = (0..=n)
            .map(|i| {
                let k = i % n;
                let ang = k as f32 / n as f32 * TAU - PI / 2.0;
                let rr = rad + st.wave[k] * rad * 0.55;
                c + vec2(ang.cos(), ang.sin()) * rr
            })
            .collect();
        if layer == 0 {
            pts.iter_mut().for_each(|q| *q = c + (*q - c) * 1.01);
        }
        p.add(Shape::line(pts, Stroke::new(wid, alpha(col(pal, 0.2 + st.energy * 1.5 + layer as f32 * 0.1), a))));
    }
    p.circle_stroke(c, rad * 0.55, Stroke::new(1.5, alpha(col(pal, 0.9), 0.4 + st.beat * 0.5)));
    p.circle_filled(c, rad * 0.12 * (1.0 + st.beat), alpha(col(pal, 0.5), 0.7));
}

fn synthwave(p: &egui::Painter, r: Rect, st: &VizState, pal: &Pal) {
    let hy = r.top() + r.height() * 0.58;
    let cx = r.center().x;
    // sun with cut stripes
    let sun_r = r.height() * (0.2 + st.bass * 0.03);
    let sun_c = pos2(cx, hy - sun_r * 0.35);
    for k in 0..24 {
        let t = k as f32 / 24.0;
        let y0 = sun_c.y - sun_r + t * sun_r * 2.0;
        let h = sun_r * 2.0 / 24.0;
        let cut = if t > 0.5 { ((t - 0.5) * 2.0).powf(0.8) * 0.8 } else { 0.0 };
        let mid = (y0 + h * 0.5 - sun_c.y) / sun_r;
        if mid.abs() >= 1.0 || (k % 3 == 0 && cut > 0.15) {
            continue;
        }
        let half = (1.0 - mid * mid).sqrt() * sun_r;
        p.rect_filled(Rect::from_min_max(pos2(cx - half, y0), pos2(cx + half, y0 + h * (1.0 - cut * 0.7))), 0.0, col(pal, 0.35 + t * 0.5));
    }
    // spectrum mountains on the horizon (mirrored)
    let n = BANDS;
    let mut pts = vec![pos2(r.left(), hy)];
    for i in 0..n * 2 {
        let b = if i < n { n - 1 - i } else { i - n };
        let x = r.left() + i as f32 / (n * 2 - 1) as f32 * r.width();
        pts.push(pos2(x, hy - st.bands[b].powf(1.3) * r.height() * 0.28));
    }
    pts.push(pos2(r.right(), hy));
    p.add(Shape::convex_polygon(vec![pos2(r.left(), hy), pos2(r.right(), hy), pos2(r.right(), hy + 1.0), pos2(r.left(), hy + 1.0)], pal.bg1, Stroke::NONE));
    p.add(Shape::line(pts, Stroke::new(2.4, col(pal, 0.1 + st.energy))));
    // ground
    let ground = Rect::from_min_max(pos2(r.left(), hy), r.right_bottom());
    p.rect_filled(ground, 0.0, alpha(pal.bg1, 0.92));
    for k in -14..=14 {
        let x_end = cx + k as f32 * r.width() * 0.11;
        p.line_segment([pos2(cx + k as f32 * 3.0, hy), pos2(x_end, r.bottom())], Stroke::new(1.2, alpha(col(pal, 0.3), 0.55)));
    }
    let speed = st.t * (0.5 + st.bass * 1.2);
    for i in 0..16 {
        let z = ((i as f32 + speed * 3.0) % 16.0) / 16.0;
        let y = hy + (r.bottom() - hy) * z * z;
        p.line_segment([pos2(r.left(), y), pos2(r.right(), y)], Stroke::new(1.0 + z * 1.5, alpha(col(pal, 0.6), 0.2 + z * 0.7)));
    }
    p.line_segment([pos2(r.left(), hy), pos2(r.right(), hy)], Stroke::new(2.0, col(pal, 0.45)));
}

fn particles(p: &egui::Painter, r: Rect, st: &mut VizState, pal: &Pal, dt: f32) {
    let c = r.center();
    let m = r.width().min(r.height());
    if st.beat > 0.98 {
        for _ in 0..70 {
            let a = st.rnd() * TAU;
            let sp = (0.25 + st.rnd() * 0.9) * m * (0.6 + st.energy);
            let hue = st.rnd();
            let size = 1.5 + st.rnd() * 3.5;
            st.particles.push(Particle { pos: c, vel: vec2(a.cos(), a.sin()) * sp, life: 1.0, hue, size });
        }
    }
    let trickle = (st.energy * 6.0) as usize;
    for _ in 0..trickle {
        let a = st.rnd() * TAU;
        let hue = st.rnd();
        let v = (0.08 + st.rnd() * 0.2) * m;
        st.particles.push(Particle { pos: c, vel: vec2(a.cos(), a.sin()) * v, life: 0.8, hue, size: 1.2 });
    }
    if st.particles.len() > 1500 {
        let cut = st.particles.len() - 1500;
        st.particles.drain(0..cut);
    }
    for q in st.particles.iter_mut() {
        q.pos += q.vel * dt;
        q.vel *= 1.0 - dt * 0.6;
        q.life -= dt * 0.55;
    }
    st.particles.retain(|q| q.life > 0.0);
    for q in &st.particles {
        let cc = col(pal, q.hue);
        p.circle_filled(q.pos, q.size * (0.5 + q.life), alpha(cc, q.life.min(1.0)));
    }
    let rr = m * 0.07 * (1.0 + st.beat * 0.7 + st.bass * 0.5);
    p.circle_filled(c, rr * 1.8, alpha(col(pal, 0.2), 0.12));
    p.circle_filled(c, rr, alpha(col(pal, 0.55), 0.85));
}

fn phonk(p: &egui::Painter, r: Rect, st: &VizState, pal: &Pal, title: &str, bpm: f32) {
    let c = r.center();
    let diag = (r.width() * r.width() + r.height() * r.height()).sqrt() * 0.5;
    // radial speed lines flying outwards
    for i in 0..90u32 {
        let a = hash(i * 7 + 1) * TAU;
        let speed = 0.25 + hash(i * 13 + 5) * 0.9;
        let ph = (hash(i * 3 + 9) + st.t * speed * (0.5 + st.energy * 2.0) * bpm / 140.0) % 1.0;
        let len = (0.05 + hash(i * 17 + 2) * 0.35) * (0.3 + st.energy * 2.2) * diag;
        let r0 = (0.1 + ph * 0.95) * diag;
        let d = vec2(a.cos(), a.sin());
        let cc = if hash(i * 29 + 3) > 0.7 { Color32::WHITE } else { col(pal, hash(i) * 0.5) };
        p.line_segment([c + d * r0, c + d * (r0 + len)], Stroke::new(1.0 + hash(i * 5) * 2.0 + st.beat * 1.5, alpha(cc, ph * 0.9)));
    }
    // kick flash + vignette
    p.rect_filled(r, 0.0, alpha(col(pal, 0.0), st.beat * 0.20));
    let top = Rect::from_min_max(r.left_top(), pos2(r.right(), r.top() + r.height() * 0.07));
    let bot = Rect::from_min_max(pos2(r.left(), r.bottom() - r.height() * 0.07), r.right_bottom());
    p.rect_filled(top, 0.0, alpha(Color32::BLACK, 0.55));
    p.rect_filled(bot, 0.0, alpha(Color32::BLACK, 0.55));
    // title with a bass-reactive pulse and a red offset shadow
    let size = (r.height() * 0.17).min(r.width() * 0.1) * (1.0 + st.beat * 0.22);
    let text = title.to_uppercase();
    let font = FontId::proportional(size.max(18.0));
    p.text(c + vec2(4.0, 4.0), Align2::CENTER_CENTER, &text, font.clone(), alpha(col(pal, 0.0), 0.9));
    p.text(c - vec2(2.0, 2.0), Align2::CENTER_CENTER, &text, font.clone(), alpha(col(pal, 0.6), 0.7));
    p.text(c, Align2::CENTER_CENTER, &text, font, Color32::WHITE);
    p.text(c + vec2(0.0, size * 0.75), Align2::CENTER_CENTER, format!("{bpm:.0} BPM"), FontId::monospace((size * 0.22).max(11.0)), alpha(Color32::WHITE, 0.7));
    // thin spectrum along the bottom
    let n = BANDS;
    let w = r.width() / n as f32;
    for i in 0..n {
        let h = st.bands[i].powf(1.3) * r.height() * 0.2;
        p.rect_filled(Rect::from_min_max(pos2(r.left() + i as f32 * w + 1.0, bot.top() - h), pos2(r.left() + (i + 1) as f32 * w - 1.0, bot.top())), 0.0, alpha(col(pal, i as f32 / n as f32), 0.9));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analyzer_finds_the_tone_and_beats() {
        let mut st = VizState::new();
        let sr = 48000.0;
        let tone: Vec<f32> = (0..FFT_N).map(|i| (TAU * 1000.0 * i as f32 / sr).sin() * 0.3).collect();
        for _ in 0..30 {
            st.analyze(&tone, sr, 1.0 / 60.0);
        }
        let best = st.bands.iter().enumerate().fold((0, 0.0f32), |m, (i, v)| if *v > m.1 { (i, *v) } else { m }).0;
        let f_lo = 35.0 * (16000.0f32 / 35.0).powf(best as f32 / BANDS as f32);
        let f_hi = 35.0 * (16000.0f32 / 35.0).powf((best + 1) as f32 / BANDS as f32);
        assert!(f_lo <= 1050.0 && f_hi >= 950.0, "peak band {f_lo}..{f_hi}");
        assert!(st.bands[best] > 0.5);
        // silence -> bars fall, a sudden low tone -> beat
        let silence = vec![0.0f32; FFT_N];
        for _ in 0..200 {
            st.analyze(&silence, sr, 1.0 / 60.0);
        }
        assert!(st.energy < 0.05);
        let kick: Vec<f32> = (0..FFT_N).map(|i| (TAU * 60.0 * i as f32 / sr).sin() * 0.8).collect();
        let mut got_beat = false;
        for _ in 0..5 {
            st.analyze(&kick, sr, 1.0 / 60.0);
            got_beat |= st.beat > 0.9;
        }
        assert!(got_beat, "no beat detected");
    }
}
