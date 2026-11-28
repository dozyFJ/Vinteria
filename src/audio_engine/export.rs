//! Offline WAV export with verification of the written file.
use super::Engine;
use crate::project::Project;
use crate::sequencer::{PPQ, TICKS_PER_BAR};
use std::path::Path;
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum BitDepth {
    Pcm16,
    Pcm24,
    Float32,
}

impl BitDepth {
    pub fn label(self) -> &'static str {
        match self {
            BitDepth::Pcm16 => "16-bit PCM",
            BitDepth::Pcm24 => "24-bit PCM",
            BitDepth::Float32 => "32-bit float",
        }
    }
}

#[derive(Clone, Copy)]
pub struct ExportSettings {
    pub sample_rate: u32,
    pub depth: BitDepth,
    /// number of bars to render (the song length in song mode)
    pub bars: u32,
    pub tail_secs: f32,
}

/// Length in bars that an export should cover for this project.
pub fn default_bars(p: &Project) -> u32 {
    if p.song_mode {
        let e = p.arrangement.song_end_bar();
        if e > 0 {
            return e;
        }
        p.loop_bars
    } else {
        p.patterns
            .get(p.current_pattern)
            .map(|pt| (pt.len_ticks() + TICKS_PER_BAR - 1) / TICKS_PER_BAR)
            .unwrap_or(1)
            .max(1)
    }
}

/// Render the project offline (interleaved stereo f32).
pub fn render(project: &Project, s: &ExportSettings) -> Vec<f32> {
    let mut p = project.clone();
    p.loop_bars = if p.song_mode { s.bars.max(1) } else { p.loop_bars };
    if p.song_mode {
        p.loop_on = true;
        p.loop_start = 0;
    }
    let mut e = Engine::new(s.sr());
    e.set_project(Arc::new(p));
    e.play();
    let spt = 60.0 / (project.bpm as f64 * PPQ as f64) * s.sr() as f64;
    let main_frames = (spt * (s.bars * TICKS_PER_BAR) as f64).round() as usize;
    let tail_frames = (s.tail_secs * s.sr()) as usize;
    let mut out = vec![0.0f32; (main_frames + tail_frames) * 2];
    let (a, b) = out.split_at_mut(main_frames * 2);
    for chunk in a.chunks_mut(2048) {
        e.render(chunk);
    }
    e.stop();
    for chunk in b.chunks_mut(2048) {
        e.render(chunk);
    }
    out
}

/// Render a song interval at its original timeline position. Discard the pre-roll
/// so sustained notes, sample offsets, automation and effect state reach the start intact.
pub fn render_range(project: &Project, settings: &ExportSettings, start: u32, end: u32) -> Result<Vec<f32>, String> {
    if end <= start { return Err("Empty render range".into()); }
    let bpm = project.bpm.clamp(20.0, 400.0);
    let frames_at = |tick| (f64::from(tick) * 60.0 * f64::from(settings.sample_rate)
        / (f64::from(bpm) * f64::from(PPQ))).round() as usize;
    let first = frames_at(start);
    let last = frames_at(end);
    if last > settings.sample_rate as usize * 600 || last.saturating_sub(first) > settings.sample_rate as usize * 300 {
        return Err("Choose a shorter range within the first ten minutes.".into());
    }
    let mut project = project.clone();
    project.bpm = bpm;
    project.song_mode = true;
    project.metronome = false;
    project.loop_on = true;
    project.loop_start = 0;
    project.loop_bars = end.div_ceil(TICKS_PER_BAR).saturating_add(1);
    let mut engine = Engine::new(settings.sr());
    engine.set_project(Arc::new(project));
    engine.play();
    let mut scratch = [0.0; 2048];
    let mut discard = first * 2;
    while discard > 0 {
        let count = discard.min(scratch.len());
        engine.render(&mut scratch[..count]);
        discard -= count;
    }
    let mut out = vec![0.0; (last - first) * 2];
    for chunk in out.chunks_mut(scratch.len()) {
        engine.render(chunk);
    }
    Ok(out)
}

impl ExportSettings {
    fn sr(&self) -> f32 {
        self.sample_rate as f32
    }
}

