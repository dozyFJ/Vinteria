//! Serum-style wavetable synth. Tables are generated once (additive synthesis, 32 morph frames each) in several
//! band-limited versions ("mips"), and the version matching the note pitch is played so that high notes never alias.
use super::{FilterType, WavetableParams, WtTarget};
use crate::audio_engine::dsp::{Adsr, Rng};
use crate::effects::Svf;
use crate::piano_roll::midi_to_freq;
use std::f32::consts::TAU;
use std::sync::OnceLock;
use crate::audio_engine::fastmath::Fast;

pub const FRAME: usize = 2048;
pub const FRAMES: usize = 32;
pub const MAX_UNISON: usize = 7;
/// band-limited versions: 64, 32, 16, 8, 4, 2 harmonics
pub const MIPS: usize = 6;
const MAX_HARMONICS: usize = 64;
const VOICES: usize = 12;
/// filter / morph modulation is evaluated every CONTROL samples (cheap, inaudible)
const CONTROL: u32 = 8;
/// most unison oscillators (per table) one synth runs at a time
const UNISON_BUDGET: usize = 42;

pub struct Table {
    pub name: &'static str,
    /// mips[m][frame][sample]
    pub mips: Vec<Vec<Vec<f32>>>,
}

pub fn tables() -> &'static Vec<Table> {
    static T: OnceLock<Vec<Table>> = OnceLock::new();
    T.get_or_init(|| {
        vec![
            build("Basic Shapes", |t, h| {
                let hf = h as f32;
                let sine = if h == 1 { 1.0 } else { 0.0 };
                let tri = if h % 2 == 1 { (if (h / 2) % 2 == 0 { 1.0 } else { -1.0 }) / (hf * hf) } else { 0.0 };
                let saw = 1.0 / hf;
                let sq = if h % 2 == 1 { 1.0 / hf } else { 0.0 };
                let shapes = [sine, tri, saw, sq];
                let x = t * 3.0;
                let i = (x.floor() as usize).min(2);
                let f = x - i as f32;
                shapes[i] * (1.0 - f) + shapes[i + 1] * f
            }),
            build("Digital Sweep", |t, h| {
                let hf = h as f32;
                let centre = 2.0 + t * 30.0;
                (1.0 / hf.powf(1.2 - t * 0.8)) * (1.0 + 3.0 * (-((hf - centre) * (hf - centre)) / 6.0).exp())
            }),
            build("Vowels", |t, h| {
                let hf = h as f32;
                let g = |c: f32, w: f32, a: f32| a * (-((hf - c) / w).powi(2)).exp();
                (g(3.0 + 3.0 * t, 1.5, 1.0) + g(8.0 + 10.0 * t, 2.5, 0.7) + g(18.0 + 8.0 * t, 3.0, 0.4)) / hf.powf(0.4)
            }),
            build("Metal FM", |t, h| {
                let r = ((h as u32).wrapping_mul(7919).wrapping_add((t * 31.0) as u32 * 104729) % 97) as f32 / 97.0;
                (r * 2.0 - 1.0) / (h as f32).powf(0.8)
            }),
            // 4: drawbar organ - the morph pulls the drawbars out, from soft to full
            build("Organ", |t, h| {
                let bars = [(1usize, 0.8f32), (2, 1.0), (3, 0.7), (4, 0.9), (6, 0.5), (8, 0.7), (10, 0.35), (12, 0.3), (16, 0.45)];
                bars.iter().find(|(n, _)| *n == h).map_or(0.0, |(_, a)| a * (0.25 + 0.75 * t))
                    + if h == 1 { 0.5 * (1.0 - t) } else { 0.0 }
            }),
            // 5: flute / breath - nearly a sine, a faint 2nd and 3rd harmonic that the morph adds
            build("Flute", |t, h| {
                let hf = h as f32;
                match h {
                    1 => 1.0,
                    2 => 0.12 + 0.25 * t,
                    3 => 0.04 + 0.2 * t,
                    _ => (0.05 * t) / hf.powf(1.5),
                }
            }),
            // 6: reeds (accordion / clarinet / harmonium) - odd harmonics at one end, all harmonics with a nasal peak at the other
            build("Reeds", |t, h| {
                let hf = h as f32;
                let odd = if h % 2 == 1 { 1.0 / hf.powf(0.9) } else { 0.04 / hf };
                let reed = (1.0 / hf.powf(0.75)) * (1.0 + 1.6 * (-((hf - 7.0) * (hf - 7.0)) / 10.0).exp());
                odd * (1.0 - t) + reed * t
            }),
            // 7: brass - the upper harmonics open up with the morph, like a swell on a horn
            build("Brass", |t, h| {
                let hf = h as f32;
                let cut = 3.0 + t * 26.0;
                (1.0 / hf.powf(0.8)) * (-(hf / cut).powi(2)).exp() * if h == 1 { 0.8 } else { 1.0 }
            }),
            // 8: soft saw - a clean saw that brightens with the morph, for warm leads and strings
            build("Soft Saw", |t, h| {
                let hf = h as f32;
                (1.0 / hf) * (-(hf / (6.0 + 40.0 * t)).powi(2)).exp()
            }),
            // 9: plucked string - the harmonics of a string struck at a point; the morph moves the point from the bridge (bright) to the neck (round)
            build("String", |t, h| {
                let hf = h as f32;
                let at = 0.08 + 0.4 * t;
                ((std::f32::consts::PI * hf * at).sin().abs()) / hf.powf(1.1)
            }),
        ]
    })
}

