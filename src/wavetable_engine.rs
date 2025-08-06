// src/wavetable_engine.rs
use crate::synth::{
    Adsr, AdsrSettings, Engine, Filter, FilterSettings, Lfo, LfoRateMode, LfoSettings,
    ModDestination, ModRouting, ModSource, WAVETABLE_SIZE,
};
use crate::synth::{FastTanh, EXP_LUT, POW2_LUT}; // Use our performance utilities
use egui::{epaint, lerp, Rect}; // Added `Rect` for the cache
use rayon::prelude::*; // Import Rayon for parallel processing
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, RwLock};

// --- New Data Structure for Saturation ---
#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
#[serde(default)]
pub struct SaturationSettings {
    pub drive: f32,
    pub compensation_amount: f32,
    pub compensation_bias: f32,
}

impl Default for SaturationSettings {
    fn default() -> Self {
        Self {
            drive: 0.0,
            compensation_amount: 0.5,
            compensation_bias: 0.5,
        }
    }
}

/// A snapshot of all values that affect the wavetable visualizer.
/// Used to detect when the visualizer needs to be redrawn and cached.
#[derive(PartialEq, Clone, Copy, Debug, Default)]
pub struct VisualizerSnapshot {
    final_wavetable_pos: u32,
    final_filter_cutoff: u32,
    pitch_mod: u32,
    bell_pos_mod: u32,
    bell_amount_mod: u32,
    bell_width_mod: u32,
    saturation_mod: u32,
    // A generation counter to force redraws on major state changes like loading a sample
    generation: u64,
}

// --- Engine-Specific UI State ---
pub struct WavetableEngineState {
    pub amp_adsr: AdsrSettings,
    pub filter_adsr: AdsrSettings,
    pub filter_settings: Arc<RwLock<FilterSettings>>,
    pub wavetable_mixer_settings: Arc<RwLock<WavetableMixerSettings>>,
    pub lfo_settings: Arc<RwLock<LfoSettings>>,
    pub lfo2_settings: Arc<RwLock<LfoSettings>>,
    pub mod_matrix: Arc<RwLock<Vec<ModRouting>>>,
    pub saturation_settings: Arc<RwLock<SaturationSettings>>,
    pub is_polyphonic: bool,
    pub wavetable_names: [String; 4],
    pub wavetable_sources: [WavetableSource; 4],
    pub window_positions: [f32; 4],
    pub wavetable_set: Arc<RwLock<WavetableSet>>,
    pub wavetable_position: Arc<AtomicU32>,
    pub saturation_mod_atomic: Arc<AtomicU32>,
    pub volume: Arc<AtomicU32>,
    pub peak_meter: Arc<AtomicU32>,
    pub lfo_value_atomic: Arc<AtomicU32>,
    pub lfo2_value_atomic: Arc<AtomicU32>,
    pub env2_value_atomic: Arc<AtomicU32>,
    pub pitch_mod_atomic: Arc<AtomicU32>,
    pub bell_pos_atomic: Arc<AtomicU32>,
    pub bell_amount_atomic: Arc<AtomicU32>,
    pub bell_width_atomic: Arc<AtomicU32>,
    pub displayed_peak_level: f32,

    // --- New fields for UI Caching & Feedback ---
    pub final_wt_pos_atomic: Arc<AtomicU32>,
    pub final_cutoff_atomic: Arc<AtomicU32>,
    pub visualizer_cache: Vec<epaint::Shape>,
    pub last_snapshot: VisualizerSnapshot,
    pub force_redraw_generation: u64,
    pub last_visualizer_rect: Rect,

    // --- New fields for UI-side processing ---
    pub original_sources: [Arc<Vec<f32>>; 4],
    pub source_sample_rates: [u32; 4],
}

