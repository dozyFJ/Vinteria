pub mod arrangement;
pub mod audio_engine;
pub mod audio_rec;
pub mod updater;
#[cfg(target_os = "android")]
mod android_update;
pub mod bounce;
pub mod effects;
pub mod fx;
pub mod fx_ext;
pub mod google_auth;
pub mod instruments;
pub mod midi_in;
pub mod mixer;
pub mod piano_roll;
pub mod project;
pub mod sequencer;
pub mod session;
pub mod templates;
pub mod theory;
pub mod ui;

use audio_engine::device::{AudioIo, Shared};
use audio_engine::Cmd;
use std::sync::Arc;

/// Headless check of the real audio path: opens the default device, plays the demo for a few seconds
/// and reports callback count and peak level.
fn play_test() {
    let p = Arc::new(project::Project::demo());
    let shared = Shared::new(Arc::clone(&p));
    let audio = AudioIo::new(Arc::clone(&shared));
    if let Some(e) = &audio.error {
        eprintln!("AUDIO ERROR: {e}");
        std::process::exit(2);
    }
    println!("device: {} @ {} Hz (devices: {:?})", audio.device_name, audio.sample_rate, audio.devices);
    shared.send(Cmd::Play);
    let mut peak = 0.0f32;
    for _ in 0..60 {
        std::thread::sleep(std::time::Duration::from_millis(50));
        peak = peak.max(shared.peak(crate::mixer::NUM_STRIPS)[0]).max(shared.peak(crate::mixer::NUM_STRIPS)[1]);
    }
    println!(
        "playing={} callbacks={} master_peak={:.3} position_ticks={}",
        shared.playing.load(std::sync::atomic::Ordering::Relaxed),
        shared.callbacks.load(std::sync::atomic::Ordering::Relaxed),
        peak,
        shared.position.load(std::sync::atomic::Ordering::Relaxed)
    );
    shared.send(Cmd::Stop);
    std::thread::sleep(std::time::Duration::from_millis(100));
    if peak < 0.05 {
        eprintln!("FAIL: no audio level detected");
        std::process::exit(1);
    }
    println!("PLAY TEST OK");
}

/// Offline CPU benchmark of the audio engine: renders each template pattern and prints the load.
fn bench() {
    for (name, make) in templates::TEMPLATES.iter().take(2) {
        for pat in 0..3 {
            let mut p = make();
            p.current_pattern = pat;
            let mut e = audio_engine::Engine::new(48000.0);
            e.set_project(Arc::new(p));
            e.play();
            let secs = 10.0;
            let mut buf = vec![0.0f32; 48000 * 10 * 2];
            let t0 = std::time::Instant::now();
            for c in buf.chunks_mut(512) {
                e.render(c);
            }
            let el = t0.elapsed().as_secs_f32();
            let peak = buf.iter().fold(0.0f32, |m, v| m.max(v.abs()));
            println!("{name} pattern {pat}: CPU {:.1}% of one core, peak {peak:.2}", el / secs * 100.0);
        }
    }
}

fn bench_variant(label: &str, mutate: impl Fn(&mut project::Project)) {
    let mut p = templates::phonk_drift();
    p.current_pattern = 1;
    mutate(&mut p);
    let mut e = audio_engine::Engine::new(48000.0);
    e.set_project(Arc::new(p));
    e.play();
    let mut buf = vec![0.0f32; 48000 * 10 * 2];
    let t0 = std::time::Instant::now();
    for c in buf.chunks_mut(512) {
        e.render(c);
    }
    println!("{label}: CPU {:.1}%", t0.elapsed().as_secs_f32() / 10.0 * 100.0);
}

