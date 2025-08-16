#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod asset;
mod audio_device;
mod audio_engine;
mod audio_io;
mod fx; // New
mod fx_components; // New
mod looper;
mod midi;
mod mixer;
mod preset;
mod sampler;
mod settings;
mod synth;
mod synth_view;
mod theme;
mod ui;
mod wavetable_engine;
mod sampler_engine;
mod theory;
mod slicer;
mod atmo;

use crate::app::CypherApp;

fn main() -> anyhow::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            // Setting maximized to true starts the window maximized.
            .with_maximized(true),
        ..Default::default()
    };

    let run_result = eframe::run_native(
        "Cypher Looper",
        native_options,
        Box::new(|cc| {
            let app = CypherApp::new(cc).expect("Failed to create CypherApp");
            Ok(Box::new(app))
        }),
    );

    if let Err(e) = run_result {
        return Err(anyhow::anyhow!("Eframe run error: {}", e));
    }

    Ok(())
}