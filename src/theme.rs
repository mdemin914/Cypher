use crate::looper::NUM_LOOPERS;
use egui::{epaint, style, Color32, CornerRadius, Stroke, Visuals};
use serde::{Deserialize, Serialize};

// --- Helper Functions for Default Colors ---

// General
fn default_dark_mode() -> bool { true }
fn default_main_background() -> Color32 { Color32::from_gray(20) }
fn default_white() -> Color32 { Color32::WHITE }
fn default_black() -> Color32 { Color32::BLACK }
fn default_dark_gray() -> Color32 { Color32::from_gray(30) }
fn default_gray() -> Color32 { Color32::from_gray(60) }
fn default_light_gray() -> Color32 { Color32::from_gray(170) }

// Top Bar
fn default_top_bar_background() -> Color32 { Color32::from_gray(35) }
fn default_top_bar_button_bg() -> Color32 { Color32::from_gray(55) }
fn default_top_bar_text_color() -> Color32 { Color32::from_gray(200) }
fn default_top_bar_separator_color() -> Color32 { Color32::from_gray(80) }
fn default_transport_bar_fill() -> Color32 { Color32::from_rgb(100, 100, 200) }
fn default_transport_bar_background() -> Color32 { Color32::from_gray(20) }
fn default_xrun_text_color() -> Color32 { Color32::RED }

// Instrument Panel
fn default_instrument_panel_bg() -> Color32 { Color32::from_gray(45) }
fn default_instrument_button_bg() -> Color32 { Color32::from_gray(55) }
fn default_instrument_active_bg() -> Color32 { Color32::from_rgb(0, 100, 0) }
fn default_input_armed_bg() -> Color32 { Color32::from_rgb(139, 0, 0) }
fn default_input_monitor_bg() -> Color32 { Color32::from_rgb(0, 0, 139) }
fn default_fader_track_bg() -> Color32 { Color32::from_gray(20) }
fn default_fader_thumb_color() -> Color32 { Color32::from_gray(120) }
fn default_instrument_label_color() -> Color32 { Color32::from_gray(200) }

// Transport Controls
fn default_transport_panel_bg() -> Color32 { Color32::from_gray(45) }
fn default_transport_button_bg() -> Color32 { Color32::from_gray(55) }
fn default_transport_play_active_bg() -> Color32 { Color32::from_rgb(0, 120, 0) }
fn default_transport_clear_bg() -> Color32 { Color32::from_rgb(180, 50, 0) }
fn default_transport_label_color() -> Color32 { Color32::from_gray(200) }
fn default_transport_mute_active_bg() -> Color32 { Color32::from_rgb(0, 150, 255) } // New color for mute

// Loopers
fn default_looper_empty_bg() -> Color32 { Color32::from_gray(40) }
fn default_looper_armed_bg() -> Color32 { Color32::from_rgb(204, 204, 0) }
fn default_looper_recording_bg() -> Color32 { Color32::from_rgb(139, 0, 0) }
fn default_looper_overdubbing_bg() -> Color32 { Color32::from_rgb(255, 140, 0) }
fn default_looper_progress_bar_bg() -> Color32 { Color32::from_gray(80) }
fn default_looper_clear_button_bg() -> Color32 { Color32::from_gray(55) }
fn default_looper_text_color() -> Color32 { Color32::WHITE }
fn default_looper_track_colors() -> [Color32; NUM_LOOPERS] {
    [
        Color32::from_rgb(230, 25, 75),   // Red
        Color32::from_rgb(60, 180, 75),   // Green
        Color32::from_rgb(255, 225, 25),  // Yellow
        Color32::from_rgb(0, 130, 200),   // Blue
        Color32::from_rgb(245, 130, 48),  // Orange
        Color32::from_rgb(145, 30, 180),  // Purple
        Color32::from_rgb(70, 240, 240),  // Cyan
        Color32::from_rgb(240, 50, 230),  // Magenta
        Color32::from_rgb(210, 245, 60),  // Lime
        Color32::from_rgb(250, 190, 212), // Pink
        Color32::from_rgb(0, 128, 128),   // Teal
        Color32::from_rgb(220, 190, 255), // Lavender
    ]
}

