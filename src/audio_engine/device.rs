//! Audio device management (cpal) and the lock-free bridge between UI and the audio callback.
use super::{Cmd, Engine};
use crate::mixer::NUM_STRIPS;
use crate::project::Project;
use arc_swap::ArcSwap;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, SampleFormat, SampleRate, StreamConfig};
use crossbeam_channel::{bounded, Receiver, Sender};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, AtomicUsize, Ordering};
use std::sync::Arc;

pub const SCOPE_LEN: usize = 512;
/// raw mono samples for the spectrum analyser / visualizer (power of two)
pub const WAVE_LEN: usize = 8192;

/// State shared between UI thread and audio thread. Only atomics / lock-free structures.
pub struct Shared {
    pub project: ArcSwap<Project>,
    pub cmd_tx: Sender<Cmd>,
    cmd_rx: Receiver<Cmd>,
    garbage_tx: Sender<Arc<Project>>,
    garbage_rx: Receiver<Arc<Project>>,
    peaks: Vec<[AtomicU32; 2]>,
    pub position: AtomicU32,
    /// audio engine load in percent (f32 bits)
    pub cpu: AtomicU32,
    /// progress of the sound preview player (f32 bits, above 1.0 = not playing)
    pub preview_pos: AtomicU32,
    pub playing: AtomicBool,
    pub callbacks: AtomicU64,
    /// master compressor gain reduction in dB (f32 bits)
    pub gr: AtomicU32,
    pub live_track: AtomicU8,
    /// per lane: running clip code and queued (next bar) code, see `session::code`; QUEUED_STOP = stop requested
    pub session_run: Vec<AtomicU32>,
    pub session_queue: Vec<AtomicU32>,
    scope: Vec<AtomicU32>,
    wave: Vec<AtomicU32>,
    wave_idx: AtomicUsize,
    scope_idx: AtomicUsize,
}

impl Shared {
    pub fn new(project: Arc<Project>) -> Arc<Self> {
        let (cmd_tx, cmd_rx) = bounded(1024);
        let (garbage_tx, garbage_rx) = bounded(64);
        Arc::new(Shared {
            project: ArcSwap::from(project),
            cmd_tx,
            cmd_rx,
            garbage_tx,
            garbage_rx,
            peaks: (0..=NUM_STRIPS).map(|_| [AtomicU32::new(0), AtomicU32::new(0)]).collect(),
            position: AtomicU32::new(0),
            cpu: AtomicU32::new(0),
            preview_pos: AtomicU32::new(2.0f32.to_bits()),
            playing: AtomicBool::new(false),
            callbacks: AtomicU64::new(0),
            gr: AtomicU32::new(0),
            live_track: AtomicU8::new(0),
            session_run: (0..crate::session::MAX_LANES).map(|_| AtomicU32::new(0)).collect(),
            session_queue: (0..crate::session::MAX_LANES).map(|_| AtomicU32::new(0)).collect(),
            scope: (0..SCOPE_LEN).map(|_| AtomicU32::new(0)).collect(),
            wave: (0..WAVE_LEN).map(|_| AtomicU32::new(0)).collect(),
            wave_idx: AtomicUsize::new(0),
            scope_idx: AtomicUsize::new(0),
        })
    }
    pub fn send(&self, c: Cmd) {
        let _ = self.cmd_tx.try_send(c);
    }
    pub fn peak(&self, ch: usize) -> [f32; 2] {
        [f32::from_bits(self.peaks[ch][0].load(Ordering::Relaxed)), f32::from_bits(self.peaks[ch][1].load(Ordering::Relaxed))]
    }
    /// The most recent `n` output samples (mono), oldest first.
    pub fn wave_snapshot(&self, n: usize) -> Vec<f32> {
        let end = self.wave_idx.load(Ordering::Relaxed);
        let n = n.min(WAVE_LEN);
        (0..n).map(|i| f32::from_bits(self.wave[(end + WAVE_LEN - n + i) & (WAVE_LEN - 1)].load(Ordering::Relaxed))).collect()
    }
    pub fn scope_snapshot(&self) -> Vec<f32> {
        let idx = self.scope_idx.load(Ordering::Relaxed);
        (0..SCOPE_LEN)
            .map(|i| f32::from_bits(self.scope[(idx + i) % SCOPE_LEN].load(Ordering::Relaxed)))
            .collect()
    }
    /// Free old project snapshots handed back by the audio thread (call from the UI thread).
    pub fn drain_garbage(&self) {
        while self.garbage_rx.try_recv().is_ok() {}
    }
}

