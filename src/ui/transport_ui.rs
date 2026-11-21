//! Header (Top Bar), Modern Solid: transport, BPM, position, PAT / SONG, Loop, Metronome, Stars and MIDI in the centre;
//! master level, CPU / RAM, Settings, Dark / Light and the window buttons on the right.
use super::{theme::*, widgets::db, Tab, VinteriaApp};
use crate::{audio_engine::Cmd, mixer::NUM_STRIPS, sequencer::{PPQ, TICKS_PER_BAR}};
use egui::{pos2, vec2, Align2, Color32, FontId, Painter, Pos2, Rect, RichText, Sense, Shape, Stroke, Ui};
use std::sync::atomic::Ordering;

#[derive(Clone, Copy, PartialEq)]
enum Ic { Rewind, Play, Pause, Stop, Record, Metronome, Loop, Star, Gear, Sun, Moon, Minimize, Maximize, Restore, Close }

fn poly(p: &Painter, pts: Vec<Pos2>, col: Color32) {
    p.add(Shape::convex_polygon(pts, col, Stroke::NONE));
}

fn draw(p: &Painter, ic: Ic, c: Pos2, ink: Color32) {
    let s = Stroke::new(1.7_f32, ink);
    match ic {
        Ic::Play => poly(p, vec![c + vec2(-4.0, -8.0), c + vec2(-4.0, 8.0), c + vec2(8.0, 0.0)], ink),
        Ic::Pause => {
            p.rect_filled(Rect::from_center_size(c + vec2(-4.0, 0.0), vec2(4.5, 14.0)), 1.5, ink);
            p.rect_filled(Rect::from_center_size(c + vec2(4.0, 0.0), vec2(4.5, 14.0)), 1.5, ink);
        }
        Ic::Stop => {
            p.rect_filled(Rect::from_center_size(c, vec2(11.0, 11.0)), 2.0, ink);
        }
        Ic::Record => { p.circle_filled(c, 6.0, ink); }
        Ic::Rewind => {
            p.line_segment([c + vec2(-7.0, -7.0), c + vec2(-7.0, 7.0)], s);
            poly(p, vec![c + vec2(6.0, -7.0), c + vec2(6.0, 7.0), c + vec2(-4.0, 0.0)], ink);
        }
        Ic::Metronome => {
            p.line_segment([c + vec2(-8.0, 8.0), c + vec2(0.0, -9.0)], s);
            p.line_segment([c + vec2(0.0, -9.0), c + vec2(8.0, 8.0)], s);
            p.line_segment([c + vec2(-8.0, 8.0), c + vec2(8.0, 8.0)], s);
            p.line_segment([c + vec2(0.0, 5.0), c + vec2(6.0, -5.0)], s);
            p.circle_filled(c + vec2(4.0, -2.0), 2.2, ink);
        }
        Ic::Loop => {
            p.rect_stroke(Rect::from_center_size(c, vec2(18.0, 11.0)), 5.0, s);
            poly(p, vec![c + vec2(3.0, -9.5), c + vec2(3.0, -2.5), c + vec2(8.0, -6.0)], ink);
            poly(p, vec![c + vec2(-3.0, 9.5), c + vec2(-3.0, 2.5), c + vec2(-8.0, 6.0)], ink);
        }
        Ic::Star => {
            let pt = |i: usize, r: f32| {
                let a = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::TAU / 10.0;
                c + vec2(a.cos(), a.sin()) * r
            };
            poly(p, (0..5).map(|k| pt(k * 2 + 1, 4.4)).collect(), ink);
            for k in 0..5 {
                poly(p, vec![pt(k * 2, 9.5), pt((k * 2 + 9) % 10, 4.4), pt(k * 2 + 1, 4.4)], ink);
            }
        }
        Ic::Gear => {
            p.circle_stroke(c, 5.0, Stroke::new(2.2, ink));
            for k in 0..8 {
                let a = k as f32 * std::f32::consts::TAU / 8.0;
                let d = vec2(a.cos(), a.sin());
                p.line_segment([c + d * 7.0, c + d * 9.5], Stroke::new(2.4, ink));
            }
        }
        Ic::Sun => {
            p.circle_filled(c, 4.5, ink);
            for k in 0..8 {
                let a = k as f32 * std::f32::consts::TAU / 8.0;
                let d = vec2(a.cos(), a.sin());
                p.line_segment([c + d * 7.0, c + d * 9.5], s);
            }
        }
        Ic::Moon => {
            p.circle_filled(c, 8.0, ink);
            p.circle_filled(c + vec2(4.5, -3.5), 7.0, SIDE());
        }
        Ic::Minimize => {
            p.line_segment([c + vec2(-6.0, 4.0), c + vec2(6.0, 4.0)], Stroke::new(1.8, ink));
        }
        Ic::Maximize => {
            p.rect_stroke(Rect::from_center_size(c, vec2(11.0, 11.0)), 1.5, s);
        }
        Ic::Restore => {
            p.rect_stroke(Rect::from_center_size(c + vec2(-1.5, 1.5), vec2(9.0, 9.0)), 1.5, s);
            p.line_segment([c + vec2(-1.0, -6.0), c + vec2(6.5, -6.0)], s);
            p.line_segment([c + vec2(6.5, -6.0), c + vec2(6.5, 1.0)], s);
        }
        Ic::Close => {
            p.line_segment([c + vec2(-5.5, -5.5), c + vec2(5.5, 5.5)], Stroke::new(1.9, ink));
            p.line_segment([c + vec2(-5.5, 5.5), c + vec2(5.5, -5.5)], Stroke::new(1.9, ink));
        }
    }
}

