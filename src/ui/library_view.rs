// src/ui/library_view.rs
use crate::app::{CypherApp, LibraryView};
use crate::asset::{Asset, AssetRef, SampleRef};
use crate::audio_engine::AudioCommand;
use crate::sampler::SamplerKit;
use crate::settings;
use crate::ui;
use egui::{
    epaint, vec2, Align, Align2, Button, Color32, DragAndDrop, Frame, Id, Layout, Response,
    RichText, Rounding, ScrollArea, Sense, Stroke, Ui, Window,
};
use rfd::FileDialog;
use std::cmp::max;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;

pub fn draw_library_panel(app: &mut CypherApp, ui: &mut Ui) {
    let frame = Frame::none().fill(app.theme.library.panel_background);
    frame.show(ui, |ui| {
        ui.set_min_height(210.0);
        // --- TOP TOOLBAR ---
        ui.horizontal(|ui| {
            if ui.add(Button::new("Rescan Library").fill(app.theme.library.button_bg)).clicked() {
                app.rescan_asset_library();
                app.rescan_chord_styles();
            }
            ui.separator();

            // Tab Buttons
            let sample_bg = if app.library_view == LibraryView::Samples { app.theme.library.tab_active_bg } else { app.theme.library.tab_inactive_bg };
            if ui.add(Button::new("Samples").fill(sample_bg)).clicked() { app.library_view = LibraryView::Samples; app.library_path.clear(); }

            let synth_bg = if app.library_view == LibraryView::Synths { app.theme.library.tab_active_bg } else { app.theme.library.tab_inactive_bg };
            if ui.add(Button::new("Synths").fill(synth_bg)).clicked() { app.library_view = LibraryView::Synths; app.library_path.clear(); }

            let kit_bg = if app.library_view == LibraryView::Kits { app.theme.library.tab_active_bg } else { app.theme.library.tab_inactive_bg };
            if ui.add(Button::new("Kits").fill(kit_bg)).clicked() { app.library_view = LibraryView::Kits; app.library_path.clear(); }

            let keys_bg = if app.library_view == LibraryView::EightyEightKeys { app.theme.library.tab_active_bg } else { app.theme.library.tab_inactive_bg };
            if ui.add(Button::new("88Keys").fill(keys_bg)).clicked() { app.library_view = LibraryView::EightyEightKeys; app.library_path.clear(); }

            // --- Path/Back button only for asset views ---
            if app.library_view != LibraryView::EightyEightKeys {
                ui.separator();

                if !app.library_path.is_empty() {
                    if ui.add(Button::new("⬅ Back").fill(app.theme.library.button_bg)).clicked() {
                        app.library_path.pop();
                    }
                }
                let path_str = if app.library_path.is_empty() {
                    match app.library_view {
                        LibraryView::Samples => "Samples",
                        LibraryView::Synths => "Synths",
                        LibraryView::Kits => "Kits",
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
            ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
                ui::eighty_eight_keys_view::draw_88_keys_panel(app, ui);
            });
        } else {
            let category_root = match app.library_view {
                LibraryView::Samples => &app.asset_library.sample_root,
                LibraryView::Synths => &app.asset_library.synth_root,
                LibraryView::Kits => &app.asset_library.kit_root,
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
            let theme = app.theme.clone();

            ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
                // --- NEW: Height-aware responsive grid logic ---
                const CARD_HEIGHT: f32 = 80.0;
                const SPACING: f32 = 10.0;
                let cell_height = CARD_HEIGHT + SPACING;

                let total_items = current_folder.subfolders.len() + current_folder.assets.len();
                if total_items == 0 {
                    return; // Nothing to draw
                }

                // 1. Calculate how many rows can fit in the available vertical space.
                let available_height = ui.available_height();
                let max_rows_possible = max(1, (available_height / cell_height).floor() as usize);

                // 2. Calculate how many columns we need to fit all items into that many rows.
                let num_cols = (total_items as f32 / max_rows_possible as f32).ceil() as usize;

                // 3. Use egui::Grid to lay out the items.
                let mut item_index = 0;
                egui::Grid::new("responsive_asset_grid")
                    .spacing([SPACING, SPACING])
                    .show(ui, |ui| {
                        for folder_name in current_folder.subfolders.keys() {
                            if draw_folder_card(ui, folder_name, &theme).clicked() {
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
                            };

                            if response.clicked() {
                                match asset {
                                    Asset::SynthPreset(preset_ref) => preset_to_load = Some(preset_ref.path().clone()),
                                    Asset::SamplerKit(kit_ref) => kit_to_load = Some(kit_ref.path().clone()),
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
        }
    });
}

fn draw_folder_card(ui: &mut Ui, name: &str, theme: &crate::theme::Theme) -> Response {
    let size = vec2(100.0, 80.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());

    if ui.is_rect_visible(rect) {
        let visuals = ui.style().interact(&response);
        let fill_color = if response.hovered() { theme.library.card_hovered_bg } else { theme.library.card_bg };
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
        let fill_color = if response.hovered() { theme.library.card_hovered_bg } else { theme.library.card_bg };
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
        .default_size([480.0, 480.0])
        .pivot(Align2::CENTER_CENTER)
        .default_pos(ctx.screen_rect().center())
        .show(ctx, |ui| {
            let trash_mode_id = Id::new("trash_mode");
            let mut trash_mode = ui.memory_mut(|m| *m.data.get_temp_mut_or_default(trash_mode_id));

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
                            let kit = SamplerKit {
                                pads: app.sampler_pad_info.clone().map(|s_ref| {
                                    s_ref.map(|sample| {
                                        if let Ok(relative_path) = sample.path.strip_prefix(&config_dir) {
                                            return relative_path.to_path_buf();
                                        }
                                        sample.path
                                    })
                                }),
                            };
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

            let playing_mask = app.playing_pads.load(Ordering::Relaxed);
            let spacing = 10.0;
            let pad_size = (ui.available_width() - spacing * 3.0) / 4.0;
            let size_vec = vec2(pad_size, pad_size);
            let mut sample_to_load: Option<(usize, SampleRef)> = None;

            egui::Grid::new("sample_pad_grid").spacing([spacing, spacing]).show(ui, |ui| {
                for i in 0..16 {
                    let visual_row = i / 4;
                    let visual_col = i % 4;
                    let logical_pad_index = (3 - visual_row) * 4 + visual_col;
                    let response = draw_pad(ui, logical_pad_index, app, size_vec, playing_mask, trash_mode);

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
            }
        });
    app.sample_pad_window_open = is_open;
}

fn draw_pad(
    ui: &mut Ui,
    pad_index: usize,
    app: &mut CypherApp,
    size: egui::Vec2,
    playing_mask: u16,
    trash_mode: bool,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());

    if trash_mode && response.clicked() {
        app.send_command(AudioCommand::ClearSample { pad_index });
        app.sampler_pad_info[pad_index] = None;
    }

    let is_being_dragged_over = ui.rect_contains_pointer(rect) && DragAndDrop::has_any_payload(ui.ctx());
    let is_playing = (playing_mask >> pad_index) & 1 == 1;

    let pad_color = if is_being_dragged_over && !trash_mode {
        ui.style().visuals.selection.bg_fill
    } else {
        app.theme.sampler_pad_window.pad_bg_color
    };

    let visual_row = 3 - (pad_index / 4);
    let mut outline_color = if trash_mode && response.hovered() {
        app.theme.sampler_pad_window.pad_trash_hover_outline_color
    } else {
        match visual_row {
            3 => app.theme.sampler_pad_window.pad_outline_row_4_color,
            2 => app.theme.sampler_pad_window.pad_outline_row_3_color,
            1 => app.theme.sampler_pad_window.pad_outline_row_2_color,
            _ => app.theme.sampler_pad_window.pad_outline_row_1_color,
        }
    };
    if is_playing {
        outline_color = app.theme.sampler_pad_window.pad_playing_outline_color;
    }

    ui.painter().rect(rect, Rounding::from(5.0), pad_color, Stroke::new(2.0, outline_color), epaint::StrokeKind::Inside);

    if let Some(sample) = &app.sampler_pad_info[pad_index] {
        let text_color = ui.style().visuals.text_color();
        let name_galley = ui.painter().layout(sample.name.to_string(), egui::FontId::proportional(14.0), text_color, rect.width() - 8.0);
        ui.painter().galley(rect.center() - (name_galley.size() / 2.0), name_galley, text_color);
    }

    response
}