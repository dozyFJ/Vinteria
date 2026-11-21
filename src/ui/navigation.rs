//! Vertical workspace rail on the left edge, under the header.
use super::{theme::*, Tab};
use egui::{pos2, vec2, Align2, Color32, FontId, Rect, Sense, Stroke};

#[derive(Clone, Copy)]
pub(super) enum Action { View(Tab), Browser(u8) }

#[derive(Clone, Copy, PartialEq)]
enum Icon { Playlist, Rack, Roll, Mixer, Session, Plugins, Browser }

/// small line icons drawn with the painter, so they follow the theme colours
fn draw_icon(p: &egui::Painter, icon: Icon, c: egui::Pos2, col: egui::Color32) {
    let s = Stroke::new(1.8, col);
    match icon {
        Icon::Browser => {
            p.rect_stroke(Rect::from_center_size(c, vec2(24.0, 18.0)), 3.0, s);
            p.line_segment([c + vec2(-5.0, -9.0), c + vec2(-5.0, 9.0)], s);
            for y in [-4.0, 2.0] {
                p.line_segment([c + vec2(0.0, y), c + vec2(7.0, y)], s);
            }
        }
        Icon::Playlist => {
            for (i, w) in [18.0f32, 12.0, 15.0].into_iter().enumerate() {
                let y = c.y - 8.0 + i as f32 * 8.0;
                p.rect_filled(Rect::from_min_size(pos2(c.x - 11.0, y - 2.0), vec2(w + 4.0, 4.0)), 2.0, col);
            }
        }
        Icon::Rack => {
            for r in 0..3 {
                for k in 0..3 {
                    p.rect_filled(Rect::from_min_size(pos2(c.x - 11.0 + k as f32 * 8.0, c.y - 11.0 + r as f32 * 8.0), vec2(6.0, 6.0)), 1.5, col);
                }
            }
        }
        Icon::Roll => {
            p.rect_stroke(Rect::from_center_size(c, vec2(22.0, 22.0)), 3.0, s);
            for k in 0..3 {
                p.line_segment([pos2(c.x - 11.0 + 7.0 * (k + 1) as f32, c.y - 11.0), pos2(c.x - 11.0 + 7.0 * (k + 1) as f32, c.y + 11.0)], s);
            }
            for k in [0.0f32, 1.0, 2.0] {
                if k != 1.0 { p.rect_filled(Rect::from_min_size(pos2(c.x - 11.0 + 7.0 * (k + 1.0) - 3.0, c.y - 11.0), vec2(6.0, 12.0)), 1.0, col); }
            }
        }
        Icon::Mixer => {
            for (i, kn) in [-0.4f32, 0.3, -0.1].into_iter().enumerate() {
                let x = c.x - 9.0 + i as f32 * 9.0;
                p.line_segment([pos2(x, c.y - 11.0), pos2(x, c.y + 11.0)], s);
                p.rect_filled(Rect::from_center_size(pos2(x, c.y + kn * 14.0), vec2(7.0, 4.0)), 1.5, col);
            }
        }
        Icon::Session => {
            p.rect_stroke(Rect::from_center_size(c, vec2(24.0, 24.0)), 4.0, s);
            for (dx, dy) in [(-6.0f32, -6.0f32), (6.0, -6.0), (-6.0, 6.0), (6.0, 6.0)] {
                p.circle_filled(pos2(c.x + dx, c.y + dy), 2.4, col);
            }
        }
        Icon::Plugins => {
            p.circle_stroke(c + vec2(0.0, 2.0), 9.0, s);
            p.line_segment([pos2(c.x - 4.0, c.y - 12.0), pos2(c.x - 4.0, c.y - 6.0)], s);
            p.line_segment([pos2(c.x + 4.0, c.y - 12.0), pos2(c.x + 4.0, c.y - 6.0)], s);
            p.circle_filled(c + vec2(0.0, 2.0), 2.5, col);
        }
    }
}

