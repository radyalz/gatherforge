use crate::{
    model::{Decision, Library, Phase, Track},
    util::{
        Result, audio_ext, copy_verified, err, hash_file, main_artist, norm, rel_to, safe_name,
        safe_rel, unique_path,
    },
};
use lofty::{
    prelude::{Accessor, TaggedFileExt},
    read_from_path,
};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};
use walkdir::WalkDir;

const MARKER: &str = ".gatherforge-owned";
const STATE: &str = "gatherforge-state.json";

fn workspace(root: &Path) -> PathBuf {
    root.join("xxx")
}
fn ensure_workspace(root: &Path) -> Result<PathBuf> {
    if !root.is_dir() || root.parent().is_none() {
        return Err("Choose a regular music folder, not a drive root".into());
    }
    let work = workspace(root);
    if work.exists()
        && !work.join(MARKER).is_file()
        && !work.join(".music-organizer-owned").is_file()
    {
        return Err(format!(
            "Refusing to use existing unowned folder: {}. Rename it or select another library.",
            work.display()
        ));
    }
    fs::create_dir_all(&work).map_err(err)?;
    if !work.join(MARKER).exists() {
        fs::write(work.join(MARKER), b"GatherForge v3 workspace\n").map_err(err)?;
    }
    Ok(work)
}

pub fn load(root: &Path) -> Result<Library> {
    let path = workspace(root).join(STATE);
    if !path.exists() {
        let legacy = workspace(root).join("state.json");
        if legacy.is_file() {
            if !workspace(root).join(".music-organizer-owned").is_file() {
                return Err("Legacy state found without its ownership marker".into());
            }
            return import_legacy(&legacy);
        }
        return Ok(Library::default());
    }
    if !workspace(root).join(MARKER).exists()
        && !workspace(root).join(".music-organizer-owned").exists()
    {
        return Err("Unowned xxx directory: refusing to load state".into());
    }
    let data = fs::read(path).map_err(err)?;
    serde_json::from_slice(&data).map_err(err)
}

fn import_legacy(path: &Path) -> Result<Library> {
    #[derive(serde::Deserialize)]
    struct OldState {
        phase: String,
        tracks: Vec<OldTrack>,
        #[serde(default)]
        aliases: std::collections::BTreeMap<String, String>,
    }
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct OldTrack {
        id: u64,
        original: String,
        stage: String,
        filename: String,
        artist: String,
        #[serde(default)]
        raw_artist: String,
        title: String,
        format: String,
        bytes: u64,
        action: String,
        #[serde(default)]
        duplicate_of: i64,
        #[serde(default)]
        published: String,
    }
    let old: OldState = serde_json::from_slice(&fs::read(path).map_err(err)?).map_err(err)?;
    let phase = match old.phase.as_str() {
        "review" => Phase::Review,
        "staging" => Phase::Staging,
        "staged" => Phase::Staged,
        "published" => Phase::Published,
        "finalizing" => Phase::Finalizing,
        "finalized" => Phase::Finalized,
        other => return Err(format!("Unknown legacy phase: {other}")),
    };
    let tracks = old
        .tracks
        .into_iter()
        .map(|t| Track {
            id: t.id,
            original: t.original,
            staged: t.stage,
            filename: t.filename,
            artist: t.artist,
            raw_artist: t.raw_artist,
            title: t.title,
            format: t.format,
            size: t.bytes,
            action: match t.action.as_str() {
                "keep" => Decision::Keep,
                "skip" => Decision::Skip,
                _ => Decision::Review,
            },
            duplicate_of: if t.duplicate_of <= 0 {
                None
            } else {
                Some(t.duplicate_of as u64)
            },
            duplicate_kind: if t.action == "exact" {
                Some("Exact bytes (legacy)".into())
            } else if t.action == "possible" {
                Some("Possible duplicate (legacy)".into())
            } else {
                None
            },
            published: if t.published.is_empty() {
                None
            } else {
                Some(t.published)
            },
        })
        .collect();
    Ok(Library {
        version: 3,
        phase,
        tracks,
        aliases: old.aliases,
    })
}

pub fn save(root: &Path, lib: &Library) -> Result<()> {
    let work = ensure_workspace(root)?;
    let dest = work.join(STATE);
    let temp = work.join("gatherforge-state.json.tmp");
    let bytes = serde_json::to_vec_pretty(lib).map_err(err)?;
    fs::write(&temp, bytes).map_err(err)?;
    // On Windows, rename does not replace an existing destination.
    if dest.exists() {
        let backup = work.join("gatherforge-state.json.bak");
        if backup.exists() {
            fs::remove_file(&backup).map_err(err)?;
        }
        fs::rename(&dest, &backup).map_err(err)?;
        if let Err(e) = fs::rename(&temp, &dest) {
            let _ = fs::rename(&backup, &dest);
            return Err(err(e));
        }
        let _ = fs::remove_file(&backup);
    } else {
        fs::rename(&temp, &dest).map_err(err)?;
    }
    Ok(())
}

