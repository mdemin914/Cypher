// src/theme.rs
use crate::looper::NUM_LOOPERS;
use egui::{epaint, Color32, CornerRadius, Stroke, Visuals};
use serde::{Deserialize, Serialize};

// --- Helper Functions for New Default Colors ---

// General
fn default_dark_mode() -> bool { true }
fn default_main_background() -> Color32 { Color32::from_rgba_unmultiplied(0, 5, 25, 255) }
fn default_global_text_color() -> Color32 { Color32::from_rgba_unmultiplied(255, 133, 0, 255) }
fn default_window_stroke_color() -> Color32 { Color32::from_rgba_unmultiplied(46, 0, 85, 85) }
fn default_white() -> Color32 { Color32::WHITE }
fn default_black() -> Color32 { Color32::BLACK }

// Top Bar
fn default_top_bar_background() -> Color32 { Color32::from_rgba_unmultiplied(0, 5, 23, 255) }
fn default_top_bar_button_bg() -> Color32 { Color32::from_rgba_unmultiplied(0, 25, 25, 255) }
fn default_top_bar_text_color() -> Color32 { Color32::from_rgba_unmultiplied(255, 133, 0, 255) }
fn default_top_bar_separator_color() -> Color32 { Color32::from_rgba_unmultiplied(91, 91, 91, 255) }
fn default_top_bar_transport_fill() -> Color32 { Color32::from_rgba_unmultiplied(0, 25, 25, 255) }
fn default_top_bar_transport_background() -> Color32 { Color32::from_rgba_unmultiplied(0, 0, 25, 255) }
fn default_top_bar_xrun_text() -> Color32 { Color32::from_rgba_unmultiplied(255, 0, 0, 255) }
fn default_top_bar_session_button_bg() -> Color32 { Color32::from_rgba_unmultiplied(98, 0, 83, 255) }
fn default_top_bar_session_save_as_bg() -> Color32 { Color32::from_rgba_unmultiplied(51, 0, 111, 255) }

// Instrument Panel
fn default_instrument_panel_bg() -> Color32 { Color32::from_rgba_unmultiplied(0, 5, 23, 255) }
fn default_instrument_button_bg() -> Color32 { Color32::from_rgba_unmultiplied(23, 0, 49, 255) }
fn default_instrument_active_bg() -> Color32 { Color32::from_rgba_unmultiplied(70, 0, 34, 255) }
fn default_instrument_input_armed_bg() -> Color32 { Color32::from_rgba_unmultiplied(139, 0, 0, 255) }
fn default_instrument_input_monitor_bg() -> Color32 { Color32::from_rgba_unmultiplied(0, 0, 139, 255) }
fn default_instrument_fader_track_bg() -> Color32 { Color32::from_rgba_unmultiplied(12, 0, 14, 255) }
fn default_instrument_fader_thumb_color() -> Color32 { Color32::from_rgba_unmultiplied(112, 0, 255, 255) }
fn default_instrument_label_color() -> Color32 { Color32::from_rgba_unmultiplied(142, 0, 255, 255) }

// Transport Controls
fn default_transport_panel_bg() -> Color32 { Color32::from_rgba_unmultiplied(0, 5, 23, 255) }
fn default_transport_button_bg() -> Color32 { Color32::from_rgba_unmultiplied(23, 0, 49, 255) }
fn default_transport_play_active_bg() -> Color32 { Color32::from_rgba_unmultiplied(23, 0, 0, 255) }
fn default_transport_clear_bg() -> Color32 { Color32::from_rgba_unmultiplied(180, 0, 81, 255) }
fn default_transport_label_color() -> Color32 { Color32::from_rgba_unmultiplied(142, 0, 255, 255) }
fn default_transport_mute_active_bg() -> Color32 { Color32::from_rgba_unmultiplied(255, 100, 0, 255) }
fn default_transport_record_button_bg() -> Color32 { Color32::from_rgba_unmultiplied(133, 0, 101, 255) }
fn default_transport_record_active_bg() -> Color32 { Color32::from_rgba_unmultiplied(180, 0, 0, 255) }

// Loopers
fn default_looper_empty_bg() -> Color32 { Color32::from_rgba_unmultiplied(0, 3, 15, 255) }
fn default_looper_armed_bg() -> Color32 { Color32::from_rgba_unmultiplied(19, 6, 0, 255) }
fn default_looper_recording_bg() -> Color32 { Color32::from_rgba_unmultiplied(139, 0, 0, 255) }
fn default_looper_overdubbing_bg() -> Color32 { Color32::from_rgba_unmultiplied(136, 0, 0, 255) }
fn default_looper_progress_bar_bg() -> Color32 { Color32::from_rgba_unmultiplied(20, 0, 78, 255) }
fn default_looper_clear_button_bg() -> Color32 { Color32::from_rgba_unmultiplied(13, 0, 25, 255) }
fn default_looper_text_color() -> Color32 { Color32::from_rgba_unmultiplied(255, 255, 255, 255) }
fn default_looper_track_colors() -> [Color32; NUM_LOOPERS] {
    [
        Color32::from_rgba_unmultiplied(255, 0, 79, 255),
        Color32::from_rgba_unmultiplied(0, 255, 69, 255),
        Color32::from_rgba_unmultiplied(255, 225, 25, 255),
        Color32::from_rgba_unmultiplied(0, 167, 255, 255),
        Color32::from_rgba_unmultiplied(255, 0, 0, 255),
        Color32::from_rgba_unmultiplied(205, 0, 255, 255),
        Color32::from_rgba_unmultiplied(0, 255, 255, 255),
        Color32::from_rgba_unmultiplied(156, 0, 253, 255),
        Color32::from_rgba_unmultiplied(255, 93, 0, 255),
        Color32::from_rgba_unmultiplied(255, 0, 155, 255),
        Color32::from_rgba_unmultiplied(0, 157, 157, 255),
        Color32::from_rgba_unmultiplied(255, 0, 209, 255),
    ]
}

