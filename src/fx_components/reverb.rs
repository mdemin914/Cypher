// src/fx_components/reverb.rs

//! A Schroeder-style reverb effect component.
//!
//! This is a more complex, high-level component that internally manages a network of
//! delay lines (comb filters) and phase diffusers (all-pass filters) to create a
//! reverberant sound.

use crate::fx_components::DspComponent;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

// Scaler for storing float values in atomics.
pub const PARAM_SCALER: f32 = 1_000_000.0;

// --- Shared, Automatable Parameters ---

#[derive(Debug, Clone)]
pub struct Params {
    /// Room size (0.0 to 1.0). Stored as `size * PARAM_SCALER`.
    pub size: Arc<AtomicU32>,
    /// Decay time (0.0 to 1.0). Stored as `decay * PARAM_SCALER`.
    pub decay: Arc<AtomicU32>,
    /// High-frequency damping (0.0 to 1.0). Stored as `damping * PARAM_SCALER`.
    pub damping: Arc<AtomicU32>,
    pub bypassed: Arc<AtomicBool>,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            size: Arc::new(AtomicU32::new((0.7 * PARAM_SCALER) as u32)),
            decay: Arc::new(AtomicU32::new((0.8 * PARAM_SCALER) as u32)),
            damping: Arc::new(AtomicU32::new((0.5 * PARAM_SCALER) as u32)),
            bypassed: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl Params {
    /// Helper to get a specific parameter by name for MIDI mapping.
    pub fn get_param(&self, name: &str) -> Option<Arc<AtomicU32>> {
        match name {
            "size" => Some(self.size.clone()),
            "decay" => Some(self.decay.clone()),
            "damping" => Some(self.damping.clone()),
            _ => None,
        }
    }
}

// --- Internal Building Blocks for the Reverb ---

/// A simple one-pole low-pass filter, used for damping the reverb tail.
#[derive(Debug, Clone, Copy, Default)]
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

/// A delay line with feedback, a core part of a reverb's sound.
#[derive(Debug, Clone)]
struct CombFilter {
    buffer: Vec<f32>,
    write_pos: usize,
    delay_length: usize,
    damping_filter: DampingFilter,
}
impl CombFilter {
    fn new(max_delay_samples: usize) -> Self {
        Self {
            buffer: vec![0.0; max_delay_samples.max(1)],
            write_pos: 0,
            delay_length: max_delay_samples.max(1),
            damping_filter: DampingFilter::default(),
        }
    }
    #[inline(always)]
    fn process(&mut self, input: f32, feedback: f32, damping: f32) -> f32 {
        let read_index = (self.write_pos + self.buffer.len() - self.delay_length)
            % self.buffer.len();
        let output = self.buffer[read_index];
        let damped_output = self.damping_filter.process(output, damping);
        self.buffer[self.write_pos] = input + damped_output * feedback;
        self.write_pos = (self.write_pos + 1) % self.buffer.len();
        output
    }
}

/// A filter that smears the phase of a signal, used to increase echo density.
#[derive(Debug, Clone)]
struct AllPassFilter {
    buffer: Vec<f32>,
    write_pos: usize,
    delay_length: usize,
}
impl AllPassFilter {
    fn new(max_delay_samples: usize) -> Self {
        Self {
            buffer: vec![0.0; max_delay_samples.max(1)],
            write_pos: 0,
            delay_length: max_delay_samples.max(1),
        }
    }
    #[inline(always)]
    fn process(&mut self, input: f32) -> f32 {
        let read_index = (self.write_pos + self.buffer.len() - self.delay_length)
            % self.buffer.len();
        let delayed = self.buffer[read_index];
        let output = -input + delayed;
        self.buffer[self.write_pos] = input + delayed * 0.5; // G = 0.5 (fixed)
        self.write_pos = (self.write_pos + 1) % self.buffer.len();
        output
    }
}

// --- Main Public Reverb Struct ---

#[derive(Debug)]
pub struct Reverb {
    params: Params,
    comb_filters: [CombFilter; 4],
    all_pass_filters: [AllPassFilter; 2],
    base_comb_delays: [f32; 4],
    base_allpass_delays: [f32; 2],
    sample_rate: f32,
}

impl Reverb {
    pub fn new(sample_rate: f32, params: Params) -> Self {
        let sr_factor = sample_rate / 44100.0;
        // Prime numbers are good for delay lengths to avoid periodic artifacts.
        let base_comb_delays = [1117.0, 1187.0, 1277.0, 1351.0];
        let base_allpass_delays = [223.0, 557.0];

        // Allocate buffers with a bit of extra room for size modulation.
        let max_size_multiplier = 1.5;

        Self {
            params,
            comb_filters: [
                CombFilter::new((base_comb_delays[0] * sr_factor * max_size_multiplier) as usize),
                CombFilter::new((base_comb_delays[1] * sr_factor * max_size_multiplier) as usize),
                CombFilter::new((base_comb_delays[2] * sr_factor * max_size_multiplier) as usize),
                CombFilter::new((base_comb_delays[3] * sr_factor * max_size_multiplier) as usize),
            ],
            all_pass_filters: [
                AllPassFilter::new((base_allpass_delays[0] * sr_factor * max_size_multiplier) as usize),
                AllPassFilter::new((base_allpass_delays[1] * sr_factor * max_size_multiplier) as usize),
            ],
            base_comb_delays,
            base_allpass_delays,
            sample_rate,
        }
    }
}

impl DspComponent for Reverb {
    fn get_mod_output(&mut self, _input_sample: f32) -> f32 {
        0.0
    }

