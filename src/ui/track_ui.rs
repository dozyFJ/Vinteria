//! Detail panel pieces for the selected Playlist / Session track.
//! MIDI track: a one-line strip to pick the sound (synth) - or drop one from the Browser - above the piano roll.
//! Audio track: a sample view; audio tracks only ever hold WAV sounds.
use super::browser::DragItem;
use super::library::Library;
use super::theme::*;
use super::widgets::section_label;
use crate::arrangement::TrackKind;
use crate::audio_engine::device::Shared;
use crate::audio_engine::Cmd;
use crate::mixer::CH_AUDIO;
use crate::piano_roll::Track;
use crate::project::Project;
use crate::sequencer::TICKS_PER_BAR;
use egui::{pos2, vec2, Align2, FontId, Sense, Stroke, Ui};

/// One-line track strip. Returns the synth when the user (re)assigned the track's sound.
pub fn header(ui: &mut Ui, project: &mut Project, lane: usize) -> Option<Track> {
    let mut chosen = None;
    let kind = project.arrangement.tracks[lane].kind;
    let strip = ui.horizontal(|ui| {
        section_label(ui, "TRACK");
        ui.add(egui::TextEdit::singleline(&mut project.arrangement.tracks[lane].name).desired_width(120.0));
        ui.label(egui::RichText::new(if kind == TrackKind::Audio { "AUDIO" } else { "MIDI" }).strong().color(TEAL_HI()));
        ui.toggle_value(&mut project.arrangement.tracks[lane].mute, "Mute");
        ui.label(egui::RichText::new("TRANSPOSE").color(DIM()).size(11.0));
        ui.add(egui::DragValue::new(&mut project.arrangement.tracks[lane].transpose).range(-24..=24).suffix(" st")).on_hover_text("Semitones: shifts MIDI notes; on audio tracks it changes the sound's pitch (and speed)");
        ui.label(egui::RichText::new("OUT").color(DIM()).size(11.0));
        super::mixer_ui::output_picker(ui, ("trk_out", lane), &mut project.arrangement.tracks[lane].output);
        let clips = project.arrangement.clips.iter().filter(|c| c.lane == lane).count();
        ui.label(egui::RichText::new(format!("{clips} clip(s)")).color(DIM()).size(11.0));
        let _ = &mut chosen;
    });
    if kind == TrackKind::Midi {
        let r = strip.response;
        if let Some(pl) = r.dnd_hover_payload::<DragItem>() {
            let ok = matches!(&*pl, DragItem::Instrument(_));
            ui.painter().rect_stroke(r.rect.expand(2.0), corner(), Stroke::new(2.0, if ok { TEAL_HI() } else { RED }));
        }
        if let Some(pl) = r.dnd_release_payload::<DragItem>() {
            if let DragItem::Instrument(t) = &*pl {
                project.arrangement.tracks[lane].instrument = Some(*t);
                chosen = Some(*t);
            }
        }
    }
    chosen
}

fn strip_sliders(ui: &mut Ui, project: &mut Project, ch: usize, title: &str) {
    let Some(c) = project.channels.get_mut(ch) else { return };
    ui.label(egui::RichText::new(title).color(DIM()).size(11.0));
    egui::Grid::new(("strip", ch)).num_columns(2).spacing([10.0, 4.0]).show(ui, |ui| {
        ui.label("Volume");
        ui.add(egui::Slider::new(&mut c.volume, 0.0..=1.0).show_value(false));
        ui.end_row();
        ui.label("Pan");
        ui.add(egui::Slider::new(&mut c.pan, -1.0..=1.0).show_value(false));
        ui.end_row();
        ui.label("Delay send");
        ui.add(egui::Slider::new(&mut c.delay_send, 0.0..=1.0).show_value(false));
        ui.end_row();
    });
}

