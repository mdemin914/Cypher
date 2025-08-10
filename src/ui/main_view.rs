// src/ui/main_view.rs

use crate::app::CypherApp;
use crate::audio_engine::AudioCommand;
use crate::fx;
use crate::looper::{LooperState, NUM_LOOPERS};
use crate::settings;
use crate::synth_view;
use crate::ui;
use crate::ui::about_view::draw_about_window;
use crate::ui::fx_editor_view::draw_fx_editor_window;
use crate::ui::midi_mapping_view::draw_midi_mapping_window;
use crate::ui::mixer_view::horizontal_volume_fader;
use crate::ui::slicer_view::draw_slicer_window;
use chrono::Local;
use egui::{
    epaint::{self, PathShape},
    vec2, Align2, Button, CentralPanel, Color32, CornerRadius, Frame, Id, Layout, Margin,
    ProgressBar, Rect, RichText, Sense, Shape, Stroke, TopBottomPanel, Ui, Vec2,
};
use std::f32::consts::TAU;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;

pub fn draw_main_view(app: &mut CypherApp, ctx: &egui::Context) {
    if app.options_window_open {
        ui::draw_options_window(app, ctx);
    }
    if app.sample_pad_window_open {
        ui::draw_sample_pad_window(app, ctx);
    }
    if app.synth_editor_window_open {
        synth_view::draw_synth_editor_window(app, ctx);
    }
    if app.theme_editor_window_open {
        ui::draw_theme_editor_window(app, ctx);
    }
    if app.slicer_window_open {
        draw_slicer_window(app, ctx);
    }
    if app.midi_mapping_window_open {
        draw_midi_mapping_window(app, ctx);
    }
    if app.about_window_open {
        draw_about_window(app, ctx);
    }
    if app.fx_editor_window_open {
        draw_fx_editor_window(app, ctx);
    }

    // --- Draw Notification Overlay ---
    if let Some((msg, _)) = &app.recording_notification {
        egui::Area::new(Id::new("recording_notification_area"))
            .anchor(Align2::CENTER_TOP, vec2(0.0, 50.0))
            .show(ctx, |ui| {
                let frame = Frame::popup(ui.style())
                    .fill(Color32::from_black_alpha(200))
                    .stroke(Stroke::new(1.0, Color32::WHITE));
                frame.show(ui, |ui| {
                    ui.label(RichText::new(msg).color(Color32::WHITE).size(16.0));
                });
            });
    }

    TopBottomPanel::top("options_bar")
        .frame(Frame::new().fill(app.theme.top_bar.background))
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                let button = Button::new("Options").fill(app.theme.top_bar.button_bg);
                if ui.add(button).clicked() {
                    app.options_window_open = true;
                }
                let button = Button::new("Theme").fill(app.theme.top_bar.button_bg);
                if ui.add(button).clicked() {
                    app.theme_editor_window_open = true;
                }
                let button = Button::new("Slicer").fill(app.theme.top_bar.button_bg);
                if ui.add(button).clicked() {
                    app.slicer_window_open = true;
                }

                ui.separator();

                let save_button = Button::new("Save").fill(app.theme.top_bar.session_button_bg);
                if ui.add(save_button).clicked() {
                    app.save_session(app.current_session_path.clone());
                }

                let save_as_button =
                    Button::new("Save As...").fill(app.theme.top_bar.session_save_as_button_bg);
                if ui.add(save_as_button).clicked() {
                    app.save_session(None);
                }

                ui.separator();

                let len = app.transport_len_samples.load(Ordering::Relaxed);
                let sr = app.active_sample_rate;

                let bpm_text = if len > 0 && sr > 0 {
                    let bpm = (sr as f64 * 60.0 * 4.0) / len as f64;
                    format!("BPM: {:.1}", bpm)
                } else {
                    "BPM: ---".to_string()
                };
                ui.label(
                    RichText::new(bpm_text)
                        .monospace()
                        .color(app.theme.top_bar.text_color),
                );

                ui.separator();

                ui.label(
                    RichText::new("Transport:")
                        .monospace()
                        .color(app.theme.top_bar.text_color),
                );

                let playhead = app.transport_playhead.load(Ordering::Relaxed);
                let progress = if len > 0 {
                    playhead as f32 / len as f32
                } else {
                    0.0
                };

                let progress_bar = ProgressBar::new(progress)
                    .show_percentage()
                    .desired_width(200.0)
                    .fill(app.theme.top_bar.transport_bar_fill);
                ui.add(progress_bar);

                ui.separator();

                let cpu_load_val = app.cpu_load.load(Ordering::Relaxed);
                let cpu_load_percent = cpu_load_val as f32 / 10.0;
                let cpu_text = RichText::new(format!("CPU: {:>5.1}%", cpu_load_percent))
                    .monospace()
                    .color(app.theme.top_bar.text_color);
                ui.label(cpu_text);

                let xruns = app.xrun_count.load(Ordering::Relaxed);
                let mut xrun_text = RichText::new(format!("Xruns: {}", xruns)).monospace();
                if xruns > 0 {
                    xrun_text = xrun_text.color(app.theme.top_bar.xrun_text_color);
                } else {
                    xrun_text = xrun_text.color(app.theme.top_bar.text_color);
                }
                ui.label(xrun_text);
            });
        });

    TopBottomPanel::bottom("library_panel")
        .resizable(true)
        .default_height(200.0)
        .min_height(50.0)
        .frame(Frame::new().fill(app.theme.library.panel_background))
        .show(ctx, |ui| {
            ui::draw_library_panel(app, ui);
        });

    TopBottomPanel::bottom("mixer_panel")
        .resizable(true)
        .default_height(220.0)
        .min_height(100.0)
        .frame(Frame::new().fill(app.theme.mixer.panel_background))
        .show(ctx, |ui| {
            ui::draw_mixer_panel(app, ui);
        });

    CentralPanel::default()
        .frame(Frame::new().fill(app.theme.main_background))
        .show(ctx, |ui| {
            let top_section_height = 120.0;
            ui.allocate_ui(vec2(ui.available_width(), top_section_height), |ui| {
                ui.columns(4, |cols| {
                    draw_synth_panel(app, &mut cols[0]);
                    draw_sampler_panel(app, &mut cols[1]);
                    draw_audio_input_panel(app, &mut cols[2]);
                    draw_transport_panel(app, &mut cols[3]);
                });
            });
            ui.separator();
            draw_looper_grid(app, ui);
        });
}