fn micro() {
    use std::time::Instant;
    let n = 480000usize;
    let sr = 48000.0f32;
    let p = project::Project::default();
    let mut acc = 0.0f32;
    let mut wt = instruments::wavetable::WtSynth::new();
    let t = Instant::now();
    for _ in 0..n { acc += wt.render_sample(&p.wt, sr); }
    println!("wavetable idle: {:.0} ns/sample", t.elapsed().as_nanos() as f64 / n as f64);
    let mut an = instruments::analog::AnalogSynth::new();
    let t = Instant::now();
    for _ in 0..n { acc += an.render_sample(&p.analog, sr); }
    println!("analog idle: {:.0} ns/sample", t.elapsed().as_nanos() as f64 / n as f64);
    let mut sy = instruments::synth::Synth::new();
    let t = Instant::now();
    for _ in 0..n { acc += sy.render_sample(&p.synth, sr); }
    println!("poly idle: {:.0} ns/sample", t.elapsed().as_nanos() as f64 / n as f64);
    let mut b = instruments::bass808::Bass808::new();
    let t = Instant::now();
    for _ in 0..n { acc += b.render_sample(&p.bass, sr); }
    println!("808 idle: {:.0} ns/sample", t.elapsed().as_nanos() as f64 / n as f64);
    let mut lim = effects::Limiter::new();
    let t = Instant::now();
    for i in 0..n { let (l, r) = lim.process((i % 7) as f32 * 0.01, 0.0, sr); acc += l + r; }
    println!("limiter: {:.0} ns/sample", t.elapsed().as_nanos() as f64 / n as f64);
    let mut d = effects::Delay::new(sr);
    let t = Instant::now();
    for _ in 0..n { let (l, r) = d.process(0.0, 0.0, 30000.0, 0.4); acc += l + r; }
    println!("delay: {:.0} ns/sample", t.elapsed().as_nanos() as f64 / n as f64);
    let mut lane = instruments::drums::DrumLane::new();
    let mut rng = audio_engine::dsp::Rng::new(1);
    let t = Instant::now();
    for _ in 0..n { for k in 0..5 { acc += lane.render(k, None, sr, &mut rng); } }
    println!("5 drum lanes idle: {:.0} ns/sample", t.elapsed().as_nanos() as f64 / n as f64);
    println!("({acc})");
}

/// The program icon drawn in code: a striped synthwave sun over a grid, on a dark rounded square.
fn make_icon() -> egui::IconData {
    // the Vinteria "V" (assets/vinteria.ico is the same picture for the shortcut)
    egui::IconData { rgba: include_bytes!("../assets/icon128.rgba").to_vec(), width: 128, height: 128 }
}

/// Windows plays its "ding" when a window gets Alt+key, F10 or a key in the menu mode that Alt / F10 switches on and there is no menu
/// to take it. The program has no native menu, so these messages are answered here and the ding never starts.
#[cfg(windows)]
pub fn silence_system_beeps() {
    use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
    static DONE: AtomicBool = AtomicBool::new(false);
    static PREV: AtomicIsize = AtomicIsize::new(0);
    #[link(name = "user32")]
    extern "system" {
        fn GetActiveWindow() -> isize;
        fn SetWindowLongPtrW(h: isize, index: i32, new: isize) -> isize;
        fn CallWindowProcW(prev: isize, h: isize, msg: u32, w: usize, l: isize) -> isize;
    }
    unsafe extern "system" fn proc_(h: isize, msg: u32, w: usize, l: isize) -> isize {
        match msg {
            0x0120 => return 0x0001_0000, // WM_MENUCHAR: MNC_CLOSE, no beep
            0x0106 => return 0,           // WM_SYSCHAR: Alt+key text
            0x0112 if (w & 0xFFF0) == 0xF100 => return 0, // WM_SYSCOMMAND / SC_KEYMENU: Alt or F10 opening the (missing) menu
            _ => {}
        }
        let r = CallWindowProcW(PREV.load(Ordering::Relaxed), h, msg, w, l);
        // WM_NCHITTEST answering "nowhere" / "error" makes Windows ding on a click: such a point counts as the window itself
        if msg == 0x0084 && (r == -2 || r == 0) {
            return 1; // HTCLIENT
        }
        r
    }
    if DONE.load(Ordering::Relaxed) {
        return;
    }
    let h = unsafe { GetActiveWindow() };
    if h == 0 {
        return;
    }
    let prev = unsafe { SetWindowLongPtrW(h, -4, proc_ as usize as isize) };
    if prev != 0 {
        PREV.store(prev, Ordering::Relaxed);
        DONE.store(true, Ordering::Relaxed);
    }
}

