//! Song templates (Phonk Drift, Phonk House) and one-click mix presets.
use crate::arrangement::Clip;
use crate::instruments::{AnalogParams, AnalogWave, Bass808Params, WavetableParams};
use crate::mixer::*;
use crate::piano_roll::{Note, Track};
use crate::project::{MasterFx, Project};
use crate::sequencer::*;

pub const TEMPLATES: [(&str, fn() -> Project); 40] = [
    ("Drift Phonk Hard (cowbell)", drift_hard),
    ("Phonk House 2 (cowbell)", phonk_house_2),
    ("Neon Garage · House Phonk", neon_garage),
    ("Night Oversteer · Drift Phonk", night_oversteer),
    ("Glass Rain · Melodic Type Beat", glass_rain),
    ("Dubstep (wobble)", dubstep),
    ("Pop (bright)", pop_beat),
    ("Melodic Type Beat", melodic_beat),
    ("Trap Beat (808 Mafia)", mafia_808),
    ("Bandana I Style (bouncy trap)", bandana_one),
    ("Brazilian Funk (tamborzao)", brazil_funk),
    ("Dark Trap (Untitled2455)", dark_trap),
    ("OG Buda (dark trap)", og_buda),
    ("Soda Luv x OG Buda (super trap)", soda_luv),
    ("Untitle34 - Bounce (your sounds)", untitle34_bounce),
    ("Untitle34 - Full (+ pad and arp)", untitle34_full),
    ("OG Buda 2 (hard 808 slides)", buda_hard),
    ("OG Buda 3 (dark bells)", buda_bells),
    ("Soda Luv 2 (rolling hats)", soda_rolls),
    ("Yeat Rage", yeat_rage_beat),
    ("Memphis Phonk", memphis_phonk),
    ("Metro Dark Trap", metro_dark),
    ("Lex Luger Hard", lex_luger),
    ("NY Drill", ny_drill),
    ("Russian Melodic Trap", russian_melodic),
    ("Hard Trap (aggressive)", hard_trap),
    ("Hard Trap 2 (rage 808)", hard_trap_2),
    ("Riddim Dubstep", riddim_dubstep),
    ("Riddim Dubstep 2 (tearout)", riddim_tearout),
    ("Lo-fi Hip-Hop", lofi_hiphop),
    ("House (four on the floor)", house_four),
    ("Techno", techno),
    ("Drum and Bass", dnb),
    ("Reggaeton (dembow)", reggaeton),
    ("Synthwave", synthwave),
    ("Boom Bap", boom_bap),
    ("Amapiano (log drum)", amapiano),
    ("Hardbass (donk)", hardbass),
    ("Donk Funk (house)", donk_funk),
    ("Empty project", empty),
];

/// Load the sound files a template refers to (the sampler tracks, the audio pool). A file that is missing falls back to the
/// built-in factory sound of the same kind, so the beat plays even without the kits. `load` reads one WAV.
pub fn hydrate_samples(p: &mut Project, load: &mut dyn FnMut(&std::path::Path) -> Option<std::sync::Arc<crate::project::SampleData>>) {
    let mut fix = |r: &mut crate::project::SampleRef| {
        if r.data.is_some() {
            return;
        }
        let path = std::path::PathBuf::from(&r.path);
        if let Some(d) = load(&path) {
            r.data = Some(d);
            return;
        }
        let low = r.name.to_lowercase();
        let fallback = if low.contains("snare") {
            "Factory Snare.wav"
        } else if low.contains("open") || low.contains("hat") {
            "Factory Hi-Hat.wav"
        } else if low.contains("clap") {
            "Factory Clap.wav"
        } else {
            "Factory Kick.wav"
        };
        let fp = std::path::Path::new("samples").join(fallback);
        if let Some(d) = load(&fp) {
            r.path = fp.to_string_lossy().to_string();
            r.data = Some(d);
        }
    };
    for t in p.arrangement.tracks.iter_mut() {
        if let Some(r) = t.sampler.sample.as_mut() {
            fix(r);
        }
    }
    for r in p.audio_pool.iter_mut() {
        fix(r);
    }
    if let Some(r) = p.sampler.sample.as_mut() {
        fix(r);
    }
}

/// Put a template into the playlist the user already has: its tracks and clips are appended on new lanes (the project's tempo,
/// mixer and the sounds already in use stay), sounds the project does not use yet come from the template.
/// Returns the number of tracks added, or Err when the playlist has no room.
pub fn merge_template(dst: &mut Project, mut t: Project) -> Result<usize, String> {
    use crate::session::MAX_LANES;
    dst.adopt_track_params();
    t.adopt_track_params();
    let lane_off = dst.arrangement.tracks.len();
    if lane_off + t.arrangement.tracks.len() > MAX_LANES {
        return Err(format!("No room: the playlist holds {MAX_LANES} tracks, this template adds {}", t.arrangement.tracks.len()));
    }
    let used = |dst: &Project, tr: Track| dst.arrangement.tracks.iter().any(|x| x.instrument == Some(tr));
    if !used(dst, Track::Synth) {
        dst.synth = t.synth.clone();
    }
    if !used(dst, Track::Bass808) {
        dst.bass = t.bass.clone();
    }
    if !used(dst, Track::Wavetable) {
        dst.wt = t.wt.clone();
    }
    if !used(dst, Track::Analog) {
        dst.analog = t.analog.clone();
    }
    if !used(dst, Track::Sampler) {
        dst.sampler = t.sampler.clone();
    }
    if !used(dst, Track::Drums) {
        dst.drum_samples = t.drum_samples.clone();
        dst.drum_fx = t.drum_fx.clone();
    }
    let pat_off = dst.patterns.len();
    let pool_off = dst.audio_pool.len();
    for mut p in t.patterns.iter().cloned() {
        p.clip_only = true;
        dst.patterns.push(p);
    }
    dst.audio_pool.extend(t.audio_pool.iter().cloned());
    let added = t.arrangement.tracks.len();
    dst.arrangement.tracks.extend(t.arrangement.tracks.iter().cloned());
    for mut c in t.arrangement.clips.iter().cloned() {
        c.lane += lane_off;
        c.pattern += if c.audio { pool_off } else { pat_off };
        dst.arrangement.clips.push(c);
    }
    dst.loop_bars = dst.loop_bars.max(dst.arrangement.song_end_bar().min(256));
    Ok(added)
}

/// The drum sounds (kick, snare, clap, hat, perc; files relative to the samples folder) a template plays with: the kits
/// "808 Mafia", "Phonk Kit" and "Yeat KanKan Kit". Templates without an entry keep the synthesized drums / default kit.
pub fn template_kit(name: &str) -> Option<[&'static str; 5]> {
    // hardbass: only the hardest hits of the packs - distorted kick, snare as a clap layer, bright hats
    const HARDBASS: [&str; 5] = ["Phonk Kit/KICK/AGRESSIVE KICK.wav", "Phonk Kit/SNARE/AGRESSIVE SNARE.wav", "808 Mafia/Claps/TM88 Clap.wav", "Phonk Kit/HIHAT/HIHAT (3).wav", "Phonk Kit/PERC/PERC (4).wav"];
    const FUNKHOUSE: [&str; 5] = ["Phonk Kit/KICK/KICK (2).wav", "808 Mafia/Snares/Young Chop Snare.wav", "808 Mafia/Claps/Southside Clap (1).wav", "808 Mafia/Hi Hats/Southside Hi Hat.wav", "Phonk Kit/PERC/PERC (1).wav"];
    const PHONK: [&str; 5] = ["Phonk Kit/KICK/AGRESSIVE KICK.wav", "Phonk Kit/SNARE/AGRESSIVE SNARE.wav", "808 Mafia/Claps/Metro Boomin' Clap (1).wav", "Phonk Kit/HIHAT/HIHAT (1).wav", "Phonk Kit/PERC/PERC (1).wav"];
    const TRAP: [&str; 5] = ["808 Mafia/Kicks/TM88 Kick.wav", "808 Mafia/Snares/Metro Boomin Snare (1).wav", "808 Mafia/Claps/TM88 Clap.wav", "808 Mafia/Hi Hats/TM88 Hi Hat (1).wav", "Yeat KanKan Kit/Percs/Go To YeatxKan Perc 2.wav"];
    const YEAT: [&str; 5] = ["Phonk Kit/KICK/KICK (3).wav", "Yeat KanKan Kit/Snares/Go To Snare 1.wav", "Yeat KanKan Kit/Claps/Go To Yeat Clap 1.wav", "Yeat KanKan Kit/Hats/HighHat.wav", "Yeat KanKan Kit/Percs/Go To YeatxKan Perc 2.wav"];
    const BAP: [&str; 5] = ["808 Mafia/Kicks/Lex Luger Kick.wav", "808 Mafia/Snares/Lex Luger Snare (1).wav", "808 Mafia/Claps/Southside Clap (1).wav", "808 Mafia/Hi Hats/Lex Luger Hi Hat.wav", "Phonk Kit/PERC/PERC (2).wav"];
    const LOFI: [&str; 5] = ["808 Mafia/Kicks/Southside Kick.wav", "Yeat KanKan Kit/Snares/Bop Snare.wav", "808 Mafia/Claps/Southside Clap (1).wav", "Yeat KanKan Kit/Hats/Click Hat.wav", "Phonk Kit/PERC/PERC (3).wav"];
    match name {
        "Hardbass (donk)" => Some(HARDBASS),
        "Donk Funk (house)" => Some(FUNKHOUSE),
        "Drift Phonk Hard (cowbell)" | "Phonk House 2 (cowbell)" => Some(PHONK),
        "Trap Beat (808 Mafia)" => Some(TRAP),
        "Dark Trap (Untitled2455)" | "Bandana I Style (bouncy trap)" | "Melodic Type Beat" => Some(YEAT),
        "Boom Bap" | "Lex Luger Hard" => Some(BAP),
        "Amapiano (log drum)" => Some(LOFI),
        "Russian Melodic Trap" => Some(YEAT),
        "Hard Trap (aggressive)" => Some(TRAP),
        "Hard Trap 2 (rage 808)" => Some(YEAT),
        "Riddim Dubstep" | "Riddim Dubstep 2 (tearout)" => Some(["Phonk Kit/KICK/AGRESSIVE KICK.wav", "Phonk Kit/SNARE/AGRESSIVE SNARE.wav", "808 Mafia/Claps/TM88 Clap.wav", "Phonk Kit/HIHAT/HIHAT (3).wav", "Phonk Kit/PERC/PERC (4).wav"]),
        "OG Buda (dark trap)" => Some(YEAT),
        "Soda Luv x OG Buda (super trap)" | "Soda Luv 2 (rolling hats)" => Some(["Phonk Kit/KICK/KICK (3).wav", "808 Mafia/Snares/Metro Boomin Snare (1).wav", "Yeat KanKan Kit/Claps/Go To Yeat Clap 1.wav", "808 Mafia/Hi Hats/TM88 Hi Hat (1).wav", "Yeat KanKan Kit/Percs/Go To YeatxKan Perc 2.wav"]),
        "OG Buda 2 (hard 808 slides)" | "OG Buda 3 (dark bells)" => Some(["Phonk Kit/KICK/AGRESSIVE KICK.wav", "Yeat KanKan Kit/Snares/Go To Snare 1.wav", "808 Mafia/Claps/TM88 Clap.wav", "Yeat KanKan Kit/Hats/HighHat.wav", "Yeat KanKan Kit/Percs/Go To YeatxKan Perc 3.wav"]),
        "Memphis Phonk" => Some(PHONK),
        "Metro Dark Trap" => Some(TRAP),
        "Yeat Rage" => Some(["Phonk Kit/KICK/KICK (5).wav", "Yeat KanKan Kit/Snares/Go To Snare 3.wav", "Yeat KanKan Kit/Claps/Go To Yeat Clap 2.wav", "Yeat KanKan Kit/Hats/Hit Hat.wav", "Yeat KanKan Kit/Percs/Go To YeatxKan Perc 3.wav"]),
        "NY Drill" => Some(["Phonk Kit/KICK/KICK (2).wav", "808 Mafia/Snares/Young Chop Snare.wav", "808 Mafia/Claps/Southside Clap (1).wav", "808 Mafia/Hi Hats/Southside Hi Hat.wav", "Yeat KanKan Kit/Percs/Go To YeatxKan Perc 4.wav"]),
        "Lo-fi Hip-Hop" => Some(LOFI),
        _ => None,
    }
}

/// The phonk templates play their hook with a real cowbell from the Phonk Kit (file relative to the samples folder).
pub fn cowbell_sample(name: &str) -> Option<&'static str> {
    match name {
        "Drift Phonk Hard (cowbell)" => Some("Phonk Kit/PERC/cowbell (1).wav"),
        "Phonk House 2 (cowbell)" => Some("Phonk Kit/PERC/cowbell (3).wav"),
        "Memphis Phonk" => Some("Phonk Kit/PERC/cowbell (2).wav"),
        _ => None,
    }
}

/// The cowbell file of a template, only if it is really in the samples folder. Without it the template keeps its hook on
/// the Analog synth: moving the notes to an empty Sampler would make the melody silent.
pub fn cowbell_sample_path(name: &str) -> Option<std::path::PathBuf> {
    let path = std::path::Path::new("samples").join(cowbell_sample(name)?);
    crate::project::load_wav(&path).is_ok().then_some(path)
}

/// Move the cowbell hook from the synthesized Analog voice to the Sampler (which then plays the cowbell sample). The sampler's
/// root note becomes the middle pitch of the hook, so the hook's main notes sound at the sample's own pitch.
pub fn cowbell_to_sampler(p: &mut Project) {
    let mut pitches: Vec<u8> = Vec::new();
    for pat in p.patterns.iter_mut() {
        for n in pat.notes.iter_mut().filter(|n| n.track == Track::Analog) {
            n.track = Track::Sampler;
            pitches.push(n.pitch);
        }
    }
    for t in p.arrangement.tracks.iter_mut() {
        if t.instrument == Some(Track::Analog) {
            t.instrument = Some(Track::Sampler);
            if t.name == "Analog" {
                t.name = "Cowbell".into();
            }
        }
    }
    if !pitches.is_empty() {
        pitches.sort();
        p.sampler.root = pitches[pitches.len() / 2];
    }
}

/// Mixing of a template on the mixer, with the program's own effects: the sound groups go through inserts, each with a short
/// chain (glue compressor, EQ, saturation, delay, reverb, chorus). Applied when a template is loaded, after the levels /
/// pan / sends of its mix preset.
/// Templates that bring their own routing and mix (finish_mix would overwrite it).
pub fn has_own_mix(name: &str) -> bool {
    name.starts_with("Untitle34")
}

pub fn finish_mix(p: &mut Project) {
    use crate::fx::{FxKind, FxSlot};
    p.insert_fx.resize_with(NUM_INSERTS, Vec::new);
    let fx = |kind: FxKind, set: &[(usize, f32)]| {
        let mut s = FxSlot::new(kind);
        for &(i, v) in set {
            s.p[i] = v;
        }
        s
    };
    let sampler = Track::Sampler.channel();
    let groups: Vec<(&str, Vec<usize>, Vec<FxSlot>)> = vec![
        ("Drums", vec![0, 1, 2], vec![fx(FxKind::Compressor, &[(0, -16.0), (1, 3.0), (2, 2.0)]), fx(FxKind::Eq3, &[(0, 1.5), (2, 1.0)])]),
        ("Hats and Perc", vec![3, 4, sampler], vec![fx(FxKind::Eq3, &[(0, -8.0), (2, 2.0)])]),
        ("808", vec![CH_BASS], vec![fx(FxKind::Distortion, &[(0, 0.25), (2, 0.5)]), fx(FxKind::Compressor, &[(0, -18.0), (1, 3.0), (2, 2.0)])]),
        ("Lead", vec![CH_SYNTH], vec![fx(FxKind::Delay, &[(2, 0.2)]), fx(FxKind::Reverb, &[(0, 0.5), (2, 0.18)])]),
        ("Pad and Keys", vec![CH_WT, CH_AN], vec![fx(FxKind::Chorus, &[(2, 0.25)]), fx(FxKind::Reverb, &[(0, 0.75), (2, 0.3)])]),
    ];
    for (i, (name, chans, chain)) in groups.into_iter().enumerate() {
        p.channels[NUM_CH + i].name = name.to_string();
        for c in chans {
            if c < NUM_CH {
                p.channels[c].output = Some(i);
            }
        }
        p.insert_fx[i] = chain;
    }
}

fn empty() -> Project {
    let mut p = Project::default();
    p.arrangement.tracks = crate::arrangement::Arrangement::starter_tracks();
    p
}

const T: u32 = TICKS_PER_STEP;

/// note on a 16th-step grid
fn n(track: Track, step: u32, len_steps: u32, pitch: u8, vel: u8, slide: bool) -> Note {
    Note { start: step * T, len: len_steps * T - 2, pitch, vel, track, slide, mute: false, pan: 0, chance: 100 }
}

fn steps(p: &mut Pattern, lane: usize, list: &[usize], vel: u8) {
    p.set_steps(lane, list, vel);
}

/// 808 cowbell: two pulse oscillators a fifth apart, instant decay.
fn cowbell() -> AnalogParams {
    AnalogParams {
        wave1: AnalogWave::Pulse,
        wave2: AnalogWave::Pulse,
        semi2: 7.0,
        detune2: 2.0,
        mix: 0.5,
        pw: 0.5,
        cutoff: 3600.0,
        res: 0.2,
        drive: 2.0,
        env_amt: 0.4,
        keytrack: 0.6,
        attack: 0.001,
        decay: 0.22,
        sustain: 0.0,
        release: 0.09,
        f_attack: 0.001,
        f_decay: 0.12,
        f_sustain: 0.0,
        f_release: 0.08,
        drift: 0.1,
        volume: 0.8,
        ..AnalogParams::default()
    }
}

fn clips(p: &mut Project, plan: &[(usize, u32)]) {
    if p.arrangement.tracks.is_empty() {
        p.arrangement.add_track(crate::arrangement::TrackKind::Midi);
    }
    let mut bar = 0;
    p.arrangement.clips.clear();
    for &(pat, bars) in plan {
        let pb = ((p.patterns[pat].len_ticks() + TICKS_PER_BAR - 1) / TICKS_PER_BAR).max(1);
        let mut b = 0;
        while b < bars {
            p.arrangement.clips.push(Clip::bars(pat, 0, bar + b, pb, false));
            b += pb;
        }
        bar += bars;
    }
    p.loop_bars = bar;
    // one track per channel (drums + each instrument) instead of whole patterns on a single track
    p.spread_clips_over_tracks();
}

// ---------------------------------------------------------------- Phonk Drift
pub fn phonk_drift() -> Project {
    house_style(HouseSpec {
        name: "Phonk Drift",
        bpm: 132.0,
        hook: [(0, 2, 65), (3, 1, 68), (4, 2, 72), (8, 2, 70), (11, 1, 68), (12, 2, 65), (16, 2, 63), (19, 1, 65), (20, 2, 68), (24, 2, 67), (27, 1, 65), (28, 2, 63)],
        bass: [29, 29, 32, 29, 37, 37, 36, 32],
        chord_a: [53, 56, 60],
        chord_b: [49, 53, 56],
        pump_drive: 5.0,
    })
}

