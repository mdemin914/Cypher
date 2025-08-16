// src/ui/fx_editor_view.rs

//! The generic UI for the modular FX system.
//! It reads an FxPreset and dynamically builds the controls for the component chain.

use crate::app::CypherApp;
use crate::audio_engine::AudioCommand;
use crate::fx::{FxChainLink, FxComponentType, FxPreset, ModulationRoutingData};
use crate::fx_components::*;
use crate::settings;
use egui::{
    Align2, Button, ComboBox, Frame, Grid, RichText, ScrollArea, Slider, Ui, Window,
};
use rfd::FileDialog;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::Ordering;

pub fn draw_fx_editor_window(app: &mut CypherApp, ctx: &egui::Context) {
    let mut is_open = app.fx_editor_window_open;
    let theme = app.theme.synth_editor_window.clone();

    // Correctly read the active_fx_target for the title
    let title = if let Some(target) = *app.active_fx_target.read().unwrap() {
        format!("FX Chain Editor - {}", target)
    } else {
        "FX Chain Editor".to_string()
    };

    Window::new(title)
        .open(&mut is_open)
        .frame(Frame::window(&ctx.style()).fill(theme.background))
        .default_size([400.0, 500.0])
        .pivot(Align2::CENTER_CENTER)
        .default_pos(ctx.screen_rect().center())
        .show(ctx, |ui| {
            // Correctly read the active_fx_target for the main logic
            let target = if let Some(t) = *app.active_fx_target.read().unwrap() {
                t
            } else {
                ui.label("No FX Chain selected.");
                return;
            };

            let mut component_to_remove = None;
            let mut component_to_move: Option<(usize, i8)> = None; // (index, direction: -1 up, 1 down)
            let mut new_component_type: Option<FxComponentType> = None;
            let mut clear_chain_clicked = false;
            let mut preset_to_load_path: Option<PathBuf> = None;
            let mut save_preset_as = false;

            let mut any_mod_ui_changed = false;

            // --- Top Bar ---
            ui.horizontal(|ui| {
                let preset_name = app.fx_presets.get(&target).map_or("Load Preset...", |p| &p.name);
                ComboBox::from_id_salt("fx_preset_load_combo")
                    .selected_text(preset_name)
                    .show_ui(ui, |ui| {
                        for (name, path) in &app.available_fx_presets {
                            if ui.selectable_label(preset_name == name, name).clicked() {
                                preset_to_load_path = Some(path.clone());
                            }
                        }
                    });

                if ui.button("Save Preset As...").clicked() {
                    save_preset_as = true;
                }
                if ui.button("Clear Chain").clicked() {
                    clear_chain_clicked = true;
                }
            });
            ui.separator();

            // --- Master Controls ---
            ui.horizontal(|ui| {
                if let Some(wet_dry_mix_atomic) = app.fx_wet_dry_mixes.get(&target) {
                    let mut wet_dry_mix_f32 = wet_dry_mix_atomic.load(Ordering::Relaxed) as f32 / delay::PARAM_SCALER;
                    ui.label("Dry/Wet");
                    if ui.add(Slider::new(&mut wet_dry_mix_f32, 0.0..=1.0)).changed() {
                        wet_dry_mix_atomic.store((wet_dry_mix_f32 * delay::PARAM_SCALER) as u32, Ordering::Relaxed);
                    }
                } else {
                    ui.add_enabled(false, Slider::new(&mut 0.0, 0.0..=1.0).text("Dry/Wet"));
                }
                ui.separator();

                ComboBox::from_id_salt("add_component_combo")
                    .selected_text("Add Component...")
                    .show_ui(ui, |ui| {
                        let all_types = [
                            FxComponentType::Gain, FxComponentType::Delay, FxComponentType::Filter,
                            FxComponentType::Lfo, FxComponentType::EnvelopeFollower,
                            FxComponentType::Waveshaper, FxComponentType::Quantizer,
                            FxComponentType::Reverb, FxComponentType::Flanger,
                            FxComponentType::Formant,
                        ];
                        for comp_type in all_types {
                            if ui.selectable_label(false, format!("{:?}", comp_type)).clicked() {
                                new_component_type = Some(comp_type);
                            }
                        }
                    });
            });
            ui.separator();

            // --- Main Component Chain Area ---
            ScrollArea::vertical().show(ui, |ui| {
                if let Some(preset) = app.fx_presets.get_mut(&target) {
                    let chain_len = preset.chain.len();
                    let chain_clone_for_mods = preset.chain.clone(); // Clone for modulation target list

                    for i in 0..chain_len {
                        Frame::group(ui.style()).fill(theme.section_bg).show(ui, |ui| {
                            let link = &mut preset.chain[i];
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    if ui.add_enabled(i > 0, Button::new("Up")).clicked() {
                                        component_to_move = Some((i, -1));
                                    }
                                    if ui.add_enabled(i < chain_len - 1, Button::new("Down")).clicked() {
                                        component_to_move = Some((i, 1));
                                    }
                                });

                                ui.label(RichText::new(format!("{:?}", link.component_type)).strong());
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if ui.add(Button::new("❌").frame(false)).clicked() {
                                        component_to_remove = Some(i);
                                    }
                                    let mut bypassed = link.params.bypassed().load(Ordering::Relaxed);
                                    if ui.toggle_value(&mut bypassed, "Bypass").changed() {
                                        link.params.bypassed().store(bypassed, Ordering::Relaxed);
                                    }
                                });
                            });
                            ui.separator();

                            if draw_component_ui(ui, link, i, &chain_clone_for_mods) {
                                any_mod_ui_changed = true;
                            }
                        });
                    }
                }
            });

            // --- Apply pending actions after all UI drawing and mutable borrows are done ---
            let mut structure_changed = false;
            if let Some(path) = preset_to_load_path {
                if let Ok(json_string) = fs::read_to_string(path) {
                    if let Ok(loaded_preset) = serde_json::from_str::<FxPreset>(&json_string) {
                        app.fx_presets.insert(target, loaded_preset);
                        structure_changed = true;
                    }
                }
            }

            if let Some(comp_type) = new_component_type {
                app.fx_presets.entry(target).or_default().chain.push(FxChainLink::new(comp_type));
                structure_changed = true;
            }

            if let Some(index) = component_to_remove {
                if let Some(preset) = app.fx_presets.get_mut(&target) {
                    if index < preset.chain.len() {
                        preset.chain.remove(index);
                        preset.chain.iter_mut().for_each(|link| {
                            link.modulations.retain(|m| m.source_component_index != index && m.target_component_index != index);
                            for m in &mut link.modulations {
                                if m.source_component_index > index { m.source_component_index -= 1; }
                                if m.target_component_index > index { m.target_component_index -= 1; }
                            }
                        });
                        structure_changed = true;
                    }
                }
            }
            if let Some((index, direction)) = component_to_move {
                if let Some(preset) = app.fx_presets.get_mut(&target) {
                    let new_index = (index as isize + direction as isize) as usize;
                    preset.chain.swap(index, new_index);
                    for link in preset.chain.iter_mut() {
                        for modulation in &mut link.modulations {
                            if modulation.source_component_index == index { modulation.source_component_index = new_index; }
                            else if modulation.source_component_index == new_index { modulation.source_component_index = index; }
                            if modulation.target_component_index == index { modulation.target_component_index = new_index; }
                            else if modulation.target_component_index == new_index { modulation.target_component_index = index; }
                        }
                    }
                    structure_changed = true;
                }
            }
            if save_preset_as {
                if let Some(preset) = app.fx_presets.get_mut(&target) {
                    if let Some(path) = FileDialog::new()
                        .add_filter("json", &["json"])
                        .set_directory(settings::get_config_dir().unwrap_or_default().join("FX"))
                        .save_file()
                    {
                        // Update the preset's internal name before saving.
                        if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                            preset.name = name.to_string();
                        }

                        if let Ok(json_string) = serde_json::to_string_pretty(preset) {
                            if fs::write(path, json_string).is_ok() {
                                app.rescan_fx_presets();
                            }
                        }
                    }
                }
            }
            if clear_chain_clicked {
                app.fx_presets.remove(&target);
                app.send_command(AudioCommand::ClearFxRack(target));
            }

            if any_mod_ui_changed {
                structure_changed = true;
            }

            if structure_changed {
                if let Some(preset) = app.fx_presets.get(&target) {
                    app.send_command(AudioCommand::LoadFxRack(target, preset.clone()));
                }
            }
        });

    if !is_open {
        // If the window was closed, clear the active target
        *app.active_fx_target.write().unwrap() = None;
    }
    app.fx_editor_window_open = is_open;
}

