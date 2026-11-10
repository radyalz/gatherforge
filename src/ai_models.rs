//! Native, app-contained model lifecycle. No Python model-manager sidecar.
// GATHERFORGE_LAYA_NATIVE_V394: native in-process caption-classification model.
//! Models are catalog-pinned, verified, and live beside the installed app.
//! This module manages weights/configs; reel_flow uses native Laya for classification.
use crate::theme;
use eframe::egui::{self, RichText};
use serde::{Deserialize, Serialize};
use sha2::Digest;
use std::{
    collections::{HashMap, VecDeque},
    env, fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    thread,
    time::Duration,
};

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
struct AiConfig {
    schema_version: u32,
    storage_limit_mb: u32,
    auto_download: bool,
    speech_backend: String,
    speech_model: String,
    speech_language: String,
    ocr_backend: String,
    vision_backend: String,
    classification_backend: String,
    speech_custom_path: String,
    ocr_custom_path: String,
    vision_custom_path: String,
    classification_custom_path: String,
    api_endpoint: String,
    api_model: String,
    api_key_env: String,
    send_private_media_to_api: bool,
    auto_unsave: bool,
    require_approval_before_unsave: bool,
}
impl Default for AiConfig {
    fn default() -> Self {
        Self {
            schema_version: 3,
            storage_limit_mb: 1100,
            auto_download: true,
            speech_backend: "managed_local".into(),
            speech_model: "tiny".into(),
            speech_language: "auto".into(),
            ocr_backend: "managed_local".into(),
            vision_backend: "disabled".into(),
            classification_backend: "laya_local".into(),
            speech_custom_path: String::new(),
            ocr_custom_path: String::new(),
            vision_custom_path: String::new(),
            classification_custom_path: String::new(),
            api_endpoint: String::new(),
            api_model: String::new(),
            api_key_env: "GATHERFORGE_AI_API_KEY".into(),
            send_private_media_to_api: false,
            auto_unsave: false,
            require_approval_before_unsave: true,
        }
    }
}
impl AiConfig {
    fn load(home: &Path) -> Self {
        let internal = home.join("models/config.local.json");
        if let Ok(bytes) = fs::read(&internal) {
            if let Ok(mut config) = serde_json::from_slice::<Self>(&bytes) {
                if config.schema_version < 3 {
                    config.schema_version = 3;
                    // v3.9.2 never actually ran a classifier. Upgrade its placeholder
                    // rules default to the requested native Laya backend.
                    if config.classification_backend == "rules" {
                        config.classification_backend = "laya_local".into();
                    }
                    config.storage_limit_mb = config.storage_limit_mb.max(1000);
                }
                return config;
            }
        }
        // One-time migration from v3.9.1. New writes are only beside the app.
        if let Some(old) = dirs::config_dir().map(|p| p.join("gatherforge/ai.json")) {
            if let Ok(bytes) = fs::read(old) {
                if let Ok(mut config) = serde_json::from_slice::<Self>(&bytes) {
                    config.schema_version = 3;
                    if config.classification_backend == "rules" {
                        config.classification_backend = "laya_local".into();
                    }
                    config.storage_limit_mb = config.storage_limit_mb.max(1000);
                    config.auto_download = true;
                    config.auto_unsave = false;
                    config.require_approval_before_unsave = true;
                    return config;
                }
            }
        }
        Self::default()
    }
    fn save(&self, home: &Path) -> Result<(), String> {
        if !(1..=102400).contains(&self.storage_limit_mb) {
            return Err("Invalid model storage limit".into());
        }
        if self.api_key_env.is_empty()
            || !self
                .api_key_env
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            return Err(
                "API credential must be the name of an environment variable, not a secret".into(),
            );
        }
        let path = home.join("models/config.local.json");
        if path.is_symlink() {
            return Err("Refusing symlink config".into());
        }
        fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
        let temp = path.with_extension("json.tmp");
        if temp.is_symlink() {
            return Err("Refusing symlink temporary config".into());
        }
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        fs::write(&temp, bytes).map_err(|e| e.to_string())?;
        fs::rename(&temp, path).map_err(|e| e.to_string())
    }
}

#[derive(Clone, Deserialize)]
struct Model {
    id: String,
    title: String,
    role: String,
    engine: String,
    size_mb: u32,
    relative_path: String,
    url: String,
    hash_type: String,
    hash: String,
    #[serde(default)]
    size_bytes: Option<u64>,
    #[serde(default)]
    default_install: bool,
    #[serde(default)]
    repo_bundlable: bool,
}
#[derive(Deserialize)]
struct Catalog {
    models: Vec<Model>,
}

