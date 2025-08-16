//! A formant filter effect that simulates changes in the vocal tract.
use crate::fx_components::DspComponent;
use std::collections::BTreeMap;
use std::f32::consts::PI;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

// Scaler for storing float values in atomics.
pub const PARAM_SCALER: f32 = 1_000_000.0;
// Offset for character to allow negative values. Stored as `(character + 1.0) * SCALER`.
pub const CHARACTER_OFFSET: f32 = 1.0;

/// Shared, automatable parameters for the Formant component.
#[derive(Debug, Clone)]
pub struct Params {
    /// Formant character/shift (-1.0 to 1.0).
    pub character: Arc<AtomicU32>,
    /// Resonance/Q of the formant peaks (0.0 to 1.0).
    pub resonance: Arc<AtomicU32>,
    pub bypassed: Arc<AtomicBool>,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            // Default to 0.0 character. Stored as (0.0 + 1.0) * SCALER = 1_000_000
            character: Arc::new(AtomicU32::new(
                ((0.0 + CHARACTER_OFFSET) * PARAM_SCALER) as u32
            )),
            // Default to medium resonance
            resonance: Arc::new(AtomicU32::new((0.7 * PARAM_SCALER) as u32)),
            bypassed: Arc::new(AtomicBool::new(false)),
        }
    }
}

// THIS IS THE NEWLY ADDED BLOCK TO FIX THE ERROR
impl Params {
    /// Helper to get a specific parameter by name for MIDI mapping.
    pub fn get_param(&self, name: &str) -> Option<Arc<AtomicU32>> {
        match name {
            "character" => Some(self.character.clone()),
            "resonance" => Some(self.resonance.clone()),
            _ => None,
        }
    }
}

/// A simple state-variable filter, configured for band-pass output.
#[derive(Debug, Clone, Copy, Default)]
struct BandPassFilter {
    z1: f32,
    z2: f32,
}
impl BandPassFilter {
    #[inline(always)]
    fn process(&mut self, input: f32, freq: f32, q: f32, sample_rate: f32) -> f32 {
        let g = (PI * freq / sample_rate).tan();
        let k = 1.0 / q.max(0.01); // Ensure q is not zero

        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        let a3 = g * a2;

        let v3 = input - self.z2;
        let v1 = a1 * self.z1 + a2 * v3; // Band-pass output
        let v2 = self.z2 + a2 * self.z1 + a3 * v3; // Low-pass output

        self.z1 = (2.0 * v1 - self.z1).clamp(-1e6, 1e6);
        self.z2 = (2.0 * v2 - self.z2).clamp(-1e6, 1e6);

        v1
    }
}

/// The audio-thread state for the Formant component.
#[derive(Debug)]
pub struct Formant {
    params: Params,
    sample_rate: f32,
    filters: [BandPassFilter; 5],
    // Base formant frequencies for a neutral "ah" vowel sound.
    base_formants: [f32; 5],
}

impl Formant {
    pub fn new(sample_rate: f32, params: Params) -> Self {
        Self {
            params,
            sample_rate,
            filters: [BandPassFilter::default(); 5],
            base_formants: [700.0, 1220.0, 2600.0, 3500.0, 4500.0],
        }
    }
}

impl DspComponent for Formant {
    fn get_mod_output(&mut self, _input_sample: f32) -> f32 {
        0.0 // Formant is an audio effect, not a modulator
    }

    #[inline]
    fn process_audio(&mut self, input: f32, _mods: &BTreeMap<String, f32>) -> f32 {
        if self.params.bypassed.load(Ordering::Relaxed) {
            return input;
        }

        // --- 1. Get Target Values from Atomics ---
        // Note: Modulation is not yet implemented for this component's internal params.
        let character =
            (self.params.character.load(Ordering::Relaxed) as f32 / PARAM_SCALER) - CHARACTER_OFFSET;
        let resonance = self.params.resonance.load(Ordering::Relaxed) as f32 / PARAM_SCALER;

        // --- 2. Calculate final parameters ---
        // Map character (-1 to 1) to a frequency shift ratio (e.g., 0.7 to 1.3)
        let shift_ratio = 1.0 + character * 0.3;
        // Map resonance (0 to 1) to a Q factor (e.g., 1.0 to 20.0)
        let q = 1.0 + resonance * 19.0;

        // --- 3. Process the signal through the parallel filter bank ---
        let mut output = 0.0;
        for (i, filter) in self.filters.iter_mut().enumerate() {
            let formant_freq = (self.base_formants[i] * shift_ratio)
                .clamp(20.0, self.sample_rate / 2.0 - 20.0);
            output += filter.process(input, formant_freq, q, self.sample_rate);
        }

        // Normalize the output of the 5 filters and mix with dry signal.
        // The FxRack will handle the final wet/dry mix, so we return the 100% wet signal.
        (output * 0.2).clamp(-1.0, 1.0)
    }
}