/// Dynamically draws the UI for a single FxChainLink.
fn draw_component_ui(ui: &mut Ui, link: &mut FxChainLink, index: usize, chain: &[FxChainLink]) -> bool {
    let mut modulation_was_changed = false;

    let grid_id = format!("component_grid_{}", index);
    Grid::new(grid_id).show(ui, |ui| match &link.params {
        ComponentParams::Gain(p) => {
            ui.label("Gain (dB)");
            let mut gain_db = (p.gain_db.load(Ordering::Relaxed) as f32 / gain::DB_SCALER) - gain::DB_OFFSET;
            if ui.add(Slider::new(&mut gain_db, -60.0..=24.0)).changed() {
                p.gain_db.store(((gain_db + gain::DB_OFFSET) * gain::DB_SCALER) as u32, Ordering::Relaxed);
            }
            ui.end_row();
        }
        ComponentParams::Delay(p) => {
            ui.label("Time (ms)");
            let mut time_ms = p.time_ms.load(Ordering::Relaxed) as f32 / delay::PARAM_SCALER;
            if ui.add(Slider::new(&mut time_ms, 0.0..=2000.0)).changed() {
                p.time_ms.store((time_ms * delay::PARAM_SCALER) as u32, Ordering::Relaxed);
            }
            ui.end_row();

            ui.label("Feedback");
            let mut feedback = p.feedback.load(Ordering::Relaxed) as f32 / delay::PARAM_SCALER;
            if ui.add(Slider::new(&mut feedback, 0.0..=0.99)).changed() {
                p.feedback.store((feedback * delay::PARAM_SCALER) as u32, Ordering::Relaxed);
            }
            ui.end_row();

            ui.label("Damping");
            let mut damping = p.damping.load(Ordering::Relaxed) as f32 / delay::PARAM_SCALER;
            if ui.add(Slider::new(&mut damping, 0.0..=1.0)).changed() {
                p.damping.store((damping * delay::PARAM_SCALER) as u32, Ordering::Relaxed);
            }
            ui.end_row();
        }
        ComponentParams::Filter(p) => {
            ui.label("Mode");
            let mut mode = filter::FilterMode::from(p.mode.load(Ordering::Relaxed));
            let initial_mode = mode;
            ComboBox::from_id_salt(format!("filter_mode_combo_{}", index))
                .selected_text(format!("{:?}", mode))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut mode, filter::FilterMode::LowPass, "LowPass");
                    ui.selectable_value(&mut mode, filter::FilterMode::HighPass, "HighPass");
                    ui.selectable_value(&mut mode, filter::FilterMode::BandPass, "BandPass");
                });
            if initial_mode != mode {
                p.mode.store(mode as u32, Ordering::Relaxed);
            }
            ui.end_row();

            ui.label("Frequency (Hz)");
            let mut freq = p.frequency_hz.load(Ordering::Relaxed) as f32 / filter::PARAM_SCALER;
            if ui.add(Slider::new(&mut freq, 20.0..=20000.0).logarithmic(true)).changed() {
                p.frequency_hz.store((freq * filter::PARAM_SCALER) as u32, Ordering::Relaxed);
            }
            ui.end_row();

            ui.label("Resonance");
            let mut res = p.resonance.load(Ordering::Relaxed) as f32 / filter::PARAM_SCALER;
            if ui.add(Slider::new(&mut res, 0.0..=1.0)).changed() {
                p.resonance.store((res * filter::PARAM_SCALER) as u32, Ordering::Relaxed);
            }
            ui.end_row();
        }
        ComponentParams::Waveshaper(p) => {
            ui.label("Drive (Pre-Gain dB)");
            let mut drive = p.drive_db.load(Ordering::Relaxed) as f32 / waveshaper::DB_SCALER;
            if ui.add(Slider::new(&mut drive, 0.0..=48.0)).changed() {
                p.drive_db.store((drive * waveshaper::DB_SCALER) as u32, Ordering::Relaxed);
            }
            ui.end_row();
        }
        ComponentParams::Reverb(p) => {
            ui.label("Size");
            let mut size = p.size.load(Ordering::Relaxed) as f32 / reverb::PARAM_SCALER;
            if ui.add(Slider::new(&mut size, 0.0..=1.0)).changed() {
                p.size.store((size * reverb::PARAM_SCALER) as u32, Ordering::Relaxed);
            }
            ui.end_row();

            ui.label("Decay");
            let mut decay = p.decay.load(Ordering::Relaxed) as f32 / reverb::PARAM_SCALER;
            if ui.add(Slider::new(&mut decay, 0.0..=1.0)).changed() {
                p.decay.store((decay * reverb::PARAM_SCALER) as u32, Ordering::Relaxed);
            }
            ui.end_row();

            ui.label("Damping");
            let mut damping = p.damping.load(Ordering::Relaxed) as f32 / reverb::PARAM_SCALER;
            if ui.add(Slider::new(&mut damping, 0.0..=1.0)).changed() {
                p.damping.store((damping * reverb::PARAM_SCALER) as u32, Ordering::Relaxed);
            }
            ui.end_row();
        }
        ComponentParams::Lfo(p) => {
            ui.label("Rate (Hz)");
            let mut freq = p.frequency_hz.load(Ordering::Relaxed) as f32 / lfo::PARAM_SCALER;
            if ui.add(Slider::new(&mut freq, 0.01..=20.0).logarithmic(true)).changed() {
                p.frequency_hz.store((freq * lfo::PARAM_SCALER) as u32, Ordering::Relaxed);
            }
            ui.end_row();
        }
        ComponentParams::Flanger(p) => {
            ui.label("Rate (Hz)");
            let mut rate = p.rate_hz.load(Ordering::Relaxed) as f32 / flanger::PARAM_SCALER;
            if ui.add(Slider::new(&mut rate, 0.01..=10.0).logarithmic(true)).changed() {
                p.rate_hz.store((rate * flanger::PARAM_SCALER) as u32, Ordering::Relaxed);
            }
            ui.end_row();

            ui.label("Depth (ms)");
            let mut depth = p.depth_ms.load(Ordering::Relaxed) as f32 / flanger::PARAM_SCALER;
            if ui.add(Slider::new(&mut depth, 0.1..=10.0)).changed() {
                p.depth_ms.store((depth * flanger::PARAM_SCALER) as u32, Ordering::Relaxed);
            }
            ui.end_row();

            ui.label("Feedback");
            let mut feedback = (p.feedback.load(Ordering::Relaxed) as f32 / flanger::PARAM_SCALER) - flanger::FEEDBACK_OFFSET;
            if ui.add(Slider::new(&mut feedback, -0.99..=0.99)).changed() {
                p.feedback.store(((feedback + flanger::FEEDBACK_OFFSET) * flanger::PARAM_SCALER) as u32, Ordering::Relaxed);
            }
            ui.end_row();
        }
        ComponentParams::EnvelopeFollower(p) => {
            ui.label("Attack (ms)");
            let mut attack = p.attack_ms.load(Ordering::Relaxed) as f32 / envelope_follower::PARAM_SCALER;
            if ui.add(Slider::new(&mut attack, 1.0..=200.0)).changed() {
                p.attack_ms.store((attack * envelope_follower::PARAM_SCALER) as u32, Ordering::Relaxed);
            }
            ui.end_row();

            ui.label("Release (ms)");
            let mut release = p.release_ms.load(Ordering::Relaxed) as f32 / envelope_follower::PARAM_SCALER;
            if ui.add(Slider::new(&mut release, 10.0..=1000.0)).changed() {
                p.release_ms.store((release * envelope_follower::PARAM_SCALER) as u32, Ordering::Relaxed);
            }
            ui.end_row();

            ui.label("Sensitivity");
            let mut sensitivity = p.sensitivity.load(Ordering::Relaxed) as f32 / envelope_follower::PARAM_SCALER;
            if ui.add(Slider::new(&mut sensitivity, 0.1..=100.0).logarithmic(true)).changed() {
                p.sensitivity.store((sensitivity * envelope_follower::PARAM_SCALER) as u32, Ordering::Relaxed);
            }
            ui.end_row();

        }
        ComponentParams::Quantizer(p) => {
            ui.label("Bit Depth");
            let mut bits = p.bit_depth.load(Ordering::Relaxed) as f32 / quantizer::PARAM_SCALER;
            if ui.add(Slider::new(&mut bits, 1.0..=16.0)).changed() {
                p.bit_depth.store((bits * quantizer::PARAM_SCALER) as u32, Ordering::Relaxed);
            }
            ui.end_row();

            ui.label("Downsample");
            let mut downsample = p.downsample.load(Ordering::Relaxed) as f32 / quantizer::PARAM_SCALER;
            if ui.add(Slider::new(&mut downsample, 1.0..=50.0)).changed() {
                p.downsample.store((downsample * quantizer::PARAM_SCALER) as u32, Ordering::Relaxed);
            }
            ui.end_row();
        }
        ComponentParams::Formant(p) => {
            ui.label("Character");
            let mut character = (p.character.load(Ordering::Relaxed) as f32 / formant::PARAM_SCALER) - formant::CHARACTER_OFFSET;
            if ui.add(Slider::new(&mut character, -1.0..=1.0)).changed() {
                p.character.store(((character + formant::CHARACTER_OFFSET) * formant::PARAM_SCALER) as u32, Ordering::Relaxed);
            }
            ui.end_row();

            ui.label("Resonance");
            let mut resonance = p.resonance.load(Ordering::Relaxed) as f32 / formant::PARAM_SCALER;
            if ui.add(Slider::new(&mut resonance, 0.0..=1.0)).changed() {
                p.resonance.store((resonance * formant::PARAM_SCALER) as u32, Ordering::Relaxed);
            }
            ui.end_row();
        }
    });

    let is_modulator = matches!(link.component_type, FxComponentType::Lfo | FxComponentType::EnvelopeFollower);
    if is_modulator {
        egui::collapsing_header::CollapsingHeader::new("Modulations")
            .id_salt(format!("mod_header_{}", index))
            .show(ui, |ui| {
                let mut mod_to_remove = None;
                for (mod_idx, modulation) in link.modulations.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label("->");

                        let selected_target_text = if modulation.target_component_index < chain.len() {
                            format!("Slot {}: {:?}", modulation.target_component_index + 1, chain[modulation.target_component_index].component_type)
                        } else {
                            "Invalid Target".to_string()
                        };

                        if ComboBox::from_id_salt(format!("mod_target_slot_{}_{}", index, mod_idx))
                            .selected_text(selected_target_text)
                            .show_ui(ui, |ui| {
                                for (target_idx, target_link) in chain.iter().enumerate() {
                                    if target_idx == index { continue; }
                                    let target_text = format!("Slot {}: {:?}", target_idx + 1, target_link.component_type);
                                    if ui.selectable_label(modulation.target_component_index == target_idx, &target_text).clicked() {
                                        modulation.target_component_index = target_idx;
                                        modulation.target_parameter_name.clear();
                                        modulation_was_changed = true;
                                    }
                                }
                            }).response.changed() {
                            // This block is intentionally empty. The logic is inside the show_ui call.
                        };

                        let available_params = get_available_params(chain.get(modulation.target_component_index).map(|l| l.component_type));

                        if !available_params.contains(&modulation.target_parameter_name.as_str()) {
                            modulation.target_parameter_name = available_params.get(0).unwrap_or(&"").to_string();
                        }

                        let selected_param_text = modulation.target_parameter_name.clone();
                        if ComboBox::from_id_salt(format!("mod_target_param_{}_{}", index, mod_idx))
                            .selected_text(&selected_param_text)
                            .show_ui(ui, |ui| {
                                for param in available_params {
                                    if ui.selectable_value(&mut modulation.target_parameter_name, param.to_string(), param).changed() {
                                        modulation_was_changed = true;
                                    }
                                }
                            }).response.changed() {
                            // This block is intentionally empty. The logic is inside the show_ui call.
                        };

                        let (min, max) = get_mod_amount_range(&modulation.target_parameter_name);
                        if ui.add(Slider::new(&mut modulation.amount, min..=max)).changed() {
                            modulation_was_changed = true;
                        }
                        if ui.button("x").clicked() {
                            mod_to_remove = Some(mod_idx);
                        }
                    });
                }
                if let Some(idx) = mod_to_remove {
                    link.modulations.remove(idx);
                    modulation_was_changed = true;
                }

                if ui.button("+ Add Modulation Target").clicked() {
                    let first_valid_target = (0..chain.len()).find(|&i| i != index);
                    if let Some(target_idx) = first_valid_target {
                        link.modulations.push(ModulationRoutingData {
                            source_component_index: index,
                            target_component_index: target_idx,
                            ..Default::default()
                        });
                    }
                    modulation_was_changed = true;
                }
            });
    }
    modulation_was_changed
}

