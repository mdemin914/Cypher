// src/audio_io.rs

use crate::audio_engine::AudioEngine;
use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{
    BufferSize, Device, FromSample, HostId, Sample, SampleFormat, Stream, StreamConfig,
};
use ringbuf::HeapProducer;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::Arc;

pub fn init_and_run_streams(
    host_id: HostId,
    input_device_name: Option<String>,
    output_device_name: Option<String>,
    requested_sample_rate: Option<u32>,
    requested_buffer_size: Option<u32>,
    audio_input_producer: HeapProducer<f32>,
    engine: AudioEngine,
    xrun_count: Arc<AtomicUsize>,
) -> Result<(Stream, Stream, u32, u32)> {
    let host = cpal::host_from_id(host_id)?;
    let input_device = if let Some(name) = &input_device_name {
        host.input_devices()?
            .find(|d| d.name().ok().as_ref() == Some(name))
            .ok_or_else(|| anyhow::anyhow!("Input device not found: {}", name))?
    } else {
        host.default_input_device()
            .ok_or_else(|| anyhow::anyhow!("No default input device"))?
    };
    let output_device = if let Some(name) = &output_device_name {
        host.output_devices()?
            .find(|d| d.name().ok().as_ref() == Some(name))
            .ok_or_else(|| anyhow::anyhow!("Output device not found: {}", name))?
    } else {
        host.default_output_device()
            .ok_or_else(|| anyhow::anyhow!("No default output device"))?
    };
    println!("Using input device: {}", input_device.name()?);
    println!("Using output device: {}", output_device.name()?);

    let default_input_config = input_device.default_input_config()?;
    let default_output_config = output_device.default_output_config()?;

    let sample_format = default_output_config.sample_format();

    let mut final_input_config: StreamConfig = default_input_config.into();
    if let Some(sr) = requested_sample_rate {
        final_input_config.sample_rate = cpal::SampleRate(sr);
    }
    if let Some(bs) = requested_buffer_size {
        final_input_config.buffer_size = BufferSize::Fixed(bs);
    }

    let mut final_output_config: StreamConfig = default_output_config.into();
    if let Some(sr) = requested_sample_rate {
        final_output_config.sample_rate = cpal::SampleRate(sr);
    }
    if let Some(bs) = requested_buffer_size {
        final_output_config.buffer_size = BufferSize::Fixed(bs);
    }

    fn run<T>(
        input_device: &Device,
        input_config: &StreamConfig,
        output_device: &Device,
        output_config: &StreamConfig,
        audio_producer: HeapProducer<f32>,
        engine: AudioEngine,
        xrun_count: Arc<AtomicUsize>,
    ) -> Result<(Stream, Stream)>
    where
        T: Sample + cpal::SizedSample + FromSample<f32>,
        f32: FromSample<T>,
    {
        let input_latency_compensation_ms = engine.input_latency_compensation_ms.clone();
        let input_stream =
            build_input_stream::<T>(input_device, input_config, audio_producer, xrun_count.clone())?;
        let output_stream =
            build_output_stream::<T>(output_device, output_config, engine, xrun_count, input_latency_compensation_ms, output_config.sample_rate.0)?;
        input_stream.play()?;
        output_stream.play()?;
        Ok((input_stream, output_stream))
    }

    let (input_stream, output_stream) = match sample_format {
        SampleFormat::F32 => run::<f32>(&input_device, &final_input_config, &output_device, &final_output_config, audio_input_producer, engine, xrun_count)?,
        SampleFormat::I16 => run::<i16>(&input_device, &final_input_config, &output_device, &final_output_config, audio_input_producer, engine, xrun_count)?,
        SampleFormat::U16 => run::<u16>(&input_device, &final_input_config, &output_device, &final_output_config, audio_input_producer, engine, xrun_count)?,
        format => return Err(anyhow::anyhow!("Unsupported sample format {}", format)),
    };

    let active_sr = final_output_config.sample_rate.0;
    let active_bs = match final_output_config.buffer_size {
        BufferSize::Fixed(size) => size,
        BufferSize::Default => 512, // A reasonable assumption if default
    };

    println!(
        "Successfully started streams with Sample Rate: {} Hz, Buffer Size: {} Samples",
        active_sr, active_bs
    );

    Ok((input_stream, output_stream, active_sr, active_bs))
}

fn build_input_stream<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    mut producer: HeapProducer<f32>,
    xrun_count: Arc<AtomicUsize>,
) -> Result<Stream>
where
    T: Sample + cpal::SizedSample,
    f32: FromSample<T>,
{
    let err_fn = {
        let xrun_count_clone = xrun_count.clone();
        move |err| {
            eprintln!("an error occurred on input stream: {}", err);
            xrun_count_clone.fetch_add(1, Ordering::Relaxed);
        }
    };
    let channels = config.channels as usize;

    let stream = device.build_input_stream(
        config,
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            for frame in data.chunks(channels) {
                let mono_sample =
                    frame.iter().map(|s| f32::from_sample(*s)).sum::<f32>() / (channels as f32);
                if producer.push(mono_sample).is_err() {
                    // buffer full, drop sample
                }
            }
        },
        err_fn,
        None,
    )?;
    Ok(stream)
}

fn build_output_stream<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    mut engine: AudioEngine,
    xrun_count: Arc<AtomicUsize>,
    input_latency_compensation_ms: Arc<AtomicU32>,
    sample_rate: u32,
) -> Result<Stream>
where
    T: Sample + cpal::SizedSample + FromSample<f32>,
{
    let channels = config.channels as usize;
    let err_fn = {
        let xrun_count_clone = xrun_count.clone();
        move |err| {
            eprintln!("an error occurred on output stream: {}", err);
            xrun_count_clone.fetch_add(1, Ordering::Relaxed);
        }
    };
    let mut input_buffer: Vec<f32> = vec![];

    let stream = device.build_output_stream(
        config,
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            engine.handle_commands();
            let num_samples = data.len() / channels;
            input_buffer.resize(num_samples, 0.0);

            let consumer = &mut engine.input_consumer;

            // --- Smart Latency Manager ---
            // 1. Get the desired latency in ms from the UI, convert to samples
            let latency_ms = input_latency_compensation_ms.load(Ordering::Relaxed) as f32 / 100.0;
            let target_len = (latency_ms / 1000.0 * sample_rate as f32).round() as usize;

            // 2. If the queue is longer than our desired safety buffer, drain the excess
            if consumer.len() > target_len {
                consumer.skip(consumer.len() - target_len);
            }

            let samples_read = consumer.pop_slice(&mut input_buffer);

            if samples_read < num_samples {
                input_buffer[samples_read..]
                    .iter_mut()
                    .for_each(|s| *s = 0.0);
            }
            // **THE FIX IS HERE**: Pass the buffer as mutable
            let output_buffer = engine.process_buffer(&mut input_buffer);
            for (i, frame) in data.chunks_mut(channels).enumerate() {
                let sample_value = output_buffer.get(i).copied().unwrap_or(0.0);
                for sample in frame.iter_mut() {
                    *sample = T::from_sample(sample_value);
                }
            }
        },
        err_fn,
        None,
    )?;
    Ok(stream)
}