// Mixer
fn default_mixer_panel_background() -> Color32 { Color32::from_rgba_unmultiplied(0, 0, 10, 255) }
fn default_mixer_label_color() -> Color32 { Color32::from_rgba_unmultiplied(200, 200, 200, 255) }
fn default_mixer_fader_track_bg() -> Color32 { Color32::from_rgba_unmultiplied(26, 0, 71, 255) }
fn default_mixer_fader_thumb_color() -> Color32 { Color32::from_rgba_unmultiplied(255, 149, 3, 255) }
fn default_mixer_button_off_bg() -> Color32 { Color32::from_rgba_unmultiplied(0, 25, 25, 255) }
fn default_mixer_mute_on_bg() -> Color32 { Color32::from_rgba_unmultiplied(27, 0, 85, 255) }
fn default_mixer_solo_on_bg() -> Color32 { Color32::from_rgba_unmultiplied(127, 0, 65, 255) }
fn default_mixer_meter_normal_color() -> Color32 { Color32::from_rgba_unmultiplied(255, 162, 0, 255) }
fn default_mixer_meter_clip_color() -> Color32 { Color32::from_rgba_unmultiplied(200, 0, 0, 255) }
fn default_mixer_limiter_gr_color() -> Color32 { Color32::from_rgba_unmultiplied(255, 0, 75, 255) }
fn default_mixer_limiter_active_bg() -> Color32 { Color32::from_rgba_unmultiplied(0, 25, 25, 255) }

// Library
fn default_library_panel_background() -> Color32 { Color32::from_rgba_unmultiplied(0, 2, 15, 255) }
fn default_library_button_bg() -> Color32 { Color32::from_rgba_unmultiplied(0, 25, 25, 255) }
fn default_library_tab_active_bg() -> Color32 { Color32::from_rgba_unmultiplied(74, 0, 27, 255) }
fn default_library_tab_inactive_bg() -> Color32 { Color32::from_rgba_unmultiplied(16, 0, 44, 255) }
fn default_library_card_bg() -> Color32 { Color32::from_rgba_unmultiplied(17, 0, 52, 255) }
fn default_library_card_hovered_bg() -> Color32 { Color32::from_rgba_unmultiplied(38, 0, 19, 255) }
fn default_library_text_color() -> Color32 { Color32::from_rgba_unmultiplied(255, 127, 0, 255) }

// Options Window
fn default_options_window_bg() -> Color32 { Color32::from_rgba_unmultiplied(0, 5, 25, 255) }
fn default_options_heading_color() -> Color32 { Color32::from_rgba_unmultiplied(119, 0, 255, 255) }
fn default_options_label_color() -> Color32 { Color32::from_rgba_unmultiplied(255, 139, 0, 255) }
fn default_options_widget_bg() -> Color32 { Color32::from_rgba_unmultiplied(26, 0, 77, 255) }
fn default_options_slider_grab_color() -> Color32 { Color32::from_rgba_unmultiplied(190, 80, 255, 255) }
fn default_options_bpm_rounding_on_bg() -> Color32 { Color32::from_rgba_unmultiplied(48, 0, 92, 255) }

// Sampler Pad Window
fn default_sampler_pad_window_bg() -> Color32 { Color32::from_rgba_unmultiplied(2, 0, 28, 255) }
fn default_sampler_pad_bg() -> Color32 { Color32::from_rgba_unmultiplied(6, 2, 16, 255) }
fn default_sampler_pad_playing_outline_color() -> Color32 { Color32::from_rgba_unmultiplied(106, 255, 0, 255) }
fn default_sampler_pad_trash_hover_outline_color() -> Color32 { Color32::from_rgba_unmultiplied(255, 215, 0, 255) }
fn default_sampler_pad_trash_mode_active_bg() -> Color32 { Color32::from_rgba_unmultiplied(180, 0, 0, 255) }
fn default_sampler_pad_kit_button_bg() -> Color32 { Color32::from_rgba_unmultiplied(0, 25, 25, 255) }
fn default_sampler_pad_outline_row_1_color() -> Color32 { Color32::from_rgba_unmultiplied(205, 37, 37, 255) }
fn default_sampler_pad_outline_row_2_color() -> Color32 { Color32::from_rgba_unmultiplied(0, 172, 0, 255) }
fn default_sampler_pad_outline_row_3_color() -> Color32 { Color32::from_rgba_unmultiplied(0, 126, 173, 255) }
fn default_sampler_pad_outline_row_4_color() -> Color32 { Color32::from_rgba_unmultiplied(188, 0, 188, 255) }

