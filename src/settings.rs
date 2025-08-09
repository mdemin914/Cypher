// src/settings.rs

use crate::fx;
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeMap;
use std::env;
use std::fmt;
use std::fs;
use std::path::PathBuf;

/// A simple, copyable ID for a MIDI CC message, used internally for real-time modulation.
/// This remains unchanged to avoid performance issues on the audio thread.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MidiControlId {
    pub channel: u8,
    pub cc: u8,
}

/// A complete, unique identifier for a high-level MIDI control mapping.
/// It includes the port name to distinguish between different devices.
/// This is NOT `Copy` because `String` is a heap-allocated type.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FullMidiControlId {
    pub port_name: String,
    pub channel: u8,
    pub cc: u8,
}

// Custom implementation to convert the struct into a single string for JSON map keys.
impl Serialize for FullMidiControlId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Format: "port_name|channel|cc"
        // An empty port_name will result in a leading pipe, which is fine. e.g., "|0|74"
        serializer.serialize_str(&format!("{}|{}|{}", self.port_name, self.channel, self.cc))
    }
}

// Custom implementation to parse the string from a JSON map key back into the struct.
impl<'de> Deserialize<'de> for FullMidiControlId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct FullMidiControlIdVisitor;

        impl<'de> de::Visitor<'de> for FullMidiControlIdVisitor {
            type Value = FullMidiControlId;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a string in the format 'port_name|channel|cc'")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                let parts: Vec<&str> = value.splitn(3, '|').collect();
                if parts.len() != 3 {
                    return Err(E::custom(format!(
                        "expected 3 parts separated by '|', found {} in '{}'",
                        parts.len(),
                        value
                    )));
                }

                let port_name = parts[0].to_string();
                let channel = parts[1].parse::<u8>().map_err(E::custom)?;
                let cc = parts[2].parse::<u8>().map_err(E::custom)?;

                Ok(FullMidiControlId {
                    port_name,
                    channel,
                    cc,
                })
            }
        }

        deserializer.deserialize_str(FullMidiControlIdVisitor)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ControllableParameter {
    // Looper
    Looper(usize),

    // Mixer
    MixerVolume(usize),
    MixerToggleMute(usize),
    MixerToggleSolo(usize),

    // Instruments
    SynthToggleActive,
    SynthMasterVolume,
    SamplerToggleActive,
    SamplerMasterVolume,

    // Audio Input
    InputToggleArm,
    InputToggleMonitor,

    // Transport
    TransportTogglePlay,
    TransportToggleMuteAll,
    TransportClearAll,
    TransportToggleRecord,

    // Master Section
    MasterVolume,
    LimiterThreshold,

    // --- NEW: FX Parameters ---
    Fx(FxParamIdentifier),
}

/// Uniquely identifies a single parameter within a specific FX rack.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FxParamIdentifier {
    pub point: fx::InsertionPoint,
    pub component_index: usize,
    pub param_name: FxParamName, // Use a fixed-size enum instead of a String
}

/// An enum representing all possible parameter names for all FX components.
/// This is necessary to make `FxParamIdentifier` and `ControllableParameter` `Copy`.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FxParamName {
    // Shared
    Bypass,
    WetDry,
    // Gain
    GainDb,
    // Delay
    TimeMs,
    Feedback,
    Damping,
    // Filter
    Mode,
    FrequencyHz,
    Resonance,
    // LFO
    Waveform,
    // EnvelopeFollower
    AttackMs,
    ReleaseMs,
    // Waveshaper
    DriveDb,
    // Quantizer
    BitDepth,
    Downsample,
    // Reverb
    Size,
    Decay,
    // Flanger
    RateHz,
    DepthMs,
}

impl FxParamName {
    pub fn as_str(&self) -> &'static str {
        match self {
            FxParamName::Bypass => "bypassed",
            FxParamName::WetDry => "wet_dry_mix",
            FxParamName::GainDb => "gain_db",
            FxParamName::TimeMs => "time_ms",
            FxParamName::Feedback => "feedback",
            FxParamName::Damping => "damping",
            FxParamName::Mode => "mode",
            FxParamName::FrequencyHz => "frequency_hz",
            FxParamName::Resonance => "resonance",
            FxParamName::Waveform => "waveform",
            FxParamName::AttackMs => "attack_ms",
            FxParamName::ReleaseMs => "release_ms",
            FxParamName::DriveDb => "drive_db",
            FxParamName::BitDepth => "bit_depth",
            FxParamName::Downsample => "downsample",
            FxParamName::Size => "size",
            FxParamName::Decay => "decay",
            FxParamName::RateHz => "rate_hz",
            FxParamName::DepthMs => "depth_ms",
        }
    }
}

