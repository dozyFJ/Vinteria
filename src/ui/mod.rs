//! UI: main window, transport, tabs, dialogs. Talks to the engine only through `Shared` (atomics + channels).
mod aos_themes;
pub mod arrangement_ui;
mod devices_ui;
pub mod browser;
pub mod browser_panel;
pub mod instruments_ui;
pub mod library;
pub mod mixer_ui;
pub mod roll_ui;
pub mod sample_window;
pub mod sequencer_ui;
pub mod slicer_ui;
pub mod visualizer;
pub mod session_ui;
pub mod theme;
pub mod track_ui;
pub mod widgets;
mod mobile;
pub use mobile::CRASH_FILE;
mod navigation;
mod account_ui;
mod transport_ui;
mod fx_slots;
pub mod plugin_ui;
pub mod plugins_ui;
pub mod smart_steps;

use crate::audio_engine::device::{AudioIo, Shared};
use crate::audio_engine::export::{self, BitDepth, ExportSettings};
use crate::audio_engine::Cmd;
use crate::midi_in::MidiIn;
use crate::mixer::NUM_STRIPS;
use crate::piano_roll::{Note, Track};
use crate::project::Project;
use crate::sequencer::*;
use crossbeam_channel::{unbounded, Receiver};
use egui::{pos2, vec2, Color32, Key, RichText, Stroke};
#[cfg(any(target_os = "android", target_arch = "wasm32"))]
use crate::rfd;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use theme::*;
use widgets::*;

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Tab {
    Sequencer,
    Track,
    PianoRoll,
    Arrangement,
    Session,
    Mixer,
    Instruments,
    Plugins,
    /// no view requested (the request is consumed every frame)
    Idle,
}

/// A number that changes whenever something that is heard in Song mode changes (so the player waveform is re-rendered).
fn song_signature(p: &Project) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    p.bpm.to_bits().hash(&mut h);
    p.swing.to_bits().hash(&mut h);
    for c in &p.arrangement.clips {
        (c.pattern, c.lane, c.start, c.len, c.audio, c.offset, c.gain.to_bits()).hash(&mut h);
    }
    for t in &p.arrangement.tracks {
        (t.mute, t.solo, t.volume.to_bits(), t.instrument.is_some()).hash(&mut h);
    }
    for pat in &p.patterns {
        pat.length_steps.hash(&mut h);
        for n in &pat.notes {
            (n.start, n.len, n.pitch, n.vel).hash(&mut h);
        }
        for l in &pat.drums {
            for (i, s) in l.iter().enumerate() {
                if s.on {
                    (i, s.vel).hash(&mut h);
                }
            }
        }
    }
    for c in &p.channels {
        (c.volume.to_bits(), c.mute).hash(&mut h);
    }
    h.finish()
}

/// Working-set memory of this process in MB (0 where it cannot be read).
#[cfg(windows)]
fn process_memory_mb() -> f32 {
    #[repr(C)]
    struct Pmc {
        cb: u32,
        page_faults: u32,
        peak_ws: usize,
        ws: usize,
        a: usize,
        b: usize,
        c: usize,
        d: usize,
        pagefile: usize,
        peak_pagefile: usize,
    }
    #[link(name = "psapi")]
    extern "system" {
        fn GetProcessMemoryInfo(h: isize, p: *mut Pmc, cb: u32) -> i32;
    }
    extern "system" {
        fn GetCurrentProcess() -> isize;
    }
    let mut m = Pmc { cb: std::mem::size_of::<Pmc>() as u32, page_faults: 0, peak_ws: 0, ws: 0, a: 0, b: 0, c: 0, d: 0, pagefile: 0, peak_pagefile: 0 };
    unsafe {
        if GetProcessMemoryInfo(GetCurrentProcess(), &mut m, m.cb) != 0 {
            return m.ws as f32 / 1_048_576.0;
        }
    }
    0.0
}

#[cfg(not(windows))]
fn process_memory_mb() -> f32 {
    0.0
}

/// Folder where the hub keeps its projects.
fn hub_dir() -> PathBuf {
    // a chosen sync folder (a cloud-drive / Syncthing folder shared by the PC and the phone) wins
    if let Ok(s) = std::fs::read_to_string("sync_folder.txt") {
        let p = PathBuf::from(s.trim());
        if !s.trim().is_empty() && (p.is_dir() || std::fs::create_dir_all(&p).is_ok()) {
            return p;
        }
    }
    let base = std::env::var_os("USERPROFILE").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    let d = base.join("Documents").join("Winteria Projects");
    let _ = std::fs::create_dir_all(&d);
    d
}

/// Where safety copies of a project go before it is replaced (and the rolling autosave).
fn backup_dir() -> PathBuf {
    let d = hub_dir().join("Backups");
    let _ = std::fs::create_dir_all(&d);
    d
}

/// Remember the sync folder (empty = the default hub folder).
fn set_sync_folder(p: Option<&std::path::Path>) {
    let _ = std::fs::write("sync_folder.txt", p.map(|p| p.to_string_lossy().to_string()).unwrap_or_default());
}

/// Every project the hub knows: the hub folder, the Documents folder and the recent list, newest first.
fn hub_projects(recent: &[PathBuf]) -> Vec<(PathBuf, std::time::SystemTime, u64)> {
    let hub = hub_dir();
    // Discover legacy projects in place, including those not in the recent list.
    let legacy = hub.parent().filter(|_| hub.ends_with("Winteria Projects"))
        .map(|parent| parent.join("SYNTHESIA Projects"));
    #[cfg(target_arch = "wasm32")]
    {
        let mut projects = crate::web::projects(&hub);
        if let Some(dir) = &legacy {
            projects.extend(crate::web::projects(dir));
            projects.sort_by_key(|(_, saved, _)| std::cmp::Reverse(*saved));
        }
        return projects;
    }
    let mut files: Vec<PathBuf> = Vec::new();
    for dir in [Some(hub.clone()), legacy, hub.parent().map(|p| p.to_path_buf())].into_iter().flatten() {
        if let Ok(rd) = std::fs::read_dir(dir) {
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().map_or(false, |x| x.eq_ignore_ascii_case("wd") || x.eq_ignore_ascii_case("vpd") || x.eq_ignore_ascii_case("synthesia")) {
                    files.push(p);
                }
            }
        }
    }
    files.extend(recent.iter().cloned());
    let mut seen = std::collections::HashSet::new();
    let mut out: Vec<(PathBuf, std::time::SystemTime, u64)> = Vec::new();
    for p in files {
        let key = p.to_string_lossy().to_lowercase();
        if !seen.insert(key) {
            continue;
        }
        if let Ok(m) = std::fs::metadata(&p) {
            out.push((p, m.modified().unwrap_or(std::time::UNIX_EPOCH), m.len()));
        }
    }
    out.sort_by(|a, b| b.1.cmp(&a.1));
    out
}

/// Full-window dark splash in the Windows 11 style: a ring of dots that fan out, speed round and gather again, text underneath.
fn splash(ctx: &egui::Context, id: &str, sub: &str) {
    let screen = ctx.screen_rect();
    egui::Area::new(egui::Id::new(id)).order(egui::Order::Foreground).fixed_pos(screen.min).show(ctx, |ui| {
        let (rect, _) = ui.allocate_exact_size(screen.size(), egui::Sense::click_and_drag());
        ui.painter().rect_filled(rect, 0.0, Color32::from_black_alpha(215));
        let c = rect.center() - egui::vec2(0.0, 24.0);
        let t = ui.input(|i| i.time) as f32;
        let (period, travel, gap) = (2.6_f32, 1.7_f32, 0.11_f32);
        for k in 0..6 {
            let u = (((t % period) - k as f32 * gap) / travel).clamp(0.0, 1.0);
            let e = u * u * (3.0 - 2.0 * u);
            let a = -std::f32::consts::FRAC_PI_2 + e * std::f32::consts::TAU;
            let fade = if u <= 0.0 || u >= 1.0 { 0.0 } else { (u.min(1.0 - u) * 8.0).min(1.0) };
            let p = c + egui::vec2(a.cos(), a.sin()) * 22.0;
            ui.painter().circle_filled(p, 3.6 - k as f32 * 0.25, Color32::from_white_alpha((255.0 * fade) as u8));
        }
        ui.painter().text(rect.center() + egui::vec2(0.0, 48.0), egui::Align2::CENTER_CENTER, "Rendering...", egui::FontId::proportional(20.0), Color32::WHITE);
        ui.painter().text(rect.center() + egui::vec2(0.0, 76.0), egui::Align2::CENTER_CENTER, sub, egui::FontId::proportional(13.0), Color32::from_white_alpha(170));
    });
}

#[cfg(not(target_arch = "wasm32"))]
struct TrackRender {
    source_name: String,
    result: Receiver<Result<Option<(Project, usize)>, String>>,
}

struct ExportUi {
    open: bool,
    rate: u32,
    depth: BitDepth,
    bars: u32,
    tail: f32,
    /// render the whole arrangement (true) or the selected pattern looped (false), like the FL export dialog
    song: bool,
    /// render only the ticked Playlist tracks (`pick[lane]`) instead of everything
    only_picked: bool,
    pick: Vec<bool>,
    rx: Option<Receiver<Result<String, String>>>,
    result: String,
    /// where the render in flight is saved: the finished file is put on a new Playlist track
    path: Option<std::path::PathBuf>,
}

pub struct VinteriaApp {
    project: Project,
    published: Arc<Project>,
    shared: Arc<Shared>,
    audio: AudioIo,
    midi: MidiIn,
    tab: Tab,
    /// main view (Playlist / Mixer) and the Ableton-style detail dock (Step Seq / Piano Roll / Instruments)
    main: Tab,
    dock: Tab,
    dock_open: bool,
    /// drum lane whose sound window (FL Sampler style) is open
    sample_win: Option<usize>,
    /// the plugin has its own window (preset library + engine editor)
    plugin_window: bool,
    /// Channel Rack and Mixer open from the toolbar as floating windows; the bottom dock is the selected track's clip / device view
    show_rack: bool,
    show_mixer: bool,
    /// the Piano Roll is a floating window like the Channel Rack and Mixer
    show_roll: bool,
    /// Channel Rack: the step rows under the launchpad (off by default)
    pad_steps: bool,
    /// the launchpad pad whose piano roll was last set up
    pad_last: Option<(usize, usize)>,
    /// Channel Rack view: 0 = Steps, 1 = Piano roll
    rack_mode: u8,
    sent_lane: usize,
    /// how many times a split-off channel clip is repeated in the Playlist
    split_repeat: u32,
    last_auto_check: web_time::Instant,
    /// undo history: snapshots of the project before an edit (edits within 0.4 s are one step)
    undo: Vec<Arc<Project>>,
    redo: Vec<Arc<Project>>,
    last_undo: web_time::Instant,
    undo_skip: bool,
    /// the Piano Roll page exists only after a MIDI track (or a Channel Rack sound) was picked
    roll_tab: bool,
    /// recently opened / saved projects (persisted in recent_projects.txt) and the project browser window
    recent: Vec<PathBuf>,
    show_projects: bool,
    /// the project hub (tiles of projects) is shown instead of the program; shown at start and from the Hub button
    hub_open: bool,
    /// Retrospective Record: the notes played so far (the keyboard / MIDI), kept even while not recording
    retro: Vec<crate::project::RetroNote>,
    retro_held: HashMap<u8, (f64, u8)>,
    started: web_time::Instant,
    /// the project as it was last saved / opened (what "unsaved changes" are measured against)
    saved: Arc<Project>,
    last_autosave: web_time::Instant,
    /// start of the logo splash shown when the program opens
    #[cfg(target_os = "android")]
    splash: Option<web_time::Instant>,
    /// phone layout: dock tab (0 Projects, 1 Studio, 2 Settings, 3 Profile) and the section picked in the icon rail
    m_tab: usize,
    m_sec: usize,
    m_export: mobile::MobileExport,
    #[cfg(not(target_arch = "wasm32"))]
    track_render: Option<TrackRender>,
    /// waveform of the whole arrangement (signature it was rendered for, peaks), the render in flight and the one waiting
    song_wave: Option<(u64, Vec<f32>)>,
    song_rx: Option<Receiver<(u64, Vec<f32>)>>,
    song_pending: Option<(u64, web_time::Instant)>,
    hub_cache: Option<Vec<(PathBuf, std::time::SystemTime, u64)>>,
    last_sel_lane: usize,
    dock_prev: Tab,
    dock_jump: Option<Tab>,
    dock_h: f32,
    seq: sequencer_ui::SeqState,
    roll: roll_ui::RollState,
    arr: arrangement_ui::ArrState,
    lib: library::Library,
    viz: visualizer::VizState,
    inst: instruments_ui::Inst,
    show_help: bool,
    browser_open: bool,
    account_open: bool,
    /// the exe this live copy was started from and its time stamp: a new build replaces it and the program restarts into it
    live_origin: Option<(PathBuf, std::time::SystemTime)>,
    #[cfg(not(target_os = "android"))]
    updater: crate::updater::Updater,
    #[cfg(target_os = "android")]
    pub(crate) android_updater: Option<crate::android_update::Updater>,
    update_seen: Option<(std::time::SystemTime, web_time::Instant)>,
    last_update_check: web_time::Instant,
    export: ExportUi,
    path: Option<PathBuf>,
    status: String,
    /// short notices at the bottom centre: (text, born, kind 0 info / 1 done / 2 problem)
    toasts: Vec<(String, web_time::Instant, u8)>,
    last_status: String,
    record: bool,
    /// what the record button captures: 0 notes (keyboard / MIDI), 1 audio (input device), 2 both
    rec_mode: u8,
    recorder: crate::audio_rec::AudioRecorder,
    /// song position (ticks) where the audio recording began
    rec_from: u32,
    #[cfg(target_arch = "wasm32")]
    voice_lane: usize,
    octave: i32,
    live_track: Track,
    key_held: HashMap<Key, u8>,
    rec_held: HashMap<u8, (u32, u32)>,
    meters: [[f32; 2]; NUM_STRIPS + 1],
    show_audio: bool,
    buffer_sel: u32,
    rate_sel: u32,
    dev_sel: String,
}