/// Theme list and light / dark switch: tiles like the pages above (icon on top, label below), over the profile.
fn theme_buttons(ui: &mut egui::Ui, compact: bool) {
    let tw = if compact { 38.0 } else { 112.0 };
    let ix = |r: Rect| if compact { r.center().x } else { r.left() + 24.0 };
    let tile = |ui: &mut egui::Ui| {
        let (r, resp) = ui.allocate_exact_size(vec2(tw, 36.0), Sense::click());
        let pill = r.shrink2(vec2(2.0, 0.0));
        ui.painter().rect_filled(pill, 0.0, Color32::from_white_alpha(if resp.hovered() { 34 } else { 16 }));
        ui.painter().rect_stroke(pill, 0.0, Stroke::new(1.0_f32, Color32::from_white_alpha(if resp.hovered() { 90 } else { 46 })));
        let ink = if resp.hovered() { TEXT() } else { DIM() };
        (r, resp, ink)
    };
    let label = |ui: &egui::Ui, r: Rect, text: &str, ink: egui::Color32| {
        if compact {
            return;
        }
        ui.painter().text(pos2(r.left() + 44.0, r.center().y), Align2::LEFT_CENTER, text, FontId::proportional(13.0), ink);
    };
    // theme list: a palette (circle with colour dots)
    let (r, resp, ink) = tile(ui);
    let c = pos2(ix(r), r.center().y);
    ui.painter().circle_stroke(c, 10.0, Stroke::new(1.8, ink));
    for (dx, dy, col) in [(-4.0f32, -3.0f32, TEAL()), (3.5, -3.5, TEAL_HI()), (0.0, 4.0, CYAN())] {
        ui.painter().circle_filled(c + vec2(dx, dy), 2.4, col);
    }
    label(ui, r, "Theme", ink);
    let resp = resp.on_hover_text(format!("Theme: {}", theme_name()));
    let pop = egui::Id::new("nav_theme_popup");
    if resp.clicked() {
        ui.memory_mut(|m| m.toggle_popup(pop));
    }
    // the theme list opens as a panel at the left edge, the themes as tiles two to a row
    if ui.memory(|m| m.is_popup_open(pop)) {
        let top = resp.rect.bottom() + 6.0;
        let ar = egui::Area::new(pop.with("left_panel")).order(egui::Order::Foreground).fixed_pos(pos2(6.0, top)).show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                // a side panel: full height of the window, narrow, the themes two to a row
                ui.set_width(262.0);
                let h = (ui.ctx().screen_rect().height() - top - 24.0).max(200.0);
                ui.set_min_height(h);
                egui::ScrollArea::vertical().max_height(h).auto_shrink([false, false]).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    theme_picker(ui);
                });
            });
        });
        let out = ui.ctx().input(|i| i.pointer.any_pressed() && i.pointer.interact_pos().map_or(false, |p| !ar.response.rect.contains(p) && !resp.rect.contains(p)));
        if out || ui.ctx().input(|i| i.key_pressed(egui::Key::Escape)) {
            ui.memory_mut(|m| m.close_popup());
        }
    }
    // light / dark switch: the sun in the dark theme, the moon in the light one
    let (r, resp, ink) = tile(ui);
    let c = pos2(ix(r), r.center().y);
    if is_dark() {
        ui.painter().circle_filled(c, 5.0, ink);
        for k in 0..8 {
            let a = k as f32 * std::f32::consts::TAU / 8.0;
            let d = vec2(a.cos(), a.sin());
            ui.painter().line_segment([c + d * 8.0, c + d * 11.0], Stroke::new(1.8, ink));
        }
    } else {
        ui.painter().circle_filled(c, 9.0, ink);
        ui.painter().circle_filled(c + vec2(5.0, -4.0), 8.0, SIDE());
    }
    label(ui, r, if is_dark() { "Light" } else { "Dark" }, ink);
    if resp.on_hover_text(if is_dark() { "Switch to the light theme" } else { "Switch to the dark theme" }).clicked() {
        set_auto(false);
        set_dark(!is_dark());
        save_prefs();
        apply_visuals(ui.ctx());
    }
}

