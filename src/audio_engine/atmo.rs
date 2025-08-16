// FILE: src\audio_engine\atmo.rs
// ==============================

use crate::atmo::{AtmoLayerParams, AtmoScene, PlaybackMode};
use hound;
use ringbuf::{HeapConsumer, HeapRb};
use rubato::{
    Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

// NEW: Define a safe maximum buffer size to pre-allocate memory.
const MAX_BUFFER_SIZE: usize = 2048;

/// Manages reading a WAV file from disk in a separate thread and feeding it to the audio thread.
/// Now supports seeking and limiting the number of samples read.
struct StreamingWavReader {
    consumer: HeapConsumer<f32>,
    io_thread_handle: Option<JoinHandle<()>>,
    should_exit: Arc<AtomicBool>,
}

impl StreamingWavReader {
    fn new(
        path: PathBuf,
        target_sr: f32,
        start_sample: Option<u32>,
        samples_to_read: Option<u32>,
    ) -> (Self, Arc<AtomicBool>) {
        let buffer_size_ms = 500.0;
        let ringbuf_capacity = (target_sr * (buffer_size_ms / 1000.0)) as usize;
        let rb = HeapRb::new(ringbuf_capacity);
        let (mut producer, consumer) = rb.split();

        let should_exit = Arc::new(AtomicBool::new(false));
        let is_finished = Arc::new(AtomicBool::new(false));

        let should_exit_clone = should_exit.clone();
        let is_finished_clone = is_finished.clone();

        let thread_path = path.clone();
        let io_thread_handle = Some(thread::spawn(move || {
            const GAIN_BOOST: f32 = 4.0;

            let file = match File::open(&thread_path) {
                Ok(f) => f,
                Err(e) => {
                    eprintln!("[Atmo I/O Thread] Failed to open file '{}': {}", thread_path.display(), e);
                    is_finished_clone.store(true, Ordering::Relaxed);
                    return;
                }
            };

            let mut reader = match hound::WavReader::new(BufReader::new(file)) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("[Atmo I/O Thread] Failed to read wav header for '{}': {}", thread_path.display(), e);
                    is_finished_clone.store(true, Ordering::Relaxed);
                    return;
                }
            };

            // --- SEEK LOGIC ---
            if let Some(start_offset) = start_sample {
                if let Err(e) = reader.seek(start_offset) {
                    eprintln!("[Atmo I/O Thread] Failed to seek to {} in '{}': {}", start_offset, thread_path.display(), e);
                    // Continue from the start if seek fails
                }
            }

            let source_spec = reader.spec();
            let source_sr = source_spec.sample_rate as f32;
            let num_channels = source_spec.channels as usize;
            let bits_per_sample = source_spec.bits_per_sample;

            let mut resampler: Option<SincFixedIn<f32>> = None;
            let ratio = target_sr as f64 / source_sr as f64;
            if (ratio - 1.0).abs() > 1e-6 {
                let params = SincInterpolationParameters {
                    sinc_len: 256, f_cutoff: 0.95, interpolation: SincInterpolationType::Linear,
                    oversampling_factor: 256, window: WindowFunction::BlackmanHarris2,
                };
                match SincFixedIn::new(ratio, 2.0, params, 1024, 1) {
                    Ok(r) => resampler = Some(r),
                    Err(e) => {
                        eprintln!("[Atmo I/O Thread] Failed to create resampler for '{}': {}", thread_path.display(), e);
                        is_finished_clone.store(true, Ordering::Relaxed);
                        return;
                    }
                }
            }

            let mut total_samples_read = 0u32;
            let max_samples_to_read = samples_to_read.unwrap_or(u32::MAX);

            if bits_per_sample == 16 {
                let mut samples_iterator = reader.into_samples::<i16>();
                'main_loop: loop {
                    if should_exit_clone.load(Ordering::Relaxed) || total_samples_read >= max_samples_to_read { break; }
                    if producer.free_len() < 1024 { thread::sleep(std::time::Duration::from_millis(10)); continue; }

                    let remaining_samples = (max_samples_to_read - total_samples_read) as usize;
                    let chunk_size = 1024.min(remaining_samples);
                    let mut chunk_raw = Vec::with_capacity(chunk_size * num_channels);

                    for _ in 0..(chunk_size * num_channels) {
                        match samples_iterator.next() {
                            Some(Ok(sample)) => chunk_raw.push(sample),
                            Some(Err(e)) => { eprintln!("[Atmo I/O Thread] Error decoding sample in '{}': {}", thread_path.display(), e); break 'main_loop; }
                            None => break,
                        }
                    }
                    if chunk_raw.is_empty() { break 'main_loop; }
                    let mono_samples: Vec<f32> = chunk_raw.chunks_exact(num_channels).map(|c| (c.iter().map(|&s| s as f32).sum::<f32>() / num_channels as f32) / i16::MAX as f32 * GAIN_BOOST).collect();
                    total_samples_read += mono_samples.len() as u32;

                    if let Some(resampler) = &mut resampler {
                        if let Ok(resampled) = resampler.process(&[mono_samples], None) { if let Some(chunk) = resampled.get(0) { producer.push_slice(chunk); } }
                    } else { producer.push_slice(&mono_samples); }
                }
            } else if bits_per_sample == 24 {
                let mut samples_iterator = reader.into_samples::<i32>();
                'main_loop: loop {
                    if should_exit_clone.load(Ordering::Relaxed) || total_samples_read >= max_samples_to_read { break; }
                    if producer.free_len() < 1024 { thread::sleep(std::time::Duration::from_millis(10)); continue; }

                    let remaining_samples = (max_samples_to_read - total_samples_read) as usize;
                    let chunk_size = 1024.min(remaining_samples);
                    let mut chunk_raw = Vec::with_capacity(chunk_size * num_channels);

                    for _ in 0..(chunk_size * num_channels) {
                        match samples_iterator.next() {
                            Some(Ok(sample)) => chunk_raw.push(sample),
                            Some(Err(e)) => { eprintln!("[Atmo I/O Thread] Error decoding sample in '{}': {}", thread_path.display(), e); break 'main_loop; }
                            None => break,
                        }
                    }
                    if chunk_raw.is_empty() { break 'main_loop; }
                    let mono_samples: Vec<f32> = chunk_raw.chunks_exact(num_channels).map(|c| (c.iter().map(|&s| s as f32).sum::<f32>() / num_channels as f32) / 8388607.0 * GAIN_BOOST).collect();
                    total_samples_read += mono_samples.len() as u32;

                    if let Some(resampler) = &mut resampler {
                        if let Ok(resampled) = resampler.process(&[mono_samples], None) { if let Some(chunk) = resampled.get(0) { producer.push_slice(chunk); } }
                    } else { producer.push_slice(&mono_samples); }
                }
            } else {
                eprintln!("[Atmo I/O Thread] Unsupported bit depth {} for file '{}'", bits_per_sample, thread_path.display());
            }

            is_finished_clone.store(true, Ordering::Relaxed);
        }));

        (
            Self {
                consumer,
                io_thread_handle,
                should_exit,
            },
            is_finished,
        )
    }
}

impl Drop for StreamingWavReader {
    fn drop(&mut self) {
        self.should_exit.store(true, Ordering::Relaxed);
        if let Some(handle) = self.io_thread_handle.take() {
            let _ = handle.join();
        }
    }
}

/// A simple one-pole low-pass filter for the Atmo engine.
#[derive(Debug, Clone, Copy, Default)]
struct AtmoFilter {
    z1: f32,
}
impl AtmoFilter {
    #[inline(always)]
    fn process(&mut self, input: f32, coeff: f32) -> f32 {
        let output = input * (1.0 - coeff) + self.z1 * coeff;
        self.z1 = output;
        output
    }
}

/// The current playback state of an Atmo voice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AtmoVoiceState {
    Idle,
    FadingIn,
    Playing,
    FadingOut,
}