impl std::fmt::Display for ControllableParameter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ControllableParameter::Looper(i) => write!(f, "Looper {} Trigger", i + 1),
            ControllableParameter::MixerVolume(i) => write!(f, "Mixer Ch {} Volume", i + 1),
            ControllableParameter::MixerToggleMute(i) => write!(f, "Mixer Ch {} Mute", i + 1),
            ControllableParameter::MixerToggleSolo(i) => write!(f, "Mixer Ch {} Solo", i + 1),
            ControllableParameter::SynthToggleActive => write!(f, "Synth Active Toggle"),
            ControllableParameter::SynthMasterVolume => write!(f, "Synth Master Volume"),
            ControllableParameter::SamplerToggleActive => write!(f, "Sampler Active Toggle"),
            ControllableParameter::SamplerMasterVolume => write!(f, "Sampler Master Volume"),
            ControllableParameter::InputToggleArm => write!(f, "Input Arm Toggle"),
            ControllableParameter::InputToggleMonitor => write!(f, "Input Monitor Toggle"),
            ControllableParameter::TransportTogglePlay => write!(f, "Transport Play/Stop"),
            ControllableParameter::TransportToggleMuteAll => write!(f, "Transport Mute All"),
            ControllableParameter::TransportClearAll => write!(f, "Transport Clear All"),
            ControllableParameter::TransportToggleRecord => write!(f, "Transport Record Toggle"),
            ControllableParameter::MasterVolume => write!(f, "Master Volume"),
            ControllableParameter::LimiterThreshold => write!(f, "Limiter Threshold"),
            ControllableParameter::Fx(id) => {
                // Handle the special case for Wet/Dry mix to prevent overflow.
                if id.component_index == usize::MAX {
                    write!(f, "FX {}:{}", id.point, id.param_name.as_str())
                } else {
                    write!(
                        f,
                        "FX {}:C{}:{}",
                        id.point,
                        id.component_index + 1,
                        id.param_name.as_str()
                    )
                }
            }
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(default)]
pub struct AppSettings {
    pub host_name: Option<String>,
    pub midi_port_names: Vec<String>,
    pub midi_channel: u8,
    pub input_device: Option<String>,
    pub output_device: Option<String>,
    pub sample_rate: Option<u32>,
    pub buffer_size: Option<u32>,
    pub input_latency_compensation_ms: f32,
    pub last_sampler_kit: Option<PathBuf>,
    pub last_synth_preset: Option<PathBuf>,
    pub last_theme: Option<PathBuf>,
    pub bpm_rounding: bool,
    pub midi_mappings: BTreeMap<FullMidiControlId, ControllableParameter>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            host_name: None,
            midi_port_names: Vec::new(),
            midi_channel: 0,
            input_device: None,
            output_device: None,
            sample_rate: None,
            buffer_size: None,
            input_latency_compensation_ms: 5.0,
            last_sampler_kit: None,
            last_synth_preset: None,
            last_theme: None,
            bpm_rounding: false,
            midi_mappings: BTreeMap::new(),
        }
    }
}

pub fn get_config_dir() -> Option<PathBuf> {
    if let Ok(exe_path) = env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let app_settings_dir = exe_dir.join("AppSettings");

            // Ensure main directory and subdirectories exist
            for dir in [
                &app_settings_dir,
                &app_settings_dir.join("Samples"),
                &app_settings_dir.join("SynthPresets"),
                &app_settings_dir.join("Kits"),
                &app_settings_dir.join("Themes"),
                &app_settings_dir.join("LiveRecordings"),
                &app_settings_dir.join("Sessions"),
                &app_settings_dir.join("FX"),
            ] {
                if !dir.exists() {
                    if let Err(e) = fs::create_dir_all(dir) {
                        eprintln!("Failed to create directory at {}: {}", dir.display(), e);
                        return None;
                    }
                }
            }
            return Some(app_settings_dir);
        }
    }
    eprintln!("Could not determine application directory.");
    None
}

pub fn save_settings(settings: &mut AppSettings) {
    if let Some(dir) = get_config_dir() {
        let path = dir.join("settings.json");
        match serde_json::to_string_pretty(settings) {
            Ok(json_string) => {
                if let Err(e) = fs::write(&path, json_string) {
                    eprintln!("Failed to write settings to {}: {}", path.display(), e);
                }
            }
            Err(e) => {
                eprintln!("Failed to serialize settings: {}", e);
            }
        }
    }
}

pub fn load_settings() -> AppSettings {
    if let Some(dir) = get_config_dir() {
        let path = dir.join("settings.json");
        if path.exists() {
            return match fs::read_to_string(&path) {
                Ok(json_string) => match serde_json::from_str::<AppSettings>(&json_string) {
                    Ok(settings) => {
                        // The BTreeMap is now deserialized directly.
                        settings
                    }
                    Err(e) => {
                        eprintln!("Failed to parse settings file, using defaults. Error: {}", e);
                        AppSettings::default()
                    }
                },
                Err(e) => {
                    eprintln!("Failed to read settings file, using defaults. Error: {}", e);
                    AppSettings::default()
                }
            };
        }
    }
    AppSettings::default()
}