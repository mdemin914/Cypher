// FILE: src\audio_engine\helpers.rs
// =================================

use anyhow::Result;
use hound;
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

pub struct Limiter {
    attack_coeffs: f32,
    envelope: f32,
    pub gain_reduction_db: Arc<AtomicU32>,
}

impl Limiter {
    pub fn new(sample_rate: f32, gain_reduction_db: Arc<AtomicU32>) -> Self {
        let attack_ms = 0.01;
        Self {
            attack_coeffs: (-(1.0 / (attack_ms * 0.001 * sample_rate))).exp(),
            envelope: 0.0,
            gain_reduction_db,
        }
    }

    pub fn process(&mut self, input: f32, threshold: f32, release_coeffs: f32) -> f32 {
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

pub struct Metronome {
    phase: f32,
    envelope: f32,
    sample_rate: f32,
    pitch: f32, // Add this field
}

impl Metronome {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            phase: 0.0,
            envelope: 0.0,
            sample_rate,
            pitch: 880.0, // Default pitch
        }
    }

    pub fn trigger(&mut self, pitch_hz: f32) {
        self.envelope = 1.0;
        self.phase = 0.0;
        self.pitch = pitch_hz; // Set the pitch for this specific trigger
    }

    pub fn process(&mut self) -> f32 {
        if self.envelope <= 1e-6 {
            return 0.0;
        }
        let phase_inc = self.pitch / self.sample_rate; // Use the stored pitch
        self.phase = (self.phase + phase_inc) % 1.0;
        let sine_sample = (self.phase * std::f32::consts::TAU).sin();

        let output = sine_sample * self.envelope;
        self.envelope *= 0.999; // Very fast exponential decay

        output
    }
}

pub fn write_wav_file(path: &Path, audio_buffer: &[f32], sample_rate: f32) -> Result<()> {
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

pub fn trim_silence(audio_buffer: Vec<f32>) -> Vec<f32> {
    const SILENCE_THRESHOLD: f32 = 0.005; // RMS threshold
    const BLOCK_SIZE: usize = 512; // Analyze in chunks of 512 samples
    const REQUIRED_BLOCKS: usize = 3; // Need 3 consecutive blocks of sound to confirm start

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