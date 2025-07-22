// src/audio_engine.rs
use crate::looper::{LooperState, SharedLooperState, WAVEFORM_DOWNSAMPLE_SIZE, NUM_LOOPERS};
use crate::mixer::TrackMixerState;
use crate::sampler_engine::{self, NUM_SAMPLE_SLOTS};
use crate::synth::{
    AdsrSettings, Engine, EngineParamsUnion, EngineWithVolumeAndPeak, LfoRateMode, SamplerParams,
    Synth, SynthEngine, WavetableParams,
};
use crate::wavetable_engine;
use anyhow::Result;
use ringbuf::HeapConsumer;
use rubato::{
    Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU32, AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Instant;

const LOOPER_ARM_THRESHOLD: f32 = 0.05;

#[derive(PartialEq, Clone, Copy)]
pub enum TransportState {
    Playing,
    Paused,
}

struct Limiter {
    attack_coeffs: f32,
    envelope: f32,
    gain_reduction_db: Arc<AtomicU32>,
}

impl Limiter {
    fn new(sample_rate: f32, gain_reduction_db: Arc<AtomicU32>) -> Self {
        let attack_ms = 0.01;
        Self {
            attack_coeffs: (-(1.0 / (attack_ms * 0.001 * sample_rate))).exp(),
            envelope: 0.0,
            gain_reduction_db,
        }
    }

    fn process(&mut self, input: f32, threshold: f32, release_coeffs: f32) -> f32 {
        let input_abs = input.abs();

        self.envelope = if input_abs > self.envelope {
            self.attack_coeffs * (self.envelope - input_abs) + input_abs
        } else {
            release_coeffs * (self.envelope - input_abs) + input_abs
        };
        self.envelope = self.envelope.max(1e-6);

        let gain = if self.envelope > threshold {
            threshold / self.envelope
        } else {
            1.0
        };

        let reduction_db = 20.0 * gain.log10();
        let reduction_scaled = (-reduction_db.clamp(-24.0, 0.0) * 1_000_000.0) as u32;
        self.gain_reduction_db
            .store(reduction_scaled, Ordering::Relaxed);

        input * gain
    }
}

struct Looper {
    shared_state: SharedLooperState,
    audio: Vec<f32>,
    pending_command: bool,
    stop_is_queued: bool,
    cycles_recorded: u32,
    playhead: usize,
    samples_since_waveform_update: usize,
}

impl Looper {
    fn new(shared_state: SharedLooperState) -> Self {
        Self {
            shared_state,
            audio: Vec::new(),
            pending_command: false,
            stop_is_queued: false,
            cycles_recorded: 0,
            playhead: 0,
            samples_since_waveform_update: 0,
        }
    }
}

#[derive(Default, Clone)]
struct SamplerPad {
    audio: Arc<Vec<f32>>,
    playhead: usize,
    is_playing: bool,
    volume: f32,
}

#[derive(Debug, Clone)]
pub struct MidiMessage {
    pub status: u8,
    pub data1: u8,
    pub data2: u8,
}

#[derive(Debug, Clone)]
pub enum AudioCommand {
    ToggleLooper(usize),
    ArmLooper(usize),
    ClearLooper(usize),
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
    SetMasterVolume(f32),
    SetLimiterThreshold(f32),
    ToggleLimiter,
    SetLimiterReleaseMode(LfoRateMode),
    SetLimiterReleaseMs(f32),
    SetLimiterReleaseSync(f32),
    PlayTransport,
    PauseTransport,
    ClearAllAndPlay,
}

pub struct AudioEngine {
    command_consumer: HeapConsumer<AudioCommand>,
    pub input_consumer: HeapConsumer<f32>,
    loopers: Vec<Looper>,
    pub synth: Synth,
    sampler_pads: Vec<SamplerPad>,
    pub synth_is_active: Arc<AtomicBool>,
    pub audio_input_is_armed: Arc<AtomicBool>,
    pub audio_input_is_monitored: Arc<AtomicBool>,
    pub sampler_is_active: Arc<AtomicBool>,
    selected_midi_channel: Arc<AtomicU8>,
    pub transport_playhead: Arc<AtomicUsize>,
    pub transport_len_samples: Arc<AtomicUsize>,
    pub transport_is_playing: Arc<AtomicBool>,
    transport_state: TransportState,
    sample_rate: f32,
    playing_pads: Arc<AtomicU16>,
    pub track_mixer_state: Arc<RwLock<TrackMixerState>>,
    pub peak_meters: Arc<[AtomicU32; NUM_LOOPERS]>,
    cpu_load: Arc<AtomicU32>,
    input_peak_meter: Arc<AtomicU32>,
    pub input_latency_compensation_ms: Arc<AtomicU32>,
    sampler_volume: Arc<AtomicU32>,
    sampler_peak_meter: Arc<AtomicU32>,
    master_volume: Arc<AtomicU32>,
    limiter_is_active: Arc<AtomicBool>,
    limiter_threshold: Arc<AtomicU32>,
    limiter_release_mode: LfoRateMode,
    limiter_release_ms: Arc<AtomicU32>,
    limiter_release_sync_rate: Arc<AtomicU32>,
    limiter: Limiter,
    master_peak_meter: Arc<AtomicU32>,
    synth_master_volume: Arc<AtomicU32>,
    synth_master_peak_meter: Arc<AtomicU32>,
    engine_volumes: [Arc<AtomicU32>; 2],
    engine_peak_meters: [Arc<AtomicU32>; 2],
    bpm_rounding: bool,
    // Buffers for block-based processing
    engine_0_buffer: Vec<f32>,
    engine_1_buffer: Vec<f32>,
}

impl AudioEngine {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        command_consumer: HeapConsumer<AudioCommand>,
        input_consumer: HeapConsumer<f32>,
        sample_rate: f32,
        selected_midi_channel: Arc<AtomicU8>,
        playing_pads: Arc<AtomicU16>,
        track_mixer_state: Arc<RwLock<TrackMixerState>>,
        peak_meters: Arc<[AtomicU32; NUM_LOOPERS]>,
        cpu_load: Arc<AtomicU32>,
        input_peak_meter: Arc<AtomicU32>,
        audio_input_is_armed: Arc<AtomicBool>,
        audio_input_is_monitored: Arc<AtomicBool>,
        input_latency_compensation_ms: Arc<AtomicU32>,
        sampler_volume: Arc<AtomicU32>,
        sampler_peak_meter: Arc<AtomicU32>,
        master_volume: Arc<AtomicU32>,
        limiter_is_active: Arc<AtomicBool>,
        limiter_threshold: Arc<AtomicU32>,
        limiter_release_ms: Arc<AtomicU32>,
        limiter_release_sync_rate: Arc<AtomicU32>,
        gain_reduction_db: Arc<AtomicU32>,
        master_peak_meter: Arc<AtomicU32>,
        synth_master_volume: Arc<AtomicU32>,
        synth_master_peak_meter: Arc<AtomicU32>,
        engine_params: [EngineWithVolumeAndPeak; 2],
        bpm_rounding: bool,
        transport_is_playing: Arc<AtomicBool>,
    ) -> (Self, Vec<SharedLooperState>) {
        let looper_states: Vec<SharedLooperState> =
            (0..NUM_LOOPERS).map(|_| SharedLooperState::new()).collect();
        let loopers: Vec<Looper> = looper_states
            .iter()
            .map(|s| Looper::new(s.clone()))
            .collect();

        // Separate the Arcs from the parameters before moving engine_params
        let engine_volumes = [
            engine_params[0].0.clone(),
            engine_params[1].0.clone(),
        ];
        let engine_peak_meters = [
            engine_params[0].1.clone(),
            engine_params[1].1.clone(),
        ];

        let synth = Synth::new(sample_rate, engine_params);

        let engine = Self {
            command_consumer,
            input_consumer,
            loopers,
            synth,
            sampler_pads: vec![SamplerPad::default(); 16],
            synth_is_active: Arc::new(AtomicBool::new(false)),
            audio_input_is_armed,
            audio_input_is_monitored,
            sampler_is_active: Arc::new(AtomicBool::new(false)),
            selected_midi_channel,
            transport_playhead: Arc::new(AtomicUsize::new(0)),
            transport_len_samples: Arc::new(AtomicUsize::new(0)),
            transport_is_playing,
            transport_state: TransportState::Playing,
            sample_rate,
            playing_pads,
            track_mixer_state,
            peak_meters,
            cpu_load,
            input_peak_meter,
            input_latency_compensation_ms,
            sampler_volume,
            sampler_peak_meter,
            master_volume,
            limiter_is_active,
            limiter_threshold,
            limiter_release_mode: LfoRateMode::Hz,
            limiter_release_ms,
            limiter_release_sync_rate,
            limiter: Limiter::new(sample_rate, gain_reduction_db),
            master_peak_meter,
            synth_master_volume,
            synth_master_peak_meter,
            engine_volumes,
            engine_peak_meters,
            bpm_rounding,
            // Initialize with a default size; will be resized in process_buffer
            engine_0_buffer: vec![0.0; 512],
            engine_1_buffer: vec![0.0; 512],
        };

        (engine, looper_states)
    }

    pub fn handle_commands(&mut self) {
        while let Some(command) = self.command_consumer.pop() {
            match command {
                AudioCommand::PlayTransport => {
                    self.transport_state = TransportState::Playing;
                    self.transport_is_playing.store(true, Ordering::Relaxed);
                }
                AudioCommand::PauseTransport => {
                    self.transport_state = TransportState::Paused;
                    self.transport_is_playing.store(false, Ordering::Relaxed);
                }
                AudioCommand::ClearAllAndPlay => {
                    self.transport_state = TransportState::Playing;
                    self.transport_is_playing.store(true, Ordering::Relaxed);
                    self.transport_playhead.store(0, Ordering::Relaxed);
                    self.transport_len_samples.store(0, Ordering::Relaxed);
                    for i in 0..NUM_LOOPERS {
                        self.clear_looper(i);
                    }
                    if let Ok(mut mixer_state) = self.track_mixer_state.write() {
                        *mixer_state = TrackMixerState::default();
                    }
                }
                AudioCommand::ToggleLooper(id) => {
                    let looper = &mut self.loopers[id];
                    let current_state = looper.shared_state.get();

                    if current_state == LooperState::Playing {
                        looper.shared_state.set(LooperState::Overdubbing);
                    } else if current_state == LooperState::Overdubbing {
                        looper.shared_state.set(LooperState::Playing);
                    } else {
                        looper.pending_command = true;
                        if current_state == LooperState::Empty {
                            looper.shared_state.set(LooperState::Armed);
                        }
                    }
                }
                AudioCommand::ArmLooper(id) => self.arm_looper(id),
                AudioCommand::ClearLooper(id) => self.clear_looper(id),
                AudioCommand::SetMasterVolume(vol) => self
                    .master_volume
                    .store((vol * 1_000_000.0) as u32, Ordering::Relaxed),
                AudioCommand::SetLimiterThreshold(thresh) => self
                    .limiter_threshold
                    .store((thresh * 1_000_000.0) as u32, Ordering::Relaxed),
                AudioCommand::ToggleLimiter => {
                    let is_active = self.limiter_is_active.load(Ordering::Relaxed);
                    self.limiter_is_active.store(!is_active, Ordering::Relaxed);
                }
                AudioCommand::SetLimiterReleaseMode(mode) => self.limiter_release_mode = mode,
                AudioCommand::SetLimiterReleaseMs(ms) => self
                    .limiter_release_ms
                    .store((ms * 1000.0) as u32, Ordering::Relaxed),
                AudioCommand::SetLimiterReleaseSync(rate) => self
                    .limiter_release_sync_rate
                    .store((rate * 1_000_000.0) as u32, Ordering::Relaxed),
                AudioCommand::SetAmpAdsr(idx, settings) => {
                    if let Some(engine) = self.synth.engines.get_mut(idx) {
                        engine.set_amp_adsr(settings);
                    }
                }
                AudioCommand::SetFilterAdsr(idx, settings) => {
                    if let Some(engine) = self.synth.engines.get_mut(idx) {
                        engine.set_filter_adsr(settings);
                    }
                }
                AudioCommand::SetSynthMode(idx, poly) => {
                    if let Some(engine) = self.synth.engines.get_mut(idx) {
                        engine.set_polyphonic(poly);
                    }
                }
                AudioCommand::ResetWavetables(idx) => {
                    if let Some(engine) = self.synth.engines.get_mut(idx) {
                        engine.reset_to_defaults();
                    }
                }
                AudioCommand::SetWavetable {
                    engine_index,
                    slot_index,
                    audio_data,
                    name,
                } => {
                    if let Some(engine) = self.synth.engines.get_mut(engine_index) {
                        engine.set_wavetable(slot_index, audio_data, name);
                    }
                }
                AudioCommand::LoadSampleForSamplerSlot {
                    engine_index,
                    slot_index,
                    audio_data,
                } => {
                    if let Some(SynthEngine::Sampler(s)) =
                        self.synth.engines.get_mut(engine_index)
                    {
                        s.load_sample_for_slot(slot_index, audio_data);
                    }
                }
                AudioCommand::SetSamplerSettings {
                    engine_index,
                    root_notes,
                    global_fine_tune_cents,
                    fade_out,
                } => {
                    if let Some(SynthEngine::Sampler(s)) =
                        self.synth.engines.get_mut(engine_index)
                    {
                        s.set_sampler_settings(root_notes, global_fine_tune_cents, fade_out);
                    }
                }
                AudioCommand::ChangeEngineType {
                    engine_index,
                    volume,
                    peak_meter,
                    params,
                } => {
                    let new_engine = Synth::create_engine(self.sample_rate, params);
                    self.synth.engines[engine_index] = new_engine;
                    self.engine_volumes[engine_index] = volume;
                    self.engine_peak_meters[engine_index] = peak_meter;
                }
                AudioCommand::MidiMessage(msg) => {
                    let channel = msg.status & 0x0F;
                    let selected_channel = self.selected_midi_channel.load(Ordering::Relaxed);

                    if channel == selected_channel {
                        let note = msg.data1;
                        let velocity = msg.data2;
                        if msg.status & 0xF0 == 0x90 && velocity > 0 {
                            let mut note_consumed = false;
                            if self.sampler_is_active.load(Ordering::Relaxed) {
                                if (48..=63).contains(&note) {
                                    let pad_index = (note - 48) as usize;
                                    if let Some(pad) = self.sampler_pads.get_mut(pad_index) {
                                        if !pad.audio.is_empty() {
                                            pad.volume = velocity as f32 / 127.0;
                                            pad.is_playing = true;
                                            pad.playhead = 0;
                                            note_consumed = true;
                                        }
                                    }
                                }
                            }
                            if !note_consumed && self.synth_is_active.load(Ordering::Relaxed) {
                                self.synth.note_on(note, velocity);
                            }
                        } else {
                            // Note-off events are sent to both instruments to prevent stuck notes
                            // if the user switches active instruments while holding a key.
                            self.synth.note_off(note);
                        }
                    }
                }
                AudioCommand::ActivateSynth => self.synth_is_active.store(true, Ordering::Relaxed),
                AudioCommand::DeactivateSynth => {
                    self.synth_is_active.store(false, Ordering::Relaxed)
                }
                AudioCommand::ToggleAudioInputArm => {
                    let is_armed = self.audio_input_is_armed.load(Ordering::Relaxed);
                    self.audio_input_is_armed
                        .store(!is_armed, Ordering::Relaxed);
                }
                AudioCommand::ToggleAudioInputMonitoring => {
                    let is_monitored = self.audio_input_is_monitored.load(Ordering::Relaxed);
                    self.audio_input_is_monitored
                        .store(!is_monitored, Ordering::Relaxed);
                }
                AudioCommand::ActivateSampler => {
                    self.sampler_is_active.store(true, Ordering::Relaxed)
                }
                AudioCommand::DeactivateSampler => {
                    self.sampler_is_active.store(false, Ordering::Relaxed)
                }
                AudioCommand::LoadSamplerSample {
                    pad_index,
                    audio_data,
                } => {
                    if let Some(pad) = self.sampler_pads.get_mut(pad_index) {
                        pad.audio = audio_data;
                    }
                }
                AudioCommand::ClearSample { pad_index } => {
                    if let Some(pad) = self.sampler_pads.get_mut(pad_index) {
                        pad.audio = Arc::new(vec![]);
                        pad.is_playing = false;
                    }
                }
            }
        }
    }

    fn update_waveform_summary(&mut self, looper_id: usize) {
        let looper = &self.loopers[looper_id];
        let audio = &looper.audio;
        let mut summary = Vec::with_capacity(WAVEFORM_DOWNSAMPLE_SIZE);

        if audio.is_empty() {
            if let Ok(mut w) = looper.shared_state.get_waveform_summary().write() {
                w.clear();
            }
            return;
        }

        let chunk_size = (audio.len() as f32 / WAVEFORM_DOWNSAMPLE_SIZE as f32).max(1.0) as usize;

        for chunk in audio.chunks(chunk_size) {
            let peak = chunk.iter().fold(0.0f32, |max, &v| max.max(v.abs()));
            summary.push(peak);
        }

        if let Ok(mut w) = looper.shared_state.get_waveform_summary().write() {
            *w = summary;
        }
    }

    fn arm_looper(&mut self, id: usize) {
        for (i, looper) in self.loopers.iter_mut().enumerate() {
            if i != id && looper.shared_state.get() == LooperState::Armed {
                looper.shared_state.set(LooperState::Empty);
            }
        }
        self.loopers[id].shared_state.set(LooperState::Armed);
    }

    fn clear_looper(&mut self, id: usize) {
        let looper = &mut self.loopers[id];
        looper.audio.clear();
        looper.playhead = 0;
        looper.pending_command = false;
        looper.stop_is_queued = false;
        looper.cycles_recorded = 0;
        looper.shared_state.set(LooperState::Empty);
        looper.shared_state.set_length_in_cycles(0);
        looper.shared_state.set_playhead(0);

        self.update_waveform_summary(id);

        if self.loopers.iter().all(|l| l.audio.is_empty()) {
            self.transport_len_samples.store(0, Ordering::Relaxed);
            self.transport_playhead.store(0, Ordering::Relaxed);
        }
    }

    pub fn process_buffer(&mut self, mic_buffer: &[f32]) -> Vec<f32> {
        let start_time = Instant::now();
        let num_samples = mic_buffer.len();
        let mut output_buffer = vec![0.0; num_samples];
        let mut transport_len = self.transport_len_samples.load(Ordering::Relaxed);
        let mut transport_playhead = self.transport_playhead.load(Ordering::Relaxed);
        let mut playing_mask = 0u16;

        // --- Resize engine buffers if needed ---
        if self.engine_0_buffer.len() != num_samples {
            self.engine_0_buffer.resize(num_samples, 0.0);
            self.engine_1_buffer.resize(num_samples, 0.0);
        }

        // --- Block-based Synth Processing (once per callback) ---
        if self.synth_is_active.load(Ordering::Relaxed) {
            self.synth.process(
                &mut self.engine_0_buffer,
                &mut self.engine_1_buffer,
                transport_len,
            );
        } else {
            // Ensure buffers are silent if synth is inactive
            self.engine_0_buffer.fill(0.0);
            self.engine_1_buffer.fill(0.0);
        }

        // --- Initialize Peak Metering for the Block ---
        let mut engine_peak_buffers = [0.0f32; 2];
        let mut synth_master_peak_buffer = 0.0f32;
        let mut sampler_peak_buffer = 0.0f32;
        let mut master_peak_buffer = 0.0f32;

        let release_coeffs = match self.limiter_release_mode {
            LfoRateMode::Hz => {
                let release_ms =
                    self.limiter_release_ms.load(Ordering::Relaxed) as f32 / 1000.0;
                (-(1.0 / (release_ms * 0.001 * self.sample_rate))).exp()
            }
            LfoRateMode::Sync => {
                if transport_len > 0 {
                    let sync_rate = self.limiter_release_sync_rate.load(Ordering::Relaxed) as f32
                        / 1_000_000.0;
                    let release_samples = (transport_len as f32) / sync_rate;
                    (-(1.0 / release_samples)).exp()
                } else {
                    let release_ms = 80.0;
                    (-(1.0 / (release_ms * 0.001 * self.sample_rate))).exp()
                }
            }
        };

        let input_peak = mic_buffer
            .iter()
            .fold(0.0f32, |max, &val| max.max(val.abs()));
        self.input_peak_meter
            .store((input_peak * u32::MAX as f32) as u32, Ordering::Relaxed);

        let mixer_state = self.track_mixer_state.read().unwrap().clone();
        let is_any_soloed = mixer_state.tracks.iter().any(|t| t.is_soloed);
        let mut buffer_peaks = [0.0f32; NUM_LOOPERS];

        let synth_master_vol_f32 =
            self.synth_master_volume.load(Ordering::Relaxed) as f32 / 1_000_000.0;
        let sampler_vol_f32 = self.sampler_volume.load(Ordering::Relaxed) as f32 / 1_000_000.0;

        // --- Main Mixing Loop (per-sample) ---
        for i in 0..num_samples {
            let just_wrapped = transport_len > 0 && transport_playhead == 0;

            if just_wrapped {
                // This logic handles looper state changes at the start of a new cycle
                // It remains unchanged.
                let mut loopers_to_update_waveform = Vec::new();

                for (id, looper) in self.loopers.iter_mut().enumerate() {
                    let was_overdubbing = looper.shared_state.get() == LooperState::Overdubbing;

                    if looper.pending_command {
                        let current_state = looper.shared_state.get();
                        match current_state {
                            LooperState::Recording => looper.stop_is_queued = true,
                            LooperState::Empty | LooperState::Armed => {
                                looper.audio.clear();
                                looper.playhead = 0;
                                looper.cycles_recorded = 0;
                                looper.stop_is_queued = false;
                                looper.shared_state.set(LooperState::Recording);
                                looper.shared_state.set_length_in_cycles(0);
                                looper.shared_state.set_playhead(0);
                            }
                            LooperState::Playing => {
                                looper.shared_state.set(LooperState::Overdubbing)
                            }
                            LooperState::Overdubbing => {
                                looper.shared_state.set(LooperState::Playing)
                            }
                            LooperState::Stopped => {
                                looper.playhead = 0;
                                looper.shared_state.set_playhead(0);
                                looper.shared_state.set(LooperState::Playing);
                            }
                        }
                        looper.pending_command = false;
                    }

                    if was_overdubbing {
                        loopers_to_update_waveform.push(id);
                    }
                }

                let mut loopers_to_clear = Vec::new();
                for (id, looper) in self.loopers.iter_mut().enumerate() {
                    if looper.stop_is_queued {
                        let final_len = transport_len * (looper.cycles_recorded as usize);
                        if final_len > 0 {
                            looper.audio.resize(final_len, 0.0);
                            looper.shared_state.set(LooperState::Playing);
                            looper.playhead = 0;
                            looper
                                .shared_state
                                .set_length_in_cycles(looper.cycles_recorded);
                            looper.shared_state.set_playhead(0);
                            loopers_to_update_waveform.push(id);
                        } else {
                            loopers_to_clear.push(id);
                        }
                        looper.stop_is_queued = false;
                    }
                }

                for id in loopers_to_update_waveform {
                    self.update_waveform_summary(id);
                }
                for id in loopers_to_clear {
                    self.clear_looper(id);
                }
                for looper in self.loopers.iter_mut() {
                    if looper.shared_state.get() == LooperState::Recording {
                        looper.cycles_recorded += 1;
                    }
                }
            }

            if transport_len == 0 {
                // This logic for the first loop detection remains unchanged.
                let mut looper_id_to_process = None;
                for id in 0..self.loopers.len() {
                    if self.loopers[id].pending_command
                        && self.loopers[id].shared_state.get() == LooperState::Recording
                    {
                        looper_id_to_process = Some(id);
                        break;
                    }
                }
                if let Some(id) = looper_id_to_process {
                    let looper = &mut self.loopers[id];
                    let mut new_len = looper.audio.len();
                    if new_len > 0 {
                        if self.bpm_rounding {
                            // Assuming 4 beats per loop
                            let bpm =
                                (self.sample_rate * 60.0 * 4.0) / new_len as f32;
                            let rounded_bpm = bpm.round();
                            new_len = ((self.sample_rate * 60.0 * 4.0) / rounded_bpm) as usize;
                            looper.audio.resize(new_len, 0.0);
                            println!(
                                "Original BPM: {:.2}, Rounded BPM: {}, New Length: {}",
                                bpm, rounded_bpm, new_len
                            );
                        }

                        self.transport_len_samples.store(new_len, Ordering::Relaxed);
                        transport_len = new_len;
                        looper.shared_state.set(LooperState::Playing);
                        looper.playhead = 0;
                        looper.cycles_recorded = 1;
                        looper.shared_state.set_length_in_cycles(1);
                        looper.shared_state.set_playhead(0);
                        looper.pending_command = false;
                        self.update_waveform_summary(id);
                    }
                }
            }

            let sampler_is_active = self.sampler_is_active.load(Ordering::Relaxed);
            let audio_input_is_armed = self.audio_input_is_armed.load(Ordering::Relaxed);
            let audio_input_is_monitored = self.audio_input_is_monitored.load(Ordering::Relaxed);

            // --- Sampler Processing ---
            let mut raw_sampler_output = 0.0;
            if sampler_is_active {
                for (pad_idx, pad) in self.sampler_pads.iter_mut().enumerate() {
                    if pad.is_playing {
                        if let Some(sample) = pad.audio.get(pad.playhead) {
                            raw_sampler_output += *sample * pad.volume;
                            pad.playhead += 1;
                            playing_mask |= 1 << pad_idx;
                        } else {
                            pad.is_playing = false;
                        }
                    }
                }
            }
            sampler_peak_buffer = sampler_peak_buffer.max(raw_sampler_output.abs());
            let final_sampler_output = raw_sampler_output * sampler_vol_f32;

            // --- Synth Mixing (from pre-filled buffers) ---
            let vol0 = self.engine_volumes[0].load(Ordering::Relaxed) as f32 / 1_000_000.0;
            let vol1 = self.engine_volumes[1].load(Ordering::Relaxed) as f32 / 1_000_000.0;
            let final_engine_outputs = [self.engine_0_buffer[i] * vol0, self.engine_1_buffer[i] * vol1];
            engine_peak_buffers[0] = engine_peak_buffers[0].max(final_engine_outputs[0].abs());
            engine_peak_buffers[1] = engine_peak_buffers[1].max(final_engine_outputs[1].abs());
            let summed_engine_output = final_engine_outputs[0] + final_engine_outputs[1];
            synth_master_peak_buffer = synth_master_peak_buffer.max(summed_engine_output.abs());
            let final_synth_output = summed_engine_output * synth_master_vol_f32;

            let mic_input = mic_buffer[i];

            // --- Record Input Signal ---
            let record_input = {
                let mut total = 0.0;
                if audio_input_is_armed {
                    total += mic_input;
                }
                // The live synth output is part of the record signal
                total += final_synth_output;

                if sampler_is_active {
                    total += final_sampler_output;
                }
                total
            };

            // --- Looper Processing ---
            let mut looper_output = 0.0;
            for (id, looper) in self.loopers.iter_mut().enumerate() {
                let state = looper.shared_state.get();
                match state {
                    LooperState::Armed => {
                        if transport_len == 0 && record_input.abs() > LOOPER_ARM_THRESHOLD {
                            looper.shared_state.set(LooperState::Recording);
                            looper.cycles_recorded = 1;
                        }
                    }
                    LooperState::Recording => {
                        looper.audio.push(record_input);
                        looper.samples_since_waveform_update += 1;
                    }
                    LooperState::Playing | LooperState::Overdubbing => {
                        if !looper.audio.is_empty() {
                            let sample_to_play = looper.audio[looper.playhead];
                            buffer_peaks[id] = buffer_peaks[id].max(sample_to_play.abs());
                            let track_state = &mixer_state.tracks[id];

                            let is_audible = if is_any_soloed {
                                track_state.is_soloed
                            } else {
                                !track_state.is_muted
                            };

                            if is_audible {
                                looper_output += sample_to_play * track_state.volume;
                            }
                            if state == LooperState::Overdubbing {
                                looper.audio[looper.playhead] =
                                    (sample_to_play + record_input).clamp(-1.0, 1.0);
                                looper.samples_since_waveform_update += 1;
                            }
                            if self.transport_state == TransportState::Playing {
                                looper.playhead = (looper.playhead + 1) % looper.audio.len();
                                looper.shared_state.set_playhead(looper.playhead);
                            }
                        }
                    }
                    _ => {}
                }
            }

            let live_sampler_output = if sampler_is_active {
                final_sampler_output
            } else {
                0.0
            };
            let monitored_input = if audio_input_is_monitored {
                mic_input
            } else {
                0.0
            };

            // --- Final Mixdown and Limiter ---
            let pre_master_mix =
                looper_output + final_synth_output + live_sampler_output + monitored_input;

            master_peak_buffer = master_peak_buffer.max(pre_master_mix.abs());

            let master_vol = self.master_volume.load(Ordering::Relaxed) as f32 / 1_000_000.0;
            let final_mix = pre_master_mix * master_vol;

            if self.limiter_is_active.load(Ordering::Relaxed) {
                let threshold =
                    self.limiter_threshold.load(Ordering::Relaxed) as f32 / 1_000_000.0;
                output_buffer[i] = self.limiter.process(final_mix, threshold, release_coeffs);
            } else {
                self.limiter.gain_reduction_db.store(0, Ordering::Relaxed);
                output_buffer[i] = final_mix.clamp(-1.0, 1.0);
            }

            if transport_len > 0 && self.transport_state == TransportState::Playing {
                transport_playhead = (transport_playhead + 1) % transport_len;
            }
        }

        // --- Post-Loop Updates ---
        // --- Realtime waveform updates ---
        for id in 0..self.loopers.len() {
            if self.loopers[id].samples_since_waveform_update >= 256 {
                self.update_waveform_summary(id);
                self.loopers[id].samples_since_waveform_update = 0;
            }
        }

        for i in 0..2 {
            self.engine_peak_meters[i].store(
                (engine_peak_buffers[i].clamp(0.0, 1.0) * u32::MAX as f32) as u32,
                Ordering::Relaxed,
            );
        }

        self.transport_playhead
            .store(transport_playhead, Ordering::Relaxed);
        self.playing_pads.store(playing_mask, Ordering::Relaxed);
        self.synth_master_peak_meter.store(
            (synth_master_peak_buffer * u32::MAX as f32) as u32,
            Ordering::Relaxed,
        );
        self.sampler_peak_meter.store(
            (sampler_peak_buffer * u32::MAX as f32) as u32,
            Ordering::Relaxed,
        );
        self.master_peak_meter.store(
            (master_peak_buffer * u32::MAX as f32) as u32,
            Ordering::Relaxed,
        );

        for i in 0..NUM_LOOPERS {
            self.peak_meters[i].store(
                (buffer_peaks[i].clamp(0.0, 1.0) * u32::MAX as f32) as u32,
                Ordering::Relaxed,
            );
        }

        let elapsed = start_time.elapsed();
        if num_samples > 0 {
            let buffer_duration_seconds = num_samples as f32 / self.sample_rate;
            let load_ratio = elapsed.as_secs_f32() / buffer_duration_seconds;
            self.cpu_load
                .store((load_ratio * 1000.0) as u32, Ordering::Relaxed);
        }

        output_buffer
    }
}