fn build(name: &'static str, amp: impl Fn(f32, usize) -> f32) -> Table {
    // exact sine lookup: sin(2*pi*k*i/FRAME) == sine[(k*i) % FRAME]
    let sine: Vec<f32> = (0..FRAME).map(|i| (i as f32 / FRAME as f32 * TAU).sin()).collect();
    let mut mips = Vec::with_capacity(MIPS);
    for m in 0..MIPS {
        let hmax = MAX_HARMONICS >> m;
        let mut frames = Vec::with_capacity(FRAMES);
        for f in 0..FRAMES {
            let t = f as f32 / (FRAMES - 1) as f32;
            let amps: Vec<f32> = (1..=hmax).map(|h| amp(t, h)).collect();
            let mut w = vec![0.0f32; FRAME];
            for (i, v) in w.iter_mut().enumerate() {
                let mut acc = 0.0;
                for (k, a) in amps.iter().enumerate() {
                    acc += a * sine[(i * (k + 1)) % FRAME];
                }
                *v = acc;
            }
            let peak = w.iter().fold(1e-6f32, |mx, v| mx.max(v.abs()));
            for v in w.iter_mut() {
                *v *= 0.9 / peak;
            }
            frames.push(w);
        }
        mips.push(frames);
    }
    Table { name, mips }
}

/// Which band-limited version to use for a fundamental of `freq` Hz (no harmonic above 0.45 * sample rate).
pub fn mip_for(freq: f32, sr: f32) -> usize {
    let allowed = 0.45 * sr / freq.max(1.0);
    let mut m = 0;
    while m + 1 < MIPS && (MAX_HARMONICS >> m) as f32 > allowed {
        m += 1;
    }
    m
}

/// Wavetable lookup: `pos` 0..1 morphs between frames, `phase` 0..1 within the cycle.
#[inline]
pub fn lookup(t: &Table, mip: usize, pos: f32, phase: f32) -> f32 {
    let frames = &t.mips[mip.min(MIPS - 1)];
    let fp = pos.clamp(0.0, 1.0) * (FRAMES - 1) as f32;
    let f0 = fp as usize;
    let f1 = (f0 + 1).min(FRAMES - 1);
    let ff = fp - f0 as f32;
    let x = phase * FRAME as f32;
    let i = x as usize % FRAME;
    let j = (i + 1) % FRAME;
    let fr = x - x.floor();
    let a = frames[f0][i] * (1.0 - fr) + frames[f0][j] * fr;
    let b = frames[f1][i] * (1.0 - fr) + frames[f1][j] * fr;
    a * (1.0 - ff) + b * ff
}