/// Google account button at the bottom of the rail: a round avatar with the initial, and a popup to sign in / out.
fn profile_button(ui: &mut egui::Ui, compact: bool) {
    use crate::google_auth as g;
    let profile = g::profile();
    let status = g::status();
    let (rect, resp) = ui.allocate_exact_size(vec2(if compact { 38.0 } else { 112.0 }, 36.0), Sense::click());
    let p = ui.painter();
    let pill = rect.shrink2(vec2(2.0, 0.0));
    p.rect_filled(pill, 0.0, Color32::from_white_alpha(if resp.hovered() { 34 } else { 16 }));
    p.rect_stroke(pill, 0.0, Stroke::new(1.0_f32, Color32::from_white_alpha(if resp.hovered() { 90 } else { 46 })));
    let c = pos2(if compact { rect.center().x } else { rect.left() + 24.0 }, rect.center().y);
    let initial = profile.as_ref().and_then(|p| p.name.chars().next().or_else(|| p.email.chars().next())).map(|ch| ch.to_uppercase().to_string()).unwrap_or_else(|| "G".into());
    p.rect_filled(Rect::from_center_size(c, vec2(26.0, 26.0)), 0.0, if profile.is_some() { TEAL() } else { PANEL3() });
    p.rect_stroke(Rect::from_center_size(c, vec2(26.0, 26.0)), 0.0, Stroke::new(1.5, if profile.is_some() { TEAL() } else { DIM() }));
    p.text(c, Align2::CENTER_CENTER, initial, FontId::proportional(14.0), if profile.is_some() { Color32::WHITE } else { TEXT() });
    if !compact {
        p.text(pos2(rect.left() + 44.0, rect.center().y), Align2::LEFT_CENTER, if profile.is_some() { "Profile" } else { "Sign in" }, FontId::proportional(13.0), if resp.hovered() { TEXT() } else { DIM() });
    }
    let resp = resp.on_hover_text("Google account");
    let pop = egui::Id::new("google_profile_popup");
    if resp.clicked() {
        // the account panel on the left replaces the old popup
        ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("account_toggle"), true));
    }
    if false { egui::popup::popup_above_or_below_widget(ui, pop, &resp, egui::AboveOrBelow::Below, egui::PopupCloseBehavior::CloseOnClickOutside, |ui| {
        ui.set_min_width(240.0);
        if let Some(pr) = &profile {
            ui.label(egui::RichText::new(if pr.name.is_empty() { "Google account" } else { &pr.name }).strong().color(TEXT()));
            ui.label(egui::RichText::new(&pr.email).size(11.0).color(DIM()));
            ui.add_space(6.0);
            if ui.button("Sign out").clicked() {
                g::sign_out();
            }
            return;
        }
        ui.label(egui::RichText::new("Google account").strong().color(TEXT()));
        if !g::configured() {
            ui.label(egui::RichText::new("One-time setup: create a \"Desktop app\" OAuth client in Google Cloud Console and put its client_id and client_secret into google_oauth.json next to the program.").size(11.0).color(DIM()));
            if ui.button("Create google_oauth.json").clicked() {
                let path = g::create_config_template();
                let _ = std::process::Command::new("notepad").arg(path).spawn();
            }
        }
        match &status {
            g::Status::Waiting => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Waiting for the browser...");
                });
            }
            g::Status::Failed(e) => {
                ui.colored_label(RED, e);
            }
            g::Status::SignedOut => {}
        }
        if ui.add_enabled(status != g::Status::Waiting, egui::Button::new("Sign in with Google")).clicked() {
            g::sign_in();
        }
    }); }
    if status == g::Status::Waiting {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(300));
    }
}

pub(super) fn show(ctx: &egui::Context, main: Tab, home: bool) -> Option<Action> {
    let mut action = None;
    // the main menu: one row of pills under the header, the pages on the left, theme / light / account on the right
    egui::TopBottomPanel::top("workspace_navigation")
        .resizable(false)
        .exact_height(52.0)
        .frame(egui::Frame::none().fill(BG()).inner_margin(egui::Margin::symmetric(14.0, 6.0)))
        .show(ctx, |ui| {
            ui.spacing_mut().item_spacing = vec2(6.0, 0.0);
            let items: [(&str, Icon, bool, &str, Action); 1] = [
                ("Playlist", Icon::Playlist, !home && main == Tab::Arrangement, "Arrangement - F5", Action::View(Tab::Arrangement)),
            ];
            // one rounded bar behind the pages, like the AOS dock; the open page is a solid accent pill
            let bar_w = items.len() as f32 * 128.0 + 12.0;
            let bar = Rect::from_min_size(ui.cursor().min, vec2(bar_w, 40.0));
            ui.allocate_ui_at_rect(bar.shrink2(vec2(0.0, 3.0)), |ui| {
                ui.horizontal(|ui| {
                    for (label, icon, active, tip, act) in items {
                        let (rect, resp) = ui.allocate_exact_size(vec2(122.0, 34.0), Sense::click());
                        let hot = resp.hovered();
                        let p = ui.painter();
                        if active {
                            p.rect_filled(rect, 3.0, TEAL());
                        } else if hot {
                            p.rect_filled(rect, 3.0, PANEL3());
                        }
                        let ink = if active { INSET() } else if hot { TEXT() } else { DIM() };
                        draw_icon(p, icon, pos2(rect.left() + 24.0, rect.center().y), ink);
                        p.text(pos2(rect.left() + 44.0, rect.center().y), Align2::LEFT_CENTER, label, FontId::proportional(13.5), ink);
                        if resp.on_hover_text(tip).clicked() {
                            action = Some(act);
                        }
                    }
                });
            });
        });
    action
}

/// Light / dark, the theme list and the account: small round buttons in the header.
pub(super) fn header_buttons(ui: &mut egui::Ui) {
    ui.spacing_mut().item_spacing.x = 2.0;
    theme_buttons(ui, true);
    profile_button(ui, true);
}
