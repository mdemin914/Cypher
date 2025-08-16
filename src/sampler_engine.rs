// src/sampler_engine.rs
use crate::synth::{
    Adsr, AdsrSettings, Engine, Filter, FilterSettings, Lfo, LfoRateMode, LfoSettings,
    ModDestination, ModRouting, ModSource,
};
use crate::synth::{FastTanh, POW2_LUT};
use crate::wavetable_engine::{SaturationSettings, WavetableSet};
use egui::{epaint, lerp, Rect};
use rayon::prelude::*; // Import Rayon for parallel processing
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};

// --- New Structs for Multi-sampling ---

pub const NUM_SAMPLE_SLOTS: usize = 8;

/// Holds the audio data and settings for a single multi-sample slot.
#[derive(Clone, Default)]
struct SampleSlot {
    audio_data: Arc<Vec<f32>>,
    root_note: u8,
}

// A snapshot of all values that affect the sampler visualizer.
#[derive(PartialEq, Clone, Copy, Debug, Default)]
pub struct SamplerVisualizerSnapshot {
    pub final_filter_cutoff: u32,
    pub pitch_mod: u32,
    pub saturation_mod: u32,
    pub fade_out: u32,
    pub redraw_generation: u32, // <-- ADDED THIS to bust the cache
}

// --- Engine-Specific UI State ---
pub struct SamplerEngineState {
    pub amp_adsr: AdsrSettings,
    pub filter_adsr: AdsrSettings,
    pub filter_settings: Arc<RwLock<FilterSettings>>,
    pub lfo_settings: Arc<RwLock<LfoSettings>>,
    pub lfo2_settings: Arc<RwLock<LfoSettings>>,
    pub mod_matrix: Arc<RwLock<Vec<ModRouting>>>,
    pub saturation_settings: Arc<RwLock<SaturationSettings>>,
    pub is_polyphonic: bool,

    // Sampler specific (Multi-sample)
    pub sample_names: [String; NUM_SAMPLE_SLOTS],
    pub sample_paths: [Option<PathBuf>; NUM_SAMPLE_SLOTS],
    pub sample_data_for_ui: [Arc<RwLock<Vec<f32>>>; NUM_SAMPLE_SLOTS],
    pub root_notes: [u8; NUM_SAMPLE_SLOTS],
    pub global_fine_tune_cents: f32,
    pub fade_out: f32,

    // Shared Atomics
    pub volume: Arc<AtomicU32>,
    pub peak_meter: Arc<AtomicU32>,
    pub lfo_value_atomic: Arc<AtomicU32>,
    pub lfo2_value_atomic: Arc<AtomicU32>,
    pub env2_value_atomic: Arc<AtomicU32>,
    pub pitch_mod_atomic: Arc<AtomicU32>,
    pub amp_mod_atomic: Arc<AtomicU32>,
    pub saturation_mod_atomic: Arc<AtomicU32>,
    pub final_cutoff_atomic: Arc<AtomicU32>,
    pub last_triggered_slot_index: Arc<AtomicUsize>, // <-- ADDED THIS

    // UI state
    pub displayed_peak_level: f32,
    pub visualizer_cache: Vec<epaint::Shape>,
    pub last_snapshot: SamplerVisualizerSnapshot,
    pub last_visualizer_rect: Rect,
    pub force_redraw_generation: u32, // <-- ADDED THIS
}