/// flat icon button: no frame at rest, a soft plate on hover, the accent when active;
/// Play / Pause is the one filled brass disc of the transport
fn button(ui: &mut Ui, ic: Ic, size: egui::Vec2, active: bool, accent: Color32, tip: &str) -> bool {
    let (r, resp) = ui.allocate_exact_size(size, Sense::click());
    let hot = resp.hovered();
    let down = resp.is_pointer_button_down_on();
    let p = ui.painter();
    let main = matches!(ic, Ic::Play | Ic::Pause);
    let ink;
    if main {
        let disc = Rect::from_center_size(r.center(), vec2(size.y, size.y));
        let fill = if down { tint(TEAL(), Color32::BLACK, 0.2) } else if hot { tint(TEAL(), Color32::WHITE, 0.15) } else { TEAL() };
        p.rect_filled(disc, 0.0, fill);
        ink = INSET();
    } else {
        if active {
            p.rect_filled(r, corner(), tint(PANEL2(), accent, if is_dark() { 0.38 } else { 0.28 }));
        } else if hot {
            p.rect_filled(r, corner(), PANEL3());
        }
        ink = if active { if matches!(ic, Ic::Record) { RED } else { accent } } else if matches!(ic, Ic::Record) { RED } else if hot { TEXT() } else { DIM() };
    }
    draw(p, ic, r.center(), ink);
    let clicked = resp.on_hover_text(tip).clicked();
    if clicked {
        // the audio thread answers a moment later: keep redrawing so the icon follows at once
        let t = ui.input(|i| i.time);
        ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("transport_click_t"), t));
    }
    clicked
}

/// text pill
fn pill(ui: &mut Ui, text: &str, w: f32, active: bool, tip: &str) -> bool {
    let (r, resp) = ui.allocate_exact_size(vec2(w, 36.0), Sense::click());
    let p = ui.painter();
    if active {
        gloss(p, r, 0.0, TEAL());
    } else if resp.hovered() {
        p.rect_filled(r, 0.0, PANEL3());
    }
    p.text(r.center(), Align2::CENTER_CENTER, text, FontId::proportional(12.5), if active { Color32::WHITE } else { DIM() });
    resp.on_hover_text(tip).clicked()
}

