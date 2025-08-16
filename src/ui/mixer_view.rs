// src/ui/mixer_view.rs

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

const CLICK_DRAG_THRESHOLD: f32 = 5.0;

// --- Helper Functions ---
fn linear_to_db(linear: f32) -> f32 {
    if linear <= 1e-6 {
        -f32::INFINITY
    } else {
        20.0 * linear.log10()
    }
}

// --- Pitch Mapping Helper Functions ---
const MIN_PITCH_HZ: f32 = 110.0; // A2
const MAX_PITCH_HZ: f32 = 1760.0; // A6

fn pitch_to_fader_value(pitch_hz: f32) -> f32 {
    let normalized = (pitch_hz.clamp(MIN_PITCH_HZ, MAX_PITCH_HZ) - MIN_PITCH_HZ)
        / (MAX_PITCH_HZ - MIN_PITCH_HZ);
    normalized * 1.5
}

fn fader_value_to_pitch(fader_value: f32) -> f32 {
    let normalized = fader_value / 1.5;
    MIN_PITCH_HZ + normalized * (MAX_PITCH_HZ - MIN_PITCH_HZ)
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
        let thumb_rect =
            Rect::from_center_size(thumb_center, vec2(thumb_width, rect.height() + 4.0));
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
    let track_color = app.theme.loopers.track_colors[track_id];
    let mut fx_button_clicked = false;
    let mut mute_button_clicked = false;
    let mut solo_button_clicked = false;

    // Isolate the lock and copy the data we need for drawing.
    let (is_muted, is_soloed, mut volume) = {
        let mixer_state = app.track_mixer_state.read().unwrap();
        let track = &mixer_state.tracks[track_id];
        (track.is_muted, track.is_soloed, track.volume)
    };

    ui.with_layout(Layout::bottom_up(Align::Center), |ui| {
        // --- Track Label ---
        ui.label(
            RichText::new(format!("Track {}", track_id + 1))
                .monospace()
                .color(track_color),
        );
        ui.add_space(4.0);

        let available_width = ui.available_width();
        let half_width = available_width * 0.5;
        let fx_button_size = vec2(half_width, 20.0);

        // --- FX Button (centered on its own row) ---
        ui.horizontal(|ui| {
            ui.add_space(half_width / 2.0); // Add spacer to center the button
            let fx_button = egui::Button::new(RichText::new("FX").monospace().size(12.0))
                .fill(app.theme.mixer.mute_off_bg)
                .sense(Sense::click_and_drag());
            let response = ui.add_sized(fx_button_size, fx_button);
            if response.clicked()
                || (response.drag_stopped()
                && response.drag_delta().length() < CLICK_DRAG_THRESHOLD)
            {
                fx_button_clicked = true;
            }
        });

        ui.add_space(2.0);

        // --- Mute/Solo Buttons ---
        ui.horizontal(|ui| {
            let spacing = ui.style().spacing.item_spacing.x;
            let button_width = ((available_width - spacing) / 2.0).max(0.0);
            let button_size = vec2(button_width, 20.0);

            let mute_button =
                egui::Button::new(RichText::new("M").monospace().size(12.0))
                    .fill(if is_muted {
                        app.theme.mixer.mute_on_bg
                    } else {
                        app.theme.mixer.mute_off_bg
                    })
                    .sense(Sense::click_and_drag());
            let response = ui.add_sized(button_size, mute_button);
            if response.clicked()
                || (response.drag_stopped()
                && response.drag_delta().length() < CLICK_DRAG_THRESHOLD)
            {
                mute_button_clicked = true;
            }

            let solo_button =
                egui::Button::new(RichText::new("S").monospace().size(12.0))
                    .fill(if is_soloed {
                        app.theme.mixer.solo_on_bg
                    } else {
                        app.theme.mixer.solo_off_bg
                    })
                    .sense(Sense::click_and_drag());
            let response = ui.add_sized(button_size, solo_button);
            if response.clicked()
                || (response.drag_stopped()
                && response.drag_delta().length() < CLICK_DRAG_THRESHOLD)
            {
                solo_button_clicked = true;
            }
        });
        ui.add_space(4.0);

        // --- Volume Readout ---
        let db_text = {
            let db = linear_to_db(volume);
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
        let fader_response = volume_fader(
            ui,
            &mut volume,
            app.displayed_peak_levels[track_id],
            &app.theme,
            track_color,
            track_color, // Pass track_color for the meter as well
        );

        // --- Apply Changes After Drawing ---
        if fader_response.dragged() {
            // Send command instead of locking here to avoid potential UI stalls
            app.send_command(AudioCommand::SetMixerTrackVolume { track_index: track_id, volume });
        }
    });

    // --- Apply deferred button clicks after the layout is done ---
    if fx_button_clicked {
        app.handle_fx_button_click(fx::InsertionPoint::Looper(track_id));
    }
    if mute_button_clicked {
        app.send_command(AudioCommand::ToggleMixerMute(track_id));
    }
    if solo_button_clicked {
        app.send_command(AudioCommand::ToggleMixerSolo(track_id));
    }
}

fn draw_master_strip(ui: &mut Ui, app: &mut CypherApp) {
    let mut vol = app.master_volume.load(Ordering::Relaxed) as f32 / 1_000_000.0;
    let master_fader_bg = app.theme.mixer.fader_track_bg.gamma_multiply(3.5);
    let mut fx_button_clicked = false;

    ui.with_layout(Layout::bottom_up(Align::Center), |ui| {
        ui.label(RichText::new("Master").color(app.theme.mixer.label_color));
        ui.add_space(4.0);

        let available_width = ui.available_width();
        let half_width = available_width * 0.5;
        let fx_button_size = vec2(half_width, 20.0);

        // --- Master FX Button (centered on its own row) ---
        ui.horizontal(|ui| {
            ui.add_space(half_width / 2.0);
            let fx_button = egui::Button::new(RichText::new("FX").monospace().size(12.0))
                .fill(app.theme.mixer.mute_off_bg)
                .sense(Sense::click_and_drag());
            let response = ui.add_sized(fx_button_size, fx_button);
            if response.clicked()
                || (response.drag_stopped()
                && response.drag_delta().length() < CLICK_DRAG_THRESHOLD)
            {
                fx_button_clicked = true;
            }
        });
        ui.add_space(2.0);

        match app.limiter_release_mode {
            LfoRateMode::Hz => {
                ui.scope(|ui| {
                    let visuals = &mut ui.style_mut().visuals.widgets;
                    visuals.inactive.bg_fill = app.theme.mixer.fader_track_bg;
                    visuals.hovered.bg_fill = app.theme.mixer.fader_track_bg.linear_multiply(1.2);
                    visuals.active.bg_fill = app.theme.mixer.fader_thumb_color;

                    let mut release_ms =
                        app.limiter_release_ms.load(Ordering::Relaxed) as f32 / 1000.0;
                    if ui.add(
                        DragValue::new(&mut release_ms)
                            .speed(0.1)
                            .range(1.0..=1000.0)
                            .suffix("ms"),
                    )
                        .changed()
                    {
                        app.send_command(AudioCommand::SetLimiterReleaseMs(release_ms));
                    }
                });
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
                            if ui.selectable_value(&mut current_rate, rate_val, rate_label)
                                .clicked()
                            {
                                app.send_command(AudioCommand::SetLimiterReleaseSync(current_rate));
                            }
                        }
                    });
            }
        }

        ui.horizontal(|ui| {
            if ui.selectable_value(&mut app.limiter_release_mode, LfoRateMode::Hz, "Hz")
                .clicked()
            {
                app.send_command(AudioCommand::SetLimiterReleaseMode(LfoRateMode::Hz));
            }
            if ui.selectable_value(&mut app.limiter_release_mode, LfoRateMode::Sync, "Sync")
                .clicked()
            {
                app.send_command(AudioCommand::SetLimiterReleaseMode(LfoRateMode::Sync));
            }
        });

        ui.label(RichText::new("Thresh").monospace().size(10.0));

        let is_active = app.limiter_is_active.load(Ordering::Relaxed);
        let bypass_button =
            egui::Button::new(RichText::new("Limiter").monospace().size(12.0))
                .fill(if is_active {
                    app.theme.mixer.limiter_active_bg
                } else {
                    app.theme.mixer.mute_off_bg
                })
                .sense(Sense::click_and_drag());
        let response = ui.add(bypass_button);
        if response.clicked()
            || (response.drag_stopped() && response.drag_delta().length() < CLICK_DRAG_THRESHOLD)
        {
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
            )
                .dragged()
            {
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
            )
                .dragged()
            {
                app.send_command(AudioCommand::SetMasterVolume(vol));
            }
        });
    });

    if fx_button_clicked {
        app.handle_fx_button_click(fx::InsertionPoint::Master);
    }
}