impl SamplerEngineState {
    pub fn new() -> Self {
        Self {
            amp_adsr: Default::default(),
            filter_adsr: Default::default(),
            filter_settings: Arc::new(RwLock::new(Default::default())),
            lfo_settings: Arc::new(RwLock::new(Default::default())),
            lfo2_settings: Arc::new(RwLock::new(Default::default())),
            mod_matrix: Arc::new(RwLock::new(Vec::new())),
            saturation_settings: Arc::new(RwLock::new(Default::default())),
            is_polyphonic: true,
            sample_names: std::array::from_fn(|_| "Empty".to_string()),
            sample_paths: Default::default(), // This correctly creates [None; 8]
            sample_data_for_ui: std::array::from_fn(|_| Arc::new(RwLock::new(Vec::new()))),
            root_notes: std::array::from_fn(|i| (24 + i * 12) as u8), // Default root notes C2, C3, ...
            global_fine_tune_cents: 0.0,
            fade_out: 0.01,
            volume: Arc::new(AtomicU32::new(1_000_000)),
            peak_meter: Arc::new(AtomicU32::new(0)),
            lfo_value_atomic: Arc::new(AtomicU32::new(0)),
            lfo2_value_atomic: Arc::new(AtomicU32::new(0)),
            env2_value_atomic: Arc::new(AtomicU32::new(0)),
            pitch_mod_atomic: Arc::new(AtomicU32::new(500_000)),
            amp_mod_atomic: Arc::new(AtomicU32::new(500_000)),
            saturation_mod_atomic: Arc::new(AtomicU32::new(0)),
            final_cutoff_atomic: Arc::new(AtomicU32::new(1_000_000)),
            last_triggered_slot_index: Arc::new(AtomicUsize::new(0)), // <-- INITIALIZED
            displayed_peak_level: 0.0,
            visualizer_cache: Vec::new(),
            last_snapshot: SamplerVisualizerSnapshot::default(),
            last_visualizer_rect: Rect::ZERO,
            force_redraw_generation: 0, // <-- INITIALIZED
        }
    }

    /// Helper method to create a new snapshot of the current state.
    pub fn get_visualizer_snapshot(&self) -> SamplerVisualizerSnapshot {
        SamplerVisualizerSnapshot {
            final_filter_cutoff: self.final_cutoff_atomic.load(Ordering::Relaxed),
            pitch_mod: self.pitch_mod_atomic.load(Ordering::Relaxed),
            saturation_mod: self.saturation_mod_atomic.load(Ordering::Relaxed),
            fade_out: (self.fade_out * 1_000_000.0) as u32,
            redraw_generation: self.force_redraw_generation, // <-- ADDED THIS
        }
    }
}

// --- Engine-Specific Preset ---
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct SamplerEnginePreset {
    pub volume: f32,
    pub amp_adsr: AdsrSettings,
    pub filter_adsr: AdsrSettings,
    pub filter: FilterSettings,
    pub lfo_settings: LfoSettings,
    pub lfo2_settings: LfoSettings,
    pub mod_matrix: Vec<ModRouting>,
    pub saturation_settings: SaturationSettings,
    pub is_polyphonic: bool,

    // Sampler specific (Multi-sample)
    pub sample_paths: [Option<PathBuf>; NUM_SAMPLE_SLOTS],
    pub root_notes: [u8; NUM_SAMPLE_SLOTS],
    pub global_fine_tune_cents: f32,
    pub fade_out: f32,
}

impl Default for SamplerEnginePreset {
    fn default() -> Self {
        Self {
            volume: 1.0,
            amp_adsr: Default::default(),
            filter_adsr: Default::default(),
            filter: Default::default(),
            lfo_settings: Default::default(),
            lfo2_settings: Default::default(),
            mod_matrix: Vec::new(),
            saturation_settings: Default::default(),
            is_polyphonic: true,
            sample_paths: Default::default(),
            root_notes: std::array::from_fn(|i| (24 + i * 12) as u8), // Default root notes C2, C3, ...
            global_fine_tune_cents: 0.0,
            fade_out: 0.01,
        }
    }
}

// --- Voice and Main Engine Logic ---

const NUM_VOICES: usize = 16; // Increased for better polyphony with multi-sampling

/// A struct to hold the pre-calculated modulation values for a single sample.
#[derive(Default, Clone, Copy)]
struct ModulationValues {
    pitch: f32,
    amp: f32,
    cutoff: f32,
    saturation: f32,
}

struct Voice {
    note_id: u8,
    sample_rate: f32,
    phase: f32,
    base_pitch_ratio: f32,
    velocity: f32,
    amp_adsr: Adsr,
    filter_adsr: Adsr,
    filter: Filter,
    age: u32,
    sample_data: Arc<Vec<f32>>, // Each voice now holds its own sample data
    // Buffer to hold the most recent processed modulation values for UI feedback
    last_mod_values: ModulationValues,
    last_env2_value: f32,
    last_drive_value: f32,
}