// ---------------------------------------------------------------- Phonk House
pub fn phonk_house() -> Project {
    let mut p = Project { name: "Phonk House".into(), bpm: 128.0, swing: 0.0, ..Project::default() };
    let all4: Vec<usize> = (0..32).step_by(4).collect();

    let mut drop = Pattern::new("House Drop", 32);
    steps(&mut drop, 0, &all4, 120);
    steps(&mut drop, 2, &[4, 12, 20, 28], 106);
    steps(&mut drop, 1, &[28], 70);
    let offbeat: Vec<usize> = (0..32).filter(|s| s % 4 == 2).collect();
    steps(&mut drop, 3, &(0..32).filter(|s| s % 2 == 1).collect::<Vec<_>>(), 50);
    steps(&mut drop, 3, &offbeat, 104);
    steps(&mut drop, 4, &[7, 15, 23, 31], 72);
    // off-beat 808 pump
    let bass = [29u8, 29, 32, 29, 36, 36, 34, 32];
    for (i, &pitch) in bass.iter().enumerate() {
        drop.notes.push(n(Track::Bass808, 2 + i as u32 * 4, 2, pitch, 122, false));
    }
    // cowbell hook
    let hook: [(u32, u32, u8); 12] = [
        (0, 1, 77), (3, 1, 75), (6, 1, 73), (10, 1, 72), (12, 2, 73), (14, 1, 70),
        (16, 1, 75), (19, 1, 73), (22, 1, 72), (26, 1, 70), (28, 2, 72), (30, 1, 68),
    ];
    for &(s, l, pitch) in &hook {
        drop.notes.push(n(Track::Analog, s, l, pitch, 108, false));
    }
    // chord stabs
    for pitch in [53u8, 56, 60] {
        drop.notes.push(n(Track::Wavetable, 6, 2, pitch, 100, false));
        drop.notes.push(n(Track::Wavetable, 10, 2, pitch, 92, false));
    }
    for pitch in [49u8, 53, 56] {
        drop.notes.push(n(Track::Wavetable, 22, 2, pitch, 100, false));
        drop.notes.push(n(Track::Wavetable, 26, 2, pitch, 92, false));
    }

    let mut intro = Pattern::new("House Intro", 32);
    steps(&mut intro, 0, &all4, 112);
    steps(&mut intro, 3, &offbeat, 80);
    steps(&mut intro, 4, &[7, 15, 23, 31], 60);

    let mut brk = Pattern::new("House Break", 32);
    steps(&mut brk, 3, &(0..32).step_by(8).collect::<Vec<_>>(), 60);
    for &(s, l, pitch) in &hook {
        brk.notes.push(n(Track::Analog, s, l, pitch, 96, false));
    }
    for pitch in [53u8, 56, 60, 65] {
        brk.notes.push(n(Track::Wavetable, 0, 16, pitch, 80, false));
    }
    for pitch in [49u8, 53, 56, 61] {
        brk.notes.push(n(Track::Wavetable, 16, 16, pitch, 80, false));
    }

    p.patterns = vec![intro, drop, brk];
    p.analog = cowbell();
    p.wt = WavetableParams { table_a: 0, pos_a: 0.66, unison: 5, uni_detune: 20.0, cutoff: 6500.0, attack: 0.002, decay: 0.18, sustain: 0.0, release: 0.14, volume: 0.75, ..WavetableParams::default() };
    p.bass = Bass808Params { drive: 3.0, glide_ms: 1.0, decay: 0.3, sustain: 0.1, release: 0.1, pitch_drop: 8.0, tone: 3200.0, volume: 0.9, ..Bass808Params::default() };
    p.delay_beats = 0.75;
    p.delay_feedback = 0.35;
    p.delay_return = 0.28;
    clips(&mut p, &[(0, 4), (1, 8), (2, 4), (1, 8), (0, 2)]);
    apply_mix(&mut p, "Club");
    p
}

// ---------------------------------------------------------------- shared helpers for the trap styles
fn mel(p: &mut Pattern, track: Track, list: &[(u32, u32, u8)], vel: u8) {
    for &(s, l, pitch) in list {
        p.notes.push(n(track, s, l, pitch, vel, false));
    }
}

fn chord(p: &mut Pattern, track: Track, step: u32, len: u32, pitches: &[u8], vel: u8) {
    for &pitch in pitches {
        p.notes.push(n(track, step, len, pitch, vel, false));
    }
}

/// (step, length, pitch, slide)
fn sub(p: &mut Pattern, list: &[(u32, u32, u8, bool)], vel: u8) {
    for &(s, l, pitch, slide) in list {
        p.notes.push(n(Track::Bass808, s, l, pitch, vel, slide));
    }
}

fn range(from: usize, to: usize, by: usize) -> Vec<usize> {
    (from..to).step_by(by).collect()
}

// ---------------------------------------------------------------- generic half-time style builder (own drums / sounds / structure per style)
struct Style {
    name: &'static str,
    bpm: f32,
    swing: f32,
    drums: fn(&mut Pattern, bool),
    sub: &'static [(u32, u32, u8, bool)],
    lead_track: Track,
    lead: &'static [(u32, u32, u8)],
    counter_track: Track,
    counter: &'static [(u32, u32, u8)],
    chord_a: [u8; 3],
    chord_b: [u8; 3],
    setup: fn(&mut Project),
    mix: &'static str,
    delay: (f32, f32, f32),
}

fn style_project(sp: Style) -> Project {
    let mut p = Project { name: sp.name.into(), bpm: sp.bpm, swing: sp.swing, ..Project::default() };
    let pad = |pat: &mut Pattern, vel: u8| {
        chord(pat, Track::Wavetable, 0, 16, &sp.chord_a, vel);
        chord(pat, Track::Wavetable, 16, 16, &sp.chord_b, vel);
    };
    let first_a = sp.sub[0].2;
    let first_b = sp.sub.iter().find(|s| s.0 >= 16).map_or(first_a, |s| s.2);

    let mut intro = Pattern::new(&format!("{} Intro", sp.name), 32);
    mel(&mut intro, sp.lead_track, sp.lead, 74);
    pad(&mut intro, 48);

    let mut groove = Pattern::new(&format!("{} Groove", sp.name), 32);
    (sp.drums)(&mut groove, false);
    sub(&mut groove, sp.sub, 120);
    mel(&mut groove, sp.lead_track, sp.lead, 92);
    mel(&mut groove, sp.counter_track, sp.counter, 70);
    pad(&mut groove, 46);

    let mut brk = Pattern::new(&format!("{} Break", sp.name), 32);
    steps(&mut brk, 3, &range(0, 32, 8), 50);
    sub(&mut brk, &[(0, 16, first_a, false), (16, 16, first_b, false)], 104);
    mel(&mut brk, sp.lead_track, sp.lead, 84);
    pad(&mut brk, 70);

    let mut hook = Pattern::new(&format!("{} Hook", sp.name), 32);
    (sp.drums)(&mut hook, true);
    sub(&mut hook, sp.sub, 127);
    mel(&mut hook, sp.lead_track, sp.lead, 104);
    mel(&mut hook, sp.counter_track, sp.counter, 80);
    let up: Vec<(u32, u32, u8)> = sp.lead.iter().map(|&(s, l, pch)| (s, l, pch + 12)).collect();
    mel(&mut hook, sp.counter_track, &up, 58);
    pad(&mut hook, 56);

    p.patterns = vec![intro, groove, brk, hook];
    (sp.setup)(&mut p);
    p.delay_beats = sp.delay.0;
    p.delay_feedback = sp.delay.1;
    p.delay_return = sp.delay.2;
    clips(&mut p, &[(0, 4), (1, 8), (3, 8), (2, 4), (1, 4), (3, 8)]);
    apply_mix(&mut p, sp.mix);
    p
}

// ---------------------------------------------------------------- 808 Mafia: dark bells, hard sliding 808
fn mafia_drums(pat: &mut Pattern, full: bool) {
    steps(pat, 0, &[0, 7, 10, 16, 23, 26], 122);
    steps(pat, 1, &[8, 24], 108);
    steps(pat, 2, &[8, 24], 94);
    steps(pat, 3, &range(0, 32, 2), 76);
    steps(pat, 3, &range(1, 32, 4), 52);
    steps(pat, 4, &[11, 27], 64);
    if full {
        steps(pat, 3, &[12, 13, 14, 15, 28, 29, 30, 31], 62);
    }
}

pub fn mafia_808() -> Project {
    style_project(Style {
        name: "808 Mafia",
        bpm: 140.0,
        swing: 0.0,
        drums: mafia_drums,
        sub: &[(0, 6, 29, false), (7, 3, 29, false), (10, 6, 32, true), (16, 6, 25, false), (23, 3, 25, false), (26, 6, 27, true)],
        lead_track: Track::Synth,
        lead: &[(0, 2, 72), (3, 2, 75), (6, 2, 77), (8, 4, 75), (14, 2, 68), (16, 2, 73), (19, 2, 77), (22, 2, 80), (24, 4, 77), (30, 2, 68)],
        counter_track: Track::Analog,
        counter: &[(0, 8, 60), (16, 8, 61)],
        chord_a: [53, 56, 60],
        chord_b: [49, 53, 56],
        setup: |p| {
            p.synth = crate::instruments::SynthParams::preset("Bell");
            p.analog = AnalogParams::preset("PWM Strings");
            p.wt = WavetableParams { volume: 0.45, ..WavetableParams::preset("Vowel Pad") };
            p.bass = Bass808Params { drive: 6.5, glide_ms: 110.0, decay: 1.4, sustain: 0.6, release: 0.22, pitch_drop: 10.0, tone: 2200.0, volume: 0.9, ..Bass808Params::default() };
        },
        mix: "Trap",
        delay: (0.75, 0.36, 0.3),
    })
}

// ---------------------------------------------------------------- Yeat: bouncy 808, wonky detuned lead, twinkling bells
fn rage_lead() -> AnalogParams {
    AnalogParams {
        wave1: AnalogWave::Saw,
        wave2: AnalogWave::Saw,
        semi2: 0.0,
        detune2: 26.0,
        mix: 0.5,
        cutoff: 2300.0,
        res: 0.3,
        drive: 2.5,
        env_amt: 0.6,
        keytrack: 0.5,
        attack: 0.002,
        decay: 0.26,
        sustain: 0.25,
        release: 0.14,
        f_attack: 0.001,
        f_decay: 0.2,
        f_sustain: 0.1,
        f_release: 0.12,
        drift: 0.3,
        volume: 0.8,
        ..AnalogParams::default()
    }
}

fn rage_drums(pat: &mut Pattern, full: bool) {
    steps(pat, 0, &[0, 3, 6, 10, 16, 19, 22, 26], 122);
    steps(pat, 2, &[8, 24], 104);
    steps(pat, 1, &[8, 24], 90);
    steps(pat, 3, &range(0, 32, 1), 60);
    steps(pat, 3, &range(2, 32, 4), 92);
    steps(pat, 4, &[5, 21, 29], 70);
    if full {
        steps(pat, 1, &[14, 15, 30, 31], 66);
        steps(pat, 0, &[13, 29], 98);
    }
}

pub fn yeat_rage() -> Project {
    style_project(Style {
        name: "Yeat Rage",
        bpm: 146.0,
        swing: 0.06,
        drums: rage_drums,
        sub: &[(0, 2, 30, false), (3, 2, 30, false), (6, 2, 33, false), (10, 3, 30, true), (14, 2, 28, false), (16, 2, 33, false), (19, 2, 33, false), (22, 2, 35, false), (26, 3, 33, true), (30, 2, 28, false)],
        lead_track: Track::Analog,
        lead: &[(0, 2, 73), (2, 1, 76), (3, 2, 73), (6, 2, 69), (8, 2, 66), (11, 2, 69), (14, 2, 73), (16, 2, 76), (19, 2, 69), (22, 2, 66), (24, 3, 69), (28, 2, 73)],
        counter_track: Track::Synth,
        counter: &[(0, 1, 85), (4, 1, 88), (8, 1, 85), (12, 1, 90), (16, 1, 85), (20, 1, 88), (24, 1, 92), (28, 1, 88)],
        chord_a: [54, 57, 61],
        chord_b: [50, 54, 57],
        setup: |p| {
            p.analog = rage_lead();
            p.synth = crate::instruments::SynthParams::preset("Lead");
            p.wt = WavetableParams { volume: 0.5, ..WavetableParams::preset("Super Saw") };
            p.bass = Bass808Params { drive: 8.5, glide_ms: 70.0, decay: 0.8, sustain: 0.45, release: 0.15, pitch_drop: 12.0, tone: 2800.0, volume: 0.9, ..Bass808Params::default() };
        },
        mix: "Trap",
        delay: (0.5, 0.3, 0.26),
    })
}

// ---------------------------------------------------------------- Plugg: airy plucks, soft sub, sparse drums
fn plugg_drums(pat: &mut Pattern, full: bool) {
    steps(pat, 0, &[0, 10, 16, 26], 112);
    steps(pat, 1, &[8, 24], 80);
    steps(pat, 2, &[8, 24], 96);
    steps(pat, 3, &[2, 6, 10, 14, 18, 22, 26, 30], 58);
    steps(pat, 3, &[4, 12, 20, 28], 40);
    steps(pat, 4, &[15, 31], 54);
    if full {
        steps(pat, 3, &[29, 30, 31], 50);
    }
}

pub fn plugg_dream() -> Project {
    style_project(Style {
        name: "Plugg Dream",
        bpm: 138.0,
        swing: 0.03,
        drums: plugg_drums,
        sub: &[(0, 10, 33, false), (10, 6, 33, false), (16, 10, 29, false), (26, 6, 31, true)],
        lead_track: Track::Synth,
        lead: &[(0, 1, 69), (2, 1, 72), (4, 1, 76), (6, 1, 72), (8, 1, 69), (10, 1, 72), (16, 1, 65), (18, 1, 69), (20, 1, 72), (22, 1, 69), (24, 1, 65), (28, 2, 77)],
        counter_track: Track::Analog,
        counter: &[(0, 6, 81), (8, 4, 79), (16, 6, 77), (24, 4, 76)],
        chord_a: [57, 60, 64],
        chord_b: [53, 57, 60],
        setup: |p| {
            p.synth = crate::instruments::SynthParams { lp_cutoff: 4200.0, release: 0.3, ..crate::instruments::SynthParams::preset("Pluck") };
            p.analog = AnalogParams::preset("Soft Keys");
            p.wt = WavetableParams { volume: 0.55, ..WavetableParams::preset("Vowel Pad") };
            p.bass = Bass808Params { drive: 2.5, glide_ms: 60.0, decay: 1.1, sustain: 0.5, release: 0.3, pitch_drop: 5.0, tone: 1500.0, volume: 0.85, ..Bass808Params::default() };
        },
        mix: "Trap",
        delay: (0.75, 0.45, 0.4),
    })
}

// ---------------------------------------------------------------- UK Drill: gliding 808, skippy hats, sparse snare
fn drill_drums(pat: &mut Pattern, full: bool) {
    steps(pat, 0, &[0, 11, 16, 27], 120);
    steps(pat, 1, &[12, 28], 106);
    steps(pat, 2, &[12, 28], 80);
    steps(pat, 3, &[0, 2, 4, 6, 7, 8, 10, 12, 14, 15, 16, 18, 20, 22, 23, 24, 26, 28, 30, 31], 66);
    steps(pat, 4, &[6, 22], 60);
    if full {
        steps(pat, 3, &[13, 14, 29, 30], 54);
        steps(pat, 1, &[31], 70);
    }
}

pub fn uk_drill() -> Project {
    style_project(Style {
        name: "UK Drill",
        bpm: 142.0,
        swing: 0.0,
        drums: drill_drums,
        sub: &[(0, 8, 26, false), (11, 5, 29, true), (16, 8, 24, false), (27, 5, 31, true)],
        lead_track: Track::Analog,
        lead: &[(0, 3, 62), (3, 3, 65), (6, 2, 69), (8, 3, 67), (11, 3, 65), (16, 3, 60), (19, 3, 63), (22, 3, 67), (26, 2, 60), (28, 2, 62)],
        counter_track: Track::Synth,
        counter: &[(0, 16, 74), (16, 16, 72)],
        chord_a: [50, 53, 57],
        chord_b: [48, 51, 55],
        setup: |p| {
            p.analog = AnalogParams { cutoff: 1500.0, decay: 0.5, sustain: 0.3, ..AnalogParams::preset("Soft Keys") };
            p.synth = crate::instruments::SynthParams::preset("Pad");
            p.wt = WavetableParams { volume: 0.4, ..WavetableParams::preset("Vowel Pad") };
            p.bass = Bass808Params { drive: 4.0, glide_ms: 140.0, decay: 1.2, sustain: 0.6, release: 0.2, pitch_drop: 6.0, tone: 1800.0, volume: 0.95, ..Bass808Params::default() };
        },
        mix: "Trap",
        delay: (0.75, 0.3, 0.2),
    })
}

// ---------------------------------------------------------------- Morgenshtern (old school): cold bells, hard 808, rolling hats
pub fn morgen_old() -> Project {
    let mut p = Project { name: "Morgenshtern Old".into(), bpm: 140.0, swing: 0.0, ..Project::default() };
    // C# minor -> A, original bell line
    let bells: [(u32, u32, u8); 10] = [
        (0, 2, 73), (3, 2, 76), (6, 2, 80), (8, 4, 76), (14, 2, 73),
        (16, 2, 73), (19, 2, 76), (22, 2, 81), (24, 4, 80), (30, 2, 76),
    ];
    let low: [(u32, u32, u8, bool); 6] = [(0, 6, 37, false), (7, 3, 37, false), (10, 6, 37, true), (16, 6, 33, false), (23, 3, 33, false), (26, 6, 35, true)];
    let groove = |pat: &mut Pattern, rolls: bool| {
        steps(pat, 0, &[0, 7, 10, 16, 23, 26], 122);
        steps(pat, 1, &[8, 24], 112);
        steps(pat, 2, &[8, 24], 96);
        steps(pat, 3, &range(0, 32, 2), 78);
        steps(pat, 3, &range(1, 32, 4), 54);
        if rolls {
            steps(pat, 3, &[12, 13, 14, 15, 28, 29, 30, 31], 64);
            steps(pat, 1, &[30, 31], 74);
        }
        steps(pat, 4, &[11, 27], 66);
        sub(pat, &low, 124);
    };
    let pad = |pat: &mut Pattern, vel: u8| {
        chord(pat, Track::Wavetable, 0, 16, &[61, 64, 68], vel);
        chord(pat, Track::Wavetable, 16, 16, &[57, 61, 64], vel);
    };

    let mut intro = Pattern::new("Morgen Intro", 32);
    mel(&mut intro, Track::Synth, &bells, 76);
    pad(&mut intro, 56);
    steps(&mut intro, 3, &range(0, 32, 4), 56);

    let mut verse = Pattern::new("Morgen Verse", 32);
    groove(&mut verse, false);
    mel(&mut verse, Track::Synth, &bells, 70);
    pad(&mut verse, 44);

    let mut hook = Pattern::new("Morgen Hook", 32);
    groove(&mut hook, true);
    mel(&mut hook, Track::Synth, &bells, 96);
    mel(&mut hook, Track::Analog, &bells, 72);
    pad(&mut hook, 62);

    let mut brk = Pattern::new("Morgen Break", 32);
    steps(&mut brk, 3, &range(0, 32, 8), 52);
    sub(&mut brk, &[(0, 16, 37, false), (16, 16, 33, false)], 108);
    mel(&mut brk, Track::Synth, &bells, 84);
    pad(&mut brk, 74);

    p.patterns = vec![intro, verse, hook, brk];
    p.synth = crate::instruments::SynthParams::preset("Bell");
    p.analog = rage_lead();
    p.wt = WavetableParams { volume: 0.5, ..WavetableParams::preset("Vowel Pad") };
    p.bass = Bass808Params { drive: 6.0, glide_ms: 90.0, decay: 1.2, sustain: 0.55, release: 0.2, pitch_drop: 10.0, tone: 2400.0, volume: 0.9, ..Bass808Params::default() };
    p.delay_beats = 0.75;
    p.delay_feedback = 0.34;
    p.delay_return = 0.3;
    clips(&mut p, &[(0, 4), (1, 8), (2, 8), (3, 4), (1, 8), (2, 8)]);
    apply_mix(&mut p, "Trap");
    p
}

// ---------------------------------------------------------------- Big Baby Tape: bouncy 808, bright hook, fast hats
pub fn bbt_bounce() -> Project {
    let mut p = Project { name: "Big Baby Tape Bounce".into(), bpm: 150.0, swing: 0.05, ..Project::default() };
    // F# minor hook
    let hook: [(u32, u32, u8); 12] = [
        (0, 1, 66), (1, 1, 69), (3, 1, 73), (4, 2, 69), (8, 1, 66), (10, 1, 69), (12, 2, 64),
        (16, 1, 66), (17, 1, 69), (19, 1, 76), (20, 2, 73), (26, 2, 71),
    ];
    let bounce: [(u32, u32, u8, bool); 10] = [
        (0, 2, 30, false), (3, 2, 30, false), (6, 2, 33, false), (10, 3, 30, true), (14, 2, 28, false),
        (16, 2, 33, false), (19, 2, 33, false), (23, 2, 35, false), (26, 3, 33, true), (30, 2, 28, false),
    ];
    let drums = |pat: &mut Pattern, busy: bool| {
        steps(pat, 0, &[0, 3, 7, 10, 16, 19, 23, 26], 120);
        steps(pat, 2, &[8, 24], 108);
        steps(pat, 1, &[8, 24], 92);
        steps(pat, 3, &range(0, 32, 1), 62);
        steps(pat, 3, &range(2, 32, 4), 94);
        if busy {
            steps(pat, 3, &[14, 15, 30, 31], 48);
            steps(pat, 1, &[15, 31], 66);
        }
        steps(pat, 4, &[5, 21, 29], 70);
    };
    let pad = |pat: &mut Pattern| {
        chord(pat, Track::Wavetable, 0, 16, &[54, 57, 61], 50);
        chord(pat, Track::Wavetable, 16, 16, &[52, 56, 59], 50);
    };

    let mut intro = Pattern::new("Bounce Intro", 32);
    mel(&mut intro, Track::Analog, &hook, 72);
    steps(&mut intro, 3, &range(0, 32, 2), 56);
    pad(&mut intro);

    let mut main = Pattern::new("Bounce Main", 32);
    drums(&mut main, true);
    sub(&mut main, &bounce, 124);
    mel(&mut main, Track::Analog, &hook, 98);
    mel(&mut main, Track::Synth, &hook, 58);
    pad(&mut main);

    let mut brk = Pattern::new("Bounce Break", 32);
    steps(&mut brk, 3, &range(0, 32, 4), 54);
    sub(&mut brk, &[(0, 16, 30, false), (16, 16, 33, false)], 110);
    mel(&mut brk, Track::Analog, &hook, 88);
    pad(&mut brk);

    let mut drop = Pattern::new("Bounce Drop", 32);
    drums(&mut drop, false);
    steps(&mut drop, 0, &[13, 29], 100);
    sub(&mut drop, &bounce, 127);
    let up: Vec<(u32, u32, u8)> = hook.iter().map(|&(s, l, pch)| (s, l, pch + 12)).collect();
    mel(&mut drop, Track::Analog, &up, 94);
    mel(&mut drop, Track::Synth, &hook, 62);

    p.patterns = vec![intro, main, brk, drop];
    p.analog = rage_lead();
    p.synth = crate::instruments::SynthParams::preset("Pluck");
    p.wt = WavetableParams { volume: 0.5, ..WavetableParams::preset("Vowel Pad") };
    p.bass = Bass808Params { drive: 8.0, glide_ms: 70.0, decay: 0.8, sustain: 0.45, release: 0.15, pitch_drop: 12.0, tone: 2800.0, volume: 0.9, ..Bass808Params::default() };
    p.delay_beats = 0.5;
    p.delay_feedback = 0.3;
    p.delay_return = 0.26;
    clips(&mut p, &[(0, 4), (1, 8), (2, 4), (3, 8), (1, 8)]);
    apply_mix(&mut p, "Trap");
    p
}