// Synth Editor
fn default_synth_editor_bg() -> Color32 { Color32::from_rgba_unmultiplied(6, 5, 23, 255) }
fn default_synth_editor_engine_panel_bg() -> Color32 { Color32::from_rgba_unmultiplied(18, 0, 57, 255) }
fn default_synth_editor_section_bg() -> Color32 { Color32::from_rgba_unmultiplied(14, 6, 28, 255) }
fn default_synth_editor_visualizer_bg() -> Color32 { Color32::from_rgba_unmultiplied(16, 2, 37, 255) }
fn default_synth_editor_label_color() -> Color32 { Color32::from_rgba_unmultiplied(200, 200, 200, 255) }
fn default_synth_editor_control_bg() -> Color32 { Color32::from_rgba_unmultiplied(175, 6, 6, 255) }
fn default_synth_editor_control_hover_bg() -> Color32 { Color32::from_rgba_unmultiplied(129, 150, 0, 255) }
fn default_synth_editor_slider_track_color() -> Color32 { Color32::from_rgba_unmultiplied(60, 0, 104, 255) }
fn default_synth_editor_slider_grab_color() -> Color32 { Color32::from_rgba_unmultiplied(190, 80, 255, 255) }
fn default_synth_editor_slider_grab_hover_color() -> Color32 { Color32::from_rgba_unmultiplied(215, 0, 141, 255) }
fn default_synth_editor_button_bg() -> Color32 { Color32::from_rgba_unmultiplied(23, 19, 63, 255) }
fn default_synth_editor_button_hover_bg() -> Color32 { Color32::from_rgba_unmultiplied(102, 0, 63, 255) }
fn default_synth_editor_button_active_bg() -> Color32 { Color32::from_rgba_unmultiplied(40, 0, 108, 255) }
fn default_synth_editor_button_text_color() -> Color32 { Color32::from_rgba_unmultiplied(255, 144, 0, 255) }
fn default_synth_editor_tab_bg() -> Color32 { Color32::from_rgba_unmultiplied(202, 83, 83, 255) }
fn default_synth_editor_tab_active_bg() -> Color32 { Color32::from_rgba_unmultiplied(0, 16, 83, 255) }
fn default_synth_editor_tab_text_color() -> Color32 { Color32::from_rgba_unmultiplied(255, 171, 0, 255) }
fn default_synth_editor_combo_popup_bg() -> Color32 { Color32::from_rgba_unmultiplied(255, 0, 246, 255) }
fn default_synth_editor_combo_selection_bg() -> Color32 { Color32::from_rgba_unmultiplied(69, 0, 102, 255) }
fn default_synth_editor_wt_slot_name_color() -> Color32 { Color32::from_rgba_unmultiplied(173, 216, 230, 255) }
fn default_synth_editor_wt_preview_active_waveform_color() -> Color32 { Color32::from_rgba_unmultiplied(0, 150, 255, 255) }
fn default_synth_editor_wt_preview_inactive_waveform_color() -> Color32 { Color32::from_rgba_unmultiplied(85, 88, 92, 255) }
fn default_synth_editor_wt_preview_final_waveform_color() -> Color32 { Color32::from_rgba_unmultiplied(109, 255, 0, 255) }
fn default_synth_editor_wt_preview_bell_filtered_waveform_color() -> Color32 { Color32::from_rgba_unmultiplied(255, 0, 218, 255) }
fn default_synth_editor_mod_pitch_color() -> Color32 { Color32::from_rgba_unmultiplied(255, 80, 80, 255) }
fn default_synth_editor_mod_filter_color() -> Color32 { Color32::from_rgba_unmultiplied(255, 163, 0, 255) }
fn default_synth_editor_mod_amp_cold_color() -> Color32 { Color32::from_rgba_unmultiplied(0, 180, 255, 255) }
fn default_synth_editor_mod_amp_hot_color() -> Color32 { Color32::from_rgba_unmultiplied(255, 140, 0, 255) }

// Piano Keys
fn default_piano_white_key_color() -> Color32 { Color32::from_rgba_unmultiplied(240, 240, 240, 255) }
fn default_piano_black_key_color() -> Color32 { Color32::from_rgba_unmultiplied(20, 20, 20, 255) }
fn default_piano_played_key_color() -> Color32 { Color32::from_rgba_unmultiplied(255, 0, 0, 255) }
fn default_piano_outline_color() -> Color32 { Color32::from_rgba_unmultiplied(90, 90, 90, 255) }

