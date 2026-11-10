# GatherForge Design Rules — Mandatory

## GATHERFORGE_CORNER_RULE_V1 — maximum UI corner radius

**Music library track cards and their artwork frames use 8 px corners at the user's request. All other app-owned UI corners remain strictly less than 4 px, normally 2 px.**

- The exception applies only to the music track-card surface and artwork frame in `src/components/tracks.rs`; it does not apply to buttons, dialogs, other cards or Reel panels.
- Rust/egui components elsewhere use `theme::RADIUS` (2). Browser controls use CSS `border-radius: 2px` or less. App-authored SVG rectangles use `rx <= 2` and `ry <= 2`.
- Circular artwork, icons and system-owned chrome are outside this rectangular-component rule.
- Tests must enforce the 8px music-card exception and the under-4px rule everywhere else.


Validation command: `python3 -m unittest discover -s tests -q` followed by `cargo test --release --quiet` on Fedora.
