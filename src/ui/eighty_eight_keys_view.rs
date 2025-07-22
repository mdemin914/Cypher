// src/ui/eighty_eight_keys_view.rs
use crate::app::{ChordDisplayMode, CypherApp, TheoryMode};
use crate::theory::{self, Scale};
use egui::{
    epaint, vec2, Align2, Color32, ComboBox, Frame, Pos2, Rect, RichText, Rounding, Sense, Shape,
    Stroke, Ui,
};
use std::collections::BTreeMap;

// MIDI note numbers for an 88-key piano (A0 to C8)
const FIRST_KEY: u8 = 21;
const LAST_KEY: u8 = 108;

/// Helper function to check if a MIDI note is a black key.
fn is_black_key(note: u8) -> bool {
    matches!(note % 12, 1 | 3 | 6 | 8 | 10)
}

/// Helper function to calculate the horizontal center of a given key.
fn get_key_center_x(note: u8, available_rect: &Rect, white_key_width: f32) -> f32 {
    let mut white_key_index_before = 0;
    // Count the number of white keys strictly before the target note.
    for k in FIRST_KEY..note {
        if !is_black_key(k) {
            white_key_index_before += 1;
        }
    }

    if is_black_key(note) {
        // A black key is visually centered on the line separating two white keys.
        // This position is the right edge of the preceding white key.
        available_rect.min.x + (white_key_index_before as f32 * white_key_width)
    } else {
        // A white key is centered in the middle of its own area.
        available_rect.min.x + (white_key_index_before as f32 * white_key_width) + (white_key_width / 2.0)
    }
}

pub fn draw_88_keys_panel(app: &mut CypherApp, ui: &mut Ui) {
    // --- Toolbar ---
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Mode:").color(app.theme.library.text_color));
            if ui
                .selectable_value(&mut app.theory_mode, TheoryMode::Scales, "Scales")
                .clicked()
            {
                app.displayed_theory_notes.clear();
            }
            if ui
                .selectable_value(&mut app.theory_mode, TheoryMode::Chords, "Chords")
                .clicked()
            {
                app.displayed_theory_notes.clear();
            }

            ui.separator();

            match app.theory_mode {
                TheoryMode::Scales => {
                    ui.label(RichText::new("Scale:").color(app.theme.library.text_color));
                    ComboBox::from_id_source("scale_selector")
                        .selected_text(app.selected_scale.to_string())
                        .show_ui(ui, |ui| {
                            for scale in Scale::ALL {
                                ui.selectable_value(
                                    &mut app.selected_scale,
                                    scale,
                                    scale.to_string(),
                                );
                            }
                        });
                }
                TheoryMode::Chords => {
                    ui.label(RichText::new("Display:").color(app.theme.library.text_color));
                    ui.selectable_value(
                        &mut app.chord_display_mode,
                        ChordDisplayMode::Spread,
                        "Spread",
                    );
                    ui.selectable_value(
                        &mut app.chord_display_mode,
                        ChordDisplayMode::Stacked,
                        "Stacked",
                    );

                    ui.separator();

                    ui.label(RichText::new("Style:").color(app.theme.library.text_color));
                    let selected_name = app.selected_chord_style.name.clone();
                    let mut style_to_load = None;

                    ComboBox::from_id_source("chord_style_selector")
                        .selected_text(selected_name)
                        .show_ui(ui, |ui| {
                            for (name, path) in &app.available_chord_styles {
                                if ui.selectable_label(&app.selected_chord_style.name == name, name).clicked() {
                                    style_to_load = Some(path.clone());
                                }
                            }
                        });

                    if let Some(path) = style_to_load {
                        app.load_chord_style(&path);
                    }
                }
            }
        });
        ui.separator();

        Frame::none()
            .inner_margin(egui::Margin::same(10))
            .show(ui, |ui| {
                draw_piano_keyboard(app, ui);
            });
    });
}

