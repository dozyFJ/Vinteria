//! Music theory helpers: scales, circle of fifths, diatonic chords and chord naming.

pub const SCALES: [(&str, &[u8]); 13] = [
    ("Major", &[0, 2, 4, 5, 7, 9, 11]),
    ("Minor", &[0, 2, 3, 5, 7, 8, 10]),
    ("Dorian", &[0, 2, 3, 5, 7, 9, 10]),
    ("Phrygian", &[0, 1, 3, 5, 7, 8, 10]),
    ("Lydian", &[0, 2, 4, 6, 7, 9, 11]),
    ("Mixolydian", &[0, 2, 4, 5, 7, 9, 10]),
    ("Locrian", &[0, 1, 3, 5, 6, 8, 10]),
    ("Harmonic minor", &[0, 2, 3, 5, 7, 8, 11]),
    ("Melodic minor", &[0, 2, 3, 5, 7, 9, 11]),
    ("Major pentatonic", &[0, 2, 4, 7, 9]),
    ("Minor pentatonic", &[0, 3, 5, 7, 10]),
    ("Blues", &[0, 3, 5, 6, 7, 10]),
    ("Chromatic", &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]),
];

pub const CHROMATIC: usize = 12;

const SHARPS: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
const FLATS: [&str; 12] = ["C", "Db", "D", "Eb", "E", "F", "Gb", "G", "Ab", "A", "Bb", "B"];

/// Outer ring of the circle of fifths (major keys, clockwise from the top) and the inner ring (relative minors).
pub const CIRCLE_MAJOR: [&str; 12] = ["C", "G", "D", "A", "E", "B", "F#", "Db", "Ab", "Eb", "Bb", "F"];
pub const CIRCLE_MINOR: [&str; 12] = ["Am", "Em", "Bm", "F#m", "C#m", "G#m", "D#m", "Bbm", "Fm", "Cm", "Gm", "Dm"];

/// Pitch class of the key at circle position `pos` (0 = top, one fifth per step clockwise).
pub fn circle_root(pos: usize) -> u8 {
    ((pos * 7) % 12) as u8
}

/// Circle position of a pitch class.
pub fn circle_pos(root: u8) -> usize {
    ((root as usize % 12) * 7) % 12
}

/// True for scales whose third is minor (so the "home" ring on the circle is the inner one).
pub fn is_minor_like(scale: usize) -> bool {
    let s = SCALES[scale.min(CHROMATIC)].1;
    s.contains(&3) && !s.contains(&4)
}

fn uses_flats(root: u8, scale: usize) -> bool {
    let major_root = if is_minor_like(scale) { (root + 3) % 12 } else { root % 12 };
    matches!(major_root, 5 | 10 | 3 | 8 | 1)
}

/// Pitch-class name spelled for the key (flats in flat keys).
pub fn pc_name(root: u8, scale: usize, pc: u8) -> &'static str {
    if uses_flats(root, scale) { FLATS[(pc % 12) as usize] } else { SHARPS[(pc % 12) as usize] }
}

pub fn in_scale(root: u8, scale: usize, pitch: u8) -> bool {
    let rel = (pitch % 12 + 12 - root % 12) % 12;
    SCALES[scale.min(CHROMATIC)].1.contains(&rel)
}

pub fn is_root(root: u8, pitch: u8) -> bool {
    pitch % 12 == root % 12
}

/// Nearest scale tone (ties go down).
pub fn snap_to_scale(root: u8, scale: usize, pitch: u8) -> u8 {
    for d in 0..12i32 {
        for p in [pitch as i32 - d, pitch as i32 + d] {
            if (0..=127).contains(&p) && in_scale(root, scale, p as u8) {
                return p as u8;
            }
        }
    }
    pitch
}

/// Next scale tone above (`dir > 0`) or below the pitch.
pub fn step_in_scale(root: u8, scale: usize, pitch: u8, dir: i32) -> u8 {
    let mut p = pitch as i32;
    loop {
        p += dir.signum();
        if !(0..=127).contains(&p) {
            return pitch;
        }
        if in_scale(root, scale, p as u8) {
            return p as u8;
        }
    }
}

/// Stack thirds on a scale tone: `count` = 3 gives a triad, 4 a seventh chord, ...
pub fn stack_chord(root: u8, scale: usize, tone: u8, count: usize) -> Vec<u8> {
    let steps = SCALES[scale.min(CHROMATIC)].1;
    let n = steps.len();
    let rel = (tone % 12 + 12 - root % 12) % 12;
    let Some(d) = steps.iter().position(|s| *s == rel) else { return vec![tone] };
    let base = tone as i32 - steps[d] as i32;
    (0..count)
        .map(|j| {
            let idx = d + 2 * j;
            (base + (idx / n) as i32 * 12 + steps[idx % n] as i32).clamp(0, 127) as u8
        })
        .collect()
}