const FRAME_MASK: usize = FRAME - 1;

/// the two morph frames around `pos` and the blend between them
#[inline]
fn frame_pair(pos: f32) -> (usize, usize, f32) {
    let fp = pos.clamp(0.0, 1.0) * (FRAMES - 1) as f32;
    let f0 = fp as usize;
    (f0, (f0 + 1).min(FRAMES - 1), fp - f0 as f32)
}

/// one frame as a fixed-size array, so indexing it with `& FRAME_MASK` needs no bounds checks
#[inline]
fn frame_ref(t: &Table, mip: usize, frame: usize) -> &[f32; FRAME] {
    t.mips[mip.min(MIPS - 1)][frame].as_slice().try_into().expect("frames are FRAME samples long")
}

/// one oscillator: linear interpolation inside the frame, then between the two morph frames
#[inline]
fn osc(a: &[f32; FRAME], b: &[f32; FRAME], ff: f32, phase: f32) -> f32 {
    let x = phase * FRAME as f32;
    let i = x as usize;
    let fr = x - i as f32;
    let (i, j) = (i & FRAME_MASK, (i + 1) & FRAME_MASK);
    let va = a[i] + (a[j] - a[i]) * fr;
    if ff < 0.002 {
        return va;
    }
    let vb = b[i] + (b[j] - b[i]) * fr;
    va + (vb - va) * ff
}

#[derive(Clone, Copy)]
struct Voice {
    active: bool,
    fresh: bool,
    note: u8,
    vel: f32,
    freq: f32,
    mip_a: usize,
    mip_b: usize,
    pa: [f32; MAX_UNISON],
    pb: [f32; MAX_UNISON],
    sub: f32,
    amp: Adsr,
    fenv: Adsr,
    filt: Svf,
    tick: u32,
    cut: f32,
    posmod: f32,
    end_tick: u64,
    age: u64,
    tgt: f32,
    gl: f32,
}

pub struct WtSynth {
    voices: [Voice; VOICES],
    lfo: f32,
    counter: u64,
    rng: Rng,
}

impl WtSynth {
    pub fn new() -> Self {
        let _ = tables(); // generate on the constructing thread, never in the audio callback
        let v = Voice {
            active: false,
            fresh: false,
            note: 0,
            vel: 0.0,
            freq: 440.0,
            mip_a: 0,
            mip_b: 0,
            pa: [0.0; MAX_UNISON],
            pb: [0.0; MAX_UNISON],
            sub: 0.0,
            amp: Adsr::new(),
            fenv: Adsr::new(),
            filt: Svf::default(),
            tick: 0,
            cut: 1000.0,
            posmod: 0.0,
            end_tick: u64::MAX,
            age: 0,
            tgt: 440.0,
            gl: 0.0,
        };
        WtSynth { voices: [v; VOICES], lfo: 0.0, counter: 0, rng: Rng::new(777) }
    }

