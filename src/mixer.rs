use crate::looper::NUM_LOOPERS;
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TrackMixerState {
    pub tracks: [MixerTrackState; NUM_LOOPERS],
}

impl Default for TrackMixerState {
    fn default() -> Self {
        Self {
            tracks: [MixerTrackState::default(); NUM_LOOPERS],
        }
    }
}