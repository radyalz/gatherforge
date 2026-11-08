//! Download operations run on worker threads. The graphical interface never invokes a shell.
use crate::util::{Result, err};
use std::{
    env, fs,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::Sender,
    },
    thread,
    time::Duration,
};

#[derive(Clone)]
pub struct DownloadSettings {
    pub urls: String,
    pub convert_mp3: bool,
    pub browser: String,
    pub single: bool,
}

pub enum DownloadEvent {
    Line(String),
    Complete(Result<()>),
}

pub fn executable() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(exe) = env::current_exe() {
        if let Some(dir) = exe.parent() {
            for name in [
                "yt-dlp",
                "yt-dlp.exe",
                "yt-dlp_linux",
                "yt-dlp_linux_aarch64",
                "yt-dlp_macos",
            ] {
                candidates.push(dir.join(name));
            }
        }
    }
    let paths = env::var_os("PATH").unwrap_or_default();
    for dir in env::split_paths(&paths) {
        candidates.push(dir.join("yt-dlp"));
        candidates.push(dir.join("yt-dlp.exe"));
    }
    candidates.into_iter().find(|p| p.is_file())
}

fn urls(input: &str) -> Result<Vec<String>> {
    let mut result = Vec::new();
    for s in input
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty() && !s.starts_with('#'))
    {
        if result.len() >= 100 {
            return Err("Maximum 100 links per batch".into());
        }
        let parsed = url::Url::parse(s).map_err(err)?;
        if parsed.scheme() != "https" {
            return Err(format!("HTTPS link required: {s}"));
        }
        let host = parsed
            .host_str()
            .ok_or("Missing host")?
            .to_ascii_lowercase();
        if host == "localhost"
            || host.ends_with(".localhost")
            || host.ends_with(".local")
            || host == "127.0.0.1"
        {
            return Err("Local addresses are not valid download sources".into());
        }
        if !result.iter().any(|x| x == s) {
            result.push(s.to_owned());
        }
    }
    if result.is_empty() {
        return Err("Paste at least one URL".into());
    }
    Ok(result)
}

fn build_command(root: &Path, settings: &DownloadSettings) -> Result<Command> {
    let tool=executable().ok_or("yt-dlp is not installed. Place the official standalone yt-dlp executable beside GatherForge, or install yt-dlp on PATH.")?;
    if settings.convert_mp3 && !has_ffmpeg() {
        return Err("MP3 conversion requires FFmpeg. Use original quality instead.".into());
    }
    let links = urls(&settings.urls)?;
    if !["none", "firefox", "chrome", "chromium", "edge", "brave"]
        .contains(&settings.browser.as_str())
    {
        return Err("Unsupported browser session".into());
    }
    let archive = root.join("xxx").join("_downloads").join("archive.txt");
    fs::create_dir_all(archive.parent().unwrap()).map_err(err)?;
    fs::create_dir_all(root.join("_Incoming")).map_err(err)?;
    let mut cmd = Command::new(tool);
    cmd.arg("--ignore-config")
        .arg("--newline")
        .arg("--continue")
        .arg("--no-overwrites")
        .arg("--ignore-errors")
        .arg("--download-archive")
        .arg(archive)
        .arg("-P")
        .arg(root.join("_Incoming"))
        .arg("-o")
        .arg("%(artist,uploader)s - %(title)s [%(id)s].%(ext)s")
        .arg("--extractor-args")
        .arg("soundcloud:formats=*")
        .arg(if settings.single {
            "--no-playlist"
        } else {
            "--yes-playlist"
        })
        .arg("-f")
        .arg("bestaudio/best");
    if settings.convert_mp3 {
        cmd.args(["-x", "--audio-format", "mp3", "--audio-quality", "0"]);
    }
    if settings.browser != "none" {
        cmd.arg("--cookies-from-browser").arg(&settings.browser);
    }
    cmd.arg("--");
    for link in links {
        cmd.arg(link);
    }
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    Ok(cmd)
}

fn has_ffmpeg() -> bool {
    let path = env::var_os("PATH").unwrap_or_default();
    env::split_paths(&path).any(|dir| {
        dir.join(if cfg!(windows) {
            "ffmpeg.exe"
        } else {
            "ffmpeg"
        })
        .is_file()
    })
}

pub fn run(
    root: &Path,
    settings: DownloadSettings,
    tx: Sender<DownloadEvent>,
    cancel: Arc<AtomicBool>,
) {
    let result = (|| -> Result<()> {
        // Do not use an existing unrelated xxx folder; checked independently of library staging.
        let work = root.join("xxx");
        if work.exists()
            && !work.join(".gatherforge-owned").exists()
            && !work.join(".music-organizer-owned").exists()
        {
            return Err("The xxx folder exists but was not created by GatherForge".into());
        }
        fs::create_dir_all(&work).map_err(err)?;
        if !work.join(".gatherforge-owned").exists() {
            fs::write(
                work.join(".gatherforge-owned"),
                "GatherForge v3 workspace\n",
            )
            .map_err(err)?;
        }
        let mut child = build_command(root, &settings)?.spawn().map_err(err)?;
        let stdout = child.stdout.take().ok_or("Missing stdout")?;
        let stderr = child.stderr.take().ok_or("Missing stderr")?;
        let mut handles = vec![];
        for stream in [
            Box::new(stdout) as Box<dyn std::io::Read + Send>,
            Box::new(stderr) as Box<dyn std::io::Read + Send>,
        ] {
            let sender = tx.clone();
            handles.push(thread::spawn(move || {
                for line in BufReader::new(stream)
                    .lines()
                    .map_while(std::result::Result::ok)
                {
                    let _ = sender.send(DownloadEvent::Line(line));
                }
            }));
        }
        let status = loop {
            if cancel.load(Ordering::Relaxed) {
                let _ = child.kill();
                let _ = child.wait();
                return Err("Download cancelled".into());
            }
            if let Some(status) = child.try_wait().map_err(err)? {
                break status;
            }
            thread::sleep(Duration::from_millis(120));
        };
        for h in handles {
            let _ = h.join();
        }
        if status.success() {
            Ok(())
        } else {
            Err(format!("yt-dlp finished with status: {status}"))
        }
    })();
    let _ = tx.send(DownloadEvent::Complete(result));
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_non_https() {
        assert!(urls("http://example.com/video").is_err());
    }
    #[test]
    fn deduplicates_links() {
        assert_eq!(
            urls("https://example.com/a\nhttps://example.com/a")
                .unwrap()
                .len(),
            1
        );
    }
}