/// Audio-track view: waveform of the selected clip's sound, info and the audio channel strip.
pub fn sample_panel(ui: &mut Ui, project: &mut Project, lane: usize, sel_sample: Option<usize>, _lib: &mut Library, shared: &Shared) {
    // the sound of the clicked clip, otherwise the first clip on this track
    let idx = sel_sample
        .filter(|i| project.arrangement.clips.iter().any(|c| c.audio && c.lane == lane && c.pattern == *i))
        .or_else(|| project.arrangement.clips.iter().find(|c| c.audio && c.lane == lane).map(|c| c.pattern));
    let Some(sample) = idx.and_then(|i| project.audio_pool.get(i).cloned()) else {
        ui.add_space(10.0);
        ui.label(egui::RichText::new("This audio track is empty.").color(DIM()));
        ui.label(egui::RichText::new("Drag a WAV sound from the Browser onto the track in the Playlist.").color(DIM()));
        return;
    };
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width((ui.available_width() - 300.0).max(300.0));
            ui.label(egui::RichText::new(&sample.name).size(18.0).strong().color(TEXT()));
            let Some(d) = sample.data.clone() else {
                ui.label(egui::RichText::new("sound data is not loaded").color(RED));
                return;
            };
            let secs = d.mono.len() as f32 / d.rate as f32;
            let bars = secs * project.bpm / 60.0 / 4.0;
            ui.label(egui::RichText::new(format!("{:.1} kHz  -  {secs:.2} s  -  {bars:.2} bars at {:.0} BPM  -  {}", d.rate as f32 / 1000.0, project.bpm, sample.path)).color(DIM()).size(11.0));
            let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), (ui.available_height() - 36.0).clamp(80.0, 180.0)), Sense::click());
            let p = ui.painter_at(rect);
            p.rect_filled(rect, corner(), INSET());
            let n = d.mono.len();
            let w = rect.width().max(1.0) as usize;
            let mid = rect.center().y;
            for x in 0..w {
                let a0 = x * n / w;
                let a1 = ((x + 1) * n / w).max(a0 + 1).min(n);
                let stride = ((a1 - a0) / 32).max(1);
                let (mut pk, mut i) = (0.0f32, a0);
                while i < a1 {
                    pk = pk.max(d.mono[i].abs());
                    i += stride;
                }
                let h = pk.min(1.0) * (rect.height() * 0.5 - 3.0);
                p.line_segment([pos2(rect.left() + x as f32, mid - h), pos2(rect.left() + x as f32, mid + h)], Stroke::new(1.0, TEAL()));
            }
            // bar lines at the project tempo
            let bar_px = 60.0 / project.bpm * 4.0 / secs * rect.width();
            let mut x = bar_px;
            while x < rect.width() && bar_px > 6.0 {
                p.line_segment([pos2(rect.left() + x, rect.top()), pos2(rect.left() + x, rect.bottom())], Stroke::new(1.0, LIGHT().linear_multiply(0.35)));
                x += bar_px;
            }
            p.text(rect.left_top() + vec2(8.0, 6.0), Align2::LEFT_TOP, "click to preview", FontId::proportional(10.0), LIGHT());
            if resp.clicked() {
                shared.send(Cmd::Preview(d));
            }
            let _ = TICKS_PER_BAR;
        });
        ui.add_space(20.0);
        ui.vertical(|ui| strip_sliders(ui, project, CH_AUDIO, "Audio channel (shared by all audio tracks)"));
    });
}

/// Track strip stacked vertically (for the narrow column under the Playlist's track list).
pub fn header_vertical(ui: &mut Ui, project: &mut Project, lane: usize) {
    let kind = project.arrangement.tracks[lane].kind;
    ui.add(egui::TextEdit::singleline(&mut project.arrangement.tracks[lane].name).desired_width(f32::INFINITY));
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(if kind == TrackKind::Audio { "AUDIO" } else { "MIDI" }).strong().color(TEAL_HI()));
        ui.toggle_value(&mut project.arrangement.tracks[lane].mute, "Mute");
    });
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("TRANSPOSE").color(DIM()).size(11.0));
        ui.add(egui::DragValue::new(&mut project.arrangement.tracks[lane].transpose).range(-24..=24).suffix(" st"));
    });
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("OUT").color(DIM()).size(11.0));
        super::mixer_ui::output_picker(ui, ("trk_out_v", lane), &mut project.arrangement.tracks[lane].output);
    });
    ui.add_space(4.0);
    let clips = project.arrangement.clips.iter().filter(|c| c.lane == lane).count();
    ui.label(egui::RichText::new(format!("{clips} clip(s)")).color(DIM()).size(11.0));
}

/// The Piano Roll page for an audio track: its clips with their waveforms (cutting and moving happens in the Playlist).
pub fn audio_clip_view(ui: &mut egui::Ui, project: &crate::project::Project, lane: usize) {
    let Some(track) = project.arrangement.tracks.get(lane) else { return };
    ui.label(egui::RichText::new(format!("AUDIO TRACK - {}", track.name)).strong().size(15.0));
    ui.label(egui::RichText::new("The audio clips of this track. Cut, move and stretch them in the Playlist.").color(DIM()));
    ui.add_space(6.0);
    let clips: Vec<_> = project.arrangement.clips.iter().filter(|c| c.lane == lane && c.audio).copied().collect();
    if clips.is_empty() {
        ui.label(egui::RichText::new("No clips on this track yet - drag a sound onto it in the Playlist.").color(DIM()));
        return;
    }
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        for c in clips {
            let Some(sample) = project.audio_pool.get(c.pattern) else { continue };
            ui.label(egui::RichText::new(format!("{} - bar {}, {} bars", sample.name, c.start_bar() + 1, (c.len + crate::sequencer::TICKS_PER_BAR - 1) / crate::sequencer::TICKS_PER_BAR)).strong());
            let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 100.0), egui::Sense::hover());
            let p = ui.painter_at(rect);
            p.rect_filled(rect, corner(), INSET());
            if let Some(d) = &sample.data {
                let n = d.mono.len().max(1);
                let w = rect.width().max(1.0) as usize;
                let mid = rect.center().y;
                let col = TEAL_HI();
                for x in 0..w {
                    let a0 = x * n / w;
                    let a1 = (((x + 1) * n / w).max(a0 + 1)).min(n);
                    let stride = ((a1 - a0) / 32).max(1);
                    let (mut pk, mut i) = (0.0f32, a0);
                    while i < a1 {
                        pk = pk.max(d.mono[i].abs());
                        i += stride;
                    }
                    let h = pk.min(1.0) * (rect.height() * 0.5 - 4.0);
                    p.line_segment([egui::pos2(rect.left() + x as f32, mid - h), egui::pos2(rect.left() + x as f32, mid + h)], egui::Stroke::new(1.0, col));
                }
            }
            ui.add_space(8.0);
        }
    });
}