fn read_music_tags(path: &Path, rel: &str) -> (String, String) {
    let stem = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let (fallback_artist, fallback_title) = match stem.split_once(" - ") {
        Some((a, t)) if !a.trim().is_empty() => (a.trim().to_string(), t.trim().to_string()),
        _ => {
            let parent = Path::new(rel)
                .parent()
                .and_then(|x| x.file_name())
                .and_then(|x| x.to_str())
                .unwrap_or("");
            let artist =
                if parent.is_empty() || matches!(parent, "_Incoming" | "Music" | "Downloads") {
                    "Unknown Artist"
                } else {
                    parent
                };
            (artist.to_owned(), stem)
        }
    };
    if let Ok(meta) = read_from_path(path) {
        if let Some(tag) = meta.primary_tag().or_else(|| meta.first_tag()) {
            let artist = tag
                .artist()
                .map(|s| s.to_string())
                .filter(|v| !v.trim().is_empty())
                .unwrap_or(fallback_artist);
            let title = tag
                .title()
                .map(|s| s.to_string())
                .filter(|v| !v.trim().is_empty())
                .unwrap_or(fallback_title);
            return (artist, title);
        }
    }
    (fallback_artist, fallback_title)
}

pub fn scan(root: &Path, old: &Library) -> Result<Library> {
    if !matches!(old.phase, Phase::Review | Phase::Finalized) {
        return Err(
            "Current batch is staged/published. Finish or restore it before a new scan.".into(),
        );
    }
    ensure_workspace(root)?;
    let mut lib = Library {
        version: 3,
        phase: Phase::Review,
        tracks: vec![],
        aliases: old.aliases.clone(),
    };
    // Never re-stage previously published songs when starting a new batch.
    let work = workspace(root);
    let history_path = work.join("published-history.json");
    let mut published: HashSet<String> = fs::read(&history_path)
        .ok()
        .and_then(|data| serde_json::from_slice(&data).ok())
        .unwrap_or_default();
    // Import historical paths from the previous Go implementation as well.
    if let Ok(data) = fs::read(work.join("history.json")) {
        if let Ok(rows) = serde_json::from_slice::<Vec<serde_json::Value>>(&data) {
            for row in rows {
                if let Some(path) = row.get("published").and_then(|v| v.as_str()) {
                    published.insert(path.replace('\\', "/"));
                }
            }
        }
    }
    if old.phase == Phase::Finalized {
        for t in &old.tracks {
            if let Some(path) = &t.published {
                published.insert(path.replace('\\', "/"));
            }
        }
    }
    if !published.is_empty() {
        fs::write(
            &history_path,
            serde_json::to_vec_pretty(&published).map_err(err)?,
        )
        .map_err(err)?;
    }
    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            let p = e.path();
            if p == root {
                return true;
            }
            let rel = p.strip_prefix(root).unwrap_or(p);
            !matches!(rel.components().next(),Some(std::path::Component::Normal(s))
            if matches!(s.to_str(),Some("xxx"|".git"|"target"|"_tools")))
        })
        .filter_map(std::result::Result::ok)
    {
        if !entry.file_type().is_file() || !audio_ext(entry.path()) {
            continue;
        }
        let rel = rel_to(root, entry.path())?;
        if published.contains(&rel) {
            continue;
        }
        let (raw_artist, title) = read_music_tags(entry.path(), &rel);
        let main = main_artist(&raw_artist);
        let artist = lib
            .aliases
            .get(&norm(&raw_artist))
            .or_else(|| lib.aliases.get(&norm(&main)))
            .cloned()
            .unwrap_or(main);
        let ext = format!(
            ".{}",
            entry
                .path()
                .extension()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase()
        );
        let name = entry.file_name().to_string_lossy().to_string();
        let id = lib.tracks.len() as u64 + 1;
        lib.tracks.push(Track {
            id,
            original: rel.clone(),
            staged: format!("_staging/{rel}"),
            filename: name,
            artist,
            raw_artist,
            title,
            format: ext,
            size: entry.metadata().map_err(err)?.len(),
            action: Decision::Keep,
            duplicate_of: None,
            duplicate_kind: None,
            published: None,
        });
    }
    // Case-insensitive artist merging prevents separate "Drake" / "drake" folders.
    let mut casing: HashMap<String, String> = HashMap::new();
    for track in &mut lib.tracks {
        let key = norm(&track.artist);
        if let Some(canonical) = casing.get(&key) {
            track.artist = canonical.clone();
        } else {
            casing.insert(key, track.artist.clone());
        }
    }
    // Only hash files with a matching size; no expensive full-collection hash pass.
    let mut sizes: HashMap<u64, Vec<usize>> = HashMap::new();
    for (i, t) in lib.tracks.iter().enumerate() {
        sizes.entry(t.size).or_default().push(i);
    }
    let mut hashed: HashMap<blake3::Hash, u64> = HashMap::new();
    for group in sizes.values().filter(|g| g.len() > 1) {
        for &i in group {
            let path = safe_rel(root, &lib.tracks[i].original)?;
            let digest = hash_file(&path)?;
            if let Some(&other) = hashed.get(&digest) {
                lib.tracks[i].duplicate_of = Some(other);
                lib.tracks[i].duplicate_kind = Some("Exact bytes".into());
                lib.tracks[i].action = Decision::Review;
            } else {
                hashed.insert(digest, lib.tracks[i].id);
            }
        }
    }
    let mut songs = HashMap::<String, u64>::new();
    for t in &mut lib.tracks {
        if t.duplicate_of.is_some() {
            continue;
        }
        let key = format!("{}|{}", norm(&t.artist), norm(&t.title));
        if let Some(&previous) = songs.get(&key) {
            t.duplicate_of = Some(previous);
            t.duplicate_kind = Some("Same artist + title".into());
            t.action = Decision::Review;
        } else {
            songs.insert(key, t.id);
        }
    }
    save(root, &lib)?;
    Ok(lib)
}

