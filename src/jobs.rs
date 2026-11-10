//! Local download queue. A job is one submitted URL (a playlist URL may contain many tracks).
//! Removing a job removes only its history. No media files are ever deleted here.
use crate::{
    downloader::{self, DownloadEvent, DownloadSettings},
    instagram,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeSet, HashMap},
    fs,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const MAX_HISTORY: usize = 500;
const PARALLEL_LIMIT: usize = 1; // Both media workflows use a shared yt-dlp archive; serialize writes.

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum State {
    Queued,
    Running,
    Cancelling,
    Completed,
    Failed,
    Cancelled,
    Interrupted,
}
impl State {
    pub fn label(self) -> &'static str {
        match self {
            Self::Queued => "Queued",
            Self::Running => "Downloading",
            Self::Cancelling => "Stopping",
            Self::Completed => "Completed",
            Self::Failed => "Failed",
            Self::Cancelled => "Cancelled",
            Self::Interrupted => "Interrupted",
        }
    }
    pub fn is_pending(self) -> bool {
        matches!(self, Self::Queued | Self::Running | Self::Cancelling)
    }
    pub fn can_retry(self) -> bool {
        matches!(self, Self::Failed | Self::Cancelled | Self::Interrupted)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Kind {
    Music {
        root: PathBuf,
        mp3: bool,
        browser: String,
        single: bool,
    },
    Reel {
        root: PathBuf,
        browser: String,
    },
    SavedBrowser {
        root: PathBuf,
        auto_unsave: bool,
    },
}
impl Kind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Music { .. } => "Music",
            Self::Reel { .. } => "Instagram Reel",
            Self::SavedBrowser { .. } => "Saved collections browser",
        }
    }
    pub fn is_reel(&self) -> bool {
        matches!(self, Self::Reel { .. } | Self::SavedBrowser { .. })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Job {
    pub id: u64,
    pub batch_id: u64,
    pub batch_name: String,
    pub source: String,
    pub kind: Kind,
    pub state: State,
    pub progress: Option<f32>,
    pub speed: String,
    pub eta: String,
    pub current_item: String,
    pub message: String,
    pub had_error: bool,
    pub created_unix: u64,
    pub finished_unix: Option<u64>,
}
impl Default for Job {
    fn default() -> Self {
        Self {
            id: 0,
            batch_id: 0,
            batch_name: String::new(),
            source: String::new(),
            kind: Kind::Music {
                root: PathBuf::new(),
                mp3: false,
                browser: "none".into(),
                single: false,
            },
            state: State::Queued,
            progress: None,
            speed: String::new(),
            eta: String::new(),
            current_item: String::new(),
            message: String::new(),
            had_error: false,
            created_unix: 0,
            finished_unix: None,
        }
    }
}
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Store {
    version: u32,
    next_id: u64,
    jobs: Vec<Job>,
}

pub struct Queue {
    store: Store,
    tx: Sender<(u64, DownloadEvent)>,
    rx: Receiver<(u64, DownloadEvent)>,
    running: HashMap<u64, Arc<AtomicBool>>,
    remove_when_stopped: BTreeSet<u64>,
    pub log: Vec<String>,
    last_save: Instant,
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn store_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("gatherforge")
        .join("download-queue.json")
}

fn source_urls(raw: &str, instagram_only: bool) -> Result<Vec<String>, String> {
    let mut links = Vec::new();
    for line in raw
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty() && !s.starts_with('#'))
    {
        if links.len() >= 100 {
            return Err("Maximum 100 URLs per submission. Split larger lists into batches.".into());
        }
        let parsed = url::Url::parse(line).map_err(|e| format!("Invalid URL: {e}"))?;
        if parsed.scheme() != "https"
            || !parsed.username().is_empty()
            || parsed.password().is_some()
        {
            return Err("Use HTTPS URLs without embedded credentials.".into());
        }
        let host = parsed.host_str().unwrap_or("").to_ascii_lowercase();
        if host.is_empty()
            || host == "localhost"
            || host.ends_with(".localhost")
            || host.ends_with(".local")
            || host == "127.0.0.1"
            || host == "::1"
        {
            return Err("Local addresses are not download sources.".into());
        }
        if instagram_only {
            let path: Vec<_> = parsed
                .path_segments()
                .ok_or("Invalid Reel URL")?
                .filter(|s| !s.is_empty())
                .collect();
            if !["instagram.com", "www.instagram.com", "m.instagram.com"].contains(&host.as_str())
                || path.len() != 2
                || !["reel", "reels", "p"].contains(&path[0])
            {
                return Err(format!("Use a direct Instagram Reel URL: {line}"));
            }
        }
        if !links.iter().any(|x| x == line) {
            links.push(line.to_owned());
        }
    }
    if links.is_empty() {
        return Err("Enter at least one link.".into());
    }
    Ok(links)
}