// ---------------------------------------------------------------- Big Baby Tape dark plugg: arps, long sliding sub, sparse hits
pub fn bbt_plugg() -> Project {
    let mut p = Project { name: "Big Baby Tape Dark Plugg".into(), bpm: 144.0, swing: 0.04, ..Project::default() };
    // D minor -> Bb arps
    let arp_dm: [(u32, u32, u8); 8] = [(0, 2, 74), (2, 2, 77), (4, 2, 81), (6, 2, 77), (8, 2, 74), (10, 2, 77), (12, 2, 84), (14, 2, 81)];
    let arp_bb: [(u32, u32, u8); 8] = [(16, 2, 70), (18, 2, 74), (20, 2, 77), (22, 2, 74), (24, 2, 70), (26, 2, 74), (28, 2, 82), (30, 2, 77)];
    let pad = |pat: &mut Pattern, vel: u8| {
        chord(pat, Track::Wavetable, 0, 16, &[62, 65, 69], vel);
        chord(pat, Track::Wavetable, 16, 16, &[58, 62, 65], vel);
    };

    let mut intro = Pattern::new("Dark Intro", 32);
    mel(&mut intro, Track::Synth, &arp_dm, 66);
    mel(&mut intro, Track::Synth, &arp_bb, 66);
    pad(&mut intro, 46);
    steps(&mut intro, 3, &range(0, 32, 4), 48);

    let mut groove = Pattern::new("Dark Groove", 32);
    steps(&mut groove, 0, &[0, 6, 10, 16, 22, 26], 116);
    steps(&mut groove, 1, &[8, 24], 98);
    steps(&mut groove, 2, &[8, 24], 92);
    steps(&mut groove, 3, &range(0, 32, 2), 60);
    steps(&mut groove, 3, &[13, 14, 15, 29, 30, 31], 58);
    steps(&mut groove, 4, &[7, 23], 62);
    sub(&mut groove, &[(0, 6, 26, false), (6, 3, 26, false), (10, 6, 29, true), (16, 6, 22, false), (22, 3, 22, false), (26, 6, 24, true)], 118);
    mel(&mut groove, Track::Synth, &arp_dm, 90);
    mel(&mut groove, Track::Synth, &arp_bb, 90);
    pad(&mut groove, 54);

    let mut brk = Pattern::new("Dark Break", 32);
    steps(&mut brk, 3, &range(0, 32, 8), 46);
    sub(&mut brk, &[(0, 16, 26, false), (16, 16, 22, false)], 104);
    mel(&mut brk, Track::Synth, &arp_dm, 78);
    mel(&mut brk, Track::Synth, &arp_bb, 78);
    pad(&mut brk, 72);

    p.patterns = vec![intro, groove, brk];
    p.synth = crate::instruments::SynthParams::preset("Pluck");
    p.wt = WavetableParams { volume: 0.5, ..WavetableParams::preset("Vowel Pad") };
    p.bass = Bass808Params { drive: 5.0, glide_ms: 130.0, decay: 1.3, sustain: 0.6, release: 0.25, pitch_drop: 8.0, tone: 2000.0, volume: 0.9, ..Bass808Params::default() };
    p.delay_beats = 0.75;
    p.delay_feedback = 0.4;
    p.delay_return = 0.36;
    clips(&mut p, &[(0, 4), (1, 8), (2, 4), (1, 8), (0, 2)]);
    apply_mix(&mut p, "Trap");
    p
}


// ---------------------------------------------------------------- house-energy styles (Phonk House formula: 4-on-floor, off-beat 808 pump, cowbell hook, stabs)
struct HouseSpec {
    name: &'static str,
    bpm: f32,
    hook: [(u32, u32, u8); 12],
    bass: [u8; 8],
    chord_a: [u8; 3],
    chord_b: [u8; 3],
    pump_drive: f32,
}

fn house_style(sp: HouseSpec) -> Project {
    let mut p = Project { name: sp.name.into(), bpm: sp.bpm, swing: 0.0, ..Project::default() };
    let all4 = range(0, 32, 4);
    let offbeat: Vec<usize> = (0..32).filter(|s| s % 4 == 2).collect();
    let groove = |pat: &mut Pattern, full: bool| {
        steps(pat, 0, &all4, 120);
        steps(pat, 2, &[4, 12, 20, 28], 106);
        steps(pat, 1, &[28], 70);
        steps(pat, 3, &(0..32).filter(|s| s % 2 == 1).collect::<Vec<_>>(), 50);
        steps(pat, 3, &offbeat, 104);
        steps(pat, 4, &[7, 15, 23, 31], 72);
        if full {
            steps(pat, 1, &[12, 28], 96);
        }
        for (i, &pitch) in sp.bass.iter().enumerate() {
            pat.notes.push(n(Track::Bass808, 2 + i as u32 * 4, 2, pitch, 122, false));
        }
    };

    let mut intro = Pattern::new(&format!("{} Intro", sp.name), 32);
    steps(&mut intro, 0, &all4, 112);
    steps(&mut intro, 3, &offbeat, 80);
    steps(&mut intro, 4, &[7, 15, 23, 31], 60);
    mel(&mut intro, Track::Analog, &sp.hook[..6], 88);

    let mut drop = Pattern::new(&format!("{} Drop", sp.name), 32);
    groove(&mut drop, true);
    mel(&mut drop, Track::Analog, &sp.hook, 108);
    for step in [6u32, 10] {
        chord(&mut drop, Track::Wavetable, step, 2, &sp.chord_a, 100);
        chord(&mut drop, Track::Wavetable, step + 16, 2, &sp.chord_b, 100);
    }
    chord(&mut drop, Track::Synth, 0, 2, &[sp.chord_a[2] + 12], 70);
    chord(&mut drop, Track::Synth, 16, 2, &[sp.chord_b[2] + 12], 70);

    let mut brk = Pattern::new(&format!("{} Break", sp.name), 32);
    steps(&mut brk, 3, &range(0, 32, 8), 60);
    mel(&mut brk, Track::Analog, &sp.hook, 96);
    chord(&mut brk, Track::Wavetable, 0, 16, &[sp.chord_a[0], sp.chord_a[1], sp.chord_a[2], sp.chord_a[0] + 12], 80);
    chord(&mut brk, Track::Wavetable, 16, 16, &[sp.chord_b[0], sp.chord_b[1], sp.chord_b[2], sp.chord_b[0] + 12], 80);
    chord(&mut brk, Track::Synth, 0, 16, &sp.chord_a, 60);

    p.patterns = vec![intro, drop, brk];
    p.analog = cowbell();
    p.synth = crate::instruments::SynthParams::preset("Pluck");
    p.wt = WavetableParams { table_a: 0, pos_a: 0.66, unison: 5, uni_detune: 20.0, cutoff: 6500.0, attack: 0.002, decay: 0.18, sustain: 0.0, release: 0.14, volume: 0.75, ..WavetableParams::default() };
    p.bass = Bass808Params { drive: sp.pump_drive, glide_ms: 1.0, decay: 0.3, sustain: 0.1, release: 0.1, pitch_drop: 8.0, tone: 3200.0, volume: 0.9, ..Bass808Params::default() };
    p.delay_beats = 0.75;
    p.delay_feedback = 0.35;
    p.delay_return = 0.28;
    clips(&mut p, &[(0, 4), (1, 8), (2, 4), (1, 8), (2, 2), (1, 4)]);
    apply_mix(&mut p, "Club");
    p
}

fn neon_garage() -> Project {
    house_style(HouseSpec {
        name: "Neon Garage · House Phonk",
        bpm: 130.0,
        hook: [(0, 1, 78), (3, 1, 81), (5, 1, 76), (8, 2, 73), (11, 1, 76), (14, 2, 78),
            (16, 1, 76), (18, 1, 78), (21, 1, 81), (24, 2, 80), (27, 1, 76), (30, 2, 73)],
        bass: [30, 30, 33, 37, 28, 28, 32, 35],
        chord_a: [54, 57, 61],
        chord_b: [52, 56, 59],
        pump_drive: 3.6,
    })
}

fn oversteer_drums(pat: &mut Pattern, full: bool) {
    steps(pat, 0, &[0, 3, 7, 10, 14, 16, 19, 23, 26, 29], 124);
    steps(pat, 1, &[4, 12, 20, 28], 115);
    steps(pat, 2, &[4, 12, 20, 28], 94);
    steps(pat, 3, &range(0, 32, 2), 74);
    steps(pat, 4, &[6, 15, 22, 31], 85);
    if full {
        steps(pat, 3, &[13, 15, 25, 27, 29, 30, 31], 83);
    }
}

fn night_oversteer() -> Project {
    style_project(Style {
        name: "Night Oversteer · Drift Phonk",
        bpm: 160.0,
        swing: 0.05,
        drums: oversteer_drums,
        sub: &[(0, 4, 37, false), (6, 4, 37, false), (11, 4, 40, true),
            (16, 6, 33, false), (23, 3, 40, true), (27, 4, 35, false)],
        lead_track: Track::Analog,
        lead: &[(0, 1, 73), (2, 1, 76), (5, 1, 80), (7, 1, 76), (9, 1, 74), (12, 2, 73),
            (16, 1, 69), (19, 1, 73), (21, 1, 76), (24, 2, 74), (27, 1, 71), (30, 2, 73)],
        counter_track: Track::Synth,
        counter: &[(6, 2, 61), (14, 1, 64), (22, 2, 57), (30, 2, 61)],
        chord_a: [49, 52, 56],
        chord_b: [45, 49, 52],
        setup: |p| {
            p.analog = cowbell();
            p.synth = crate::instruments::SynthParams::preset("Pluck");
            p.wt = WavetableParams { volume: 0.42, ..WavetableParams::preset("Vowel Pad") };
            p.bass = Bass808Params { drive: 6.0, glide_ms: 58.0, decay: 0.55, sustain: 0.4,
                release: 0.14, pitch_drop: 8.0, tone: 2900.0, volume: 0.82, ..Bass808Params::default() };
        },
        mix: "Phonk",
        delay: (0.5, 0.24, 0.17),
    })
}

fn glass_rain() -> Project {
    style_project(Style {
        name: "Glass Rain · Melodic Type Beat",
        bpm: 138.0,
        swing: 0.07,
        drums: melodic_drums,
        sub: &[(0, 6, 35, false), (7, 6, 35, false), (13, 3, 38, true),
            (16, 6, 31, false), (23, 5, 31, false), (28, 4, 33, true)],
        lead_track: Track::Synth,
        lead: &[(0, 3, 78), (4, 2, 74), (7, 3, 71), (11, 2, 73), (14, 2, 74),
            (16, 3, 79), (21, 2, 74), (24, 3, 71), (28, 2, 69), (30, 2, 66)],
        counter_track: Track::Analog,
        counter: &[(4, 6, 62), (12, 4, 66), (20, 8, 59), (29, 3, 62)],
        chord_a: [59, 62, 66],
        chord_b: [55, 59, 62],
        setup: |p| {
            p.synth = crate::instruments::SynthParams::preset("Bell");
            p.analog = AnalogParams::preset("Soft Keys");
            p.wt = WavetableParams { volume: 0.48, ..WavetableParams::preset("Vowel Pad") };
            p.bass = Bass808Params { drive: 2.8, glide_ms: 85.0, decay: 0.95, sustain: 0.54,
                release: 0.16, tone: 2350.0, volume: 0.82, ..Bass808Params::default() };
        },
        mix: "Trap",
        delay: (0.75, 0.37, 0.25),
    })
}

fn club_drums(pat: &mut Pattern, full: bool) {
    steps(pat, 0, &range(0, 32, 4), 118);
    steps(pat, 2, &[4, 12, 20, 28], 100);
    steps(pat, 1, &[12, 28], 60);
    steps(pat, 3, &range(2, 32, 4), 92);
    steps(pat, 3, &range(1, 32, 2), 44);
    steps(pat, 4, &[7, 23], 60);
    if full {
        steps(pat, 1, &[14, 15, 30, 31], 70);
    }
}

pub fn morgen_house() -> Project {
    style_project(Style {
        name: "Morgenshtern House",
        bpm: 126.0,
        swing: 0.0,
        drums: club_drums,
        sub: &[(2, 2, 25, false), (6, 2, 25, false), (10, 2, 28, false), (14, 2, 25, false), (18, 2, 33, false), (22, 2, 33, false), (26, 2, 32, false), (30, 2, 28, false)],
        lead_track: Track::Synth,
        lead: &[(0, 1, 73), (3, 1, 76), (6, 1, 80), (10, 1, 76), (12, 2, 73), (14, 1, 69), (16, 1, 73), (19, 1, 81), (22, 1, 80), (26, 1, 76), (28, 2, 73), (30, 1, 69)],
        counter_track: Track::Analog,
        counter: &[(0, 2, 61), (4, 2, 64), (8, 2, 68), (12, 2, 64), (16, 2, 57), (20, 2, 61), (24, 2, 64), (28, 2, 61)],
        chord_a: [61, 64, 68],
        chord_b: [57, 61, 64],
        setup: |p| {
            p.synth = crate::instruments::SynthParams::preset("Bell");
            p.analog = AnalogParams::preset("Soft Keys");
            p.wt = WavetableParams { volume: 0.55, ..WavetableParams::preset("Super Saw") };
            p.bass = Bass808Params { drive: 3.0, glide_ms: 1.0, decay: 0.28, sustain: 0.1, release: 0.1, pitch_drop: 6.0, tone: 2600.0, volume: 0.9, ..Bass808Params::default() };
        },
        mix: "Club",
        delay: (0.75, 0.33, 0.25),
    })
}

pub fn bbt_house() -> Project {
    style_project(Style {
        name: "Big Baby Tape House",
        bpm: 130.0,
        swing: 0.0,
        drums: club_drums,
        sub: &[(2, 3, 30, false), (6, 2, 30, false), (10, 3, 33, false), (14, 2, 30, false), (18, 3, 28, false), (22, 2, 28, false), (26, 3, 35, false), (30, 2, 33, false)],
        lead_track: Track::Analog,
        lead: &[(0, 1, 78), (3, 1, 76), (6, 1, 73), (10, 1, 76), (12, 2, 78), (14, 1, 71), (16, 1, 76), (19, 1, 73), (22, 1, 71), (26, 1, 73), (28, 2, 76), (30, 1, 69)],
        counter_track: Track::Synth,
        counter: &[(0, 2, 54), (6, 2, 57), (12, 2, 61), (16, 2, 52), (22, 2, 56), (28, 2, 59)],
        chord_a: [54, 57, 61],
        chord_b: [52, 56, 59],
        setup: |p| {
            p.analog = AnalogParams::preset("Sync Lead");
            p.synth = crate::instruments::SynthParams::preset("Lead");
            p.wt = WavetableParams { volume: 0.5, ..WavetableParams::preset("Metal Pluck") };
            p.bass = Bass808Params { drive: 4.5, glide_ms: 1.0, decay: 0.3, sustain: 0.15, release: 0.1, pitch_drop: 8.0, tone: 3000.0, volume: 0.9, ..Bass808Params::default() };
        },
        mix: "Club",
        delay: (0.5, 0.3, 0.22),
    })
}

pub fn bbt_phonk() -> Project {
    house_style(HouseSpec {
        name: "Big Baby Tape Phonk",
        bpm: 140.0,
        hook: [(0, 2, 65), (3, 1, 68), (4, 2, 72), (8, 2, 70), (11, 1, 68), (12, 2, 65), (16, 2, 63), (19, 1, 65), (20, 2, 68), (24, 2, 67), (27, 1, 65), (28, 2, 63)],
        bass: [29, 29, 32, 29, 37, 37, 36, 32],
        chord_a: [53, 56, 60],
        chord_b: [49, 53, 56],
        pump_drive: 5.0,
    })
}


