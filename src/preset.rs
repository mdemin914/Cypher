// src/preset.rs
use crate::sampler_engine;
use crate::wavetable_engine;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum SynthEnginePreset {
    Wavetable(wavetable_engine::WavetableEnginePreset),
    Sampler(sampler_engine::SamplerEnginePreset),
}

impl Default for SynthEnginePreset {
    fn default() -> Self {
        // Default to wavetable to not break existing functionality
        SynthEnginePreset::Wavetable(Default::default())
    }
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SynthPreset {
    pub engine_presets: [SynthEnginePreset; 2],
}