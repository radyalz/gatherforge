# Rune / Rune Kit frontend preview (experimental)

The Rune UI is a separate preview executable, **not** the application's production frontend.
It uses a pinned commit of `anomalyco/rune` and shows navigable layout sections plus dark/light palette switching. It does not yet invoke downloading, categorization, Reel OCR, or change files.

```bash
cargo run --manifest-path ui/forge-rune/Cargo.toml
```

A network connection is needed to fetch the pre-release Git dependency. Do not combine the Rune, GPUI and egui runtime trees in one window; keep the existing egui app available until a complete, tested Rune implementation is ready.

After compiling, test the actual UI on Fedora, and only then extract application core events/data types into a separate reusable crate. Unicode shaping and model manager integration are follow-on work.