fn draw_atmo_strip(ui: &mut Ui, app: &mut CypherApp) {
    let mut vol = app.atmo_master_volume.load(Ordering::Relaxed) as f32 / 1_000_000.0;
    let atmo_fader_bg = app.theme.mixer.fader_track_bg.gamma_multiply(3.0);
    let mut fx_button_clicked = false;

    ui.with_layout(Layout::bottom_up(Align::Center), |ui| {
        ui.label(RichText::new("Atmo").color(app.theme.mixer.label_color));
        ui.add_space(4.0);

        let available_width = ui.available_width();
        let half_width = available_width * 0.5;
        let fx_button_size = vec2(half_width, 20.0);

        ui.horizontal(|ui| {
            ui.add_space(half_width / 2.0);
            let fx_button = egui::Button::new(RichText::new("FX").monospace().size(12.0))
                .fill(app.theme.mixer.mute_off_bg)
                .sense(Sense::click_and_drag());
            let response = ui.add_sized(fx_button_size, fx_button);
            if response.clicked()
                || (response.drag_stopped()
                && response.drag_delta().length() < CLICK_DRAG_THRESHOLD)
            {
                fx_button_clicked = true;
            }
        });
        ui.add_space(2.0);

        ui.add_space(24.0); // Spacer to align with other strips

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

        if volume_fader(
            ui,
            &mut vol,
            app.displayed_atmo_peak_level,
            &app.theme,
            atmo_fader_bg,
            app.theme.mixer.meter_normal_color,
        )
            .dragged()
        {
            app.atmo_master_volume.store((vol * 1_000_000.0) as u32, Ordering::Relaxed);
        }
    });
    if fx_button_clicked {
        app.handle_fx_button_click(fx::InsertionPoint::Atmo);
    }
}

