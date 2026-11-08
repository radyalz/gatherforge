package main

import (
	"bufio"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"sync"
	"time"
)

const releaseURL = "https://github.com/yt-dlp/yt-dlp/releases/latest/download"

// The downloader is deliberately separate from the organizer's staging state.
// All newly retrieved files land in _Incoming until a user scans and stages them.
type DownloadManager struct {
	mu                sync.Mutex
	root, work        string
	active            bool
	mode              string
	message           string
	logs              []string
	logCounter        int64
	urls              []string
	started, finished string
	cancel            context.CancelFunc
	exitCode          int
}

type DownloadOptions struct {
	URLs    string `json:"urls"`
	Quality string `json:"quality"` // native | mp3
	Browser string `json:"browser"` // none | firefox | chrome | chromium | edge | brave
	Scope   string `json:"scope"`   // playlist | single
}

func (d *DownloadManager) addLog(message string) {
	d.mu.Lock()
	defer d.mu.Unlock()
	if len(message) > 4000 {
		message = message[:4000]
	}
	d.logs = append(d.logs, message)
	d.logCounter++
	if len(d.logs) > 240 {
		d.logs = d.logs[len(d.logs)-240:]
	}
}

func (d *DownloadManager) toolName() (string, error) {
	switch runtime.GOOS {
	case "windows":
		return "yt-dlp.exe", nil
	case "linux":
		switch runtime.GOARCH {
		case "amd64":
			return "yt-dlp_linux", nil
		case "arm64":
			return "yt-dlp_linux_aarch64", nil
		}
	case "darwin":
		return "yt-dlp_macos", nil
	}
	return "", fmt.Errorf("automatic tool installation is unsupported on %s/%s; put yt-dlp on your PATH", runtime.GOOS, runtime.GOARCH)
}

func (d *DownloadManager) findTool() (string, error) {
	name, err := d.toolName()
	if err == nil {
		local := filepath.Join(d.work, "_tools", name)
		if st, e := os.Stat(local); e == nil && !st.IsDir() {
			return local, nil
		}
	}
	if path, err := exec.LookPath("yt-dlp"); err == nil {
		return path, nil
	}
	if runtime.GOOS == "windows" {
		if path, err := exec.LookPath("yt-dlp.exe"); err == nil {
			return path, nil
		}
	}
	return "", errors.New("yt-dlp not installed: click Install yt-dlp first")
}

func (d *DownloadManager) status() map[string]any {
	d.mu.Lock()
	defer d.mu.Unlock()
	tool, err := d.findTool()
	_, ffErr := exec.LookPath("ffmpeg")
	logs := append([]string(nil), d.logs...)
	urls := append([]string(nil), d.urls...)
	return map[string]any{"active": d.active, "mode": d.mode, "message": d.message,
		"logs": logs, "logCounter": d.logCounter, "urls": urls, "started": d.started, "finished": d.finished,
		"exitCode": d.exitCode, "toolInstalled": err == nil, "toolPath": tool,
		"ffmpegInstalled": ffErr == nil, "incoming": filepath.Join(d.root, "_Incoming")}
}

func (d *DownloadManager) begin(mode string, urls []string) (context.Context, error) {
	d.mu.Lock()
	defer d.mu.Unlock()
	if d.active {
		return nil, errors.New("another download or installation is already running")
	}
	ctx, cancel := context.WithCancel(context.Background())
	d.cancel = cancel
	d.active = true
	d.mode = mode
	d.urls = append([]string(nil), urls...)
	d.started = time.Now().Format(time.RFC3339)
	d.finished = ""
	d.logs = nil
	d.logCounter = 0
	d.message = mode + " started"
	d.exitCode = 0
	return ctx, nil
}
func (d *DownloadManager) end(err error) {
	d.mu.Lock()
	defer d.mu.Unlock()
	d.active = false
	d.finished = time.Now().Format(time.RFC3339)
	d.cancel = nil
	if err != nil {
		d.message = err.Error()
		d.exitCode = 1
	} else {
		d.message = d.mode + " completed"
		d.exitCode = 0
	}
}
func (d *DownloadManager) stop() error {
	d.mu.Lock()
	defer d.mu.Unlock()
	if !d.active || d.cancel == nil {
		return errors.New("nothing is running")
	}
	d.cancel()
	d.message = "Cancellation requested"
	return nil
}

