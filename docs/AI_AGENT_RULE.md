# AI implementation rules

This file summarizes active requirements. The canonical model layout is
`GATHERFORGE_PROJECT_LOCAL_MODELS_V3` in the repository's `AGENTS.md`; the older
V2 `~/.local/share/gatherforge/` layout is historical and must not be revived.

- Keep model catalog, local configuration and weights under `<repo>/models/`,
  beside `GatherForge-Rust-Linux`. Never create a second active model root.
- Model lifecycle and user-facing settings belong in the native Rust UI. Show
  real paths and status; do not require a Python/CLI sidecar for normal use.
- Pin revisions and verify hashes before use. Never silently overwrite
  mismatched files. Keep local configuration, credentials, partial downloads,
  media and optional large weights out of Git.
- A model file is not proof of a working inference stage. Report missing
  runtimes as unavailable, and claim operational status only after a successful
  build and end-to-end run on the target platform.
- Reel category output is a suggestion. It does not approve, organize or
  unsave content by itself. Preserve originals; unsave only through the
  documented human-review gate in [REEL_PIPELINE.md](REEL_PIPELINE.md).
- Never send local media or captions to an external service without explicit
  user action and a documented feature.
- Keep app-owned UI corners strictly below 4 px (standard 2 px), except 8 px music track cards and their artwork frames.
  See [DESIGN_RULES.md](DESIGN_RULES.md).

For detailed storage and integrity requirements, see
[AI_MODEL_POLICY.md](AI_MODEL_POLICY.md) and [MODEL_STORAGE.md](MODEL_STORAGE.md).

The historical `GATHERFORGE_APP_CONTAINED_MODELS_V2` policy is superseded by `GATHERFORGE_PROJECT_LOCAL_MODELS_V3`; see `AGENTS.md`.
