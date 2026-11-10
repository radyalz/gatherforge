# GatherForge model storage — project-local policy (v3.9.3)

The canonical working installation is `/mnt/shared/Projects/MusicOrganizer` (or the repository selected by the installer). The binary `GatherForge-Rust-Linux` lives directly in the root and the `models/` directory is its sibling. `~/.local/bin/GatherForge` is a symlink to that binary, not another installation. The desktop launcher uses the same symlink. The Rust app resolves `current_exe()` to the project binary and reads and writes `<repo>/models/`.

- `models/catalog.json`: pinned model URLs, hashes, storage and roles.
- `models/config.example.json`: public template.
- `models/config.local.json`: active per-user preferences (Git ignored; no actual API secrets).
- `models/bundled/asr/ggml-tiny-q5_1.bin`: small Whisper Q5 model, eligible for Git if verified and license reviewed.
- `models/bundled/ocr/tessdata/eng.traineddata` and `fas.traineddata`: OCR defaults, eligible for Git.
- `models/installed/vision/*.gguf`: optional large model weights (Git ignored).

`tools/gatherforge_project_models.py --apply` copies checksum-verified assets from the previous `~/.local/share/gatherforge/models` directory into this project. It does not move or delete existing model files. If default weights are missing, it attempts pinned verified downloads. Optional vision models are NOT downloaded automatically. If offline, the app remains usable and missing models are shown as missing. The update itself contains no weights; existing verified files are reused.

To inspect: `ls -lh models/bundled/asr/ models/bundled/ocr/tessdata/`, `git status --short --untracked-files=all models/bundled`. Do not commit credentials or user media. `AGENTS.md` enforces this canonical location. Updates must not silently change executable location or reintroduce an XDG-only model store. Preserve the 2px UI corner radius everywhere.

This document covers **storage and launching**. The v3.10 Reel pipeline is implemented separately in `src/reel_flow.rs` and `tools/reel_analysis.py`; see `docs/REEL_PIPELINE.md` for the ASR/OCR/vision stages, `analysis.json` schema and approval gate. Actual inference is unverified until it compiles and runs on Fedora, so downloaded weights alone still cannot determine whether unsaving a Reel is safe.