fn draw_looper_grid(app: &mut CypherApp, ui: &mut Ui) {
    ui.with_layout(Layout::left_to_right(egui::Align::TOP).with_main_wrap(true), |ui| {
        let num_cols = 6;
        let spacing = ui.style().spacing.item_spacing;
        let available_width = ui.available_width();
        let available_height = ui.available_height();

        let looper_width =
            ((available_width - (spacing.x * (num_cols - 1) as f32)) / num_cols as f32).floor();
        let looper_height = ((available_height - spacing.y) / 2.0).floor();

        if looper_width <= 0.0 || looper_height <= 0.0 {
            return;
        }
        let looper_size = vec2(looper_width, looper_height);

        for id in 0..NUM_LOOPERS {
            let state = app.looper_states[id].get();
            let length_in_cycles = app.looper_states[id].get_length_in_cycles();
            let looper_playhead = app.looper_states[id].get_playhead();
            let transport_len = app.transport_len_samples.load(Ordering::Relaxed) as f32;

            let progress = if length_in_cycles > 0 && transport_len > 0.0 {
                let total_samples = length_in_cycles as f32 * transport_len;
                (looper_playhead as f32 % total_samples) / total_samples
            } else {
                app.transport_playhead.load(Ordering::Relaxed) as f32 / transport_len.max(1.0)
            };

            let waveform_summary = app.looper_states[id].get_waveform_summary();
            let (main_response, clear_response) = draw_looper_button(
                ui,
                app,
                id,
                state,
                progress,
                looper_size,
                &app.theme,
                waveform_summary,
            );

            let main_button_id = main_response.id;
            if main_response.is_pointer_button_down_on() {
                let was_already_pressed = ui.memory_mut(|m| {
                    let already_pressed = m.data.get_temp_mut_or_default::<bool>(main_button_id);
                    if *already_pressed {
                        true
                    } else {
                        *already_pressed = true;
                        false
                    }
                });

                if !was_already_pressed {
                    // Logic is now unified in the audio engine
                    app.send_command(AudioCommand::LooperPress(id));
                }
            } else {
                ui.memory_mut(|m| m.data.insert_temp(main_button_id, false));
            }

            if let Some(clear_resp) = clear_response {
                let clear_button_id = clear_resp.id;
                if clear_resp.is_pointer_button_down_on() {
                    let was_already_pressed = ui.memory_mut(|m| {
                        let already_pressed =
                            m.data.get_temp_mut_or_default::<bool>(clear_button_id);
                        if *already_pressed {
                            true
                        } else {
                            *already_pressed = true;
                            false
                        }
                    });
                    if !was_already_pressed {
                        app.send_command(AudioCommand::ClearLooper(id));
                    }
                } else {
                    ui.memory_mut(|m| m.data.insert_temp(clear_button_id, false));
                }
            }
        }
    });
}

