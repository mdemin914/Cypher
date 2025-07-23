use crate::app::{CypherApp, SlicerState};
use crate::settings;
use crate::slicer;
use crate::theme::SlicerWindowTheme;
use egui::{
    epaint, CentralPanel, Frame, Grid, Pos2, Rect, RichText, Sense, Shape, Slider, Stroke,
    TextEdit, TopBottomPanel, Ui, Window,
};
use rfd::FileDialog;
use std::fs;
use std::path::Path;

fn recalculate_slices(state: &mut SlicerState) {
    let source_audio = if let Some(sa) = &state.source_audio {
        sa
    } else {
        return;
    };
    let total_samples = source_audio.data.len();
    if total_samples == 0 {
        return;
    }

    let num_points = 4096.min(total_samples);
    let samples_per_point = total_samples as f32 / num_points as f32;
    let mut visual_peaks = Vec::with_capacity(num_points);
    for i in 0..num_points {
        let start = (i as f32 * samples_per_point) as usize;
        let end = ((i + 1) as f32 * samples_per_point) as usize;
        let chunk = &source_audio.data[start.min(total_samples)..end.min(total_samples)];
        let peak = chunk.iter().fold(0.0f32, |max, &v| max.max(v.abs()));
        visual_peaks.push(peak);
    }

    state.slice_regions = slicer::find_slices_from_visual_peaks(
        &visual_peaks,
        samples_per_point,
        state.threshold,
        state.min_silence_ms,
        source_audio.sample_rate,
        &source_audio.data,
    );
}

fn load_slicer_sample(app: &mut CypherApp) {
    if let Some(path) = FileDialog::new().add_filter("wav", &["wav"]).pick_file() {
        match crate::app::load_source_audio_file_with_sr(&path) {
            Ok(source_audio) => {
                let total_samples = source_audio.data.len();
                let slicer_state = &mut app.slicer_state;
                if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                    slicer_state.base_export_name = name.to_string();
                }
                slicer_state.source_audio = Some(source_audio);
                slicer_state.view_start_sample = 0;
                slicer_state.view_end_sample = total_samples;
                recalculate_slices(slicer_state);
            }
            Err(e) => {
                eprintln!("Failed to load sample for slicer: {}", e);
                app.slicer_state.source_audio = None;
            }
        }
    }
}

fn export_slices(app: &mut CypherApp) {
    let state = &app.slicer_state;
    let source_audio = if let Some(sa) = &state.source_audio {
        sa
    } else {
        return;
    };

    if state.base_export_name.is_empty() {
        eprintln!("Export failed: Base filename cannot be empty.");
        return;
    }

    if let Some(config_dir) = settings::get_config_dir() {
        let samples_root = config_dir.join("Samples");
        let mut export_dir = samples_root.join(&state.export_parent_path);

        if !state.export_new_folder_name.is_empty() {
            export_dir = export_dir.join(&state.export_new_folder_name);
        }

        if let Err(e) = fs::create_dir_all(&export_dir) {
            eprintln!(
                "Failed to create export directory {}: {}",
                export_dir.display(),
                e
            );
            return;
        }

        let total_samples = source_audio.data.len();
        let tail_samples = (state.tail_ms / 1000.0 * source_audio.sample_rate as f32).round() as usize;

        const FADE_MS: f32 = 5.0;
        let fade_samples = (FADE_MS / 1000.0 * source_audio.sample_rate as f32) as usize;

        for (i, (start_sample, end_sample)) in state.slice_regions.iter().enumerate() {
            let extended_end_sample = (*end_sample + tail_samples).min(total_samples);

            if *start_sample >= extended_end_sample {
                continue;
            }

            let mut slice_data = source_audio.data[*start_sample..extended_end_sample].to_vec();
            let slice_len = slice_data.len();

            if slice_len > fade_samples * 2 {
                for i in 0..fade_samples {
                    let gain = i as f32 / fade_samples as f32;
                    slice_data[i] *= gain;
                }
                for i in 0..fade_samples {
                    let gain = i as f32 / fade_samples as f32;
                    slice_data[slice_len - 1 - i] *= gain;
                }
            }

            let filename = format!("{} {}.wav", state.base_export_name, i + 1);
            let path = export_dir.join(filename);

            let spec = hound::WavSpec {
                channels: 1,
                sample_rate: source_audio.sample_rate,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            };

            match hound::WavWriter::create(&path, spec) {
                Ok(mut writer) => {
                    for &sample in &slice_data {
                        let amplitude = i16::MAX as f32;
                        writer.write_sample((sample * amplitude) as i16).ok();
                    }
                    writer.finalize().ok();
                }
                Err(e) => {
                    eprintln!("Failed to create wav file at {}: {}", path.display(), e);
                }
            }
        }
        app.rescan_asset_library();
    }
}