impl WavetableEngineState {
    pub fn new() -> Self {
        let default_tables = WavetableSet::new_basic();
        Self {
            amp_adsr: AdsrSettings::default(),
            filter_adsr: AdsrSettings::default(),
            filter_settings: Arc::new(RwLock::new(FilterSettings::default())),
            wavetable_mixer_settings: Arc::new(RwLock::new(WavetableMixerSettings::default())),
            lfo_settings: Arc::new(RwLock::new(LfoSettings::default())),
            lfo2_settings: Arc::new(RwLock::new(LfoSettings::default())),
            mod_matrix: Arc::new(RwLock::new(Vec::new())),
            saturation_settings: Arc::new(RwLock::new(Default::default())),
            is_polyphonic: true,
            wavetable_names: [
                default_tables.tables[0].name.clone(),
                default_tables.tables[1].name.clone(),
                default_tables.tables[2].name.clone(),
                default_tables.tables[3].name.clone(),
            ],
            wavetable_sources: [
                WavetableSource::Default(default_tables.tables[0].name.clone()),
                WavetableSource::Default(default_tables.tables[1].name.clone()),
                WavetableSource::Default(default_tables.tables[2].name.clone()),
                WavetableSource::Default(default_tables.tables[3].name.clone()),
            ],
            window_positions: [0.0; 4],
            wavetable_set: Arc::new(RwLock::new(WavetableSet::new_basic())),
            wavetable_position: Arc::new(AtomicU32::new(0)),
            saturation_mod_atomic: Arc::new(AtomicU32::new(0)),
            volume: Arc::new(AtomicU32::new(1_000_000)),
            peak_meter: Arc::new(AtomicU32::new(0)),
            lfo_value_atomic: Arc::new(AtomicU32::new(0)),
            lfo2_value_atomic: Arc::new(AtomicU32::new(0)),
            env2_value_atomic: Arc::new(AtomicU32::new(0)),
            pitch_mod_atomic: Arc::new(AtomicU32::new(500_000)),
            bell_pos_atomic: Arc::new(AtomicU32::new(500_000)),
            bell_amount_atomic: Arc::new(AtomicU32::new(500_000)),
            bell_width_atomic: Arc::new(AtomicU32::new(500_000)),
            displayed_peak_level: 0.0,
            final_wt_pos_atomic: Arc::new(AtomicU32::new(0)),
            final_cutoff_atomic: Arc::new(AtomicU32::new(1_000_000)),
            visualizer_cache: Vec::new(),
            last_snapshot: VisualizerSnapshot::default(),
            force_redraw_generation: 0,
            last_visualizer_rect: Rect::ZERO,
            original_sources: std::array::from_fn(|i| Arc::new(default_tables.tables[i].table.clone())),
            source_sample_rates: [48000; 4], // Placeholder, will be overwritten
        }
    }

    /// Helper method to create a new snapshot of the current state.
    pub fn get_visualizer_snapshot(&self) -> VisualizerSnapshot {
        VisualizerSnapshot {
            final_wavetable_pos: self.final_wt_pos_atomic.load(Ordering::Relaxed),
            final_filter_cutoff: self.final_cutoff_atomic.load(Ordering::Relaxed),
            pitch_mod: self.pitch_mod_atomic.load(Ordering::Relaxed),
            bell_pos_mod: self.bell_pos_atomic.load(Ordering::Relaxed),
            bell_amount_mod: self.bell_amount_atomic.load(Ordering::Relaxed),
            bell_width_mod: self.bell_width_atomic.load(Ordering::Relaxed),
            saturation_mod: self.saturation_mod_atomic.load(Ordering::Relaxed),
            generation: self.force_redraw_generation,
        }
    }
}

// --- Engine-Specific Preset ---
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct WavetableEnginePreset {
    pub volume: f32,
    pub amp_adsr: AdsrSettings,
    pub filter_adsr: AdsrSettings,
    pub filter: FilterSettings,
    pub lfo_settings: LfoSettings,
    pub lfo2_settings: LfoSettings,
    pub mod_matrix: Vec<ModRouting>,
    pub saturation_settings: SaturationSettings,
    pub wavetable_position_m_u32: u32,
    pub is_polyphonic: bool,
    pub wavetable_sources: [WavetableSource; 4],
    pub window_positions: [f32; 4],
    pub wavetable_mixer: WavetableMixerSettings,
}