mod io {
    use super::*;

    pub struct AudioIo {
        pub shared: Arc<Shared>,
        stream: Option<cpal::Stream>,
        pub devices: Vec<String>,
        pub device_name: String,
        pub sample_rate: u32,
        pub rates: Vec<u32>,
        pub buffer_frames: u32,
        pub error: Option<String>,
        err_flag: Arc<AtomicBool>,
    }

    impl AudioIo {
        pub fn new(shared: Arc<Shared>) -> Self {
            let mut a = AudioIo {
                shared,
                stream: None,
                devices: vec![],
                device_name: String::new(),
                sample_rate: 48000,
                rates: vec![],
                buffer_frames: 0,
                error: None,
                err_flag: Arc::new(AtomicBool::new(false)),
            };
            a.refresh_devices();
            let default = cpal::default_host().default_output_device().and_then(|d| d.name().ok());
            let name = default.or_else(|| a.devices.first().cloned()).unwrap_or_default();
            // 512 frames: a safe margin so other programs / a busy window do not starve the callback (crackling)
            let preferred_rate = if cfg!(target_os = "android") { 48000 } else { 0 };
            crate::boot_log(&format!("audio device '{name}'"));
            a.open(&name, preferred_rate, 512);
            a
        }

        /// No audio device at all (phone safe mode after a crash at start-up).
        pub fn silent(shared: Arc<Shared>) -> Self {
            AudioIo {
                shared,
                stream: None,
                devices: vec![],
                device_name: String::new(),
                sample_rate: 48000,
                rates: vec![],
                buffer_frames: 0,
                error: Some("Safe mode: sound is off".into()),
                err_flag: Arc::new(AtomicBool::new(false)),
            }
        }

        pub fn refresh_devices(&mut self) {
            let host = cpal::default_host();
            self.devices = host.output_devices().map(|it| it.filter_map(|d| d.name().ok()).collect()).unwrap_or_default();
            #[cfg(target_os = "android")]
            if let Some(name) = host.default_output_device().and_then(|device| device.name().ok()) {
                if !self.devices.contains(&name) {
                    self.devices.insert(0, name);
                }
            }
        }

        pub fn has_stream(&self) -> bool {
            self.stream.is_some()
        }

        /// Detects stream errors reported by the driver (e.g. device unplugged).
        pub fn poll_error(&mut self) {
            if self.err_flag.swap(false, Ordering::Relaxed) {
                self.error = Some("Audio stream error (device removed?) - choose a device again".into());
                self.stream = None;
            }
        }

        fn find_device(name: &str) -> Option<cpal::Device> {
            let host = cpal::default_host();
            #[cfg(target_os = "android")]
            if let Some(device) = host.default_output_device().filter(|device| device.name().is_ok_and(|default| default == name)) {
                return Some(device);
            }
            host.output_devices().ok()?.find(|d| d.name().map(|n| n == name).unwrap_or(false))
        }

