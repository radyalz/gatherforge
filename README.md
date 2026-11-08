# GatherForge

A **local, portable downloader and artist-first music library manager** with a web-based UI. Written in Go, using only the standard library. Runs as a **single compiled app** on Linux or Windows; the embedded local web UI opens in your browser. No hosted service, database, or Python packages needed for the organizer itself.

> **Version 1.1.0 — initial integrated downloader release.** Test on a copy of a small music library before relying on it. Only download music you have permission to save, respecting platform terms and applicable laws.

## Features

- **Download:** paste a single music-track URL, a playlist, a likes URL, or up to 100 URLs (one per line, and each can itself represent a long playlist). Supported sources depend on yt-dlp extractors.
- **Best original audio:** `bestaudio/best`, plus `soundcloud:formats=*`, without transcoding by default. The available quality depends on the source and account access; lossless is **not** guaranteed.
- **Optional MP3:** requires FFmpeg installed on the system; explicit quality option in the UI.
- **Private content:** optional browser-cookie selection for Firefox, Chrome, Chromium, Edge, or Brave (your browser profile must be accessible).
- **Safe resume:** download archive at `xxx/_downloads/archive.txt` avoids downloading already completed media IDs on repeat runs; interrupted downloads can often resume.
- **Organize:** recursively scan nested folders and determine the **first credited artist** using embedded metadata or filename fallback. Individual tracks and whole artist groups can be renamed/merged in the UI.
- **Duplicates:** detect exact duplicates by file hash and flag matching artist/title candidates; possible duplicates require your review before publishing.
- **Staging and review:** move originals into `xxx/_staging`, then publish **copies** into artist folders. Staged files are not removed until you explicitly finalize after verification. Unselected duplicates are retained in `xxx/_Not_Published`.
- **Excel reports:** create `.xlsx` overview and per-artist worksheets under `xxx/_reports`.
- **Runs relative to the executable:** copying a binary into a different music directory changes its library root; it does **not** operate on the terminal's current directory.

## Quick start (download prebuilt binary)

Copy the Linux or Windows executable into the music folder you want to manage, for example:

```text
~/Downloads/Music/
  GatherForge-Linux-x64
  _Incoming/                # downloads land here
  xxx/                      # app-managed data (auto-created)
  <Artist>/                 # published folders
```

Linux:

```bash
cd ~/Downloads/Music
chmod +x GatherForge-Linux-x64
./GatherForge-Linux-x64
```

Windows: put `GatherForge-Windows-x64.exe` into your target music folder and double-click it.

The application starts a server on `127.0.0.1` and opens the browser. Close the terminal/application window to stop it. It does not expose an internet-facing server.

1. Click **Install / update yt-dlp**. This fetches the appropriate **official yt-dlp standalone** executable into `xxx/_tools/` and verifies it against the official release's `SHA2-256SUMS` manifest. It requires internet access for installation. Alternatively, a pre-installed `yt-dlp` on `PATH` is detected automatically.
2. Paste URLs, one per line, then choose quality, browser-cookie mode, and playlist behavior. Click **Start download**. Progress is displayed in the app. You can stop the job at any point.
3. Downloaded files appear under `_Incoming`. Once downloading finishes, click **Scan / rescan**, edit artists and duplicate decisions, then **Export Excel**.
4. **Move to staging**, **Publish artist folders**, and—only once you have verified the output—**Finalize / clean staging**. You can restore staging before publishing.

**Important:** If you installed the downloader before your first scan, the app automatically creates and marks its managed `xxx` folder. It refuses to adopt a pre-existing, unmarked `xxx` directory. Do not manually delete or rename `xxx` mid-batch.

### Example: SoundCloud likes

Replace `USERNAME` with your SoundCloud username, or paste any track/playlist URL:

```text
https://soundcloud.com/USERNAME/likes
```

If necessary, select the browser that is logged in to your SoundCloud account. Accessibility of individual tracks can depend on login, restrictions, or removal.

## Build from one source folder

