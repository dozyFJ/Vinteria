//! Phone / tablet layout: two modes.
//! **Hub** (launcher) on start: Home · Plugins · Settings · Profile — pick or create a project.
//! **DAW** after entering a project: Playlist · Channel Rack · Piano Roll · Mixer — like FL Mobile.
use super::*;
use crate::mixer::{NUM_CH, NUM_INSERTS};
use egui::{Button, Frame, Margin, RichText};

const HUB_TABS: [&str; 4] = ["Home", "Plugins", "Settings", "Profile"];
const DAW_TABS: [&str; 4] = ["Playlist", "Channels", "Piano Roll", "Mixer"];
const TOUCH: f32 = 46.0;

fn page_title(ui: &mut egui::Ui, title: &str) {
    ui.label(RichText::new(title).size(20.0).strong().color(TEXT()));
    ui.add_space(2.0);
}

fn page_pills(ui: &mut egui::Ui, id_salt: &str, pills: &[&str]) -> usize {
    let id = egui::Id::new(id_salt);
    let mut sel: usize = ui.data(|d| d.get_temp(id)).unwrap_or(0).min(pills.len().saturating_sub(1));
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(8.0, 4.0);
        for (i, name) in pills.iter().enumerate() {
            let on = sel == i;
            let b = Button::new(RichText::new(*name).size(13.0).color(if on { Color32::WHITE } else { DIM() }))
                .fill(if on { TEAL() } else { PANEL3() })
                .stroke(Stroke::new(1.0, if on { TEAL() } else { MID() }))
                .rounding(17.0);
            if ui.add(b).clicked() {
                sel = i;
            }
        }
    });
    ui.data_mut(|d| d.insert_temp(id, sel));
    ui.add_space(6.0);
    sel
}

impl VinteriaApp {
    pub(super) fn is_mobile(&self) -> bool {
        cfg!(target_os = "android") || std::env::var_os("SYNTHESIA_MOBILE").is_some()
    }

    pub(super) fn mobile_ui(&mut self, ctx: &egui::Context) {
        let screen = ctx.screen_rect();
        let portrait = screen.width() < screen.height();
        ctx.set_pixels_per_point(ctx.pixels_per_point());

        if self.m_daw {
            self.mobile_daw_mode(ctx, portrait);
        } else {
            self.mobile_hub_mode(ctx, portrait);
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(250));
    }

    // ───────────────────── HUB MODE (launcher) ─────────────────────

