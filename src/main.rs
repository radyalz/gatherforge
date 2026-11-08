mod app;
mod components;
mod downloader;
mod library;
mod model;
mod theme;
mod util;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("GatherForge — Collection Studio")
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