impl Voice {
    fn new(sample_rate: f32) -> Self {
        Self {
            note_id: 0,
            sample_rate,
            phase: 0.0,
            base_pitch_ratio: 1.0,
            velocity: 0.0,
            amp_adsr: Adsr::new(Default::default(), sample_rate),
            filter_adsr: Adsr::new(Default::default(), sample_rate),
            filter: Filter::new(),
            age: u32::MAX,
            sample_data: Arc::new(Vec::new()),
            last_mod_values: ModulationValues::default(),
            last_env2_value: 0.0,
            last_drive_value: 0.0,
        }
    }

    pub fn is_active(&self) -> bool {
        self.amp_adsr.state != crate::synth::AdsrState::Idle && !self.sample_data.is_empty()
    }

    fn process_sample(
        &mut self,
        cents_ratio: f32,
        fade_out_norm: f32,
        filter_settings: FilterSettings,
        saturation_settings: SaturationSettings,
        mod_matrix: &[ModRouting],
        base_mods: ModulationValues,
    ) -> f32 {
        self.age = self.age.saturating_add(1);
        let amp_env_val = self.amp_adsr.process();
        if amp_env_val < 1e-6 {
            return 0.0;
        }
        self.last_env2_value = self.filter_adsr.process();

        // Start with base mods and add voice-specific modulation
        let mut final_mods = base_mods;
        for routing in mod_matrix.iter() {
            let source_val = match routing.source {
                ModSource::Env2 => self.last_env2_value,
                ModSource::Velocity => self.velocity,
                _ => continue,
            };
            let mod_val = source_val * routing.amount;
            match routing.destination {
                ModDestination::Pitch => final_mods.pitch += mod_val,
                ModDestination::FilterCutoff => final_mods.cutoff += mod_val,
                ModDestination::Amplitude => final_mods.amp += mod_val,
                ModDestination::Saturation => final_mods.saturation += mod_val,
                _ => {}
            }
        }

        let sample_len = self.sample_data.len();
        if sample_len == 0 {
            return 0.0;
        }

        // Apply fade out
        let mut fade_gain = 1.0;
        let fade_out_samples = (sample_len as f32 * fade_out_norm.clamp(0.0, 0.5)) as usize;
        if fade_out_samples > 0 {
            let fade_start_point = sample_len.saturating_sub(fade_out_samples);
            if self.phase >= fade_start_point as f32 {
                let phase_in_fade = self.phase - fade_start_point as f32;
                fade_gain = 1.0 - (phase_in_fade / fade_out_samples as f32);
                fade_gain = fade_gain.clamp(0.0, 1.0);
            }
        }

        let raw_sample = SamplerEngine::get_interpolated_sample(&self.sample_data, self.phase);

        // --- OPTIMIZED SATURATION LOGIC ---
        let final_saturation_mod = final_mods.saturation.clamp(-1.0, 1.0);
        let total_drive = (final_saturation_mod * saturation_settings.drive * 10.0).max(0.0);
        let saturated_sample = (raw_sample * (1.0 + total_drive)).fast_tanh();

        let t = (total_drive / 10.0).clamp(0.0, 1.0);
        let p0 = 1.0;
        let p2 = 1.0 - saturation_settings.compensation_amount;
        let p1 = lerp(p0..=p2, saturation_settings.compensation_bias);
        let makeup_gain = (1.0 - t).powi(2) * p0 + 2.0 * (1.0 - t) * t * p1 + t.powi(2) * p2;

        let compensated_sample = saturated_sample * makeup_gain;

        let mut final_filter_settings = filter_settings;
        final_filter_settings.cutoff =
            (filter_settings.cutoff + final_mods.cutoff).clamp(0.0, 1.0);
        let filtered_sample = self.filter.process(
            compensated_sample,
            final_filter_settings,
            self.sample_rate,
        );
        let voice_output = filtered_sample
            * 0.8
            * self.velocity
            * amp_env_val
            * (1.0 + final_mods.amp).max(0.0)
            * fade_gain;

        // --- OPTIMIZED PITCH CALCULATION ---
        let mod_pitch_ratio = POW2_LUT.get_interpolated(final_mods.pitch);
        let phase_inc = self.base_pitch_ratio * cents_ratio * mod_pitch_ratio;
        self.phase += phase_inc;

        if self.phase >= (sample_len - 1) as f32 || self.phase < 0.0 {
            self.amp_adsr.reset();
        }

        // Store the final modulation values for this sample for UI feedback
        self.last_mod_values = final_mods;
        self.last_drive_value = total_drive / 10.0;

        voice_output
    }

