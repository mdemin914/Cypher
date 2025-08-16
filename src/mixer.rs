// src/mixer.rs

use crate::looper::NUM_LOOPERS;
use crate::synth::LfoRateMode;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct MixerTrackState {
    pub volume: f32,
    pub is_muted: bool,
    pub is_soloed: bool,
}

impl Default for MixerTrackState {
    fn default() -> Self {
        Self {
            volume: 1.0, // Represents 0 dB
            is_muted: false,
            is_soloed: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct MetronomeTrackState {
    pub volume: f32,
    pub pitch_hz: f32,
    pub accent_pitch_hz: f32,
    pub is_muted: bool,
}

impl Default for MetronomeTrackState {
    fn default() -> Self {
        Self {
            volume: 0.0,         // Default to silent
            pitch_hz: 880.0,     // A5
            accent_pitch_hz: 1320.0, // E6 (a fifth above A5)
            is_muted: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MixerState {
    pub tracks: [MixerTrackState; NUM_LOOPERS],
    pub metronome: MetronomeTrackState,
    pub master_volume_m_u32: u32,
    pub limiter_is_active: bool,
    pub limiter_threshold_m_u32: u32,
    pub limiter_release_mode: LfoRateMode,
    pub limiter_release_ms_m_u32: u32,
    pub limiter_release_sync_rate_m_u32: u32,
}

impl Default for MixerState {
    fn default() -> Self {
        Self {
            tracks: [MixerTrackState::default(); NUM_LOOPERS],
            metronome: MetronomeTrackState::default(),
            master_volume_m_u32: 1_000_000,
            limiter_is_active: true,
            limiter_threshold_m_u32: 1_000_000,
            limiter_release_mode: LfoRateMode::Hz,
            limiter_release_ms_m_u32: 80_000,
            limiter_release_sync_rate_m_u32: 1_000_000,
        }
    }
}