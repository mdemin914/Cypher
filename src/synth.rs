// src/synth.rs
use crate::sampler_engine;
use crate::settings::MidiControlId;
use crate::wavetable_engine::{
    self, SaturationSettings, WavetableEngine, WavetableMixerSettings, WavetableSet,
};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::f32::consts::{PI, TAU};
use std::sync::atomic::{AtomicU32, AtomicUsize};
use std::sync::{Arc, RwLock};

// --- Algorithmic Optimizations: Fast Approximations & LUTs ---

/// A trait for providing a fast, approximate hyperbolic tangent function.
pub trait FastTanh {
    fn fast_tanh(self) -> Self;
}

impl FastTanh for f32 {
    /// A 3rd-order polynomial approximation of tanh(x).
    /// It is much faster than the standard library's `tanh` function.
    #[inline(always)]
    fn fast_tanh(self) -> Self {
        let x2 = self * self;
        // This is a Pade approximant, not a Taylor expansion.
        // It's chosen for its behavior across a wider range.
        self * (27.0 + x2) / (27.0 + 9.0 * x2)
    }
}

const LUT_SIZE: usize = 4096;

/// A generic lookup table for expensive functions.
pub struct Lut {
    table: [f32; LUT_SIZE],
    min_input: f32,
    max_input: f32,
    input_range: f32,
}

impl Lut {
    /// Creates a new LUT by applying a function `f` over a given input range.
    fn new<F>(min_input: f32, max_input: f32, f: F) -> Self
    where
        F: Fn(f32) -> f32,
    {
        let mut table = [0.0; LUT_SIZE];
        let input_range = max_input - min_input;
        for i in 0..LUT_SIZE {
            let phase = i as f32 / (LUT_SIZE - 1) as f32;
            let input = min_input + phase * input_range;
            table[i] = f(input);
        }
        Self {
            table,
            min_input,
            max_input,
            input_range,
        }
    }

    /// Gets a value from the LUT using linear interpolation.
    #[inline(always)]
    pub fn get_interpolated(&self, input: f32) -> f32 {
        let clamped_input = input.clamp(self.min_input, self.max_input);
        let normalized_pos = (clamped_input - self.min_input) / self.input_range;
        let scaled_pos = normalized_pos * (LUT_SIZE - 1) as f32;

        let idx_floor = scaled_pos.floor() as usize;
        let frac = scaled_pos.fract();

        // Check bounds to prevent reading past the end of the array
        if idx_floor >= LUT_SIZE - 1 {
            return self.table[LUT_SIZE - 1];
        }

        let val1 = self.table[idx_floor];
        let val2 = self.table[idx_floor + 1];

        val1 + frac * (val2 - val1) // Linear interpolation
    }
}

// Static lookup table for pitch modulation: 2.0.powf(x)
// Covers a range of +/- 5 octaves (pitch_mod = -60.0 to 60.0 semitones)
pub static POW2_LUT: Lazy<Lut> =
    Lazy::new(|| Lut::new(-60.0, 60.0, |x| 2.0_f32.powf(x / 12.0)));

// Static lookup table for the bell filter calculation: (-x).exp()
// A range of 0.0 to 10.0 is more than sufficient, as exp(-10) is tiny.
pub static EXP_LUT: Lazy<Lut> = Lazy::new(|| Lut::new(0.0, 10.0, |x| (-x).exp()));

pub const WAVETABLE_SIZE: usize = 2048;

// --- Generic Engine Trait ---
pub trait Engine {
    /// Processes a block of audio samples, writing the output into `output_buffer`.
    fn process(
        &mut self,
        output_buffer: &mut [f32],
        musical_bar_len: usize,
        midi_cc_values: &Arc<[[AtomicU32; 128]; 16]>,
    );
    fn note_on(&mut self, note: u8, velocity: u8);
    fn note_off(&mut self, note: u8);
    fn set_polyphonic(&mut self, poly: bool);
    fn set_amp_adsr(&mut self, settings: AdsrSettings);
    fn set_filter_adsr(&mut self, settings: AdsrSettings);
    fn reset_to_defaults(&mut self);
    fn set_wavetable(&mut self, slot_index: usize, audio_data: Arc<Vec<f32>>, name: String);
}

// --- Synth Engine Enum ---
pub enum SynthEngine {
    Wavetable(WavetableEngine),
    Sampler(sampler_engine::SamplerEngine),
}

