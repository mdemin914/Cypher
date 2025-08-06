use crate::app::{CypherApp, EngineState, SynthUISection};
use crate::asset::Asset;
use crate::audio_engine::AudioCommand;
use crate::sampler_engine::NUM_SAMPLE_SLOTS;
use crate::synth::{
    AdsrSettings, FilterMode, LfoRateMode, LfoWaveform, ModDestination, ModRouting, ModSource,
};
use crate::theme::SynthEditorTheme;
use crate::wavetable_engine::{WavetableSet, WavetableSource};
use egui::{
    epaint::{self, PathShape, RectShape, StrokeKind},
    lerp, pos2, Align, Align2, Button, Color32, ComboBox, CornerRadius, DragAndDrop, Frame, Layout,
    ProgressBar, Rect, RichText, ScrollArea, Sense, Shape, Slider, Stroke, Ui, Vec2, Window,
};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, RwLock, RwLockWriteGuard};

const SYNTH_EDITOR_MAX_WIDTH: f32 = 1000.0;
const SYNTH_EDITOR_DEFAULT_HEIGHT: f32 = 750.0;

/// A custom button widget that directly uses the colors from the `SynthEditorTheme`.
fn custom_button(
    ui: &mut Ui,
    text: impl Into<egui::WidgetText>,
    theme: &SynthEditorTheme,
) -> egui::Response {
    let text = text.into();
    // Use `None` to indicate no text wrapping.
    let galley = text.into_galley(ui, None, f32::INFINITY, egui::TextStyle::Button);
    let padding = ui.spacing().button_padding * 2.0;
    let desired_size = galley.size() + padding;
    let (rect, response) = ui.allocate_at_least(desired_size, Sense::click());

    if ui.is_rect_visible(rect) {
        let (bg_color, text_color) = {
            let text_color = theme.button_text_color;
            if response.is_pointer_button_down_on() {
                (theme.button_active_bg, text_color)
            } else if response.hovered() {
                (theme.button_hover_bg, text_color)
            } else {
                (theme.button_bg, text_color)
            }
        };

        ui.painter().rect(
            rect,
            CornerRadius::ZERO, // No rounding
            bg_color,
            Stroke::NONE,
            StrokeKind::Inside,
        );

        let text_pos = rect.center() - galley.size() / 2.0;
        ui.painter().galley(text_pos, galley, text_color);
    }

    response
}

#[derive(Clone, Copy)]
struct AdsrUiSettings {
    attack: f32,
    decay: f32,
    sustain: f32,
    release: f32,
}

impl AdsrUiSettings {
    fn from_settings(settings: &AdsrSettings) -> Self {
        Self {
            attack: (settings.attack / 2.0).powf(0.25),
            decay: (settings.decay / 2.0).powf(0.25),
            sustain: settings.sustain,
            release: (settings.release / 4.0).powf(0.25),
        }
    }
}

pub fn draw_synth_editor_window(app: &mut CypherApp, ctx: &egui::Context) {
    let mut is_open = app.synth_editor_window_open;
    let mut window_title = "Synth Editor".to_string();
    if let Some(path) = &app.settings.last_synth_preset {
        if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
            window_title = format!("Synth Editor - {}", name);
        }
    }

    let theme = app.theme.synth_editor_window.clone();

    Window::new(window_title)
        .open(&mut is_open)
        .frame(Frame::window(&ctx.style()).fill(theme.background))
        .resizable(false)
        .default_width(SYNTH_EDITOR_MAX_WIDTH)
        .default_height(SYNTH_EDITOR_DEFAULT_HEIGHT)
        .pivot(Align2::CENTER_CENTER)
        .default_pos(ctx.screen_rect().center())
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                if custom_button(ui, "New Preset", &theme).clicked() {
                    app.initialize_new_preset();
                }
                if custom_button(ui, "Save Preset", &theme).clicked() {
                    app.save_preset();
                }
                if custom_button(ui, "Load Preset", &theme).clicked() {
                    app.load_preset();
                }
            });
            ui.separator();

            ui.columns(2, |columns| {
                ScrollArea::vertical()
                    .id_salt("engine_0_scroll")
                    .show(&mut columns[0], |ui| {
                        ui.set_min_height(SYNTH_EDITOR_DEFAULT_HEIGHT - 50.0);
                        draw_engine_panel(app, ui, 0);
                    });
                ScrollArea::vertical()
                    .id_salt("engine_1_scroll")
                    .show(&mut columns[1], |ui| {
                        ui.set_min_height(SYNTH_EDITOR_DEFAULT_HEIGHT - 50.0);
                        draw_engine_panel(app, ui, 1);
                    });
            });
        });
    app.synth_editor_window_open = is_open;
}

