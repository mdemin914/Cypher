// src/ui/options_view.rs
use crate::app::CypherApp;
use cpal::traits::{DeviceTrait};
use egui::{Button, ComboBox, DragValue, Frame, Grid, RichText, Slider, Window};
use std::sync::atomic::Ordering;

pub fn draw_options_window(app: &mut CypherApp, ctx: &egui::Context) {
    let mut midi_port_changed = false;
    let mut save_and_close = false;
    let mut apply_was_clicked = false;
    let mut host_changed = false;
    let mut close_options_and_open_about = false; // <-- NEW FLAG

    Window::new("Options")
        .open(&mut app.options_window_open)
        .frame(Frame::window(&ctx.style()).fill(app.theme.options_window.background))
        .resizable(false)
        .default_width(450.0)
        .show(ctx, |ui| {
            let style = ui.style_mut();
            style.visuals.widgets.inactive.bg_fill = app.theme.options_window.widget_bg;
            style.visuals.widgets.hovered.bg_fill = app.theme.options_window.widget_bg.linear_multiply(1.2);
            style.visuals.widgets.active.bg_fill = app.theme.options_window.slider_grab_color;
            style.visuals.widgets.noninteractive.bg_fill = app.theme.options_window.widget_bg;


            ui.heading(RichText::new("MIDI Settings").color(app.theme.options_window.heading_color));
            ui.add_space(10.0);

            Grid::new("midi_settings_grid")
                .num_columns(2)
                .spacing([120.0, 8.0])
                .show(ui, |ui| {
                    let mut current_midi_port_index = app.selected_midi_port_index;
                    let selected_port_name = if let Some(index) = current_midi_port_index {
                        app.midi_ports.get(index).map_or("Invalid Port", |p| &p.0)
                    } else {
                        "No MIDI devices found"
                    };

                    ComboBox::new("midi_port_combo", "")
                        .selected_text(selected_port_name)
                        .show_ui(ui, |ui| {
                            for (i, (name, _)) in app.midi_ports.iter().enumerate() {
                                if ui.selectable_label(current_midi_port_index == Some(i), name).clicked() {
                                    current_midi_port_index = Some(i);
                                }
                            }
                        });

                    if current_midi_port_index != app.selected_midi_port_index {
                        app.selected_midi_port_index = current_midi_port_index;
                        midi_port_changed = true;
                    }
                    ui.label(RichText::new("MIDI Input Device").color(app.theme.options_window.label_color));
                    ui.end_row();

                    let mut channel = app.selected_midi_channel.load(Ordering::Relaxed) + 1;
                    ui.horizontal(|ui| {
                        ui.add(Slider::new(&mut channel, 1..=16).show_value(false));
                        ui.add(DragValue::new(&mut channel).range(1..=16).speed(0.1));
                    });
                    app.selected_midi_channel.store(channel - 1, Ordering::Relaxed);
                    ui.label(RichText::new("MIDI Channel").color(app.theme.options_window.label_color));
                    ui.end_row();
                });

            ui.add_space(4.0);
            if ui.add(Button::new("MIDI Control Setup").fill(app.theme.options_window.widget_bg)).clicked() {
                app.midi_mapping_window_open = true;
            }

            ui.separator();
            ui.heading(RichText::new("Audio Settings").color(app.theme.options_window.heading_color));
            ui.label(RichText::new("Applying new audio settings will reset the current session.").color(app.theme.options_window.label_color));
            ui.add_space(10.0);

            Grid::new("audio_settings_grid")
                .num_columns(2)
                .spacing([120.0, 8.0])
                .show(ui, |ui| {
                    let selected_host_name = app.available_hosts[app.selected_host_index].name();
                    ComboBox::new("host_combo", "").selected_text(selected_host_name)
                        .show_ui(ui, |ui| {
                            for (i, host_id) in app.available_hosts.iter().enumerate() {
                                if ui.selectable_label(app.selected_host_index == i, host_id.name()).clicked() {
                                    if app.selected_host_index != i {
                                        app.selected_host_index = i;
                                        host_changed = true;
                                    }
                                }
                            }
                        });
                    ui.label(RichText::new("Audio Host").color(app.theme.options_window.label_color));
                    ui.end_row();

                    let selected_input_name = app.selected_input_device_index.and_then(|i| app.input_devices.get(i)).map(|(s, _)| s.clone());
                    ComboBox::new("input_device_combo", "")
                        .selected_text(selected_input_name.as_deref().unwrap_or("Select a device"))
                        .show_ui(ui, |ui| {
                            for (i, (name, _)) in app.input_devices.iter().enumerate() {
                                if ui.selectable_label(app.selected_input_device_index == Some(i), name).clicked() {
                                    app.selected_input_device_index = Some(i);
                                }
                            }
                        });
                    ui.label(RichText::new("Input Device").color(app.theme.options_window.label_color));
                    ui.end_row();

                    let selected_output_name = app.selected_output_device_index.and_then(|i| app.output_devices.get(i)).map(|(s, _)| s.clone());
                    ComboBox::new("output_device_combo", "")
                        .selected_text(selected_output_name.as_deref().unwrap_or("Select a device"))
                        .show_ui(ui, |ui| {
                            for (i, (name, _)) in app.output_devices.iter().enumerate() {
                                if ui.selectable_label(app.selected_output_device_index == Some(i), name).clicked() {
                                    app.selected_output_device_index = Some(i);
                                }
                            }
                        });
                    ui.label(RichText::new("Output Device").color(app.theme.options_window.label_color));
                    ui.end_row();

                    let mut supported_rates = app.sample_rates.clone();
                    if let Some(device) = app.selected_output_device_index.and_then(|i| app.output_devices.get(i)).map(|(_,d)|d) {
                        if let Ok(configs) = device.supported_output_configs() {
                            let mut rates = Vec::new();
                            for config_range in configs {
                                for &rate in &app.sample_rates {
                                    if rate >= config_range.min_sample_rate().0 && rate <= config_range.max_sample_rate().0 {
                                        if !rates.contains(&rate) { rates.push(rate); }
                                    }
                                }
                            }
                            rates.sort_unstable();
                            if !rates.is_empty() { supported_rates = rates; }
                        }
                    }
                    if app.sample_rates.get(app.selected_sample_rate_index).is_some() {
                        let current_sr = app.sample_rates[app.selected_sample_rate_index];
                        if !supported_rates.contains(&current_sr) {
                            if !supported_rates.is_empty() {
                                if let Some(pos) = app.sample_rates.iter().position(|r| *r == supported_rates[0]) {
                                    app.selected_sample_rate_index = pos;
                                }
                            }
                        }
                    }
                    let selected_sr_text = if let Some(sr) = app.sample_rates.get(app.selected_sample_rate_index) { sr.to_string() } else { "N/A".to_string() };
                    ComboBox::new("sample_rate_combo", "").selected_text(selected_sr_text)
                        .show_ui(ui, |ui| {
                            for rate in supported_rates {
                                if let Some(pos) = app.sample_rates.iter().position(|r| *r == rate) {
                                    if ui.selectable_label(app.selected_sample_rate_index == pos, rate.to_string()).clicked() {
                                        app.selected_sample_rate_index = pos;
                                    }
                                }
                            }
                        });
                    ui.label(RichText::new(format!("Sample Rate (Active: {})", app.active_sample_rate)).color(app.theme.options_window.label_color));
                    ui.end_row();

                    let selected_bs_text = if let Some(bs) = app.buffer_sizes.get(app.selected_buffer_size_index) { bs.to_string() } else { "N/A".to_string() };
                    ComboBox::new("buffer_size_combo", "").selected_text(selected_bs_text)
                        .show_ui(ui, |ui| {
                            for (i, size) in app.buffer_sizes.iter().enumerate() {
                                if ui.selectable_label(app.selected_buffer_size_index == i, size.to_string()).clicked() {
                                    app.selected_buffer_size_index = i;
                                }
                            }
                        });
                    ui.label(RichText::new(format!("Buffer Size (Active: {})", app.active_buffer_size)).color(app.theme.options_window.label_color));
                    ui.end_row();

                    let mut comp_f32 = app.input_latency_compensation_ms.load(Ordering::Relaxed) as f32 / 100.0;
                    let slider = Slider::new(&mut comp_f32, 0.0..=50.0).suffix(" ms");
                    if ui.add(slider).on_hover_text("Adds a small safety buffer to the audio input to prevent crackling. Higher values are more stable but increase latency.").changed() {
                        app.input_latency_compensation_ms.store((comp_f32 * 100.0).round() as u32, Ordering::Relaxed);
                    }
                    ui.label(RichText::new("Input Safety Buffer").color(app.theme.options_window.label_color));
                    ui.end_row();

                    let is_active = app.settings.bpm_rounding;
                    let button_color = if is_active { app.theme.options_window.bpm_rounding_on_bg } else { app.theme.options_window.widget_bg };
                    let button = Button::new("BPM Rounding").fill(button_color);
                    if ui.add(button).clicked() {
                        app.settings.bpm_rounding = !is_active;
                        app.bpm_rounding_setting_changed_unapplied = true;
                    };
                    if app.bpm_rounding_setting_changed_unapplied {
                        ui.colored_label(egui::Color32::YELLOW, "Restart audio engine to apply.");
                    } else {
                        ui.label("");
                    }
                    ui.end_row();

                    let selected_input_name_check = app.selected_input_device_index.and_then(|i| app.input_devices.get(i)).map(|(s, _)| s.clone());
                    let selected_output_name_check = app.selected_output_device_index.and_then(|i| app.output_devices.get(i)).map(|(s, _)| s.clone());
                    let audio_settings_have_changed = selected_input_name_check != app.active_input_device_name
                        || selected_output_name_check != app.active_output_device_name
                        || app.sample_rates[app.selected_sample_rate_index] != app.active_sample_rate
                        || app.buffer_sizes[app.selected_buffer_size_index] != app.active_buffer_size;

                    let apply_button = Button::new("Apply").fill(app.theme.options_window.widget_bg);
                    if ui.add_enabled(audio_settings_have_changed || app.bpm_rounding_setting_changed_unapplied, apply_button).clicked() {
                        apply_was_clicked = true;
                    }
                    if let Some((msg, color)) = &app.audio_settings_status {
                        ui.colored_label(*color, msg);
                    } else { ui.label(""); }
                    ui.end_row();
                });

            ui.add_space(20.0);
            ui.separator();
            ui.add_space(10.0);

            ui.horizontal(|ui| {
                if ui.add(Button::new("About Cypher").fill(app.theme.options_window.widget_bg)).clicked() {
                    // This now sets our local flag instead of borrowing `app` mutably.
                    close_options_and_open_about = true;
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let save_button = Button::new("Save and Close").fill(app.theme.options_window.widget_bg);
                    if ui.add(save_button).clicked() {
                        save_and_close = true;
                    }
                });
            });
        });

    // --- Actions are now performed AFTER the window's mutable borrow is released ---

    if close_options_and_open_about {
        app.about_window_open = true;
        app.options_window_open = false;
    }

    if host_changed {
        app.on_host_changed();
    }
    if apply_was_clicked {
        app.apply_audio_settings();
    }
    if midi_port_changed {
        if let Err(e) = app.reconnect_midi() {
            eprintln!("Failed to reconnect MIDI: {}", e);
        }
    }
    if save_and_close {
        app.save_settings();
        app.options_window_open = false;
    }
}