use eframe::egui;
use std::{
    collections::HashSet,
    fs,
    path::PathBuf,
    process::Command,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};
use walkdir::WalkDir;

struct Track {
    artist: String,
    title: String,
    duplicate: bool,
}

struct GatherForge {
    root: String,
    urls: String,
    search: String,
    tab: usize,
    tracks: Vec<Track>,
    status: String,
    log: Arc<Mutex<String>>,
    busy: Arc<AtomicBool>,
}

impl Default for GatherForge {
    fn default() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        Self {
            root: format!("{home}/Downloads/Music"),
            urls: String::new(),
            search: String::new(),
            tab: 0,
            tracks: Vec::new(),
            status: "Ready".into(),
            log: Arc::new(Mutex::new(String::new())),
            busy: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl GatherForge {
    fn scan(&mut self) {
        let root = PathBuf::from(&self.root);
        if !root.is_dir() {
            self.status = "Select a valid music folder".into();
            return;
        }

        let mut seen = HashSet::new();
        self.tracks.clear();

        for entry in WalkDir::new(&root)
            .into_iter()
            .filter_entry(|e| !matches!(e.file_name().to_str(), Some(".git" | "target" | "xxx")))
            .flatten()
        {
            if !entry.file_type().is_file() {
                continue;
            }

            let path = entry.path();
            let ext = path
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_lowercase();

            if !["mp3", "flac", "wav", "m4a", "opus", "ogg", "aac"].contains(&ext.as_str()) {
                continue;
            }

            let stem = path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();

            let (artist, title) = stem.split_once(" - ").unwrap_or(("Unknown Artist", &stem));

            let lower = artist.to_lowercase();
            let end = [" feat.", " ft.", " x ", " vs ", " & ", ", "]
                .iter()
                .filter_map(|separator| lower.find(separator))
                .min()
                .unwrap_or(artist.len());

            let main_artist = artist[..end].trim().to_string();
            let key = format!("{}:{}", main_artist.to_lowercase(), title.to_lowercase());

            let duplicate = !seen.insert(key);

            self.tracks.push(Track {
                artist: main_artist,
                title: title.to_string(),
                duplicate,
            });
        }

        self.status = format!("Found {} tracks", self.tracks.len());
    }

    fn download(&mut self) {
        if self.busy.swap(true, Ordering::SeqCst) {
            return;
        }

        let urls: Vec<String> = self
            .urls
            .lines()
            .map(str::trim)
            .filter(|s| s.starts_with("https://"))
            .map(String::from)
            .collect();

        if urls.is_empty() {
            self.busy.store(false, Ordering::SeqCst);
            self.status = "Enter a download URL".into();
            return;
        }

        let output = PathBuf::from(&self.root).join("_Incoming");
        let log = self.log.clone();
        let busy = self.busy.clone();

        thread::spawn(move || {
            if let Err(e) = fs::create_dir_all(&output) {
                *log.lock().unwrap() = e.to_string();
                busy.store(false, Ordering::SeqCst);
                return;
            }

            let result = Command::new("yt-dlp")
                .arg("--yes-playlist")
                .arg("--ignore-errors")
                .arg("-f")
                .arg("bestaudio/best")
                .arg("--download-archive")
                .arg(output.join("downloaded.txt"))
                .arg("-o")
                .arg(output.join("%(uploader)s - %(title)s [%(id)s].%(ext)s"))
                .args(urls)
                .output();

            let text = match result {
                Ok(result) => format!(
                    "{}\n{}\nExit: {}",
                    String::from_utf8_lossy(&result.stdout),
                    String::from_utf8_lossy(&result.stderr),
                    result.status
                ),
                Err(e) => e.to_string(),
            };

            *log.lock().unwrap() = text;
            busy.store(false, Ordering::SeqCst);
        });

        self.status = "Downloading...".into();
    }
}

mod components;

impl eframe::App for GatherForge {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        use components as c;

        let mut style = (*ctx.style()).clone();
        style.visuals = egui::Visuals::dark();
        style.visuals.panel_fill = c::BG;
        style.visuals.window_fill = c::BG;
        style.spacing.item_spacing = egui::vec2(10.0, 10.0);
        ctx.set_style(style);

        egui::SidePanel::left("nav")
            .exact_width(232.0)
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(c::BG)
                    .inner_margin(egui::Margin::symmetric(16, 24)),
            )
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new("GATHER  /  FORGE")
                        .size(19.0)
                        .strong()
                        .color(c::ACCENT),
                );

                ui.label(
                    egui::RichText::new("YOUR CONTENT WORKSPACE")
                        .size(10.0)
                        .color(c::MUTED),
                );

                ui.add_space(34.0);

                for (id, name) in [
                    "Overview",
                    "Downloads",
                    "Music library",
                    "Duplicates",
                    "Artists",
                ]
                .iter()
                .enumerate()
                {
                    if c::nav(ui, self.tab == id, &format!("{:02}", id + 1), name) {
                        self.tab = id;
                    }
                }

                ui.add_space(24.0);
                ui.separator();