impl VinteriaApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        crate::boot_log("theme");
        theme::apply(&cc.egui_ctx);
        crate::boot_log("project");
        let mut project = Project::default();
        project.arrangement.tracks = crate::arrangement::Arrangement::starter_tracks();
        let mut lib = library::Library::new();
        lib.apply_default_kit(&mut project);
        // the tracks' own synth settings exist from the start, so the first frame changes nothing (no phantom undo step)
        project.adopt_track_params();
        let published = Arc::new(project.clone());
        let shared = Shared::new(Arc::clone(&published));
        crate::boot_log(if crate::safe_mode() { "audio skipped (safe mode)" } else { "audio open" });
        let audio = if crate::safe_mode() { AudioIo::silent(Arc::clone(&shared)) } else { AudioIo::new(Arc::clone(&shared)) };
        crate::boot_log("audio ready, midi");
        let midi = MidiIn::new(Arc::clone(&shared));
        let status = match &audio.error {
            Some(e) => e.clone(),
            None => format!("Audio: {} @ {} Hz", audio.device_name, audio.sample_rate),
        };
        let dev_sel = audio.device_name.clone();
        let rate_sel = audio.sample_rate;
        let live_origin = std::env::args().find_map(|a| a.strip_prefix("--live=").map(PathBuf::from)).and_then(|p| {
            let m = std::fs::metadata(&p).ok()?.modified().ok()?;
            Some((p, m))
        });
        let mut app = VinteriaApp {
            project,
            retro: Vec::new(),
            retro_held: HashMap::new(),
            started: web_time::Instant::now(),
            saved: Arc::clone(&published),
            last_autosave: web_time::Instant::now(),
            published,
            shared,
            audio,
            midi,
            tab: Tab::Arrangement,
            main: Tab::Arrangement,
            dock: Tab::PianoRoll,
            dock_open: false,
            sample_win: None,
            plugin_window: false,
            show_rack: false,
            show_mixer: false,
            show_roll: false,
            pad_steps: false,
            pad_last: None,
            rack_mode: 0,
            sent_lane: usize::MAX,
            split_repeat: 4,
            last_auto_check: web_time::Instant::now().checked_sub(std::time::Duration::from_secs(60)).unwrap_or_else(web_time::Instant::now),
            undo: vec![],
            redo: vec![],
            last_undo: web_time::Instant::now(),
            undo_skip: false,
            roll_tab: false,
            recent: std::fs::read_to_string("recent_projects.txt").map(|t| t.lines().map(PathBuf::from).filter(|p| p.exists()).collect()).unwrap_or_default(),
            show_projects: false,
            hub_open: false,
            #[cfg(target_os = "android")]
            splash: None,
            m_tab: 4,
            m_sec: 0,
            m_export: Default::default(),
            #[cfg(not(target_arch = "wasm32"))]
            track_render: None,
            song_wave: None,
            song_rx: None,
            song_pending: None,
            hub_cache: None,
            last_sel_lane: 0,
            dock_prev: Tab::PianoRoll,
            dock_jump: None,
            dock_h: 440.0,
            seq: sequencer_ui::SeqState::new(),
            roll: roll_ui::RollState::new(),
            arr: arrangement_ui::ArrState::new(),
            lib,
            viz: visualizer::VizState::new(),
            inst: instruments_ui::Inst::Poly,
            show_help: false,
            browser_open: true,
            account_open: false,
            live_origin,
            #[cfg(not(target_os = "android"))]
            updater: crate::updater::Updater::new(),
            #[cfg(target_os = "android")]
            android_updater: None,
            update_seen: None,
            last_update_check: web_time::Instant::now(),
            export: ExportUi { open: false, rate: 44100, depth: BitDepth::Pcm24, bars: 8, tail: 2.0, song: true, only_picked: false, pick: Vec::new(), rx: None, result: String::new(), path: None },
            path: None,
            status,
            toasts: Vec::new(),
            last_status: String::new(),
            record: false,
            rec_mode: 0,
            recorder: Default::default(),
            rec_from: 0,
            #[cfg(target_arch = "wasm32")]
            voice_lane: 0,
            octave: 4,
            live_track: Track::Synth,
            key_held: HashMap::new(),
            rec_held: HashMap::new(),
            meters: [[0.0; 2]; NUM_STRIPS + 1],
            show_audio: false,
            buffer_sel: 512,
            rate_sel,
            dev_sel,
        };
        app.restore_session();
        app
    }

    /// Every new status message is tracked for a few seconds; the status bar tints its text by the message kind
    /// (no floating pills: messages live in the one bottom bar).
    fn toasts_ui(&mut self, ctx: &egui::Context) {
        if self.status != self.last_status {
            self.last_status = self.status.clone();
            let text = self.status.trim().to_string();
            let low = text.to_lowercase();
            let skip = text.is_empty() || text.starts_with("Audio:") || self.toasts.last().map_or(false, |t| t.0 == text && t.1.elapsed().as_secs_f32() < 1.0);
            if !skip {
                let kind = if ["fail", "error", "could not", "not available", "nothing to", "select a", "no free"].iter().any(|k| low.contains(k)) {
                    2
                } else if ["saved", "opened", "added", "sent", "rendered", "loaded", "exported", "verified", "starred", "restored", "reopened", "updated"].iter().any(|k| low.contains(k)) {
                    1
                } else {
                    0
                };
                self.toasts.push((text, web_time::Instant::now(), kind));
                if self.toasts.len() > 3 {
                    self.toasts.remove(0);
                }
            }
        }
        const LIFE: f32 = 3.4;
        self.toasts.retain(|t| t.1.elapsed().as_secs_f32() < LIFE);
        if !self.toasts.is_empty() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn render_overlay(&mut self, ctx: &egui::Context) -> bool {
        let id = egui::Id::new("render_job");
        if let Some(job) = self.track_render.as_ref() {
            let result = match job.result.try_recv() {
                Ok(result) => result,
                Err(crossbeam_channel::TryRecvError::Empty) => {
                    splash(ctx, "render_splash", &format!("{} → new audio track", job.source_name));
                    ctx.request_repaint_after(std::time::Duration::from_millis(16));
                    return true;
                }
                Err(crossbeam_channel::TryRecvError::Disconnected) => Err("Rendering stopped unexpectedly".into()),
            };
            self.track_render = None;
            self.arr.msg = match result {
                Ok(Some((project, lane))) => {
                    if self.is_mobile() {
                        self.m_playlist_undo_step();
                    }
                    self.project = project;
                    if self.is_mobile() {
                        self.arr.sel_lane = lane;
                        self.arr.sel_clip = Some(self.project.arrangement.clips.len() - 1);
                        self.arr.sel.clear();
                        let start = self.project.arrangement.clips.last().map_or(0, |clip| clip.start);
                        ctx.data_mut(|d| {
                            d.insert_temp(egui::Id::new("m_playlist_reveal_track"), lane);
                            d.insert_temp(egui::Id::new("m_playlist_reveal_tick"), start);
                        });
                    }
                    format!("Rendered to {}", self.project.arrangement.tracks[lane].name)
                }
                Ok(None) => "Nothing audible to render".into(),
                Err(error) => format!("Render failed: {error}"),
            };
            self.status.clone_from(&self.arr.msg);
            return false;
        }

        let (target, selection) = ctx.data_mut(|d| (
            d.remove_temp::<Option<usize>>(id),
            d.remove_temp::<Vec<usize>>(egui::Id::new("render_selection_job")),
        ));
        let Some(target) = target.or_else(|| selection.as_ref().map(|_| None)) else { return false; };
        let source_name = match target {
            Some(lane) => {
                let Some(track) = self.project.arrangement.tracks.get(lane) else { return false };
                track.name.clone()
            }
            None if selection.is_some() => "Selected clips".into(),
            None => "Playlist mix".into(),
        };
        let mut project = self.project.clone();
        let (tx, result) = crossbeam_channel::bounded(1);
        let worker = std::thread::Builder::new().name("winteria-track-render".into()).spawn(move || {
            let result = match (target, selection) {
                (_, Some(indices)) => crate::bounce::render_selection_to_audio(&mut project, &indices),
                (Some(lane), None) => crate::bounce::render_track_to_audio(&mut project, lane),
                (None, None) => crate::bounce::render_mix_to_audio(&mut project),
            }.map(|lane| lane.map(|lane| (project, lane)));
            let _ = tx.send(result);
        });
        match worker {
            Ok(_) => {
                splash(ctx, "render_splash", &format!("{source_name} → new audio track"));
                self.track_render = Some(TrackRender { source_name, result });
                ctx.request_repaint_after(std::time::Duration::from_millis(16));
                true
            }
            Err(error) => {
                self.arr.msg = format!("Could not start render: {error}");
                self.status.clone_from(&self.arr.msg);
                false
            }
        }
    }

    /// The dock under the Playlist: Track Settings / Instruments / Track Effects, and the clip editor.
    fn bottom_dock(&mut self, ctx: &egui::Context, editor_dock: bool) {
        egui::TopBottomPanel::bottom(if editor_dock { "device_chain_dock_editor" } else { "device_chain_dock" })
            .resizable(true)
            .height_range(240.0..=820.0)
            .default_height(if editor_dock { (ctx.screen_rect().height() * 0.5).clamp(380.0, 640.0) } else { 330.0 })
            .frame(egui::Frame::none().fill(SIDE()).stroke(Stroke::new(border_w(), border())).inner_margin(egui::Margin::symmetric(16.0, 12.0)))
            .show(ctx, |ui| {
                devices_ui::dock(
                    ui,
                    &mut self.project,
                    &mut self.arr,
                    &mut self.roll,
                    &mut self.live_track,
                    &mut self.inst,
                    &mut self.plugin_window,
                    &mut self.lib,
                    self.shared.position.load(Ordering::Relaxed),
                    &self.shared,
                );
            });
    }

    /// After a restart caused by an update: open the project the user was working in (with its unsaved changes).
    fn restore_session(&mut self) {
        if self.live_origin.is_none() {
            return;
        }
        let Ok(text) = std::fs::read_to_string("session_restore.txt") else { return };
        let _ = std::fs::remove_file("session_restore.txt");
        let mut it = text.lines();
        let path = it.next().filter(|l| !l.is_empty()).map(PathBuf::from);
        if it.next() == Some("dirty") {
            if let Ok((p, _)) = Project::load(std::path::Path::new("restart_recovery.synthesia")) {
                self.project = p;
                self.undo.clear();
                self.redo.clear();
                self.undo_skip = true;
                self.path = path;
                self.status = "Updated: your project was restored with its unsaved changes".into();
            }
            let _ = std::fs::remove_file("restart_recovery.synthesia");
        } else if let Some(p) = path {
            self.open_path(p);
            self.status = "Updated: your project was reopened".into();
        }
    }

    /// A new build replaced the exe we were started from: keep the work, start the new version and end this one.
    fn check_update(&mut self, ctx: &egui::Context) {
        let Some((orig, started)) = self.live_origin.clone() else { return };
        if self.last_update_check.elapsed() < std::time::Duration::from_millis(1000) {
            return;
        }
        self.last_update_check = web_time::Instant::now();
        ctx.request_repaint_after(std::time::Duration::from_secs(1));
        let Ok(meta) = std::fs::metadata(&orig) else { return };
        let Ok(m) = meta.modified() else { return };
        if m == started {
            self.update_seen = None;
            return;
        }
        // wait until the build has finished writing the file
        match self.update_seen {
            Some((seen, since)) if seen == m => {
                if since.elapsed() > std::time::Duration::from_secs(2) && meta.len() > 1_000_000 && std::fs::File::open(&orig).is_ok() {
                    self.restart_into(&orig, m);
                }
            }
            _ => self.update_seen = Some((m, web_time::Instant::now())),
        }
    }

    fn restart_into(&mut self, orig: &std::path::Path, stamp: std::time::SystemTime) {
        self.shared.send(Cmd::Stop);
        let mut text = self.path.as_ref().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
        text.push('\n');
        if self.dirty() {
            if self.project.save(std::path::Path::new("restart_recovery.synthesia")).is_err() {
                // the work could not be kept: do not restart, tell the user
                self.status = "A new version is ready, but the project could not be kept: save it and restart the program".into();
                self.live_origin = Some((orig.to_path_buf(), stamp));
                self.update_seen = None;
                return;
            }
            text.push_str("dirty");
        } else {
            text.push_str("clean");
        }
        let _ = std::fs::write("session_restore.txt", text);
        let args: Vec<String> = std::env::args().skip(1).filter(|a| !a.starts_with("--live=")).collect();
        if std::process::Command::new(orig).args(args).spawn().is_ok() {
            std::process::exit(0);
        }
        let _ = std::fs::remove_file("session_restore.txt");
        self.live_origin = Some((orig.to_path_buf(), stamp));
    }

    /// true when the project has changes that were not saved
    fn dirty(&self) -> bool {
        self.project != *self.saved
    }

    fn mark_clean(&mut self) {
        self.saved = Arc::new(self.project.clone());
    }

    /// Before the project is replaced (New, Open, a template) or the program closes: keep a copy of unsaved work in the
    /// Backups folder. Only projects that hold something are copied; the 20 newest copies are kept.
    fn backup_if_dirty(&mut self) -> Option<PathBuf> {
        let holds_something = !self.project.arrangement.clips.is_empty() || self.project.patterns.iter().any(|p| !p.notes.is_empty());
        if !self.dirty() || !holds_something {
            return None;
        }
        let secs = web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let name: String = self.project.name.chars().map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' { c } else { '_' }).collect();
        let path = backup_dir().join(format!("{name} {secs}.wd"));
        if self.project.save(&path).is_err() {
            return None;
        }
        // keep the newest 20
        if let Ok(rd) = std::fs::read_dir(backup_dir()) {
            let mut files: Vec<(std::time::SystemTime, PathBuf)> = rd.flatten().filter(|e| e.file_name().to_string_lossy() != "_autosave.wd").filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path()))).collect();
            files.sort_by(|a, b| b.0.cmp(&a.0));
            for (_, p) in files.into_iter().skip(20) {
                let _ = std::fs::remove_file(p);
            }
        }
        Some(path)
    }

    /// Hand edits to the audio thread (lock-free snapshot swap) and keep the undo history.
    fn publish_edits(&mut self) {
        if self.project != *self.published {
            if !self.undo_skip && self.last_undo.elapsed() > std::time::Duration::from_millis(400) {
                self.undo.push(Arc::clone(&self.published));
                if self.undo.len() > 100 {
                    self.undo.remove(0);
                }
                self.redo.clear();
            }
            if !self.undo_skip {
                self.last_undo = web_time::Instant::now();
            }
            self.undo_skip = false;
            let snap = Arc::new(self.project.clone());
            self.shared.project.store(Arc::clone(&snap));
            self.published = snap;
        }
    }

    fn playing(&self) -> bool {
        self.shared.playing.load(Ordering::Relaxed)
    }

    fn toggle_play(&mut self) {
        if self.playing() {
            // Play while playing is a pause; the Stop button rewinds
            self.shared.send(Cmd::Pause);
        } else {
            if self.main == Tab::Arrangement {
                self.project.song_mode = true;
            } else if matches!(self.main, Tab::Sequencer | Tab::PianoRoll) {
                self.project.song_mode = false;
            }
            self.shared.send(Cmd::Play);
        }
    }

    fn return_to_start(&mut self, ctx: &egui::Context, keep_playing: bool) {
        self.shared.send(if keep_playing { Cmd::PlayFrom(0) } else { Cmd::Stop });
        if !keep_playing { self.record = false; }
        ctx.data_mut(|d| d.insert_temp(egui::Id::new("playlist_return_to_start"), true));
        ctx.request_repaint();
    }

    // ---- project file handling ----
    fn load_template(&mut self, idx: usize) {
        self.shared.send(Cmd::Stop);
        self.backup_if_dirty();
        let (name, make) = crate::templates::TEMPLATES[idx];
        self.project = make();
        if let Some(kit) = crate::templates::template_kit(name) {
            self.lib.apply_kit(&mut self.project, &kit);
        }
        if let Some(file) = crate::templates::cowbell_sample(name) {
            crate::templates::cowbell_to_sampler(&mut self.project);
            self.lib.assign_sampler(&mut self.project, &std::path::PathBuf::from("samples").join(file));
        }
        {
            let lib = &mut self.lib;
            crate::templates::hydrate_samples(&mut self.project, &mut |p| lib.get(p).ok());
        }
        if !crate::templates::has_own_mix(name) {
            crate::templates::finish_mix(&mut self.project);
        }
        self.lib.apply_default_kit(&mut self.project);
        self.path = None;
        self.mark_clean();
        self.status = format!("Template: {name} - press Play (Pattern mode loops the selected pattern, Song mode plays the playlist)");
    }

    #[cfg(target_os = "android")]
    fn splash_ui(&mut self, ctx: &egui::Context) -> bool {
        if self.splash.is_none() {
            if let Some(android) = &self.android_updater {
                android.play_startup_sound();
            }
        }
        let elapsed = self.splash.get_or_insert_with(web_time::Instant::now).elapsed().as_secs_f32();
        if elapsed >= 0.86 {
            return false;
        }

        let screen = ctx.screen_rect();
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("startup_splash"),
        ));
        painter.rect_filled(screen, 0.0, egui::Color32::from_rgb(14, 16, 23));

        let center = screen.center();
        let radius = (screen.width().min(screen.height()) * 0.13).clamp(20.0, 56.0);
        let ring_width = radius * 0.065;
        let accent = egui::Color32::from_rgb(167, 156, 255);
        painter.circle_stroke(
            center,
            radius,
            egui::Stroke::new(ring_width, egui::Color32::from_rgb(43, 46, 63)),
        );

        let progress = (elapsed / 0.48).clamp(0.0, 1.0);
        if progress > 0.0 {
            let inner_radius = radius - ring_width;
            let level = inner_radius * (1.0 - 1.28 * progress);
            let amplitude = inner_radius * 0.11 * progress.min(1.0) * (1.0 - 0.55 * progress);
            let surface = egui::Color32::from_rgb(91, 75, 151);
            let depth = egui::Color32::from_rgb(36, 33, 72);
            let mut water = egui::Mesh::default();
            const STRIPS: u32 = 64;
            for i in 0..=STRIPS {
                let x = 2.0 * i as f32 / STRIPS as f32 - 1.0;
                let extent = inner_radius * (1.0 - x * x).max(0.0).sqrt();
                let wave = (x * 4.2 - elapsed * 12.0).sin()
                    + 0.28 * (x * 8.0 + elapsed * 8.0).sin();
                let y = (level + amplitude * wave).clamp(-extent, extent);
                water.colored_vertex(center + egui::vec2(x * inner_radius, y), surface);
                water.colored_vertex(center + egui::vec2(x * inner_radius, extent), depth);
                if i < STRIPS {
                    let vertex = 2 * i;
                    water.add_triangle(vertex, vertex + 1, vertex + 2);
                    water.add_triangle(vertex + 2, vertex + 1, vertex + 3);
                }
            }
            painter.add(egui::Shape::mesh(water));
        }

        if progress >= 1.0 {
            painter.circle_stroke(center, radius, egui::Stroke::new(ring_width, accent));
        } else if progress > 0.0 {
            let segments = (64.0 * progress).ceil() as usize;
            let points = (0..=segments)
                .map(|i| {
                    let angle = std::f32::consts::TAU * progress * i as f32 / segments as f32
                        - std::f32::consts::FRAC_PI_2;
                    center + egui::vec2(angle.cos(), angle.sin()) * radius
                })
                .collect();
            painter.add(egui::Shape::line(points, egui::Stroke::new(ring_width, accent)));
        }

        let reveal = ((elapsed - 0.48) / 0.18).clamp(0.0, 1.0);
        if reveal > 0.0 {
            let alpha = (255.0 * reveal * reveal * (3.0 - 2.0 * reveal)).round() as u8;
            let points = [(-0.54, -0.25), (-0.30, 0.28), (0.0, -0.17), (0.30, 0.28), (0.54, -0.25)]
                .into_iter()
                .map(|(x, y)| center + egui::vec2(x, y) * radius)
                .collect();
            let color = egui::Color32::from_rgba_unmultiplied(231, 236, 255, alpha);
            painter.add(egui::Shape::line(points, egui::Stroke::new(radius * 0.10, color)));
            painter.text(
                center + egui::vec2(0.0, radius + 14.0),
                egui::Align2::CENTER_TOP,
                "Winteria DAW",
                egui::FontId::proportional((radius * 0.48).clamp(17.0, 27.0)),
                color,
            );
        }

        ctx.request_repaint();
        true
    }

    /// Action history: every undo step as a row; a click returns the project to that moment (the later steps stay redoable).
    fn history_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("HISTORY").size(12.0).color(DIM()));
            if ui.add_enabled(!self.undo.is_empty(), egui::Button::new("Undo")).clicked() {
                self.undo();
            }
            if ui.add_enabled(!self.redo.is_empty(), egui::Button::new("Redo")).clicked() {
                self.redo();
            }
        });
        let n = self.undo.len();
        let mut jump = None;
        egui::ScrollArea::vertical().id_salt("history_scroll").auto_shrink([false, true]).max_height(110.0).show(ui, |ui| {
            ui.label(RichText::new(format!("now (step {})", n + 1)).size(11.0).color(TEAL()));
            for k in (0..n).rev() {
                if ui.selectable_label(false, RichText::new(format!("step {}", k + 1)).size(11.0)).on_hover_text("Go back to this moment").clicked() {
                    jump = Some(k);
                }
            }
            for k in (0..self.redo.len()).rev() {
                ui.label(RichText::new(format!("undone {}", self.redo.len() - k)).size(11.0).color(DIM()));
            }
        });
        if let Some(k) = jump {
            while self.undo.len() > k {
                self.undo();
            }
        }
    }

    /// A template INTO the current playlist: its tracks and clips are added under the ones already there. An empty project just
    /// becomes the template.
    fn add_template(&mut self, idx: usize) {
        if self.project.arrangement.clips.is_empty() {
            self.load_template(idx);
            return;
        }
        let (name, make) = crate::templates::TEMPLATES[idx];
        let mut t = make();
        if let Some(kit) = crate::templates::template_kit(name) {
            self.lib.apply_kit(&mut t, &kit);
        }
        if let Some(file) = crate::templates::cowbell_sample(name) {
            crate::templates::cowbell_to_sampler(&mut t);
            self.lib.assign_sampler(&mut t, &std::path::PathBuf::from("samples").join(file));
        }
        {
            let lib = &mut self.lib;
            crate::templates::hydrate_samples(&mut t, &mut |p| lib.get(p).ok());
        }
        match crate::templates::merge_template(&mut self.project, t) {
            Ok(n) => self.status = format!("Added {n} track(s) of \"{name}\" to the playlist"),
            Err(e) => self.status = e,
        }
    }

    fn undo(&mut self) {
        if let Some(prev) = self.undo.pop() {
            self.redo.push(Arc::new(self.project.clone()));
            self.project = (*prev).clone();
            self.undo_skip = true;
            self.status = format!("Undo ({} left)", self.undo.len());
        }
    }

    fn redo(&mut self) {
        if let Some(next) = self.redo.pop() {
            self.undo.push(Arc::new(self.project.clone()));
            self.project = (*next).clone();
            self.undo_skip = true;
            self.status = "Redo".into();
        }
    }

    fn new_project(&mut self) {
        self.shared.send(Cmd::Stop);
        self.backup_if_dirty();
        self.project = Project::default();
        self.project.arrangement.tracks = crate::arrangement::Arrangement::starter_tracks();
        self.lib.apply_default_kit(&mut self.project);
        self.path = None;
        self.mark_clean();
        self.status = "New project".into();
    }

    /// save the project, then rebuild and relaunch the app from its folder (a detached helper builds after we exit)
    fn restart(&mut self, ctx: &egui::Context) {
        if self.path.is_some() {
            self.save_project(false);
        }
        let cmd = "timeout /t 2 /nobreak >nul & cargo build --release & start \"\" target\\release\\VinteriaDaw.exe";
        match std::process::Command::new("cmd").args(["/C", "start", "Winteria DAW rebuild", "cmd", "/C", cmd]).spawn() {
            Ok(_) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            Err(e) => self.status = format!("Restart failed: {e}"),
        }
    }

    /// Save into the hub folder: a new project gets a free name there, a project that already lives there keeps its file.
    fn save_to_hub(&mut self) -> bool {
        let dir = hub_dir();
        let stem: String = self.project.name.chars().map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' { c } else { '_' }).collect();
        let stem = if stem.trim().is_empty() { "Project".to_string() } else { stem };
        let mut path = dir.join(format!("{stem}.wd"));
        match &self.path {
            Some(p) if p.parent() == Some(dir.as_path()) => path = p.clone(),
            _ => {
                let mut n = 2;
                #[cfg(target_arch = "wasm32")]
                let exists = |path: &std::path::Path| crate::web::read_project(path).is_ok();
                #[cfg(not(target_arch = "wasm32"))]
                let exists = |path: &std::path::Path| path.exists();
                while exists(&path) {
                    path = dir.join(format!("{stem} {n}.wd"));
                    n += 1;
                }
            }
        }
        let original_name = self.project.name.clone();
        if let Some(st) = path.file_stem() {
            self.project.name = st.to_string_lossy().to_string();
        }
        match self.project.save(&path) {
            Ok(()) => {
                self.status = format!("Saved to the hub: {}", path.display());
                self.push_recent(&path);
                self.path = Some(path);
                self.hub_cache = None;
                self.mark_clean();
                true
            }
            Err(e) => {
                self.project.name = original_name;
                self.status = format!("Save failed: {e}");
                false
            }
        }
    }

    /// The project hub: a start screen with a tile for every project, newest first. Picking one opens the program.
    fn hub_ui(&mut self, ctx: &egui::Context) {
        if self.hub_cache.is_none() {
            self.hub_cache = Some(hub_projects(&self.recent));
        }
        let mut pick: Option<Option<PathBuf>> = None;
        let mut tpl: Option<usize> = None;
        let mut resume = false;
        let mx = if ctx.screen_rect().width() < 700.0 { 12.0 } else { ((ctx.screen_rect().width() - 780.0) * 0.5).max(24.0) };
        let list = self.hub_cache.clone().unwrap_or_default();
        let now = std::time::SystemTime::now();
        let age_text = |modified: std::time::SystemTime| {
            let age = now.duration_since(modified).map(|d| d.as_secs()).unwrap_or(0);
            if age < 90 { "just now".to_string() } else if age < 3600 { format!("{} min ago", age / 60) } else if age < 86400 { format!("{} h ago", age / 3600) } else { format!("{} days ago", age / 86400) }
        };
        // one column, scrolling down: a wide horizontal card for every project
        egui::CentralPanel::default().frame(egui::Frame::none().inner_margin(egui::Margin::symmetric(mx, 16.0))).show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Winteria DAW").size(26.0).strong().color(TEAL()));
                ui.label(RichText::new(" BETA 1 ").size(11.0).strong().color(Color32::WHITE).background_color(CYAN()));
                ui.label(RichText::new(format!("v{}  -  Winteria Studio  -  project hub", env!("CARGO_PKG_VERSION"))).size(14.0).color(DIM()));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Open file...").clicked() {
                        pick = rfd::FileDialog::new().add_filter("Winteria DAW project (*.wd)", &["wd", "vpd", "synthesia"]).pick_file().map(Some);
                    }
                    if self.path.is_some() && ui.button("Back to the program").clicked() {
                        resume = true;
                    }
                });
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("Sync folder: {}", hub_dir().display())).size(11.0).color(DIM()));
                if ui.small_button("Change...").on_hover_text("Pick one folder for the PC and the phone (Google Drive / OneDrive / Syncthing): projects saved there show up on both").clicked() {
                    if let Some(dir) = rfd::FileDialog::new().set_title("Sync folder for Winteria DAW projects").pick_folder() {
                        set_sync_folder(Some(&dir));
                        self.hub_cache = None;
                    }
                }
                if ui.small_button("Default").clicked() {
                    set_sync_folder(None);
                    self.hub_cache = None;
                }
            });
            ui.add_space(6.0);
            egui::ScrollArea::vertical().id_salt("hub_cards_scroll").auto_shrink([false, false]).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                let w = ui.available_width();
                // new project card
                let (rect, r) = ui.allocate_exact_size(vec2(w, 46.0), egui::Sense::click());
                ui.painter().rect_filled(rect, card(), (if r.hovered() { PANEL3() } else { PANEL2() }).linear_multiply(0.86));
                ui.painter().rect_stroke(rect.shrink(1.0), card(), Stroke::new(1.0, if r.hovered() { TEAL() } else { MID() }));
                ui.painter().text(rect.left_center() + vec2(26.0, 0.0), egui::Align2::CENTER_CENTER, "+", egui::FontId::proportional(28.0), TEAL());
                ui.painter().text(rect.left_center() + vec2(54.0, 0.0), egui::Align2::LEFT_CENTER, "New project", egui::FontId::proportional(14.0), TEXT());
                if r.clicked() {
                    pick = Some(None);
                }
                for (i, (path, modified, size)) in list.iter().enumerate() {
                    let (rect, r) = ui.allocate_exact_size(vec2(w, 58.0), egui::Sense::click());
                    let p = ui.painter();
                    p.rect_filled(rect, card(), (if r.hovered() { PANEL3() } else { PANEL2() }).linear_multiply(0.86));
                    p.rect_stroke(rect, card(), Stroke::new(1.0, if r.hovered() { TEAL() } else { border() }));
                    let col = lane_col(i);
                    // coloured block with the first letter on the left
                    let block = egui::Rect::from_min_size(rect.min, vec2(58.0, rect.height()));
                    p.rect_filled(block, card(), col.linear_multiply(0.28));
                    p.rect_filled(egui::Rect::from_min_size(rect.min, vec2(4.0, rect.height())), 0.0, col);
                    let name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                    let initial = name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default();
                    p.text(block.center(), egui::Align2::CENTER_CENTER, initial, egui::FontId::proportional(26.0), col);
                    p.text(rect.left_top() + vec2(72.0, 8.0), egui::Align2::LEFT_TOP, &name, egui::FontId::proportional(14.0), TEXT());
                    p.text(rect.left_top() + vec2(72.0, 28.0), egui::Align2::LEFT_TOP, format!("{}  -  {} KB", age_text(*modified), size / 1024), egui::FontId::proportional(11.0), DIM());
                    p.text(rect.left_top() + vec2(72.0, 42.0), egui::Align2::LEFT_TOP, path.parent().map(|d| d.display().to_string()).unwrap_or_default(), egui::FontId::proportional(9.0), LIGHT());
                    p.text(rect.right_center() - vec2(20.0, 0.0), egui::Align2::RIGHT_CENTER, "Open", egui::FontId::proportional(13.0), if r.hovered() { TEAL() } else { LIGHT() });
                    if r.on_hover_text(path.display().to_string()).clicked() {
                        pick = Some(Some(path.clone()));
                    }
                }
                // ready-made starting points
                ui.add_space(8.0);
                ui.label(RichText::new("TEMPLATES").size(12.0).strong().color(TEAL_HI()));
                for (i, (name, _)) in crate::templates::TEMPLATES.iter().enumerate() {
                    if name.starts_with("Empty") {
                        continue;
                    }
                    let (rect, r) = ui.allocate_exact_size(vec2(w, 34.0), egui::Sense::click());
                    let p = ui.painter();
                    p.rect_filled(rect, card(), (if r.hovered() { PANEL3() } else { PANEL2() }).linear_multiply(0.86));
                    p.rect_stroke(rect, card(), Stroke::new(1.0, if r.hovered() { TEAL() } else { border() }));
                    p.rect_filled(egui::Rect::from_min_size(rect.min, vec2(4.0, rect.height())), 0.0, lane_col(i));
                    p.text(rect.left_center() + vec2(16.0, 0.0), egui::Align2::LEFT_CENTER, *name, egui::FontId::proportional(13.0), TEXT());
                    p.text(rect.right_center() - vec2(16.0, 0.0), egui::Align2::RIGHT_CENTER, "Template", egui::FontId::proportional(12.0), if r.hovered() { TEAL() } else { LIGHT() });
                    if r.clicked() {
                        tpl = Some(i);
                    }
                }
                ui.add_space(20.0);
            });
        });
        if let Some(i) = tpl {
            self.load_template(i);
            self.hub_open = false;
            return;
        }
        match pick {
            Some(Some(path)) => {
                self.open_path(path);
                self.hub_open = false;
            }
            Some(None) => {
                self.new_project();
                self.hub_open = false;
            }
            None => {
                if resume {
                    self.hub_open = false;
                }
            }
        }
    }

    fn push_recent(&mut self, path: &PathBuf) {
        self.recent.retain(|p| p != path);
        self.recent.insert(0, path.clone());
        self.recent.truncate(20);
        let text: Vec<String> = self.recent.iter().map(|p| p.to_string_lossy().to_string()).collect();
        let _ = std::fs::write("recent_projects.txt", text.join("\n"));
    }

    fn open_project(&mut self) {
        if cfg!(target_os = "android") {
            self.hub_open = true;
            return;
        }

        let Some(path) = rfd::FileDialog::new().add_filter("Winteria DAW project (*.wd)", &["wd", "vpd", "synthesia"]).pick_file() else { return };
        self.open_path(path);
    }

    fn open_path(&mut self, path: PathBuf) -> bool {
        match Project::load(&path) {
            Ok((p, warns)) => {
                self.shared.send(Cmd::Stop);
                self.backup_if_dirty();
                self.project = p;
                // the undo list belongs to the project that is gone: undoing into it under the new file name would overwrite this file
                self.undo.clear();
                self.redo.clear();
                self.undo_skip = true;
                self.mark_clean();
                self.status = if warns.is_empty() { format!("Opened {}", path.display()) } else { format!("Opened with warnings: {}", warns.join("; ")) };
                self.push_recent(&path);
                self.path = Some(path);
                true
            }
            Err(e) => {
                self.status = format!("Open failed: {e}");
                false
            }
        }
    }

    fn save_project(&mut self, save_as: bool) {
        let path = if save_as || self.path.is_none() {
            rfd::FileDialog::new().add_filter("Winteria DAW project (*.wd)", &["wd"]).set_directory(hub_dir()).set_file_name(format!("{}.wd", self.project.name)).save_file()
        } else {
            self.path.clone()
        };
        let Some(mut path) = path else { return };
        if path.extension().is_none() {
            path.set_extension("wd");
        }
        // notes of deleted clips are not written to the file
        self.project.prune_orphan_patterns();
        if let Some(stem) = path.file_stem() {
            self.project.name = stem.to_string_lossy().to_string();
        }
        match self.project.save(&path) {
            Ok(()) => {
                self.mark_clean();
                self.status = format!("Saved {}", path.display());
                self.push_recent(&path);
                self.path = Some(path);
            }
            Err(e) => self.status = format!("Save failed: {e}"),
        }
    }

    /// Where recorded notes go: (pattern, tick inside it). Pattern mode records into the current pattern; Song mode into
    /// the selected MIDI track's own pattern, adding a clip at the playhead bar when none is there yet.
    fn rec_target(&mut self) -> Option<(usize, u32)> {
        let pos = self.shared.position.load(Ordering::Relaxed);
        if !self.project.song_mode {
            return Some((self.project.current_pattern, pos));
        }
        let lane = self.project.arrangement.tracks.iter().position(|t| t.arm && t.kind == crate::arrangement::TrackKind::Midi).unwrap_or(self.arr.sel_lane);
        if self.project.arrangement.kind(lane) != crate::arrangement::TrackKind::Midi {
            return None;
        }
        let pi = self.project.lane_pattern(lane);
        let bar = pos / TICKS_PER_BAR;
        let existing = self.project.arrangement.clips.iter().find(|c| c.lane == lane && !c.audio && c.start_tick() <= pos && pos < c.end_tick()).copied();
        let clip = match existing {
            Some(c) => c,
            None => {
                let bars = ((self.project.patterns[pi].len_ticks() + TICKS_PER_BAR - 1) / TICKS_PER_BAR).max(1);
                let c = crate::arrangement::Clip::bars(pi, lane, bar, bars, false);
                if !self.project.arrangement.can_place(&c, None) {
                    return None;
                }
                self.project.arrangement.clips.push(c);
                c
            }
        };
        Some((clip.pattern, pos - clip.start_tick() + clip.offset))
    }

    /// The input setting of the track that receives live notes (the armed MIDI track, else the selected one) decides which
    /// source it listens to: All Ins, Keyboard (computer keys), MIDI device, or No Input.
    fn input_allows(&self, from_keyboard: bool) -> bool {
        let tracks = &self.project.arrangement.tracks;
        let lane = tracks.iter().position(|t| t.arm && t.kind == crate::arrangement::TrackKind::Midi).unwrap_or(self.arr.sel_lane);
        match tracks.get(lane).filter(|t| t.kind == crate::arrangement::TrackKind::Midi).map(|t| t.input) {
            None | Some(0) => true,
            Some(1) => from_keyboard,
            Some(2) => !from_keyboard,
            _ => false,
        }
    }

    // ---- live note input (computer keyboard / MIDI) ----
    /// Retrospective Record: drop what was played (even though Record was off) into a new MIDI clip on the selected / armed MIDI track.
    fn retro_capture(&mut self) {
        if self.retro.is_empty() {
            self.status = "Retro: nothing played yet - play on the keyboard / MIDI keys, then press RETRO".into();
            return;
        }
        let tracks = &self.project.arrangement.tracks;
        let lane = tracks.iter().position(|t| t.arm && t.kind == crate::arrangement::TrackKind::Midi).or_else(|| (tracks.get(self.arr.sel_lane).map(|t| t.kind) == Some(crate::arrangement::TrackKind::Midi)).then_some(self.arr.sel_lane));
        let lane = match lane {
            Some(l) => l,
            None => {
                self.project.arrangement.add_track(crate::arrangement::TrackKind::Midi);
                self.project.arrangement.tracks.len() - 1
            }
        };
        let track = self.live_track;
        let notes = std::mem::take(&mut self.retro);
        match self.project.retro_to_clip(lane, track, &notes) {
            Some(_) => {
                self.arr.sel_lane = lane;
                self.status = format!("Retro: {} note(s) became a clip on '{}'", notes.len(), self.project.arrangement.tracks[lane].name);
            }
            None => {
                self.retro = notes;
                self.status = "Retro: those notes do not fit in a clip".into();
            }
        }
    }

    fn live_note_on(&mut self, pitch: u8, vel: u8, send: bool) {
        // `send` is true for the computer keyboard (the MIDI thread plays device notes itself)
        let allowed = self.input_allows(send);
        if allowed {
            self.retro_held.insert(pitch, (self.started.elapsed().as_secs_f64(), vel));
        }
        // an empty MIDI track makes no sound until the user drops a plugin on it
        if send && self.project.arrangement.silent(self.arr.sel_lane) {
            let empty = self.project.arrangement.tracks.get(self.arr.sel_lane).map_or(false, |t| !t.mute);
            self.status = if empty { "This track has no plugin yet: drop one from Browser - Plugins onto the track, or use the Devices panel on the right.".into() } else { "This track is muted.".into() };
        }
        if send && allowed && !self.project.arrangement.silent(self.arr.sel_lane) {
            self.shared.send(Cmd::NoteOn { track: self.live_track, pitch, vel });
        }
        if allowed && self.record && self.rec_mode != 1 && self.playing() {
            if let Some((cur, pos)) = self.rec_target() {
                let pat = &mut self.project.patterns[cur];
                let len = pat.len_ticks();
                let start = ((pos + TICKS_PER_STEP / 2) / TICKS_PER_STEP * TICKS_PER_STEP) % len;
                pat.notes.push(Note { start, len: TICKS_PER_STEP, pitch, vel, track: self.live_track, slide: false, mute: false, pan: 0, chance: 100 });
                self.rec_held.insert(pitch, (start, pos));
            }
        }
    }

    fn live_note_off(&mut self, pitch: u8, send: bool) {
        if let Some((t, vel)) = self.retro_held.remove(&pitch) {
            let now = self.started.elapsed().as_secs_f64();
            self.retro.push(crate::project::RetroNote { t, len_s: (now - t).max(0.05), pitch, vel });
            // only the last two minutes / 600 notes are kept
            self.retro.retain(|n| now - n.t < 120.0);
            if self.retro.len() > 600 {
                self.retro.remove(0);
            }
        }
        if send {
            self.shared.send(Cmd::NoteOff { track: self.live_track, pitch });
        }
        if let Some((start, pos0)) = self.rec_held.remove(&pitch) {
            let Some((cur, pos)) = self.rec_target() else { return };
            let pat = &mut self.project.patterns[cur];
            let plen = pat.len_ticks();
            let dt = (pos + plen - pos0) % plen;
            let l = ((dt + 6) / 12 * 12).max(TICKS_PER_STEP).min(plen - start);
            let track = self.live_track;
            if let Some(n) = pat.notes.iter_mut().rev().find(|n| n.start == start && n.pitch == pitch && n.track == track) {
                n.len = l;
            }
        }
    }

    fn handle_keys(&mut self, ctx: &egui::Context) {
        while let Ok(ev) = self.midi.rx.try_recv() {
            if ev.on {
                self.live_note_on(ev.pitch, ev.vel, false);
            } else {
                self.live_note_off(ev.pitch, false);
            }
        }
        // the window lost focus: key releases never arrive, so let go of every held piano key (no stuck notes)
        if !ctx.input(|i| i.focused) && !self.key_held.is_empty() {
            let held: Vec<u8> = self.key_held.drain().map(|(_, p)| p).collect();
            for p in held {
                self.live_note_off(p, true);
            }
        }
        // F1..F12: windows / views (work even while a text field has focus)
        let fkeys = [Key::F1, Key::F2, Key::F3, Key::F4, Key::F5, Key::F6, Key::F7, Key::F8, Key::F9, Key::F10, Key::F11, Key::F12];
        // Ctrl+F5: save, rebuild with cargo and relaunch
        if ctx.input(|i| i.modifiers.command && i.key_pressed(Key::F5)) {
            self.restart(ctx);
            return;
        }
        for (n, k) in fkeys.iter().enumerate() {
            if ctx.input(|i| i.key_pressed(*k) && !i.modifiers.command) {
                self.function_key(n + 1);
            }
        }
        // Releases must reach the instrument even after a text field takes focus,
        // or while Ctrl/Alt is held. Only new piano presses are focus-dependent.
        let released: Vec<Key> = self.key_held.keys().copied()
            .filter(|key| !ctx.input(|i| i.key_down(*key))).collect();
        for key in released {
            if let Some(pitch) = self.key_held.remove(&key) {
                self.live_note_off(pitch, true);
            }
        }
        if ctx.wants_keyboard_input() {
            return;
        }
        // the Playlist's key commands (select all, duplicate, quantize, repeat...) are off while the piano roll has the keyboard
        let arr_keys = self.main == Tab::Arrangement && !roll_ui::roll_has_focus(ctx);
        // one window at a time: while the piano roll has the keyboard the Playlist keeps no selected clips, and the other way round
        if roll_ui::roll_has_focus(ctx) {
            if !self.arr.sel.is_empty() {
                self.arr.sel.clear();
                self.arr.sel_clip = None;
            }
        } else if !self.arr.sel.is_empty() {
            self.roll.clear_selection();
        }
        let events = ctx.input(|i| i.events.clone());
        for ev in events {
            let egui::Event::Key { key, pressed, repeat, modifiers, .. } = ev else { continue };
            if repeat {
                continue;
            }
            if modifiers.command {
                if pressed {
                    match key {
                        Key::S => self.save_project(modifiers.shift),
                        Key::Z => {
                            if modifiers.shift {
                                self.redo()
                            } else {
                                self.undo()
                            }
                        }
                        Key::Y => self.redo(),
                        Key::O => self.open_project(),
                        Key::N => self.new_project(),
                        Key::E => self.function_key(12),
                        // Ableton: new tracks / clips, duplicate, export, browser
                        Key::T => self.project.arrangement.add_track(if modifiers.shift { crate::arrangement::TrackKind::Audio } else { crate::arrangement::TrackKind::Midi }),
                        Key::M if modifiers.shift => arrangement_ui::new_midi_clip(&mut self.project, &mut self.arr),
                        Key::K if modifiers.shift => self.retro_capture(),
                        Key::D if arr_keys => arrangement_ui::duplicate_selection(&mut self.project, &mut self.arr),
                        // Ctrl+Shift+Up / Down: the selected clips a whole octave up / down
                        Key::ArrowUp | Key::ArrowDown if arr_keys && modifiers.shift => {
                            let ids = self.arr.selection();
                            let d = if key == Key::ArrowUp { 12 } else { -12 };
                            let n = arrangement_ui::transpose_clips(&mut self.project, &ids, d);
                            self.status = format!("{n} clip(s) {d:+} semitones");
                        }
                        // Ctrl+K: cut the selected clips at the playhead (all clips under it when none is selected)
                        // Ctrl+M: mute / unmute the selected clips (FL's clip mute)
                        Key::M if arr_keys && !modifiers.shift => {
                            let ids = self.arr.selection();
                            let all_muted = ids.iter().all(|&i| self.project.arrangement.clips.get(i).map_or(true, |c| c.mute));
                            for i in ids {
                                if let Some(c) = self.project.arrangement.clips.get_mut(i) {
                                    c.mute = !all_muted;
                                }
                            }
                        }
                        Key::K if arr_keys && !modifiers.shift => {
                            let at = self.shared.position.load(Ordering::Relaxed);
                            let mut ids = self.arr.selection();
                            if ids.is_empty() {
                                ids = (0..self.project.arrangement.clips.len()).collect();
                            }
                            let n = arrangement_ui::slice_at(&mut self.project, &ids, at);
                            self.status = if n == 0 { "Nothing to cut at the playhead".into() } else { format!("Cut {n} clip(s) at the playhead") };
                        }
                        Key::R if arr_keys && !modifiers.shift => {
                            let ids = self.arr.selection();
                            let c = arrangement_ui::repeat_clips(&mut self.project, &ids, 2);
                            self.status = format!("{c} clip(s) repeated x2");
                        }
                        Key::Q if arr_keys => {
                            let ids = self.arr.selection();
                            let c = arrangement_ui::quantize_clips(&mut self.project, &ids, self.arr.snap);
                            self.status = format!("{c} clip(s) quantized");
                        }
                        Key::L if arr_keys => {
                            let (n, left) = plugins_ui::route_tracks_to_mixer(&mut self.project);
                            self.status = if n == 0 && left == 0 { "Every track already has a mixer insert".into() } else if left > 0 { format!("{n} track(s) sent to the mixer, {left} left without an insert (the mixer has 8)") } else { format!("{n} track(s) sent to the mixer") };
                        }
                        Key::A if arr_keys => self.arr.sel = (0..self.project.arrangement.clips.len()).collect(),
                        Key::R if modifiers.shift => {
                            self.export.bars = export::default_bars(&self.project);
                            self.export.open = true;
                        }
                        Key::B if modifiers.alt => self.browser_open = !self.browser_open,
                        _ => {}
                    }
                }
                continue;
            }
            if pressed && key == Key::Space {
                self.toggle_play();
                continue;
            }
            // Playlist: arrows nudge the selected clips by the grid, Alt+2 / 3 / 4 / 6 / 8 cut them into equal parts
            if pressed && arr_keys {
                let ids = self.arr.selection();
                if !ids.is_empty() {
                    let snap = self.arr.snap.max(6) as i64;
                    match key {
                        Key::ArrowLeft => {
                            arrangement_ui::nudge_clips(&mut self.project, &ids, -snap);
                        }
                        Key::ArrowRight => {
                            arrangement_ui::nudge_clips(&mut self.project, &ids, snap);
                        }
                        _ => {}
                    }
                    // Shift+Up / Down: the selected clips one semitone higher / lower, with Ctrl a whole octave
                    if modifiers.shift && !modifiers.alt {
                        let step = 1;
                        match key {
                            Key::ArrowUp => {
                                let n = arrangement_ui::transpose_clips(&mut self.project, &ids, step);
                                self.status = format!("{n} clip(s) +{step} semitone(s)");
                            }
                            Key::ArrowDown => {
                                let n = arrangement_ui::transpose_clips(&mut self.project, &ids, -step);
                                self.status = format!("{n} clip(s) -{step} semitone(s)");
                            }
                            _ => {}
                        }
                    }
                    if modifiers.alt {
                        let parts = match key {
                            Key::Num2 => 2,
                            Key::Num3 => 3,
                            Key::Num4 => 4,
                            Key::Num6 => 6,
                            Key::Num8 => 8,
                            _ => 0,
                        };
                        if parts > 0 {
                            let n = arrangement_ui::slice_into_parts(&mut self.project, &ids, parts, self.arr.snap);
                            self.status = format!("{n} cut(s): clips cut into {parts} parts");
                            self.arr.sel.clear();
                            continue;
                        }
                    }
                }
            }
            // Ableton: Tab flips Playlist <-> Session; FL / Ableton: Home rewinds, Delete removes the selected clip
            if pressed && key == Key::Tab && matches!(self.main, Tab::Arrangement | Tab::Session) && !roll_ui::roll_has_focus(ctx) {
                self.tab = if self.main == Tab::Arrangement { Tab::Session } else { Tab::Arrangement };
                continue;
            }
            if pressed && key == Key::Home {
                self.return_to_start(ctx, false);
                continue;
            }
            if pressed && matches!(key, Key::Delete | Key::Backspace) && arr_keys {
                arrangement_ui::delete_selection(&mut self.project, &mut self.arr);
            }
            if pressed && key == Key::Minus {
                self.octave = (self.octave - 1).max(0);
            } else if pressed && key == Key::Equals {
                self.octave = (self.octave + 1).min(8);
            } else if let Some(semi) = key_semitone(key) {
                if pressed && !self.key_held.contains_key(&key) {
                    let pitch = (12 * (self.octave + 1) + semi).clamp(0, 127) as u8;
                    self.key_held.insert(key, pitch);
                    self.live_note_on(pitch, 100, true);
                } else if !pressed {
                    if let Some(p) = self.key_held.remove(&key) {
                        self.live_note_off(p, true);
                    }
                }
            }
        }
    }

    /// project name + quick stats, pinned under the browser
    fn project_info(&self, ui: &mut egui::Ui) {
        let a = &self.project.arrangement;
        let clips = a.clips.len();
        ui.label(RichText::new(&self.project.name).strong().color(TEXT()));
        ui.label(RichText::new(format!("{:.1} BPM  |  {} tracks  |  {} clips  |  {} patterns", self.project.bpm, a.tracks.len(), clips, self.project.patterns.len())).size(10.0).color(DIM()));
        ui.label(RichText::new(self.path.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "unsaved".into())).size(10.0).color(DIM()));
    }

    /// Small status widgets for the header: audio load, memory of the program, sample rate, voices and the clock.
    fn sys_widgets(&mut self, ui: &mut egui::Ui) {
        let cpu = f32::from_bits(self.shared.cpu.load(Ordering::Relaxed)).clamp(0.0, 100.0);
        let ram = process_memory_mb();
        let col = if cpu > 80.0 { RED } else if cpu > 50.0 { AMBER } else { GREEN };
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            ui.label(RichText::new("CPU").size(10.0).color(DIM()));
            let (rect, _) = ui.allocate_exact_size(vec2(70.0, 10.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, 1.0, INSET());
            ui.painter().rect_filled(egui::Rect::from_min_size(rect.min, vec2(rect.width() * cpu / 100.0, rect.height())), 1.0, col);
            ui.label(RichText::new(format!("{cpu:.0}%")).monospace().size(10.0).color(TEXT()));
        })
        .response
        .on_hover_text("Audio engine load: time spent rendering a buffer / the time that buffer lasts");
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            ui.label(RichText::new("RAM").size(10.0).color(DIM()));
            ui.label(RichText::new(format!("{ram:.0} MB")).monospace().size(12.0).color(TEXT()));
        })
        .response
        .on_hover_text("Memory used by this program");
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            ui.label(RichText::new("AUDIO").size(10.0).color(DIM()));
            ui.label(RichText::new(format!("{} kHz", self.audio.sample_rate as f32 / 1000.0)).monospace().size(12.0).color(TEXT()));
        })
        .response
        .on_hover_text("Sample rate of the output device");
        let clips = self.project.arrangement.clips.len();
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            ui.label(RichText::new("PROJECT").size(10.0).color(DIM()));
            ui.label(RichText::new(format!("{} tr / {} clips", self.project.arrangement.tracks.len(), clips)).monospace().size(12.0).color(TEXT()));
        });
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));
    }

    /// The player of the song in the Playlist: the waveform of the whole arrangement (rendered in the background whenever it
    /// changes), a playhead that moves with the Playlist, Play / Stop, and a click anywhere to jump there.
    fn song_player(&mut self, ui: &mut egui::Ui) {
        if let Some(rx) = &self.song_rx {
            if let Ok(r) = rx.try_recv() {
                self.song_wave = Some(r);
                self.song_rx = None;
            }
        }
        let sig = song_signature(&self.project);
        if self.song_wave.as_ref().map(|w| w.0) != Some(sig) {
            if self.song_pending.map(|p| p.0) != Some(sig) {
                self.song_pending = Some((sig, web_time::Instant::now()));
            }
            if let Some((ps, t0)) = self.song_pending {
                if ps == sig && t0.elapsed().as_secs_f32() > 1.5 && self.song_rx.is_none() {
                    let mut proj = self.project.clone();
                    proj.song_mode = true;
                    let (tx, rx) = unbounded();
                    self.song_rx = Some(rx);
                    self.song_pending = None;
                    std::thread::spawn(move || {
                        let bars = proj.arrangement.song_end_bar().max(1);
                        let out = export::render(&proj, &ExportSettings { sample_rate: 22050, depth: BitDepth::Pcm16, bars, tail_secs: 0.0 });
                        let n = 480usize;
                        let frames = (out.len() / 2).max(1);
                        let mut peaks = vec![0.0f32; n];
                        for i in 0..out.len() / 2 {
                            let b = (i * n / frames).min(n - 1);
                            peaks[b] = peaks[b].max(out[2 * i].abs().max(out[2 * i + 1].abs()));
                        }
                        let _ = tx.send((sig, peaks));
                    });
                }
            }
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(400));
        }
        let song_end = self.project.arrangement.song_end_bar().max(1);
        let playing = self.shared.playing.load(Ordering::Relaxed);
        let pos = self.shared.position.load(Ordering::Relaxed);
        let in_song = self.project.song_mode;
        ui.horizontal(|ui| {
            ui.label(RichText::new("SONG PLAYER").strong().size(13.0));
            if ui.button(if playing && in_song { "Stop" } else { "Play" }).clicked() {
                if playing && in_song {
                    self.shared.send(Cmd::Stop);
                } else {
                    self.project.song_mode = true;
                    self.shared.send(Cmd::Play);
                }
            }
            let txt = if in_song && playing { format!("bar {} / {}", (pos / TICKS_PER_BAR + 1).min(song_end), song_end) } else { format!("{song_end} bars") };
            ui.label(RichText::new(txt).monospace().size(11.0).color(DIM()));
            if self.song_rx.is_some() || self.song_pending.is_some() {
                ui.label(RichText::new("updating...").size(10.0).color(LIGHT()));
            }
        });
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 84.0), egui::Sense::click_and_drag());
        let p = ui.painter_at(rect);
        p.rect_filled(rect, corner(), INSET());
        for b in (0..song_end).step_by(4) {
            let x = rect.left() + b as f32 / song_end as f32 * rect.width();
            p.line_segment([egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())], Stroke::new(1.0, border().linear_multiply(0.6)));
        }
        if let Some((_, peaks)) = &self.song_wave {
            let top = peaks.iter().cloned().fold(0.05f32, f32::max);
            let mid = rect.center().y;
            let n = peaks.len();
            for x in 0..rect.width() as usize {
                let v = peaks[(x * n / rect.width().max(1.0) as usize).min(n - 1)] / top;
                let h = v.min(1.0) * (rect.height() * 0.5 - 3.0);
                let t = x as f32 / rect.width();
                let col = lerp_color(Color32::from_rgb(0xFF, 0x4F, 0xC8), Color32::from_rgb(0x4D, 0xA3, 0xFF), t);
                p.line_segment([egui::pos2(rect.left() + x as f32, mid - h), egui::pos2(rect.left() + x as f32, mid + h)], Stroke::new(1.0, col.linear_multiply(0.85)));
            }
        } else {
            p.text(rect.center(), egui::Align2::CENTER_CENTER, "rendering the arrangement...", egui::FontId::proportional(12.0), LIGHT());
        }
        if in_song && playing {
            let f = (pos as f32 / (song_end * TICKS_PER_BAR) as f32).clamp(0.0, 1.0);
            let x = rect.left() + f * rect.width();
            p.line_segment([egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())], Stroke::new(1.5, CYAN()));
            ui.ctx().request_repaint();
        }
        if resp.clicked() || resp.dragged() {
            if let Some(pt) = resp.interact_pointer_pos() {
                let f = ((pt.x - rect.left()) / rect.width()).clamp(0.0, 0.999);
                self.project.song_mode = true;
                self.shared.send(Cmd::PlayFrom((f * (song_end * TICKS_PER_BAR) as f32) as u32));
            }
        }
        resp.on_hover_text("Click or drag to jump anywhere in the song");
    }

    fn header_brand(&mut self, ui: &mut egui::Ui) {
        // glass: a faint light fill and a light edge on the header buttons, a bit brighter under the pointer
        ui.scope(|ui| {
            let w = &mut ui.visuals_mut().widgets;
            for v in [&mut w.inactive, &mut w.hovered, &mut w.active, &mut w.open] {
                v.weak_bg_fill = Color32::TRANSPARENT;
                v.bg_fill = Color32::TRANSPARENT;
                v.bg_stroke = Stroke::new(1.0, TEXT().linear_multiply(0.38));
                v.rounding = egui::Rounding::ZERO;
            }
            ui.spacing_mut().button_padding = vec2(10.0, 5.0);
            self.toolbar(ui);
        });
    }

    fn transport(&mut self, ui: &mut egui::Ui) { transport_ui::show(self, ui); }

    /// F1..F12 like FL Studio: every view and tool has a function key.
    fn function_key(&mut self, n: usize) {
        match n {
            1 => self.show_help = !self.show_help,
            2 => self.open_inst(instruments_ui::Inst::Poly),
            3 => self.open_inst(instruments_ui::Inst::Bass),
            4 => self.open_inst(instruments_ui::Inst::Wavetable),
            5 => self.tab = Tab::Arrangement,
            6 => self.tab = Tab::Sequencer,
            7 => self.tab = Tab::PianoRoll,
            8 => self.browser_open = !self.browser_open,
            9 => self.tab = Tab::Mixer,
            10 => self.show_audio = !self.show_audio,
            11 => self.open_inst(instruments_ui::Inst::Analog),
            _ => {
                self.export.bars = export::default_bars(&self.project);
                self.export.open = true;
            }
        }
    }

    fn open_inst(&mut self, inst: instruments_ui::Inst) {
        self.tab = Tab::Instruments;
        self.inst = inst;
        self.live_track = inst.track();
        self.roll.track = inst.track();
    }

    /// FL-style toolbar: window buttons (with their F-keys) and the project menu.
    fn toolbar(&mut self, ui: &mut egui::Ui) {
        {
            ui.menu_button("File", |ui| {
                if ui.button("New").clicked() {
                    self.new_project();
                    ui.close_menu();
                }
                ui.menu_button("Add template to this playlist", |ui| {
                    egui::ScrollArea::vertical().id_salt("templates_add_menu").max_height(440.0).auto_shrink([true, true]).show(ui, |ui| {
                        for i in 0..crate::templates::TEMPLATES.len() {
                            if crate::templates::TEMPLATES[i].0.starts_with("Empty") {
                                continue;
                            }
                            if ui.button(crate::templates::TEMPLATES[i].0).on_hover_text("Its tracks and clips are added under your tracks").clicked() {
                                self.add_template(i);
                                ui.close_menu();
                            }
                        }
                    });
                });
                ui.menu_button("New project from template", |ui| {
                    egui::ScrollArea::vertical().id_salt("templates_menu").max_height(440.0).auto_shrink([true, true]).show(ui, |ui| {
                        for i in 0..crate::templates::TEMPLATES.len() {
                            if ui.button(crate::templates::TEMPLATES[i].0).clicked() {
                                self.load_template(i);
                                ui.close_menu();
                            }
                        }
                    });
                });
                if ui.button("Open...").clicked() {
                    self.open_project();
                    ui.close_menu();
                }
                ui.menu_button("Recent", |ui| {
                    if self.recent.is_empty() {
                        ui.label("No recent projects");
                    }
                    for p in self.recent.clone() {
                        let name = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                        if ui.button(name).on_hover_text(p.display().to_string()).clicked() {
                            self.open_path(p);
                            ui.close_menu();
                        }
                    }
                });
                if ui.button("Project hub...").clicked() {
                    self.hub_open = true;
                    self.hub_cache = None;
                    ui.close_menu();
                }
                if ui.button("Save to hub").on_hover_text("Save this project into the hub folder without asking for a place").clicked() {
                    self.save_to_hub();
                    ui.close_menu();
                }
                if ui.add_enabled(!self.undo.is_empty(), egui::Button::new("Undo  Ctrl+Z")).clicked() {
                    self.undo();
                    ui.close_menu();
                }
                if ui.add_enabled(!self.redo.is_empty(), egui::Button::new("Redo  Ctrl+Y")).clicked() {
                    self.redo();
                    ui.close_menu();
                }
                if ui.button("Save").clicked() {
                    self.save_project(false);
                    ui.close_menu();
                }
                if ui.button("Save as...").clicked() {
                    self.save_project(true);
                    ui.close_menu();
                }
                if ui.button("Export WAV (F12)").clicked() {
                    self.function_key(12);
                    ui.close_menu();
                }
            });
            ui.menu_button("Edit", |ui| {
                if ui.add_enabled(!self.undo.is_empty(), egui::Button::new("Undo  Ctrl+Z")).clicked() {
                    self.undo();
                    ui.close_menu();
                }
                if ui.add_enabled(!self.redo.is_empty(), egui::Button::new("Redo  Ctrl+Y")).clicked() {
                    self.redo();
                    ui.close_menu();
                }
                ui.separator();
                if ui.button(format!("Capture recent notes ({})", self.retro.len())).on_hover_text("Retrospective record · Ctrl+Shift+K").clicked() {
                    self.retro_capture();
                    ui.close_menu();
                }
            });
            ui.menu_button("View", |ui| {
                ui.checkbox(&mut self.browser_open, "Browser  F8");
                ui.separator();
                for (label, tab) in [("Playlist  F5", Tab::Arrangement), ("Channel Rack  F6", Tab::Sequencer), ("Piano Roll  F7", Tab::PianoRoll), ("Mixer  F9", Tab::Mixer), ("Session", Tab::Session)] {
                    if ui.button(label).clicked() {
                        self.hub_open = false;
                        self.tab = tab;
                        ui.close_menu();
                    }
                }
                ui.separator();
                if ui.button("Project hub...").clicked() {
                    self.hub_open = true;
                    self.hub_cache = None;
                    ui.close_menu();
                }
            });
            ui.menu_button("Options", |ui| {
                ui.set_min_width(240.0);
                ui.label(RichText::new("GROOVE").small().color(DIM()));
                ui.add(egui::Slider::new(&mut self.project.swing, 0.0..=1.0).text("Swing"));
                ui.checkbox(&mut self.project.metronome, "Metronome");
                ui.checkbox(&mut self.project.loop_on, "Loop the Song range");
                ui.separator();
                arrangement_ui::steps_selector(ui, &mut self.project);
                ui.separator();
                if ui.button("Audio / MIDI settings...").clicked() {
                    self.show_audio = true;
                    ui.close_menu();
                }
                ui.label(RichText::new("UI Scale").small().color(DIM()));
                let mut pct = ui_scale() as u32;
                if ui.add(egui::Slider::new(&mut pct, SCALE_MIN as u32..=SCALE_MAX as u32).suffix("%")).changed() {
                    set_ui_scale(pct as u8);
                    apply_scale(ui.ctx());
                }
                ui.label(RichText::new("Accent").small().color(DIM()));
                ui.horizontal_wrapped(|ui| {
                    for (i, n) in ACCENTS.iter().enumerate() {
                        if ui.selectable_label(accent() == i as u8, *n).clicked() {
                            set_accent(i as u8);
                            apply_visuals(ui.ctx());
                        }
                    }
                });
                let mut au = auto();
                if ui.checkbox(&mut au, "Auto theme (dark 19:00 - 07:00)").clicked() {
                    set_auto(au);
                    if au {
                        set_dark(night_now());
                        apply_visuals(ui.ctx());
                    }
                }
            });
            ui.menu_button("Tools", |ui| {
                if ui.button("Audio / MIDI settings...  F10").clicked() {
                    self.show_audio = true;
                    ui.close_menu();
                }
                if ui.button("Export WAV...  F12").clicked() {
                    self.function_key(12);
                    ui.close_menu();
                }
                if ui.button(format!("Capture recent notes ({})", self.retro.len())).clicked() {
                    self.retro_capture();
                    ui.close_menu();
                }
            });
            if toolbar_button(ui, "Browser", "F8", self.browser_open).clicked() {
                self.browser_open = !self.browser_open;
            }
            if toolbar_button(ui, "Help", "", self.show_help).clicked() {
                self.show_help = !self.show_help;
            }
        }
    }

    fn help_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_help;
        egui::Window::new("Keyboard shortcuts").open(&mut open).default_width(420.0).show(ctx, |ui| {
            ui.label(RichText::new(format!("Winteria DAW  Beta 1  (v{})  -  Winteria Studio", env!("CARGO_PKG_VERSION"))).strong().color(TEAL()));
            ui.label(RichText::new("Wave Audio Synthesis Digital - a DAW written from scratch: own audio engine, synths, sequencer, piano roll, playlist, mixer").size(11.0).color(DIM()));
            ui.add_space(6.0);
            let rows = [
                ("F1", "this help"),
                ("F2", "Poly Synth editor"),
                ("F3", "808 Bass editor"),
                ("F4", "Wavetable synth editor (Serum-style)"),
                ("F5", "Playlist / arrangement"),
                ("F6", "Step sequencer (channel rack)"),
                ("F7", "Piano roll"),
                ("F8", "show / hide the Browser"),
                ("F9", "Mixer"),
                ("F10", "Audio / MIDI settings"),
                ("F11", "Analog synth editor (Diva-style)"),
                ("F12", "Export WAV"),
                ("Space", "play / stop"),
                ("Z S X D C V G B H N J M", "play notes like FL (Q 2 W 3 E R 5 T 6 Y 7 U: octave above)"),
                ("- / =", "typing piano octave down / up"),
                ("Ctrl+S / Ctrl+Shift+S", "save / save as"),
                ("Ctrl+O / N", "open / new"),
                ("Ctrl+E or Ctrl+Shift+R", "export WAV (Ableton)"),
                ("Ctrl+T / Ctrl+Shift+T", "new MIDI / audio track (Ableton)"),
                ("Ctrl+Shift+M", "new MIDI clip on the selected track"),
                ("Playlist: Ctrl+K", "cut the selected clips at the playhead"),
                ("Playlist: Alt+2/3/4/6/8", "cut the selected clips into equal parts"),
                ("Playlist: Ctrl+R", "repeat the selected clips x2 (MIDI loops, audio is copied)"),
                ("Playlist: Ctrl+Q", "quantize the starts of the selected clips to the grid"),
                ("Playlist: arrows", "move the selected clips by the grid"),
                ("Playlist: Shift+click / box", "select several clips; Ctrl/Alt+drag copies them"),
                ("Ctrl+D", "duplicate the selected clip (Playlist, already built in)"),
                ("Delete", "delete the selected clip (Playlist)"),
                ("Tab", "Playlist <-> Session (Ableton)"),
                ("Home", "stop and go back to the start"),
                ("Ctrl+Alt+B", "show / hide the Browser (Ableton)"),
                ("Ctrl+Z / Ctrl+Y / Ctrl+Shift+Z", "undo / redo"),
                ("Piano roll", "Ctrl+A select all - Ctrl+C/X/V - Ctrl+D duplicate - Ctrl+Q quantize - arrows nudge - Delete"),
            ];
            // spreadsheet look: column letters, row numbers, a cell border around every value
            let line = Stroke::new(1.0, border());
            let head_fill = tint(SIDE(), TEAL(), 0.30);
            let cell = |ui: &mut egui::Ui, w: f32, fill: egui::Color32, text: RichText| {
                egui::Frame::none().fill(fill).stroke(line).inner_margin(egui::Margin::symmetric(6.0, 3.0)).show(ui, |ui| {
                    ui.set_min_width(w);
                    ui.set_max_width(w);
                    ui.add(egui::Label::new(text).truncate());
                });
            };
            egui::ScrollArea::vertical().max_height(560.0).show(ui, |ui| {
                ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                egui::Grid::new("keys").num_columns(3).spacing([0.0, 0.0]).show(ui, |ui| {
                    cell(ui, 28.0, head_fill, RichText::new("").strong());
                    cell(ui, 210.0, head_fill, RichText::new("A   Shortcut").strong().color(TEAL()));
                    cell(ui, 460.0, head_fill, RichText::new("B   Action").strong().color(TEAL()));
                    ui.end_row();
                    for (n, (k, d)) in rows.into_iter().enumerate() {
                        let fill = if n % 2 == 0 { PANEL2() } else { BG() };
                        cell(ui, 28.0, head_fill, RichText::new(format!("{}", n + 1)).size(10.5).color(DIM()));
                        cell(ui, 210.0, fill, RichText::new(k).strong().color(GOLD()));
                        cell(ui, 460.0, fill, RichText::new(d));
                        ui.end_row();
                    }
                });
            });
        });
        self.show_help = open;
    }

    fn audio_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_audio;
        egui::Window::new("Audio / MIDI settings").open(&mut open).default_width(380.0).show(ctx, |ui| {
            let mut reopen = false;
            ui.horizontal(|ui| {
                ui.label("Output device");
                egui::ComboBox::from_id_salt("dev").width(260.0).selected_text(self.dev_sel.clone()).show_ui(ui, |ui| {
                    for d in self.audio.devices.clone() {
                        if ui.selectable_value(&mut self.dev_sel, d.clone(), d).clicked() {
                            reopen = true;
                        }
                    }
                });
                if ui.button("Rescan").clicked() {
                    self.audio.refresh_devices();
                }
            });
            ui.horizontal(|ui| {
                ui.label("Sample rate");
                egui::ComboBox::from_id_salt("sr").selected_text(format!("{} Hz", self.rate_sel)).show_ui(ui, |ui| {
                    for r in self.audio.rates.clone() {
                        if ui.selectable_value(&mut self.rate_sel, r, format!("{r} Hz")).clicked() {
                            reopen = true;
                        }
                    }
                });
                ui.label("Buffer");
                egui::ComboBox::from_id_salt("buf").selected_text(if self.buffer_sel == 0 { "driver default".to_string() } else { format!("{} frames", self.buffer_sel) }).show_ui(ui, |ui| {
                    for b in [0u32, 64, 128, 256, 512, 1024] {
                        let label = if b == 0 { "driver default".to_string() } else { format!("{b} frames") };
                        if ui.selectable_value(&mut self.buffer_sel, b, label).clicked() {
                            reopen = true;
                        }
                    }
                });
            });
            if reopen {
                self.audio.open(&self.dev_sel, self.rate_sel, self.buffer_sel);
                self.rate_sel = self.audio.sample_rate;
                self.status = match &self.audio.error {
                    Some(e) => e.clone(),
                    None => format!("Audio: {} @ {} Hz", self.audio.device_name, self.audio.sample_rate),
                };
            }
            if let Some(e) = &self.audio.error {
                ui.colored_label(RED, e);
            } else {
                let lat = if self.audio.buffer_frames > 0 { format!("{:.1} ms", self.audio.buffer_frames as f32 / self.audio.sample_rate as f32 * 1000.0) } else { "driver default".into() };
                ui.label(RichText::new(format!("Running: {} @ {} Hz, buffer latency {}, callbacks {}", self.audio.device_name, self.audio.sample_rate, lat, self.shared.callbacks.load(Ordering::Relaxed))).color(GREEN));
            }
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("MIDI input");
                let sel = self.midi.selected.clone().unwrap_or_else(|| "none".into());
                egui::ComboBox::from_id_salt("midi").width(240.0).selected_text(sel).show_ui(ui, |ui| {
                    if ui.selectable_label(self.midi.selected.is_none(), "none").clicked() {
                        self.midi.disconnect();
                    }
                    for p in self.midi.ports.clone() {
                        if ui.selectable_label(self.midi.selected.as_deref() == Some(&p), &p).clicked() {
                            self.midi.connect(&p);
                        }
                    }
                });
                if ui.button("Rescan").clicked() {
                    self.midi.refresh();
                }
            });
            if let Some(e) = &self.midi.error {
                ui.colored_label(RED, e);
            }
            if self.midi.ports.is_empty() {
                ui.label(RichText::new("No MIDI devices found. The computer keyboard works as a piano like in FL (Z S X D C V... row, Q 2 W 3 E... row above, - / = octave).").color(DIM()));
            }
        });
        self.show_audio = open;
    }

    /// Record button + mode: audio from the input device is captured while recording and put on an audio track when it stops.
    fn audio_recording(&mut self) {
        let want = self.record && self.rec_mode != 0;
        if want && !self.recorder.active() {
            #[cfg(target_os = "android")]
            match self.android_updater.as_ref().and_then(|android| android.microphone_permission_state().ok()) {
                Some(1) => {}
                Some(0) => return,
                _ => {
                    self.record = false;
                    self.status = "Microphone permission is required for voice recording".into();
                    return;
                }
            }
            match self.recorder.start() {
                Ok(()) => {
                    self.rec_from = self.shared.position.load(Ordering::Relaxed);
                    self.status = "Recording audio...".into();
                }
                Err(e) => {
                    self.status = format!("Audio recording failed: {e}");
                    if self.rec_mode == 1 {
                        self.record = false;
                    }
                }
            }
        } else if !want && self.recorder.active() {
            if let Some((data, rate)) = self.recorder.stop() {
                self.finish_recording(data, rate);
            }
        }
    }

    fn finish_recording(&mut self, data: Vec<f32>, rate: u32) {
        use crate::arrangement::{Clip, TrackKind};
        if data.len() < 2 || rate == 0 {
            return;
        }
        let dir = hub_dir().join("Recordings");
        let _ = std::fs::create_dir_all(&dir);
        let mut n = 1;
        let path = loop {
            let p = dir.join(format!("Recording {n}.wav"));
            if !p.exists() {
                break p;
            }
            n += 1;
        };
        if let Err(e) = crate::audio_rec::write_wav(&path, &data, rate) {
            self.status = format!("Could not save the recording: {e}");
            return;
        }
        let Some(idx) = browser::pool_for(&mut self.project, &mut self.lib, &path) else { return };
        // the armed audio track takes it, else the selected audio track, else a new one
        let arr = &self.project.arrangement;
        let lane = arr
            .tracks
            .iter()
            .position(|t| t.kind == TrackKind::Audio && t.arm)
            .or_else(|| arr.tracks.get(self.arr.sel_lane).filter(|t| t.kind == TrackKind::Audio).map(|_| self.arr.sel_lane));
        let name = format!("Recording {n}");
        let lane = match lane.or_else(|| browser::resolve_lane(&mut self.project, usize::MAX, TrackKind::Audio, &name)) {
            Some(l) => l,
            None => return,
        };
        let secs = data.len() as f32 / rate as f32;
        let ticks = ((secs * self.project.bpm / 60.0 * crate::sequencer::PPQ as f32).round() as u32).max(24);
        let mut c = Clip::new(idx, lane, self.rec_from, ticks, true);
        // recorded over something: the take goes to a fresh track instead of failing silently
        if !self.project.arrangement.can_place(&c, None) {
            if let Some(l2) = browser::resolve_lane(&mut self.project, usize::MAX, TrackKind::Audio, &name) {
                c.lane = l2;
            }
        }
        if self.project.arrangement.can_place(&c, None) {
            self.project.arrangement.clips.push(c);
            self.project.song_mode = true;
            self.arr.sel_lane = c.lane;
            self.arr.sel_clip = Some(self.project.arrangement.clips.len() - 1);
            self.status = format!("Recorded {secs:.1} s -> {}", path.display());
        }
    }

    /// The file that was just exported lands on a new audio track of the Playlist, from the start.
    fn import_render(&mut self, path: &std::path::Path) {
        use crate::arrangement::{Clip, TrackKind};
        let Some(idx) = browser::pool_for(&mut self.project, &mut self.lib, path) else { return };
        let secs = self.project.audio_pool[idx].data.as_ref().map_or(0.0, |d| d.mono.len() as f32 / d.rate.max(1) as f32);
        let name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        if let Some(lane) = browser::resolve_lane(&mut self.project, usize::MAX, TrackKind::Audio, &name) {
            let ticks = ((secs * self.project.bpm / 60.0 * crate::sequencer::PPQ as f32).round() as u32).max(24);
            let c = Clip::new(idx, lane, 0, ticks, true);
            if self.project.arrangement.can_place(&c, None) {
                self.project.arrangement.clips.push(c);
                self.export.result.push_str("  (added to the Playlist as a new track)");
            }
        }
    }

    fn export_window(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.export.rx {
            if let Ok(r) = rx.try_recv() {
                self.export.result = match r {
                    Ok(m) => format!("Verified {m}"),
                    Err(e) => format!("EXPORT FAILED: {e}"),
                };
                if self.export.result.starts_with("Verified") {
                    if let Some(path) = self.export.path.take() {
                        self.import_render(&path);
                    }
                    // done: the card goes away, the result stays in the status line
                    self.export.open = false;
                }
                self.status = self.export.result.clone();
                self.export.rx = None;
            }
        }
        if self.export.rx.is_some() {
            splash(ctx, "export_splash", "Your song is being rendered to a WAV file");
            ctx.request_repaint();
        }
        let mut open = self.export.open;
        // the same dim full-window backdrop as the render splash, and the settings in a card in the middle of it (no title bar)
        if open && self.export.rx.is_none() {
            let screen = ctx.screen_rect();
            egui::Area::new(egui::Id::new("export_backdrop")).order(egui::Order::Middle).fixed_pos(screen.min).show(ctx, |ui| {
                let (rect, r) = ui.allocate_exact_size(screen.size(), egui::Sense::click());
                ui.painter().rect_filled(rect, 0.0, Color32::from_black_alpha(190));
                if r.clicked() {
                    open = false;
                }
            });
        }
        egui::Window::new("Export WAV").title_bar(false).resizable(false).collapsible(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).default_width(380.0).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Export WAV").size(20.0).family(egui::FontFamily::Name("serif".into())).color(TEXT()));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("x").on_hover_text("Close").clicked() {
                        open = false;
                    }
                });
            });
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Render");
                let before = self.export.song;
                ui.selectable_value(&mut self.export.song, true, "Song").on_hover_text("The whole Playlist arrangement");
                ui.selectable_value(&mut self.export.song, false, "Pattern").on_hover_text("Only the selected pattern, looped");
                if before != self.export.song {
                    let mut tmp = self.project.clone();
                    tmp.song_mode = self.export.song;
                    self.export.bars = export::default_bars(&tmp);
                }
            });
            ui.horizontal(|ui| {
                ui.label("Sample rate");
                for r in [44100u32, 48000, 96000] {
                    ui.selectable_value(&mut self.export.rate, r, format!("{r}"));
                }
            });
            ui.horizontal(|ui| {
                ui.label("Bit depth");
                for d in [BitDepth::Pcm16, BitDepth::Pcm24, BitDepth::Float32] {
                    ui.selectable_value(&mut self.export.depth, d, d.label());
                }
            });
            ui.horizontal(|ui| {
                ui.label("Length (bars)");
                ui.add(egui::DragValue::new(&mut self.export.bars).range(1..=512));
                if ui.button("Auto").clicked() {
                    self.export.bars = export::default_bars(&self.project);
                }
                ui.label("Tail (s)");
                ui.add(egui::DragValue::new(&mut self.export.tail).range(0.0..=10.0).speed(0.1));
            });
            // which tracks go into the file: everything, or only the ticked MIDI / audio tracks (the others are silent in the render)
            let n_tracks = self.project.arrangement.tracks.len();
            self.export.pick.resize(n_tracks, true);
            ui.checkbox(&mut self.export.only_picked, "Render only the tracks I pick").on_hover_text("The tracks that are not ticked stay silent in the file; the Playlist itself is not changed");
            if self.export.only_picked {
                ui.horizontal(|ui| {
                    let kinds: Vec<bool> = self.project.arrangement.tracks.iter().map(|t| t.kind == crate::arrangement::TrackKind::Audio).collect();
                    if ui.small_button("All").clicked() {
                        self.export.pick.iter_mut().for_each(|b| *b = true);
                    }
                    if ui.small_button("None").clicked() {
                        self.export.pick.iter_mut().for_each(|b| *b = false);
                    }
                    if ui.small_button("Only MIDI").clicked() {
                        for (b, a) in self.export.pick.iter_mut().zip(&kinds) {
                            *b = !*a;
                        }
                    }
                    if ui.small_button("Only audio").clicked() {
                        for (b, a) in self.export.pick.iter_mut().zip(&kinds) {
                            *b = *a;
                        }
                    }
                });
                egui::ScrollArea::vertical().max_height(150.0).id_salt("export_tracks").show(ui, |ui| {
                    for l in 0..n_tracks {
                        let t = &self.project.arrangement.tracks[l];
                        let kind = if t.kind == crate::arrangement::TrackKind::Audio { "AUDIO" } else { "MIDI" };
                        let label = RichText::new(format!("{kind}  {}", t.name)).color(lane_col(l));
                        ui.checkbox(&mut self.export.pick[l], label);
                    }
                });
                if !self.export.pick.iter().any(|b| *b) {
                    ui.label(RichText::new("No track is ticked: the file would be silent.").color(AMBER));
                }
            }
            ui.label(RichText::new(if self.export.song { "Renders the Playlist arrangement." } else { "Renders the current pattern, looped for the chosen length." }).color(DIM()));
            let busy = self.export.rx.is_some();
            if ui.add_enabled(!busy, egui::Button::new("Render and save...")).clicked() {
                if let Some(path) = rfd::FileDialog::new().add_filter("WAV audio", &["wav"]).set_file_name(format!("{}.wav", self.project.name)).save_file() {
                    let s = ExportSettings { sample_rate: self.export.rate, depth: self.export.depth, bars: self.export.bars, tail_secs: self.export.tail };
                    let mut proj = self.project.clone();
                    proj.song_mode = self.export.song;
                    if self.export.only_picked {
                        proj.render_only_tracks(&self.export.pick);
                    }
                    let (tx, rx) = unbounded();
                    self.export.rx = Some(rx);
                    self.export.path = Some(path.clone());
                    self.export.result = "Rendering...".into();
                    std::thread::spawn(move || {
                        let _ = tx.send(export::export(&proj, &s, &path).map(|m| format!("{} -> {}", m, path.display())));
                    });
                }
            }
            if busy {
                ui.spinner();
            }
            if !self.export.result.is_empty() {
                let bad = self.export.result.starts_with("EXPORT FAILED");
                ui.label(RichText::new(&self.export.result).color(if bad { RED } else { GREEN }));
            }
        });
        self.export.open = open;
    }
}

