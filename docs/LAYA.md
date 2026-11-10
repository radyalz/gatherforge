# Laya Multilingual inside GatherForge

Model: `convaiinnovations/laya-multilingual` (Apache-2.0, ~322M parameters).
Chosen for mixed English/Persian Reel captions. Laya is not a transcriber or
vision model: it answers typed classification questions about text.

## Location

```text
MusicOrganizer/
  models/
    catalog.json                 # tracked
    config.example.json          # tracked
    config.local.json            # ignored; actual user settings
    installed/
      classification/
        laya-multilingual/
          model.safetensors      # ~647 MB, verified hash; ignored
          encoder/config.json
          tokenizer/tokenizer.json
          tokenizer/tokenizer_config.json
          rl_agent_config.json
```

The application fetches the large model and small support JSONs automatically
when `auto_download` is enabled and `classification_backend=laya_local`.
Use Settings → AI & local models to check/repair the model or change the
classifier to rules. An existing `config.local.json` is migrated in-place on
startup, retaining other settings and the chosen model storage budget (raised
to at least 1000 MB when upgrading an older default if necessary).

Main weight URL (revision-pinned):
`https://huggingface.co/convaiinnovations/laya-multilingual/resolve/453577addb8a9c85395d2449a91fce4742f5f2f1/model.safetensors`

SHA-256: `9d628fd971b700382ac6f65920a86f149777b2e748e0c955fb3b19695aa8f204`

Auxiliary tokenizer and model JSON files use the same pinned revision, are
checked as JSON, and are stored next to the weights. The package itself
contains no weights. The Rust model engine is `laya = 0.1.1` using Candle,
not a Python or ONNX inference service.

## Actual behavior

The desktop application exposes **Suggest category with Laya** on each Reel
manifest card with a nonempty caption. It operates in a background thread and
suggests one of nine broad categories. The result is an ephemeral
**unapproved suggestion** visible in the UI; it is not yet persisted to media
manifests and not used to move files.

**Not done**: audio ASR, OCR invocation, visual frame analysis, Reel folder
reorganization based on Laya suggestions, classification approval storage, and
a safe Instagram unsave-after-review workflow. Both auto-unsave and the old
unsave browser entry are disabled in this version because successful download
is insufficient proof of content understanding.

## Operational requirements

On CPU, Laya upcasts checkpoint weights to f32; allow around 2.4 GB of free
RAM. Startup/cold inference can be slower than a rules-only classifier. Model
weights are excluded from Git by the existing `models/.gitignore` `/installed/`
entry. No cloud API or Docker is required for caption classification.

### Troubleshooting

- Missing weights: enable automatic setup and use Install / repair in the UI.
- Checksum failure: the app refuses to replace mismatched files without user
  inspection; never delete a local model automatically.
- Missing metadata JSON: click Install / repair again when online.
- Cargo build errors: the installer restores prior source/docs and leaves the
  old compiled app intact. Report the exact Rust error output for diagnosis.