    pub fn note_on(&mut self, note: u8, vel: f32, end_tick: u64) {
        self.counter += 1;
        let idx = self.voices.iter().position(|v| !v.active).unwrap_or_else(|| {
            self.voices.iter().enumerate().min_by_key(|(_, v)| v.age).map(|(i, _)| i).unwrap_or(0)
        });
        let mut pa = [0.0; MAX_UNISON];
        let mut pb = [0.0; MAX_UNISON];
        for k in 0..MAX_UNISON {
            // random start phases give the typical unison "width"
            pa[k] = (self.rng.noise() * 0.5 + 0.5).ffract();
            pb[k] = (self.rng.noise() * 0.5 + 0.5).ffract();
        }
        let v = &mut self.voices[idx];
        v.active = true;
        v.fresh = true;
        v.note = note;
        v.vel = vel;
        v.freq = midi_to_freq(note as f32);
        v.pa = pa;
        v.pb = pb;
        v.sub = 0.0;
        v.amp = Adsr::new();
        v.amp.gate_on();
        v.fenv = Adsr::new();
        v.fenv.gate_on();
        v.filt.reset();
        v.tick = 0;
        v.end_tick = end_tick;
        v.age = self.counter;
        v.tgt = v.freq;
        v.gl = 0.0;
    }

    /// FL-style slide: the newest sounding voice glides to `note` over the length of the slide note instead of a new voice starting.
    /// Returns false when nothing is sounding (the caller then plays the note normally).
    pub fn slide_to(&mut self, note: u8, vel: f32, end_tick: u64, glide_s: f32, sr: f32) -> bool {
        let Some(v) = self.voices.iter_mut().filter(|v| v.active && !v.amp.is_released()).max_by_key(|v| v.age) else { return false };
        v.note = note;
        v.vel = vel;
        v.end_tick = end_tick;
        v.tgt = midi_to_freq(note as f32);
        v.gl = 1.0 - (-1.0 / (glide_s.max(0.01) * 0.3 * sr)).exp();
        true
    }

    pub fn note_off(&mut self, note: u8) {
        for v in self.voices.iter_mut() {
            if v.active && v.note == note && v.end_tick == u64::MAX && !v.amp.is_released() {
                v.amp.gate_off();
                v.fenv.gate_off();
            }
        }
    }

    pub fn release_due(&mut self, abs_tick: u64) {
        for v in self.voices.iter_mut() {
            if v.active && v.end_tick <= abs_tick && !v.amp.is_released() {
                v.amp.gate_off();
                v.fenv.gate_off();
            }
        }
    }

    pub fn release_all(&mut self) {
        for v in self.voices.iter_mut() {
            v.amp.gate_off();
            v.fenv.gate_off();
        }
    }