fn meter_h(ui: &mut egui::Ui, lr: [f32; 2]) {
    let (rect, _) = ui.allocate_exact_size(vec2(90.0, 28.0), egui::Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 0.0, INSET());
    for (i, v) in lr.iter().enumerate() {
        let frac = ((db(*v) + 60.0) / 60.0).clamp(0.0, 1.0);
        let r = egui::Rect::from_min_size(pos2(rect.left() + 2.0, rect.top() + 3.0 + i as f32 * 12.0), vec2((rect.width() - 4.0) * frac, 9.0));
        let col = if frac > 0.97 { RED } else { lerp_color(TEAL(), TEAL_HI(), frac) };
        p.rect_filled(r, 0.0, col);
    }
}

/// The computer keyboard as a piano, laid out like FL Studio: the lower row Z S X D C V G B H N J M , L . ; / starts at the
/// base octave, the upper row Q 2 W 3 E R 5 T 6 Y 7 U I 9 O 0 P continues one octave above.
fn key_semitone(k: Key) -> Option<i32> {
    Some(match k {
        Key::Z => 0,
        Key::S => 1,
        Key::X => 2,
        Key::D => 3,
        Key::C => 4,
        Key::V => 5,
        Key::G => 6,
        Key::B => 7,
        Key::H => 8,
        Key::N => 9,
        Key::J => 10,
        Key::M => 11,
        Key::Comma => 12,
        Key::L => 13,
        Key::Period => 14,
        Key::Semicolon => 15,
        Key::Slash => 16,
        Key::Q => 12,
        Key::Num2 => 13,
        Key::W => 14,
        Key::Num3 => 15,
        Key::E => 16,
        Key::R => 17,
        Key::Num5 => 18,
        Key::T => 19,
        Key::Num6 => 20,
        Key::Y => 21,
        Key::Num7 => 22,
        Key::U => 23,
        Key::I => 24,
        Key::Num9 => 25,
        Key::O => 26,
        Key::Num0 => 27,
        Key::P => 28,
        _ => return None,
    })
}