pub fn draw_slicer_window(app: &mut CypherApp, ctx: &egui::Context) {
    let mut is_open = app.slicer_window_open;
    let theme = app.theme.slicer_window.clone();

    let window_size = egui::vec2(800.0, 600.0);
    let screen_rect = ctx.screen_rect();
    let center_pos = egui::pos2(
        screen_rect.center().x - window_size.x / 2.0,
        screen_rect.center().y - window_size.y / 2.0,
    );

    Window::new("Sample Slicer")
        .open(&mut is_open)
        .frame(Frame::window(&ctx.style()).fill(theme.background))
        .default_size(window_size)
        .default_pos(center_pos)
        .resizable(true)
        .show(ctx, |ui| {
            let sample_is_loaded = app.slicer_state.source_audio.is_some();

            TopBottomPanel::top("slicer_top_panel")
                .frame(Frame::none().fill(theme.background))
                .show_inside(ui, |ui| {
                    if sample_is_loaded {
                        let mut params_changed = false;

                        ui.horizontal(|ui| {
                            if ui.add(egui::Button::new("Load New Sample").fill(theme.button_bg)).clicked() {
                                load_slicer_sample(app);
                            }
                            ui.separator();
                            ui.label(
                                RichText::new(format!(
                                    "Detected Slices: {}",
                                    app.slicer_state.slice_regions.len()
                                ))
                                    .color(theme.label_color),
                            );
                        });
                        ui.separator();

                        ui.scope(|ui| {
                            let visuals = ui.visuals_mut();
                            visuals.widgets.inactive.bg_fill = theme.slider_track_color;
                            visuals.widgets.hovered.bg_fill = theme.slider_grab_color;
                            visuals.widgets.active.bg_fill = theme.slider_grab_color;
                            visuals.selection.bg_fill = theme.slider_grab_color;

                            visuals.widgets.inactive.bg_stroke = Stroke::NONE;
                            visuals.widgets.hovered.bg_stroke = Stroke::NONE;
                            visuals.widgets.active.bg_stroke = Stroke::NONE;

                            Grid::new("slicer_params_grid").show(ui, |ui| {
                                ui.label(RichText::new("Silence Threshold").color(theme.label_color));
                                if ui.add(Slider::new(&mut app.slicer_state.threshold, 0.0..=0.2).logarithmic(true)).changed() {
                                    params_changed = true;
                                }
                                ui.end_row();

                                ui.label(RichText::new("Min Silence (ms)").color(theme.label_color));
                                if ui.add(Slider::new(&mut app.slicer_state.min_silence_ms, 1.0..=1000.0)).changed() {
                                    params_changed = true;
                                }
                                ui.end_row();

                                ui.label(RichText::new("Tail (ms)").color(theme.label_color));
                                ui.add(Slider::new(&mut app.slicer_state.tail_ms, 0.0..=10000.0));
                                ui.end_row();
                            });
                        });

                        ui.separator();
                        if params_changed {
                            recalculate_slices(&mut app.slicer_state);
                        }
                    }
                });

            TopBottomPanel::bottom("slicer_bottom_panel")
                .frame(Frame::none().fill(theme.background))
                .show_inside(ui, |ui| {
                    if sample_is_loaded {
                        ui.separator();
                        ui.heading(RichText::new("Export").color(theme.label_color));

                        ui.scope(|ui| {
                            let visuals = ui.visuals_mut();
                            visuals.widgets.inactive.bg_fill = theme.text_edit_bg;
                            visuals.widgets.hovered.bg_fill = theme.text_edit_bg;
                            visuals.widgets.active.bg_fill = theme.text_edit_bg;

                            let border_color = theme.slider_track_color;
                            visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, border_color);
                            visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, theme.label_color);
                            visuals.widgets.active.bg_stroke = Stroke::new(1.0, theme.label_color);

                            Grid::new("export_grid").num_columns(2).show(ui, |ui| {
                                ui.label(RichText::new("Base Filename").color(theme.label_color));
                                ui.add(TextEdit::singleline(&mut app.slicer_state.base_export_name).desired_width(200.0));
                                ui.end_row();

                                ui.label(RichText::new("Parent Folder").color(theme.label_color));
                                ui.horizontal(|ui| {
                                    let parent_path_display = if app.slicer_state.export_parent_path.as_os_str().is_empty() {
                                        "Samples/".to_string()
                                    } else {
                                        format!("Samples/{}/", app.slicer_state.export_parent_path.to_string_lossy().replace('\\', "/"))
                                    };
                                    ui.label(RichText::new(parent_path_display).color(theme.label_color));
                                    if ui.button("Change...").clicked() {
                                        if let Some(config_dir) = settings::get_config_dir() {
                                            let samples_dir = config_dir.join("Samples");
                                            if !samples_dir.exists() {
                                                fs::create_dir_all(&samples_dir).ok();
                                            }
                                            if let Some(picked_path) = FileDialog::new().set_directory(&samples_dir).pick_folder() {
                                                if let Ok(relative_path) = picked_path.strip_prefix(&samples_dir) {
                                                    app.slicer_state.export_parent_path = relative_path.to_path_buf();
                                                }
                                            }
                                        }
                                    }
                                });
                                ui.end_row();

                                ui.label(RichText::new("New Subfolder (optional)").color(theme.label_color));
                                ui.add(TextEdit::singleline(&mut app.slicer_state.export_new_folder_name).desired_width(200.0));
                                ui.end_row();
                            });
                        });

                        if ui.add(egui::Button::new("Export Slices").fill(theme.button_bg)).clicked() {
                            export_slices(app);
                        }
                    }
                });

            CentralPanel::default()
                .frame(Frame::none())
                .show_inside(ui, |ui| {
                    if sample_is_loaded {
                        Frame::none().fill(theme.waveform_bg_color).show(ui, |ui| {
                            ui.label(RichText::new("Waveform").color(theme.label_color));
                            draw_interactive_waveform(ui, &mut app.slicer_state, &theme);
                        });
                    } else {
                        ui.vertical_centered_justified(|ui| {
                            if ui.button("Load Sample...").clicked() {
                                load_slicer_sample(app);
                            }
                        });
                    }
                });
        });

    app.slicer_window_open = is_open;
}

