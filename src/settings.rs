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

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FullMidiControlId {
    pub port_name: String,
    pub channel: u8,
    pub cc: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FullMidiNoteId {
    pub port_name: String,
    pub channel: u8,
    pub note: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FullMidiIdentifier {
    ControlChange(FullMidiControlId),
    Note(FullMidiNoteId),
}

impl Serialize for FullMidiIdentifier {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let s = match self {
            FullMidiIdentifier::ControlChange(id) => {
                format!("cc|{}|{}|{}", id.port_name, id.channel, id.cc)
            }
            FullMidiIdentifier::Note(id) => {
                format!("note|{}|{}|{}", id.port_name, id.channel, id.note)
            }
        };
        serializer.serialize_str(&s)
    }
}

impl<'de> Deserialize<'de> for FullMidiIdentifier {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct FullMidiIdentifierVisitor;

        impl<'de> de::Visitor<'de> for FullMidiIdentifierVisitor {
            type Value = FullMidiIdentifier;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str(
                    "a string in the format 'cc|port|chan|val' or 'note|port|chan|val'",
                )
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                let mut parts = value.splitn(4, '|');
                let id_type = parts
                    .next()
                    .ok_or_else(|| E::custom("missing identifier type"))?;

                let port_name = parts
                    .next()
                    .ok_or_else(|| E::custom("missing port name"))?
                    .to_string();
                let channel_str = parts.next().ok_or_else(|| E::custom("missing channel"))?;
                let value_str = parts.next().ok_or_else(|| E::custom("missing value"))?;

                if parts.next().is_some() {
                    return Err(E::custom(format!(
                        "expected 4 parts separated by '|', found more in '{}'",
                        value
                    )));
                }

                let channel = channel_str.parse::<u8>().map_err(E::custom)?;

                match id_type {
                    "cc" => {
                        let cc = value_str.parse::<u8>().map_err(E::custom)?;
                        Ok(FullMidiIdentifier::ControlChange(FullMidiControlId {
                            port_name,
                            channel,
                            cc,
                        }))
                    }
                    "note" => {
                        let note = value_str.parse::<u8>().map_err(E::custom)?;
                        Ok(FullMidiIdentifier::Note(FullMidiNoteId {
                            port_name,
                            channel,
                            note,
                        }))
                    }
                    _ => Err(E::custom(format!("unknown identifier type '{}'", id_type))),
                }
            }
        }

        deserializer.deserialize_str(FullMidiIdentifierVisitor)
    }
}

// NEW: An enum to define how a MIDI control should be interpreted.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MidiControlMode {
    #[default]
    Absolute,
    Relative,
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
    ToggleSynthEditor,
    SamplerToggleActive,
    SamplerMasterVolume,
    ToggleSamplerEditor,

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

    // FX Parameters
    Fx(FxParamIdentifier),
    FxFocusedWetDry,
    FxFocusedPresetChange, // New
    ToggleFxEditor(fx::InsertionPoint),

    // Atmo Engine
    AtmoMasterVolume,
    AtmoXY(u8), // 0 for X, 1 for Y
    AtmoLayerVolume(usize),
    ToggleAtmoEditor,

    // Metronome
    MetronomeVolume,
    MetronomePitch,
    MetronomeToggleMute,
}

impl ControllableParameter {
    pub fn is_continuous(&self) -> bool {
        matches!(
            self,
            ControllableParameter::MixerVolume(_)
                | ControllableParameter::SynthMasterVolume
                | ControllableParameter::SamplerMasterVolume
                | ControllableParameter::MasterVolume
                | ControllableParameter::LimiterThreshold
                | ControllableParameter::Fx(_)
                | ControllableParameter::FxFocusedWetDry
                | ControllableParameter::FxFocusedPresetChange
                | ControllableParameter::AtmoMasterVolume
                | ControllableParameter::AtmoXY(_)
                | ControllableParameter::AtmoLayerVolume(_)
                | ControllableParameter::MetronomeVolume
                | ControllableParameter::MetronomePitch
        )
    }
}

