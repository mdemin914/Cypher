use crate::app::{CypherApp, LibraryView};
use crate::asset::{Asset, AssetRef, SampleRef};
use crate::audio_engine::AudioCommand;
use crate::sampler::{SamplerKit, SamplerPadFxSettings, SamplerPadSettings};
use crate::settings;
use crate::synth::AdsrSettings;
use crate::ui;
use egui::{
    epaint, vec2, Align, Align2, Button, Color32, DragAndDrop, Frame, Grid, Id, Layout, Margin,
    Response, RichText, Rounding, ScrollArea, Sense, Slider, Stroke, Ui, Window,
};
use rfd::FileDialog;
use std::cmp::max;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

// A small distance threshold. If the user presses and releases within this distance,
// it's considered a click, even if they wiggled their finger slightly.
const CLICK_DRAG_THRESHOLD: f32 = 5.0;
const PAD_FLASH_DURATION: Duration = Duration::from_millis(150);

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

fn slider_to_time(value: f32, max_time: f32) -> f32 {
    value.powf(4.0) * max_time
}


pub fn draw_library_panel(app: &mut CypherApp, ui: &mut Ui) {
    let frame = Frame::none()
        .fill(app.theme.library.panel_background)
        .inner_margin(Margin { left: 0, right: 0, top: 8, bottom: 8 });

    frame.show(ui, |ui| {
        ui.set_min_height(250.0);
        // --- TOP TOOLBAR ---
        ui.horizontal(|ui| {
            let button_min_size = vec2(80.0, 50.0);

            let rescan_button = Button::new("Rescan Library")
                .min_size(button_min_size)
                .fill(app.theme.library.button_bg);
            if ui.add(rescan_button).clicked() {
                app.rescan_asset_library();
                app.rescan_chord_styles();
            }
            ui.separator();

            // Tab Buttons
            let sample_bg = if app.library_view == LibraryView::Samples { app.theme.library.tab_active_bg } else { app.theme.library.tab_inactive_bg };
            let samples_button = Button::new("Samples").min_size(button_min_size).fill(sample_bg);
            if ui.add(samples_button).clicked() { app.library_view = LibraryView::Samples; app.library_path.clear(); }

            let synth_bg = if app.library_view == LibraryView::Synths { app.theme.library.tab_active_bg } else { app.theme.library.tab_inactive_bg };
            let synths_button = Button::new("Synths").min_size(button_min_size).fill(synth_bg);
            if ui.add(synths_button).clicked() { app.library_view = LibraryView::Synths; app.library_path.clear(); }

            let kit_bg = if app.library_view == LibraryView::Kits { app.theme.library.tab_active_bg } else { app.theme.library.tab_inactive_bg };
            let kits_button = Button::new("Kits").min_size(button_min_size).fill(kit_bg);
            if ui.add(kits_button).clicked() { app.library_view = LibraryView::Kits; app.library_path.clear(); }

            let session_bg = if app.library_view == LibraryView::Sessions { app.theme.library.tab_active_bg } else { app.theme.library.tab_inactive_bg };
            let sessions_button = Button::new("Sessions").min_size(button_min_size).fill(session_bg);
            if ui.add(sessions_button).clicked() { app.library_view = LibraryView::Sessions; app.library_path.clear(); }

            let keys_bg = if app.library_view == LibraryView::EightyEightKeys { app.theme.library.tab_active_bg } else { app.theme.library.tab_inactive_bg };
            let keys_button = Button::new("88Keys").min_size(button_min_size).fill(keys_bg);
            if ui.add(keys_button).clicked() { app.library_view = LibraryView::EightyEightKeys; app.library_path.clear(); }


            // --- Path/Back button only for asset views ---
            if app.library_view != LibraryView::EightyEightKeys {
                ui.separator();

                if !app.library_path.is_empty() {
                    let back_button = Button::new("⬅ Back")
                        .min_size(button_min_size)
                        .fill(app.theme.library.button_bg);
                    if ui.add(back_button).clicked() {
                        app.library_path.pop();
                    }
                }
                let path_str = if app.library_path.is_empty() {
                    match app.library_view {
                        LibraryView::Samples => "Samples",
                        LibraryView::Synths => "Synths",
                        LibraryView::Kits => "Kits",
                        LibraryView::Sessions => "Sessions",
                        _ => "", // Should not happen
                    }.to_string()
                } else {
                    app.library_path.join(" / ")
                };
                ui.label(RichText::new(format!("Browsing: {}", path_str)).color(app.theme.library.text_color));
            }
        });
        ui.separator();

        // --- MAIN CONTENT AREA ---
        if app.library_view == LibraryView::EightyEightKeys {
            ScrollArea::vertical().auto_shrink([false, true]).show(ui, |ui| {
                ui::eighty_eight_keys_view::draw_88_keys_panel(app, ui);
            });
        } else {
            let category_root = match app.library_view {
                LibraryView::Samples => &app.asset_library.sample_root,
                LibraryView::Synths => &app.asset_library.synth_root,
                LibraryView::Kits => &app.asset_library.kit_root,
                LibraryView::Sessions => &app.asset_library.session_root,
                _ => return,
            };
            let mut current_folder = category_root;
            for segment in &app.library_path {
                if let Some(folder) = current_folder.subfolders.get(segment) {
                    current_folder = folder;
                } else {
                    app.library_path.clear();
                    current_folder = category_root;
                    break;
                }
            }

            let mut preset_to_load: Option<PathBuf> = None;
            let mut kit_to_load: Option<PathBuf> = None;
            let mut session_to_load: Option<PathBuf> = None;
            let theme = app.theme.clone();

            ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
                const CARD_WIDTH: f32 = 100.0;
                const SPACING: f32 = 20.0;
                const SCROLL_RESERVATION_WIDTH: f32 = CARD_WIDTH * 2.0 + SPACING;

                let total_items = current_folder.subfolders.len() + current_folder.assets.len();
                if total_items == 0 {
                    return;
                }

                // Calculate number of columns based on available width, reserving space on the right
                let available_width = ui.available_width();
                let effective_width = (available_width - SCROLL_RESERVATION_WIDTH).max(CARD_WIDTH);
                let num_cols = max(1, ((effective_width + SPACING) / (CARD_WIDTH + SPACING)).floor() as usize);
                let mut item_index = 0;

                egui::Grid::new("responsive_asset_grid")
                    .spacing([SPACING, SPACING])
                    .show(ui, |ui| {
                        for folder_name in current_folder.subfolders.keys() {
                            let response = draw_folder_card(ui, folder_name, &theme);

                            let is_clicked = response.clicked()
                                || (response.drag_stopped() && response.drag_delta().length() < CLICK_DRAG_THRESHOLD);

                            if is_clicked {
                                app.library_path.push(folder_name.clone());
                            }
                            item_index += 1;
                            if item_index % num_cols == 0 {
                                ui.end_row();
                            }
                        }

                        for asset in &current_folder.assets {
                            let response = match asset {
                                Asset::Sample(sample_ref) => {
                                    draw_asset_card(ui, sample_ref, "🎵", asset.clone(), Sense::drag(), &theme)
                                }
                                Asset::SynthPreset(preset_ref) => {
                                    draw_asset_card(ui, preset_ref, "🎹", asset.clone(), Sense::click_and_drag(), &theme)
                                }
                                Asset::SamplerKit(kit_ref) => {
                                    draw_asset_card(ui, kit_ref, "🥁", asset.clone(), Sense::click_and_drag(), &theme)
                                }
                                Asset::Session(session_ref) => {
                                    draw_asset_card(ui, session_ref, "💾", asset.clone(), Sense::click_and_drag(), &theme)
                                }
                            };

                            let is_clicked = response.clicked()
                                || (response.drag_stopped() && response.drag_delta().length() < CLICK_DRAG_THRESHOLD);

                            if is_clicked {
                                match asset {
                                    Asset::SynthPreset(preset_ref) => preset_to_load = Some(preset_ref.path().clone()),
                                    Asset::SamplerKit(kit_ref) => kit_to_load = Some(kit_ref.path().clone()),
                                    Asset::Session(session_ref) => session_to_load = Some(session_ref.path().clone()),
                                    _ => {}
                                }
                            }

                            item_index += 1;
                            if item_index % num_cols == 0 {
                                ui.end_row();
                            }
                        }
                    });
            });

            if let Some(path) = preset_to_load {
                app.load_preset_from_path(&path);
                app.send_command(AudioCommand::ActivateSynth);
                app.send_command(AudioCommand::DeactivateSampler);
            }
            if let Some(path) = kit_to_load {
                app.load_kit(&path);
                app.send_command(AudioCommand::ActivateSampler);
                app.send_command(AudioCommand::DeactivateSynth);
            }
            if let Some(path) = session_to_load {
                app.load_session(&path);
            }
        }
    });
}

