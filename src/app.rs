use crate::{components, theme};
use crate::{
    downloader::{self, DownloadEvent, DownloadSettings},
    library,
    model::{Decision, Library, Phase, Track},
    util::{self, Result},
};
use eframe::egui::{self, RichText, Vec2};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    thread,
};

#[derive(Default, Clone, Copy, PartialEq, Eq)]
enum Page {
    #[default]
    Overview,
    Downloads,
    Library,
    Duplicates,
    Artists,
    Settings,
}
impl Page {
    fn label(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Downloads => "Downloads",
            Self::Library => "Music library",
            Self::Duplicates => "Duplicate review",
            Self::Artists => "Artist folders",
            Self::Settings => "Settings",
        }
    }
    fn initials(self) -> &'static str {
        match self {
            Self::Overview => "01",
            Self::Downloads => "02",
            Self::Library => "03",
            Self::Duplicates => "04",
            Self::Artists => "05",
            Self::Settings => "06",
        }
    }
}
#[derive(Default, Serialize, Deserialize)]
struct LocalSettings {
    library: Option<PathBuf>,
}
fn settings_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("gatherforge")
        .join("settings.json")
}
fn default_folder() -> PathBuf {
    if let Some(p) = LocalSettings::load().library {
        if p.is_dir() {
            return p;
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            if parent
                .file_name()
                .map(|x| x == "debug" || x == "release")
                .unwrap_or(false)
            {
                return std::env::current_dir().unwrap_or_else(|_| parent.to_path_buf());
            }
            return parent.to_path_buf();
        }
    }
    dirs::audio_dir()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}
impl LocalSettings {
    fn load() -> Self {
        std::fs::read(settings_path())
            .ok()
            .and_then(|d| serde_json::from_slice(&d).ok())
            .unwrap_or_default()
    }
    fn save(&self) {
        let path = settings_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(bytes) = serde_json::to_vec_pretty(self) {
            let _ = std::fs::write(path, bytes);
        }
    }
}

enum Operation {
    Scan,
    Stage,
    Publish,
    Restore,
    Finalize,
    Export,
}
impl Operation {
    fn label(&self) -> &'static str {
        match self {
            Self::Scan => "Scan",
            Self::Stage => "Stage",
            Self::Publish => "Publish",
            Self::Restore => "Restore",
            Self::Finalize => "Finalize",
            Self::Export => "Export XLSX",
        }
    }
    fn run(self, root: &Path, state: &mut Library) -> Result<String> {
        match self {
            Self::Scan => {
                *state = library::scan(root, state)?;
                Ok(format!("Scanned {} audio files", state.tracks.len()))
            }
            Self::Stage => {
                library::stage(root, state)?;
                Ok("Originals moved into staging; no originals deleted".into())
            }
            Self::Publish => {
                library::publish(root, state)?;
                Ok("Approved tracks copied and verified".into())
            }
            Self::Restore => {
                library::restore(root, state)?;
                Ok("Staged originals restored".into())
            }
            Self::Finalize => {
                library::finalize(root, state)?;
                Ok("Verified copies finalized; skipped files archived".into())
            }
            Self::Export => {
                let path = library::reports(root, state)?;
                Ok(format!("Excel reports: {}", path.display()))
            }
        }
    }
}
struct Finished {
    state: Library,
    result: Result<String>,
}

pub struct GatherForge {
    page: Page,
    root: PathBuf,
    state: Library,
    busy: bool,
    core_rx: Option<Receiver<Finished>>,
    download_rx: Option<Receiver<DownloadEvent>>,
    download_cancel: Arc<AtomicBool>,
    downloading: bool,
    logs: Vec<String>,
    error: Option<String>,
    notice: String,
    confirm: Option<Operation>,
    filter: String,
    only_kept: bool,
    selected: Option<u64>,
    edit_artist: String,
    edit_decision: Decision,
    selected_group: Option<String>,
    rename_group: String,
    urls: String,
    mp3: bool,
    browser: String,
    single: bool,
}

