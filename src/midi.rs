use crate::audio_engine::{AudioCommand, MidiMessage};
use crate::fx;
use crate::fx_components::*;
use crate::settings::{
    ControllableParameter, FullMidiControlId, FullMidiIdentifier, FullMidiNoteId,
    FxParamIdentifier, FxParamName, MidiControlId, MidiControlMode,
};
use anyhow::Result;
use midir::{Ignore, MidiInput, MidiInputConnection, MidiInputPort};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, AtomicI8, AtomicU32, AtomicU64, Ordering};
use std::sync::{mpsc::Sender, Arc, RwLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const APP_NAME: &str = "Cypher Looper";

const DEBOUNCE_DURATION: Duration = Duration::from_millis(50);
const LONG_PRESS_DURATION: Duration = Duration::from_millis(500);
const HOLD_CHECK_INTERVAL: Duration = Duration::from_millis(50);
const RELATIVE_SENSITIVITY: f32 = 0.005;

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
    audio_note_channel: u8,
    device_control_channels: BTreeMap<String, u8>,
    relative_encoder_multiplier: f32,
    midi_mappings: Arc<RwLock<BTreeMap<FullMidiIdentifier, ControllableParameter>>>,
    midi_mapping_modes: Arc<RwLock<BTreeMap<FullMidiIdentifier, MidiControlMode>>>,
    midi_learn_target: Arc<RwLock<Option<ControllableParameter>>>,
    last_midi_cc_message: Arc<RwLock<Option<(FullMidiIdentifier, Instant)>>>,
    midi_cc_values: Arc<[[AtomicU32; 128]; 16]>,
    midi_mod_matrix_learn_target: Arc<RwLock<Option<(usize, usize)>>>,
    last_learned_mod_source: Arc<RwLock<Option<MidiControlId>>>,
    should_exit: Arc<AtomicBool>,
    should_clear_all_from_midi: Arc<AtomicBool>,
    fx_presets: BTreeMap<fx::InsertionPoint, fx::FxPreset>,
    fx_wet_dry_mixes: BTreeMap<fx::InsertionPoint, Arc<AtomicU32>>,
    atmo_master_volume: Arc<AtomicU32>,
    atmo_layer_volumes: [Arc<AtomicU32>; 4],
    atmo_xy_coords: Arc<AtomicU64>,
    active_fx_target: Arc<RwLock<Option<fx::InsertionPoint>>>,
    midi_fx_editor_toggle_request: Arc<RwLock<Option<fx::InsertionPoint>>>,
    midi_atmo_editor_toggle_request: Arc<AtomicBool>,
    midi_synth_editor_toggle_request: Arc<AtomicBool>,
    midi_sampler_editor_toggle_request: Arc<AtomicBool>,
    midi_fx_preset_change_request: Arc<AtomicI8>, // New
    midi_mapping_inversions: Arc<RwLock<BTreeMap<FullMidiIdentifier, bool>>>,
) -> Result<(MidiInputConnection<()>, JoinHandle<()>)> {
    let mut midi_in = MidiInput::new(APP_NAME)?;
    midi_in.ignore(Ignore::None);

    let in_port_name = midi_in.port_name(&port)?;
    println!("Opening MIDI connection to: {}", in_port_name);

    let held_buttons = Arc::new(RwLock::new(BTreeMap::<FullMidiIdentifier, Instant>::new()));

    let held_buttons_clone = held_buttons.clone();
    let mappings_clone = midi_mappings.clone();
    let command_sender_clone = command_sender.clone();
    let timer_handle = thread::spawn(move || {
        while !should_exit.load(Ordering::Relaxed) {
            thread::sleep(HOLD_CHECK_INTERVAL);
            let mut cleared_by_hold = BTreeSet::new();
            if let Ok(held_buttons_reader) = held_buttons_clone.read() {
                for (identifier, press_time) in held_buttons_reader.iter() {
                    if press_time.elapsed() >= LONG_PRESS_DURATION {
                        if let Ok(mappings) = mappings_clone.read() {
                            if let Some(&ControllableParameter::Looper(index)) =
                                mappings.get(identifier)
                            {
                                command_sender_clone.send(AudioCommand::ClearLooper(index)).ok();
                                cleared_by_hold.insert(identifier.clone());
                            }
                        }
                    }
                }
            }
            if !cleared_by_hold.is_empty() {
                if let Ok(mut held_buttons_writer) = held_buttons_clone.write() {
                    for identifier in cleared_by_hold {
                        held_buttons_writer.remove(&identifier);
                    }
                }
            }
        }
        println!("MIDI timer thread for '{}' exited gracefully.", in_port_name);
    });

    let mut last_press_times: BTreeMap<FullMidiIdentifier, Instant> = BTreeMap::new();
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
                    let note = message[1];
                    let velocity = message[2];
                    let is_note_on = status == 0x90 && velocity > 0;

                    if channel == audio_note_channel {
                        let msg = MidiMessage {
                            status: message[0],
                            data1: note,
                            data2: velocity,
                        };
                        if let Ok(mut notes) = live_midi_notes.write() {
                            if is_note_on {
                                notes.insert(note);
                            } else {
                                notes.remove(&note);
                            }
                        }
                        command_sender.send(AudioCommand::MidiMessage(msg)).ok();
                        return;
                    }

                    if let Some(&control_channel) = device_control_channels.get(&port_name_clone) {
                        if channel == control_channel {
                            let note_id = FullMidiNoteId {
                                port_name: port_name_clone.clone(),
                                channel,
                                note,
                            };
                            let identifier = FullMidiIdentifier::Note(note_id);
                            if let Ok(mut last_msg) = last_midi_cc_message.write() {
                                *last_msg = Some((identifier.clone(), Instant::now()));
                            }

                            if let Some(param) = midi_learn_target.write().unwrap().take() {
                                let mut mappings = midi_mappings.write().unwrap();
                                mappings.retain(|_, v| *v != param);
                                mappings.insert(identifier, param);
                                println!("MIDI learn: Mapped {:?} to Note {} on channel {} from device '{}'", param, note, channel + 1, port_name_clone);
                                return;
                            }

                            if let Ok(mappings) = midi_mappings.read() {
                                if let Some(&param) = mappings.get(&identifier) {
                                    if is_note_on {
                                        let now = Instant::now();
                                        let last_press = last_press_times.entry(identifier.clone()).or_insert_with(|| now.checked_sub(DEBOUNCE_DURATION * 2).unwrap_or(now));
                                        if now.duration_since(*last_press) > DEBOUNCE_DURATION {
                                            handle_button_press(param, &command_sender, &should_clear_all_from_midi, &midi_fx_editor_toggle_request, &midi_atmo_editor_toggle_request, &midi_synth_editor_toggle_request, &midi_sampler_editor_toggle_request, &midi_fx_preset_change_request);
                                            *last_press = now;
                                        }
                                        if let ControllableParameter::Looper(_) = param {
                                            held_buttons.write().unwrap().entry(identifier).or_insert(Instant::now());
                                        }
                                    } else {
                                        if let ControllableParameter::Looper(_) = param {
                                            held_buttons.write().unwrap().remove(&identifier);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                0xB0 => {
                    let cc = message[1];
                    let value = message[2];
                    let identifier = FullMidiIdentifier::ControlChange(FullMidiControlId {
                        port_name: port_name_clone.clone(),
                        channel,
                        cc,
                    });

                    if let Some(chan_array) = midi_cc_values.get(channel as usize) {
                        if let Some(atomic_val) = chan_array.get(cc as usize) {
                            atomic_val.store(
                                (value as f32 / 127.0 * 1_000_000.0) as u32,
                                Ordering::Relaxed,
                            );
                        }
                    }
                    if let Ok(mut last_msg) = last_midi_cc_message.write() {
                        *last_msg = Some((identifier.clone(), Instant::now()));
                    }

                    if midi_mod_matrix_learn_target
                        .try_read()
                        .map_or(false, |g| g.is_some())
                    {
                        if let Ok(mut last_learned) = last_learned_mod_source.write() {
                            *last_learned = Some(MidiControlId { channel, cc });
                        }
                        return;
                    }

                    if let Some(param) = midi_learn_target.write().unwrap().take() {
                        let mut mappings = midi_mappings.write().unwrap();
                        mappings.retain(|_, v| *v != param);
                        mappings.insert(identifier, param);
                        println!(
                            "MIDI learn: Mapped {:?} to CC {} on channel {} from '{}'",
                            param,
                            cc,
                            channel + 1,
                            port_name_clone
                        );
                        return;
                    }

                    let wildcard_id = FullMidiIdentifier::ControlChange(FullMidiControlId {
                        port_name: "".to_string(),
                        channel,
                        cc,
                    });
                    let mappings = midi_mappings.read().unwrap();
                    let modes = midi_mapping_modes.read().unwrap();
                    let inversions = midi_mapping_inversions.read().unwrap();

                    if let Some(&param) =
                        mappings.get(&identifier).or_else(|| mappings.get(&wildcard_id))
                    {
                        let mode = modes
                            .get(&identifier)
                            .or_else(|| modes.get(&wildcard_id))
                            .copied()
                            .unwrap_or_default();

                        let is_inverted = inversions.get(&identifier).copied().unwrap_or(false) || inversions.get(&wildcard_id).copied().unwrap_or(false);

                        match mode {
                            MidiControlMode::Absolute => {
                                // Logic to handle button-like actions for both non-continuous params and special cases
                                if !param.is_continuous() || param == ControllableParameter::FxFocusedPresetChange {
                                    if value > 64 {
                                        let now = Instant::now();
                                        let last_press = last_press_times.entry(identifier.clone()).or_insert_with(|| now.checked_sub(DEBOUNCE_DURATION * 2).unwrap_or(now));
                                        if now.duration_since(*last_press) > DEBOUNCE_DURATION {
                                            handle_button_press(param, &command_sender, &should_clear_all_from_midi, &midi_fx_editor_toggle_request, &midi_atmo_editor_toggle_request, &midi_synth_editor_toggle_request, &midi_sampler_editor_toggle_request, &midi_fx_preset_change_request);
                                            *last_press = now;
                                        }
                                        if let ControllableParameter::Looper(_) = param {
                                            held_buttons.write().unwrap().entry(identifier).or_insert(Instant::now());
                                        }
                                    } else {
                                        if let ControllableParameter::Looper(_) = param {
                                            held_buttons.write().unwrap().remove(&identifier);
                                        }
                                    }
                                } else {
                                    // Logic for true continuous parameters
                                    let final_value = if is_inverted { 127 - value } else { value };
                                    match param {
                                        ControllableParameter::Fx(id) => {
                                            handle_fx_cc(&fx_presets, &fx_wet_dry_mixes, id, final_value);
                                        }
                                        ControllableParameter::FxFocusedWetDry => {
                                            if let Ok(target_opt) = active_fx_target.read() {
                                                if let Some(target) = *target_opt {
                                                    if let Some(atomic_param) = fx_wet_dry_mixes.get(&target) {
                                                        let scaled_val = scale_midi_to_param(FxParamName::WetDry, final_value);
                                                        atomic_param.store(scaled_val, Ordering::Relaxed);
                                                    }
                                                }
                                            }
                                        }
                                        _ => {
                                            handle_absolute_cc(param, final_value, &command_sender, &atmo_master_volume, &atmo_layer_volumes, &atmo_xy_coords);
                                        }
                                    }
                                }
                            }
                            MidiControlMode::Relative => {
                                let delta_raw = (value as i8 - 64) as f32 * RELATIVE_SENSITIVITY * relative_encoder_multiplier;
                                let delta = if is_inverted { -delta_raw } else { delta_raw };

                                if delta.abs() > 1e-6 {
                                    match param {
                                        ControllableParameter::Fx(id) => {
                                            if id.param_name == FxParamName::WetDry {
                                                if let Some(atomic_param) =
                                                    fx_wet_dry_mixes.get(&id.point)
                                                {
                                                    let current_val = atomic_param.load(Ordering::Relaxed) as f32 / 1_000_000.0;
                                                    let new_val = (current_val + delta).clamp(0.0, 1.0);
                                                    atomic_param.store((new_val * 1_000_000.0) as u32, Ordering::Relaxed);
                                                }
                                            } else if let Some(preset) = fx_presets.get(&id.point) {
                                                if let Some(link) = preset.chain.get(id.component_index) {
                                                    if let Some(atomic_param) = link.params.get_param(id.param_name.as_str()) {
                                                        let current_val = atomic_param.load(Ordering::Relaxed) as f32 / 1_000_000.0;
                                                        let new_val = (current_val + delta).clamp(0.0, 1.0);
                                                        atomic_param.store((new_val * 1_000_000.0) as u32, Ordering::Relaxed);
                                                    }
                                                }
                                            }
                                        }
                                        ControllableParameter::FxFocusedWetDry => {
                                            if let Ok(target_opt) = active_fx_target.read() {
                                                if let Some(target) = *target_opt {
                                                    if let Some(atomic_param) =
                                                        fx_wet_dry_mixes.get(&target)
                                                    {
                                                        let current_val = atomic_param.load(Ordering::Relaxed) as f32 / 1_000_000.0;
                                                        let new_val = (current_val + delta).clamp(0.0, 1.0);
                                                        atomic_param.store((new_val * 1_000_000.0) as u32, Ordering::Relaxed);
                                                    }
                                                }
                                            }
                                        }
                                        ControllableParameter::FxFocusedPresetChange => {
                                            let direction = if value > 64 { 1 } else { -1 };
                                            midi_fx_preset_change_request.store(direction, Ordering::Relaxed);
                                        }
                                        _ => {
                                            command_sender.send(AudioCommand::AdjustParameterRelative { parameter: param, delta }).ok();
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

fn handle_button_press(
    param: ControllableParameter,
    command_sender: &Sender<AudioCommand>,
    should_clear_all_from_midi: &Arc<AtomicBool>,
    midi_fx_editor_toggle_request: &Arc<RwLock<Option<fx::InsertionPoint>>>,
    midi_atmo_editor_toggle_request: &Arc<AtomicBool>,
    midi_synth_editor_toggle_request: &Arc<AtomicBool>,
    midi_sampler_editor_toggle_request: &Arc<AtomicBool>,
    midi_fx_preset_change_request: &Arc<AtomicI8>,
) {
    let mut command = None;

    match param {
        ControllableParameter::Looper(index) => command = Some(AudioCommand::LooperPress(index)),
        ControllableParameter::MixerToggleMute(index) => {
            command = Some(AudioCommand::ToggleMixerMute(index))
        }
        ControllableParameter::MixerToggleSolo(index) => {
            command = Some(AudioCommand::ToggleMixerSolo(index))
        }
        ControllableParameter::SynthToggleActive => command = Some(AudioCommand::ToggleSynth),
        ControllableParameter::SamplerToggleActive => command = Some(AudioCommand::ToggleSampler),
        ControllableParameter::InputToggleArm => command = Some(AudioCommand::ToggleAudioInputArm),
        ControllableParameter::InputToggleMonitor => {
            command = Some(AudioCommand::ToggleAudioInputMonitoring)
        }
        ControllableParameter::TransportTogglePlay => command = Some(AudioCommand::ToggleTransport),
        ControllableParameter::TransportToggleMuteAll => {
            command = Some(AudioCommand::ToggleMuteAll)
        }
        ControllableParameter::TransportToggleRecord => command = Some(AudioCommand::ToggleRecord),
        ControllableParameter::MetronomeToggleMute => {
            command = Some(AudioCommand::ToggleMetronomeMute)
        }
        ControllableParameter::TransportClearAll => {
            should_clear_all_from_midi.store(true, Ordering::Relaxed);
        }
        ControllableParameter::ToggleFxEditor(point) => {
            *midi_fx_editor_toggle_request.write().unwrap() = Some(point);
        }
        ControllableParameter::ToggleAtmoEditor => {
            midi_atmo_editor_toggle_request.store(true, Ordering::Relaxed);
        }
        ControllableParameter::ToggleSynthEditor => {
            midi_synth_editor_toggle_request.store(true, Ordering::Relaxed);
        }
        ControllableParameter::ToggleSamplerEditor => {
            midi_sampler_editor_toggle_request.store(true, Ordering::Relaxed);
        }
        ControllableParameter::FxFocusedPresetChange => {
            // This is for binary buttons (e.g., Note On/Off or CC > 64) to increment the preset.
            midi_fx_preset_change_request.store(1, Ordering::Relaxed);
        }
        _ => {}
    };

    if let Some(cmd) = command {
        command_sender.send(cmd).ok();
    }
}


fn handle_absolute_cc(
    param: ControllableParameter,
    value: u8,
    command_sender: &Sender<AudioCommand>,
    atmo_master_volume: &Arc<AtomicU32>,
    atmo_layer_volumes: &[Arc<AtomicU32>; 4],
    atmo_xy_coords: &Arc<AtomicU64>,
) {
    match param {
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
        ControllableParameter::MetronomeVolume => {
            let vol = (value as f32 / 127.0) * 1.5;
            command_sender.send(AudioCommand::SetMetronomeVolume(vol)).ok();
        }
        ControllableParameter::MetronomePitch => {
            let pitch = 220.0 + (value as f32 / 127.0) * (2000.0 - 220.0);
            command_sender.send(AudioCommand::SetMetronomePitch(pitch)).ok();
        }
        ControllableParameter::AtmoMasterVolume => {
            let vol_scaled = (value as f32 / 127.0 * 1_500_000.0) as u32;
            atmo_master_volume.store(vol_scaled, Ordering::Relaxed);
        }
        ControllableParameter::AtmoLayerVolume(index) => {
            if let Some(vol_atomic) = atmo_layer_volumes.get(index) {
                let vol_scaled = (value as f32 / 127.0 * 1_500_000.0) as u32;
                vol_atomic.store(vol_scaled, Ordering::Relaxed);
            }
        }
        ControllableParameter::AtmoXY(axis) => {
            let new_val_norm = value as f32 / 127.0;
            let new_val_u32 = (new_val_norm * u32::MAX as f32) as u32;
            let current_packed = atmo_xy_coords.load(Ordering::Relaxed);
            let new_packed = if axis == 0 {
                let y_u32 = current_packed as u32;
                (new_val_u32 as u64) << 32 | (y_u32 as u64)
            } else {
                let x_u32 = (current_packed >> 32) as u32;
                (x_u32 as u64) << 32 | (new_val_u32 as u64)
            };
            atmo_xy_coords.store(new_packed, Ordering::Relaxed);
        }
        // FX parameters are now handled outside this function
        _ => {}
    }
}

fn handle_fx_cc(
    presets: &BTreeMap<fx::InsertionPoint, fx::FxPreset>,
    wet_dry_mixes: &BTreeMap<fx::InsertionPoint, Arc<AtomicU32>>,
    id: FxParamIdentifier,
    value: u8,
) {
    if id.param_name == FxParamName::WetDry {
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

fn scale_midi_to_param(param_name: FxParamName, midi_value: u8) -> u32 {
    let val_norm = midi_value as f32 / 127.0;
    match param_name {
        FxParamName::GainDb => {
            let db = -60.0 + val_norm * (24.0 - -60.0);
            ((db + gain::DB_OFFSET) * gain::DB_SCALER) as u32
        }
        FxParamName::TimeMs => ((val_norm * 2000.0) * delay::PARAM_SCALER) as u32,
        FxParamName::Feedback => (val_norm * 0.99 * delay::PARAM_SCALER) as u32,
        FxParamName::Damping => (val_norm * delay::PARAM_SCALER) as u32,
        FxParamName::FrequencyHz => {
            let freq = 20.0 * (20000.0f32 / 20.0).powf(val_norm);
            (freq * filter::PARAM_SCALER) as u32
        }
        FxParamName::Resonance => (val_norm * filter::PARAM_SCALER) as u32,
        FxParamName::DriveDb => ((val_norm * 48.0) * waveshaper::DB_SCALER) as u32,
        FxParamName::BitDepth => {
            let bits = 1.0 + val_norm * 15.0;
            (bits * quantizer::PARAM_SCALER) as u32
        }
        FxParamName::Downsample => {
            let factor = 1.0 + val_norm * 49.0;
            (factor * quantizer::PARAM_SCALER) as u32
        }
        FxParamName::Size | FxParamName::Decay => (val_norm * reverb::PARAM_SCALER) as u32,
        FxParamName::RateHz => {
            let rate = 0.01 * (10.0f32 / 0.01).powf(val_norm);
            (rate * flanger::PARAM_SCALER) as u32
        }
        FxParamName::DepthMs => {
            let depth = 0.1 + val_norm * (10.0 - 0.1);
            (depth * flanger::PARAM_SCALER) as u32
        }
        FxParamName::Mode | FxParamName::Waveform => (val_norm * 5.0).round() as u32,
        FxParamName::AttackMs => {
            ((1.0 + val_norm * 199.0) * envelope_follower::PARAM_SCALER) as u32
        }
        FxParamName::ReleaseMs => {
            ((10.0 + val_norm * 990.0) * envelope_follower::PARAM_SCALER) as u32
        }
        FxParamName::WetDry => (val_norm * delay::PARAM_SCALER) as u32,
        FxParamName::Bypass => (val_norm > 0.5) as u32,
    }
}