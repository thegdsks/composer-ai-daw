//! Scale tables and key membership (D23). Pure data and arithmetic: no model types, no I/O.
//! Pitches are MIDI numbers; a pitch class is `pitch % 12` with 0 = C.

use serde::{Deserialize, Serialize};

/// Scales a project key can use. JSON names are snake_case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scale {
    Major,
    Minor,
    Dorian,
    Mixolydian,
    PentatonicMajor,
    PentatonicMinor,
    HarmonicMinor,
}

impl Scale {
    pub const ALL: [Scale; 7] = [
        Scale::Major,
        Scale::Minor,
        Scale::Dorian,
        Scale::Mixolydian,
        Scale::PentatonicMajor,
        Scale::PentatonicMinor,
        Scale::HarmonicMinor,
    ];

    /// Semitone offsets above the root, ascending, starting with 0.
    pub fn intervals(self) -> &'static [u8] {
        match self {
            Scale::Major => &[0, 2, 4, 5, 7, 9, 11],
            Scale::Minor => &[0, 2, 3, 5, 7, 8, 10],
            Scale::Dorian => &[0, 2, 3, 5, 7, 9, 10],
            Scale::Mixolydian => &[0, 2, 4, 5, 7, 9, 10],
            Scale::PentatonicMajor => &[0, 2, 4, 7, 9],
            Scale::PentatonicMinor => &[0, 3, 5, 7, 10],
            Scale::HarmonicMinor => &[0, 2, 3, 5, 7, 8, 11],
        }
    }

    /// Stable lower case name, the same text as the JSON form.
    pub fn name(self) -> &'static str {
        match self {
            Scale::Major => "major",
            Scale::Minor => "minor",
            Scale::Dorian => "dorian",
            Scale::Mixolydian => "mixolydian",
            Scale::PentatonicMajor => "pentatonic_major",
            Scale::PentatonicMinor => "pentatonic_minor",
            Scale::HarmonicMinor => "harmonic_minor",
        }
    }

    /// Position in [`Scale::ALL`], stable for hashing.
    pub fn index(self) -> u8 {
        Scale::ALL.iter().position(|s| *s == self).unwrap_or(0) as u8
    }
}

/// A root pitch class (0 = C .. 11 = B) with a scale.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Key {
    pub root: u8,
    pub scale: Scale,
}

impl Key {
    pub fn is_valid(&self) -> bool {
        self.root < 12
    }

    /// Pitch classes of the scale in degree order, starting at the root. Empty for an invalid root.
    pub fn pitch_classes(&self) -> Vec<u8> {
        if !self.is_valid() {
            return Vec::new();
        }
        self.scale
            .intervals()
            .iter()
            .map(|i| (self.root + i) % 12)
            .collect()
    }

    /// Bit n set when pitch class n is in the key.
    pub fn mask(&self) -> u16 {
        self.pitch_classes().iter().fold(0, |m, pc| m | (1 << pc))
    }

    /// True when the MIDI pitch belongs to the key. Always false for an invalid root.
    pub fn contains(&self, pitch: u8) -> bool {
        self.mask() & (1 << (pitch % 12)) != 0
    }
}

/// Stacked-thirds chord kinds found in the diatonic scales.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Quality {
    Major,
    Minor,
    Diminished,
    Augmented,
}

impl Quality {
    fn from_triad(third: u8, fifth: u8) -> Quality {
        match (third, fifth) {
            (4, 7) => Quality::Major,
            (3, 7) => Quality::Minor,
            (3, 6) => Quality::Diminished,
            _ => Quality::Augmented,
        }
    }

    fn is_upper(self) -> bool {
        matches!(self, Quality::Major | Quality::Augmented)
    }
}

/// One diatonic chord of a key with a plain name and default voicing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Chord {
    /// 1 to 7, the scale degree the chord is built on.
    pub degree: u8,
    /// Plain name such as "Dm", "G7" or "Bdim".
    pub name: String,
    /// Roman numeral shown as a disclosure, such as "ii" or "V7".
    pub roman: String,
    pub quality: Quality,
    /// Chord tones as pitch classes, root first.
    pub pitch_classes: Vec<u8>,
    /// MIDI pitches of the root position voicing in the requested octave.
    pub pitches: Vec<u8>,
    /// Semitone offsets above the root, ascending.
    #[serde(skip)]
    pub intervals: Vec<u8>,
    #[serde(skip)]
    pub root: u8,
}