// Mixer
fn default_mixer_panel_background() -> Color32 { Color32::from_gray(38) }
fn default_mixer_label_color() -> Color32 { Color32::from_gray(200) }
fn default_mute_on_bg() -> Color32 { Color32::from_rgb(255, 100, 0) }
fn default_solo_on_bg() -> Color32 { Color32::from_rgb(0, 150, 255) }
fn default_meter_normal_color() -> Color32 { Color32::from_rgb(0, 180, 0) }
fn default_meter_clip_color() -> Color32 { Color32::from_rgb(200, 0, 0) }
fn default_limiter_gr_color() -> Color32 { Color32::from_rgb(255, 140, 0) }
fn default_limiter_active_bg() -> Color32 { Color32::from_rgb(0, 100, 150) }

// Library
fn default_library_panel_background() -> Color32 { Color32::from_gray(32) }
fn default_library_button_bg() -> Color32 { Color32::from_gray(55) }
fn default_library_tab_active_bg() -> Color32 { Color32::from_rgb(145, 30, 180) }
fn default_library_tab_inactive_bg() -> Color32 { Color32::TRANSPARENT }
fn default_library_card_bg() -> Color32 { Color32::from_gray(50) }
fn default_library_card_hovered_bg() -> Color32 { Color32::from_gray(75) }
fn default_library_text_color() -> Color32 { Color32::from_gray(200) }

// Options Window
fn default_options_window_bg() -> Color32 { Color32::from_gray(38) }
fn default_options_heading_color() -> Color32 { Color32::WHITE }
fn default_options_label_color() -> Color32 { Color32::from_gray(200) }
fn default_options_widget_bg() -> Color32 { Color32::from_gray(55) }
fn default_options_slider_grab_color() -> Color32 { Color32::from_rgb(190, 80, 255) }
fn default_bpm_rounding_on_bg() -> Color32 { Color32::from_rgb(0, 150, 255) }

// Sampler Pad Window
fn default_sampler_pad_window_bg() -> Color32 { Color32::from_gray(38) }
fn default_pad_bg() -> Color32 { Color32::from_gray(40) }
fn default_pad_playing_outline_color() -> Color32 { Color32::WHITE }
fn default_pad_trash_hover_outline_color() -> Color32 { Color32::from_rgb(255, 215, 0) }
fn default_trash_mode_active_bg() -> Color32 { Color32::from_rgb(180, 0, 0) }
fn default_kit_button_bg() -> Color32 { Color32::from_gray(80) }
fn default_pad_outline_row_1_color() -> Color32 { Color32::from_rgb(200, 50, 50) }
fn default_pad_outline_row_2_color() -> Color32 { Color32::from_rgb(50, 200, 50) }
fn default_pad_outline_row_3_color() -> Color32 { Color32::from_rgb(50, 150, 200) }
fn default_pad_outline_row_4_color() -> Color32 { Color32::from_rgb(200, 50, 200) }

