# Native UI development dependencies

Three crates are deliberately **separate**, not one Cargo workspace:

- `forge-rune`: experimental Rune + Rune Kit interactive frontend, pinned to an exact Rune Git commit.
- `references/gpui`: GPUI Kit pinned to its semver 0.7 series for **behavior/API reference**; not embedded in Rune.
- `references/egui`: egui-shadcn 0.5.0 + eframe 0.33.3 for **component design and comparison**; not embedded in Rune.

Use `cargo fetch --manifest-path ...` to obtain dependency sources. Fetching a dependency does **not** install a system GUI package, does not change the existing `Cargo.toml`, and is not the same as migrating GatherForge.

Rune docs: https://github.com/anomalyco/rune/tree/dev/crates/rune-kit
GPUI Kit gallery: https://gpui-kit.com/gallery/
egui-shadcn gallery: https://ferrismind.github.io/shadcn-rs/

## Resource policy

Run only one UI prototype build at a time. Rune and GPUI can have large dependency graphs; `cargo fetch` needs network access and disk space. Keep the production egui build pinned separately. Avoid `cargo clean` or deleting caches on the user's PC.