/// A single voice within an Atmo layer, responsible for playing one sample.
struct AtmoVoice {
    stream_reader: Option<StreamingWavReader>,
    is_finished: Arc<AtomicBool>,
    current_pan: [f32; 2],
    filter: AtmoFilter,

    // State management for playback and crossfading
    state: AtmoVoiceState,
    samples_to_play: u32,
    samples_played: u32,
    fade_gain: f32,
    fade_samples_total: u32,
    fade_samples_processed: u32,
}

impl AtmoVoice {
    fn new() -> Self {
        Self {
            stream_reader: None,
            is_finished: Arc::new(AtomicBool::new(false)),
            current_pan: [0.707, 0.707],
            filter: AtmoFilter::default(),
            state: AtmoVoiceState::Idle,
            samples_to_play: 0,
            samples_played: 0,
            fade_gain: 0.0,
            fade_samples_total: 0,
            fade_samples_processed: 0,
        }
    }

    /// Starts playing a sample from a given path and start offset.
    fn start(
        &mut self,
        path: PathBuf,
        rate: f32,
        pan: f32,
        target_sr: f32,
        start_sample: Option<u32>,
        samples_to_play: Option<u32>,
        fade_in_duration_samples: u32,
    ) {
        let samples_to_read = samples_to_play.map(|s| (s as f32 / rate).ceil() as u32);
        let (reader, is_finished) =
            StreamingWavReader::new(path, target_sr * rate, start_sample, samples_to_read);
        self.stream_reader = Some(reader);
        self.is_finished = is_finished;
        let pan_rad = pan * std::f32::consts::FRAC_PI_2;
        self.current_pan = [pan_rad.cos(), pan_rad.sin()];
        self.samples_to_play = samples_to_play.unwrap_or(u32::MAX);
        self.samples_played = 0;

        self.fade_samples_total = fade_in_duration_samples;
        self.fade_samples_processed = 0;
        self.fade_gain = 0.0;
        self.state = AtmoVoiceState::FadingIn;
    }

