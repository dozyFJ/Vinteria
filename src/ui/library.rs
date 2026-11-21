//! Sample library: scans folders for WAV files, previews them and assigns them to drum lanes.
use super::theme::*;
use crate::audio_engine::device::Shared;
use crate::audio_engine::Cmd;
use super::widgets::lerp_color;
use crate::project::{load_wav, Project, SampleData, SampleRef};
use crate::sequencer::{DRUM_NAMES, NUM_DRUMS};
#[cfg(target_os = "android")]
use crate::rfd;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Folder tree of the sample directories.
pub enum Node {
    Dir(String, PathBuf, Vec<Node>),
    File(PathBuf),
}

fn scan_dir(path: &Path, depth: usize, budget: &mut usize) -> Vec<Node> {
    let mut dirs = vec![];
    let mut files = vec![];
    if let Ok(rd) = std::fs::read_dir(path) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() && depth < 5 {
                let kids = scan_dir(&p, depth + 1, budget);
                if !kids.is_empty() {
                    dirs.push(Node::Dir(e.file_name().to_string_lossy().to_string(), p, kids));
                }
            } else if *budget > 0 && p.extension().map(|x| x.eq_ignore_ascii_case("wav")).unwrap_or(false) {
                *budget -= 1;
                files.push(Node::File(p));
            }
        }
    }
    let key = |n: &Node| match n {
        Node::Dir(name, ..) => name.to_lowercase(),
        Node::File(p) => p.file_name().map(|f| f.to_string_lossy().to_lowercase()).unwrap_or_default(),
    };
    dirs.sort_by_key(key);
    files.sort_by_key(key);
    dirs.extend(files);
    dirs
}

pub struct Library {
    pub tree: Vec<Node>,
    pub search: String,
    pub dirs: Vec<PathBuf>,
    pub files: Vec<PathBuf>,
    pub selected: Option<PathBuf>,
    /// loaded samples are kept alive for the whole session so the audio thread never frees sample memory
    cache: HashMap<PathBuf, Arc<SampleData>>,
    pub message: String,
    pub favorites: Vec<PathBuf>,
    pub recent: Vec<PathBuf>,
    pub tags: HashMap<PathBuf, String>,
    /// play a sound when it is clicked in the Browser
    pub auto_preview: bool,
    pub fav_only: bool,
    pub preview_vol: f32,
    bits: HashMap<PathBuf, u16>,
}

fn read_list(file: &str) -> Vec<PathBuf> {
    std::fs::read_to_string(file).map(|t| t.lines().map(str::trim).filter(|l| !l.is_empty()).map(PathBuf::from).filter(|p| p.exists()).collect()).unwrap_or_default()
}

impl Library {
    pub fn new() -> Self {
        let dir = PathBuf::from("samples");
        if !dir.exists() {
            let _ = std::fs::create_dir_all(&dir);
            write_factory_samples(&dir);
        }
        let mut dirs = vec![dir];
        // folders added with "+ Folder" in earlier sessions
        if let Ok(t) = std::fs::read_to_string("library_dirs.txt") {
            for line in t.lines().map(str::trim).filter(|l| !l.is_empty()) {
                let p = PathBuf::from(line);
                if p.is_dir() && !dirs.contains(&p) {
                    dirs.push(p);
                }
            }
        }
        let tags = std::fs::read_to_string("library_tags.txt").map(|t| t.lines().filter_map(|l| l.split_once('\t')).map(|(p, t)| (PathBuf::from(p), t.to_string())).collect()).unwrap_or_default();
        let mut l = Library { tree: vec![], search: String::new(), dirs, files: vec![], selected: None, cache: HashMap::new(), message: String::new(), favorites: read_list("library_favorites.txt"), recent: read_list("library_recent.txt"), tags, auto_preview: true, fav_only: false, preview_vol: 1.0, bits: HashMap::new() };
        l.rescan();
        l
    }

    /// Default drum kit: the 808 Mafia pack shipped in samples/808 Mafia (Kick, Snare, Clap, Hi-Hat, Perc = open hat).
    /// Only fills lanes that have no sample yet; falls back to the built-in sounds when a file is missing.
    /// Give the drum lanes the sounds a template asks for (files relative to the samples folder, one per lane). A lane whose
    /// file is missing keeps the sound it has.
    pub fn apply_kit(&mut self, project: &mut Project, kit: &[&str]) {
        for (lane, file) in kit.iter().enumerate().take(NUM_DRUMS) {
            let path = PathBuf::from("samples").join(file);
            if path.exists() {
                self.assign(project, lane, &path);
            }
        }
    }