/// The diatonic chord on scale degree `degree` (0-based) with its tonic around middle C.
pub fn diatonic_chord(root: u8, scale: usize, degree: usize, sevenths: bool) -> Vec<u8> {
    let steps = SCALES[scale.min(CHROMATIC)].1;
    let tone = 60 + root % 12 + steps[degree % steps.len()];
    stack_chord(root, scale, tone, if sevenths { 4 } else { 3 })
}

const CHORD_TYPES: [(&str, &[u8]); 16] = [
    ("", &[0, 4, 7]),
    ("m", &[0, 3, 7]),
    ("dim", &[0, 3, 6]),
    ("aug", &[0, 4, 8]),
    ("sus2", &[0, 2, 7]),
    ("sus4", &[0, 5, 7]),
    ("5", &[0, 7]),
    ("maj7", &[0, 4, 7, 11]),
    ("m7", &[0, 3, 7, 10]),
    ("7", &[0, 4, 7, 10]),
    ("m7b5", &[0, 3, 6, 10]),
    ("dim7", &[0, 3, 6, 9]),
    ("mMaj7", &[0, 3, 7, 11]),
    ("6", &[0, 4, 7, 9]),
    ("m6", &[0, 3, 7, 9]),
    ("add9", &[0, 2, 4, 7]),
];

/// Name a set of pitches ("Cmaj7", "Am", "C/E"); None when it is not a known chord.
pub fn chord_name(pitches: &[u8], root: u8, scale: usize) -> Option<String> {
    if pitches.len() < 2 {
        return None;
    }
    let bass = *pitches.iter().min()? % 12;
    let mut pcs: Vec<u8> = pitches.iter().map(|p| p % 12).collect();
    pcs.sort_unstable();
    pcs.dedup();
    let mut candidates = vec![bass];
    candidates.extend(pcs.iter().copied().filter(|p| *p != bass));
    for r in candidates {
        let mut iv: Vec<u8> = pcs.iter().map(|p| (p + 12 - r) % 12).collect();
        iv.sort_unstable();
        if let Some((suffix, _)) = CHORD_TYPES.iter().find(|(_, t)| **t == iv[..]) {
            let slash = if r != bass { format!("/{}", pc_name(root, scale, bass)) } else { String::new() };
            return Some(format!("{}{}{}", pc_name(root, scale, r), suffix, slash));
        }
    }
    None
}