/// One-click mix: channel levels, pan, delay sends and the master bus.
pub fn apply_mix(p: &mut Project, name: &str) {
    let vol: [f32; NUM_CH];
    let pan: [f32; NUM_CH];
    let send: [f32; NUM_CH];
    let fx: MasterFx;
    match name {
        "Phonk" => {
            //     kick  snare clap  hat   perc  synth 808   wt    analog audio
            vol = [0.95, 0.78, 0.70, 0.48, 0.55, 0.55, 0.90, 0.62, 0.78, 0.85];
            pan = [0.0, 0.0, 0.0, 0.18, -0.22, 0.0, 0.0, 0.0, 0.1, 0.0];
            send = [0.0, 0.08, 0.14, 0.0, 0.1, 0.2, 0.0, 0.28, 0.3, 0.0];
            fx = MasterFx { comp_on: true, threshold_db: -11.0, ratio: 3.0, makeup_db: 3.0, low_db: 2.0, high_db: 1.5 };
        }
        "Trap" => {
            //     kick  snare clap  hat   perc  synth 808   wt    analog audio
            vol = [1.0, 0.75, 0.7, 0.5, 0.45, 0.6, 0.95, 0.45, 0.62, 0.85];
            pan = [0.0, 0.0, 0.0, 0.18, -0.2, 0.0, 0.0, 0.0, 0.1, 0.0];
            send = [0.0, 0.08, 0.12, 0.0, 0.08, 0.25, 0.0, 0.3, 0.2, 0.0];
            fx = MasterFx { comp_on: true, threshold_db: -10.0, ratio: 3.5, makeup_db: 3.5, low_db: 3.0, high_db: 1.0 };
        }
        "Club" => {
            vol = [1.0, 0.7, 0.8, 0.5, 0.5, 0.55, 0.82, 0.7, 0.75, 0.85];
            pan = [0.0, 0.0, 0.0, 0.15, -0.2, 0.0, 0.0, 0.0, 0.12, 0.0];
            send = [0.0, 0.1, 0.12, 0.0, 0.08, 0.2, 0.0, 0.22, 0.28, 0.0];
            fx = MasterFx { comp_on: true, threshold_db: -9.0, ratio: 4.0, makeup_db: 4.0, low_db: 3.0, high_db: 2.0 };
        }
        _ => {
            let d = default_channels();
            for (i, c) in p.channels.iter_mut().enumerate() {
                let output = c.output;
                *c = d[i].clone();
                c.output = output;
            }
            p.master_fx = MasterFx::default();
            return;
        }
    }
    for (i, c) in p.channels.iter_mut().enumerate().take(NUM_CH) {
        c.volume = vol[i];
        c.pan = pan[i];
        c.delay_send = send[i];
        c.mute = false;
        c.solo = false;
    }
    p.master_fx = fx;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio_engine::Engine;
    use std::sync::Arc;

    fn render(p: Project, secs: f32) -> (Vec<f32>, [[f32; 2]; NUM_STRIPS + 1]) {
        let mut e = Engine::new(48000.0);
        e.set_project(Arc::new(p));
        e.play();
        let mut buf = vec![0.0f32; (48000.0 * secs) as usize * 2];
        for c in buf.chunks_mut(512) {
            e.render(c);
        }
        (buf, e.take_peaks())
    }

    #[test]
    fn templates_play_every_instrument() {
        for (name, make) in TEMPLATES.iter().take(2) {
            let mut p = make();
            p.song_mode = false;
            p.current_pattern = 1; // the full pattern
            let (buf, peaks) = render(p, 4.0);
            assert!(buf.iter().all(|v| v.is_finite()), "{name}");
            let peak = buf.iter().fold(0.0f32, |m, v| m.max(v.abs()));
            assert!(peak > 0.2 && peak <= 1.0, "{name} peak {peak}");
            for ch in [0, 1, 2, 3, 4, CH_BASS, CH_WT, CH_AN] {
                assert!(peaks[ch][0] + peaks[ch][1] > 0.0, "{name}: channel {ch} silent");
            }
        }
    }

    #[test]
    fn templates_are_multitrack_and_play_in_song_mode() {
        for (name, make) in TEMPLATES.iter().filter(|(n, _)| !n.starts_with("Empty") && !n.starts_with("Untitle34")) {
            let mut p = make();
            let tracks = p.arrangement.tracks.len();
            assert!(tracks >= 5, "{name}: only {tracks} tracks");
            // every clip is a single channel's part, not a whole pattern
            assert!(p.arrangement.clips.iter().all(|c| c.lane < tracks), "{name}");
            p.song_mode = true;
            let (buf, peaks) = render(p, 14.0);
            let peak = buf.iter().fold(0.0f32, |m, v| m.max(v.abs()));
            assert!(peak > 0.2 && peak <= 1.0, "{name} song-mode peak {peak}");
            assert!(peaks[0][0] + peaks[0][1] > 0.0, "{name}: kick silent in song mode");
        }
    }

    #[test]
    fn style_templates_play_and_roundtrip() {
        for (name, make) in TEMPLATES.iter().filter(|(n, _)| !n.starts_with("Phonk") && !n.starts_with("Empty") && !n.starts_with("Untitle34")) {
            let mut p = make();
            assert!(p.arrangement.clips.len() >= 8, "{name}");
            p.song_mode = false;
            p.current_pattern = 1; // the full groove
            let (buf, peaks) = render(p.clone(), 4.0);
            assert!(buf.iter().all(|v| v.is_finite()), "{name}");
            let peak = buf.iter().fold(0.0f32, |m, v| m.max(v.abs()));
            assert!(peak > 0.2 && peak <= 1.0, "{name} peak {peak}");
            for ch in [0, 1, 2, 3, CH_BASS, CH_SYNTH] {
                assert!(peaks[ch][0] + peaks[ch][1] > 0.0, "{name}: channel {ch} silent");
            }
            let path = std::env::temp_dir().join(format!("synthesia_style_{}.synthesia", name.split(' ').next().unwrap()));
            p.adopt_track_params(); // loading does this too
            p.save(&path).unwrap();
            let (q, w) = Project::load(&path).unwrap();
            let _ = std::fs::remove_file(&path);
            assert!(w.is_empty() && p == q, "{name}");
        }
    }

    #[test]
    fn templates_have_arrangement_and_roundtrip() {
        for (name, make) in TEMPLATES.iter().take(2) {
            let mut p = make();
            p.adopt_track_params(); // loading does this too
            assert!(p.arrangement.clips.len() >= 8, "{name}");
            assert!(p.loop_bars >= 16, "{name}");
            let path = std::env::temp_dir().join(format!("synthesia_tpl_{}.synthesia", name.replace(' ', "_")));
            p.save(&path).unwrap();
            let (q, w) = Project::load(&path).unwrap();
            let _ = std::fs::remove_file(&path);
            assert!(w.is_empty());
            assert!(p == q, "{name}");
        }
    }

    #[test]
    fn master_chain_compresses_and_stays_finite() {
        let mut p = phonk_drift();
        p.current_pattern = 1;
        let (loud, _) = render(p.clone(), 3.0);
        p.master_fx = MasterFx { comp_on: true, threshold_db: -30.0, ratio: 10.0, makeup_db: 0.0, low_db: 6.0, high_db: -6.0 };
        let (squashed, _) = render(p, 3.0);
        assert!(squashed.iter().all(|v| v.is_finite()));
        let rms = |b: &Vec<f32>| (b.iter().map(|v| v * v).sum::<f32>() / b.len() as f32).sqrt();
        assert!(rms(&squashed) < rms(&loud), "compressor should reduce level");
    }
}

#[cfg(test)]
mod diag {
    use super::*;
    use crate::audio_engine::Engine;
    use std::sync::Arc;

    #[test]
    fn audio_diagnostics() {
        for (name, make) in TEMPLATES.iter().take(2) {
            for pat in 0..3 {
                let mut p = make();
                p.current_pattern = pat;
                let mut e = Engine::new(48000.0);
                e.set_project(Arc::new(p));
                e.play();
                let secs = 8.0;
                let mut buf = vec![0.0f32; 48000 * 8 * 2];
                let t0 = web_time::Instant::now();
                for c in buf.chunks_mut(512) {
                    e.render(c);
                }
                let el = t0.elapsed().as_secs_f32();
                let peak = buf.iter().fold(0.0f32, |m, v| m.max(v.abs()));
                let clipped = buf.iter().filter(|v| v.abs() >= 0.97).count();
                let mut max_jump = 0.0f32;
                let mut hf = 0.0f64;
                let mut tot = 0.0f64;
                for i in 1..buf.len() / 2 {
                    let d = (buf[i * 2] - buf[(i - 1) * 2]).abs();
                    max_jump = max_jump.max(d);
                    hf += (d as f64) * (d as f64);
                    tot += (buf[i * 2] as f64).powi(2);
                }
                println!("{name} pat{pat}: peak {peak:.3} clipped {clipped} max_jump {max_jump:.3} hf/tot {:.3} cpu {:.1}% of realtime", hf / tot.max(1e-9), el / secs * 100.0);
            }
        }
    }
}


// ---------------------------------------------------------------- new phonk house: faster, higher cowbell hook, harder pump
pub fn phonk_house_2() -> Project {
    house_style(HouseSpec {
        name: "Phonk House 2",
        bpm: 126.0,
        hook: [(0, 1, 79), (3, 1, 77), (6, 1, 74), (8, 1, 72), (10, 1, 74), (12, 2, 75), (16, 1, 79), (19, 1, 77), (22, 1, 74), (24, 1, 72), (27, 1, 70), (30, 1, 67)],
        bass: [31, 31, 34, 31, 36, 36, 34, 31],
        chord_a: [55, 58, 62],
        chord_b: [51, 55, 58],
        pump_drive: 5.5,
    })
}

// ---------------------------------------------------------------- hard drift phonk: 146 BPM, driving cowbell, distorted 808
pub fn drift_hard() -> Project {
    house_style(HouseSpec {
        name: "Drift Phonk Hard",
        bpm: 146.0,
        hook: [(0, 2, 72), (3, 1, 75), (4, 2, 77), (8, 2, 75), (11, 1, 72), (12, 2, 70), (16, 2, 68), (19, 1, 70), (20, 2, 72), (24, 2, 70), (27, 1, 68), (28, 2, 65)],
        bass: [27, 27, 30, 27, 32, 32, 30, 27],
        chord_a: [51, 54, 58],
        chord_b: [49, 52, 56],
        pump_drive: 7.0,
    })
}


// ---------------------------------------------------------------- Hardbass: 150 BPM, kick on every beat, off-beat donk bass, screech lead
fn hardbass_drums(pat: &mut Pattern, full: bool) {
    steps(pat, 0, &range(0, 32, 4), 127);
    steps(pat, 2, &[4, 12, 20, 28], 112);
    steps(pat, 1, &[4, 12, 20, 28], 70);
    steps(pat, 3, &range(2, 32, 4), 100);
    steps(pat, 3, &range(1, 32, 2), 44);
    steps(pat, 4, &[7, 15, 23, 31], 76);
    if full {
        steps(pat, 1, &[28, 29, 30, 31], 96);
        steps(pat, 0, &[30, 31], 118);
    }
}

pub fn hardbass() -> Project {
    style_project(Style {
        name: "Hardbass",
        bpm: 150.0,
        swing: 0.0,
        drums: hardbass_drums,
        // the classic hardbass line: the donk sits on the off-beats between the kicks
        sub: &[(2, 1, 33, false), (6, 1, 33, false), (10, 1, 36, false), (14, 1, 33, false), (18, 1, 33, false), (22, 1, 33, false), (26, 1, 31, false), (30, 1, 33, false)],
        lead_track: Track::Wavetable,
        lead: &[(0, 2, 69), (4, 2, 72), (8, 2, 69), (12, 2, 67), (16, 2, 69), (20, 2, 72), (24, 2, 74), (28, 2, 72)],
        counter_track: Track::Synth,
        counter: &[(2, 1, 81), (6, 1, 79), (10, 1, 81), (14, 1, 76), (18, 1, 81), (22, 1, 79), (26, 1, 77), (30, 1, 76)],
        chord_a: [57, 60, 64],
        chord_b: [53, 57, 60],
        setup: |p| {
            p.wt = WavetableParams::preset("Hardbass Donk");
            p.synth = crate::instruments::SynthParams::preset("Lead");
            p.bass = Bass808Params { drive: 6.0, glide_ms: 1.0, decay: 0.16, sustain: 0.0, release: 0.06, pitch_drop: 10.0, tone: 2600.0, volume: 0.9, ..Bass808Params::default() };
        },
        mix: "Club",
        delay: (0.75, 0.25, 0.18),
    })
}

// ---------------------------------------------------------------- Donk funk house: 124 BPM, four on the floor, syncopated donk bass and stabs
fn donk_funk_drums(pat: &mut Pattern, full: bool) {
    steps(pat, 0, &range(0, 32, 4), 118);
    steps(pat, 2, &[4, 12, 20, 28], 100);
    steps(pat, 3, &range(2, 32, 4), 96);
    steps(pat, 3, &range(1, 32, 4), 52);
    steps(pat, 4, &[3, 10, 19, 26], 70);
    steps(pat, 1, &[12, 28], 58);
    if full {
        steps(pat, 1, &[14, 30], 80);
        steps(pat, 3, &[15, 31], 84);
    }
}

pub fn donk_funk() -> Project {
    style_project(Style {
        name: "Donk Funk",
        bpm: 124.0,
        swing: 0.08,
        drums: donk_funk_drums,
        sub: &[(0, 1, 33, false), (3, 1, 33, false), (6, 1, 36, false), (8, 1, 33, false), (11, 1, 31, false), (14, 1, 33, false), (16, 1, 33, false), (19, 1, 33, false), (22, 1, 36, false), (24, 1, 38, false), (27, 1, 36, false), (30, 1, 33, false)],
        lead_track: Track::Wavetable,
        lead: &[(2, 1, 69), (5, 1, 72), (10, 1, 74), (13, 1, 72), (18, 1, 69), (21, 1, 72), (26, 1, 76), (29, 1, 74)],
        counter_track: Track::Synth,
        counter: &[(0, 1, 81), (6, 1, 79), (8, 1, 76), (14, 1, 79), (16, 1, 81), (22, 1, 83), (24, 1, 79), (30, 1, 76)],
        chord_a: [57, 60, 64],
        chord_b: [55, 59, 62],
        setup: |p| {
            p.wt = WavetableParams::preset("Donk Funk");
            p.synth = crate::instruments::SynthParams::preset("Pluck");
            p.bass = Bass808Params { drive: 3.5, glide_ms: 1.0, decay: 0.22, sustain: 0.05, release: 0.08, pitch_drop: 6.0, tone: 2400.0, volume: 0.85, ..Bass808Params::default() };
        },
        mix: "Club",
        delay: (0.75, 0.3, 0.22),
    })
}

// ---------------------------------------------------------------- Brazilian funk: tamborzao drums, short 808, cowbell + pluck stabs
fn brazil_drums(pat: &mut Pattern, full: bool) {
    steps(pat, 0, &[0, 3, 6, 8, 11, 14, 16, 19, 22, 24, 27, 30], 118);
    steps(pat, 2, &[4, 12, 20, 28], 104);
    steps(pat, 1, &[7, 15, 23, 31], 66);
    steps(pat, 3, &range(0, 32, 2), 70);
    steps(pat, 3, &range(1, 32, 4), 48);
    steps(pat, 4, &[2, 5, 10, 13, 18, 21, 26, 29], 84);
    if full {
        steps(pat, 1, &[28, 29, 30, 31], 82);
        steps(pat, 3, &[12, 13, 14, 15], 64);
    }
}

pub fn brazil_funk() -> Project {
    style_project(Style {
        name: "Brazilian Funk",
        bpm: 130.0,
        swing: 0.0,
        drums: brazil_drums,
        sub: &[(0, 2, 31, false), (3, 2, 31, false), (6, 2, 34, false), (8, 2, 31, false), (11, 2, 29, false), (14, 2, 31, false), (16, 2, 31, false), (19, 2, 31, false), (22, 2, 34, false), (24, 2, 36, false), (27, 2, 34, false), (30, 2, 31, false)],
        lead_track: Track::Synth,
        lead: &[(0, 1, 67), (3, 1, 67), (6, 1, 70), (8, 1, 67), (11, 1, 65), (14, 1, 63), (16, 1, 67), (19, 1, 67), (22, 1, 70), (24, 1, 72), (27, 1, 70), (30, 1, 67)],
        counter_track: Track::Analog,
        counter: &[(2, 1, 72), (6, 1, 75), (10, 1, 72), (14, 1, 70), (18, 1, 72), (22, 1, 75), (26, 1, 77), (30, 1, 75)],
        chord_a: [55, 58, 62],
        chord_b: [53, 57, 60],
        setup: |p| {
            p.synth = crate::instruments::SynthParams::preset("Pluck");
            p.analog = cowbell();
            p.wt = WavetableParams { volume: 0.4, ..WavetableParams::preset("Vowel Pad") };
            p.bass = Bass808Params { drive: 4.0, glide_ms: 2.0, decay: 0.35, sustain: 0.1, release: 0.1, pitch_drop: 8.0, tone: 2800.0, volume: 0.9, ..Bass808Params::default() };
        },
        mix: "Club",
        delay: (0.75, 0.3, 0.25),
    })
}


// ---------------------------------------------------------------- Dubstep: half-time 140, wobbling growl bass, saw stabs
fn dubstep_drums(pat: &mut Pattern, full: bool) {
    steps(pat, 0, &[0, 11, 16, 27], 124);
    steps(pat, 1, &[8, 24], 120);
    steps(pat, 2, &[8, 24], 100);
    steps(pat, 3, &range(0, 32, 2), 62);
    steps(pat, 4, &[14, 30], 70);
    if full {
        steps(pat, 3, &[12, 13, 14, 15, 28, 29, 30, 31], 70);
        steps(pat, 1, &[30, 31], 90);
    }
}

pub fn dubstep() -> Project {
    style_project(Style {
        name: "Dubstep",
        bpm: 140.0,
        swing: 0.0,
        drums: dubstep_drums,
        sub: &[(0, 8, 28, false), (10, 5, 28, false), (16, 8, 26, false), (26, 5, 31, false)],
        lead_track: Track::Synth,
        lead: &[(0, 1, 76), (6, 1, 79), (8, 2, 76), (16, 1, 74), (22, 1, 77), (24, 2, 74)],
        counter_track: Track::Wavetable,
        counter: &[(0, 3, 40), (4, 2, 40), (8, 4, 43), (14, 2, 40), (16, 3, 38), (20, 2, 38), (24, 4, 41), (30, 2, 38)],
        chord_a: [52, 55, 59],
        chord_b: [50, 53, 57],
        setup: |p| {
            p.synth = crate::instruments::SynthParams::preset("Lead");
            p.wt = WavetableParams { volume: 0.6, ..WavetableParams::preset("Growl Bass") };
            p.analog = AnalogParams::preset("Sync Lead");
            p.bass = Bass808Params { drive: 5.0, glide_ms: 20.0, decay: 1.0, sustain: 0.5, release: 0.15, pitch_drop: 6.0, tone: 1800.0, volume: 0.9, ..Bass808Params::default() };
        },
        mix: "Club",
        delay: (0.5, 0.3, 0.2),
    })
}

// ---------------------------------------------------------------- Pop: bright chord progression, plucked hook, backbeat
fn pop_drums(pat: &mut Pattern, full: bool) {
    steps(pat, 0, &[0, 8, 16, 24, 14, 30], 112);
    steps(pat, 2, &[4, 12, 20, 28], 104);
    steps(pat, 1, &[12, 28], 70);
    steps(pat, 3, &range(0, 32, 2), 66);
    steps(pat, 4, &range(1, 32, 2), 40);
    if full {
        steps(pat, 3, &[15, 31], 90);
        steps(pat, 1, &[30, 31], 80);
    }
}

pub fn pop_beat() -> Project {
    style_project(Style {
        name: "Pop",
        bpm: 118.0,
        swing: 0.08,
        drums: pop_drums,
        sub: &[(0, 6, 33, false), (8, 6, 33, false), (16, 6, 29, false), (24, 6, 29, false)],
        lead_track: Track::Synth,
        lead: &[(0, 2, 76), (3, 1, 72), (4, 2, 69), (8, 2, 72), (12, 2, 76), (16, 2, 77), (19, 1, 72), (20, 2, 69), (24, 2, 72), (28, 2, 77)],
        counter_track: Track::Analog,
        counter: &[(0, 8, 64), (8, 8, 67), (16, 8, 65), (24, 8, 69)],
        chord_a: [57, 60, 64],
        chord_b: [53, 57, 60],
        setup: |p| {
            p.synth = crate::instruments::SynthParams::preset("Pluck");
            p.analog = AnalogParams::preset("Soft Keys");
            p.wt = WavetableParams { volume: 0.5, ..WavetableParams::preset("Vowel Pad") };
            p.bass = Bass808Params { drive: 1.5, glide_ms: 5.0, decay: 0.5, sustain: 0.3, release: 0.15, pitch_drop: 4.0, tone: 2600.0, volume: 0.8, ..Bass808Params::default() };
        },
        mix: "Club",
        delay: (0.75, 0.3, 0.25),
    })
}

// ---------------------------------------------------------------- Melodic type beat: bells over a sliding 808
fn melodic_drums(pat: &mut Pattern, full: bool) {
    steps(pat, 0, &[0, 6, 10, 16, 22, 26], 120);
    steps(pat, 1, &[8, 24], 112);
    steps(pat, 2, &[8, 24], 92);
    steps(pat, 3, &range(0, 32, 2), 72);
    steps(pat, 3, &range(1, 32, 4), 50);
    steps(pat, 4, &[12, 28], 60);
    if full {
        steps(pat, 3, &[14, 15, 30, 31], 66);
    }
}

pub fn melodic_beat() -> Project {
    style_project(Style {
        name: "Melodic Type Beat",
        bpm: 142.0,
        swing: 0.0,
        drums: melodic_drums,
        sub: &[(0, 6, 29, false), (6, 4, 29, false), (10, 6, 32, true), (16, 6, 27, false), (22, 4, 27, false), (26, 6, 30, true)],
        lead_track: Track::Synth,
        lead: &[(0, 2, 72), (2, 2, 75), (4, 2, 79), (6, 2, 75), (8, 2, 77), (10, 2, 75), (12, 2, 72), (14, 2, 70), (16, 2, 70), (18, 2, 74), (20, 2, 77), (22, 2, 74), (24, 2, 72), (26, 2, 70), (28, 4, 67)],
        counter_track: Track::Analog,
        counter: &[(0, 16, 60), (16, 16, 58)],
        chord_a: [53, 56, 60],
        chord_b: [51, 55, 58],
        setup: |p| {
            p.synth = crate::instruments::SynthParams::preset("Bell");
            p.analog = AnalogParams::preset("PWM Strings");
            p.wt = WavetableParams { volume: 0.5, ..WavetableParams::preset("Vowel Pad") };
            p.bass = Bass808Params { drive: 4.0, glide_ms: 90.0, decay: 1.3, sustain: 0.6, release: 0.2, pitch_drop: 9.0, tone: 2400.0, volume: 0.9, ..Bass808Params::default() };
        },
        mix: "Trap",
        delay: (0.75, 0.4, 0.3),
    })
}

// ---------------------------------------------------------------- "Bandana I" style: dark, bouncy trap with a small bell motif
fn bandana_drums(pat: &mut Pattern, full: bool) {
    steps(pat, 0, &[0, 6, 9, 16, 22, 25], 122);
    steps(pat, 1, &[8, 24], 110);
    steps(pat, 2, &[8, 24], 98);
    steps(pat, 3, &range(0, 32, 2), 74);
    steps(pat, 3, &range(1, 32, 4), 52);
    steps(pat, 4, &[3, 19], 66);
    if full {
        steps(pat, 3, &[12, 13, 14, 15, 28, 29, 30, 31], 64);
    }
}

