use crate::downloader::{self, DownloadEvent};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::Sender,
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use walkdir::WalkDir;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Reel {
    pub id: String,
    pub creator: String,
    pub caption: String,
    pub url: String,
    pub date: String,
    pub duration: Option<f64>,
    pub views: Option<u64>,
    pub likes: Option<u64>,
    pub comments: Option<u64>,
    pub video: String,
    pub collection: String,
    pub instagram_unsaved: bool,
}

pub fn refresh(root: &Path) -> Result<Vec<Reel>, String> {
    if !root.is_dir() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in WalkDir::new(root)
        .max_depth(5)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy();
        let manifest_item = name == "manifest.json"
            && entry.path() != root.join("manifest.json")
            && entry
                .path()
                .strip_prefix(root)
                .ok()
                .map(|r| r.components().count() >= 4)
                .unwrap_or(false);
        let legacy = name.ends_with(".info.json");
        if !manifest_item && !legacy {
            continue;
        }
        if entry
            .metadata()
            .map(|m| m.len() > 8_000_000)
            .unwrap_or(true)
        {
            continue;
        }
        let Ok(raw) = fs::read(entry.path()) else {
            continue;
        };
        let Ok(v) = serde_json::from_slice::<Value>(&raw) else {
            continue;
        };
        if manifest_item {
            if v.get("downloaded").and_then(Value::as_bool) != Some(true) {
                continue;
            }
            let file = v.get("video_file").and_then(Value::as_str).unwrap_or("");
            if file.is_empty()
                || file.contains('/')
                || file.contains('\\')
                || file == "."
                || file == ".."
            {
                continue;
            }
            let media = entry.path().parent().unwrap_or(root).join(file);
            if media.is_symlink()
                || !media.is_file()
                || media.metadata().map(|m| m.len() == 0).unwrap_or(true)
            {
                continue;
            }
            let mut reel = serde_json::from_value::<Reel>(v.clone()).unwrap_or_default();
            if reel.id.is_empty() {
                reel.id = v
                    .get("shortcode")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned();
            }
            if reel.url.is_empty() {
                reel.url = v
                    .get("source_url")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned();
            }
            reel.video = media
                .strip_prefix(root)
                .unwrap_or(&media)
                .to_string_lossy()
                .to_string();
            out.push(reel);
            continue;
        }
        let s = |key: &str| {
            v.get(key)
                .and_then(Value::as_str)
                .unwrap_or("")
                .chars()
                .take(2000)
                .collect::<String>()
        };
        let Some(stem) = name.strip_suffix(".info.json") else {
            continue;
        };
        let video = ["mp4", "mkv", "webm", "mov"]
            .into_iter()
            .map(|e| entry.path().with_file_name(format!("{stem}.{e}")))
            .find(|p| p.is_file() && !p.is_symlink());
        let relative = video
            .and_then(|p| {
                p.strip_prefix(root)
                    .ok()
                    .map(|s| s.to_string_lossy().to_string())
            })
            .unwrap_or_default();
        if relative.is_empty() {
            continue;
        }
        out.push(Reel {
            id: s("id"),
            creator: s("uploader_id"),
            caption: s("description"),
            url: s("webpage_url"),
            date: s("upload_date"),
            duration: v.get("duration").and_then(Value::as_f64),
            views: v.get("view_count").and_then(Value::as_u64),
            likes: v.get("like_count").and_then(Value::as_u64),
            comments: v.get("comment_count").and_then(Value::as_u64),
            video: relative,
            collection: String::new(),
            instagram_unsaved: false,
        });
    }
    // Direct Reel manifests supersede legacy info.json entries; avoid duplicate cards.
    let direct_ids: std::collections::BTreeSet<String> = out
        .iter()
        .filter(|r| r.collection == "Direct")
        .map(|r| r.id.clone())
        .collect();
    out.retain(|r| !r.collection.is_empty() || !direct_ids.contains(&r.id));
    // Prefer per-Reel manifests when both legacy metadata and a collection manifest exist.
    out.sort_by(|a, b| {
        a.collection
            .is_empty()
            .cmp(&b.collection.is_empty())
            .then_with(|| b.date.cmp(&a.date))
            .then_with(|| a.id.cmp(&b.id))
    });
    let mut seen = std::collections::BTreeSet::new();
    out.retain(|r| {
        !r.id.is_empty() && seen.insert((r.id.clone(), r.creator.clone(), r.collection.clone()))
    });
    let manifest = serde_json::json!({"version":2,"generated_unix":SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),"reels":out});
    let bytes = serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?;
    let temp = root.join(".manifest.new");
    if temp.exists() {
        return Err("Temporary manifest file exists; refusing overwrite".into());
    }
    fs::write(&temp, bytes).map_err(|e| e.to_string())?;
    if let Err(e) = fs::rename(&temp, root.join("manifest.json")) {
        let _ = fs::remove_file(&temp);
        return Err(e.to_string());
    }
    Ok(out)
}