enum Update {
    Progress(u64, u64),
    Done(Result<String, String>),
}
struct Task {
    id: String,
    action: String,
    cancel: Arc<AtomicBool>,
    rx: Receiver<Update>,
    progress: (u64, u64),
}

fn app_home() -> PathBuf {
    if let Some(dir) = env::var_os("GATHERFORGE_HOME") {
        let path = PathBuf::from(dir);
        if path.is_absolute() {
            return path;
        }
    }
    env::current_exe()
        .ok()
        .and_then(|p| p.canonicalize().ok())
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
}
fn valid_relative(rel: &str) -> bool {
    let p = Path::new(rel);
    !p.as_os_str().is_empty()
        && !p.is_absolute()
        && p.components().all(|p| matches!(p, Component::Normal(_)))
}
fn validate_catalog(items: Vec<Model>) -> Vec<Model> {
    items
        .into_iter()
        .filter(|m| {
            !m.id.is_empty()
                && m.id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
                && valid_relative(&m.relative_path)
                && (m.url.starts_with("https://huggingface.co/")
                    || m.url.starts_with("https://raw.githubusercontent.com/"))
                && (m.hash_type == "sha256" || m.hash_type == "git_blob_sha1")
                && m.hash.len() == if m.hash_type == "sha256" { 64 } else { 40 }
        })
        .collect()
}
fn digest(path: &Path, m: &Model) -> Result<String, String> {
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let size = file.metadata().map_err(|e| e.to_string())?.len();
    let mut buffer = [0u8; 131072];
    if m.hash_type == "git_blob_sha1" {
        let mut h = sha1::Sha1::new();
        h.update(format!("blob {size}\0").as_bytes());
        loop {
            let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            h.update(&buffer[..n]);
        }
        Ok(format!("{:x}", h.finalize()))
    } else {
        let mut h = sha2::Sha256::new();
        loop {
            let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            h.update(&buffer[..n]);
        }
        Ok(format!("{:x}", h.finalize()))
    }
}
fn verified(path: &Path, m: &Model) -> Result<bool, String> {
    if !path.is_file() || path.is_symlink() {
        return Ok(false);
    }
    if let Some(size) = m.size_bytes {
        if fs::metadata(path).map_err(|e| e.to_string())?.len() != size {
            return Ok(false);
        }
    }
    Ok(digest(path, m)?.eq_ignore_ascii_case(&m.hash))
}
fn bundled(home: &Path, m: &Model) -> PathBuf {
    home.join("models/bundled").join(&m.relative_path)
}
fn installed(home: &Path, m: &Model) -> PathBuf {
    home.join("models/installed").join(&m.relative_path)
}

