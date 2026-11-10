# GatherForge Rune — full Forge frontend source

A native Rune desktop interface powered by the real GatherForge Rust music library, Instagram manifests and download queue. The seven-button placeholder has been replaced by a four-panel operational Workspace and dedicated pages.

Features: combined media table; local video and audio preview; Reel browser with caption and persisted evidence; manual Reel analysis/category/approval; virtualized music and media tables; artist editing and keep/review/skip; explicit confirmation for stage, publish, restore and finalize; persistent music/Reel download jobs with cancel and retry; project-local model inventory, install and verification using the existing native Rust model engine; dark/light Forge colors.

Launch after installing:

```bash
./GatherForge-Rune-Linux
```

Development launch:

```bash
cargo run --manifest-path ui/forge-rune/Cargo.toml --release
```

The source reuses `../../../src/library.rs`, `instagram.rs`, `jobs.rs`, `downloader.rs`, `model.rs`, `util.rs`; build from the project directory rather than copying the Rune crate by itself. The old egui application is preserved as a separate executable for existing advanced operations.

The pre-release Rune dependency is pinned to a specific Git revision. Source changes were checked against the supplied archive and the Python tests, but Rust compilation, runtime behavior and screenshot fidelity were not verifiable in this environment.

## Cyber Forge visual integration
The frontend renders the agreed four-panel command center with enhanced Forge navigation, decorative chrome, real cover previews and Reel filmstrip frames. Rune video errors display the decoder message and offer an external player; nothing fabricates ASR or OCR. All original queue, categorizer and review actions continue calling the existing engines. The baseline in `src/main.rs` is the 2026-11-10 complete native implementation, not the earlier empty Rune navigation prototype.
