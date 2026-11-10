use eframe::egui::{self, ColorImage, TextureHandle, TextureOptions};
use std::{io::Read, time::{Duration, Instant}, collections::{HashMap, HashSet}, path::{Path, PathBuf}, process::{Command, Stdio}, sync::mpsc::{self, Receiver, Sender}, thread};

pub struct Previews {
    tx: Sender<(PathBuf, Option<(usize, usize, Vec<u8>)>)>,
    rx: Receiver<(PathBuf, Option<(usize, usize, Vec<u8>)>)>,
    cache: HashMap<PathBuf, (Option<TextureHandle>, u64)>,
    clock: u64,
    pending: HashSet<PathBuf>,
}
impl Previews {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        Self { tx, rx, cache: HashMap::new(), clock: 0, pending: HashSet::new() }
    }
    pub fn texture(&mut self, ctx: &egui::Context, root: &Path, relative: &str) -> Option<TextureHandle> {
        while let Ok((path, result)) = self.rx.try_recv() {
            self.pending.remove(&path);
            let texture = result.map(|(w, h, rgb)| {
                ctx.load_texture(path.to_string_lossy(), ColorImage::from_rgb([w, h], &rgb), TextureOptions::LINEAR)
            });
            if self.cache.len() >= 128 && !self.cache.contains_key(&path) {
                if let Some(key) = self.cache.iter().min_by_key(|(_, (_, touched))| *touched).map(|(key, _)| key.clone()) { self.cache.remove(&key); }
            }
            self.clock = self.clock.wrapping_add(1);
            self.cache.insert(path, (texture, self.clock));
        }
        let base = root.canonicalize().ok()?;
        let p = root.join(relative).canonicalize().ok()?;
        if !p.starts_with(&base) || !p.is_file() { return None; }
        self.clock = self.clock.wrapping_add(1);
        if let Some((found, touched)) = self.cache.get_mut(&p) { *touched = self.clock; return found.clone(); }
        if !self.pending.contains(&p) && self.pending.len() < 2 {
            self.pending.insert(p.clone());
            let sender = self.tx.clone();
            let repaint = ctx.clone();
            thread::spawn(move || {
                let result = make_preview(&p);
                let _ = sender.send((p, result));
                repaint.request_repaint();
            });
        }
        None
    }
}
fn parse_ppm(bytes: &[u8]) -> Option<(usize, usize, Vec<u8>)> {
    let mut pos = 0usize;
    fn token<'a>(b: &'a [u8], p: &mut usize) -> Option<&'a [u8]> {
        while *p < b.len() && b[*p].is_ascii_whitespace() { *p += 1; }
        let start = *p;
        while *p < b.len() && !b[*p].is_ascii_whitespace() { *p += 1; }
        if *p == start || *p == b.len() { return None; }
        let out = &b[start..*p]; *p += 1; Some(out)
    }
    if token(bytes, &mut pos)? != b"P6" { return None; }
    let width: usize = std::str::from_utf8(token(bytes, &mut pos)?).ok()?.parse().ok()?;
    let height: usize = std::str::from_utf8(token(bytes, &mut pos)?).ok()?.parse().ok()?;
    if token(bytes, &mut pos)? != b"255" || width == 0 || height == 0 || width > 240 || height > 320 { return None; }
    let count = width.checked_mul(height)?.checked_mul(3)?;
    if bytes.len().checked_sub(pos)? != count { return None; }
    Some((width, height, bytes[pos..].to_vec()))
}

fn make_preview(path: &Path) -> Option<(usize, usize, Vec<u8>)> {
    let mut child = Command::new("ffmpeg").args(["-v", "error", "-nostdin", "-i"]).arg(path)
        .args(["-frames:v", "1", "-vf", "scale=240:320:force_original_aspect_ratio=decrease", "-f", "image2pipe", "-vcodec", "ppm", "pipe:1"])
        .stdout(Stdio::piped()).stderr(Stdio::null()).spawn().ok()?;
    let stdout = child.stdout.take()?;
    let reader = thread::spawn(move || {
        let mut data = Vec::new();
        let _ = stdout.take(240 * 320 * 3 + 1024).read_to_end(&mut data);
        data
    });
    let start = Instant::now();
    let success = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.success(),
            Ok(None) if start.elapsed() < Duration::from_secs(15) => thread::sleep(Duration::from_millis(50)),
            _ => { let _ = child.kill(); let _ = child.wait(); break false; }
        }
    };
    let data = reader.join().ok()?;
    if success { parse_ppm(&data) } else { None }
}

#[cfg(test)]
mod tests {
    use super::{parse_ppm, make_preview};
    use std::{fs, process::Command, time::{SystemTime, UNIX_EPOCH}};
    #[test]
    fn ffmpeg_produces_decodable_preview_when_installed() {
        if Command::new("ffmpeg").arg("-version").output().is_err() { return; }
        let path = std::env::temp_dir().join(format!("gf-preview-{}-{}.ppm", std::process::id(), SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));
        fs::write(&path, b"P6\n2 1\n255\n\xff\x00\x00\x00\xff\x00").unwrap();
        let result = make_preview(&path);
        fs::remove_file(&path).unwrap();
        let (w, h, pixels) = result.expect("ffmpeg could not create a thumbnail");
        assert!(w > 0 && h > 0 && w <= 240 && h <= 320);
        assert_eq!(pixels.len(), w * h * 3);
    }
    #[test] fn ppm_accepts_valid_and_rejects_bad_data() {
        assert_eq!(parse_ppm(b"P6\n1 1\n255\n\x01\x02\x03"), Some((1, 1, vec![1, 2, 3])));
        assert!(parse_ppm(b"P6\n241 1\n255\n").is_none());
        assert!(parse_ppm(b"P6\n1 1\n255\n\x01").is_none());
    }
}