    #[inline]
    fn process_audio(&mut self, input: f32, mods: &BTreeMap<String, f32>) -> f32 {
        if self.params.bypassed.load(Ordering::Relaxed) {
            return input;
        }

        // --- 1. Get Target Values (Atomics + Modulation) ---
        let target_size = {
            let base = self.params.size.load(Ordering::Relaxed) as f32 / PARAM_SCALER;
            let mod_val = mods.get("size").copied().unwrap_or(0.0);
            (base + mod_val).clamp(0.0, 1.0)
        };
        let target_decay = {
            let base = self.params.decay.load(Ordering::Relaxed) as f32 / PARAM_SCALER;
            let mod_val = mods.get("decay").copied().unwrap_or(0.0);
            (base + mod_val).clamp(0.0, 1.0)
        };
        let target_damping = {
            let base = self.params.damping.load(Ordering::Relaxed) as f32 / PARAM_SCALER;
            let mod_val = mods.get("damping").copied().unwrap_or(0.0);
            (base + mod_val).clamp(0.0, 1.0)
        };

        // --- 2. Update Internal DSP Parameters ---
        let sr_factor = self.sample_rate / 44100.0;
        let size_multiplier = 0.5 + target_size;
        let damping_coeff = target_damping.powf(2.0) * 0.4 + 0.05;

        for (i, filter) in self.comb_filters.iter_mut().enumerate() {
            let new_delay = ((self.base_comb_delays[i] * size_multiplier) * sr_factor).round() as usize;
            filter.delay_length = new_delay.max(1).min(filter.buffer.len());
        }
        for (i, filter) in self.all_pass_filters.iter_mut().enumerate() {
            let new_delay = ((self.base_allpass_delays[i] * size_multiplier) * sr_factor).round() as usize;
            filter.delay_length = new_delay.max(1).min(filter.buffer.len());
        }

        // --- 3. Process Audio ---
        let comb_out = self
            .comb_filters
            .iter_mut()
            .map(|f| f.process(input, target_decay, damping_coeff))
            .sum::<f32>() * 0.25; // Average the parallel comb filters

        let wet_signal = self
            .all_pass_filters
            .iter_mut()
            .fold(comb_out, |acc, f| f.process(acc));

        // Return the 100% wet signal. The FxRack is responsible for the final mix.
        wet_signal
    }
}