impl GatherForge {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::install(&cc.egui_ctx);
        let root = default_folder();
        let (state, error) = match library::load(&root) {
            Ok(s) => (s, None),
            Err(e) => (Library::default(), Some(e)),
        };
        Self {
            page: Page::Overview,
            root,
            state,
            busy: false,
            core_rx: None,
            download_rx: None,
            download_cancel: Arc::new(AtomicBool::new(false)),
            downloading: false,
            logs: vec![],
            error,
            notice: "Desktop mode — all operations stay on your computer".into(),
            confirm: None,
            filter: String::new(),
            only_kept: false,
            selected: None,
            edit_artist: String::new(),
            edit_decision: Decision::Keep,
            selected_group: None,
            rename_group: String::new(),
            urls: String::new(),
            mp3: false,
            browser: "none".into(),
            single: false,
        }
    }

    fn poll(&mut self) {
        if let Some(rx) = &self.core_rx {
            if let Ok(done) = rx.try_recv() {
                self.busy = false;
                self.state = done.state;
                match done.result {
                    Ok(msg) => {
                        self.notice = msg;
                        self.error = None;
                    }
                    Err(e) => {
                        self.error = Some(e);
                    }
                }
                // Re-read state on partial stage/publish failure so resumption remains safe.
                if let Ok(state) = library::load(&self.root) {
                    self.state = state;
                }
                self.core_rx = None;
            }
        }
        if let Some(rx) = &self.download_rx {
            let mut done = None;
            while let Ok(event) = rx.try_recv() {
                match event {
                    DownloadEvent::Line(line) => {
                        self.logs.push(line);
                        if self.logs.len() > 1000 {
                            self.logs.drain(..200);
                        }
                    }
                    DownloadEvent::Complete(result) => done = Some(result),
                }
            }
            if let Some(result) = done {
                self.downloading = false;
                self.download_rx = None;
                match result {
                    Ok(()) => {
                        self.notice =
                            "Download batch finished. Scan the library to review new files.".into();
                        self.error = None;
                    }
                    Err(e) => self.error = Some(e),
                }
            }
        }
    }

    fn run_operation(&mut self, op: Operation) {
        if self.busy {
            return;
        }
        let (tx, rx) = mpsc::channel();
        let root = self.root.clone();
        let mut state = self.state.clone();
        self.busy = true;
        self.notice = format!("{} in progress…", op.label());
        self.error = None;
        self.core_rx = Some(rx);
        thread::spawn(move || {
            let result = op.run(&root, &mut state);
            let _ = tx.send(Finished { state, result });
        });
    }
    fn start_download(&mut self) {
        if self.downloading {
            return;
        }
        let settings = DownloadSettings {
            urls: self.urls.clone(),
            convert_mp3: self.mp3,
            browser: self.browser.clone(),
            single: self.single,
        };
        let (tx, rx) = mpsc::channel();
        let root = self.root.clone();
        self.download_cancel = Arc::new(AtomicBool::new(false));
        let cancel = Arc::clone(&self.download_cancel);
        self.download_rx = Some(rx);
        self.downloading = true;
        self.logs.clear();
        self.page = Page::Downloads;
        thread::spawn(move || downloader::run(&root, settings, tx, cancel));
    }
    fn change_root(&mut self, p: PathBuf) {
        if self.busy || self.downloading {
            return;
        }
        if !p.is_dir() {
            self.error = Some("Choose an existing directory".into());
            return;
        }
        self.root = p;
        match library::load(&self.root) {
            Ok(lib) => {
                self.state = lib;
                self.error = None;
            }
            Err(e) => {
                self.state = Library::default();
                self.error = Some(e);
            }
        }
        LocalSettings {
            library: Some(self.root.clone()),
        }
        .save();
        self.selected = None;
        self.selected_group = None;
    }
    fn select_track(&mut self, t: &Track) {
        self.selected = Some(t.id);
        self.edit_artist = t.artist.clone();
        self.edit_decision = t.action;
    }
    fn save_track(&mut self) {
        if self.busy
            || matches!(
                self.state.phase,
                Phase::Published | Phase::Finalizing | Phase::Finalized
            )
        {
            return;
        }
        if let Some(id) = self.selected {
            if let Err(e) = library::update_track(
                &self.root,
                &mut self.state,
                id,
                Some(&self.edit_artist),
                Some(self.edit_decision),
            ) {
                self.error = Some(e);
            } else {
                self.notice = "Track assignment saved".into();
            }
        }
    }
    fn choose_folder(&mut self) {
        if self.busy || self.downloading {
            return;
        }
        if let Some(p) = rfd::FileDialog::new()
            .set_directory(&self.root)
            .pick_folder()
        {
            self.change_root(p);
        }
    }

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        egui::Frame::new()
            .fill(theme::SIDEBAR)
            .inner_margin(egui::Margin::symmetric(17, 24))
            .show(ui, |ui| {
                ui.set_min_height((ui.available_height() - 48.0).max(510.0));
                components::chrome::brand(ui);
                ui.add_space(40.0);
                ui.label(
                    RichText::new("Library desk")
                        .size(12.0)
                        .color(theme::SIGNAL),
                );
                ui.add_space(12.0);
                for page in [
                    Page::Overview,
                    Page::Downloads,
                    Page::Library,
                    Page::Duplicates,
                    Page::Artists,
                ] {
                    let badge = if page == Page::Duplicates {
                        Some(self.state.duplicates())
                    } else {
                        None
                    };
                    if components::chrome::nav(ui, "", page.label(), self.page == page, badge) {
                        self.page = page;
                    }
                    ui.add_space(5.0);
                }
                ui.add_space(25.0);
                if components::chrome::nav(
                    ui,
                    "",
                    Page::Settings.label(),
                    self.page == Page::Settings,
                    None,
                ) {
                    self.page = Page::Settings;
                }
                ui.add_space((ui.available_height() - 98.0).max(16.0));
                ui.separator();
                ui.add_space(9.0);
                ui.label(
                    RichText::new("Private by default")
                        .size(12.0)
                        .strong()
                        .color(theme::INK),
                );
                ui.label(
                    RichText::new("Your files and decisions stay on this computer.")
                        .size(11.0)
                        .color(egui::Color32::from_rgb(174, 185, 198)),
                );
            });
    }

    fn header(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(self.page.label())
                        .size(28.0)
                        .strong()
                        .color(theme::WHITE),
                );
                ui.label(
                    RichText::new("A quieter way to work with your music collection.")
                        .size(12.0)
                        .color(theme::MUTED),
                );
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add(components::controls::button("Choose folder", false))
                    .clicked()
                {
                    self.choose_folder();
                }
                components::controls::pill(
                    ui,
                    if self.busy || self.downloading {
                        "Working"
                    } else {
                        "Ready"
                    },
                    if self.busy || self.downloading {
                        theme::CORAL
                    } else {
                        theme::LAVENDER
                    },
                );
            });
        });
        ui.add_space(13.0);
        components::chrome::path_strip(
            ui,
            &self.root.display().to_string(),
            self.state.phase.label(),
        );
        if let Some(error) = self.error.clone() {
            ui.add_space(10.0);
            egui::Frame::new()
                .fill(theme::ERROR.gamma_multiply(0.10))
                .corner_radius(egui::CornerRadius::same(9))
                .inner_margin(egui::Margin::same(13))
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            RichText::new(format!("Action needed: {error}"))
                                .size(12.0)
                                .color(theme::ERROR),
                        );
                        if ui.button("Dismiss").clicked() {
                            self.error = None;
                        }
                    });
                });
        }
        ui.add_space(21.0);
    }

    fn overview(&mut self, ui: &mut egui::Ui) {
        components::cards::hero(ui, self.state.tracks.len(), self.state.folders().len());
        ui.add_space(25.0);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Your collection at a glance")
                    .size(19.0)
                    .strong()
                    .color(theme::WHITE),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.link("See all tracks").clicked() {
                    self.page = Page::Library;
                }
            });
        });
        ui.add_space(13.0);
        let available = ui.available_width();
        let unit = (available / 3.0).max(100.0);
        egui::Frame::new()
            .fill(theme::SURFACE)
            .stroke(egui::Stroke::new(1.0, theme::STROKE))
            .corner_radius(egui::CornerRadius::same(14))
            .inner_margin(egui::Margin::same(21))
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    components::cards::metric(
                        ui,
                        "Tracks",
                        self.state.tracks.len(),
                        "Found in this library",
                        theme::WHITE,
                        unit - 12.0,
                    );
                    components::cards::metric(
                        ui,
                        "Artist folders",
                        self.state.folders().len(),
                        "Based on the first artist",
                        theme::LIME,
                        unit - 12.0,
                    );
                    components::cards::metric(
                        ui,
                        "For review",
                        self.state.duplicates(),
                        "Potential duplicates",
                        theme::CORAL,
                        unit - 12.0,
                    );
                });
            });
        ui.add_space(22.0);
        if self.state.tracks.is_empty() {
            components::cards::surface().show(ui,|ui| {
                ui.set_min_height(142.0);
                ui.label(RichText::new("Start with the music you already have").size(18.0).strong());
                ui.label(RichText::new("Scan your folder to review tracks, then decide what belongs in each artist collection.")
                    .size(13.0).color(theme::MUTED));
                ui.add_space(13.0);
                ui.horizontal(|ui| {
                    if ui.add_enabled(!self.busy,components::controls::button("Scan my library",true)).clicked() {
                        self.run_operation(Operation::Scan);
                    }
                    if ui.add(components::controls::button("Download music",false)).clicked() {
                        self.page=Page::Downloads;
                    }
                });
            });
        } else {
            ui.columns(2, |cols| {
                components::cards::surface().show(&mut cols[0], |ui| {
                    ui.label(
                        RichText::new("Organize your next batch")
                            .size(17.0)
                            .strong(),
                    );
                    ui.label(
                        RichText::new(format!("Current state: {}", self.state.phase.label()))
                            .size(12.0)
                            .color(theme::MUTED),
                    );
                    ui.add_space(16.0);
                    for (n, title, detail) in [
                        ("1", "Review", "Correct artists and choose what to keep"),
                        ("2", "Stage", "Preserve originals before sorting"),
                        ("3", "Publish", "Copy verified files into artist folders"),
                    ] {
                        components::cards::process_step(ui, n, title, detail, theme::LIME);
                        ui.add_space(15.0);
                    }
                    ui.add_space(7.0);
                    if ui
                        .add(components::controls::button("Open review desk", true))
                        .clicked()
                    {
                        self.page = Page::Library;
                    }
                });
                components::cards::surface().show(&mut cols[1], |ui| {
                    ui.label(RichText::new("Recently indexed").size(17.0).strong());
                    ui.add_space(9.0);
                    for t in self.state.tracks.iter().take(4) {
                        ui.label(RichText::new(&t.title).size(12.0).strong());
                        ui.label(RichText::new(&t.artist).size(11.0).color(theme::MUTED));
                        ui.add_space(9.0);
                    }
                    if ui.link("Browse artist folders").clicked() {
                        self.page = Page::Artists;
                    }
                });
            });
        }
    }

    fn downloads(&mut self, ui: &mut egui::Ui) {
        ui.label(
            RichText::new("Bring a track, a playlist, or an entire collection.")
                .size(19.0)
                .strong(),
        );
        ui.label(
            RichText::new(
                "Paste links below. Downloads stay separate in _Incoming until you scan them.",
            )
            .size(12.0)
            .color(theme::MUTED),
        );
        ui.add_space(18.0);
        components::cards::surface().show(ui,|ui| {
            ui.label(RichText::new("Download queue").size(16.0).strong());
            ui.add_space(5.0);
            ui.label(RichText::new("One URL per line · SoundCloud tracks, Likes and playlists when accessible")
                .size(11.0).color(theme::MUTED));
            ui.add_space(9.0);
            ui.add(egui::TextEdit::multiline(&mut self.urls)
                .desired_rows(6).desired_width(f32::INFINITY)
                .hint_text("https://soundcloud.com/artist/track\nhttps://soundcloud.com/artist/sets/playlist"));
            ui.add_space(12.0);
            ui.horizontal_wrapped(|ui| {
                ui.checkbox(&mut self.mp3,"Convert to MP3").on_hover_text("Needs FFmpeg; conversion may reduce quality");
                ui.checkbox(&mut self.single,"Single tracks only");
                egui::ComboBox::from_id_salt("browser_cookies")
                    .selected_text(format!("Sign-in: {}",self.browser)).show_ui(ui,|ui| {
                    for browser in ["none","firefox","chrome","chromium","edge","brave"] {
                        ui.selectable_value(&mut self.browser,browser.into(),browser);
                    }
                });
            });
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui.add_enabled(!self.downloading,components::controls::button("Start downloads",true)).clicked() {self.start_download();}
                if ui.add_enabled(self.downloading,components::controls::button("Stop batch",false)).clicked() {
                    self.download_cancel.store(true,Ordering::Relaxed);
                }
                components::controls::pill(ui,if self.downloading {"Downloading"} else {"Idle"},
                    if self.downloading {theme::CORAL} else {theme::LAVENDER});
            });
            if downloader::executable().is_none() {
                ui.add_space(10.0);
                ui.label(RichText::new("yt-dlp isn't installed. Install it before starting a download.")
                    .color(theme::ERROR).size(12.0));
            }
        });
        ui.add_space(18.0);
        egui::Frame::new()
            .fill(theme::HERO)
            .corner_radius(egui::CornerRadius::same(15))
            .inner_margin(egui::Margin::same(19))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("Activity")
                            .size(16.0)
                            .strong()
                            .color(theme::INK),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("Clear log").clicked() {
                            self.logs.clear();
                        }
                    });
                });
                ui.add_space(10.0);
                egui::ScrollArea::vertical()
                    .max_height(260.0)
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        if self.logs.is_empty() {
                            ui.label(
                                RichText::new("Download progress and errors will appear here.")
                                    .size(12.0)
                                    .color(egui::Color32::from_rgb(188, 199, 211)),
                            );
                        }
                        for line in self.logs.iter().rev().take(160).rev() {
                            let color = if line.to_lowercase().contains("error") {
                                theme::SIGNAL
                            } else {
                                egui::Color32::from_rgb(207, 217, 227)
                            };
                            ui.label(RichText::new(line).monospace().size(11.0).color(color));
                        }
                    });
            });
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(!self.busy, components::controls::button("Scan", true))
                .clicked()
            {
                self.run_operation(Operation::Scan);
            }
            if ui
                .add_enabled(
                    !self.busy,
                    components::controls::button("Stage originals", false),
                )
                .clicked()
            {
                self.confirm = Some(Operation::Stage);
            }
            if ui
                .add_enabled(
                    !self.busy,
                    components::controls::button("Export Excel", false),
                )
                .clicked()
            {
                self.run_operation(Operation::Export);
            }
            if ui
                .add_enabled(
                    !self.busy,
                    components::controls::button("Publish approved", false),
                )
                .clicked()
            {
                self.confirm = Some(Operation::Publish);
            }
            if ui
                .add_enabled(!self.busy, components::controls::button("Restore", false))
                .clicked()
            {
                self.confirm = Some(Operation::Restore);
            }
            if ui
                .add_enabled(!self.busy, components::controls::button("Finalize", false))
                .clicked()
            {
                self.confirm = Some(Operation::Finalize);
            }
        });
    }

    fn track_list(&mut self, ui: &mut egui::Ui, duplicates_only: bool) {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(if duplicates_only {
                    "Potential matches"
                } else {
                    "All tracks"
                })
                .size(11.0)
                .strong()
                .color(theme::LIME),
            );
            ui.add(
                egui::TextEdit::singleline(&mut self.filter)
                    .desired_width(210.0)
                    .hint_text("Search songs / artists..."),
            );
            ui.checkbox(&mut self.only_kept, "Keep only");
        });
        ui.add_space(10.0);
        let query = self.filter.to_lowercase();
        let rows: Vec<Track> = self
            .state
            .tracks
            .iter()
            .filter(|t| {
                (!duplicates_only || t.duplicate_of.is_some())
                    && (!self.only_kept || t.action == Decision::Keep)
                    && (query.is_empty()
                        || format!("{} {} {}", t.artist, t.title, t.filename)
                            .to_lowercase()
                            .contains(&query))
            })
            .cloned()
            .collect();
        ui.label(
            RichText::new(format!("{} results", rows.len()))
                .size(10.0)
                .color(theme::MUTED),
        );
        egui::ScrollArea::vertical()
            .max_height(430.0)
            .show(ui, |ui| {
                for track in &rows {
                    if components::tracks::track_row(
                        ui,
                        &track.artist,
                        &track.title,
                        &track.format,
                        track.action.label(),
                        self.selected == Some(track.id),
                        track.duplicate_of.is_some(),
                    ) {
                        self.select_track(track);
                    }
                    ui.add_space(5.0);
                }
            });
    }

    fn inspector(&mut self, ui: &mut egui::Ui) {
        components::cards::surface().show(ui, |ui| {
            ui.label(
                RichText::new("Track details")
                    .size(11.0)
                    .strong()
                    .color(theme::LAVENDER),
            );
            ui.add_space(10.0);
            if let Some(track) = self
                .selected
                .and_then(|id| self.state.tracks.iter().find(|t| t.id == id))
                .cloned()
            {
                ui.label(RichText::new(&track.title).size(18.0).strong());
                ui.label(
                    RichText::new(&track.filename)
                        .size(11.0)
                        .color(theme::MUTED),
                );
                ui.add_space(15.0);
                ui.label(
                    RichText::new("Primary artist folder")
                        .size(10.0)
                        .color(theme::MUTED),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut self.edit_artist).desired_width(f32::INFINITY),
                );
                ui.add_space(10.0);
                ui.label(RichText::new("Decision").size(10.0).color(theme::MUTED));
                ui.horizontal_wrapped(|ui| {
                    for decision in [Decision::Keep, Decision::Skip, Decision::Review] {
                        ui.selectable_value(&mut self.edit_decision, decision, decision.label());
                    }
                });
                ui.add_space(14.0);
                if let Some(other) = track.duplicate_of {
                    components::controls::pill(
                        ui,
                        &format!("Possible match #{other}"),
                        theme::CORAL,
                    );
                    ui.label(
                        RichText::new(track.duplicate_kind.unwrap_or_default())
                            .size(11.0)
                            .color(theme::MUTED),
                    );
                    ui.add_space(10.0);
                }
                if ui
                    .add_enabled(
                        !self.busy,
                        components::controls::button("Save track changes", true),
                    )
                    .clicked()
                {
                    self.save_track();
                }
                ui.add_space(16.0);
                ui.label(RichText::new("Source path").size(10.0).color(theme::MUTED));
                ui.label(
                    RichText::new(&track.original)
                        .size(11.0)
                        .color(theme::MUTED),
                );
                ui.label(
                    RichText::new(format!(
                        "{:.1} MB  ·  {}",
                        track.size as f64 / 1_048_576.0,
                        track.format
                    ))
                    .size(11.0)
                    .color(theme::MUTED),
                );
            } else {
                ui.label(
                    RichText::new("Select a track to review its artist and publishing decision.")
                        .size(13.0)
                        .color(theme::MUTED),
                );
            }
        });
    }

    fn library(&mut self, ui: &mut egui::Ui, duplicates_only: bool) {
        components::controls::section_heading(
            ui,
            "Your collection",
            if duplicates_only {
                "Review matches"
            } else {
                "Music Library"
            },
            "Originals are protected until you explicitly finalize a verified batch.",
        );
        ui.add_space(16.0);
        self.toolbar(ui);
        ui.add_space(18.0);
        let available = ui.available_width();
        if available < 710.0 {
            self.track_list(ui, duplicates_only);
            ui.add_space(12.0);
            self.inspector(ui);
        } else {
            ui.horizontal(|ui| {
                ui.allocate_ui_with_layout(
                    Vec2::new((available * 0.64).max(370.0), 495.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        self.track_list(ui, duplicates_only);
                    },
                );
                ui.add_space(8.0);
                ui.allocate_ui_with_layout(
                    Vec2::new((available * 0.32).max(220.0), 495.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        self.inspector(ui);
                    },
                );
            });
        }
        ui.add_space(8.0);
        ui.label(RichText::new("Publish copies approved tracks and verifies them. Finalize is a separate, confirmed operation.")
            .size(12.0).color(theme::MUTED));
    }

    fn artists(&mut self, ui: &mut egui::Ui) {
        components::controls::section_heading(
            ui,
            "Collection structure",
            "Artist folders",
            "Rename or merge groups before publishing. Features and collaborators remain under the primary artist.",
        );
        ui.add_space(18.0);
        let groups = self.state.folders();
        ui.columns(2, |cols| {
            components::cards::surface().show(&mut cols[0], |ui| {
                ui.label(RichText::new(format!("{} artist groups", groups.len())).size(11.0).color(theme::LIME));
                ui.add_space(12.0);
                egui::ScrollArea::vertical().max_height(430.0).show(ui, |ui| {
                    for (artist, count) in &groups {
                        if ui.selectable_label(self.selected_group.as_ref() == Some(artist),
                            format!("{artist}    /    {count} tracks")).clicked() {
                            self.selected_group = Some(artist.clone());
                            self.rename_group = artist.clone();
                        }
                    }
                });
            });
            components::cards::surface().show(&mut cols[1], |ui| {
                ui.label(RichText::new("Edit folder name").size(11.0).strong().color(theme::LAVENDER));
                ui.add_space(12.0);
                if let Some(original) = self.selected_group.clone() {
                    ui.label(RichText::new(format!("Currently: {original}")).size(12.0).color(theme::MUTED));
                    ui.add(egui::TextEdit::singleline(&mut self.rename_group).desired_width(f32::INFINITY));
                    ui.add_space(12.0);
                    if ui.add_enabled(!self.busy, components::controls::button("Rename / merge folder", true)).clicked() {
                        match library::rename_group(&self.root, &mut self.state, &original, &self.rename_group) {
                            Ok(()) => { self.notice = "Artist folders updated".into(); self.selected_group = Some(self.rename_group.clone()); self.error = None; }
                            Err(e) => self.error = Some(e),
                        }
                    }
                    ui.add_space(18.0);
                    ui.label(RichText::new("Merging a name changes the planned folder, not the audio files. Files move only after staging and publishing.")
                        .size(12.0).color(theme::MUTED));
                } else {
                    ui.label(RichText::new("Choose an artist group on the left to edit it.").color(theme::MUTED));
                }
            });
        });
    }

    fn settings(&mut self, ui: &mut egui::Ui) {
        components::controls::section_heading(
            ui,
            "Your preferences",
            "Settings",
            "Simple, transparent controls for a local desktop toolkit.",
        );
        ui.add_space(18.0);
        components::cards::surface().show(ui, |ui| {
            ui.heading("Library directory");
            ui.label(RichText::new(self.root.display().to_string()).color(theme::MUTED));
            ui.add_space(10.0);
            if ui.add(components::controls::button("Select another library", true)).clicked() { self.choose_folder(); }
            ui.add_space(23.0);
            ui.separator();
            ui.add_space(12.0);
            ui.heading("Privacy & engine");
            ui.label(RichText::new("Native Rust UI  ·  Local file engine  ·  No browser").color(theme::LIME));
            ui.label(RichText::new(format!("Download tool: {}", downloader::executable()
                .map(|p| p.display().to_string()).unwrap_or_else(|| "Not installed".into())))
                .size(12.0).color(theme::MUTED));
            ui.add_space(13.0);
            ui.label(RichText::new("Preferences are saved locally. Staging and reports live in the chosen library's GatherForge-owned xxx folder.")
                .size(12.0).color(theme::MUTED));
            ui.add_space(10.0);
            ui.label(RichText::new("GatherForge 3.2 — Collection Studio").size(11.0).color(theme::MUTED));
        });
    }

    fn confirm_popup(&mut self, ctx: &egui::Context) {
        if let Some(operation) = self.confirm.take() {
            let mut open = true;
            let mut accepted = false;
            let mut cancelled = false;
            egui::Window::new("Confirm file operation").collapsible(false).resizable(false)
                .open(&mut open).anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.set_width(380.0);
                    ui.label(RichText::new(format!("{} this batch?", operation.label())).size(18.0).strong());
                    ui.add_space(9.0);
                    ui.label(RichText::new(match operation {
                        Operation::Finalize => "Staged originals are deleted only after every kept published copy is verified. Unpublished tracks are archived.",
                        Operation::Stage => "Move found originals into protected staging. Nothing is deleted.",
                        Operation::Publish => "Copy approved music into artist folders; originals remain in staging.",
                        Operation::Restore => "Restore staged originals to their source locations without overwriting existing files.",
                        _ => "Proceed with this operation?",
                    }).color(theme::MUTED));
                    ui.add_space(18.0);
                    ui.horizontal(|ui| {
                        if ui.add(components::controls::button("Confirm", true)).clicked() { accepted = true; }
                        if ui.add(components::controls::button("Cancel", false)).clicked() { cancelled = true; }
                    });
                });
            if accepted {
                self.run_operation(operation);
            } else if open && !cancelled {
                self.confirm = Some(operation);
            }
        }
    }
}

impl eframe::App for GatherForge {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll();
        if self.busy || self.downloading {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        egui::SidePanel::left("main_sidebar")
            .exact_width(theme::SIDEBAR_WIDTH)
            .resizable(false)
            .frame(egui::Frame::new().fill(theme::SIDEBAR))
            .show(ctx, |ui| {
                self.sidebar(ui);
            });
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme::BASE))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().id_salt("body").show(ui, |ui| {
                    egui::Frame::new()
                        .inner_margin(egui::Margin::same(theme::GUTTER as i8))
                        .show(ui, |ui| {
                            self.header(ui);
                            match self.page {
                                Page::Overview => self.overview(ui),
                                Page::Downloads => self.downloads(ui),
                                Page::Library => self.library(ui, false),
                                Page::Duplicates => self.library(ui, true),
                                Page::Artists => self.artists(ui),
                                Page::Settings => self.settings(ui),
                            }
                            ui.add_space(25.0);
                            ui.separator();
                            ui.label(RichText::new(&self.notice).size(11.0).color(theme::MUTED));
                        });
                });
            });
        self.confirm_popup(ctx);
    }
}
