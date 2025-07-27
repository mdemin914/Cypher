// src/ui/about_view.rs
use crate::app::CypherApp;
use egui::{Align, Frame, RichText, Ui, Window};

pub fn draw_about_window(app: &mut CypherApp, ctx: &egui::Context) {
    // --- Text Size Constants ---
    // You can easily adjust all text sizes for this window here.
    const HEADING_SIZE: f32 = 28.0;
    const VERSION_SIZE: f32 = 14.0;
    const LINK_SIZE: f32 = 14.0;
    const DESCRIPTION_SIZE: f32 = 15.0;
    const SUBHEADING_SIZE: f32 = 14.0;
    const LIST_ITEM_SIZE: f32 = 13.0;
    const CREDITS_SIZE: f32 = 12.0;

    let mut is_open = app.about_window_open;
    let theme = &app.theme.about_window;

    Window::new("About Cypher Looper")
        .open(&mut is_open)
        .collapsible(false)
        .resizable(false)
        .frame(Frame::window(&ctx.style()).fill(theme.background))
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new("Cypher Looper")
                        .color(theme.heading_color)
                        .size(HEADING_SIZE),
                );
                ui.label(
                    RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION")))
                        .color(theme.text_color)
                        .size(VERSION_SIZE)
                        .monospace(),
                );
                ui.add_space(5.0);
                ui.hyperlink_to(
                    RichText::new("github.com/WormJuice/Cypher")
                        .color(theme.link_color)
                        .size(LINK_SIZE),
                    "https://github.com/WormJuiceDev/Cypher",
                );
            });

            ui.add_space(15.0);
            ui.separator();
            ui.add_space(10.0);

            ui.with_layout(egui::Layout::top_down(Align::Min), |ui| {
                ui.label(
                    RichText::new(
                        "A multi-track live looper and dual-engine synthesizer/sampler built in Rust.",
                    )
                        .color(theme.text_color)
                        .size(DESCRIPTION_SIZE),
                );
                ui.add_space(15.0);
                ui.label(
                    RichText::new("Built with:")
                        .strong()
                        .color(theme.text_color)
                        .size(SUBHEADING_SIZE),
                );
                ui.label(RichText::new(" • eframe / egui").color(theme.text_color).size(LIST_ITEM_SIZE));
                ui.label(RichText::new(" • cpal").color(theme.text_color).size(LIST_ITEM_SIZE));
                ui.label(RichText::new(" • midir").color(theme.text_color).size(LIST_ITEM_SIZE));
                ui.label(RichText::new(" • rodio, rubato, hound").color(theme.text_color).size(LIST_ITEM_SIZE));

                ui.add_space(10.0);
                ui.label(RichText::new("AI-Assisted Developer: WormJuice").color(theme.text_color).size(CREDITS_SIZE));
                ui.label(RichText::new("Coder: Gemini 2.5 pro").color(theme.text_color).size(CREDITS_SIZE));
                ui.label(RichText::new("IDE: Jetbrains RustRover").color(theme.text_color).size(CREDITS_SIZE));
            });
        });

    app.about_window_open = is_open;
}