pub fn update_track(
    root: &Path,
    lib: &mut Library,
    id: u64,
    artist: Option<&str>,
    decision: Option<Decision>,
) -> Result<()> {
    if matches!(
        lib.phase,
        Phase::Published | Phase::Finalizing | Phase::Finalized
    ) {
        return Err("Cannot change decisions after publishing".into());
    }
    let track = lib
        .tracks
        .iter_mut()
        .find(|t| t.id == id)
        .ok_or("Track not found")?;
    if let Some(a) = artist {
        track.artist = safe_name(a);
    }
    if let Some(d) = decision {
        track.action = d;
    }
    save(root, lib)
}

pub fn rename_group(root: &Path, lib: &mut Library, previous: &str, next: &str) -> Result<()> {
    if matches!(
        lib.phase,
        Phase::Published | Phase::Finalizing | Phase::Finalized
    ) {
        return Err("Cannot rename after publishing".into());
    }
    let next = safe_name(next);
    lib.aliases.insert(norm(previous), next.clone());
    for t in &mut lib.tracks {
        if norm(&t.artist) == norm(previous) {
            t.artist = next.clone();
        }
    }
    save(root, lib)
}

pub fn stage(root: &Path, lib: &mut Library) -> Result<()> {
    if !matches!(lib.phase, Phase::Review | Phase::Staging) || lib.tracks.is_empty() {
        return Err("Scan tracks before staging".into());
    }
    let work = ensure_workspace(root)?;
    lib.phase = Phase::Staging;
    save(root, lib)?;
    for t in &lib.tracks {
        let from = safe_rel(root, &t.original)?;
        let to = safe_rel(&work, &t.staged)?;
        if to.exists() {
            if from.exists() {
                return Err(format!("Both copies exist: {}", t.original));
            }
            continue;
        }
        fs::create_dir_all(to.parent().ok_or("No staging parent")?).map_err(err)?;
        fs::rename(&from, &to).map_err(|e| format!("Cannot stage {}: {e}", t.original))?;
    }
    lib.phase = Phase::Staged;
    save(root, lib)
}

pub fn restore(root: &Path, lib: &mut Library) -> Result<()> {
    if !matches!(lib.phase, Phase::Staging | Phase::Staged) {
        return Err("Restore is allowed only before publishing".into());
    }
    let work = ensure_workspace(root)?;
    for t in &lib.tracks {
        let from = safe_rel(&work, &t.staged)?;
        let to = safe_rel(root, &t.original)?;
        if !from.exists() && to.exists() {
            continue;
        }
        if to.exists() {
            return Err(format!(
                "Cannot restore; destination occupied: {}",
                to.display()
            ));
        }
        if !from.exists() {
            return Err(format!("Staged file missing: {}", t.staged));
        }
        fs::create_dir_all(to.parent().ok_or("No destination")?).map_err(err)?;
        fs::rename(&from, &to).map_err(err)?;
    }
    lib.phase = Phase::Review;
    save(root, lib)
}

