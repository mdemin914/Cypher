// src/fx_components/quantizer.rs

//! A "lo-fi" effect that reduces the bit depth and/or sample rate of a signal.
use crate::fx_components::DspComponent;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

// Scaler for storing float values in atomics.
pub const PARAM_SCALER: f32 = 1_000_000.0;

/// Shared, automatable parameters for the Quantizer component.
#[derive(Debug, Clone)]
pub struct Params {
    /// Bit depth (1.0 to 16.0). Stored as `bit_depth * PARAM_SCALER`.
    pub bit_depth: Arc<AtomicU32>,
    /// Downsample factor (1 to 50). Stored as `downsample * PARAM_SCALER`.
    pub downsample: Arc<AtomicU32>,
    pub bypassed: Arc<AtomicBool>,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            bit_depth: Arc::new(AtomicU32::new((16.0 * PARAM_SCALER) as u32)),
            downsample: Arc::new(AtomicU32::new((1.0 * PARAM_SCALER) as u32)),
            bypassed: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl Params {
    /// Helper to get a specific parameter by name for MIDI mapping.
    pub fn get_param(&self, name: &str) -> Option<Arc<AtomicU32>> {
        match name {
            "bit_depth" => Some(self.bit_depth.clone()),
            "downsample" => Some(self.downsample.clone()),
            _ => None,
        }
    }
}

/// The audio-thread state for the Quantizer component.
#[derive(Debug)]
pub struct Quantizer {
    params: Params,
    sample_counter: u32,
    last_sample: f32,
}

impl Quantizer {
    pub fn new(params: Params) -> Self {
        Self {
            params,
            sample_counter: 0,
            last_sample: 0.0,
        }
    }
}

impl DspComponent for Quantizer {
    fn get_mod_output(&mut self, _input_sample: f32) -> f32 {
        0.0 // Quantizer is an audio effect, not a modulator
    }

    #[inline]
    fn process_audio(&mut self, input: f32, mods: &BTreeMap<String, f32>) -> f32 {
        if self.params.bypassed.load(Ordering::Relaxed) {
            return input;
        }

        // --- 1. Get Target Values (Atomics + Modulation) ---
        let target_bit_depth = {
            let base = self.params.bit_depth.load(Ordering::Relaxed) as f32 / PARAM_SCALER;
            let mod_val = mods.get("bit_depth").copied().unwrap_or(0.0);
            (base + mod_val).clamp(1.0, 16.0)
        };

        let target_downsample = {
            let base = self.params.downsample.load(Ordering::Relaxed) as f32 / PARAM_SCALER;
            let mod_val = mods.get("downsample").copied().unwrap_or(0.0);
            // Ensure the factor is at least 1 to prevent division by zero in the modulo.
            (base + mod_val).round().max(1.0) as u32
        };

        // ====================================================================
        // === BUG FIX: Replaced the unstable counter logic with a robust modulo-based approach. ===
        // This ensures the sample-and-hold timing is always stable, even when `target_downsample`
        // is modulated rapidly. This will eliminate the "porcupine" sound.
        if self.sample_counter == 0 {
            // It's time to take a new sample.
            self.last_sample = input;
        }

        // The counter now increments and wraps around cleanly using the modulo operator.
        self.sample_counter = (self.sample_counter + 1) % target_downsample;

        // For all samples in the cycle (including the first), we output the last held sample.
        let downsampled_input = self.last_sample;
        // ====================================================================


        // --- 3. Process Bit Crushing (This part was already correct) ---
        let num_steps = 2.0_f32.powf(target_bit_depth);
        let inv_num_steps = 1.0 / num_steps;

        let scaled_sample = (downsampled_input * 0.5 + 0.5) * num_steps;
        let quantized_sample_scaled = scaled_sample.round();

        (quantized_sample_scaled * inv_num_steps) * 2.0 - 1.0
    }
}