fn draw_metronome_strip(ui: &mut Ui, app: &mut CypherApp) {
    let metro_fader_bg = app.theme.mixer.fader_track_bg.gamma_multiply(3.0);
    let mut mute_button_clicked = false;
    let mut pitch_changed = false;
    let mut accent_pitch_changed = false;
    let mut volume_changed = false;

    let (is_muted, mut volume, pitch_hz, accent_pitch_hz) = {
        let mixer_state = app.track_mixer_state.read().unwrap();
        let metro = &mixer_state.metronome;
        (metro.is_muted, metro.volume, metro.pitch_hz, metro.accent_pitch_hz)
    };

    let mut pitch_fader_val = pitch_to_fader_value(pitch_hz);
    let mut accent_pitch_fader_val = pitch_to_fader_value(accent_pitch_hz);

    ui.with_layout(Layout::bottom_up(Align::Center), |ui| {
        // --- Bottom elements ---
        ui.label(RichText::new("Metronome").color(app.theme.mixer.label_color));
        ui.add_space(4.0);

        let available_width = ui.available_width();
        let button_size = vec2(available_width * 0.5, 20.0);
        ui.horizontal(|ui| {
            ui.add_space((available_width - button_size.x) / 2.0);
            let mute_button = egui::Button::new(RichText::new("M").monospace().size(12.0))
                .fill(if is_muted {
                    app.theme.mixer.mute_on_bg
                } else {
                    app.theme.mixer.mute_off_bg
                })
                .sense(Sense::click_and_drag());
            let response = ui.add_sized(button_size, mute_button);
            if response.clicked()
                || (response.drag_stopped()
                && response.drag_delta().length() < CLICK_DRAG_THRESHOLD)
            {
                mute_button_clicked = true;
            }
        });
        ui.add_space(4.0); // Space between Mute button and labels.

        // --- Labels and Readouts (in two separate rows) ---
        let fader_width = 20.0;
        let label_text_size = 10.0;
        let readout_bg = app.theme.mixer.fader_track_bg;
        let readout_text_color = app.theme.global_text_color;

        // ROW 2 (Bottom): The names ("Vol", "Pit", "Acc")
        ui.horizontal(|ui| {
            let spacing = ui.style().spacing.item_spacing.x;
            let total_fader_group_width = fader_width * 3.0 + spacing * 2.0;
            let side_margin = (ui.available_width() - total_fader_group_width).max(0.0) / 2.0;
            ui.add_space(side_margin);

            ui.scope(|ui| { ui.set_width(fader_width); ui.centered_and_justified(|ui| ui.label(RichText::new("Vol").monospace().size(label_text_size))); });
            ui.scope(|ui| { ui.set_width(fader_width); ui.centered_and_justified(|ui| ui.label(RichText::new("Pit").monospace().size(label_text_size))); });
            ui.scope(|ui| { ui.set_width(fader_width); ui.centered_and_justified(|ui| ui.label(RichText::new("Acc").monospace().size(label_text_size))); });
        });

        // ROW 1 (Top): The values
        ui.horizontal(|ui| {
            let spacing = ui.style().spacing.item_spacing.x;
            let total_fader_group_width = fader_width * 3.0 + spacing * 2.0;
            let side_margin = (ui.available_width() - total_fader_group_width).max(0.0) / 2.0;
            ui.add_space(side_margin);

            let db_text = {
                let db = linear_to_db(volume);
                if db.is_infinite() { "inf".to_string() } else { format!("{:.0}", db) }
            };
            let pitch_text = format!("{:.0}", pitch_hz);
            let accent_text = format!("{:.0}", accent_pitch_hz);

            ui.scope(|ui| { ui.set_width(fader_width); ui.centered_and_justified(|ui| ui.label(RichText::new(db_text).monospace().size(label_text_size).background_color(readout_bg).color(readout_text_color))); });
            ui.scope(|ui| { ui.set_width(fader_width); ui.centered_and_justified(|ui| ui.label(RichText::new(pitch_text).monospace().size(label_text_size).background_color(readout_bg).color(readout_text_color))); });
            ui.scope(|ui| { ui.set_width(fader_width); ui.centered_and_justified(|ui| ui.label(RichText::new(accent_text).monospace().size(label_text_size).background_color(readout_bg).color(readout_text_color))); });
        });

        ui.add_space(5.0); // Space between labels and faders

        // --- Top element (Faders) ---
        let fader_layout = Layout::left_to_right(Align::Min)
            .with_cross_align(Align::Min);

        ui.with_layout(fader_layout, |ui| {
            let spacing = ui.style().spacing.item_spacing.x;
            let total_fader_group_width = fader_width * 3.0 + spacing * 2.0;
            let side_margin = (ui.available_width() - total_fader_group_width).max(0.0) / 2.0;
            ui.add_space(side_margin);

            if volume_fader(ui, &mut volume, 0.0, &app.theme, metro_fader_bg, Color32::TRANSPARENT).dragged() {
                volume_changed = true;
            }
            if volume_fader(ui, &mut pitch_fader_val, 0.0, &app.theme, metro_fader_bg, Color32::TRANSPARENT).dragged() {
                pitch_changed = true;
            }
            if volume_fader(ui, &mut accent_pitch_fader_val, 0.0, &app.theme, metro_fader_bg, Color32::TRANSPARENT).dragged() {
                accent_pitch_changed = true;
            }
        });
    });

    if mute_button_clicked { app.send_command(AudioCommand::ToggleMetronomeMute); }
    if volume_changed { app.send_command(AudioCommand::SetMetronomeVolume(volume)); }
    if pitch_changed {
        let new_pitch_hz = fader_value_to_pitch(pitch_fader_val);
        app.send_command(AudioCommand::SetMetronomePitch(new_pitch_hz));
    }
    if accent_pitch_changed {
        let new_accent_pitch_hz = fader_value_to_pitch(accent_pitch_fader_val);
        app.send_command(AudioCommand::SetMetronomeAccentPitch(new_accent_pitch_hz));
    }
}