const SHARP_NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];
const FLAT_NAMES: [&str; 12] = [
    "C", "Db", "D", "Eb", "E", "F", "Gb", "G", "Ab", "A", "Bb", "B",
];
const ROMAN: [&str; 7] = ["I", "II", "III", "IV", "V", "VI", "VII"];

/// The seven note scale whose chords a key uses. Pentatonics borrow their parent scale.
fn parent(scale: Scale) -> Scale {
    match scale {
        Scale::PentatonicMajor => Scale::Major,
        Scale::PentatonicMinor => Scale::Minor,
        s => s,
    }
}

/// True when the key reads better with flats (F, Bb, Eb, Ab, Db majors and relatives).
fn prefers_flats(key: &Key) -> bool {
    let major_root = match parent(key.scale) {
        Scale::Major => key.root,
        Scale::Mixolydian => (key.root + 5) % 12,
        Scale::Dorian => (key.root + 10) % 12,
        _ => (key.root + 3) % 12,
    };
    matches!(major_root, 1 | 3 | 5 | 8 | 10)
}

/// Plain note name of a pitch class, spelled with flats when `flats` is set.
pub fn note_name(pc: u8, flats: bool) -> &'static str {
    let table = if flats { &FLAT_NAMES } else { &SHARP_NAMES };
    table[usize::from(pc % 12)]
}

/// Name of the key's root spelled for the key, such as "Bb" for B flat major.
pub fn key_root_name(key: &Key) -> &'static str {
    note_name(key.root % 12, prefers_flats(key))
}

fn suffix(quality: Quality, seventh: Option<u8>) -> &'static str {
    match (quality, seventh) {
        (Quality::Major, None) => "",
        (Quality::Minor, None) => "m",
        (Quality::Diminished, None) => "dim",
        (Quality::Augmented, None) => "aug",
        (Quality::Major, Some(11)) => "maj7",
        (Quality::Major, _) => "7",
        (Quality::Minor, Some(11)) => "m(maj7)",
        (Quality::Minor, _) => "m7",
        (Quality::Diminished, Some(9)) => "dim7",
        (Quality::Diminished, _) => "m7b5",
        (Quality::Augmented, _) => "maj7#5",
    }
}

fn roman(degree: usize, quality: Quality, seventh: Option<u8>) -> String {
    let base = ROMAN[degree];
    let mut out = if quality.is_upper() {
        base.to_string()
    } else {
        base.to_lowercase()
    };
    match (quality, seventh) {
        (Quality::Diminished, None) => out.push_str("dim"),
        (Quality::Augmented, None) => out.push_str("aug"),
        (_, None) => {}
        (Quality::Major, Some(11)) => out.push_str("maj7"),
        (Quality::Diminished, Some(9)) => out.push_str("dim7"),
        (Quality::Diminished, Some(_)) => out.push_str("7b5"),
        (_, Some(11)) => out.push_str("(maj7)"),
        (_, Some(_)) => out.push('7'),
    }
    out
}

/// MIDI pitches of chord tones above `root_pc` with the root in `octave` (4 holds middle C)
/// and the lowest `inversion` tones lifted an octave. Empty when any tone leaves 0..=127.
pub fn voice(root_pc: u8, intervals: &[u8], octave: i8, inversion: u8) -> Vec<u8> {
    let base = (i32::from(octave) + 1) * 12 + i32::from(root_pc % 12);
    let mut tones: Vec<i32> = intervals.iter().map(|i| base + i32::from(*i)).collect();
    for _ in 0..usize::from(inversion).min(tones.len().saturating_sub(1)) {
        let low = tones.remove(0);
        tones.push(low + 12);
    }
    if tones.iter().any(|t| !(0..=127).contains(t)) {
        return Vec::new();
    }
    tones.into_iter().map(|t| t as u8).collect()
}