fn draw_looper_button(
    ui: &mut Ui,
    app: &CypherApp,
    id: usize,
    state: LooperState,
    progress: f32,
    size: Vec2,
    theme: &crate::theme::Theme,
    waveform_summary: Arc<std::sync::RwLock<Vec<f32>>>,
) -> (egui::Response, Option<egui::Response>) {
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let mut clear_response = None;

    if ui.is_rect_visible(rect) {
        let center = rect.center();
        let base_radius = rect.height().min(rect.width()) * 0.45;

        let bg_color = if state == LooperState::Armed {
            theme.loopers.armed_bg
        } else {
            theme.loopers.empty_bg
        };

        let waveform_color = match state {
            LooperState::Recording => theme.loopers.recording_bg,
            LooperState::Playing => theme.loopers.track_colors[id],
            LooperState::Overdubbing => theme.loopers.overdubbing_bg,
            _ => Color32::BLACK,
        };

        let stroke = Stroke::new(1.0, theme.loopers.track_colors[id]);
        ui.painter().rect(
            rect,
            CornerRadius::ZERO,
            bg_color,
            stroke,
            epaint::StrokeKind::Inside,
        );

        let waveform = waveform_summary.read().unwrap();
        if !waveform.is_empty() {
            let inner_radius = base_radius * 0.2;
            let outer_radius = base_radius;
            let num_points = waveform.len();

            for (i, peak) in waveform.iter().enumerate() {
                let angle = (i as f32 / num_points as f32) * TAU - TAU / 4.0;
                let start_point = center + vec2(angle.cos(), angle.sin()) * outer_radius;
                let end_point = center
                    + vec2(angle.cos(), angle.sin())
                    * (outer_radius - peak * (outer_radius - inner_radius));
                ui.painter().line_segment(
                    [start_point, end_point],
                    Stroke::new(1.0, waveform_color),
                );
            }
        }

        if state != LooperState::Empty {
            if state != LooperState::Armed {
                ui.painter().add(Shape::circle_stroke(
                    center,
                    base_radius,
                    Stroke::new(4.0, theme.loopers.progress_bar_bg),
                ));
                let start_angle = -TAU / 4.0;
                let end_angle = start_angle + progress * TAU;
                let progress_color = theme.loopers.track_colors[id];
                let points: Vec<_> = (0..=100)
                    .map(|i| {
                        let angle = start_angle + (end_angle - start_angle) * (i as f32 / 100.0);
                        center + vec2(angle.cos(), angle.sin()) * base_radius
                    })
                    .collect();
                ui.painter().add(Shape::Path(PathShape {
                    points,
                    closed: false,
                    fill: Color32::TRANSPARENT,
                    stroke: Stroke::new(4.0, progress_color).into(),
                }));
            }

            let button_size = vec2(80.0, 30.0);
            let clear_button_rect = Rect::from_min_size(
                rect.min + vec2(4.0, rect.height() - button_size.y - 4.0),
                button_size,
            );
            let resp = ui.interact(clear_button_rect, Id::new(("clear", id)), Sense::click());
            let clear_visuals = ui.style().interact(&resp);
            ui.painter().rect(
                clear_button_rect,
                clear_visuals.corner_radius,
                theme.loopers.clear_button_bg,
                clear_visuals.bg_stroke,
                epaint::StrokeKind::Inside,
            );
            ui.painter().text(
                clear_button_rect.center(),
                Align2::CENTER_CENTER,
                "Clear",
                egui::FontId::monospace(14.0),
                theme.loopers.text_color,
            );
            clear_response = Some(resp);
        }

        // Draw armed indicator (circle in top right corner)
        let indicator_radius = 8.0;
        let indicator_pos = egui::pos2(rect.max.x - indicator_radius - 4.0, rect.min.y + indicator_radius + 4.0);
        
        let stop_is_queued = app.looper_states[id].get_stop_is_queued();
        let pending_command = app.looper_states[id].get_pending_command();
        let should_show_red = stop_is_queued || (pending_command && state == LooperState::Recording);
        
        if should_show_red {
            ui.painter().add(Shape::circle_filled(
                indicator_pos,
                indicator_radius,
                Color32::RED,
            ));
        }

        let id_color = theme.loopers.track_colors[id];
        let id_galley = ui.painter().layout_no_wrap(
            format!("Looper {}", id + 1),
            egui::FontId::monospace(14.0),
            id_color,
        );
        let id_pos = center - id_galley.size() / 2.0;
        ui.painter().galley(id_pos, id_galley, id_color);
    }
    (response, clear_response)
}

