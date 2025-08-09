// src/fx_components/flanger.rs

//! A dedicated Flanger effect component.
//!
//! This component encapsulates the specific signal path required for a flanger effect,
//! including a modulated delay line, a feedback path, and a final wet/dry mix stage.

use crate::fx_components::{delay::DelayLine, lfo::Lfo, DspComponent};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

// Scaler for storing float values in atomics.
pub const PARAM_SCALER: f32 = 1_000_000.0;
// Offset for feedback to allow negative values. Stored as (feedback + 1.0) * SCALER.
pub const FEEDBACK_OFFSET: f32 = 1.0;

/// Shared, automatable parameters for the Flanger component.
#[derive(Debug, Clone)]
pub struct Params {
    /// LFO rate in Hz. Stored as `rate_hz * PARAM_SCALER`.
    pub rate_hz: Arc<AtomicU32>,
    /// Modulation depth in milliseconds. Stored as `depth_ms * PARAM_SCALER`.
    pub depth_ms: Arc<AtomicU32>,
    /// Feedback amount (-1.0 to 1.0). Stored as `(feedback + 1.0) * PARAM_SCALER`.
    pub feedback: Arc<AtomicU32>,
    pub bypassed: Arc<AtomicBool>,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            rate_hz: Arc::new(AtomicU32::new((0.2 * PARAM_SCALER) as u32)),
            depth_ms: Arc::new(AtomicU32::new((5.0 * PARAM_SCALER) as u32)),
            feedback: Arc::new(AtomicU32::new(
                ((0.85 + FEEDBACK_OFFSET) * PARAM_SCALER) as u32
            )),
            bypassed: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl Params {
    /// Helper to get a specific parameter by name for MIDI mapping.
    pub fn get_param(&self, name: &str) -> Option<Arc<AtomicU32>> {
        match name {
            "rate_hz" => Some(self.rate_hz.clone()),
            "depth_ms" => Some(self.depth_ms.clone()),
            "feedback" => Some(self.feedback.clone()),
            _ => None,
        }
    }
}

/// The audio-thread state for the Flanger component.
#[derive(Debug)]
pub struct Flanger {
    params: Params,
    // Internal DSP components
    lfo: Lfo,
    delay_line: DelayLine,
}

impl Flanger {
    pub fn new(sample_rate: f32, params: Params) -> Self {
        // Create dummy params for the internal LFO and Delay, as they aren't controlled externally.
        let lfo_params = crate::fx_components::lfo::Params::default();
        let delay_params = crate::fx_components::delay::Params::default();

        Self {
            params,
            // A flanger needs a very short max delay time. 20ms is plenty.
            delay_line: DelayLine::new(20.0, sample_rate, delay_params),
            lfo: Lfo::new(sample_rate, lfo_params),
        }
    }
}

impl DspComponent for Flanger {
    fn get_mod_output(&mut self, _input_sample: f32) -> f32 {
        0.0 // Flanger is an audio processor, not a modulator
    }

    #[inline]
    fn process_audio(&mut self, input: f32, _mods: &BTreeMap<String, f32>) -> f32 {
        if self.params.bypassed.load(Ordering::Relaxed) {
            return input;
        }

        // --- 1. Get Target Values from Atomics ---
        // Note: Flanger does not currently support external modulation of its parameters.
        let target_rate_hz = self.params.rate_hz.load(Ordering::Relaxed) as f32 / PARAM_SCALER;
        let target_depth_ms =
            self.params.depth_ms.load(Ordering::Relaxed) as f32 / PARAM_SCALER;
        let target_feedback =
            (self.params.feedback.load(Ordering::Relaxed) as f32 / PARAM_SCALER) - FEEDBACK_OFFSET;

        // --- 2. Process Audio ---

        // Get the LFO's modulation signal. Output is -1.0 to 1.0.
        let lfo_out = self.lfo.process_sample(
            target_rate_hz,
            crate::fx_components::lfo::LfoWaveform::Sine,
        );

        // Calculate the modulated delay time.
        // The LFO swings around a central delay time determined by the depth.
        let center_delay_ms = target_depth_ms;
        let modulated_delay_ms = center_delay_ms + lfo_out * target_depth_ms;

        let final_delay_ms = modulated_delay_ms.max(0.1);

        // Read the delayed (wet) signal from the delay line.
        let wet_sample = self.delay_line.read_ms(final_delay_ms);

        // Calculate the signal to be written back into the delay line.
        let write_sample = input + wet_sample * target_feedback;
        self.delay_line.write(write_sample.clamp(-1.0, 1.0));

        // Return the 100% wet signal. The FxRack handles the final mix.
        wet_sample
    }
}