    pub fn apply_default_kit(&mut self, project: &mut Project) {
        const KIT: [&str; NUM_DRUMS] = [
            "Kicks/TM88 Kick.wav",
            "Snares/Metro Boomin Snare (1).wav",
            "Claps/Metro Boomin' Clap (1).wav",
            "Hi Hats/Metro Boomin' Hi Hat.wav",
            "Open Hats/Classic Metro Open Hat.wav",
        ];
        for (lane, file) in KIT.iter().enumerate() {
            if project.drum_samples[lane].is_none() {
                let path = PathBuf::from("samples").join("808 Mafia").join(file);
                if path.exists() {
                    self.assign(project, lane, &path);
                }
            }
        }
        if project.sampler.sample.is_none() {
            let path = PathBuf::from("samples").join("808 Mafia").join("808s").join("Metro Boomin' 808.wav");
            if path.exists() {
                self.assign_sampler(project, &path);
            }
        }
        self.message.clear();
    }

    /// Remember the extra folders for the next session.
    pub fn save_dirs(&self) {
        let extra: Vec<String> = self.dirs.iter().skip(1).map(|d| d.to_string_lossy().to_string()).collect();
        let _ = std::fs::write("library_dirs.txt", extra.join("\n"));
    }

    pub fn rescan(&mut self) {
        let mut budget = 4000;
        self.tree = self.dirs.iter().map(|d| Node::Dir(d.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_else(|| d.to_string_lossy().to_string()), d.clone(), scan_dir(d, 0, &mut budget))).collect();
        // the flat list (used by the search) holds every sound of the tree, sub-folders included
        fn flatten(nodes: &[Node], out: &mut Vec<PathBuf>) {
            for n in nodes {
                match n {
                    Node::Dir(_, _, kids) => flatten(kids, out),
                    Node::File(p) => out.push(p.clone()),
                }
            }
        }
        let mut files = vec![];
        flatten(&self.tree, &mut files);
        files.sort();
        self.files = files;
    }

    pub fn get(&mut self, path: &Path) -> Result<Arc<SampleData>, String> {
        if let Some(d) = self.cache.get(path) {
            return Ok(Arc::clone(d));
        }
        let d = Arc::new(load_wav(path)?);
        self.cache.insert(path.to_path_buf(), Arc::clone(&d));
        Ok(d)
    }

    pub fn assign(&mut self, project: &mut Project, lane: usize, path: &Path) {
        match self.get(path) {
            Ok(data) => {
                let name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                project.drum_samples[lane] = Some(SampleRef { name: name.clone(), path: path.to_string_lossy().to_string(), data: Some(data) });
                project.drum_fx.resize(NUM_DRUMS, crate::project::SampleFx::default());
                project.drum_fx[lane] = crate::project::SampleFx::default();
                self.message = format!("{name} -> {}", DRUM_NAMES[lane]);
            }
            Err(e) => self.message = format!("Load failed: {e}"),
        }
    }

    pub fn assign_sampler(&mut self, project: &mut Project, path: &Path) {
        match self.get(path) {
            Ok(data) => {
                let name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                project.sampler.sample = Some(SampleRef { name: name.clone(), path: path.to_string_lossy().to_string(), data: Some(data) });
                self.message = format!("{name} -> 808 Sampler");
            }
            Err(e) => self.message = format!("Load failed: {e}"),
        }
    }

    /// A WAV dropped on a MIDI track: the track plays that sound on every key (the 808 Sampler, with its own sample).
    pub fn assign_track_sampler(&mut self, project: &mut Project, lane: usize, path: &Path) {
        match self.get(path) {
            Ok(data) => {
                if lane >= project.arrangement.tracks.len() {
                    return;
                }
                let name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                let t = &mut project.arrangement.tracks[lane];
                t.sampler.sample = Some(SampleRef { name: name.clone(), path: path.to_string_lossy().to_string(), data: Some(data) });
                t.instrument = Some(crate::piano_roll::Track::Sampler);
                t.blank = false;
                self.message = format!("{name} -> {}", t.name);
            }
            Err(e) => self.message = format!("Load failed: {e}"),
        }
    }