fn draw_folder_card(ui: &mut Ui, name: &str, theme: &crate::theme::Theme) -> Response {
    let size = vec2(100.0, 80.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());

    if ui.is_rect_visible(rect) {
        let visuals = ui.style().interact(&response);
        let fill_color = if response.hovered() || response.is_pointer_button_down_on() {
            theme.library.card_hovered_bg
        } else {
            theme.library.card_bg
        };
        let frame = Frame::group(ui.style()).rounding(visuals.rounding()).fill(fill_color).stroke(visuals.bg_stroke);
        ui.painter().add(frame.paint(rect));

        let icon_galley = ui.painter().layout_no_wrap("📁".to_string(), egui::FontId::proportional(32.0), theme.library.text_color);
        let name_galley = ui.painter().layout(name.to_string(), egui::FontId::monospace(12.0), theme.library.text_color, rect.width() - 8.0);
        let icon_pos = egui::pos2(rect.center().x - icon_galley.size().x / 2.0, rect.top() + 12.0);
        let name_pos = egui::pos2(rect.center().x - name_galley.size().x / 2.0, rect.bottom() - name_galley.size().y - 8.0);
        ui.painter().galley(icon_pos, icon_galley, theme.library.text_color);
        ui.painter().galley(name_pos, name_galley, theme.library.text_color);
    }
    response
}

