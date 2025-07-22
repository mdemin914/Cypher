// src/midi.rs
use crate::audio_engine::{AudioCommand, MidiMessage};
use anyhow::Result;
use midir::{Ignore, MidiInput, MidiInputConnection, MidiInputPort};
use std::collections::BTreeSet;
use std::sync::{mpsc::Sender, Arc, RwLock};

const APP_NAME: &str = "Cypher Looper";

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
) -> Result<MidiInputConnection<()>> {
    let mut midi_in = MidiInput::new(APP_NAME)?;
    midi_in.ignore(Ignore::None);

    let in_port_name = midi_in.port_name(&port)?;
    println!("Opening MIDI connection to: {}", in_port_name);

    let conn_out = match midi_in.connect(
        &port,
        "cypher-midi-in",
        move |_stamp, message, _| {
            // We now update both the UI state and send a command to the audio thread.
            if message.len() == 3 {
                let msg = MidiMessage {
                    status: message[0],
                    data1: message[1],
                    data2: message[2],
                };

                // Update the UI state's set of live notes directly.
                if let Ok(mut notes) = live_midi_notes.write() {
                    let status = msg.status & 0xF0;
                    let note = msg.data1;
                    let velocity = msg.data2;
                    if status == 0x90 && velocity > 0 { // Note On
                        notes.insert(note);
                    } else { // Note Off (status 0x80 or velocity 0)
                        notes.remove(&note);
                    }
                }

                // Forward the raw message to the audio thread for sound generation.
                let command = AudioCommand::MidiMessage(msg);
                if command_sender.send(command).is_err() {
                    // This happens if the audio engine's command proxy thread has been shut down.
                }
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