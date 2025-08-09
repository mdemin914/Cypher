// src/fx_components/gain.rs

//! A simple audio gain component.
//!
//! Multiplies the incoming audio signal by a given factor.
use crate::fx_components::DspComponent;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

// Scaler for dB parameter storage. (value_db + DB_OFFSET) * DB_SCALER.
pub const DB_SCALER: f32 = 100_000.0;
// The offset used to ensure the stored value is always positive. The UI range is -60 to 24.
pub const DB_OFFSET: f32 = 60.0;

/// Shared, automatable parameters for the Gain component.
#[derive(Debug, Clone)]
pub struct Params {
    /// Gain in dB, stored as a scaled u32: `(value_db + 60.0) * 100_000.0`
    pub gain_db: Arc<AtomicU32>,
    pub bypassed: Arc<AtomicBool>,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            // Default to 0.0 dB. Stored as (0.0 + 60.0) * 100_000 = 6_000_000.
            gain_db: Arc::new(AtomicU32::new(
                ((0.0 + DB_OFFSET) * DB_SCALER) as u32,
            )),
            bypassed: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl Params {
    /// Helper to get a specific parameter by name for MIDI mapping.
    pub fn get_param(&self, name: &str) -> Option<Arc<AtomicU32>> {
        match name {
            "gain_db" => Some(self.gain_db.clone()),
            _ => None,
        }
    }
}

/// The audio-thread state for the Gain component. It holds a clone of the shared parameters.
#[derive(Debug)]
pub struct Gain {
    params: Params,
}

impl Gain {
    pub fn new(params: Params) -> Self {
        Self { params }
    }
}

impl DspComponent for Gain {
    fn get_mod_output(&mut self, _input_sample: f32) -> f32 {
        0.0 // Gain is not a modulator
    }

    #[inline]
    fn process_audio(&mut self, input: f32, mods: &BTreeMap<String, f32>) -> f32 {
        if self.params.bypassed.load(Ordering::Relaxed) {
            return input;
        }

        // 1. Get the base dB value from the atomic parameter, converting it back to a float.
        let base_gain_u32 = self.params.gain_db.load(Ordering::Relaxed);
        let base_gain_db = (base_gain_u32 as f32 / DB_SCALER) - DB_OFFSET;

        // 2. Get the modulation value from other components, which also comes in as dB.
        let gain_mod_db = mods.get("gain_db").copied().unwrap_or(0.0);

        // 3. Calculate the final gain in dB by summing the base value and modulation.
        let final_gain_db = base_gain_db + gain_mod_db;

        // 4. Convert the final dB value to a linear gain factor for multiplication.
        let final_linear_gain = 10.0_f32.powf(final_gain_db.clamp(-60.0, 30.0) / 20.0);

        // Optimization: if gain is very close to 1.0, just pass the input through.
        if (final_linear_gain - 1.0).abs() < 1e-6 {
            return input;
        }

        input * final_linear_gain
    }
}