/// Uniquely identifies a single parameter within a specific FX rack.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FxParamIdentifier {
    pub point: fx::InsertionPoint,
    pub component_index: usize,
    pub param_name: FxParamName,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FxParamName {
    Bypass,
    WetDry,
    GainDb,
    TimeMs,
    Feedback,
    Damping,
    Mode,
    FrequencyHz,
    Resonance,
    Waveform,
    AttackMs,
    ReleaseMs,
    DriveDb,
    BitDepth,
    Downsample,
    Size,
    Decay,
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
            ControllableParameter::ToggleSynthEditor => write!(f, "Toggle Synth Editor"),
            ControllableParameter::SamplerToggleActive => write!(f, "Sampler Active Toggle"),
            ControllableParameter::SamplerMasterVolume => write!(f, "Sampler Master Volume"),
            ControllableParameter::ToggleSamplerEditor => write!(f, "Toggle Sampler Editor"),
            ControllableParameter::InputToggleArm => write!(f, "Input Arm Toggle"),
            ControllableParameter::InputToggleMonitor => write!(f, "Input Monitor Toggle"),
            ControllableParameter::TransportTogglePlay => write!(f, "Transport Play/Stop"),
            ControllableParameter::TransportToggleMuteAll => write!(f, "Transport Mute All"),
            ControllableParameter::TransportClearAll => write!(f, "Transport Clear All"),
            ControllableParameter::TransportToggleRecord => write!(f, "Transport Record Toggle"),
            ControllableParameter::MasterVolume => write!(f, "Master Volume"),
            ControllableParameter::LimiterThreshold => write!(f, "Limiter Threshold"),
            ControllableParameter::Fx(id) => {
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
            ControllableParameter::FxFocusedWetDry => write!(f, "Focused FX Rack Dry/Wet"),
            ControllableParameter::FxFocusedPresetChange => write!(f, "Focused FX Preset Change"), // New
            ControllableParameter::ToggleFxEditor(point) => {
                write!(f, "Toggle FX Editor ({})", point)
            }
            ControllableParameter::AtmoMasterVolume => write!(f, "Atmo Master Volume"),
            ControllableParameter::AtmoXY(axis) => {
                write!(f, "Atmo Pad {}", if *axis == 0 { "X" } else { "Y" })
            }
            ControllableParameter::AtmoLayerVolume(i) => write!(f, "Atmo Layer {} Volume", i + 1),
            ControllableParameter::ToggleAtmoEditor => write!(f, "Toggle Atmosphere Editor"),
            ControllableParameter::MetronomeVolume => write!(f, "Metronome Volume"),
            ControllableParameter::MetronomePitch => write!(f, "Metronome Pitch"),
            ControllableParameter::MetronomeToggleMute => write!(f, "Metronome Mute Toggle"),
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(default)]
pub struct AppSettings {
    pub host_name: Option<String>,
    pub midi_port_names: Vec<String>,
    pub audio_note_channel: u8,
    pub midi_device_control_channels: BTreeMap<String, u8>,
    pub input_device: Option<String>,
    pub output_device: Option<String>,
    pub sample_rate: Option<u32>,
    pub buffer_size: Option<u32>,
    pub input_latency_compensation_ms: f32,
    pub last_sampler_kit: Option<PathBuf>,
    pub last_synth_preset: Option<PathBuf>,
    pub last_theme: Option<PathBuf>,
    pub bpm_rounding: bool,
    pub relative_encoder_multiplier: f32,
    pub midi_mappings: BTreeMap<FullMidiIdentifier, ControllableParameter>,
    pub midi_mapping_modes: BTreeMap<FullMidiIdentifier, MidiControlMode>,
    pub midi_mapping_inversions: BTreeMap<FullMidiIdentifier, bool>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            host_name: None,
            midi_port_names: Vec::new(),
            audio_note_channel: 0,
            midi_device_control_channels: BTreeMap::new(),
            input_device: None,
            output_device: None,
            sample_rate: None,
            buffer_size: None,
            input_latency_compensation_ms: 5.0,
            last_sampler_kit: None,
            last_synth_preset: None,
            last_theme: None,
            bpm_rounding: false,
            relative_encoder_multiplier: 1.0,
            midi_mappings: BTreeMap::new(),
            midi_mapping_modes: BTreeMap::new(),
            midi_mapping_inversions: BTreeMap::new(),
        }
    }
}

pub fn get_config_dir() -> Option<PathBuf> {
    if let Ok(exe_path) = env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let app_settings_dir = exe_dir.join("AppSettings");
            for dir in [
                &app_settings_dir,
                &app_settings_dir.join("Samples"),
                &app_settings_dir.join("SynthPresets"),
                &app_settings_dir.join("Kits"),
                &app_settings_dir.join("Themes"),
                &app_settings_dir.join("LiveRecordings"),
                &app_settings_dir.join("Sessions"),
                &app_settings_dir.join("FX"),
                &app_settings_dir.join("Atmospheres"),
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
    // Optimization: remove default modes before saving to keep the json clean.
    settings
        .midi_mapping_modes
        .retain(|_, &mut mode| mode != MidiControlMode::default());

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
                        // Backwards compatibility for the old `midi_channel` field.
                        if let Ok(raw_value) =
                            serde_json::from_str::<serde_json::Value>(&json_string)
                        {
                            if raw_value.get("audio_note_channel").is_none() {
                                if let Some(old_channel) =
                                    raw_value.get("midi_channel").and_then(|v| v.as_u64())
                                {
                                    settings.audio_note_channel = old_channel as u8;
                                }
                            }
                        }
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