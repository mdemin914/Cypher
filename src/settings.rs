// src/settings.rs

use serde::{de, Deserialize, Deserializer, Serialize};
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::PathBuf;

#[derive(
    Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord,
)]
pub struct MidiControlId {
    pub channel: u8,
    pub cc: u8,
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
        }
    }
}

// **THE FIX IS HERE**: A custom deserializer to handle old and new settings files.
fn deserialize_midi_mappings<'de, D>(
    deserializer: D,
) -> Result<Vec<(MidiControlId, ControllableParameter)>, D::Error>
where
    D: Deserializer<'de>,
{
    struct MidiMappingsVisitor;

    impl<'de> de::Visitor<'de> for MidiMappingsVisitor {
        type Value = Vec<(MidiControlId, ControllableParameter)>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a sequence (array) or an empty map")
        }

        // Handles the NEW, correct format: `[...]`
        fn visit_seq<S>(self, seq: S) -> Result<Self::Value, S::Error>
        where
            S: de::SeqAccess<'de>,
        {
            Deserialize::deserialize(de::value::SeqAccessDeserializer::new(seq))
        }

        // Handles the OLD, incorrect format: `{}`
        fn visit_map<M>(self, _map: M) -> Result<Self::Value, M::Error>
        where
            M: de::MapAccess<'de>,
        {
            // If we find a map, we know it's the old, invalid format.
            // We can't parse its contents, so we gracefully return an empty Vec.
            // The next time settings are saved, it will be in the correct format.
            Ok(Vec::new())
        }
    }

    deserializer.deserialize_any(MidiMappingsVisitor)
}


#[derive(Serialize, Deserialize, Debug)]
#[serde(default)]
pub struct AppSettings {
    pub host_name: Option<String>,
    pub midi_port_name: Option<String>,
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

    #[serde(skip)]
    pub midi_mappings: BTreeMap<MidiControlId, ControllableParameter>,

    #[serde(rename = "midi_mappings")]
    #[serde(deserialize_with = "deserialize_midi_mappings")]
    pub midi_mappings_vec: Vec<(MidiControlId, ControllableParameter)>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            host_name: None,
            midi_port_name: None,
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
            midi_mappings_vec: Vec::new(),
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
    // Before saving, ensure the Vec is up-to-date with the live BTreeMap.
    settings.midi_mappings_vec = settings.midi_mappings.iter().map(|(k, v)| (*k, *v)).collect();

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
                    Ok(mut settings) => {
                        // After loading, convert the Vec back into the BTreeMap for app use.
                        settings.midi_mappings = settings.midi_mappings_vec.iter().cloned().collect();
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