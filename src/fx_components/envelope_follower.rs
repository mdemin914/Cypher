// src/fx_components/envelope_follower.rs

//! Tracks the amplitude envelope of an audio signal.
use crate::fx_components::DspComponent;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

// Scaler for storing float values in atomics.
pub const PARAM_SCALER: f32 = 1_000_000.0;

/// Shared, automatable parameters for the EnvelopeFollower component.
#[derive(Debug, Clone)]
pub struct Params {
    /// Attack time in milliseconds. Stored as `attack_ms * PARAM_SCALER`.
    pub attack_ms: Arc<AtomicU32>,
    /// Release time in milliseconds. Stored as `release_ms * PARAM_SCALER`.
    pub release_ms: Arc<AtomicU32>,
    /// Pre-gain to boost the input signal, making the follower more or less sensitive.
    pub sensitivity: Arc<AtomicU32>,
    pub bypassed: Arc<AtomicBool>,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            attack_ms: Arc::new(AtomicU32::new((10.0 * PARAM_SCALER) as u32)),
            release_ms: Arc::new(AtomicU32::new((150.0 * PARAM_SCALER) as u32)),
            // Default sensitivity of 1.0 (no boost).
            sensitivity: Arc::new(AtomicU32::new((1.0 * PARAM_SCALER) as u32)),
            bypassed: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl Params {
    /// Helper to get a specific parameter by name for MIDI mapping.
    pub fn get_param(&self, name: &str) -> Option<Arc<AtomicU32>> {
        match name {
            "attack_ms" => Some(self.attack_ms.clone()),
            "release_ms" => Some(self.release_ms.clone()),
            "sensitivity" => Some(self.sensitivity.clone()),
            _ => None,
        }
    }
}

/// The audio-thread state for the EnvelopeFollower component.
#[derive(Debug)]
pub struct EnvelopeFollower {
    params: Params,
    envelope: f32,
    sample_rate: f32,
}

impl EnvelopeFollower {
    pub fn new(sample_rate: f32, params: Params) -> Self {
        Self {
            params,
            envelope: 0.0,
            sample_rate,
        }
    }

    /// Helper to calculate the coefficient from a time in milliseconds.
    #[inline]
    fn time_to_coeff(time_ms: f32, sample_rate: f32) -> f32 {
        const EPSILON: f32 = 1e-9;
        (-(1.0 / (time_ms.max(0.1) * 0.001 * sample_rate + EPSILON))).exp()
    }
}

impl DspComponent for EnvelopeFollower {
    #[inline]
    fn get_mod_output(&mut self, input_sample: f32) -> f32 {
        if self.params.bypassed.load(Ordering::Relaxed) {
            return 0.0;
        }

        // --- 1. Load Parameters ---
        let target_attack_ms = self.params.attack_ms.load(Ordering::Relaxed) as f32 / PARAM_SCALER;
        let target_release_ms = self.params.release_ms.load(Ordering::Relaxed) as f32 / PARAM_SCALER;
        let sensitivity = self.params.sensitivity.load(Ordering::Relaxed) as f32 / PARAM_SCALER;

        let attack_coeff = Self::time_to_coeff(target_attack_ms, self.sample_rate);
        let release_coeff = Self::time_to_coeff(target_release_ms, self.sample_rate);

        // --- 2. Apply Sensitivity (Input Gain) ---
        // This is the crucial step. We boost the input signal *before* detection.
        // This allows you to make the follower react strongly even to quiet signals.
        let gained_input_abs = input_sample.abs() * sensitivity;

        // --- 3. Envelope Detection ---
        if gained_input_abs > self.envelope {
            // Attack phase
            self.envelope = (1.0 - attack_coeff) * gained_input_abs + attack_coeff * self.envelope;
        } else {
            // Release phase
            self.envelope = (1.0 - release_coeff) * gained_input_abs + release_coeff * self.envelope;
        }

        // --- 4. Final Output ---
        // Clamp the final output to the standard 0.0 to 1.0 modulation range.
        // This is important because a high sensitivity can push the envelope > 1.0,
        // but the modulation signal should be predictably capped.
        self.envelope.clamp(0.0, 1.0)
    }

    #[inline]
    fn process_audio(&mut self, input: f32, _mods: &BTreeMap<String, f32>) -> f32 {
        // The envelope follower is a modulation source; it doesn't affect the audio path.
        input
    }
}