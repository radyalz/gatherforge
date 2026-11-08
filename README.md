# GatherForge Desktop
Native cross-platform desktop application for batch music downloads, artist-first organization, duplicate review, Excel reporting, and safe staging.

Go core communicates with a Python Tkinter desktop UI through subprocess commands. **No browser or local web server.**

On Linux: install Go and Python Tk, then run `./GatherForge`. This builds the core if needed. The graphical desktop app runs from its source directory, but you can choose any music-library directory in the GUI.

Workflow: Scan → review/edit artists and possible duplicates → Stage → Export XLSX → Publish → Finalize (only after confirming). The program does not delete staged originals before publishing and verifying them.

Downloader: Enter any number of permitted track or playlist URLs, install official yt-dlp using the app button, and use Download best available audio. Downloaded files go into the selected library's `_Incoming` directory.

GitHub Actions builds one-file desktop packages for Linux, Windows and macOS on each push. For local packaging, run `python -m pip install pyinstaller && python scripts/build-desktop.py` on your target OS.

No user-specific paths, playlists or credentials belong in the repository. Audio and staging directories are excluded by `.gitignore`.