fn favorites() -> Vec<String> {
    std::fs::read_to_string("favorite_projects.txt").map(|t| t.lines().map(str::to_string).collect()).unwrap_or_default()
}

/// Everything right of the menus, in one row.
pub(super) fn show(app: &mut VinteriaApp, ui: &mut Ui) {
    let playing = app.playing();
    if let Some(t) = ui.ctx().data(|d| d.get_temp::<f64>(egui::Id::new("transport_click_t"))) {
        if ui.input(|i| i.time) - t < 0.5 {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(8));
        }
    }
    // a frameless window is not always maximised by the OS at start-up
    let maxed = egui::Id::new("start_maximized");
    if ui.ctx().data(|d| d.get_temp::<bool>(maxed)).is_none() {
        ui.ctx().data_mut(|d| d.insert_temp(maxed, true));
        if std::env::var("SYNTHESIA_SIZE").is_err() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Maximized(true));
        }
    }
    let acc = TEAL();
    ui.spacing_mut().item_spacing.x = 6.0;

    // ---- centre: transport
    if button(ui, Ic::Rewind, vec2(34.0, 34.0), false, acc, "Return to start · Home") {
        app.return_to_start(ui.ctx(), playing);
    }
    // the round button plays and pauses in place; Stop (next to it) rewinds
    if button(ui, if playing { Ic::Pause } else { Ic::Play }, vec2(40.0, 40.0), playing, acc, if playing { "Pause · Space" } else { "Play from the playhead · Space" }) {
        app.toggle_play();
    }
    if button(ui, Ic::Stop, vec2(34.0, 34.0), false, acc, "Stop and rewind") { app.return_to_start(ui.ctx(), false); }
    // one pill: the dot arms / disarms recording, the label shows what is captured, the chevron opens the mode list
    {
        let (r, _) = ui.allocate_exact_size(vec2(104.0, 34.0), Sense::hover());
        let main_r = Rect::from_min_max(r.min, pos2(r.right() - 26.0, r.bottom()));
        let chev_r = Rect::from_min_max(pos2(r.right() - 26.0, r.top()), r.max);
        let m = ui.interact(main_r, ui.id().with("rec_main"), Sense::click());
        let c = ui.interact(chev_r, ui.id().with("rec_mode"), Sense::click());
        let p = ui.painter();
        let fill = if app.record { tint(PANEL2(), RED, 0.28) } else if m.hovered() || c.hovered() { PANEL3() } else { PANEL2() };
        p.rect_filled(r, 0.0, fill);
        p.rect_stroke(r, 0.0, Stroke::new(border_w(), if app.record { RED } else { border() }));
        p.circle_filled(pos2(r.left() + 19.0, r.center().y), 6.0, if app.record { RED } else { RED.linear_multiply(0.6) });
        p.text(pos2(r.left() + 33.0, r.center().y), Align2::LEFT_CENTER, match app.rec_mode { 0 => "Notes", 1 => "Audio", _ => "Both" }, FontId::proportional(12.0), if app.record { TEXT() } else { DIM() });
        let cc = chev_r.center();
        p.add(Shape::line(vec![cc + vec2(-4.0, -2.0), cc + vec2(0.0, 2.5), cc + vec2(4.0, -2.0)], Stroke::new(1.6, DIM())));
        if m.on_hover_text("Record notes / audio (the arrow picks what is captured)").clicked() {
            app.record = !app.record;
            if app.record && !playing { app.shared.send(Cmd::Play); }
        }
        let pop = egui::Id::new("rec_mode_pop");
        if c.clicked() { ui.memory_mut(|m| m.toggle_popup(pop)); }
        egui::popup::popup_below_widget(ui, pop, &c, egui::PopupCloseBehavior::CloseOnClick, |ui| {
            ui.set_min_width(190.0);
            ui.label(RichText::new("Record mode").small().color(DIM()));
            ui.selectable_value(&mut app.rec_mode, 0, "Notes (keyboard / MIDI)");
            ui.selectable_value(&mut app.rec_mode, 1, "Audio (input device)");
            ui.selectable_value(&mut app.rec_mode, 2, "Notes + audio");
        });
    }
    ui.add_space(4.0);
    // Update: pulls a newer build from GitHub over the exe; the running copy restarts into it by itself
    #[cfg(not(target_os = "android"))]
    {
        use crate::updater::UpState;
        let orig = app.live_origin.as_ref().map(|o| o.0.clone()).or_else(|| std::env::current_exe().ok()).unwrap_or_default();
        // automatic: look at start-up and every 10 minutes; a newer build is fetched at once and the program restarts into it by itself
        {
            let now = ui.input(|i| i.time);
            let last_id = egui::Id::new("updater_last_check");
            let last: f64 = ui.ctx().data(|d| d.get_temp(last_id)).unwrap_or(-1.0e9);
            let cur = app.updater.get();
            if now - last > 600.0 && matches!(cur, UpState::Idle | UpState::Latest | UpState::Error(_)) {
                ui.ctx().data_mut(|d| d.insert_temp(last_id, now));
                app.updater.check(orig.clone());
            }
            if matches!(cur, UpState::Available(_)) {
                app.updater.download(orig.clone());
            }
            ui.ctx().request_repaint_after(std::time::Duration::from_secs(5));
        }
        let st = app.updater.get();
        let (label, tip, hot) = match &st {
            UpState::Idle => ("Update".to_string(), "Check GitHub for a newer build".to_string(), false),
            UpState::Checking => ("Checking...".into(), "Asking GitHub".into(), false),
            UpState::Available(_) => ("Update v".into(), "A newer build is on GitHub: click to download it. The program restarts by itself and keeps your project.".into(), true),
            UpState::Latest => ("Up to date".into(), "This is the newest build (click to check again)".into(), false),
            UpState::Downloading => ("Downloading...".into(), "Fetching the new build".into(), true),
            UpState::Done => ("Restarting...".into(), "Installed: restarting into the new build".into(), true),
            UpState::Error(e) => ("Update failed".into(), format!("{e} (click to retry)"), false),
            UpState::NoRepo => ("Update".into(), "Put the GitHub raw URL into update_url.txt next to the program".into(), false),
        };
        ui.scope(|ui| {
            ui.spacing_mut().button_padding = vec2(10.0, 6.0);
            let w = &mut ui.visuals_mut().widgets;
            for v in [&mut w.inactive, &mut w.hovered, &mut w.active] {
                v.rounding = egui::Rounding::ZERO;
                v.bg_stroke = Stroke::new(border_w(), if hot { TEAL() } else { border() });
            }
            w.inactive.weak_bg_fill = if hot { TEAL().gamma_multiply(0.22) } else { Color32::from_white_alpha(16) };
            if ui.button(RichText::new(label).size(11.5).color(if hot { TEAL() } else { DIM() })).on_hover_text(tip).clicked() {
                match st {
                    UpState::Available(_) => app.updater.download(orig),
                    UpState::Checking | UpState::Downloading | UpState::Done => {}
                    _ => app.updater.check(orig),
                }
            }
        });
        if matches!(st, UpState::Checking | UpState::Downloading | UpState::Done) {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(300));
        }
        ui.add_space(4.0);
    }
    // BPM
    egui::Frame::none().fill(INSET()).rounding(corner()).stroke(Stroke::new(border_w(), border())).inner_margin(egui::Margin::symmetric(8.0, 0.0)).show(ui, |ui| {
        ui.vertical(|ui| {
            ui.set_width(76.0);
            ui.set_max_width(76.0);
            ui.set_min_height(40.0);
            ui.add_space(3.0);
            {
                // the number sits flat on the plate: no second box around it
                let w = &mut ui.visuals_mut().widgets;
                for v in [&mut w.inactive, &mut w.hovered, &mut w.active] {
                    v.weak_bg_fill = Color32::TRANSPARENT;
                    v.bg_fill = Color32::TRANSPARENT;
                    v.bg_stroke = Stroke::NONE;
                    v.expansion = 0.0;
                }
            }
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.visuals_mut().override_text_color = Some(TEAL());
            ui.add(egui::DragValue::new(&mut app.project.bpm).range(40.0..=300.0).speed(0.1).fixed_decimals(2)).on_hover_text("Tempo · drag or double-click to type");
            ui.label(RichText::new("BPM").size(8.5).color(DIM()));
        });
    });
    // time counter
    let pos = app.shared.position.load(Ordering::Relaxed);
    let (r, _) = ui.allocate_exact_size(vec2(120.0, 40.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(r, corner(), INSET());
    p.rect_stroke(r, corner(), Stroke::new(border_w(), border()));
    let step = (pos % PPQ) / (PPQ / 4) + 1;
    p.text(r.center() + vec2(0.0, -5.0), Align2::CENTER_CENTER, format!("{:03}.{}.{step}", pos / TICKS_PER_BAR + 1, (pos % TICKS_PER_BAR) / PPQ + 1), FontId::monospace(19.0), TEAL());
    let seconds = pos as f64 * 60.0 / (app.project.bpm.max(1.0) as f64 * PPQ as f64);
    p.text(r.center() + vec2(0.0, 12.0), Align2::CENTER_CENTER, format!("{:02}:{:05.2}  BAR:BEAT", seconds as u64 / 60, seconds % 60.0), FontId::monospace(8.5), DIM());
    // time signature (the engine counts in 4/4)
    let (r, resp) = ui.allocate_exact_size(vec2(46.0, 40.0), Sense::hover());
    ui.painter().rect_filled(r, corner(), INSET());
    ui.painter().rect_stroke(r, corner(), Stroke::new(border_w(), border()));
    ui.painter().text(r.center(), Align2::CENTER_CENTER, "4/4", FontId::proportional(14.0), TEXT());
    resp.on_hover_text("Time signature: Winteria counts in 4/4");
    // PAT / SONG
    egui::Frame::none().fill(PANEL2()).rounding(0.0).stroke(Stroke::new(border_w(), border())).inner_margin(egui::Margin::symmetric(2.0, 2.0)).show(ui, |ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let before = app.project.song_mode;
        if pill(ui, "PAT", 48.0, !app.project.song_mode, "Pattern mode: loop the selected pattern") { app.project.song_mode = false; }
        if pill(ui, "SONG", 52.0, app.project.song_mode, "Song mode: play the Playlist") { app.project.song_mode = true; }
        if before != app.project.song_mode { app.tab = if app.project.song_mode { Tab::Arrangement } else { Tab::Sequencer }; }
    });
    // on a wide header the Loop / Metronome / Star / MIDI group sits in the right-hand zone instead (see show_toggles)
    let toggles_right = ui.ctx().data(|d| d.get_temp::<bool>(egui::Id::new("toggles_right"))).unwrap_or(false);
    if !toggles_right {
        show_toggles(app, ui);
    }
}