/// Diatonic triads (`sevenths` false) or seventh chords of the key, one per scale degree.
/// Pentatonic keys use the chords of their parent major or minor scale. Empty for an
/// invalid root or an octave outside -1..=8.
pub fn chords(key: &Key, sevenths: bool, octave: i8) -> Vec<Chord> {
    if !key.is_valid() || !(-1..=8).contains(&octave) {
        return Vec::new();
    }
    let iv = parent(key.scale).intervals();
    let flats = prefers_flats(key);
    let n = iv.len();
    (0..n)
        .map(|d| {
            let at = |k: usize| {
                let step = d + k;
                iv[step % n] + (step / n) as u8 * 12 - iv[d]
            };
            let (third, fifth) = (at(2), at(4));
            let quality = Quality::from_triad(third, fifth);
            let seventh = sevenths.then(|| at(6));
            let mut intervals = vec![0, third, fifth];
            intervals.extend(seventh);
            let root = (key.root + iv[d]) % 12;
            Chord {
                degree: d as u8 + 1,
                name: format!("{}{}", note_name(root, flats), suffix(quality, seventh)),
                roman: roman(d, quality, seventh),
                quality,
                pitch_classes: intervals.iter().map(|i| (root + i) % 12).collect(),
                pitches: voice(root, &intervals, octave, 0),
                intervals,
                root,
            }
        })
        .collect()
}

impl Chord {
    /// MIDI pitches in `octave` with the lowest `inversion` tones moved up an octave.
    pub fn voicing(&self, octave: i8, inversion: u8) -> Vec<u8> {
        voice(self.root, &self.intervals, octave, inversion)
    }
}

/// 1 based scale degree of the pitch class of `pitch`, or None when it is outside the key.
pub fn degree_of(key: &Key, pitch: u8) -> Option<u8> {
    key.pitch_classes()
        .iter()
        .position(|pc| *pc == pitch % 12)
        .map(|i| i as u8 + 1)
}

/// MIDI pitch of scale degree `degree` (1 is the root just above middle C; 0 and negatives
/// go below, past the last degree wraps up an octave). None when out of range.
pub fn pitch_of_degree(key: &Key, degree: i32) -> Option<u8> {
    if !key.is_valid() {
        return None;
    }
    let intervals = key.scale.intervals();
    let n = intervals.len() as i32;
    let step = degree - 1;
    let idx = step.rem_euclid(n) as usize;
    let pitch = 60 + i32::from(key.root) + i32::from(intervals[idx]) + step.div_euclid(n) * 12;
    u8::try_from(pitch).ok().filter(|p| *p <= 127)
}

/// Nearest pitch that belongs to the key, the lower one on a tie; the pitch itself when
/// already in key. None for an invalid key or a pitch above 127.
pub fn nearest_in_key(key: &Key, pitch: u8) -> Option<u8> {
    if !key.is_valid() || pitch > 127 {
        return None;
    }
    (0..=12u8).find_map(|d| {
        let down = pitch.checked_sub(d).filter(|p| key.contains(*p));
        let up = pitch
            .checked_add(d)
            .filter(|p| *p <= 127 && key.contains(*p));
        down.or(up)
    })
}

/// Result of `theory.scale`: the scale pitch classes of a key in degree order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ScaleInfo {
    pub root: u8,
    pub scale: &'static str,
    pub pitch_classes: Vec<u8>,
    pub mask: u16,
}

/// Result of `theory.chords`: triads and sevenths of a key at an octave.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChordsInfo {
    pub root: u8,
    pub scale: &'static str,
    pub root_name: &'static str,
    pub octave: i8,
    pub triads: Vec<Chord>,
    pub sevenths: Vec<Chord>,
}

/// Result of `theory.pitch`: key membership, 1 based degree and nearest in key pitch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PitchInfo {
    pub pitch: u8,
    pub in_key: bool,
    pub degree: Option<u8>,
    pub nearest: Option<u8>,
}

const BAD_ROOT: &str = "root must be 0 (C) to 11 (B)";

/// Scale table for a key, or the message for an invalid root.
pub fn scale_info(key: &Key) -> Result<ScaleInfo, &'static str> {
    if !key.is_valid() {
        return Err(BAD_ROOT);
    }
    Ok(ScaleInfo {
        root: key.root,
        scale: key.scale.name(),
        pitch_classes: key.pitch_classes(),
        mask: key.mask(),
    })
}