impl Default for WavetableEnginePreset {
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
            wavetable_position_m_u32: 0,
            is_polyphonic: true,
            wavetable_sources: [
                WavetableSource::Default("Sine".to_string()),
                WavetableSource::Default("Saw".to_string()),
                WavetableSource::Default("Square".to_string()),
                WavetableSource::Default("Triangle".to_string()),
            ],
            window_positions: [0.0; 4],
            wavetable_mixer: Default::default(),
        }
    }
}

// --- Engine-Specific Data ---

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum WavetableSource {
    Default(String),
    File(std::path::PathBuf),
}

impl Default for WavetableSource {
    fn default() -> Self {
        WavetableSource::Default("Sine".to_string())
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub struct WavetableMixerSettings {
    pub layer_volumes: [f32; 5], // 0-3 for slots, 4 for blended
}

impl Default for WavetableMixerSettings {
    fn default() -> Self {
        Self {
            layer_volumes: [0.0, 0.0, 0.0, 0.0, 1.0], // Default to old behavior (blended only)
        }
    }
}

#[derive(Clone, Debug)]
pub struct Wavetable {
    pub name: String,
    pub table: Vec<f32>,
}

#[derive(Debug)]
pub struct WavetableSet {
    pub tables: Vec<Wavetable>,
}

impl WavetableSet {
    pub fn new_basic() -> Self {
        let mut tables = Vec::new();
        tables.push(Wavetable {
            name: "Sine".to_string(),
            table: (0..WAVETABLE_SIZE)
                .map(|i| {
                    let phase = i as f32 / WAVETABLE_SIZE as f32;
                    (phase * std::f32::consts::TAU).sin()
                })
                .collect(),
        });
        tables.push(Wavetable {
            name: "Saw".to_string(),
            table: (0..WAVETABLE_SIZE)
                .map(|i| {
                    let phase = i as f32 / WAVETABLE_SIZE as f32;
                    2.0 * (phase - phase.round())
                })
                .collect(),
        });
        tables.push(Wavetable {
            name: "Square".to_string(),
            table: (0..WAVETABLE_SIZE)
                .map(|i| {
                    if (i as f32 / WAVETABLE_SIZE as f32) < 0.5 {
                        1.0
                    } else {
                        -1.0
                    }
                })
                .collect(),
        });
        tables.push(Wavetable {
            name: "Triangle".to_string(),
            table: (0..WAVETABLE_SIZE)
                .map(|i| {
                    let phase = i as f32 / WAVETABLE_SIZE as f32;
                    (2.0 * phase - 1.0).abs() * 2.0 - 1.0
                })
                .collect(),
        });
        Self { tables }
    }

    fn get_sample(&self, morph_pos: f32, phase: f32) -> f32 {
        if self.tables.is_empty() {
            return 0.0;
        }

        let num_tables = self.tables.len() as f32;
        let morph_pos = morph_pos.clamp(0.0, num_tables - 1.0001);

        let table1_idx = (morph_pos.floor() as usize).min(self.tables.len() - 1);
        let table2_idx = (morph_pos.ceil() as usize).min(self.tables.len() - 1);
        let morph_frac = morph_pos.fract();

        let sample1 = Self::get_interpolated_sample(&self.tables[table1_idx].table, phase);
        if table1_idx == table2_idx {
            return sample1;
        }

        let sample2 = Self::get_interpolated_sample(&self.tables[table2_idx].table, phase);

        sample1 * (1.0 - morph_frac) + sample2 * morph_frac
    }

    pub fn get_interpolated_sample(table: &[f32], phase: f32) -> f32 {
        let table_len = table.len();
        if table_len == 0 {
            return 0.0;
        }

        let wrapped_phase = phase % table_len as f32;
        let idx_floor = wrapped_phase.floor() as usize;
        let idx_ceil = (idx_floor + 1) % table_len;
        let frac = wrapped_phase.fract();

        let val1 = table[idx_floor];
        let val2 = table[idx_ceil];

        val1 * (1.0 - frac) + val2 * frac
    }
}

// --- Voice and Main Engine Logic ---

const NUM_VOICES: usize = 10;

/// A struct to hold the pre-calculated modulation values for a single sample.
#[derive(Default, Clone, Copy)]
struct ModulationValues {
    wt_pos: f32,
    pitch: f32,
    amp: f32,
    cutoff: f32,
    bell_pos: f32,
    bell_amount: f32,
    bell_width: f32,
    saturation: f32,
}

struct Voice {
    note_id: u8,
    sample_rate: f32,
    phase: f32,
    base_frequency: f32,
    velocity: f32,
    amp_adsr: Adsr,
    filter_adsr: Adsr,
    filter: Filter,
    age: u32,
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
            base_frequency: 440.0,
            velocity: 0.0,
            amp_adsr: Adsr::new(AdsrSettings::default(), sample_rate),
            filter_adsr: Adsr::new(AdsrSettings::default(), sample_rate),
            filter: Filter::new(),
            age: u32::MAX,
            last_mod_values: ModulationValues::default(),
            last_env2_value: 0.0,
            last_drive_value: 0.0,
        }
    }