        /// Open `name` with preferred sample rate (0 = auto) and buffer size in frames (0 = driver default).
        pub fn open(&mut self, name: &str, want_rate: u32, buffer_frames: u32) {
            self.stream = None;
            self.error = None;
            self.shared.playing.store(false, Ordering::Relaxed);
            let Some(device) = Self::find_device(name) else {
                self.error = Some(format!("Audio device '{name}' not found"));
                return;
            };
            self.device_name = name.to_string();
            let ranges: Vec<_> = match device.supported_output_configs() {
                Ok(r) => r.collect(),
                Err(e) => {
                    self.error = Some(format!("Cannot query device: {e}"));
                    return;
                }
            };
            let mut rates: Vec<u32> = [44100u32, 48000, 88200, 96000]
                .into_iter()
                .filter(|r| ranges.iter().any(|c| c.min_sample_rate().0 <= *r && *r <= c.max_sample_rate().0))
                .collect();
            if rates.is_empty() {
                if let Ok(d) = device.default_output_config() {
                    rates.push(d.sample_rate().0);
                }
            }
            self.rates = rates.clone();
            let rate = if want_rate != 0 && rates.contains(&want_rate) {
                want_rate
            } else if let Ok(d) = device.default_output_config() {
                d.sample_rate().0
            } else {
                *rates.first().unwrap_or(&48000)
            };
            // prefer f32, then i16; require >= 2 channels
            let pick = ranges
                .iter()
                .filter(|c| c.channels() >= 2 && c.min_sample_rate().0 <= rate && rate <= c.max_sample_rate().0)
                .min_by_key(|c| match c.sample_format() {
                    SampleFormat::F32 => 0,
                    SampleFormat::I16 => 1,
                    _ => 5,
                });
            let Some(pick) = pick else {
                self.error = Some("No stereo output configuration available".into());
                return;
            };
            let cfg = pick.with_sample_rate(SampleRate(rate));
            let fmt = cfg.sample_format();
            let mut sc: StreamConfig = cfg.into();
            let result = if buffer_frames > 0 {
                sc.buffer_size = BufferSize::Fixed(buffer_frames);
                match self.build(&device, &sc, fmt) {
                    Ok(s) => Ok(s),
                    Err(_) => {
                        sc.buffer_size = BufferSize::Default;
                        self.build(&device, &sc, fmt)
                    }
                }
            } else {
                self.build(&device, &sc, fmt)
            };
            match result {
                Ok(stream) => {
                    if let Err(e) = stream.play() {
                        self.error = Some(format!("Cannot start stream: {e}"));
                        return;
                    }
                    self.sample_rate = rate;
                    self.buffer_frames = if matches!(sc.buffer_size, BufferSize::Fixed(_)) { buffer_frames } else { 0 };
                    self.stream = Some(stream);
                }
                Err(e) => self.error = Some(format!("Cannot open device: {e}")),
            }
        }

        fn build(&self, device: &cpal::Device, sc: &StreamConfig, fmt: SampleFormat) -> Result<cpal::Stream, String> {
            let shared = Arc::clone(&self.shared);
            let channels = sc.channels as usize;
            let sr = sc.sample_rate.0 as f32;
            let err_flag = Arc::clone(&self.err_flag);
            let err_fn = move |_e: cpal::StreamError| err_flag.store(true, Ordering::Relaxed);
            let mut core = CallbackCore::new(shared, sr);
            match fmt {
                SampleFormat::F32 => device
                    .build_output_stream(sc, move |d: &mut [f32], _| core.fill(d, channels, |x| x), err_fn, None)
                    .map_err(|e| e.to_string()),
                SampleFormat::I16 => device
                    .build_output_stream(
                        sc,
                        move |d: &mut [i16], _| core.fill(d, channels, |x| (x.clamp(-1.0, 1.0) * 32767.0) as i16),
                        err_fn,
                        None,
                    )
                    .map_err(|e| e.to_string()),
                f => Err(format!("unsupported sample format {f:?}")),
            }
        }
    }

    /// Everything the audio callback owns. `fill` performs no allocation and takes no locks.
    struct CallbackCore {
        shared: Arc<Shared>,
        engine: Engine,
        scratch: Vec<f32>,
        scope_counter: usize,
        wave_pos: usize,
        prio_set: bool,
    }

    /// The audio callback thread runs above normal priority, so a busy desktop or the UI thread cannot starve it.
    #[cfg(windows)]
    fn raise_audio_thread_priority() {
        extern "system" {
            fn GetCurrentThread() -> isize;
            fn SetThreadPriority(h: isize, p: i32) -> i32;
        }
        unsafe {
            SetThreadPriority(GetCurrentThread(), 15); // THREAD_PRIORITY_TIME_CRITICAL
        }
    }
    #[cfg(not(windows))]
    fn raise_audio_thread_priority() {}

