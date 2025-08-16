// src/ui/midi_mapping_view.rs

use crate::app::CypherApp;
use crate::fx;
use crate::looper::NUM_LOOPERS;
use crate::settings::{
    ControllableParameter, FullMidiIdentifier, FxParamIdentifier, FxParamName, MidiControlMode,
};
use egui::{Button, CentralPanel, Checkbox, Frame, RichText, ScrollArea, TopBottomPanel, Ui, Window};
use std::collections::BTreeMap;

// Helper to convert MIDI note number to name (e.g., 60 -> C4)
fn note_to_name(note: u8) -> String {
    const NOTES: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    let octave = (note / 12) as i8 - 1;
    let note_name = NOTES[(note % 12) as usize];
    format!("{} ({}{})", note, note_name, octave)
}

pub fn draw_midi_mapping_window(app: &mut CypherApp, ctx: &egui::Context) {
    let mut is_open = app.midi_mapping_window_open;
    let theme = app.theme.midi_mapping_window.clone();
    let mut should_close_by_button = false;

    Window::new("MIDI Control Setup")
        .open(&mut is_open)
        .frame(Frame::window(&ctx.style()).fill(theme.background))
        .resizable(true)
        .default_width(750.0)
        .default_height(500.0)
        .show(ctx, |ui| {
            // --- Top Panel for static info ---
            TopBottomPanel::top("midi_mapping_top_panel")
                .frame(Frame::new().inner_margin(egui::Margin::symmetric(0, 8)))
                .show_inside(ui, |ui| {
                    ui.vertical_centered(|ui| {
                        if let Ok(last_msg) = app.last_midi_cc_message.read() {
                            let text = if let Some((id, _)) = &*last_msg {
                                match id {
                                    FullMidiIdentifier::ControlChange(cc_id) => {
                                        format!(
                                            "Last received: '{}' - Chan {} - CC {}",
                                            cc_id.port_name,
                                            cc_id.channel + 1,
                                            cc_id.cc
                                        )
                                    }
                                    FullMidiIdentifier::Note(note_id) => {
                                        format!(
                                            "Last received: '{}' - Chan {} - Note {}",
                                            note_id.port_name,
                                            note_id.channel + 1,
                                            note_to_name(note_id.note)
                                        )
                                    }
                                }
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
                            app.save_settings();
                        }
                        if ui.add(Button::new("Close").fill(theme.button_bg)).clicked() {
                            should_close_by_button = true;
                        }
                    });
                });

            // --- Central Panel for the scrollable content ---
            CentralPanel::default().show_inside(ui, |ui| {
                // Create the reverse lookup with OWNED data in its own scope to release the read lock immediately.
                let reverse_lookup: BTreeMap<ControllableParameter, FullMidiIdentifier> = {
                    let mappings = app.midi_mappings.read().unwrap();
                    mappings.iter().map(|(k, v)| (*v, k.clone())).collect()
                }; // The `mappings` read lock is dropped here.

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
                            columns[2]
                                .vertical_centered(|ui| ui.label(RichText::new("Actions").strong()));
                        });
                    });

                    // --- Focused Controls Section ---
                    ui.collapsing(
                        RichText::new("Focused Controls")
                            .strong()
                            .color(theme.label_color),
                        |ui| {
                            let params = [
                                ControllableParameter::FxFocusedWetDry,
                                ControllableParameter::FxFocusedPresetChange, // New
                            ];
                            for (i, param) in params.iter().enumerate() {
                                let row_color = if i % 2 == 0 { theme.row_even_bg } else { theme.row_odd_bg };
                                Frame::new().fill(row_color).show(ui, |ui| {
                                    draw_mapping_row(ui, *param, &reverse_lookup, app);
                                });
                            }
                        },
                    );

                    // --- Instruments Section ---
                    ui.collapsing(
                        RichText::new("Instruments")
                            .strong()
                            .color(theme.label_color),
                        |ui| {
                            let params = [
                                ControllableParameter::ToggleSynthEditor,
                                ControllableParameter::SynthToggleActive,
                                ControllableParameter::SynthMasterVolume,
                                ControllableParameter::ToggleSamplerEditor,
                                ControllableParameter::SamplerToggleActive,
                                ControllableParameter::SamplerMasterVolume,
                            ];
                            for (i, param) in params.iter().enumerate() {
                                let row_color = if i % 2 == 0 { theme.row_even_bg } else { theme.row_odd_bg };
                                Frame::new().fill(row_color).show(ui, |ui| {
                                    draw_mapping_row(ui, *param, &reverse_lookup, app);
                                });
                            }
                        },
                    );

                    // --- Atmosphere Section ---
                    ui.collapsing(
                        RichText::new("Atmosphere")
                            .strong()
                            .color(theme.label_color),
                        |ui| {
                            let mut params = vec![
                                ControllableParameter::ToggleAtmoEditor,
                                ControllableParameter::AtmoMasterVolume,
                                ControllableParameter::AtmoXY(0), // X-axis
                                ControllableParameter::AtmoXY(1), // Y-axis
                            ];
                            for i in 0..4 {
                                params.push(ControllableParameter::AtmoLayerVolume(i));
                            }
                            for (i, param) in params.iter().enumerate() {
                                let row_color = if i % 2 == 0 { theme.row_even_bg } else { theme.row_odd_bg };
                                Frame::new().fill(row_color).show(ui, |ui| {
                                    draw_mapping_row(ui, *param, &reverse_lookup, app);
                                });
                            }
                        },
                    );

                    // --- Audio Input Section ---
                    ui.collapsing(
                        RichText::new("Audio Input")
                            .strong()
                            .color(theme.label_color),
                        |ui| {
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
                        },
                    );

                    // --- Transport Section ---
                    ui.collapsing(
                        RichText::new("Transport")
                            .strong()
                            .color(theme.label_color),
                        |ui| {
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
                        },
                    );

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

                    // --- Metronome Section ---
                    ui.collapsing(RichText::new("Metronome").strong().color(theme.label_color), |ui| {
                        let params = [
                            ControllableParameter::MetronomeVolume,
                            ControllableParameter::MetronomePitch,
                            ControllableParameter::MetronomeToggleMute,
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
                                fx::InsertionPoint::Atmo,
                            ],
                        ]
                            .concat();

                        let mut row_index = 0;

                        // First, draw all Wet/Dry parameters
                        for point in &all_insertion_points {
                            let row_color = if row_index % 2 == 0 { theme.row_even_bg } else { theme.row_odd_bg };
                            let wet_dry_param =
                                ControllableParameter::Fx(FxParamIdentifier {
                                    point: *point,
                                    component_index: usize::MAX, // Special index for wet/dry
                                    param_name: FxParamName::WetDry,
                                });
                            Frame::new().fill(row_color).show(ui, |ui| {
                                draw_mapping_row(ui, wet_dry_param, &reverse_lookup, app);
                            });
                            row_index += 1;
                        }

                        // Then, draw all Toggle Editor parameters
                        for point in &all_insertion_points {
                            let row_color = if row_index % 2 == 0 { theme.row_even_bg } else { theme.row_odd_bg };
                            let toggle_param = ControllableParameter::ToggleFxEditor(*point);
                            Frame::new().fill(row_color).show(ui, |ui| {
                                draw_mapping_row(ui, toggle_param, &reverse_lookup, app);
                            });
                            row_index += 1;
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

fn draw_mapping_row(
    ui: &mut Ui,
    param: ControllableParameter,
    reverse_lookup: &BTreeMap<ControllableParameter, FullMidiIdentifier>,
    app: &mut CypherApp,
) {
    let theme = &app.theme.midi_mapping_window;

    ui.columns(3, |columns| {
        columns[0].label(param.to_string());

        let assigned_id = reverse_lookup.get(&param);

        let assignment_text = if let Some(identifier) = assigned_id {
            match identifier {
                FullMidiIdentifier::ControlChange(control_id) => {
                    let device_str = if control_id.port_name.is_empty() { "[Any Device]" } else { &control_id.port_name };
                    format!("'{}' - Ch {} - CC {}", device_str, control_id.channel + 1, control_id.cc)
                }
                FullMidiIdentifier::Note(note_id) => {
                    let device_str = if note_id.port_name.is_empty() { "[Any Device]" } else { &note_id.port_name };
                    format!("'{}' - Ch {} - {}", device_str, note_id.channel + 1, note_to_name(note_id.note))
                }
            }
        } else {
            "Unassigned".to_string()
        };
        columns[1].label(assignment_text);

        columns[2].horizontal(|ui| {
            let is_learning_this = { *app.midi_learn_target.read().unwrap() == Some(param) };
            let learn_button_text = if is_learning_this { "Listening..." } else { "Learn" };
            let learn_button = Button::new(learn_button_text).fill(if is_learning_this { theme.learn_button_bg.linear_multiply(1.5) } else { theme.learn_button_bg });
            if ui.add(learn_button).clicked() {
                let mut learn_target = app.midi_learn_target.write().unwrap();
                *learn_target = if is_learning_this { None } else { Some(param) };
            }

            if ui.add(Button::new("Clear").fill(theme.button_bg)).clicked() {
                if let Some(id) = assigned_id {
                    app.midi_mappings.write().unwrap().remove(id);
                    app.midi_mapping_modes.write().unwrap().remove(id);
                    app.midi_mapping_inversions.write().unwrap().remove(id); // Also clear inversion setting
                }
                if is_learning_this { *app.midi_learn_target.write().unwrap() = None; }
            }

            if let Some(id) = assigned_id {
                if param.is_continuous() {
                    ui.add_space(10.0);
                    // --- UI for selecting Absolute/Relative mode ---
                    let mut modes = app.midi_mapping_modes.write().unwrap();
                    let mut mode = modes.get(id).copied().unwrap_or_default();

                    let is_abs = mode == MidiControlMode::Absolute;
                    if ui.selectable_label(is_abs, "Abs").on_hover_text("Absolute Mode").clicked() {
                        mode = MidiControlMode::Absolute;
                    }

                    let is_rel = mode == MidiControlMode::Relative;
                    if ui.selectable_label(is_rel, "Rel").on_hover_text("Relative Mode (for infinite encoders)").clicked() {
                        mode = MidiControlMode::Relative;
                    }

                    if mode == MidiControlMode::default() {
                        modes.remove(id);
                    } else {
                        modes.insert(id.clone(), mode);
                    }

                    // --- UI for Inversion ---
                    ui.add_space(10.0);
                    let mut inversions = app.midi_mapping_inversions.write().unwrap();
                    let mut is_inverted = inversions.get(id).copied().unwrap_or(false);
                    if ui
                        .add(Checkbox::new(&mut is_inverted, "Inv"))
                        .on_hover_text("Invert the direction of this control")
                        .changed()
                    {
                        if is_inverted {
                            inversions.insert(id.clone(), true);
                        } else {
                            inversions.remove(id);
                        }
                    }
                }
            }
        });
    });
}