    pub fn import_dialog(&mut self, project: &mut Project, lane: usize) {
        if let Some(path) = rfd::FileDialog::new().add_filter("WAV audio", &["wav"]).set_title("Import WAV sample").pick_file() {
            if let Some(dir) = path.parent() {
                if !self.dirs.iter().any(|d| d == dir) {
                    self.dirs.push(dir.to_path_buf());
                    self.rescan();
                }
            }
            self.assign(project, lane, &path);
        }
    }

    /// Browser section: import, folders, list with preview / assign / drag onto an audio track.
    pub fn show(&mut self, ui: &mut egui::Ui, project: &mut Project, lane: &mut usize, shared: &Shared) {
        ui.horizontal(|ui| {
            if ui.button("Import WAV...").on_hover_text("Put a WAV file on the Playlist as an audio clip").clicked() {
                if let Some(path) = rfd::FileDialog::new().add_filter("WAV audio", &["wav"]).pick_file() {
                    self.send_to_playlist(project, &path, false);
                }
            }
            if ui.button("+ Folder").clicked() {
                if let Some(d) = rfd::FileDialog::new().pick_folder() {
                    if !self.dirs.contains(&d) {
                        self.dirs.push(d);
                        self.save_dirs();
                    }
                    self.rescan();
                }
            }
            if ui.button("Rescan").on_hover_text("Rescan folders").clicked() {
                self.rescan();
            }
        });
        if !self.message.is_empty() {
            ui.label(egui::RichText::new(&self.message).color(DIM()).size(11.5));
        }
        ui.label(egui::RichText::new("click: hear it - right-click: send to the Playlist - drag it onto a track").color(DIM()).size(10.5));
        ui.add_space(4.0);
        let q = self.search.trim().to_lowercase();
        if q.is_empty() {
            let tree = std::mem::take(&mut self.tree);
            self.show_nodes(ui, &tree, 0, project, lane, shared);
            self.tree = tree;
        } else {
            let files: Vec<PathBuf> = self.files.iter().filter(|f| f.file_name().map(|n| n.to_string_lossy().to_lowercase().contains(&q)).unwrap_or(false) || self.tags.get(*f).map_or(false, |t| t.to_lowercase().contains(&q))).filter(|f| !self.fav_only || self.favorites.contains(f)).take(300).cloned().collect();
            for f in files {
                self.file_row(ui, &f, project, lane, shared);
            }
        }
    }

