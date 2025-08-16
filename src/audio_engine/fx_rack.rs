// FILE: src\audio_engine\fx_rack.rs
// ==================================

use crate::fx;
use crate::fx_components;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

const PARAM_SCALER: f32 = 1_000_000.0;

/// Manages and processes a chain of DSP components with modulation.
pub struct FxRack {
    components: Vec<Box<dyn fx_components::DspComponent>>,
    mod_routings: Vec<fx::ModulationRoutingData>,
    wet_dry_mix: Arc<AtomicU32>, // Now an atomic for real-time control
    mod_outputs: Vec<f32>,       // Buffer to store current mod outputs
}

impl FxRack {
    /// Creates a new FxRack from a preset "recipe".
    pub fn new(preset: &fx::FxPreset, wet_dry_mix: Arc<AtomicU32>, sample_rate: f32) -> Self {
        let mut components: Vec<Box<dyn fx_components::DspComponent>> = Vec::new();
        let mut mod_routings = Vec::new();

        for link in &preset.chain {
            let component: Box<dyn fx_components::DspComponent> = match &link.params {
                fx_components::ComponentParams::Gain(p) => {
                    Box::new(fx_components::Gain::new(p.clone()))
                }
                fx_components::ComponentParams::Delay(p) => {
                    Box::new(fx_components::DelayLine::new(2000.0, sample_rate, p.clone()))
                }
                fx_components::ComponentParams::Filter(p) => {
                    Box::new(fx_components::Filter::new(sample_rate, p.clone()))
                }
                fx_components::ComponentParams::Lfo(p) => {
                    Box::new(fx_components::Lfo::new(sample_rate, p.clone()))
                }
                fx_components::ComponentParams::EnvelopeFollower(p) => {
                    Box::new(fx_components::EnvelopeFollower::new(sample_rate, p.clone()))
                }
                fx_components::ComponentParams::Waveshaper(p) => {
                    Box::new(fx_components::Waveshaper::new(p.clone()))
                }
                fx_components::ComponentParams::Quantizer(p) => {
                    Box::new(fx_components::Quantizer::new(p.clone()))
                }
                fx_components::ComponentParams::Reverb(p) => {
                    Box::new(fx_components::Reverb::new(sample_rate, p.clone()))
                }
                fx_components::ComponentParams::Flanger(p) => {
                    Box::new(fx_components::Flanger::new(sample_rate, p.clone()))
                }
                fx_components::ComponentParams::Formant(p) => {
                    Box::new(fx_components::Formant::new(sample_rate, p.clone()))
                }
            };
            components.push(component);
        }

        // Collect all modulations from all links in the chain
        for link in &preset.chain {
            mod_routings.extend_from_slice(&link.modulations);
        }

        Self {
            mod_outputs: vec![0.0; components.len()],
            components,
            mod_routings,
            wet_dry_mix, // Use the persistent atomic passed in
        }
    }

    /// Processes an entire audio buffer using a two-pass system for modulation.
    pub fn process_buffer(&mut self, buffer: &mut [f32]) {
        let wet_dry_mix_u32 = self.wet_dry_mix.load(Ordering::Relaxed);
        let wet_mix = wet_dry_mix_u32 as f32 / PARAM_SCALER;

        if wet_mix < 1e-9 && self.components.is_empty() {
            return; // Optimization: If 100% dry and no components, do nothing.
        }

        let dry_mix = 1.0 - wet_mix;

        for sample in buffer.iter_mut() {
            let dry_sample = *sample;

            let fx_chain_input = dry_sample * wet_mix;
            let mut wet_output = fx_chain_input;

            for (i, component) in self.components.iter_mut().enumerate() {
                self.mod_outputs[i] = component.get_mod_output(dry_sample);
            }

            for (i, component) in self.components.iter_mut().enumerate() {
                let mut mods = BTreeMap::new();
                for route in &self.mod_routings {
                    if route.target_component_index == i {
                        let mod_signal =
                            self.mod_outputs[route.source_component_index] * route.amount;
                        *mods
                            .entry(route.target_parameter_name.clone())
                            .or_insert(0.0) += mod_signal;
                    }
                }
                wet_output = component.process_audio(wet_output, &mods);
            }
            *sample = (dry_sample * dry_mix) + wet_output;
        }
    }
}