/// Helper to get a list of modulatable parameters for a given component type.
fn get_available_params(comp_type: Option<FxComponentType>) -> Vec<&'static str> {
    match comp_type {
        Some(FxComponentType::Gain) => vec!["gain_db"],
        Some(FxComponentType::Delay) => vec!["time_ms", "feedback", "damping"],
        Some(FxComponentType::Filter) => vec!["frequency_hz", "resonance"],
        Some(FxComponentType::Waveshaper) => vec!["drive_db"],
        Some(FxComponentType::Quantizer) => vec!["bit_depth", "downsample"],
        Some(FxComponentType::Reverb) => vec!["size", "decay", "damping"],
        Some(FxComponentType::Flanger) => vec!["rate_hz", "depth_ms", "feedback"],
        Some(FxComponentType::EnvelopeFollower) => vec!["attack_ms", "release_ms"],
        Some(FxComponentType::Formant) => vec!["character", "resonance"],
        _ => vec![],
    }
}

/// Helper to get a sensible slider range for a given modulation target parameter.
fn get_mod_amount_range(param_name: &str) -> (f32, f32) {
    match param_name {
        "frequency_hz" => (-10000.0, 10000.0),
        "time_ms" | "depth_ms" => (-50.0, 50.0),
        "feedback" | "resonance" | "damping" | "size" | "decay" | "character" => (-1.0, 1.0),
        "semitones" => (-24.0, 24.0),
        "cents" => (-100.0, 100.0),
        "gain_db" => (-24.0, 24.0),
        "drive_db" => (0.0, 48.0),
        "bit_depth" => (-15.0, 15.0),
        "downsample" => (0.0, 50.0),
        "attack_ms" | "release_ms" => (-500.0, 500.0),
        _ => (-1.0, 1.0),
    }
}