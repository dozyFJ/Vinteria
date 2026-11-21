//! Mixer UI: channel strips with fader, pan, mute/solo, delay send, meters, master + delay FX.
use super::theme::*;
use super::widgets::*;
use crate::mixer::{NUM_CH, NUM_INSERTS, NUM_STRIPS};
use crate::project::Project;
use egui::{vec2, Ui};

pub fn show(ui: &mut Ui, project: &mut Project, meters: &[[f32; 2]; NUM_STRIPS + 1], gr_db: f32) {
    section_label(ui, "MIXER");
    // like FL: the effect slots of the chosen insert and the master bus stay visible in a column on the right
    egui::SidePanel::right("mixer_side").resizable(true).default_width(330.0).min_width(260.0).max_width(520.0).frame(egui::Frame::none().fill(PANEL().linear_multiply(1.0)).inner_margin(egui::Margin::symmetric(8.0, 6.0))).show_inside(ui, |ui| {
        egui::ScrollArea::vertical().auto_shrink([false, false]).id_salt("mix_side_scroll").show(ui, |ui| {
            fx_rack(ui, project, true);
            ui.add_space(8.0);
            master_bus(ui, project, gr_db);
        });
    });
    // like FL: the Master strip on the left, then the inserts as tall strips side by side
    egui::ScrollArea::both().auto_shrink([false, false]).id_salt("mix_scroll").show(ui, |ui| {
        // the strips share the width of the page: nine equal columns, the Master first
        let sw = ((ui.available_width() - 40.0) / (NUM_INSERTS as f32 + 1.0) - 12.0).clamp(70.0, 150.0);
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            master_strip(ui, project, meters[NUM_STRIPS], sw);
            ui.add_space(10.0);
            for c in NUM_CH..NUM_STRIPS {
                strip(ui, project, c, meters[c], sw);
            }
        });
    });
}

/// The Master channel as a tall strip (fader + meter), the first one of the row.
fn master_strip(ui: &mut Ui, project: &mut Project, m: [f32; 2], w: f32) {
    egui::Frame::none().fill(PANEL2()).rounding(2.0).inner_margin(egui::Margin::symmetric(5.0, 6.0)).stroke(egui::Stroke::new(1.5, TEAL())).show(ui, |ui| {
        ui.set_width(w);
        ui.spacing_mut().slider_width = w - 16.0;
        ui.vertical_centered(|ui| {
            ui.add_sized([w, 18.0], egui::Label::new(egui::RichText::new("MASTER").strong().size(10.5).color(TEAL_HI())));
            ui.add_space(26.0);
            let fader_h = (ui.ctx().screen_rect().height() * 0.5).clamp(170.0, 560.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().slider_width = fader_h;
                ui.add(egui::Slider::new(&mut project.master_volume, 0.0..=1.2).vertical().show_value(false));
                meter(ui, vec2(14.0, fader_h), m);
            });
            ui.label(egui::RichText::new(format!("{:+.1}", db(project.master_volume))).monospace().size(10.0).color(DIM()));
        });
    });
}