func parseURLs(text string) ([]string, error) {
	var urls []string
	seen := map[string]bool{}
	for _, line := range strings.Split(strings.ReplaceAll(text, "\r", ""), "\n") {
		line = strings.TrimSpace(line)
		if line == "" || strings.HasPrefix(line, "#") {
			continue
		}
		if len(urls) >= 100 {
			return nil, errors.New("maximum 100 URLs per batch; each URL can be a playlist")
		}
		u, e := url.Parse(line)
		if e != nil || u.Scheme != "https" || u.Hostname() == "" || u.User != nil {
			return nil, fmt.Errorf("invalid HTTPS media URL: %q", line)
		}
		host := strings.ToLower(u.Hostname())
		if host == "localhost" || strings.HasSuffix(host, ".localhost") || strings.HasSuffix(host, ".local") {
			return nil, errors.New("local URLs are not allowed")
		}
		if seen[line] {
			continue
		}
		seen[line] = true
		urls = append(urls, line)
	}
	if len(urls) == 0 {
		return nil, errors.New("enter at least one media URL, one per line")
	}
	return urls, nil
}
func (d *DownloadManager) buildArgs(opts DownloadOptions, urls []string) ([]string, error) {
	if opts.Quality == "" {
		opts.Quality = "native"
	}
	if opts.Browser == "" {
		opts.Browser = "none"
	}
	if opts.Scope == "" {
		opts.Scope = "playlist"
	}
	if opts.Quality != "native" && opts.Quality != "mp3" {
		return nil, errors.New("invalid quality choice")
	}
	browserOK := map[string]bool{"none": true, "firefox": true, "chrome": true, "chromium": true, "edge": true, "brave": true}
	if !browserOK[opts.Browser] {
		return nil, errors.New("unsupported browser selection")
	}
	if opts.Scope != "playlist" && opts.Scope != "single" {
		return nil, errors.New("invalid track/playlist choice")
	}
	if opts.Quality == "mp3" {
		if _, e := exec.LookPath("ffmpeg"); e != nil {
			return nil, errors.New("MP3 conversion requires FFmpeg; choose Best original audio instead")
		}
	}
	// Each argument is passed directly to yt-dlp (never through a shell).
	args := []string{"--ignore-config", "--newline", "--continue", "--no-overwrites", "--ignore-errors",
		"--download-archive", filepath.Join(d.work, "_downloads", "archive.txt"),
		"-P", filepath.Join(d.root, "_Incoming"),
		"-o", "%(artist,uploader)s - %(title)s [%(id)s].%(ext)s",
		"--extractor-args", "soundcloud:formats=*"}
	if opts.Scope == "single" {
		args = append(args, "--no-playlist")
	} else {
		args = append(args, "--yes-playlist")
	}
	if opts.Quality == "mp3" {
		args = append(args, "-f", "bestaudio/best", "-x", "--audio-format", "mp3", "--audio-quality", "0")
	} else {
		args = append(args, "-f", "bestaudio/best")
	}
	if opts.Browser != "none" {
		args = append(args, "--cookies-from-browser", opts.Browser)
	}
	args = append(args, "--")
	args = append(args, urls...)
	return args, nil
}

// ensureWorkspace refuses to use an unrelated existing folder named xxx.
func (d *DownloadManager) ensureWorkspace() error {
	marker := filepath.Join(d.work, ".music-organizer-owned")
	if st, err := os.Stat(d.work); err == nil {
		if !st.IsDir() {
			return errors.New("xxx is a file; choose another music directory")
		}
		if _, err := os.Stat(marker); err != nil {
			return errors.New("existing xxx folder is not owned by GatherForge; rename it before installing yt-dlp or downloading")
		}
		return nil
	} else if !os.IsNotExist(err) {
		return err
	}
	if err := os.MkdirAll(d.work, 0755); err != nil {
		return err
	}
	return os.WriteFile(marker, []byte("GatherForge: managed staging folder. Do not remove manually while a project is active.\n"), 0644)
}