#[cfg(not(windows))]
pub fn silence_system_beeps() {}

/// The program runs from a private copy of its exe (`live/VinteriaDaw-<pid>.exe`). Windows will not let a build replace an exe
/// that is running, so this keeps `VinteriaDaw.exe` free for `cargo build`: the running copy notices the new file and restarts
/// itself into it (see `VinteriaApp::check_update`). Returns true when the copy was started and this process should end.
#[cfg(not(target_os = "android"))]
fn relaunch_as_live_copy() -> bool {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a.starts_with("--live=")) || std::env::var("VINTERIA_NO_LIVE").is_ok() {
        return false;
    }
    let Ok(exe) = std::env::current_exe() else { return false };
    let Some(dir) = exe.parent() else { return false };
    let live_dir = dir.join("live");
    if std::fs::create_dir_all(&live_dir).is_err() {
        return false;
    }
    // copies of earlier runs: the ones still running refuse to be deleted, which is fine
    if let Ok(rd) = std::fs::read_dir(&live_dir) {
        for e in rd.flatten() {
            let _ = std::fs::remove_file(e.path());
        }
    }
    let copy = live_dir.join(format!("VinteriaDaw-{}.exe", std::process::id()));
    if std::fs::copy(&exe, &copy).is_err() {
        return false;
    }
    std::process::Command::new(&copy).args(args.iter().skip(1)).arg(format!("--live={}", exe.display())).spawn().is_ok()
}