fn draw_interactive_waveform(ui: &mut Ui, state: &mut crate::app::SlicerState, theme: &SlicerWindowTheme) {
    let desired_rect = ui.available_rect_before_wrap();
    let (response, painter) =
        ui.allocate_painter(desired_rect.size(), Sense::click_and_drag());
    let rect = response.rect;

    let source_audio = if let Some(sa) = &state.source_audio {
        sa
    } else {
        return;
    };

    let total_samples = source_audio.data.len();
    if total_samples == 0 {
        return;
    }

    if response.hovered() {
        let scroll = ui.ctx().input(|i| i.raw_scroll_delta);
        if scroll.y != 0.0 {
            let zoom_factor = if scroll.y > 0.0 { 0.8 } else { 1.25 };
            let pointer_pos = ui.ctx().input(|i| i.pointer.hover_pos()).unwrap_or(rect.center());
            let hover_ratio = ((pointer_pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
            let view_span = (state.view_end_sample - state.view_start_sample) as f32;
            let sample_at_hover = state.view_start_sample as f32 + view_span * hover_ratio;
            let new_view_span = (view_span * zoom_factor).max(50.0);
            let new_start = sample_at_hover - new_view_span * hover_ratio;
            let new_end = new_start + new_view_span;
            state.view_start_sample = new_start.round() as usize;
            state.view_end_sample = new_end.round() as usize;
        }
    }
    if response.dragged() {
        let view_span = (state.view_end_sample - state.view_start_sample) as f32;
        let pixel_delta = response.drag_delta().x;
        let sample_delta = (pixel_delta / rect.width() * view_span).round() as isize;
        state.view_start_sample = (state.view_start_sample as isize - sample_delta).max(0) as usize;
        state.view_end_sample = (state.view_end_sample as isize - sample_delta).max(0) as usize;
    }
    if state.view_end_sample <= state.view_start_sample {
        state.view_end_sample = state.view_start_sample + 1;
    }
    state.view_end_sample = state.view_end_sample.min(total_samples);
    state.view_start_sample = state.view_start_sample.min(state.view_end_sample.saturating_sub(1));

    let view_start = state.view_start_sample;
    let view_end = state.view_end_sample;
    let view_span = view_end - view_start;
    if view_span == 0 { return; }

    let samples_per_pixel = view_span as f32 / rect.width();
    let sample_to_x = |sample_idx: usize| {
        rect.min.x + (sample_idx.saturating_sub(view_start)) as f32 / view_span as f32 * rect.width()
    };

    let num_pixels = rect.width().ceil() as usize;
    for pixel_x_offset in 0..num_pixels {
        let sample_start_f = view_start as f32 + pixel_x_offset as f32 * samples_per_pixel;
        let sample_end_f = sample_start_f + samples_per_pixel;
        let sample_start_idx = (sample_start_f.floor() as usize).min(total_samples);
        let sample_end_idx = (sample_end_f.ceil() as usize).min(total_samples);
        if sample_start_idx >= sample_end_idx { continue; }
        let chunk = &source_audio.data[sample_start_idx..sample_end_idx];
        let peak = chunk.iter().fold(0.0f32, |max, &v| max.max(v.abs()));
        let x = rect.min.x + pixel_x_offset as f32;
        let y_center = rect.center().y;
        let y_offset = peak * rect.height() / 2.0;
        painter.line_segment([Pos2::new(x, y_center - y_offset), Pos2::new(x, y_center + y_offset)], Stroke::new(1.0, theme.waveform_color));
    }

    let tail_samples = (state.tail_ms / 1000.0 * source_audio.sample_rate as f32).round() as usize;
    let overlay_color = theme.slice_marker_color.gamma_multiply(0.35);

    for (start_sample, end_sample) in &state.slice_regions {
        let extended_end_sample = (*end_sample + tail_samples).min(total_samples);
        if extended_end_sample < view_start || *start_sample > view_end { continue; }
        let x1 = sample_to_x(*start_sample);
        let x2 = sample_to_x(extended_end_sample);
        let overlay_rect = Rect::from_x_y_ranges(x1..=x2, rect.y_range());
        painter.rect_filled(overlay_rect, epaint::Rounding::ZERO, overlay_color);
    }

    let y_center = rect.center().y;
    let y_offset = state.threshold * rect.height() / 2.0;
    let line_stroke = Stroke::new(1.0, theme.slice_marker_color.gamma_multiply(0.5));
    painter.hline(rect.x_range(), y_center - y_offset, line_stroke);
    painter.hline(rect.x_range(), y_center + y_offset, line_stroke);
}