pub fn publish(root: &Path, lib: &mut Library) -> Result<()> {
    if !matches!(lib.phase, Phase::Staged | Phase::Published) {
        return Err("Stage the batch before publishing".into());
    }
    if lib.reviews() > 0 {
        return Err(format!(
            "Resolve {} duplicate-review decisions before publishing",
            lib.reviews()
        ));
    }
    let work = ensure_workspace(root)?;
    for i in 0..lib.tracks.len() {
        if lib.tracks[i].action != Decision::Keep {
            continue;
        }
        let from = safe_rel(&work, &lib.tracks[i].staged)?;
        if !from.exists() {
            return Err(format!("Staged file missing: {}", from.display()));
        }
        if let Some(rel) = &lib.tracks[i].published {
            let to = safe_rel(root, rel)?;
            if to.exists() && hash_file(&from)? == hash_file(&to)? {
                continue;
            }
            return Err(format!("Previously published copy was modified: {rel}"));
        }
        let dir = root.join(safe_name(&lib.tracks[i].artist));
        let stem = Path::new(&lib.tracks[i].filename)
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy();
        let target = unique_path(&dir, &stem, &lib.tracks[i].format);
        copy_verified(&from, &target)?;
        lib.tracks[i].published = Some(rel_to(root, &target)?);
        save(root, lib)?;
    }
    lib.phase = Phase::Published;
    save(root, lib)
}

pub fn finalize(root: &Path, lib: &mut Library) -> Result<()> {
    if !matches!(lib.phase, Phase::Published | Phase::Finalizing) {
        return Err("Publish the batch before finalizing".into());
    }
    let work = ensure_workspace(root)?;
    // Verify EVERY kept copy before deleting even one staged original.
    for t in lib.tracks.iter().filter(|t| t.action == Decision::Keep) {
        let published = t.published.as_ref().ok_or("Unpublished kept track")?;
        let target = safe_rel(root, published)?;
        if !target.exists() {
            return Err(format!("Published track missing: {published}"));
        }
        let source = safe_rel(&work, &t.staged)?;
        if source.exists() && hash_file(&source)? != hash_file(&target)? {
            return Err(format!("Verification failed: {published}"));
        }
        if !source.exists() && lib.phase != Phase::Finalizing {
            return Err(format!("Staged original missing: {}", t.staged));
        }
    }
    lib.phase = Phase::Finalizing;
    save(root, lib)?;
    for i in 0..lib.tracks.len() {
        let t = &lib.tracks[i];
        let from = safe_rel(&work, &t.staged)?;
        if !from.exists() {
            continue;
        }
        if t.action == Decision::Keep {
            fs::remove_file(&from).map_err(err)?; // only after full-file verification
        } else {
            let dir = work.join("_Not_Published").join(t.action.label());
            fs::create_dir_all(&dir).map_err(err)?;
            let stem = Path::new(&t.filename)
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy();
            let target = unique_path(&dir, &stem, &t.format);
            fs::rename(&from, &target).map_err(err)?;
            lib.tracks[i].staged = rel_to(&work, &target)?;
        }
        save(root, lib)?;
    }
    lib.phase = Phase::Finalized;
    save(root, lib)
}

