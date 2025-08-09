use crate::app::CypherApp;
use crate::audio_engine::AudioCommand;
use crate::fx;
use crate::looper::NUM_LOOPERS;
use crate::synth::LfoRateMode;
use egui::{
    epaint, vec2, Align, Color32, ComboBox, CornerRadius, DragValue, Frame, Layout, Pos2, Rect,
    Response, RichText, Sense, Stroke, Ui,
};
use std::sync::atomic::Ordering;

// --- Helper Functions ---
fn linear_to_db(linear: f32) -> f32 {
    if linear <= 1e-6 {
        -f32::INFINITY
    } else {
        20.0 * linear.log10()
    }
}

// Custom volume fader widget (vertical)
fn volume_fader(
    ui: &mut Ui,
    value: &mut f32,
    peak_level: f32,
    theme: &crate::theme::Theme,
    track_color: Color32,
    meter_color: Color32,
) -> Response {
    let desired_height = ui.available_height().max(0.0);
    let desired_size = vec2(20.0, desired_height);
    let (rect, response) = ui.allocate_exact_size(desired_size, Sense::drag());

    // --- Interaction Logic ---
    if response.dragged() {
        if let Some(pos) = response.interact_pointer_pos() {
            let relative_y = 1.0 - (pos.y - rect.top()) / rect.height();
            *value = (relative_y.clamp(0.0, 1.0) * 1.5).clamp(0.0, 1.5);
        }
    }

    // --- Drawing Logic ---
    if ui.is_rect_visible(rect) {
        let painter = ui.painter_at(rect);

        // 1. Draw the fader track (background) with a darkened version of the track color
        let r = (track_color.r() as f32 * 0.2) as u8;
        let g = (track_color.g() as f32 * 0.2) as u8;
        let b = (track_color.b() as f32 * 0.2) as u8;
        let fader_bg_color = Color32::from_rgb(r, g, b);

        painter.rect(
            rect,
            CornerRadius::from(3.0),
            fader_bg_color,
            Stroke::NONE,
            epaint::StrokeKind::Inside,
        );

        // 2. Draw the peak meter inside the track
        if peak_level > 0.0 {
            let post_fader_peak = peak_level * *value;
            let bar_height = rect.height() * (post_fader_peak / 1.5).clamp(0.0, 1.0);
            let bar_rect = Rect::from_min_size(
                rect.left_bottom() - vec2(0.0, bar_height),
                vec2(rect.width(), bar_height),
            );
            let color = if post_fader_peak > 1.0 {
                theme.mixer.meter_clip_color
            } else {
                meter_color // Use the passed-in meter color
            };
            painter.rect_filled(bar_rect, CornerRadius::from(3.0), color);
        }

        // 3. Draw the fader thumb
        let thumb_height = 8.0;
        let thumb_y = rect.top() + rect.height() * (1.0 - (*value / 1.5).clamp(0.0, 1.0));
        let thumb_center = Pos2::new(rect.center().x, thumb_y);
        let thumb_rect =
            Rect::from_center_size(thumb_center, vec2(rect.width() + 4.0, thumb_height));
        painter.rect(
            thumb_rect,
            CornerRadius::from(3.0),
            theme.mixer.fader_thumb_color,
            Stroke::new(1.0, theme.global_text_color),
            epaint::StrokeKind::Inside,
        );
    }

    response
}

fn gain_reduction_meter(
    ui: &mut Ui,
    reduction_normalized: f32,
    theme: &crate::theme::Theme,
) -> Response {
    let desired_height = ui.available_height().max(0.0);
    let desired_size = vec2(10.0, desired_height);
    let (rect, response) = ui.allocate_exact_size(desired_size, Sense::hover());

    if ui.is_rect_visible(rect) {
        let painter = ui.painter_at(rect);
        // 1. Draw the meter track (background)
        painter.rect(
            rect,
            CornerRadius::from(3.0),
            theme.mixer.fader_track_bg,
            Stroke::NONE,
            epaint::StrokeKind::Inside,
        );
        // 2. Draw the reduction bar
        if reduction_normalized > 0.0 {
            let bar_height = rect.height() * reduction_normalized.clamp(0.0, 1.0);
            let bar_rect =
                Rect::from_min_size(rect.left_top(), vec2(rect.width(), bar_height));
            painter.rect_filled(bar_rect, CornerRadius::from(3.0), theme.mixer.limiter_gr_color);
        }
    }
    response
}

