//! Local Reel processing: Python stdlib media scanner + native in-process Laya decisions.
//! Never unsaves anything, never modifies the original downloaded video.
use crate::instagram::Reel;
use eframe::egui;
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    thread,
    time::{Duration, Instant},
};

const CATEGORIES: &[&str] = &[
    "Food and recipes",
    "Fitness and health",
    "Travel and places",
    "Technology and gadgets",
    "Home and DIY",
    "Education and tutorials",
    "Entertainment and humor",
    "Shopping and products",
    "Other",
];
pub fn categories() -> &'static [&'static str] {
    CATEGORIES
}
fn home() -> Result<PathBuf, String> {
    let p = std::env::current_exe().map_err(|e| e.to_string())?;
    Ok(p.parent().ok_or("Executable has no parent")?.to_path_buf())
}
fn script() -> Result<PathBuf, String> {
    let p = home()?.join("tools/reel_analysis.py");
    if !p.is_file() {
        return Err("Reel analysis helper missing; reinstall the Reel pipeline".into());
    }
    Ok(p)
}

// Drain both pipes while the helper is running, so long or multilingual
// transcripts cannot fill an OS pipe and leave the scan waiting forever.
fn read_helper_pipe<R: std::io::Read>(mut pipe: R) -> Result<Vec<u8>, String> {
    const LIMIT: usize = 2_100_000;
    let mut output = Vec::new();
    let mut buffer = [0_u8; 8192];
    let mut overflow = false;
    loop {
        let count = pipe.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 { break; }
        let remaining = LIMIT.saturating_sub(output.len());
        output.extend_from_slice(&buffer[..count.min(remaining)]);
        if count > remaining { overflow = true; }
    }
    if overflow { Err("Reel helper output exceeded its size limit".into()) } else { Ok(output) }
}

