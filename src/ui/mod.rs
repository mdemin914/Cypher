mod main_view;
mod options_view;
mod library_view;
mod mixer_view;
mod theme_editor_view;
mod eighty_eight_keys_view;
// Added

pub use main_view::draw_main_view;
pub use options_view::draw_options_window;
pub use library_view::{draw_library_panel, draw_sample_pad_window};
pub use mixer_view::draw_mixer_panel;
pub use theme_editor_view::draw_theme_editor_window; // Added