// Custom volume fader widget (horizontal)
pub fn horizontal_volume_fader(
    ui: &mut Ui,
    _id_source: impl std::hash::Hash,
    value: &mut f32,
    peak_level: f32,
    track_bg: Color32,
    theme: &crate::theme::Theme,
) -> Response {
    let desired_size = vec2(ui.available_width() * 0.8, 20.0);
    let (rect, response) = ui.allocate_exact_size(desired_size, Sense::drag());

    if response.dragged() {
        if let Some(pos) = response.interact_pointer_pos() {
            let relative_x = (pos.x - rect.left()) / rect.width();
            *value = (relative_x.clamp(0.0, 1.0) * 1.5).clamp(0.0, 1.5);
        }
    }

    if ui.is_rect_visible(rect) {
        let painter = ui.painter_at(rect);

        painter.rect(
            rect,
            CornerRadius::ZERO,
            track_bg,
            Stroke::NONE,
            epaint::StrokeKind::Inside,
        );

        if peak_level > 0.0 {
            let post_fader_peak = peak_level * *value;
            let bar_width = rect.width() * (post_fader_peak / 1.5).clamp(0.0, 1.0);
            let bar_rect = Rect::from_min_size(rect.left_top(), vec2(bar_width, rect.height()));
            let color = if post_fader_peak > 1.0 {
                theme.mixer.meter_clip_color
            } else {
                theme.mixer.meter_normal_color
            };
            painter.rect_filled(bar_rect, CornerRadius::ZERO, color);
        }

        let thumb_width = 8.0;
        let thumb_x = rect.left() + rect.width() * (*value / 1.5).clamp(0.0, 1.0);
        let thumb_center = Pos2::new(thumb_x, rect.center().y);
        let thumb_rect = Rect::from_center_size(
            thumb_center,
            vec2(thumb_width, rect.height() + 4.0),
        );
        painter.rect(
            thumb_rect,
            CornerRadius::from(3.0),
            theme.instrument_panel.fader_thumb_color,
            Stroke::new(1.0, theme.global_text_color),
            epaint::StrokeKind::Inside,
        );
    }

    response
}

fn draw_track_strip(ui: &mut Ui, app: &mut CypherApp, track_id: usize) {
    let mut mixer_state = app.track_mixer_state.write().unwrap();
    let track = &mut mixer_state.tracks[track_id];
    let track_color = app.theme.loopers.track_colors[track_id];

    ui.with_layout(Layout::bottom_up(Align::Center), |ui| {
        // --- Track Label ---
        ui.label(
            RichText::new(format!("Track {}", track_id + 1))
                .monospace()
                .color(track_color),
        );
        ui.add_space(4.0);

        // --- FX Button ---
        let fx_button = egui::Button::new("FX").fill(app.theme.mixer.mute_off_bg);
        if ui.add(fx_button).clicked() {
            app.active_fx_target = Some(fx::InsertionPoint::Looper(track_id));
            app.fx_editor_window_open = true;
        }
        ui.add_space(2.0);

        // --- Mute/Solo Buttons ---
        ui.horizontal(|ui| {
            let spacing = ui.style().spacing.item_spacing.x;
            let available_width = ui.available_width();
            let button_width = ((available_width - spacing) / 2.0).max(0.0);
            let button_size = vec2(button_width, 20.0);

            let mute_button = egui::Button::new(RichText::new("M").monospace().size(12.0)).fill(
                if track.is_muted {
                    app.theme.mixer.mute_on_bg
                } else {
                    app.theme.mixer.mute_off_bg
                },
            );
            if ui.add_sized(button_size, mute_button).clicked() {
                track.is_muted = !track.is_muted;
            }

            let solo_button = egui::Button::new(RichText::new("S").monospace().size(12.0)).fill(
                if track.is_soloed {
                    app.theme.mixer.solo_on_bg
                } else {
                    app.theme.mixer.solo_off_bg
                },
            );
            if ui.add_sized(button_size, solo_button).clicked() {
                track.is_soloed = !track.is_soloed;
            }
        });
        ui.add_space(4.0);

        // --- Volume Readout ---
        let db_text = {
            let db = linear_to_db(track.volume);
            if db.is_infinite() {
                "-inf".to_string()
            } else {
                format!("{:.1}", db)
            }
        };
        ui.label(
            RichText::new(db_text)
                .monospace()
                .size(10.0)
                .background_color(app.theme.mixer.fader_track_bg)
                .color(app.theme.global_text_color),
        );
        ui.add_space(5.0);

        // --- Fader ---
        volume_fader(
            ui,
            &mut track.volume,
            app.displayed_peak_levels[track_id],
            &app.theme,
            track_color,
            track_color, // Pass track_color for the meter as well
        );
    });
}