fn draw_engine_panel(app: &mut CypherApp, ui: &mut Ui, engine_index: usize) {
    let theme = app.theme.synth_editor_window.clone();
    let frame = Frame::new().fill(theme.engine_panel_bg);

    frame.show(ui, |ui| {
        let mut command_to_send = None;

        // --- Engine Header ---
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("ENGINE {}:", engine_index + 1))
                    .strong()
                    .color(theme.label_color),
            );

            ui.scope(|ui| {
                let visuals = &mut ui.style_mut().visuals.widgets;
                visuals.inactive.bg_fill = theme.control_bg;
                visuals.hovered.bg_fill = theme.control_hover_bg;
                visuals.active.bg_fill = theme.control_hover_bg;

                let is_wavetable =
                    matches!(app.engine_states[engine_index], EngineState::Wavetable(_));
                let selected_text = if is_wavetable {
                    "Wavetable"
                } else {
                    "Sampler"
                };
                ComboBox::from_id_salt(format!("engine_type_{}", engine_index))
                    .selected_text(selected_text)
                    .show_ui(ui, |ui| {
                        let style = ui.style_mut();
                        style.visuals.panel_fill = theme.combo_popup_bg;
                        style.visuals.selection.bg_fill = theme.combo_selection_bg;
                        if ui
                            .add(egui::Button::new("Wavetable").selected(is_wavetable))
                            .clicked()
                        {
                            app.set_engine_type(engine_index, true);
                        }
                        if ui
                            .add(egui::Button::new("Sampler").selected(!is_wavetable))
                            .clicked()
                        {
                            app.set_engine_type(engine_index, false);
                        }
                    });
            });

            match &mut app.engine_states[engine_index] {
                EngineState::Wavetable(state) => {
                    ui.scope(|ui| {
                        let visuals = &mut ui.style_mut().visuals.widgets;
                        visuals.inactive.bg_fill = theme.slider_track_color;
                        visuals.hovered.bg_fill = theme.slider_grab_hover_color;
                        visuals.active.bg_fill = theme.slider_grab_color;
                        ui.style_mut().visuals.slider_trailing_fill = true;

                        if ui.toggle_value(&mut state.is_polyphonic, RichText::new("Poly").monospace())
                            .changed()
                        {
                            command_to_send = Some(AudioCommand::SetSynthMode(
                                engine_index,
                                state.is_polyphonic,
                            ));
                            state.force_redraw_generation += 1;
                        }
                        let mut vol = state.volume.load(Ordering::Relaxed) as f32 / 1_000_000.0;
                        if ui.add(
                            Slider::new(&mut vol, 0.0..=1.5)
                                .text(RichText::new(format!("Vol E{}", engine_index)).color(theme.label_color)),
                        )
                            .changed()
                        {
                            state
                                .volume
                                .store((vol * 1_000_000.0) as u32, Ordering::Relaxed);
                        }
                    });
                }
                EngineState::Sampler(state) => {
                    ui.scope(|ui| {
                        let visuals = &mut ui.style_mut().visuals.widgets;
                        visuals.inactive.bg_fill = theme.slider_track_color;
                        visuals.hovered.bg_fill = theme.slider_grab_hover_color;
                        visuals.active.bg_fill = theme.slider_grab_color;
                        ui.style_mut().visuals.slider_trailing_fill = true;

                        if ui.toggle_value(&mut state.is_polyphonic, RichText::new("Poly").monospace())
                            .changed()
                        {
                            command_to_send = Some(AudioCommand::SetSynthMode(
                                engine_index,
                                state.is_polyphonic,
                            ));
                        }
                        let mut vol = state.volume.load(Ordering::Relaxed) as f32 / 1_000_000.0;
                        if ui.add(
                            Slider::new(&mut vol, 0.0..=1.5)
                                .text(RichText::new(format!("Vol E{}", engine_index)).color(theme.label_color)),
                        )
                            .changed()
                        {
                            state
                                .volume
                                .store((vol * 1_000_000.0) as u32, Ordering::Relaxed);
                        }
                    });
                }
            }
        });

        if let Some(cmd) = command_to_send {
            app.send_command(cmd);
        }
        ui.add_space(4.0);

        let (peak, is_wt) = match &mut app.engine_states[engine_index] {
            EngineState::Wavetable(state) => (state.displayed_peak_level, true),
            EngineState::Sampler(state) => (state.displayed_peak_level, false),
        };
        ui.add(
            ProgressBar::new(peak)
                .desired_height(4.0)
                .fill(app.theme.top_bar.transport_bar_fill),
        );

        // --- Visualizer ---
        Frame::new().fill(theme.visualizer_bg).show(ui, |ui| {
            let rect = ui.available_rect_before_wrap();
            let visualizer_height = 100.0;
            let rect = Rect::from_min_size(rect.min, Vec2::new(rect.width(), visualizer_height));
            ui.allocate_rect(rect, egui::Sense::hover());

            // Always request a repaint for continuous modulation
            ui.ctx().request_repaint();

            if ui.is_rect_visible(rect) {
                if is_wt {
                    draw_wavetable_preview(app, ui, rect, engine_index);
                } else {
                    draw_sampler_waveform_preview(app, ui, rect, engine_index);
                }
            }
        });
        ui.separator();

        // --- Section Tabs ---
        ui.horizontal_wrapped(|ui| {
            ui.scope(|ui| {
                let visuals = &mut ui.style_mut().visuals;
                visuals.widgets.inactive.bg_fill = theme.tab_bg;
                visuals.widgets.hovered.bg_fill = theme.control_hover_bg;
                visuals.widgets.active.bg_fill = theme.tab_active_bg;
                visuals.selection.bg_fill = theme.tab_active_bg;
                visuals.widgets.inactive.fg_stroke.color = theme.tab_text_color;
                visuals.widgets.hovered.fg_stroke.color = theme.tab_text_color;
                visuals.widgets.active.fg_stroke.color = theme.tab_text_color;

                let is_wavetable =
                    matches!(app.engine_states[engine_index], EngineState::Wavetable(_));
                let sections = [
                    if is_wavetable {
                        SynthUISection::Wavetable
                    } else {
                        SynthUISection::Sampler
                    },
                    SynthUISection::Saturation,
                    SynthUISection::Filter,
                    SynthUISection::VolumeEnv,
                    SynthUISection::FilterEnv,
                    SynthUISection::Lfo1,
                    SynthUISection::Lfo2,
                    SynthUISection::ModMatrix,
                ];
                for section in sections {
                    let is_selected = app.active_synth_section[engine_index] == section;
                    if ui
                        .add(egui::Button::new(section.to_string()).selected(is_selected).frame(true))
                        .clicked()
                    {
                        app.active_synth_section[engine_index] = section;
                    }
                }
            });
        });
        ui.separator();

        // --- Control Section ---
        Frame::new().fill(theme.section_bg).show(ui, |ui| {
            match app.active_synth_section[engine_index] {
                SynthUISection::Wavetable => draw_wavetable_controls(app, ui, engine_index),
                SynthUISection::Sampler => draw_sampler_controls(app, ui, engine_index),
                SynthUISection::Saturation => draw_saturation_controls(app, ui, engine_index),
                SynthUISection::Filter => draw_filter_controls(app, ui, engine_index),
                SynthUISection::VolumeEnv => draw_amp_env_controls(app, ui, engine_index),
                SynthUISection::FilterEnv => draw_filter_env_controls(app, ui, engine_index),
                SynthUISection::Lfo1 => draw_lfo_controls(app, ui, engine_index, 1),
                SynthUISection::Lfo2 => draw_lfo_controls(app, ui, engine_index, 2),
                SynthUISection::ModMatrix => draw_mod_matrix_controls(app, ui, engine_index),
            }
        });
    });
}

