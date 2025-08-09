// src/ui/midi_mapping_view.rs

use crate::app::CypherApp;
use crate::fx;
use crate::looper::NUM_LOOPERS;
use crate::settings::{ControllableParameter, FullMidiControlId, FxParamIdentifier, FxParamName};
use egui::{Button, CentralPanel, Frame, RichText, ScrollArea, TopBottomPanel, Ui, Window};
use std::collections::BTreeMap;

pub fn draw_midi_mapping_window(app: &mut CypherApp, ctx: &egui::Context) {
    let mut is_open = app.midi_mapping_window_open;
    let theme = app.theme.midi_mapping_window.clone();
    let mut should_close_by_button = false;

    Window::new("MIDI Control Setup")
        .open(&mut is_open)
        .frame(Frame::window(&ctx.style()).fill(theme.background))
        .resizable(true)
        .default_width(600.0)
        .default_height(500.0)
        .show(ctx, |ui| {
            // --- Top Panel for static info ---
            TopBottomPanel::top("midi_mapping_top_panel")
                .frame(Frame::new().inner_margin(egui::Margin::symmetric(0, 8)))
                .show_inside(ui, |ui| {
                    ui.vertical_centered(|ui| {
                        if let Ok(last_cc) = app.last_midi_cc_message.read() {
                            let text = if let Some((id, _)) = &*last_cc {
                                format!(
                                    "Last received: '{}' - Chan {} - CC {}",
                                    id.port_name,
                                    id.channel + 1,
                                    id.cc
                                )
                            } else {
                                "Move a control on your MIDI device to see it here.".to_string()
                            };
                            ui.label(RichText::new(text).color(theme.label_color));
                        }
                    });
                    ui.separator();
                });

            // --- Bottom Panel for Save/Close buttons ---
            TopBottomPanel::bottom("midi_mapping_bottom_panel")
                .frame(Frame::new().inner_margin(egui::Margin::same(8)))
                .show_inside(ui, |ui| {
                    ui.horizontal(|ui| {
                        if ui.add(Button::new("Apply & Save").fill(theme.button_bg)).clicked() {
                            // Instantly save the settings when clicked.
                            app.save_settings();
                        }
                        if ui.add(Button::new("Close").fill(theme.button_bg)).clicked() {
                            // Set a flag to close the window after this frame.
                            should_close_by_button = true;
                        }
                    });
                });

            // --- Central Panel for the scrollable content ---
            CentralPanel::default().show_inside(ui, |ui| {
                // Create a reverse mapping for efficient lookup
                let mappings = app.midi_mappings.read().unwrap();
                let reverse_lookup: BTreeMap<_, _> =
                    mappings.iter().map(|(k, v)| (*v, k.clone())).collect();
                drop(mappings);

                ScrollArea::vertical().show(ui, |ui| {
                    // --- Header Row ---
                    Frame::new().fill(theme.header_bg).show(ui, |ui| {
                        ui.columns(3, |columns| {
                            columns[0].vertical_centered(|ui| {
                                ui.label(RichText::new("Parameter").strong())
                            });
                            columns[1].vertical_centered(|ui| {
                                ui.label(RichText::new("Assigned Control").strong())
                            });
                            columns[2].vertical_centered(|ui| ui.label(RichText::new("Actions").strong()));
                        });
                    });

                    // --- Instruments Section ---
                    ui.collapsing(RichText::new("Instruments").strong().color(theme.label_color), |ui| {
                        let params = [
                            ControllableParameter::SynthToggleActive,
                            ControllableParameter::SynthMasterVolume,
                            ControllableParameter::SamplerToggleActive,
                            ControllableParameter::SamplerMasterVolume,
                        ];
                        for (i, param) in params.iter().enumerate() {
                            let row_color = if i % 2 == 0 { theme.row_even_bg } else { theme.row_odd_bg };
                            Frame::new().fill(row_color).show(ui, |ui| {
                                draw_mapping_row(ui, *param, &reverse_lookup, app);
                            });
                        }
                    });

                    // --- Audio Input Section ---
                    ui.collapsing(RichText::new("Audio Input").strong().color(theme.label_color), |ui| {
                        let params = [
                            ControllableParameter::InputToggleArm,
                            ControllableParameter::InputToggleMonitor,
                        ];
                        for (i, param) in params.iter().enumerate() {
                            let row_color = if i % 2 == 0 { theme.row_even_bg } else { theme.row_odd_bg };
                            Frame::new().fill(row_color).show(ui, |ui| {
                                draw_mapping_row(ui, *param, &reverse_lookup, app);
                            });
                        }
                    });

                    // --- Transport Section ---
                    ui.collapsing(RichText::new("Transport").strong().color(theme.label_color), |ui| {
                        let params = [
                            ControllableParameter::TransportTogglePlay,
                            ControllableParameter::TransportToggleMuteAll,
                            ControllableParameter::TransportClearAll,
                            ControllableParameter::TransportToggleRecord,
                        ];
                        for (i, param) in params.iter().enumerate() {
                            let row_color = if i % 2 == 0 { theme.row_even_bg } else { theme.row_odd_bg };
                            Frame::new().fill(row_color).show(ui, |ui| {
                                draw_mapping_row(ui, *param, &reverse_lookup, app);
                            });
                        }
                    });

                    // --- Master Section ---
                    ui.collapsing(RichText::new("Master").strong().color(theme.label_color), |ui| {
                        let params = [
                            ControllableParameter::MasterVolume,
                            ControllableParameter::LimiterThreshold,
                        ];
                        for (i, param) in params.iter().enumerate() {
                            let row_color = if i % 2 == 0 { theme.row_even_bg } else { theme.row_odd_bg };
                            Frame::new().fill(row_color).show(ui, |ui| {
                                draw_mapping_row(ui, *param, &reverse_lookup, app);
                            });
                        }
                    });

                    // --- Looper Triggers Section ---
                    ui.collapsing(RichText::new("Loopers").strong().color(theme.label_color), |ui| {
                        for i in 0..NUM_LOOPERS {
                            let param = ControllableParameter::Looper(i);
                            let row_color = if i % 2 == 0 { theme.row_even_bg } else { theme.row_odd_bg };
                            Frame::new().fill(row_color).show(ui, |ui| {
                                draw_mapping_row(ui, param, &reverse_lookup, app);
                            });
                        }
                    });

                    // --- Mixer Faders Section ---
                    ui.collapsing(RichText::new("Mixer").strong().color(theme.label_color), |ui| {
                        for i in 0..NUM_LOOPERS {
                            let param = ControllableParameter::MixerVolume(i);
                            let row_color = if i % 2 == 0 { theme.row_even_bg } else { theme.row_odd_bg };
                            Frame::new().fill(row_color).show(ui, |ui| {
                                draw_mapping_row(ui, param, &reverse_lookup, app);
                            });
                        }
                        for i in 0..NUM_LOOPERS {
                            let param = ControllableParameter::MixerToggleMute(i);
                            let row_color = if i % 2 == 0 { theme.row_even_bg } else { theme.row_odd_bg };
                            Frame::new().fill(row_color).show(ui, |ui| {
                                draw_mapping_row(ui, param, &reverse_lookup, app);
                            });
                        }
                        for i in 0..NUM_LOOPERS {
                            let param = ControllableParameter::MixerToggleSolo(i);
                            let row_color = if i % 2 == 0 { theme.row_even_bg } else { theme.row_odd_bg };
                            Frame::new().fill(row_color).show(ui, |ui| {
                                draw_mapping_row(ui, param, &reverse_lookup, app);
                            });
                        }
                    });

                    // --- FX Section ---
                    ui.collapsing(RichText::new("FX Racks").strong().color(theme.label_color), |ui| {
                        let all_insertion_points = [
                            (0..NUM_LOOPERS).map(fx::InsertionPoint::Looper).collect::<Vec<_>>(),
                            (0..2).map(fx::InsertionPoint::Synth).collect::<Vec<_>>(),
                            vec![
                                fx::InsertionPoint::Sampler,
                                fx::InsertionPoint::Input,
                                fx::InsertionPoint::Master,
                            ],
                        ].concat();

                        for (point_idx, point) in all_insertion_points.iter().enumerate() {
                            let wet_dry_param = ControllableParameter::Fx(FxParamIdentifier {
                                point: *point,
                                component_index: usize::MAX, // Special index for wet/dry
                                param_name: FxParamName::WetDry
                            });
                            let row_color = if point_idx % 2 == 0 { theme.row_even_bg } else { theme.row_odd_bg };
                            Frame::new().fill(row_color).show(ui, |ui| {
                                draw_mapping_row(ui, wet_dry_param, &reverse_lookup, app);
                            });
                        }
                    });
                });
            });
        });

    if should_close_by_button {
        app.midi_mapping_window_open = false;
    } else {
        app.midi_mapping_window_open = is_open;
    }
}

