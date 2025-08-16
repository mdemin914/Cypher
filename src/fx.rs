// src/fx.rs

//! Defines the data structures for the modular FX system.
//! These structs are designed to be serialized to and from a format like JSON,
//! representing a user-created effect chain "recipe".

use crate::fx_components;
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::sync::atomic::Ordering;

/// Uniquely identifies a location in the audio pipeline where an FX Rack can be inserted.
/// This is used by the host application to manage the FX chains.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum InsertionPoint {
    Looper(usize),
    Synth(usize),
    Sampler,
    Input,
    Master,
    Atmo,
}

// Custom implementation to convert the enum to a string for JSON map keys.
impl Serialize for InsertionPoint {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let s = match self {
            InsertionPoint::Looper(i) => format!("Looper_{}", i),
            InsertionPoint::Synth(i) => format!("Synth_{}", i),
            InsertionPoint::Sampler => "Sampler".to_string(),
            InsertionPoint::Input => "Input".to_string(),
            InsertionPoint::Master => "Master".to_string(),
            InsertionPoint::Atmo => "Atmo".to_string(),
        };
        serializer.serialize_str(&s)
    }
}

// Custom implementation to parse the string from a JSON map key back into the enum.
impl<'de> Deserialize<'de> for InsertionPoint {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        if let Some((prefix, suffix)) = s.split_once('_') {
            let index = suffix.parse::<usize>().map_err(de::Error::custom)?;
            match prefix {
                "Looper" => Ok(InsertionPoint::Looper(index)),
                "Synth" => Ok(InsertionPoint::Synth(index)),
                _ => Err(de::Error::custom(format!(
                    "Unknown insertion point prefix: {}",
                    prefix
                ))),
            }
        } else {
            match s.as_str() {
                "Sampler" => Ok(InsertionPoint::Sampler),
                "Input" => Ok(InsertionPoint::Input),
                "Master" => Ok(InsertionPoint::Master),
                "Atmo" => Ok(InsertionPoint::Atmo),
                _ => Err(de::Error::custom(format!("Unknown insertion point: {}", s))),
            }
        }
    }
}

// Implement the Display trait for user-friendly names in the UI.
impl fmt::Display for InsertionPoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InsertionPoint::Looper(i) => write!(f, "Looper {}", i + 1),
            InsertionPoint::Synth(i) => write!(f, "Synth Engine {}", i + 1),
            InsertionPoint::Sampler => write!(f, "Sampler"),
            InsertionPoint::Input => write!(f, "Audio Input"),
            InsertionPoint::Master => write!(f, "Master Output"),
            InsertionPoint::Atmo => write!(f, "Atmosphere"),
        }
    }
}

/// The different types of core DSP components a user can add to a chain.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FxComponentType {
    Gain,
    Delay,
    Filter,
    Lfo,
    EnvelopeFollower,
    Waveshaper,
    Quantizer,
    Reverb,
    Flanger,
    Formant,
}

/// Describes how one component in the chain modulates a parameter of another.
/// This remains a simple data struct as it's only used for UI and setup, not on the audio thread directly.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(default)]
pub struct ModulationRoutingData {
    /// The index in the `chain` of the component providing the modulation signal (e.g., an LFO).
    pub source_component_index: usize,
    /// The index in the `chain` of the component whose parameter is being modulated.
    pub target_component_index: usize,
    /// The name of the parameter on the target component to be modulated (e.g., "frequency_hz").
    pub target_parameter_name: String,
    /// The depth and polarity of the modulation, from -1.0 to 1.0.
    pub amount: f32,
}

impl Default for ModulationRoutingData {
    fn default() -> Self {
        Self {
            source_component_index: 0,
            target_component_index: 0,
            target_parameter_name: "".to_string(),
            amount: 0.0,
        }
    }
}

/// Represents a single link or "pedal" in the effects chain.
/// This is the UI-thread representation.
#[derive(Debug, Clone)]
pub struct FxChainLink {
    /// The type of DSP component for this link.
    pub component_type: FxComponentType,
    /// A list of modulation routings originating from this component.
    pub modulations: Vec<ModulationRoutingData>,
    /// The shared, atomic parameters for this component instance.
    pub params: fx_components::ComponentParams,
}