pub fn draw_mixer_panel(app: &mut CypherApp, ui: &mut Ui) {
    let frame_style = Frame::new().fill(app.theme.mixer.panel_background);
    ui.group(|ui| {
        frame_style.show(ui, |ui| {
            ui.set_min_height(300.0);
            ui.label(
                RichText::new("Mixer")
                    .monospace()
                    .color(app.theme.mixer.label_color),
            );
            ui.separator();

            let stroke = Stroke::new(1.0, ui.style().visuals.window_stroke.color);

            ui.columns(NUM_LOOPERS + 3, |columns| {
                // Draw Looper Tracks with separators
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

                // Draw Atmo Strip and its separator
                let atmo_column_ui = &mut columns[NUM_LOOPERS];
                let vline_y_range = atmo_column_ui.clip_rect().y_range();
                draw_atmo_strip(atmo_column_ui, app);

                // --- THIS IS THE ADDED CODE ---
                let painter = atmo_column_ui.painter();
                let rect = atmo_column_ui.min_rect();
                painter.vline(
                    rect.right() + atmo_column_ui.style().spacing.item_spacing.x / 2.0,
                    vline_y_range,
                    stroke,
                );
                // --- END OF ADDED CODE ---

                // Draw Metronome Strip (no separator needed after it, as Master draws one before)
                draw_metronome_strip(&mut columns[NUM_LOOPERS + 1], app);

                // Draw Master Strip with its separator
                let master_column_ui = &mut columns[NUM_LOOPERS + 2];
                let vline_y_range = master_column_ui.clip_rect().y_range();
                let painter = master_column_ui.painter();
                let rect = master_column_ui.min_rect();
                painter.vline(
                    rect.left() - master_column_ui.style().spacing.item_spacing.x / 2.0,
                    vline_y_range,
                    stroke,
                );
                draw_master_strip(master_column_ui, app);
            });
        });
    });
}