    #[inline]
    pub fn render_sample(&mut self, p: &WavetableParams, sr: f32) -> f32 {
        let active = self.voices.iter().filter(|v| v.active).count();
        if active == 0 {
            return 0.0;
        }
        let tabs = tables();
        let ta = &tabs[p.table_a.min(tabs.len() - 1)];
        let tb = &tabs[p.table_b.min(tabs.len() - 1)];
        let lfo = (self.lfo * TAU).fsin();
        self.lfo = (self.lfo + p.lfo_rate / sr).ffract();
        let l = lfo * p.lfo_depth;
        // CPU guard: a big chord on a thick unison patch would run hundreds of oscillators per sample (crackling with several
        // synths), so the unison stack thins out once more than UNISON_BUDGET oscillators would be needed
        let n = (p.unison as usize).clamp(1, MAX_UNISON).min((UNISON_BUDGET / active).max(2));
        let norm = 1.0 / (n as f32).sqrt();
        // per-sample constants, hoisted out of the voice / unison loops
        let mut det = [1.0f32; MAX_UNISON];
        for (k, d) in det.iter_mut().enumerate().take(n) {
            let off = if n == 1 { 0.0 } else { (k as f32 / (n - 1) as f32 * 2.0 - 1.0) * p.uni_detune };
            *d = (off / 1200.0).fexp2();
        }
        let mul_a = (p.oct_a / 12.0).fexp2();
        let mul_b = (p.oct_b / 12.0).fexp2();
        let pitch_lfo = if p.lfo_target == WtTarget::Pitch { (l * 2.0 / 12.0).fexp2() } else { 1.0 };
        let max_det = det[n - 1].max(1.0);
        let amp_l = if p.lfo_target == WtTarget::Amp { 1.0 - p.lfo_depth * (0.5 + 0.5 * lfo) } else { 1.0 };
        let mut out = 0.0;
        for v in self.voices.iter_mut() {
            if !v.active {
                continue;
            }
            if v.gl > 0.0 {
                v.freq += (v.tgt - v.freq) * v.gl;
                // the band-limited tables follow the pitch
                if v.tick % 128 == 0 {
                    v.fresh = true;
                }
            }
            if v.fresh {
                v.fresh = false;
                v.mip_a = super::wavetable::mip_for(v.freq * mul_a * max_det * 1.06, sr);
                v.mip_b = super::wavetable::mip_for(v.freq * mul_b * max_det * 1.06, sr);
            }
            let fe = v.fenv.next(p.f_attack, p.f_decay, p.f_sustain, p.f_release, sr);
            let ae = v.amp.next(p.attack, p.decay, p.sustain, p.release, sr);
            if v.tick % CONTROL == 0 {
                v.posmod = fe * p.pos_env + if p.lfo_target == WtTarget::Position { l * 0.5 } else { 0.0 };
                v.cut = (p.cutoff * (p.env_amt * fe + if p.lfo_target == WtTarget::Cutoff { l * 2.0 } else { 0.0 }).fexp2()).clamp(20.0, 19000.0);
            }
            v.tick = v.tick.wrapping_add(1);
            let base = v.freq * pitch_lfo;
            let (pos_a, pos_b) = ((p.pos_a + v.posmod).clamp(0.0, 1.0), (p.pos_b + v.posmod).clamp(0.0, 1.0));
            let (fa, fb) = (base * mul_a / sr, base * mul_b / sr);
            let mut s = 0.0;
            // the morph frames and mip are the same for every unison oscillator: pick them once per voice
            if p.level_a > 0.001 {
                let (f0, f1, ff) = frame_pair(pos_a);
                let (a0, a1) = (frame_ref(ta, v.mip_a, f0), frame_ref(ta, v.mip_a, f1));
                let mut acc = 0.0;
                for k in 0..n {
                    acc += osc(a0, a1, ff, v.pa[k]);
                    v.pa[k] = (v.pa[k] + fa * det[k]).ffract();
                }
                s += acc * p.level_a;
            }
            if p.level_b > 0.001 {
                let (f0, f1, ff) = frame_pair(pos_b);
                let (b0, b1) = (frame_ref(tb, v.mip_b, f0), frame_ref(tb, v.mip_b, f1));
                let mut acc = 0.0;
                for k in 0..n {
                    acc += osc(b0, b1, ff, v.pb[k]);
                    v.pb[k] = (v.pb[k] + fb * det[k]).ffract();
                }
                s += acc * p.level_b;
            }
            s *= norm;
            if p.sub > 0.001 {
                v.sub = (v.sub + base * 0.5 / sr).ffract();
                s += (v.sub * TAU).fsin() * p.sub;
            }
            if p.noise > 0.001 {
                s += self.rng.noise() * p.noise * 0.5;
            }
            let (lp, hp) = v.filt.process(s, v.cut, p.res, sr);
            let y = match p.filter {
                FilterType::Lp => lp,
                FilterType::Hp => hp,
                FilterType::Bp => s - lp - hp,
            };
            out += y * ae * v.vel * amp_l;
            if v.amp.is_idle() {
                v.active = false;
            }
        }
        // gentle saturation: loud chords round off instead of clipping harshly
        let y = out * p.volume * 0.3;
        y / (1.0 + y.abs() * 0.4)
    }
}

#[cfg(test)]
mod slide_tests {
    use super::*;