pub fn bandana_one() -> Project {
    style_project(Style {
        name: "Bandana I Style",
        bpm: 144.0,
        swing: 0.0,
        drums: bandana_drums,
        sub: &[(0, 5, 26, false), (6, 3, 26, false), (9, 7, 29, true), (16, 5, 24, false), (22, 3, 24, false), (25, 7, 27, true)],
        lead_track: Track::Synth,
        lead: &[(0, 1, 68), (2, 1, 71), (4, 2, 73), (8, 1, 71), (10, 1, 68), (12, 2, 66), (16, 1, 68), (18, 1, 71), (20, 2, 75), (24, 1, 73), (26, 1, 71), (28, 2, 68)],
        counter_track: Track::Analog,
        counter: &[(0, 8, 56), (16, 8, 55)],
        chord_a: [50, 53, 57],
        chord_b: [48, 52, 55],
        setup: |p| {
            p.synth = crate::instruments::SynthParams::preset("Bell");
            p.analog = AnalogParams::preset("PWM Strings");
            p.wt = WavetableParams { volume: 0.4, ..WavetableParams::preset("Vowel Pad") };
            p.bass = Bass808Params { drive: 6.0, glide_ms: 100.0, decay: 1.2, sustain: 0.55, release: 0.2, pitch_drop: 10.0, tone: 2100.0, volume: 0.9, ..Bass808Params::default() };
        },
        mix: "Trap",
        delay: (0.75, 0.36, 0.3),
    })
}


// ---------------------------------------------------------------- more styles
fn lofi_drums(pat: &mut Pattern, full: bool) {
    steps(pat, 0, &[0, 10, 16, 26], 100);
    steps(pat, 1, &[8, 24], 70);
    steps(pat, 2, &[8, 24], 60);
    steps(pat, 3, &range(0, 32, 2), 48);
    steps(pat, 3, &range(1, 32, 4), 32);
    steps(pat, 4, &[14, 30], 50);
    if full {
        steps(pat, 3, &[15, 31], 60);
    }
}

pub fn lofi_hiphop() -> Project {
    style_project(Style {
        name: "Lo-fi Hip-Hop",
        bpm: 82.0,
        swing: 0.18,
        drums: lofi_drums,
        sub: &[(0, 6, 33, false), (8, 4, 33, false), (16, 6, 29, false), (24, 4, 31, false)],
        lead_track: Track::Synth,
        lead: &[(0, 2, 72), (4, 2, 69), (8, 2, 76), (12, 2, 72), (16, 2, 71), (20, 2, 67), (24, 2, 69), (28, 2, 64)],
        counter_track: Track::Analog,
        counter: &[(0, 8, 60), (8, 8, 64), (16, 8, 57), (24, 8, 62)],
        chord_a: [57, 60, 64],
        chord_b: [53, 57, 60],
        setup: |p| {
            p.synth = crate::instruments::SynthParams::preset("Pluck");
            p.analog = AnalogParams::preset("Soft Keys");
            p.wt = WavetableParams { volume: 0.5, ..WavetableParams::preset("Vowel Pad") };
            p.bass = Bass808Params { drive: 1.0, glide_ms: 8.0, decay: 0.6, sustain: 0.4, release: 0.2, pitch_drop: 3.0, tone: 2200.0, volume: 0.8, ..Bass808Params::default() };
        },
        mix: "Club",
        delay: (0.75, 0.35, 0.25),
    })
}

fn house_four_drums(pat: &mut Pattern, full: bool) {
    steps(pat, 0, &[0, 4, 8, 12, 16, 20, 24, 28], 118);
    steps(pat, 1, &[12, 28], 60);
    steps(pat, 2, &[4, 12, 20, 28], 100);
    steps(pat, 3, &[2, 6, 10, 14, 18, 22, 26, 30], 82);
    steps(pat, 3, &range(1, 32, 2), 38);
    steps(pat, 4, &[7, 15, 23, 31], 60);
    if full {
        steps(pat, 1, &[30, 31], 70);
    }
}

pub fn house_four() -> Project {
    style_project(Style {
        name: "House",
        bpm: 124.0,
        swing: 0.0,
        drums: house_four_drums,
        sub: &[(2, 2, 33, false), (6, 2, 33, false), (10, 2, 33, false), (14, 2, 36, false), (18, 2, 31, false), (22, 2, 31, false), (26, 2, 31, false), (30, 2, 34, false)],
        lead_track: Track::Synth,
        lead: &[(0, 1, 69), (3, 1, 72), (6, 1, 76), (8, 1, 72), (11, 1, 69), (14, 1, 67), (16, 1, 67), (19, 1, 71), (22, 1, 74), (24, 1, 71), (27, 1, 67), (30, 1, 66)],
        counter_track: Track::Analog,
        counter: &[(0, 2, 57), (4, 2, 60), (8, 2, 64), (12, 2, 60), (16, 2, 55), (20, 2, 59), (24, 2, 62), (28, 2, 59)],
        chord_a: [57, 60, 64],
        chord_b: [55, 59, 62],
        setup: |p| {
            p.synth = crate::instruments::SynthParams::preset("Pluck");
            p.analog = AnalogParams::preset("Sync Lead");
            p.wt = WavetableParams { volume: 0.5, ..WavetableParams::preset("Vowel Pad") };
            p.bass = Bass808Params { drive: 2.0, glide_ms: 4.0, decay: 0.4, sustain: 0.3, release: 0.12, pitch_drop: 3.0, tone: 2400.0, volume: 0.85, ..Bass808Params::default() };
        },
        mix: "Club",
        delay: (0.75, 0.3, 0.2),
    })
}

fn techno_drums(pat: &mut Pattern, full: bool) {
    steps(pat, 0, &[0, 4, 8, 12, 16, 20, 24, 28], 122);
    steps(pat, 1, &[4, 20], 50);
    steps(pat, 2, &[12, 28], 96);
    steps(pat, 3, &[2, 6, 10, 14, 18, 22, 26, 30], 86);
    steps(pat, 3, &range(1, 32, 2), 34);
    steps(pat, 4, &[3, 7, 11, 15, 19, 23, 27, 31], 48);
    if full {
        steps(pat, 1, &[14, 15, 30, 31], 66);
    }
}

pub fn techno() -> Project {
    style_project(Style {
        name: "Techno",
        bpm: 132.0,
        swing: 0.0,
        drums: techno_drums,
        sub: &[(1, 2, 31, false), (5, 2, 31, false), (9, 2, 34, false), (13, 2, 31, false), (17, 2, 31, false), (21, 2, 29, false), (25, 2, 31, false), (29, 2, 34, false)],
        lead_track: Track::Synth,
        lead: &[(0, 1, 67), (2, 1, 70), (3, 1, 67), (6, 1, 72), (8, 1, 67), (10, 1, 70), (12, 1, 65), (15, 1, 67), (16, 1, 67), (18, 1, 70), (19, 1, 67), (22, 1, 74), (24, 1, 72), (26, 1, 70), (28, 1, 67), (31, 1, 65)],
        counter_track: Track::Analog,
        counter: &[(0, 16, 55), (16, 16, 53)],
        chord_a: [55, 58, 62],
        chord_b: [53, 57, 60],
        setup: |p| {
            p.synth = crate::instruments::SynthParams::preset("Lead");
            p.analog = AnalogParams::preset("Sync Lead");
            p.wt = WavetableParams { volume: 0.35, ..WavetableParams::preset("Growl Bass") };
            p.bass = Bass808Params { drive: 3.0, glide_ms: 2.0, decay: 0.3, sustain: 0.2, release: 0.1, pitch_drop: 2.0, tone: 2000.0, volume: 0.85, ..Bass808Params::default() };
        },
        mix: "Club",
        delay: (0.75, 0.3, 0.2),
    })
}

fn dnb_drums(pat: &mut Pattern, full: bool) {
    steps(pat, 0, &[0, 10, 16, 26], 120);
    steps(pat, 1, &[8, 24], 116);
    steps(pat, 2, &[8, 24], 90);
    steps(pat, 3, &range(0, 32, 2), 70);
    steps(pat, 3, &range(1, 32, 4), 44);
    steps(pat, 4, &[6, 22], 60);
    if full {
        steps(pat, 1, &[14, 15, 30, 31], 76);
    }
}

pub fn dnb() -> Project {
    style_project(Style {
        name: "Drum and Bass",
        bpm: 174.0,
        swing: 0.0,
        drums: dnb_drums,
        sub: &[(0, 6, 29, false), (6, 2, 29, false), (10, 6, 32, false), (16, 6, 27, false), (22, 2, 27, false), (26, 6, 30, false)],
        lead_track: Track::Synth,
        lead: &[(0, 2, 72), (4, 2, 75), (8, 2, 79), (12, 2, 75), (16, 2, 77), (20, 2, 74), (24, 2, 72), (28, 2, 70)],
        counter_track: Track::Analog,
        counter: &[(0, 16, 60), (16, 16, 58)],
        chord_a: [53, 56, 60],
        chord_b: [51, 55, 58],
        setup: |p| {
            p.synth = crate::instruments::SynthParams::preset("Pluck");
            p.analog = AnalogParams::preset("PWM Strings");
            p.wt = WavetableParams { volume: 0.4, ..WavetableParams::preset("Growl Bass") };
            p.bass = Bass808Params { drive: 4.0, glide_ms: 6.0, decay: 0.9, sustain: 0.6, release: 0.15, pitch_drop: 4.0, tone: 1900.0, volume: 0.9, ..Bass808Params::default() };
        },
        mix: "Club",
        delay: (0.5, 0.3, 0.2),
    })
}

fn reggaeton_drums(pat: &mut Pattern, full: bool) {
    steps(pat, 0, &[0, 8, 16, 24], 118);
    steps(pat, 1, &[3, 6, 11, 14, 19, 22, 27, 30], 100);
    steps(pat, 2, &[6, 14, 22, 30], 62);
    steps(pat, 3, &range(0, 32, 2), 60);
    steps(pat, 4, &[4, 12, 20, 28], 50);
    if full {
        steps(pat, 3, &[15, 31], 80);
    }
}

pub fn reggaeton() -> Project {
    style_project(Style {
        name: "Reggaeton",
        bpm: 96.0,
        swing: 0.0,
        drums: reggaeton_drums,
        sub: &[(0, 3, 34, false), (3, 3, 34, false), (6, 2, 37, false), (8, 3, 34, false), (11, 3, 32, false), (14, 2, 34, false), (16, 3, 34, false), (19, 3, 34, false), (22, 2, 37, false), (24, 3, 39, false), (27, 3, 37, false), (30, 2, 34, false)],
        lead_track: Track::Synth,
        lead: &[(0, 1, 69), (3, 1, 72), (6, 1, 74), (8, 1, 72), (11, 1, 69), (14, 1, 67), (16, 1, 69), (19, 1, 72), (22, 1, 76), (24, 1, 74), (27, 1, 72), (30, 1, 69)],
        counter_track: Track::Analog,
        counter: &[(2, 1, 72), (6, 1, 76), (10, 1, 72), (14, 1, 69), (18, 1, 72), (22, 1, 76), (26, 1, 79), (30, 1, 76)],
        chord_a: [57, 60, 64],
        chord_b: [55, 59, 62],
        setup: |p| {
            p.synth = crate::instruments::SynthParams::preset("Pluck");
            p.analog = AnalogParams::preset("Soft Keys");
            p.wt = WavetableParams { volume: 0.4, ..WavetableParams::preset("Vowel Pad") };
            p.bass = Bass808Params { drive: 3.0, glide_ms: 8.0, decay: 0.7, sustain: 0.4, release: 0.15, pitch_drop: 6.0, tone: 2300.0, volume: 0.9, ..Bass808Params::default() };
        },
        mix: "Club",
        delay: (0.75, 0.3, 0.25),
    })
}

fn synthwave_drums(pat: &mut Pattern, full: bool) {
    steps(pat, 0, &[0, 8, 16, 22, 24], 116);
    steps(pat, 1, &[8, 24], 110);
    steps(pat, 2, &[8, 24], 90);
    steps(pat, 3, &range(0, 32, 2), 56);
    steps(pat, 4, &[14, 30], 52);
    if full {
        steps(pat, 1, &[28, 29, 30, 31], 76);
        steps(pat, 3, &[15, 31], 70);
    }
}

pub fn synthwave() -> Project {
    style_project(Style {
        name: "Synthwave",
        bpm: 118.0,
        swing: 0.0,
        drums: synthwave_drums,
        sub: &[(0, 2, 33, false), (2, 2, 33, false), (4, 2, 33, false), (6, 2, 33, false), (8, 2, 36, false), (10, 2, 36, false), (12, 2, 36, false), (14, 2, 36, false), (16, 2, 31, false), (18, 2, 31, false), (20, 2, 31, false), (22, 2, 31, false), (24, 2, 34, false), (26, 2, 34, false), (28, 2, 34, false), (30, 2, 34, false)],
        lead_track: Track::Synth,
        lead: &[(0, 3, 76), (4, 1, 72), (6, 2, 69), (8, 4, 72), (16, 3, 74), (20, 1, 71), (22, 2, 67), (24, 6, 71)],
        counter_track: Track::Analog,
        counter: &[(0, 16, 57), (16, 16, 55)],
        chord_a: [57, 60, 64],
        chord_b: [55, 59, 62],
        setup: |p| {
            p.synth = crate::instruments::SynthParams::preset("Lead");
            p.analog = AnalogParams::preset("PWM Strings");
            p.wt = WavetableParams { volume: 0.5, ..WavetableParams::preset("Super Saw") };
            p.bass = Bass808Params { drive: 2.0, glide_ms: 4.0, decay: 0.35, sustain: 0.3, release: 0.1, pitch_drop: 2.0, tone: 2200.0, volume: 0.85, ..Bass808Params::default() };
        },
        mix: "Club",
        delay: (0.75, 0.4, 0.3),
    })
}

fn boom_bap_drums(pat: &mut Pattern, full: bool) {
    steps(pat, 0, &[0, 7, 16, 23, 26], 112);
    steps(pat, 1, &[8, 24], 108);
    steps(pat, 2, &[8, 24], 84);
    steps(pat, 3, &range(0, 32, 2), 52);
    steps(pat, 3, &range(1, 32, 4), 34);
    steps(pat, 4, &[12, 28], 56);
    if full {
        steps(pat, 1, &[30, 31], 70);
    }
}

pub fn boom_bap() -> Project {
    style_project(Style {
        name: "Boom Bap",
        bpm: 90.0,
        swing: 0.12,
        drums: boom_bap_drums,
        sub: &[(0, 4, 31, false), (6, 2, 31, false), (10, 4, 34, false), (16, 4, 29, false), (22, 2, 29, false), (26, 4, 32, false)],
        lead_track: Track::Synth,
        lead: &[(0, 2, 72), (4, 1, 67), (6, 1, 70), (8, 2, 72), (12, 2, 74), (16, 2, 70), (20, 1, 65), (22, 1, 67), (24, 2, 70), (28, 2, 67)],
        counter_track: Track::Analog,
        counter: &[(0, 8, 60), (8, 8, 63), (16, 8, 58), (24, 8, 62)],
        chord_a: [53, 56, 60],
        chord_b: [51, 55, 58],
        setup: |p| {
            p.synth = crate::instruments::SynthParams::preset("Bell");
            p.analog = AnalogParams::preset("Soft Keys");
            p.wt = WavetableParams { volume: 0.45, ..WavetableParams::preset("Vowel Pad") };
            p.bass = Bass808Params { drive: 2.0, glide_ms: 10.0, decay: 0.8, sustain: 0.5, release: 0.15, pitch_drop: 4.0, tone: 2100.0, volume: 0.85, ..Bass808Params::default() };
        },
        mix: "Club",
        delay: (0.75, 0.3, 0.22),
    })
}

fn amapiano_drums(pat: &mut Pattern, full: bool) {
    steps(pat, 0, &[0, 8, 16, 24], 112);
    steps(pat, 1, &[12, 28], 100);
    steps(pat, 2, &[12, 28], 78);
    steps(pat, 3, &range(0, 32, 2), 58);
    steps(pat, 3, &range(1, 32, 2), 38);
    steps(pat, 4, &[3, 7, 11, 19, 23, 27], 64);
    if full {
        steps(pat, 4, &[14, 15, 30, 31], 72);
    }
}

pub fn amapiano() -> Project {
    style_project(Style {
        name: "Amapiano",
        bpm: 112.0,
        swing: 0.08,
        drums: amapiano_drums,
        sub: &[(0, 3, 33, false), (3, 2, 33, false), (6, 2, 36, false), (8, 3, 31, false), (11, 2, 31, false), (14, 2, 34, false), (16, 3, 33, false), (19, 2, 33, false), (22, 2, 36, false), (24, 3, 29, false), (27, 2, 31, false), (30, 2, 33, false)],
        lead_track: Track::Synth,
        lead: &[(2, 2, 72), (6, 1, 76), (9, 2, 74), (14, 2, 72), (18, 2, 76), (22, 1, 79), (25, 2, 77), (30, 2, 74)],
        counter_track: Track::Analog,
        counter: &[(0, 4, 57), (8, 4, 55), (16, 4, 57), (24, 4, 53)],
        chord_a: [57, 60, 64],
        chord_b: [55, 59, 62],
        setup: |p| {
            p.synth = crate::instruments::SynthParams::preset("Pluck");
            p.analog = AnalogParams::preset("Soft Keys");
            p.wt = WavetableParams { volume: 0.45, ..WavetableParams::preset("Vowel Pad") };
            p.bass = Bass808Params { drive: 3.5, glide_ms: 12.0, decay: 0.45, sustain: 0.35, release: 0.12, pitch_drop: 3.0, tone: 1500.0, volume: 0.9, ..Bass808Params::default() };
        },
        mix: "Club",
        delay: (0.75, 0.3, 0.25),
    })
}

// ---------------------------------------------------------------- Dark Trap: the motif of the project "Untitled2455"
// G# minor, 140 BPM: the saw lead G#-D#-G#-E over two bars, the same kick / snare / hat pattern, a sliding 808,
// arpeggio on top and a G#m - E pad. style_project turns that into intro / drop / break patterns and a full arrangement.
fn dark_trap_drums(pat: &mut Pattern, full: bool) {
    steps(pat, 0, &[0, 6, 12, 14, 20, 22, 28], 112);
    steps(pat, 1, &[8, 24], 108);
    steps(pat, 2, &[8, 24], 84);
    steps(pat, 3, &range(0, 32, 2), 72);
    steps(pat, 3, &range(1, 32, 4), 46);
    steps(pat, 4, &[4, 12, 20], 76);
    if full {
        steps(pat, 3, &[12, 13, 14, 15, 28, 29, 30, 31], 62);
        steps(pat, 1, &[30, 31], 80);
    }
}

pub fn dark_trap() -> Project {
    style_project(Style {
        name: "Dark Trap",
        bpm: 140.0,
        swing: 0.0,
        drums: dark_trap_drums,
        sub: &[(0, 6, 32, false), (6, 4, 32, false), (10, 4, 32, true), (14, 6, 35, false), (20, 4, 32, false), (24, 4, 30, true), (28, 4, 32, false)],
        lead_track: Track::Synth,
        lead: &[(0, 8, 56), (8, 8, 63), (16, 8, 56), (24, 8, 64)],
        counter_track: Track::Analog,
        counter: &[(0, 2, 68), (2, 2, 71), (4, 2, 75), (6, 2, 71), (8, 2, 68), (10, 2, 71), (12, 2, 75), (14, 2, 76), (16, 2, 68), (18, 2, 71), (20, 2, 75), (22, 2, 71), (24, 2, 64), (26, 2, 68), (28, 2, 71), (30, 2, 68)],
        chord_a: [56, 59, 63],
        chord_b: [52, 56, 59],
        setup: |p| {
            p.synth = crate::instruments::SynthParams::preset("Lead");
            p.analog = AnalogParams::preset("PWM Strings");
            p.wt = WavetableParams { volume: 0.5, ..WavetableParams::preset("Super Saw") };
            p.bass = Bass808Params { attack: 0.002, decay: 0.9, sustain: 0.55, release: 0.18, glide_ms: 90.0, pitch_drop: 10.0, drive: 2.5, tone: 3500.0, volume: 0.9, ..Bass808Params::default() };
        },
        mix: "Trap",
        delay: (0.75, 0.4, 0.3),
    })
}

/// Writes the arranged copy of the user's project "Untitled2455" (their sound design, the template's arrangement) into the
/// hub folder. Run with: cargo test write_arranged_2455 -- --ignored
#[cfg(test)]
mod arranged {
    use super::*;

    #[test]
    #[ignore]
    fn write_arranged_2455() {
        let docs = std::path::PathBuf::from(std::env::var("USERPROFILE").unwrap()).join("Documents");
        let mut t = dark_trap();
        if let Ok((u, _)) = Project::load(&docs.join("Untitled2455.synthesia")) {
            t.synth = u.synth.clone();
            t.bass = u.bass.clone();
            t.wt = u.wt.clone();
            t.analog = u.analog.clone();
            t.key_root = u.key_root;
            t.key_scale = u.key_scale;
        }
        t.name = "Untitled2455 arranged".into();
        let dir = docs.join("SYNTHESIA Projects");
        std::fs::create_dir_all(&dir).unwrap();
        t.save(&dir.join("Untitled2455 arranged.synthesia")).unwrap();
    }
}