fn draw_asset_card(
    ui: &mut Ui,
    asset_ref: &impl AssetRef,
    icon: &str,
    asset_payload: Asset,
    sense: Sense,
    theme: &crate::theme::Theme,
) -> Response {
    let size = vec2(100.0, 80.0);
    let (rect, response) = ui.allocate_exact_size(size, sense);

    if response.drag_started() {
        DragAndDrop::set_payload(ui.ctx(), asset_payload);
    }

    if ui.is_rect_visible(rect) {
        let visuals = ui.style().interact(&response);
        let fill_color = if response.hovered() || response.is_pointer_button_down_on() {
            theme.library.card_hovered_bg
        } else {
            theme.library.card_bg
        };
        let frame = Frame::group(ui.style()).rounding(visuals.rounding()).fill(fill_color).stroke(visuals.bg_stroke);
        ui.painter().add(frame.paint(rect));

        let icon_galley = ui.painter().layout_no_wrap(icon.to_string(), egui::FontId::proportional(32.0), theme.library.text_color);
        let name_galley = ui.painter().layout(asset_ref.name().to_string(), egui::FontId::monospace(12.0), theme.library.text_color, rect.width() - 8.0);
        let icon_pos = egui::pos2(rect.center().x - icon_galley.size().x / 2.0, rect.top() + 12.0);
        let name_pos = egui::pos2(rect.center().x - name_galley.size().x / 2.0, rect.bottom() - name_galley.size().y - 8.0);
        ui.painter().galley(icon_pos, icon_galley, theme.library.text_color);
        ui.painter().galley(name_pos, name_galley, theme.library.text_color);
    }
    response
}

