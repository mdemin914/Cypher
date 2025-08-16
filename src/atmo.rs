// src/atmo.rs

//! Defines the data structures for the "Atmo" generative soundscape engine.
//! These structs are designed to be serialized to and from session files.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Defines the playback behavior for an Atmo layer.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackMode {
    /// Loops a small portion of a long file to create a continuous drone.
    FragmentLooping,
    /// Triggers discrete, full samples, creating a "cloud" of sounds.
    TriggeredEvents,
}

/// The core parameters for a single layer, which can be morphed between scenes.
#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
#[serde(default)]
pub struct AtmoLayerParams {
    // Shared parameters
    pub volume: f32,
    pub playback_rate: f32,
    pub pan_randomness: f32,
    pub filter_cutoff: f32,

    // Mode-specific parameters
    pub mode: PlaybackMode,
    /// For FragmentLooping: the length of the loop as a % of the total sample length.
    pub fragment_length: f32,
    /// For TriggeredEvents: controls the timing between triggers (-100% gap to +100% overlap).
    pub density: f32,
}

impl Default for AtmoLayerParams {
    fn default() -> Self {
        Self {
            // Shared
            volume: 0.7,
            playback_rate: 1.0,
            pan_randomness: 0.5,
            filter_cutoff: 1.0,

            // Mode-specific
            mode: PlaybackMode::TriggeredEvents, // Default to the simpler mode
            fragment_length: 0.1,                // 10%
            density: 0.0,                        // No gap, no overlap
        }
    }
}

/// The configuration for a single sound-generating layer.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct AtmoLayer {
    pub sample_folder_path: Option<PathBuf>,
    pub params: AtmoLayerParams,
}

/// Defines the complete state for one of the four corners of the X/Y performance pad.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct AtmoScene {
    pub name: String,
    pub layers: [AtmoLayer; 4],
}

impl Default for AtmoScene {
    fn default() -> Self {
        Self {
            name: "New Scene".to_string(),
            layers: Default::default(),
        }
    }
}

/// The top-level preset for the Atmo engine that a user saves and loads.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct AtmoPreset {
    pub name: String,
    pub scenes: [AtmoScene; 4],
    pub xy_coords: u64, // ADD THIS LINE
}

impl Default for AtmoPreset {
    fn default() -> Self {
        // Default to center position
        let center_xy = (0.5 * u32::MAX as f32) as u32;
        Self {
            name: "Default Atmosphere".to_string(),
            scenes: Default::default(),
            xy_coords: (center_xy as u64) << 32 | (center_xy as u64),
        }
    }
}

impl AtmoPreset {
    /// Checks if any layer in any scene has a sample folder loaded.
    pub fn is_empty(&self) -> bool {
        self.scenes
            .iter()
            .all(|scene| scene.layers.iter().all(|layer| layer.sample_folder_path.is_none()))
    }
}