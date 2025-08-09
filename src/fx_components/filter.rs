// src/fx_components/filter.rs

//! A State Variable Filter implementation.
//! Provides low-pass, high-pass, and band-pass outputs.

use crate::fx_components::DspComponent;
use std::collections::BTreeMap;
use std::f32::consts::PI;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

// Scaler for storing float values in atomics.
pub const PARAM_SCALER: f32 = 1_000_000.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum FilterMode {
    LowPass = 0,
    HighPass = 1,
    BandPass = 2,
}

impl From<u32> for FilterMode {
    fn from(val: u32) -> Self {
        match val {
            1 => FilterMode::HighPass,
            2 => FilterMode::BandPass,
            _ => FilterMode::LowPass,
        }
    }
}

/// Shared, automatable parameters for the Filter component.
#[derive(Debug, Clone)]
pub struct Params {
    /// Filter mode (LP, HP, BP). Stored as a u32 (0, 1, or 2).
    pub mode: Arc<AtomicU32>,
    /// Cutoff frequency in Hz. Stored as `freq * PARAM_SCALER`.
    pub frequency_hz: Arc<AtomicU32>,
    /// Resonance (0.0 to 1.0). Stored as `resonance * PARAM_SCALER`.
    pub resonance: Arc<AtomicU32>,
    pub bypassed: Arc<AtomicBool>,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            mode: Arc::new(AtomicU32::new(FilterMode::LowPass as u32)),
            frequency_hz: Arc::new(AtomicU32::new((1000.0 * PARAM_SCALER) as u32)),
            resonance: Arc::new(AtomicU32::new((0.1 * PARAM_SCALER) as u32)),
            bypassed: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl Params {
    /// Helper to get a specific parameter by name for MIDI mapping.
    pub fn get_param(&self, name: &str) -> Option<Arc<AtomicU32>> {
        match name {
            "mode" => Some(self.mode.clone()),
            "frequency_hz" => Some(self.frequency_hz.clone()),
            "resonance" => Some(self.resonance.clone()),
            _ => None,
        }
    }
}

/// The audio-thread state for the Filter component.
#[derive(Debug)]
pub struct Filter {
    params: Params,
    sample_rate: f32,
    z1: f32,
    z2: f32,
}

impl Filter {
    pub fn new(sample_rate: f32, params: Params) -> Self {
        Self {
            params,
            sample_rate,
            z1: 0.0,
            z2: 0.0,
        }
    }
}

impl DspComponent for Filter {
    fn get_mod_output(&mut self, _input_sample: f32) -> f32 {
        0.0 // Filter is an audio effect, not a modulator
    }

    #[inline]
    fn process_audio(&mut self, input: f32, mods: &BTreeMap<String, f32>) -> f32 {
        if self.params.bypassed.load(Ordering::Relaxed) {
            return input;
        }

        // --- 1. Get Target Values (Atomics + Modulation) ---
        let mode = FilterMode::from(self.params.mode.load(Ordering::Relaxed));

        let target_cutoff_hz = {
            let base = self.params.frequency_hz.load(Ordering::Relaxed) as f32 / PARAM_SCALER;
            let mod_val = mods.get("frequency_hz").copied().unwrap_or(0.0);
            (base + mod_val).clamp(20.0, self.sample_rate / 2.0 - 20.0)
        };

        let target_resonance = {
            let base = self.params.resonance.load(Ordering::Relaxed) as f32 / PARAM_SCALER;
            let mod_val = mods.get("resonance").copied().unwrap_or(0.0);
            (base + mod_val).clamp(0.0, 1.0)
        };

        // --- 2. Calculate filter coefficients on the fly ---
        let g = (PI * target_cutoff_hz / self.sample_rate).tan();
        let k = 2.0 - 2.0 * target_resonance;
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        let a3 = g * a2;

        // --- 3. Process the sample ---
        let v3 = input - self.z2;
        let v1 = a1 * self.z1 + a2 * v3;
        let v2 = self.z2 + a2 * self.z1 + a3 * v3;

        self.z1 = (2.0 * v1 - self.z1).clamp(-1e6, 1e6); // Clamp to prevent denormals
        self.z2 = (2.0 * v2 - self.z2).clamp(-1e6, 1e6);

        match mode {
            FilterMode::LowPass => v2,
            FilterMode::HighPass => input - k * v1 - v2,
            FilterMode::BandPass => v1,
        }
    }
}