pub fn draw_sample_pad_window(app: &mut CypherApp, ctx: &egui::Context) {
    let mut is_open = app.sample_pad_window_open;
    Window::new("Sample Pads")
        .open(&mut is_open)
        .frame(Frame::window(&ctx.style()).fill(app.theme.sampler_pad_window.background))
        .default_size([650.0, 750.0])
        .resizable(true)
        .pivot(Align2::CENTER_CENTER)
        .default_pos(ctx.screen_rect().center())
        .show(ctx, |ui| {
            let editor_state_id = Id::new("active_pad_editor");
            let mut active_pad_editor = ui.memory_mut(|m| *m.data.get_temp_mut_or_default::<Option<usize>>(editor_state_id));
            let flash_timers_id = Id::new("pad_flash_timers");
            let mut flash_timers = ui.memory_mut(|m| *m.data.get_temp_mut_or_default::<[Option<Instant>; 16]>(flash_timers_id));
            let trash_mode_id = Id::new("trash_mode");
            let mut trash_mode = ui.memory_mut(|m| *m.data.get_temp_mut_or_default(trash_mode_id));

            while let Some(triggered_pad_index) = app.pad_event_consumer.pop() {
                if triggered_pad_index < 16 {
                    active_pad_editor = Some(triggered_pad_index);
                    flash_timers[triggered_pad_index] = Some(Instant::now());
                }
            }

            ui.horizontal(|ui| {
                let trash_button_text = RichText::new("🗑 Trash Mode").monospace();
                let trash_button = egui::Button::new(trash_button_text).fill(if trash_mode { app.theme.sampler_pad_window.trash_mode_active_bg } else { app.theme.sampler_pad_window.kit_button_bg });
                if ui.add(trash_button).clicked() {
                    trash_mode = !trash_mode;
                }
                ui.separator();

                let save_button = Button::new("Save Kit").fill(app.theme.sampler_pad_window.kit_button_bg);
                if ui.add(save_button).clicked() {
                    if let Some(config_dir) = settings::get_config_dir() {
                        let kits_dir = config_dir.join("Kits");
                        if let Some(path) = FileDialog::new().add_filter("json", &["json"]).set_directory(&kits_dir).save_file() {
                            let pads = std::array::from_fn(|i| {
                                let path = app.sampler_pad_info[i].as_ref().map(|s_ref| {
                                    if let Ok(relative_path) = s_ref.path.strip_prefix(&config_dir) {
                                        return relative_path.to_path_buf();
                                    }
                                    s_ref.path.clone()
                                });
                                SamplerPadSettings {
                                    path,
                                    fx: app.sampler_pad_fx_settings[i],
                                }
                            });

                            let kit = SamplerKit { pads };

                            if let Ok(json) = serde_json::to_string_pretty(&kit) {
                                if let Err(e) = fs::write(&path, json) {
                                    eprintln!("Failed to save kit: {}", e);
                                } else {
                                    app.settings.last_sampler_kit = Some(path);
                                    app.rescan_asset_library();
                                }
                            }
                        }
                    }
                }

                let load_button = Button::new("Load Kit").fill(app.theme.sampler_pad_window.kit_button_bg);
                if ui.add(load_button).clicked() {
                    if let Some(config_dir) = settings::get_config_dir() {
                        let kits_dir = config_dir.join("Kits");
                        if let Some(path) = FileDialog::new().add_filter("json", &["json"]).set_directory(kits_dir).pick_file() {
                            app.load_kit(&path);
                        }
                    }
                }
            });
            ui.memory_mut(|m| m.data.insert_temp(trash_mode_id, trash_mode));
            ui.separator();

            let current_playing_mask = app.playing_pads.load(Ordering::Relaxed);
            let spacing = 10.0;
            let pad_size = (ui.available_width() - spacing * 3.0) / 4.0;
            let size_vec = vec2(pad_size, pad_size);
            let mut sample_to_load: Option<(usize, SampleRef)> = None;

            egui::Grid::new("sample_pad_grid").spacing([spacing, spacing]).show(ui, |ui| {
                for i in 0..16 {
                    let visual_row = i / 4;
                    let visual_col = i % 4;
                    let logical_pad_index = (3 - visual_row) * 4 + visual_col;
                    let is_active_editor = active_pad_editor == Some(logical_pad_index);

                    let response = draw_pad(ui, logical_pad_index, app, size_vec, current_playing_mask, &mut flash_timers, trash_mode, is_active_editor);

                    if response.clicked() {
                        if trash_mode {
                            app.send_command(AudioCommand::ClearSample { pad_index: logical_pad_index });
                            app.sampler_pad_info[logical_pad_index] = None;
                            app.sampler_pad_fx_settings[logical_pad_index] = SamplerPadFxSettings::default();
                        } else {
                            if active_pad_editor == Some(logical_pad_index) {
                                active_pad_editor = None;
                            } else {
                                active_pad_editor = Some(logical_pad_index);
                            }
                        }
                    }

                    if !trash_mode {
                        let is_hovered = ui.rect_contains_pointer(response.rect);
                        if is_hovered && ui.input(|i| i.pointer.any_released()) {
                            if let Some(asset) = DragAndDrop::take_payload::<Asset>(ui.ctx()) {
                                if let Asset::Sample(sample_ref) = (*asset).clone() {
                                    sample_to_load = Some((logical_pad_index, sample_ref));
                                }
                            }
                        }
                    }

                    if (i + 1) % 4 == 0 {
                        ui.end_row();
                    }
                }
            });

            if let Some((pad_index, sample_ref)) = sample_to_load {
                app.load_sample_for_pad(pad_index, sample_ref);
                active_pad_editor = Some(pad_index);
            }

            ui.add_space(10.0);

            if let Some(pad_index) = active_pad_editor {
                ui.separator();
                draw_pad_fx_editor(app, ui, pad_index);
            }

            ui.memory_mut(|m| m.data.insert_temp(editor_state_id, active_pad_editor));
            ui.memory_mut(|m| m.data.insert_temp(flash_timers_id, flash_timers));

        });
    app.sample_pad_window_open = is_open;
}

