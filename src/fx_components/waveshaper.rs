// src/fx_components/waveshaper.rs

//! Applies non-linear distortion to an audio signal.
use crate::fx_components::DspComponent;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

// Scaler for dB parameter storage. (value_db * DB_SCALER).
pub const DB_SCALER: f32 = 100_000.0;

trait FastTanh {
    fn fast_tanh(self) -> Self;
}

impl FastTanh for f32 {
    #[inline(always)]
    fn fast_tanh(self) -> Self {
        let x2 = self * self;
        self * (27.0 + x2) / (27.0 + 9.0 * x2)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum WaveshaperMode {
    HardClip = 0,
    Saturation = 1,
    Sine = 2,
    Fold = 3,
}

impl From<u32> for WaveshaperMode {
    fn from(val: u32) -> Self {
        match val {
            1 => WaveshaperMode::Saturation,
            2 => WaveshaperMode::Sine,
            3 => WaveshaperMode::Fold,
            _ => WaveshaperMode::HardClip,
        }
    }
}

/// Shared, automatable parameters for the Waveshaper component.
#[derive(Debug, Clone)]
pub struct Params {
    /// Waveshaper mode. Stored as a u32 (0-3).
    pub mode: Arc<AtomicU32>,
    /// Pre-gain drive in dB. Stored as `drive_db * DB_SCALER`.
    pub drive_db: Arc<AtomicU32>,
    pub bypassed: Arc<AtomicBool>,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            mode: Arc::new(AtomicU32::new(WaveshaperMode::Saturation as u32)),
            // Default to 0 dB drive
            drive_db: Arc::new(AtomicU32::new(0)),
            bypassed: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl Params {
    /// Helper to get a specific parameter by name for MIDI mapping.
    pub fn get_param(&self, name: &str) -> Option<Arc<AtomicU32>> {
        match name {
            "mode" => Some(self.mode.clone()),
            "drive_db" => Some(self.drive_db.clone()),
            _ => None,
        }
    }
}

/// The audio-thread state for the Waveshaper component.
#[derive(Debug)]
pub struct Waveshaper {
    params: Params,
}

impl Waveshaper {
    pub fn new(params: Params) -> Self {
        Self { params }
    }
}

impl DspComponent for Waveshaper {
    fn get_mod_output(&mut self, _input_sample: f32) -> f32 {
        0.0 // Waveshaper is an audio effect, not a modulator
    }

    #[inline]
    fn process_audio(&mut self, input: f32, mods: &BTreeMap<String, f32>) -> f32 {
        if self.params.bypassed.load(Ordering::Relaxed) {
            return input;
        }

        // --- 1. Get Target Values (Atomics + Modulation) ---
        let mode = WaveshaperMode::from(self.params.mode.load(Ordering::Relaxed));

        let target_drive_db = {
            let base = self.params.drive_db.load(Ordering::Relaxed) as f32 / DB_SCALER;
            let mod_val = mods.get("drive_db").copied().unwrap_or(0.0);
            base + mod_val
        };

        // --- 2. Calculate final linear gain ---
        let final_drive_gain = 10.0_f32.powf(target_drive_db.clamp(0.0, 48.0) / 20.0);

        let driven_input = input * final_drive_gain;

        // --- 3. Apply selected shaping function ---
        match mode {
            WaveshaperMode::HardClip => driven_input.clamp(-1.0, 1.0),
            WaveshaperMode::Saturation => driven_input.fast_tanh(),
            WaveshaperMode::Sine => (driven_input * std::f32::consts::FRAC_PI_2).sin(),
            WaveshaperMode::Fold => {
                if driven_input > 1.0 || driven_input < -1.0 {
                    let pi2 = std::f32::consts::PI * 2.0;
                    ((driven_input + 1.0) * pi2 / 4.0).sin()
                } else {
                    driven_input
                }
            }
        }
    }
}