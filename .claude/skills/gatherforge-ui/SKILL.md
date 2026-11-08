---
name: gatherforge-ui
description: Design, critique or implement the native Rust egui desktop UI for GatherForge. Use for screen layouts, component styling, interaction states, accessibility, and workflow clarity.
---
# GatherForge UI design practice

Apply these source-informed principles, adapted for Rust egui rather than a browser:
- Anthropic frontend-design: begin from the user's real task, commit to one recognizable visual world, and avoid default AI dashboard treatments.
- Impeccable / operate: visible task states and recoverable operations matter more than decoration. Use normal desktop affordances, visible focus, empty/error/working states, and clear verbs.
- better-ui: use optically aligned icons, repeatable spacing and corner systems, restrained motion, and selected/hover states that clarify what can be edited.

Read `docs/DESIGN_SYSTEM.md` and `docs/COMPONENTS.md` before changing UI. The visual world is 'Collection Studio': pale paper content, midnight side navigation, cobalt actions, a single warm vinyl illustration on Overview. Never turn every group into an equal card, never use all-caps eyebrow labels above each heading, and never replace icons with emoji.

Use existing `src/library.rs` and `src/downloader.rs` for all effects. Presentation components must be pure. Do not change scan/stage/publish/finalize semantics as part of a visual change. Never delete or overwrite a user's music. On a UI task, evaluate at least the empty library, selected track, duplicate review, downloading, error and staging confirmation states. Preserve keyboard focus and legible contrast.

Implementation: change `src/theme.rs`, `src/components/*`, or screen composition in `src/app.rs`; keep business logic isolated. Check `cargo fmt --all`, `cargo test`, and `cargo build --release`. A Git push requires a clean staged diff, privacy check, and a verified public Git identity.

Inspiration and design references:
- https://github.com/anthropics/skills/tree/main/skills/frontend-design
- https://github.com/pbakaus/impeccable
- https://github.com/jakubkrehel/skills/tree/main/skills/better-ui
