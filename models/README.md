# GatherForge models — co-located with the application

The Rust executable discovers its application directory and reads model files *next to itself*, not from a mandatory external Python manager.

- `catalog.json`: verified download sources, pinned hashes, model IDs and roles (Git tracked).
- `config.example.json`: safe settings template (Git tracked).
- `config.local.json`: GUI-created real configuration (Git ignored).
- `bundled/`: vendor-approved small model weights; optionally committed to the repository (allowlisted).
- `installed/`: GUI-downloaded models; ignored, maintained automatically on launch.
- `cache/`: temporary local cache; ignored.
- `bundled/classification/keyword_rules.json`: small built-in metadata-only rules (not a trained model).

Default automatic models are Whisper Tiny Q5 multilingual (~32MB), Tesseract English and Persian fast data (<5MB each). Optional vision assets are not preinstalled. User can disable automatic downloads in Settings. If a bundled model validates against its pinned hash, GatherForge uses it directly, without duplicate storage.

For repository maintainers: `python3 tools/vendor_light_models.py --default --best-effort` downloads these small models *into this repo* with verification, ready for an intentional commit. This utility is for release preparation only. It is **not** a runtime requirement and users do not need to run it separately when installing.

Model weights alone cannot perform inference. GatherForge's v3.10 Reel pipeline (`src/reel_flow.rs` driving `tools/reel_analysis.py`, plus native Laya classification in `src/ai_models.rs`) does invoke whisper.cpp and Tesseract when their binaries are present, and records missing stages as unavailable in `analysis.json`. Optional binaries are built by `tools/setup_reel_engines.sh` from the in-app setup buttons. None of this is verified until it builds and runs on Fedora. Unsaving never happens without human approval, a matching video hash and a verified organized hard link.