impl FxChainLink {
    pub fn new(component_type: FxComponentType) -> Self {
        Self {
            component_type,
            modulations: Vec::new(),
            params: fx_components::ComponentParams::new(component_type),
        }
    }
}

/// A serializable version of `FxChainLink` for saving/loading presets.
/// It stores parameter values directly instead of the atomic `Arc`s.
#[derive(Serialize, Deserialize, Debug, Clone)]
struct SerializableFxChainLink {
    component_type: FxComponentType,
    modulations: Vec<ModulationRoutingData>,
    bypassed: bool,
    parameters: serde_json::Value,
}

/// The top-level structure for an FX Preset file.
/// This represents the entire effect chain that can be saved and loaded.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct FxPreset {
    pub name: String,
    pub author: String,
    // NOTE: wet_dry_mix has been removed from here. It's now managed per-InsertionPoint.
    #[serde(
        rename = "chain",
        serialize_with = "serialize_chain",
        deserialize_with = "deserialize_chain"
    )]
    pub chain: Vec<FxChainLink>,
}

impl Default for FxPreset {
    fn default() -> Self {
        Self {
            name: "New Preset".to_string(),
            author: "".to_string(),
            chain: Vec::new(),
        }
    }
}

// --- Custom Serialization and Deserialization Logic ---

fn serialize_chain<S>(chain: &[FxChainLink], serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    use fx_components::*;
    use serde::ser::SerializeSeq;

    let mut seq = serializer.serialize_seq(Some(chain.len()))?;

    for link in chain {
        let serializable_link = SerializableFxChainLink {
            component_type: link.component_type,
            modulations: link.modulations.clone(),
            bypassed: link.params.bypassed().load(Ordering::Relaxed),
            parameters: match &link.params {
                ComponentParams::Gain(p) => {
                    let gain_db =
                        (p.gain_db.load(Ordering::Relaxed) as f32 / gain::DB_SCALER) - gain::DB_OFFSET;
                    serde_json::json!({ "gain_db": gain_db })
                }
                ComponentParams::Delay(p) => {
                    let time_ms = p.time_ms.load(Ordering::Relaxed) as f32 / delay::PARAM_SCALER;
                    let feedback = p.feedback.load(Ordering::Relaxed) as f32 / delay::PARAM_SCALER;
                    let damping = p.damping.load(Ordering::Relaxed) as f32 / delay::PARAM_SCALER;
                    serde_json::json!({ "time_ms": time_ms, "feedback": feedback, "damping": damping })
                }
                ComponentParams::Filter(p) => {
                    let mode = p.mode.load(Ordering::Relaxed);
                    let frequency_hz =
                        p.frequency_hz.load(Ordering::Relaxed) as f32 / filter::PARAM_SCALER;
                    let resonance =
                        p.resonance.load(Ordering::Relaxed) as f32 / filter::PARAM_SCALER;
                    serde_json::json!({ "mode": mode, "frequency_hz": frequency_hz, "resonance": resonance })
                }
                ComponentParams::Lfo(p) => {
                    let waveform = p.waveform.load(Ordering::Relaxed);
                    let frequency_hz =
                        p.frequency_hz.load(Ordering::Relaxed) as f32 / lfo::PARAM_SCALER;
                    serde_json::json!({ "waveform": waveform, "frequency_hz": frequency_hz })
                }
                ComponentParams::EnvelopeFollower(p) => {
                    let attack_ms = p.attack_ms.load(Ordering::Relaxed) as f32
                        / envelope_follower::PARAM_SCALER;
                    let release_ms = p.release_ms.load(Ordering::Relaxed) as f32
                        / envelope_follower::PARAM_SCALER;
                    let sensitivity = p.sensitivity.load(Ordering::Relaxed) as f32
                        / envelope_follower::PARAM_SCALER;
                    serde_json::json!({ "attack_ms": attack_ms, "release_ms": release_ms, "sensitivity": sensitivity })
                }
                ComponentParams::Waveshaper(p) => {
                    let mode = p.mode.load(Ordering::Relaxed);
                    let drive_db =
                        p.drive_db.load(Ordering::Relaxed) as f32 / waveshaper::DB_SCALER;
                    serde_json::json!({ "mode": mode, "drive_db": drive_db })
                }
                ComponentParams::Quantizer(p) => {
                    let bit_depth =
                        p.bit_depth.load(Ordering::Relaxed) as f32 / quantizer::PARAM_SCALER;
                    let downsample =
                        p.downsample.load(Ordering::Relaxed) as f32 / quantizer::PARAM_SCALER;
                    serde_json::json!({ "bit_depth": bit_depth, "downsample": downsample })
                }
                ComponentParams::Reverb(p) => {
                    let size = p.size.load(Ordering::Relaxed) as f32 / reverb::PARAM_SCALER;
                    let decay = p.decay.load(Ordering::Relaxed) as f32 / reverb::PARAM_SCALER;
                    let damping = p.damping.load(Ordering::Relaxed) as f32 / reverb::PARAM_SCALER;
                    serde_json::json!({ "size": size, "decay": decay, "damping": damping })
                }
                ComponentParams::Flanger(p) => {
                    let rate_hz = p.rate_hz.load(Ordering::Relaxed) as f32 / flanger::PARAM_SCALER;
                    let depth_ms =
                        p.depth_ms.load(Ordering::Relaxed) as f32 / flanger::PARAM_SCALER;
                    let feedback = (p.feedback.load(Ordering::Relaxed) as f32
                        / flanger::PARAM_SCALER)
                        - flanger::FEEDBACK_OFFSET;
                    serde_json::json!({ "rate_hz": rate_hz, "depth_ms": depth_ms, "feedback": feedback })
                }
                ComponentParams::Formant(p) => {
                    let character = (p.character.load(Ordering::Relaxed) as f32 / formant::PARAM_SCALER) - formant::CHARACTER_OFFSET;
                    let resonance = p.resonance.load(Ordering::Relaxed) as f32 / formant::PARAM_SCALER;
                    serde_json::json!({ "character": character, "resonance": resonance })
                }
            },
        };
        seq.serialize_element(&serializable_link)?;
    }
    seq.end()
}