fn download(
    home: &Path,
    m: &Model,
    cancel: &AtomicBool,
    tx: &Sender<Update>,
    limit_mb: u32,
) -> Result<String, String> {
    let bundled_path = bundled(home, m);
    if verified(&bundled_path, m)? {
        if m.id == LAYA_ID {
            ensure_laya_parts(bundled_path.parent().ok_or("Bad Laya path")?, cancel)?;
        }
        return Ok(format!(
            "Verified bundled model: {}",
            bundled_path.display()
        ));
    }
    let dest = installed(home, m);
    if verified(&dest, m)? {
        if m.id == LAYA_ID {
            ensure_laya_parts(dest.parent().ok_or("Bad Laya path")?, cancel)?;
        }
        return Ok(format!("Verified installed model: {}", dest.display()));
    }
    if dest.exists() {
        return Err(format!(
            "An invalid model already exists at {}. Inspect before replacing.",
            dest.display()
        ));
    }
    if cancel.load(Ordering::Relaxed) {
        return Err("Cancelled".into());
    }
    let parent = dest.parent().ok_or("Invalid model path")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    if parent.is_symlink() || dest.is_symlink() {
        return Err("Refusing symlinked destination".into());
    }
    let part = dest.with_extension(format!(
        "{}.part",
        dest.extension().and_then(|e| e.to_str()).unwrap_or("model")
    ));
    if part.is_symlink() {
        return Err("Refusing symlinked partial download".into());
    }
    let mut offset = part.metadata().map(|m| m.len()).unwrap_or(0);
    let ceiling = m
        .size_bytes
        .unwrap_or(u64::from(m.size_mb.saturating_add(10)) * 1_048_576);
    if ceiling > u64::from(limit_mb) * 1_048_576 {
        return Err("Exceeds configured model budget".into());
    }
    // Respect the aggregate budget: bundled assets, installed models, and partials.
    let usage = walkdir::WalkDir::new(home.join("models"))
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file() && !e.path().is_symlink())
        .filter_map(|e| e.metadata().ok().map(|m| m.len()))
        .sum::<u64>();
    if usage.saturating_add(ceiling.saturating_sub(offset)) > u64::from(limit_mb) * 1_048_576 {
        return Err("Not enough app model storage budget".into());
    }
    if offset > ceiling {
        return Err("Existing partial file is oversized".into());
    }
    let client = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(15))
        .timeout_read(Duration::from_secs(35))
        .build();
    let mut request = client.get(&m.url).set("User-Agent", "GatherForge/3.9.4");
    if offset > 0 {
        request = request.set("Range", &format!("bytes={offset}-"));
    }
    let response = request.call().map_err(|e| e.to_string())?;
    let status = response.status();
    if offset > 0 && status == 206 {
        let expected = format!("bytes {offset}-");
        if !response
            .header("Content-Range")
            .unwrap_or("")
            .starts_with(&expected)
        {
            return Err("Resume offset mismatch".into());
        }
    } else if status == 200 {
        offset = 0; // Server ignored Range: safely restart instead of appending.
    } else {
        return Err(format!("Unexpected HTTP status {status}"));
    }
    let mut file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(offset > 0)
        .truncate(offset == 0)
        .open(&part)
        .map_err(|e| e.to_string())?;
    let mut source = response.into_reader();
    let mut buffer = [0u8; 131072];
    let mut count = offset;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("Cancelled (partial kept for resume)".into());
        }
        let n = source.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        count += n as u64;
        if count > ceiling {
            return Err("Download exceeded catalog size bound".into());
        }
        file.write_all(&buffer[..n]).map_err(|e| e.to_string())?;
        let _ = tx.send(Update::Progress(count, ceiling));
    }
    file.flush().map_err(|e| e.to_string())?;
    drop(file);
    if !verified(&part, m)? {
        return Err("Hash mismatch: incomplete/corrupt download left as .part".into());
    }
    fs::rename(&part, &dest).map_err(|e| e.to_string())?;
    if m.id == LAYA_ID {
        ensure_laya_parts(dest.parent().ok_or("Bad Laya path")?, cancel)?;
    }
    Ok(format!("Installed and verified: {}", dest.display()))
}