    fn note_on(
        &mut self,
        note: u8,
        velocity: u8,
        pitch_ratio: f32,
        sample_data: Arc<Vec<f32>>,
    ) {
        self.note_id = note;
        self.phase = 0.0;
        self.base_pitch_ratio = pitch_ratio;
        self.velocity = velocity as f32 / 127.0;
        self.sample_data = sample_data;
        self.amp_adsr.note_on();
        self.filter_adsr.note_on();
        self.age = 0;
    }

    fn note_off(&mut self) {
        self.amp_adsr.note_off();
        self.filter_adsr.note_off();
    }
}

pub struct SamplerEngine {
    voices: Vec<Voice>,
    is_polyphonic: bool,
    sample_rate: f32,

    // Sample data and settings
    sample_slots: [SampleSlot; NUM_SAMPLE_SLOTS],
    global_fine_tune_cents: f32,
    fade_out_norm: f32,

    // LFOs and Modulation
    lfo1: Lfo,
    lfo2: Lfo,
    dummy_wavetable_set: WavetableSet, // For LFOs that might use wavetables
    pub filter_settings: Arc<RwLock<FilterSettings>>,
    pub lfo_settings: Arc<RwLock<LfoSettings>>,
    pub lfo2_settings: Arc<RwLock<LfoSettings>>,
    pub mod_matrix: Arc<RwLock<Vec<ModRouting>>>,
    pub saturation_settings: Arc<RwLock<SaturationSettings>>,

    // Atomics for UI feedback
    pub lfo_value_atomic: Arc<AtomicU32>,
    pub lfo2_value_atomic: Arc<AtomicU32>,
    pub env2_value_atomic: Arc<AtomicU32>,
    pub pitch_mod_atomic: Arc<AtomicU32>,
    pub amp_mod_atomic: Arc<AtomicU32>,
    pub saturation_mod_atomic: Arc<AtomicU32>,
    pub final_cutoff_atomic: Arc<AtomicU32>,
    pub last_triggered_slot_index: Arc<AtomicUsize>, // <-- ADDED THIS

    // Per-voice buffers for parallel processing
    voice_outputs: Vec<Vec<f32>>,
}