pub fn write_wav(path: &Path, data: &[f32], s: &ExportSettings) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|error| error.to_string())?;
    write_wav_to(std::io::BufWriter::new(file), data, s)
}

pub(crate) fn write_wav_to(output: impl std::io::Write + std::io::Seek, data: &[f32], s: &ExportSettings) -> Result<(), String> {
    let (bits, fmt) = match s.depth {
        BitDepth::Pcm16 => (16, hound::SampleFormat::Int),
        BitDepth::Pcm24 => (24, hound::SampleFormat::Int),
        BitDepth::Float32 => (32, hound::SampleFormat::Float),
    };
    let spec = hound::WavSpec { channels: 2, sample_rate: s.sample_rate, bits_per_sample: bits, sample_format: fmt };
    let mut w = hound::WavWriter::new(output, spec).map_err(|e| e.to_string())?;
    let mut seed = 0x9E37_79B9u32;
    let mut rnd = move || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed as f32 / u32::MAX as f32
    };
    for &v in data {
        let v = v.clamp(-1.0, 1.0);
        match s.depth {
            BitDepth::Pcm16 => {
                // TPDF dither, 1 LSB
                let d = (rnd() - rnd()) / 32768.0;
                let q = ((v + d) * 32767.0).round().clamp(-32768.0, 32767.0) as i16;
                w.write_sample(q).map_err(|e| e.to_string())?;
            }
            BitDepth::Pcm24 => {
                let q = (v * 8_388_607.0).round() as i32;
                w.write_sample(q).map_err(|e| e.to_string())?;
            }
            BitDepth::Float32 => w.write_sample(v).map_err(|e| e.to_string())?,
        }
    }
    w.finalize().map_err(|e| e.to_string())
}

/// Re-open the written file and compare it with the rendered data.
pub fn verify_wav(path: &Path, data: &[f32], s: &ExportSettings) -> Result<String, String> {
    let mut r = hound::WavReader::open(path).map_err(|e| format!("cannot re-open: {e}"))?;
    let spec = r.spec();
    let expect_bits = match s.depth {
        BitDepth::Pcm16 => 16,
        BitDepth::Pcm24 => 24,
        BitDepth::Float32 => 32,
    };
    if spec.channels != 2 || spec.sample_rate != s.sample_rate || spec.bits_per_sample != expect_bits {
        return Err(format!("header mismatch: {:?}", spec));
    }
    if r.len() as usize != data.len() {
        return Err(format!("length mismatch: file {} vs rendered {}", r.len(), data.len()));
    }
    let tol = match s.depth {
        BitDepth::Pcm16 => 3.0 / 32768.0,
        BitDepth::Pcm24 => 2.0 / 8_388_608.0,
        BitDepth::Float32 => 0.0,
    };
    let mut max_diff = 0.0f32;
    let mut peak = 0.0f32;
    let mut n = 0usize;
    let mut check = |v: f32, orig: f32| -> Result<(), String> {
        if !v.is_finite() {
            return Err("non-finite sample in file".into());
        }
        max_diff = max_diff.max((v - orig.clamp(-1.0, 1.0)).abs());
        peak = peak.max(v.abs());
        Ok(())
    };
    match s.depth {
        BitDepth::Float32 => {
            for (v, o) in r.samples::<f32>().zip(data) {
                check(v.map_err(|e| e.to_string())?, *o)?;
                n += 1;
            }
        }
        BitDepth::Pcm16 => {
            for (v, o) in r.samples::<i16>().zip(data) {
                check(v.map_err(|e| e.to_string())? as f32 / 32767.0, *o)?;
                n += 1;
            }
        }
        BitDepth::Pcm24 => {
            for (v, o) in r.samples::<i32>().zip(data) {
                check(v.map_err(|e| e.to_string())? as f32 / 8_388_607.0, *o)?;
                n += 1;
            }
        }
    }
    if n != data.len() {
        return Err("could not read all samples back".into());
    }
    if max_diff > tol {
        return Err(format!("data mismatch: max difference {max_diff}"));
    }
    if peak < 1e-4 {
        return Err("exported file is silent".into());
    }
    let secs = (data.len() / 2) as f32 / s.sample_rate as f32;
    Ok(format!(
        "OK: {:.1}s, {} Hz, {}, peak {:.1} dBFS, max error {:.2e}",
        secs,
        s.sample_rate,
        s.depth.label(),
        20.0 * peak.log10(),
        max_diff
    ))
}

