# GatherForge AI model policy · v3.10

## Installed layout

`GATHERFORGE_PROJECT_LOCAL_MODELS_V3` supersedes the older `GATHERFORGE_APP_CONTAINED_MODELS_V2` layout for the working installation. The executable is `<repo>/GatherForge-Rust-Linux` and the only managed model root is `<repo>/models/` (`models/catalog.json`, `models/config.local.json`, `models/bundled/`, `models/installed/`). `~/.local/bin/GatherForge` and the desktop launcher are symlinks to that binary; do not create a second active store under `~/.local/share/gatherforge/`. See `docs/MODEL_STORAGE.md` for the migration and inspection steps.

## What works

The Rust Settings panel manages model weights directly. Its background worker downloads the three default small weights, resumes `.part` transfers, bounds size, verifies SHA-256/Git-blob-SHA1, reports progress, supports cancellation and retry, and loads bundled weights before downloading. Users do not run a separate Python script or edit external config. Configuration and API backend selection are shown in Settings; app saves `models/config.local.json` beside itself.

## Repo weights and Git rules

The repository explicitly allows three small model files under `models/bundled`: Whisper Tiny multilingual Q5 (~32 MB), Tesseract fast English (~4 MB) and Persian (<1 MB). Maintainer bootstrap `tools/vendor_light_models.py` may fetch/check them once for an intentional Git commit. The installer automatically attempts this on Fedora after a successful build. The ZIP is not itself binary-weight-inclusive. Other model weights, private config, user-generated media, cache and partial downloads are ignored. Do not auto-push them. Larger vision weights (~131+104 MB) exceed the GitHub 100 MiB file cap and remain optional downloads.

## Trust and safety

Downloaded assets are verified against catalog digests and published only after integrity checks. The bundled files must also match before being trusted. Local configuration must not contain raw API secrets; only an environment-variable name is recorded. External API requests, private media uploads and unsaving Reels are not performed by the model manager. Never mark a Reel eligible for unsave based solely on successful model download. Keep the 2px corner rule.

## Inference pipeline (v3.10)

The Reel pipeline is wired in source: `src/reel_flow.rs` drives `tools/reel_analysis.py` for the `analyze`, `approve` and `record-category` stages, and native in-process Laya classification lives in `src/ai_models.rs`. Missing runtimes are reported as unavailable stages and recorded in `analysis.json`; a stage is never fabricated as successful. Unsaving requires a human-approved `analysis.json`, a matching video SHA-256 and a verified organized hard link (`approved_for_unsave()` in `tools/reel_analysis.py`).

Weights are still only assets. Optional binaries are prepared by `tools/setup_reel_engines.sh` (invoked from the in-app **Setup speech engine** / **Setup optional vision engine** buttons) and require Git, CMake, a C++ compiler and network access on first use. This package does not compile them itself, and real transcription, OCR, vision and Laya inference are unverified until the Fedora build and a live run confirm them. Do not describe a stage as working, or unsave anything, before that verification.

## Local testing

`python3 -m unittest discover -s tests -q`; `cargo test --release`; `cargo build --release`. Rust compiling must be verified on Fedora. Rollback restores tracked source if the build fails and does not touch the previous executable. The installer does not commit or push.

## Arabic subtitle OCR

GatherForge supports a pinned, integrity-verified `ara.traineddata` model from the Apache-2.0-licensed Tesseract `tessdata_fast` collection. Like the English and Persian models it stays under `<repo>/models/`. `models/installed/ocr/tessdata` is an active OCR language-data location when managed locally; it is not a separate model root. The analysis pipeline considers both bundled and installed language assets.