impl SamplerEngine {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        sample_rate: f32,
        filter_settings: Arc<RwLock<FilterSettings>>,
        lfo_settings: Arc<RwLock<LfoSettings>>,
        lfo2_settings: Arc<RwLock<LfoSettings>>,
        mod_matrix: Arc<RwLock<Vec<ModRouting>>>,
        saturation_settings: Arc<RwLock<SaturationSettings>>,
        lfo_value_atomic: Arc<AtomicU32>,
        lfo2_value_atomic: Arc<AtomicU32>,
        env2_value_atomic: Arc<AtomicU32>,
        pitch_mod_atomic: Arc<AtomicU32>,
        amp_mod_atomic: Arc<AtomicU32>,
        saturation_mod_atomic: Arc<AtomicU32>,
        final_cutoff_atomic: Arc<AtomicU32>,
        last_triggered_slot_index: Arc<AtomicUsize>, // <-- ADDED THIS
    ) -> Self {
        let voices = (0..NUM_VOICES).map(|_| Voice::new(sample_rate)).collect();
        Self {
            voices,
            is_polyphonic: true,
            sample_rate,
            sample_slots: Default::default(),
            global_fine_tune_cents: 0.0,
            fade_out_norm: 0.01,
            lfo1: Lfo::new(sample_rate),
            lfo2: Lfo::new(sample_rate),
            dummy_wavetable_set: WavetableSet::new_basic(),
            filter_settings,
            lfo_settings,
            lfo2_settings,
            mod_matrix,
            saturation_settings,
            lfo_value_atomic,
            lfo2_value_atomic,
            env2_value_atomic,
            pitch_mod_atomic,
            amp_mod_atomic,
            saturation_mod_atomic,
            final_cutoff_atomic,
            last_triggered_slot_index, // <-- STORED
            voice_outputs: vec![vec![0.0; 2048]; NUM_VOICES], // Max buffer size
        }
    }

    fn get_lfo_freq(sample_rate: f32, settings: LfoSettings, musical_bar_len: usize) -> f32 {
        match settings.mode {
            LfoRateMode::Hz => settings.hz_rate,
            LfoRateMode::Sync => {
                if musical_bar_len > 0 {
                    (sample_rate / musical_bar_len as f32) * settings.sync_rate
                } else {
                    0.0
                }
            }
        }
    }

    fn get_interpolated_sample(table: &[f32], phase: f32) -> f32 {
        let table_len = table.len();
        if table_len < 2 {
            return table.get(0).copied().unwrap_or(0.0);
        }

        let idx_floor = phase.floor() as usize;
        let idx_ceil = idx_floor + 1;

        if idx_ceil >= table_len {
            return table.get(idx_floor).copied().unwrap_or(0.0);
        }

        let frac = phase.fract();

        let val1 = table[idx_floor];
        let val2 = table[idx_ceil];

        val1 * (1.0 - frac) + val2 * frac
    }

    fn note_to_freq(note: u8) -> f32 {
        440.0 * 2.0_f32.powf((note as f32 - 69.0) / 12.0)
    }

    pub fn load_sample_for_slot(&mut self, slot_index: usize, audio_data: Arc<Vec<f32>>) {
        if let Some(slot) = self.sample_slots.get_mut(slot_index) {
            slot.audio_data = audio_data;
        }
    }

    pub fn set_sampler_settings(
        &mut self,
        root_notes: [u8; NUM_SAMPLE_SLOTS],
        global_fine_tune_cents: f32,
        fade_out: f32,
    ) {
        for (i, slot) in self.sample_slots.iter_mut().enumerate() {
            slot.root_note = root_notes[i];
        }
        self.global_fine_tune_cents = global_fine_tune_cents;
        self.fade_out_norm = fade_out;
    }
}