    pub fn is_active(&self) -> bool {
        self.amp_adsr.state != crate::synth::AdsrState::Idle
    }

    /// This is the performance-critical "hot loop" function.
    /// It now processes a single sample using pre-calculated base modulation values.
    fn process_sample(
        &mut self,
        wavetable_set: &WavetableSet,
        filter_settings: FilterSettings,
        wavetable_mixer_settings: WavetableMixerSettings,
        saturation_settings: SaturationSettings,
        base_morph_pos: f32,
        mod_matrix: &[ModRouting],
        base_mods: ModulationValues,
    ) -> f32 {
        self.age = self.age.saturating_add(1);

        let amp_env_val = self.amp_adsr.process();
        self.last_env2_value = self.filter_adsr.process();

        // Start with the base mods and add voice-specific modulation
        let mut final_mods = base_mods;
        for routing in mod_matrix.iter() {
            let source_val = match routing.source {
                // These are handled in the outer loop
                ModSource::Lfo1 | ModSource::Lfo2 | ModSource::Static | ModSource::MidiCC(_) => continue,
                // Voice-specific sources
                ModSource::Env2 => self.last_env2_value,
                ModSource::Velocity => self.velocity,
            };
            let mod_val = source_val * routing.amount;
            match routing.destination {
                ModDestination::WavetablePosition => final_mods.wt_pos += mod_val,
                ModDestination::Pitch => final_mods.pitch += mod_val,
                ModDestination::Amplitude => final_mods.amp += mod_val,
                ModDestination::FilterCutoff => final_mods.cutoff += mod_val,
                ModDestination::BellPosition => final_mods.bell_pos += mod_val,
                ModDestination::BellAmount => final_mods.bell_amount += mod_val,
                ModDestination::BellWidth => final_mods.bell_width += mod_val,
                ModDestination::Saturation => final_mods.saturation += mod_val,
            }
        }

        let num_tables = wavetable_set.tables.len().max(1) as f32;
        let wt_pos_scaler = num_tables;
        let final_morph_pos = base_morph_pos + (final_mods.wt_pos * wt_pos_scaler);

        // --- OPTIMIZED PITCH CALCULATION ---
        let final_frequency = self.base_frequency * POW2_LUT.get_interpolated(final_mods.pitch);

        let phase_inc = final_frequency / self.sample_rate * WAVETABLE_SIZE as f32;
        self.phase = (self.phase + phase_inc) % WAVETABLE_SIZE as f32;

        let mut layer_output = 0.0;
        for i in 0..4 {
            if wavetable_mixer_settings.layer_volumes[i] > 1e-6 {
                if let Some(table) = wavetable_set.tables.get(i) {
                    let layer_sample =
                        WavetableSet::get_interpolated_sample(&table.table, self.phase);
                    layer_output += layer_sample * wavetable_mixer_settings.layer_volumes[i];
                }
            }
        }

        let mut blended_output = 0.0;
        if wavetable_mixer_settings.layer_volumes[4] > 1e-6 {
            let blended_sample = wavetable_set.get_sample(final_morph_pos, self.phase);
            let phase_norm = self.phase / WAVETABLE_SIZE as f32;
            let bell_pos = (final_mods.bell_pos * 0.5 + 0.5).clamp(0.0, 1.0);
            let sigma =
                (0.15_f32 * POW2_LUT.get_interpolated(-2.0 * final_mods.bell_width)).clamp(0.02, 1.0);

            // --- OPTIMIZED BELL FILTER CALCULATION ---
            let exp_input = (phase_norm - bell_pos).powi(2) / (2.0 * sigma.powi(2));
            let bell_shape = EXP_LUT.get_interpolated(exp_input);
            let bell_effect = bell_shape * final_mods.bell_amount;

            let bell_filtered_sample = blended_sample * (1.0 + bell_effect);
            blended_output = bell_filtered_sample * wavetable_mixer_settings.layer_volumes[4];
        }

        let final_osc_sample = layer_output + blended_output;

        // --- OPTIMIZED SATURATION LOGIC ---
        let final_saturation_mod = final_mods.saturation.clamp(-1.0, 1.0);
        let total_drive = (final_saturation_mod * saturation_settings.drive * 10.0).max(0.0);
        let saturated_sample = (final_osc_sample * (1.0 + total_drive)).fast_tanh(); // Use fast_tanh

        let t = (total_drive / 10.0).clamp(0.0, 1.0);
        let p0 = 1.0;
        let p2 = 1.0 - saturation_settings.compensation_amount;
        let p1 = lerp(p0..=p2, saturation_settings.compensation_bias);
        let makeup_gain = (1.0 - t).powi(2) * p0 + 2.0 * (1.0 - t) * t * p1 + t.powi(2) * p2;

        let compensated_sample = saturated_sample * makeup_gain;

        let mut final_filter_settings = filter_settings;
        final_filter_settings.cutoff =
            (filter_settings.cutoff + final_mods.cutoff).clamp(0.0, 1.0);
        let filtered_sample = self
            .filter
            .process(compensated_sample, final_filter_settings, self.sample_rate);

        let output = filtered_sample * 0.5 * self.velocity * amp_env_val * (1.0 + final_mods.amp);

        // Store the final modulation values for this sample for UI feedback
        self.last_mod_values = final_mods;
        self.last_drive_value = total_drive / 10.0;

        output
    }