fn draw_piano_keyboard(app: &mut CypherApp, ui: &mut Ui) {
    let available_rect = ui.available_rect_before_wrap();
    let painter = ui.painter_at(available_rect);
    let theme = &app.theme.piano_keys;

    let white_keys: Vec<u8> = (FIRST_KEY..=LAST_KEY).filter(|&k| !is_black_key(k)).collect();
    let num_white_keys = white_keys.len();

    let white_key_width = available_rect.width() / num_white_keys as f32;
    let white_key_height = available_rect.height();
    let black_key_width = white_key_width * 0.6;
    let black_key_height = white_key_height * 0.6;

    let white_key_size = vec2(white_key_width, white_key_height);
    let black_key_size = vec2(black_key_width, black_key_height);

    let live_notes = app.live_midi_notes.read().unwrap();
    let is_stacked_mode =
        app.theory_mode == TheoryMode::Chords && app.chord_display_mode == ChordDisplayMode::Stacked;

    // --- Draw White Keys ---
    for (i,&note) in white_keys.iter().enumerate() {
        let key_x = available_rect.min.x + i as f32 * white_key_width;
        let key_rect = Rect::from_min_size(Pos2::new(key_x, available_rect.min.y), white_key_size);
        let mut fill_color = theme.white_key_color;

        // First, check for live notes being held down. This applies to ALL modes.
        if live_notes.contains(&note) {
            fill_color = theme.played_key_color;
        // If not a live note, check for suggestion colors, but ONLY in Spread Mode.
        } else if !is_stacked_mode {
            if let Some((_, color_index)) = app.displayed_theory_notes.iter().find(|(n, _)| *n == note) {
                fill_color = app.theme.loopers.track_colors[*color_index % app.theme.loopers.track_colors.len()];
            }
        }

        painter.rect(key_rect, Rounding::ZERO, fill_color, Stroke::new(1.0, theme.outline_color), epaint::StrokeKind::Inside);
    }

    // --- Draw Black Keys ---
    let mut white_key_index = 0;
    for note in FIRST_KEY..=LAST_KEY {
        if !is_black_key(note) {
            white_key_index += 1;
        } else {
            let key_x = available_rect.min.x + (white_key_index as f32 * white_key_width) - (black_key_width / 2.0);
            let key_rect = Rect::from_min_size(Pos2::new(key_x, available_rect.min.y), black_key_size);
            let mut fill_color = theme.black_key_color;

            // First, check for live notes being held down. This applies to ALL modes.
            if live_notes.contains(&note) {
                fill_color = theme.played_key_color;
            // If not a live note, check for suggestion colors, but ONLY in Spread Mode.
            } else if !is_stacked_mode {
                if let Some((_, color_index)) = app.displayed_theory_notes.iter().find(|(n, _)| *n == note) {
                    fill_color = app.theme.loopers.track_colors[*color_index % app.theme.loopers.track_colors.len()];
                }
            }

            painter.rect(key_rect, Rounding::ZERO, fill_color, Stroke::new(1.0, theme.outline_color), epaint::StrokeKind::Inside);
        }
    }

    // --- Draw Stacked Circles (if in stacked mode) ---
    if is_stacked_mode && !app.displayed_theory_notes.is_empty() {
        let mut notes_by_pitch_class: BTreeMap<u8, Vec<(u8, usize)>> = BTreeMap::new();
        for &(note, color_index) in &app.displayed_theory_notes {
            notes_by_pitch_class.entry(note % 12).or_default().push((note, color_index));
        }

        for (_pitch_class, notes_in_stack) in &notes_by_pitch_class {
            let anchor_note = notes_in_stack.iter().min_by_key( | (note,
            _) | note).unwrap().0;
            let key_center_x = get_key_center_x(anchor_note,
            &available_rect,
            white_key_width);
            let radius = white_key_width * 0.2;
            let stack_offset = radius * 2.2;
            let baseline_y = if is_black_key(anchor_note) {
            available_rect.min.y + black_key_height - radius - 5.0
            } else {
            available_rect.max.y - radius - 5.0
            };
            let mut sorted_stack = notes_in_stack.clone();
            sorted_stack.sort_by_key( | (note,
            _) | * note);
            for (i,
            &(_note,
            color_index)) in sorted_stack.iter().enumerate() {
            let y_pos = baseline_y - (i as f32 * stack_offset);
            let center = Pos2::new(key_center_x, y_pos);
            let color = app.theme.loopers.track_colors[color_index % app.theme.loopers.track_colors.len()];
            painter.circle(center, radius, color, Stroke::new(1.5, theme.outline_color));
            }
        }
    }
}