# Historical note: v3.9.2 model-management update

This addendum records the v3.9.2 model-management change. Its original V2
storage path and capability statements are superseded by the current rules in
`AGENTS.md`, `README.md`, and `docs/AI_MODEL_POLICY.md`.

The AI model manager is now native inside the Rust application. After installation, its executable, catalog, `models/config.local.json`, bundled weights and automatically downloaded weights live together under `~/.local/share/gatherforge/` (or beside the development executable). The UI at **Settings → AI & local models** displays their real paths and manages default downloads automatically; no separate model-manager CLI is required.

Small weights (Whisper Tiny multilingual Q5, English/Persian Tesseract data) can be verified and committed under `models/bundled/` because they are below 100 MiB. The installer attempts to vendor them into the working repo on Fedora, but the ZIP itself is source-only. Optional SmolVLM2 weights, private config and cache are ignored by Git. No credentials or Reels belong in the repository.

**Current status:** the old `~/.local/share/gatherforge/` path is not the active
layout; use project-local `<repo>/models/` (`GATHERFORGE_PROJECT_LOCAL_MODELS_V3`).
Reel analysis and review gates are present in source, but runtime availability,
end-to-end inference and the live browser flow remain unverified until tested on
the target Fedora installation. See `docs/REEL_PIPELINE.md` and
`docs/MODEL_STORAGE.md`.
