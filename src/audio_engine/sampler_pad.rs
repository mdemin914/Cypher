// FILE: src\audio_engine\sampler_pad.rs
// =====================================

use crate::sampler::SamplerPadFxSettings;
use crate::synth::{Adsr};
use std::sync::Arc;

/// A delay line with feedback, a core part of a reverb's sound.
#[derive(Clone)]
pub struct CombFilter {
    buffer: Vec<f32>,
    index: usize,
    delay_length: usize,
    feedback: f32,
}
impl CombFilter {
    fn new(max_delay_samples: usize) -> Self {
        Self {
            buffer: vec![0.0; max_delay_samples.max(1)],
            index: 0,
            delay_length: max_delay_samples.max(1),
            feedback: 0.0,
        }
    }
    fn process(&mut self, input: f32) -> f32 {
        let read_index = (self.index + self.buffer.len() - self.delay_length) % self.buffer.len();
        let output = self.buffer[read_index];
        self.buffer[self.index] = input + output * self.feedback;
        self.index = (self.index + 1) % self.buffer.len();
        output
    }
}

/// A filter that smears the phase of a signal, used to increase echo density.
#[derive(Clone)]
pub struct AllPassFilter {
    buffer: Vec<f32>,
    index: usize,
    delay_length: usize,
}
impl AllPassFilter {
    fn new(max_delay_samples: usize) -> Self {
        Self {
            buffer: vec![0.0; max_delay_samples.max(1)],
            index: 0,
            delay_length: max_delay_samples.max(1),
        }
    }
    fn process(&mut self, input: f32) -> f32 {
        let read_index = (self.index + self.buffer.len() - self.delay_length) % self.buffer.len();
        let delayed = self.buffer[read_index];
        let output = -input + delayed;
        self.buffer[self.index] = input + delayed * 0.5; // G = 0.5
        self.index = (self.index + 1) % self.buffer.len();
        output
    }
}

/// A Schroeder-style reverb for the sampler pads.
#[derive(Clone)]
pub struct SamplerPadReverb {
    comb_filters: [CombFilter; 4],
    all_pass_filters: [AllPassFilter; 2],
    base_comb_delays: [f32; 4],
    base_allpass_delays: [f32; 2],
}

impl SamplerPadReverb {
    pub fn new(sample_rate: f32) -> Self {
        let sr_factor = sample_rate / 44100.0;
        let base_comb_delays = [1116.0, 1188.0, 1277.0, 1356.0];
        let base_allpass_delays = [225.0, 556.0];
        Self {
            comb_filters: [
                CombFilter::new((base_comb_delays[0] * sr_factor) as usize),
                CombFilter::new((base_comb_delays[1] * sr_factor) as usize),
                CombFilter::new((base_comb_delays[2] * sr_factor) as usize),
                CombFilter::new((base_comb_delays[3] * sr_factor) as usize),
            ],
            all_pass_filters: [
                AllPassFilter::new((base_allpass_delays[0] * sr_factor) as usize),
                AllPassFilter::new((base_allpass_delays[1] * sr_factor) as usize),
            ],
            base_comb_delays,
            base_allpass_delays,
        }
    }

    pub fn process(&mut self, input: f32) -> f32 {
        let comb_out = self
            .comb_filters
            .iter_mut()
            .map(|f| f.process(input))
            .sum::<f32>()
            * 0.25;
        self.all_pass_filters
            .iter_mut()
            .fold(comb_out, |acc, f| f.process(acc))
    }

    pub fn set_params(&mut self, size: f32, decay: f32, sample_rate: f32) {
        let sr_factor = sample_rate / 44100.0;
        for (i, filter) in self.comb_filters.iter_mut().enumerate() {
            let new_delay = ((self.base_comb_delays[i] * size) * sr_factor).round() as usize;
            filter.delay_length = new_delay.max(1).min(filter.buffer.len());
            filter.feedback = decay;
        }
        for (i, filter) in self.all_pass_filters.iter_mut().enumerate() {
            let new_delay = ((self.base_allpass_delays[i] * size) * sr_factor).round() as usize;
            filter.delay_length = new_delay.max(1).min(filter.buffer.len());
        }
    }

    pub fn clear(&mut self) {
        for f in &mut self.comb_filters {
            f.buffer.fill(0.0);
        }
        for f in &mut self.all_pass_filters {
            f.buffer.fill(0.0);
        }
    }
}

/// The audio-thread state for a single sampler pad.
#[derive(Clone)]
pub struct SamplerPad {
    pub audio: Arc<Vec<f32>>,
    pub playhead: f32,
    pub volume: f32,
    pub fx: SamplerPadFxSettings,
    pub amp_adsr: Adsr,
    pub reverb: SamplerPadReverb,
    pub gate_counter: usize,
    pub was_gate_open: bool,
}

impl SamplerPad {
    pub fn new(sample_rate: f32) -> Self {
        let fx = SamplerPadFxSettings::default();
        Self {
            audio: Arc::new(vec![]),
            playhead: 0.0,
            volume: 1.0,
            fx,
            amp_adsr: Adsr::new(fx.adsr, sample_rate),
            reverb: SamplerPadReverb::new(sample_rate),
            gate_counter: 0,
            was_gate_open: false,
        }
    }
}