impl Engine for SynthEngine {
    fn process(
        &mut self,
        output_buffer: &mut [f32],
        musical_bar_len: usize,
        midi_cc_values: &Arc<[[AtomicU32; 128]; 16]>,
    ) {
        match self {
            SynthEngine::Wavetable(e) => e.process(output_buffer, musical_bar_len, midi_cc_values),
            SynthEngine::Sampler(e) => e.process(output_buffer, musical_bar_len, midi_cc_values),
        }
    }

    fn note_on(&mut self, note: u8, velocity: u8) {
        match self {
            SynthEngine::Wavetable(e) => e.note_on(note, velocity),
            SynthEngine::Sampler(e) => e.note_on(note, velocity),
        }
    }

    fn note_off(&mut self, note: u8) {
        match self {
            SynthEngine::Wavetable(e) => e.note_off(note),
            SynthEngine::Sampler(e) => e.note_off(note),
        }
    }

    fn set_polyphonic(&mut self, poly: bool) {
        match self {
            SynthEngine::Wavetable(e) => e.set_polyphonic(poly),
            SynthEngine::Sampler(e) => e.set_polyphonic(poly),
        }
    }

    fn set_amp_adsr(&mut self, settings: AdsrSettings) {
        match self {
            SynthEngine::Wavetable(e) => e.set_amp_adsr(settings),
            SynthEngine::Sampler(e) => e.set_amp_adsr(settings),
        }
    }

    fn set_filter_adsr(&mut self, settings: AdsrSettings) {
        match self {
            SynthEngine::Wavetable(e) => e.set_filter_adsr(settings),
            SynthEngine::Sampler(e) => e.set_filter_adsr(settings),
        }
    }

    fn reset_to_defaults(&mut self) {
        match self {
            SynthEngine::Wavetable(e) => e.reset_to_defaults(),
            SynthEngine::Sampler(e) => e.reset_to_defaults(),
        }
    }

    fn set_wavetable(&mut self, slot_index: usize, audio_data: Arc<Vec<f32>>, name: String) {
        match self {
            SynthEngine::Wavetable(e) => e.set_wavetable(slot_index, audio_data, name),
            SynthEngine::Sampler(e) => e.set_wavetable(slot_index, audio_data, name), // (no-op)
        }
    }
}

// --- Main Synth Struct (unchanged logic, but now holds the enum) ---
pub struct Synth {
    pub engines: [SynthEngine; 2],
}

impl Synth {
    pub fn new(sample_rate: f32, engine_params: [EngineWithVolumeAndPeak; 2]) -> Self {
        let [(_, _, params0), (_, _, params1)] = engine_params;

        let engines = [
            Self::create_engine(sample_rate, params0),
            Self::create_engine(sample_rate, params1),
        ];
        Self { engines }
    }

    pub fn create_engine(sample_rate: f32, params: EngineParamsUnion) -> SynthEngine {
        match params {
            EngineParamsUnion::Wavetable(p) => {
                SynthEngine::Wavetable(wavetable_engine::WavetableEngine::new(
                    sample_rate, p.0, p.1, p.2, p.3, p.4, p.5, p.6, p.7, p.8, p.9, p.10, p.11, p.12,
                    p.13, p.14, p.15, p.16, p.17,
                ))
            }
            EngineParamsUnion::Sampler(p) => {
                SynthEngine::Sampler(sampler_engine::SamplerEngine::new(
                    sample_rate, p.0, p.1, p.2, p.3, p.4, p.5, p.6, p.7, p.8, p.9, p.10, p.11, p.12
                ))
            }
        }
    }

    /// Processes both engines, filling their respective output buffers.
    pub fn process(
        &mut self,
        engine_0_output: &mut [f32],
        engine_1_output: &mut [f32],
        musical_bar_len: usize,
        midi_cc_values: &Arc<[[AtomicU32; 128]; 16]>,
    ) {
        self.engines[0].process(engine_0_output, musical_bar_len, midi_cc_values);
        self.engines[1].process(engine_1_output, musical_bar_len, midi_cc_values);
    }

    pub fn note_on(&mut self, note: u8, velocity: u8) {
        self.engines[0].note_on(note, velocity);
        self.engines[1].note_on(note, velocity);
    }

    pub fn note_off(&mut self, note: u8) {
        self.engines[0].note_off(note);
        self.engines[1].note_off(note);
    }
}

// --- Shared Helper Structs and Enums (still live here) ---

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct AdsrSettings {
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
}

impl Default for AdsrSettings {
    fn default() -> Self {
        Self {
            attack: 0.01,
            decay: 0.1,
            sustain: 0.8,
            release: 0.2,
        }
    }
}

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum AdsrState {
    Idle,
    Attack,
    Decay,
    Sustain,
    Release,
}