fn deserialize_chain<'de, D>(deserializer: D) -> Result<Vec<FxChainLink>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use fx_components::*;
    use serde::de::Error;

    let s_chain: Vec<SerializableFxChainLink> = Vec::deserialize(deserializer)?;
    let mut chain = Vec::with_capacity(s_chain.len());

    for s_link in s_chain {
        let params = ComponentParams::new(s_link.component_type);
        params.bypassed().store(s_link.bypassed, Ordering::Relaxed);

        let p_map = s_link
            .parameters
            .as_object()
            .ok_or_else(|| D::Error::custom("Parameters must be a map"))?;

        match &params {
            ComponentParams::Gain(p) => {
                let gain_db = p_map.get("gain_db").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                p.gain_db.store(
                    ((gain_db + gain::DB_OFFSET) * gain::DB_SCALER) as u32,
                    Ordering::Relaxed,
                );
            }
            ComponentParams::Delay(p) => {
                let time_ms =
                    p_map.get("time_ms").and_then(|v| v.as_f64()).unwrap_or(250.0) as f32;
                let feedback =
                    p_map.get("feedback").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                let damping = p_map.get("damping").and_then(|v| v.as_f64()).unwrap_or(0.5) as f32;
                p.time_ms
                    .store((time_ms * delay::PARAM_SCALER) as u32, Ordering::Relaxed);
                p.feedback
                    .store((feedback * delay::PARAM_SCALER) as u32, Ordering::Relaxed);
                p.damping
                    .store((damping * delay::PARAM_SCALER) as u32, Ordering::Relaxed);
            }
            ComponentParams::Filter(p) => {
                let mode = p_map.get("mode").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                let frequency_hz =
                    p_map.get("frequency_hz").and_then(|v| v.as_f64()).unwrap_or(1000.0) as f32;
                let resonance =
                    p_map.get("resonance").and_then(|v| v.as_f64()).unwrap_or(0.1) as f32;
                p.mode.store(mode, Ordering::Relaxed);
                p.frequency_hz.store(
                    (frequency_hz * filter::PARAM_SCALER) as u32,
                    Ordering::Relaxed,
                );
                p.resonance.store(
                    (resonance * filter::PARAM_SCALER) as u32,
                    Ordering::Relaxed,
                );
            }
            ComponentParams::Lfo(p) => {
                let waveform = p_map.get("waveform").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                let frequency_hz =
                    p_map.get("frequency_hz").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
                p.waveform.store(waveform, Ordering::Relaxed);
                p.frequency_hz.store(
                    (frequency_hz * lfo::PARAM_SCALER) as u32,
                    Ordering::Relaxed,
                );
            }
            ComponentParams::EnvelopeFollower(p) => {
                let attack_ms =
                    p_map.get("attack_ms").and_then(|v| v.as_f64()).unwrap_or(10.0) as f32;
                let release_ms =
                    p_map.get("release_ms").and_then(|v| v.as_f64()).unwrap_or(150.0) as f32;
                let sensitivity =
                    p_map.get("sensitivity").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
                p.attack_ms.store(
                    (attack_ms * envelope_follower::PARAM_SCALER) as u32,
                    Ordering::Relaxed,
                );
                p.release_ms.store(
                    (release_ms * envelope_follower::PARAM_SCALER) as u32,
                    Ordering::Relaxed,
                );
                p.sensitivity.store(
                    (sensitivity * envelope_follower::PARAM_SCALER) as u32,
                    Ordering::Relaxed,
                );
            }
            ComponentParams::Waveshaper(p) => {
                let mode = p_map.get("mode").and_then(|v| v.as_u64()).unwrap_or(1) as u32; // Default to Saturation
                let drive_db = p_map.get("drive_db").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                p.mode.store(mode, Ordering::Relaxed);
                p.drive_db.store(
                    (drive_db * waveshaper::DB_SCALER) as u32,
                    Ordering::Relaxed,
                );
            }
            ComponentParams::Quantizer(p) => {
                let bit_depth =
                    p_map.get("bit_depth").and_then(|v| v.as_f64()).unwrap_or(16.0) as f32;
                let downsample =
                    p_map.get("downsample").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
                p.bit_depth.store(
                    (bit_depth * quantizer::PARAM_SCALER) as u32,
                    Ordering::Relaxed,
                );
                p.downsample.store(
                    (downsample * quantizer::PARAM_SCALER) as u32,
                    Ordering::Relaxed,
                );
            }
            ComponentParams::Reverb(p) => {
                let size = p_map.get("size").and_then(|v| v.as_f64()).unwrap_or(0.7) as f32;
                let decay = p_map.get("decay").and_then(|v| v.as_f64()).unwrap_or(0.8) as f32;
                let damping = p_map.get("damping").and_then(|v| v.as_f64()).unwrap_or(0.5) as f32;
                p.size
                    .store((size * reverb::PARAM_SCALER) as u32, Ordering::Relaxed);
                p.decay
                    .store((decay * reverb::PARAM_SCALER) as u32, Ordering::Relaxed);
                p.damping
                    .store((damping * reverb::PARAM_SCALER) as u32, Ordering::Relaxed);
            }
            ComponentParams::Flanger(p) => {
                let rate_hz = p_map.get("rate_hz").and_then(|v| v.as_f64()).unwrap_or(0.2) as f32;
                let depth_ms =
                    p_map.get("depth_ms").and_then(|v| v.as_f64()).unwrap_or(5.0) as f32;
                let feedback =
                    p_map.get("feedback").and_then(|v| v.as_f64()).unwrap_or(0.85) as f32;
                p.rate_hz
                    .store((rate_hz * flanger::PARAM_SCALER) as u32, Ordering::Relaxed);
                p.depth_ms
                    .store((depth_ms * flanger::PARAM_SCALER) as u32, Ordering::Relaxed);
                p.feedback.store(
                    ((feedback + flanger::FEEDBACK_OFFSET) * flanger::PARAM_SCALER) as u32,
                    Ordering::Relaxed,
                );
            }
            ComponentParams::Formant(p) => {
                let character = p_map.get("character").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                let resonance = p_map.get("resonance").and_then(|v| v.as_f64()).unwrap_or(0.7) as f32;
                p.character.store(((character + formant::CHARACTER_OFFSET) * formant::PARAM_SCALER) as u32, Ordering::Relaxed);
                p.resonance.store((resonance * formant::PARAM_SCALER) as u32, Ordering::Relaxed);
            }
        }

        chain.push(FxChainLink {
            component_type: s_link.component_type,
            modulations: s_link.modulations,
            params,
        });
    }

    Ok(chain)
}