// Synth Editor
fn default_synth_editor_bg() -> Color32 { Color32::from_gray(28) }
fn default_synth_engine_panel_bg() -> Color32 { Color32::from_gray(35) }
fn default_synth_section_bg() -> Color32 { Color32::from_gray(42) }
fn default_visualizer_bg() -> Color32 { Color32::from_rgb(20, 20, 30) }
fn default_synth_label_color() -> Color32 { Color32::from_gray(200) }
fn default_control_bg() -> Color32 { Color32::from_gray(40) }
fn default_control_hover_bg() -> Color32 { Color32::from_gray(55) }
fn default_slider_track_color() -> Color32 { Color32::from_gray(30) }
fn default_slider_grab_color() -> Color32 { Color32::from_rgb(190, 80, 255) }
fn default_slider_grab_hover_color() -> Color32 { Color32::from_rgb(220, 120, 255) }
fn default_button_bg() -> Color32 { Color32::from_gray(80) }
fn default_button_hover_bg() -> Color32 { Color32::from_gray(100) }
fn default_button_active_bg() -> Color32 { Color32::from_gray(60) }
fn default_button_text_color() -> Color32 { Color32::WHITE }
fn default_tab_bg() -> Color32 { Color32::from_gray(50) }
fn default_tab_active_bg() -> Color32 { Color32::from_rgb(80, 60, 110) }
fn default_tab_text_color() -> Color32 { Color32::from_gray(200) }
fn default_combo_popup_bg() -> Color32 { Color32::from_gray(40) }
fn default_combo_selection_bg() -> Color32 { Color32::from_rgb(100, 80, 150) }
fn default_wt_slot_name_color() -> Color32 { Color32::LIGHT_BLUE }
fn default_wt_preview_active_waveform_color() -> Color32 { Color32::from_rgb(0, 150, 255) }
fn default_wt_preview_inactive_waveform_color() -> Color32 { Color32::from_gray(100) }
fn default_wt_preview_final_waveform_color() -> Color32 { Color32::from_rgb(255, 255, 255) }
fn default_wt_preview_bell_filtered_waveform_color() -> Color32 { Color32::from_rgb(0, 255, 128) }
fn default_mod_pitch_color() -> Color32 { Color32::from_rgb(255, 80, 80) }
fn default_mod_filter_color() -> Color32 { Color32::from_rgb(80, 160, 255) }
fn default_mod_amp_cold_color() -> Color32 { Color32::from_rgb(0, 180, 255) }
fn default_mod_amp_hot_color() -> Color32 { Color32::from_rgb(255, 140, 0) }

// --- New: Piano Keys ---
fn default_piano_white_key_color() -> Color32 { Color32::from_gray(240) }
fn default_piano_black_key_color() -> Color32 { Color32::from_gray(20) }
fn default_piano_played_key_color() -> Color32 { Color32::from_rgb(255, 0, 0) }
fn default_piano_outline_color() -> Color32 { Color32::from_gray(90) }

// --- New: Slicer Window ---
fn default_slicer_background() -> Color32 { Color32::from_gray(38) }
fn default_slicer_waveform_color() -> Color32 { Color32::from_rgb(150, 150, 255) }
fn default_slicer_waveform_bg_color() -> Color32 { Color32::from_gray(25) }
fn default_slicer_slice_marker_color() -> Color32 { Color32::from_rgb(255, 215, 0) }
fn default_slicer_label_color() -> Color32 { Color32::from_gray(200) }
fn default_slicer_button_bg() -> Color32 { Color32::from_gray(80) }
fn default_slicer_slider_track_color() -> Color32 { Color32::from_gray(50) }
fn default_slicer_slider_grab_color() -> Color32 { Color32::from_rgb(190, 80, 255) }
fn default_slicer_text_edit_bg() -> Color32 { Color32::from_gray(20) }

