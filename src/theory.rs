// src/theory.rs

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Represents all the musical scales the application knows about.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Scale {
    Ionian,        // Major
    NaturalMinor,  // Aeolian
    HarmonicMinor,
    MelodicMinor,
    Dorian,
    Phrygian,
    Lydian,
    Mixolydian,
    Locrian,
    MajorPentatonic,
    MinorPentatonic,
    Blues,
    WholeTone,
    Chromatic,
}

impl Scale {
    /// An array of all available scales for easy iteration in the UI.
    pub const ALL: [Scale; 14] = [
        Scale::Ionian,
        Scale::Dorian,
        Scale::Phrygian,
        Scale::Lydian,
        Scale::Mixolydian,
        Scale::NaturalMinor,
        Scale::Locrian,
        Scale::HarmonicMinor,
        Scale::MelodicMinor,
        Scale::MajorPentatonic,
        Scale::MinorPentatonic,
        Scale::Blues,
        Scale::WholeTone,
        Scale::Chromatic,
    ];

    /// Returns the interval pattern (in semitones from the root) for the scale.
    pub fn get_intervals(&self) -> &'static [u8] {
        match self {
            Scale::Ionian => &[0, 2, 4, 5, 7, 9, 11],
            Scale::Dorian => &[0, 2, 3, 5, 7, 9, 10],
            Scale::Phrygian => &[0, 1, 3, 5, 7, 8, 10],
            Scale::Lydian => &[0, 2, 4, 6, 7, 9, 11],
            Scale::Mixolydian => &[0, 2, 4, 5, 7, 9, 10],
            Scale::NaturalMinor => &[0, 2, 3, 5, 7, 8, 10],
            Scale::Locrian => &[0, 1, 3, 5, 6, 8, 10],
            Scale::HarmonicMinor => &[0, 2, 3, 5, 7, 8, 11],
            Scale::MelodicMinor => &[0, 2, 3, 5, 7, 9, 11],
            Scale::MajorPentatonic => &[0, 2, 4, 7, 9],
            Scale::MinorPentatonic => &[0, 3, 5, 7, 10],
            Scale::Blues => &[0, 3, 5, 6, 7, 10],
            Scale::WholeTone => &[0, 2, 4, 6, 8, 10],
            Scale::Chromatic => &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
        }
    }
}

impl std::fmt::Display for Scale {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Scale::Ionian => "Major (Ionian)",
            Scale::NaturalMinor => "Minor (Aeolian)",
            Scale::HarmonicMinor => "Harmonic Minor",
            Scale::MelodicMinor => "Melodic Minor",
            Scale::Dorian => "Dorian",
            Scale::Phrygian => "Phrygian",
            Scale::Lydian => "Lydian",
            Scale::Mixolydian => "Mixolydian",
            Scale::Locrian => "Locrian",
            Scale::MajorPentatonic => "Major Pentatonic",
            Scale::MinorPentatonic => "Minor Pentatonic",
            Scale::Blues => "Blues",
            Scale::WholeTone => "Whole Tone",
            Scale::Chromatic => "Chromatic",
        };
        write!(f, "{}", name)
    }
}

/// Generates a vector of MIDI note numbers for a given scale and root note.
pub fn get_scale_notes(root_note: u8, scale: Scale) -> Vec<u8> {
    let intervals = scale.get_intervals();
    intervals
        .iter()
        .map(|&interval| root_note + interval)
        .collect()
}

/// Represents the quality of a chord (e.g., Major, Minor 7th).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ChordQuality {
    MajorTriad,
    MinorTriad,
    DiminishedTriad,
    AugmentedTriad,
    DominantSeventh,
    MajorSeventh,
    MinorSeventh,
}

impl ChordQuality {
    /// An array of all chord qualities we can currently recognize.
    const RECOGNIZABLE: [ChordQuality; 7] = [
        ChordQuality::MajorTriad,
        ChordQuality::MinorTriad,
        ChordQuality::DominantSeventh,
        ChordQuality::MajorSeventh,
        ChordQuality::MinorSeventh,
        ChordQuality::DiminishedTriad,
        ChordQuality::AugmentedTriad,
    ];

    /// Returns the interval pattern (in semitones from the root) for the chord quality.
    pub fn get_intervals(&self) -> &'static [u8] {
        match self {
            ChordQuality::MajorTriad => &[0, 4, 7],
            ChordQuality::MinorTriad => &[0, 3, 7],
            ChordQuality::DiminishedTriad => &[0, 3, 6],
            ChordQuality::AugmentedTriad => &[0, 4, 8],
            ChordQuality::DominantSeventh => &[0, 4, 7, 10],
            ChordQuality::MajorSeventh => &[0, 4, 7, 11],
            ChordQuality::MinorSeventh => &[0, 3, 7, 10],
        }
    }
}

/// Represents a recognized chord, with its root and quality.
#[derive(Debug, Clone)]
pub struct Chord {
    pub root: u8,
    pub quality: ChordQuality,
}

