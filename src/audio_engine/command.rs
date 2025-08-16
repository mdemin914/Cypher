// FILE: src\audio_engine\command.rs
// ==================================

use crate::atmo::AtmoScene;
use crate::fx;
use crate::mixer::MixerState;
use crate::sampler::SamplerPadFxSettings;
use crate::sampler_engine::NUM_SAMPLE_SLOTS;
use crate::settings;
use crate::synth::{AdsrSettings, EngineParamsUnion, LfoRateMode};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct MidiMessage {
    pub status: u8,
    pub data1: u8,
    pub data2: u8,
}

#[derive(Debug)]
pub enum AudioCommand {
    LooperPress(usize),
    ToggleLooperPlayback(usize),
    ClearLooper(usize),
    HalveTempo,
    DoubleTempo,
    SetTempoState { master_index: usize, multiplier: u32 },
    MidiMessage(MidiMessage),
    ActivateSynth,
    DeactivateSynth,
    SetSynthMode(usize, bool),
    SetAmpAdsr(usize, AdsrSettings),
    SetFilterAdsr(usize, AdsrSettings),
    ResetWavetables(usize),
    SetWavetable {
        engine_index: usize,
        slot_index: usize,
        audio_data: Arc<Vec<f32>>,
        name: String,
    },
    LoadSampleForSamplerSlot {
        engine_index: usize,
        slot_index: usize,
        audio_data: Arc<Vec<f32>>,
    },
    SetSamplerSettings {
        engine_index: usize,
        root_notes: [u8; NUM_SAMPLE_SLOTS],
        global_fine_tune_cents: f32,
        fade_out: f32,
    },
    ChangeEngineType {
        engine_index: usize,
        volume: Arc<AtomicU32>,
        peak_meter: Arc<AtomicU32>,
        params: EngineParamsUnion,
    },
    ToggleAudioInputArm,
    ToggleAudioInputMonitoring,
    ActivateSampler,
    DeactivateSampler,
    LoadSamplerSample {
        pad_index: usize,
        audio_data: Arc<Vec<f32>>,
    },
    ClearSample {
        pad_index: usize,
    },
    SetSamplerPadFx {
        pad_index: usize,
        settings: SamplerPadFxSettings,
    },
    SetMasterVolume(f32),
    SetLimiterThreshold(f32),
    ToggleLimiter,
    SetLimiterReleaseMode(LfoRateMode),
    SetLimiterReleaseMs(f32),
    SetLimiterReleaseSync(f32),
    PlayTransport,
    StopTransport,
    ClearAllAndPlay,
    ClearAll,
    StartOutputRecording,
    StopOutputRecording {
        output_path: PathBuf,
    },
    SaveSessionAudio {
        session_path: PathBuf,
    },
    LoadLoopAudio {
        looper_index: usize,
        path: PathBuf,
        original_sample_rate: u32,
        length_in_cycles: u32,
    },
    SetTransportLen(usize),
    SetMixerState(MixerState),
    SetMixerTrackVolume {
        track_index: usize,
        volume: f32,
    },
    SetMetronomeVolume(f32),
    SetMetronomePitch(f32),
    SetMetronomeAccentPitch(f32),
    ToggleMetronomeMute,
    ToggleSynth,
    SetSynthMasterVolume(f32),
    ToggleSampler,
    SetSamplerMasterVolume(f32),
    ToggleTransport,
    ToggleMuteAll,
    ToggleRecord,
    ToggleMixerMute(usize),
    ToggleMixerSolo(usize),

    // --- FX Commands ---
    LoadFxRack(fx::InsertionPoint, fx::FxPreset),
    ClearFxRack(fx::InsertionPoint),

    // --- Atmo Commands ---
    ClearAtmoLayer {
        scene_index: usize,
        layer_index: usize,
    },
    LoadAtmoLayer {
        scene_index: usize,
        layer_index: usize,
        samples: Vec<(PathBuf, u32)>,
    },
    SetAtmoScene {
        scene_index: usize,
        scene: AtmoScene,
    },
    // for relative encoder support
    AdjustParameterRelative {
        parameter: settings::ControllableParameter,
        delta: f32, // e.g., +0.01 for clockwise, -0.01 for counter-clockwise
    },
}