impl eframe::App for VinteriaApp {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        // closing with unsaved work keeps a copy in the Backups folder
        self.backup_if_dirty();
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let frame_no = ctx.cumulative_pass_nr();
        if frame_no < 4 {
            crate::boot_log(&format!("frame {frame_no}"));
        } else if frame_no == 4 {
            crate::boot_done();
        }
        #[cfg(target_os = "android")]
        if self.splash_ui(ctx) {
            return;
        }
        paint_glass_background(ctx);
        self.audio.keep_alive();
        crate::silence_system_beeps();
        self.check_update(ctx);
        self.toasts_ui(ctx);
        #[cfg(not(target_arch = "wasm32"))]
        if self.render_overlay(ctx) {
            return;
        }
        // rolling autosave of unsaved work (not while playing: writing a big file must not touch the audio)
        if self.last_autosave.elapsed() > std::time::Duration::from_secs(90) {
            self.last_autosave = web_time::Instant::now();
            if self.dirty() && !self.playing() {
                let _ = self.project.save(&backup_dir().join("_autosave.wd"));
            }
        }
        // MIDI tracks always sit above the audio tracks: follow the selection and the open editor to the new lane numbers
        if let Some(map) = self.project.group_tracks() {
            if let Some(&n) = map.get(self.arr.sel_lane) {
                self.arr.sel_lane = n;
            }
            let id = devices_ui::dock_editor_id();
            if let Some(l) = ctx.data(|d| d.get_temp::<usize>(id)) {
                if let Some(&n) = map.get(l) {
                    ctx.data_mut(|d| d.insert_temp(id, n));
                }
            }
        }
        // every track with a built-in synth plays its own copy of the plugin's settings
        self.project.adopt_track_params();
        // Reclaim retired snapshots on the UI thread, including on mobile.
        self.shared.drain_garbage();
        // phones get their own layout (Pulse style); the PC keeps the AOS desktop
        if self.is_mobile() {
            self.hub_open = false;
            #[cfg(not(target_arch = "wasm32"))]
            self.audio_recording();
            #[cfg(target_arch = "wasm32")]
            if let Some(voice) = crate::web::take_voice_result() {
                if self.rec_mode == 1 {
                    self.record = false;
                }
                match voice {
                    Ok(path) => {
                        use crate::arrangement::TrackKind;
                        let lane = Some(self.voice_lane)
                            .filter(|&lane| self.project.arrangement.tracks.get(lane).is_some_and(|track| track.kind == TrackKind::Audio))
                            .or_else(|| self.project.arrangement.tracks.iter().position(|track| track.kind == TrackKind::Audio))
                            .unwrap_or(self.project.arrangement.tracks.len());
                        let before = self.project.arrangement.clips.len();
                        let bpm = self.project.bpm;
                        let item = browser::DragItem::Sample(path.clone());
                        self.lib.message.clear();
                        let placed_lane = arrangement_ui::drop_item(&mut self.project, &mut self.lib, &item, lane, self.rec_from, bpm);
                        let index = if self.project.arrangement.clips.len() > before {
                            Ok(before)
                        } else {
                            self.lib.insert_audio_clip(&mut self.project, &path, placed_lane, 1)
                        };
                        match index {
                            Ok(index) => {
                                let clip = self.project.arrangement.clips[index];
                                self.arr.sel_lane = clip.lane;
                                self.arr.sel_clip = Some(index);
                                ctx.data_mut(|d| {
                                    d.insert_temp(egui::Id::new("m_playlist_reveal_tick"), clip.start);
                                    d.insert_temp(egui::Id::new("m_playlist_reveal_track"), clip.lane);
                                });
                                self.status = "Voice recording added to Playlist".into();
                            }
                            Err(error) => self.status = error,
                        }
                    }
                    Err(error) => self.status = error,
                }
            }
            self.mobile_ui(ctx);
            self.publish_edits();
            return;
        }
        // the selected Playlist track is the one whose sampler plays live notes
        if self.arr.sel_lane != self.sent_lane {
            self.sent_lane = self.arr.sel_lane;
            self.shared.send(Cmd::SetLiveLane(self.arr.sel_lane));
        }
        // automatic day / night theme
        if auto() && self.last_auto_check.elapsed() > std::time::Duration::from_secs(20) {
            self.last_auto_check = web_time::Instant::now();
            if night_now() != is_dark() {
                set_dark(night_now());
                apply_visuals(ctx);
            }
        }
        // debugging aid: `SYNTH_VIZ=<template 0..5> synthesia.exe` opens the visualizer and starts playback
        static VIZ_ONCE: std::sync::Once = std::sync::Once::new();
        VIZ_ONCE.call_once(|| {
            if let Ok(v) = std::env::var("SYNTH_VIZ") {
                self.viz.open = v != "x";
                self.viz.template = v.parse::<usize>().unwrap_or(0).min(visualizer::TEMPLATES.len() - 1);
                if let Ok(t) = std::env::var("SYNTH_VIZ_THEME") {
                    self.viz.theme = t.parse::<usize>().unwrap_or(0).min(visualizer::THEMES.len() - 1);
                }
                if let Ok(t) = std::env::var("SYNTH_VIZ_TPL") {
                    self.load_template(t.parse::<usize>().unwrap_or(0).min(crate::templates::TEMPLATES.len() - 1));
                    self.project.current_pattern = 1.min(self.project.patterns.len() - 1);
                }
                self.shared.send(Cmd::Play);
            }
        });
        self.handle_keys(ctx);
        if self.viz.fullscreen {
            // Avee-style full-window visualizer: only the visuals, Esc / double-click to leave
            let (sr, title, bpm, playing) = (self.audio.sample_rate as f32, self.project.name.clone(), self.project.bpm, self.shared.playing.load(Ordering::Relaxed));
            egui::CentralPanel::default().frame(egui::Frame::none().fill(egui::Color32::BLACK)).show(ctx, |ui| {
                visualizer::show(ui, &mut self.viz, &self.shared, sr, &title, bpm, playing);
            });
            ctx.request_repaint();
            return;
        }
        self.shared.live_track.store(self.live_track.id(), Ordering::Relaxed);
        for c in 0..=NUM_STRIPS {
            let p = self.shared.peak(c);
            for k in 0..2 {
                self.meters[c][k] = p[k].max(self.meters[c][k] * 0.88);
            }
        }