                ui.label(
                    egui::RichText::new("NATIVE RUST  •  v3.1")
                        .color(c::MUTED)
                        .size(11.0),
                );
            });

        egui::TopBottomPanel::bottom("status")
            .frame(
                egui::Frame::new()
                    .fill(c::PANEL)
                    .inner_margin(egui::Margin::symmetric(20, 10)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("●").color(c::ACCENT));
                    ui.label(&self.status);

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new("LOCAL PROCESSING")
                                .color(c::MUTED)
                                .size(11.0),
                        );
                    });
                });
            });

        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(c::BG)
                    .inner_margin(egui::Margin::same(28)),
            )
            .show(ctx, |ui| match self.tab {
                0 => {
                    c::heading(
                        ui,
                        "Your workspace",
                        "A single place to collect, inspect and organize your media.",
                    );

                    egui::Frame::new()
                        .fill(egui::Color32::from_rgb(70, 55, 115))
                        .corner_radius(egui::CornerRadius::same(20))
                        .inner_margin(egui::Margin::same(24))
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new("CREATE SPACE FOR WHAT MATTERS")
                                    .color(c::GOLD)
                                    .strong()
                                    .size(12.0),
                            );

                            ui.add_space(8.0);

                            ui.label(
                                egui::RichText::new("Make the mess meaningful.")
                                    .color(egui::Color32::WHITE)
                                    .size(26.0)
                                    .strong(),
                            );

                            ui.add_space(8.0);
                            ui.label("From playlists to a collection you can actually navigate.");
                            ui.add_space(12.0);

                            if ui
                                .add_sized([160.0, 38.0], c::action("Scan your library"))
                                .clicked()
                            {
                                self.scan();
                            }
                        });

                    ui.add_space(20.0);

                    ui.horizontal_wrapped(|ui| {
                        let artists = self
                            .tracks
                            .iter()
                            .map(|x| x.artist.to_lowercase())
                            .collect::<std::collections::HashSet<_>>()
                            .len();

                        let duplicates = self.tracks.iter().filter(|x| x.duplicate).count();

                        c::metric(
                            ui,
                            "TOTAL TRACKS",
                            self.tracks.len().to_string(),
                            "Audio collection",
                        );
                        c::metric(ui, "MAIN ARTISTS", artists.to_string(), "Artist groups");
                        c::metric(
                            ui,
                            "REVIEW FLAGS",
                            duplicates.to_string(),
                            "Possible matches",
                        );
                    });

                    ui.add_space(20.0);

                    c::panel(ui, |ui| {
                        ui.label(
                            egui::RichText::new("LIBRARY LOCATION")
                                .strong()
                                .color(c::ACCENT),
                        );

                        ui.add(
                            egui::TextEdit::singleline(&mut self.root).desired_width(f32::INFINITY),
                        );

                        ui.label(
                            egui::RichText::new(
                                "Music stays on your device. Scan never moves any files.",
                            )
                            .color(c::MUTED),
                        );
                    });
                }

                1 => {
                    c::heading(
                        ui,
                        "Download studio",
                        "Queue a track, playlist, or multiple URLs.",
                    );

                    c::panel(ui, |ui| {
                        ui.label(egui::RichText::new("SOURCES").strong().color(c::ACCENT));

                        ui.add(
                            egui::TextEdit::multiline(&mut self.urls)
                                .hint_text("Paste one URL per line")
                                .desired_rows(7)
                                .desired_width(f32::INFINITY),
                        );

                        ui.add_space(8.0);

                        if ui
                            .add_enabled(
                                !self.busy.load(std::sync::atomic::Ordering::SeqCst),
                                c::action("Download best available audio"),
                            )
                            .clicked()
                        {
                            self.download();
                        }

                        ui.label(
                            egui::RichText::new("Uses yt-dlp; output is stored in _Incoming.")
                                .color(c::MUTED),
                        );
                    });

                    ui.add_space(18.0);

                    c::panel(ui, |ui| {
                        ui.label(egui::RichText::new("ACTIVITY LOG").strong().color(c::GOLD));

                        let text = self.log.lock().unwrap().clone();

                        egui::ScrollArea::vertical()
                            .max_height(290.0)
                            .show(ui, |ui| {
                                ui.label(if text.is_empty() {
                                    "No downloads yet."
                                } else {
                                    &text
                                });
                            });
                    });
                }

                _ => {
                    let title = match self.tab {
                        2 => "Music library",
                        3 => "Duplicate review",
                        _ => "Artist explorer",
                    };

                    c::heading(ui, title, "Search, inspect and review your collection.");

                    ui.horizontal(|ui| {
                        if ui.add(c::action("Scan now")).clicked() {
                            self.scan();
                        }

                        ui.label(format!("{} tracks indexed", self.tracks.len()));
                    });

                    ui.add(
                        egui::TextEdit::singleline(&mut self.search)
                            .hint_text("Search by artist or title...")
                            .desired_width(f32::INFINITY),
                    );

                    ui.add_space(12.0);

                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for track in &self.tracks {
                            if self.tab == 3 && !track.duplicate {
                                continue;
                            }

                            let text = format!("{}  ·  {}", track.artist, track.title);

                            if !text.to_lowercase().contains(&self.search.to_lowercase()) {
                                continue;
                            }

                            c::panel(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new("♫").color(c::ACCENT).size(22.0));

                                    ui.vertical(|ui| {
                                        ui.label(
                                            egui::RichText::new(&track.title).strong().size(15.0),
                                        );

                                        ui.label(
                                            egui::RichText::new(&track.artist).color(c::MUTED),
                                        );
                                    });

                                    if track.duplicate {
                                        ui.label(
                                            egui::RichText::new("POSSIBLE DUPLICATE")
                                                .color(c::GOLD)
                                                .size(11.0),
                                        );
                                    }
                                });
                            });

                            ui.add_space(5.0);
                        }
                    });
                }
            });

        ctx.request_repaint_after(std::time::Duration::from_millis(400));
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("GatherForge")
            .with_inner_size([1200.0, 760.0]),
        ..Default::default()
    };

    eframe::run_native(
        "GatherForge",
        options,
        Box::new(|_| Ok(Box::new(GatherForge::default()))),
    )
}