    fn note_on(&mut self, note: u8, velocity: u8) {
        self.note_id = note;
        self.base_frequency = 440.0 * 2.0_f32.powf((note as f32 - 69.0) / 12.0);
        self.velocity = velocity as f32 / 127.0;
        self.amp_adsr.note_on();
        self.filter_adsr.note_on();
        self.age = 0;
    }

    fn note_off(&mut self) {
        self.amp_adsr.note_off();
        self.filter_adsr.note_off();
    }
}

pub struct WavetableEngine {
    voices: Vec<Voice>,
    pub wavetable_set: Arc<RwLock<WavetableSet>>,
    is_polyphonic: bool,
    sample_rate: f32,
    lfo1: Lfo,
    lfo2: Lfo,
    pub wavetable_position_atomic: Arc<AtomicU32>,
    pub filter_settings: Arc<RwLock<FilterSettings>>,
    pub wavetable_mixer_settings: Arc<RwLock<WavetableMixerSettings>>,
    pub lfo_settings: Arc<RwLock<LfoSettings>>,
    pub lfo2_settings: Arc<RwLock<LfoSettings>>,
    pub mod_matrix: Arc<RwLock<Vec<ModRouting>>>,
    pub saturation_settings: Arc<RwLock<SaturationSettings>>,
    pub lfo_value_atomic: Arc<AtomicU32>,
    pub lfo2_value_atomic: Arc<AtomicU32>,
    pub env2_value_atomic: Arc<AtomicU32>,
    pub pitch_mod_atomic: Arc<AtomicU32>,
    pub bell_pos_atomic: Arc<AtomicU32>,
    pub bell_amount_atomic: Arc<AtomicU32>,
    pub bell_width_atomic: Arc<AtomicU32>,
    pub saturation_mod_atomic: Arc<AtomicU32>,
    pub final_wt_pos_atomic: Arc<AtomicU32>,
    pub final_cutoff_atomic: Arc<AtomicU32>,
    // Per-voice buffers for parallel processing
    voice_outputs: Vec<Vec<f32>>,
}