fn draw_pad(
    ui: &mut Ui,
    pad_index: usize,
    app: &CypherApp,
    size: egui::Vec2,
    _playing_mask: u16, // This is now unused for color, but kept for potential future use
    flash_timers: &mut [Option<Instant>; 16],
    trash_mode: bool,
    is_active_editor: bool,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());

    let is_being_dragged_over = ui.rect_contains_pointer(rect) && DragAndDrop::has_any_payload(ui.ctx());

    let mut is_flashing = false;
    if let Some(flash_start) = flash_timers[pad_index] {
        if flash_start.elapsed() < PAD_FLASH_DURATION {
            is_flashing = true;
            ui.ctx().request_repaint(); // Keep repainting until flash is over
        } else {
            flash_timers[pad_index] = None;
        }
    }

    let pad_color = if is_being_dragged_over && !trash_mode {
        ui.style().visuals.selection.bg_fill
    } else {
        app.theme.sampler_pad_window.pad_bg_color
    };

    let visual_row = 3 - (pad_index / 4);

    // CORRECTED LOGIC: The highlight color is now ONLY determined by the short flash timer.
    let outline_color = if is_flashing {
        app.theme.sampler_pad_window.pad_playing_outline_color
    } else if trash_mode && response.hovered() {
        app.theme.sampler_pad_window.pad_trash_hover_outline_color
    } else {
        match visual_row {
            3 => app.theme.sampler_pad_window.pad_outline_row_4_color,
            2 => app.theme.sampler_pad_window.pad_outline_row_3_color,
            1 => app.theme.sampler_pad_window.pad_outline_row_2_color,
            _ => app.theme.sampler_pad_window.pad_outline_row_1_color,
        }
    };

    let stroke_width = if is_active_editor { 4.0 } else { 2.0 };
    ui.painter().rect(rect, Rounding::from(5.0), pad_color, Stroke::new(stroke_width, outline_color), epaint::StrokeKind::Inside);

    if let Some(sample) = &app.sampler_pad_info[pad_index] {
        let text_color = ui.style().visuals.text_color();
        let name_galley = ui.painter().layout(sample.name.to_string(), egui::FontId::proportional(14.0), text_color, rect.width() - 8.0);
        ui.painter().galley(rect.center() - (name_galley.size() / 2.0), name_galley, text_color);
    }

    response
}