pub fn desktop_main() -> eframe::Result<()> {
    // the program has no console window: a panic (the UI or the audio thread) would vanish silently and the sound just stops,
    // so every panic is written to vinteria_crash.log next to the program
    std::panic::set_hook(Box::new(|info| {
        let t = std::thread::current();
        let line = format!("[thread {}] {}
", t.name().unwrap_or("unnamed"), info);
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open("vinteria_crash.log") {
            use std::io::Write;
            let _ = f.write_all(line.as_bytes());
        }
    }));
    if std::env::args().any(|a| a == "--micro") {
        micro();
        return Ok(());
    }
    if std::env::args().any(|a| a == "--bench2") {
        bench_variant("baseline", |_| {});
        bench_variant("no master fx", |p| p.master_fx.comp_on = false);
        bench_variant("no master fx/eq", |p| p.master_fx = project::MasterFx::default());
        bench_variant("no delay", |p| { p.delay_return = 0.0; for c in p.channels.iter_mut() { c.delay_send = 0.0; } });
        bench_variant("no patterns notes", |p| { for pt in p.patterns.iter_mut() { pt.notes.clear(); } });
        bench_variant("no notes no drums", |p| { for pt in p.patterns.iter_mut() { pt.notes.clear(); for l in pt.drums.iter_mut() { for s in l.iter_mut() { s.on = false; } } } });
        return Ok(());
    }
    if std::env::args().any(|a| a == "--bench") {
        bench();
        return Ok(());
    }
    if std::env::args().any(|a| a == "--play-test") {
        play_test();
        return Ok(());
    }
    if let Some(pos) = std::env::args().position(|a| a == "--export") {
        let args: Vec<String> = std::env::args().collect();
        let input = args.get(pos + 1).unwrap_or_else(|| { eprintln!("Usage: --export <input.wd> [output.wav]"); std::process::exit(1); });
        let default_out = "export.wav".to_string();
        let output = args.get(pos + 2).unwrap_or(&default_out);
        let aggressive = args.iter().any(|a| a == "--aggressive");
        eprintln!("Loading project: {input}");
        let (mut p, _warnings) = project::Project::load(std::path::Path::new(input)).unwrap_or_else(|e| { eprintln!("LOAD ERROR: {e}"); std::process::exit(1); });
        if aggressive {
            use crate::sequencer::TICKS_PER_BAR;
            eprintln!("Applying aggressive drift phonk settings...");

            // crank 808 distortion — hard drift phonk style
            p.bass.drive = 14.0;
            p.bass.pitch_drop = 16.0;
            p.bass.tone = 2000.0;
            p.bass.decay = 0.5;
            p.bass.sustain = 0.35;
            p.bass.glide_ms = 80.0;

            // master compression: smash it
            p.master_fx.comp_on = true;
            p.master_fx.threshold_db = -6.0;
            p.master_fx.ratio = 8.0;
            p.master_fx.makeup_db = 5.0;
            p.master_fx.low_db = 4.0;
            p.master_fx.high_db = 3.0;

            // master chain: saturator → distortion → clipper
            let mut sat = crate::fx::FxSlot::new(crate::fx::FxKind::Saturator);
            sat.p = [12.0, 1.0, 5000.0, 0.65];
            sat.x[0] = 0.0;
            let mut dist = crate::fx::FxSlot::new(crate::fx::FxKind::Distortion);
            dist.p = [0.6, 3000.0, 0.75, -2.0];
            let mut clip = crate::fx::FxSlot::new(crate::fx::FxKind::Clipper);
            clip.p[0] = -1.5;
            p.master_chain.clear();
            p.master_chain.push(sat);
            p.master_chain.push(dist);
            p.master_chain.push(clip);

            p.master_volume = 0.88;
            p.delay_return = 0.3;
            p.delay_feedback = 0.4;
            p.delay_beats = 0.75;

            // sidechain pump on 808 channel for bounce
            if p.duck.len() > crate::mixer::CH_BASS {
                p.duck[crate::mixer::CH_BASS] = 0.35;
            }

            // duplicate existing clips to make the track longer (loop the arrangement 3x)
            let song_end = p.arrangement.clips.iter().map(|c| c.start + c.len).max().unwrap_or(4 * TICKS_PER_BAR);
            let orig_clips: Vec<crate::arrangement::Clip> = p.arrangement.clips.clone();
            for rep in 1..=2u32 {
                for c in &orig_clips {
                    let mut dup = c.clone();
                    dup.start += song_end * rep;
                    p.arrangement.clips.push(dup);
                }
            }

            // add extra hi-hat density to all patterns for bounce
            for pat in &mut p.patterns {
                if pat.drums.len() > 3 {
                    for i in 0..pat.drums[3].len() {
                        if !pat.drums[3][i].on && i % 2 == 1 {
                            pat.drums[3][i] = crate::sequencer::Step::new(true, 45);
                        }
                    }
                }
                // add perc ghost hits for groove
                if pat.drums.len() > 4 {
                    for i in (0..pat.drums[4].len()).step_by(4) {
                        if !pat.drums[4][i.saturating_add(1).min(pat.drums[4].len() - 1)].on {
                            let idx = (i + 1).min(pat.drums[4].len() - 1);
                            pat.drums[4][idx] = crate::sequencer::Step::new(true, 55);
                        }
                    }
                }
            }
        }
        p.song_mode = true;
        let settings = audio_engine::export::ExportSettings { sample_rate: 44100, depth: audio_engine::export::BitDepth::Pcm24, bars: audio_engine::export::default_bars(&p), tail_secs: 2.0 };
        eprintln!("Rendering {} bars at {} Hz...", settings.bars, settings.sample_rate);
        let data = audio_engine::export::render(&p, &settings);
        audio_engine::export::write_wav(std::path::Path::new(output), &data, &settings).unwrap_or_else(|e| { eprintln!("WRITE ERROR: {e}"); std::process::exit(1); });
        eprintln!("Exported to: {output}");
        return Ok(());
    }
    #[cfg(not(target_os = "android"))]
    if relaunch_as_live_copy() {
        return Ok(());
    }
    // `SYNTHESIA_SIZE=360x780` opens a phone-sized window (with SYNTHESIA_MOBILE=1 it shows the phone layout)
    let forced = std::env::var("SYNTHESIA_SIZE").ok().and_then(|s| {
        let (w, h) = s.split_once('x')?;
        Some([w.parse::<f32>().ok()?, h.parse::<f32>().ok()?])
    });
    let win_size = forced.unwrap_or([1440.0, 900.0]);
    let min_size = if forced.is_some() { [200.0, 200.0] } else { [1100.0, 720.0] };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size(win_size).with_min_inner_size(min_size).with_title("Winteria DAW").with_decorations(false).with_resizable(true).with_maximized(forced.is_none()).with_icon(std::sync::Arc::new(make_icon())),
        ..Default::default()
    };
    eframe::run_native("Winteria DAW", options, Box::new(|cc| Ok(Box::new(ui::VinteriaApp::new(cc)))))
}