/// Attempts to recognize a chord from a set of played MIDI notes.
/// Handles inversions by checking all possible rotations of the notes.
pub fn recognize_chord(notes: &BTreeSet<u8>) -> Option<Chord> {
    if notes.len() < 3 {
        return None;
    }

    // Iterate through each note in the set as a potential root.
    for &potential_root in notes {
        let intervals: BTreeSet<u8> = notes
            .iter()
            .map(|&note| (note as i16 - potential_root as i16).rem_euclid(12) as u8)
            .collect();

        // The root note itself should have an interval of 0.
        if !intervals.contains(&0) {
            continue;
        }

        // Check if the derived intervals match any known chord quality.
        for quality in ChordQuality::RECOGNIZABLE {
            let quality_intervals: BTreeSet<u8> =
                quality.get_intervals().iter().copied().collect();
            if intervals == quality_intervals {
                return Some(Chord {
                    root: potential_root,
                    quality,
                });
            }
        }
    }

    None
}

/// A structure deserialized from JSON that defines the "flavor" of chord suggestions.
#[derive(Serialize, Deserialize, Default, Debug, Clone)]
#[serde(default)]
pub struct ChordStyle {
    pub name: String,
    // Maps a harmonic function (e.g., "dominant") to a specific chord quality.
    pub suggestions: BTreeMap<String, ChordQuality>,
}


/// Builds a vector of MIDI notes for a given root and chord quality, starting in a specific octave.
/// Includes logic to shift the chord by an octave if it would go off the ends of an 88-key piano.
pub fn build_chord_notes(root: u8, quality: ChordQuality, start_octave: u8) -> Vec<u8> {
    const FIRST_KEY: u8 = 21; // A0
    const LAST_KEY: u8 = 108;  // C8

    let intervals = quality.get_intervals();

    // Correctly calculate the target root note for the given octave index.
    // MIDI C0 is 12, C1 is 24, etc. Octave index 1 starts at MIDI note 24.
    // Our octave numbers are 1-8, so we use them directly.
    let target_root = (root % 12) + (start_octave * 12);

    let mut notes: Vec<u8> = intervals
        .iter()
        .map(|&interval| target_root + interval)
        .collect();

    // Check boundaries and shift the entire chord if necessary.
    if let Some(&max_note) = notes.iter().max() {
        if max_note > LAST_KEY {
            // Shift all notes down one octave.
            for note in &mut notes {
                *note = note.saturating_sub(12);
            }
        }
    }

    if let Some(&min_note) = notes.iter().min() {
        if min_note < FIRST_KEY {
            // Shift all notes up one octave.
            for note in &mut notes {
                *note = note.saturating_add(12);
            }
        }
    }

    notes
}

/// Gets four harmonically related chord suggestions based on the Circle of Fifths.
///
/// # Arguments
/// * `played_chord` - The chord the user is currently playing.
/// * `style` - The `ChordStyle` loaded from JSON, which dictates the quality of suggested chords.
///
/// # Returns
/// A vector containing four suggested chords, each represented by a tuple of its
/// `ChordQuality` and its root `u8` note. Returns an empty vector if no suggestions can be made.
pub fn get_chord_suggestions(
    played_chord: &Chord,
    style: &ChordStyle,
) -> Vec<(ChordQuality, u8)> {
    let root = played_chord.root;
    let mut suggestions = Vec::new();

    // 1. Dominant (V) - A fifth up
    let dominant_root = (root + 7) % 12;
    if let Some(&quality) = style.suggestions.get("dominant") {
        suggestions.push((quality, dominant_root));
    }

    // 2. Subdominant (IV) - A fourth up (or a fifth down)
    let subdominant_root = (root + 5) % 12;
    if let Some(&quality) = style.suggestions.get("subdominant") {
        suggestions.push((quality, subdominant_root));
    }

    // 3. Relative Minor/Major
    let is_major = matches!(
        played_chord.quality,
        ChordQuality::MajorTriad | ChordQuality::MajorSeventh | ChordQuality::DominantSeventh
    );
    if is_major {
        // Suggest the relative minor (vi)
        let rel_minor_root = (root + 9) % 12;
        if let Some(&quality) = style.suggestions.get("relative_minor") {
            suggestions.push((quality, rel_minor_root));
        }
    } else {
        // Suggest the relative major (bIII)
        let rel_major_root = (root + 3) % 12;
        if let Some(&quality) = style.suggestions.get("relative_major") {
            suggestions.push((quality, rel_major_root));
        }
    }

    // 4. Secondary Dominant (V/V) - The dominant of the dominant
    let sec_dominant_root = (dominant_root + 7) % 12; // (root + 7 + 7) % 12
    if let Some(&quality) = style.suggestions.get("dominant_of_dominant") {
        suggestions.push((quality, sec_dominant_root));
    }

    suggestions
}