    fn mobile_hub_mode(&mut self, ctx: &egui::Context, portrait: bool) {
        egui::TopBottomPanel::top("m_hub_top").frame(Frame::none().fill(SIDE()).inner_margin(Margin::symmetric(12.0, 10.0))).show(ctx, |ui| {
            neon_line(ui.painter(), ui.clip_rect());
            ui.horizontal(|ui| {
                ui.label(RichText::new("Vinteria Daw").size(22.0).strong().color(TEAL()));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new("Beta").size(12.0).color(DIM()));
                });
            });
        });

        if portrait {
            egui::TopBottomPanel::bottom("m_hub_tabs").frame(Frame::none().fill(SIDE()).stroke(Stroke::new(border_w(), border())).inner_margin(Margin::symmetric(6.0, 6.0))).show(ctx, |ui| {
                ui.columns(HUB_TABS.len(), |cols| {
                    for (i, name) in HUB_TABS.iter().enumerate() {
                        let w = cols[i].available_width();
                        self.hub_tab_button(&mut cols[i], i, name, vec2(w, TOUCH));
                    }
                });
            });
        } else {
            egui::SidePanel::left("m_hub_rail").exact_width(118.0).resizable(false).frame(Frame::none().fill(SIDE()).stroke(Stroke::new(border_w(), border())).inner_margin(Margin::symmetric(6.0, 8.0))).show(ctx, |ui| {
                ui.spacing_mut().item_spacing.y = 8.0;
                for (i, name) in HUB_TABS.iter().enumerate() {
                    let w = ui.available_width();
                    self.hub_tab_button(ui, i, name, vec2(w, TOUCH + 8.0));
                }
            });
        }

        egui::CentralPanel::default().frame(Frame::none().fill(BG()).inner_margin(Margin::symmetric(8.0, 8.0))).show(ctx, |ui| {
            match self.m_tab {
                0 => self.hub_home(ui),
                1 => self.hub_plugins(ui),
                2 => self.hub_settings(ui, ctx),
                _ => self.hub_profile(ui),
            }
        });
    }

    fn hub_tab_button(&mut self, ui: &mut egui::Ui, i: usize, name: &str, size: egui::Vec2) {
        let on = self.m_tab == i;
        let b = Button::new(RichText::new(name).size(14.0).strong().color(if on { Color32::WHITE } else { DIM() }))
            .fill(if on { TEAL().linear_multiply(0.7) } else { PANEL3() })
            .stroke(Stroke::new(1.0, if on { TEAL() } else { MID() }));
        if ui.add_sized(size, b).clicked() {
            self.m_tab = i;
        }
    }

    fn hub_home(&mut self, ui: &mut egui::Ui) {
        page_title(ui, "Home");
        let sub = page_pills(ui, "hub_home_pills", &["All", "Recent", "Templates", "Sounds"]);
        if self.hub_cache.is_none() {
            self.hub_cache = Some(hub_projects(&self.recent));
        }
        let list = self.hub_cache.clone().unwrap_or_default();
        let mut open = None;
        let mut tpl = None;

        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            // current project card
            Frame::none().fill(PANEL2()).rounding(card()).stroke(Stroke::new(1.0, TEAL())).inner_margin(14.0).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new("Current project").size(11.0).color(DIM()));
                ui.label(RichText::new(self.project.name.clone()).size(18.0).strong());
                ui.label(RichText::new(format!("{:.0} BPM  ·  {} tracks  ·  {} clips", self.project.bpm, self.project.arrangement.tracks.len(), self.project.arrangement.clips.len())).size(12.0).color(DIM()));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.add_sized([140.0, TOUCH], Button::new(RichText::new("Open DAW").strong().color(Color32::WHITE)).fill(TEAL())).clicked() {
                        self.m_daw = true;
                        self.m_tab = 0;
                    }
                    if ui.add_sized([140.0, TOUCH], Button::new("+ New project")).clicked() {
                        self.new_project();
                        self.m_daw = true;
                        self.m_tab = 0;
                    }
                });
            });
            ui.add_space(12.0);

            match sub {
                3 => {
                    // Sounds
                    self.hub_sounds_list(ui);
                }
                2 => {
                    // Templates
                    ui.label(RichText::new("TEMPLATES").size(12.0).strong().color(TEAL_HI()));
                    ui.add_space(4.0);
                    for (i, (name, _)) in crate::templates::TEMPLATES.iter().enumerate() {
                        if name.starts_with("Empty") { continue; }
                        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 52.0), egui::Sense::click());
                        let p = ui.painter();
                        p.rect_filled(rect, card(), PANEL2());
                        p.rect_stroke(rect, card(), Stroke::new(1.0, border()));
                        p.rect_filled(egui::Rect::from_min_size(rect.min, vec2(5.0, rect.height())), 0.0, lane_col(i));
                        p.text(rect.left_center() + vec2(18.0, 0.0), egui::Align2::LEFT_CENTER, *name, egui::FontId::proportional(15.0), TEXT());
                        p.text(rect.right_center() - vec2(14.0, 0.0), egui::Align2::RIGHT_CENTER, "Template", egui::FontId::proportional(11.0), LIGHT());
                        if resp.clicked() { tpl = Some(i); }
                        ui.add_space(5.0);
                    }
                }
                _ => {
                    // All or Recent
                    if ui.add_sized([ui.available_width(), TOUCH + 4.0], Button::new(RichText::new("+  New project").strong().size(16.0))).clicked() {
                        self.new_project();
                        self.m_daw = true;
                        self.m_tab = 0;
                    }
                    ui.add_space(8.0);
                    let show_recent = sub == 1;
                    let items: Vec<(PathBuf, u64)> = if show_recent {
                        self.recent.iter().filter_map(|p| std::fs::metadata(p).ok().map(|m| (p.clone(), m.len()))).collect()
                    } else {
                        list.iter().map(|(p, _, s)| (p.clone(), *s)).collect()
                    };
                    if items.is_empty() {
                        ui.label(RichText::new(if show_recent { "No recently opened projects." } else { "No saved projects yet. Start from a template." }).color(DIM()));
                    }
                    for (i, (path, size)) in items.iter().enumerate() {
                        let name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 66.0), egui::Sense::click());
                        let col = lane_col(i);
                        let p = ui.painter();
                        p.rect_filled(rect, card(), PANEL2());
                        p.rect_stroke(rect, card(), Stroke::new(1.0, border()));
                        let block = egui::Rect::from_min_size(rect.min, vec2(60.0, rect.height()));
                        p.rect_filled(block, card(), col.linear_multiply(0.28));
                        p.rect_filled(egui::Rect::from_min_size(rect.min, vec2(4.0, rect.height())), 0.0, col);
                        p.text(block.center(), egui::Align2::CENTER_CENTER, name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default(), egui::FontId::proportional(28.0), col);
                        p.text(rect.left_top() + vec2(74.0, 12.0), egui::Align2::LEFT_TOP, &name, egui::FontId::proportional(16.0), TEXT());
                        p.text(rect.left_top() + vec2(74.0, 38.0), egui::Align2::LEFT_TOP, format!("{} KB", size / 1024), egui::FontId::proportional(11.0), DIM());
                        if resp.clicked() { open = Some(path.clone()); }
                        ui.add_space(6.0);
                    }
                }
            }
        });

        if let Some(i) = tpl {
            self.load_template(i);
            self.m_daw = true;
            self.m_tab = 0;
        }
        if let Some(path) = open {
            self.open_path(path);
            self.m_daw = true;
            self.m_tab = 0;
        }
    }

    fn hub_sounds_list(&mut self, ui: &mut egui::Ui) {
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
            ui.label(RichText::new("No sounds found. Put WAV files into the samples folder.").color(DIM()));
        }
        let mut last = String::new();
        for (kit, name) in &list {
            if *kit != last {
                ui.add_space(8.0);
                ui.label(RichText::new(kit.to_uppercase()).size(12.0).strong().color(TEAL_HI()));
                last = kit.clone();
            }
            let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 38.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, card(), PANEL2());
            ui.painter().text(rect.left_center() + vec2(12.0, 0.0), egui::Align2::LEFT_CENTER, name, egui::FontId::proportional(14.0), TEXT());
            ui.add_space(3.0);
        }
    }

    fn hub_plugins(&mut self, ui: &mut egui::Ui) {
        page_title(ui, "Plugins");
        let sub = page_pills(ui, "hub_plug_pills", &["Instruments", "Effects"]);
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            if sub == 0 {
                for (i, name) in ["Wavetable Synth", "Analog Synth", "Poly Synth", "Bass 808", "Drum Machine", "Sampler"].iter().enumerate() {
                    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 56.0), egui::Sense::click());
                    let col = lane_col(i);
                    let p = ui.painter();
                    p.rect_filled(rect, card(), PANEL2());
                    p.rect_stroke(rect, card(), Stroke::new(1.0, border()));
                    p.rect_filled(egui::Rect::from_min_size(rect.min, vec2(5.0, rect.height())), 0.0, col);
                    p.text(rect.left_center() + vec2(18.0, 0.0), egui::Align2::LEFT_CENTER, *name, egui::FontId::proportional(15.0), TEXT());
                    p.text(rect.right_center() - vec2(14.0, 0.0), egui::Align2::RIGHT_CENTER, "Built-in", egui::FontId::proportional(11.0), LIGHT());
                    let _ = resp;
                    ui.add_space(5.0);
                }
            } else {
                for (i, name) in ["Reverb", "Delay", "Compressor", "Limiter", "EQ", "Chorus", "Distortion", "Filter"].iter().enumerate() {
                    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 56.0), egui::Sense::click());
                    let col = lane_col(i + 6);
                    let p = ui.painter();
                    p.rect_filled(rect, card(), PANEL2());
                    p.rect_stroke(rect, card(), Stroke::new(1.0, border()));
                    p.rect_filled(egui::Rect::from_min_size(rect.min, vec2(5.0, rect.height())), 0.0, col);
                    p.text(rect.left_center() + vec2(18.0, 0.0), egui::Align2::LEFT_CENTER, *name, egui::FontId::proportional(15.0), TEXT());
                    p.text(rect.right_center() - vec2(14.0, 0.0), egui::Align2::RIGHT_CENTER, "Built-in", egui::FontId::proportional(11.0), LIGHT());
                    let _ = resp;
                    ui.add_space(5.0);
                }
            }
        });
    }

    fn hub_settings(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        page_title(ui, "Settings");
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.label(RichText::new("THEME").size(12.0).strong().color(TEAL_HI()));
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui.add_sized([120.0, TOUCH], Button::new("Dark")).clicked() {
                    set_dark(true);
                    save_prefs();
                    apply_visuals(ctx);
                }
                if ui.add_sized([120.0, TOUCH], Button::new("Light")).clicked() {
                    set_dark(false);
                    save_prefs();
                    apply_visuals(ctx);
                }
            });
            ui.add_space(8.0);
            ui.label(RichText::new("ACCENT").size(12.0).strong().color(TEAL_HI()));
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                for (i, n) in ACCENTS.iter().enumerate() {
                    if ui.add_sized([96.0, 40.0], Button::new(*n).selected(accent() == i as u8)).clicked() {
                        set_accent(i as u8);
                        apply_visuals(ctx);
                    }
                }
            });
            ui.add_space(12.0);
            ui.label(RichText::new("SYNC WITH PC").size(12.0).strong().color(TEAL_HI()));
            ui.add_space(4.0);
            ui.label(RichText::new(format!("Projects folder: {}", hub_dir().display())).size(12.0));
            ui.label(RichText::new("On the PC choose the same cloud folder (Google Drive, OneDrive, Syncthing) in the Hub. A project saved on one device then appears in Projects on the other.").size(11.0).color(DIM()));
            ui.add_space(12.0);
            ui.label(RichText::new("ABOUT").size(12.0).strong().color(TEAL_HI()));
            ui.add_space(4.0);
            ui.label(format!("Vinteria Daw  Beta 1  v{}", env!("CARGO_PKG_VERSION")));
            ui.label(RichText::new("Wave Audio Synthesis Digital  ·  Vinteria Studio").size(12.0).color(DIM()));
        });
    }

    fn hub_profile(&mut self, ui: &mut egui::Ui) {
        page_title(ui, "Profile");
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            Frame::none().fill(PANEL2()).rounding(card()).stroke(Stroke::new(1.0, border())).inner_margin(14.0).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(56.0, 56.0), egui::Sense::hover());
                    ui.painter().rect_filled(r, 28.0, TEAL().linear_multiply(0.35));
                    ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, "V", egui::FontId::proportional(28.0), TEAL());
                    ui.vertical(|ui| {
                        ui.label(RichText::new("Vinteria Studio").size(18.0).strong());
                        ui.label(RichText::new("Guest  ·  sign-in comes in a later beta").size(12.0).color(DIM()));
                    });
                });
            });
            ui.add_space(12.0);
            ui.label(RichText::new("Sign-in with Google, cloud projects and collaboration are coming in the next beta.").size(12.0).color(DIM()));
        });
    }

    // ───────────────────── DAW MODE (FL Mobile style) ─────────────────────

    fn mobile_daw_mode(&mut self, ctx: &egui::Context, portrait: bool) {
        // top transport bar
        egui::TopBottomPanel::top("m_daw_top").frame(Frame::none().fill(SIDE()).inner_margin(Margin::symmetric(8.0, 6.0))).show(ctx, |ui| {
            neon_line(ui.painter(), ui.clip_rect());
            ui.horizontal_wrapped(|ui| {
                if ui.add_sized([52.0, TOUCH], Button::new(RichText::new("\u{2190}").size(18.0))).clicked() {
                    self.m_daw = false;
                    self.m_tab = 0;
                    self.hub_cache = None;
                }
                let name = if self.project.name.chars().count() > 14 { format!("{}...", self.project.name.chars().take(14).collect::<String>()) } else { self.project.name.clone() };
                ui.label(RichText::new(name).strong().size(16.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let playing = self.playing();
                    let label = if playing { "STOP" } else { "PLAY" };
                    if ui.add_sized([80.0, TOUCH], Button::new(RichText::new(label).strong().color(Color32::WHITE)).fill(TEAL())).clicked() {
                        self.toggle_play();
                    }
                    ui.add_sized([88.0, TOUCH], egui::DragValue::new(&mut self.project.bpm).range(40.0..=300.0).speed(0.5).suffix(" BPM"));
                });
            });
        });

        // bottom tabs like FL Mobile
        if portrait {
            egui::TopBottomPanel::bottom("m_daw_tabs").frame(Frame::none().fill(SIDE()).stroke(Stroke::new(border_w(), border())).inner_margin(Margin::symmetric(6.0, 6.0))).show(ctx, |ui| {
                ui.columns(DAW_TABS.len(), |cols| {
                    for (i, name) in DAW_TABS.iter().enumerate() {
                        let w = cols[i].available_width();
                        self.daw_tab_button(&mut cols[i], i, name, vec2(w, TOUCH));
                    }
                });
            });
        } else {
            egui::SidePanel::left("m_daw_rail").exact_width(118.0).resizable(false).frame(Frame::none().fill(SIDE()).stroke(Stroke::new(border_w(), border())).inner_margin(Margin::symmetric(6.0, 8.0))).show(ctx, |ui| {
                ui.spacing_mut().item_spacing.y = 8.0;
                for (i, name) in DAW_TABS.iter().enumerate() {
                    let w = ui.available_width();
                    self.daw_tab_button(ui, i, name, vec2(w, TOUCH + 8.0));
                }
            });
        }

        egui::CentralPanel::default().frame(Frame::none().fill(BG()).inner_margin(Margin::symmetric(8.0, 8.0))).show(ctx, |ui| {
            match self.m_tab {
                0 => self.daw_playlist(ui),
                1 => self.daw_channels(ui, portrait),
                2 => self.daw_piano_roll(ui),
                _ => self.daw_mixer(ui),
            }
        });
    }

    fn daw_tab_button(&mut self, ui: &mut egui::Ui, i: usize, name: &str, size: egui::Vec2) {
        let on = self.m_tab == i;
        let b = Button::new(RichText::new(name).size(14.0).strong().color(if on { Color32::WHITE } else { DIM() }))
            .fill(if on { TEAL().linear_multiply(0.7) } else { PANEL3() })
            .stroke(Stroke::new(1.0, if on { TEAL() } else { MID() }));
        if ui.add_sized(size, b).clicked() {
            self.m_tab = i;
        }
    }

    // Playlist (arrangement overview)
    fn daw_playlist(&mut self, ui: &mut egui::Ui) {
        page_title(ui, "Playlist");
        self.mobile_arrange(ui);
    }

    // Channel Rack: tracks list + add buttons (like FL's channel rack)
    fn daw_channels(&mut self, ui: &mut egui::Ui, portrait: bool) {
        page_title(ui, "Channels");
        let sub = page_pills(ui, "daw_ch_pills", &["Tracks", "Patterns"]);
        match sub {
            0 => self.mobile_tracks(ui, portrait),
            _ => self.daw_patterns(ui),
        }
    }

    fn daw_patterns(&mut self, ui: &mut egui::Ui) {
        let n = self.project.patterns.len();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for i in 0..n {
                let pat = &self.project.patterns[i];
                let name = pat.name.clone();
                let steps = pat.notes.len();
                let col = lane_col(i);
                let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 52.0), egui::Sense::click());
                let p = ui.painter();
                p.rect_filled(rect, card(), PANEL2());
                p.rect_stroke(rect, card(), Stroke::new(1.0, border()));
                p.rect_filled(egui::Rect::from_min_size(rect.min, vec2(5.0, rect.height())), 0.0, col);
                p.text(rect.left_center() + vec2(18.0, 0.0), egui::Align2::LEFT_CENTER, &name, egui::FontId::proportional(15.0), TEXT());
                p.text(rect.right_center() - vec2(14.0, 0.0), egui::Align2::RIGHT_CENTER, format!("{} notes", steps), egui::FontId::proportional(11.0), LIGHT());
                if resp.clicked() {
                    self.project.current_pattern = i;
                }
                if self.project.current_pattern == i {
                    p.rect_stroke(rect, card(), Stroke::new(2.0, TEAL()));
                }
                ui.add_space(5.0);
            }
            if n == 0 {
                ui.label(RichText::new("No patterns yet. Add a track first.").color(DIM()));
            }
        });
    }

    // Piano Roll (note grid for the current pattern)
    fn daw_piano_roll(&mut self, ui: &mut egui::Ui) {
        page_title(ui, "Piano Roll");
        let pat = self.project.current_pattern;
        if pat >= self.project.patterns.len() {
            ui.label(RichText::new("Select a pattern in the Channels tab.").color(DIM()));
            return;
        }
        let pattern = &self.project.patterns[pat];
        ui.label(RichText::new(format!("Pattern: {}", pattern.name)).size(14.0).color(DIM()));
        ui.add_space(4.0);

        let key_w = 36.0f32;
        let step_w = 16.0f32;
        let key_h = 14.0f32;
        let keys = 48u8;
        let base = 36u8; // C2
        let steps = 64usize;

        egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
            let (rect, _resp) = ui.allocate_exact_size(vec2(key_w + steps as f32 * step_w, keys as f32 * key_h), egui::Sense::click());
            let p = ui.painter_at(rect);
            p.rect_filled(rect, 0.0, BG());

            // draw grid
            for k in 0..keys {
                let y = rect.top() + k as f32 * key_h;
                let pitch = base + keys - 1 - k;
                let is_black = matches!(pitch % 12, 1 | 3 | 6 | 8 | 10);
                let row_r = egui::Rect::from_min_size(egui::pos2(rect.left(), y), vec2(rect.width(), key_h));
                if is_black {
                    p.rect_filled(row_r, 0.0, SIDE());
                }
                // key label
                let note_names = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
                let oct = (pitch / 12) as i32 - 1;
                let label = format!("{}{}", note_names[(pitch % 12) as usize], oct);
                p.text(egui::pos2(rect.left() + 4.0, y + key_h * 0.5), egui::Align2::LEFT_CENTER, label, egui::FontId::monospace(9.0), if is_black { LIGHT() } else { DIM() });

                p.line_segment([egui::pos2(rect.left() + key_w, y), egui::pos2(rect.right(), y)], Stroke::new(0.5, border()));
            }
            // vertical beat lines
            for s in 0..=steps {
                let x = rect.left() + key_w + s as f32 * step_w;
                let stroke = if s % 16 == 0 { Stroke::new(1.0, MID()) } else if s % 4 == 0 { Stroke::new(0.5, MID()) } else { Stroke::new(0.5, border()) };
                p.line_segment([egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())], stroke);
            }
            // draw notes
            let tps = TICKS_PER_BAR as f32 / 16.0;
            for n in &pattern.notes {
                if n.pitch < base || n.pitch >= base + keys { continue; }
                let row = (base + keys - 1 - n.pitch) as f32;
                let x = rect.left() + key_w + n.start as f32 / tps * step_w;
                let w = (n.len as f32 / tps * step_w).max(4.0);
                let note_r = egui::Rect::from_min_size(egui::pos2(x, rect.top() + row * key_h + 1.0), vec2(w, key_h - 2.0));
                p.rect_filled(note_r, 2.0, TEAL().linear_multiply(0.5 + n.vel as f32 / 254.0));
                p.rect_stroke(note_r, 2.0, Stroke::new(1.0, TEAL()));
            }
        });
    }

    // Mixer
    fn daw_mixer(&mut self, ui: &mut egui::Ui) {
        page_title(ui, "Mixer");
        self.mobile_mixer(ui);
    }

    // ───────────────────── shared rendering helpers ─────────────────────

    fn mobile_arrange(&mut self, ui: &mut egui::Ui) {
        let (bar_w, lane_h, hdr, ruler) = (46.0f32, 46.0f32, 92.0f32, 26.0f32);
        let tracks = self.project.arrangement.tracks.len();
        if tracks == 0 {
            ui.label("No tracks yet. Open a template from the Hub.");
            return;
        }
        let bars = (self.project.arrangement.song_end_bar() + 8).max(16);
        let pos = self.shared.position.load(Ordering::Relaxed);
        let playing = self.shared.playing.load(Ordering::Relaxed) && self.project.song_mode;
        egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
            let (rect, resp) = ui.allocate_exact_size(vec2(hdr + bars as f32 * bar_w, ruler + tracks as f32 * lane_h), egui::Sense::click());
            let p = ui.painter_at(rect);
            let gx = rect.left() + hdr;
            let gy = rect.top() + ruler;
            p.rect_filled(rect, 0.0, BG());
            for l in 0..tracks {
                let r = egui::Rect::from_min_size(egui::pos2(rect.left(), gy + l as f32 * lane_h), vec2(rect.width(), lane_h - 1.0));
                p.rect_filled(r, 0.0, if l % 2 == 0 { SIDE() } else { BG() });
                let t = &self.project.arrangement.tracks[l];
                let col = lane_col(l);
                p.rect_filled(egui::Rect::from_min_size(r.min, vec2(4.0, r.height())), 0.0, col);
                let name: String = t.name.chars().take(12).collect();
                p.text(r.left_center() + vec2(10.0, 0.0), egui::Align2::LEFT_CENTER, name, egui::FontId::proportional(13.0), if t.mute { LIGHT() } else { TEXT() });
            }
            for b in 0..=bars {
                let x = gx + b as f32 * bar_w;
                p.line_segment([egui::pos2(x, rect.top() + ruler - 4.0), egui::pos2(x, rect.bottom())], Stroke::new(1.0, if b % 4 == 0 { MID() } else { border() }));
                if b < bars && (b % 2 == 0 || bar_w >= 40.0) {
                    p.text(egui::pos2(x + 4.0, rect.top() + 5.0), egui::Align2::LEFT_TOP, format!("{}", b + 1), egui::FontId::monospace(11.0), DIM());
                }
            }
            for c in &self.project.arrangement.clips {
                let x0 = gx + c.start as f32 / TICKS_PER_BAR as f32 * bar_w;
                let w = (c.len as f32 / TICKS_PER_BAR as f32 * bar_w).max(6.0);
                let r = egui::Rect::from_min_size(egui::pos2(x0, gy + c.lane as f32 * lane_h), vec2(w, lane_h - 1.0)).shrink2(vec2(1.0, 4.0));
                let col = lane_col(c.lane);
                p.rect_filled(r, small(), col.linear_multiply(0.7));
                p.rect_stroke(r, small(), Stroke::new(1.0, col));
                if r.width() > 40.0 {
                    let name = if c.audio { self.project.audio_pool.get(c.pattern).map(|s| s.name.clone()).unwrap_or_default() } else { self.project.patterns.get(c.pattern).map(|p| p.name.clone()).unwrap_or_default() };
                    let text: String = name.chars().take(((r.width() - 8.0) / 6.5) as usize).collect();
                    p.text(r.left_center() + vec2(5.0, 0.0), egui::Align2::LEFT_CENTER, text, egui::FontId::proportional(11.0), Color32::WHITE);
                }
            }
            if playing {
                let x = gx + pos as f32 / TICKS_PER_BAR as f32 * bar_w;
                p.line_segment([egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())], Stroke::new(2.0, CYAN()));
            }
            let _ = resp;
        });
    }

    fn mobile_tracks(&mut self, ui: &mut egui::Ui, portrait: bool) {
        let n = self.project.arrangement.tracks.len();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.add_sized([120.0, TOUCH], Button::new("+ MIDI track")).clicked() {
                    self.project.arrangement.add_track(crate::arrangement::TrackKind::Midi);
                }
                if ui.add_sized([120.0, TOUCH], Button::new("+ Audio track")).clicked() {
                    self.project.arrangement.add_track(crate::arrangement::TrackKind::Audio);
                }
            });
            ui.add_space(6.0);
            if portrait {
                for l in 0..n {
                    self.mobile_track_card(ui, l);
                }
            } else {
                ui.columns(2, |cols| {
                    for l in 0..n {
                        let c = &mut cols[l % 2];
                        self.mobile_track_card(c, l);
                    }
                });
            }
        });
    }

    fn mobile_track_card(&mut self, ui: &mut egui::Ui, l: usize) {
        let col = lane_col(l);
        let audio = self.project.arrangement.tracks[l].kind == crate::arrangement::TrackKind::Audio;
        Frame::none().fill(PANEL2()).rounding(card()).stroke(Stroke::new(1.0, border())).inner_margin(10.0).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(6.0, 28.0), egui::Sense::hover());
                ui.painter().rect_filled(r, 2.0, col);
                let t = &mut self.project.arrangement.tracks[l];
                ui.label(RichText::new(if audio { "AUDIO" } else { "MIDI" }).size(10.0).strong().color(col));
                ui.add(egui::TextEdit::singleline(&mut t.name).desired_width(ui.available_width() - 150.0).font(egui::FontId::proportional(15.0)));
                for (flag, label, on) in [(0u8, "M", AMBER), (1, "S", TEAL_HI())] {
                    let v = if flag == 0 { &mut t.mute } else { &mut t.solo };
                    let b = Button::new(RichText::new(label).strong().color(if *v { Color32::WHITE } else { DIM() })).fill(if *v { on } else { PANEL3() });
                    if ui.add_sized([46.0, 40.0], b).clicked() {
                        *v = !*v;
                    }
                }
            });
            let t = &mut self.project.arrangement.tracks[l];
            ui.spacing_mut().slider_width = ui.available_width() - 70.0;
            ui.horizontal(|ui| {
                ui.add(egui::Slider::new(&mut t.volume, 0.0..=1.5).show_value(false));
                ui.label(RichText::new(format!("{:+.1} dB", db(t.volume))).monospace().size(11.0).color(DIM()));
            });
            if let Some(inst) = t.instrument {
                ui.label(RichText::new(instruments_ui::Inst::from_track(inst).title()).size(11.0).color(DIM()));
            }
        });
        ui.add_space(6.0);
    }

    fn mobile_mixer(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::horizontal().auto_shrink([false, false]).show(ui, |ui| {
            ui.horizontal_top(|ui| {
                self.mobile_strip(ui, None);
                for i in 0..NUM_INSERTS {
                    self.mobile_strip(ui, Some(i));
                }
            });
        });
    }

    fn mobile_strip(&mut self, ui: &mut egui::Ui, insert: Option<usize>) {
        Frame::none().fill(PANEL2()).rounding(card()).stroke(Stroke::new(1.0, if insert.is_none() { TEAL() } else { border() })).inner_margin(8.0).show(ui, |ui| {
            ui.set_width(70.0);
            ui.vertical_centered(|ui| {
                ui.spacing_mut().slider_width = (ui.ctx().screen_rect().height() * 0.4).clamp(140.0, 320.0);
                match insert {
                    None => {
                        ui.label(RichText::new("MASTER").size(11.0).strong().color(TEAL_HI()));
                        ui.add(egui::Slider::new(&mut self.project.master_volume, 0.0..=1.2).vertical().show_value(false));
                        ui.label(RichText::new(format!("{:+.1}", db(self.project.master_volume))).monospace().size(11.0).color(DIM()));
                    }
                    Some(i) => {
                        let ch = &mut self.project.channels[NUM_CH + i];
                        ui.label(RichText::new(format!("{}", i + 1)).size(14.0).strong().color(TEAL_HI()));
                        ui.add(egui::TextEdit::singleline(&mut ch.name).desired_width(54.0).font(egui::FontId::proportional(10.0)));
                        ui.add(egui::Slider::new(&mut ch.volume, 0.0..=1.5).vertical().show_value(false));
                        ui.label(RichText::new(format!("{:+.1}", db(ch.volume))).monospace().size(11.0).color(DIM()));
                        let b = Button::new(RichText::new("M").strong().color(if ch.mute { Color32::WHITE } else { DIM() })).fill(if ch.mute { AMBER } else { PANEL3() });
                        if ui.add_sized([46.0, 38.0], b).clicked() {
                            ch.mute = !ch.mute;
                        }
                    }
                }
            });
        });
    }
}