    fn is_active(&self) -> bool {
        self.state != AtmoVoiceState::Idle
    }

    /// Triggers the fade-out process for this voice.
    fn fade_out(&mut self, fade_out_duration_samples: u32) {
        if self.state == AtmoVoiceState::Playing || self.state == AtmoVoiceState::FadingIn {
            self.state = AtmoVoiceState::FadingOut;
            self.fade_samples_total = fade_out_duration_samples;
            self.fade_samples_processed = 0;
        }
    }

    /// Processes one sample of audio. Returns the audio frame.
    fn process(&mut self, filter_coeff: f32) -> [f32; 2] {
        if self.state == AtmoVoiceState::Idle {
            return [0.0, 0.0];
        }

        // Update fade gain based on state
        match self.state {
            AtmoVoiceState::FadingIn => {
                if self.fade_samples_processed < self.fade_samples_total {
                    self.fade_samples_processed += 1;
                    self.fade_gain =
                        self.fade_samples_processed as f32 / self.fade_samples_total as f32;
                } else {
                    self.fade_gain = 1.0;
                    self.state = AtmoVoiceState::Playing;
                }
            }
            AtmoVoiceState::FadingOut => {
                if self.fade_samples_processed < self.fade_samples_total {
                    self.fade_samples_processed += 1;
                    self.fade_gain =
                        1.0 - (self.fade_samples_processed as f32 / self.fade_samples_total as f32);
                } else {
                    self.fade_gain = 0.0;
                    self.state = AtmoVoiceState::Idle;
                    self.stream_reader = None; // Stop reading from disk
                    return [0.0, 0.0];
                }
            }
            _ => {} // Playing or Idle, gain is stable
        }

        if let Some(reader) = &mut self.stream_reader {
            if self.samples_played >= self.samples_to_play {
                self.state = AtmoVoiceState::Idle;
                self.stream_reader = None;
                return [0.0, 0.0];
            }

            if let Some(mut sample) = reader.consumer.pop() {
                self.samples_played += 1;
                sample = self.filter.process(sample, filter_coeff);
                let final_sample = sample * self.fade_gain;
                [
                    final_sample * self.current_pan[0],
                    final_sample * self.current_pan[1],
                ]
            } else {
                if self.is_finished.load(Ordering::Relaxed) {
                    self.state = AtmoVoiceState::Idle;
                    self.stream_reader = None;
                }
                [0.0, 0.0]
            }
        } else {
            self.state = AtmoVoiceState::Idle;
            [0.0, 0.0]
        }
    }
}

/// Manages a single layer of the atmosphere, including its voices and sample pool.
pub struct AtmoLayerProcessor {
    voices: Vec<AtmoVoice>,
    samples: Vec<(PathBuf, u32)>, // Now stores (path, length)
    next_trigger_countdown: i64,
    sample_rate: f32,
}

