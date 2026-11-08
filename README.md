# GatherForge 3.2 — Collection Studio

A native desktop toolkit for collecting, reviewing, organizing and publishing audio safely.

- **Engine:** Rust (`library.rs`, `downloader.rs`, `model.rs`); no Python or Go runtime.
- **UI:** egui/eframe native window, **not** a browser or localhost web server.
- **Design:** reusable `src/components` and documented `docs` design/engineering rules.
- **Downloads:** single tracks, playlists or batches via external yt-dlp, saved in `_Incoming`; best available format by default.
- **Library:** recursive scan, embedded metadata when present, first artist, duplicate review, editable names, group merging, owned staging, verified publishing, and Excel reports.

## Linux development

```bash
source "$HOME/.cargo/env"
cargo fmt --all -- --check
cargo test
cargo build --release
./target/release/gatherforge
```

Requires development packages for native window support on Linux. The program looks for `yt-dlp` beside the executable or on `PATH`. FFmpeg is only needed when converting to MP3.

## Safe library workflow

Select the **music library**, not this source repository. Scan → review → stage → export review report → publish → finalize after verifying files. Original music is never deleted during scanning or publishing. Finalization explicitly deletes only successfully verified staged copies; unapproved files are retained separately.

## Project structure

```text
src/
  main.rs          # native desktop entry point
  app.rs           # screens, interaction and commands
  theme.rs         # all design tokens
  components/      # reusable pure UI drawing elements
  library.rs       # scanner / staging / reports
  downloader.rs    # yt-dlp integration
  model.rs, util.rs

docs/              # visual rules, component catalog, safety, references
scripts/           # release builds and safe migration
.github/workflows/ # cross-platform CI
```

## Migration from the previous Go/Python and initial Rust preview

Do not erase `.git` or your music directories. The included `scripts/install_into_repo.sh` checks the Git repository identity and working tree, backs up any local uncommitted Rust preview, imports the new source, builds/tests, and commits/pushes only after successful checks. No force push.

If your current project has an active staged/published batch, finish or restore it **in the current version** before migrating the file engine. New projects read legacy state only when its ownership marker is present.

## Privacy

Do not add absolute device paths, credentials, browser cookies, downloaded music or audio metadata dumps to the public source. `docs/SAFETY_AND_RELEASE.md` and `.gitignore` explain the rules.

## UI design skills

This source includes a repository-specific Claude skill at `.claude/skills/gatherforge-ui/SKILL.md`. It summarizes the design principles adapted from Anthropic frontend-design, Impeccable, and better-ui. See `docs/DESIGN_SYSTEM.md` and `docs/COMPONENTS.md`. The full third-party skills are not vendored.

## Install / update an existing repository

From the extracted source, run `bash scripts/install_into_repo.sh /path/to/your/repository`. The installer backs up the local Rust prototype, compiles the new source, migrates tracked files, commits, tags and pushes only after passing checks. Do not migrate a currently staged batch before resolving it.

## Install / update an existing repository

From the extracted source, run `bash scripts/install_into_repo.sh /path/to/your/repository`. The installer backs up the local Rust prototype, compiles the new source, migrates tracked files, commits, tags and pushes only after passing checks. Do not migrate a currently staged batch before resolving it.

## Install / update an existing repository

From the extracted source, run `bash scripts/install_into_repo.sh /path/to/your/repository`. The installer backs up the local Rust prototype, compiles the new source, migrates tracked files, commits, tags and pushes only after passing checks. Do not migrate a currently staged batch before resolving it.