/// Launch the shared DAW interface from Android’s native activity.
#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(app: android_activity::AndroidApp) {
    // the app's private storage is the working directory: samples, theme, recent projects and the hub folder live there
    if let Some(dir) = app.internal_data_path() {
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::env::set_current_dir(&dir);
    }
    android_panic_hook();
    use winit::platform::android::EventLoopBuilderExtAndroid;
    let mut options = eframe::NativeOptions::default();
    let activity = app.clone();
    options.event_loop_builder = Some(Box::new(move |builder| {
        builder.with_android_app(app);
    }));
    let _ = eframe::run_native("Winteria DAW", options, Box::new(move |cc| {
        let mut app = ui::VinteriaApp::new(cc);
        app.android_updater = Some(android_update::Updater::new(activity));
        Ok(Box::new(app))
    }));
}

/// A panic on the phone closes the app without a trace: write it (with a backtrace) to `crash.txt` in the app's storage and to
/// logcat, so the next start can show what happened.
#[cfg(target_os = "android")]
fn android_panic_hook() {
    extern "C" {
        fn __android_log_write(prio: i32, tag: *const std::os::raw::c_char, text: *const std::os::raw::c_char) -> i32;
    }
    std::panic::set_hook(Box::new(|info| {
        let t = std::thread::current();
        let bt = std::backtrace::Backtrace::force_capture();
        let report = format!("Winteria {} crashed\n[thread {}] {}\n\n{}", env!("CARGO_PKG_VERSION"), t.name().unwrap_or("unnamed"), info, bt);
        let _ = std::fs::write(ui::CRASH_FILE, &report);
        if let (Ok(tag), Ok(text)) = (std::ffi::CString::new("Winteria"), std::ffi::CString::new(report.replace('\0', " "))) {
            unsafe {
                __android_log_write(6, tag.as_ptr(), text.as_ptr());
            }
        }
    }));
}

/// Route named saves to private app storage until Android file pickers are available.
#[cfg(target_os = "android")]
pub mod rfd {
    use std::path::{Path, PathBuf};
    #[derive(Default)]
    pub struct FileDialog {
        directory: PathBuf,
        file_name: Option<String>,
    }
    impl FileDialog {
        pub fn new() -> Self {
            Self::default()
        }
        pub fn add_filter(self, _name: &str, _ext: &[&str]) -> Self {
            self
        }
        pub fn set_title(self, _t: impl Into<String>) -> Self {
            self
        }
        pub fn set_directory(mut self, directory: impl AsRef<Path>) -> Self {
            self.directory = directory.as_ref().to_path_buf();
            self
        }
        pub fn set_file_name(mut self, name: impl Into<String>) -> Self {
            self.file_name = Some(name.into());
            self
        }
        pub fn pick_file(self) -> Option<PathBuf> {
            None
        }
        pub fn save_file(self) -> Option<PathBuf> {
            let name = self.file_name?;
            let name = Path::new(&name).file_name()?;
            let path = self.directory.join(name);
            if !path.exists() {
                return Some(path);
            }

            let stem = path.file_stem()?.to_str()?;
            let extension = path.extension()?.to_str()?;
            (2..).map(|number| self.directory.join(format!("{stem}-{number}.{extension}")))
                .find(|candidate| !candidate.exists())
        }
        pub fn pick_folder(self) -> Option<PathBuf> {
            None
        }
    }
}
