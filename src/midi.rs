// src/midi.rs
use crate::audio_engine::{AudioCommand, MidiMessage};
use crate::settings::{ControllableParameter, MidiControlId};
use anyhow::Result;
use midir::{Ignore, MidiInput, MidiInputConnection, MidiInputPort};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{mpsc::Sender, Arc, RwLock};
use std::time::{Duration, Instant};

const APP_NAME: &str = "Cypher Looper";
const DEBOUNCE_DURATION: Duration = Duration::from_millis(10);

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
    midi_mappings: Arc<RwLock<BTreeMap<MidiControlId, ControllableParameter>>>,
    midi_learn_target: Arc<RwLock<Option<ControllableParameter>>>,
    last_midi_cc_message: Arc<RwLock<Option<(MidiControlId, Instant)>>>,
    midi_cc_values: Arc<[[AtomicU32; 128]; 16]>,
    midi_mod_matrix_learn_target: Arc<RwLock<Option<(usize, usize)>>>,
    last_learned_mod_source: Arc<RwLock<Option<MidiControlId>>>,
) -> Result<MidiInputConnection<()>> {
    let mut midi_in = MidiInput::new(APP_NAME)?;
    midi_in.ignore(Ignore::None);

    let in_port_name = midi_in.port_name(&port)?;
    println!("Opening MIDI connection to: {}", in_port_name);

    // State for debouncing button-style CCs
    let mut last_press_times: BTreeMap<MidiControlId, Instant> = BTreeMap::new();

    let conn_out = match midi_in.connect(
        &port,
        "cypher-midi-in",
        move |_stamp, message, _| {
            if message.len() < 3 {
                return;
            }

            let status = message[0] & 0xF0;
            let channel = message[0] & 0x0F;

            match status {
                // Note On/Off
                0x90 | 0x80 => {
                    let msg = MidiMessage {
                        status: message[0],
                        data1: message[1],
                        data2: message[2],
                    };

                    if let Ok(mut notes) = live_midi_notes.write() {
                        let note = msg.data1;
                        let velocity = msg.data2;
                        if status == 0x90 && velocity > 0 {
                            notes.insert(note);
                        } else {
                            notes.remove(&note);
                        }
                    }

                    let command = AudioCommand::MidiMessage(msg);
                    command_sender.send(command).ok();
                }
                // Control Change (CC)
                0xB0 => {
                    let cc = message[1];
                    let value = message[2];
                    let control_id = MidiControlId { channel, cc };

                    // --- ALWAYS update the shared CC state for modulation ---
                    let scaled_value = (value as f32 / 127.0 * 1_000_000.0) as u32;
                    if let Some(chan_array) = midi_cc_values.get(channel as usize) {
                        if let Some(atomic_val) = chan_array.get(cc as usize) {
                            atomic_val.store(scaled_value, Ordering::Relaxed);
                        }
                    }

                    // --- Update UI feedback for the MIDI mapping window ---
                    if let Ok(mut last_cc) = last_midi_cc_message.write() {
                        *last_cc = Some((control_id, Instant::now()));
                    }

                    // --- MIDI Learn Logic for Mod Matrix (DEADLOCK-SAFE) ---
                    // This now only writes to its own dedicated Arc, it does not read the target.
                    if let Ok(target) = midi_mod_matrix_learn_target.try_read() {
                        if target.is_some() {
                            if let Ok(mut last_learned) = last_learned_mod_source.write() {
                                *last_learned = Some(control_id);
                            }
                            // Once we've captured the CC, we don't process it as a regular mapped control this one time.
                            return;
                        }
                    }

                    // --- MIDI Learn Logic for Main Mappings ---
                    let target_param_to_set: Option<ControllableParameter>;
                    {
                        let mut learn_target = midi_learn_target.write().unwrap();
                        target_param_to_set = learn_target.take();
                    }

                    if let Some(param) = target_param_to_set {
                        let mut mappings = midi_mappings.write().unwrap();
                        mappings.retain(|_, v| *v != param);
                        mappings.insert(control_id, param);
                        println!("MIDI learn: Mapped {:?} to CC {} on channel {}", param, cc, channel + 1);
                        return;
                    }

                    // --- Mapped Control Execution Logic ---
                    if let Ok(mappings) = midi_mappings.read() {
                        if let Some(param) = mappings.get(&control_id) {
                            match *param {
                                // SPECIAL CASE: Looper triggers need to be instant and not rely on value > 64
                                ControllableParameter::Looper(index) => {
                                    let now = Instant::now();
                                    let last_press = last_press_times.entry(control_id).or_insert_with(|| now.checked_sub(DEBOUNCE_DURATION * 2).unwrap_or(now));
                                    if now.duration_since(*last_press) > DEBOUNCE_DURATION {
                                        command_sender.send(AudioCommand::LooperPress(index)).ok();
                                        *last_press = now;
                                    }
                                }

                                // FADER-LIKE CONTROLS
                                ControllableParameter::MixerVolume(index) => {
                                    let volume = (value as f32 / 127.0) * 1.5;
                                    command_sender.send(AudioCommand::SetMixerTrackVolume { track_index: index, volume }).ok();
                                }
                                ControllableParameter::SynthMasterVolume => {
                                    let volume = (value as f32 / 127.0) * 1.5;
                                    command_sender.send(AudioCommand::SetSynthMasterVolume(volume)).ok();
                                }
                                ControllableParameter::SamplerMasterVolume => {
                                    let volume = (value as f32 / 127.0) * 1.5;
                                    command_sender.send(AudioCommand::SetSamplerMasterVolume(volume)).ok();
                                }
                                ControllableParameter::MasterVolume => {
                                    let volume = (value as f32 / 127.0) * 1.5;
                                    command_sender.send(AudioCommand::SetMasterVolume(volume)).ok();
                                }
                                ControllableParameter::LimiterThreshold => {
                                    let threshold = value as f32 / 127.0;
                                    command_sender.send(AudioCommand::SetLimiterThreshold(threshold)).ok();
                                }

                                // ALL OTHER BUTTON-LIKE CONTROLS
                                _ => {
                                    if value > 64 { // Only trigger on "press" (value > half)
                                        let now = Instant::now();
                                        let last_press = last_press_times.entry(control_id).or_insert_with(|| now.checked_sub(DEBOUNCE_DURATION * 2).unwrap_or(now));
                                        if now.duration_since(*last_press) > DEBOUNCE_DURATION {
                                            let command = match *param {
                                                ControllableParameter::MixerToggleMute(index) => Some(AudioCommand::ToggleMixerMute(index)),
                                                ControllableParameter::MixerToggleSolo(index) => Some(AudioCommand::ToggleMixerSolo(index)),
                                                ControllableParameter::SynthToggleActive => Some(AudioCommand::ToggleSynth),
                                                ControllableParameter::SamplerToggleActive => Some(AudioCommand::ToggleSampler),
                                                ControllableParameter::InputToggleArm => Some(AudioCommand::ToggleAudioInputArm),
                                                ControllableParameter::InputToggleMonitor => Some(AudioCommand::ToggleAudioInputMonitoring),
                                                ControllableParameter::TransportTogglePlay => Some(AudioCommand::ToggleTransport),
                                                ControllableParameter::TransportToggleMuteAll => Some(AudioCommand::ToggleMuteAll),
                                                ControllableParameter::TransportClearAll => Some(AudioCommand::ClearAll),
                                                ControllableParameter::TransportToggleRecord => Some(AudioCommand::ToggleRecord),
                                                _ => None, // This branch will now only contain non-button parameters, which we already handled
                                            };
                                            if let Some(cmd) = command {
                                                command_sender.send(cmd).ok();
                                                *last_press = now;
                                            }
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

    println!("Connection open to {}. Enjoy!", in_port_name);
    Ok(conn_out)
}