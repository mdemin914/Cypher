use crate::audio_engine::{AudioCommand, MidiMessage};
use crate::settings::{ControllableParameter, MidiControlId};
use anyhow::Result;
use midir::{Ignore, MidiInput, MidiInputConnection, MidiInputPort};
use std::collections::{BTreeMap, BTreeSet};
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

pub fn connect_midi(
    command_sender: Sender<AudioCommand>,
    live_midi_notes: Arc<RwLock<BTreeSet<u8>>>,
    port: MidiInputPort,
    midi_mappings: Arc<RwLock<BTreeMap<MidiControlId, ControllableParameter>>>,
    midi_learn_target: Arc<RwLock<Option<ControllableParameter>>>,
    last_midi_cc_message: Arc<RwLock<Option<(MidiControlId, Instant)>>>,
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

                    // Update UI feedback for the MIDI mapping window
                    if let Ok(mut last_cc) = last_midi_cc_message.write() {
                        *last_cc = Some((control_id, Instant::now()));
                    }

                    // --- MIDI Learn Logic (Refactored to prevent deadlock) ---
                    let target_param_to_set: Option<ControllableParameter>;
                    { // Scoped to release the lock immediately
                        let mut learn_target = midi_learn_target.write().unwrap();
                        target_param_to_set = learn_target.take(); // take() gets the value and sets the Option to None
                    }

                    if let Some(param) = target_param_to_set {
                        let mut mappings = midi_mappings.write().unwrap();
                        // First, remove any existing mapping for this parameter to avoid duplicates
                        mappings.retain(|_, v| *v != param);
                        // Then, insert the new mapping
                        mappings.insert(control_id, param);
                        println!("MIDI learn: Mapped {:?} to CC {} on channel {}", param, cc, channel + 1);
                        return; // Don't process the command immediately after learning
                    }

                    // --- Mapped Control Execution Logic ---
                    if let Ok(mappings) = midi_mappings.read() {
                        if let Some(param) = mappings.get(&control_id) {
                            match *param {
                                ControllableParameter::Looper(index) => {
                                    let now = Instant::now();
                                    // Get the last press time, or initialize it to a time in the past
                                    // so that the first press always succeeds.
                                    let last_press = last_press_times.entry(control_id).or_insert_with(|| {
                                        now.checked_sub(DEBOUNCE_DURATION * 2).unwrap_or(now)
                                    });

                                    // If enough time has passed since the last press, send the command.
                                    if now.duration_since(*last_press) > DEBOUNCE_DURATION {
                                        command_sender.send(AudioCommand::LooperPress(index)).ok();
                                        *last_press = now;
                                    }
                                }
                                ControllableParameter::MixerVolume(index) => {
                                    // Map MIDI 0-127 to our fader range 0.0-1.5
                                    let volume = (value as f32 / 127.0) * 1.5;
                                    command_sender.send(AudioCommand::SetMixerTrackVolume {
                                        track_index: index,
                                        volume,
                                    }).ok();
                                }
                            }
                        }
                    }
                }
                _ => {} // Ignore other message types
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