fn draw_master_strip(ui: &mut Ui, app: &mut CypherApp) {
    let mut vol = app.master_volume.load(Ordering::Relaxed) as f32 / 1_000_000.0;
    let master_fader_bg = app.theme.mixer.fader_track_bg.gamma_multiply(3.5);

    ui.with_layout(Layout::bottom_up(Align::Center), |ui| {
        ui.label(RichText::new("Master").color(app.theme.mixer.label_color));
        ui.add_space(4.0);

        // --- Master FX Button ---
        let fx_button = egui::Button::new("FX").fill(app.theme.mixer.mute_off_bg);
        if ui.add(fx_button).clicked() {
            app.active_fx_target = Some(fx::InsertionPoint::Master);
            app.fx_editor_window_open = true;
        }
        ui.add_space(2.0);

        match app.limiter_release_mode {
            LfoRateMode::Hz => {
                let mut release_ms =
                    app.limiter_release_ms.load(Ordering::Relaxed) as f32 / 1000.0;
                if ui
                    .add(
                        DragValue::new(&mut release_ms)
                            .speed(0.1)
                            .range(1.0..=1000.0)
                            .suffix("ms"),
                    )
                    .changed()
                {
                    app.send_command(AudioCommand::SetLimiterReleaseMs(release_ms));
                }
            }
            LfoRateMode::Sync => {
                const TRP: f32 = 2.0 / 3.0;
                const DOT: f32 = 1.5;
                let rates = [
                    (32.0, "1/128"), (16.0 * DOT, "1/64d"), (16.0, "1/64"), (16.0 * TRP, "1/64t"),
                    (8.0 * DOT, "1/32d"), (8.0, "1/32"), (8.0 * TRP, "1/32t"), (4.0 * DOT, "1/16d"),
                    (4.0, "1/16"), (4.0 * TRP, "1/16t"), (2.0 * DOT, "1/8d"), (2.0, "1/8"),
                    (2.0 * TRP, "1/8t"), (1.0 * DOT, "1/4d"), (1.0, "1/4"), (1.0 * TRP, "1/4t"),
                    (0.5 * DOT, "1/2d"), (0.5, "1/2"), (0.5 * TRP, "1/2t"), (0.25, "1 bar"),
                ];
                let mut current_rate =
                    app.limiter_release_sync_rate.load(Ordering::Relaxed) as f32 / 1_000_000.0;
                let current_label = rates
                    .iter()
                    .find(|(r, _)| (*r - current_rate).abs() < 1e-6)
                    .map_or_else(|| current_rate.to_string(), |(_, l)| l.to_string());

                ComboBox::from_id_salt("limiter_sync_rate")
                    .selected_text(current_label)
                    .show_ui(ui, |ui| {
                        for (rate_val, rate_label) in rates {
                            if ui.selectable_value(&mut current_rate, rate_val, rate_label).clicked() {
                                app.send_command(AudioCommand::SetLimiterReleaseSync(current_rate));
                            }
                        }
                    });
            }
        }

        ui.horizontal(|ui| {
            if ui.selectable_value(&mut app.limiter_release_mode, LfoRateMode::Hz, "Hz").clicked() {
                app.send_command(AudioCommand::SetLimiterReleaseMode(LfoRateMode::Hz));
            }
            if ui.selectable_value(&mut app.limiter_release_mode, LfoRateMode::Sync, "Sync").clicked() {
                app.send_command(AudioCommand::SetLimiterReleaseMode(LfoRateMode::Sync));
            }
        });

        ui.label(RichText::new("Thresh").monospace().size(10.0));

        let is_active = app.limiter_is_active.load(Ordering::Relaxed);
        let bypass_button =
            egui::Button::new(RichText::new("Limiter").monospace().size(12.0)).fill(if is_active {
                app.theme.mixer.limiter_active_bg
            } else {
                app.theme.mixer.mute_off_bg
            });

        if ui.add(bypass_button).clicked() {
            app.send_command(AudioCommand::ToggleLimiter);
        }

        ui.add_space(4.0);
        let db_text = {
            let db = linear_to_db(vol);
            if db.is_infinite() {
                "-inf".to_string()
            } else {
                format!("{:.1}", db)
            }
        };
        ui.label(
            RichText::new(db_text)
                .monospace()
                .size(10.0)
                .background_color(app.theme.mixer.fader_track_bg)
                .color(app.theme.global_text_color),
        );
        ui.add_space(5.0);

        let layout = Layout::left_to_right(Align::Min).with_cross_align(Align::Min);
        ui.with_layout(layout, |ui| {
            let meter_group_width = 10.0 + 2.0 + 20.0 + 2.0 + 20.0;
            let spacer = (ui.available_width() - meter_group_width) / 2.0;
            if spacer > 0.0 {
                ui.add_space(spacer);
            }

            gain_reduction_meter(ui, app.displayed_gain_reduction, &app.theme);
            ui.add_space(2.0);

            let mut threshold =
                app.limiter_threshold.load(Ordering::Relaxed) as f32 / 1_000_000.0;
            if volume_fader(
                ui,
                &mut threshold,
                0.0,
                &app.theme,
                master_fader_bg,
                app.theme.mixer.meter_normal_color, // Use global theme color
            ).dragged() {
                app.send_command(AudioCommand::SetLimiterThreshold(threshold));
            }
            ui.add_space(2.0);

            if volume_fader(
                ui,
                &mut vol,
                app.displayed_master_peak_level,
                &app.theme,
                master_fader_bg,
                app.theme.mixer.meter_normal_color, // Use global theme color
            ).dragged() {
                app.send_command(AudioCommand::SetMasterVolume(vol));
            }
        });
    });
}

pub fn draw_mixer_panel(app: &mut CypherApp, ui: &mut Ui) {
    // **FIXED**: Re-introduced ui.group() and set frame color correctly.
    let frame_style = Frame::new().fill(app.theme.mixer.panel_background);
    ui.group(|ui| {
        frame_style.show(ui, |ui| {
            ui.set_min_height(300.0);
            ui.label(RichText::new("Mixer").monospace().color(app.theme.mixer.label_color));
            ui.separator();

            let stroke = Stroke::new(1.0, ui.style().visuals.window_stroke.color);

            ui.columns(NUM_LOOPERS + 1, |columns| {
                for i in 0..NUM_LOOPERS {
                    let column_ui = &mut columns[i];
                    let vline_y_range = column_ui.clip_rect().y_range();

                    draw_track_strip(column_ui, app, i);

                    let painter = column_ui.painter();
                    let rect = column_ui.min_rect();
                    painter.vline(
                        rect.right() + column_ui.style().spacing.item_spacing.x / 2.0,
                        vline_y_range,
                        stroke,
                    );
                }
                draw_master_strip(&mut columns[NUM_LOOPERS], app);
            });
        });
    });
}