    fn show_nodes(&mut self, ui: &mut egui::Ui, nodes: &[Node], depth: usize, project: &mut Project, lane: &mut usize, shared: &Shared) {
        for n in nodes {
            match n {
                Node::Dir(name, path, kids) => {
                    let id = egui::Id::new(("dir_open", path));
                    let mut open = ui.data_mut(|d| *d.get_persisted_mut_or_insert_with(id, || depth == 0));
                    let (rect, r) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 28.0), egui::Sense::click());
                    let p = ui.painter();
                    if r.hovered() {
                        p.rect_filled(rect, 0.0, PANEL3());
                    }
                    // folder glyph
                    let f = egui::Rect::from_min_size(rect.left_center() + egui::vec2(8.0, -6.0), egui::vec2(16.0, 12.0));
                    p.rect_filled(egui::Rect::from_min_size(f.min - egui::vec2(0.0, 2.0), egui::vec2(7.0, 3.0)), 1.0, SKY);
                    p.rect_filled(f, 2.0, SKY);
                    p.text(rect.left_center() + egui::vec2(32.0, 0.0), egui::Align2::LEFT_CENTER, name, egui::FontId::proportional(13.0), TEXT());
                    // disclosure chevron
                    let cv = rect.right_center() - egui::vec2(14.0, 0.0);
                    let chevron = if open {
                        vec![cv + egui::vec2(-4.0, -2.0), cv + egui::vec2(0.0, 2.0), cv + egui::vec2(4.0, -2.0)]
                    } else {
                        vec![cv + egui::vec2(-2.0, -4.0), cv + egui::vec2(2.0, 0.0), cv + egui::vec2(-2.0, 4.0)]
                    };
                    p.add(egui::Shape::line(chevron, egui::Stroke::new(1.5, DIM())));
                    if r.clicked() {
                        open = !open;
                        ui.data_mut(|d| d.insert_persisted(id, open));
                    }
                    if open {
                        ui.horizontal(|ui| {
                            ui.add_space(14.0);
                            ui.vertical(|ui| self.show_nodes(ui, kids, depth + 1, project, lane, shared));
                        });
                    }
                    ui.add_space(2.0);
                }
                Node::File(f) => self.file_row(ui, f, project, lane, shared),
            }
        }
    }

    fn file_row(&mut self, ui: &mut egui::Ui, f: &PathBuf, project: &mut Project, lane: &mut usize, shared: &Shared) {
        let name = f.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let sel = self.selected.as_ref() == Some(f);
        let (rect, r) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 22.0), egui::Sense::click_and_drag());
        if sel {
            ui.painter().rect_filled(rect, 0.0, TEAL().linear_multiply(if is_dark() { 0.28 } else { 0.18 }));
            ui.painter().rect_stroke(rect, 0.0, egui::Stroke::new(border_w(), TEAL()));
        } else if r.hovered() {
            ui.painter().rect_filled(rect, 0.0, PANEL3());
        }
        // small "sample" glyph + name
        let g = egui::Rect::from_min_size(rect.left_center() + egui::vec2(6.0, -5.0), egui::vec2(10.0, 10.0));
        ui.painter().rect_filled(g, small() * 0.5, lane_col(6));
        ui.painter().text(g.center(), egui::Align2::CENTER_CENTER, ">", egui::FontId::monospace(8.0), egui::Color32::WHITE);
        ui.painter().text(rect.left_center() + egui::vec2(22.0, 0.0), egui::Align2::LEFT_CENTER, &name, egui::FontId::proportional(13.0), TEXT());
        r.dnd_set_drag_payload(super::browser::DragItem::Sample(f.clone()));
        let r = r.on_hover_text(f.to_string_lossy());
        if self.favorites.contains(f) {
            ui.painter().circle_filled(rect.right_center() - egui::vec2(10.0, 0.0), 3.5, TEAL_HI());
        }
        if r.clicked() {
            self.selected = Some(f.clone());
            self.touch_recent(f);
            match self.get(f) {
                Ok(d) => {
                    if self.auto_preview {
                        shared.send(Cmd::Preview(d));
                    }
                }
                Err(e) => self.message = e,
            }
        }
        // (a double click used to swap the selected drum lane's own sound for this one: a kick turned into a snare. Only the right-click menu does that now.)
        r.context_menu(|ui| {
            if ui.button("Send to selected drum lane").clicked() {
                self.assign(project, *lane, f);
                ui.close_menu();
            }
            if ui.button("Use as 808 Sampler sound").clicked() {
                self.assign_sampler(project, f);
                ui.close_menu();
            }
            ui.separator();
            if ui.button("Send to playlist as an audio clip").clicked() {
                self.send_to_playlist(project, f, false);
                ui.close_menu();
            }
            if ui.button("Send to playlist as an audio track").clicked() {
                self.send_to_playlist(project, f, true);
                ui.close_menu();
            }
            ui.separator();
            if ui.button("Open parent folder").clicked() {
                if let Some(dir) = f.parent() {
                    let _ = std::process::Command::new("explorer").arg(dir).spawn();
                }
                ui.close_menu();
            }
            if ui.button("Locate file").clicked() {
                let _ = std::process::Command::new("explorer").arg(format!("/select,{}", f.display())).spawn();
                ui.close_menu();
            }
        });
    }

    // ---------------------------------------------------------------- Browser sections

    fn persist_lists(&self) {
        let join = |v: &[PathBuf]| v.iter().map(|p| p.to_string_lossy().to_string()).collect::<Vec<_>>().join("\n");
        let _ = std::fs::write("library_favorites.txt", join(&self.favorites));
        let _ = std::fs::write("library_recent.txt", join(&self.recent));
        let tags: Vec<String> = self.tags.iter().map(|(p, t)| format!("{}\t{}", p.to_string_lossy(), t)).collect();
        let _ = std::fs::write("library_tags.txt", tags.join("\n"));
    }

    pub fn is_fav(&self, p: &Path) -> bool {
        self.favorites.iter().any(|f| f == p)
    }

    pub fn toggle_fav(&mut self, p: &Path) {
        if let Some(i) = self.favorites.iter().position(|f| f == p) {
            self.favorites.remove(i);
        } else {
            self.favorites.push(p.to_path_buf());
        }
        self.persist_lists();
    }

    pub fn favorite_files(&self) -> Vec<PathBuf> {
        self.favorites.clone()
    }

    pub fn touch_recent(&mut self, p: &Path) {
        self.recent.retain(|r| r != p);
        self.recent.insert(0, p.to_path_buf());
        self.recent.truncate(30);
        self.persist_lists();
    }

    pub fn set_tag(&mut self, p: &Path, tag: String) {
        if tag.trim().is_empty() {
            self.tags.remove(p);
        } else {
            self.tags.insert(p.to_path_buf(), tag.replace('\t', " ").replace('\n', " "));
        }
        self.persist_lists();
    }

    /// "44.1 kHz   24 bit   0:00.412" of a sound
    pub fn format_info(&mut self, p: &PathBuf) -> String {
        let Ok(d) = self.get(p) else { return String::new() };
        let bits = *self.bits.entry(p.clone()).or_insert_with(|| hound::WavReader::open(p).map(|r| r.spec().bits_per_sample).unwrap_or(16));
        let secs = d.mono.len() as f32 / d.rate.max(1) as f32;
        format!("{:.1} kHz   {} bit   {}:{:06.3}", d.rate as f32 / 1000.0, bits, secs as u32 / 60, secs % 60.0)
    }

    pub fn send_to_playlist_pub(&mut self, project: &mut Project, f: &Path, new_track: bool) {
        self.send_to_playlist(project, f, new_track);
    }

    /// a plain list of sounds (Favorites / Recent)
    pub fn show_files(&mut self, ui: &mut egui::Ui, files: &[PathBuf], project: &mut Project, lane: &mut usize, shared: &Shared) {
        for f in files {
            self.file_row(ui, f, project, lane, shared);
        }
    }

    /// The three browser actions as menu items (shown from the three-dots button of the Browser header).
    pub fn actions_menu(&mut self, ui: &mut egui::Ui, project: &mut Project) {
        if ui.button("Import WAV...").on_hover_text("Put a WAV file on the Playlist as an audio clip").clicked() {
            if let Some(path) = rfd::FileDialog::new().add_filter("WAV audio", &["wav"]).pick_file() {
                self.send_to_playlist(project, &path, false);
            }
        }
        if ui.button("+ Folder").clicked() {
            if let Some(d) = rfd::FileDialog::new().pick_folder() {
                if !self.dirs.contains(&d) {
                    self.dirs.push(d);
                    self.save_dirs();
                }
                self.rescan();
            }
        }
        if ui.button("Rescan").on_hover_text("Rescan folders").clicked() {
            self.rescan();
        }
    }

    /// Samples: the factory library folders as a tree
    pub fn show_samples(&mut self, ui: &mut egui::Ui, project: &mut Project, lane: &mut usize, shared: &Shared) {
        if !self.message.is_empty() {
            ui.label(egui::RichText::new(&self.message).color(DIM()).size(11.5));
        }
        let tree = std::mem::take(&mut self.tree);
        if let Some(Node::Dir(_, _, kids)) = tree.first() {
            self.show_nodes(ui, kids, 1, project, lane, shared);
        }
        self.tree = tree;
    }

    /// User Library: the folders added with "+ Folder"
    pub fn show_user(&mut self, ui: &mut egui::Ui, project: &mut Project, lane: &mut usize, shared: &Shared) {
        if self.tree.len() <= 1 {
            ui.label(egui::RichText::new("Add your own sample folders with + Folder.").size(11.0).color(DIM()));
        }
        let tree = std::mem::take(&mut self.tree);
        if tree.len() > 1 {
            self.show_nodes(ui, &tree[1..], 1, project, lane, shared);
        }
        self.tree = tree;
    }

    /// Waveform of the selected sound with a moving playhead (click it to play from that point).
    pub fn preview_wave(&mut self, ui: &mut egui::Ui, shared: &Shared, h: f32) {
        let data = self.selected.clone().and_then(|f| self.get(&f).ok());
        let progress = f32::from_bits(shared.preview_pos.load(std::sync::atomic::Ordering::Relaxed));
        let playing = progress <= 1.0;
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), h), egui::Sense::click_and_drag());
        let p = ui.painter_at(rect);
        p.rect_filled(rect, corner(), INSET());
        let wave = lerp_color(TEAL(), egui::Color32::WHITE, 0.25);
        match data {
            Some(d) => {
                let n = d.mono.len();
                let w = rect.width().max(1.0) as usize;
                let mid = rect.center().y;
                for x in 0..w {
                    let a0 = x * n / w;
                    let a1 = (((x + 1) * n / w).max(a0 + 1)).min(n);
                    let stride = ((a1 - a0) / 32).max(1);
                    let mut pk = 0.0f32;
                    let mut i = a0;
                    while i < a1 {
                        pk = pk.max(d.mono[i].abs());
                        i += stride;
                    }
                    let hh = pk.min(1.0) * (rect.height() * 0.5 - 3.0);
                    let played = playing && (x as f32 / w as f32) < progress;
                    p.line_segment([egui::pos2(rect.left() + x as f32, mid - hh), egui::pos2(rect.left() + x as f32, mid + hh)], egui::Stroke::new(1.0, if played { CYAN() } else { wave }));
                }
                if playing {
                    ui.ctx().request_repaint();
                }
                if resp.clicked() || resp.dragged() {
                    if let Some(pos) = resp.interact_pointer_pos() {
                        let f = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 0.999);
                        shared.send(Cmd::PreviewFrom(d, f));
                    }
                }
            }
            None => {
                p.text(rect.center(), egui::Align2::CENTER_CENTER, "click a sound in the Browser", egui::FontId::proportional(12.0), LIGHT());
            }
        }
    }

    /// Put a sound on the Playlist: as a clip at the end of the first audio track, or on a new audio track of its own.
    fn send_to_playlist(&mut self, project: &mut Project, f: &Path, new_track: bool) {
        let lane = if new_track { None } else {
            project.arrangement.tracks.iter().position(|t| t.kind == crate::arrangement::TrackKind::Audio)
        };
        let _ = self.insert_audio_clip(project, f, lane);
    }

    pub(super) fn insert_audio_clip(&mut self, project: &mut Project, f: &Path, lane: Option<usize>) -> Result<usize, String> {
        use crate::arrangement::{Clip, TrackKind};
        let data = self.get(f)?;
        let secs = data.mono.len() as f32 / data.rate as f32;
        let name = f.file_stem().map(|x| x.to_string_lossy().to_string()).unwrap_or_default();
        let idx = project.pool_index(&name, &f.to_string_lossy(), data);
        let lane = match lane.filter(|&l| project.arrangement.tracks.get(l).is_some_and(|t| t.kind == TrackKind::Audio)) {
            Some(l) => l,
            _ => {
                project.arrangement.add_track(TrackKind::Audio);
                let l = project.arrangement.tracks.len() - 1;
                project.arrangement.tracks[l].name = name.clone();
                l
            }
        };
        let start = project.arrangement.clips.iter().filter(|c| c.lane == lane).map(|c| c.end_tick()).max().unwrap_or(0);
        let ticks = ((secs * project.bpm / 60.0 * crate::sequencer::PPQ as f32).round() as u32).max(24);
        let c = Clip::new(idx, lane, start, ticks, true);
        if !project.arrangement.can_place(&c, None) {
            return Err("Cannot place this audio clip on the track.".into());
        }
        project.arrangement.clips.push(c);
        project.song_mode = true;
        Ok(project.arrangement.clips.len() - 1)
    }

    /// The sound player: Play / Stop, the waveform with a moving playhead (click it to start from that point) and the time.
    pub fn waveform(&mut self, ui: &mut egui::Ui, shared: &Shared) {
        let data = self.selected.clone().and_then(|f| self.get(&f).ok());
        let progress = f32::from_bits(shared.preview_pos.load(std::sync::atomic::Ordering::Relaxed));
        let playing = progress <= 1.0;
        ui.horizontal(|ui| {
            if ui.add_enabled(data.is_some(), egui::Button::new(if playing { "Restart" } else { "Play" })).clicked() {
                if let Some(d) = &data {
                    shared.send(Cmd::Preview(d.clone()));
                }
            }
            if ui.add_enabled(playing, egui::Button::new("Stop")).clicked() {
                shared.send(Cmd::PreviewStop);
            }
            if ui.button("Open...").on_hover_text("Load any WAV file into the player").clicked() {
                if let Some(path) = rfd::FileDialog::new().add_filter("WAV audio", &["wav"]).pick_file() {
                    match self.get(&path) {
                        Ok(d) => {
                            self.selected = Some(path);
                            shared.send(Cmd::Preview(d));
                        }
                        Err(e) => self.message = format!("Load failed: {e}"),
                    }
                }
            }
            if let Some(d) = &data {
                let total = d.mono.len() as f32 / d.rate as f32;
                let at = if playing { progress * total } else { 0.0 };
                ui.label(egui::RichText::new(format!("{:.2} / {:.2} s", at, total)).monospace().size(11.0).color(DIM()));
            }
        });
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 72.0), egui::Sense::click_and_drag());
        let p = ui.painter_at(rect);
        p.rect_filled(rect, corner(), INSET());
        // the well is dark in both modes, so the trace is a light tint of the accent
        let wave = lerp_color(TEAL(), egui::Color32::WHITE, 0.6);
        match data {
            Some(d) => {
                let n = d.mono.len();
                let w = rect.width().max(1.0) as usize;
                let mid = rect.center().y;
                for x in 0..w {
                    let a0 = x * n / w;
                    let a1 = (((x + 1) * n / w).max(a0 + 1)).min(n);
                    let stride = ((a1 - a0) / 32).max(1);
                    let mut pk = 0.0f32;
                    let mut i = a0;
                    while i < a1 {
                        pk = pk.max(d.mono[i].abs());
                        i += stride;
                    }
                    let h = pk.min(1.0) * (rect.height() * 0.5 - 3.0);
                    let played = playing && (x as f32 / w as f32) < progress;
                    p.line_segment([egui::pos2(rect.left() + x as f32, mid - h), egui::pos2(rect.left() + x as f32, mid + h)], egui::Stroke::new(1.0, if played { wave } else { wave.linear_multiply(0.55) }));
                }
                if playing {
                    let x = rect.left() + progress * rect.width();
                    p.line_segment([egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())], egui::Stroke::new(1.5, CYAN()));
                    ui.ctx().request_repaint();
                }
                p.text(rect.right_bottom() - egui::vec2(4.0, 3.0), egui::Align2::RIGHT_BOTTOM, format!("{:.1} kHz", d.rate as f32 / 1000.0), egui::FontId::monospace(10.0), LIGHT());
                if resp.clicked() || resp.dragged() {
                    if let Some(pos) = resp.interact_pointer_pos() {
                        let f = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 0.999);
                        shared.send(Cmd::PreviewFrom(d, f));
                    }
                }
            }
            None => {
                p.text(rect.center(), egui::Align2::CENTER_CENTER, "click a sound in the Browser", egui::FontId::proportional(12.0), LIGHT());
            }
        }
    }
}