#[derive(Clone, Copy, Debug)]
pub struct Adsr {
    pub settings: AdsrSettings,
    pub state: AdsrState,
    pub current_level: f32,
    sample_rate: f32,
}

impl Adsr {
    pub fn new(settings: AdsrSettings, sample_rate: f32) -> Self {
        Self {
            settings,
            state: AdsrState::Idle,
            current_level: 0.0,
            sample_rate,
        }
    }

    pub fn set_settings(&mut self, settings: AdsrSettings) {
        self.settings = settings;
    }

    pub fn note_on(&mut self) {
        self.state = AdsrState::Attack;
    }

    pub fn note_off(&mut self) {
        if self.state != AdsrState::Idle {
            self.state = AdsrState::Release;
        }
    }

    pub fn reset(&mut self) {
        self.state = AdsrState::Idle;
        self.current_level = 0.0;
    }

    pub fn process(&mut self) -> f32 {
        match self.state {
            AdsrState::Idle => 0.0,
            AdsrState::Attack => {
                if self.settings.attack > 0.0 {
                    let attack_rate = 1.0 / (self.settings.attack * self.sample_rate);
                    self.current_level += attack_rate;
                } else {
                    self.current_level = 1.0;
                }

                if self.current_level >= 1.0 {
                    self.current_level = 1.0;
                    self.state = AdsrState::Decay;
                }
                self.current_level
            }
            AdsrState::Decay => {
                if self.settings.decay > 0.0 {
                    let decay_rate =
                        (1.0 - self.settings.sustain) / (self.settings.decay * self.sample_rate);
                    self.current_level -= decay_rate;
                } else {
                    self.current_level = self.settings.sustain;
                }

                if self.current_level <= self.settings.sustain {
                    self.current_level = self.settings.sustain;
                    self.state = AdsrState::Sustain;
                }
                self.current_level
            }
            AdsrState::Sustain => self.settings.sustain,
            AdsrState::Release => {
                if self.settings.release > 0.0 {
                    let release_rate =
                        self.current_level / (self.settings.release * self.sample_rate);
                    self.current_level -= release_rate;
                } else {
                    self.current_level = 0.0;
                }

                if self.current_level <= 0.0 {
                    self.current_level = 0.0;
                    self.state = AdsrState::Idle;
                }
                self.current_level
            }
        }
    }
}

pub struct Filter {
    z1: f32,
    z2: f32,
}

impl Filter {
    pub fn new() -> Self {
        Self { z1: 0.0, z2: 0.0 }
    }

    pub fn process(&mut self, input: f32, settings: FilterSettings, sample_rate: f32) -> f32 {
        let cutoff_freq = 20.0 * (20000.0f32 / 20.0).powf(settings.cutoff); // Logarithmic mapping
        let g = (PI * cutoff_freq / sample_rate).tan();
        let k = 2.0 - 2.0 * settings.resonance.clamp(0.0, 0.99); // Resonance to Q mapping

        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        let a3 = g * a2;

        let v3 = input - self.z2;
        let v1 = a1 * self.z1 + a2 * v3;
        let v2 = self.z2 + a2 * self.z1 + a3 * v3;
        self.z1 = 2.0 * v1 - self.z1;
        self.z2 = 2.0 * v2 - self.z2;

        match settings.mode {
            FilterMode::LowPass => v2,
            FilterMode::HighPass => input - k * v1 - v2,
            FilterMode::BandPass => v1,
        }
    }
}

pub struct Lfo {
    phase: f32,
    last_output: f32,
    sample_rate: f32,
}

