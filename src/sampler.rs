use crate::synth::AdsrSettings;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
#[serde(default)]
pub struct SamplerPadFxSettings {
    pub volume: f32,
    pub pitch_semitones: f32,
    pub adsr: AdsrSettings,
    pub distortion_amount: f32, // 0.0 to 1.0
    pub reverb_mix: f32,        // 0.0 to 1.0
    pub reverb_size: f32,       // 0.0 to 1.0
    pub reverb_decay: f32,      // 0.0 to 1.0
    pub is_reverb_gated: bool,
    pub gate_close_time_ms: f32, // e.g., 0 to 2000ms
}

impl Default for SamplerPadFxSettings {
    fn default() -> Self {
        Self {
            volume: 1.0,
            pitch_semitones: 0.0,
            // Default ADSR is "play as is"
            adsr: AdsrSettings {
                attack: 0.0,
                decay: 0.0,
                sustain: 1.0,
                release: 4.0,
            },
            distortion_amount: 0.0,
            reverb_mix: 0.0,
            reverb_size: 0.7,
            reverb_decay: 0.8,
            is_reverb_gated: false,
            gate_close_time_ms: 0.0,
        }
    }
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
#[serde(default)]
pub struct SamplerPadSettings {
    pub path: Option<PathBuf>,
    pub fx: SamplerPadFxSettings,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
#[serde(default)]
pub struct SamplerKit {
    // An array of 16 pad settings, including path and fx.
    pub pads: [SamplerPadSettings; 16],
}