impl AtmoLayerProcessor {
    pub fn new(sample_rate: f32) -> Self {
        const NUM_VOICES_PER_LAYER: usize = 8;
        Self {
            voices: (0..NUM_VOICES_PER_LAYER)
                .map(|_| AtmoVoice::new())
                .collect(),
            samples: Vec::new(),
            next_trigger_countdown: 0,
            sample_rate,
        }
    }

    pub fn load_samples(&mut self, samples: Vec<(PathBuf, u32)>) {
        self.samples = samples;
        for voice in self.voices.iter_mut() {
            voice.state = AtmoVoiceState::Idle;
            voice.stream_reader = None;
        }
    }

    fn start_new_fragment_for_voice(&mut self, voice_index: usize, params: &AtmoLayerParams) {
        if self.samples.is_empty() {
            return;
        }
        let (path, total_length) =
            self.samples[(rand::random::<f32>() * self.samples.len() as f32) as usize].clone();

        let fragment_length = (total_length as f32 * params.fragment_length).ceil() as u32;
        if fragment_length == 0 {
            return;
        }

        let safe_start = (total_length as f32 * 0.01).ceil() as u32;
        let safe_end = (total_length as f32 * 0.99).floor() as u32;

        let random_start = if safe_end <= safe_start + fragment_length {
            0
        } else {
            let max_start_point = safe_end - fragment_length;
            safe_start + (rand::random::<f32>() * (max_start_point - safe_start) as f32) as u32
        };

        let pan = (rand::random::<f32>() * 2.0 - 1.0) * params.pan_randomness;
        let crossfade_samples = (1.0 * self.sample_rate) as u32;

        if let Some(voice) = self.voices.get_mut(voice_index) {
            voice.start(
                path,
                params.playback_rate,
                pan,
                self.sample_rate,
                Some(random_start),
                Some(fragment_length),
                crossfade_samples,
            );
        }
    }

    fn start_new_one_shot_for_voice(&mut self, voice_index: usize, params: &AtmoLayerParams) {
        if self.samples.is_empty() {
            return;
        }
        let (path, total_length) =
            self.samples[(rand::random::<f32>() * self.samples.len() as f32) as usize].clone();

        let playback_len = (total_length as f32 / params.playback_rate).ceil() as u32;
        self.next_trigger_countdown =
            (playback_len as f32 * (1.0 - params.density.clamp(-1.0, 1.0))) as i64;

        let pan = (rand::random::<f32>() * 2.0 - 1.0) * params.pan_randomness;

        if let Some(voice) = self.voices.get_mut(voice_index) {
            voice.start(
                path,
                params.playback_rate,
                pan,
                self.sample_rate,
                Some(0),
                None,
                0,
            );
        }
    }

    /// Processes a full buffer for this layer, adding its output to the buffer.
    pub fn process(&mut self, params: &AtmoLayerParams, output_buffer: &mut [[f32; 2]]) {
        if self.samples.is_empty() {
            return;
        }

        for i in 0..output_buffer.len() {
            self.next_trigger_countdown -= 1;

            // --- Triggering / Crossfade Logic ---
            if params.mode == PlaybackMode::FragmentLooping {
                let crossfade_samples = (1.0 * self.sample_rate) as u32;
                let mut voice_to_fade_out = None;

                // Check if any playing voice is about to end
                for (voice_index, voice) in self.voices.iter().enumerate() {
                    if voice.state == AtmoVoiceState::Playing
                        && (voice.samples_to_play - voice.samples_played) <= crossfade_samples
                    {
                        voice_to_fade_out = Some(voice_index);
                        break;
                    }
                }

                if let Some(fading_voice_index) = voice_to_fade_out {
                    // Find a new voice to start the next fragment
                    let available_voice_index = self.voices.iter().position(|v| !v.is_active());
                    if let Some(new_voice_index) = available_voice_index {
                        self.start_new_fragment_for_voice(new_voice_index, params);
                        if let Some(fading_voice) = self.voices.get_mut(fading_voice_index) {
                            fading_voice.fade_out(crossfade_samples);
                        }
                    }
                }
                // If no voices are active at all, start one.
                else if !self.voices.iter().any(|v| v.is_active()) {
                    if let Some(voice_index) = self.voices.iter().position(|v| !v.is_active()) {
                        self.start_new_fragment_for_voice(voice_index, params);
                    }
                }
            } else {
                // TriggeredEvents Mode
                if self.next_trigger_countdown <= 0 {
                    if let Some(voice_index) = self.voices.iter().position(|v| !v.is_active()) {
                        self.start_new_one_shot_for_voice(voice_index, params);
                    }
                }
            }

            // --- Process All Voices ---
            let filter_coeff = params.filter_cutoff.powi(2) * 0.95 + 0.01;
            let mut frame = [0.0, 0.0];

            for voice in self.voices.iter_mut() {
                if !voice.is_active() {
                    continue;
                }
                let voice_frame = voice.process(filter_coeff);
                frame[0] += voice_frame[0];
                frame[1] += voice_frame[1];
            }

            // --- Mix to Output ---
            output_buffer[i][0] += frame[0] * params.volume;
            output_buffer[i][1] += frame[1] * params.volume;
        }
    }
}