/// The whole mixer as one column going down (for the right panel of the Playlist): master, the inserts as two-line cards,
/// the effects of the chosen insert and the master bus.
pub fn show_column(ui: &mut Ui, project: &mut Project, meters: &[[f32; 2]; NUM_STRIPS + 1], gr_db: f32) {
    let sel_id = egui::Id::new("mix_fx_sel");
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Mixer").strong().size(14.0));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.menu_button("Mix presets", |ui| {
                for name in ["Flat", "Phonk", "Club"] {
                    if ui.button(name).on_hover_text("Set channel levels, pan, sends and the master bus").clicked() {
                        crate::templates::apply_mix(project, name);
                        ui.close_menu();
                    }
                }
            });
        });
    });
    ui.add_space(4.0);
    // pages: the channels, the effects of the chosen insert, the master bus and delay
    let page_id = egui::Id::new("mix_page");
    let mut page = ui.data(|d| d.get_temp::<u8>(page_id)).unwrap_or(0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for (i, n) in ["Channels", "Effects", "Master"].into_iter().enumerate() {
            let (rect, resp) = ui.allocate_exact_size(vec2((ui.available_width() - 8.0) / 3.0, 26.0), egui::Sense::click());
            let on = page == i as u8;
            ui.painter().rect_filled(rect, 13.0, if on { TEAL().gamma_multiply(0.30) } else if resp.hovered() { PANEL3() } else { PANEL2() });
            ui.painter().rect_stroke(rect, 13.0, egui::Stroke::new(1.0_f32, if on { TEAL() } else { border() }));
            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, n, egui::FontId::proportional(12.0), if on { TEXT() } else { DIM() });
            if resp.clicked() { page = i as u8; }
        }
    });
    ui.data_mut(|d| d.insert_temp(page_id, page));
    ui.add_space(6.0);
    match page {
        0 => {
    let mut route_drop: Option<(usize, usize)> = None;
    // the inserts first, the Master bus under them
    for c in (NUM_CH..NUM_STRIPS).chain(std::iter::once(NUM_STRIPS)) {
        let is_master = c == NUM_STRIPS;
        let i = c.wrapping_sub(NUM_CH);
        let selected = ui.data(|d| d.get_temp::<usize>(sel_id)).unwrap_or(0) == if is_master { NUM_INSERTS } else { i };
        let tab_col = if is_master { MID() } else { lane_col(i) };
        let m = meters[c];
        let muted = !is_master && project.channels[c].mute;
        egui::Frame::none().fill(PANEL2()).rounding(card()).inner_margin(egui::Margin::symmetric(5.0, 3.0))
            .stroke(egui::Stroke::new(if selected { 1.5 } else { border_w() }, if selected { TEAL() } else { border() })).show(ui, |ui| {
                ui.set_width(ui.available_width());
                if muted {
                    ui.multiply_opacity(0.55);
                }
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    ui.spacing_mut().button_padding = egui::vec2(6.0, 2.0);
                    ui.spacing_mut().interact_size.y = 20.0;
                    let name = if is_master { "Master".to_string() } else { project.channels[c].name.clone() };
                    let (rect, resp) = ui.allocate_exact_size(vec2(58.0, 20.0), egui::Sense::click());
                    gloss(ui.painter(), rect, 8.0, tab_col.linear_multiply(if selected { 1.0 } else { 0.8 }));
                    let short: String = name.chars().take(8).collect();
                    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, short, egui::FontId::proportional(11.0), egui::Color32::WHITE);
                    if is_master && resp.clicked() {
                        ui.data_mut(|d| d.insert_temp(sel_id, NUM_INSERTS));
                    }
                    if !is_master {
                        if resp.clicked() {
                            ui.data_mut(|d| d.insert_temp(sel_id, i));
                        }
                        resp.context_menu(|ui| {
                            ui.label("Rename");
                            ui.text_edit_singleline(&mut project.channels[c].name);
                        });
                        if let Some(pl) = resp.dnd_release_payload::<super::arrangement_ui::TrackDrag>() {
                            route_drop = Some((pl.0, i));
                        }
                        resp.on_hover_text(format!("{} - click: effects below, right-click: rename, drop a track here to route it", name));
                    }
                    // fixed parts to the right of the fader: dB, meter, pan, M / S, send
                    let fixed = if is_master { 36.0 + 48.0 + 16.0 + 16.0 } else { 36.0 + 40.0 + 24.0 + 24.0 + 2.0 * 26.0 + 6.0 * 4.0 + 14.0 };
                    ui.spacing_mut().slider_width = (ui.available_width() - fixed).max(40.0);
                    let vol: &mut f32 = if is_master { &mut project.master_volume } else { &mut project.channels[c].volume };
                    ui.add(egui::Slider::new(vol, 0.0..=if is_master { 1.2 } else { 1.5 }).show_value(false));
                    ui.label(egui::RichText::new(format!("{:.1}", db(*vol))).monospace().size(9.5).color(DIM()));
                    let (mr, _) = ui.allocate_exact_size(vec2(36.0, 12.0), egui::Sense::hover());
                    ui.painter().rect_filled(mr, 3.0, INSET());
                    for (k, v) in m.iter().enumerate() {
                        let frac = ((db(*v) + 60.0) / 60.0).clamp(0.0, 1.0);
                        let col = if frac > 0.92 { RED } else if frac > 0.78 { AMBER } else { GREEN };
                        ui.painter().rect_filled(egui::Rect::from_min_size(mr.min + vec2(1.0, 1.5 + k as f32 * 4.5), vec2((mr.width() - 2.0) * frac, 3.5)), 1.5, col);
                    }
                    if !is_master {
                        let ch = &mut project.channels[c];
                        mini_knob(ui, &mut ch.pan, -1.0, 1.0, 0.0, "Pan");
                        ui.toggle_value(&mut ch.mute, "M");
                        ui.toggle_value(&mut ch.solo, "S");
                        mini_knob(ui, &mut ch.delay_send, 0.0, 1.0, 0.0, "Delay send");
                    }
                });
            });
        ui.add_space(2.0);
    }
    if let Some((track, i)) = route_drop {
        if let Some(t) = project.arrangement.tracks.get_mut(track) {
            t.output = Some(i);
        }
        ui.data_mut(|d| d.insert_temp(sel_id, i));
    }
    // the effect slots live under the Effects tab, not here
        }
        1 => {
    egui::Frame::none().fill(PANEL2()).rounding(card()).stroke(egui::Stroke::new(border_w(), border())).inner_margin(6.0).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(egui::RichText::new("FX Slots - Selected Track").strong().size(12.5));
        ui.add_space(2.0);
        fx_rack(ui, project, false);
    });
        }
        _ => {
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new("DELAY").color(DIM()).size(10.0));
                ui.add(egui::DragValue::new(&mut project.delay_beats).range(0.0625..=3.0).speed(0.01).suffix(" beats"));
                lin_knob(ui, "Feedback", &mut project.delay_feedback, 0.0, 0.9, "");
                lin_knob(ui, "Return", &mut project.delay_return, 0.0, 1.0, "");
            });
            master_bus(ui, project, gr_db);
        }
    }
}

