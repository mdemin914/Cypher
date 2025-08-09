// src/fx_components/delay.rs

//! A fractional delay line using a circular buffer and linear interpolation.
use crate::fx_components::DspComponent;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

// Scaler for storing float values in atomics.
pub const PARAM_SCALER: f32 = 1_000_000.0;

/// Shared, automatable parameters for the Delay component.
#[derive(Debug, Clone)]
pub struct Params {
    /// Delay time in milliseconds. Stored as `time_ms * PARAM_SCALER`.
    pub time_ms: Arc<AtomicU32>,
    /// Feedback amount (0.0 to 1.0). Stored as `feedback * PARAM_SCALER`.
    pub feedback: Arc<AtomicU32>,
    /// High-frequency damping (0.0 to 1.0). Stored as `damping * PARAM_SCALER`.
    pub damping: Arc<AtomicU32>,
    pub bypassed: Arc<AtomicBool>,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            time_ms: Arc::new(AtomicU32::new((250.0 * PARAM_SCALER) as u32)),
            feedback: Arc::new(AtomicU32::new(0)),
            damping: Arc::new(AtomicU32::new((0.5 * PARAM_SCALER) as u32)),
            bypassed: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl Params {
    /// Helper to get a specific parameter by name for MIDI mapping.
    pub fn get_param(&self, name: &str) -> Option<Arc<AtomicU32>> {
        match name {
            "time_ms" => Some(self.time_ms.clone()),
            "feedback" => Some(self.feedback.clone()),
            "damping" => Some(self.damping.clone()),
            _ => None,
        }
    }
}

/// A simple one-pole low-pass filter used for damping the feedback signal.
#[derive(Debug, Default)]
struct DampingFilter {
    z1: f32,
}
impl DampingFilter {
    #[inline(always)]
    fn process(&mut self, input: f32, coeff: f32) -> f32 {
        let output = input * (1.0 - coeff) + self.z1 * coeff;
        self.z1 = output;
        output
    }
}


/// The audio-thread state for the Delay component.
#[derive(Debug)]
pub struct DelayLine {
    params: Params,
    buffer: Vec<f32>,
    write_pos: usize,
    max_delay_samples: usize,
    sample_rate: f32,
    damping_filter: DampingFilter,
    // Smoothed parameter values to prevent clicks
    smoothed_time_ms: f32,
    smoothed_feedback: f32,
    smoothed_damping: f32,
}

impl DelayLine {
    pub fn new(max_delay_ms: f32, sample_rate: f32, params: Params) -> Self {
        let max_delay_samples = ((max_delay_ms / 1000.0 * sample_rate).ceil() as usize).max(1);

        let initial_time_ms = params.time_ms.load(Ordering::Relaxed) as f32 / PARAM_SCALER;
        let initial_feedback = params.feedback.load(Ordering::Relaxed) as f32 / PARAM_SCALER;
        let initial_damping = params.damping.load(Ordering::Relaxed) as f32 / PARAM_SCALER;

        Self {
            params,
            buffer: vec![0.0; max_delay_samples],
            write_pos: 0,
            max_delay_samples,
            sample_rate,
            damping_filter: DampingFilter::default(),
            smoothed_time_ms: initial_time_ms,
            smoothed_feedback: initial_feedback,
            smoothed_damping: initial_damping,
        }
    }

    #[inline]
    pub fn write(&mut self, sample: f32) {
        self.buffer[self.write_pos] = sample;
        self.write_pos = (self.write_pos + 1) % self.max_delay_samples;
    }

    #[inline]
    pub fn read(&self, delay_samples: f32) -> f32 {
        let read_pos_float = (self.write_pos as f32 - delay_samples + self.max_delay_samples as f32)
            % self.max_delay_samples as f32;
        let index1 = read_pos_float.floor() as usize;
        let index2 = (index1 + 1) % self.max_delay_samples;
        let fraction = read_pos_float.fract();
        let sample1 = self.buffer[index1];
        let sample2 = self.buffer[index2];
        sample1 + fraction * (sample2 - sample1)
    }

    #[inline]
    pub fn read_ms(&self, delay_ms: f32) -> f32 {
        let delay_samples = (delay_ms / 1000.0 * self.sample_rate).max(0.0);
        self.read(delay_samples)
    }
}

impl DspComponent for DelayLine {
    fn get_mod_output(&mut self, _input_sample: f32) -> f32 {
        0.0 // Delay is an audio effect, not a modulator
    }

    #[inline]
    fn process_audio(&mut self, input: f32, mods: &BTreeMap<String, f32>) -> f32 {
        if self.params.bypassed.load(Ordering::Relaxed) {
            return input;
        }

        const SMOOTHING_COEFF: f32 = 0.9995; // Tune for responsiveness vs. artifacts

        // --- 1. Get Target Values (Atomics + Modulation) ---
        let target_time_ms = {
            let base = self.params.time_ms.load(Ordering::Relaxed) as f32 / PARAM_SCALER;
            let mod_val = mods.get("time_ms").copied().unwrap_or(0.0);
            (base + mod_val).clamp(0.1, (self.max_delay_samples as f32 / self.sample_rate) * 1000.0)
        };
        let target_feedback = {
            let base = self.params.feedback.load(Ordering::Relaxed) as f32 / PARAM_SCALER;
            let mod_val = mods.get("feedback").copied().unwrap_or(0.0);
            (base + mod_val).clamp(0.0, 0.999)
        };
        let target_damping = {
            let base = self.params.damping.load(Ordering::Relaxed) as f32 / PARAM_SCALER;
            let mod_val = mods.get("damping").copied().unwrap_or(0.0);
            (base + mod_val).clamp(0.0, 1.0)
        };

        // --- 2. Smooth the parameters to prevent clicks ---
        self.smoothed_time_ms =
            SMOOTHING_COEFF * self.smoothed_time_ms + (1.0 - SMOOTHING_COEFF) * target_time_ms;
        self.smoothed_feedback =
            SMOOTHING_COEFF * self.smoothed_feedback + (1.0 - SMOOTHING_COEFF) * target_feedback;
        self.smoothed_damping =
            SMOOTHING_COEFF * self.smoothed_damping + (1.0 - SMOOTHING_COEFF) * target_damping;

        // --- 3. Process Audio ---
        let delayed_sample = self.read_ms(self.smoothed_time_ms);
        let damped_sample = self.damping_filter.process(delayed_sample, self.smoothed_damping);
        let write_sample = input + damped_sample * self.smoothed_feedback;

        self.write(write_sample.clamp(-1.0, 1.0));

        // Return the wet signal for the FxRack to mix
        delayed_sample
    }
}