/// Loop, Metronome, Star and MIDI input: one menu button.
pub(super) fn show_toggles(app: &mut VinteriaApp, ui: &mut Ui) {
    ui.scope(|ui| {
        ui.spacing_mut().button_padding = vec2(16.0, 8.0);
        let w = &mut ui.visuals_mut().widgets;
        for v in [&mut w.inactive, &mut w.hovered, &mut w.active, &mut w.open] {
            v.rounding = egui::Rounding::ZERO;
            v.bg_stroke = Stroke::new(border_w(), border());
        }
        w.inactive.weak_bg_fill = PANEL2();
        ui.menu_button(RichText::new("Controls  v").size(12.5).color(if app.project.loop_on || app.project.metronome { TEAL() } else { TEXT() }), |ui| {
            ui.set_min_width(220.0);
            ui.checkbox(&mut app.project.loop_on, "Loop the Song range");
            ui.checkbox(&mut app.project.metronome, "Metronome");
            let key = app.path.as_ref().map(|p| p.to_string_lossy().to_string());
            let starred = key.as_ref().map_or(false, |k| favorites().iter().any(|f| f == k));
            if ui.selectable_label(starred, if starred { "Starred project" } else { "Star this project" }).clicked() {
                match key {
                    Some(k) => {
                        let mut f = favorites();
                        if let Some(i) = f.iter().position(|x| *x == k) { f.remove(i); } else { f.push(k); }
                        let _ = std::fs::write("favorite_projects.txt", f.join("\n"));
                        app.status = if starred { "Star removed".into() } else { "Project starred".into() };
                    }
                    None => app.status = "Save the project first to star it".into(),
                }
            }
            ui.separator();
            let connected = app.midi.selected.is_some();
            ui.menu_button(if connected { "MIDI input  ●" } else { "MIDI input  ○" }, |ui| {
                ui.set_min_width(220.0);
                if ui.selectable_label(app.midi.selected.is_none(), "none").clicked() { app.midi.disconnect(); }
                for p in app.midi.ports.clone() {
                    if ui.selectable_label(app.midi.selected.as_deref() == Some(&p), &p).clicked() { app.midi.connect(&p); }
                }
                if app.midi.ports.is_empty() { ui.label(RichText::new("No MIDI devices found").color(DIM())); }
                if ui.button("Rescan").clicked() { app.midi.refresh(); }
                if let Some(e) = &app.midi.error { ui.colored_label(RED, e); }
            });
        });
    });
}