impl WavetableEngine {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        sample_rate: f32,
        wavetable_set: Arc<RwLock<WavetableSet>>,
        wavetable_position_atomic: Arc<AtomicU32>,
        filter_settings: Arc<RwLock<FilterSettings>>,
        wavetable_mixer_settings: Arc<RwLock<WavetableMixerSettings>>,
        lfo_settings: Arc<RwLock<LfoSettings>>,
        lfo2_settings: Arc<RwLock<LfoSettings>>,
        mod_matrix: Arc<RwLock<Vec<ModRouting>>>,
        saturation_settings: Arc<RwLock<SaturationSettings>>,
        lfo_value_atomic: Arc<AtomicU32>,
        lfo2_value_atomic: Arc<AtomicU32>,
        env2_value_atomic: Arc<AtomicU32>,
        pitch_mod_atomic: Arc<AtomicU32>,
        bell_pos_atomic: Arc<AtomicU32>,
        bell_amount_atomic: Arc<AtomicU32>,
        bell_width_atomic: Arc<AtomicU32>,
        saturation_mod_atomic: Arc<AtomicU32>,
        final_wt_pos_atomic: Arc<AtomicU32>,
        final_cutoff_atomic: Arc<AtomicU32>,
    ) -> Self {
        let voices = (0..NUM_VOICES).map(|_| Voice::new(sample_rate)).collect();

        Self {
            voices,
            wavetable_set,
            is_polyphonic: true,
            sample_rate,
            lfo1: Lfo::new(sample_rate),
            lfo2: Lfo::new(sample_rate),
            wavetable_position_atomic,
            filter_settings,
            wavetable_mixer_settings,
            lfo_settings,
            lfo2_settings,
            mod_matrix,
            saturation_settings,
            lfo_value_atomic,
            lfo2_value_atomic,
            env2_value_atomic,
            pitch_mod_atomic,
            bell_pos_atomic,
            bell_amount_atomic,
            bell_width_atomic,
            saturation_mod_atomic,
            final_wt_pos_atomic,
            final_cutoff_atomic,
            voice_outputs: vec![vec![0.0; 2048]; NUM_VOICES], // Max buffer size
        }
    }

    fn get_lfo_freq(sample_rate: f32, settings: LfoSettings, transport_len_samples: usize) -> f32 {
        match settings.mode {
            LfoRateMode::Hz => settings.hz_rate,
            LfoRateMode::Sync => {
                if transport_len_samples > 0 {
                    (sample_rate / transport_len_samples as f32) * settings.sync_rate
                } else {
                    0.0
                }
            }
        }
    }
}

