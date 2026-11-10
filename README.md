# GatherForge

GatherForge is a native Rust/egui desktop app for managing a music library and reviewing saved Instagram Reels. The current source snapshot is **v3.10**. It is a project/source archive, not a verified machine backup or a guarantee that optional runtimes are installed.

## Start here

- [Current capabilities and verification status](#current-status)
- [Build and run](#build-and-run)
- [Documentation map](#documentation-map)
- [Privacy and safe use](#privacy-and-safe-use)

## Current status

Reel intelligence includes optional speech recognition and OCR stages, with distributed frame sampling and English/Arabic/Persian OCR. Your local project can be located at `/mnt/shared/Projects/MusicOrganizer`.

The source includes music scanning, review/staging/publish workflows, download queues, a Reel manifest and analysis/approval workflow, and project-local model management. Reel analysis is implemented in source: it records available and unavailable ASR/OCR/vision stages, combines text evidence for local Laya suggestions, and keeps category approval separate from model suggestions. Organizing uses verified hard links and preserves originals. Unsave is an explicit user action gated by human approval, matching media SHA-256 and verified organization.

**Verification is incomplete.** This snapshot has not been verified here with a Fedora release build, real local inference across the supported engines, or the live Instagram browser flow. Engine binaries may need to be prepared from Settings and required source-build tools/network access. Missing engines should be reported as unavailable. Treat suggestions as unverified until you validate the build and a live run on the target machine; do not infer working inference from the presence of model weights.

The canonical model directory is `<repo>/models/`, adjacent to `GatherForge-Rust-Linux`. Do not create a second active model store under `~/.local/share/gatherforge/`. See [model storage](docs/MODEL_STORAGE.md) and [AI model policy](docs/AI_MODEL_POLICY.md).

## Build and run

On Linux, install Rust and the native development libraries required by eframe, then:

```bash
cargo fmt --all -- --check
cargo test
cargo build --release
./target/release/gatherforge
```

The installed launcher may point to the repository's `GatherForge-Rust-Linux`; see [model storage](docs/MODEL_STORAGE.md). `yt-dlp` is required for supported downloads and must be available beside the executable or on `PATH`. FFmpeg is needed only for audio conversion. Optional Reel engines have separate setup requirements described in [Reel pipeline](docs/REEL_PIPELINE.md).

## Documentation map

| Topic | Guide |
| --- | --- |
| App workflows | [Workspaces](docs/WORKSPACES.md) |
| Reel analysis, approval and unsave gate | [Reel pipeline](docs/REEL_PIPELINE.md) |
| Local Laya classifier and limits | [Laya](docs/LAYA.md) |
| Model paths, downloads and Git rules | [Model storage](docs/MODEL_STORAGE.md), [AI model policy](docs/AI_MODEL_POLICY.md) |
| Visual and component constraints | [Design rules](docs/DESIGN_RULES.md), [design system](docs/DESIGN_SYSTEM.md), [components](docs/COMPONENTS.md) |
| Release, migration and privacy | [Safety and release](docs/SAFETY_AND_RELEASE.md), [Privacy](PRIVACY.md) |

## Privacy and safe use

Keep media, browser cookies, private configuration, API secrets and downloaded optional weights out of Git. Reel classification suggestions are not approvals. Originals must remain in place, and unsaving must pass the documented review gate. See [Privacy](PRIVACY.md) and [Safety and release](docs/SAFETY_AND_RELEASE.md).

## Permanent project requirements

- The `GATHERFORGE_CORNER_RULE_V1` keeps app-owned corners at 2 px (strictly below 4 px), except for the requested 8 px music library cards and their artwork frames. See [Design Rules](docs/DESIGN_RULES.md).
- The only managed model root is `<repo>/models/`.
- Never describe optional inference or the live Instagram interaction as verified without a successful target-machine run.
