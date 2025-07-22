// src/ui/theme_editor_view.rs
use crate::app::CypherApp;
use crate::looper::NUM_LOOPERS;
use egui::{collapsing_header::CollapsingHeader, Grid, ScrollArea, Window};

pub fn draw_theme_editor_window(app: &mut CypherApp, ctx: &egui::Context) {
    let mut is_open = app.theme_editor_window_open;
    let mut theme_to_load = None;

    Window::new("Theme Editor")
        .open(&mut is_open)
        .default_size([400.0, 600.0])
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Save Theme").clicked() {
                    app.save_theme();
                }
                if ui.button("Rescan Themes").clicked() {
                    app.rescan_available_themes();
                }
                if ui.button("Reset to Default").clicked() {
                    app.theme = Default::default();
                    app.settings.last_theme = None;
                }
            });
            ui.separator();

            ScrollArea::vertical().show(ui, |ui| {
                CollapsingHeader::new("Saved Themes")
                    .default_open(true)
                    .show(ui, |ui| {
                        if app.available_themes.is_empty() {
                            ui.label("No saved themes found in 'AppSettings/Themes'.");
                        } else {
                            for theme_chunk in app.available_themes.chunks(4) {
                                ui.horizontal(|ui| {
                                    for (name, path) in theme_chunk {
                                        if ui.button(name).clicked() {
                                            theme_to_load = Some(path.clone());
                                        }
                                    }
                                });
                            }
                        }
                    });

                CollapsingHeader::new("Edit Current Theme")
                    .default_open(true)
                    .show(ui, |ui| {
                        CollapsingHeader::new("Globals")
                            .default_open(true)
                            .show(ui, |ui| {
                                Grid::new("globals_theme").show(ui, |ui| {
                                    ui.label("Dark Mode");
                                    ui.toggle_value(&mut app.theme.dark_mode, "");
                                    ui.end_row();

                                    ui.label("Main Background");
                                    ui.color_edit_button_srgba(&mut app.theme.main_background);
                                    ui.end_row();

                                    ui.label("Global Text");
                                    ui.color_edit_button_srgba(&mut app.theme.global_text_color);
                                    ui.end_row();

                                    ui.label("Window Stroke");
                                    ui.color_edit_button_srgba(&mut app.theme.window_stroke_color);
                                    ui.end_row();
                                });
                            });

                        CollapsingHeader::new("Top Bar")
                            .default_open(false)
                            .show(ui, |ui| {
                                Grid::new("top_bar_theme").show(ui, |ui| {
                                    ui.label("Background");
                                    ui.color_edit_button_srgba(&mut app.theme.top_bar.background);
                                    ui.end_row();
                                    ui.label("Button Background");
                                    ui.color_edit_button_srgba(&mut app.theme.top_bar.button_bg);
                                    ui.end_row();
                                    ui.label("Text");
                                    ui.color_edit_button_srgba(&mut app.theme.top_bar.text_color);
                                    ui.end_row();
                                    ui.label("Separator");
                                    ui.color_edit_button_srgba(&mut app.theme.top_bar.separator_color);
                                    ui.end_row();
                                    ui.label("Transport Bar");
                                    ui.color_edit_button_srgba(&mut app.theme.top_bar.transport_bar_fill);
                                    ui.end_row();
                                    ui.label("Transport BG");
                                    ui.color_edit_button_srgba(&mut app.theme.top_bar.transport_bar_background);
                                    ui.end_row();
                                    ui.label("XRun Text");
                                    ui.color_edit_button_srgba(&mut app.theme.top_bar.xrun_text_color);
                                    ui.end_row();
                                });
                            });

                        CollapsingHeader::new("Instrument Panels")
                            .default_open(false)
                            .show(ui, |ui| {
                                Grid::new("instrument_panel_theme").show(ui, |ui| {
                                    ui.label("Background");
                                    ui.color_edit_button_srgba(&mut app.theme.instrument_panel.panel_background);
                                    ui.end_row();
                                    ui.label("Button BG");
                                    ui.color_edit_button_srgba(&mut app.theme.instrument_panel.button_bg);
                                    ui.end_row();
                                    ui.label("Button Active BG");
                                    ui.color_edit_button_srgba(&mut app.theme.instrument_panel.button_active_bg);
                                    ui.end_row();
                                    ui.label("Input Armed BG");
                                    ui.color_edit_button_srgba(&mut app.theme.instrument_panel.input_armed_bg);
                                    ui.end_row();
                                    ui.label("Input Monitor BG");
                                    ui.color_edit_button_srgba(&mut app.theme.instrument_panel.input_monitor_bg);
                                    ui.end_row();
                                    ui.label("Fader Track BG");
                                    ui.color_edit_button_srgba(&mut app.theme.instrument_panel.fader_track_bg);
                                    ui.end_row();
                                    ui.label("Fader Thumb");
                                    ui.color_edit_button_srgba(&mut app.theme.instrument_panel.fader_thumb_color);
                                    ui.end_row();
                                    ui.label("Label");
                                    ui.color_edit_button_srgba(&mut app.theme.instrument_panel.label_color);
                                    ui.end_row();
                                });
                            });

                        CollapsingHeader::new("Transport Controls")
                            .default_open(false)
                            .show(ui, |ui| {
                                Grid::new("transport_controls_theme").show(ui, |ui| {
                                    ui.label("Background");
                                    ui.color_edit_button_srgba(&mut app.theme.transport_controls.panel_background);
                                    ui.end_row();
                                    ui.label("Button BG");
                                    ui.color_edit_button_srgba(&mut app.theme.transport_controls.button_bg);
                                    ui.end_row();
                                    ui.label("Play Active BG");
                                    ui.color_edit_button_srgba(&mut app.theme.transport_controls.play_active_bg);
                                    ui.end_row();
                                    ui.label("Mute Active BG");
                                    ui.color_edit_button_srgba(&mut app.theme.transport_controls.mute_active_bg);
                                    ui.end_row();
                                    ui.label("Clear BG");
                                    ui.color_edit_button_srgba(&mut app.theme.transport_controls.clear_button_bg);
                                    ui.end_row();
                                    ui.label("Label");
                                    ui.color_edit_button_srgba(&mut app.theme.transport_controls.label_color);
                                    ui.end_row();
                                });
                            });

                        CollapsingHeader::new("Loopers")
                            .default_open(false)
                            .show(ui, |ui| {
                                Grid::new("looper_theme").show(ui, |ui| {
                                    ui.label("Empty BG");
                                    ui.color_edit_button_srgba(&mut app.theme.loopers.empty_bg);
                                    ui.end_row();
                                    ui.label("Armed BG");
                                    ui.color_edit_button_srgba(&mut app.theme.loopers.armed_bg);
                                    ui.end_row();
                                    ui.label("Recording BG");
                                    ui.color_edit_button_srgba(&mut app.theme.loopers.recording_bg);
                                    ui.end_row();
                                    ui.label("Overdub BG");
                                    ui.color_edit_button_srgba(&mut app.theme.loopers.overdubbing_bg);
                                    ui.end_row();
                                    ui.label("Progress Bar BG");
                                    ui.color_edit_button_srgba(&mut app.theme.loopers.progress_bar_bg);
                                    ui.end_row();
                                    ui.label("Clear Button BG");
                                    ui.color_edit_button_srgba(&mut app.theme.loopers.clear_button_bg);
                                    ui.end_row();
                                    ui.label("Text");
                                    ui.color_edit_button_srgba(&mut app.theme.loopers.text_color);
                                    ui.end_row();
                                });
                                ui.collapsing("Track Colors", |ui| {
                                    Grid::new("looper_track_colors").show(ui, |ui| {
                                        for i in 0..NUM_LOOPERS {
                                            ui.label(format!("Looper {}", i + 1));
                                            ui.color_edit_button_srgba(&mut app.theme.loopers.track_colors[i]);
                                            if (i + 1) % 4 == 0 {
                                                ui.end_row();
                                            }
                                        }
                                    });
                                });
                            });

                        CollapsingHeader::new("Mixer")
                            .default_open(false)
                            .show(ui, |ui| {
                                Grid::new("mixer_theme").show(ui, |ui| {
                                    ui.label("Background");
                                    ui.color_edit_button_srgba(&mut app.theme.mixer.panel_background);
                                    ui.end_row();
                                    ui.label("Label");
                                    ui.color_edit_button_srgba(&mut app.theme.mixer.label_color);
                                    ui.end_row();
                                    ui.label("Fader Track BG");
                                    ui.color_edit_button_srgba(&mut app.theme.mixer.fader_track_bg);
                                    ui.end_row();
                                    ui.label("Fader Thumb");
                                    ui.color_edit_button_srgba(&mut app.theme.mixer.fader_thumb_color);
                                    ui.end_row();
                                    ui.label("Mute Off BG");
                                    ui.color_edit_button_srgba(&mut app.theme.mixer.mute_off_bg);
                                    ui.end_row();
                                    ui.label("Mute On BG");
                                    ui.color_edit_button_srgba(&mut app.theme.mixer.mute_on_bg);
                                    ui.end_row();
                                    ui.label("Solo Off BG");
                                    ui.color_edit_button_srgba(&mut app.theme.mixer.solo_off_bg);
                                    ui.end_row();
                                    ui.label("Solo On BG");
                                    ui.color_edit_button_srgba(&mut app.theme.mixer.solo_on_bg);
                                    ui.end_row();
                                    ui.label("Meter Normal");
                                    ui.color_edit_button_srgba(&mut app.theme.mixer.meter_normal_color);
                                    ui.end_row();
                                    ui.label("Meter Clip");
                                    ui.color_edit_button_srgba(&mut app.theme.mixer.meter_clip_color);
                                    ui.end_row();
                                    ui.label("Limiter GR");
                                    ui.color_edit_button_srgba(&mut app.theme.mixer.limiter_gr_color);
                                    ui.end_row();
                                    ui.label("Limiter Active BG");
                                    ui.color_edit_button_srgba(&mut app.theme.mixer.limiter_active_bg);
                                    ui.end_row();
                                });
                            });

                        CollapsingHeader::new("Library")
                            .default_open(false)
                            .show(ui, |ui| {
                                Grid::new("library_theme").show(ui, |ui| {
                                    ui.label("Background");
                                    ui.color_edit_button_srgba(&mut app.theme.library.panel_background);
                                    ui.end_row();
                                    ui.label("Button BG");
                                    ui.color_edit_button_srgba(&mut app.theme.library.button_bg);
                                    ui.end_row();
                                    ui.label("Active Tab BG");
                                    ui.color_edit_button_srgba(&mut app.theme.library.tab_active_bg);
                                    ui.end_row();
                                    ui.label("Inactive Tab BG");
                                    ui.color_edit_button_srgba(&mut app.theme.library.tab_inactive_bg);
                                    ui.end_row();
                                    ui.label("Card BG");
                                    ui.color_edit_button_srgba(&mut app.theme.library.card_bg);
                                    ui.end_row();
                                    ui.label("Card Hovered BG");
                                    ui.color_edit_button_srgba(&mut app.theme.library.card_hovered_bg);
                                    ui.end_row();
                                    ui.label("Text");
                                    ui.color_edit_button_srgba(&mut app.theme.library.text_color);
                                    ui.end_row();
                                });
                            });

                        CollapsingHeader::new("Options Window")
                            .default_open(false)
                            .show(ui, |ui| {
                                Grid::new("options_theme").show(ui, |ui| {
                                    ui.label("Background");
                                    ui.color_edit_button_srgba(&mut app.theme.options_window.background);
                                    ui.end_row();
                                    ui.label("Heading Text");
                                    ui.color_edit_button_srgba(&mut app.theme.options_window.heading_color);
                                    ui.end_row();
                                    ui.label("Label Text");
                                    ui.color_edit_button_srgba(&mut app.theme.options_window.label_color);
                                    ui.end_row();
                                    ui.label("Widget BG");
                                    ui.color_edit_button_srgba(&mut app.theme.options_window.widget_bg);
                                    ui.end_row();
                                    ui.label("Slider Grab");
                                    ui.color_edit_button_srgba(&mut app.theme.options_window.slider_grab_color);
                                    ui.end_row();
                                    ui.label("BPM Rounding On BG");
                                    ui.color_edit_button_srgba(&mut app.theme.options_window.bpm_rounding_on_bg);
                                    ui.end_row();
                                });
                            });

                        CollapsingHeader::new("Sampler Pad Window")
                            .default_open(false)
                            .show(ui, |ui| {
                                Grid::new("pad_theme").show(ui, |ui| {
                                    ui.label("Background");
                                    ui.color_edit_button_srgba(&mut app.theme.sampler_pad_window.background);
                                    ui.end_row();
                                    ui.label("Pad Background");
                                    ui.color_edit_button_srgba(&mut app.theme.sampler_pad_window.pad_bg_color);
                                    ui.end_row();
                                    ui.label("Kit Button BG");
                                    ui.color_edit_button_srgba(&mut app.theme.sampler_pad_window.kit_button_bg);
                                    ui.end_row();
                                    ui.label("Trash Mode Active BG");
                                    ui.color_edit_button_srgba(&mut app.theme.sampler_pad_window.trash_mode_active_bg);
                                    ui.end_row();
                                    ui.label("Playing Outline");
                                    ui.color_edit_button_srgba(&mut app.theme.sampler_pad_window.pad_playing_outline_color);
                                    ui.end_row();
                                    ui.label("Trash Hover Outline");
                                    ui.color_edit_button_srgba(&mut app.theme.sampler_pad_window.pad_trash_hover_outline_color);
                                    ui.end_row();
                                    ui.label("Row 1 Outline");
                                    ui.color_edit_button_srgba(&mut app.theme.sampler_pad_window.pad_outline_row_1_color);
                                    ui.end_row();
                                    ui.label("Row 2 Outline");
                                    ui.color_edit_button_srgba(&mut app.theme.sampler_pad_window.pad_outline_row_2_color);
                                    ui.end_row();
                                    ui.label("Row 3 Outline");
                                    ui.color_edit_button_srgba(&mut app.theme.sampler_pad_window.pad_outline_row_3_color);
                                    ui.end_row();
                                    ui.label("Row 4 Outline");
                                    ui.color_edit_button_srgba(&mut app.theme.sampler_pad_window.pad_outline_row_4_color);
                                    ui.end_row();
                                });
                            });

                        CollapsingHeader::new("Synth Editor")
                            .default_open(false)
                            .show(ui, |ui| {
                                let theme = &mut app.theme.synth_editor_window;
                                ui.collapsing("General", |ui| {
                                    Grid::new("synth_theme_general").show(ui, |ui| {
                                        ui.label("Background");
                                        ui.color_edit_button_srgba(&mut theme.background);
                                        ui.end_row();
                                        ui.label("Engine Panel BG");
                                        ui.color_edit_button_srgba(&mut theme.engine_panel_bg);
                                        ui.end_row();
                                        ui.label("Section BG");
                                        ui.color_edit_button_srgba(&mut theme.section_bg);
                                        ui.end_row();
                                        ui.label("Visualizer BG");
                                        ui.color_edit_button_srgba(&mut theme.visualizer_bg);
                                        ui.end_row();
                                        ui.label("Label Text");
                                        ui.color_edit_button_srgba(&mut theme.label_color);
                                        ui.end_row();
                                    });
                                });
                                ui.collapsing("Buttons", |ui| {
                                    Grid::new("synth_theme_buttons").show(ui, |ui| {
                                        ui.label("Button BG");
                                        ui.color_edit_button_srgba(&mut theme.button_bg);
                                        ui.end_row();
                                        ui.label("Button Hover BG");
                                        ui.color_edit_button_srgba(&mut theme.button_hover_bg);
                                        ui.end_row();
                                        ui.label("Button Active BG");
                                        ui.color_edit_button_srgba(&mut theme.button_active_bg);
                                        ui.end_row();
                                        ui.label("Button Text");
                                        ui.color_edit_button_srgba(&mut theme.button_text_color);
                                        ui.end_row();
                                    });
                                });
                                ui.collapsing("Tabs", |ui| {
                                    Grid::new("synth_theme_tabs").show(ui, |ui| {
                                        ui.label("Tab BG");
                                        ui.color_edit_button_srgba(&mut theme.tab_bg);
                                        ui.end_row();
                                        ui.label("Tab Active BG");
                                        ui.color_edit_button_srgba(&mut theme.tab_active_bg);
                                        ui.end_row();
                                        ui.label("Tab Text");
                                        ui.color_edit_button_srgba(&mut theme.tab_text_color);
                                        ui.end_row();
                                    });
                                });
                                ui.collapsing("ComboBoxes & Controls", |ui| {
                                    Grid::new("synth_theme_controls").show(ui, |ui| {
                                        ui.label("Control BG");
                                        ui.color_edit_button_srgba(&mut theme.control_bg);
                                        ui.end_row();
                                        ui.label("Control Hover BG");
                                        ui.color_edit_button_srgba(&mut theme.control_hover_bg);
                                        ui.end_row();
                                        ui.label("Popup BG");
                                        ui.color_edit_button_srgba(&mut theme.combo_popup_bg);
                                        ui.end_row();
                                        ui.label("Popup Selection BG");
                                        ui.color_edit_button_srgba(&mut theme.combo_selection_bg);
                                        ui.end_row();
                                    });
                                });
                                ui.collapsing("Sliders", |ui| {
                                    Grid::new("synth_theme_sliders").show(ui, |ui| {
                                        ui.label("Slider Track");
                                        ui.color_edit_button_srgba(&mut theme.slider_track_color);
                                        ui.end_row();
                                        ui.label("Slider Grab (Active)");
                                        ui.color_edit_button_srgba(&mut theme.slider_grab_color);
                                        ui.end_row();
                                        ui.label("Slider Grab (Hover)");
                                        ui.color_edit_button_srgba(&mut theme.slider_grab_hover_color);
                                        ui.end_row();
                                    });
                                });
                                ui.collapsing("Visualizers", |ui| {
                                    Grid::new("synth_theme_viz").show(ui, |ui| {
                                        ui.label("Wavetable Slot Name");
                                        ui.color_edit_button_srgba(&mut theme.wt_slot_name_color);
                                        ui.end_row();
                                        ui.label("WT Preview Active");
                                        ui.color_edit_button_srgba(&mut theme.wt_preview_active_waveform_color);
                                        ui.end_row();
                                        ui.label("WT Preview Inactive");
                                        ui.color_edit_button_srgba(&mut theme.wt_preview_inactive_waveform_color);
                                        ui.end_row();
                                        ui.label("WT Preview Final");
                                        ui.color_edit_button_srgba(&mut theme.wt_preview_final_waveform_color);
                                        ui.end_row();
                                        ui.label("WT Preview Bell-Filtered");
                                        ui.color_edit_button_srgba(&mut theme.wt_preview_bell_filtered_waveform_color);
                                        ui.end_row();
                                        ui.label("Mod: Pitch");
                                        ui.color_edit_button_srgba(&mut theme.mod_pitch_color);
                                        ui.end_row();
                                        ui.label("Mod: Filter");
                                        ui.color_edit_button_srgba(&mut theme.mod_filter_color);
                                        ui.end_row();
                                        ui.label("Mod: Amp (Cold)");
                                        ui.color_edit_button_srgba(&mut theme.mod_amp_cold_color);
                                        ui.end_row();
                                        ui.label("Mod: Amp (Hot)");
                                        ui.color_edit_button_srgba(&mut theme.mod_amp_hot_color);
                                        ui.end_row();
                                    });
                                });
                            });
                    });
            });
        });

    if let Some(path) = theme_to_load {
        app.load_theme_from_path(&path);
    }
    app.theme_editor_window_open = is_open;
}