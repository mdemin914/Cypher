// src/fx_components/mod.rs

// Declare all component modules
pub mod delay;
pub mod envelope_follower;
pub mod filter;
pub mod flanger;
pub mod formant;
pub mod gain;
pub mod lfo;
pub mod quantizer;
pub mod reverb;
pub mod waveshaper;

// Publicly export the primary struct and the new Params struct from each module
pub use delay::{DelayLine, Params as DelayParams};
pub use envelope_follower::{EnvelopeFollower, Params as EnvelopeFollowerParams};
pub use filter::{Filter, Params as FilterParams};
pub use flanger::{Flanger, Params as FlangerParams};
pub use formant::{Formant, Params as FormantParams};
pub use gain::{Gain, Params as GainParams};
pub use lfo::{Lfo, Params as LfoParams};
pub use quantizer::{Quantizer, Params as QuantizerParams};
pub use reverb::{Reverb, Params as ReverbParams};
pub use waveshaper::{Waveshaper, Params as WaveshaperParams};

use crate::fx::FxComponentType;
use std::any::Any;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU32};
use std::sync::Arc;

/// A generic, clonable container for the shared atomic parameters of any DSP component.
/// This is held by the `FxChainLink` on the UI thread and cloned for the `FxRack` on the audio thread.
#[derive(Debug, Clone)]
pub enum ComponentParams {
    Gain(GainParams),
    Delay(DelayParams),
    Filter(FilterParams),
    Lfo(LfoParams),
    EnvelopeFollower(EnvelopeFollowerParams),
    Waveshaper(WaveshaperParams),
    Quantizer(QuantizerParams),
    Reverb(ReverbParams),
    Flanger(FlangerParams),
    Formant(FormantParams),
}

impl ComponentParams {
    /// Creates a new set of default parameters for a given component type.
    pub fn new(component_type: FxComponentType) -> Self {
        match component_type {
            FxComponentType::Gain => ComponentParams::Gain(GainParams::default()),
            FxComponentType::Delay => ComponentParams::Delay(DelayParams::default()),
            FxComponentType::Filter => ComponentParams::Filter(FilterParams::default()),
            FxComponentType::Lfo => ComponentParams::Lfo(LfoParams::default()),
            FxComponentType::EnvelopeFollower => {
                ComponentParams::EnvelopeFollower(EnvelopeFollowerParams::default())
            }
            FxComponentType::Waveshaper => {
                ComponentParams::Waveshaper(WaveshaperParams::default())
            }
            FxComponentType::Quantizer => ComponentParams::Quantizer(QuantizerParams::default()),
            FxComponentType::Reverb => ComponentParams::Reverb(ReverbParams::default()),
            FxComponentType::Flanger => ComponentParams::Flanger(FlangerParams::default()),
            FxComponentType::Formant => ComponentParams::Formant(FormantParams::default()),
        }
    }

    /// Returns the shared `bypassed` atomic bool for this component.
    pub fn bypassed(&self) -> Arc<AtomicBool> {
        match self {
            ComponentParams::Gain(p) => p.bypassed.clone(),
            ComponentParams::Delay(p) => p.bypassed.clone(),
            ComponentParams::Filter(p) => p.bypassed.clone(),
            ComponentParams::Lfo(p) => p.bypassed.clone(),
            ComponentParams::EnvelopeFollower(p) => p.bypassed.clone(),
            ComponentParams::Waveshaper(p) => p.bypassed.clone(),
            ComponentParams::Quantizer(p) => p.bypassed.clone(),
            ComponentParams::Reverb(p) => p.bypassed.clone(),
            ComponentParams::Flanger(p) => p.bypassed.clone(),
            ComponentParams::Formant(p) => p.bypassed.clone(),
        }
    }

    /// Retrieves a specific parameter's atomic value by its string name.
    /// This is used for MIDI mapping.
    pub fn get_param(&self, name: &str) -> Option<Arc<AtomicU32>> {
        match self {
            ComponentParams::Gain(p) => p.get_param(name),
            ComponentParams::Delay(p) => p.get_param(name),
            ComponentParams::Filter(p) => p.get_param(name),
            ComponentParams::Lfo(p) => p.get_param(name),
            ComponentParams::EnvelopeFollower(p) => p.get_param(name),
            ComponentParams::Waveshaper(p) => p.get_param(name),
            ComponentParams::Quantizer(p) => p.get_param(name),
            ComponentParams::Reverb(p) => p.get_param(name),
            ComponentParams::Flanger(p) => p.get_param(name),
            ComponentParams::Formant(p) => p.get_param(name),
        }
    }
}

/// A common interface for all audio processing components within an FXRack.
/// This has been refactored for the real-time atomic parameter system.
pub trait DspComponent: Send + Sync + Any {
    /// If this component is a modulator (LFO, EnvFollower), this advances its state
    /// and returns its current output value. For audio processors, this does nothing
    /// and should return 0.0. The input sample is for envelope followers.
    fn get_mod_output(&mut self, input_sample: f32) -> f32;

    /// Processes a single audio sample.
    /// Modulation from other components is passed in via the `mods` BTreeMap.
    fn process_audio(&mut self, input: f32, mods: &BTreeMap<String, f32>) -> f32;
}