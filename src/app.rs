use crate::{components, theme};
use crate::{
    downloader,
    jobs::{self, Kind, State},
    library,
    model::{Decision, Library, Phase, Track},
    util::Result,
};
use eframe::egui::{self, RichText, Vec2};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver},
    thread,
};

#[derive(Default, Clone, Copy, PartialEq, Eq)]
enum Page {
    #[default]
    Overview,
    Manager,
    Downloads,
    Library,
    Duplicates,
    Artists,
    ReelDownload,
    ReelManifest,
    Settings,
}
impl Page {
    fn label(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Manager => "All activity",
            Self::Downloads => "Music downloads",
            Self::Library => "Music library",
            Self::Duplicates => "Duplicate review",
            Self::Artists => "Artist folders",
            Self::ReelDownload => "Reel downloads",
            Self::ReelManifest => "Reel feed",
            Self::Settings => "Settings",
        }
    }
    fn initials(self) -> &'static str {
        match self {
            Self::Overview => "01",
            Self::Manager => "02",
            Self::Downloads => "03",
            Self::Library => "04",
            Self::Duplicates => "05",
            Self::Artists => "06",
            Self::ReelDownload => "07",
            Self::ReelManifest => "08",
            Self::Settings => "09",
        }
    }
}
#[derive(Default, Clone, Copy, PartialEq, Eq)]
enum QueueTab {
    #[default]
    All,
    Active,
    Completed,
    Attention,
}
impl QueueTab {
    fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Active => "Active",
            Self::Completed => "Completed",
            Self::Attention => "Needs attention",
        }
    }
    fn accepts(self, state: State) -> bool {
        match self {
            Self::All => true,
            Self::Active => state.is_pending(),
            Self::Completed => state == State::Completed,
            Self::Attention => state.can_retry(),
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum QueueScope {
    Music,
    Reels,
    All,
}
impl QueueScope {
    fn accepts(self, job: &jobs::Job) -> bool {
        match self {
            Self::Music => !job.kind.is_reel(),
            Self::Reels => job.kind.is_reel(),
            Self::All => true,
        }
    }
    fn id(self) -> u8 {
        match self {
            Self::Music => 1,
            Self::Reels => 2,
            Self::All => 3,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::Music => "Music queue",
            Self::Reels => "Reel queue",
            Self::All => "All downloads",
        }
    }
}
#[derive(Default, Clone, Copy, PartialEq, Eq)]
enum ReelSource {
    #[default]
    Direct,
    Saved,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct LocalSettings {
    library: Option<PathBuf>,
    dark: bool,
    reels_folder: Option<PathBuf>,
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
    jobs: jobs::Queue,
    manager_search: String,
    queue_tab: QueueTab,
    reel_source: ReelSource,
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
    dark: bool,
    reel_urls: String,
    reel_filter: String,
    reel_root: PathBuf,
    reels: Vec<crate::instagram::Reel>,
    saved_unsave_after: bool,
    ai_models: crate::ai_models::Panel,
    reel_flow: crate::reel_flow::Flow,
    reel_previews: crate::reel_previews::Previews,
}

impl GatherForge {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let prefs = LocalSettings::load();
        theme::install(&cc.egui_ctx, prefs.dark);
        let root = default_folder();
        let (state, error) = match library::load(&root) {
            Ok(s) => (s, None),
            Err(e) => (Library::default(), Some(e)),
        };
        let reel_root = prefs.reels_folder.clone().unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("Videos/GatherForge/Reels")
        });
        Self {
            page: Page::Overview,
            root,
            state,
            busy: false,
            core_rx: None,
            jobs: jobs::Queue::load(),
            manager_search: String::new(),
            queue_tab: QueueTab::All,
            reel_source: ReelSource::Direct,
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
            dark: prefs.dark,
            reel_urls: String::new(),
            reel_filter: String::new(),
            reel_root: reel_root.clone(),
            reels: crate::instagram::refresh(&reel_root).unwrap_or_default(),
            saved_unsave_after: false,
            ai_models: crate::ai_models::Panel::new(),
            reel_flow: crate::reel_flow::Flow::new(),
            reel_previews: crate::reel_previews::Previews::new(),
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
        if self.jobs.poll() {
            if let Ok(reels) = crate::instagram::refresh(&self.reel_root) {
                self.reels = reels;
            }
        }
        if self.reel_flow.changed {
            self.reel_flow.changed = false;
            self.reel_flow.selected.clear();
            if let Ok(reels) = crate::instagram::refresh(&self.reel_root) {
                self.reels = reels;
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
        let kind = Kind::Music {
            root: self.root.clone(),
            mp3: self.mp3,
            browser: self.browser.clone(),
            single: self.single,
        };
        match self.jobs.add_urls(&self.urls, kind) {
            Ok(n) => {
                self.notice = format!("Added {n} music item(s) to your queue.");
                self.error = None;
                self.urls.clear();
            }
            Err(e) => self.error = Some(e),
        }
    }
    fn change_root(&mut self, p: PathBuf) {
        if self.busy || self.jobs.has_active() {
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
        let mut prefs = LocalSettings::load();
        prefs.library = Some(self.root.clone());
        prefs.dark = self.dark;
        prefs.reels_folder = Some(self.reel_root.clone());
        prefs.save();
        self.selected = None;
        self.selected_group = None;
    }
    fn music_media_relative(&self, track: &Track) -> Option<PathBuf> {
        let mut candidates = vec![PathBuf::from(&track.original), PathBuf::from("xxx").join(&track.staged)];
        if let Some(published) = &track.published { candidates.push(PathBuf::from(published)); }
        candidates.into_iter().find(|path| self.root.join(path).is_file())
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
        if self.busy || self.jobs.has_active() {
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
            .inner_margin(egui::Margin::symmetric(14, 18))
            .show(ui, |ui| {
                ui.set_min_height((ui.available_height() - 36.0).max(0.0));
                components::chrome::brand(ui);
                ui.add_space(20.0);
                ui.label(
                    RichText::new("WORKSPACE")
                        .size(10.0)
                        .strong()
                        .color(theme::SIGNAL),
                );
                if components::chrome::nav(
                    ui,
                    "",
                    Page::Overview.label(),
                    self.page == Page::Overview,
                    None,
                ) {
                    self.page = Page::Overview;
                }
                ui.add_space(14.0);
                ui.label(
                    RichText::new("MUSIC")
                        .size(10.0)
                        .strong()
                        .color(theme::SIGNAL),
                );
                for p in [
                    Page::Downloads,
                    Page::Library,
                    Page::Duplicates,
                    Page::Artists,
                ] {
                    let badge = if p == Page::Duplicates {
                        Some(self.state.duplicates())
                    } else if p == Page::Downloads {
                        Some(
                            self.jobs
                                .jobs()
                                .iter()
                                .filter(|j| !j.kind.is_reel() && j.state.is_pending())
                                .count(),
                        )
                    } else {
                        None
                    };
                    if components::chrome::nav(ui, "", p.label(), self.page == p, badge) {
                        self.page = p;
                        self.queue_tab = QueueTab::All;
                        self.manager_search.clear();
                    }
                }
                ui.add_space(14.0);
                ui.label(
                    RichText::new("INSTAGRAM")
                        .size(10.0)
                        .strong()
                        .color(theme::SIGNAL),
                );
                for p in [Page::ReelDownload, Page::ReelManifest] {
                    let badge = if p == Page::ReelDownload {
                        Some(
                            self.jobs
                                .jobs()
                                .iter()
                                .filter(|j| j.kind.is_reel() && j.state.is_pending())
                                .count(),
                        )
                    } else {
                        None
                    };
                    if components::chrome::nav(ui, "", p.label(), self.page == p, badge) {
                        self.page = p;
                        self.queue_tab = QueueTab::All;
                        self.manager_search.clear();
                    }
                }
                ui.add_space(14.0);
                ui.label(
                    RichText::new("PREFERENCES")
                        .size(10.0)
                        .strong()
                        .color(theme::SIGNAL),
                );
                if components::chrome::nav(
                    ui,
                    "",
                    Page::Settings.label(),
                    self.page == Page::Settings,
                    None,
                ) {
                    self.page = Page::Settings;
                }
                ui.add_space((ui.available_height() - 82.0).max(6.0));
                if components::chrome::nav(
                    ui,
                    "",
                    Page::Manager.label(),
                    self.page == Page::Manager,
                    None,
                ) {
                    self.page = Page::Manager;
                }
                ui.label(
                    RichText::new("Local-first  /  Your files stay yours")
                        .size(10.0)
                        .color(theme::INK),
                );
            });
    }

    fn header(&mut self, ui: &mut egui::Ui) {
        let reel_page = matches!(self.page, Page::ReelDownload | Page::ReelManifest);
        let working = self.busy || self.jobs.has_active() || self.reel_flow.busy();
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("GATHERFORGE").size(11.0).strong().color(theme::lime()));
            ui.label(RichText::new(format!("/ {}", self.page.label())).size(12.0).color(theme::muted()));
            components::controls::pill(ui, if working { "WORKING" } else { "READY" }, if working { theme::lime() } else { theme::lavender() });
        });
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            let path = if reel_page { &self.reel_root } else { &self.root };
            ui.label(RichText::new(if reel_page { "Reels folder:" } else { "Music folder:" }).size(11.0).color(theme::muted()));
            ui.add(egui::Label::new(RichText::new(path.display().to_string()).size(11.0).color(theme::muted())).truncate());
            if ui.small_button("Change folder").clicked() {
                if reel_page { self.choose_reel_folder(); } else { self.choose_folder(); }
            }
        });
        if let Some(error) = self.error.clone() {
            ui.add_space(8.0);
            egui::Frame::new().fill(theme::error().gamma_multiply(0.10)).corner_radius(egui::CornerRadius::same(theme::RADIUS)).inner_margin(egui::Margin::same(13)).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(format!("Action needed: {error}")).size(12.0).color(theme::error()));
                    if ui.button("Dismiss").clicked() { self.error = None; }
                });
            });
        }
        ui.add_space(16.0);
        ui.separator();
        ui.add_space(16.0);
    }

    fn overview(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Your media workspace").size(27.0).strong().color(theme::white()));
        ui.label(RichText::new("Download, review and organize your music and Reels.").size(13.0).color(theme::muted()));
        ui.add_space(18.0);
        ui.horizontal_wrapped(|ui| {
            if ui.add(components::controls::button("Music library", true)).clicked() { self.page = Page::Library; }
            if ui.add(components::controls::button("Download music", false)).clicked() { self.page = Page::Downloads; }
            if ui.add(components::controls::button("Download Reels", false)).clicked() { self.page = Page::ReelDownload; }
            if ui.add(components::controls::button("Review Reels", false)).clicked() { self.page = Page::ReelManifest; }
        });
        ui.add_space(20.0);
        ui.label(RichText::new("MUSIC AT A GLANCE").size(11.0).strong().color(theme::lime()));
        ui.add_space(8.0);
        components::cards::surface().show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                components::cards::metric(ui, "Tracks", self.state.tracks.len(), "In the library", theme::white(), 0.0);
                ui.add_space(24.0);
                components::cards::metric(ui, "Artists", self.state.folders().len(), "Artist groups", theme::lime(), 0.0);
                ui.add_space(24.0);
                components::cards::metric(ui, "Possible duplicates", self.state.duplicates(), "Available for review", theme::coral(), 0.0);
            });
        });
        ui.add_space(20.0);
        if self.jobs.has_active() {
            ui.label(RichText::new(format!("{} active downloads · {} waiting", self.jobs.active_count(), self.jobs.queued_count())).strong());
            if ui.link("View all activity").clicked() { self.page = Page::Manager; }
            ui.add_space(16.0);
        }
        if self.state.tracks.is_empty() {
            ui.label(RichText::new("Start with your music folder").size(17.0).strong());
            ui.label(RichText::new("Scan the selected folder to review tracks and artists.").color(theme::muted()));
            ui.add_space(8.0);
            if ui.add_enabled(!self.busy, components::controls::button("Scan my library", true)).clicked() { self.run_operation(Operation::Scan); }
        } else {
            ui.label(RichText::new("Next: review your library").size(17.0).strong());
            ui.label(RichText::new(format!("Current state: {}. Review tracks and artist names before staging and publishing.", self.state.phase.label())).color(theme::muted()));
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                if ui.add(components::controls::button("Open music library", true)).clicked() { self.page = Page::Library; }
                if ui.add(components::controls::button("Review duplicates", false)).clicked() { self.page = Page::Duplicates; }
                if ui.add(components::controls::button("Artist folders", false)).clicked() { self.page = Page::Artists; }
            });
        }
    }

    fn downloads(&mut self, ui: &mut egui::Ui) {
        self.download_heading(
            ui,
            "MUSIC / DOWNLOADS",
            "Download music",
            "Add links, choose options, and follow progress below.",
        );
        ui.add_space(18.0);
        components::cards::surface().show(ui,|ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("1  ADD LINKS").size(11.0).strong().color(theme::lime()));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui| {
                    components::controls::pill(ui,"SOUNDCLOUD + AUDIO",theme::lavender());
                });
            });
            ui.add_space(7.0);
            ui.label(RichText::new("Paste one music link per line").size(17.0).strong());
            ui.label(RichText::new("Each link becomes a separate queue item.").size(11.0).color(theme::muted()));
            ui.add_space(10.0);
            ui.add(egui::TextEdit::multiline(&mut self.urls).desired_rows(4).desired_width(ui.available_width().clamp(0.0, 1200.0))
                .hint_text("https://soundcloud.com/artist/track\nhttps://soundcloud.com/artist/sets/playlist"));
            ui.add_space(9.0);
            ui.horizontal_wrapped(|ui| {
                ui.checkbox(&mut self.mp3,"Convert to MP3");
                ui.checkbox(&mut self.single,"Single tracks only");
                egui::ComboBox::from_id_salt("music_browser_cookies")
                    .selected_text(format!("Browser session: {}",self.browser)).show_ui(ui,|ui| {
                    for b in ["none","firefox","chrome","chromium","edge","brave"] {ui.selectable_value(&mut self.browser,b.into(),b);}
                });
            });
            ui.add_space(12.0);
            ui.horizontal_wrapped(|ui| {
                if ui.add_enabled(!self.urls.trim().is_empty(), components::controls::button("Start downloads",true)).clicked(){self.start_download();}
                ui.label(RichText::new("Downloads are saved in _Incoming. Partial files are preserved when cancelled.")
                    .size(11.0).color(theme::muted()));
            });
            if downloader::executable().is_none(){
                ui.label(RichText::new("yt-dlp is not installed or is missing from PATH.").color(theme::error()));
            }
        });
        ui.add_space(22.0);
        self.queue_board(ui, QueueScope::Music);
    }

    fn download_heading(&mut self, ui: &mut egui::Ui, eyebrow: &str, title: &str, subtitle: &str) {
        ui.label(
            RichText::new(eyebrow)
                .size(11.0)
                .strong()
                .color(theme::lime()),
        );
        ui.add_space(5.0);
        ui.label(
            RichText::new(title)
                .size(30.0)
                .strong()
                .color(theme::white()),
        );
        ui.label(RichText::new(subtitle).size(12.5).color(theme::muted()));
    }

    fn job_title(job: &jobs::Job) -> String {
        if matches!(job.kind, Kind::SavedBrowser { .. }) {
            return "Saved collection browser".into();
        }
        let segments: Vec<_> = job
            .source
            .split('?')
            .next()
            .unwrap_or("")
            .split('/')
            .filter(|p| !p.is_empty() && *p != "reel" && *p != "reels" && *p != "p")
            .collect();
        let candidate = segments.last().copied().unwrap_or("Download");
        if candidate.len() > 60 {
            candidate.chars().take(60).collect()
        } else {
            candidate.replace('-', " ").replace('_', " ")
        }
    }

    fn queue_board(&mut self, ui: &mut egui::Ui, scope: QueueScope) {
        let all: Vec<jobs::Job> = self
            .jobs
            .jobs()
            .iter()
            .filter(|j| scope.accepts(j))
            .cloned()
            .collect();
        let running = all
            .iter()
            .filter(|j| matches!(j.state, State::Running | State::Cancelling))
            .count();
        let waiting = all.iter().filter(|j| j.state == State::Queued).count();
        let done = all.iter().filter(|j| j.state == State::Completed).count();
        let attention = all.iter().filter(|j| j.state.can_retry()).count();
        ui.horizontal_wrapped(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    RichText::new("DOWNLOAD ACTIVITY")
                        .size(11.0)
                        .strong()
                        .color(theme::lime()),
                );
                ui.label(RichText::new(scope.name()).size(23.0).strong());
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add(components::controls::button("Clear completed", false))
                    .on_hover_text(
                        "Remove completed entries from this queue only; media files are kept.",
                    )
                    .clicked()
                {
                    self.jobs
                        .clear_completed_for(scope == QueueScope::Reels, scope == QueueScope::All);
                }
            });
        });
        ui.add_space(12.0);
        components::cards::surface().show(ui, |ui| {
            ui.columns(4, |cols| {
                components::cards::metric(
                    &mut cols[0],
                    "Downloading",
                    running,
                    "Live jobs",
                    theme::lime(),
                    0.0,
                );
                components::cards::metric(
                    &mut cols[1],
                    "Waiting",
                    waiting,
                    "Next in line",
                    theme::white(),
                    0.0,
                );
                components::cards::metric(
                    &mut cols[2],
                    "Completed",
                    done,
                    "Kept on disk",
                    theme::lavender(),
                    0.0,
                );
                components::cards::metric(
                    &mut cols[3],
                    "Attention",
                    attention,
                    "Retry or remove",
                    theme::coral(),
                    0.0,
                );
            });
        });
        ui.add_space(12.0);
        ui.horizontal_wrapped(|ui| {
            for tab in [
                QueueTab::All,
                QueueTab::Active,
                QueueTab::Completed,
                QueueTab::Attention,
            ] {
                let active = self.queue_tab == tab;
                if ui.selectable_label(active, tab.label()).clicked() {
                    self.queue_tab = tab;
                }
            }
            ui.add_space(10.0);
            ui.add(
                egui::TextEdit::singleline(&mut self.manager_search)
                    .desired_width(210.0)
                    .hint_text("Find a link or batch..."),
            );
        });
        ui.add_space(14.0);
        let search = self.manager_search.trim().to_ascii_lowercase();
        let visible: Vec<jobs::Job> = all
            .into_iter()
            .filter(|j| {
                self.queue_tab.accepts(j.state)
                    && (search.is_empty()
                        || format!(
                            "{} {} {} {} {}",
                            j.source,
                            j.batch_name,
                            j.kind.label(),
                            j.state.label(),
                            j.current_item
                        )
                        .to_ascii_lowercase()
                        .contains(&search))
            })
            .collect();
        let mut batches: Vec<u64> = visible.iter().map(|j| j.batch_id).collect();
        batches.sort_unstable();
        batches.dedup();
        batches.reverse();
        enum Action {
            Cancel(u64),
            Retry(u64),
            Remove(u64),
            CancelBatch(u64),
            RetryBatch(u64),
            RemoveBatch(u64),
        }
        let mut actions = Vec::<Action>::new();
        if visible.is_empty() {
            components::cards::surface().show(ui, |ui| {
                ui.add_space(15.0);
                ui.label(
                    RichText::new("Nothing to show here yet")
                        .size(17.0)
                        .strong(),
                );
                ui.label(
                    RichText::new(
                        "Add a link above, or change the queue filters to find older jobs.",
                    )
                    .size(12.0)
                    .color(theme::muted()),
                );
                ui.add_space(15.0);
            });
        }
        for batch in batches {
            let items: Vec<jobs::Job> = visible
                .iter()
                .filter(|j| j.batch_id == batch)
                .cloned()
                .collect();
            let Some(first) = items.first() else { continue };
            let full: Vec<_> = self
                .jobs
                .jobs()
                .iter()
                .filter(|j| j.batch_id == batch && scope.accepts(j))
                .collect();
            let completed = full.iter().filter(|j| j.state == State::Completed).count();
            let active = full.iter().any(|j| j.state.is_pending());
            let retry = full.iter().any(|j| j.state.can_retry());
            let batch_count = full.len();
            components::cards::surface().show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new(format!("BATCH {:04}", batch))
                            .size(11.0)
                            .strong()
                            .color(theme::lime()),
                    );
                    ui.label(RichText::new(&first.batch_name).size(15.0).strong());
                    components::controls::pill(
                        ui,
                        &format!("{completed}/{batch_count} finished"),
                        theme::lavender(),
                    );
                    if active && ui.small_button("Cancel batch").clicked() {
                        actions.push(Action::CancelBatch(batch));
                    }
                    if retry && ui.small_button("Retry failed").clicked() {
                        actions.push(Action::RetryBatch(batch));
                    }
                    if ui
                        .small_button("Remove batch")
                        .on_hover_text("Remove history entries; never delete files.")
                        .clicked()
                    {
                        actions.push(Action::RemoveBatch(batch));
                    }
                });
                if batch_count > 1 {
                    ui.add(
                        egui::ProgressBar::new(completed as f32 / batch_count as f32)
                            .desired_width(ui.available_width().clamp(0.0, 1200.0)),
                    );
                }
                ui.add_space(7.0);
                egui::CollapsingHeader::new(format!(
                    "{} item{}",
                    items.len(),
                    if items.len() == 1 { "" } else { "s" }
                ))
                .id_salt(("gf-download-batch", scope.id(), batch))
                .default_open(true)
                .show(ui, |ui| {
                    for item in &items {
                        egui::Frame::new()
                            .fill(theme::surface_raised().gamma_multiply(0.68))
                            .stroke(egui::Stroke::new(1.0_f32, theme::stroke()))
                            .corner_radius(egui::CornerRadius::same(theme::RADIUS))
                            .inner_margin(egui::Margin::same(13))
                            .show(ui, |ui| {
                                ui.horizontal_wrapped(|ui| {
                                    ui.label(
                                        RichText::new(Self::job_title(item)).size(14.0).strong(),
                                    );
                                    let tint = match item.state {
                                        State::Completed => theme::lavender(),
                                        State::Failed => theme::error(),
                                        State::Cancelled | State::Interrupted => theme::coral(),
                                        _ => theme::lime(),
                                    };
                                    components::controls::pill(ui, item.state.label(), tint);
                                    if item.state.is_pending()
                                        && item.state != State::Cancelling
                                        && ui.small_button("Cancel").clicked()
                                    {
                                        actions.push(Action::Cancel(item.id));
                                    }
                                    if item.state.can_retry() && ui.small_button("Retry").clicked()
                                    {
                                        actions.push(Action::Retry(item.id));
                                    }
                                    if ui
                                        .small_button("Remove")
                                        .on_hover_text(
                                            "Remove queue entry only, not the downloaded media.",
                                        )
                                        .clicked()
                                    {
                                        actions.push(Action::Remove(item.id));
                                    }
                                });
                                if let Some(p) = item.progress {
                                    ui.add(
                                        egui::ProgressBar::new(p)
                                            .desired_width(ui.available_width().clamp(0.0, 1200.0))
                                            .show_percentage(),
                                    );
                                } else if item.state == State::Running {
                                    ui.horizontal(|ui| {
                                        ui.spinner();
                                        ui.label(
                                            RichText::new("Preparing download...")
                                                .size(11.0)
                                                .color(theme::muted()),
                                        );
                                    });
                                }
                                if item.state == State::Queued {
                                    ui.label(
                                        RichText::new("Waiting for the previous item to finish")
                                            .size(11.0)
                                            .color(theme::muted()),
                                    );
                                }
                                if !item.speed.is_empty() || !item.eta.is_empty() {
                                    ui.label(
                                        RichText::new(format!(
                                            "{}   {}",
                                            item.speed,
                                            if item.eta.is_empty() {
                                                String::new()
                                            } else {
                                                format!("ETA {}", item.eta)
                                            }
                                        ))
                                        .size(11.0)
                                        .color(theme::muted()),
                                    );
                                }
                                egui::CollapsingHeader::new("Details & source")
                                    .id_salt(("gf-job-detail", item.id))
                                    .default_open(false)
                                    .show(ui, |ui| {
                                        ui.label(
                                            RichText::new(&item.source)
                                                .size(11.0)
                                                .color(theme::muted()),
                                        )
                                        .on_hover_text(&item.source);
                                        if !item.current_item.is_empty() {
                                            ui.label(
                                                RichText::new(&item.current_item)
                                                    .size(11.0)
                                                    .color(theme::muted()),
                                            );
                                        }
                                        if !item.message.is_empty() {
                                            ui.label(
                                                RichText::new(&item.message)
                                                    .size(11.0)
                                                    .color(theme::muted()),
                                            );
                                        }
                                    });
                            });
                        ui.add_space(7.0);
                    }
                });
            });
            ui.add_space(11.0);
        }
        for action in actions {
            match action {
                Action::Cancel(id) => self.jobs.cancel(id),
                Action::Retry(id) => self.jobs.retry(id),
                Action::Remove(id) => self.jobs.remove(id),
                Action::CancelBatch(id) => self.jobs.cancel_batch(id),
                Action::RetryBatch(id) => self.jobs.retry_batch(id),
                Action::RemoveBatch(id) => self.jobs.remove_batch(id),
            }
        }
        ui.add_space(8.0);
        ui.label(RichText::new("Remove and Clear completed only change queue history. Downloaded and partial files remain on disk.")
            .size(11.0).color(theme::muted()));
        egui::CollapsingHeader::new("Technical activity log")
            .id_salt(("gf-log", scope.id()))
            .default_open(false)
            .show(ui, |ui| {
                if ui.small_button("Clear log display").clicked() {
                    self.jobs.log.clear();
                }
                egui::ScrollArea::vertical().drag_to_scroll(false)
                    .max_height(180.0)
                    .show(ui, |ui| {
                        for line in self.jobs.log.iter().rev().take(40) {
                            ui.label(
                                RichText::new(line)
                                    .monospace()
                                    .size(10.0)
                                    .color(theme::muted()),
                            );
                        }
                    });
            });
    }

    fn download_manager(&mut self, ui: &mut egui::Ui) {
        self.download_heading(
            ui,
            "WORKSPACE / ACTIVITY",
            "All your transfers.",
            "A cross-platform history of audio, Reels and collection sessions.",
        );
        ui.add_space(18.0);
        self.queue_board(ui, QueueScope::All);
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
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new(if duplicates_only {
                    "Potential matches"
                } else {
                    "All tracks"
                })
                .size(11.0)
                .strong()
                .color(theme::lime()),
            );
            ui.add(
                egui::TextEdit::singleline(&mut self.filter)
                    .desired_width(184.0)
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
                .color(theme::muted()),
        );
        egui::ScrollArea::vertical().drag_to_scroll(false)
            .max_height(430.0)
            .show(ui, |ui| {
                let count = ((ui.available_width() / 190.0).floor() as usize).clamp(1, 4);
                for chunk in rows.chunks(count) {
                    ui.columns(count, |columns| {
                        for (col, track) in columns.iter_mut().zip(chunk.iter()) {
                            let context = col.ctx().clone();
                            let card_rect = egui::Rect::from_min_size(col.cursor().min, Vec2::new(col.available_width(), 202.0));
                            let artwork = if col.is_rect_visible(card_rect) {
                                let path = self.music_media_relative(track);
                                path.as_ref().and_then(|p| p.to_str()).and_then(|p| self.reel_previews.texture(&context, &self.root, p))
                            } else { None };
                            if components::tracks::track_row(col, &track.artist, &track.title, &track.format, track.action.label(), self.selected == Some(track.id), track.duplicate_of.is_some(), artwork.as_ref()) {
                                self.select_track(track);
                            }
                        }
                    });
                    ui.add_space(10.0);
                }
            });
    }

    fn inspector(&mut self, ui: &mut egui::Ui) {
        components::cards::surface().show(ui, |ui| {
            ui.label(
                RichText::new("Track details")
                    .size(11.0)
                    .strong()
                    .color(theme::lavender()),
            );
            ui.add_space(10.0);
            if let Some(track) = self
                .selected
                .and_then(|id| self.state.tracks.iter().find(|t| t.id == id))
                .cloned()
            {
                ui.label(RichText::new(&track.title).size(18.0).strong());
                if ui.add(components::controls::button("Play track", false)).on_hover_text("Open this file in the system music player").clicked() {
                    let path = self.music_media_relative(&track);
                    let video = path.as_ref().and_then(|p| std::fs::canonicalize(self.root.join(p)).ok());
                    match (std::fs::canonicalize(&self.root), video) {
                        (Ok(root), Some(media)) if media.starts_with(&root) && media.is_file() => {
                            if let Err(e) = std::process::Command::new("xdg-open").arg(media).spawn() { self.error = Some(format!("Could not open track: {e}")); }
                        }
                        _ => self.error = Some("Cannot open track: file is missing or outside the music folder.".into()),
                    }
                }
                ui.label(
                    RichText::new(&track.filename)
                        .size(11.0)
                        .color(theme::muted()),
                );
                ui.add_space(15.0);
                ui.label(
                    RichText::new("Primary artist folder")
                        .size(10.0)
                        .color(theme::muted()),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut self.edit_artist).desired_width(ui.available_width().clamp(0.0, 1200.0)),
                );
                ui.add_space(10.0);
                ui.label(RichText::new("Decision").size(10.0).color(theme::muted()));
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
                        theme::coral(),
                    );
                    ui.label(
                        RichText::new(track.duplicate_kind.unwrap_or_default())
                            .size(11.0)
                            .color(theme::muted()),
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
                ui.label(
                    RichText::new("Source path")
                        .size(10.0)
                        .color(theme::muted()),
                );
                ui.label(
                    RichText::new(&track.original)
                        .size(11.0)
                        .color(theme::muted()),
                );
                ui.label(
                    RichText::new(format!(
                        "{:.1} MB  ·  {}",
                        track.size as f64 / 1_048_576.0,
                        track.format
                    ))
                    .size(11.0)
                    .color(theme::muted()),
                );
            } else {
                ui.label(
                    RichText::new("Select a track to review its artist and publishing decision.")
                        .size(13.0)
                        .color(theme::muted()),
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
                    Vec2::new((available - 16.0) * 0.64, 495.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        self.track_list(ui, duplicates_only);
                    },
                );
                ui.allocate_ui_with_layout(
                    Vec2::new((available - 16.0) * 0.36, 495.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        self.inspector(ui);
                    },
                );
            });
        }
        ui.add_space(8.0);
        ui.label(RichText::new("Publish copies approved tracks and verifies them. Finalize is a separate, confirmed operation.")
            .size(12.0).color(theme::muted()));
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
                ui.label(RichText::new(format!("{} artist groups", groups.len())).size(11.0).color(theme::lime()));
                ui.add_space(12.0);
                egui::ScrollArea::vertical().drag_to_scroll(false).max_height(430.0).show(ui, |ui| {
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
                ui.label(RichText::new("Edit folder name").size(11.0).strong().color(theme::lavender()));
                ui.add_space(12.0);
                if let Some(original) = self.selected_group.clone() {
                    ui.label(RichText::new(format!("Currently: {original}")).size(12.0).color(theme::muted()));
                    ui.add(egui::TextEdit::singleline(&mut self.rename_group).desired_width(ui.available_width().clamp(0.0, 1200.0)));
                    ui.add_space(12.0);
                    if ui.add_enabled(!self.busy, components::controls::button("Rename / merge folder", true)).clicked() {
                        match library::rename_group(&self.root, &mut self.state, &original, &self.rename_group) {
                            Ok(()) => { self.notice = "Artist folders updated".into(); self.selected_group = Some(self.rename_group.clone()); self.error = None; }
                            Err(e) => self.error = Some(e),
                        }
                    }
                    ui.add_space(18.0);
                    ui.label(RichText::new("Merging a name changes the planned folder, not the audio files. Files move only after staging and publishing.")
                        .size(12.0).color(theme::muted()));
                } else {
                    ui.label(RichText::new("Choose an artist group on the left to edit it.").color(theme::muted()));
                }
            });
        });
    }

    fn choose_reel_folder(&mut self) {
        if self.jobs.has_active() || self.reel_flow.busy() {
            self.error = Some("Wait for downloads or analysis to finish before changing the Reels folder.".into());
            return;
        }
        if let Some(dir) = rfd::FileDialog::new().pick_folder() {
            self.reel_root = dir;
            let mut prefs = LocalSettings::load();
            prefs.reels_folder = Some(self.reel_root.clone());
            prefs.dark = self.dark;
            prefs.save();
            match crate::instagram::refresh(&self.reel_root) {
                Ok(reels) => self.reels = reels,
                Err(e) => self.error = Some(e),
            }
        }
    }
    fn open_saved_browser(&mut self) {
        match self.jobs.add_saved_browser(self.reel_root.clone(), false) {
            Ok(()) => {
                self.notice =
                    "Saved collections browser added to Reel downloads. Track or cancel it below."
                        .into();
                self.error = None;
            }
            Err(e) => self.error = Some(e),
        }
    }
    fn reel_downloads(&mut self, ui: &mut egui::Ui) {
        self.download_heading(
            ui,
            "INSTAGRAM / DOWNLOADS",
            "Download Reels",
            "Add individual Reel links or open a Saved collection, then follow progress below.",
        );
        ui.add_space(17.0);
        components::cards::surface().show(ui,|ui|{
            ui.horizontal_wrapped(|ui|{
                if ui.selectable_label(self.reel_source==ReelSource::Direct,"Individual links").clicked(){self.reel_source=ReelSource::Direct;}
                if ui.selectable_label(self.reel_source==ReelSource::Saved,"Saved collections").clicked(){self.reel_source=ReelSource::Saved;}
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{
                    if ui.link("Review downloaded Reels").clicked(){self.page=Page::ReelManifest;}
                });
            });
            ui.separator();ui.add_space(12.0);
            match self.reel_source {
                ReelSource::Direct=>{
                    ui.label(RichText::new("Paste individual Reel links").size(17.0).strong());
                    ui.label(RichText::new("Paste one instagram.com/reel/ link per line. Profile pages are not individual Reels.")
                        .size(11.0).color(theme::muted()));
                    ui.add_space(10.0);
                    ui.add(egui::TextEdit::multiline(&mut self.reel_urls).desired_rows(4).desired_width(ui.available_width().clamp(0.0, 1200.0))
                        .hint_text("https://www.instagram.com/reel/XXXXXXXX/"));
                    ui.add_space(12.0);
                    ui.horizontal_wrapped(|ui|{
                        egui::ComboBox::from_id_salt("instagram_browser")
                            .selected_text(format!("Browser session: {}",self.browser)).show_ui(ui,|ui|{
                            for b in ["none","firefox","chrome","chromium","brave","edge"]{ui.selectable_value(&mut self.browser,b.to_owned(),b);}
                        });
                        if ui.add_enabled(!self.reel_urls.trim().is_empty(), components::controls::button("Start Reel downloads",true)).clicked(){
                            let kind=Kind::Reel{root:self.reel_root.clone(),browser:self.browser.clone()};
                            match self.jobs.add_urls(&self.reel_urls,kind){
                                Ok(n)=>{self.notice=format!("Added {n} Reel(s) to your queue.");self.error=None;self.reel_urls.clear();},
                                Err(e)=>self.error=Some(e),
                            }
                        }
                    });
                }
                ReelSource::Saved=>{
                    ui.label(RichText::new("Collect from Instagram Saved").size(17.0).strong());
                    ui.label(RichText::new("Sign in using the separate browser. Open Saved, choose one collection, and start its download inside that browser.")
                        .size(12.0).color(theme::muted()));
                    ui.add_space(12.0);
                    self.saved_unsave_after=false;
                    ui.add_enabled(false,egui::Checkbox::new(&mut self.saved_unsave_after,"Unsave after classification review (not yet implemented)"));
                    ui.label(RichText::new("Safety gate: only verified, analyzed, explicitly approved and organized Reels can be unsaved from the browser.")
                        .size(11.0).color(theme::muted()));
                    ui.add_space(12.0);
                    if ui.add(components::controls::button("Open Saved collections",true)).clicked(){self.open_saved_browser();}
                    ui.add_space(7.0);
                    ui.label(RichText::new("This browser session appears as a single cancellable job. Individual Reels inside it cannot yet be stopped separately.")
                        .size(11.0).color(theme::muted()));
                }
            }
            ui.add_space(10.0);
            ui.label(RichText::new(format!("Saving to {}",self.reel_root.display())).size(11.0).color(theme::muted()));
        });
        ui.add_space(22.0);
        self.queue_board(ui, QueueScope::Reels);
    }

    fn reel_manifest(&mut self, ui: &mut egui::Ui) {
        components::controls::section_heading(ui, "INSTAGRAM", "Review your Reels", "Analyze downloaded Reels, check the evidence, then choose where to organize them.");
        ui.add_space(16.0);
        ui.horizontal_wrapped(|ui| {
            if ui.add_enabled(!self.reel_flow.busy() && !self.reels.is_empty(), components::controls::button("Analyze all", true)).clicked() {
                self.reel_flow.analyze(self.reel_root.clone(), self.reels.clone());
            }
            if ui.add_enabled(!self.reel_flow.busy(), components::controls::button("Refresh list", false)).clicked() {
                match crate::instagram::refresh(&self.reel_root) {
                    Ok(reels) => self.reels = reels,
                    Err(e) => self.error = Some(e),
                }
            }
            if self.reel_flow.busy() && ui.button("Cancel analysis").clicked() { self.reel_flow.cancel(); }
            if !self.reel_flow.busy() && ui.button("Restore removed records").on_hover_text("Restore all previously removed Reel metadata when there are no filename conflicts.").clicked() {
                match crate::instagram::restore_hidden_metadata(&self.reel_root) {
                    Ok(n) => { self.notice = format!("Restored {n} metadata file(s)."); match crate::instagram::refresh(&self.reel_root) { Ok(r) => self.reels = r, Err(e) => self.error = Some(e) } },
                    Err(e) => self.error = Some(e),
                }
            }
        });
        if !self.reel_flow.notice.is_empty() {
            ui.add_space(6.0);
            ui.label(RichText::new(&self.reel_flow.notice).size(12.0).color(theme::muted()));
        }
        ui.add_space(10.0);
        ui.collapsing("Analysis tools and Instagram actions", |ui| {
            ui.label("Speech and vision engines are optional. Missing tools are reported in each Reel's analysis details.");
            ui.horizontal_wrapped(|ui| {
                if ui.add_enabled(!self.reel_flow.busy(), egui::Button::new("Set up speech engine")).clicked() { self.reel_flow.setup_engine(false); }
                if ui.add_enabled(!self.reel_flow.busy(), egui::Button::new("Set up vision engine")).clicked() { self.reel_flow.setup_engine(true); }
                if ui.button("Open Saved browser").clicked() { self.open_saved_browser(); }
            });
            ui.label(RichText::new("Unsaving is a separate action in the browser and requires approval and verification.").size(11.0).color(theme::muted()));
        });
        ui.add_space(14.0);
        ui.label(RichText::new("YOUR REELS").size(11.0).strong().color(theme::lime()));
        ui.add_space(5.0);
        let search_width = ui.available_width().clamp(0.0, 700.0);
        ui.add(egui::TextEdit::singleline(&mut self.reel_filter).desired_width(search_width).hint_text("Search by creator, caption, category or ID"));
        let q = self.reel_filter.to_lowercase();
        let reels = self.reels.clone();
        let mut analyze_request: Option<crate::instagram::Reel> = None;
        let mut approve_request: Option<(crate::instagram::Reel, String)> = None;
        let mut play_request: Option<crate::instagram::Reel> = None;
        let mut remove_request: Option<crate::instagram::Reel> = None;
        let mut shown = 0usize;
        for reel in reels {
            let analysis = crate::reel_flow::Flow::read(&self.reel_root, &reel.video);
            let category = analysis.as_ref().and_then(|v| v.get("approved_category")).and_then(serde_json::Value::as_str).unwrap_or("");
            if !format!("{} {} {} {} {}", reel.creator, reel.caption, reel.id, reel.date, category).to_lowercase().contains(&q) { continue; }
            shown += 1;
            ui.push_id((&reel.id, &reel.video), |ui| {
                ui.set_max_width(720.0);
                components::cards::surface().show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        let creator = if reel.creator.is_empty() { "Unknown creator".to_owned() } else { format!("@{}", reel.creator) };
                        ui.label(RichText::new(creator).size(15.0).strong());
                        if !reel.date.is_empty() { ui.label(RichText::new(&reel.date).size(11.0).color(theme::muted())); }
                        ui.label(RichText::new(format!("ID: {}", reel.id)).size(11.0).color(theme::muted()));
                    });
                    ui.add_space(8.0);
                    ui.allocate_ui(Vec2::new(200.0, 300.0), |ui| {
                        if ui.is_rect_visible(ui.max_rect()) {
                            if let Some(texture) = self.reel_previews.texture(ui.ctx(), &self.reel_root, &reel.video) {
                                let image = egui::Image::new(&texture).max_width(200.0).max_height(300.0).sense(egui::Sense::click());
                                if ui.add(image).on_hover_text("Open video in your default player").clicked() { play_request = Some(reel.clone()); }
                            } else {
                                ui.label(RichText::new("Preview loading or unavailable").color(theme::muted()));
                            }
                        }
                    });
                    if ui.add(components::controls::button("Play downloaded video", false)).on_hover_text("Opens the downloaded file in your default video player.").clicked() {
                        play_request = Some(reel.clone());
                    }
                    ui.add_space(5.0);
                    let caption: String = reel.caption.chars().take(200).collect();
                    if caption.trim().is_empty() { ui.label(RichText::new("No caption provided").color(theme::muted())); }
                    else { ui.label(caption); }
                    if !reel.url.is_empty() { ui.hyperlink_to("View original on Instagram", &reel.url); }
                    ui.add_space(8.0);
                    if let Some(a) = analysis.as_ref() {
                        let state = a.get("status").and_then(serde_json::Value::as_str).unwrap_or("");
                        let suggestion = a.get("category_suggested").and_then(serde_json::Value::as_str).filter(|s| !s.trim().is_empty());
                        let label = match state { "approved" => "Organized", "needs_review" => "Ready for review", _ => "Analysis status unknown" };
                        ui.label(RichText::new(label).strong().color(theme::lavender()));
                        if let Some(s) = suggestion { ui.label(format!("Suggested category: {s}")); }
                        else if state != "approved" { ui.label(RichText::new("No category suggestion. Choose a category after reviewing the evidence.").color(theme::muted())); }
                        if state == "approved" && !category.is_empty() { ui.label(format!("Category: {category}")); }
                        if let Some(warnings) = a.get("review_warnings").and_then(serde_json::Value::as_array) {
                            if !warnings.is_empty() { ui.label(RichText::new(format!("{} analysis issue(s) — check details below", warnings.len())).color(theme::coral())); }
                        }
                        ui.collapsing("Analysis details", |ui| {
                            ui.label(RichText::new("Automatic text can contain mistakes. OCR samples about every 2 seconds (up to 64 frames).").size(11.0).color(theme::muted()));
                            for (name, key) in [("Speech", "asr"), ("Screen text", "ocr"), ("Visual analysis", "vision")] {
                                let item = a.get(key);
                                let status = item.and_then(|v| v.get("state")).and_then(serde_json::Value::as_str).unwrap_or("not recorded");
                                ui.label(RichText::new(format!("{name}: {status}")).strong());
                                if key == "asr" {
                                    if let Some(model) = item.and_then(|v| v.get("model")).and_then(serde_json::Value::as_str) {
                                        ui.label(RichText::new(format!("Speech model: {model}")).size(11.0).color(theme::muted()));
                                    }
                                    if let Some(seconds) = item.and_then(|v| v.get("seconds_processed")).and_then(serde_json::Value::as_f64) {
                                        let duration = item.and_then(|v| v.get("audio_duration_seconds")).and_then(serde_json::Value::as_f64);
                                        let label = match duration { Some(total) => format!("Audio processed: {seconds:.1} / {total:.1} seconds"), None => format!("Audio processed: {seconds:.1} seconds") };
                                        ui.label(RichText::new(label).size(11.0).color(theme::muted()));
                                    }
                                }
                                if key == "ocr" {
                                    if let Some(count) = item.and_then(|v| v.get("frames_processed")).and_then(serde_json::Value::as_u64) {
                                        let requested = item.and_then(|v| v.get("frames_requested")).and_then(serde_json::Value::as_u64);
                                        let label = match requested { Some(total) => format!("Frames checked: {count} / {total}"), None => format!("Frames checked: {count}") };
                                        ui.label(RichText::new(label).size(11.0).color(theme::muted()));
                                    }
                                    if let Some(languages) = item.and_then(|v| v.get("languages")).and_then(serde_json::Value::as_array) {
                                        let names = languages.iter().filter_map(serde_json::Value::as_str).collect::<Vec<_>>().join(", ");
                                        if !names.is_empty() { ui.label(RichText::new(format!("OCR languages: {names}")).size(11.0).color(theme::muted())); }
                                    }
                                }
                                if let Some(reason) = item.and_then(|v| v.get("reason")).and_then(serde_json::Value::as_str).filter(|s| !s.is_empty()) { ui.label(reason); }
                                if let Some(text) = item.and_then(|v| v.get("text")).and_then(serde_json::Value::as_str).filter(|s| !s.is_empty()) { ui.label(text); }
                                ui.add_space(4.0);
                            }
                            ui.label(RichText::new(format!("File: {}", reel.video)).size(11.0).color(theme::muted()));
                        });
                        if state == "needs_review" {
                            ui.add_space(6.0);
                            ui.label(RichText::new("Choose a category").strong());
                            let can_approve = !self.reel_flow.busy();
                            let choice = self.reel_flow.select_category(&reel.id, a);
                            ui.horizontal_wrapped(|ui| {
                                let selected = if choice.is_empty() { "Select a category" } else { choice.as_str() };
                                egui::ComboBox::from_id_salt("category").selected_text(selected).show_ui(ui, |ui| {
                                    for c in crate::reel_flow::categories() { ui.selectable_value(choice, (*c).to_owned(), *c); }
                                });
                                if ui.add_enabled(can_approve && !choice.is_empty(), components::controls::button("Approve & organize", true))
                                    .on_hover_text("Verify the video and create an organized hard link; the original stays in place.").clicked() {
                                    approve_request = Some((reel.clone(), choice.clone()));
                                }
                            });
                        }
                    } else { ui.label(RichText::new("Not analyzed yet").strong().color(theme::muted())); }
                    ui.add_space(8.0);
                    ui.horizontal_wrapped(|ui| {
                        if ui.add_enabled(!self.reel_flow.busy(), components::controls::button(if analysis.is_some() { "Analyze again" } else { "Analyze Reel" }, false)).clicked() { analyze_request = Some(reel.clone()); }
                        if ui.add_enabled(!self.reel_flow.busy(), components::controls::button("Remove record", false)).on_hover_text("Move this Reel's metadata aside; the video and analysis remain on disk.").clicked() { remove_request = Some(reel.clone()); }
                    });
                });
            });
            ui.add_space(10.0);
        }
        if shown == 0 {
            components::cards::surface().show(ui, |ui| {
                ui.label(RichText::new(if self.reels.is_empty() { "No downloaded Reels yet" } else { "No Reels match your search" }).size(16.0).strong());
                ui.label(RichText::new(if self.reels.is_empty() { "Download a Reel, then return here and refresh the list." } else { "Try another search term." }).color(theme::muted()));
            });
        }
        let pending_id = egui::Id::new("gf-reel-remove-pending");
        if let Some(reel) = remove_request { ui.ctx().data_mut(|d| d.insert_temp(pending_id, (self.reel_root.clone(), reel))); }
        let pending = ui.ctx().data(|d| d.get_temp::<(PathBuf, crate::instagram::Reel)>(pending_id));
        if let Some((pending_root, reel)) = pending {
            egui::Window::new("Remove Reel record?").id(egui::Id::new("gf-reel-remove-dialog")).collapsible(false).resizable(false).show(ui.ctx(), |ui| {
                ui.label(format!("Remove the record for Reel {} from this list?", reel.id));
                ui.label("Its manifest metadata will be moved aside and can be restored. The video and analysis stay on disk. This does not unsave it on Instagram.");
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() { ui.ctx().data_mut(|d| d.remove::<(PathBuf, crate::instagram::Reel)>(pending_id)); }
                    if ui.add_enabled(!self.reel_flow.busy(), components::controls::button("Remove record", true)).clicked() {
                        if self.reel_root != pending_root {
                            self.error = Some("The Reels folder changed; select the record again.".into());
                        } else { match crate::instagram::hide_reel_metadata(&self.reel_root, &reel) {
                            Ok(()) => match crate::instagram::refresh(&self.reel_root) {
                                Ok(items) => { self.reels = items; self.notice = "Record removed; video and analysis preserved.".into(); },
                                Err(e) => self.error = Some(e),
                            },
                            Err(e) => self.error = Some(e),
                        } }
                        ui.ctx().data_mut(|d| d.remove::<(PathBuf, crate::instagram::Reel)>(pending_id));
                    }
                });
            });
        }
        if let Some(reel) = play_request {
            let root = std::fs::canonicalize(&self.reel_root);
            let video = std::fs::canonicalize(self.reel_root.join(&reel.video));
            match (root, video) {
                (Ok(root), Ok(video)) if video.starts_with(&root) && video.is_file() => {
                    if let Err(e) = std::process::Command::new("xdg-open").arg(video).spawn() { self.error = Some(format!("Could not open video: {e}")); }
                }
                _ => self.error = Some("Cannot open this video: file is missing or outside the Reel folder.".into()),
            }
        }
        if let Some(reel) = analyze_request { self.reel_flow.analyze(self.reel_root.clone(), vec![reel]); }
        if let Some((reel, category)) = approve_request { self.reel_flow.approve(self.reel_root.clone(), reel, category); }
    }

    fn settings(&mut self, ui: &mut egui::Ui) {
        components::controls::section_heading(
            ui,
            "Your preferences",
            "Settings",
            "Choose the folders, appearance, and local tools used by GatherForge.",
        );
        ui.add_space(18.0);
        components::cards::surface().show(ui, |ui| {
            ui.heading("Appearance");
            let mut dark=self.dark;
            ui.horizontal(|ui|{ui.selectable_value(&mut dark,false,"Light");ui.selectable_value(&mut dark,true,"Dark");});
            if dark!=self.dark{self.dark=dark;theme::install(ui.ctx(),dark);let mut prefs=LocalSettings::load();prefs.dark=dark;prefs.save();}
            ui.separator();
            ui.heading("Reels storage");
            ui.label(self.reel_root.display().to_string());
            if ui.button("Choose Reel storage").clicked(){self.choose_reel_folder();}
            ui.separator();
            ui.heading("Music library");
            ui.label(RichText::new(self.root.display().to_string()).color(theme::muted()));
            ui.add_space(10.0);
            if ui.add(components::controls::button("Select another library", true)).clicked() { self.choose_folder(); }
            ui.add_space(23.0);
            ui.separator();
            ui.add_space(12.0);
            ui.heading("Privacy & engine");
            ui.label(RichText::new("Native Rust UI  ·  Local file engine  ·  Explicit file operations").color(theme::lime()));
            ui.label(RichText::new(format!("Download tool: {}", downloader::executable()
                .map(|p| p.display().to_string()).unwrap_or_else(|| "Not installed".into())))
                .size(12.0).color(theme::muted()));
            ui.add_space(13.0);
            ui.label(RichText::new("Preferences are saved locally. Staging and reports are stored in the selected music library.")
                .size(12.0).color(theme::muted()));
            ui.add_space(14.0);
            ui.separator();
            ui.add_space(10.0);
            ui.heading("AI and local models");
            ui.label(RichText::new("Manage local models and review their availability below.").size(12.0).color(theme::muted()));
            ui.add_space(12.0);
            self.ai_models.ui(ui);
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
                    }).color(theme::muted()));
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

