# Component contracts

`src/theme.rs`: all palette and spacing choices. No hard-coded user path or data.

`src/components/chrome.rs`: navigation, hand-painted line icons, and active-library breadcrumb; may not mutate files.

`src/components/cards.rs`: single thematic vinyl hero, borderless numeric summaries, and operation-step content. Should not be used to put every line inside an identical tile.

`src/components/controls.rs`: primary and secondary buttons, status pills, typographic section heading. Buttons are visually consistent across screens.

`src/components/tracks.rs`: performant row renderer, generated artist swatch, visible review flags, selection and hover states. No filesystem calls.

`src/app.rs`: screen composition and user-initiated actions; retains Go-to-Rust migrated engine via `src/library.rs` and `src/downloader.rs`. Stage, restore, publish and finalize continue to require confirmation.

## Reviewer checklist
- Empty library: what do I click first?
- Download error: does the log say what happened?
- Potential duplicate: can I inspect before publishing?
- Publish: is staging preserved?
- Small window: are controls still reachable?