fn write_factory_samples(dir: &Path) {
    use crate::instruments::drums::render_factory;
    for (kind, name) in DRUM_NAMES.iter().enumerate() {
        let data = render_factory(kind, 44100);
        let spec = hound::WavSpec { channels: 1, sample_rate: 44100, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
        if let Ok(mut w) = hound::WavWriter::create(dir.join(format!("Factory {name}.wav")), spec) {
            for s in data {
                let _ = w.write_sample((s.clamp(-1.0, 1.0) * 32767.0) as i16);
            }
            let _ = w.finalize();
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn search_list_contains_sounds_from_sub_folders() {
        let l = Library::new();
        let samples = std::path::Path::new("samples");
        // the 808 Mafia / Phonk kits live in sub-folders: the flat list used by the search has to reach them
        assert!(l.files.iter().any(|f| f.parent().map_or(false, |d| d != samples)), "no file from a sub-folder in the search list");
    }

    use super::*;

    #[test]
    fn default_kit_loads_all_lanes() {
        let mut p = Project::default();
        let mut l = Library::new();
        l.apply_default_kit(&mut p);
        for lane in 0..NUM_DRUMS {
            let s = p.drum_samples[lane].as_ref().unwrap_or_else(|| panic!("lane {lane} has no sample"));
            assert!(s.data.as_ref().unwrap().mono.len() > 100, "{}", s.name);
        }
    }
}