fn draw_wavetable_controls(app: &mut CypherApp, ui: &mut Ui, engine_index: usize) {
    let mut sample_to_load: Option<(usize, PathBuf)> = None;
    let theme = app.theme.synth_editor_window.clone();
    let mut changed = false;
    let mut reset_to_default = false; // New flag
    let mut slot_to_reset = None; // New flag

    let frame_fill = app.theme.synth_editor_window.engine_panel_bg; // Clone theme data before borrow

    // This will hold the action to perform after the mutable borrow is released
    let mut deferred_action: Option<(usize, f32)> = None;

    if let EngineState::Wavetable(state) = &mut app.engine_states[engine_index] {
        ui.scope(|ui| {
            let visuals = &mut ui.style_mut().visuals.widgets;
            visuals.inactive.bg_fill = theme.slider_track_color;
            visuals.hovered.bg_fill = theme.slider_grab_hover_color;
            visuals.active.bg_fill = theme.slider_grab_color;
            ui.style_mut().visuals.slider_trailing_fill = true;

            ui.add_space(8.0);
            let num_tables = state
                .wavetable_set
                .read()
                .map_or(1.0, |g| g.tables.len() as f32)
                .max(1.0);
            let mut wt_pos = state.wavetable_position.load(Ordering::Relaxed) as f32 / 1_000_000.0;
            if ui.add(
                Slider::new(&mut wt_pos, 0.0..=(num_tables - 1.0))
                    .text(RichText::new("Position").color(theme.label_color)),
            )
                .changed()
            {
                state
                    .wavetable_position
                    .store((wt_pos * 1_000_000.0) as u32, Ordering::Relaxed);
                changed = true;
            }
            ui.add_space(10.0);
            ui.label(
                RichText::new("Wavetable Slots")
                    .monospace()
                    .size(14.0)
                    .color(theme.label_color),
            );
            ui.add_space(4.0);
        });

        for i in 0..4 {
            let frame = Frame::new().fill(frame_fill);
            let group_response = frame.show(ui, |ui| {
                ui.set_min_width(ui.available_width() - 10.0);
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("Slot {}:", i + 1)).color(theme.label_color));
                        ui.label(
                            RichText::new(&state.wavetable_names[i])
                                .monospace()
                                .color(theme.wt_slot_name_color),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if matches!(&state.wavetable_sources[i], WavetableSource::File(_)) {
                                if ui.add(Button::new("Clear").small().fill(theme.button_bg)).clicked() {
                                    slot_to_reset = Some(i);
                                }
                            }
                        });
                    });
                    if matches!(&state.wavetable_sources[i], WavetableSource::File(_)) {
                        ui.scope(|ui| {
                            let visuals = &mut ui.style_mut().visuals.widgets;
                            visuals.inactive.bg_fill = theme.slider_track_color;
                            visuals.hovered.bg_fill = theme.slider_grab_hover_color;
                            visuals.active.bg_fill = theme.slider_grab_color;
                            ui.style_mut().visuals.slider_trailing_fill = true;

                            let mut window_pos = state.window_positions[i];
                            let slider_text = format!("Window E{} S{}", engine_index, i);
                            if ui.add(
                                Slider::new(&mut window_pos, 0.0..=1.0)
                                    .text(RichText::new(slider_text).color(theme.label_color)),
                            )
                                .changed()
                            {
                                // Defer the action instead of calling it directly
                                deferred_action = Some((i, window_pos));
                                changed = true;
                            }
                        });
                    } else {
                        ui.add_space(ui.spacing().interact_size.y);
                    }
                });
            });
            let drop_target_rect = group_response.response.rect;
            let is_hovered = ui.rect_contains_pointer(drop_target_rect);
            if is_hovered && DragAndDrop::has_any_payload(ui.ctx()) {
                ui.painter().rect_stroke(
                    drop_target_rect,
                    CornerRadius::ZERO,
                    ui.style().visuals.selection.stroke,
                    StrokeKind::Inside,
                );
            }
            if is_hovered && ui.input(|i| i.pointer.any_released()) {
                if let Some(payload) = DragAndDrop::take_payload::<Asset>(ui.ctx()) {
                    if let Asset::Sample(sample_ref) = (*payload).clone() {
                        sample_to_load = Some((i, sample_ref.path));
                    }
                }
            }
            ui.add_space(4.0);
        }

        ui.add_space(10.0);
        ui.label(
            RichText::new("Layer Mixer")
                .monospace()
                .size(14.0)
                .color(theme.label_color),
        );
        ui.add_space(4.0);

        ui.scope(|ui| {
            let visuals = &mut ui.style_mut().visuals.widgets;
            visuals.inactive.bg_fill = theme.slider_track_color;
            visuals.hovered.bg_fill = theme.slider_grab_hover_color;
            visuals.active.bg_fill = theme.slider_grab_color;
            ui.style_mut().visuals.slider_trailing_fill = true;

            if let Ok(mut mixer) = state.wavetable_mixer_settings.write() {
                changed |= ui.add(Slider::new(&mut mixer.layer_volumes[0], 0.0..=1.0).text(RichText::new("L1").color(theme.label_color))).changed();
                changed |= ui.add(Slider::new(&mut mixer.layer_volumes[1], 0.0..=1.0).text(RichText::new("L2").color(theme.label_color))).changed();
                changed |= ui.add(Slider::new(&mut mixer.layer_volumes[2], 0.0..=1.0).text(RichText::new("L3").color(theme.label_color))).changed();
                changed |= ui.add(Slider::new(&mut mixer.layer_volumes[3], 0.0..=1.0).text(RichText::new("L4").color(theme.label_color))).changed();
                ui.separator();
                changed |= ui.add(Slider::new(&mut mixer.layer_volumes[4], 0.0..=1.0).text(RichText::new("Blend").color(theme.label_color))).changed();
            }
        });
        ui.add_space(8.0);

        if custom_button(ui, "Reset to Default", &theme).clicked() {
            reset_to_default = true;
        }

        if changed {
            state.force_redraw_generation += 1;
        }
    } // The mutable borrow of `app.engine_states` ends here.

    // Now, we can safely perform the deferred action
    if let Some((slot_index, window_pos)) = deferred_action {
        if let EngineState::Wavetable(state) = &mut app.engine_states[engine_index] {
            state.window_positions[slot_index] = window_pos;
        }
        app.generate_and_send_wavetable(engine_index, slot_index, window_pos);
    }

    if let Some(slot_idx) = slot_to_reset {
        app.reset_wavetable_slot_to_default(engine_index, slot_idx);
    }

    // Handle actions that need a full &mut app borrow
    if reset_to_default {
        app.initialize_wavetable_preset(engine_index);
        // We need to re-borrow to set the flag after initialization
        if let EngineState::Wavetable(state) = &mut app.engine_states[engine_index] {
            state.force_redraw_generation += 1;
        }
    }

    if let Some((slot_index, path)) = sample_to_load {
        app.load_wav_for_synth_slot(engine_index, slot_index, path);
        if let EngineState::Wavetable(state) = &mut app.engine_states[engine_index] {
            state.force_redraw_generation += 1;
        }
    }
}

fn draw_sampler_controls(app: &mut CypherApp, ui: &mut Ui, engine_index: usize) {
    let mut command_to_send: Option<AudioCommand> = None;
    let theme = app.theme.synth_editor_window.clone();
    let mut sample_to_load: Option<(usize, PathBuf)> = None;
    let mut slot_to_clear: Option<usize> = None;

    // Helper function to convert MIDI note number to a name (e.g., 60 -> "C4")
    fn midi_to_note_name(note: u8) -> String {
        const NOTES: [&str; 12] = [
            "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
        ];
        let octave = (note as i8 / 12) - 1;
        let note_name = NOTES[(note % 12) as usize];
        format!("{}{}", note_name, octave)
    }

    if let EngineState::Sampler(state) = &mut app.engine_states[engine_index] {
        let mut settings_changed = false;

        ui.label(RichText::new("Sample Slots").color(theme.label_color));
        ui.add_space(4.0);

        // This content is now directly in the outer ScrollArea from draw_engine_panel
        for i in 0..NUM_SAMPLE_SLOTS {
            let group_response = ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("Slot {}:", i + 1))
                            .color(theme.label_color),
                    );
                    ui.label(
                        RichText::new(&state.sample_names[i])
                            .monospace()
                            .color(app.theme.synth_editor_window.wt_slot_name_color),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if state.sample_paths[i].is_some() {
                            if ui.add(Button::new("Clear").small().fill(theme.button_bg)).clicked() {
                                slot_to_clear = Some(i);
                            }
                        }
                    });
                });
                // Custom row for the slider and the note name label
                ui.horizontal(|ui| {
                    if ui
                        .add(
                            Slider::new(&mut state.root_notes[i], 0..=127)
                                .text(RichText::new("Root Note").color(theme.label_color)),
                        )
                        .changed()
                    {
                        settings_changed = true;
                    }
                    ui.monospace(midi_to_note_name(state.root_notes[i]));
                });
            });

            let drop_target_rect = group_response.response.rect;
            let is_hovered = ui.rect_contains_pointer(drop_target_rect);

            if is_hovered {
                if DragAndDrop::has_any_payload(ui.ctx()) {
                    ui.painter().rect_stroke(
                        drop_target_rect,
                        CornerRadius::ZERO,
                        ui.style().visuals.selection.stroke,
                        StrokeKind::Inside,
                    );
                }
                if ui.input(|inp| inp.pointer.any_released()) {
                    if let Some(payload) = DragAndDrop::take_payload::<Asset>(ui.ctx()) {
                        if let Asset::Sample(sample_ref) = (*payload).clone() {
                            sample_to_load = Some((i, sample_ref.path));
                        }
                    }
                }
            }
            ui.add_space(4.0);
        }

        ui.separator();
        ui.add_space(4.0);
        ui.label(RichText::new("Global Settings").color(theme.label_color));

        ui.scope(|ui| {
            let visuals = &mut ui.style_mut().visuals.widgets;
            visuals.inactive.bg_fill = theme.slider_track_color;
            visuals.hovered.bg_fill = theme.slider_grab_hover_color;
            visuals.active.bg_fill = theme.slider_grab_color;
            ui.style_mut().visuals.slider_trailing_fill = true;

            if ui
                .add(
                    Slider::new(&mut state.global_fine_tune_cents, -100.0..=100.0)
                        .text(RichText::new("Global Fine Tune").color(theme.label_color))
                        .suffix(" ¢")
                        .fixed_decimals(1),
                )
                .changed()
            {
                settings_changed = true;
            }
            if ui
                .add(
                    Slider::new(&mut state.fade_out, 0.0..=0.5)
                        .text(RichText::new("Fade Out").color(theme.label_color)),
                )
                .changed()
            {
                settings_changed = true;
            }
        });

        if settings_changed {
            command_to_send = Some(AudioCommand::SetSamplerSettings {
                engine_index,
                root_notes: state.root_notes,
                global_fine_tune_cents: state.global_fine_tune_cents,
                fade_out: state.fade_out,
            });
        }
    }

    if let Some(slot_idx) = slot_to_clear {
        app.clear_sample_for_sampler_slot(engine_index, slot_idx);
    }
    if let Some((slot_index, path)) = sample_to_load {
        app.load_sample_for_sampler_slot(engine_index, slot_index, path);
    }
    if let Some(cmd) = command_to_send {
        app.send_command(cmd);
    }
}

