// src/ui/atmo_view.rs

use crate::app::CypherApp;
use crate::asset::Asset;
use crate::atmo;
use crate::atmo::AtmoPreset;
use crate::audio_engine::AudioCommand;
use crate::settings;
use egui::{
    epaint::StrokeKind, vec2, Align2, Color32, ComboBox, CornerRadius, DragValue, Frame, Grid, Id,
    RichText, Sense, Stroke, Ui, Window,
};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use walkdir::WalkDir;

pub fn draw_atmo_window(app: &mut CypherApp, ctx: &egui::Context) {
    let mut is_open = app.atmo_window_open;
    let theme = app.theme.synth_editor_window.clone();

    Window::new("Atmosphere Engine")
        .open(&mut is_open)
        .frame(Frame::window(&ctx.style()).fill(theme.background))
        .default_size([800.0, 600.0])
        .resizable(false)
        .collapsible(false)
        .pivot(Align2::CENTER_CENTER)
        .default_pos(ctx.screen_rect().center())
        .show(ctx, |ui| {
            let active_scene_id = Id::new("active_atmo_scene");
            let mut active_scene_index =
                ui.memory_mut(|m| *m.data.get_temp_mut_or_default::<usize>(active_scene_id));

            // --- Top Toolbar for Presets ---
            draw_atmo_toolbar(ui, app);
            ui.separator();

            ui.columns(2, |columns| {
                // --- Left Column: X/Y Pad and Scene Selectors ---
                columns[0].vertical(|ui| {
                    let available_width = ui.available_width();
                    let pad_size = vec2(available_width, available_width);
                    draw_xy_pad(ui, app, ctx, pad_size);

                    ui.separator();
                    draw_scene_selectors(ui, app, &mut active_scene_index);
                });

                // --- Right Column: Layer Controls ---
                columns[1].vertical(|ui| {
                    ui.heading(format!(
                        "Editing Scene: {}",
                        app.atmo.scenes[active_scene_index].name
                    ));
                    ui.separator();
                    draw_layer_controls(ui, app, active_scene_index, ctx);
                });
            });

            ui.memory_mut(|m| m.data.insert_temp(active_scene_id, active_scene_index));
        });

    app.atmo_window_open = is_open;
}

fn draw_atmo_toolbar(ui: &mut Ui, app: &mut CypherApp) {
    let mut preset_to_load_path: Option<PathBuf> = None;
    let mut save_preset_as = false;

    ui.horizontal(|ui| {
        let preset_name = &app.atmo.name;
        ComboBox::from_id_salt("atmo_preset_load_combo")
            .selected_text(preset_name)
            .show_ui(ui, |ui| {
                for (name, path) in &app.available_atmo_presets {
                    if ui.selectable_label(preset_name == name, name).clicked() {
                        preset_to_load_path = Some(path.clone());
                    }
                }
            });

        if ui.button("Save Preset As...").clicked() {
            save_preset_as = true;
        }
        if ui.button("Clear Preset").clicked() {
            app.atmo = AtmoPreset::default();
            // We need to clear all layers on the audio thread too
            for scene_index in 0..4 {
                for layer_index in 0..4 {
                    app.send_command(AudioCommand::ClearAtmoLayer { scene_index, layer_index });
                }
            }
        }
    });

    if let Some(path) = preset_to_load_path {
        app.load_atmo_preset_from_path(&path);
    }
    if save_preset_as {
        app.save_atmo_preset();
    }
}

fn draw_xy_pad(ui: &mut Ui, app: &mut CypherApp, _ctx: &egui::Context, desired_size: egui::Vec2) {
    ui.label("Performance Pad");
    let (response, painter) = ui.allocate_painter(desired_size, Sense::drag());
    let rect = response.rect;

    painter.rect_filled(
        rect,
        CornerRadius::ZERO,
        app.theme.synth_editor_window.visualizer_bg,
    );

    // Draw the mix radius circle
    let mix_radius = rect.width() * 0.5;
    painter.circle_stroke(
        rect.center(),
        mix_radius,
        Stroke::new(1.0, Color32::from_white_alpha(30)),
    );

    // Read current coords for drawing the puck
    let packed_coords = app.atmo_xy_coords.load(Ordering::Relaxed);
    let x_u32 = (packed_coords >> 32) as u32;
    let y_u32 = packed_coords as u32;
    let norm_x = x_u32 as f32 / u32::MAX as f32;
    let norm_y = y_u32 as f32 / u32::MAX as f32;
    let puck_pos = rect.lerp_inside(vec2(norm_x, norm_y));

    // Draw scene labels
    for (i, scene) in app.atmo.scenes.iter().enumerate() {
        let (align, offset) = match i {
            0 => (Align2::LEFT_TOP, vec2(5.0, 5.0)),
            1 => (Align2::RIGHT_TOP, vec2(-5.0, 5.0)),
            2 => (Align2::LEFT_BOTTOM, vec2(5.0, -5.0)),
            _ => (Align2::RIGHT_BOTTOM, vec2(-5.0, -5.0)),
        };
        painter.text(
            align.pos_in_rect(&rect) + offset,
            align,
            &scene.name,
            egui::FontId::proportional(14.0),
            app.theme.synth_editor_window.label_color,
        );
    }

    // Draw puck and lines
    painter.vline(puck_pos.x, rect.y_range(), Stroke::new(1.0, Color32::GRAY));
    painter.hline(rect.x_range(), puck_pos.y, Stroke::new(1.0, Color32::GRAY));
    painter.circle_filled(
        puck_pos,
        8.0,
        app.theme.synth_editor_window.slider_grab_color,
    );
    painter.circle_stroke(puck_pos, 8.0, Stroke::new(1.0, Color32::WHITE));

    // Handle interaction
    if response.dragged() {
        if let Some(pos) = response.interact_pointer_pos() {
            let new_norm_x = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
            let new_norm_y = ((pos.y - rect.top()) / rect.height()).clamp(0.0, 1.0);

            let new_x_u32 = (new_norm_x * u32::MAX as f32) as u32;
            let new_y_u32 = (new_norm_y * u32::MAX as f32) as u32;

            let new_packed = (new_x_u32 as u64) << 32 | (new_y_u32 as u64);
            app.atmo_xy_coords.store(new_packed, Ordering::Relaxed);
        }
    }
}