// Helper function to draw a single row in the mapping table
fn draw_mapping_row(
    ui: &mut Ui,
    param: ControllableParameter,
    reverse_lookup: &BTreeMap<ControllableParameter, FullMidiControlId>,
    app: &mut CypherApp,
) {
    let theme = &app.theme.midi_mapping_window;

    ui.columns(3, |columns| {
        // --- Column 1: Parameter Name ---
        columns[0].label(param.to_string());

        // --- Column 2: Assigned Control ---
        let assignment_text = if let Some(control_id) = reverse_lookup.get(&param) {
            let device_str = if control_id.port_name.is_empty() {
                "[Any Device]".to_string()
            } else {
                format!("'{}'", control_id.port_name)
            };
            format!("{} - Chan {} - CC {}", device_str, control_id.channel + 1, control_id.cc)
        } else {
            "Unassigned".to_string()
        };
        columns[1].label(assignment_text);

        // --- Column 3: Buttons ---
        columns[2].horizontal(|ui| {
            let is_learning_this = {
                let learn_target = app.midi_learn_target.read().unwrap();
                *learn_target == Some(param)
            };

            let learn_button_text = if is_learning_this { "Listening..." } else { "Learn" };
            let learn_button = Button::new(learn_button_text).fill(if is_learning_this {
                theme.learn_button_bg.linear_multiply(1.5)
            } else {
                theme.learn_button_bg
            });

            if ui.add(learn_button).clicked() {
                let mut learn_target = app.midi_learn_target.write().unwrap();
                *learn_target = if is_learning_this { None } else { Some(param) };
            }

            if ui.add(Button::new("Clear").fill(theme.button_bg)).clicked() {
                app.midi_mappings.write().unwrap().retain(|_, v| *v != param);
                if is_learning_this {
                    *app.midi_learn_target.write().unwrap() = None;
                }
            }
        });
    });
}