fn draw_synth_panel(app: &mut CypherApp, ui: &mut Ui) {
    let frame = Frame::new()
        .fill(app.theme.instrument_panel.panel_background)
        .inner_margin(Margin::from(10.0));
    frame.show(ui, |ui| {
        ui.with_layout(
            egui::Layout::top_down(egui::Align::Center).with_cross_justify(true),
            |ui| {
                ui.label(
                    RichText::new("Synth")
                        .monospace()
                        .color(app.theme.instrument_panel.label_color),
                );
                ui.add_space(4.0);

                ui.horizontal(|ui| {
                    let spacing = ui.style().spacing.item_spacing.x;
                    let button_width = ((ui.available_width() - (spacing * 2.0)) / 3.0).max(0.0);
                    let button_size = vec2(button_width, 30.0);

                    let editor_button =
                        Button::new("Editor").fill(app.theme.instrument_panel.button_bg);
                    if ui.add_sized(button_size, editor_button).clicked() {
                        app.synth_editor_window_open = true;
                        app.sample_pad_window_open = false;
                        app.send_command(AudioCommand::ActivateSynth);
                        app.send_command(AudioCommand::DeactivateSampler);
                    }

                    let fx_button = Button::new("FX").fill(app.theme.instrument_panel.button_bg);
                    if ui.add_sized(button_size, fx_button).clicked() {
                        app.active_fx_target = Some(fx::InsertionPoint::Synth(0));
                        app.fx_editor_window_open = true;
                    }

                    let is_active = app.synth_is_active.load(Ordering::Relaxed);
                    let button_text = if is_active { "ACTIVE" } else { "INACTIVE" };
                    let button_color = if is_active {
                        app.theme.instrument_panel.button_active_bg
                    } else {
                        app.theme.instrument_panel.button_bg
                    };
                    let active_button = Button::new(button_text).fill(button_color);
                    if ui.add_sized(button_size, active_button).clicked() {
                        if is_active {
                            app.send_command(AudioCommand::DeactivateSynth);
                        } else {
                            app.send_command(AudioCommand::ActivateSynth);
                            app.send_command(AudioCommand::DeactivateSampler);
                            app.synth_editor_window_open = false;
                            app.sample_pad_window_open = false;
                        }
                    }
                });

                ui.add_space(4.0);
                let mut vol_f32 =
                    app.synth_master_volume.load(Ordering::Relaxed) as f32 / 1_000_000.0;
                if horizontal_volume_fader(
                    ui,
                    "synth_master_vol_fader",
                    &mut vol_f32,
                    app.displayed_synth_master_peak_level,
                    app.theme.instrument_panel.fader_track_bg,
                    &app.theme,
                )
                    .dragged()
                {
                    app.synth_master_volume
                        .store((vol_f32 * 1_000_000.0) as u32, Ordering::Relaxed);
                }
            },
        );
    });
}