fn draw_scene_selectors(ui: &mut Ui, app: &mut CypherApp, active_scene_index: &mut usize) {
    ui.add_space(10.0);
    ui.label("Scenes");
    let mut scene_changed = false;
    for i in 0..4 {
        ui.horizontal(|ui| {
            if ui
                .selectable_value(active_scene_index, i, format!("Scene {}", i + 1))
                .changed()
            {
                // We don't need to set a flag here, just selecting is a UI-only change
            }
            if ui
                .add(egui::TextEdit::singleline(&mut app.atmo.scenes[i].name))
                .changed()
            {
                scene_changed = true;
            }
        });
    }
    // If a name changed, only update the single scene on the audio thread
    if scene_changed {
        app.send_command(AudioCommand::SetAtmoScene {
            scene_index: *active_scene_index,
            scene: app.atmo.scenes[*active_scene_index].clone(),
        });
    }
}

fn draw_layer_controls(
    ui: &mut Ui,
    app: &mut CypherApp,
    active_scene_index: usize,
    ctx: &egui::Context,
) {
    let mut param_changed = false;
    let mut samples_to_load: Option<(usize, PathBuf)> = None;

    for i in 0..4 {
        let response = Frame::group(ui.style())
            .fill(app.theme.synth_editor_window.section_bg)
            .show(ui, |ui| {
                let layer = &mut app.atmo.scenes[active_scene_index].layers[i];
                ui.label(format!("Layer {}", i + 1));

                let path_text = if let Some(path) = &layer.sample_folder_path {
                    path.display().to_string()
                } else {
                    "Drop sample folder here".to_string()
                };
                ui.label(RichText::new(path_text).small());
                ui.separator();

                // --- Mode Toggle ---
                ui.horizontal(|ui| {
                    if ui.selectable_value(&mut layer.params.mode, atmo::PlaybackMode::FragmentLooping, "Fragment Looping").changed() {
                        param_changed = true;
                    }
                    if ui.selectable_value(&mut layer.params.mode, atmo::PlaybackMode::TriggeredEvents, "Triggered Events").changed() {
                        param_changed = true;
                    }
                });
                ui.add_space(4.0);

                ui.scope(|ui| {
                    let visuals = ui.visuals_mut();
                    visuals.widgets.inactive.bg_fill = app.theme.synth_editor_window.slider_track_color;
                    visuals.widgets.hovered.bg_fill = app.theme.synth_editor_window.slider_grab_hover_color;
                    visuals.widgets.active.bg_fill = app.theme.synth_editor_window.slider_grab_color;

                    Grid::new(format!("atmo_layer_grid_{}", i))
                        .num_columns(3) // 3 COLUMNS
                        .spacing([10.0, 4.0])
                        .show(ui, |ui| {
                            ui.label("Volume");
                            param_changed |= ui.add(egui::Slider::new(&mut layer.params.volume, 0.0..=1.5).show_value(false)).changed();
                            param_changed |= ui.add(DragValue::new(&mut layer.params.volume).speed(0.01).fixed_decimals(2)).changed();
                            ui.end_row();

                            ui.label("Playback Rate");
                            param_changed |= ui.add(egui::Slider::new(&mut layer.params.playback_rate, 0.25..=4.0).show_value(false)).changed();
                            param_changed |= ui.add(DragValue::new(&mut layer.params.playback_rate).speed(0.01).fixed_decimals(2)).changed();
                            ui.end_row();

                            // --- Conditional Controls ---
                            match layer.params.mode {
                                atmo::PlaybackMode::FragmentLooping => {
                                    ui.label("Fragment Length");
                                    if ui.add(egui::Slider::new(&mut layer.params.fragment_length, 0.01..=1.0).show_value(false))
                                        .on_hover_text("Length of the looping audio segment as a percentage of the total file length.")
                                        .changed() {
                                        param_changed = true;
                                    }
                                    let mut length_percent = layer.params.fragment_length * 100.0;
                                    if ui.add(DragValue::new(&mut length_percent).speed(0.1).range(1.0..=100.0).suffix("%")).changed() {
                                        layer.params.fragment_length = length_percent / 100.0;
                                        param_changed = true;
                                    }
                                }
                                atmo::PlaybackMode::TriggeredEvents => {
                                    ui.label("Density / Overlap");
                                    if ui.add(egui::Slider::new(&mut layer.params.density, -1.0..=1.0).show_value(false))
                                        .on_hover_text("Timing between triggered events. < 0 adds a gap, > 0 creates an overlap.")
                                        .changed() {
                                        param_changed = true;
                                    }
                                    let mut density_percent = layer.params.density * 100.0;
                                    if ui.add(DragValue::new(&mut density_percent).speed(0.1).range(-100.0..=100.0).suffix("%")).changed() {
                                        layer.params.density = density_percent / 100.0;
                                        param_changed = true;
                                    }
                                }
                            }
                            ui.end_row();

                            ui.label("Pan Randomness");
                            param_changed |= ui.add(egui::Slider::new(&mut layer.params.pan_randomness, 0.0..=1.0).show_value(false)).changed();
                            param_changed |= ui.add(DragValue::new(&mut layer.params.pan_randomness).speed(0.01).fixed_decimals(2)).changed();
                            ui.end_row();

                            ui.label("Filter Cutoff");
                            param_changed |= ui.add(egui::Slider::new(&mut layer.params.filter_cutoff, 0.0..=1.0).show_value(false)).changed();
                            param_changed |= ui.add(DragValue::new(&mut layer.params.filter_cutoff).speed(0.01).fixed_decimals(3)).changed();
                            ui.end_row();
                        });
                });
            })
            .response;

        // --- Handle Drag and Drop from both OS and internal Library ---
        let is_hovered = ui.rect_contains_pointer(response.rect);
        if is_hovered {
            let is_os_file_hover = ctx.input(|i| !i.raw.hovered_files.is_empty());
            let is_internal_drag = egui::DragAndDrop::has_any_payload(ctx);

            if is_os_file_hover || is_internal_drag {
                ui.painter().rect_stroke(
                    response.rect,
                    CornerRadius::ZERO,
                    ui.style().visuals.selection.stroke,
                    StrokeKind::Inside,
                );
            }
        }

        if ui.input(|i| i.pointer.any_released()) && is_hovered {
            let mut dropped_path: Option<PathBuf> = None;

            if let Some(asset_payload) = egui::DragAndDrop::take_payload::<Asset>(ctx) {
                if let Asset::Folder(folder_ref) = (*asset_payload).clone() {
                    dropped_path = Some(folder_ref.path);
                }
            } else if !ctx.input(|i| i.raw.dropped_files.is_empty()) {
                if let Some(file) = ctx.input(|i| i.raw.dropped_files[0].path.clone()) {
                    dropped_path = Some(if file.is_dir() {
                        file
                    } else {
                        file.parent().unwrap_or(&file).to_path_buf()
                    });
                }
            }
            if let Some(path) = dropped_path {
                samples_to_load = Some((i, path));
            }
        }
    }

    if let Some((layer_index, path)) = samples_to_load {
        // Scan for WAV files and their lengths. This is the fix.
        let samples: Vec<(PathBuf, u32)> = WalkDir::new(&path)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_type().is_file()
                    && e.path().extension().map_or(false, |ext| ext == "wav")
            })
            .filter_map(|e| {
                let path = e.path().to_path_buf();
                hound::WavReader::open(&path)
                    .ok()
                    .map(|reader| (path, reader.duration()))
            })
            .collect();

        if !samples.is_empty() {
            app.send_command(AudioCommand::ClearAtmoLayer {
                scene_index: active_scene_index,
                layer_index,
            });

            let relative_path = if let Some(config_dir) = settings::get_config_dir() {
                path.strip_prefix(config_dir)
                    .unwrap_or(&path)
                    .to_path_buf()
            } else {
                path.clone()
            };

            if layer_index == 0 {
                if let Some(folder_name) = path.file_name().and_then(|s| s.to_str()) {
                    app.atmo.scenes[active_scene_index].name = folder_name.to_string();
                }
            }

            app.atmo.scenes[active_scene_index].layers[layer_index].sample_folder_path =
                Some(relative_path);

            // Send the crucial command with the sample list to the audio engine.
            app.send_command(AudioCommand::LoadAtmoLayer {
                scene_index: active_scene_index,
                layer_index,
                samples,
            });
            param_changed = true;
        }
    }

    if param_changed {
        app.send_command(AudioCommand::SetAtmoScene {
            scene_index: active_scene_index,
            scene: app.atmo.scenes[active_scene_index].clone(),
        });
    }
}