impl Engine for SamplerEngine {
    fn process(
        &mut self,
        output_buffer: &mut [f32],
        musical_bar_len: usize,
        midi_cc_values: &Arc<[[AtomicU32; 128]; 16]>,
    ) {
        let block_size = output_buffer.len();
        output_buffer.fill(0.0);

        // --- Read all shared data once per block ---
        let lfo1_settings = *self.lfo_settings.read().unwrap();
        let lfo2_settings = *self.lfo2_settings.read().unwrap();
        let filter_settings = *self.filter_settings.read().unwrap();
        let saturation_settings = *self.saturation_settings.read().unwrap();
        let mod_matrix = self.mod_matrix.read().unwrap();

        // --- Pre-calculate LFOs for the entire block ---
        let mut lfo1_output = vec![0.0; block_size];
        let mut lfo2_output = vec![0.0; block_size];
        let lfo1_freq = Self::get_lfo_freq(self.sample_rate, lfo1_settings, musical_bar_len);
        let lfo2_freq = Self::get_lfo_freq(self.sample_rate, lfo2_settings, musical_bar_len);
        for i in 0..block_size {
            lfo1_output[i] =
                self.lfo1
                    .process(lfo1_freq, lfo1_settings.waveform, &self.dummy_wavetable_set);
            lfo2_output[i] =
                self.lfo2
                    .process(lfo2_freq, lfo2_settings.waveform, &self.dummy_wavetable_set);
        }

        // --- Resize voice output buffers if necessary ---
        if self.voice_outputs[0].len() != block_size {
            for buffer in &mut self.voice_outputs {
                buffer.resize(block_size, 0.0);
            }
        }

        // --- Parallel Processing of Voices ---
        let cents_ratio = POW2_LUT.get_interpolated(self.global_fine_tune_cents / 100.0);
        let fade_out_norm = self.fade_out_norm;

        self.voices
            .par_iter_mut()
            .zip(self.voice_outputs.par_iter_mut())
            .for_each(|(voice, voice_output_buffer)| {
                if !voice.is_active() {
                    voice_output_buffer.fill(0.0);
                    return;
                }

                for i in 0..block_size {
                    // Pre-calculate modulation from non-voice-specific sources
                    let mut base_mods = ModulationValues::default();
                    for routing in mod_matrix.iter() {
                        let source_val = match routing.source {
                            ModSource::Lfo1 => lfo1_output[i],
                            ModSource::Lfo2 => lfo2_output[i],
                            ModSource::Static => 1.0,
                            ModSource::MidiCC(id) => {
                                midi_cc_values[id.channel as usize][id.cc as usize].load(Ordering::Relaxed) as f32 / 1_000_000.0
                            }
                            _ => continue,
                        };
                        let mod_val = source_val * routing.amount;
                        match routing.destination {
                            ModDestination::Pitch => base_mods.pitch += mod_val,
                            ModDestination::FilterCutoff => base_mods.cutoff += mod_val,
                            ModDestination::Amplitude => base_mods.amp += mod_val,
                            ModDestination::Saturation => base_mods.saturation += mod_val,
                            _ => {}
                        }
                    }

                    voice_output_buffer[i] = voice.process_sample(
                        cents_ratio,
                        fade_out_norm,
                        filter_settings,
                        saturation_settings,
                        &mod_matrix,
                        base_mods,
                    );
                }
            });

        // --- Final Mixdown ---
        for voice_buffer in &self.voice_outputs {
            for i in 0..block_size {
                output_buffer[i] += voice_buffer[i];
            }
        }

        // --- Update UI Atomics (Unconditional) ---
        let oldest_voice = self.voices.iter().filter(|v| v.is_active()).min_by_key(|v| v.age);

        let (mods, last_env2, last_drive) = if let Some(voice) = oldest_voice {
            (voice.last_mod_values, voice.last_env2_value, voice.last_drive_value)
        } else {
            let mut idle_mods = ModulationValues::default();
            let lfo1_val = *lfo1_output.last().unwrap_or(&0.0);
            let lfo2_val = *lfo2_output.last().unwrap_or(&0.0);
            for routing in mod_matrix.iter() {
                let source_val = match routing.source {
                    ModSource::Lfo1 => lfo1_val,
                    ModSource::Lfo2 => lfo2_val,
                    ModSource::Static => 1.0,
                    ModSource::MidiCC(id) => {
                        midi_cc_values[id.channel as usize][id.cc as usize].load(Ordering::Relaxed) as f32 / 1_000_000.0
                    }
                    _ => 0.0,
                };
                let mod_val = source_val * routing.amount;
                match routing.destination {
                    ModDestination::Pitch => idle_mods.pitch += mod_val,
                    ModDestination::FilterCutoff => idle_mods.cutoff += mod_val,
                    ModDestination::Saturation => idle_mods.saturation += mod_val,
                    _ => {}
                }
            }
            let final_saturation_mod = idle_mods.saturation.clamp(-1.0, 1.0);
            let total_drive = (final_saturation_mod * saturation_settings.drive * 10.0).max(0.0);

            (idle_mods, 0.0, total_drive / 10.0)
        };

        self.lfo_value_atomic.store(((lfo1_output.last().unwrap_or(&0.0) * 0.5 + 0.5) * 1_000_000.0) as u32, Ordering::Relaxed);
        self.lfo2_value_atomic.store(((lfo2_output.last().unwrap_or(&0.0) * 0.5 + 0.5) * 1_000_000.0) as u32, Ordering::Relaxed);
        self.env2_value_atomic.store((last_env2 * 1_000_000.0) as u32, Ordering::Relaxed);
        self.pitch_mod_atomic.store(((mods.pitch * 0.5 + 0.5).clamp(0.0, 1.0) * 1_000_000.0) as u32, Ordering::Relaxed);
        self.amp_mod_atomic.store(((mods.amp * 0.5 + 0.5).clamp(0.0, 1.0) * 1_000_000.0) as u32, Ordering::Relaxed);
        self.saturation_mod_atomic.store((last_drive.clamp(0.0, 1.0) * 1_000_000.0) as u32, Ordering::Relaxed);

        // Final, absolute values for visualizer
        let base_cutoff = filter_settings.cutoff;
        let final_cutoff = (base_cutoff + mods.cutoff).clamp(0.0, 1.0);
        self.final_cutoff_atomic.store((final_cutoff * 1_000_000.0) as u32, Ordering::Relaxed);
    }