// --- New Hierarchical Theme Structs ---

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
    #[serde(default = "default_transport_bar_fill")] pub transport_bar_fill: Color32,
    #[serde(default = "default_transport_bar_background")] pub transport_bar_background: Color32,
    #[serde(default = "default_xrun_text_color")] pub xrun_text_color: Color32,
}
impl Default for TopBarTheme { fn default() -> Self { Self { background: default_top_bar_background(), button_bg: default_top_bar_button_bg(), text_color: default_top_bar_text_color(), separator_color: default_top_bar_separator_color(), transport_bar_fill: default_transport_bar_fill(), transport_bar_background: default_transport_bar_background(), xrun_text_color: default_xrun_text_color() } } }

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct InstrumentPanelTheme {
    #[serde(default = "default_instrument_panel_bg")] pub panel_background: Color32,
    #[serde(default = "default_instrument_button_bg")] pub button_bg: Color32,
    #[serde(default = "default_instrument_active_bg")] pub button_active_bg: Color32,
    #[serde(default = "default_input_armed_bg")] pub input_armed_bg: Color32,
    #[serde(default = "default_input_monitor_bg")] pub input_monitor_bg: Color32,
    #[serde(default = "default_fader_track_bg")] pub fader_track_bg: Color32,
    #[serde(default = "default_fader_thumb_color")] pub fader_thumb_color: Color32,
    #[serde(default = "default_instrument_label_color")] pub label_color: Color32,
}
impl Default for InstrumentPanelTheme { fn default() -> Self { Self { panel_background: default_instrument_panel_bg(), button_bg: default_instrument_button_bg(), button_active_bg: default_instrument_active_bg(), input_armed_bg: default_input_armed_bg(), input_monitor_bg: default_input_monitor_bg(), fader_track_bg: default_fader_track_bg(), fader_thumb_color: default_fader_thumb_color(), label_color: default_instrument_label_color() } } }

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct TransportControlTheme {
    #[serde(default = "default_transport_panel_bg")] pub panel_background: Color32,
    #[serde(default = "default_transport_button_bg")] pub button_bg: Color32,
    #[serde(default = "default_transport_play_active_bg")] pub play_active_bg: Color32,
    #[serde(default = "default_transport_mute_active_bg")] pub mute_active_bg: Color32,
    #[serde(default = "default_transport_clear_bg")] pub clear_button_bg: Color32,
    #[serde(default = "default_transport_label_color")] pub label_color: Color32,
}
impl Default for TransportControlTheme { fn default() -> Self { Self { panel_background: default_transport_panel_bg(), button_bg: default_transport_button_bg(), play_active_bg: default_transport_play_active_bg(), mute_active_bg: default_transport_mute_active_bg(), clear_button_bg: default_transport_clear_bg(), label_color: default_transport_label_color() } } }


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
    #[serde(default = "default_fader_track_bg")] pub fader_track_bg: Color32,
    #[serde(default = "default_fader_thumb_color")] pub fader_thumb_color: Color32,
    #[serde(default = "default_dark_gray")] pub mute_off_bg: Color32,
    #[serde(default = "default_mute_on_bg")] pub mute_on_bg: Color32,
    #[serde(default = "default_dark_gray")] pub solo_off_bg: Color32,
    #[serde(default = "default_solo_on_bg")] pub solo_on_bg: Color32,
    #[serde(default = "default_meter_normal_color")] pub meter_normal_color: Color32,
    #[serde(default = "default_meter_clip_color")] pub meter_clip_color: Color32,
    #[serde(default = "default_limiter_gr_color")] pub limiter_gr_color: Color32,
    #[serde(default = "default_limiter_active_bg")] pub limiter_active_bg: Color32,
}
impl Default for MixerTheme { fn default() -> Self { Self { panel_background: default_mixer_panel_background(), label_color: default_mixer_label_color(), fader_track_bg: default_fader_track_bg(), fader_thumb_color: default_fader_thumb_color(), mute_off_bg: default_dark_gray(), mute_on_bg: default_mute_on_bg(), solo_off_bg: default_dark_gray(), solo_on_bg: default_solo_on_bg(), meter_normal_color: default_meter_normal_color(), meter_clip_color: default_meter_clip_color(), limiter_gr_color: default_limiter_gr_color(), limiter_active_bg: default_limiter_active_bg() } } }

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
    #[serde(default = "default_bpm_rounding_on_bg")] pub bpm_rounding_on_bg: Color32,
}
impl Default for OptionsWindowTheme { fn default() -> Self { Self { background: default_options_window_bg(), heading_color: default_options_heading_color(), label_color: default_options_label_color(), widget_bg: default_options_widget_bg(), slider_grab_color: default_options_slider_grab_color(), bpm_rounding_on_bg: default_bpm_rounding_on_bg() } } }

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct SamplerPadWindowTheme {
    #[serde(default = "default_sampler_pad_window_bg")] pub background: Color32,
    #[serde(default = "default_pad_bg")] pub pad_bg_color: Color32,
    #[serde(default = "default_white")] pub pad_playing_outline_color: Color32,
    #[serde(default = "default_pad_trash_hover_outline_color")] pub pad_trash_hover_outline_color: Color32,
    #[serde(default = "default_trash_mode_active_bg")] pub trash_mode_active_bg: Color32,
    #[serde(default = "default_kit_button_bg")] pub kit_button_bg: Color32,
    #[serde(default = "default_pad_outline_row_1_color")] pub pad_outline_row_1_color: Color32,
    #[serde(default = "default_pad_outline_row_2_color")] pub pad_outline_row_2_color: Color32,
    #[serde(default = "default_pad_outline_row_3_color")] pub pad_outline_row_3_color: Color32,
    #[serde(default = "default_pad_outline_row_4_color")] pub pad_outline_row_4_color: Color32,
}
impl Default for SamplerPadWindowTheme { fn default() -> Self { Self { background: default_sampler_pad_window_bg(), pad_bg_color: default_pad_bg(), pad_playing_outline_color: default_white(), pad_trash_hover_outline_color: default_pad_trash_hover_outline_color(), trash_mode_active_bg: default_trash_mode_active_bg(), kit_button_bg: default_kit_button_bg(), pad_outline_row_1_color: default_pad_outline_row_1_color(), pad_outline_row_2_color: default_pad_outline_row_2_color(), pad_outline_row_3_color: default_pad_outline_row_3_color(), pad_outline_row_4_color: default_pad_outline_row_4_color() } } }

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct SynthEditorTheme {
    // General
    #[serde(default = "default_synth_editor_bg")] pub background: Color32,
    #[serde(default = "default_synth_engine_panel_bg")] pub engine_panel_bg: Color32,
    #[serde(default = "default_synth_section_bg")] pub section_bg: Color32,
    #[serde(default = "default_visualizer_bg")] pub visualizer_bg: Color32,
    #[serde(default = "default_synth_label_color")] pub label_color: Color32,

    // Controls (ComboBox, etc.)
    #[serde(default = "default_control_bg")] pub control_bg: Color32,
    #[serde(default = "default_control_hover_bg")] pub control_hover_bg: Color32,
    #[serde(default = "default_combo_popup_bg")] pub combo_popup_bg: Color32,
    #[serde(default = "default_combo_selection_bg")] pub combo_selection_bg: Color32,

    // Sliders
    #[serde(default = "default_slider_track_color")] pub slider_track_color: Color32,
    #[serde(default = "default_slider_grab_color")] pub slider_grab_color: Color32,
    #[serde(default = "default_slider_grab_hover_color")] pub slider_grab_hover_color: Color32,

    // Buttons
    #[serde(default = "default_button_bg")] pub button_bg: Color32,
    #[serde(default = "default_button_hover_bg")] pub button_hover_bg: Color32,
    #[serde(default = "default_button_active_bg")] pub button_active_bg: Color32,
    #[serde(default = "default_button_text_color")] pub button_text_color: Color32,

    // Tabs
    #[serde(default = "default_tab_bg")] pub tab_bg: Color32,
    #[serde(default = "default_tab_active_bg")] pub tab_active_bg: Color32,
    #[serde(default = "default_tab_text_color")] pub tab_text_color: Color32,

    // Wavetable specific
    #[serde(default = "default_wt_slot_name_color")] pub wt_slot_name_color: Color32,
    #[serde(default = "default_wt_preview_active_waveform_color")] pub wt_preview_active_waveform_color: Color32,
    #[serde(default = "default_wt_preview_inactive_waveform_color")] pub wt_preview_inactive_waveform_color: Color32,
    #[serde(default = "default_wt_preview_final_waveform_color")] pub wt_preview_final_waveform_color: Color32,
    #[serde(default = "default_wt_preview_bell_filtered_waveform_color")] pub wt_preview_bell_filtered_waveform_color: Color32,

    // Modulation visualizer colors
    #[serde(default = "default_mod_pitch_color")] pub mod_pitch_color: Color32,
    #[serde(default = "default_mod_filter_color")] pub mod_filter_color: Color32,
    #[serde(default = "default_mod_amp_cold_color")] pub mod_amp_cold_color: Color32,
    #[serde(default = "default_mod_amp_hot_color")] pub mod_amp_hot_color: Color32,
}
impl Default for SynthEditorTheme { fn default() -> Self { Self {
    background: default_synth_editor_bg(),
    engine_panel_bg: default_synth_engine_panel_bg(),
    section_bg: default_synth_section_bg(),
    visualizer_bg: default_visualizer_bg(),
    label_color: default_synth_label_color(),
    control_bg: default_control_bg(),
    control_hover_bg: default_control_hover_bg(),
    combo_popup_bg: default_combo_popup_bg(),
    combo_selection_bg: default_combo_selection_bg(),
    slider_track_color: default_slider_track_color(),
    slider_grab_color: default_slider_grab_color(),
    slider_grab_hover_color: default_slider_grab_hover_color(),
    button_bg: default_button_bg(),
    button_hover_bg: default_button_hover_bg(),
    button_active_bg: default_button_active_bg(),
    button_text_color: default_button_text_color(),
    tab_bg: default_tab_bg(),
    tab_active_bg: default_tab_active_bg(),
    tab_text_color: default_tab_text_color(),
    wt_slot_name_color: default_wt_slot_name_color(),
    wt_preview_active_waveform_color: default_wt_preview_active_waveform_color(),
    wt_preview_inactive_waveform_color: default_wt_preview_inactive_waveform_color(),
    wt_preview_final_waveform_color: default_wt_preview_final_waveform_color(),
    wt_preview_bell_filtered_waveform_color: default_wt_preview_bell_filtered_waveform_color(),
    mod_pitch_color: default_mod_pitch_color(),
    mod_filter_color: default_mod_filter_color(),
    mod_amp_cold_color: default_mod_amp_cold_color(),
    mod_amp_hot_color: default_mod_amp_hot_color(),
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


#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct Theme {
    #[serde(default = "default_dark_mode")] pub dark_mode: bool,
    #[serde(default = "default_main_background")] pub main_background: Color32,
    #[serde(default = "default_white")] pub global_text_color: Color32,
    #[serde(default = "default_dark_gray")] pub window_stroke_color: Color32,

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
}


/// Adapter to convert our custom Theme into egui::Visuals
/// This now only sets the most basic global properties.
/// The rest of the styling is applied directly in the UI code.
impl From<&Theme> for Visuals {
    fn from(theme: &Theme) -> Self {
        let mut visuals = if theme.dark_mode {
            Visuals::dark()
        } else {
            Visuals::light()
        };

        visuals.override_text_color = Some(theme.global_text_color);
        visuals.window_fill = default_black(); // Keep window background consistent
        visuals.panel_fill = default_dark_gray(); // For popups like combo box
        visuals.window_stroke = Stroke::new(1.0, theme.window_stroke_color);
        visuals.selection.bg_fill = theme.top_bar.transport_bar_fill; // Use a distinct selection color
        visuals.selection.stroke = Stroke::new(1.0, theme.global_text_color);

        // A generic default for un-themed widgets
        visuals.widgets.inactive.bg_fill = Color32::from_gray(50);
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, Color32::from_gray(80));
        visuals.widgets.hovered.bg_fill = Color32::from_gray(70);
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, Color32::from_gray(120));
        visuals.widgets.active.bg_fill = Color32::from_gray(90);
        visuals.widgets.active.bg_stroke = Stroke::new(1.0, Color32::WHITE);

        visuals.popup_shadow = epaint::Shadow::NONE;
        visuals.window_shadow = epaint::Shadow::NONE;
        visuals.collapsing_header_frame = true;

        visuals
    }
}