fn progress_from_line(line: &str) -> Option<(f32, String, String)> {
    if let Some(detail) = line.split_once("GF_PROGRESS:").map(|(_, v)| v) {
        let mut p = detail.trim().split('|');
        let percent = p
            .next()?
            .trim()
            .trim_end_matches('%')
            .trim()
            .parse::<f32>()
            .ok()?;
        let speed = p.next().unwrap_or("").trim().chars().take(48).collect();
        let eta = p.next().unwrap_or("").trim().chars().take(48).collect();
        return Some(((percent / 100.0).clamp(0.0, 1.0), speed, eta));
    }
    // Fallback for yt-dlp versions that print their traditional [download] lines.
    if !line.contains("[download]") {
        return None;
    }
    let at = line.find('%')?;
    let left = &line[..at];
    let start = left
        .char_indices()
        .rev()
        .find(|(_, c)| !c.is_ascii_digit() && *c != '.' && *c != ' ')
        .map_or(0, |(i, _)| i + 1);
    let percent = left[start..].trim().parse::<f32>().ok()?;
    let speed = line
        .split(" at ")
        .nth(1)
        .and_then(|s| s.split_whitespace().next())
        .unwrap_or("")
        .chars()
        .take(48)
        .collect();
    let eta = line
        .split(" ETA ")
        .nth(1)
        .and_then(|s| s.split_whitespace().next())
        .unwrap_or("")
        .chars()
        .take(48)
        .collect();
    Some(((percent / 100.0).clamp(0.0, 1.0), speed, eta))
}