/// Processes all 4 layers for a single scene.
pub struct AtmoSceneProcessor {
    pub layers: [AtmoLayerProcessor; 4],
}

impl AtmoSceneProcessor {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            layers: std::array::from_fn(|_| AtmoLayerProcessor::new(sample_rate)),
        }
    }

    /// Processes all 4 layers, applying direct volume controls, and writes the result to a buffer.
    pub fn process(
        &mut self,
        scene_params: &AtmoScene,
        layer_volumes: &[Arc<AtomicU32>; 4],
        output_buffer: &mut [[f32; 2]],
    ) {
        // Clear the buffer before processing
        output_buffer.iter_mut().for_each(|frame| *frame = [0.0, 0.0]);

        for (i, layer_processor) in self.layers.iter_mut().enumerate() {
            let mut params = scene_params.layers[i].params;
            let direct_layer_vol =
                layer_volumes[i].load(Ordering::Relaxed) as f32 / super::PARAM_SCALER;
            params.volume *= direct_layer_vol; // Apply the direct volume fader from the mixer
            layer_processor.process(&params, output_buffer); // This adds its output to the buffer
        }
    }
}

/// An Automatic Gain Control processor to normalize the Atmo engine's output in real-time.
struct AtmoAutoGain {
    envelope: f32,
    attack_coeff: f32,
    release_coeff: f32,
}

impl AtmoAutoGain {
    fn new(sample_rate: f32) -> Self {
        const ATTACK_MS: f32 = 20.0;
        const RELEASE_MS: f32 = 500.0;
        const TARGET_RMS: f32 = 0.25; // Target loudness

        Self {
            envelope: TARGET_RMS,
            attack_coeff: (-(1.0 / (ATTACK_MS * 0.001 * sample_rate))).exp(),
            release_coeff: (-(1.0 / (RELEASE_MS * 0.001 * sample_rate))).exp(),
        }
    }

    fn process(&mut self, frame: &mut [f32; 2]) {
        const TARGET_RMS: f32 = 0.25;
        const MAX_GAIN: f32 = 8.0; // +18 dB limit to prevent extreme gain on silence

        // 1. Get RMS of the current stereo frame
        let frame_rms = ((frame[0] * frame[0] + frame[1] * frame[1]) * 0.5).sqrt();

        // 2. Update the envelope follower
        if frame_rms > self.envelope {
            self.envelope = self.attack_coeff * (self.envelope - frame_rms) + frame_rms;
        } else {
            self.envelope = self.release_coeff * (self.envelope - frame_rms) + frame_rms;
        }
        self.envelope = self.envelope.max(1e-9); // Prevent division by zero

        // 3. Calculate makeup gain
        let makeup_gain = (TARGET_RMS / self.envelope).min(MAX_GAIN);

        // 4. Apply gain to the frame
        frame[0] *= makeup_gain;
        frame[1] *= makeup_gain;
    }
}

/// The main Atmo engine on the audio thread.
pub struct AtmoEngine {
    pub scene_processors: [AtmoSceneProcessor; 4],
    scenes: [AtmoScene; 4],
    xy_coords: Arc<AtomicU64>,
    pub layer_volumes: [Arc<AtomicU32>; 4],
    scene_buffers: [Vec<[f32; 2]>; 4],
    auto_gain: AtmoAutoGain,
}

