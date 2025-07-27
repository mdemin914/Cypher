// src/midi.rs
use crate::audio_engine::{AudioCommand, MidiMessage};
use crate::settings::{ControllableParameter, MidiControlId};
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
// Hold duration for the clear action. 1 second is a safe value to prevent accidental clears.
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
    midi_mappings: Arc<RwLock<BTreeMap<MidiControlId, ControllableParameter>>>,
    midi_learn_target: Arc<RwLock<Option<ControllableParameter>>>,
    last_midi_cc_message: Arc<RwLock<Option<(MidiControlId, Instant)>>>,
    midi_cc_values: Arc<[[AtomicU32; 128]; 16]>,
    midi_mod_matrix_learn_target: Arc<RwLock<Option<(usize, usize)>>>,
    last_learned_mod_source: Arc<RwLock<Option<MidiControlId>>>,
    should_exit: Arc<AtomicBool>, // <-- NEW PARAMETER
) -> Result<(MidiInputConnection<()>, JoinHandle<()>)> { // <-- CHANGED RETURN TYPE
    let mut midi_in = MidiInput::new(APP_NAME)?;
    midi_in.ignore(Ignore::None);

    let in_port_name = midi_in.port_name(&port)?;
    println!("Opening MIDI connection to: {}", in_port_name);

    // --- NEW THREAD-SAFE STATE FOR HOLD DETECTION ---
    let held_looper_buttons = Arc::new(RwLock::new(BTreeMap::<MidiControlId, Instant>::new()));

    // --- NEW TIMER THREAD FOR CONTINUOUS HOLD CHECKING ---
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
                        if let Some(ControllableParameter::Looper(index)) =
                            mappings_clone.read().unwrap().get(control_id).copied()
                        {
                            command_sender_clone
                                .send(AudioCommand::ClearLooper(index))
                                .ok();
                            cleared_by_hold.insert(*control_id);
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
        println!("MIDI timer thread exited gracefully.");
    });


    // State for debouncing simple button presses. This remains local to the MIDI thread.
    let mut last_press_times: BTreeMap<MidiControlId, Instant> = BTreeMap::new();

    let conn_out = match midi_in.connect(
        &port,
        "cypher-midi-in",
        move |_stamp, message, _| {
            if message.len() < 3 { return; }

            let status = message[0] & 0xF0;
            let channel = message[0] & 0x0F;

            match status {
                0x90 | 0x80 => { // Note On/Off (Unchanged)
                    let msg = MidiMessage { status: message[0], data1: message[1], data2: message[2] };
                    if let Ok(mut notes) = live_midi_notes.write() {
                        if msg.status & 0xF0 == 0x90 && msg.data2 > 0 { notes.insert(msg.data1); } else { notes.remove(&msg.data1); }
                    }
                    command_sender.send(AudioCommand::MidiMessage(msg)).ok();
                }
                0xB0 => { // Control Change (CC)
                    let cc = message[1];
                    let value = message[2];
                    let control_id = MidiControlId { channel, cc };

                    // Shared CC State & UI Feedback (Unchanged)
                    let scaled_value = (value as f32 / 127.0 * 1_000_000.0) as u32;
                    if let Some(chan_array) = midi_cc_values.get(channel as usize) {
                        if let Some(atomic_val) = chan_array.get(cc as usize) {
                            atomic_val.store(scaled_value, Ordering::Relaxed);
                        }
                    }
                    if let Ok(mut last_cc) = last_midi_cc_message.write() { *last_cc = Some((control_id, Instant::now())); }

                    // MIDI Learn Logic (Unchanged)
                    if let Ok(target) = midi_mod_matrix_learn_target.try_read() {
                        if target.is_some() {
                            if let Ok(mut last_learned) = last_learned_mod_source.write() { *last_learned = Some(control_id); }
                            return;
                        }
                    }
                    let target_param_to_set = midi_learn_target.write().unwrap().take();
                    if let Some(param) = target_param_to_set {
                        let mut mappings = midi_mappings.write().unwrap();
                        mappings.retain(|_, v| *v != param);
                        mappings.insert(control_id, param);
                        println!("MIDI learn: Mapped {:?} to CC {} on channel {}", param, cc, channel + 1);
                        return;
                    }

                    // Mapped Control Execution Logic
                    if let Ok(mappings) = midi_mappings.read() {
                        if let Some(param) = mappings.get(&control_id).copied() {
                            match param {
                                // --- REVISED LOGIC FOR LOOPER TRIGGERS ---
                                ControllableParameter::Looper(index) => {
                                    if value > 64 { // PRESS event
                                        // 1. Handle the TAP action
                                        let now = Instant::now();
                                        let last_press = last_press_times.entry(control_id).or_insert_with(|| now.checked_sub(DEBOUNCE_DURATION * 2).unwrap_or(now));
                                        if now.duration_since(*last_press) > DEBOUNCE_DURATION {
                                            command_sender.send(AudioCommand::LooperPress(index)).ok();
                                            *last_press = now;
                                        }
                                        // 2. Start the HOLD timer
                                        held_looper_buttons.write().unwrap().entry(control_id).or_insert(Instant::now());
                                    } else { // RELEASE event
                                        // Stop the HOLD timer
                                        held_looper_buttons.write().unwrap().remove(&control_id);
                                    }
                                }

                                // FADER-LIKE CONTROLS (Unchanged)
                                ControllableParameter::MixerVolume(index) => { let vol = (value as f32 / 127.0) * 1.5; command_sender.send(AudioCommand::SetMixerTrackVolume { track_index: index, volume: vol }).ok(); }
                                ControllableParameter::SynthMasterVolume => { let vol = (value as f32 / 127.0) * 1.5; command_sender.send(AudioCommand::SetSynthMasterVolume(vol)).ok(); }
                                ControllableParameter::SamplerMasterVolume => { let vol = (value as f32 / 127.0) * 1.5; command_sender.send(AudioCommand::SetSamplerMasterVolume(vol)).ok(); }
                                ControllableParameter::MasterVolume => { let vol = (value as f32 / 127.0) * 1.5; command_sender.send(AudioCommand::SetMasterVolume(vol)).ok(); }
                                ControllableParameter::LimiterThreshold => { let thresh = value as f32 / 127.0; command_sender.send(AudioCommand::SetLimiterThreshold(thresh)).ok(); }

                                // ALL OTHER BUTTON-LIKE CONTROLS (Unchanged)
                                _ => {
                                    if value > 64 {
                                        let now = Instant::now();
                                        let last_press = last_press_times.entry(control_id).or_insert_with(|| now.checked_sub(DEBOUNCE_DURATION * 2).unwrap_or(now));
                                        if now.duration_since(*last_press) > DEBOUNCE_DURATION {
                                            let command = match param {
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
                                                _ => None,
                                            };
                                            if let Some(cmd) = command { command_sender.send(cmd).ok(); *last_press = now; }
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
    // Return both the connection and the handle
    Ok((conn_out, timer_handle))
}