impl Drop for GatherForge {
    fn drop(&mut self) {
        self.reel_flow.cancel();
        self.jobs.shutdown();
    }
}

impl eframe::App for GatherForge {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll();
        self.ai_models.tick(ctx);
        self.reel_flow.poll(ctx);
        if self.busy || self.jobs.has_active() || self.reel_flow.busy() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        egui::SidePanel::left("main_sidebar")
            .exact_width(theme::SIDEBAR_WIDTH)
            .resizable(false)
            .frame(egui::Frame::new().fill(theme::SIDEBAR))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("sidebar_scroll")
                    .drag_to_scroll(false)
                    .show(ui, |ui| self.sidebar(ui));
            });
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme::base()))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().drag_to_scroll(false).id_salt(("body", self.page.initials())).show(ui, |ui| {
                    egui::Frame::new()
                        .inner_margin(egui::Margin::same(theme::GUTTER as i8))
                        .show(ui, |ui| {
                            self.header(ui);
                            match self.page {
                                Page::Overview => self.overview(ui),
                                Page::Manager => self.download_manager(ui),
                                Page::Downloads => self.downloads(ui),
                                Page::Library => self.library(ui, false),
                                Page::Duplicates => self.library(ui, true),
                                Page::Artists => self.artists(ui),
                                Page::ReelDownload => self.reel_downloads(ui),
                                Page::ReelManifest => self.reel_manifest(ui),
                                Page::Settings => self.settings(ui),
                            }
                            ui.add_space(25.0);
                            ui.separator();
                            ui.label(RichText::new(&self.notice).size(11.0).color(theme::muted()));
                        });
                });
            });
        self.confirm_popup(ctx);
    }
}