// The official Laya checkpoint contains a weight file and four JSON metadata files.
// It is not usable with only model.safetensors, so repair installs all five.
const LAYA_ID: &str = "laya-multilingual";
const LAYA_REV: &str = "453577addb8a9c85395d2449a91fce4742f5f2f1";
const LAYA_PARTS: [(&str, &str, u64); 4] = [
    (
        "encoder/config.json",
        "83f6916d13ef0f556ac461f28308dc2bffa7ebeadee8ec9e2db5812020ea5bb4",
        1938,
    ),
    (
        "tokenizer/tokenizer.json",
        "609d8f4c067cd3950f88594c5a802616cea245823836ef5848ee4fc40aab5b6f",
        34363188,
    ),
    (
        "tokenizer/tokenizer_config.json",
        "424b69444bf7b5809dc2cd2e36d0bd71b8055124dd24274d6db3c655d38205e7",
        502,
    ),
    (
        "rl_agent_config.json",
        "25061739243b617ad88d1219ba6f8a9c86c5881ca28df024fa2d9b3b2fcc30c6",
        472,
    ),
];
fn valid_laya_part(path: &Path, expected_hash: &str, expected_size: u64) -> bool {
    if path.is_symlink() || !path.is_file() {
        return false;
    }
    let Ok(bytes) = fs::read(path) else {
        return false;
    };
    if bytes.len() as u64 != expected_size {
        return false;
    }
    let mut h = sha2::Sha256::new();
    h.update(&bytes);
    if format!("{:x}", h.finalize()) != expected_hash {
        return false;
    }
    serde_json::from_slice::<serde_json::Value>(&bytes).is_ok()
}
fn laya_ready(dir: &Path) -> bool {
    LAYA_PARTS
        .iter()
        .all(|(rel, hash, size)| valid_laya_part(&dir.join(rel), hash, *size))
}
fn ensure_laya_parts(dir: &Path, cancel: &AtomicBool) -> Result<(), String> {
    let client = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(15))
        .timeout_read(Duration::from_secs(40))
        .build();
    for (rel, hash, size) in LAYA_PARTS {
        if cancel.load(Ordering::Relaxed) {
            return Err("Laya metadata setup cancelled".into());
        }
        let target = dir.join(rel);
        if valid_laya_part(&target, hash, size) {
            continue;
        }
        if target.is_file() || target.is_symlink() {
            return Err(format!(
                "Laya asset exists but failed verification: {}",
                target.display()
            ));
        }
        let parent = target.parent().ok_or("Invalid Laya directory")?;
        if parent.is_symlink() {
            return Err("Refusing symlink Laya directory".into());
        }
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let url = format!(
            "https://huggingface.co/convaiinnovations/laya-multilingual/resolve/{LAYA_REV}/{rel}"
        );
        let response = client
            .get(&url)
            .set("User-Agent", "GatherForge/3.9.4")
            .call()
            .map_err(|e| format!("Laya metadata {rel}: {e}"))?;
        if response.status() != 200 {
            return Err(format!("Laya {rel}: HTTP {}", response.status()));
        }
        let mut bytes = Vec::new();
        response
            .into_reader()
            .take(size + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() as u64 != size {
            return Err(format!("Laya asset size mismatch: {rel}"));
        }
        let mut h = sha2::Sha256::new();
        h.update(&bytes);
        if format!("{:x}", h.finalize()) != hash {
            return Err(format!("Laya asset SHA-256 mismatch: {rel}"));
        }
        let _: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|e| format!("Invalid Laya metadata {rel}: {e}"))?;
        let tmp = target.with_extension("json.part");
        if tmp.exists() || tmp.is_symlink() {
            return Err("Laya asset temporary file exists".into());
        }
        fs::write(&tmp, bytes).map_err(|e| e.to_string())?;
        if let Err(e) = fs::rename(&tmp, &target) {
            let _ = fs::remove_file(&tmp);
            return Err(e.to_string());
        }
    }
    Ok(())
}
pub(crate) fn categorize_with_agent(
    agent: &laya::Agent,
    caption: &str,
) -> Result<(String, f64), String> {
    if caption.trim().is_empty() {
        return Err("No text evidence for classification".into());
    }
    let questions = vec![(
        "category".to_string(),
        laya::Question::choice(
            "Choose the main subject of this Instagram Reel using caption, speech, and visible text. If unclear, choose Other.",
            [
                "Food and recipes",
                "Fitness and health",
                "Travel and places",
                "Technology and gadgets",
                "Home and DIY",
                "Education and tutorials",
                "Entertainment and humor",
                "Shopping and products",
                "Other",
            ],
        ),
    )];
    let text = caption.chars().take(5000).collect::<String>();
    let result = agent
        .system_one(&serde_json::json!(text), &questions)
        .map_err(|e| e.to_string())?;
    let v = serde_json::to_value(&result).map_err(|e| e.to_string())?;
    let category = v
        .pointer("/answers/category/choice")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("Other")
        .to_owned();
    let confidence = v
        .pointer("/answers/category/confidence")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(0.0)
        .clamp(0.0, 1.0);
    Ok((category, confidence))
}
fn categorize_laya(dir: &Path, caption: &str) -> Result<String, String> {
    if !laya_ready(dir) {
        return Err("Laya metadata missing; use Install / repair".into());
    }
    let agent = laya::Agent::from_dir(dir, Default::default()).map_err(|e| e.to_string())?;
    let (cat, confidence) = categorize_with_agent(&agent, caption)?;
    Ok(format!(
        "{cat} (confidence {confidence:.2}) — review required before organizing or unsaving"
    ))
}