// Slicer Window
fn default_slicer_background() -> Color32 { Color32::from_rgba_unmultiplied(0, 5, 23, 255) }
fn default_slicer_waveform_color() -> Color32 { Color32::from_rgba_unmultiplied(91, 255, 0, 255) }
fn default_slicer_waveform_bg_color() -> Color32 { Color32::from_rgba_unmultiplied(5, 1, 21, 255) }
fn default_slicer_slice_marker_color() -> Color32 { Color32::from_rgba_unmultiplied(255, 114, 0, 255) }
fn default_slicer_label_color() -> Color32 { Color32::from_rgba_unmultiplied(200, 200, 200, 255) }
fn default_slicer_button_bg() -> Color32 { Color32::from_rgba_unmultiplied(13, 34, 13, 255) }
fn default_slicer_slider_track_color() -> Color32 { Color32::from_rgba_unmultiplied(8, 0, 20, 255) }
fn default_slicer_slider_grab_color() -> Color32 { Color32::from_rgba_unmultiplied(190, 80, 255, 255) }
fn default_slicer_text_edit_bg() -> Color32 { Color32::from_rgba_unmultiplied(8, 0, 40, 255) }

// MIDI Mapping Window
fn default_midi_mapping_background() -> Color32 { Color32::from_rgba_unmultiplied(0, 5, 25, 255) }
fn default_midi_mapping_label_color() -> Color32 { Color32::from_rgba_unmultiplied(255, 255, 255, 255) }
fn default_midi_mapping_button_bg() -> Color32 { Color32::from_rgba_unmultiplied(55, 0, 20, 255) }
fn default_midi_mapping_learn_button_bg() -> Color32 { Color32::from_rgba_unmultiplied(42, 0, 76, 255) }
fn default_midi_mapping_row_even_bg() -> Color32 { Color32::from_rgba_unmultiplied(78, 31, 45, 255) }
fn default_midi_mapping_row_odd_bg() -> Color32 { Color32::from_rgba_unmultiplied(0, 44, 60, 255) }
fn default_midi_mapping_header_bg() -> Color32 { Color32::from_rgba_unmultiplied(19, 0, 54, 255) }

// --- NEW DEFAULTS FOR ABOUT WINDOW ---
fn default_about_window_bg() -> Color32 { Color32::from_rgba_unmultiplied(15, 10, 35, 255) }
fn default_about_window_heading() -> Color32 { Color32::from_rgba_unmultiplied(255, 133, 0, 255) }
fn default_about_window_text() -> Color32 { Color32::from_rgba_unmultiplied(200, 200, 220, 255) }
fn default_about_window_link() -> Color32 { Color32::from_rgba_unmultiplied(110, 180, 255, 255) }