fn run(
    root: &Path,
    video: &str,
    models: &Path,
    action: &str,
    category: &str,
    cancel: &AtomicBool,
) -> Result<Value, String> {
    let script = script()?;
    let mut cmd = Command::new("python3");
    cmd.arg(script)
        .arg(action)
        .arg("--root")
        .arg(root)
        .arg("--video")
        .arg(video)
        .arg("--models")
        .arg(models)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if !category.is_empty() {
        cmd.arg("--category").arg(category);
    }
    // Each scan gets its own Unix process group so cancellation also stops
    // FFmpeg, Tesseract and Whisper children, not just the Python parent.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    let stdout = child.stdout.take().ok_or("Reel helper stdout unavailable")?;
    let stderr = child.stderr.take().ok_or("Reel helper stderr unavailable")?;
    let stdout_reader = thread::spawn(move || read_helper_pipe(stdout));
    let stderr_reader = thread::spawn(move || read_helper_pipe(stderr));
    let since = Instant::now();
    loop {
        if cancel.load(Ordering::Relaxed) || since.elapsed() > Duration::from_secs(1800) {
            #[cfg(unix)]
            {
                unsafe extern "C" { fn kill(pid: i32, signal: i32) -> i32; }
                if let Ok(pid) = i32::try_from(child.id()) {
                    // The spawned child owns the new process group whose ID is its PID.
                    if pid > 0 { unsafe { let _ = kill(-pid, 9); } }
                }
            }
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdout_reader.join();
            let _ = stderr_reader.join();
            return Err("Reel analysis cancelled or exceeded the 30-minute limit".into());
        }
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            let output = stdout_reader.join().map_err(|_| "Could not read Reel helper output")??;
            let errors = stderr_reader.join().map_err(|_| "Could not read Reel helper errors")??;
            if !status.success() {
                return Err(format!(
                    "Reel helper failed: {}",
                    String::from_utf8_lossy(&errors)
                        .chars()
                        .take(400)
                        .collect::<String>()
                ));
            }
            return serde_json::from_slice(&output)
                .map_err(|e| format!("Invalid Reel analysis response: {e}"));
        }
        thread::sleep(Duration::from_millis(120));
    }
}
fn record(
    root: &Path,
    video: &str,
    models: &Path,
    category: &str,
    confidence: f64,
    cancel: &AtomicBool,
) -> Result<(), String> {
    let script = script()?;
    let mut cmd = Command::new("python3");
    cmd.arg(script)
        .arg("record-category")
        .arg("--root")
        .arg(root)
        .arg("--video")
        .arg(video)
        .arg("--models")
        .arg(models)
        .arg("--category")
        .arg(category)
        .arg("--confidence")
        .arg(confidence.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    let start = Instant::now();
    loop {
        if cancel.load(Ordering::Relaxed) || start.elapsed() > Duration::from_secs(30) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Category update cancelled".into());
        }
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            let out = child.wait_with_output().map_err(|e| e.to_string())?;
            if !status.success() {
                return Err(String::from_utf8_lossy(&out.stderr)
                    .chars()
                    .take(400)
                    .collect());
            }
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }
}
#[derive(Debug)]
enum Update {
    Message(String),
    Done(Result<String, String>),
}
pub struct Flow {
    rx: Option<Receiver<Update>>,
    cancel: Option<Arc<AtomicBool>>,
    pub notice: String,
    pub selected: std::collections::BTreeMap<String, String>,
    pub changed: bool,
}
impl Flow {
    pub fn new() -> Self {
        Self {
            rx: None,
            cancel: None,
            notice: "Analyze and approve Reels before unsaving. No media is deleted.".into(),
            selected: Default::default(),
            changed: false,
        }
    }
    pub fn busy(&self) -> bool {
        self.rx.is_some()
    }
    pub fn cancel(&self) {
        if let Some(flag) = &self.cancel {
            flag.store(true, Ordering::Relaxed)
        }
    }
    pub fn poll(&mut self, ctx: &egui::Context) {
        let mut done = None;
        if let Some(rx) = &self.rx {
            for _ in 0..24 {
                match rx.try_recv() {
                    Ok(Update::Message(msg)) => self.notice = msg,
                    Ok(Update::Done(v)) => {
                        done = Some(v);
                        break;
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        done = Some(Err("Analysis worker disconnected".into()));
                        break;
                    }
                }
            }
        }
        if let Some(v) = done {
            self.notice = match v {
                Ok(s) => s,
                Err(e) => format!("Analysis error: {e}"),
            };
            self.rx = None;
            self.cancel = None;
            self.changed = true;
        }
        if self.busy() {
            ctx.request_repaint_after(Duration::from_millis(150));
        }
    }
    pub fn analyze(&mut self, root: PathBuf, items: Vec<Reel>) {
        if self.busy() || items.is_empty() {
            return;
        }
        let Ok(home) = home() else {
            self.notice = "Cannot locate app directory".into();
            return;
        };
        let models = home.join("models");
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&cancel);
        let (tx, rx) = mpsc::channel::<Update>();
        self.cancel = Some(cancel);
        self.rx = Some(rx);
        self.notice = format!("Analyzing {} Reel(s)...", items.len());
        thread::spawn(move || {
            let mut ok = 0;
            let mut failed = 0;
            let total = items.len();
            // Run speech/OCR/vision before loading the ~2.4GB Laya model. Keep only text.
            let mut evidence: Vec<(String, String, String)> = Vec::new();
            for (i, reel) in items.iter().enumerate() {
                if flag.load(Ordering::Relaxed) {
                    break;
                }
                let _ = tx.send(Update::Message(format!(
                    "Reel {}/{}: extracting evidence for {}",
                    i + 1,
                    total,
                    reel.id
                )));
                match run(&root, &reel.video, &models, "analyze", "", &flag) {
                    Ok(info) => {
                        let rel = info
                            .get("video_relative")
                            .and_then(Value::as_str)
                            .unwrap_or(&reel.video)
                            .to_owned();
                        let text = info
                            .get("text")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_owned();
                        evidence.push((reel.id.clone(), rel, text));
                    }
                    Err(e) => {
                        failed += 1;
                        let _ = tx.send(Update::Message(format!("{}: {e}", reel.id)));
                    }
                }
            }
            if !flag.load(Ordering::Relaxed) && !evidence.is_empty() {
                let _ = tx.send(Update::Message(
                    "Loading local Laya once for the scanned batch...".into(),
                ));
                let dir = models.join("installed/classification/laya-multilingual");
                let agent = if dir.join("model.safetensors").is_file() {
                    match laya::Agent::from_dir(&dir, Default::default()) {
                        Ok(agent) => Some(agent),
                        Err(e) => {
                            let _ = tx.send(Update::Message(format!(
                                "Laya load failed: {e}; manual category review required"
                            )));
                            None
                        }
                    }
                } else {
                    let _ = tx.send(Update::Message(
                        "Laya model missing; manual category selection required".into(),
                    ));
                    None
                };
                for (i, (id, rel, text)) in evidence.into_iter().enumerate() {
                    if flag.load(Ordering::Relaxed) {
                        break;
                    }
                    let _ = tx.send(Update::Message(format!(
                        "Laya {}/{}: categorizing {}",
                        i + 1,
                        total,
                        id
                    )));
                    if let Some(agent) = &agent {
                        if text.trim().is_empty() {
                            ok += 1;
                            continue;
                        }
                        match crate::ai_models::categorize_with_agent(agent, &text) {
                            Ok((category, confidence)) => {
                                match record(&root, &rel, &models, &category, confidence, &flag) {
                                    Ok(()) => {
                                        ok += 1;
                                    }
                                    Err(e) => {
                                        failed += 1;
                                        let _ = tx.send(Update::Message(format!("{id}: {e}")));
                                    }
                                }
                            }
                            Err(e) => {
                                failed += 1;
                                let _ =
                                    tx.send(Update::Message(format!("Laya failed for {id}: {e}")));
                            }
                        }
                    } else {
                        ok += 1;
                    }
                }
            }
            let _=tx.send(Update::Done(Ok(format!("Scan finished: {ok} reviewed/pending, {failed} error(s). Inspect evidence and approve each category before filing.{}",
                if flag.load(Ordering::Relaxed){" Cancelled remaining jobs."}else{""}))));
        });
    }
    pub fn setup_engine(&mut self, vision: bool) {
        if self.busy() {
            return;
        }
        let Ok(home) = home() else {
            self.notice = "Cannot find app directory".into();
            return;
        };
        let script = home.join("tools/setup_reel_engines.sh");
        if !script.is_file() {
            self.notice = "AI runtime installer missing from project".into();
            return;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&cancel);
        let (tx, rx) = mpsc::channel();
        self.cancel = Some(cancel);
        self.rx = Some(rx);
        self.notice = if vision {
            "Building optional local vision engine..."
        } else {
            "Building local Whisper speech engine..."
        }
        .into();
        thread::spawn(move || {
            let mut cmd = Command::new("bash");
            cmd.arg(&script)
                .arg(if vision { "--vision" } else { "--speech" })
                .current_dir(&home)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            let result = (|| -> Result<String, String> {
                let mut child = cmd.spawn().map_err(|e| e.to_string())?;
                let started = Instant::now();
                loop {
                    if flag.load(Ordering::Relaxed) || started.elapsed() > Duration::from_secs(1800)
                    {
                        let _ = child.kill();
                        let _ = child.wait();
                        return Err("Engine installation cancelled/timed out".into());
                    }
                    if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
                        let output = child.wait_with_output().map_err(|e| e.to_string())?;
                        if !status.success() {
                            return Err(format!(
                                "Engine build failed: {}",
                                String::from_utf8_lossy(&output.stderr)
                                    .chars()
                                    .rev()
                                    .take(350)
                                    .collect::<String>()
                                    .chars()
                                    .rev()
                                    .collect::<String>()
                            ));
                        }
                        return Ok("Local inference engine installed. Analyze a Reel now.".into());
                    }
                    thread::sleep(Duration::from_millis(220));
                }
            })();
            let _ = tx.send(Update::Done(result));
        });
    }
    pub fn approve(&mut self, root: PathBuf, reel: Reel, category: String) {
        if self.busy() {
            return;
        }
        if !CATEGORIES.contains(&category.as_str()) {
            self.notice = "Select one valid category".into();
            return;
        }
        let Ok(home) = home() else { return };
        let models = home.join("models");
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&cancel);
        let (tx, rx) = mpsc::channel();
        self.cancel = Some(cancel);
        self.rx = Some(rx);
        self.notice = format!("Verifying and filing {}...", reel.id);
        thread::spawn(move || {
            let result=run(&root,&reel.video,&models,"approve",&category,&flag).map(|v|
                format!("Approved and filed {} under {}. Eligible for manual browser unsave after verification.",
                    v.get("id").and_then(Value::as_str).unwrap_or("Reel"),category));
            let _ = tx.send(Update::Done(result));
        });
    }
    pub fn read(root: &Path, video_rel: &str) -> Option<Value> {
        let rel = Path::new(video_rel);
        if rel.is_absolute()
            || rel
                .components()
                .any(|v| matches!(v, std::path::Component::ParentDir))
        {
            return None;
        }
        let path = root.join(rel).parent()?.join("analysis.json");
        if path.is_symlink() || !path.is_file() || path.metadata().ok()?.len() > 100_000 {
            return None;
        }
        let bytes = fs::read(path).ok()?;
        serde_json::from_slice(&bytes).ok()
    }
    pub fn select_category<'a>(&'a mut self, id: &str, analysis: &Value) -> &'a mut String {
        let proposed = analysis
            .get("approved_category")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .or_else(|| analysis.get("category_suggested").and_then(Value::as_str))
            .filter(|s| CATEGORIES.contains(s))
            .unwrap_or("");
        self.selected
            .entry(id.to_owned())
            .or_insert_with(|| proposed.to_owned())
    }
}