/// The OG Buda style beat for the user's project "Untitled333333333333333synthesia": their synth melody, new dark-trap drums,
/// a sliding 808 and the usual intro / drop / break arrangement. Run with: cargo test write_buda_beat -- --ignored
#[cfg(test)]
mod buda {
    use super::*;

    fn buda_drums(pat: &mut Pattern, full: bool) {
        steps(pat, 0, &[0, 3, 10, 16, 19, 26, 29], 118);
        steps(pat, 1, &[8, 24], 120);
        steps(pat, 2, &[8, 24], 96);
        steps(pat, 3, &range(0, 32, 2), 74);
        steps(pat, 3, &[6, 7, 14, 15, 22, 23, 30, 31], 58);
        steps(pat, 3, &[1, 5, 9, 13, 17, 21, 25, 29], 40);
        steps(pat, 4, &[12, 28], 70);
        if full {
            steps(pat, 3, &[12, 13, 28, 29, 30, 31], 62);
            steps(pat, 1, &[30, 31], 84);
            steps(pat, 0, &[22], 104);
        }
    }

    #[test]
    #[ignore]
    fn write_buda_beat() {
        let docs = std::path::PathBuf::from(std::env::var("USERPROFILE").unwrap()).join("Documents");
        let (u, _) = Project::load(&docs.join("Untitled333333333333333synthesia.synthesia")).unwrap();
        let lead: Vec<(u32, u32, u8)> = u.patterns[1].notes.iter().filter(|n| n.len >= 24).map(|n| (n.start / 24, (n.len / 24).max(1), n.pitch)).collect();
        let lead: &'static [(u32, u32, u8)] = Box::leak(lead.into_boxed_slice());
        let mut t = style_project(Style {
            name: "OG Buda Beat",
            bpm: 140.0,
            swing: 0.0,
            drums: buda_drums,
            sub: &[(0, 4, 27, false), (3, 3, 27, false), (6, 2, 30, true), (10, 6, 27, false), (16, 4, 27, false), (19, 3, 25, false), (22, 2, 30, true), (26, 6, 27, false)],
            lead_track: Track::Synth,
            lead,
            counter_track: Track::Analog,
            counter: &[(2, 1, 75), (6, 1, 78), (10, 1, 75), (14, 1, 73), (18, 1, 75), (22, 1, 78), (26, 1, 82), (30, 1, 78)],
            chord_a: [51, 54, 58],
            chord_b: [47, 51, 54],
            setup: |p| {
                p.analog = AnalogParams::preset("PWM Strings");
                p.wt = WavetableParams { volume: 0.4, ..WavetableParams::preset("Vowel Pad") };
                p.bass = Bass808Params { attack: 0.002, decay: 1.2, sustain: 0.6, release: 0.2, glide_ms: 100.0, pitch_drop: 10.0, drive: 5.0, tone: 2100.0, volume: 0.9, ..Bass808Params::default() };
            },
            mix: "Trap",
            delay: (0.75, 0.36, 0.3),
        });
        t.synth = u.synth.clone();
        t.key_root = u.key_root;
        t.key_scale = u.key_scale;
        t.name = "OG Buda Beat".into();
        let dir = docs.join("SYNTHESIA Projects");
        std::fs::create_dir_all(&dir).unwrap();
        t.save(&dir.join("OG Buda Beat.synthesia")).unwrap();
    }
}


#[cfg(test)]
mod kits {
    use super::*;

    #[test]
    fn untitle34_templates_use_only_the_project_sounds_and_play_loud_but_clean() {
        use crate::audio_engine::export::{self, BitDepth, ExportSettings};
        for full in [false, true] {
            let mut p = untitle34(full);
            hydrate_samples(&mut p, &mut |path| crate::project::load_wav(path).ok().map(std::sync::Arc::new));
            // every sample the tracks and the pool refer to is loaded (the kit files, or the factory fallbacks)
            assert!(p.audio_pool.iter().all(|r| r.data.is_some()), "pool sample missing");
            assert!(p.arrangement.tracks.iter().filter(|t| t.instrument == Some(Track::Sampler)).all(|t| t.sampler.sample.as_ref().map_or(false, |r| r.data.is_some())), "sampler sample missing");
            // the clones of the muted tracks are there
            assert!(p.arrangement.tracks.iter().any(|t| t.name == "Pad (clone)") && p.arrangement.tracks.iter().any(|t| t.name == "Arp (clone)"));
            // 12 bars, 4 inserts routed and with effects
            assert_eq!(p.arrangement.song_end_bar(), 12);
            assert!(p.insert_fx.iter().take(4).all(|c| !c.is_empty()));
            let data = export::render(&p, &ExportSettings { sample_rate: 48000, depth: BitDepth::Pcm16, bars: 12, tail_secs: 0.0 });
            assert!(data.iter().all(|v| v.is_finite()));
            let peak = data.iter().fold(0.0f32, |m, v| m.max(v.abs()));
            assert!(peak > 0.5 && peak <= 1.0, "peak {peak}");
            // the same project converts and survives a save / load
            p.adopt_track_params();
            let path = std::env::temp_dir().join(format!("wynteria_u34_{full}.synthesia"));
            p.save(&path).unwrap();
            let (q, w) = Project::load(&path).unwrap();
            let _ = std::fs::remove_file(&path);
            assert!(w.is_empty(), "{w:?}");
            assert_eq!(q.arrangement.clips.len(), p.arrangement.clips.len());
        }
        // the Bounce version mutes pad and arp, the Full version plays them
        let (b, f) = (untitle34(false), untitle34(true));
        let muted = |p: &Project, n: &str| p.arrangement.tracks.iter().find(|t| t.name == n).map_or(false, |t| t.mute);
        assert!(muted(&b, "Pad") && muted(&b, "Arp") && !muted(&f, "Pad") && !muted(&f, "Arp"));
    }

    #[test]
    fn merging_a_template_keeps_the_playlist_consistent() {
        let mut a = og_buda();
        let before_tracks = a.arrangement.tracks.len();
        let before_clips = a.arrangement.clips.len();
        let added = merge_template(&mut a, soda_luv()).unwrap();
        assert_eq!(a.arrangement.tracks.len(), before_tracks + added);
        assert!(a.arrangement.clips.len() > before_clips);
        for c in &a.arrangement.clips {
            assert!(c.lane < a.arrangement.tracks.len());
            if c.audio {
                assert!(c.pattern < a.audio_pool.len());
            } else {
                assert!(c.pattern < a.patterns.len());
            }
        }
    }

    #[test]
    fn mixed_templates_stay_in_range_and_use_the_inserts() {
        use crate::audio_engine::Engine;
        use std::sync::Arc;
        for (name, make) in TEMPLATES.iter().filter(|(n, _)| !n.starts_with("Empty") && !has_own_mix(n)) {
            let mut p = make();
            finish_mix(&mut p);
            assert!(p.insert_fx.iter().take(5).all(|c| !c.is_empty()), "{name}: an insert has no effects");
            p.song_mode = false;
            p.current_pattern = 1;
            let mut e = Engine::new(48000.0);
            e.set_project(Arc::new(p));
            e.play();
            let mut buf = vec![0.0f32; 48000 * 2 * 4];
            for c in buf.chunks_mut(512) {
                e.render(c);
            }
            assert!(buf.iter().all(|v| v.is_finite()), "{name}: not finite");
            let peak = buf.iter().fold(0.0f32, |m, v| m.max(v.abs()));
            assert!(peak > 0.15 && peak <= 1.0, "{name}: mixed peak {peak}");
        }
    }

    #[test]
    fn phonk_cowbell_hook_is_heard_through_the_sampler() {
        use crate::audio_engine::Engine;
        use std::sync::Arc;
        for (name, make) in TEMPLATES.iter() {
            // the Phonk pack is local only (not in git / the APK); without it see cowbell_hook_stays_on_analog_without_the_pack
            let Some(path) = cowbell_sample_path(name) else { continue };
            let mut p = make();
            cowbell_to_sampler(&mut p);
            let data = crate::project::load_wav(&path).unwrap();
            p.sampler.sample = Some(crate::project::SampleRef { name: "cowbell".into(), path: path.to_string_lossy().to_string(), data: Some(Arc::new(data)) });
            p.song_mode = false;
            p.current_pattern = 1;
            let mut e = Engine::new(48000.0);
            e.set_project(Arc::new(p));
            e.play();
            let mut buf = vec![0.0f32; 48000 * 2 * 4];
            for c in buf.chunks_mut(512) {
                e.render(c);
            }
            let peaks = e.take_peaks();
            let ch = Track::Sampler.channel();
            assert!(peaks[ch][0] + peaks[ch][1] > 0.0, "{name}: the cowbell sample is silent");
        }
    }

    #[test]
    fn cowbell_hook_stays_on_analog_without_the_pack() {
        use crate::audio_engine::Engine;
        use std::sync::Arc;
        for (name, make) in TEMPLATES.iter() {
            if cowbell_sample(name).is_none() || cowbell_sample_path(name).is_some() {
                continue;
            }
            // what load_template does when the cowbell file is missing: the notes stay on the Analog synth and sound
            let mut p = make();
            assert!(p.patterns.iter().any(|pat| pat.notes.iter().any(|n| n.track == Track::Analog)), "{name}: no Analog hook");
            p.song_mode = false;
            p.current_pattern = 1;
            let mut e = Engine::new(48000.0);
            e.set_project(Arc::new(p));
            e.play();
            let mut buf = vec![0.0f32; 48000 * 2 * 4];
            for c in buf.chunks_mut(512) {
                e.render(c);
            }
            let peaks = e.take_peaks();
            let ch = Track::Analog.channel();
            assert!(peaks[ch][0] + peaks[ch][1] > 0.0, "{name}: the hook is silent without the cowbell pack");
        }
    }

    #[test]
    fn every_kit_sound_of_a_template_exists_in_the_samples_folder() {
        // the kits are local-only packs (AGENTS.md: no third-party samples in git / the APK); check them only where present
        if !["808 Mafia", "Phonk Kit", "Yeat KanKan Kit"].iter().all(|d| std::path::Path::new("samples").join(d).is_dir()) {
            eprintln!("sample packs not present, skipping");
            return;
        }
        for (name, _) in TEMPLATES.iter() {
            if let Some(kit) = template_kit(name) {
                for file in kit {
                    let path = std::path::Path::new("samples").join(file);
                    assert!(path.exists(), "{name}: missing {}", path.display());
                    assert!(crate::project::load_wav(&path).is_ok(), "{name}: cannot read {}", path.display());
                }
            }
            if let Some(file) = cowbell_sample(name) {
                let path = std::path::Path::new("samples").join(file);
                assert!(crate::project::load_wav(&path).is_ok(), "{name}: cannot read cowbell {}", path.display());
                let mut p = (TEMPLATES.iter().find(|t| t.0 == *name).unwrap().1)();
                cowbell_to_sampler(&mut p);
                assert!(p.patterns.iter().all(|pt| pt.notes.iter().all(|n| n.track != Track::Analog)), "{name}: analog notes left");
                assert!(p.patterns.iter().any(|pt| pt.notes.iter().any(|n| n.track == Track::Sampler)), "{name}: no cowbell notes");
            }
        }
    }
}


// ---------------------------------------------------------------- hard-hitting trap family (they play with the drum kits)

// OG Buda: dark, bouncy, D# minor; the melody is the saw motif D# - A# - D#(oct) over a sliding 808, rolling hats
fn og_buda_drums(pat: &mut Pattern, full: bool) {
    steps(pat, 0, &[0, 3, 10, 16, 19, 26, 29], 118);
    steps(pat, 1, &[8, 24], 120);
    steps(pat, 2, &[8, 24], 96);
    steps(pat, 3, &range(0, 32, 2), 74);
    steps(pat, 3, &[6, 7, 14, 15, 22, 23, 30, 31], 58);
    steps(pat, 3, &[1, 5, 9, 13, 17, 21, 25, 29], 40);
    steps(pat, 4, &[12, 28], 70);
    if full {
        steps(pat, 3, &[12, 13, 28, 29, 30, 31], 62);
        steps(pat, 1, &[30, 31], 84);
        steps(pat, 0, &[22], 104);
    }
}

pub fn og_buda() -> Project {
    style_project(Style {
        name: "OG Buda",
        bpm: 140.0,
        swing: 0.0,
        drums: og_buda_drums,
        sub: &[(0, 4, 27, false), (3, 3, 27, false), (6, 2, 30, true), (10, 6, 27, false), (16, 4, 27, false), (19, 3, 25, false), (22, 2, 30, true), (26, 6, 27, false)],
        lead_track: Track::Synth,
        lead: &[(0, 3, 63), (2, 2, 75), (3, 3, 70), (4, 2, 63), (6, 2, 75), (8, 3, 63), (10, 2, 75), (11, 3, 70), (12, 2, 63), (14, 2, 75), (16, 3, 63), (18, 2, 75), (19, 3, 70), (20, 2, 63), (22, 2, 75), (24, 3, 63), (26, 2, 75), (27, 3, 70), (28, 2, 63), (30, 2, 75)],
        counter_track: Track::Analog,
        counter: &[(2, 1, 75), (6, 1, 78), (10, 1, 75), (14, 1, 73), (18, 1, 75), (22, 1, 78), (26, 1, 82), (30, 1, 78)],
        chord_a: [51, 54, 58],
        chord_b: [47, 51, 54],
        setup: |p| {
            p.synth = crate::instruments::SynthParams::preset("Pluck");
            p.analog = AnalogParams::preset("PWM Strings");
            p.wt = WavetableParams { volume: 0.4, ..WavetableParams::preset("Vowel Pad") };
            p.bass = Bass808Params { attack: 0.002, decay: 1.2, sustain: 0.6, release: 0.2, glide_ms: 100.0, pitch_drop: 10.0, drive: 5.0, tone: 2100.0, volume: 0.9, ..Bass808Params::default() };
        },
        mix: "Trap",
        delay: (0.75, 0.36, 0.3),
    })
}

// Soda Luv x OG Buda: F# minor, bouncy kick / 808 pairs with slides, a plucky bell hook, rolling hats, sparkling counter-line
fn soda_luv_drums(pat: &mut Pattern, full: bool) {
    steps(pat, 0, &[0, 3, 6, 10, 16, 19, 22, 26], 118);
    steps(pat, 1, &[8, 24], 118);
    steps(pat, 2, &[8, 24], 98);
    steps(pat, 3, &range(0, 32, 2), 72);
    steps(pat, 3, &range(1, 32, 4), 44);
    steps(pat, 3, &[13, 14, 15, 29, 30, 31], 64);
    steps(pat, 4, &[11, 27], 66);
    if full {
        steps(pat, 3, &[12, 28], 70);
        steps(pat, 0, &[30], 108);
        steps(pat, 1, &[23, 31], 80);
    }
}

pub fn soda_luv() -> Project {
    style_project(Style {
        name: "Soda Luv x OG Buda",
        bpm: 142.0,
        swing: 0.0,
        drums: soda_luv_drums,
        sub: &[(0, 3, 30, false), (3, 2, 30, false), (5, 3, 33, true), (8, 3, 28, false), (12, 4, 30, false), (16, 3, 30, false), (19, 2, 30, false), (21, 3, 26, true), (24, 4, 28, false), (28, 2, 33, false), (30, 2, 30, true)],
        lead_track: Track::Synth,
        lead: &[(0, 2, 78), (3, 1, 81), (4, 2, 85), (7, 1, 83), (8, 3, 81), (12, 2, 78), (14, 2, 76), (16, 2, 78), (19, 1, 81), (20, 2, 86), (23, 1, 85), (24, 3, 83), (28, 2, 81), (30, 2, 78)],
        counter_track: Track::Analog,
        counter: &[(2, 1, 90), (6, 1, 93), (10, 1, 90), (14, 1, 88), (18, 1, 90), (22, 1, 93), (26, 1, 97), (30, 1, 93)],
        chord_a: [54, 57, 61],
        chord_b: [50, 54, 57],
        setup: |p| {
            p.synth = crate::instruments::SynthParams::preset("Bell");
            p.analog = AnalogParams::preset("PWM Strings");
            p.wt = WavetableParams { volume: 0.4, ..WavetableParams::preset("Vowel Pad") };
            p.bass = Bass808Params { attack: 0.002, decay: 1.3, sustain: 0.6, release: 0.2, glide_ms: 130.0, pitch_drop: 10.0, drive: 5.5, tone: 2100.0, volume: 0.9, ..Bass808Params::default() };
        },
        mix: "Trap",
        delay: (0.75, 0.4, 0.35),
    })
}

// OG Buda 2: harder and faster-hit, D# minor, long sliding 808s, double kicks, snare rolls into the hook
fn buda_hard_drums(pat: &mut Pattern, full: bool) {
    steps(pat, 0, &[0, 2, 7, 10, 16, 18, 23, 26, 28], 124);
    steps(pat, 1, &[8, 24], 124);
    steps(pat, 2, &[8, 24], 104);
    steps(pat, 3, &range(0, 32, 2), 76);
    steps(pat, 3, &[3, 7, 11, 15, 19, 23, 27, 31], 48);
    steps(pat, 3, &[14, 15, 30, 31], 70);
    steps(pat, 4, &[12, 29], 72);
    if full {
        steps(pat, 1, &[26, 27, 28, 29, 30, 31], 90);
        steps(pat, 0, &[30], 112);
    }
}

pub fn buda_hard() -> Project {
    style_project(Style {
        name: "OG Buda 2",
        bpm: 144.0,
        swing: 0.0,
        drums: buda_hard_drums,
        sub: &[(0, 2, 27, false), (2, 5, 27, true), (7, 3, 30, false), (10, 6, 27, true), (16, 2, 25, false), (18, 5, 25, true), (23, 3, 28, false), (26, 6, 30, true)],
        lead_track: Track::Synth,
        lead: &[(0, 2, 63), (2, 2, 70), (4, 2, 75), (6, 2, 70), (8, 3, 63), (11, 1, 66), (12, 2, 70), (14, 2, 75), (16, 2, 61), (18, 2, 68), (20, 2, 73), (22, 2, 68), (24, 3, 61), (27, 1, 63), (28, 2, 68), (30, 2, 73)],
        counter_track: Track::Analog,
        counter: &[(1, 1, 87), (5, 1, 90), (9, 1, 87), (13, 1, 85), (17, 1, 85), (21, 1, 88), (25, 1, 92), (29, 1, 88)],
        chord_a: [51, 54, 58],
        chord_b: [49, 52, 56],
        setup: |p| {
            p.synth = crate::instruments::SynthParams::preset("Pluck");
            p.analog = AnalogParams::preset("PWM Strings");
            p.wt = WavetableParams { volume: 0.35, ..WavetableParams::preset("Vowel Pad") };
            p.bass = Bass808Params { attack: 0.002, decay: 1.6, sustain: 0.65, release: 0.25, glide_ms: 160.0, pitch_drop: 12.0, drive: 7.0, tone: 2000.0, volume: 0.92, ..Bass808Params::default() };
        },
        mix: "Trap",
        delay: (0.5, 0.34, 0.28),
    })
}

// OG Buda 3: dark bell loop, sparse kicks, heavy 808, clap snare and hat stutters
fn buda_bells_drums(pat: &mut Pattern, full: bool) {
    steps(pat, 0, &[0, 6, 10, 16, 22, 27], 120);
    steps(pat, 1, &[8, 24], 116);
    steps(pat, 2, &[8, 24], 100);
    steps(pat, 3, &range(0, 32, 2), 68);
    steps(pat, 3, &[5, 13, 21, 29], 46);
    steps(pat, 3, &[11, 12, 27, 28, 29], 64);
    steps(pat, 4, &[4, 20], 66);
    if full {
        steps(pat, 3, &[14, 15, 30, 31], 74);
        steps(pat, 0, &[3, 19], 100);
    }
}

pub fn buda_bells() -> Project {
    style_project(Style {
        name: "OG Buda 3",
        bpm: 138.0,
        swing: 0.0,
        drums: buda_bells_drums,
        sub: &[(0, 6, 28, false), (6, 4, 28, false), (10, 6, 31, true), (16, 6, 26, false), (22, 5, 26, false), (27, 5, 29, true)],
        lead_track: Track::Synth,
        lead: &[(0, 2, 76), (3, 2, 79), (6, 2, 83), (8, 4, 79), (14, 2, 76), (16, 2, 74), (19, 2, 78), (22, 2, 81), (24, 4, 78), (30, 2, 74)],
        counter_track: Track::Wavetable,
        counter: &[(0, 8, 64), (8, 8, 67), (16, 8, 62), (24, 8, 66)],
        chord_a: [52, 55, 59],
        chord_b: [50, 53, 57],
        setup: |p| {
            p.synth = crate::instruments::SynthParams::preset("Bell");
            p.analog = AnalogParams::preset("PWM Strings");
            p.wt = WavetableParams { volume: 0.45, ..WavetableParams::preset("Vowel Pad") };
            p.bass = Bass808Params { attack: 0.002, decay: 1.5, sustain: 0.6, release: 0.22, glide_ms: 120.0, pitch_drop: 10.0, drive: 6.0, tone: 2100.0, volume: 0.9, ..Bass808Params::default() };
        },
        mix: "Trap",
        delay: (0.75, 0.38, 0.32),
    })
}