// --- Hierarchical Theme Structs ---

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct PianoKeyTheme {
    #[serde(default = "default_piano_white_key_color")] pub white_key_color: Color32,
    #[serde(default = "default_piano_black_key_color")] pub black_key_color: Color32,
    #[serde(default = "default_piano_played_key_color")] pub played_key_color: Color32,
    #[serde(default = "default_piano_outline_color")] pub outline_color: Color32,
}
impl Default for PianoKeyTheme { fn default() -> Self { Self { white_key_color: default_piano_white_key_color(), black_key_color: default_piano_black_key_color(), played_key_color: default_piano_played_key_color(), outline_color: default_piano_outline_color() } } }

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct TopBarTheme {
    #[serde(default = "default_top_bar_background")] pub background: Color32,
    #[serde(default = "default_top_bar_button_bg")] pub button_bg: Color32,
    #[serde(default = "default_top_bar_text_color")] pub text_color: Color32,
    #[serde(default = "default_top_bar_separator_color")] pub separator_color: Color32,
    #[serde(default = "default_top_bar_transport_fill")] pub transport_bar_fill: Color32,
    #[serde(default = "default_top_bar_transport_background")] pub transport_bar_background: Color32,
    #[serde(default = "default_top_bar_xrun_text")] pub xrun_text_color: Color32,
    #[serde(default = "default_top_bar_session_button_bg")] pub session_button_bg: Color32,
    #[serde(default = "default_top_bar_session_save_as_bg")] pub session_save_as_button_bg: Color32,
}
impl Default for TopBarTheme { fn default() -> Self { Self { background: default_top_bar_background(), button_bg: default_top_bar_button_bg(), text_color: default_top_bar_text_color(), separator_color: default_top_bar_separator_color(), transport_bar_fill: default_top_bar_transport_fill(), transport_bar_background: default_top_bar_transport_background(), xrun_text_color: default_top_bar_xrun_text(), session_button_bg: default_top_bar_session_button_bg(), session_save_as_button_bg: default_top_bar_session_save_as_bg() } } }

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct InstrumentPanelTheme {
    #[serde(default = "default_instrument_panel_bg")] pub panel_background: Color32,
    #[serde(default = "default_instrument_button_bg")] pub button_bg: Color32,
    #[serde(default = "default_instrument_active_bg")] pub button_active_bg: Color32,
    #[serde(default = "default_instrument_input_armed_bg")] pub input_armed_bg: Color32,
    #[serde(default = "default_instrument_input_monitor_bg")] pub input_monitor_bg: Color32,
    #[serde(default = "default_instrument_fader_track_bg")] pub fader_track_bg: Color32,
    #[serde(default = "default_instrument_fader_thumb_color")] pub fader_thumb_color: Color32,
    #[serde(default = "default_instrument_label_color")] pub label_color: Color32,
}
impl Default for InstrumentPanelTheme { fn default() -> Self { Self { panel_background: default_instrument_panel_bg(), button_bg: default_instrument_button_bg(), button_active_bg: default_instrument_active_bg(), input_armed_bg: default_instrument_input_armed_bg(), input_monitor_bg: default_instrument_input_monitor_bg(), fader_track_bg: default_instrument_fader_track_bg(), fader_thumb_color: default_instrument_fader_thumb_color(), label_color: default_instrument_label_color() } } }

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct TransportControlTheme {
    #[serde(default = "default_transport_panel_bg")] pub panel_background: Color32,
    #[serde(default = "default_transport_button_bg")] pub button_bg: Color32,
    #[serde(default = "default_transport_play_active_bg")] pub play_active_bg: Color32,
    #[serde(default = "default_transport_mute_active_bg")] pub mute_active_bg: Color32,
    #[serde(default = "default_transport_clear_bg")] pub clear_button_bg: Color32,
    #[serde(default = "default_transport_label_color")] pub label_color: Color32,
    #[serde(default = "default_transport_record_button_bg")] pub record_button_bg: Color32,
    #[serde(default = "default_transport_record_active_bg")] pub record_active_bg: Color32,
}
impl Default for TransportControlTheme { fn default() -> Self { Self { panel_background: default_transport_panel_bg(), button_bg: default_transport_button_bg(), play_active_bg: default_transport_play_active_bg(), mute_active_bg: default_transport_mute_active_bg(), clear_button_bg: default_transport_clear_bg(), label_color: default_transport_label_color(), record_button_bg: default_transport_record_button_bg(), record_active_bg: default_transport_record_active_bg() } } }

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct LooperTheme {
    #[serde(default = "default_looper_empty_bg")] pub empty_bg: Color32,
    #[serde(default = "default_looper_armed_bg")] pub armed_bg: Color32,
    #[serde(default = "default_looper_recording_bg")] pub recording_bg: Color32,
    #[serde(default = "default_looper_overdubbing_bg")] pub overdubbing_bg: Color32,
    #[serde(default = "default_looper_progress_bar_bg")] pub progress_bar_bg: Color32,
    #[serde(default = "default_looper_clear_button_bg")] pub clear_button_bg: Color32,
    #[serde(default = "default_looper_text_color")] pub text_color: Color32,
    #[serde(default = "default_looper_track_colors")] pub track_colors: [Color32; NUM_LOOPERS],
}
impl Default for LooperTheme { fn default() -> Self { Self { empty_bg: default_looper_empty_bg(), armed_bg: default_looper_armed_bg(), recording_bg: default_looper_recording_bg(), overdubbing_bg: default_looper_overdubbing_bg(), progress_bar_bg: default_looper_progress_bar_bg(), clear_button_bg: default_looper_clear_button_bg(), text_color: default_looper_text_color(), track_colors: default_looper_track_colors() } } }

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct MixerTheme {
    #[serde(default = "default_mixer_panel_background")] pub panel_background: Color32,
    #[serde(default = "default_mixer_label_color")] pub label_color: Color32,
    #[serde(default = "default_mixer_fader_track_bg")] pub fader_track_bg: Color32,
    #[serde(default = "default_mixer_fader_thumb_color")] pub fader_thumb_color: Color32,
    #[serde(default = "default_mixer_button_off_bg")] pub mute_off_bg: Color32,
    #[serde(default = "default_mixer_mute_on_bg")] pub mute_on_bg: Color32,
    #[serde(default = "default_mixer_button_off_bg")] pub solo_off_bg: Color32,
    #[serde(default = "default_mixer_solo_on_bg")] pub solo_on_bg: Color32,
    #[serde(default = "default_mixer_meter_normal_color")] pub meter_normal_color: Color32,
    #[serde(default = "default_mixer_meter_clip_color")] pub meter_clip_color: Color32,
    #[serde(default = "default_mixer_limiter_gr_color")] pub limiter_gr_color: Color32,
    #[serde(default = "default_mixer_limiter_active_bg")] pub limiter_active_bg: Color32,
}
impl Default for MixerTheme { fn default() -> Self { Self { panel_background: default_mixer_panel_background(), label_color: default_mixer_label_color(), fader_track_bg: default_mixer_fader_track_bg(), fader_thumb_color: default_mixer_fader_thumb_color(), mute_off_bg: default_mixer_button_off_bg(), mute_on_bg: default_mixer_mute_on_bg(), solo_off_bg: default_mixer_button_off_bg(), solo_on_bg: default_mixer_solo_on_bg(), meter_normal_color: default_mixer_meter_normal_color(), meter_clip_color: default_mixer_meter_clip_color(), limiter_gr_color: default_mixer_limiter_gr_color(), limiter_active_bg: default_mixer_limiter_active_bg() } } }

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct LibraryTheme {
    #[serde(default = "default_library_panel_background")] pub panel_background: Color32,
    #[serde(default = "default_library_button_bg")] pub button_bg: Color32,
    #[serde(default = "default_library_tab_active_bg")] pub tab_active_bg: Color32,
    #[serde(default = "default_library_tab_inactive_bg")] pub tab_inactive_bg: Color32,
    #[serde(default = "default_library_card_bg")] pub card_bg: Color32,
    #[serde(default = "default_library_card_hovered_bg")] pub card_hovered_bg: Color32,
    #[serde(default = "default_library_text_color")] pub text_color: Color32,
}
impl Default for LibraryTheme { fn default() -> Self { Self { panel_background: default_library_panel_background(), button_bg: default_library_button_bg(), tab_active_bg: default_library_tab_active_bg(), tab_inactive_bg: default_library_tab_inactive_bg(), card_bg: default_library_card_bg(), card_hovered_bg: default_library_card_hovered_bg(), text_color: default_library_text_color() } } }

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct OptionsWindowTheme {
    #[serde(default = "default_options_window_bg")] pub background: Color32,
    #[serde(default = "default_options_heading_color")] pub heading_color: Color32,
    #[serde(default = "default_options_label_color")] pub label_color: Color32,
    #[serde(default = "default_options_widget_bg")] pub widget_bg: Color32,
    #[serde(default = "default_options_slider_grab_color")] pub slider_grab_color: Color32,
    #[serde(default = "default_options_bpm_rounding_on_bg")] pub bpm_rounding_on_bg: Color32,
}
impl Default for OptionsWindowTheme { fn default() -> Self { Self { background: default_options_window_bg(), heading_color: default_options_heading_color(), label_color: default_options_label_color(), widget_bg: default_options_widget_bg(), slider_grab_color: default_options_slider_grab_color(), bpm_rounding_on_bg: default_options_bpm_rounding_on_bg() } } }

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct SamplerPadWindowTheme {
    #[serde(default = "default_sampler_pad_window_bg")] pub background: Color32,
    #[serde(default = "default_sampler_pad_bg")] pub pad_bg_color: Color32,
    #[serde(default = "default_sampler_pad_playing_outline_color")] pub pad_playing_outline_color: Color32,
    #[serde(default = "default_sampler_pad_trash_hover_outline_color")] pub pad_trash_hover_outline_color: Color32,
    #[serde(default = "default_sampler_pad_trash_mode_active_bg")] pub trash_mode_active_bg: Color32,
    #[serde(default = "default_sampler_pad_kit_button_bg")] pub kit_button_bg: Color32,
    #[serde(default = "default_sampler_pad_outline_row_1_color")] pub pad_outline_row_1_color: Color32,
    #[serde(default = "default_sampler_pad_outline_row_2_color")] pub pad_outline_row_2_color: Color32,
    #[serde(default = "default_sampler_pad_outline_row_3_color")] pub pad_outline_row_3_color: Color32,
    #[serde(default = "default_sampler_pad_outline_row_4_color")] pub pad_outline_row_4_color: Color32,
}
impl Default for SamplerPadWindowTheme { fn default() -> Self { Self { background: default_sampler_pad_window_bg(), pad_bg_color: default_sampler_pad_bg(), pad_playing_outline_color: default_sampler_pad_playing_outline_color(), pad_trash_hover_outline_color: default_sampler_pad_trash_hover_outline_color(), trash_mode_active_bg: default_sampler_pad_trash_mode_active_bg(), kit_button_bg: default_sampler_pad_kit_button_bg(), pad_outline_row_1_color: default_sampler_pad_outline_row_1_color(), pad_outline_row_2_color: default_sampler_pad_outline_row_2_color(), pad_outline_row_3_color: default_sampler_pad_outline_row_3_color(), pad_outline_row_4_color: default_sampler_pad_outline_row_4_color() } } }

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct SynthEditorTheme {
    #[serde(default = "default_synth_editor_bg")] pub background: Color32,
    #[serde(default = "default_synth_editor_engine_panel_bg")] pub engine_panel_bg: Color32,
    #[serde(default = "default_synth_editor_section_bg")] pub section_bg: Color32,
    #[serde(default = "default_synth_editor_visualizer_bg")] pub visualizer_bg: Color32,
    #[serde(default = "default_synth_editor_label_color")] pub label_color: Color32,
    #[serde(default = "default_synth_editor_control_bg")] pub control_bg: Color32,
    #[serde(default = "default_synth_editor_control_hover_bg")] pub control_hover_bg: Color32,
    #[serde(default = "default_synth_editor_combo_popup_bg")] pub combo_popup_bg: Color32,
    #[serde(default = "default_synth_editor_combo_selection_bg")] pub combo_selection_bg: Color32,
    #[serde(default = "default_synth_editor_slider_track_color")] pub slider_track_color: Color32,
    #[serde(default = "default_synth_editor_slider_grab_color")] pub slider_grab_color: Color32,
    #[serde(default = "default_synth_editor_slider_grab_hover_color")] pub slider_grab_hover_color: Color32,
    #[serde(default = "default_synth_editor_button_bg")] pub button_bg: Color32,
    #[serde(default = "default_synth_editor_button_hover_bg")] pub button_hover_bg: Color32,
    #[serde(default = "default_synth_editor_button_active_bg")] pub button_active_bg: Color32,
    #[serde(default = "default_synth_editor_button_text_color")] pub button_text_color: Color32,
    #[serde(default = "default_synth_editor_tab_bg")] pub tab_bg: Color32,
    #[serde(default = "default_synth_editor_tab_active_bg")] pub tab_active_bg: Color32,
    #[serde(default = "default_synth_editor_tab_text_color")] pub tab_text_color: Color32,
    #[serde(default = "default_synth_editor_wt_slot_name_color")] pub wt_slot_name_color: Color32,
    #[serde(default = "default_synth_editor_wt_preview_active_waveform_color")] pub wt_preview_active_waveform_color: Color32,
    #[serde(default = "default_synth_editor_wt_preview_inactive_waveform_color")] pub wt_preview_inactive_waveform_color: Color32,
    #[serde(default = "default_synth_editor_wt_preview_final_waveform_color")] pub wt_preview_final_waveform_color: Color32,
    #[serde(default = "default_synth_editor_wt_preview_bell_filtered_waveform_color")] pub wt_preview_bell_filtered_waveform_color: Color32,
    #[serde(default = "default_synth_editor_mod_pitch_color")] pub mod_pitch_color: Color32,
    #[serde(default = "default_synth_editor_mod_filter_color")] pub mod_filter_color: Color32,
    #[serde(default = "default_synth_editor_mod_amp_cold_color")] pub mod_amp_cold_color: Color32,
    #[serde(default = "default_synth_editor_mod_amp_hot_color")] pub mod_amp_hot_color: Color32,
}
impl Default for SynthEditorTheme { fn default() -> Self { Self {
    background: default_synth_editor_bg(),
    engine_panel_bg: default_synth_editor_engine_panel_bg(),
    section_bg: default_synth_editor_section_bg(),
    visualizer_bg: default_synth_editor_visualizer_bg(),
    label_color: default_synth_editor_label_color(),
    control_bg: default_synth_editor_control_bg(),
    control_hover_bg: default_synth_editor_control_hover_bg(),
    combo_popup_bg: default_synth_editor_combo_popup_bg(),
    combo_selection_bg: default_synth_editor_combo_selection_bg(),
    slider_track_color: default_synth_editor_slider_track_color(),
    slider_grab_color: default_synth_editor_slider_grab_color(),
    slider_grab_hover_color: default_synth_editor_slider_grab_hover_color(),
    button_bg: default_synth_editor_button_bg(),
    button_hover_bg: default_synth_editor_button_hover_bg(),
    button_active_bg: default_synth_editor_button_active_bg(),
    button_text_color: default_synth_editor_button_text_color(),
    tab_bg: default_synth_editor_tab_bg(),
    tab_active_bg: default_synth_editor_tab_active_bg(),
    tab_text_color: default_synth_editor_tab_text_color(),
    wt_slot_name_color: default_synth_editor_wt_slot_name_color(),
    wt_preview_active_waveform_color: default_synth_editor_wt_preview_active_waveform_color(),
    wt_preview_inactive_waveform_color: default_synth_editor_wt_preview_inactive_waveform_color(),
    wt_preview_final_waveform_color: default_synth_editor_wt_preview_final_waveform_color(),
    wt_preview_bell_filtered_waveform_color: default_synth_editor_wt_preview_bell_filtered_waveform_color(),
    mod_pitch_color: default_synth_editor_mod_pitch_color(),
    mod_filter_color: default_synth_editor_mod_filter_color(),
    mod_amp_cold_color: default_synth_editor_mod_amp_cold_color(),
    mod_amp_hot_color: default_synth_editor_mod_amp_hot_color(),
} } }

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct SlicerWindowTheme {
    #[serde(default = "default_slicer_background")] pub background: Color32,
    #[serde(default = "default_slicer_waveform_color")] pub waveform_color: Color32,
    #[serde(default = "default_slicer_waveform_bg_color")] pub waveform_bg_color: Color32,
    #[serde(default = "default_slicer_slice_marker_color")] pub slice_marker_color: Color32,
    #[serde(default = "default_slicer_label_color")] pub label_color: Color32,
    #[serde(default = "default_slicer_button_bg")] pub button_bg: Color32,
    #[serde(default = "default_slicer_slider_track_color")] pub slider_track_color: Color32,
    #[serde(default = "default_slicer_slider_grab_color")] pub slider_grab_color: Color32,
    #[serde(default = "default_slicer_text_edit_bg")] pub text_edit_bg: Color32,
}
impl Default for SlicerWindowTheme { fn default() -> Self { Self { background: default_slicer_background(), waveform_color: default_slicer_waveform_color(), waveform_bg_color: default_slicer_waveform_bg_color(), slice_marker_color: default_slicer_slice_marker_color(), label_color: default_slicer_label_color(), button_bg: default_slicer_button_bg(), slider_track_color: default_slicer_slider_track_color(), slider_grab_color: default_slicer_slider_grab_color(), text_edit_bg: default_slicer_text_edit_bg(), } } }

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct MidiMappingTheme {
    #[serde(default = "default_midi_mapping_background")] pub background: Color32,
    #[serde(default = "default_midi_mapping_label_color")] pub label_color: Color32,
    #[serde(default = "default_midi_mapping_button_bg")] pub button_bg: Color32,
    #[serde(default = "default_midi_mapping_learn_button_bg")] pub learn_button_bg: Color32,
    #[serde(default = "default_midi_mapping_row_even_bg")] pub row_even_bg: Color32,
    #[serde(default = "default_midi_mapping_row_odd_bg")] pub row_odd_bg: Color32,
    #[serde(default = "default_midi_mapping_header_bg")] pub header_bg: Color32,
}
impl Default for MidiMappingTheme { fn default() -> Self { Self { background: default_midi_mapping_background(), label_color: default_midi_mapping_label_color(), button_bg: default_midi_mapping_button_bg(), learn_button_bg: default_midi_mapping_learn_button_bg(), row_even_bg: default_midi_mapping_row_even_bg(), row_odd_bg: default_midi_mapping_row_odd_bg(), header_bg: default_midi_mapping_header_bg(), } } }