    fn note_on(&mut self, note: u8, velocity: u8) {
        if self.lfo_settings.read().unwrap().retrigger {
            self.lfo1.reset_phase();
        }
        if self.lfo2_settings.read().unwrap().retrigger {
            self.lfo2.reset_phase();
        }

        let octave = note / 12;
        let ideal_slot_index = (octave.saturating_sub(1)).min((NUM_SAMPLE_SLOTS - 1) as u8) as usize;

        // Find the best available slot and its index
        let chosen_slot_and_index = self.sample_slots[ideal_slot_index..]
            .iter()
            .enumerate()
            .find(|(_, slot)| !slot.audio_data.is_empty())
            .map(|(i, slot)| (slot, ideal_slot_index + i)) // Adjust index relative to the full array
            .or_else(|| {
                self.sample_slots[..ideal_slot_index]
                    .iter()
                    .enumerate()
                    .rfind(|(_, slot)| !slot.audio_data.is_empty())
                    .map(|(i, slot)| (slot, i)) // Map to swap tuple order
            })
            .or_else(|| {
                self.sample_slots
                    .iter()
                    .enumerate()
                    .find(|(_, slot)| !slot.audio_data.is_empty())
                    .map(|(i, slot)| (slot, i)) // Map to swap tuple order
            });

        if let Some((slot, index)) = chosen_slot_and_index {
            self.last_triggered_slot_index
                .store(index, Ordering::Relaxed);

            let target_voice = if self.is_polyphonic {
                self.voices.iter_mut().max_by_key(|v| {
                    let priority = match v.amp_adsr.state {
                        crate::synth::AdsrState::Idle => 2,
                        crate::synth::AdsrState::Release => 1,
                        _ => 0, // Attack, Decay, Sustain
                    };
                    (priority, v.age)
                })
            } else {
                for v in self.voices.iter_mut() {
                    v.note_off();
                }
                self.voices.get_mut(0)
            };
            if let Some(voice) = target_voice {
                let note_freq = Self::note_to_freq(note);
                let root_freq = Self::note_to_freq(slot.root_note);
                let pitch_ratio = note_freq / root_freq;
                voice.note_on(note, velocity, pitch_ratio, slot.audio_data.clone());
            }
        }
    }

    fn note_off(&mut self, note: u8) {
        for voice in self.voices.iter_mut().filter(|v| v.note_id == note && v.is_active()) {
            voice.note_off();
        }
    }

    fn set_polyphonic(&mut self, poly: bool) {
        self.is_polyphonic = poly;
        if !poly {
            let mut active_voices: Vec<&mut Voice> =
                self.voices.iter_mut().filter(|v| v.is_active()).collect();
            if active_voices.len() > 1 {
                active_voices.sort_by_key(|v| v.age);
                for voice in active_voices.into_iter().rev().skip(1) {
                    voice.note_off();
                }
            }
        }
    }

    fn set_amp_adsr(&mut self, settings: AdsrSettings) {
        for voice in &mut self.voices {
            voice.amp_adsr.set_settings(settings);
        }
    }

    fn set_filter_adsr(&mut self, settings: AdsrSettings) {
        for voice in &mut self.voices {
            voice.filter_adsr.set_settings(settings);
        }
    }

    fn reset_to_defaults(&mut self) {
        self.sample_slots = Default::default();
    }

    fn set_wavetable(&mut self, _slot_index: usize, _audio_data: Arc<Vec<f32>>, _name: String) {
        // This engine does not use wavetables, but this is required to conform to the Engine trait.
    }
}