fn draw_saturation_controls(app: &mut CypherApp, ui: &mut Ui, engine_index: usize) {
    ui.add_space(8.0);
    let theme = app.theme.synth_editor_window.clone();
    let mut changed = false;

    let (settings_arc, sat_mod_atomic) = match &app.engine_states[engine_index] {
        EngineState::Wavetable(s) => (
            s.saturation_settings.clone(),
            s.saturation_mod_atomic.clone(),
        ),
        EngineState::Sampler(s) => (
            s.saturation_settings.clone(),
            s.saturation_mod_atomic.clone(),
        ),
    };

    let mut settings = *settings_arc.read().unwrap();

    ui.scope(|ui| {
        let visuals = &mut ui.style_mut().visuals.widgets;
        visuals.inactive.bg_fill = theme.slider_track_color;
        visuals.hovered.bg_fill = theme.slider_grab_hover_color;
        visuals.active.bg_fill = theme.slider_grab_color;
        ui.style_mut().visuals.slider_trailing_fill = true;
        if ui.add(
            Slider::new(&mut settings.drive, 0.0..=1.0)
                .text(RichText::new("Drive").color(theme.label_color)),
        )
            .changed()
        {
            changed = true;
        }
    });

    ui.add_space(10.0);
    ui.label(RichText::new("Compensation Curve:").color(theme.label_color));

    let (response, painter) =
        ui.allocate_painter(Vec2::new(ui.available_width(), 80.0), Sense::drag());
    let rect = response.rect;
    painter.rect(
        rect,
        CornerRadius::ZERO,
        theme.visualizer_bg,
        Stroke::NONE,
        StrokeKind::Inside,
    );

    let p0 = pos2(rect.left(), rect.bottom());
    let p2 = pos2(
        rect.right(),
        rect.bottom() - rect.height() * settings.compensation_amount,
    );
    let control_x = lerp(rect.left()..=rect.right(), 0.5);
    let control_y = lerp(p0.y..=p2.y, settings.compensation_bias);
    let p1 = pos2(control_x, control_y);

    if response.dragged() {
        if let Some(pos) = response.interact_pointer_pos() {
            let y_range = p2.y..=p0.y;
            let new_bias = 1.0 - (pos.y - y_range.start()) / (y_range.end() - y_range.start());
            settings.compensation_bias = new_bias.clamp(0.0, 1.0);
            changed = true;
        }
    }
    let curve_shape = epaint::CubicBezierShape::from_points_stroke(
        [p0, p1, p2, p2],
        false,
        Color32::TRANSPARENT,
        Stroke::new(2.0, theme.slider_grab_color),
    );
    painter.add(curve_shape);
    painter.circle_filled(p1, 4.0, theme.slider_grab_color);

    ui.scope(|ui| {
        let visuals = &mut ui.style_mut().visuals.widgets;
        visuals.inactive.bg_fill = theme.slider_track_color;
        visuals.hovered.bg_fill = theme.slider_grab_hover_color;
        visuals.active.bg_fill = theme.slider_grab_color;
        ui.style_mut().visuals.slider_trailing_fill = true;
        ui.horizontal(|ui| {
            if ui.add(
                Slider::new(&mut settings.compensation_amount, 0.0..=1.0)
                    .text(RichText::new("Amount").color(theme.label_color)),
            )
                .changed()
            {
                changed = true;
            }
            if ui.add(
                Slider::new(&mut settings.compensation_bias, 0.0..=1.0)
                    .text(RichText::new("Bias").color(theme.label_color)),
            )
                .changed()
            {
                changed = true;
            }
        });
    });

    if changed {
        *settings_arc.write().unwrap() = settings;
        if let EngineState::Wavetable(s) = &mut app.engine_states[engine_index] {
            s.force_redraw_generation += 1;
        }
    }

    ui.add_space(10.0);
    ui.label(RichText::new("Modulation Level:").color(theme.label_color));
    let mod_level = sat_mod_atomic.load(Ordering::Relaxed) as f32 / 1_000_000.0;
    ui.add(
        ProgressBar::new(mod_level)
            .show_percentage()
            .fill(theme.slider_grab_color),
    );
}

fn slider_to_time(value: f32, max_time: f32) -> f32 {
    value.powf(4.0) * max_time
}

fn draw_filter_controls(app: &mut CypherApp, ui: &mut Ui, engine_index: usize) {
    let theme = app.theme.synth_editor_window.clone();

    let mut ui_fn = |filter: &mut RwLockWriteGuard<crate::synth::FilterSettings>| -> bool {
        let mut local_changed = false;
        ui.horizontal(|ui| {
            ui.label(RichText::new("Mode:").color(theme.label_color));
            ui.scope(|ui| {
                let visuals = &mut ui.style_mut().visuals.widgets;
                visuals.inactive.bg_fill = theme.control_bg;
                visuals.hovered.bg_fill = theme.control_hover_bg;
                visuals.active.bg_fill = theme.control_hover_bg;

                let initial_mode = filter.mode;
                ComboBox::from_id_salt(format!("filter_mode_combo_{}", engine_index))
                    .selected_text(filter.mode.to_string())
                    .show_ui(ui, |ui| {
                        let style = ui.style_mut();
                        style.visuals.panel_fill = theme.combo_popup_bg;
                        style.visuals.selection.bg_fill = theme.combo_selection_bg;
                        for mode in FilterMode::ALL {
                            if ui.add(egui::Button::new(mode.to_string()).selected(filter.mode == mode))
                                .clicked()
                            {
                                filter.mode = mode;
                            }
                        }
                    });
                if initial_mode != filter.mode {
                    local_changed = true;
                }
            });
        });

        ui.scope(|ui| {
            let visuals = &mut ui.style_mut().visuals.widgets;
            visuals.inactive.bg_fill = theme.slider_track_color;
            visuals.hovered.bg_fill = theme.slider_grab_hover_color;
            visuals.active.bg_fill = theme.slider_grab_color;
            ui.style_mut().visuals.slider_trailing_fill = true;

            let mut cutoff_log = filter.cutoff.powf(1.0 / 4.0);
            if ui.add(
                Slider::new(&mut cutoff_log, 0.0..=1.0)
                    .text(RichText::new("Cutoff").color(theme.label_color)),
            )
                .changed()
            {
                filter.cutoff = cutoff_log.powf(4.0);
                local_changed = true;
            }
            if ui.add(
                Slider::new(&mut filter.resonance, 0.0..=1.0)
                    .text(RichText::new("Resonance").color(theme.label_color)),
            )
                .changed()
            {
                local_changed = true;
            }
        });
        local_changed
    };
    match &mut app.engine_states[engine_index] {
        EngineState::Wavetable(s) => {
            if let Ok(mut filter) = s.filter_settings.write() {
                if ui_fn(&mut filter) {
                    s.force_redraw_generation += 1;
                }
            }
        }
        EngineState::Sampler(s) => {
            if let Ok(mut filter) = s.filter_settings.write() {
                ui_fn(&mut filter);
            }
        }
    };
}