func (d *DownloadManager) startDownload(opts DownloadOptions) error {
	urls, err := parseURLs(opts.URLs)
	if err != nil {
		return err
	}
	binary, err := d.findTool()
	if err != nil {
		return err
	}
	args, err := d.buildArgs(opts, urls)
	if err != nil {
		return err
	}
	ctx, err := d.begin("download", urls)
	if err != nil {
		return err
	}
	go func() {
		var result error
		defer func() { d.end(result) }()
		if e := os.MkdirAll(filepath.Join(d.work, "_downloads"), 0755); e != nil {
			result = e
			return
		}
		if e := os.MkdirAll(filepath.Join(d.root, "_Incoming"), 0755); e != nil {
			result = e
			return
		}
		cmd := exec.CommandContext(ctx, binary, args...)
		// Merge stdout and stderr into a single line-oriented feed.
		rd, wr, err := os.Pipe()
		if err != nil {
			result = err
			return
		}
		cmd.Stdout = wr
		cmd.Stderr = wr
		d.addLog("Downloading " + fmt.Sprint(len(urls)) + " URL(s) into _Incoming. Best available quality is not necessarily lossless.")
		if err := cmd.Start(); err != nil {
			_ = rd.Close()
			_ = wr.Close()
			result = err
			return
		}
		_ = wr.Close()
		done := make(chan struct{})
		go func() {
			s := bufio.NewScanner(rd)
			s.Buffer(make([]byte, 4096), 1<<20)
			for s.Scan() {
				d.addLog(s.Text())
			}
			if e := s.Err(); e != nil {
				d.addLog("Log stream: " + e.Error())
			}
			close(done)
		}()
		result = cmd.Wait()
		_ = rd.Close()
		<-done
		if ctx.Err() != nil {
			result = errors.New("download cancelled")
		}
		if result != nil {
			d.addLog("Download finished with errors: " + result.Error())
		} else {
			d.addLog("Download complete. Click Scan / rescan to review new audio.")
		}
	}()
	return nil
}

// installRelease verifies the yt-dlp binary against the SHA-256 list from the same official release.
func installRelease(ctx context.Context, baseURL, fileName, dest string, logf func(string)) error {
	client := &http.Client{Timeout: 5 * time.Minute}
	get := func(path string, limit int64) ([]byte, error) {
		req, e := http.NewRequestWithContext(ctx, "GET", strings.TrimRight(baseURL, "/")+"/"+path, nil)
		if e != nil {
			return nil, e
		}
		resp, e := client.Do(req)
		if e != nil {
			return nil, e
		}
		defer resp.Body.Close()
		if resp.StatusCode != 200 {
			return nil, fmt.Errorf("download %s returned HTTP %d", path, resp.StatusCode)
		}
		content, e := io.ReadAll(io.LimitReader(resp.Body, limit+1))
		if e != nil {
			return nil, e
		}
		if int64(len(content)) > limit {
			return nil, fmt.Errorf("file %s exceeds safety size limit", path)
		}
		return content, nil
	}
	logf("Fetching SHA-256 checksum manifest from official yt-dlp release...")
	sums, err := get("SHA2-256SUMS", 1024*1024)
	if err != nil {
		return err
	}
	checksum := ""
	for _, line := range strings.Split(string(sums), "\n") {
		fields := strings.Fields(line)
		if len(fields) >= 2 && strings.TrimPrefix(fields[1], "*") == fileName {
			checksum = strings.ToLower(fields[0])
			break
		}
	}
	if len(checksum) != 64 {
		return fmt.Errorf("no SHA-256 checksum for %s", fileName)
	}
	logf("Downloading " + fileName + " (first installation can be large)...")
	data, err := get(fileName, 150<<20)
	if err != nil {
		return err
	}
	digest := sha256.Sum256(data)
	if hex.EncodeToString(digest[:]) != checksum {
		return errors.New("SHA-256 mismatch: downloaded file rejected")
	}
	logf("Checksum verified.")
	if err = os.MkdirAll(filepath.Dir(dest), 0755); err != nil {
		return err
	}
	temp := dest + ".partial"
	if err = os.WriteFile(temp, data, 0755); err != nil {
		return err
	}
	if err = os.Chmod(temp, 0755); err != nil {
		_ = os.Remove(temp)
		return err
	}
	if err = os.Rename(temp, dest); err != nil {
		_ = os.Remove(temp)
		return err
	}
	logf("yt-dlp installed successfully: " + dest)
	return nil
}

func (d *DownloadManager) install() error {
	name, err := d.toolName()
	if err != nil {
		return err
	}
	ctx, err := d.begin("installation", nil)
	if err != nil {
		return err
	}
	go func() {
		dest := filepath.Join(d.work, "_tools", name)
		result := installRelease(ctx, releaseURL, name, dest, d.addLog)
		d.end(result)
	}()
	return nil
}