// --- NEW THEME STRUCT ---
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct AboutWindowTheme {
    #[serde(default = "default_about_window_bg")] pub background: Color32,
    #[serde(default = "default_about_window_heading")] pub heading_color: Color32,
    #[serde(default = "default_about_window_text")] pub text_color: Color32,
    #[serde(default = "default_about_window_link")] pub link_color: Color32,
}
impl Default for AboutWindowTheme {
    fn default() -> Self {
        Self {
            background: default_about_window_bg(),
            heading_color: default_about_window_heading(),
            text_color: default_about_window_text(),
            link_color: default_about_window_link(),
        }
    }
}

// --- MAIN THEME STRUCT ---
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct Theme {
    #[serde(default = "default_dark_mode")] pub dark_mode: bool,
    #[serde(default = "default_main_background")] pub main_background: Color32,
    #[serde(default = "default_global_text_color")] pub global_text_color: Color32,
    #[serde(default = "default_window_stroke_color")] pub window_stroke_color: Color32,

    pub top_bar: TopBarTheme,
    pub instrument_panel: InstrumentPanelTheme,
    pub transport_controls: TransportControlTheme,
    pub loopers: LooperTheme,
    pub mixer: MixerTheme,
    pub library: LibraryTheme,
    pub options_window: OptionsWindowTheme,
    pub sampler_pad_window: SamplerPadWindowTheme,
    pub synth_editor_window: SynthEditorTheme,
    pub piano_keys: PianoKeyTheme,
    pub slicer_window: SlicerWindowTheme,
    pub midi_mapping_window: MidiMappingTheme,
    pub about_window: AboutWindowTheme, // <-- ADDED
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            dark_mode: default_dark_mode(),
            main_background: default_main_background(),
            global_text_color: default_global_text_color(),
            window_stroke_color: default_window_stroke_color(),
            top_bar: Default::default(),
            instrument_panel: Default::default(),
            transport_controls: Default::default(),
            loopers: Default::default(),
            mixer: Default::default(),
            library: Default::default(),
            options_window: Default::default(),
            sampler_pad_window: Default::default(),
            synth_editor_window: Default::default(),
            piano_keys: Default::default(),
            slicer_window: Default::default(),
            midi_mapping_window: Default::default(),
            about_window: Default::default(), // <-- ADDED
        }
    }
}