fn draw_adsr_sliders(
    ui: &mut Ui,
    ui_settings: &mut AdsrUiSettings,
    theme: &SynthEditorTheme,
) -> bool {
    let mut changed = false;
    ui.scope(|ui| {
        let visuals = &mut ui.style_mut().visuals.widgets;
        visuals.inactive.bg_fill = theme.slider_track_color;
        visuals.hovered.bg_fill = theme.slider_grab_hover_color;
        visuals.active.bg_fill = theme.slider_grab_color;
        ui.style_mut().visuals.slider_trailing_fill = true;

        changed |= ui.add(Slider::new(&mut ui_settings.attack, 0.0..=1.0).text(RichText::new("Attack").color(theme.label_color))).changed();
        changed |= ui.add(Slider::new(&mut ui_settings.decay, 0.0..=1.0).text(RichText::new("Decay").color(theme.label_color))).changed();
        changed |= ui.add(Slider::new(&mut ui_settings.sustain, 0.0..=1.0).text(RichText::new("Sustain").color(theme.label_color))).changed();
        changed |= ui.add(Slider::new(&mut ui_settings.release, 0.0..=1.0).text(RichText::new("Release").color(theme.label_color))).changed();
    });
    changed
}

fn draw_amp_env_controls(app: &mut CypherApp, ui: &mut Ui, engine_index: usize) {
    ui.add_space(8.0);
    let theme = app.theme.synth_editor_window.clone();
    let (amp_adsr, changed) = match &mut app.engine_states[engine_index] {
        EngineState::Wavetable(state) => {
            let mut ui_settings = AdsrUiSettings::from_settings(&state.amp_adsr);
            let changed = draw_adsr_sliders(ui, &mut ui_settings, &theme);
            if changed {
                state.force_redraw_generation += 1;
            }
            let new_settings = AdsrSettings {
                attack: slider_to_time(ui_settings.attack, 2.0),
                decay: slider_to_time(ui_settings.decay, 2.0),
                sustain: ui_settings.sustain,
                release: slider_to_time(ui_settings.release, 4.0),
            };
            state.amp_adsr = new_settings;
            (new_settings, changed)
        }
        EngineState::Sampler(state) => {
            let mut ui_settings = AdsrUiSettings::from_settings(&state.amp_adsr);
            let changed = draw_adsr_sliders(ui, &mut ui_settings, &theme);
            let new_settings = AdsrSettings {
                attack: slider_to_time(ui_settings.attack, 2.0),
                decay: slider_to_time(ui_settings.decay, 2.0),
                sustain: ui_settings.sustain,
                release: slider_to_time(ui_settings.release, 4.0),
            };
            state.amp_adsr = new_settings;
            (new_settings, changed)
        }
    };
    if changed {
        app.send_command(AudioCommand::SetAmpAdsr(engine_index, amp_adsr));
    }
}

fn draw_filter_env_controls(app: &mut CypherApp, ui: &mut Ui, engine_index: usize) {
    ui.add_space(8.0);
    let theme = app.theme.synth_editor_window.clone();
    let (filter_adsr, changed) = match &mut app.engine_states[engine_index] {
        EngineState::Wavetable(state) => {
            let mut ui_settings = AdsrUiSettings::from_settings(&state.filter_adsr);
            let changed = draw_adsr_sliders(ui, &mut ui_settings, &theme);
            if changed {
                state.force_redraw_generation += 1;
            }
            let new_settings = AdsrSettings {
                attack: slider_to_time(ui_settings.attack, 2.0),
                decay: slider_to_time(ui_settings.decay, 2.0),
                sustain: ui_settings.sustain,
                release: slider_to_time(ui_settings.release, 4.0),
            };
            state.filter_adsr = new_settings;
            (new_settings, changed)
        }
        EngineState::Sampler(state) => {
            let mut ui_settings = AdsrUiSettings::from_settings(&state.filter_adsr);
            let changed = draw_adsr_sliders(ui, &mut ui_settings, &theme);
            let new_settings = AdsrSettings {
                attack: slider_to_time(ui_settings.attack, 2.0),
                decay: slider_to_time(ui_settings.decay, 2.0),
                sustain: ui_settings.sustain,
                release: slider_to_time(ui_settings.release, 4.0),
            };
            state.filter_adsr = new_settings;
            (new_settings, changed)
        }
    };
    if changed {
        app.send_command(AudioCommand::SetFilterAdsr(engine_index, filter_adsr));
    }
}

fn draw_lfo_controls(app: &mut CypherApp, ui: &mut Ui, engine_index: usize, lfo_num: usize) {
    let theme = app.theme.synth_editor_window.clone();
    let mut changed = false;
    let mut ui_fn = |lfo: &mut RwLockWriteGuard<crate::synth::LfoSettings>, is_wavetable: bool| {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Shape:").color(theme.label_color));
            ui.scope(|ui| {
                let visuals = &mut ui.style_mut().visuals.widgets;
                visuals.inactive.bg_fill = theme.control_bg;
                visuals.hovered.bg_fill = theme.control_hover_bg;
                visuals.active.bg_fill = theme.control_hover_bg;

                let initial_waveform = lfo.waveform;
                ComboBox::from_id_salt(format!("lfo_shape_combo_{}_{}", engine_index, lfo_num))
                    .selected_text(lfo.waveform.to_string())
                    .show_ui(ui, |ui| {
                        let style = ui.style_mut();
                        style.visuals.panel_fill = theme.combo_popup_bg;
                        style.visuals.selection.bg_fill = theme.combo_selection_bg;
                        for shape in LfoWaveform::ALL {
                            if !is_wavetable
                                && matches!(
                                    shape,
                                    LfoWaveform::Wavetable1
                                        | LfoWaveform::Wavetable2
                                        | LfoWaveform::Wavetable3
                                        | LfoWaveform::Wavetable4
                                )
                            {
                                ui.add_enabled(
                                    false,
                                    egui::Button::new(shape.to_string()).selected(false),
                                );
                            } else {
                                if ui.add(egui::Button::new(shape.to_string()).selected(lfo.waveform == shape))
                                    .clicked()
                                {
                                    lfo.waveform = shape;
                                }
                            }
                        }
                    });
                if initial_waveform != lfo.waveform {
                    changed = true;
                }
            });
            if ui.toggle_value(&mut lfo.retrigger, RichText::new("Retrigger").color(theme.label_color))
                .changed()
            {
                changed = true;
            }
        });
        ui.horizontal(|ui| {
            ui.label(RichText::new("Rate:").color(theme.label_color));
            ui.scope(|ui| {
                let visuals = &mut ui.style_mut().visuals.widgets;
                visuals.inactive.bg_fill = theme.control_bg;
                visuals.hovered.bg_fill = theme.control_hover_bg;
                visuals.active.bg_fill = theme.control_hover_bg;

                match lfo.mode {
                    LfoRateMode::Hz => {
                        if ui.add(
                            egui::DragValue::new(&mut lfo.hz_rate)
                                .speed(0.01)
                                .range(0.01..=20.0),
                        )
                            .changed()
                        {
                            changed = true;
                        }
                    }
                    LfoRateMode::Sync => {
                        const TRP: f32 = 2.0 / 3.0;
                        const DOT: f32 = 1.5;
                        let rates = [
                            (32.0, "1/128"),
                            (16.0 * DOT, "1/64d"),
                            (16.0, "1/64"),
                            (16.0 * TRP, "1/64t"),
                            (8.0 * DOT, "1/32d"),
                            (8.0, "1/32"),
                            (8.0 * TRP, "1/32t"),
                            (4.0 * DOT, "1/16d"),
                            (4.0, "1/16"),
                            (4.0 * TRP, "1/16t"),
                            (2.0 * DOT, "1/8d"),
                            (2.0, "1/8"),
                            (2.0 * TRP, "1/8t"),
                            (1.0 * DOT, "1/4d"),
                            (1.0, "1/4"),
                            (1.0 * TRP, "1/4t"),
                            (0.5 * DOT, "1/2d"),
                            (0.5, "1/2"),
                            (0.5 * TRP, "1/2t"),
                            (0.25, "1 bar"),
                        ];
                        let current_label = rates
                            .iter()
                            .find(|(r, _)| (*r - lfo.sync_rate).abs() < 1e-6)
                            .map_or_else(|| lfo.sync_rate.to_string(), |(_, l)| l.to_string());
                        let initial_rate = lfo.sync_rate;
                        ComboBox::from_id_salt(format!("lfo_sync_rate_{}_{}", engine_index, lfo_num))
                            .selected_text(current_label)
                            .show_ui(ui, |ui| {
                                let style = ui.style_mut();
                                style.visuals.panel_fill = theme.combo_popup_bg;
                                style.visuals.selection.bg_fill = theme.combo_selection_bg;
                                for (rate_val, rate_label) in rates {
                                    if ui.add(egui::Button::new(rate_label).selected(
                                        lfo.sync_rate == rate_val,
                                    ))
                                        .clicked()
                                    {
                                        lfo.sync_rate = rate_val;
                                    }
                                }
                            });
                        if initial_rate != lfo.sync_rate {
                            changed = true;
                        }
                    }
                }
            });

            ui.scope(|ui| {
                let visuals = &mut ui.style_mut().visuals;
                visuals.widgets.inactive.bg_fill = theme.control_bg;
                visuals.selection.bg_fill = theme.button_active_bg; // Use button color for selection

                if ui.add(egui::Button::new("Hz").selected(lfo.mode == LfoRateMode::Hz).frame(true))
                    .clicked()
                {
                    lfo.mode = LfoRateMode::Hz;
                    changed = true;
                }
                if ui.add(egui::Button::new("Sync").selected(lfo.mode == LfoRateMode::Sync).frame(true))
                    .clicked()
                {
                    lfo.mode = LfoRateMode::Sync;
                    changed = true;
                }
            });
        });
    };
    match &mut app.engine_states[engine_index] {
        EngineState::Wavetable(s) => {
            let settings = if lfo_num == 1 {
                &s.lfo_settings
            } else {
                &s.lfo2_settings
            };
            if let Ok(mut lfo) = settings.write() {
                ui_fn(&mut lfo, true);
            }
            if changed {
                s.force_redraw_generation += 1;
            }
        }
        EngineState::Sampler(s) => {
            let settings = if lfo_num == 1 {
                &s.lfo_settings
            } else {
                &s.lfo2_settings
            };
            if let Ok(mut lfo) = settings.write() {
                ui_fn(&mut lfo, false);
            }
        }
    };
}

