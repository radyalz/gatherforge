use std::{
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub type Result<T> = std::result::Result<T, String>;
pub fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
pub fn norm(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

pub fn safe_name(text: &str) -> String {
    let clean: String = text
        .trim()
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\"/\\|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .take(100)
        .collect();
    let clean = clean.trim_matches(|c: char| c == ' ' || c == '.');
    let clean = if clean.is_empty() {
        "Unknown Artist"
    } else {
        clean
    };
    if clean.eq_ignore_ascii_case("xxx")
        || ["CON", "PRN", "AUX", "NUL", "COM1", "LPT1", "COM2", "LPT2"]
            .contains(&clean.to_uppercase().as_str())
    {
        format!("_{clean}")
    } else {
        clean.to_owned()
    }
}

pub fn main_artist(raw: &str) -> String {
    let raw = raw.trim();
    if raw.is_empty() {
        return "Unknown Artist".into();
    }
    if [
        "earth, wind & fire",
        "simon & garfunkel",
        "tyler, the creator",
        "ac/dc",
        "hall & oates",
    ]
    .contains(&norm(raw).as_str())
    {
        return safe_name(raw);
    }
    let lower = raw.to_lowercase();
    let mut cut = raw.len();
    for delim in [
        " feat. ",
        " feat ",
        " featuring ",
        " ft. ",
        " ft ",
        " with ",
        " vs. ",
        " vs ",
        " versus ",
        " x ",
        " & ",
        " + ",
        " / ",
        " ; ",
        ", ",
        " (feat",
        " [feat",
        " (ft",
        " [ft",
    ] {
        if let Some(i) = lower.find(delim) {
            cut = cut.min(i);
        }
    }
    safe_name(raw.get(..cut).unwrap_or(raw))
}

pub fn audio_ext(path: &Path) -> bool {
    let ext = path
        .extension()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();
    matches!(
        ext.as_str(),
        "mp3"
            | "mp2"
            | "flac"
            | "wav"
            | "m4a"
            | "m4b"
            | "aac"
            | "ogg"
            | "oga"
            | "opus"
            | "wma"
            | "aif"
            | "aiff"
            | "ape"
            | "wv"
            | "alac"
            | "tta"
            | "dsf"
            | "dff"
            | "webm"
            | "mka"
    )
}

pub fn hash_file(path: &Path) -> Result<blake3::Hash> {
    let mut f = File::open(path).map_err(err)?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0u8; 1024 * 1024];
    loop {
        let n = f.read(&mut buffer).map_err(err)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(hasher.finalize())
}

pub fn copy_verified(source: &Path, dest: &Path) -> Result<()> {
    if dest.exists() {
        return Err(format!("Destination already exists: {}", dest.display()));
    }
    std::fs::create_dir_all(dest.parent().ok_or("Missing parent")?).map_err(err)?;
    let part = dest.with_extension(format!(
        "{}.gatherforge-part",
        dest.extension().unwrap_or_default().to_string_lossy()
    ));
    if part.exists() {
        return Err(format!("Existing partial copy: {}", part.display()));
    }
    let result = (|| -> Result<()> {
        let mut input = File::open(source).map_err(err)?;
        let mut output = File::create_new(&part).map_err(err)?;
        std::io::copy(&mut input, &mut output).map_err(err)?;
        output.flush().map_err(err)?;
        output.sync_all().map_err(err)?;
        if hash_file(source)? != hash_file(&part)? {
            return Err("Copy verification failed".into());
        }
        std::fs::rename(&part, dest).map_err(err)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    result
}

pub fn rel_to(root: &Path, path: &Path) -> Result<String> {
    let rel = path.strip_prefix(root).map_err(err)?;
    if rel
        .components()
        .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return Err("Unsafe relative path".into());
    }
    Ok(rel.to_string_lossy().replace('\\', "/"))
}

pub fn safe_rel(root: &Path, rel: &str) -> Result<PathBuf> {
    let p = Path::new(rel);
    if p.components()
        .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return Err(format!("Unsafe relative path: {rel}"));
    }
    Ok(root.join(p))
}

pub fn unique_path(parent: &Path, stem: &str, ext: &str) -> PathBuf {
    let basename = safe_name(stem);
    for n in 0..100_000 {
        let fname = if n == 0 {
            format!("{basename}{ext}")
        } else {
            format!("{basename} ({}){ext}", n + 1)
        };
        let target = parent.join(fname);
        if !target.exists() {
            return target;
        }
    }
    parent.join(format!("{basename}-overflow{ext}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn first_artist() {
        assert_eq!(main_artist("Drake feat. Future"), "Drake");
        assert_eq!(main_artist("Travis Scott vs Drake"), "Travis Scott");
        assert_eq!(main_artist("Earth, Wind & Fire"), "Earth, Wind & Fire");
    }
    #[test]
    fn name_is_safe() {
        assert_eq!(safe_name("../evil"), "_evil");
        assert_eq!(safe_name("xxx"), "_xxx");
    }
}