impl AtmoEngine {
    pub fn new(
        sample_rate: f32,
        xy_coords: Arc<AtomicU64>,
        layer_volumes: [Arc<AtomicU32>; 4],
    ) -> Self {
        Self {
            scene_processors: std::array::from_fn(|_| AtmoSceneProcessor::new(sample_rate)),
            scenes: Default::default(),
            xy_coords,
            layer_volumes,
            // MODIFIED: Initialize scene buffers to their maximum safe size.
            scene_buffers: std::array::from_fn(|_| vec![[0.0; 2]; MAX_BUFFER_SIZE]),
            auto_gain: AtmoAutoGain::new(sample_rate),
        }
    }

    pub fn load_layer_samples(
        &mut self,
        scene_index: usize,
        layer_index: usize,
        samples: Vec<(PathBuf, u32)>,
    ) {
        if let Some(scene_processor) = self.scene_processors.get_mut(scene_index) {
            if let Some(layer) = scene_processor.layers.get_mut(layer_index) {
                layer.load_samples(samples);
            }
        }
    }

    pub fn set_scene(&mut self, scene_index: usize, scene: AtmoScene) {
        if let Some(s) = self.scenes.get_mut(scene_index) {
            *s = scene;
        }
    }

    fn lerp(a: f32, b: f32, t: f32) -> f32 {
        a * (1.0 - t) + b * t
    }

    pub fn process(&mut self, output_buffer: &mut [[f32; 2]]) {
        // REMOVED: The entire block that resized scene_buffers has been deleted.

        const MIX_RADIUS: f32 = 0.5;

        let packed_coords = self.xy_coords.load(Ordering::Relaxed);
        let x_u32 = (packed_coords >> 32) as u32;
        let y_u32 = packed_coords as u32;
        let x = x_u32 as f32 / u32::MAX as f32;
        let y = y_u32 as f32 / u32::MAX as f32;

        let dist_from_center_sq = (x - 0.5).powi(2) + (y - 0.5).powi(2);

        if dist_from_center_sq > MIX_RADIUS * MIX_RADIUS {
            // Outside the radius: Snap to the nearest corner
            let corner_index = match (x > 0.5, y > 0.5) {
                (false, false) => 0, // Top-Left
                (true, false) => 1,  // Top-Right
                (false, true) => 2,  // Bottom-Left
                (true, true) => 3,   // Bottom-Right
            };
            self.scene_processors[corner_index].process(
                &self.scenes[corner_index],
                &self.layer_volumes,
                output_buffer, // Process directly into the output
            );
        } else {
            // Inside the radius: 4-way audio interpolation
            // 1. Process each scene into its own temporary buffer
            for i in 0..4 {
                // MODIFIED: Pass a slice of the correct length to the processor.
                self.scene_processors[i].process(
                    &self.scenes[i],
                    &self.layer_volumes,
                    &mut self.scene_buffers[i][..output_buffer.len()],
                );
            }

            // 2. Clear the main output buffer before mixing
            output_buffer.iter_mut().for_each(|frame| *frame = [0.0, 0.0]);

            // 3. Blend the four scene buffers into the main output buffer
            for i in 0..output_buffer.len() {
                // Bilinear interpolation of the audio signal from the 4 scene buffers
                // MODIFIED: Index into the slices correctly.
                let top_l = Self::lerp(self.scene_buffers[0][i][0], self.scene_buffers[1][i][0], x);
                let top_r = Self::lerp(self.scene_buffers[0][i][1], self.scene_buffers[1][i][1], x);
                let bot_l = Self::lerp(self.scene_buffers[2][i][0], self.scene_buffers[3][i][0], x);
                let bot_r = Self::lerp(self.scene_buffers[2][i][1], self.scene_buffers[3][i][1], x);

                output_buffer[i][0] = Self::lerp(top_l, bot_l, y);
                output_buffer[i][1] = Self::lerp(top_r, bot_r, y);
            }
        }

        // Apply the automatic gain control as the final stage
        for frame in output_buffer.iter_mut() {
            self.auto_gain.process(frame);
        }
    }
}