/// Triads and sevenths of a key; octave must be -1..=8.
pub fn chords_info(key: &Key, octave: i8) -> Result<ChordsInfo, &'static str> {
    if !key.is_valid() {
        return Err(BAD_ROOT);
    }
    if !(-1..=8).contains(&octave) {
        return Err("octave must be -1 to 8");
    }
    Ok(ChordsInfo {
        root: key.root,
        scale: key.scale.name(),
        root_name: key_root_name(key),
        octave,
        triads: chords(key, false, octave),
        sevenths: chords(key, true, octave),
    })
}

/// Membership facts for one pitch; pitch must be 0..=127.
pub fn pitch_info(key: &Key, pitch: u8) -> Result<PitchInfo, &'static str> {
    if !key.is_valid() {
        return Err(BAD_ROOT);
    }
    if pitch > 127 {
        return Err("pitch must be 0 to 127");
    }
    Ok(PitchInfo {
        pitch,
        in_key: key.contains(pitch),
        degree: degree_of(key, pitch),
        nearest: nearest_in_key(key, pitch),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn k(root: u8, scale: Scale) -> Key {
        Key { root, scale }
    }

    #[test]
    fn tables_are_well_formed() {
        for s in Scale::ALL {
            let iv = s.intervals();
            assert_eq!(iv[0], 0);
            assert!(iv.windows(2).all(|w| w[0] < w[1]));
            assert!(iv.iter().all(|i| *i < 12));
            assert_eq!(Scale::ALL[usize::from(s.index())], s);
            let json = serde_json::to_string(&s).unwrap();
            assert_eq!(json, format!("\"{}\"", s.name()));
        }
    }

    #[test]
    fn c_major_is_the_white_keys() {
        assert_eq!(k(0, Scale::Major).pitch_classes(), [0, 2, 4, 5, 7, 9, 11]);
        assert!(k(0, Scale::Major).contains(60));
        assert!(!k(0, Scale::Major).contains(61));
        assert!(k(0, Scale::Major).contains(71));
    }

    #[test]
    fn a_minor_shares_c_major_notes() {
        assert_eq!(k(9, Scale::Minor).mask(), k(0, Scale::Major).mask());
        assert_eq!(k(9, Scale::Minor).pitch_classes()[0], 9);
    }

    #[test]
    fn modes_and_pentatonics() {
        // D dorian and G mixolydian are also the C major notes.
        assert_eq!(k(2, Scale::Dorian).mask(), k(0, Scale::Major).mask());
        assert_eq!(k(7, Scale::Mixolydian).mask(), k(0, Scale::Major).mask());
        assert_eq!(
            k(0, Scale::PentatonicMajor).pitch_classes(),
            [0, 2, 4, 7, 9]
        );
        assert_eq!(
            k(9, Scale::PentatonicMinor).mask(),
            k(0, Scale::PentatonicMajor).mask()
        );
        // A harmonic minor raises the seventh to G sharp.
        assert!(k(9, Scale::HarmonicMinor).contains(68));
        assert!(!k(9, Scale::HarmonicMinor).contains(67));
    }

    #[test]
    fn wraps_past_b_and_rejects_bad_root() {
        assert_eq!(k(11, Scale::Major).pitch_classes(), [11, 1, 3, 4, 6, 8, 10]);
        assert!(!k(12, Scale::Major).is_valid());
        assert!(k(12, Scale::Major).pitch_classes().is_empty());
        assert!(!k(12, Scale::Major).contains(0));
    }

    fn names(key: Key, sevenths: bool) -> Vec<String> {
        chords(&key, sevenths, 4)
            .into_iter()
            .map(|c| c.name)
            .collect()
    }

    #[test]
    fn c_major_chord_table_is_golden() {
        let key = k(0, Scale::Major);
        assert_eq!(names(key, false), ["C", "Dm", "Em", "F", "G", "Am", "Bdim"]);
        assert_eq!(
            names(key, true),
            ["Cmaj7", "Dm7", "Em7", "Fmaj7", "G7", "Am7", "Bm7b5"]
        );
        let romans: Vec<_> = chords(&key, false, 4)
            .into_iter()
            .map(|c| c.roman)
            .collect();
        assert_eq!(romans, ["I", "ii", "iii", "IV", "V", "vi", "viidim"]);
        let romans: Vec<_> = chords(&key, true, 4).into_iter().map(|c| c.roman).collect();
        assert_eq!(
            romans,
            ["Imaj7", "ii7", "iii7", "IVmaj7", "V7", "vi7", "vii7b5"]
        );
        let g7 = &chords(&key, true, 4)[4];
        assert_eq!(g7.pitches, [67, 71, 74, 77]);
        assert_eq!(g7.pitch_classes, [7, 11, 2, 5]);
        assert_eq!(g7.degree, 5);
        assert_eq!(chords(&key, false, 4)[6].pitches, [71, 74, 77]);
    }

    #[test]
    fn other_keys_are_spelled_for_the_key() {
        assert_eq!(
            names(k(9, Scale::Minor), false),
            ["Am", "Bdim", "C", "Dm", "Em", "F", "G"]
        );
        assert_eq!(
            names(k(5, Scale::Major), false),
            ["F", "Gm", "Am", "Bb", "C", "Dm", "Edim"]
        );
        assert_eq!(
            names(k(7, Scale::Major), false),
            ["G", "Am", "Bm", "C", "D", "Em", "F#dim"]
        );
        assert_eq!(
            names(k(9, Scale::HarmonicMinor), false),
            ["Am", "Bdim", "Caug", "Dm", "E", "F", "G#dim"]
        );
        assert_eq!(
            names(k(2, Scale::Dorian), false),
            ["Dm", "Em", "F", "G", "Am", "Bdim", "C"]
        );
        assert_eq!(names(k(10, Scale::Mixolydian), false)[0], "Bb");
        assert_eq!(key_root_name(&k(10, Scale::Major)), "Bb");
    }

    #[test]
    fn pentatonic_borrows_parent_and_bad_input_is_empty() {
        assert_eq!(
            chords(&k(0, Scale::PentatonicMajor), false, 4),
            chords(&k(0, Scale::Major), false, 4)
        );
        assert!(chords(&k(12, Scale::Major), false, 4).is_empty());
        assert!(chords(&k(0, Scale::Major), false, 9).is_empty());
        let hi = chords(&k(11, Scale::Major), true, 8);
        assert!(hi.iter().any(|c| c.pitches.is_empty()));
    }

    #[test]
    fn inversions_lift_the_lowest_tones() {
        let c = &chords(&k(0, Scale::Major), false, 4)[0];
        assert_eq!(c.voicing(4, 0), [60, 64, 67]);
        assert_eq!(c.voicing(4, 1), [64, 67, 72]);
        assert_eq!(c.voicing(4, 2), [67, 72, 76]);
        assert_eq!(c.voicing(3, 0), [48, 52, 55]);
    }

    #[test]
    fn degree_helpers() {
        let key = k(0, Scale::Major);
        assert_eq!(degree_of(&key, 62), Some(2));
        assert_eq!(degree_of(&key, 61), None);
        assert_eq!(pitch_of_degree(&key, 1), Some(60));
        assert_eq!(pitch_of_degree(&key, 8), Some(72));
        assert_eq!(pitch_of_degree(&key, 0), Some(59));
        assert_eq!(pitch_of_degree(&k(9, Scale::Minor), 3), Some(72));
        assert_eq!(pitch_of_degree(&k(12, Scale::Minor), 1), None);
        assert_eq!(pitch_of_degree(&key, 200), None);
    }

    #[test]
    fn nearest_in_key_prefers_lower_on_ties() {
        let key = k(0, Scale::Major);
        assert_eq!(nearest_in_key(&key, 60), Some(60));
        assert_eq!(nearest_in_key(&key, 61), Some(60));
        assert_eq!(nearest_in_key(&key, 63), Some(62));
        assert_eq!(nearest_in_key(&key, 66), Some(65));
        assert_eq!(nearest_in_key(&key, 70), Some(69));
        assert_eq!(nearest_in_key(&key, 0), Some(0));
        assert_eq!(nearest_in_key(&k(1, Scale::PentatonicMajor), 0), Some(1));
        assert_eq!(nearest_in_key(&k(12, Scale::Major), 60), None);
        assert_eq!(nearest_in_key(&key, 128), None);
    }
}