pub fn export(project: &Project, s: &ExportSettings, path: &Path) -> Result<String, String> {
    let data = render(project, s);
    write_wav(path, &data, s)?;
    verify_wav(path, &data, s)
}

pub fn export_range(project: &Project, settings: &ExportSettings, start: u32, end: u32, path: &Path) -> Result<String, String> {
    let data = render_range(project, settings, start, end)?;
    write_wav(path, &data, settings)?;
    verify_wav(path, &data, settings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_roundtrip_all_depths() {
        let p = Project::demo();
        for depth in [BitDepth::Pcm16, BitDepth::Pcm24, BitDepth::Float32] {
            let s = ExportSettings { sample_rate: 44100, depth, bars: 2, tail_secs: 0.5 };
            let path = std::env::temp_dir().join(format!("synthesia_test_{depth:?}.wav"));
            let msg = export(&p, &s, &path).expect("export+verify");
            assert!(msg.starts_with("OK"), "{msg}");
            let _ = std::fs::remove_file(path);
        }
    }

    #[test]
    fn export_length_matches_bpm() {
        let mut p = Project::demo();
        p.bpm = 120.0;
        let s = ExportSettings { sample_rate: 48000, depth: BitDepth::Float32, bars: 2, tail_secs: 0.0 };
        let d = render(&p, &s);
        // 2 bars at 120 bpm = 4 seconds
        assert_eq!(d.len() / 2, 4 * 48000);
    }
}

#[cfg(test)]
mod punch_render_tests {
    use super::*;

    /// RMS of `x` after a band-pass made of two one-pole filters (rough "what earbuds play" band)
    fn band_rms(x: &[f32], sr: f32, lo: f32, hi: f32) -> f32 {
        let (a_hp, a_lp) = ((-std::f32::consts::TAU * lo / sr).exp(), 1.0 - (-std::f32::consts::TAU * hi / sr).exp());
        let (mut hp_prev_x, mut hp_y, mut lp_y, mut sum) = (0.0f32, 0.0f32, 0.0f32, 0.0f64);
        for &v in x {
            hp_y = a_hp * (hp_y + v - hp_prev_x);
            hp_prev_x = v;
            lp_y += a_lp * (hp_y - lp_y);
            sum += (lp_y * lp_y) as f64;
        }
        (sum / x.len().max(1) as f64).sqrt() as f32
    }

    #[test]
    fn punch_lifts_the_earbud_band_of_a_trap_template_and_keeps_the_ceiling() {
        let (_, make) = crate::templates::TEMPLATES.iter().find(|t| t.0.contains("Trap")).unwrap();
        let settings = ExportSettings { sample_rate: 48000, depth: BitDepth::Float32, bars: 4, tail_secs: 0.0 };
        let mut off = make();
        off.punch = 0.0;
        let mut on = make();
        on.punch = 0.6;
        let (a, b) = (render(&off, &settings), render(&on, &settings));
        let mono = |v: &[f32]| v.chunks(2).map(|f| (f[0] + f[1]) * 0.5).collect::<Vec<f32>>();
        let (a, b) = (mono(&a), mono(&b));
        let (band_a, band_b) = (band_rms(&a, 48000.0, 90.0, 360.0), band_rms(&b, 48000.0, 90.0, 360.0));
        let rms = |v: &[f32]| (v.iter().map(|x| (x * x) as f64).sum::<f64>() / v.len() as f64).sqrt() as f32;
        eprintln!("earbud band 90-360 Hz: off {band_a:.4} on {band_b:.4} (+{:.1} dB), overall RMS +{:.1} dB",
            20.0 * (band_b / band_a).log10(), 20.0 * (rms(&b) / rms(&a)).log10());
        assert!(band_b > band_a * 1.12, "Punch should lift the 90–360 Hz band: {band_a} -> {band_b}");
        assert!(b.iter().all(|v| v.is_finite() && v.abs() <= 0.95), "over the ceiling");
    }
}