pub fn run(
    root: PathBuf,
    urls: String,
    browser: String,
    tx: Sender<DownloadEvent>,
    cancel: Arc<AtomicBool>,
) {
    let result = (|| -> Result<(), String> {
        let mut links = Vec::new();
        for line in urls.lines().map(str::trim).filter(|s| !s.is_empty()) {
            let url = url::Url::parse(line).map_err(|e| e.to_string())?;
            let host = url.host_str().unwrap_or("");
            let parts: Vec<_> = url
                .path_segments()
                .ok_or("Invalid URL path")?
                .filter(|s| !s.is_empty())
                .collect();
            if url.scheme() != "https"
                || !["instagram.com", "www.instagram.com", "m.instagram.com"].contains(&host)
                || parts.len() != 2
                || !["reel", "reels", "p"].contains(&parts[0])
            {
                return Err(format!("Not a direct Instagram Reel: {line}"));
            }
            if !links.contains(&line.to_string()) {
                links.push(line.to_string());
            }
            if links.len() > 100 {
                return Err("Maximum 100 links per batch".into());
            }
        }
        if links.is_empty() {
            return Err("Paste an Instagram Reel URL".into());
        }
        let yt = downloader::executable().ok_or("yt-dlp not found")?;
        fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        let records = root.join(".gatherforge");
        fs::create_dir_all(&records).map_err(|e| e.to_string())?;
        let mut cmd = Command::new(yt);
        cmd.arg("--ignore-config").arg("--newline").arg("--continue").arg("--ignore-errors").arg("--no-overwrites")
    .arg("--progress-template").arg("download:GF_PROGRESS:%(progress._percent_str)s|%(progress._speed_str)s|%(progress._eta_str)s")
    .arg("--write-info-json").arg("--download-archive").arg(records.join("reels-downloaded.txt"))
    .arg("--no-playlist").arg("-P").arg(&root).arg("-o").arg("Collections/Direct/%(id)s/video.%(ext)s")
    .arg("-f").arg("bv*+ba/b");
        if browser != "none" {
            if !["firefox", "chrome", "chromium", "brave", "edge"].contains(&browser.as_str()) {
                return Err("Unsupported browser".into());
            }
            cmd.arg("--cookies-from-browser").arg(browser);
        }
        cmd.arg("--")
            .args(&links)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = cmd.spawn().map_err(|e| e.to_string())?;
        let stdout = child.stdout.take().ok_or("No yt-dlp stdout")?;
        let stderr = child.stderr.take().ok_or("No yt-dlp stderr")?;
        let mut readers = Vec::new();
        for stream in [
            Box::new(stdout) as Box<dyn std::io::Read + Send>,
            Box::new(stderr) as Box<dyn std::io::Read + Send>,
        ] {
            let sender = tx.clone();
            readers.push(thread::spawn(move || {
                use std::io::{BufRead, BufReader};
                for line in BufReader::new(stream).lines().map_while(Result::ok) {
                    let _ = sender.send(DownloadEvent::Line(line));
                }
            }));
        }
        let status = loop {
            if cancel.load(Ordering::Relaxed) {
                let _ = child.kill();
                let _ = child.wait();
                break Err("Stopped".to_owned());
            }
            if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
                break if status.success() {
                    Ok(())
                } else {
                    Err(format!("yt-dlp returned {status}"))
                };
            }
            thread::sleep(Duration::from_millis(150));
        };
        for reader in readers {
            let _ = reader.join();
        }
        // Each finished direct Reel receives a verified per-video manifest before indexing.
        if status.is_ok() {
            if let Ok(exe) = std::env::current_exe() {
                if let Some(home) = exe.parent() {
                    let helper = home.join("tools/reel_analysis.py");
                    if helper.is_file() {
                        for link in &links {
                            let Some(id) = url::Url::parse(link).ok().and_then(|u| {
                                u.path_segments()
                                    .and_then(|mut p| p.nth(1).map(str::to_owned))
                            }) else {
                                continue;
                            };
                            let folder = root.join("Collections/Direct").join(&id);
                            if let Ok(entries) = fs::read_dir(folder) {
                                for entry in entries.flatten() {
                                    let path = entry.path();
                                    if path.is_file()
                                        && ["mp4", "webm", "mkv", "mov"].iter().any(|e| {
                                            path.extension().and_then(|s| s.to_str()) == Some(e)
                                        })
                                    {
                                        if let Ok(rel) = path.strip_prefix(&root) {
                                            let out = Command::new("python3")
                                                .arg(&helper)
                                                .arg("normalize")
                                                .arg("--root")
                                                .arg(&root)
                                                .arg("--video")
                                                .arg(rel)
                                                .output();
                                            if let Ok(out) = out {
                                                if !out.status.success() {
                                                    let _=tx.send(DownloadEvent::Line(format!("Reel manifest normalization needs review: {}",String::from_utf8_lossy(&out.stderr))));
                                                }
                                            }
                                        }
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        match refresh(&root) {
            Ok(reels) => {
                let _ = tx.send(DownloadEvent::Line(format!(
                    "Manifest updated: {} Reels",
                    reels.len()
                )));
            }
            Err(e) => {
                let _ = tx.send(DownloadEvent::Line(format!("Manifest error: {e}")));
            }
        }
        status
    })();
    let _ = tx.send(DownloadEvent::Complete(result));
}

/// Launch a user-driven Saved Collections browser. Passwords are entered in the browser,
/// not in GatherForge. Only this helper receives temporary Instagram cookies.
pub fn browse_saved(
    root: PathBuf,
    auto_unsave: bool,
    tx: Sender<DownloadEvent>,
    cancel: Arc<AtomicBool>,
) {
    let result = (|| -> Result<(), String> {
        let home = dirs::home_dir().ok_or("Home folder unavailable")?;
        let script = home.join(".local/share/gatherforge/instagram_saved.py");
        let python = home.join(".local/share/gatherforge/venv/bin/python3");
        if !script.is_file() || !python.is_file() {
            return Err("Saved Collections browser not installed; run v3.6 install.sh".into());
        }
        fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        let mut cmd = Command::new(python);
        cmd.arg(script)
            .arg("--root")
            .arg(&root)
            .arg("--max-items")
            .arg("1000");
        if auto_unsave {
            cmd.arg("--auto-unsave");
        }
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = cmd.spawn().map_err(|e| e.to_string())?;
        let stdout = child.stdout.take().ok_or("Missing browser stdout")?;
        let stderr = child.stderr.take().ok_or("Missing browser stderr")?;
        let output = tx.clone();
        let out_thread = thread::spawn(move || {
            use std::io::{BufRead, BufReader};
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                let _ = output.send(DownloadEvent::Line(line));
            }
        });
        let errors = tx.clone();
        let err_thread = thread::spawn(move || {
            use std::io::{BufRead, BufReader};
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                let _ = errors.send(DownloadEvent::Line(line));
            }
        });
        let status = loop {
            if cancel.load(Ordering::Relaxed) {
                // Let Python terminate the current yt-dlp child and delete temporary cookies.
                let _ = Command::new("kill")
                    .arg("-TERM")
                    .arg(child.id().to_string())
                    .status();
                let mut exited = false;
                for _ in 0..40 {
                    if child.try_wait().ok().flatten().is_some() {
                        exited = true;
                        break;
                    }
                    thread::sleep(Duration::from_millis(100));
                }
                if !exited {
                    let _ = child.kill();
                    let _ = child.wait();
                }
                break Err("Saved browser stopped".to_owned());
            }
            if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
                break if status.success() {
                    Ok(())
                } else {
                    Err(format!("Saved Collections browser exited: {status}"))
                };
            }
            thread::sleep(Duration::from_millis(160));
        };
        let _ = out_thread.join();
        let _ = err_thread.join();
        match refresh(&root) {
            Ok(reels) => {
                let _ = tx.send(DownloadEvent::Line(format!(
                    "Reel manifest indexed: {} items",
                    reels.len()
                )));
            }
            Err(e) => {
                let _ = tx.send(DownloadEvent::Line(format!(
                    "Manifest refresh warning: {e}"
                )));
            }
        }
        status
    })();
    let _ = tx.send(DownloadEvent::Complete(result));
}

/// AI organization lives in a separate sidecar; source videos and yt-dlp metadata are untouched.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct ReelAnnotation {
    pub collection: String,
    pub tags: Vec<String>,
    pub notes: String,
}

pub fn read_annotations(root: &Path) -> std::collections::BTreeMap<String, ReelAnnotation> {
    let path = root.join(".gatherforge").join("reel-annotations.json");
    if path.is_symlink() {
        return Default::default();
    }
    let Ok(raw) = fs::read(path) else {
        return Default::default();
    };
    if raw.len() > 4_000_000 {
        return Default::default();
    }
    let Ok(value) = serde_json::from_slice::<Value>(&raw) else {
        return Default::default();
    };
    value
        .get("items")
        .cloned()
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default()
}

// Metadata is moved aside, never destroyed; video and analysis files stay untouched.
// The suffix deliberately does not match refresh's manifest or info.json patterns.
const HIDDEN_SUFFIX: &str = ".gatherforge-hidden";

fn checked_reel_video(root: &Path, relative: &str) -> Result<PathBuf, String> {
    use std::path::Component;
    let rel = Path::new(relative);
    if rel.as_os_str().is_empty() || rel.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err("Invalid video path".into());
    }
    let base = root.canonicalize().map_err(|e| e.to_string())?;
    let mut p = base.clone();
    for c in rel.components() {
        p.push(c.as_os_str());
        if fs::symlink_metadata(&p).map_err(|e| e.to_string())?.file_type().is_symlink() {
            return Err("Symbolic links are not allowed".into());
        }
    }
    if !p.is_file() || !p.canonicalize().map_err(|e| e.to_string())?.starts_with(base) {
        return Err("Video is missing or outside the Reel folder".into());
    }
    Ok(p)
}

fn metadata_for_reel(root: &Path, reel: &Reel) -> Result<Vec<PathBuf>, String> {
    let video = checked_reel_video(root, &reel.video)?;
    let folder = video.parent().ok_or("Missing video parent")?;
    let mut result = Vec::new();
    let manifest = folder.join("manifest.json");
    let legacy = video.with_extension("info.json");
    for (path, direct) in [(manifest, true), (legacy, false)] {
        if direct && folder == root.canonicalize().map_err(|e| e.to_string())? { continue; }
        if !path.exists() { continue; }
        let meta = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
        if meta.file_type().is_symlink() || !meta.is_file() || meta.len() > 8_000_000 { return Err("Unsafe metadata file".into()); }
        let doc: Value = serde_json::from_slice(&fs::read(&path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        let id = doc.get("id").or_else(|| doc.get("shortcode")).and_then(Value::as_str).unwrap_or("");
        if id != reel.id { return Err(format!("Metadata does not match Reel {}", reel.id)); }
        if direct && doc.get("video_file").and_then(Value::as_str) != video.file_name().and_then(|x| x.to_str()) {
            return Err("Manifest points to a different video".into());
        }
        result.push(path);
    }
    if result.is_empty() { return Err("No matching manifest or metadata found".into()); }
    Ok(result)
}

fn hidden_path(path: &Path) -> Result<PathBuf, String> {
    let name = path.file_name().and_then(|x| x.to_str()).ok_or("Invalid metadata name")?;
    Ok(path.with_file_name(format!("{name}{HIDDEN_SUFFIX}")))
}

/// Move metadata aside with a reversible suffix. Media, approval and analysis remain in place.
pub fn hide_reel_metadata(root: &Path, reel: &Reel) -> Result<(), String> {
    let paths = metadata_for_reel(root, reel)?;
    let pairs: Vec<_> = paths.iter().map(|p| Ok((p.clone(), hidden_path(p)?))).collect::<Result<_, String>>()?;
    if pairs.iter().any(|(_, d)| fs::symlink_metadata(d).is_ok()) { return Err("A removed-metadata copy already exists; restore it first".into()); }
    let mut done: Vec<(PathBuf, PathBuf)> = Vec::new();
    for (src, dst) in pairs {
        if let Err(e) = fs::hard_link(&src, &dst) {
            for (a, b) in done.iter().rev() { if fs::hard_link(b, a).is_ok() { let _ = fs::remove_file(b); } }
            return Err(e.to_string());
        }
        if let Err(e) = fs::remove_file(&src) {
            let _ = fs::remove_file(&dst);
            for (a, b) in done.iter().rev() { if fs::hard_link(b, a).is_ok() { let _ = fs::remove_file(b); } }
            return Err(e.to_string());
        }
        done.push((src, dst));
    }
    Ok(())
}

/// Restore hidden metadata; never overwrite an existing manifest or follow symlinks.
pub fn restore_hidden_metadata(root: &Path) -> Result<usize, String> {
    let base = root.canonicalize().map_err(|e| e.to_string())?;
    let mut pairs = Vec::new();
    for entry in WalkDir::new(&base).max_depth(5).follow_links(false).into_iter() {
        let entry = entry.map_err(|e| e.to_string())?;
        if !entry.file_type().is_file() { continue; }
        let name = entry.file_name().to_string_lossy();
        let Some(original) = name.strip_suffix(HIDDEN_SUFFIX) else { continue; };
        if original != "manifest.json" && !original.ends_with(".info.json") { continue; }
        let src = entry.path().to_path_buf();
        let folder = src.parent().ok_or("Missing metadata parent")?;
        if original == "manifest.json" && folder == base { continue; }
        let metadata = fs::symlink_metadata(&src).map_err(|e| e.to_string())?;
        if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 8_000_000 { continue; }
        let doc: Value = match serde_json::from_slice(&fs::read(&src).map_err(|e| e.to_string())?) { Ok(v) => v, Err(_) => continue };
        let id = doc.get("id").or_else(|| doc.get("shortcode")).and_then(Value::as_str).unwrap_or("");
        if id.is_empty() { continue; }
        let media = if original == "manifest.json" {
            let name = doc.get("video_file").and_then(Value::as_str).unwrap_or("");
            if doc.get("downloaded").and_then(Value::as_bool) != Some(true) || name.is_empty() || Path::new(name).components().count() != 1 || name == "." || name == ".." { continue; }
            folder.join(name)
        } else {
            let stem = original.strip_suffix(".info.json").unwrap_or("");
            let Some(video) = ["mp4", "mkv", "webm", "mov"].iter().map(|ext| folder.join(format!("{stem}.{ext}"))).find(|p| p.is_file()) else { continue; };
            video
        };
        let Ok(rel) = media.strip_prefix(&base) else { continue; };
        let Some(rel) = rel.to_str() else { continue; };
        if checked_reel_video(&base, rel).is_err() { continue; }
        let dst = src.with_file_name(original);
        if fs::symlink_metadata(&dst).is_ok() { return Err(format!("Cannot restore: {} already exists", dst.display())); }
        pairs.push((src, dst));
    }
    let count = pairs.len();
    let mut done: Vec<(PathBuf, PathBuf)> = Vec::new();
    for (src, dst) in pairs {
        if let Err(e) = fs::hard_link(&src, &dst) {
            for (a, b) in done.iter().rev() { if fs::hard_link(b, a).is_ok() { let _ = fs::remove_file(b); } }
            return Err(e.to_string());
        }
        if let Err(e) = fs::remove_file(&src) {
            let _ = fs::remove_file(&dst);
            for (a, b) in done.iter().rev() { if fs::hard_link(b, a).is_ok() { let _ = fs::remove_file(b); } }
            return Err(e.to_string());
        }
        done.push((src, dst));
    }
    Ok(count)
}

#[cfg(test)]
mod hidden_metadata_tests {
    use super::*;
    #[test]
    fn root_legacy_metadata_can_be_hidden_and_restored() {
        let base = std::env::temp_dir().join(format!("gf-root-test-{}-{}", std::process::id(), SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));
        fs::create_dir_all(&base).unwrap();
        fs::write(base.join("clip.mp4"), b"video").unwrap();
        fs::write(base.join("clip.info.json"), br#"{"id":"123"}"#).unwrap();
        fs::write(base.join("manifest.json"), b"aggregate").unwrap();
        let reel = Reel { id: "123".into(), video: "clip.mp4".into(), ..Default::default() };
        hide_reel_metadata(&base, &reel).unwrap();
        assert_eq!(fs::read(base.join("manifest.json")).unwrap(), b"aggregate");
        assert!(!base.join("clip.info.json").exists());
        assert_eq!(restore_hidden_metadata(&base).unwrap(), 1);
        assert!(base.join("clip.info.json").exists());
        fs::remove_dir_all(base).unwrap();
    }
    #[test]
    fn hide_and_restore_preserve_video_analysis_and_detect_conflict() {
        let base = std::env::temp_dir().join(format!("gf-reel-test-{}-{}", std::process::id(), SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));
        let folder = base.join("Collections/Direct/testid");
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("video.mp4"), b"video").unwrap();
        fs::write(folder.join("analysis.json"), b"analysis").unwrap();
        fs::write(folder.join("manifest.json"), br#"{"id":"testid","video_file":"video.mp4","downloaded":true}"#).unwrap();
        let reel = Reel { id: "testid".into(), video: "Collections/Direct/testid/video.mp4".into(), ..Default::default() };
        hide_reel_metadata(&base, &reel).unwrap();
        assert!(!folder.join("manifest.json").exists());
        assert_eq!(fs::read(folder.join("video.mp4")).unwrap(), b"video");
        assert_eq!(fs::read(folder.join("analysis.json")).unwrap(), b"analysis");
        fs::write(folder.join("manifest.json"), b"new").unwrap();
        assert!(restore_hidden_metadata(&base).is_err());
        assert_eq!(fs::read(folder.join("manifest.json")).unwrap(), b"new");
        fs::remove_file(folder.join("manifest.json")).unwrap();
        assert_eq!(restore_hidden_metadata(&base).unwrap(), 1);
        assert!(folder.join("manifest.json").exists());
        fs::remove_dir_all(base).unwrap();
    }
}