fn draw_pad_fx_editor(app: &mut CypherApp, ui: &mut Ui, pad_index: usize) {
    let mut fx_changed = false;
    let theme = &app.theme.sampler_pad_window;

    Frame::none().fill(theme.fx_panel_bg).show(ui, |ui| {
        ui.vertical_centered(|ui| {
            ui.heading(format!("Editing Pad {}", pad_index + 1));
        });

        ui.columns(2, |columns| {
            // --- ADSR Column ---
            columns[0].vertical(|ui| {
                ui.label(RichText::new("Envelope").color(theme.fx_label_color));
                ui.scope(|ui| {
                    let visuals = &mut ui.style_mut().visuals;
                    visuals.widgets.inactive.bg_fill = theme.fx_slider_track_color;
                    visuals.widgets.hovered.bg_fill = theme.fx_slider_grab_color;
                    visuals.widgets.active.bg_fill = theme.fx_slider_grab_color;

                    let fx = &mut app.sampler_pad_fx_settings[pad_index];
                    let mut ui_settings = AdsrUiSettings::from_settings(&fx.adsr);
                    if ui.add(Slider::new(&mut ui_settings.attack, 0.0..=1.0).text("Attack")).changed() { fx_changed = true; }
                    if ui.add(Slider::new(&mut ui_settings.decay, 0.0..=1.0).text("Decay")).changed() { fx_changed = true; }
                    if ui.add(Slider::new(&mut ui_settings.sustain, 0.0..=1.0).text("Sustain")).changed() { fx_changed = true; }
                    if ui.add(Slider::new(&mut ui_settings.release, 0.0..=1.0).text("Release")).changed() { fx_changed = true; }

                    fx.adsr = AdsrSettings {
                        attack: slider_to_time(ui_settings.attack, 2.0),
                        decay: slider_to_time(ui_settings.decay, 2.0),
                        sustain: ui_settings.sustain,
                        release: slider_to_time(ui_settings.release, 4.0),
                    };
                });
            });

            // --- Effects Column ---
            columns[1].vertical(|ui| {
                ui.label(RichText::new("Effects").color(theme.fx_label_color));
                ui.scope(|ui| {
                    let visuals = &mut ui.style_mut().visuals;
                    visuals.widgets.inactive.bg_fill = theme.fx_slider_track_color;
                    visuals.widgets.hovered.bg_fill = theme.fx_slider_grab_color;
                    visuals.widgets.active.bg_fill = theme.fx_slider_grab_color;

                    let fx = &mut app.sampler_pad_fx_settings[pad_index];

                    if ui.add(Slider::new(&mut fx.volume, 0.0..=1.5).text("Volume")).changed() { fx_changed = true; }
                    if ui.add(Slider::new(&mut fx.pitch_semitones, -24.0..=24.0).text("Pitch")).changed() { fx_changed = true; }
                    if ui.add(Slider::new(&mut fx.distortion_amount, 0.0..=1.0).text("Distortion")).changed() { fx_changed = true; }
                    ui.separator();
                    if ui.add(Slider::new(&mut fx.reverb_mix, 0.0..=1.0).text("Reverb Mix")).changed() { fx_changed = true; }
                    if ui.add(Slider::new(&mut fx.reverb_size, 0.0..=1.0).text("Reverb Size")).changed() { fx_changed = true; }
                    if ui.add(Slider::new(&mut fx.reverb_decay, 0.0..=1.0).text("Reverb Decay")).changed() { fx_changed = true; }
                    if ui.add(Slider::new(&mut fx.gate_close_time_ms, 0.0..=2000.0).text("Reverb Gate")).changed() { fx_changed = true; }

                    let gate_button = Button::new("Gate Reverb").fill(
                        if fx.is_reverb_gated { theme.trash_mode_active_bg } else { theme.kit_button_bg }
                    );
                    if ui.add(gate_button).clicked() {
                        fx.is_reverb_gated = !fx.is_reverb_gated;
                        fx_changed = true;
                    }
                });
            });
        });
    });

    if fx_changed {
        app.send_command(AudioCommand::SetSamplerPadFx {
            pad_index,
            settings: app.sampler_pad_fx_settings[pad_index],
        });
    }
}