Install [Go 1.23 or newer](https://go.dev/dl/) **only if you want to rebuild the source**. The released executables don't require Go.

```bash
git clone https://github.com/YOUR-USERNAME/YOUR-REPO.git
cd YOUR-REPO
go test ./...
bash scripts/check-public.sh
bash scripts/build.sh
```

This compiles Linux x64 and Windows x64 executables into `dist/`. On Windows with PowerShell, use `./scripts/build.ps1`.

### Source layout

```text
GatherForge/
├── main.go                # original organizer, metadata readers, staging + Excel + browser UI
├── downloader.go          # yt-dlp installation, URL queue, safe command runner, live logs
├── downloader_test.go     # URL, args, checksum, workspace & integration tests (offline)
├── go.mod
├── scripts/
│   ├── build.sh
│   └── build.ps1
├── .github/workflows/
│   └── build.yml         # tests and cross-platform CI artifacts on push / PR
├── .gitignore            # excludes music, staging, downloader binaries and build outputs
└── README.md
```

### Command-line mode

From the music folder where the **compiled binary** lives:

```bash
./GatherForge-Linux-x64 --help
./GatherForge-Linux-x64 --install-yt-dlp
./GatherForge-Linux-x64 --download "https://soundcloud.com/ARTIST/TRACK" "https://soundcloud.com/USER/sets/PLAYLIST"
./GatherForge-Linux-x64 --scan
./GatherForge-Linux-x64 --export
```

The graphical interface additionally offers cookie/quality choices; CLI downloads use best native audio, public-access mode, and playlist expansion.

## Publish to your own GitHub repository

Create an empty repository on GitHub (avoid auto-creating a README, since this project already has one). Unzip this source folder, then inside that folder:

```bash
git init
git branch -M main
git add .
git commit -m "Initial release: music organizer and yt-dlp downloader"
git remote add origin https://github.com/YOUR-USERNAME/YOUR-REPO.git
git push -u origin main
```

Replace `YOUR-USERNAME` and `YOUR-REPO` with your repository details. GitHub may ask you to authenticate using your configured credentials or a token. If you copied the project into an **existing** Git repository, skip `git init`/`remote add`, then use `git add`, `git commit`, `git push` according to its branch conventions.

**The source zip intentionally contains no `.git` directory or configured remote.** See [PRIVACY.md](PRIVACY.md) for a public-release checklist and how to check your commit author settings. You control the GitHub destination. CI automatically runs `go test` and builds both executable artifacts. The yt-dlp binary is **not** committed; it downloads on first use, outside Git tracking. Choose your project's license before publishing publicly.

## Caveats and security

- `yt-dlp` is an external upstream program used through `exec.CommandContext`, **not** embedded Go code. This avoids installing Python but requires downloading its standalone binary once (or placing yt-dlp on your PATH). Downloading/cookie access requires internet and platform access.
- Some formats/streams need FFmpeg. Optional MP3 conversion requires it. Transcoding compressed audio to MP3 does **not** improve quality.
- The checksum list and binary are retrieved via HTTPS from the official yt-dlp GitHub release; the digest is checked before installation. This is not a cryptographic **signature** check of the checksum list.
- Artist tags can be wrong, missing, or represent a SoundCloud uploader rather than the actual performer. The UI supports corrections. First-artist heuristics may misidentify names with `&`, `feat`, or commas.
- Filesystem operations change local files; there is no guarantee against hardware failures, process termination, or pre-existing filesystem corruption. Back up irreplaceable music first.
- Exact duplicates are byte-identical. Different encodings of the same recording need review; the app does not fingerprint audio waveforms.
- Cookies are read by yt-dlp from your local browser when you explicitly enable that option; the app does not upload your library to a server.
- A playlist may contain unavailable or private entries. The download log reports yt-dlp results, and the archive avoids repeating completed items.
- Do **not** commit your music library, staging area, browser cookies, or personal download logs. The `.gitignore` excludes app-managed directories and common audio file extensions. Use `git status` before pushing.

## Privacy before publishing

Review [PRIVACY.md](PRIVACY.md), then run `bash scripts/check-public.sh` **after staging** the files you intend to commit. The script checks staged paths and common machine-specific strings; it cannot certify that a repository contains no sensitive information. Check Git commit author/committer metadata separately.

## Development

```bash
go test -v ./...
go vet ./...
go build -trimpath -o dist/GatherForge-Linux-x64 .
```

The browser UI is embedded in the `htmlUI` raw string at the end of `main.go`; backend API handlers are in `serve()` and downloader behavior is in `downloader.go`. For UI changes, rebuild and copy the executable into a **throwaway test music folder**, rather than running `go run .`: the organizer always uses the location of the executable (including Go's temporary `go run` path).
