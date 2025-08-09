// src/midi.rs

use crate::audio_engine::{AudioCommand, MidiMessage};
use crate::fx;
use crate::fx_components::*;
use crate::settings::{ControllableParameter, FullMidiControlId, FxParamIdentifier, MidiControlId};
use anyhow::Result;
use midir::{Ignore, MidiInput, MidiInputConnection, MidiInputPort};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{mpsc::Sender, Arc, RwLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const APP_NAME: &str = "Cypher Looper";

// Generic debounce for most buttons to prevent electrical noise from double-triggering.
const DEBOUNCE_DURATION: Duration = Duration::from_millis(50);
// Hold duration for the clear action.
const LONG_PRESS_DURATION: Duration = Duration::from_millis(500);
// Polling interval for the hold-detection thread.
const HOLD_CHECK_INTERVAL: Duration = Duration::from_millis(50);

pub fn get_midi_ports() -> Result<Vec<(String, MidiInputPort)>> {
    let midi_in = MidiInput::new(APP_NAME)?;
    let ports = midi_in.ports();
    let mut result = Vec::with_capacity(ports.len());
    for port in ports.iter() {
        let name = midi_in.port_name(port)?;
        result.push((name, port.clone()));
    }
    Ok(result)
}

#[allow(clippy::too_many_arguments)]
pub fn connect_midi(
    command_sender: Sender<AudioCommand>,
    live_midi_notes: Arc<RwLock<BTreeSet<u8>>>,
    port: MidiInputPort,
    port_name: String,
    midi_mappings: Arc<RwLock<BTreeMap<FullMidiControlId, ControllableParameter>>>,
    midi_learn_target: Arc<RwLock<Option<ControllableParameter>>>,
    last_midi_cc_message: Arc<RwLock<Option<(FullMidiControlId, Instant)>>>,
    midi_cc_values: Arc<[[AtomicU32; 128]; 16]>,
    midi_mod_matrix_learn_target: Arc<RwLock<Option<(usize, usize)>>>,
    last_learned_mod_source: Arc<RwLock<Option<MidiControlId>>>,
    should_exit: Arc<AtomicBool>,
    should_clear_all_from_midi: Arc<AtomicBool>,
    fx_presets: BTreeMap<fx::InsertionPoint, fx::FxPreset>,
    fx_wet_dry_mixes: BTreeMap<fx::InsertionPoint, Arc<AtomicU32>>,
) -> Result<(MidiInputConnection<()>, JoinHandle<()>)> {
    let mut midi_in = MidiInput::new(APP_NAME)?;
    midi_in.ignore(Ignore::None);

    let in_port_name = midi_in.port_name(&port)?;
    println!("Opening MIDI connection to: {}", in_port_name);

    let held_looper_buttons = Arc::new(RwLock::new(BTreeMap::<FullMidiControlId, Instant>::new()));

    let held_buttons_clone = held_looper_buttons.clone();
    let mappings_clone = midi_mappings.clone();
    let command_sender_clone = command_sender.clone();
    let timer_handle = thread::spawn(move || {
        while !should_exit.load(Ordering::Relaxed) {
            thread::sleep(HOLD_CHECK_INTERVAL);

            let mut cleared_by_hold = BTreeSet::new();

            if let Ok(held_buttons_reader) = held_buttons_clone.read() {
                for (control_id, press_time) in held_buttons_reader.iter() {
                    if press_time.elapsed() >= LONG_PRESS_DURATION {
                        if let Ok(mappings) = mappings_clone.read() {
                            if let Some(ControllableParameter::Looper(index)) =
                                mappings.get(control_id).copied()
                            {
                                command_sender_clone
                                    .send(AudioCommand::ClearLooper(index))
                                    .ok();
                                cleared_by_hold.insert(control_id.clone());
                            }
                        }
                    }
                }
            }

            if !cleared_by_hold.is_empty() {
                if let Ok(mut held_buttons_writer) = held_buttons_clone.write() {
                    for control_id in cleared_by_hold {
                        held_buttons_writer.remove(&control_id);
                    }
                }
            }
        }
        println!("MIDI timer thread for '{}' exited gracefully.", in_port_name);
    });

    let mut last_press_times: BTreeMap<FullMidiControlId, Instant> = BTreeMap::new();
    let port_name_clone = port_name.clone();

    let conn_out = match midi_in.connect(
        &port,
        &format!("cypher-midi-in-{}", port_name),
        move |_stamp, message, _| {
            if message.len() < 3 {
                return;
            }

            let status = message[0] & 0xF0;
            let channel = message[0] & 0x0F;

            match status {
                0x90 | 0x80 => {
                    let msg = MidiMessage {
                        status: message[0],
                        data1: message[1],
                        data2: message[2],
                    };
                    if let Ok(mut notes) = live_midi_notes.write() {
                        if msg.status & 0xF0 == 0x90 && msg.data2 > 0 {
                            notes.insert(msg.data1);
                        } else {
                            notes.remove(&msg.data1);
                        }
                    }
                    command_sender.send(AudioCommand::MidiMessage(msg)).ok();
                }
                0xB0 => {
                    // Control Change (CC)
                    let cc = message[1];
                    let value = message[2];
                    let control_id = FullMidiControlId {
                        port_name: port_name_clone.clone(),
                        channel,
                        cc,
                    };

                    let scaled_value = (value as f32 / 127.0 * 1_000_000.0) as u32;
                    if let Some(chan_array) = midi_cc_values.get(channel as usize) {
                        if let Some(atomic_val) = chan_array.get(cc as usize) {
                            atomic_val.store(scaled_value, Ordering::Relaxed);
                        }
                    }
                    if let Ok(mut last_cc) = last_midi_cc_message.write() {
                        *last_cc = Some((control_id.clone(), Instant::now()));
                    }

                    if let Ok(target) = midi_mod_matrix_learn_target.try_read() {
                        if target.is_some() {
                            if let Ok(mut last_learned) = last_learned_mod_source.write() {
                                *last_learned = Some(MidiControlId { channel, cc });
                            }
                            return;
                        }
                    }
                    let target_param_to_set = midi_learn_target.write().unwrap().take();
                    if let Some(param) = target_param_to_set {
                        let mut mappings = midi_mappings.write().unwrap();
                        mappings.retain(|_, v| *v != param);
                        mappings.insert(control_id, param);
                        println!(
                            "MIDI learn: Mapped {:?} to CC {} on channel {} from device '{}'",
                            param,
                            cc,
                            channel + 1,
                            port_name_clone
                        );
                        return;
                    }

                    if let Ok(mappings) = midi_mappings.read() {
                        let wildcard_id = FullMidiControlId {
                            port_name: "".to_string(),
                            channel,
                            cc,
                        };
                        let param_opt =
                            mappings.get(&control_id).or_else(|| mappings.get(&wildcard_id));

                        if let Some(param) = param_opt.copied() {
                            match param {
                                ControllableParameter::Looper(index) => {
                                    if value > 64 {
                                        // PRESS event
                                        let now = Instant::now();
                                        let last_press = last_press_times
                                            .entry(control_id.clone())
                                            .or_insert_with(|| {
                                                now.checked_sub(DEBOUNCE_DURATION * 2)
                                                    .unwrap_or(now)
                                            });
                                        if now.duration_since(*last_press) > DEBOUNCE_DURATION {
                                            command_sender.send(AudioCommand::LooperPress(index)).ok();
                                            *last_press = now;
                                        }
                                        held_looper_buttons
                                            .write()
                                            .unwrap()
                                            .entry(control_id)
                                            .or_insert(Instant::now());
                                    } else {
                                        // RELEASE event
                                        held_looper_buttons.write().unwrap().remove(&control_id);
                                    }
                                }

                                ControllableParameter::MixerVolume(index) => {
                                    let vol = (value as f32 / 127.0) * 1.5;
                                    command_sender
                                        .send(AudioCommand::SetMixerTrackVolume {
                                            track_index: index,
                                            volume: vol,
                                        })
                                        .ok();
                                }
                                ControllableParameter::SynthMasterVolume => {
                                    let vol = (value as f32 / 127.0) * 1.5;
                                    command_sender.send(AudioCommand::SetSynthMasterVolume(vol)).ok();
                                }
                                ControllableParameter::SamplerMasterVolume => {
                                    let vol = (value as f32 / 127.0) * 1.5;
                                    command_sender.send(AudioCommand::SetSamplerMasterVolume(vol)).ok();
                                }
                                ControllableParameter::MasterVolume => {
                                    let vol = (value as f32 / 127.0) * 1.5;
                                    command_sender.send(AudioCommand::SetMasterVolume(vol)).ok();
                                }
                                ControllableParameter::LimiterThreshold => {
                                    let thresh = value as f32 / 127.0;
                                    command_sender.send(AudioCommand::SetLimiterThreshold(thresh)).ok();
                                }

                                // --- Correctly Handle FX Parameters ---
                                ControllableParameter::Fx(id) => {
                                    handle_fx_cc(&fx_presets, &fx_wet_dry_mixes, id, value);
                                }

                                _ => {
                                    if value > 64 {
                                        let now = Instant::now();
                                        let last_press = last_press_times
                                            .entry(control_id)
                                            .or_insert_with(|| {
                                                now.checked_sub(DEBOUNCE_DURATION * 2)
                                                    .unwrap_or(now)
                                            });
                                        if now.duration_since(*last_press) > DEBOUNCE_DURATION {
                                            if param == ControllableParameter::TransportClearAll {
                                                should_clear_all_from_midi.store(true, Ordering::Relaxed);
                                            } else {
                                                let command = match param {
                                                    ControllableParameter::MixerToggleMute(index) => Some(AudioCommand::ToggleMixerMute(index)),
                                                    ControllableParameter::MixerToggleSolo(index) => Some(AudioCommand::ToggleMixerSolo(index)),
                                                    ControllableParameter::SynthToggleActive => Some(AudioCommand::ToggleSynth),
                                                    ControllableParameter::SamplerToggleActive => Some(AudioCommand::ToggleSampler),
                                                    ControllableParameter::InputToggleArm => Some(AudioCommand::ToggleAudioInputArm),
                                                    ControllableParameter::InputToggleMonitor => Some(AudioCommand::ToggleAudioInputMonitoring),
                                                    ControllableParameter::TransportTogglePlay => Some(AudioCommand::ToggleTransport),
                                                    ControllableParameter::TransportToggleMuteAll => Some(AudioCommand::ToggleMuteAll),
                                                    ControllableParameter::TransportToggleRecord => Some(AudioCommand::ToggleRecord),
                                                    _ => None,
                                                };
                                                if let Some(cmd) = command {
                                                    command_sender.send(cmd).ok();
                                                }
                                            }
                                            *last_press = now;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        },
        (),
    ) {
        Ok(conn) => conn,
        Err(e) => return Err(anyhow::anyhow!("Failed to connect to MIDI port: {}", e)),
    };

    println!("Connection open to {}. Enjoy!", port_name);
    Ok((conn_out, timer_handle))
}

/// Handles a MIDI CC message targeted at an FX parameter.
fn handle_fx_cc(
    presets: &BTreeMap<fx::InsertionPoint, fx::FxPreset>,
    wet_dry_mixes: &BTreeMap<fx::InsertionPoint, Arc<AtomicU32>>,
    id: FxParamIdentifier,
    value: u8,
) {
    if id.param_name == crate::settings::FxParamName::WetDry {
        if let Some(atomic_param) = wet_dry_mixes.get(&id.point) {
            let scaled_val = scale_midi_to_param(id.param_name, value);
            atomic_param.store(scaled_val, Ordering::Relaxed);
        }
    } else if let Some(preset) = presets.get(&id.point) {
        if let Some(link) = preset.chain.get(id.component_index) {
            if let Some(atomic_param) = link.params.get_param(id.param_name.as_str()) {
                let scaled_val = scale_midi_to_param(id.param_name, value);
                atomic_param.store(scaled_val, Ordering::Relaxed);
            }
        }
    }
}

/// Scales a 0-127 MIDI value to the u32 format expected by a specific atomic parameter.
fn scale_midi_to_param(param_name: crate::settings::FxParamName, midi_value: u8) -> u32 {
    let val_norm = midi_value as f32 / 127.0; // Value normalized to 0.0 - 1.0

    use crate::settings::FxParamName::*;
    match param_name {
        GainDb => {
            let db = -60.0 + val_norm * (24.0 - -60.0); // Map to -60..24 dB
            ((db + gain::DB_OFFSET) * gain::DB_SCALER) as u32
        }
        TimeMs => ((val_norm * 2000.0) * delay::PARAM_SCALER) as u32,
        Feedback => (val_norm * 0.99 * delay::PARAM_SCALER) as u32,
        Damping => (val_norm * delay::PARAM_SCALER) as u32,
        FrequencyHz => {
            let freq = 20.0 * (20000.0f32 / 20.0).powf(val_norm); // Logarithmic mapping
            (freq * filter::PARAM_SCALER) as u32
        }
        Resonance => (val_norm * filter::PARAM_SCALER) as u32,
        DriveDb => ((val_norm * 48.0) * waveshaper::DB_SCALER) as u32,
        BitDepth => {
            let bits = 1.0 + val_norm * 15.0;
            (bits * quantizer::PARAM_SCALER) as u32
        }
        Downsample => {
            let factor = 1.0 + val_norm * 49.0;
            (factor * quantizer::PARAM_SCALER) as u32
        }
        Size | Decay => (val_norm * reverb::PARAM_SCALER) as u32,
        RateHz => {
            let rate = 0.01 * (10.0f32 / 0.01).powf(val_norm);
            (rate * flanger::PARAM_SCALER) as u32
        }
        DepthMs => {
            let depth = 0.1 + val_norm * (10.0 - 0.1);
            (depth * flanger::PARAM_SCALER) as u32
        }
        // These are not typically MIDI-mappable in the same way (they're toggles or require more complex logic)
        // A simple linear mapping is provided as a default.
        Mode | Waveform => (val_norm * 5.0).round() as u32,
        AttackMs => ((1.0 + val_norm * 199.0) * envelope_follower::PARAM_SCALER) as u32,
        ReleaseMs => ((10.0 + val_norm * 990.0) * envelope_follower::PARAM_SCALER) as u32,
        // WetDry is 0.0 to 1.0
        WetDry => (val_norm * delay::PARAM_SCALER) as u32,
        Bypass => {
            if val_norm > 0.5 {
                1
            } else {
                0
            }
        }
    }
}