/// Roman-numeral label for a diatonic chord ("I", "ii", "vii°").
pub fn roman(root: u8, scale: usize, degree: usize) -> String {
    const NUMS: [&str; 7] = ["I", "II", "III", "IV", "V", "VI", "VII"];
    let chord = diatonic_chord(root, scale, degree, false);
    let (third, fifth) = if chord.len() >= 3 { (chord[1] - chord[0], chord[2] - chord[0]) } else { (4, 7) };
    let n = NUMS[degree.min(6)];
    match (third, fifth) {
        (3, 6) => format!("{}°", n.to_lowercase()),
        (3, _) => n.to_lowercase(),
        (4, 8) => format!("{n}+"),
        _ => n.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn circle_is_fifths() {
        assert_eq!(circle_root(0), 0);
        assert_eq!(circle_root(1), 7); // G
        assert_eq!(circle_root(11), 5); // F
        for r in 0..12u8 {
            assert_eq!(circle_root(circle_pos(r)), r);
        }
    }

    #[test]
    fn scale_membership_and_snap() {
        assert!(in_scale(0, 0, 64)); // E in C major
        assert!(!in_scale(0, 0, 61)); // C# is not
        assert_eq!(snap_to_scale(0, 0, 61), 60);
        assert_eq!(step_in_scale(0, 0, 64, 1), 65);
        assert_eq!(step_in_scale(0, 0, 60, -1), 59);
    }

    #[test]
    fn diatonic_chords() {
        assert_eq!(diatonic_chord(0, 0, 0, false), vec![60, 64, 67]); // C
        assert_eq!(diatonic_chord(0, 0, 1, false), vec![62, 65, 69]); // Dm
        assert_eq!(diatonic_chord(0, 0, 6, false), vec![71, 74, 77]); // Bdim
        assert_eq!(diatonic_chord(9, 1, 0, true), vec![69, 72, 76, 79]); // Am7
        assert_eq!(roman(0, 0, 1), "ii");
        assert_eq!(roman(0, 0, 6), "vii°");
        assert_eq!(roman(0, 0, 4), "V");
    }

    #[test]
    fn chord_names() {
        assert_eq!(chord_name(&[60, 64, 67], 0, 0).unwrap(), "C");
        assert_eq!(chord_name(&[57, 60, 64, 67], 0, 0).unwrap(), "Am7");
        assert_eq!(chord_name(&[64, 67, 72], 0, 0).unwrap(), "C/E");
        assert_eq!(chord_name(&[58, 62, 65], 5, 0).unwrap(), "Bb");
        assert!(chord_name(&[60, 61, 62], 0, 0).is_none());
    }
}

/// Chord progressions by genre: `(genre, name, [(scale degree 0-based, notes in the chord)])`, one chord per bar.
/// The degrees follow the key of the project, so the same list works in any key.
pub const PROGRESSIONS: &[(&str, &str, &[(usize, usize)])] = &[
    ("Hardbass", "i - VI - III - VII", &[(0, 3), (5, 3), (2, 3), (6, 3)]),
    ("Hardbass", "i - iv - VII - III", &[(0, 3), (3, 3), (6, 3), (2, 3)]),
    ("Hardbass", "i - VII - VI - VII", &[(0, 3), (6, 3), (5, 3), (6, 3)]),
    ("Hardbass", "i - v - VI - VII", &[(0, 3), (4, 3), (5, 3), (6, 3)]),
    ("Hardbass Donk", "i - i - VI - VII", &[(0, 3), (0, 3), (5, 3), (6, 3)]),
    ("Hardbass Donk", "i - III - VII - VI", &[(0, 3), (2, 3), (6, 3), (5, 3)]),
    ("Hardbass Donk", "i - iv - i - v", &[(0, 3), (3, 3), (0, 3), (4, 3)]),
    ("Funk", "i7 - iv7 - VII7 - III7", &[(0, 4), (3, 4), (6, 4), (2, 4)]),
    ("Funk", "i7 - VI7 - iv7 - v7", &[(0, 4), (5, 4), (3, 4), (4, 4)]),
    ("Funk", "ii7 - v7 - i7 - i7", &[(1, 4), (4, 4), (0, 4), (0, 4)]),
    ("Drift", "i - VII - VI - v", &[(0, 3), (6, 3), (5, 3), (4, 3)]),
    ("Drift", "i - III - VI - VII", &[(0, 3), (2, 3), (5, 3), (6, 3)]),
    ("Drift", "i - iv - VI - v", &[(0, 3), (3, 3), (5, 3), (4, 3)]),
    ("House", "VI - VII - i - i (piano house)", &[(5, 3), (6, 3), (0, 3), (0, 3)]),
    ("House", "i7 - VII7 - VI7 - VII7 (deep)", &[(0, 4), (6, 4), (5, 4), (6, 4)]),
    ("House", "ii7 - v7 - i7 - IV7 (jazzy)", &[(1, 4), (4, 4), (0, 4), (3, 4)]),
];

/// The notes of the chord on scale degree `degree` (counted from the root, going on into the next octave) with `count` notes.
pub fn degree_chord(root: u8, scale: usize, degree: usize, count: usize) -> Vec<u8> {
    // the chromatic scale has no degrees: use the natural minor
    let sc = if scale >= CHROMATIC { 1 } else { scale };
    let steps = SCALES[sc].1;
    let n = steps.len();
    let base = 48 + (root % 12) as i32;
    let tone = (base + steps[degree % n] as i32 + 12 * (degree / n) as i32).clamp(0, 127) as u8;
    stack_chord(root, sc, tone, count)
}

#[cfg(test)]
mod progression_tests {
    use super::*;

    #[test]
    fn progressions_stay_in_the_key() {
        // A minor: i - VI - III - VII is Am, F, C, G
        let am = degree_chord(9, 1, 0, 3);
        let f = degree_chord(9, 1, 5, 3);
        assert_eq!(am.iter().map(|p| p % 12).collect::<Vec<_>>(), vec![9, 0, 4]);
        assert_eq!(f.iter().map(|p| p % 12).collect::<Vec<_>>(), vec![5, 9, 0]);
        for (_, _, chords) in PROGRESSIONS {
            for &(d, n) in chords.iter() {
                let c = degree_chord(9, 1, d, n);
                assert_eq!(c.len(), n);
                assert!(c.iter().all(|p| in_scale(9, 1, *p)));
            }
        }
    }
}