impl From<&Theme> for Visuals {
    fn from(theme: &Theme) -> Self {
        let mut visuals = if theme.dark_mode {
            Visuals::dark()
        } else {
            Visuals::light()
        };

        visuals.override_text_color = Some(theme.global_text_color);
        visuals.hyperlink_color = theme.about_window.link_color; // <-- ADDED
        visuals.window_fill = default_black();
        visuals.panel_fill = theme.mixer.panel_background;
        visuals.window_stroke = Stroke::new(1.0, theme.window_stroke_color);
        visuals.selection.bg_fill = theme.top_bar.transport_bar_fill;
        visuals.selection.stroke = Stroke::new(1.0, theme.global_text_color);

        visuals.widgets.inactive.bg_fill = theme.instrument_panel.button_bg;
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, Color32::from_gray(80));
        visuals.widgets.hovered.bg_fill = theme.instrument_panel.button_bg.linear_multiply(1.5);
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, Color32::from_gray(120));
        visuals.widgets.active.bg_fill = theme.instrument_panel.button_active_bg;
        visuals.widgets.active.bg_stroke = Stroke::new(1.0, Color32::WHITE);

        visuals.popup_shadow = epaint::Shadow::NONE;
        visuals.window_shadow = epaint::Shadow::NONE;
        visuals.collapsing_header_frame = true;

        visuals
    }
}