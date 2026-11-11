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
    // Native worker API consumed by the independent Rune UI. Model operations
    // are explicitly requested and use the same verified installer as egui.
    let args: Vec<String> = std::env::args().collect();
    if args.len() == 4 && args[1] == "--forge-model" {
        match ai_models::command_model(&args[3], &args[2]) {
            Ok(message) => {println!("{message}");return Ok(());}
            Err(error) => {eprintln!("{error}");std::process::exit(2);}
        }
    }

    let initial_page = args.windows(2)
        .find(|values| values[0] == "--forge-page")
        .map(|values| values[1].clone());
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("GatherForge — Forge Edition")
            .with_inner_size([1370.0, 850.0])
            .with_min_inner_size([950.0, 660.0]),
        ..Default::default()
    };
    eframe::run_native(
        "GatherForge",
        options,
        Box::new(move |cc| {
            let mut app = app::GatherForge::new(cc);
            if let Some(page) = initial_page.as_deref() { app.select_external_page(page); }
            Ok(Box::new(app))
        }),
    )
}