/// One insert as a horizontal row: number, name, level meter, fader, pan, delay send, M / S.
fn strip_row(ui: &mut Ui, project: &mut Project, c: usize, m: [f32; 2]) {
    let i = c - NUM_CH;
    let sel_id = egui::Id::new("mix_fx_sel");
    let selected = ui.data(|d| d.get_temp::<usize>(sel_id)).unwrap_or(0) == i;
    egui::Frame::none().fill(PANEL2()).rounding(2.0).inner_margin(egui::Margin::symmetric(8.0, 4.0)).stroke(egui::Stroke::new(if selected { 1.5 } else { 1.0 }, if selected { TEAL() } else { PANEL3() })).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            let ch = &mut project.channels[c];
            let r = ui.add_sized([30.0, 22.0], egui::Button::new(egui::RichText::new(format!("{}", i + 1)).strong().color(if selected { egui::Color32::WHITE } else { TEAL_HI() })).fill(if selected { TEAL().linear_multiply(0.5) } else { PANEL3() }));
            if r.on_hover_text("Select this insert (its effects are shown on the right)").clicked() {
                ui.data_mut(|d| d.insert_temp(sel_id, i));
            }
            ui.add(egui::TextEdit::singleline(&mut ch.name).desired_width(72.0).font(egui::FontId::proportional(12.0)));
            // horizontal level meter
            let (rect, _) = ui.allocate_exact_size(vec2(110.0, 16.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, 2.0, INSET());
            for (k, v) in m.iter().enumerate() {
                let frac = ((db(*v) + 60.0) / 60.0).clamp(0.0, 1.0);
                let y0 = rect.top() + 2.0 + k as f32 * 6.0;
                let col = if frac > 0.92 { RED } else if frac > 0.78 { AMBER } else { GREEN };
                ui.painter().rect_filled(egui::Rect::from_min_size(egui::pos2(rect.left() + 1.0, y0), vec2((rect.width() - 2.0) * frac, 5.0)), 1.0, col);
            }
            ui.spacing_mut().slider_width = 130.0;
            ui.add(egui::Slider::new(&mut ch.volume, 0.0..=1.5).show_value(false)).on_hover_text("Volume");
            ui.label(egui::RichText::new(format!("{:+.1} dB", db(ch.volume))).monospace().size(10.0).color(DIM()));
            ui.spacing_mut().slider_width = 60.0;
            ui.add(egui::Slider::new(&mut ch.pan, -1.0..=1.0).show_value(false)).on_hover_text("Pan");
            ui.add(egui::Slider::new(&mut ch.delay_send, 0.0..=1.0).show_value(false)).on_hover_text("Delay send");
            ui.toggle_value(&mut ch.mute, "M");
            ui.toggle_value(&mut ch.solo, "S");
        });
    });
}

