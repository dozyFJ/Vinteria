//! Numbered insert slots with stable positions, a plugin picker and separate editors.
use super::{theme::*, widgets::mini_knob};
use crate::{fx::{FxKind, FxSlot, ALL}, mixer::{NUM_CH, NUM_INSERTS}, project::Project};
use egui::{RichText, Ui};

fn picker(ui: &mut Ui, kind: &mut Option<FxKind>) {
    ui.label(RichText::new("Built-in plugins").small().color(DIM()));
    for plugin in ALL {
        if ui.button(plugin.name()).clicked() { *kind = Some(plugin); ui.close_menu(); }
    }
}

/// `scroll`: give the slot list a scroll area of its own (false when the caller already scrolls, e.g. the mixer column)
pub(super) fn show(ui: &mut Ui, project: &mut Project, scroll: bool) {
    project.insert_fx.resize_with(NUM_INSERTS, Vec::new);
    let sel_id = egui::Id::new("mix_fx_sel");
    let mut selected = ui.data(|d| d.get_temp::<usize>(sel_id)).unwrap_or(0).min(NUM_INSERTS);
    let master = selected == NUM_INSERTS;
    egui::ComboBox::from_id_salt("fx_channel_picker").width(ui.available_width() - 12.0)
        .selected_text(if master { "Master".to_string() } else { format!("{} · {}", selected + 1, project.channels[NUM_CH + selected].name) })
        .show_ui(ui, |ui| {
            for i in 0..NUM_INSERTS {
                ui.selectable_value(&mut selected, i, format!("{} · {}", i + 1, project.channels[NUM_CH + i].name));
            }
            ui.selectable_value(&mut selected, NUM_INSERTS, "Master");
        });
    ui.data_mut(|d| d.insert_temp(sel_id, selected));
    ui.add_space(3.0);
    ui.label(RichText::new("Click a slot to add a plugin, right-click for more").small().color(DIM()));
    ui.add_space(3.0);
    let editor_id = egui::Id::new("insert_plugin_editor");
    let mut editor = ui.data(|d| d.get_temp::<(usize, usize)>(editor_id));
    let chain = if selected == NUM_INSERTS { &mut project.master_chain } else { &mut project.insert_fx[selected] };
    let count = chain.iter().rposition(|s| !s.empty).map_or(0, |i| i + 1) + 1; // the used slots and one free slot to add the next plugin
    let area = if scroll { egui::ScrollArea::vertical() } else { egui::ScrollArea::neither() };
    area.id_salt(("insert_slots", selected)).auto_shrink([false, true]).show(ui, |ui| {
        for k in 0..count {
            ui.push_id((selected, k), |ui| {
                let occupied = chain.get(k).map_or(false, |s| !s.empty);
                let mut picked = None;
                let mut clear = false;
                let mut move_to = None;
                egui::Frame::none().fill(PANEL3().linear_multiply(0.6)).rounding(7.0).inner_margin(egui::Margin::symmetric(6.0, 3.0))
                    .stroke(egui::Stroke::new(1.0_f32, border())).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("{:02}", k + 1)).monospace().color(DIM()));
                            let label = if occupied { chain[k].kind.name().to_string() } else { format!("Slot {}", k + 1) };
                            let width = (ui.available_width() - 50.0).max(60.0);
                            let response = if occupied {
                                let r = ui.add_sized([width, 22.0], egui::Button::new(RichText::new(label).color(TEXT())).frame(false));
                                if r.clicked() { editor = Some((selected, k)); }
                                r
                            } else {
                                ui.allocate_ui_with_layout(egui::vec2(width, 22.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                    ui.menu_button(RichText::new(label).italics().color(DIM()), |ui| picker(ui, &mut picked)).response
                                }).inner
                            };
                            response.context_menu(|ui| {
                                if occupied {
                                    if ui.button("Open plugin").clicked() { editor = Some((selected, k)); ui.close_menu(); }
                                    ui.menu_button("Replace plugin", |ui| picker(ui, &mut picked));
                                    if ui.button("Clear slot").clicked() { clear = true; ui.close_menu(); }
                                    if ui.add_enabled(k > 0, egui::Button::new("Move up")).clicked() { move_to = Some(k - 1); ui.close_menu(); }
                                    if ui.add_enabled(k + 1 < count, egui::Button::new("Move down")).clicked() { move_to = Some(k + 1); ui.close_menu(); }
                                } else { picker(ui, &mut picked); }
                            });
                            if occupied {
                                mini_knob(ui, &mut chain[k].wet, 0.0, 1.0, 1.0, "Slot dry / wet");
                                super::widgets::led_tip(ui, &mut chain[k].on, "Enable / bypass plugin");
                            } else {
                                ui.label(RichText::new("○").color(MID()));
                            }
                        });
                    });
                if let Some(kind) = picked {
                    chain.resize_with(chain.len().max(k + 1), FxSlot::empty);
                    chain[k] = FxSlot::new(kind);
                    editor = Some((selected, k));
                }
                if clear { chain[k] = FxSlot::empty(); if editor == Some((selected, k)) { editor = None; } }
                if let Some(to) = move_to {
                    chain.resize_with(chain.len().max(to + 1), FxSlot::empty);
                    chain.swap(k, to);
                    if editor == Some((selected, k)) { editor = Some((selected, to)); }
                    else if editor == Some((selected, to)) { editor = Some((selected, k)); }
                }
                ui.add_space(2.0);
            });
        }
    });
    if let Some((channel, index)) = editor {
        let cname = if channel == NUM_INSERTS { "Master".to_string() } else { project.channels[NUM_CH + channel].name.clone() };
        let chain = if channel == NUM_INSERTS { Some(&mut project.master_chain) } else { project.insert_fx.get_mut(channel) };
        if let Some(slot) = chain.and_then(|chain| chain.get_mut(index)).filter(|s| !s.empty) {
            let mut open = true;
            egui::Window::new(format!("{} · Slot {} · {}", cname, index + 1, slot.kind.name()))
                .id(editor_id).open(&mut open).default_width(420.0).show(ui.ctx(), |ui| {
                    super::plugin_ui::plugin_grid(ui, slot);
                });
            if !open { editor = None; }
        } else { editor = None; }
    }
    ui.data_mut(|d| { if let Some(editor) = editor { d.insert_temp(editor_id, editor); } else { d.remove::<(usize, usize)>(editor_id); } });
}