/// Fixed right-hand group; laid out independently of transport wrapping.
pub(super) fn show_status(app: &mut VinteriaApp, ui: &mut Ui) {
    let acc = TEAL();
    ui.spacing_mut().item_spacing.x = 6.0;
    // ---- right: master level, CPU / RAM, settings, theme, window
    // one pill: master level on the left, CPU / RAM on the right
    let (pill, _) = ui.allocate_exact_size(vec2(214.0, 38.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(pill, 0.0, PANEL2());
    p.rect_stroke(pill, 0.0, Stroke::new(border_w(), border()));
    let r = Rect::from_min_size(pill.min + vec2(10.0, 2.0), vec2(96.0, 34.0));
    let peak = app.meters[NUM_STRIPS][0].max(app.meters[NUM_STRIPS][1]);
    p.text(r.left_top() + vec2(8.0, 8.0), Align2::LEFT_CENTER, "MASTER", FontId::proportional(8.5), DIM());
    p.text(r.right_top() + vec2(-8.0, 8.0), Align2::RIGHT_CENTER, format!("{:.1} dB", db(peak).max(-60.0)), FontId::monospace(8.5), if peak >= 1.0 { RED } else { DIM() });
    for (i, value) in app.meters[NUM_STRIPS].iter().enumerate() {
        let fraction = ((db(*value) + 60.0) / 60.0).clamp(0.0, 1.0);
        let lane = Rect::from_min_size(r.min + vec2(8.0, 17.0 + i as f32 * 7.0), vec2(80.0, 5.0));
        p.rect_filled(lane, 2.5, INSET());
        let col = if *value >= 1.0 { RED } else if fraction > 0.85 { AMBER } else { acc };
        p.rect_filled(Rect::from_min_size(lane.min, vec2(lane.width() * fraction, lane.height())), 2.5, col);
    }
    p.vline(pill.left() + 116.0, (pill.top() + 8.0)..=(pill.bottom() - 8.0), Stroke::new(1.0, border()));
    let cpu = f32::from_bits(app.shared.cpu.load(Ordering::Relaxed));
    let ram = super::process_memory_mb();
    p.text(pos2(pill.left() + 128.0, pill.center().y - 7.0), Align2::LEFT_CENTER, format!("CPU {cpu:.0}%"), FontId::monospace(10.5), if cpu > 80.0 { RED } else { DIM() });
    p.text(pos2(pill.left() + 128.0, pill.center().y + 7.0), Align2::LEFT_CENTER, if ram >= 1024.0 { format!("RAM {:.1} GB", ram / 1024.0) } else { format!("RAM {ram:.0} MB") }, FontId::monospace(10.5), DIM());
    ui.add_space(6.0);
    ui.scope(|ui| {
        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
            // window buttons keep the order minimise / maximise / close
            let ctx = ui.ctx().clone();
            let maximized = ctx.input(|i| i.viewport().maximized).unwrap_or(false);
            ui.spacing_mut().item_spacing.x = 4.0;
            // one plate behind the three buttons: its slot is reserved first and filled once the buttons are placed
            let plate_idx = ui.painter().add(egui::Shape::Noop);
            let mut plate: Option<Rect> = None;
            for k in 0..3 {
                let (r, resp) = ui.allocate_exact_size(vec2(36.0, 30.0), Sense::click());
                plate = Some(plate.map_or(r, |p| p.union(r)));
                let hot = resp.hovered();
                let p = ui.painter();
                p.rect_filled(r, 4.0, if hot { if k == 2 { RED } else { PANEL3() } } else { Color32::TRANSPARENT });
                let ic = [Ic::Minimize, if maximized { Ic::Restore } else { Ic::Maximize }, Ic::Close][k];
                draw(p, ic, r.center(), if hot && k == 2 { Color32::WHITE } else { TEXT() });
                if resp.on_hover_text(["Minimize", if maximized { "Restore" } else { "Maximize" }, "Close"][k]).clicked() {
                    ctx.send_viewport_cmd(match k {
                        0 => egui::ViewportCommand::Minimized(true),
                        1 => egui::ViewportCommand::Maximized(!maximized),
                        _ => egui::ViewportCommand::Close,
                    });
                }
            }
            if let Some(pr) = plate {
                let _ = (plate_idx, pr);
            }
        });
    });
}