impl Queue {
    pub fn load() -> Self {
        let (tx, rx) = mpsc::channel();
        let mut store = if !store_path().is_symlink() {
            fs::read(store_path())
                .ok()
                .filter(|data| data.len() < 4_000_000)
                .and_then(|data| serde_json::from_slice(&data).ok())
                .unwrap_or_default()
        } else {
            Store::default()
        };
        for job in &mut store.jobs {
            if matches!(
                job.state,
                State::Running | State::Cancelling | State::Queued
            ) {
                job.state = State::Interrupted;
                job.message = "App closed before completion. Use Retry to resume safely.".into();
                job.speed.clear();
                job.eta.clear();
                job.finished_unix = None;
            }
        }
        Self {
            store,
            tx,
            rx,
            running: HashMap::new(),
            remove_when_stopped: BTreeSet::new(),
            log: Vec::new(),
            last_save: Instant::now(),
        }
    }
    pub fn jobs(&self) -> &[Job] {
        &self.store.jobs
    }
    pub fn has_active(&self) -> bool {
        self.store.jobs.iter().any(|j| j.state.is_pending())
    }
    pub fn active_count(&self) -> usize {
        self.store
            .jobs
            .iter()
            .filter(|j| matches!(j.state, State::Running | State::Cancelling))
            .count()
    }
    pub fn queued_count(&self) -> usize {
        self.store
            .jobs
            .iter()
            .filter(|j| j.state == State::Queued)
            .count()
    }
    pub fn history_count(&self) -> usize {
        self.store.jobs.len()
    }
    fn next_id(&mut self) -> u64 {
        let top = self
            .store
            .jobs
            .iter()
            .map(|j| j.id.max(j.batch_id))
            .max()
            .unwrap_or(0);
        self.store.next_id = self.store.next_id.max(top).saturating_add(1);
        self.store.next_id
    }
    pub fn add_urls(&mut self, raw: &str, kind: Kind) -> Result<usize, String> {
        let links = source_urls(raw, kind.is_reel())?;
        if self.store.jobs.len() + links.len() > MAX_HISTORY {
            return Err("Queue history limit reached. Remove completed entries first.".into());
        }
        let batch = self.next_id();
        let batch_name = match &kind {
            Kind::Music { .. } => "Music downloads",
            Kind::Reel { .. } => "Instagram Reels",
            Kind::SavedBrowser { .. } => "Saved browser",
        };
        for url in &links {
            let id = self.next_id();
            self.store.jobs.push(Job {
                id,
                batch_id: batch,
                batch_name: batch_name.into(),
                source: url.clone(),
                kind: kind.clone(),
                created_unix: now(),
                ..Default::default()
            });
        }
        self.save();
        self.start_next();
        Ok(links.len())
    }
    pub fn add_saved_browser(&mut self, root: PathBuf, auto_unsave: bool) -> Result<(), String> {
        if self.store.jobs.len() >= MAX_HISTORY {
            return Err("Queue history limit reached. Remove completed entries first.".into());
        }
        if self
            .store
            .jobs
            .iter()
            .any(|j| matches!(j.kind, Kind::SavedBrowser { .. }) && j.state.is_pending())
        {
            return Err("A Saved collections browser is already queued or running.".into());
        }
        let batch = self.next_id();
        let id = self.next_id();
        self.store.jobs.push(Job {
            id,
            batch_id: batch,
            batch_name: "Saved collections browser".into(),
            source: "Choose collection inside the browser".into(),
            kind: Kind::SavedBrowser { root, auto_unsave },
            created_unix: now(),
            ..Default::default()
        });
        self.save();
        self.start_next();
        Ok(())
    }
    fn save(&mut self) {
        let path = store_path();
        if path.is_symlink() {
            return;
        }
        let Some(parent) = path.parent() else {
            return;
        };
        if fs::create_dir_all(parent).is_err() {
            return;
        }
        self.store.version = 1;
        let Ok(data) = serde_json::to_vec_pretty(&self.store) else {
            return;
        };
        let temp = path.with_extension(format!("json.{}.tmp", std::process::id()));
        if temp.is_symlink() {
            return;
        }
        if fs::write(&temp, data).is_ok() {
            let _ = fs::rename(&temp, &path);
        }
        self.last_save = Instant::now();
    }
    fn start_next(&mut self) {
        let mut started = false;
        while self.running.len() < PARALLEL_LIMIT {
            let Some(job) = self
                .store
                .jobs
                .iter_mut()
                .find(|j| j.state == State::Queued)
            else {
                break;
            };
            let id = job.id;
            let kind = job.kind.clone();
            let url = job.source.clone();
            job.state = State::Running;
            job.message = "Starting yt-dlp...".into();
            job.had_error = false;
            job.progress = None;
            job.speed.clear();
            job.eta.clear();
            let cancel = Arc::new(AtomicBool::new(false));
            self.running.insert(id, Arc::clone(&cancel));
            started = true;
            let main_tx = self.tx.clone();
            thread::spawn(move || {
                let (tx, rx) = mpsc::channel();
                let forward = thread::spawn(move || {
                    while let Ok(event) = rx.recv() {
                        let complete = matches!(event, DownloadEvent::Complete(_));
                        if main_tx.send((id, event)).is_err() {
                            break;
                        }
                        if complete {
                            break;
                        }
                    }
                });
                let backup_tx = tx.clone();
                let status =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match kind {
                        Kind::Music {
                            root,
                            mp3,
                            browser,
                            single,
                        } => downloader::run(
                            &root,
                            DownloadSettings {
                                urls: url,
                                convert_mp3: mp3,
                                browser,
                                single,
                            },
                            tx,
                            cancel,
                        ),
                        Kind::Reel { root, browser } => {
                            instagram::run(root, url, browser, tx, cancel)
                        }
                        Kind::SavedBrowser { root, auto_unsave } => {
                            instagram::browse_saved(root, auto_unsave, tx, cancel)
                        }
                    }));
                if status.is_err() {
                    let _ = backup_tx.send(DownloadEvent::Complete(Err(
                        "Download worker crashed".into()
                    )));
                }
                drop(backup_tx);
                let _ = forward.join();
            });
        }
        if started {
            self.save();
        }
    }
    pub fn poll(&mut self) -> bool {
        let mut reel_finished = false;
        let mut changed = false;
        let mut finished_change = false;
        while let Ok((id, event)) = self.rx.try_recv() {
            if let Some(job) = self.store.jobs.iter_mut().find(|j| j.id == id) {
                match event {
                    DownloadEvent::Line(line) => {
                        if let Some((progress, speed, eta)) = progress_from_line(&line) {
                            job.progress = Some(progress);
                            job.speed = speed;
                            job.eta = eta;
                        } else if let Some(detail) = line.strip_prefix("PROGRESS ") {
                            if let Some(ratio) = detail.split_whitespace().next() {
                                if let Some((a, b)) = ratio.split_once('/') {
                                    if let (Ok(a), Ok(b)) = (a.parse::<f32>(), b.parse::<f32>()) {
                                        if b > 0.0 {
                                            job.progress = Some((a / b).clamp(0.0, 1.0));
                                        }
                                    }
                                }
                            }
                        }
                        if line.to_ascii_lowercase().contains("error:") {
                            job.had_error = true;
                        }
                        if line.contains("[download] Downloading item")
                            || line.contains("Destination:")
                        {
                            job.current_item = line.chars().take(160).collect();
                        }
                        job.message = line.chars().take(200).collect();
                        self.log.push(format!(
                            "#{} {}",
                            id,
                            line.chars().take(260).collect::<String>()
                        ));
                        if self.log.len() > 300 {
                            self.log.drain(..80);
                        }
                        changed = true;
                    }
                    DownloadEvent::Complete(result) => {
                        let was_stopping = job.state == State::Cancelling;
                        job.finished_unix = Some(now());
                        job.speed.clear();
                        job.eta.clear();
                        job.state = if was_stopping {
                            State::Cancelled
                        } else if result.is_err() || job.had_error {
                            State::Failed
                        } else {
                            State::Completed
                        };
                        job.message = match result {
                            Ok(()) if job.had_error => {
                                "yt-dlp reported an error; some items may be missing.".into()
                            }
                            Ok(()) => "Finished successfully".into(),
                            Err(e) => e,
                        };
                        if job.state == State::Completed {
                            job.progress = Some(1.0);
                        } else {
                            job.progress = None;
                        }
                        reel_finished |= job.kind.is_reel();
                        self.running.remove(&id);
                        changed = true;
                        finished_change = true;
                    }
                }
            }
        }
        if !self.remove_when_stopped.is_empty() {
            let completed: Vec<_> = self
                .remove_when_stopped
                .iter()
                .copied()
                .filter(|id| !self.running.contains_key(id))
                .collect();
            for id in completed {
                self.store.jobs.retain(|j| j.id != id);
                self.remove_when_stopped.remove(&id);
                changed = true;
            }
        }
        if changed && (finished_change || self.last_save.elapsed() > Duration::from_millis(750)) {
            self.save();
        }
        self.start_next();
        reel_finished
    }
    pub fn cancel(&mut self, id: u64) {
        if let Some(j) = self.store.jobs.iter_mut().find(|j| j.id == id) {
            match j.state {
                State::Queued => {
                    j.state = State::Cancelled;
                    j.message = "Cancelled before starting".into();
                    j.finished_unix = Some(now());
                }
                State::Running => {
                    j.state = State::Cancelling;
                    j.message = "Stopping download safely...".into();
                    if let Some(t) = self.running.get(&id) {
                        t.store(true, Ordering::Relaxed);
                    }
                }
                _ => {}
            }
        }
        self.save();
        self.start_next();
    }
    pub fn cancel_batch(&mut self, batch: u64) {
        let ids: Vec<_> = self
            .store
            .jobs
            .iter()
            .filter(|j| j.batch_id == batch && j.state.is_pending())
            .map(|j| j.id)
            .collect();
        for id in ids {
            self.cancel(id);
        }
    }
    pub fn retry(&mut self, id: u64) {
        if let Some(j) = self.store.jobs.iter_mut().find(|j| j.id == id) {
            if j.state.can_retry() {
                j.state = State::Queued;
                j.progress = None;
                j.had_error = false;
                j.message = "Queued for retry (existing files retained)".into();
                j.finished_unix = None;
            }
        }
        self.save();
        self.start_next();
    }
    pub fn retry_batch(&mut self, batch: u64) {
        let ids: Vec<_> = self
            .store
            .jobs
            .iter()
            .filter(|j| j.batch_id == batch && j.state.can_retry())
            .map(|j| j.id)
            .collect();
        for id in ids {
            self.retry(id);
        }
    }
    pub fn remove(&mut self, id: u64) {
        if self.running.contains_key(&id) {
            self.remove_when_stopped.insert(id);
            self.cancel(id);
            return;
        }
        self.store.jobs.retain(|j| j.id != id);
        self.save();
    }
    pub fn remove_batch(&mut self, batch: u64) {
        let ids: Vec<_> = self
            .store
            .jobs
            .iter()
            .filter(|j| j.batch_id == batch)
            .map(|j| j.id)
            .collect();
        for id in ids {
            self.remove(id);
        }
    }
    pub fn clear_finished(&mut self) {
        self.store.jobs.retain(|j| j.state.is_pending());
        self.save();
    }
    /// Remove completed queue-history entries for one platform, or both. Media stays on disk.
    pub fn clear_completed_for(&mut self, reels: bool, all: bool) {
        self.store
            .jobs
            .retain(|j| j.state != State::Completed || (!all && j.kind.is_reel() != reels));
        self.save();
    }

    /// On desktop exit: stop running child processes and preserve retryable queue entries.
    pub fn shutdown(&mut self) {
        for job in &mut self.store.jobs {
            match job.state {
                State::Queued => {
                    job.state = State::Interrupted;
                    job.message = "App closed before starting; retry when ready".into();
                }
                State::Running => {
                    job.state = State::Cancelling;
                    job.message = "Stopping because the app is closing".into();
                }
                _ => {}
            }
        }
        for flag in self.running.values() {
            flag.store(true, Ordering::Relaxed);
        }
        self.save();
        let end = Instant::now() + Duration::from_secs(7);
        while !self.running.is_empty() && Instant::now() < end {
            thread::sleep(Duration::from_millis(90));
            let _ = self.poll();
        }
        // A still-running child is recorded as interrupted, never falsely completed.
        for job in &mut self.store.jobs {
            if job.state == State::Cancelling {
                job.state = State::Interrupted;
                job.message =
                    "Shutdown was interrupted; retry only after checking active processes".into();
            }
        }
        self.save();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parsed_music_urls_dedupe() {
        assert_eq!(
            source_urls(
                "https://soundcloud.com/a/b\nhttps://soundcloud.com/a/b",
                false
            )
            .unwrap()
            .len(),
            1
        );
    }
    #[test]
    fn invalid_reel_rejected() {
        assert!(source_urls("https://example.com/reel/xyz/", true).is_err());
    }
    #[test]
    fn progress_formats() {
        assert_eq!(
            progress_from_line("GF_PROGRESS: 42.5%|2MiB/s|00:03")
                .unwrap()
                .0,
            0.425
        );
        assert_eq!(
            progress_from_line("[download]  75.0% of 2.2MiB at 1MiB/s ETA 00:04")
                .unwrap()
                .0,
            0.75
        );
    }
    #[test]
    fn state_is_pending() {
        assert!(State::Queued.is_pending());
        assert!(!State::Interrupted.is_pending());
    }
}