    #[test]
    fn a_slide_reuses_the_sounding_voice_and_glides_to_the_new_pitch() {
        let mut s = WtSynth::new();
        let p = WavetableParams::default();
        s.note_on(57, 0.8, u64::MAX);
        for _ in 0..2000 {
            s.render_sample(&p, 48000.0);
        }
        assert!(s.slide_to(69, 0.8, u64::MAX, 0.2, 48000.0));
        assert_eq!(s.voices.iter().filter(|v| v.active).count(), 1, "no second voice starts");
        for _ in 0..48000 {
            s.render_sample(&p, 48000.0);
        }
        let v = s.voices.iter().find(|v| v.active).unwrap();
        let target = midi_to_freq(69.0);
        assert!((v.freq - target).abs() < target * 0.02, "the pitch arrived: {} vs {}", v.freq, target);
        // nothing sounding: the slide asks for a normal note
        let mut e = WtSynth::new();
        assert!(!e.slide_to(60, 0.8, u64::MAX, 0.2, 48000.0));
    }
}

#[cfg(test)]
mod preset_tests {
    use super::*;

    #[test]
    fn every_new_preset_makes_a_clean_audible_note() {
        for name in ["Drawbar Organ", "Rock Organ", "Flute", "Pan Flute", "Accordion", "Clarinet", "Brass Lead", "Warm Lead", "Square Lead", "Epic Lead", "Choir Pad", "String Ensemble", "Electric Piano", "Donk", "Hardbass Donk", "Hard Sub", "Reese Bass", "Screech Lead", "Donk Funk", "Funk Bass", "Funk Stab", "Drift House Stab", "Deep House Chord"] {
            let p = WavetableParams::preset(name);
            let mut s = WtSynth::new();
            s.note_on(60, 0.8, u64::MAX);
            let (mut peak, mut ok) = (0.0f32, true);
            for _ in 0..48000 {
                let v = s.render_sample(&p, 48000.0);
                ok &= v.is_finite();
                peak = peak.max(v.abs());
            }
            assert!(ok, "{name}: not finite");
            assert!(peak > 0.05 && peak < 1.6, "{name}: peak {peak}");
        }
    }
}

#[cfg(test)]
mod guitar_tests {
    use super::*;

    #[test]
    fn guitar_presets_are_audible_and_decay() {
        for name in ["Acoustic Guitar", "Nylon Guitar", "Clean Electric", "Overdrive Guitar", "Muted Guitar", "12 String"] {
            let p = WavetableParams::preset(name);
            assert_eq!(p.table_a, 9, "{name}");
            let mut s = WtSynth::new();
            s.note_on(52, 0.8, u64::MAX);
            let (mut early, mut late) = (0.0f32, 0.0f32);
            for i in 0..96000 {
                let v = s.render_sample(&p, 48000.0);
                assert!(v.is_finite(), "{name}");
                if i < 24000 { early = early.max(v.abs()); }
                if i > 72000 { late = late.max(v.abs()); }
            }
            assert!(early > 0.05 && early < 1.6, "{name}: early {early}");
            assert!(late < early, "{name}: a plucked string must fade ({late} vs {early})");
        }
    }
}

#[cfg(test)]
mod bench_tests {
    use super::*;

    /// prints how much of the real-time budget 6 synths playing 6-note chords use (run with --nocapture)
    #[test]
    fn six_synths_cost() {
        for name in ["Super Saw", "Epic Lead", "Rock Organ", "Growl Bass"] {
            let p = WavetableParams::preset(name);
            let mut synths: Vec<WtSynth> = (0..6).map(|_| WtSynth::new()).collect();
            for s in synths.iter_mut() {
                for n in [48u8, 52, 55, 59, 62, 67] {
                    s.note_on(n, 0.8, u64::MAX);
                }
            }
            let n = 48000 * 2;
            let t0 = std::time::Instant::now();
            let mut acc = 0.0f32;
            for _ in 0..n {
                for s in synths.iter_mut() {
                    acc += s.render_sample(&p, 48000.0);
                }
            }
            let secs = t0.elapsed().as_secs_f64();
            println!("BENCH {name}: {:.1}% of real time ({acc:.1})", secs / 2.0 * 100.0);
        }
    }
}