pub struct Panel {
    home: PathBuf,
    config: AiConfig,
    models: Vec<Model>,
    queue: VecDeque<String>,
    task: Option<Task>,
    messages: HashMap<String, String>,
    confirm_remove: Option<String>,
    notice: String,
    reel_suggestions: HashMap<String, String>,
}
impl Panel {
    pub fn new() -> Self {
        let home = app_home();
        let config = AiConfig::load(&home);
        let _ = config.save(&home);
        let models = validate_catalog(
            serde_json::from_str::<Catalog>(include_str!("../models/catalog.json"))
                .map(|c| c.models)
                .unwrap_or_default(),
        );
        let queue = if config.auto_download {
            models
                .iter()
                .filter(|m| {
                    m.default_install
                        || (m.id == LAYA_ID && config.classification_backend == "laya_local")
                })
                .map(|m| m.id.clone())
                .collect()
        } else {
            VecDeque::new()
        };
        let mut this = Self {
            home,
            config,
            models,
            queue,
            task: None,
            messages: HashMap::new(),
            confirm_remove: None,
            notice: "Native model management. Starting automatic setup.".into(),
            reel_suggestions: HashMap::new(),
        };
        this.start_next();
        this
    }
    fn find(&self, id: &str) -> Option<&Model> {
        self.models.iter().find(|m| m.id == id)
    }
    fn start(&mut self, id: &str, action: &str) {
        if self.task.is_some() {
            return;
        }
        let Some(model) = self.find(id).cloned() else {
            return;
        };
        let home = self.home.clone();
        let budget = self.config.storage_limit_mb;
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&cancel);
        let (tx, rx) = mpsc::channel::<Update>();
        let task_action = action.to_owned();
        thread::spawn(move || {
            let result = match task_action.as_str() {
                "install" | "verify" => {
                    if task_action == "verify" {
                        match verified(&bundled(&home, &model), &model).and_then(|ok| {
                            if ok {
                                Ok(true)
                            } else {
                                verified(&installed(&home, &model), &model)
                            }
                        }) {
                            Ok(true)
                                if model.id != LAYA_ID
                                    || laya_ready(
                                        installed(&home, &model).parent().unwrap_or(&home),
                                    ) =>
                            {
                                Ok("Checksum verified".into())
                            }
                            Ok(true) => Err(
                                "Laya weights present, support files missing: Install / repair"
                                    .into(),
                            ),
                            Ok(false) => Err("Model missing or hash mismatch".into()),
                            Err(e) => Err(e),
                        }
                    } else {
                        download(&home, &model, &flag, &tx, budget)
                    }
                }
                "remove" => {
                    let path = installed(&home, &model);
                    if path.is_symlink() {
                        Err("Refusing symlink deletion".into())
                    } else if path.is_file() {
                        fs::remove_file(&path)
                            .map(|_| format!("Removed managed copy: {}", path.display()))
                            .map_err(|e| e.to_string())
                    } else {
                        Ok("No removable downloaded copy (bundled models are preserved)".into())
                    }
                }
                _ => Err("Unknown model action".into()),
            };
            let _ = tx.send(Update::Done(result));
        });
        self.task = Some(Task {
            id: id.to_string(),
            action: action.to_string(),
            cancel,
            rx,
            progress: (0, 0),
        });
    }
    pub fn suggestion(&self, id: &str) -> Option<&str> {
        self.reel_suggestions.get(id).map(String::as_str)
    }
    pub fn classify_reel(&mut self, id: &str, caption: &str) {
        if self.task.is_some() {
            self.notice = "Finish the current model operation first".into();
            return;
        }
        if self.config.classification_backend != "laya_local" {
            self.notice =
                "Choose Laya Multilingual (local) as categorization backend in Settings".into();
            return;
        }
        let Some(m) = self.find(LAYA_ID).cloned() else {
            self.notice = "Laya catalog entry missing".into();
            return;
        };
        let path = installed(&self.home, &m);
        if !verified(&path, &m).unwrap_or(false) {
            self.notice = "Laya model is not installed or its checksum is invalid".into();
            return;
        }
        let Some(model_dir) = path.parent().map(Path::to_path_buf) else {
            return;
        };
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel();
        let caption = caption.to_owned();
        let id = id.to_owned();
        self.notice = format!("Analyzing Reel {} locally with Laya...", id);
        thread::spawn(move || {
            let result = categorize_laya(&model_dir, &caption);
            let _ = tx.send(Update::Done(result));
        });
        self.task = Some(Task {
            id: format!("reel:{id}"),
            action: "classify".into(),
            cancel,
            rx,
            progress: (0, 0),
        });
    }
    fn start_next(&mut self) {
        if self.task.is_some() {
            return;
        }
        if let Some(id) = self.queue.pop_front() {
            self.start(&id, "install");
        }
    }
    pub fn tick(&mut self, ctx: &egui::Context) {
        let mut finished: Option<(String, String)> = None;
        if let Some(task) = &mut self.task {
            for _ in 0..16 {
                match task.rx.try_recv() {
                    Ok(Update::Progress(n, max)) => task.progress = (n, max),
                    Ok(Update::Done(result)) => {
                        let msg = match result {
                            Ok(s) => format!("Ready: {s}"),
                            Err(s) => format!("Error: {s}"),
                        };
                        finished = Some((task.id.clone(), msg));
                        break;
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        finished =
                            Some((task.id.clone(), "Error: model worker disconnected".into()));
                        break;
                    }
                }
            }
        }
        if let Some((id, msg)) = finished {
            self.notice = msg.clone();
            if let Some(reel_id) = id.strip_prefix("reel:") {
                self.reel_suggestions
                    .insert(reel_id.to_owned(), msg.clone());
            }
            self.messages.insert(id, msg);
            self.task = None;
            self.start_next();
        }
        if self.task.is_some() || !self.queue.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(350));
        }
    }
    fn backend(ui: &mut egui::Ui, name: &str, id: &str, selected: &mut String, allow_rules: bool) {
        ui.horizontal(|ui| {
            ui.label(name);
            egui::ComboBox::from_id_salt(id)
                .selected_text(selected.as_str())
                .show_ui(ui, |ui| {
                    if allow_rules {
                        ui.selectable_value(
                            selected,
                            "laya_local".into(),
                            "Laya Multilingual · native Rust (local)",
                        );
                        ui.selectable_value(selected, "rules".into(), "Built-in rules (fallback)");
                    } else {
                        ui.selectable_value(selected, "managed_local".into(), "Managed locally");
                    }
                    ui.selectable_value(selected, "custom_local".into(), "Existing local model");
                    ui.selectable_value(selected, "api".into(), "External API (configured only)");
                    ui.selectable_value(selected, "disabled".into(), "Disabled");
                });
        });
    }
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.heading("AI & local models");
        ui.label(RichText::new("Local-first models. Laya categorizes combined Reel metadata, speech, screen text and optional visual evidence. Review approval is mandatory before filing or unsaving.")
            .size(12.0).color(theme::muted()));
        ui.add_space(8.0);
        ui.label(
            RichText::new(format!("GatherForge app folder: {}", self.home.display()))
                .size(11.0)
                .color(theme::muted()),
        );
        ui.label(
            RichText::new(format!(
                "Model config: {}",
                self.home.join("models/config.local.json").display()
            ))
            .size(11.0)
            .color(theme::muted()),
        );
        ui.label(
            RichText::new(format!(
                "Managed models: {}",
                self.home.join("models/installed").display()
            ))
            .size(11.0)
            .color(theme::muted()),
        );
        ui.label(
            RichText::new(format!(
                "Repo-bundled models: {}",
                self.home.join("models/bundled").display()
            ))
            .size(11.0)
            .color(theme::muted()),
        );
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui
                .checkbox(
                    &mut self.config.auto_download,
                    "Automatically set up default lightweight models",
                )
                .changed()
            {
                match self.config.save(&self.home) {
                    Ok(()) => {
                        if self.config.auto_download {
                            self.queue = self
                                .models
                                .iter()
                                .filter(|m| {
                                    m.default_install
                                        || (m.id == LAYA_ID
                                            && self.config.classification_backend == "laya_local")
                                })
                                .map(|m| m.id.clone())
                                .collect();
                            self.start_next();
                        } else {
                            self.queue.clear();
                            if let Some(task) = &self.task {
                                task.cancel.store(true, Ordering::Relaxed);
                            }
                        }
                    }
                    Err(err) => self.notice = err,
                }
            }
        });
        ui.horizontal(|ui| {
            ui.label("Storage budget (MB)");
            ui.add(egui::DragValue::new(&mut self.config.storage_limit_mb).range(1..=102400));
            if ui.button("Save config").clicked() {
                self.notice = self
                    .config
                    .save(&self.home)
                    .map(|_| "Configuration saved beside GatherForge".into())
                    .unwrap_or_else(|e| e);
            }
            if ui.button("Retry defaults").clicked() {
                self.queue = self
                    .models
                    .iter()
                    .filter(|m| {
                        m.default_install
                            || (m.id == LAYA_ID
                                && self.config.classification_backend == "laya_local")
                    })
                    .map(|m| m.id.clone())
                    .collect();
                self.start_next();
            }
            if self.task.is_some() && ui.button("Stop setup").clicked() {
                self.queue.clear();
                self.config.auto_download = false;
                let _ = self.config.save(&self.home);
                if let Some(t) = &self.task {
                    t.cancel.store(true, Ordering::Relaxed);
                }
            }
        });
        ui.add_space(7.0);
        ui.label(RichText::new("Models · assets and configuration").strong());
        for m in self.models.clone() {
            let bundled_path = bundled(&self.home, &m);
            let installed_path = installed(&self.home, &m);
            let bundle_present = bundled_path.is_file();
            let installed_present = installed_path.is_file();
            let busy = self.task.is_some();
            let is_current = self.task.as_ref().is_some_and(|t| t.id == m.id);
            let status = self.messages.get(&m.id).cloned().unwrap_or_else(|| {
                if bundle_present {
                    "Bundled (auto-verification pending)".into()
                } else if installed_present {
                    "Local copy (auto-verification pending)".into()
                } else {
                    "Needs download".into()
                }
            });
            egui::Frame::new()
                .fill(theme::surface())
                .stroke(egui::Stroke::new(1.0_f32, theme::stroke()))
                .corner_radius(egui::CornerRadius::same(theme::RADIUS))
                .inner_margin(egui::Margin::same(8))
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.vertical(|ui| {
                            ui.label(RichText::new(&m.title).strong());
                            ui.label(
                                RichText::new(format!(
                                    "{} · {} · ~{} MB · {}",
                                    m.role, m.engine, m.size_mb, status
                                ))
                                .size(11.0)
                                .color(theme::muted()),
                            );
                            ui.label(
                                RichText::new(format!(
                                    "Location: {}",
                                    if bundle_present {
                                        bundled_path.display().to_string()
                                    } else {
                                        installed_path.display().to_string()
                                    }
                                ))
                                .size(10.0)
                                .color(theme::muted()),
                            );
                        });
                        if ui
                            .add_enabled(!busy, egui::Button::new("Install / repair"))
                            .clicked()
                        {
                            self.queue.clear();
                            self.start(&m.id, "install");
                        }
                        if ui.add_enabled(!busy, egui::Button::new("Verify")).clicked() {
                            self.start(&m.id, "verify");
                        }
                        if installed_present
                            && ui
                                .add_enabled(!busy, egui::Button::new("Remove copy..."))
                                .clicked()
                        {
                            self.confirm_remove = Some(m.id.clone());
                        }
                        if is_current && ui.button("Cancel").clicked() {
                            if let Some(t) = &self.task {
                                t.cancel.store(true, Ordering::Relaxed);
                            }
                        }
                    });
                    if is_current {
                        if let Some(t) = &self.task {
                            if t.progress.1 > 0 {
                                let fraction =
                                    (t.progress.0 as f32 / t.progress.1 as f32).clamp(0.0, 1.0);
                                ui.add(egui::ProgressBar::new(fraction).show_percentage());
                            } else {
                                ui.spinner();
                            }
                        }
                    }
                });
        }
        if let Some(id) = self.confirm_remove.clone() {
            ui.horizontal(|ui| {
                ui.colored_label(theme::error(), format!("Remove only the app-managed downloaded copy of '{id}'? Bundled assets are protected."));
                if ui.button("Confirm remove").clicked() {
                    self.confirm_remove = None;
                    self.start(&id,"remove");
                }
                if ui.button("Keep").clicked() { self.confirm_remove = None; }
            });
        }
        ui.add_space(10.0);
        ui.label(RichText::new("Configuration · local runtimes and optional API").strong());
        Self::backend(
            ui,
            "Speech",
            "ai_speech",
            &mut self.config.speech_backend,
            false,
        );
        ui.horizontal_wrapped(|ui| {
            ui.label("Managed speech model");
            egui::ComboBox::from_id_salt("ai_speech_model")
                .selected_text(match self.config.speech_model.as_str() {
                    "tiny" => "Tiny (fast, default)",
                    "base" => "Base (better accuracy)",
                    "small" => "Small (more accurate, slower)",
                    _ => "Unknown (select again)",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut self.config.speech_model,
                        "tiny".into(),
                        "Tiny (default)",
                    );
                    ui.selectable_value(
                        &mut self.config.speech_model,
                        "base".into(),
                        "Base (optional download)",
                    );
                    ui.selectable_value(
                        &mut self.config.speech_model,
                        "small".into(),
                        "Small (optional download)",
                    );
                });
        });
        ui.label(
            RichText::new(
                "Install an optional speech model using its Install / repair button above before analyzing.",
            )
            .size(11.0)
            .color(theme::muted()),
        );
        ui.horizontal_wrapped(|ui| {
            ui.label("Speech language");
            egui::ComboBox::from_id_salt("ai_speech_language")
                .selected_text(match self.config.speech_language.as_str() {
                    "auto" => "Auto-detect",
                    "en" => "English",
                    "fa" => "Persian",
                    "ar" => "Arabic",
                    "de" => "German",
                    "es" => "Spanish",
                    "fr" => "French",
                    "tr" => "Turkish",
                    "ru" => "Russian",
                    "it" => "Italian",
                    "pt" => "Portuguese",
                    "hi" => "Hindi",
                    "ja" => "Japanese",
                    "ko" => "Korean",
                    "zh" => "Chinese",
                    _ => "Unknown (select again)",
                })
                .show_ui(ui, |ui| {
                    for (code, label) in [
                        ("auto", "Auto-detect"),
                        ("en", "English"),
                        ("fa", "Persian"),
                        ("ar", "Arabic"),
                        ("de", "German"),
                        ("es", "Spanish"),
                        ("fr", "French"),
                        ("tr", "Turkish"),
                        ("ru", "Russian"),
                        ("it", "Italian"),
                        ("pt", "Portuguese"),
                        ("hi", "Hindi"),
                        ("ja", "Japanese"),
                        ("ko", "Korean"),
                        ("zh", "Chinese"),
                    ] {
                        ui.selectable_value(
                            &mut self.config.speech_language,
                            code.into(),
                            label,
                        );
                    }
                });
        });
        Self::backend(ui, "OCR", "ai_ocr", &mut self.config.ocr_backend, false);
        Self::backend(
            ui,
            "Vision",
            "ai_vision",
            &mut self.config.vision_backend,
            false,
        );
        let prior = self.config.classification_backend.clone();
        Self::backend(
            ui,
            "Categorization",
            "ai_classify",
            &mut self.config.classification_backend,
            true,
        );
        if prior != self.config.classification_backend {
            let _ = self.config.save(&self.home);
            if self.config.classification_backend == "laya_local" && self.config.auto_download {
                self.queue.push_back(LAYA_ID.into());
                self.start_next();
            }
        }
        if self.config.classification_backend == "laya_local" {
            ui.label(RichText::new("Laya is for text/caption decisions only. It loads ~2.4 GB RAM for inference; the first request may take longer.")
                .size(11.0).color(theme::muted()));
        }
        ui.collapsing("Local model override paths and API", |ui| {
            ui.label(RichText::new("For speech, place a Whisper .bin or .gguf model under this app's models folder. For OCR, choose a directory under models containing language .traineddata files. Select Existing local model and save the configuration.")
                .size(11.0).color(theme::muted()));
            for (label, value) in [
                ("Speech model", &mut self.config.speech_custom_path),
                ("OCR model", &mut self.config.ocr_custom_path),
                ("Vision model", &mut self.config.vision_custom_path),
                (
                    "Classifier / Laya",
                    &mut self.config.classification_custom_path,
                ),
                ("API endpoint", &mut self.config.api_endpoint),
                ("API model", &mut self.config.api_model),
                ("API key environment variable", &mut self.config.api_key_env),
            ] {
                ui.horizontal(|ui| {
                    ui.label(label);
                    ui.add(egui::TextEdit::singleline(value).desired_width(340.0));
                });
            }
            ui.label(
                RichText::new(
                    "Never paste API secrets into this form. Remote media upload is disabled.",
                )
                .size(11.0)
                .color(theme::muted()),
            );
        });
        if ui.button("Save AI configuration").clicked() {
            self.notice = self
                .config
                .save(&self.home)
                .map(|_| "Saved under app/models/config.local.json".into())
                .unwrap_or_else(|e| e);
        }
        ui.label(RichText::new("Reel speech and screen-text analysis use the selected local configuration. Review results and category suggestions before approval; unsaving remains a separate action.")
            .size(11.0).color(theme::muted()));
        ui.label(
            RichText::new(&self.notice)
                .size(11.0)
                .color(theme::lavender()),
        );
    }
}

/// Headless native model operation for the Rune frontend. Reuses the same
/// catalog validation, checksum verifier, budget enforcement, resume support,
/// and project-contained storage used by the egui frontend.
pub fn command_model(id: &str, operation: &str) -> Result<String, String> {
    if !matches!(operation, "install" | "verify") {
        return Err("Only install and verify are exposed to the Rune frontend".into());
    }
    let home = app_home();
    let config = AiConfig::load(&home);
    let models = validate_catalog(
        serde_json::from_str::<Catalog>(include_str!("../models/catalog.json"))
            .map(|c| c.models)
            .unwrap_or_default(),
    );
    let model = models.iter().find(|m| m.id == id)
        .ok_or_else(|| format!("Unknown or invalid model ID: {id}"))?;
    if operation == "verify" {
        if verified(&bundled(&home, model), model)? || verified(&installed(&home, model), model)? {
            return Ok(format!("Checksum verified: {id}"));
        }
        return Err(format!("Model not installed or checksum mismatch: {id}"));
    }
    let cancel = AtomicBool::new(false);
    let (tx, _rx) = mpsc::channel();
    download(&home, model, &cancel, &tx, config.storage_limit_mb)
}
