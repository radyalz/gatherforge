mod ai_models;
mod app;
mod components;
mod downloader;
mod instagram;
mod jobs;
mod library;
mod model;
mod reel_flow;
mod reel_previews;
mod theme;
mod util;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("GatherForge — Media Workspace")
            .with_inner_size([1370.0, 850.0])
            .with_min_inner_size([950.0, 660.0]),
        ..Default::default()
    };
    eframe::run_native(
        "GatherForge",
        options,
        Box::new(|cc| Ok(Box::new(app::GatherForge::new(cc)))),
    )
}
