# GatherForge Workspaces

## Music

Download tracks, scan your music folder, browse cards by title and artist, and select a card for playback and review. Embedded album artwork is previewed when FFmpeg can read it; otherwise the card shows a generated placeholder. Search and duplicate review remain available. Stage, restore, publish, and finalize remain separate explicit actions.

## Instagram Reels

Download individual Reels or use the Saved collections browser. The review feed shows an asynchronously generated local video preview when FFmpeg is available. Click a preview or choose **Play downloaded video** to open the file in the system player. Analyze a single Reel or the current list and review the actual stage results before choosing a category.

**Remove record** requests confirmation and moves matching manifest metadata aside with a `.gatherforge-hidden` suffix. It does not delete the video or its analysis, and does not unsave anything on Instagram. **Restore removed records** restores eligible metadata without overwriting an existing file. Approval and organization require the original manifest, so a removed record must be restored before those operations. Metadata removal uses hard links and can fail safely on filesystems that do not support them.

## Appearance and limits

The music track cards and artwork frames use 8px corners. Other UI corners remain below 4px, normally 2px. Settings and appearance are local. Reel analysis suggestions require review and do not automatically unsave content. The feed and GUI must be verified on the target system.