    impl CallbackCore {
        fn new(shared: Arc<Shared>, sr: f32) -> Self {
            let mut engine = Engine::new(sr);
            engine.live_loop = true;
            let p = shared.project.load_full();
            let old = engine.set_project(p);
            drop(old); // initial placeholder, still on the UI thread
            CallbackCore { shared, engine, scratch: vec![0.0; 4096 * 2], scope_counter: 0, wave_pos: 0, prio_set: false }
        }

        fn fill<T: Copy>(&mut self, out: &mut [T], channels: usize, conv: impl Fn(f32) -> T) {
            let started = web_time::Instant::now();
            if !self.prio_set {
                self.prio_set = true;
                raise_audio_thread_priority();
            }
            // 1. pick up a new project snapshot (lock-free); hand the old one back for dropping elsewhere
            {
                let guard = self.shared.project.load();
                if Arc::as_ptr(&*guard) != self.engine.project_ptr() {
                    let new = Arc::clone(&*guard);
                    let old = self.engine.set_project(new);
                    let _ = self.shared.garbage_tx.try_send(old);
                }
            }
            // 2. commands
            while let Ok(c) = self.shared.cmd_rx.try_recv() {
                self.engine.apply(c);
            }
            // 3. render in chunks
            let zero = conv(0.0);
            let frames = out.len() / channels.max(1);
            let mut done = 0;
            while done < frames {
                let n = (frames - done).min(4096);
                self.engine.render(&mut self.scratch[..n * 2]);
                for i in 0..n {
                    let (l, r) = (self.scratch[i * 2], self.scratch[i * 2 + 1]);
                    let o = &mut out[(done + i) * channels..(done + i + 1) * channels];
                    if channels == 1 {
                        o[0] = conv((l + r) * 0.5);
                    } else {
                        o[0] = conv(l);
                        o[1] = conv(r);
                        for x in o.iter_mut().skip(2) {
                            *x = zero;
                        }
                    }
                    self.shared.wave[self.wave_pos & (WAVE_LEN - 1)].store(((l + r) * 0.5).to_bits(), Ordering::Relaxed);
                    self.wave_pos = self.wave_pos.wrapping_add(1);
                    self.scope_counter += 1;
                    if self.scope_counter >= 16 {
                        self.scope_counter = 0;
                        let idx = self.shared.scope_idx.load(Ordering::Relaxed);
                        self.shared.scope[idx % SCOPE_LEN].store(((l + r) * 0.5).to_bits(), Ordering::Relaxed);
                        self.shared.scope_idx.store((idx + 1) % SCOPE_LEN, Ordering::Relaxed);
                    }
                }
                done += n;
            }
            // 4. publish meters / transport
            let peaks = self.engine.take_peaks();
            for (i, p) in peaks.iter().enumerate() {
                self.shared.peaks[i][0].store(p[0].to_bits(), Ordering::Relaxed);
                self.shared.peaks[i][1].store(p[1].to_bits(), Ordering::Relaxed);
            }
            self.shared.position.store(self.engine.position, Ordering::Relaxed);
            self.shared.preview_pos.store(self.engine.preview_progress().unwrap_or(2.0).to_bits(), Ordering::Relaxed);
            self.shared.playing.store(self.engine.playing, Ordering::Relaxed);
            for lane in 0..crate::session::MAX_LANES {
                let (r, q) = self.engine.session_codes(lane);
                self.shared.session_run[lane].store(r, Ordering::Relaxed);
                self.shared.session_queue[lane].store(q, Ordering::Relaxed);
            }
            self.shared.wave_idx.store(self.wave_pos & (WAVE_LEN - 1), Ordering::Relaxed);
            self.shared.gr.store(self.engine.take_gr().to_bits(), Ordering::Relaxed);
            // load: time spent on this buffer / time the buffer lasts, smoothed
            let avail = (out.len() / channels.max(1)) as f32 / self.engine.sr.max(1.0);
            let load = started.elapsed().as_secs_f32() / avail.max(1e-6) * 100.0;
            let old = f32::from_bits(self.shared.cpu.load(Ordering::Relaxed));
            self.shared.cpu.store((old * 0.9 + load * 0.1).to_bits(), Ordering::Relaxed);
            self.shared.callbacks.fetch_add(1, Ordering::Relaxed);
        }
    }

}
pub use io::AudioIo;