impl Engine for WavetableEngine {
    fn process(
        &mut self,
        output_buffer: &mut [f32],
        transport_len_samples: usize,
        midi_cc_values: &Arc<[[AtomicU32; 128]; 16]>,
    ) {
        let block_size = output_buffer.len();
        output_buffer.fill(0.0); // Clear the output buffer initially

        // --- Read all shared data once per block ---
        let lfo1_settings = *self.lfo_settings.read().unwrap();
        let lfo2_settings = *self.lfo2_settings.read().unwrap();
        let filter_settings = *self.filter_settings.read().unwrap();
        let wavetable_mixer_settings = *self.wavetable_mixer_settings.read().unwrap();
        let saturation_settings = *self.saturation_settings.read().unwrap();
        let mod_matrix = self.mod_matrix.read().unwrap();
        let wavetable_set_guard = match self.wavetable_set.read() {
            Ok(guard) => guard,
            Err(_) => return, // Failed to get lock, exit early
        };

        // --- Pre-calculate LFOs for the entire block ---
        let mut lfo1_output = vec![0.0; block_size];
        let mut lfo2_output = vec![0.0; block_size];
        let lfo1_freq = Self::get_lfo_freq(self.sample_rate, lfo1_settings, transport_len_samples);
        let lfo2_freq = Self::get_lfo_freq(self.sample_rate, lfo2_settings, transport_len_samples);
        for i in 0..block_size {
            lfo1_output[i] =
                self.lfo1
                    .process(lfo1_freq, lfo1_settings.waveform, &wavetable_set_guard);
            lfo2_output[i] =
                self.lfo2
                    .process(lfo2_freq, lfo2_settings.waveform, &wavetable_set_guard);
        }

        // --- Resize voice output buffers if necessary ---
        if self.voice_outputs[0].len() != block_size {
            for buffer in &mut self.voice_outputs {
                buffer.resize(block_size, 0.0);
            }
        }

        // --- Parallel Processing of Voices ---
        self.voices
            .par_iter_mut()
            .zip(self.voice_outputs.par_iter_mut())
            .for_each(|(voice, voice_output_buffer)| {
                if !voice.is_active() {
                    // Ensure buffer is clear if voice is inactive
                    voice_output_buffer.fill(0.0);
                    return;
                }

                let base_morph_pos =
                    self.wavetable_position_atomic.load(Ordering::Relaxed) as f32 / 1_000_000.0;

                // Process this voice for the entire block
                for i in 0..block_size {
                    // Pre-calculate modulation from non-voice-specific sources for this sample
                    let mut base_mods = ModulationValues::default();
                    for routing in mod_matrix.iter() {
                        let source_val = match routing.source {
                            ModSource::Lfo1 => lfo1_output[i],
                            ModSource::Lfo2 => lfo2_output[i],
                            ModSource::Static => 1.0,
                            ModSource::MidiCC(id) => {
                                midi_cc_values[id.channel as usize][id.cc as usize].load(Ordering::Relaxed) as f32 / 1_000_000.0
                            }
                            ModSource::Env2 | ModSource::Velocity => continue,
                        };
                        let mod_val = source_val * routing.amount;
                        match routing.destination {
                            ModDestination::WavetablePosition => base_mods.wt_pos += mod_val,
                            ModDestination::Pitch => base_mods.pitch += mod_val,
                            ModDestination::Amplitude => base_mods.amp += mod_val,
                            ModDestination::FilterCutoff => base_mods.cutoff += mod_val,
                            ModDestination::BellPosition => base_mods.bell_pos += mod_val,
                            ModDestination::BellAmount => base_mods.bell_amount += mod_val,
                            ModDestination::BellWidth => base_mods.bell_width += mod_val,
                            ModDestination::Saturation => base_mods.saturation += mod_val,
                        }
                    }

                    voice_output_buffer[i] = voice.process_sample(
                        &wavetable_set_guard,
                        filter_settings,
                        wavetable_mixer_settings,
                        saturation_settings,
                        base_morph_pos,
                        &mod_matrix,
                        base_mods,
                    );
                }
            });

        // --- Final Mixdown ---
        // Sum the outputs of all voices into the main output buffer
        for voice_buffer in &self.voice_outputs {
            for i in 0..block_size {
                output_buffer[i] += voice_buffer[i];
            }
        }

        // --- Update UI Atomics (Unconditional) ---
        // This now happens every frame, regardless of voice activity, for instant UI feedback.
        let oldest_voice = self.voices.iter().filter(|v| v.is_active()).min_by_key(|v| v.age);

        // Get modulation values, either from a live voice or by simulating idle modulation.
        let (mods, last_env2, last_drive) = if let Some(voice) = oldest_voice {
            (voice.last_mod_values, voice.last_env2_value, voice.last_drive_value)
        } else {
            // When idle, we simulate modulation from LFOs and static sources for the visualizer.
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
                    _ => 0.0, // Env2 and Velocity are 0 when idle
                };
                let mod_val = source_val * routing.amount;
                match routing.destination {
                    ModDestination::WavetablePosition => idle_mods.wt_pos += mod_val,
                    ModDestination::Pitch => idle_mods.pitch += mod_val,
                    ModDestination::FilterCutoff => idle_mods.cutoff += mod_val,
                    ModDestination::BellPosition => idle_mods.bell_pos += mod_val,
                    ModDestination::BellAmount => idle_mods.bell_amount += mod_val,
                    ModDestination::BellWidth => idle_mods.bell_width += mod_val,
                    ModDestination::Saturation => idle_mods.saturation += mod_val,
                    _ => {}
                }
            }
            // Calculate final drive for saturation bar when idle
            let final_saturation_mod = idle_mods.saturation.clamp(-1.0, 1.0);
            let total_drive = (final_saturation_mod * saturation_settings.drive * 10.0).max(0.0);

