# Public repository privacy

GatherForge stores library state in the selected music directory's `xxx/` folder, not in Git. Downloaded audio lands in `_Incoming/`. Both paths and common media extensions are ignored by Git.

The app does not require login, telemetry, or a cloud account. yt-dlp may read your browser cookies **only if you explicitly select a browser session** in the download settings. Do not commit cookies, playlists containing personal URLs, imported music, or Excel reports.

The migrator never force-pushes or rewrites Git history. It refuses to run over tracked uncommitted changes or a divergent remote. A Git commit contains author metadata: use your verified GitHub no-reply email before publishing.

Before public commits, run `bash scripts/check-public.sh` after staging source changes. This is a safety net, not a substitute for reviewing `git diff --cached`.