fn draw_sampler_panel(app: &mut CypherApp, ui: &mut Ui) {
    let frame = Frame::new()
        .fill(app.theme.instrument_panel.panel_background)
        .inner_margin(Margin::from(10.0));
    frame.show(ui, |ui| {
        ui.with_layout(
            egui::Layout::top_down(egui::Align::Center).with_cross_justify(true),
            |ui| {
                ui.label(
                    RichText::new("Sampler")
                        .monospace()
                        .color(app.theme.instrument_panel.label_color),
                );
                ui.add_space(4.0);

                ui.horizontal(|ui| {
                    let spacing = ui.style().spacing.item_spacing.x;
                    let button_width = ((ui.available_width() - (spacing * 2.0)) / 3.0).max(0.0);
                    let button_size = vec2(button_width, 30.0);

                    let pads_button = Button::new("Pads").fill(app.theme.instrument_panel.button_bg);
                    if ui.add_sized(button_size, pads_button).clicked() {
                        app.sample_pad_window_open = true;
                        app.synth_editor_window_open = false;
                        app.send_command(AudioCommand::ActivateSampler);
                        app.send_command(AudioCommand::DeactivateSynth);
                    }

                    let fx_button = Button::new("FX").fill(app.theme.instrument_panel.button_bg);
                    if ui.add_sized(button_size, fx_button).clicked() {
                        app.active_fx_target = Some(fx::InsertionPoint::Sampler);
                        app.fx_editor_window_open = true;
                    }

                    let is_active = app.sampler_is_active.load(Ordering::Relaxed);
                    let button_text = if is_active { "ACTIVE" } else { "INACTIVE" };
                    let button_color = if is_active {
                        app.theme.instrument_panel.button_active_bg
                    } else {
                        app.theme.instrument_panel.button_bg
                    };
                    let active_button = Button::new(button_text).fill(button_color);
                    if ui.add_sized(button_size, active_button).clicked() {
                        if is_active {
                            app.send_command(AudioCommand::DeactivateSampler);
                        } else {
                            app.send_command(AudioCommand::ActivateSampler);
                            app.send_command(AudioCommand::DeactivateSynth);
                            app.sample_pad_window_open = false;
                            app.synth_editor_window_open = false;
                        }
                    }
                });

                ui.add_space(4.0);
                let mut vol_f32 = app.sampler_volume.load(Ordering::Relaxed) as f32 / 1_000_000.0;
                if horizontal_volume_fader(
                    ui,
                    "sampler_vol_fader",
                    &mut vol_f32,
                    app.displayed_sampler_peak_level,
                    app.theme.instrument_panel.fader_track_bg,
                    &app.theme,
                )
                    .dragged()
                {
                    app.sampler_volume
                        .store((vol_f32 * 1_000_000.0) as u32, Ordering::Relaxed);
                }
            },
        );
    });
}

fn draw_audio_input_panel(app: &mut CypherApp, ui: &mut Ui) {
    let frame = Frame::new()
        .fill(app.theme.instrument_panel.panel_background)
        .inner_margin(Margin::from(10.0));
    frame.show(ui, |ui| {
        ui.with_layout(Layout::top_down(egui::Align::Center), |ui| {
            ui.label(
                RichText::new("Audio Input")
                    .monospace()
                    .color(app.theme.instrument_panel.label_color),
            );
            ui.add_space(4.0);

            let peak = app.displayed_input_peak_level;
            let bar = ProgressBar::new(peak)
                .show_percentage()
                .desired_width(ui.available_width() - 20.0);
            ui.add(bar);
            ui.add_space(4.0);

            ui.horizontal(|ui| {
                let spacing = ui.style().spacing.item_spacing.x;
                let button_width = ((ui.available_width() - (spacing * 2.0)) / 3.0).max(0.0);
                let button_size = vec2(button_width, 30.0);

                let fx_button = Button::new("FX").fill(app.theme.instrument_panel.button_bg);
                if ui.add_sized(button_size, fx_button).clicked() {
                    app.active_fx_target = Some(fx::InsertionPoint::Input);
                    app.fx_editor_window_open = true;
                }

                let is_armed = app.audio_input_is_armed.load(Ordering::Relaxed);
                let arm_button = Button::new(RichText::new("ARM").monospace()).fill(if is_armed {
                    app.theme.instrument_panel.input_armed_bg
                } else {
                    app.theme.instrument_panel.button_bg
                });
                if ui.add_sized(button_size, arm_button).clicked() {
                    app.send_command(AudioCommand::ToggleAudioInputArm);
                }

                let is_monitored = app.audio_input_is_monitored.load(Ordering::Relaxed);
                let monitor_button =
                    Button::new(RichText::new("MON").monospace()).fill(if is_monitored {
                        app.theme.instrument_panel.input_monitor_bg
                    } else {
                        app.theme.instrument_panel.button_bg
                    });
                if ui.add_sized(button_size, monitor_button).clicked() {
                    app.send_command(AudioCommand::ToggleAudioInputMonitoring);
                }
            });
        });
    });
}