            (idle_mods, 0.0, total_drive / 10.0)
        };

        // Raw modulation values (-1 to 1, encoded to 0-1)
        self.pitch_mod_atomic.store(((mods.pitch * 0.5 + 0.5).clamp(0.0, 1.0) * 1_000_000.0) as u32, Ordering::Relaxed);
        self.bell_pos_atomic.store(((mods.bell_pos * 0.5 + 0.5).clamp(0.0, 1.0) * 1_000_000.0) as u32, Ordering::Relaxed);
        self.bell_amount_atomic.store(((mods.bell_amount * 0.5 + 0.5).clamp(0.0, 1.0) * 1_000_000.0) as u32, Ordering::Relaxed);
        self.bell_width_atomic.store(((mods.bell_width * 0.5 + 0.5).clamp(0.0, 1.0) * 1_000_000.0) as u32, Ordering::Relaxed);

        // Other modulator values
        self.lfo_value_atomic.store(((lfo1_output.last().unwrap_or(&0.0) * 0.5 + 0.5) * 1_000_000.0) as u32, Ordering::Relaxed);
        self.lfo2_value_atomic.store(((lfo2_output.last().unwrap_or(&0.0) * 0.5 + 0.5) * 1_000_000.0) as u32, Ordering::Relaxed);
        self.env2_value_atomic.store((last_env2 * 1_000_000.0) as u32, Ordering::Relaxed);
        self.saturation_mod_atomic.store((last_drive.clamp(0.0, 1.0) * 1_000_000.0) as u32, Ordering::Relaxed);

        // Final, absolute values for visualizer
        let base_wt_pos = self.wavetable_position_atomic.load(Ordering::Relaxed) as f32 / 1_000_000.0;
        let num_tables = wavetable_set_guard.tables.len().max(1) as f32;
        let final_wt_pos = base_wt_pos + (mods.wt_pos * num_tables);
        self.final_wt_pos_atomic.store((final_wt_pos.clamp(0.0, (num_tables - 1.0).max(0.0)) * 1_000_000.0) as u32, Ordering::Relaxed);

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

        if self.is_polyphonic {
            // Find the voice with the greatest age (either inactive or oldest in release)
            if let Some(voice) = self.voices.iter_mut().max_by_key(|v| v.age) {
                voice.note_on(note, velocity);
            }
        } else {
            // For monophonic mode, we always retrigger the main voice.
            // First, iterate and tell all other voices to release.
            for (i, v) in self.voices.iter_mut().enumerate() {
                if i != 0 {
                    v.note_off();
                }
            }

            // Now that the first borrow is finished, we can create a new one.
            if let Some(voice) = self.voices.get_mut(0) {
                voice.note_on(note, velocity);
            }
        }
    }

    fn note_off(&mut self, note: u8) {
        for voice in self.voices.iter_mut().filter(|v| v.note_id == note && v.is_active()) {
            voice.note_off();
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

    fn set_polyphonic(&mut self, poly: bool) {
        self.is_polyphonic = poly;
        if !poly {
            // Release all voices except the youngest one.
            let mut active_voices: Vec<&mut Voice> = self.voices.iter_mut().filter(|v| v.is_active()).collect();
            if active_voices.len() > 1 {
                active_voices.sort_by_key(|v| v.age);
                for voice in active_voices.into_iter().rev().skip(1) {
                    voice.note_off();
                }
            }
        }
    }

    fn set_wavetable(&mut self, slot_index: usize, audio_data: Arc<Vec<f32>>, name: String) {
        if let Ok(mut guard) = self.wavetable_set.write() {
            if let Some(wavetable) = guard.tables.get_mut(slot_index) {
                if !name.is_empty() {
                    wavetable.name = name;
                }
                wavetable.table = (*audio_data).clone();
            }
        }
    }

    fn reset_to_defaults(&mut self) {
        if let Ok(mut guard) = self.wavetable_set.write() {
            *guard = WavetableSet::new_basic();
        }
    }
}