/// Settings (audio, UI scale, accent, auto theme) and the dark / light switch: the top of the right-hand column.
pub(super) fn side_settings(app: &mut VinteriaApp, ui: &mut Ui) {
    let acc = TEAL();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        // Settings
        let (gr, gresp) = ui.allocate_exact_size(vec2(34.0, 34.0), Sense::click());
        let p = ui.painter();
        p.rect_filled(gr, 0.0, if gresp.hovered() { PANEL3() } else { PANEL2() });
        p.rect_stroke(gr, 0.0, Stroke::new(border_w(), border()));
        draw(p, Ic::Gear, gr.center(), TEXT());
        let gresp = gresp.on_hover_text("Settings");
        let pop = egui::Id::new("header_settings");
        if gresp.clicked() { ui.memory_mut(|m| m.toggle_popup(pop)); }
        egui::popup::popup_below_widget(ui, pop, &gresp, egui::PopupCloseBehavior::CloseOnClickOutside, |ui| {
            ui.set_min_width(240.0);
            if ui.button("Audio / MIDI settings...").clicked() { app.show_audio = true; }
            ui.separator();
            ui.label(RichText::new("UI Scale").small().color(DIM()));
            let mut pct = ui_scale() as u32;
            if ui.add(egui::Slider::new(&mut pct, SCALE_MIN as u32..=SCALE_MAX as u32).suffix("%")).changed() {
                set_ui_scale(pct as u8);
                apply_scale(ui.ctx());
            }
            ui.separator();
            ui.label(RichText::new("Accent").small().color(DIM()));
            ui.horizontal_wrapped(|ui| {
                for (i, n) in ACCENTS.iter().enumerate() {
                    if ui.selectable_label(super::theme::accent() == i as u8, *n).clicked() {
                        set_accent(i as u8);
                        apply_visuals(ui.ctx());
                    }
                }
            });
            let mut a = auto();
            if ui.checkbox(&mut a, "Auto theme (dark 19:00 - 07:00)").clicked() {
                set_auto(a);
                if a { set_dark(night_now()); apply_visuals(ui.ctx()); }
            }
        });
    });
}
