// src/audio_engine.rs
use crate::looper::{LooperState, SharedLooperState, WAVEFORM_DOWNSAMPLE_SIZE, NUM_LOOPERS};
use crate::mixer::{MixerState, MixerTrackState};
use crate::sampler_engine::{self, NUM_SAMPLE_SLOTS};
use crate::synth::{
    AdsrSettings, Engine, EngineParamsUnion, EngineWithVolumeAndPeak, LfoRateMode, SamplerParams,
    Synth, SynthEngine, WavetableParams,
};
use crate::wavetable_engine;
use anyhow::Result;
use hound;
use ringbuf::HeapConsumer;
use rodio::source::Source;
use rodio::Decoder;
use rubato::{
    Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU32, AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};
use std::thread;
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

#[derive(Debug)]
pub enum AudioCommand {
    LooperPress(usize),
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
    },
    SetTransportLen(usize),
    SetMixerState(MixerState),
    SetMixerTrackVolume {
        track_index: usize,
        volume: f32,
    },

    // New Commands for MIDI Mapping
    ToggleSynth,
    SetSynthMasterVolume(f32),
    ToggleSampler,
    SetSamplerMasterVolume(f32),
    ToggleTransport,
    ToggleMuteAll,
    ToggleRecord,
    ToggleMixerMute(usize),
    ToggleMixerSolo(usize),
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
    pub track_mixer_state: Arc<RwLock<MixerState>>,
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
    output_recording_buffer: Option<Vec<f32>>,
    pub midi_cc_values: Arc<[[AtomicU32; 128]; 16]>,
    // Signal for UI thread to handle recording logic
    pub should_toggle_record: Arc<AtomicBool>,
    // Buffers for block-based processing
    engine_0_buffer: Vec<f32>,
    engine_1_buffer: Vec<f32>,
}

fn write_wav_file(path: &Path, audio_buffer: &[f32], sample_rate: f32) -> Result<()> {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: sample_rate as u32,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec)?;
    for &sample in audio_buffer {
        let amplitude = i16::MAX as f32;
        let sample_i16 = (sample * amplitude) as i16;
        writer.write_sample(sample_i16)?; // Left channel
        writer.write_sample(sample_i16)?; // Right channel
    }
    writer.finalize()?;
    Ok(())
}

fn trim_silence(audio_buffer: Vec<f32>) -> Vec<f32> {
    const SILENCE_THRESHOLD: f32 = 0.005; // RMS threshold
    const BLOCK_SIZE: usize = 512;       // Analyze in chunks of 512 samples
    const REQUIRED_BLOCKS: usize = 3;    // Need 3 consecutive blocks of sound to confirm start

    let num_blocks = audio_buffer.len() / BLOCK_SIZE;
    let mut consecutive_sound_blocks = 0;
    let mut start_block = None;

    // Find the starting position
    for i in 0..num_blocks {
        let block = &audio_buffer[i * BLOCK_SIZE..(i + 1) * BLOCK_SIZE];
        let sum_sq: f32 = block.iter().map(|&s| s * s).sum();
        let rms = (sum_sq / BLOCK_SIZE as f32).sqrt();

        if rms > SILENCE_THRESHOLD {
            consecutive_sound_blocks += 1;
            if consecutive_sound_blocks >= REQUIRED_BLOCKS {
                start_block = Some(i.saturating_sub(REQUIRED_BLOCKS - 1));
                break;
            }
        } else {
            consecutive_sound_blocks = 0;
        }
    }

    let start_pos = match start_block {
        Some(block_idx) => block_idx * BLOCK_SIZE,
        None => return Vec::new(), // All silent
    };

    // Find the ending position (search backwards)
    consecutive_sound_blocks = 0;
    let mut end_block = None;
    for i in (0..num_blocks).rev() {
        let block = &audio_buffer[i * BLOCK_SIZE..(i + 1) * BLOCK_SIZE];
        let sum_sq: f32 = block.iter().map(|&s| s * s).sum();
        let rms = (sum_sq / BLOCK_SIZE as f32).sqrt();

        if rms > SILENCE_THRESHOLD {
            consecutive_sound_blocks += 1;
            if consecutive_sound_blocks >= REQUIRED_BLOCKS {
                end_block = Some(i);
                break;
            }
        } else {
            consecutive_sound_blocks = 0;
        }
    }

    let end_pos = match end_block {
        Some(block_idx) => (block_idx + 1) * BLOCK_SIZE,
        None => audio_buffer.len(), // Should be unreachable if start was found
    };

    if start_pos >= end_pos {
        return Vec::new();
    }

    audio_buffer[start_pos..end_pos].to_vec()
}