fn draw_mod_matrix_controls(app: &mut CypherApp, ui: &mut Ui, engine_index: usize) {
    let theme = app.theme.synth_editor_window.clone();

    let mut ui_fn = |matrix: &mut RwLockWriteGuard<Vec<ModRouting>>, is_wavetable: bool| -> bool {
        let mut to_remove = None;
        let mut matrix_changed = false;

        for (i, routing) in matrix.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                let initial_routing = routing.clone();

                ui.scope(|ui| {
                    let visuals = &mut ui.style_mut().visuals.widgets;
                    visuals.inactive.bg_fill = theme.control_bg;
                    visuals.hovered.bg_fill = theme.control_hover_bg;
                    visuals.active.bg_fill = theme.control_hover_bg;

                    let source_text = if let ModSource::MidiCC(id) = routing.source {
                        format!("MIDI CC {} (Ch {})", id.cc, id.channel + 1)
                    } else {
                        routing.source.to_string()
                    };

                    ComboBox::new(format!("source_{}_{}", engine_index, i), "")
                        .selected_text(source_text)
                        .show_ui(ui, |ui| {
                            let style = ui.style_mut();
                            style.visuals.panel_fill = theme.combo_popup_bg;
                            style.visuals.selection.bg_fill = theme.combo_selection_bg;

                            for source in ModSource::ALL {
                                if ui.add(egui::Button::new(source.to_string()).selected(
                                    routing.source == source,
                                ))
                                    .clicked()
                                {
                                    routing.source = source;
                                }
                            }
                        });

                    ui.label(RichText::new("->").color(theme.label_color));

                    ComboBox::new(format!("dest_{}_{}", engine_index, i), "")
                        .selected_text(routing.destination.to_string())
                        .show_ui(ui, |ui| {
                            let style = ui.style_mut();
                            style.visuals.panel_fill = theme.combo_popup_bg;
                            style.visuals.selection.bg_fill = theme.combo_selection_bg;
                            for dest in ModDestination::ALL {
                                let is_wt_dest = matches!(
                                    dest,
                                    ModDestination::WavetablePosition
                                        | ModDestination::BellPosition
                                        | ModDestination::BellAmount
                                        | ModDestination::BellWidth
                                );
                                if !is_wavetable && is_wt_dest {
                                    ui.add_enabled(
                                        false,
                                        egui::Button::new(dest.to_string()).selected(false),
                                    );
                                } else {
                                    if ui.add(egui::Button::new(dest.to_string()).selected(
                                        routing.destination == dest,
                                    ))
                                        .clicked()
                                    {
                                        routing.destination = dest;
                                    }
                                }
                            }
                        });
                });

                let is_learning_this = {
                    let learn_target = app.midi_mod_matrix_learn_target.read().unwrap();
                    *learn_target == Some((engine_index, i))
                };
                let learn_button_text = if is_learning_this { "Listening..." } else { "Learn" };
                let learn_button = Button::new(learn_button_text).fill(if is_learning_this {
                    theme.button_active_bg
                } else {
                    theme.button_bg
                });

                if ui.add(learn_button).clicked() {
                    let mut learn_target = app.midi_mod_matrix_learn_target.write().unwrap();
                    *learn_target = if is_learning_this {
                        None
                    } else {
                        *app.last_learned_mod_source.write().unwrap() = None;
                        Some((engine_index, i))
                    };
                }

                ui.scope(|ui| {
                    ui.style_mut().visuals.slider_trailing_fill = true;
                    let visuals = &mut ui.style_mut().visuals.widgets;
                    visuals.inactive.bg_fill = theme.slider_track_color;
                    visuals.hovered.bg_fill = theme.slider_grab_hover_color;
                    visuals.active.bg_fill = theme.slider_grab_color;

                    ui.add(Slider::new(&mut routing.amount, -1.0..=1.0).text(RichText::new("Amount").color(theme.label_color)));
                });

                if custom_button(ui, "x", &theme).clicked() {
                    to_remove = Some(i);
                }

                if *routing != initial_routing {
                    matrix_changed = true;
                }
            });
        }
        if let Some(i) = to_remove {
            matrix.remove(i);
            matrix_changed = true;
        }

        if custom_button(ui, "+ Add Routing", &theme).clicked() {
            if matrix.len() < 16 {
                matrix.push(ModRouting::default());
                matrix_changed = true;
            }
        }
        matrix_changed
    };
    match &mut app.engine_states[engine_index] {
        EngineState::Wavetable(s) => {
            if let Ok(mut matrix) = s.mod_matrix.write() {
                if ui_fn(&mut matrix, true) {
                    s.force_redraw_generation += 1;
                }
            }
        }
        EngineState::Sampler(s) => {
            if let Ok(mut matrix) = s.mod_matrix.write() {
                ui_fn(&mut matrix, false);
            }
        }
    };
}