/// Insert effects: pick an insert, then build its chain from the built-in plugins.
fn fx_rack(ui: &mut Ui, project: &mut Project, scroll: bool) {
    super::fx_slots::show(ui, project, scroll);
}

fn strip(ui: &mut Ui, project: &mut Project, c: usize, m: [f32; 2], w: f32) {
    let insert = c >= NUM_CH;
    let col = if insert { TEAL_HI() } else { LANE_COLORS[c] };
    let sel_id = egui::Id::new("mix_fx_sel");
    let selected = insert && ui.data(|d| d.get_temp::<usize>(sel_id)).unwrap_or(0) == c - NUM_CH;
    egui::Frame::none().fill(PANEL2()).rounding(2.0).inner_margin(egui::Margin::symmetric(5.0, 6.0)).stroke(egui::Stroke::new(if selected { 1.5 } else { 1.0 }, if selected { TEAL() } else { PANEL3() })).show(ui, |ui| {
        ui.set_width(w);
        ui.spacing_mut().slider_width = w - 16.0;
        ui.vertical_centered(|ui| {
            let ch = &mut project.channels[c];
            // number / name header; an insert is picked here to edit its effect chain
            if insert {
                let n = c - NUM_CH + 1;
                let r = ui.add_sized([w, 18.0], egui::Button::new(egui::RichText::new(format!("{n}")).strong().color(if selected { egui::Color32::WHITE } else { col })).fill(if selected { TEAL().linear_multiply(0.5) } else { PANEL3() }));
                if r.on_hover_text("Select this insert (its effects are shown below)").clicked() {
                    ui.data_mut(|d| d.insert_temp(sel_id, c - NUM_CH));
                }
                ui.add(egui::TextEdit::singleline(&mut ch.name).desired_width(w - 6.0).font(egui::FontId::proportional(10.0)));
            } else {
                ui.add(egui::Label::new(egui::RichText::new(&ch.name).color(col).strong().size(11.0)).truncate());
                output_picker(ui, ("mix_out", c), &mut ch.output);
            }
            // pan on top like FL, then fader + meter side by side
            ui.add(egui::Slider::new(&mut ch.pan, -1.0..=1.0).show_value(false).trailing_fill(false)).on_hover_text("Pan");
            // taller faders on tall windows
            let fader_h = (ui.ctx().screen_rect().height() * 0.5).clamp(170.0, 560.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().slider_width = fader_h;
                ui.add(egui::Slider::new(&mut ch.volume, 0.0..=1.5).vertical().show_value(false));
                meter(ui, vec2(14.0, fader_h), m);
            });
            ui.label(egui::RichText::new(format!("{:+.1}", db(ch.volume))).monospace().size(10.0).color(DIM()));
            ui.add(egui::Slider::new(&mut ch.delay_send, 0.0..=1.0).show_value(false)).on_hover_text("Delay send");
            ui.horizontal(|ui| {
                ui.toggle_value(&mut ch.mute, "M");
                ui.toggle_value(&mut ch.solo, "S");
            });
        });
    });
}

/// Compact "Master / Insert N" routing selector shared by the mixer, channel rack and playlist tracks.
pub fn output_picker(ui: &mut Ui, id: impl std::hash::Hash, output: &mut Option<usize>) {
    output_picker_w(ui, id, output, 52.0);
}

