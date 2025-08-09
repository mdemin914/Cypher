// src/fx_components/lfo.rs

//! A Low-Frequency Oscillator for generating modulation signals.
use crate::fx_components::DspComponent;
use std::collections::BTreeMap;
use std::f32::consts::TAU;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

// Scaler for storing float values in atomics.
pub const PARAM_SCALER: f32 = 1_000_000.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum LfoWaveform {
    Sine = 0,
    Triangle = 1,
    Saw = 2,
    InvSaw = 3, // Inverted Saw (Ramp)
    Square = 4,
    Random = 5, // Sample & Hold
}

impl From<u32> for LfoWaveform {
    fn from(val: u32) -> Self {
        match val {
            1 => LfoWaveform::Triangle,
            2 => LfoWaveform::Saw,
            3 => LfoWaveform::InvSaw,
            4 => LfoWaveform::Square,
            5 => LfoWaveform::Random,
            _ => LfoWaveform::Sine,
        }
    }
}

/// Shared, automatable parameters for the LFO component.
#[derive(Debug, Clone)]
pub struct Params {
    /// LFO waveform shape. Stored as a u32 (0-5).
    pub waveform: Arc<AtomicU32>,
    /// LFO rate in Hz. Stored as `freq * PARAM_SCALER`.
    pub frequency_hz: Arc<AtomicU32>,
    pub bypassed: Arc<AtomicBool>,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            waveform: Arc::new(AtomicU32::new(LfoWaveform::Sine as u32)),
            frequency_hz: Arc::new(AtomicU32::new((1.0 * PARAM_SCALER) as u32)),
            bypassed: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl Params {
    /// Helper to get a specific parameter by name for MIDI mapping.
    pub fn get_param(&self, name: &str) -> Option<Arc<AtomicU32>> {
        match name {
            "waveform" => Some(self.waveform.clone()),
            "frequency_hz" => Some(self.frequency_hz.clone()),
            _ => None,
        }
    }
}

/// The audio-thread state for the LFO component.
#[derive(Debug)]
pub struct Lfo {
    params: Params,
    phase: f32,
    sample_rate: f32,
    last_output: f32,
}

impl Lfo {
    pub fn new(sample_rate: f32, params: Params) -> Self {
        Self {
            params,
            phase: 0.0,
            sample_rate,
            last_output: 0.0,
        }
    }

    /// Processes one sample of the LFO, advancing its phase and returning the new value.
    #[inline]
    pub fn process_sample(&mut self, frequency_hz: f32, waveform: LfoWaveform) -> f32 {
        let phase_inc = frequency_hz / self.sample_rate;
        self.phase = (self.phase + phase_inc) % 1.0;

        match waveform {
            LfoWaveform::Sine => (self.phase * TAU).sin(),
            LfoWaveform::Triangle => 1.0 - 4.0 * (self.phase - 0.5).abs(),
            LfoWaveform::Saw => 2.0 * self.phase - 1.0,
            LfoWaveform::InvSaw => 1.0 - 2.0 * self.phase,
            LfoWaveform::Square => if self.phase < 0.5 { 1.0 } else { -1.0 },
            LfoWaveform::Random => {
                // Check for phase wrap-around to generate a new random value
                if self.phase < phase_inc {
                    self.last_output = rand::random::<f32>() * 2.0 - 1.0;
                }
                self.last_output
            }
        }
    }
}

impl DspComponent for Lfo {
    #[inline]
    fn get_mod_output(&mut self, _input_sample: f32) -> f32 {
        if self.params.bypassed.load(Ordering::Relaxed) {
            return 0.0;
        }

        let frequency_hz =
            self.params.frequency_hz.load(Ordering::Relaxed) as f32 / PARAM_SCALER;
        let waveform = LfoWaveform::from(self.params.waveform.load(Ordering::Relaxed));

        // The core logic is now in a reusable public method.
        self.process_sample(frequency_hz, waveform)
    }

    #[inline]
    fn process_audio(&mut self, input: f32, _mods: &BTreeMap<String, f32>) -> f32 {
        // LFO is a modulator, so it just passes audio through.
        input
    }
}