fn draw_wavetable_preview(app: &mut CypherApp, ui: &mut Ui, rect: Rect, engine_index: usize) {
    if let EngineState::Wavetable(engine_state) = &mut app.engine_states[engine_index] {
        let painter = ui.painter_at(rect);
        let theme = &app.theme.synth_editor_window;

        // --- Check cache. If valid, draw cached shapes and exit early. ---
        let current_snapshot = engine_state.get_visualizer_snapshot();
        if current_snapshot == engine_state.last_snapshot
            && rect == engine_state.last_visualizer_rect
            && !engine_state.visualizer_cache.is_empty()
        {
            painter.extend(engine_state.visualizer_cache.clone());
            return; // Cache is valid, nothing more to do.
        }
        // Invalidate cache and prepare for redraw
        engine_state.last_snapshot = current_snapshot;
        engine_state.last_visualizer_rect = rect;
        engine_state.visualizer_cache.clear();

        let background = Shape::Rect(RectShape::new(
            rect,
            CornerRadius::ZERO,
            theme.visualizer_bg,
            Stroke::NONE,
            StrokeKind::Inside,
        ));
        engine_state.visualizer_cache.push(background);

        if let Ok(guard) = engine_state.wavetable_set.read() {
            let tables = &guard.tables;
            if tables.is_empty() {
                painter.extend(engine_state.visualizer_cache.clone());
                return;
            }

            // --- Read final, fully modulated values from the audio thread ---
            let final_pos =
                engine_state.final_wt_pos_atomic.load(Ordering::Relaxed) as f32 / 1_000_000.0;
            let cutoff_norm =
                engine_state.final_cutoff_atomic.load(Ordering::Relaxed) as f32 / 1_000_000.0;

            // Unpack modulation values from 0..1 range back to -1..1 range
            let pitch_mod =
                (engine_state.pitch_mod_atomic.load(Ordering::Relaxed) as f32 / 1_000_000.0) * 2.0
                    - 1.0;
            let bell_pos_mod =
                (engine_state.bell_pos_atomic.load(Ordering::Relaxed) as f32 / 1_000_000.0) * 2.0
                    - 1.0;
            let bell_amount_mod =
                (engine_state.bell_amount_atomic.load(Ordering::Relaxed) as f32 / 1_000_000.0)
                    * 2.0
                    - 1.0;
            let bell_width_mod =
                (engine_state.bell_width_atomic.load(Ordering::Relaxed) as f32 / 1_000_000.0)
                    * 2.0
                    - 1.0;

            // Saturation is 0..1
            let saturation_drive_mod =
                engine_state.saturation_mod_atomic.load(Ordering::Relaxed) as f32 / 1_000_000.0;

            let num_tables_f = tables.len().max(1) as f32;
            const NUM_POINTS: usize = 256;
            const WAVE_HEIGHT_SCALE: f32 = 0.3;

            // --- Draw stacked inactive waveforms ---
            let vertical_spread = 15.0;
            let stack_center_offset = (num_tables_f - 1.0) / 2.0 * vertical_spread;
            for i in 0..tables.len() {
                let y_offset = (i as f32 * vertical_spread) - stack_center_offset;
                let distance = (i as f32 - final_pos).abs();
                let alpha = (1.0 - (distance / num_tables_f.max(1.0)).powi(2)).clamp(0.1, 1.0);
                let color = if distance < 1.0 {
                    theme.wt_preview_active_waveform_color
                } else {
                    theme.wt_preview_inactive_waveform_color
                }
                    .linear_multiply(alpha);
                let stroke = Stroke::new(1.0, color);
                let mut points = Vec::with_capacity(NUM_POINTS);
                for p_idx in 0..NUM_POINTS {
                    let sample_phase = p_idx as f32 / (NUM_POINTS - 1) as f32;
                    let sample = get_waveform_sample(&engine_state.wavetable_set, i, sample_phase);
                    let x = rect.min.x + sample_phase * rect.width();
                    let y = rect.center().y - sample * (rect.height() * WAVE_HEIGHT_SCALE) + y_offset;
                    points.push(egui::pos2(x, y));
                }
                engine_state
                    .visualizer_cache
                    .push(PathShape::line(points, stroke).into());
            }

            // --- Calculate base morphed waveform ---
            let table1_idx = final_pos.floor() as usize;
            let table2_idx = final_pos.ceil() as usize;
            let morph_frac = final_pos.fract();
            let final_y_offset = (final_pos * vertical_spread) - stack_center_offset;
            let mut morphed_points = Vec::with_capacity(NUM_POINTS);
            for p_idx in 0..NUM_POINTS {
                let sample_phase = p_idx as f32 / (NUM_POINTS - 1) as f32;
                let sample1 = get_waveform_sample(&engine_state.wavetable_set, table1_idx, sample_phase);
                let sample2 = get_waveform_sample(&engine_state.wavetable_set, table2_idx, sample_phase);
                let final_sample = lerp(sample1..=sample2, morph_frac);
                let x = rect.min.x + sample_phase * rect.width();
                let y =
                    rect.center().y - final_sample * (rect.height() * WAVE_HEIGHT_SCALE) + final_y_offset;
                morphed_points.push(pos2(x, y));
            }

            // --- Calculate final waveform with bell filter applied ---
            let bell_pos = (bell_pos_mod * 0.5 + 0.5).clamp(0.0, 1.0);
            let sigma = (0.15 * (2.0f32).powf(-2.0 * bell_width_mod)).clamp(0.02, 1.0);
            let mut bell_filtered_points = Vec::with_capacity(NUM_POINTS);

            for (p_idx, point) in morphed_points.iter().enumerate() {
                let phase_norm = p_idx as f32 / (NUM_POINTS - 1) as f32;
                let y_norm =
                    (rect.center().y - point.y + final_y_offset) / (rect.height() * WAVE_HEIGHT_SCALE);
                let bell_shape = (-((phase_norm - bell_pos).powi(2)) / (2.0 * sigma.powi(2))).exp();
                let bell_effect = bell_shape * bell_amount_mod;
                let bell_filtered_sample = y_norm * (1.0 + bell_effect);
                let new_y = rect.center().y
                    - bell_filtered_sample * (rect.height() * WAVE_HEIGHT_SCALE)
                    + final_y_offset;
                bell_filtered_points.push(pos2(point.x, new_y));
            }

            // --- Draw Visualizations ---

            // Amplitude bars (colored by saturation)
            let drive_level = saturation_drive_mod;
            let cold = theme.mod_amp_cold_color;
            let hot = theme.mod_amp_hot_color;
            let lerped_color = Color32::from_rgba_unmultiplied(
                lerp(cold.r() as f32..=hot.r() as f32, drive_level) as u8,
                lerp(cold.g() as f32..=hot.g() as f32, drive_level) as u8,
                lerp(cold.b() as f32..=hot.b() as f32, drive_level) as u8,
                150,
            );
            let bar_stroke = Stroke::new(1.0, lerped_color);
            for p in &bell_filtered_points {
                let original_amplitude = (rect.center().y - p.y).abs();
                let bar_top_y = rect.bottom() - original_amplitude;
                engine_state.visualizer_cache.push(Shape::line_segment(
                    [pos2(p.x, rect.bottom()), pos2(p.x, bar_top_y)],
                    bar_stroke,
                ));
            }

            // Pitch indicator
            let pitch_y_offset = pitch_mod * -15.0;
            let pitch_indicator_center = pos2(rect.left() + 10.0, rect.center().y + pitch_y_offset);
            let pitch_color = theme.mod_pitch_color;
            let p1 = pitch_indicator_center + Vec2::new(-4.0, -3.0);
            let p2 = pitch_indicator_center + Vec2::new(4.0, 0.0);
            let p3 = pitch_indicator_center + Vec2::new(-4.0, 3.0);
            engine_state
                .visualizer_cache
                .push(PathShape::line(vec![p1, p2, p3], Stroke::new(2.0, pitch_color)).into());

            // Waveform lines over the bars
            engine_state.visualizer_cache.push(
                PathShape::line(
                    morphed_points,
                    Stroke::new(1.5, theme.wt_preview_final_waveform_color),
                )
                    .into(),
            );
            engine_state.visualizer_cache.push(
                PathShape::line(
                    bell_filtered_points.clone(),
                    Stroke::new(2.0, theme.wt_preview_bell_filtered_waveform_color),
                )
                    .into(),
            );

            // Filter overlay
            let filter_mod_color = theme.mod_filter_color;
            let filter_x = rect.left() + rect.width() * cutoff_norm;
            let bar_color = Color32::from_rgba_unmultiplied(
                filter_mod_color.r(),
                filter_mod_color.g(),
                filter_mod_color.b(),
                40,
            );
            let bar_stroke = Stroke::new(1.0, bar_color);
            for p in &bell_filtered_points {
                if p.x > filter_x {
                    engine_state
                        .visualizer_cache
                        .push(Shape::line_segment([pos2(p.x, rect.top()), pos2(p.x, p.y)], bar_stroke));
                }
            }
        }
        painter.extend(engine_state.visualizer_cache.clone());
    }
}