impl AudioEngine {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        command_consumer: HeapConsumer<AudioCommand>,
        input_consumer: HeapConsumer<f32>,
        sample_rate: f32,
        selected_midi_channel: Arc<AtomicU8>,
        playing_pads: Arc<AtomicU16>,
        track_mixer_state: Arc<RwLock<MixerState>>,
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
        should_toggle_record: Arc<AtomicBool>,
        midi_cc_values: Arc<[[AtomicU32; 128]; 16]>,
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
            output_recording_buffer: None,
            midi_cc_values,
            should_toggle_record,
            // Initialize with a default size; will be resized in process_buffer
            engine_0_buffer: vec![0.0; 512],
            engine_1_buffer: vec![0.0; 512],
        };

        (engine, looper_states)
    }

    pub fn handle_commands(&mut self) {
        while let Some(command) = self.command_consumer.pop() {
            match command {
                AudioCommand::ToggleMixerMute(track_index) => {
                    if let Ok(mut mixer_state) = self.track_mixer_state.write() {
                        if let Some(track) = mixer_state.tracks.get_mut(track_index) {
                            track.is_muted = !track.is_muted;
                        }
                    }
                }
                AudioCommand::ToggleMixerSolo(track_index) => {
                    if let Ok(mut mixer_state) = self.track_mixer_state.write() {
                        if let Some(track) = mixer_state.tracks.get_mut(track_index) {
                            track.is_soloed = !track.is_soloed;
                        }
                    }
                }
                AudioCommand::ToggleSynth => {
                    let is_active = self.synth_is_active.load(Ordering::Relaxed);
                    self.synth_is_active.store(!is_active, Ordering::Relaxed);
                    if !is_active { // if we just activated it
                        self.sampler_is_active.store(false, Ordering::Relaxed);
                    }
                }
                AudioCommand::ToggleSampler => {
                    let is_active = self.sampler_is_active.load(Ordering::Relaxed);
                    self.sampler_is_active.store(!is_active, Ordering::Relaxed);
                    if !is_active { // if we just activated it
                        self.synth_is_active.store(false, Ordering::Relaxed);
                    }
                }
                AudioCommand::SetSynthMasterVolume(vol) => {
                    self.synth_master_volume.store((vol * 1_000_000.0) as u32, Ordering::Relaxed);
                }
                AudioCommand::SetSamplerMasterVolume(vol) => {
                    self.sampler_volume.store((vol * 1_000_000.0) as u32, Ordering::Relaxed);
                }
                AudioCommand::ToggleTransport => {
                    if self.transport_is_playing.load(Ordering::Relaxed) {
                        // This is the logic from the StopTransport command
                        self.transport_state = TransportState::Paused;
                        self.transport_is_playing.store(false, Ordering::Relaxed);
                        self.transport_playhead.store(0, Ordering::Relaxed);
                        for looper in self.loopers.iter_mut() {
                            looper.playhead = 0;
                            looper.shared_state.set_playhead(0);
                        }
                    } else {
                        // This is the logic from the PlayTransport command
                        self.transport_state = TransportState::Playing;
                        self.transport_is_playing.store(true, Ordering::Relaxed);
                    }
                }
                AudioCommand::ToggleMuteAll => {
                    if let Ok(mut mixer_state) = self.track_mixer_state.write() {
                        let should_mute_all = mixer_state.tracks.iter().any(|track| !track.is_muted);
                        for track in mixer_state.tracks.iter_mut() {
                            track.is_muted = should_mute_all;
                        }
                    }
                }
                AudioCommand::ToggleRecord => {
                    self.should_toggle_record.store(true, Ordering::Relaxed);
                }
                AudioCommand::StartOutputRecording => {
                    self.output_recording_buffer = Some(Vec::new());
                }
                AudioCommand::StopOutputRecording { output_path } => {
                    if let Some(buffer) = self.output_recording_buffer.take() {
                        let sample_rate = self.sample_rate;
                        thread::spawn(move || {
                            let trimmed_buffer = trim_silence(buffer);
                            if trimmed_buffer.is_empty() {
                                println!("Recording is empty after trimming silence. Not saved.");
                                return;
                            }

                            if let Err(e) = write_wav_file(&output_path, &trimmed_buffer, sample_rate) {
                                eprintln!("Failed to save recording: {}", e);
                            } else {
                                println!("Recording saved to {}", output_path.display());
                            }
                        });
                    }
                }
                AudioCommand::SaveSessionAudio { session_path } => {
                    for (i, looper) in self.loopers.iter().enumerate() {
                        if !looper.audio.is_empty() {
                            let audio_data = looper.audio.clone();
                            let path = session_path.join(format!("loop_{}.wav", i));
                            let sample_rate = self.sample_rate;
                            thread::spawn(move || {
                                // For session saving, we'll save as mono to preserve original data
                                let spec = hound::WavSpec {
                                    channels: 1,
                                    sample_rate: sample_rate as u32,
                                    bits_per_sample: 16,
                                    sample_format: hound::SampleFormat::Int,
                                };
                                if let Ok(mut writer) = hound::WavWriter::create(&path, spec) {
                                    for &sample in &audio_data {
                                        let amplitude = i16::MAX as f32;
                                        writer.write_sample((sample * amplitude) as i16).ok();
                                    }
                                    writer.finalize().ok();
                                } else {
                                    eprintln!("Failed to create session wav file at {}", path.display());
                                }
                            });
                        }
                    }
                }
                AudioCommand::LoadLoopAudio { looper_index, path, original_sample_rate } => {
                    let target_sr = self.sample_rate;
                    match Self::load_and_resample_wav_for_session(&path, original_sample_rate as f32, target_sr) {
                        Ok(audio_data) => {
                            if let Some(looper) = self.loopers.get_mut(looper_index) {
                                looper.audio = audio_data;
                                looper.playhead = 0;
                                looper.shared_state.set(LooperState::Playing);
                                looper.shared_state.set_length_in_cycles(1);
                                self.update_waveform_summary(looper_index);
                            }
                        }
                        Err(e) => eprintln!("Failed to load session loop {}: {}", path.display(), e),
                    }
                }
                AudioCommand::SetTransportLen(len) => {
                    self.transport_len_samples.store(len, Ordering::Relaxed);
                }
                AudioCommand::SetMixerState(state) => {
                    if let Ok(mut mixer_state) = self.track_mixer_state.write() {
                        *mixer_state = state;
                    }
                }
                AudioCommand::SetMixerTrackVolume { track_index, volume } => {
                    if let Ok(mut mixer_state) = self.track_mixer_state.write() {
                        if let Some(track) = mixer_state.tracks.get_mut(track_index) {
                            track.volume = volume;
                        }
                    }
                }
                AudioCommand::PlayTransport => {
                    self.transport_state = TransportState::Playing;
                    self.transport_is_playing.store(true, Ordering::Relaxed);
                }
                AudioCommand::StopTransport => {
                    self.transport_state = TransportState::Paused;
                    self.transport_is_playing.store(false, Ordering::Relaxed);
                    self.transport_playhead.store(0, Ordering::Relaxed);
                    // --- ADDED THIS LOOP ---
                    for looper in self.loopers.iter_mut() {
                        looper.playhead = 0;
                        looper.shared_state.set_playhead(0);
                    }
                    // ---------------------
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
                        *mixer_state = MixerState::default();
                    }
                }
                AudioCommand::ClearAll => {
                    self.transport_state = TransportState::Paused;
                    self.transport_is_playing.store(false, Ordering::Relaxed);
                    self.transport_playhead.store(0, Ordering::Relaxed);
                    self.transport_len_samples.store(0, Ordering::Relaxed);
                    for i in 0..NUM_LOOPERS {
                        self.clear_looper(i);
                    }
                    if let Ok(mut mixer_state) = self.track_mixer_state.write() {
                        *mixer_state = MixerState::default();
                    }
                }
                AudioCommand::LooperPress(id) => {
                    let is_playing = self.transport_is_playing.load(Ordering::Relaxed);
                    let transport_has_started = self.transport_len_samples.load(Ordering::Relaxed) > 0;
                    let state = self.loopers[id].shared_state.get();

                    match state {
                        LooperState::Empty => {
                            if !transport_has_started {
                                self.arm_looper(id);
                            } else if is_playing {
                                self.handle_toggle_looper(id);
                            }
                        }
                        LooperState::Armed => {
                            self.clear_looper(id);
                        }
                        _ => { // Playing, Overdubbing, Stopped
                            if is_playing {
                                self.handle_toggle_looper(id);
                            }
                        }
                    }
                }
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

    fn handle_toggle_looper(&mut self, id: usize) {
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
                &self.midi_cc_values,
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
                        if self.transport_state == TransportState::Playing && transport_len == 0 && record_input.abs() > LOOPER_ARM_THRESHOLD {
                            looper.shared_state.set(LooperState::Recording);
                            looper.cycles_recorded = 1;
                        }
                    }
                    LooperState::Recording => {
                        if self.transport_state == TransportState::Playing {
                            looper.audio.push(record_input);
                            looper.samples_since_waveform_update += 1;
                        }
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

                            if self.transport_state == TransportState::Playing && is_audible {
                                looper_output += sample_to_play * track_state.volume;
                            }

                            if state == LooperState::Overdubbing && self.transport_state == TransportState::Playing {
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

        if let Some(rec_buffer) = &mut self.output_recording_buffer {
            rec_buffer.extend_from_slice(&output_buffer);
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


    fn load_and_resample_wav_for_session(path: &Path, source_sr: f32, target_sr: f32) -> Result<Vec<f32>> {
        let file = BufReader::new(File::open(path)?);
        // We use hound here because rodio's decoder might not correctly read the sample rate
        // from a file it just wrote. Hound is more reliable for this specific task.
        let reader = hound::WavReader::new(file)?;
        let spec = reader.spec();

        // Session loops are saved as mono
        if spec.channels != 1 {
            return Err(anyhow::anyhow!("Expected mono WAV file for session loop"));
        }

        let mono_samples: Vec<f32> = reader
            .into_samples::<i16>()
            .filter_map(Result::ok)
            .map(|s| s as f32 / i16::MAX as f32)
            .collect();

        if (source_sr - target_sr).abs() > 1e-3 {
            println!(
                "Resampling session loop from {} Hz to {} Hz",
                source_sr, target_sr
            );
            let params = SincInterpolationParameters {
                sinc_len: 256,
                f_cutoff: 0.95,
                interpolation: SincInterpolationType::Linear,
                oversampling_factor: 256,
                window: WindowFunction::BlackmanHarris2,
            };
            let mut resampler = SincFixedIn::<f32>::new(
                target_sr as f64 / source_sr as f64,
                2.0,
                params,
                mono_samples.len(),
                1,
            )?;
            let waves_in = vec![mono_samples];
            let waves_out = resampler.process(&waves_in, None)?;
            Ok(waves_out.into_iter().next().unwrap_or_default())
        } else {
            Ok(mono_samples)
        }
    }
}