fn draw_transport_panel(app: &mut CypherApp, ui: &mut Ui) {
    let frame = Frame::new()
        .fill(app.theme.transport_controls.panel_background)
        .inner_margin(Margin::from(10.0));

    frame.show(ui, |ui| {
        // Use a layout that centers its children horizontally.
        ui.with_layout(Layout::top_down(egui::Align::Center), |ui| {
            ui.label(
                RichText::new("Playback")
                    .monospace()
                    .color(app.theme.transport_controls.label_color),
            );
            ui.add_space(8.0);

            // Use a horizontal layout for the buttons themselves
            ui.horizontal(|ui| {
                let button_size = vec2(80.0, 40.0);

                // --- Play/Stop Button ---
                let is_playing = app.transport_is_playing.load(Ordering::Relaxed);
                let play_text = if is_playing { "◼ STOP" } else { "▶ PLAY" };
                let play_color = if is_playing {
                    app.theme.transport_controls.play_active_bg
                } else {
                    app.theme.transport_controls.button_bg
                };

                let play_button = Button::new(RichText::new(play_text).monospace()).fill(play_color);
                if ui.add_sized(button_size, play_button).clicked() {
                    if is_playing {
                        app.send_command(AudioCommand::StopTransport);
                    } else {
                        app.send_command(AudioCommand::PlayTransport);
                    }
                }

                // --- Mute/Unmute All Button ---
                let is_muted = app.is_all_muted();
                let mute_text = if is_muted { "UNMUTE" } else { "MUTE ALL" };
                let mute_color = if is_muted {
                    app.theme.transport_controls.mute_active_bg
                } else {
                    app.theme.transport_controls.button_bg
                };

                let mute_button =
                    Button::new(RichText::new(mute_text).monospace()).fill(mute_color);
                if ui.add_sized(button_size, mute_button).clicked() {
                    app.toggle_mute_all();
                }

                // --- Clear All Button ---
                let clear_button = Button::new(RichText::new("CLEAR\nALL").monospace())
                    .fill(app.theme.transport_controls.clear_button_bg);
                if ui.add_sized(button_size, clear_button).clicked() {
                    app.clear_all_fx_racks();
                    app.send_command(AudioCommand::ClearAllAndPlay);
                }

                // --- Record Button ---
                let record_text = if app.is_recording_output { "■ REC" } else { "● REC" };
                let record_color = if app.is_recording_output {
                    app.theme.transport_controls.record_active_bg
                } else {
                    app.theme.transport_controls.record_button_bg
                };

                let record_button =
                    Button::new(RichText::new(record_text).monospace()).fill(record_color);
                if ui.add_sized(button_size, record_button).clicked() {
                    app.is_recording_output = !app.is_recording_output;
                    if app.is_recording_output {
                        app.send_command(AudioCommand::StartOutputRecording);
                    } else {
                        if let Some(config_dir) = settings::get_config_dir() {
                            let rec_dir = config_dir.join("LiveRecordings");
                            let timestamp = Local::now().format("%Y-%m-%d_%H-%M-%S").to_string();
                            let filename = format!("LiveRec_{}.wav", timestamp);
                            let path = rec_dir.join(filename);
                            app.send_command(AudioCommand::StopOutputRecording {
                                output_path: path.clone(),
                            });
                            app.recording_notification =
                                Some((format!("Saved to {}", path.display()), Instant::now()));
                        }
                    }
                }
            });
        });
    });
}