impl Lfo {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            phase: 0.0,
            last_output: 0.0,
            sample_rate,
        }
    }

    pub fn process(
        &mut self,
        frequency: f32,
        waveform: LfoWaveform,
        wavetable_set: &crate::wavetable_engine::WavetableSet,
    ) -> f32 {
        let phase_inc = frequency / self.sample_rate;
        self.phase = (self.phase + phase_inc) % 1.0;

        let output = match waveform {
            LfoWaveform::Sine => (self.phase * TAU).sin(),
            LfoWaveform::Triangle => 1.0 - 4.0 * (self.phase - 0.5).abs(),
            LfoWaveform::Saw => 2.0 * self.phase - 1.0,
            LfoWaveform::InvSaw => 1.0 - 2.0 * self.phase,
            LfoWaveform::Square => if self.phase < 0.5 { 1.0 } else { -1.0 },
            LfoWaveform::Random => {
                if self.phase < phase_inc {
                    self.last_output = rand::random::<f32>() * 2.0 - 1.0;
                }
                self.last_output
            }
            LfoWaveform::Wavetable1 => Self::process_wavetable(self.phase, 0, wavetable_set),
            LfoWaveform::Wavetable2 => Self::process_wavetable(self.phase, 1, wavetable_set),
            LfoWaveform::Wavetable3 => Self::process_wavetable(self.phase, 2, wavetable_set),
            LfoWaveform::Wavetable4 => Self::process_wavetable(self.phase, 3, wavetable_set),
        };
        output
    }

    fn process_wavetable(
        lfo_phase: f32, // this is 0.0 to 1.0
        table_index: usize,
        wavetable_set: &crate::wavetable_engine::WavetableSet,
    ) -> f32 {
        if let Some(table) = wavetable_set.tables.get(table_index) {
            let table_data = &table.table;
            if !table_data.is_empty() {
                // Scale LFO phase (0..1) to wavetable phase (0..table_len)
                let wt_phase = lfo_phase * table_data.len() as f32;
                return crate::wavetable_engine::WavetableSet::get_interpolated_sample(
                    table_data,
                    wt_phase,
                );
            }
        }
        0.0 // Fallback if table doesn't exist or is empty
    }

    pub fn reset_phase(&mut self) {
        self.phase = 0.0;
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub enum FilterMode {
    LowPass,
    HighPass,
    BandPass,
}
impl FilterMode {
    pub const ALL: [FilterMode; 3] =
        [FilterMode::LowPass, FilterMode::HighPass, FilterMode::BandPass];
}
impl std::fmt::Display for FilterMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FilterMode::LowPass => write!(f, "Low Pass"),
            FilterMode::HighPass => write!(f, "High Pass"),
            FilterMode::BandPass => write!(f, "Band Pass"),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub struct FilterSettings {
    pub mode: FilterMode,
    pub cutoff: f32,
    pub resonance: f32,
}
impl Default for FilterSettings {
    fn default() -> Self {
        Self {
            mode: FilterMode::LowPass,
            cutoff: 0.99,
            resonance: 0.0,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ModSource {
    Lfo1,
    Lfo2,
    Env2,
    Velocity,
    Static,
    MidiCC(MidiControlId),
}
impl ModSource {
    pub const ALL: [ModSource; 5] = [
        ModSource::Lfo1,
        ModSource::Lfo2,
        ModSource::Env2,
        ModSource::Velocity,
        ModSource::Static,
    ];
}
impl std::fmt::Display for ModSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ModSource::Lfo1 => write!(f, "Lfo1"),
            ModSource::Lfo2 => write!(f, "Lfo2"),
            ModSource::Env2 => write!(f, "Env2"),
            ModSource::Velocity => write!(f, "Velocity"),
            ModSource::Static => write!(f, "Static"),
            ModSource::MidiCC(id) => write!(f, "MIDI CC {} (Ch {})", id.cc, id.channel + 1),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub enum LfoWaveform {
    Sine,
    Triangle,
    Saw,
    InvSaw,
    Square,
    Random,
    Wavetable1,
    Wavetable2,
    Wavetable3,
    Wavetable4,
}
impl LfoWaveform {
    pub const ALL: [LfoWaveform; 10] = [
        LfoWaveform::Sine,
        LfoWaveform::Triangle,
        LfoWaveform::Saw,
        LfoWaveform::InvSaw,
        LfoWaveform::Square,
        LfoWaveform::Random,
        LfoWaveform::Wavetable1,
        LfoWaveform::Wavetable2,
        LfoWaveform::Wavetable3,
        LfoWaveform::Wavetable4,
    ];
}
impl std::fmt::Display for LfoWaveform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LfoWaveform::Sine => write!(f, "Sine"),
            LfoWaveform::Triangle => write!(f, "Triangle"),
            LfoWaveform::Saw => write!(f, "Saw"),
            LfoWaveform::InvSaw => write!(f, "InvSaw"),
            LfoWaveform::Square => write!(f, "Square"),
            LfoWaveform::Random => write!(f, "Random"),
            LfoWaveform::Wavetable1 => write!(f, "Wavetable 1"),
            LfoWaveform::Wavetable2 => write!(f, "Wavetable 2"),
            LfoWaveform::Wavetable3 => write!(f, "Wavetable 3"),
            LfoWaveform::Wavetable4 => write!(f, "Wavetable 4"),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub enum LfoRateMode {
    Hz,
    Sync,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ModDestination {
    WavetablePosition,
    Pitch,
    Amplitude,
    FilterCutoff,
    BellPosition,
    BellAmount,
    BellWidth,
    Saturation,
}
impl ModDestination {
    pub const ALL: [ModDestination; 8] = [
        ModDestination::WavetablePosition,
        ModDestination::Pitch,
        ModDestination::Amplitude,
        ModDestination::FilterCutoff,
        ModDestination::BellPosition,
        ModDestination::BellAmount,
        ModDestination::BellWidth,
        ModDestination::Saturation,
    ];
}
impl std::fmt::Display for ModDestination {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ModDestination::WavetablePosition => write!(f, "WT Position"),
            ModDestination::Pitch => write!(f, "Pitch"),
            ModDestination::Amplitude => write!(f, "Amplitude"),
            ModDestination::FilterCutoff => write!(f, "Filter Cutoff"),
            ModDestination::BellPosition => write!(f, "Bell Position"),
            ModDestination::BellAmount => write!(f, "Bell Amount"),
            ModDestination::BellWidth => write!(f, "Bell Width"),
            ModDestination::Saturation => write!(f, "Saturation"),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ModRouting {
    pub source: ModSource,
    pub destination: ModDestination,
    pub amount: f32, // -1.0 to 1.0
}
impl Default for ModRouting {
    fn default() -> Self {
        Self {
            source: ModSource::Lfo1,
            destination: ModDestination::WavetablePosition,
            amount: 0.0,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub struct LfoSettings {
    pub waveform: LfoWaveform,
    pub hz_rate: f32,
    pub sync_rate: f32,
    pub mode: LfoRateMode,
    pub retrigger: bool,
}
impl Default for LfoSettings {
    fn default() -> Self {
        Self {
            waveform: LfoWaveform::Sine,
            hz_rate: 2.0,   // 2 Hz default
            sync_rate: 1.0, // 1/4 note default
            mode: LfoRateMode::Hz,
            retrigger: false,
        }
    }
}

// --- Parameter Passing Structs ---

#[derive(Clone, Debug)]
pub struct WavetableParams(
    pub Arc<RwLock<WavetableSet>>,
    pub Arc<AtomicU32>, // Wavetable Position
    pub Arc<RwLock<FilterSettings>>,
    pub Arc<RwLock<WavetableMixerSettings>>,
    pub Arc<RwLock<LfoSettings>>,
    pub Arc<RwLock<LfoSettings>>, // LFO2
    pub Arc<RwLock<Vec<ModRouting>>>,
    pub Arc<RwLock<SaturationSettings>>,
    pub Arc<AtomicU32>, // LFO Value
    pub Arc<AtomicU32>, // LFO2 Value
    pub Arc<AtomicU32>, // Env2 Value
    pub Arc<AtomicU32>, // Pitch Mod
    pub Arc<AtomicU32>, // Bell Pos
    pub Arc<AtomicU32>, // Bell Amount
    pub Arc<AtomicU32>, // Bell Width
    pub Arc<AtomicU32>, // Saturation Mod Value
    pub Arc<AtomicU32>, // Final WT Pos (Feedback)
    pub Arc<AtomicU32>, // Final Cutoff (Feedback)
);

#[derive(Clone, Debug)]
pub struct SamplerParams(
    pub Arc<RwLock<FilterSettings>>,
    pub Arc<RwLock<LfoSettings>>,
    pub Arc<RwLock<LfoSettings>>, // LFO2
    pub Arc<RwLock<Vec<ModRouting>>>,
    pub Arc<RwLock<SaturationSettings>>,
    pub Arc<AtomicU32>,                  // LFO Value
    pub Arc<AtomicU32>,                  // LFO2 Value
    pub Arc<AtomicU32>,                  // Env2 Value
    pub Arc<AtomicU32>,                  // Pitch Mod
    pub Arc<AtomicU32>,                  // Amp Mod
    pub Arc<AtomicU32>,                  // Saturation Mod Value
    pub Arc<AtomicU32>,                  // Final Cutoff (Feedback)
    pub Arc<AtomicUsize>,                // Last triggered slot index
);

#[derive(Clone, Debug)]
pub enum EngineParamsUnion {
    Wavetable(WavetableParams),
    Sampler(SamplerParams),
}

pub type EngineWithVolumeAndPeak = (
    Arc<AtomicU32>, // Volume
    Arc<AtomicU32>, // Peak Meter
    EngineParamsUnion,
);