fn draw_sampler_waveform_preview(app: &mut CypherApp, ui: &mut Ui, rect: Rect, engine_index: usize) {
    if let EngineState::Sampler(engine_state) = &mut app.engine_states[engine_index] {
        let painter = ui.painter_at(rect);
        let theme = &app.theme.synth_editor_window;

        // --- Check cache. If valid, draw cached shapes and exit early. ---
        let current_snapshot = engine_state.get_visualizer_snapshot();
        if current_snapshot == engine_state.last_snapshot
            && rect == engine_state.last_visualizer_rect
            && !engine_state.visualizer_cache.is_empty()
        {
            painter.extend(engine_state.visualizer_cache.clone());
            return;
        }
        // Invalidate cache and prepare for redraw
        engine_state.last_snapshot = current_snapshot;
        engine_state.last_visualizer_rect = rect;
        engine_state.visualizer_cache.clear();

        let background = Shape::Rect(RectShape::new(
            rect,
            CornerRadius::ZERO,
            theme.visualizer_bg,
            Stroke::NONE,
            StrokeKind::Inside,
        ));
        engine_state.visualizer_cache.push(background);

        // --- KEY CHANGE: Use the atomic to decide which waveform to show ---
        let slot_to_draw = engine_state
            .last_triggered_slot_index
            .load(Ordering::Relaxed);

        let data_guard_opt = engine_state.sample_data_for_ui
            .get(slot_to_draw)
            .and_then(|rwlock| {
                let guard = rwlock.read().unwrap();
                if !guard.is_empty() { Some(guard) } else { None }
            })
            // Fallback to the first loaded sample if the last triggered one is somehow empty or uninitialized
            .or_else(|| {
                engine_state.sample_data_for_ui.iter().find_map(|rwlock| {
                    let guard = rwlock.read().unwrap();
                    if !guard.is_empty() {
                        Some(guard)
                    } else {
                        None
                    }
                })
            });

        // --- If a sample is loaded, draw it. Otherwise, show text. ---
        if let Some(data) = data_guard_opt {
            // --- Read final, fully modulated values from the audio thread ---
            let pitch_mod = (engine_state.pitch_mod_atomic.load(Ordering::Relaxed) as f32
                / 1_000_000.0)
                * 2.0
                - 1.0;
            let cutoff_norm =
                engine_state.final_cutoff_atomic.load(Ordering::Relaxed) as f32 / 1_000_000.0;
            let saturation_drive_mod =
                engine_state.saturation_mod_atomic.load(Ordering::Relaxed) as f32 / 1_000_000.0;

            // --- Prepare Waveform Points ---
            let num_points = (rect.width() as usize).min(data.len());
            if num_points == 0 {
                painter.extend(engine_state.visualizer_cache.clone());
                return;
            }

            let step = data.len() as f32 / num_points as f32;
            let fade_out_clamped = engine_state.fade_out.clamp(0.0, 0.5);
            let fade_start_norm = 1.0 - fade_out_clamped;
            let fade_start_point_idx = (fade_start_norm * num_points as f32) as usize;

            let mut points = Vec::with_capacity(num_points);
            for p_idx in 0..num_points {
                let sample_idx = (p_idx as f32 * step).floor() as usize;
                let mut sample = data.get(sample_idx).copied().unwrap_or(0.0);
                if p_idx >= fade_start_point_idx && fade_out_clamped > 0.0 {
                    let progress_in_fade = (p_idx - fade_start_point_idx) as f32
                        / (num_points - fade_start_point_idx) as f32;
                    let gain = (1.0 - progress_in_fade).clamp(0.0, 1.0);
                    sample *= gain;
                }
                let x = rect.min.x
                    + (p_idx as f32 / (num_points - 1).max(1) as f32) * rect.width();
                let y = rect.center().y - sample * (rect.height() * 0.45);
                points.push(egui::pos2(x, y));
            }

            // --- Draw Visualizations into cache ---
            let drive_level = saturation_drive_mod;
            let cold = theme.mod_amp_cold_color;
            let hot = theme.mod_amp_hot_color;
            let lerped_color = Color32::from_rgba_unmultiplied(
                lerp(cold.r() as f32..=hot.r() as f32, drive_level) as u8,
                lerp(cold.g() as f32..=hot.g() as f32, drive_level) as u8,
                lerp(cold.b() as f32..=hot.b() as f32, drive_level) as u8,
                150,
            );
            let bar_stroke = Stroke::new(1.0, lerped_color);
            for p in &points {
                let original_amplitude = (rect.center().y - p.y).abs();
                let bar_top_y = rect.bottom() - original_amplitude;
                engine_state.visualizer_cache.push(Shape::line_segment(
                    [pos2(p.x, rect.bottom()), pos2(p.x, bar_top_y)],
                    bar_stroke,
                ));
            }

            let pitch_y_offset = pitch_mod * -15.0;
            let pitch_indicator_center =
                pos2(rect.left() + 10.0, rect.center().y + pitch_y_offset);
            let pitch_color = theme.mod_pitch_color;
            let p1 = pitch_indicator_center + Vec2::new(-4.0, -3.0);
            let p2 = pitch_indicator_center + Vec2::new(4.0, 0.0);
            let p3 = pitch_indicator_center + Vec2::new(-4.0, 3.0);
            engine_state
                .visualizer_cache
                .push(PathShape::line(vec![p1, p2, p3], Stroke::new(2.0, pitch_color)).into());

            let stroke = Stroke::new(1.5, theme.wt_preview_final_waveform_color);
            engine_state
                .visualizer_cache
                .push(PathShape::line(points.clone(), stroke).into());

            let filter_mod_color = theme.mod_filter_color;
            let filter_x = rect.left() + rect.width() * cutoff_norm;
            let bar_color = Color32::from_rgba_unmultiplied(
                filter_mod_color.r(),
                filter_mod_color.g(),
                filter_mod_color.b(),
                40,
            );
            let bar_stroke = Stroke::new(1.0, bar_color);
            for p in &points {
                if p.x > filter_x {
                    engine_state.visualizer_cache.push(Shape::line_segment(
                        [pos2(p.x, rect.top()), pos2(p.x, p.y)],
                        bar_stroke,
                    ));
                }
            }
        } else {
            let text_shape = Shape::text(
                &painter.fonts(|f| f.clone()),
                rect.center(),
                Align2::CENTER_CENTER,
                "Load a sample",
                egui::FontId::proportional(14.0),
                theme.label_color,
            );
            engine_state.visualizer_cache.push(text_shape);
        }

        // --- Finally, paint the newly cached shapes ---
        painter.extend(engine_state.visualizer_cache.clone());
    }
}

fn get_waveform_sample(
    wavetable_set: &Arc<RwLock<WavetableSet>>,
    table_idx: usize,
    phase: f32,
) -> f32 {
    if let Ok(guard) = wavetable_set.read() {
        if let Some(table) = guard.tables.get(table_idx) {
            let table_data = &table.table;
            let table_len = table_data.len();
            if table_len == 0 {
                return 0.0;
            }
            let index = phase * (table_len - 1) as f32;
            let idx_floor = index.floor() as usize;
            let idx_ceil = (idx_floor + 1).min(table_len - 1);
            let frac = index.fract();
            let val1 = table_data.get(idx_floor).copied().unwrap_or(0.0);
            let val2 = table_data.get(idx_ceil).copied().unwrap_or(0.0);
            return val1 * (1.0 - frac) + val2 * frac;
        }
    }
    0.0
}