        egui::TopBottomPanel::top("transport").frame(glass_frame().inner_margin(egui::Margin::symmetric(16.0, 10.0))).show(ctx, |ui| {
            if cfg!(target_os = "android") {
                egui::ScrollArea::horizontal().id_salt("android_menu_bar").show(ui, |ui| {
                    ui.horizontal(|ui| {
                        #[cfg(target_os = "android")]
                        if let Some(updater) = &mut self.android_updater {
                            updater.show(ui);
                        }
                        self.header_brand(ui);
                    });
                });
                egui::ScrollArea::horizontal().id_salt("android_transport_bar").show(ui, |ui| {
                    ui.horizontal(|ui| {
                        self.transport(ui);
                        transport_ui::show_status(self, ui);
                    });
                });
                return;
            }

            let wide = ui.available_width() >= 1750.0;
            if !wide {
                ui.horizontal(|ui| { self.header_brand(ui); });
                ui.add_space(4.0);
            }
            let width = ui.available_width();
            let band = egui::Rect::from_min_max(
                pos2(ui.max_rect().left() - 16.0, ui.cursor().min.y - 10.0),
                pos2(ui.max_rect().right() + 16.0, ui.cursor().min.y + 46.0 + 10.0),
            );
            ui.painter().rect_filled(band, 0.0, SIDE());
            ui.painter().line_segment([band.left_bottom(), band.right_bottom()], Stroke::new(border_w(), border()));
            let drag_rect = egui::Rect::from_min_size(ui.cursor().min, vec2(width, 46.0));
            let drag = ui.interact(drag_rect, ui.id().with("header_window_drag"), egui::Sense::click_and_drag());
            if drag.drag_started() { ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag); }
            if drag.double_clicked() {
                let maximized = ctx.input(|i| i.viewport().maximized).unwrap_or(false);
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
            }
            // three zones: brand + menus on the left, the transport centred, meters / settings / window on the right
            // the theme / account / settings buttons only when the window is wide enough for them next to the transport
            let roomy = width >= 1750.0;
            // the right zone is exactly as wide as its content (measured last frame), so no empty gap is left in front of it
            let known_right: f32 = ctx.data(|d| d.get_temp(egui::Id::new("header_status_w"))).unwrap_or(0.0);
            let right_width = if roomy { if known_right > 100.0 { known_right + 20.0 } else { 640.0 } } else { 300.0 };
            ctx.data_mut(|d| d.insert_temp(egui::Id::new("toggles_right"), roomy));
            let left_width = if wide { 340.0 } else { 0.0 };
            let centre_width = (width - right_width - left_width - 48.0).max(80.0);
            let wid = egui::Id::new("header_transport_w");
            let rid = egui::Id::new("header_status_w");
            let window_cx = ui.max_rect().center().x;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                if wide {
                    ui.allocate_ui_with_layout(vec2(left_width, 46.0), egui::Layout::left_to_right(egui::Align::Center), |ui| { self.header_brand(ui); });
                }
                ui.allocate_ui_with_layout(vec2(centre_width, 46.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    let known: f32 = ui.data(|d| d.get_temp(wid)).unwrap_or(0.0);
                    // centred on the window, not on the zone between the brand and the right group (kept inside the zone)
                    let start = ui.cursor().min.x;
                    let want = window_cx - known * 0.5 - start;
                    ui.add_space(want.clamp(0.0, (centre_width - known).max(0.0)));
                    let x0 = ui.cursor().min.x;
                    egui::Frame::none().inner_margin(egui::Margin::symmetric(8.0, 2.0)).show(ui, |ui| {
                        theme::FLAT.store(true, std::sync::atomic::Ordering::Relaxed);
                        self.transport(ui);
                        theme::FLAT.store(false, std::sync::atomic::Ordering::Relaxed);
                    });
                    let w = ui.min_rect().right() - x0;
                    ui.data_mut(|d| d.insert_temp(wid, w));
                });
                ui.allocate_ui_with_layout(vec2(right_width, 46.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    // the group is pushed against the right edge: its width from the previous frame decides the left padding
                    let known: f32 = ui.data(|d| d.get_temp(rid)).unwrap_or(0.0);
                    ui.add_space((right_width - known).max(0.0));
                    let x0 = ui.cursor().min.x;
                    if roomy {
                        navigation::header_buttons(ui);
                        ui.add_space(6.0);
                        transport_ui::show_toggles(self, ui);
                    }
                    ui.add_space(6.0);
                    transport_ui::show_status(self, ui);
                    let w = ui.min_rect().right() - x0;
                    ui.data_mut(|d| d.insert_temp(rid, w));
                });
            });
        });
        egui::TopBottomPanel::bottom("status").frame(glass_frame().inner_margin(egui::Margin::symmetric(10.0, 4.0))).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("Octave {} (- / =)", self.octave + 1)).color(DIM()));
                if self.record {
                    ui.colored_label(RED, "REC armed - play notes on Z-M / Q-P / MIDI");
                }
                ui.separator();
                let tone = self.toasts.last().map_or(DIM(), |t| match t.2 { 1 => GREEN, 2 => RED, _ => TEAL() });
                ui.label(RichText::new(&self.status).color(tone));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let audio_info = match &self.audio.error {
                        Some(_) => "no audio device".to_string(),
                        None => format!("{} - {} Hz", self.audio.device_name, self.audio.sample_rate),
                    };
                    ui.label(RichText::new(audio_info).size(10.5).color(LIGHT())).on_hover_text("Audio output (Audio menu: device, rate, buffer)");
                    ui.label(RichText::new(format!("{} - {}", format!("{}{}", self.project.name, if self.dirty() { " *" } else { "" }), self.path.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "unsaved".into()))).color(DIM()).size(11.0));
                });
            });
        });
        // the G button of the header opens / closes the account panel (a panel of its own, left of the Browser)
        if ctx.data_mut(|d| d.remove_temp::<bool>(egui::Id::new("account_toggle"))).unwrap_or(false) {
            self.account_open = !self.account_open;
        }
        if self.account_open && !self.hub_open && self.account_panel(ctx) {
            self.account_open = false;
        }
        if self.browser_open && !self.hub_open {
            let mut out = browser_panel::Out::default();
            let sec = ctx.data(|d| d.get_temp::<u8>(egui::Id::new("browser_filter"))).unwrap_or(browser_panel::SEC_SAMPLES);
            egui::SidePanel::left("studio_browser").default_width(270.0).min_width(220.0).max_width(420.0)
                .frame(glass_frame().inner_margin(egui::Margin::symmetric(10.0, 6.0)))
                .show(ctx, |ui| {
                    // Project Files: the clip list, the action history and the project summary live at the bottom
                    if sec == browser_panel::SEC_PROJECTS {
                        egui::TopBottomPanel::bottom("browser_clips").resizable(true).height_range(120.0..=420.0).default_height(230.0).frame(egui::Frame::none().inner_margin(egui::Margin::symmetric(0.0, 4.0))).show_inside(ui, |ui| {
                            self.project_info(ui);
                            ui.separator();
                            arrangement_ui::clip_list(ui, &mut self.project, &mut self.arr);
                            ui.add_space(6.0);
                            self.history_ui(ui);
                        });
                    }
                    let recent = self.recent.clone();
                    let mut c = browser_panel::Ctx {
                        project: &mut self.project,
                        lib: &mut self.lib,
                        shared: &self.shared,
                        lane: &mut self.seq.lane,
                        sel_lane: self.arr.sel_lane,
                        tab: &mut self.tab,
                        inst: &mut self.inst,
                        live_track: &mut self.live_track,
                        roll_track: &mut self.roll.track,
                        recent: &recent,
                    };
                    browser_panel::body(ui, &mut c, &mut out);
                });
            if out.close {
                self.browser_open = false;
            }
            if let Some(path) = out.open_path.take() {
                self.open_path(path);
            }
        }
        if self.hub_open {
            self.hub_ui(ctx);
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
            return;
        }
        // page tabs: a bar of their own right under the header, centred above the work area (right of the browser / pattern columns)
        egui::TopBottomPanel::top("pages_bar").frame(glass_frame().inner_margin(egui::Margin::symmetric(14.0, 6.0))).show(ctx, |ui| {
            // centred: the row's width from the previous frame decides the left padding
            let wid = egui::Id::new("pages_bar_w");
            let row_w: f32 = ui.data(|d| d.get_temp(wid)).unwrap_or(0.0);
            let _ = row_w;
            ui.horizontal_wrapped(|ui| {
                let x0 = ui.cursor().min.x;
                let x1 = ui.cursor().min.x;
                let title = match self.main { Tab::Session => "Session", Tab::Sequencer => "Channel Rack", Tab::PianoRoll => "Piano Roll", Tab::Mixer => "Mixer", Tab::Plugins => "Plugins", _ => "Playlist" };
                if self.main == Tab::Arrangement {
                    // the Playlist title is a menu of arrangements: pick one, add a new empty one, copy or delete the current one
                    let cur = self.project.scenes.get(self.project.active_scene).map(|s| s.name.clone()).unwrap_or_default();
                    ui.menu_button(RichText::new(format!("Playlist  -  {cur}  v")).strong().size(15.0).color(TEXT()), |ui| {
                        ui.set_min_width(220.0);
                        ui.label(RichText::new("Arrangements").small().color(DIM()));
                        let names: Vec<String> = self.project.scenes.iter().map(|s| s.name.clone()).collect();
                        for (i, n) in names.iter().enumerate() {
                            if ui.selectable_label(i == self.project.active_scene, n).clicked() {
                                self.project.switch_scene(i);
                                self.arr.sel_lane = 0;
                                self.arr.sel.clear();
                                self.arr.sel_clip = None;
                                ui.close_menu();
                            }
                        }
                        ui.separator();
                        if ui.button("+ Add new arrangement").clicked() {
                            self.project.new_scene();
                            self.arr.sel_lane = 0;
                            self.arr.sel.clear();
                            self.arr.sel_clip = None;
                            ui.close_menu();
                        }
                        if ui.button("Duplicate this arrangement").clicked() {
                            self.project.duplicate_scene();
                            self.arr.sel.clear();
                            self.arr.sel_clip = None;
                            ui.close_menu();
                        }
                        if self.project.scenes.len() > 1 && ui.button("Delete this arrangement").clicked() {
                            self.project.delete_scene();
                            self.arr.sel_lane = 0;
                            self.arr.sel.clear();
                            self.arr.sel_clip = None;
                            ui.close_menu();
                        }
                    });
                } else {
                    ui.label(RichText::new(title).strong().size(15.0).color(TEXT()));
                }
                ui.separator();
                if self.main == Tab::Arrangement {
                    arrangement_ui::toolbar(ui, &mut self.project, &mut self.arr);
                }
                let w = ui.cursor().min.x - x1;
                ui.data_mut(|d| d.insert_temp(wid, w));
                let _ = x0;
            });
        });
        // a clicked clip opens the detail dock for its track
        if self.arr.open_detail {
            self.arr.open_detail = false;
            let midi = self.project.arrangement.tracks.get(self.arr.sel_lane).map(|t| t.kind) == Some(crate::arrangement::TrackKind::Midi);
            self.arr.open_roll = false;
            // a clicked clip opens its editor in the bottom dock (where Track Settings sits), not in the main window
            let _ = midi;
            let lane = self.arr.sel_lane;
            if let Some(t) = self.project.arrangement.tracks.get(lane).and_then(|t| t.instrument) {
                self.roll.track = t;
                self.live_track = t;
            }
            ctx.data_mut(|d| { d.insert_temp(devices_ui::dock_page_id(), 3u8); d.insert_temp(devices_ui::dock_editor_id(), lane); });
            roll_ui::give_focus_to_roll(ctx, true);
        }
        // "Open editor" in the Playlist's track panel: the instrument / sound window
        if std::mem::take(&mut self.arr.open_editor) {
            let midi = self.project.arrangement.tracks.get(self.arr.sel_lane).map(|t| t.kind) == Some(crate::arrangement::TrackKind::Midi);
            if midi {
                self.plugin_window = true;
            } else {
                self.dock_open = true;
                self.dock = Tab::Track;
            }
        }
        // route the last requested view: Playlist/Session/Mixer fill the centre, editors live in the detail dock
        match self.tab {
            Tab::Arrangement | Tab::Session | Tab::Plugins => self.main = self.tab,
            Tab::Sequencer => {
                self.main = Tab::Sequencer;
                self.tab = self.dock;
            }
            Tab::Mixer => {
                self.main = Tab::Mixer;
                self.tab = self.dock;
            }
            Tab::PianoRoll => {
                self.roll_tab = true;
                self.main = Tab::PianoRoll;
                self.tab = self.dock;
                let tracks = &self.project.arrangement.tracks;
                if tracks.get(self.arr.sel_lane).map(|t| t.kind) != Some(crate::arrangement::TrackKind::Midi) {
                    if let Some(l) = tracks.iter().position(|t| t.kind == crate::arrangement::TrackKind::Midi) {
                        self.arr.sel_lane = l;
                    }
                }
            }
            Tab::Idle => {}
            t => {
                self.dock = t;
                self.dock_open = true;
                if t == Tab::Instruments {
                    self.plugin_window = true;
                }
                // the piano roll / synth editor belong to a MIDI track: jump to one when an audio track is selected
                let tracks = &self.project.arrangement.tracks;
                if matches!(t, Tab::Instruments) && tracks.get(self.arr.sel_lane).map(|t| t.kind) != Some(crate::arrangement::TrackKind::Midi) {
                    if let Some(l) = tracks.iter().position(|t| t.kind == crate::arrangement::TrackKind::Midi) {
                        self.arr.sel_lane = l;
                    }
                }
                // a click on a plugin of the Browser puts it on the selected MIDI track when that has none yet, and opens it
                if t == Tab::Instruments {
                    let l = self.arr.sel_lane;
                    let empty_midi = self.project.arrangement.tracks.get(l).map_or(false, |x| x.kind == crate::arrangement::TrackKind::Midi && x.instrument.is_none());
                    if empty_midi {
                        let tr = self.inst.track();
                        self.project.arrangement.tracks[l].instrument = Some(tr);
                        self.roll.track = tr;
                        self.live_track = tr;
                    }
                }
            }
        }
        let lanes = self.project.arrangement.tracks.len();
        self.arr.sel_lane = self.arr.sel_lane.min(lanes.saturating_sub(1));
        let sel_kind = self.project.arrangement.tracks.get(self.arr.sel_lane).map(|t| t.kind).unwrap_or(crate::arrangement::TrackKind::Midi);
        if matches!(self.dock, Tab::PianoRoll | Tab::Instruments | Tab::Track) {
            if sel_kind == crate::arrangement::TrackKind::Audio {
                self.dock = Tab::Track; // sample view
            } else if matches!(self.dock, Tab::Track | Tab::PianoRoll) {
                self.dock = Tab::Instruments;
            }
        }
        self.tab = Tab::Idle;
        // selecting a track with a routed synth makes that synth the live / piano-roll instrument
        if self.arr.sel_lane != self.last_sel_lane {
            self.last_sel_lane = self.arr.sel_lane;
            self.dock_open = true;
            self.roll_tab = sel_kind == crate::arrangement::TrackKind::Midi;
            if let Some(t) = self.project.arrangement.tracks.get(self.arr.sel_lane).and_then(|t| t.instrument) {
                self.roll.track = t;
                self.live_track = t;
                self.inst = instruments_ui::Inst::from_track(t);
            }
        }
        self.roll.audible = self.project.arrangement.tracks.get(self.arr.sel_lane).map_or(true, |t| t.instrument.is_some() && !t.mute);
        self.roll.lock_track = self.main == Tab::Sequencer || (self.project.arrangement.tracks.get(self.arr.sel_lane).map_or(false, |t| t.instrument.is_some()));
        let mut rack_action = None;
        let mut roll_switch: Option<usize> = None;
        // the device view sits under the Playlist / Session only, never inside the piano roll, rack or mixer
        let sel_is_audio = self.project.arrangement.tracks.get(self.arr.sel_lane).map(|t| t.kind) == Some(crate::arrangement::TrackKind::Audio);
        // the bottom "Sound" dock of an audio track is gone: the track's card and the DEVICES column already say it all
        if false && self.dock_open && lanes > 0 && sel_is_audio && matches!(self.main, Tab::Arrangement | Tab::Session) {
            let max_h = (ctx.screen_rect().height() - 220.0).max(200.0);
            egui::TopBottomPanel::bottom("detail")
                .resizable(true)
                .height_range(200.0..=max_h)
                .default_height(self.dock_h)
                .frame(egui::Frame::none().fill(BG()).stroke(Stroke::new(border_w(), border())).inner_margin(egui::Margin::symmetric(10.0, 8.0)))
                .show(ctx, |ui| {
                    self.dock_h = ui.available_height();
                    let lane = self.arr.sel_lane;
                    let audio = sel_kind == crate::arrangement::TrackKind::Audio;
                    // Ableton-style detail tabs: Channel Rack / Piano Roll (or Sound) / Instrument / Mixer
                    ui.horizontal(|ui| {
                        if audio {
                            if ui.selectable_label(self.dock == Tab::Track, "Sound").clicked() {
                                self.dock = Tab::Track;
                                self.tab = Tab::Track;
                            }
                        } else {
                            for (t, n) in [(Tab::Instruments, "Instrument")] {
                                if ui.selectable_label(self.dock == t, n).clicked() {
                                    self.dock = t;
                                    self.tab = t;
                                }
                            }
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("Hide").clicked() {
                                self.dock_open = false;
                            }
                        });
                    });
                    ui.separator();
                    let track_view = matches!(self.dock, Tab::PianoRoll | Tab::Instruments | Tab::Track);
                    if track_view && lanes == 0 {
                        ui.label(RichText::new("Add a track in the Playlist first.").color(DIM()));
                        return;
                    }
                    if track_view {
                        track_ui::header(ui, &mut self.project, lane);
                        ui.separator();
                    }
                    match self.dock {
                        Tab::Track => track_ui::sample_panel(ui, &mut self.project, lane, self.arr.sel_sample, true, &self.shared),
                        Tab::Instruments => {
                            let has = self.project.arrangement.tracks.get(lane).map_or(false, |t| t.instrument.is_some());
                            if has {
                                // the device card: the plugin itself lives in its own window
                                let t = self.project.arrangement.tracks[lane].instrument.unwrap_or(Track::Synth);
                                ui.horizontal(|ui| {
                                    egui::Frame::none().fill(PANEL2()).rounding(corner()).inner_margin(egui::Margin::symmetric(14.0, 10.0)).stroke(Stroke::new(1.0, border())).show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            ui.label(RichText::new(instruments_ui::Inst::from_track(t).title()).strong().size(15.0).color(TEAL()));
                                            if ui.button("Open plugin").on_hover_text("Presets and every knob, in a window of its own").clicked() {
                                                self.plugin_window = true;
                                            }
                                            if ui.button("Remove").on_hover_text("Take the plugin off this track (the track becomes silent)").clicked() {
                                                let t = &mut self.project.arrangement.tracks[lane];
                                                t.instrument = None;
                                                t.blank = true;
                                                t.snd.own = false;
                                                self.plugin_window = false;
                                            }
                                        });
                                    });
                                    ui.label(RichText::new("double-click a note in the piano roll or open the plugin to change the sound").color(DIM()).size(11.0));
                                });
                            } else {
                                let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 90.0), egui::Sense::hover());
                                let over = resp.dnd_hover_payload::<browser::DragItem>().is_some();
                                ui.painter().rect_stroke(rect.shrink(2.0), 0.0, Stroke::new(if over { 2.5 } else { 1.0 }, if over { TEAL() } else { MID() }));
                                ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "Empty track - drag a synth / plugin from the Browser here", egui::FontId::proportional(13.0), DIM());
                                if let Some(pl) = resp.dnd_release_payload::<browser::DragItem>() {
                                    if let browser::DragItem::Instrument(t) = &*pl {
                                        self.project.arrangement.tracks[lane].instrument = Some(*t);
                                        self.roll.track = *t;
                                        self.live_track = *t;
                                        self.inst = instruments_ui::Inst::from_track(*t);
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                });
        }
        // the clip editor (piano roll / sample view) takes the whole width of the window under the Playlist and the Mixer
        let editor_dock = lanes > 0
            && matches!(self.main, Tab::Arrangement | Tab::Session)
            && ctx.data(|d| d.get_temp::<u8>(devices_ui::dock_page_id())) == Some(3)
            && ctx.data(|d| d.get_temp::<usize>(devices_ui::dock_editor_id())) == Some(self.arr.sel_lane);
        // the dock always spans the whole window: under the Playlist and under the Mixer
        if lanes > 0 && matches!(self.main, Tab::Arrangement | Tab::Session) {
            self.bottom_dock(ctx, editor_dock);
        }
        // left column of the Playlist (under the page tabs, so its rows line up with the lanes): the tracks, and under them
        // the clips and the action history. On a narrow window the tracks stay next to the lanes and the devices sit here.
        if self.main == Tab::Arrangement {
            egui::SidePanel::left("studio_tracks").resizable(false).exact_width(290.0).frame(egui::Frame::none().fill(SIDE()).inner_margin(egui::Margin::symmetric(14.0, 14.0))).show(ctx, |ui| {
                    arrangement_ui::tracks_column(
                        ui,
                        &mut self.project,
                        &mut self.arr,
                        &mut arrangement_ui::DetailCtx { roll_track: &mut self.roll.track, live_track: &mut self.live_track, inst: &mut self.inst },
                    );
            });
        }
        // right column of the Playlist: device view of the selected track on top, the clip list and the sound player in the
        // bottom corner. On a narrow window it would squeeze the timeline to nothing, so it only appears when there is room
        if matches!(self.main, Tab::Arrangement | Tab::Session) && ctx.screen_rect().width() >= 1500.0 {
            egui::SidePanel::right("devices_col")
                .default_width(312.0)
                .min_width(250.0)
                .max_width(460.0)
                .resizable(true)
                .frame(egui::Frame::none().fill(SIDE()).stroke(Stroke::new(border_w(), border())).inner_margin(egui::Margin::symmetric(12.0, 10.0)))
                .show(ctx, |ui| {
                    // nothing inside may be wider than the column, or the column reserves more room than it draws
                    ui.set_max_width(ui.available_width());
                    // the whole mixer lives in this column, going down
                    egui::ScrollArea::vertical().id_salt("mixer_col_scroll").auto_shrink([false, false]).show(ui, |ui| {
                        mixer_ui::show_column(ui, &mut self.project, &self.meters, f32::from_bits(self.shared.gr.load(Ordering::Relaxed)));
                    });
                });
        }
        // the device view (instrument slot + effects of the selected track) sits under the Playlist, like Ableton's
        // (moved: the device view now lives in the left column under the patterns)
        // the device view (instrument slot + effects of the selected track) under the Playlist's lanes, in the free space
        // the plugin window: preset library + engine editor of the selected track's plugin
        if self.plugin_window {
            let lane = self.arr.sel_lane;
            let found = self.project.arrangement.tracks.get(lane).and_then(|t| t.instrument.map(|i| (t.name.clone(), i)));
            match found {
                Some((tname, t)) => {
                    self.inst = instruments_ui::Inst::from_track(t);
                    let mut open = true;
                    let mut picked = None;
                    egui::Window::new(format!("{} - {}", tname, self.inst.title()))
                        .id(egui::Id::new("plugin_window"))
                        .open(&mut open)
                        .default_size([900.0, 720.0])
                        .resizable(true)
                        .show(ctx, |ui| {
                            egui::ScrollArea::both().auto_shrink([false, false]).id_salt("plugin_scroll").show(ui, |ui| {
                                picked = instruments_ui::show(ui, &mut self.project, &mut self.inst, lane);
                            });
                        });
                    if let Some(pt) = picked {
                        // a preset can live on another engine: the track follows it
                        self.project.arrangement.tracks[lane].instrument = Some(pt);
                        self.roll.track = pt;
                        self.live_track = pt;
                    }
                    if !open {
                        self.plugin_window = false;
                    }
                }
                None => self.plugin_window = false,
            }
        }
        // page tabs: a bar of their own right under the header, over the work area only: the browser column on the left keeps its full height (right of the browser / pattern columns)
        match navigation::show(ctx, self.main, self.hub_open) {
            Some(navigation::Action::View(tab)) => { self.hub_open = false; self.tab = tab; }
            Some(navigation::Action::Browser(sec)) => {
                self.browser_open = true;
                ctx.data_mut(|d| d.insert_temp(egui::Id::new("browser_filter"), sec));
            }
            None => {}
        }
        egui::CentralPanel::default().frame(egui::Frame::none().inner_margin(10.0)).show(ctx, |ui| {
            match self.main {
                Tab::Session => session_ui::show(ui, &mut self.project, &self.shared, &mut self.lib, &mut self.arr),
                Tab::Sequencer => {
                    let mut do_split = false;
                    ui.horizontal(|ui| {
                        if ui.button("+ MIDI track").on_hover_text("New MIDI track (shared with the Playlist)").clicked() {
                            self.project.arrangement.add_track(crate::arrangement::TrackKind::Midi);
                        }
                        if ui.button("+ Audio track").on_hover_text("New audio track (shared with the Playlist)").clicked() {
                            self.project.arrangement.add_track(crate::arrangement::TrackKind::Audio);
                        }
                        ui.separator();
                        ui.label(RichText::new("Write the whole beat here, then send it").color(DIM()));
                        ui.label(RichText::new("repeat").color(DIM()));
                        ui.add(egui::DragValue::new(&mut self.split_repeat).range(1..=64).suffix(" x"));
                        if ui.button("Send to Playlist").on_hover_text("The notes of every MIDI track go to the Playlist as clips on that track").clicked() {
                            do_split = true;
                        }
                    });
                    if do_split {
                        let cur = self.project.rack_pattern();
                        let sent = self.project.send_rack_to_tracks(cur, self.split_repeat);
                        if sent == 0 {
                            self.status = if self.project.patterns.get(cur).map_or(false, |p| p.clip_only) { "You are editing the notes of a Playlist clip - they already play there. Select a pattern to send.".into() } else { "Nothing to send: put some notes on the MIDI tracks first (click the steps).".into() };
                        } else {
                            self.status = format!("Sent {sent} track(s) to the Playlist, each clip on its own track");
                            self.tab = Tab::Arrangement;
                        }
                    }
                    ui.toggle_value(&mut self.pad_steps, "Step rows").on_hover_text("Show the step rows (drums and instruments) under the launchpad");
                    ui.separator();
                    // Launchpad: the clips of every MIDI and audio track as pads on top; the piano roll of the picked MIDI clip under them
                    let sel_clip = self.arr.pad_sel.and_then(|(l, s)| self.project.session.get(l, s).copied());
                    let roll_clip = sel_clip.filter(|c| !c.audio && c.pattern < self.project.patterns.len() && self.project.arrangement.tracks.get(c.lane).map_or(false, |t| t.kind == crate::arrangement::TrackKind::Midi));
                    if self.pad_last != self.arr.pad_sel {
                        self.pad_last = self.arr.pad_sel;
                        if let Some(c) = roll_clip {
                            self.project.current_pattern = c.pattern;
                            let t = self.project.arrangement.tracks[c.lane].instrument.unwrap_or(Track::Synth);
                            self.roll.track = t;
                            self.live_track = t;
                        }
                    }
                    if let Some(c) = roll_clip {
                        let name = self.project.arrangement.tracks[c.lane].name.clone();
                        let clip_name = self.project.patterns[c.pattern].name.clone();
                        let h = (ui.available_height() * 0.5).clamp(220.0, 900.0);
                        egui::TopBottomPanel::bottom("pad_roll").resizable(true).height_range(160.0..=1000.0).default_height(h).frame(egui::Frame::none().fill(BG()).inner_margin(egui::Margin { left: 0.0, right: 0.0, top: 6.0, bottom: 0.0 })).show_inside(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("PIANO ROLL").size(11.0).strong().color(DIM()));
                                ui.label(RichText::new(format!("{name} - {clip_name}")).strong().color(lane_col(c.lane)));
                                ui.label(RichText::new("click another pad to switch").size(11.0).color(DIM()));
                                if ui.small_button("Hide").on_hover_text("Close the piano roll").clicked() {
                                    self.arr.pad_sel = None;
                                }
                            });
                            self.project.current_pattern = c.pattern;
                            roll_ui::show(ui, &mut self.project, &mut self.roll, &self.shared);
                        });
                    }
                    if self.pad_steps {
                        egui::TopBottomPanel::bottom("pad_steps").resizable(true).height_range(120.0..=700.0).default_height(260.0).frame(egui::Frame::none().fill(BG()).inner_margin(egui::Margin { left: 0.0, right: 0.0, top: 6.0, bottom: 0.0 })).show_inside(ui, |ui| {
                            let cur = self.project.rack_pattern();
                            self.project.current_pattern = cur;
                            rack_action = sequencer_ui::show(ui, &mut self.project, &mut self.seq, &self.shared, &mut self.lib);
                        });
                    }
                    egui::CentralPanel::default().frame(egui::Frame::none()).show_inside(ui, |ui| {
                        session_ui::show(ui, &mut self.project, &self.shared, &mut self.lib, &mut self.arr);
                    });
                }
                Tab::Mixer => mixer_ui::show(ui, &mut self.project, &self.meters, f32::from_bits(self.shared.gr.load(Ordering::Relaxed))),
                Tab::Plugins => {
                    if let Some(t) = plugins_ui::show(ui, &mut self.project, &mut self.live_track, &mut self.roll.track, &mut self.inst, self.arr.sel_lane) {
                        self.tab = t;
                    }
                }
                Tab::PianoRoll => {
                    let lane = self.arr.sel_lane;
                    if lanes == 0 {
                        ui.label(RichText::new("Add a track in the Playlist first.").color(DIM()));
                    } else {
                        // switcher: every MIDI and audio track of the Playlist, one click to edit it here
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new("TRACKS").size(11.0).strong().color(DIM()));
                            for l in 0..lanes {
                                let t = &self.project.arrangement.tracks[l];
                                let audio = t.kind == crate::arrangement::TrackKind::Audio;
                                let col = lane_col(l);
                                let on = l == lane;
                                let text = RichText::new(format!("{}  {}", if audio { "AUDIO" } else { "MIDI" }, t.name)).size(11.5).color(if on { Color32::WHITE } else { col });
                                if ui.add(egui::Button::new(text).fill(if on { col.linear_multiply(0.5) } else { PANEL3() }).stroke(Stroke::new(1.0, if on { col } else { border() }))).clicked() {
                                    roll_switch = Some(l);
                                }
                            }
                        });
                        ui.separator();
                        if sel_kind == crate::arrangement::TrackKind::Midi {
                            if let Some(t) = track_ui::header(ui, &mut self.project, lane) {
                                self.roll.track = t;
                                self.live_track = t;
                                self.inst = instruments_ui::Inst::from_track(t);
                            }
                            ui.separator();
                            self.roll.lane = lane;
                            roll_ui::show(ui, &mut self.project, &mut self.roll, &self.shared);
                            if let Some(l) = self.roll.switch_to.take() {
                                roll_switch = Some(l);
                            }
                        } else {
                            track_ui::audio_clip_view(ui, &self.project, lane);
                        }
                    }
                }
                _ => arrangement_ui::show(
                    ui,
                    &mut self.project,
                    &mut self.arr,
                    &self.shared,
                    &mut self.lib,
                    &mut arrangement_ui::DetailCtx { roll_track: &mut self.roll.track, live_track: &mut self.live_track, inst: &mut self.inst },
                ),
            }
        });
        if let Some(l) = roll_switch {
            if l < self.project.arrangement.tracks.len() {
                self.arr.sel_lane = l;
                if self.project.arrangement.tracks[l].kind == crate::arrangement::TrackKind::Midi {
                    let pi = self.project.lane_pattern(l);
                    self.project.current_pattern = pi;
                    let t = self.project.arrangement.tracks[l].instrument.unwrap_or(Track::Synth);
                    self.roll.track = t;
                    self.live_track = t;
                }
            }
        }
        match rack_action {
            Some(sequencer_ui::RackAction::OpenRoll(t)) => {
                self.roll.track = t;
                self.live_track = t;
                self.tab = Tab::PianoRoll;
            }
            Some(sequencer_ui::RackAction::OpenTrack(l)) => {
                if l < self.project.arrangement.tracks.len() {
                    self.arr.sel_lane = l;
                    match self.project.arrangement.tracks[l].kind {
                        crate::arrangement::TrackKind::Midi => {
                            // the notes of this track open in the roll under the steps, right in the Channel Rack
                            self.project.current_pattern = self.project.rack_pattern();
                            let t = self.project.arrangement.tracks[l].instrument.unwrap_or(Track::Synth);
                            self.roll.track = t;
                            self.live_track = t;
                            if self.main != Tab::Sequencer {
                                self.tab = Tab::Sequencer;
                            }
                        }
                        crate::arrangement::TrackKind::Audio => {
                            self.dock_open = true;
                            self.tab = Tab::Arrangement;
                        }
                    }
                }
            }
            Some(sequencer_ui::RackAction::OpenSample(l)) => {
                self.sample_win = Some(l);
            }
            Some(sequencer_ui::RackAction::OpenInstrument(t)) => {
                self.open_inst(instruments_ui::Inst::from_track(t));
                self.dock = Tab::Instruments;
            }
            None => {}
        }
        if let Some(l) = self.sample_win {
            let mut open = true;
            sample_window::show(ctx, &mut self.project, &mut self.lib, &self.shared, l, &mut open);
            if !open {
                self.sample_win = None;
            }
        }
        if self.viz.open {
            let (sr, title, bpm, playing) = (self.audio.sample_rate as f32, self.project.name.clone(), self.project.bpm, self.shared.playing.load(Ordering::Relaxed));
            let mut open = true;
            egui::Window::new("Visualizer").open(&mut open).default_size([720.0, 440.0]).resizable(true).show(ctx, |ui| {
                visualizer::show(ui, &mut self.viz, &self.shared, sr, &title, bpm, playing);
            });
            if !open {
                self.viz.open = false;
            }
        }
        if self.show_projects {
            let mut open = true;
            let mut pick = None;
            egui::Window::new("Projects").open(&mut open).default_size([520.0, 360.0]).show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("Open file...").clicked() {
                        pick = rfd::FileDialog::new().add_filter("Winteria DAW project (*.wd)", &["wd", "vpd", "synthesia"]).pick_file();
                    }
                    ui.label(RichText::new(format!("Current: {}", self.project.name)).color(DIM()));
                });
                ui.separator();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for p in &self.recent {
                        ui.horizontal(|ui| {
                            let name = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                            if ui.button(RichText::new(name).strong()).clicked() {
                                pick = Some(p.clone());
                            }
                            let kb = std::fs::metadata(p).map(|m| m.len() / 1024).unwrap_or(0);
                            ui.label(RichText::new(format!("{kb} KB  {}", p.display())).size(10.0).color(DIM()));
                        });
                    }
                });
            });
            self.show_projects = open;
            if let Some(p) = pick {
                self.open_path(p);
            }
        }
        if self.show_audio {
            self.audio_window(ctx);
        }
        if self.show_help {
            self.help_window(ctx);
        }
        if self.export.open || self.export.rx.is_some() {
            self.export_window(ctx);
        }
        self.project.stamp_warp();
        self.audio_recording();
        self.publish_edits();
        // floating label under the cursor while an item is dragged out of the Browser
        if let Some(item) = egui::DragAndDrop::payload::<browser::DragItem>(ctx) {
            if let Some(pos) = ctx.pointer_hover_pos() {
                ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
                let kind = if item.kind() == crate::arrangement::TrackKind::Audio { "AUDIO" } else { "MIDI" };
                egui::Area::new(egui::Id::new("dnd_ghost")).order(egui::Order::Tooltip).fixed_pos(pos + vec2(16.0, 12.0)).interactable(false).show(ctx, |ui| {
                    egui::Frame::none().fill(SIDE()).stroke(Stroke::new(1.5, TEAL())).inner_margin(egui::Margin::symmetric(8.0, 4.0)).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(kind).size(10.0).strong().color(TEAL_HI()));
                            ui.label(RichText::new(item.label(&self.project)).color(TEXT()));
                        });
                    });
                });
            }
        }
        let _ = Color32::BLACK;
        // the UI redraws as rarely as it can get away with, so it leaves the CPU to the audio (and other programs)
        let focused = ctx.input(|i| i.focused);
        let ms = match (self.playing(), focused) {
            (true, true) => 16,
            (true, false) => 40,
            (false, true) => 33,
            (false, false) => 120,
        };
        ctx.request_repaint_after(std::time::Duration::from_millis(ms));
    }
}
