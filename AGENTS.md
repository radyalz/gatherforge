
# GatherForge project guidance

## GATHERFORGE_CORNER_RULE_V1

The music library track cards and their artwork frames use 8px corners, at the user’s explicit request. All other app-owned component corners stay strictly below 4px, default 2px. Use `theme::RADIUS` (2) for ordinary Rust/egui components and `border-radius:2px` for HTML/CSS. Preserve circular artwork and icons. See `docs/DESIGN_RULES.md`.

## GATHERFORGE_APP_CONTAINED_MODELS_V2 — superseded by GATHERFORGE_PROJECT_LOCAL_MODELS_V3

The V2 layout below is kept only as history. The active rule is `GATHERFORGE_PROJECT_LOCAL_MODELS_V3`: models live in `<repo>/models/`, with no second active store under `~/.local/share/gatherforge/`.

- AI model lifecycle MUST be native in GatherForge's Rust UI, NOT a required CLI/Python sidecar.
- The installed app root is `~/.local/share/gatherforge` containing `GatherForge`, `models/catalog.json`, `models/config.local.json`, `models/bundled/`, and `models/installed/`. In development it is the repository root containing the executable and `models/`.
- All editable model settings must appear under **Settings → AI & local models**, including actual filesystem paths, installed status, backend choices, and API endpoint/model/env-var name. Never store a raw API secret in the repository.
- Default lightweight model setup is automatic and backgrounded; it verifies checksums before use and resumes interrupted downloads. Do not block the whole GUI during downloads. A visible Stop setup control is required.
- After license and integrity verification, approved small default models may be committed under `models/bundled` in Git: `ggml-tiny-q5_1.bin` and `eng/fas.traineddata`. These are not available in a source-only ZIP unless actually included. Oversized optional GGUF/vision assets and local runtime downloads are ignored.
- Never delete, unsave, upload, or move a Reel as a side effect of model installation. Classification and inference remain disabled until implemented and tested end-to-end.
- Preserve the 2px corner rule, except 8px music-library cards. No content made outside the app root is required for default model management.

## GATHERFORGE_PROJECT_LOCAL_MODELS_V3: model directory is part of the project
- The canonical GatherForge runtime executable is `<repo>/GatherForge-Rust-Linux`.
  The menu launcher and `~/.local/bin/GatherForge` must resolve to it.
- The only managed model root is `<repo>/models/`, adjacent to the executable.
  Do not create another active model store under `~/.local/share/gatherforge/`.
- Model config and installed weights use `<repo>/models/config.local.json` and
  `<repo>/models/bundled` or `<repo>/models/installed`; UI must show real paths.
- Bundle only pinned, integrity-verified, license-reviewed small defaults in Git.
  Never commit local config, API secrets, partials, media, or optional large GGUF.
- Never claim inference or categorization works until it is implemented and tested.
- Preserve the 2px corner rule, except 8px music-library cards.

## GATHERFORGE_LAYA_NATIVE_V394 — Laya local model policy

- Keep Laya Multilingual's code, weights, configuration, and UI under the
  existing GatherForge project layout. Never silently create a second active
  model root outside `<repo>/models/`.
- Pin model revisions and check the SHA-256 of large weights. Don't silently
  overwrite a mismatched file. Large downloaded weights must remain Git-ignored.
- Laya is primarily a text decision model; use it for captions/transcripts,
  not for direct video or speech understanding. Keep first-pass rules fast.
- A model file is not a working inference pipeline. Don't label stages
  operational without tested inference. The v3.9.4 Reel button offers
  suggestions only; any organize/unsave action needs user review.
- Preserve GATHERFORGE_CORNER_RULE_V1, including its explicit music-card exception.
- Do not turn on automatic Instagram unsaving before review gates exist.

## GATHERFORGE_REEL_REVIEW_GATE_V310 — mandatory

- Keep per-Reel original URL, media size and SHA-256 in manifest.json. Saved-collection and direct Reels use the same verified analysis/approval schema.
- Local ASR/OCR/vision tools must be invoked off the egui thread. Never fabricate a successful stage if a runtime or weight is missing.
- Use native in-process Laya classification on combined text evidence; classification is always a suggestion, never a user approval.
- Preserve and keep originals in place. Organized category views may hard-link verified files but must not silently copy large videos, overwrite collisions, delete or move originals.
- Instagram unsave is forbidden unless the matching `analysis.json` is explicitly human-approved, video SHA-256 still matches, and an organized hard-link is verified. No automatic unsave on download; ambiguous browser UI must fail closed.
- Treat `tools/bin`, `tools/runtime-sources`, model caches, private config, and downloaded optional large weights as untracked, not repo payloads.
- Maintain the <4px rule outside music cards and the app-contained model-folder policy.