// Soda Luv 2: fast rolling hats, triplet-feel bounce, light plucky hook
fn soda_rolls_drums(pat: &mut Pattern, full: bool) {
    steps(pat, 0, &[0, 4, 7, 10, 16, 20, 23, 26], 118);
    steps(pat, 1, &[8, 24], 118);
    steps(pat, 2, &[8, 24], 96);
    steps(pat, 3, &range(0, 32, 2), 70);
    steps(pat, 3, &[1, 3, 9, 11, 17, 19, 25, 27], 50);
    steps(pat, 3, &[12, 13, 14, 15, 28, 29, 30, 31], 66);
    steps(pat, 4, &[6, 22], 62);
    if full {
        steps(pat, 1, &[31], 84);
        steps(pat, 0, &[30], 108);
    }
}

pub fn soda_rolls() -> Project {
    style_project(Style {
        name: "Soda Luv 2",
        bpm: 146.0,
        swing: 0.0,
        drums: soda_rolls_drums,
        sub: &[(0, 4, 30, false), (4, 3, 30, false), (7, 3, 33, true), (10, 6, 28, false), (16, 4, 30, false), (20, 3, 30, false), (23, 3, 35, true), (26, 6, 33, false)],
        lead_track: Track::Synth,
        lead: &[(0, 1, 78), (2, 1, 81), (4, 2, 85), (6, 1, 81), (8, 2, 83), (10, 2, 81), (12, 2, 78), (16, 1, 78), (18, 1, 81), (20, 2, 88), (22, 1, 85), (24, 2, 86), (26, 2, 83), (28, 4, 81)],
        counter_track: Track::Analog,
        counter: &[(3, 1, 90), (7, 1, 93), (11, 1, 90), (15, 1, 88), (19, 1, 90), (23, 1, 93), (27, 1, 97), (31, 1, 93)],
        chord_a: [54, 57, 61],
        chord_b: [52, 56, 59],
        setup: |p| {
            p.synth = crate::instruments::SynthParams::preset("Pluck");
            p.analog = AnalogParams::preset("PWM Strings");
            p.wt = WavetableParams { volume: 0.38, ..WavetableParams::preset("Vowel Pad") };
            p.bass = Bass808Params { attack: 0.002, decay: 1.2, sustain: 0.55, release: 0.2, glide_ms: 110.0, pitch_drop: 9.0, drive: 5.0, tone: 2200.0, volume: 0.9, ..Bass808Params::default() };
        },
        mix: "Trap",
        delay: (0.75, 0.4, 0.3),
    })
}

// Yeat rage: fast, detuned super-saw stabs, bouncy kick / 808 pairs, hat rolls
fn yeat_drums(pat: &mut Pattern, full: bool) {
    steps(pat, 0, &[0, 3, 6, 10, 16, 19, 22, 26, 29], 116);
    steps(pat, 1, &[8, 24], 112);
    steps(pat, 2, &[8, 24], 100);
    steps(pat, 3, &range(0, 32, 2), 66);
    steps(pat, 3, &[4, 5, 12, 13, 20, 21, 28, 29, 30, 31], 56);
    steps(pat, 4, &[14, 30], 64);
    if full {
        steps(pat, 1, &[28, 29, 30, 31], 80);
        steps(pat, 3, &[15, 31], 76);
    }
}

pub fn yeat_rage_beat() -> Project {
    style_project(Style {
        name: "Yeat Rage",
        bpm: 150.0,
        swing: 0.0,
        drums: yeat_drums,
        sub: &[(0, 3, 30, false), (3, 3, 30, false), (6, 3, 33, false), (10, 6, 30, false), (16, 3, 30, false), (19, 3, 30, false), (22, 3, 35, false), (26, 6, 33, true)],
        lead_track: Track::Synth,
        lead: &[(0, 2, 78), (2, 2, 81), (4, 2, 85), (6, 2, 81), (8, 2, 78), (10, 2, 81), (12, 2, 83), (14, 2, 81), (16, 2, 78), (18, 2, 81), (20, 2, 85), (22, 2, 88), (24, 2, 86), (26, 2, 85), (28, 2, 81), (30, 2, 78)],
        counter_track: Track::Wavetable,
        counter: &[(0, 6, 66), (8, 6, 69), (16, 6, 64), (24, 6, 68)],
        chord_a: [54, 57, 61],
        chord_b: [50, 54, 57],
        setup: |p| {
            p.synth = crate::instruments::SynthParams::preset("Lead");
            p.analog = AnalogParams::preset("Sync Lead");
            p.wt = WavetableParams { volume: 0.6, ..WavetableParams::preset("Super Saw") };
            p.bass = Bass808Params { attack: 0.002, decay: 0.8, sustain: 0.5, release: 0.15, glide_ms: 70.0, pitch_drop: 8.0, drive: 6.0, tone: 2400.0, volume: 0.9, ..Bass808Params::default() };
        },
        mix: "Trap",
        delay: (0.5, 0.35, 0.3),
    })
}

// Memphis phonk: cowbell hook, bell lead, triplet-ish hats, distorted 808
fn memphis_drums(pat: &mut Pattern, full: bool) {
    steps(pat, 0, &[0, 6, 10, 16, 22, 26], 120);
    steps(pat, 1, &[8, 24], 116);
    steps(pat, 2, &[8, 24], 92);
    steps(pat, 3, &range(0, 32, 2), 66);
    steps(pat, 3, &[7, 15, 23, 31], 60);
    steps(pat, 4, &[4, 12, 20, 28], 72);
    if full {
        steps(pat, 3, &[13, 14, 29, 30], 62);
        steps(pat, 1, &[30, 31], 84);
    }
}

pub fn memphis_phonk() -> Project {
    style_project(Style {
        name: "Memphis Phonk",
        bpm: 132.0,
        swing: 0.0,
        drums: memphis_drums,
        sub: &[(0, 6, 31, false), (6, 2, 31, false), (10, 6, 34, false), (16, 6, 29, false), (22, 2, 29, false), (26, 6, 31, false)],
        lead_track: Track::Synth,
        lead: &[(0, 2, 67), (4, 2, 70), (8, 2, 74), (12, 2, 70), (16, 2, 65), (20, 2, 68), (24, 2, 72), (28, 2, 68)],
        counter_track: Track::Analog,
        counter: &[(2, 1, 79), (6, 1, 79), (10, 1, 82), (14, 1, 79), (18, 1, 77), (22, 1, 77), (26, 1, 80), (30, 1, 77)],
        chord_a: [55, 58, 62],
        chord_b: [53, 56, 60],
        setup: |p| {
            p.synth = crate::instruments::SynthParams::preset("Bell");
            p.analog = cowbell();
            p.wt = WavetableParams { volume: 0.4, ..WavetableParams::preset("Vowel Pad") };
            p.bass = Bass808Params { attack: 0.002, decay: 0.9, sustain: 0.5, release: 0.15, glide_ms: 40.0, pitch_drop: 9.0, drive: 7.0, tone: 2300.0, volume: 0.9, ..Bass808Params::default() };
        },
        mix: "Phonk",
        delay: (0.75, 0.3, 0.25),
    })
}

// Metro-style dark trap: sparse, huge 808, slow pad / choir, triplet hat rolls only at the ends
fn metro_drums(pat: &mut Pattern, full: bool) {
    steps(pat, 0, &[0, 10, 16, 22, 26], 118);
    steps(pat, 1, &[8, 24], 112);
    steps(pat, 2, &[8, 24], 88);
    steps(pat, 3, &range(0, 32, 2), 62);
    steps(pat, 3, &[28, 29, 30, 31], 58);
    steps(pat, 4, &[14, 30], 56);
    if full {
        steps(pat, 3, &[12, 13, 14, 15], 54);
        steps(pat, 1, &[31], 76);
    }
}

pub fn metro_dark() -> Project {
    style_project(Style {
        name: "Metro Dark Trap",
        bpm: 140.0,
        swing: 0.0,
        drums: metro_drums,
        sub: &[(0, 8, 29, false), (10, 6, 29, false), (16, 8, 27, false), (26, 6, 32, true)],
        lead_track: Track::Synth,
        lead: &[(0, 8, 77), (8, 8, 75), (16, 8, 74), (24, 8, 72)],
        counter_track: Track::Analog,
        counter: &[(4, 1, 89), (12, 1, 87), (20, 1, 86), (28, 1, 84)],
        chord_a: [53, 56, 60],
        chord_b: [49, 53, 56],
        setup: |p| {
            p.synth = crate::instruments::SynthParams::preset("Pad");
            p.analog = AnalogParams::preset("Soft Keys");
            p.wt = WavetableParams { volume: 0.5, ..WavetableParams::preset("Vowel Pad") };
            p.bass = Bass808Params { attack: 0.002, decay: 1.4, sustain: 0.65, release: 0.25, glide_ms: 80.0, pitch_drop: 10.0, drive: 4.0, tone: 2000.0, volume: 0.95, ..Bass808Params::default() };
        },
        mix: "Trap",
        delay: (0.75, 0.4, 0.3),
    })
}

// Lex Luger: brass stabs, snare rolls, rolling hats, hard 808 hits
fn luger_drums(pat: &mut Pattern, full: bool) {
    steps(pat, 0, &[0, 6, 16, 22], 120);
    steps(pat, 1, &[8, 24], 120);
    steps(pat, 2, &[8, 24], 98);
    steps(pat, 3, &range(0, 32, 2), 72);
    steps(pat, 3, &[11, 15, 27, 31], 56);
    steps(pat, 4, &[12, 28], 66);
    if full {
        steps(pat, 1, &[28, 29, 30, 31], 96);
        steps(pat, 3, &[13, 14, 29, 30], 60);
    }
}

pub fn lex_luger() -> Project {
    style_project(Style {
        name: "Lex Luger Hard",
        bpm: 140.0,
        swing: 0.0,
        drums: luger_drums,
        sub: &[(0, 4, 27, false), (4, 4, 27, false), (8, 6, 30, false), (16, 4, 27, false), (20, 4, 25, false), (24, 6, 30, false)],
        lead_track: Track::Synth,
        lead: &[(0, 1, 63), (3, 1, 66), (6, 1, 70), (8, 2, 68), (16, 1, 63), (19, 1, 66), (22, 1, 70), (24, 2, 68)],
        counter_track: Track::Analog,
        counter: &[(0, 16, 51), (16, 16, 49)],
        chord_a: [51, 54, 58],
        chord_b: [49, 53, 56],
        setup: |p| {
            p.synth = crate::instruments::SynthParams::preset("Lead");
            p.analog = AnalogParams::preset("Sync Lead");
            p.wt = WavetableParams { volume: 0.45, ..WavetableParams::preset("Metal Pluck") };
            p.bass = Bass808Params { attack: 0.002, decay: 1.0, sustain: 0.55, release: 0.18, glide_ms: 30.0, pitch_drop: 9.0, drive: 6.0, tone: 2600.0, volume: 0.9, ..Bass808Params::default() };
        },
        mix: "Trap",
        delay: (0.75, 0.3, 0.25),
    })
}

// NY drill: sliding 808s, off-beat snare, bouncing hats, dark plucks
fn ny_drill_drums(pat: &mut Pattern, full: bool) {
    steps(pat, 0, &[0, 7, 10, 16, 23, 26], 118);
    steps(pat, 1, &[12, 28], 118);
    steps(pat, 2, &[12, 28], 90);
    steps(pat, 3, &range(0, 32, 2), 68);
    steps(pat, 3, &[5, 13, 21, 29], 56);
    steps(pat, 4, &[3, 19], 62);
    if full {
        steps(pat, 3, &[14, 15, 30, 31], 62);
        steps(pat, 1, &[30, 31], 76);
    }
}

pub fn ny_drill() -> Project {
    style_project(Style {
        name: "NY Drill",
        bpm: 142.0,
        swing: 0.0,
        drums: ny_drill_drums,
        sub: &[(0, 5, 29, true), (5, 4, 32, true), (10, 6, 29, false), (16, 5, 27, true), (21, 4, 30, true), (26, 6, 27, false)],
        lead_track: Track::Synth,
        lead: &[(0, 1, 65), (3, 1, 68), (6, 1, 72), (8, 1, 68), (11, 1, 65), (14, 1, 63), (16, 1, 65), (19, 1, 68), (22, 1, 72), (24, 1, 75), (27, 1, 72), (30, 1, 68)],
        counter_track: Track::Analog,
        counter: &[(0, 16, 53), (16, 16, 51)],
        chord_a: [53, 56, 60],
        chord_b: [51, 55, 58],
        setup: |p| {
            p.synth = crate::instruments::SynthParams::preset("Pluck");
            p.analog = AnalogParams::preset("PWM Strings");
            p.wt = WavetableParams { volume: 0.4, ..WavetableParams::preset("Vowel Pad") };
            p.bass = Bass808Params { attack: 0.002, decay: 1.1, sustain: 0.55, release: 0.2, glide_ms: 90.0, pitch_drop: 10.0, drive: 5.0, tone: 2100.0, volume: 0.9, ..Bass808Params::default() };
        },
        mix: "Trap",
        delay: (0.5, 0.3, 0.25),
    })
}


// Russian melodic trap (a general take on the style, not a copy of one artist): bells over a wide pad, gliding 808, light hat rolls
fn rus_drums(pat: &mut Pattern, full: bool) {
    steps(pat, 0, &[0, 7, 16, 23, 26], 112);
    steps(pat, 1, &[8, 24], 106);
    steps(pat, 2, &[8, 24], 88);
    steps(pat, 3, &range(0, 32, 2), 64);
    steps(pat, 3, &[14, 15, 30, 31], 56);
    steps(pat, 4, &[12, 28], 58);
    if full {
        steps(pat, 3, &[6, 7, 22, 23], 52);
        steps(pat, 1, &[31], 70);
    }
}

pub fn russian_melodic() -> Project {
    style_project(Style {
        name: "Russian Melodic Trap",
        bpm: 138.0,
        swing: 0.04,
        drums: rus_drums,
        sub: &[(0, 6, 30, false), (7, 3, 30, true), (10, 6, 33, true), (16, 6, 28, false), (23, 3, 28, true), (26, 6, 31, true)],
        lead_track: Track::Synth,
        lead: &[(0, 2, 78), (2, 2, 81), (4, 4, 85), (8, 2, 83), (10, 2, 81), (12, 4, 78), (16, 2, 76), (18, 2, 79), (20, 4, 83), (24, 2, 81), (26, 2, 79), (28, 4, 76)],
        counter_track: Track::Analog,
        counter: &[(0, 16, 61), (16, 16, 59)],
        chord_a: [54, 57, 61],
        chord_b: [52, 56, 59],
        setup: |p| {
            p.synth = crate::instruments::SynthParams::preset("Bell");
            p.analog = AnalogParams::preset("Soft Keys");
            p.wt = WavetableParams { volume: 0.55, ..WavetableParams::preset("Vowel Pad") };
            p.bass = Bass808Params { attack: 0.002, decay: 1.2, sustain: 0.6, release: 0.2, glide_ms: 60.0, pitch_drop: 8.0, drive: 3.5, tone: 2200.0, volume: 0.9, ..Bass808Params::default() };
        },
        mix: "Trap",
        delay: (0.75, 0.4, 0.3),
    })
}


// ---------------------------------------------------------------- hard trap and riddim dubstep

// Hard trap: heavy kick pattern, rolling hats with stutters, distorted gliding 808, dark bell and a screaming lead
fn hard_trap_drums(pat: &mut Pattern, full: bool) {
    steps(pat, 0, &[0, 3, 6, 10, 14, 16, 19, 22, 26, 30], 122);
    steps(pat, 1, &[8, 24], 124);
    steps(pat, 2, &[8, 24], 104);
    steps(pat, 3, &range(0, 32, 2), 76);
    steps(pat, 3, &[5, 7, 13, 15, 21, 23, 29, 31], 60);
    steps(pat, 4, &[4, 12, 20, 28], 72);
    if full {
        steps(pat, 3, &[12, 13, 14, 15, 28, 29, 30, 31], 68);
        steps(pat, 1, &[28, 29, 30, 31], 96);
    }
}

pub fn hard_trap() -> Project {
    style_project(Style {
        name: "Hard Trap",
        bpm: 145.0,
        swing: 0.0,
        drums: hard_trap_drums,
        sub: &[(0, 3, 28, false), (3, 3, 28, false), (6, 4, 31, true), (10, 4, 28, false), (14, 2, 26, false), (16, 3, 28, false), (19, 3, 28, false), (22, 4, 33, true), (26, 4, 28, false), (30, 2, 26, true)],
        lead_track: Track::Synth,
        lead: &[(0, 1, 64), (2, 1, 67), (4, 2, 71), (8, 1, 69), (10, 1, 67), (12, 2, 64), (16, 1, 64), (18, 1, 67), (20, 2, 71), (24, 1, 72), (26, 1, 71), (28, 2, 67)],
        counter_track: Track::Analog,
        counter: &[(0, 16, 52), (16, 16, 50)],
        chord_a: [52, 55, 59],
        chord_b: [50, 53, 57],
        setup: |p| {
            p.synth = crate::instruments::SynthParams::preset("Lead");
            p.analog = AnalogParams::preset("PWM Strings");
            p.wt = WavetableParams { volume: 0.45, ..WavetableParams::preset("Metal Pluck") };
            p.bass = Bass808Params { attack: 0.002, decay: 1.0, sustain: 0.55, release: 0.15, glide_ms: 70.0, pitch_drop: 10.0, drive: 8.0, tone: 2300.0, volume: 0.92, ..Bass808Params::default() };
        },
        mix: "Trap",
        delay: (0.5, 0.3, 0.22),
    })
}

// Hard trap 2: rage tempo, detuned saw stabs, 808 pounding on every kick
fn hard_trap_2_drums(pat: &mut Pattern, full: bool) {
    steps(pat, 0, &[0, 2, 6, 8, 10, 16, 18, 22, 24, 26], 122);
    steps(pat, 1, &[12, 28], 120);
    steps(pat, 2, &[12, 28], 100);
    steps(pat, 3, &range(0, 32, 2), 72);
    steps(pat, 3, &[3, 7, 11, 15, 19, 23, 27, 31], 58);
    steps(pat, 4, &[6, 22], 66);
    if full {
        steps(pat, 3, &[13, 14, 15, 29, 30, 31], 70);
        steps(pat, 1, &[30, 31], 92);
    }
}

pub fn hard_trap_2() -> Project {
    style_project(Style {
        name: "Hard Trap 2",
        bpm: 160.0,
        swing: 0.0,
        drums: hard_trap_2_drums,
        sub: &[(0, 2, 29, false), (2, 3, 29, false), (6, 2, 32, true), (8, 2, 29, false), (10, 5, 27, true), (16, 2, 29, false), (18, 3, 29, false), (22, 2, 34, true), (24, 2, 29, false), (26, 5, 27, true)],
        lead_track: Track::Synth,
        lead: &[(0, 2, 77), (3, 1, 80), (6, 2, 84), (8, 2, 82), (11, 1, 80), (14, 2, 77), (16, 2, 77), (19, 1, 80), (22, 2, 84), (24, 2, 87), (27, 1, 84), (30, 2, 80)],
        counter_track: Track::Wavetable,
        counter: &[(0, 3, 65), (8, 3, 68), (16, 3, 63), (24, 3, 67)],
        chord_a: [53, 56, 60],
        chord_b: [51, 55, 58],
        setup: |p| {
            p.synth = crate::instruments::SynthParams::preset("Lead");
            p.analog = AnalogParams::preset("Sync Lead");
            p.wt = WavetableParams { volume: 0.6, ..WavetableParams::preset("Super Saw") };
            p.bass = Bass808Params { attack: 0.002, decay: 0.7, sustain: 0.45, release: 0.12, glide_ms: 50.0, pitch_drop: 9.0, drive: 9.0, tone: 2500.0, volume: 0.92, ..Bass808Params::default() };
        },
        mix: "Trap",
        delay: (0.5, 0.3, 0.2),
    })
}

// Riddim dubstep: half-time 150, a minimal drum pattern and a stuttering growl bass that repeats the same "bwa-bwa" figure
fn riddim_drums(pat: &mut Pattern, full: bool) {
    steps(pat, 0, &[0, 10, 16, 26], 124);
    steps(pat, 1, &[8, 24], 122);
    steps(pat, 2, &[8, 24], 100);
    steps(pat, 3, &[4, 12, 20, 28], 80);
    steps(pat, 3, &range(1, 32, 2), 36);
    steps(pat, 4, &[14, 30], 70);
    if full {
        steps(pat, 1, &[30, 31], 96);
        steps(pat, 3, &[14, 15, 30, 31], 74);
    }
}