pub fn reports(root: &Path, lib: &Library) -> Result<PathBuf> {
    let work = ensure_workspace(root)?;
    let dir = work.join("_reports");
    fs::create_dir_all(&dir).map_err(err)?;
    let mut workbook = rust_xlsxwriter::Workbook::new();
    {
        let sheet = workbook.add_worksheet();
        sheet.set_name("Tracks").map_err(err)?;
        let headings = [
            "ID",
            "First artist",
            "Title",
            "Decision",
            "Duplicate type",
            "Duplicate of",
            "Original location",
            "Staging location",
            "Published location",
            "Format",
            "Bytes",
        ];
        for (i, h) in headings.iter().enumerate() {
            sheet.write_string(0, i as u16, *h).map_err(err)?;
        }
        for (i, t) in lib.tracks.iter().enumerate() {
            let r = i as u32 + 1;
            for (c, v) in [
                t.id.to_string(),
                t.artist.clone(),
                t.title.clone(),
                t.action.label().into(),
                t.duplicate_kind.clone().unwrap_or_default(),
                t.duplicate_of.map(|n| n.to_string()).unwrap_or_default(),
                root.join(&t.original).display().to_string(),
                work.join(&t.staged).display().to_string(),
                t.published
                    .as_ref()
                    .map(|p| root.join(p).display().to_string())
                    .unwrap_or_default(),
                t.format.clone(),
                t.size.to_string(),
            ]
            .iter()
            .enumerate()
            {
                sheet.write_string(r, c as u16, v.as_str()).map_err(err)?;
            }
        }
        sheet.set_column_width(1, 25).map_err(err)?;
        sheet.set_column_width(2, 38).map_err(err)?;
        sheet.set_column_width(6, 58).map_err(err)?;
        sheet.set_column_width(7, 58).map_err(err)?;
    }
    {
        let sheet = workbook.add_worksheet();
        sheet.set_name("Artist summary").map_err(err)?;
        sheet.write_string(0, 0, "Artist folder").map_err(err)?;
        sheet.write_string(0, 1, "Tracks").map_err(err)?;
        for (i, (name, count)) in lib.folders().iter().enumerate() {
            sheet
                .write_string((i + 1) as u32, 0, name.as_str())
                .map_err(err)?;
            sheet
                .write_number((i + 1) as u32, 1, *count as f64)
                .map_err(err)?;
        }
    }
    {
        let sheet = workbook.add_worksheet();
        sheet.set_name("Duplicates").map_err(err)?;
        for (c, h) in [
            "Artist",
            "Title",
            "Reason",
            "Original location",
            "Staging location",
            "Decision",
        ]
        .iter()
        .enumerate()
        {
            sheet.write_string(0, c as u16, *h).map_err(err)?;
        }
        for (i, t) in lib
            .tracks
            .iter()
            .filter(|t| t.duplicate_of.is_some())
            .enumerate()
        {
            for (c, v) in [
                t.artist.clone(),
                t.title.clone(),
                t.duplicate_kind.clone().unwrap_or_default(),
                root.join(&t.original).display().to_string(),
                work.join(&t.staged).display().to_string(),
                t.action.label().into(),
            ]
            .iter()
            .enumerate()
            {
                sheet
                    .write_string((i + 1) as u32, c as u16, v.as_str())
                    .map_err(err)?;
            }
        }
    }
    let master = dir.join("GatherForge_Review.xlsx");
    workbook.save(&master).map_err(err)?;
    let artists = dir.join("Artists");
    fs::create_dir_all(&artists).map_err(err)?;
    for artist in lib.folders().keys() {
        let mut book = rust_xlsxwriter::Workbook::new();
        let sheet = book.add_worksheet();
        for (c, h) in [
            "Title",
            "Decision",
            "Original location",
            "Staging location",
            "Duplicate",
        ]
        .iter()
        .enumerate()
        {
            sheet.write_string(0, c as u16, *h).map_err(err)?;
        }
        for (i, t) in lib
            .tracks
            .iter()
            .filter(|t| &t.artist == artist)
            .enumerate()
        {
            for (c, v) in [
                t.title.clone(),
                t.action.label().into(),
                root.join(&t.original).display().to_string(),
                work.join(&t.staged).display().to_string(),
                t.duplicate_kind.clone().unwrap_or_default(),
            ]
            .iter()
            .enumerate()
            {
                sheet
                    .write_string((i + 1) as u32, c as u16, v.as_str())
                    .map_err(err)?;
            }
        }
        book.save(artists.join(format!("{}.xlsx", safe_name(artist))))
            .map_err(err)?;
    }
    Ok(master)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scan_and_stage_restore() {
        let root = std::env::temp_dir().join(format!(
            "gatherforge-test-{}-{}",
            std::process::id(),
            "stage"
        ));
        if root.exists() {
            fs::remove_dir_all(&root).unwrap();
        }
        fs::create_dir_all(root.join("random")).unwrap();
        fs::write(
            root.join("random/Artist feat. Other - Song.mp3"),
            b"dummy audio",
        )
        .unwrap();
        let mut lib = scan(&root, &Library::default()).unwrap();
        assert_eq!(lib.tracks[0].artist, "Artist");
        stage(&root, &mut lib).unwrap();
        assert!(!root.join("random/Artist feat. Other - Song.mp3").exists());
        assert!(
            root.join("xxx/_staging/random/Artist feat. Other - Song.mp3")
                .exists()
        );
        restore(&root, &mut lib).unwrap();
        assert!(root.join("random/Artist feat. Other - Song.mp3").exists());
        fs::remove_dir_all(root).unwrap();
    }
}