pub fn output_picker_w(ui: &mut Ui, id: impl std::hash::Hash, output: &mut Option<usize>, width: f32) {
    let short = match output {
        None => "Master".to_string(),
        Some(i) => format!("Ins {}", *i + 1),
    };
    egui::ComboBox::from_id_salt(id).width(width).selected_text(egui::RichText::new(short).size(if width > 100.0 { 13.0 } else { 10.0 })).show_ui(ui, |ui| {
        ui.selectable_value(output, None, "Master");
        for i in 0..NUM_INSERTS {
            ui.selectable_value(output, Some(i), crate::mixer::output_label(Some(i)));
        }
    }).response.on_hover_text("Output: send this sound to the master or to a mixer insert");
}

fn master(ui: &mut Ui, project: &mut Project, m: [f32; 2]) {
    egui::Frame::none().fill(PANEL2()).rounding(0.0).inner_margin(8.0).stroke(egui::Stroke::new(1.0, PURPLE())).show(ui, |ui| {
        ui.set_width(150.0);
        ui.vertical(|ui| {
            ui.label(egui::RichText::new("MASTER").color(CYAN()).strong());
            ui.horizontal(|ui| {
                ui.add(egui::Slider::new(&mut project.master_volume, 0.0..=1.2).vertical().show_value(false));
                meter(ui, vec2(26.0, 150.0), m);
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new("DELAY").color(DIM()).size(10.0));
                    ui.add(egui::DragValue::new(&mut project.delay_beats).range(0.0625..=3.0).speed(0.01).suffix(" beats"));
                    lin_knob(ui, "Feedback", &mut project.delay_feedback, 0.0, 0.9, "");
                    lin_knob(ui, "Return", &mut project.delay_return, 0.0, 1.0, "");
                });
            });
            ui.label(egui::RichText::new(format!("{:+.1} dB", db(project.master_volume))).monospace().size(10.0).color(DIM()));
        });
    });
}

/// Mixing section: master compressor, tone control and one-click mix presets.
fn master_bus(ui: &mut Ui, project: &mut Project, gr_db: f32) {
    egui::Frame::none().fill(PANEL2()).rounding(0.0).inner_margin(8.0).stroke(egui::Stroke::new(1.0, PURPLE())).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.vertical(|ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new("MASTER BUS").color(CYAN()).strong());
                ui.toggle_value(&mut project.master_fx.comp_on, "Compressor");
                ui.label(egui::RichText::new("Mix preset:").color(DIM()).size(11.0));
                for name in ["Flat", "Phonk", "Club"] {
                    if ui.small_button(name).on_hover_text("Set channel levels, pan, sends and the master bus").clicked() {
                        crate::templates::apply_mix(project, name);
                    }
                }
            });
            ui.horizontal_wrapped(|ui| {
                let fx = &mut project.master_fx;
                lin_knob(ui, "Threshold", &mut fx.threshold_db, -40.0, 0.0, "dB");
                lin_knob(ui, "Ratio", &mut fx.ratio, 1.0, 10.0, ":1");
                lin_knob(ui, "Makeup", &mut fx.makeup_db, 0.0, 12.0, "dB");
                lin_knob(ui, "Low 110Hz", &mut fx.low_db, -12.0, 12.0, "dB");
                lin_knob(ui, "High 7.5k", &mut fx.high_db, -12.0, 12.0, "dB");
                // gain-reduction meter
                let (rect, _) = ui.allocate_exact_size(vec2(16.0, 70.0), egui::Sense::hover());
                let p = ui.painter_at(rect);
                p.rect_filled(rect, 0.0, INSET());
                let h = (gr_db / 18.0).clamp(0.0, 1.0) * rect.height();
                p.rect_filled(egui::Rect::from_min_max(rect.left_top(), egui::pos2(rect.right(), rect.top() + h)), 0.0, AMBER);
                ui.label(egui::RichText::new(format!("GR\n{gr_db:.1}")).monospace().size(10.0).color(DIM()));
            });
        });
    });
}