pub fn riddim_dubstep() -> Project {
    style_project(Style {
        name: "Riddim Dubstep",
        bpm: 150.0,
        swing: 0.0,
        drums: riddim_drums,
        sub: &[(0, 4, 28, false), (8, 4, 28, false), (16, 4, 26, false), (24, 4, 28, false)],
        lead_track: Track::Synth,
        lead: &[(0, 1, 76), (6, 1, 79), (8, 2, 76), (16, 1, 74), (22, 1, 77), (24, 2, 74)],
        counter_track: Track::Wavetable,
        counter: &[(0, 2, 40), (3, 1, 40), (4, 2, 40), (7, 1, 43), (8, 2, 40), (11, 1, 40), (12, 2, 38), (15, 1, 40), (16, 2, 40), (19, 1, 40), (20, 2, 40), (23, 1, 43), (24, 2, 41), (27, 1, 40), (28, 2, 38), (31, 1, 36)],
        chord_a: [52, 55, 59],
        chord_b: [50, 53, 57],
        setup: |p| {
            p.synth = crate::instruments::SynthParams::preset("Lead");
            p.wt = WavetableParams { volume: 0.75, ..WavetableParams::preset("Growl Bass") };
            p.analog = AnalogParams::preset("Sync Lead");
            p.bass = Bass808Params { attack: 0.002, decay: 0.5, sustain: 0.35, release: 0.1, glide_ms: 5.0, pitch_drop: 6.0, drive: 6.0, tone: 1700.0, volume: 0.85, ..Bass808Params::default() };
        },
        mix: "Club",
        delay: (0.5, 0.25, 0.15),
    })
}

// Riddim 2 (tearout): the same half-time idea, busier kicks, a faster stuttering growl and a screech lead
fn tearout_drums(pat: &mut Pattern, full: bool) {
    steps(pat, 0, &[0, 3, 10, 13, 16, 19, 26, 29], 124);
    steps(pat, 1, &[8, 24], 124);
    steps(pat, 2, &[8, 24], 104);
    steps(pat, 3, &range(0, 32, 2), 60);
    steps(pat, 4, &[6, 22], 70);
    if full {
        steps(pat, 1, &[28, 30, 31], 100);
        steps(pat, 3, &[13, 15, 29, 31], 74);
    }
}

pub fn riddim_tearout() -> Project {
    style_project(Style {
        name: "Riddim Dubstep 2",
        bpm: 150.0,
        swing: 0.0,
        drums: tearout_drums,
        sub: &[(0, 3, 28, false), (4, 2, 28, false), (8, 3, 31, false), (12, 2, 28, false), (16, 3, 26, false), (20, 2, 26, false), (24, 3, 29, false), (28, 2, 28, false)],
        lead_track: Track::Synth,
        lead: &[(0, 1, 88), (2, 1, 86), (4, 1, 88), (8, 2, 91), (12, 1, 88), (16, 1, 86), (18, 1, 84), (20, 1, 86), (24, 2, 88), (28, 2, 84)],
        counter_track: Track::Wavetable,
        counter: &[(0, 1, 40), (1, 1, 40), (2, 1, 40), (3, 1, 43), (4, 1, 40), (5, 1, 40), (6, 1, 38), (7, 1, 40), (8, 1, 40), (9, 1, 40), (10, 1, 41), (11, 1, 40), (12, 1, 38), (13, 1, 40), (14, 1, 36), (15, 1, 38), (16, 1, 40), (17, 1, 40), (18, 1, 40), (19, 1, 43), (20, 1, 40), (21, 1, 40), (22, 1, 38), (23, 1, 40), (24, 1, 41), (25, 1, 40), (26, 1, 38), (27, 1, 40), (28, 1, 36), (29, 1, 38), (30, 1, 40), (31, 1, 36)],
        chord_a: [52, 55, 59],
        chord_b: [50, 53, 57],
        setup: |p| {
            p.synth = crate::instruments::SynthParams::preset("Lead");
            p.wt = WavetableParams { volume: 0.8, ..WavetableParams::preset("Growl Bass") };
            p.analog = AnalogParams::preset("Sync Lead");
            p.bass = Bass808Params { attack: 0.002, decay: 0.45, sustain: 0.3, release: 0.1, glide_ms: 4.0, pitch_drop: 5.0, drive: 7.0, tone: 1800.0, volume: 0.85, ..Bass808Params::default() };
        },
        mix: "Club",
        delay: (0.5, 0.25, 0.15),
    })
}

#[cfg(test)]
mod inspect {
    use super::*;

    /// cargo test inspect_user_project -- --ignored --nocapture
    #[test]
    #[ignore]
    fn inspect_user_project() {
        let docs = std::path::PathBuf::from(std::env::var("USERPROFILE").unwrap()).join("Documents").join("SYNTHESIA Projects");
        let (p, w) = Project::load(&docs.join("Untitle34у5екd.synthesia")).unwrap();
        println!("warnings: {w:?}");
        println!("name {} bpm {} key {} scale {} swing {} song_mode {} patterns {} clips {} tracks {}", p.name, p.bpm, p.key_root, p.key_scale, p.swing, p.song_mode, p.patterns.len(), p.arrangement.clips.len(), p.arrangement.tracks.len());
        for (i, t) in p.arrangement.tracks.iter().enumerate() {
            let n = p.arrangement.clips.iter().filter(|c| c.lane == i).count();
            let ext: u32 = p.arrangement.clips.iter().filter(|c| c.lane == i).map(|c| c.end_bar()).max().unwrap_or(0);
            println!("track {i}: {:?} '{}' inst {:?} out {:?} vol {:.2} mute {} clips {n} ends bar {ext} sampler {:?}", t.kind, t.name, t.instrument, t.output, t.volume, t.mute, t.sampler.sample.as_ref().map(|s| s.name.clone()));
        }
        println!("pool: {:?}", p.audio_pool.iter().map(|s| (s.name.clone(), s.data.as_ref().map(|d| d.mono.len() as f32 / d.rate as f32))).collect::<Vec<_>>());
        println!("drums: {:?}", p.drum_samples.iter().map(|s| s.as_ref().map(|s| s.name.clone())).collect::<Vec<_>>());
        for (i, pt) in p.patterns.iter().enumerate().take(40) {
            println!("pattern {i} '{}' steps {} notes {} clip_only {}", pt.name, pt.length_steps, pt.notes.len(), pt.clip_only);
        }
        println!("song end bar {}", p.arrangement.song_end_bar());
    }

    fn render_stats(p: &Project, bars: u32) -> (f32, f32, f32, f32) {
        use crate::audio_engine::export::{self, BitDepth, ExportSettings};
        let mut q = p.clone();
        q.song_mode = true;
        q.loop_on = false;
        let data = export::render(&q, &ExportSettings { sample_rate: 48000, depth: BitDepth::Pcm16, bars, tail_secs: 0.0 });
        let n = data.len() / 2;
        let mono: Vec<f32> = (0..n).map(|i| (data[i * 2] + data[i * 2 + 1]) * 0.5).collect();
        let peak = mono.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        let rms = (mono.iter().map(|v| v * v).sum::<f32>() / n.max(1) as f32).sqrt();
        // low band (< 150 Hz) and high band (> 4 kHz) energy via one-pole filters
        let (mut lp, mut lp2) = (0.0f32, 0.0f32);
        let (a_lo, a_hi) = (1.0 - (-2.0 * std::f32::consts::PI * 150.0 / 48000.0).exp(), 1.0 - (-2.0 * std::f32::consts::PI * 4000.0 / 48000.0).exp());
        let (mut e_lo, mut e_hi) = (0.0f32, 0.0f32);
        for v in &mono {
            lp += a_lo * (v - lp);
            lp2 += a_hi * (v - lp2);
            e_lo += lp * lp;
            e_hi += (v - lp2) * (v - lp2);
        }
        (peak, 20.0 * rms.max(1e-9).log10(), 10.0 * (e_lo / n as f32).max(1e-12).log10(), 10.0 * (e_hi / n as f32).max(1e-12).log10())
    }

    /// cargo test write_untitle34_previews -- --ignored
    #[test]
    #[ignore]
    fn write_untitle34_previews() {
        use crate::audio_engine::export::{self, BitDepth, ExportSettings};
        let dir = std::path::Path::new("release").join("previews");
        std::fs::create_dir_all(&dir).unwrap();
        for full in [false, true] {
            let mut p = untitle34(full);
            hydrate_samples(&mut p, &mut |path| crate::project::load_wav(path).ok().map(std::sync::Arc::new));
            let st = ExportSettings { sample_rate: 44100, depth: BitDepth::Pcm16, bars: 12, tail_secs: 2.0 };
            let data = export::render(&p, &st);
            export::write_wav(&dir.join(if full { "Untitle34 Full.wav" } else { "Untitle34 Bounce.wav" }), &data, &st).unwrap();
        }
    }

    #[test]
    #[ignore]
    fn analyse_untitle34_templates() {
        for full in [false, true] {
            let mut p = untitle34(full);
            hydrate_samples(&mut p, &mut |path| crate::project::load_wav(path).ok().map(std::sync::Arc::new));
            let (pk, rms, lo, hi) = render_stats(&p, 12);
            println!("{}  ALL peak {pk:.2} rms {rms:.1} dB low {lo:.1} high {hi:.1}", if full { "FULL  " } else { "BOUNCE" });
            if !full {
                for i in 0..p.arrangement.tracks.len() {
                    let mut q = p.clone();
                    for (j, t) in q.arrangement.tracks.iter_mut().enumerate() {
                        t.mute = false;
                        t.solo = j == i;
                    }
                    let (pk, rms, lo, hi) = render_stats(&q, 12);
                    println!("  solo {i:2} {:12} peak {pk:.2} rms {rms:6.1} dB low {lo:6.1} high {hi:6.1}", p.arrangement.tracks[i].name);
                }
            }
        }
    }

    #[test]
    #[ignore]
    fn analyse_user_project() {
        let docs = std::path::PathBuf::from(std::env::var("USERPROFILE").unwrap()).join("Documents").join("SYNTHESIA Projects");
        let (p, _) = Project::load(&docs.join("Untitle34у5екd.synthesia")).unwrap();
        let (pk, rms, lo, hi) = render_stats(&p, 12);
        println!("ALL    peak {pk:.2} rms {rms:.1} dB   low(<150Hz) {lo:.1} dB   high(>4k) {hi:.1} dB");
        for i in 0..p.arrangement.tracks.len() {
            let mut q = p.clone();
            q.arrangement.tracks[i].mute = false;
            for (j, t) in q.arrangement.tracks.iter_mut().enumerate() {
                t.solo = j == i;
            }
            let (pk, rms, lo, hi) = render_stats(&q, 12);
            println!("solo {i:2} {:8} peak {pk:.2} rms {rms:6.1} dB  low {lo:6.1}  high {hi:6.1}", p.arrangement.tracks[i].name);
        }
    }
}


// ---------------------------------------------------------------- Untitle34: the user's own beat as a template
// 140 BPM, half-time trap in F# minor, 12 bars. Only the sounds of that project: 808 Mafia Snare 1, DJ SPINZ 808, Southside Hi Hat,
// Lex Luger Kick, TM88 Open Hat 2 and the wavetable / poly synth. Both versions have the same tracks; the muted tracks of the
// original (pad, arp) exist twice: the original and a clone to play with.

/// (start tick, length, pitch) lists of one 2-bar clip, taken from the project.
const U34_SNARE: &[(u32, u32, u8)] = &[(192, 192, 36), (576, 192, 36)];
const U34_808: &[(u32, u32, u8)] = &[(0, 192, 37), (144, 192, 37), (288, 192, 37), (432, 48, 37), (480, 96, 37), (576, 96, 49)];
const U34_PAD: &[(u32, u32, u8)] = &[(0, 192, 61), (384, 192, 62)];
const U34_BASS: &[(u32, u32, u8)] = &[(0, 96, 25), (144, 96, 25), (288, 144, 25), (480, 144, 25), (624, 48, 25), (720, 48, 26)];
const U34_KICK_AUDIO: &[(u32, u32)] = &[(0, 96), (144, 96), (288, 144), (480, 48), (528, 48)];
const U34_HAT_AUDIO: &[(u32, u32)] = &[(0, 48), (144, 48), (288, 48), (480, 48), (528, 48), (672, 96)];

fn u34_lead() -> Vec<(u32, u32, u8)> {
    (0..4).flat_map(|k| [(0, 192, 66), (48, 144, 61), (96, 96, 73), (144, 48, 74)].into_iter().map(move |(s, l, p)| (k * 192 + s, l, p))).collect()
}
fn u34_arp() -> Vec<(u32, u32, u8)> {
    (0..4).flat_map(|k| [(0, 24, 73), (24, 24, 85), (48, 24, 90), (96, 24, 85), (144, 24, 86)].into_iter().map(move |(s, l, p)| (k * 192 + s, l, p))).collect()
}
fn u34_hats() -> Vec<(u32, u32, u8)> {
    (0..16).map(|k| (k * 48, 24, 36)).collect()
}

/// level of every source (1.0 = as in the original project)
struct U34Mix {
    snare: f32,
    k808: f32,
    kick: f32,
    open_hat: f32,
    hats: f32,
    lead: f32,
    arp: f32,
    bass: f32,
    pad: f32,
}

const U34_MIX: U34Mix = U34Mix { snare: 3.0, k808: 0.58, kick: 1.6, open_hat: 1.5, hats: 0.6, lead: 2.4, arp: 2.75, bass: 0.6, pad: 3.3 };

fn untitle34(full: bool) -> Project {
    use crate::arrangement::TrackKind;
    use crate::fx::{FxKind, FxSlot};
    use crate::project::SampleRef;
    let mix = &U34_MIX;
    let mut p = Project { name: if full { "Untitle34 Full".into() } else { "Untitle34 Bounce".into() }, bpm: 140.0, swing: 0.0, song_mode: true, loop_bars: 12, loop_on: false, ..Project::default() };
    p.patterns.clear();
    p.patterns.push(Pattern::new("Pattern 1", 32));
    p.master_volume = 0.7;
    let kit = "samples/808 Mafia";
    let sref = |sub: &str, file: &str| crate::project::SampleRef { name: file.trim_end_matches(".wav").to_string(), path: format!("{kit}/{sub}/{file}"), data: None };
    p.audio_pool = vec![sref("Kicks", "Lex Luger Kick.wav"), sref("Open Hats", "TM88 Open Hat 2.wav")];

    // a MIDI track whose six 2-bar clips each own a copy of the notes
    let midi = |p: &mut Project, name: &str, inst: Track, sample: Option<SampleRef>, notes: &[(u32, u32, u8)], out: usize, mute: bool| -> usize {
        p.arrangement.add_track(TrackKind::Midi);
        let lane = p.arrangement.tracks.len() - 1;
        {
            let t = &mut p.arrangement.tracks[lane];
            t.name = name.to_string();
            t.instrument = Some(inst);
            t.blank = false;
            t.mute = mute;
            t.output = Some(out);
            if let Some(s) = sample {
                t.sampler.sample = Some(s);
                t.sampler.root = 36;
                t.sampler.release = 0.12;
            }
        }
        for k in 0..6u32 {
            let mut pat = Pattern::new(name, 32);
            pat.clip_only = true;
            pat.notes = notes.iter().map(|&(s, l, pt)| Note { start: s, len: l, pitch: pt, vel: 100, track: inst, slide: false, mute: false, pan: 0, chance: 100 }).collect();
            p.patterns.push(pat);
            let pi = p.patterns.len() - 1;
            p.arrangement.clips.push(Clip::new(pi, lane, k * 768, 768, false));
        }
        lane
    };
    let audio = |p: &mut Project, name: &str, pool: usize, hits: &[(u32, u32)], gain: f32, out: usize| {
        p.arrangement.add_track(TrackKind::Audio);
        let lane = p.arrangement.tracks.len() - 1;
        p.arrangement.tracks[lane].name = name.to_string();
        p.arrangement.tracks[lane].output = Some(out);
        for k in 0..6u32 {
            for &(s, l) in hits {
                let mut c = Clip::new(pool, lane, k * 768 + s, l, true);
                c.gain = gain;
                p.arrangement.clips.push(c);
            }
        }
    };
    // inserts: 0 drums, 1 808 + bass, 2 lead, 3 pad + arp
    let snare = midi(&mut p, "Snare", Track::Sampler, Some(sref("Snares", "808 Mafia - Snare 1.wav")), U34_SNARE, 0, false);
    let k808 = midi(&mut p, "808", Track::Sampler, Some(sref("808s", "DJ SPINZ 808 (THIS IS THE BEST ONE).wav")), U34_808, 1, false);
    let pad_orig = midi(&mut p, "Pad", Track::Synth, None, U34_PAD, 3, true);
    audio(&mut p, "Open hat", 1, U34_HAT_AUDIO, mix.open_hat, 0);
    audio(&mut p, "Kick", 0, U34_KICK_AUDIO, mix.kick, 0);
    let lead = midi(&mut p, "Lead", Track::Wavetable, None, &u34_lead(), 2, false);
    let arp_orig = midi(&mut p, "Arp", Track::Wavetable, None, &u34_arp(), 3, true);
    let bass = midi(&mut p, "Bass", Track::Wavetable, None, U34_BASS, 1, false);
    let hats = midi(&mut p, "Hats", Track::Sampler, Some(sref("Hi Hats", "Southside Hi Hat.wav")), &u34_hats(), 0, false);
    // clones of the two tracks that were switched off: play them or change them freely (muted in the Bounce version)
    let pad_clone = midi(&mut p, "Pad (clone)", Track::Synth, None, U34_PAD, 3, !full);
    let arp_clone = midi(&mut p, "Arp (clone)", Track::Wavetable, None, &u34_arp(), 3, !full);
    if full {
        p.arrangement.tracks[pad_orig].mute = false;
        p.arrangement.tracks[arp_orig].mute = false;
        // in the full version the clones are the spare ones
        p.arrangement.tracks[pad_clone].mute = true;
        p.arrangement.tracks[arp_clone].mute = true;
    }
    // the mix: levels per source
    p.arrangement.tracks[snare].sampler.volume = 0.9 * mix.snare;
    p.arrangement.tracks[k808].sampler.volume = 0.9 * mix.k808;
    p.arrangement.tracks[hats].sampler.volume = 0.9 * mix.hats;
    p.adopt_track_params();
    p.arrangement.tracks[arp_orig].snd.wt.volume = 0.7 * mix.arp;
    p.arrangement.tracks[arp_clone].snd.wt.volume = 0.7 * mix.arp;
    p.arrangement.tracks[lead].snd.wt.volume = 0.7 * mix.lead;
    // the bass: the same wavetable synth, rounded off so that it only supports the 808
    {
        let w = &mut p.arrangement.tracks[bass].snd.wt;
        w.volume = 0.7 * mix.bass;
        w.cutoff = 420.0;
        w.res = 0.1;
        w.unison = 1;
    }
    for l in [pad_orig, pad_clone] {
        p.arrangement.tracks[l].snd.synth.volume *= mix.pad;
    }
    // insert effects
    p.insert_fx.resize_with(NUM_INSERTS, Vec::new);
    let fx = |kind: FxKind, set: &[(usize, f32)]| {
        let mut s = FxSlot::new(kind);
        for &(i, v) in set {
            s.p[i] = v;
        }
        s
    };
    let names = ["Drums", "808 and Bass", "Lead", "Pad and Arp"];
    for (i, n) in names.iter().enumerate() {
        p.channels[NUM_CH + i].name = n.to_string();
    }
    p.insert_fx[0] = vec![fx(FxKind::Compressor, &[(0, -14.0), (1, 3.0), (2, 2.0)]), fx(FxKind::Eq3, &[(0, 1.0), (2, 1.5)])];
    p.insert_fx[1] = vec![fx(FxKind::Distortion, &[(0, 0.18), (2, 0.45)]), fx(FxKind::Compressor, &[(0, -16.0), (1, 3.0), (2, 2.0)])];
    p.insert_fx[2] = vec![fx(FxKind::Delay, &[(2, 0.18)]), fx(FxKind::Reverb, &[(0, 0.5), (2, 0.16)])];
    p.insert_fx[3] = vec![fx(FxKind::Chorus, &[(2, 0.25)]), fx(FxKind::Reverb, &[(0, 0.75), (2, 0.28)])];
    p.delay_beats = 0.75;
    p.delay_feedback = 0.4;
    p.delay_return = 0.3;
    p.master_fx = MasterFx { comp_on: true, threshold_db: -10.0, ratio: 2.5, makeup_db: 2.0, low_db: 1.2, high_db: 0.5 };
    p
}

pub fn untitle34_bounce() -> Project {
    untitle34(false)
}

pub fn untitle34_full() -> Project {
    untitle34(true)
}
