package main

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"reflect"
	"runtime"
	"strings"
	"testing"
	"time"
)

func TestParseURLs(t *testing.T) {
	got, err := parseURLs("# ignore\n https://soundcloud.com/USERNAME/likes\nhttps://soundcloud.com/USERNAME/likes\n https://example.com/track \n")
	if err != nil {
		t.Fatal(err)
	}
	want := []string{"https://soundcloud.com/USERNAME/likes", "https://example.com/track"}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("want %v got %v", want, got)
	}
	for _, bad := range []string{"file:///etc/passwd", "http://soundcloud.com/a", "https://localhost/", "https://foo.local/song", "--rm /"} {
		if _, err := parseURLs(bad); err == nil {
			t.Errorf("should reject: %q", bad)
		}
	}
}
func TestArgsQualityAndSafety(t *testing.T) {
	d := &DownloadManager{root: t.TempDir(), work: filepath.Join(t.TempDir(), "xxx")}
	args, err := d.buildArgs(DownloadOptions{Quality: "native", Browser: "firefox", Scope: "playlist"}, []string{"https://soundcloud.com/user/sets/one", "https://soundcloud.com/user/sets/two"})
	if err != nil {
		t.Fatal(err)
	}
	joined := strings.Join(args, "|")
	for _, s := range []string{"--download-archive", "--yes-playlist", "--cookies-from-browser|firefox", "bestaudio/best", "--|https://soundcloud.com/user/sets/one|https://soundcloud.com/user/sets/two"} {
		if !strings.Contains(joined, s) {
			t.Errorf("missing %s in %q", s, joined)
		}
	}
	if strings.Contains(joined, "--audio-format|mp3") {
		t.Error("native should not convert audio")
	}
	if _, err := d.buildArgs(DownloadOptions{Browser: "evil"}, []string{"https://example.com"}); err == nil {
		t.Error("invalid browser accepted")
	}
}
func TestWorkspaceOwnership(t *testing.T) {
	root := t.TempDir()
	d := &DownloadManager{root: root, work: filepath.Join(root, "xxx")}
	if err := d.ensureWorkspace(); err != nil {
		t.Fatal(err)
	}
	if err := d.ensureWorkspace(); err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(filepath.Join(root, "xxx", ".music-organizer-owned")); err != nil {
		t.Fatal(err)
	}
	if err := os.Remove(filepath.Join(root, "xxx", ".music-organizer-owned")); err != nil {
		t.Fatal(err)
	}
	if err := d.ensureWorkspace(); err == nil {
		t.Error("should refuse an unowned folder")
	}
}
func TestInstallChecksumAndRejectTampering(t *testing.T) {
	payload := []byte("standalone yt-dlp binary fixture")
	digest := sha256.Sum256(payload)
	checksum := hex.EncodeToString(digest[:])
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/SHA2-256SUMS":
			fmt.Fprintln(w, checksum+"  yt-dlp_linux")
		case "/yt-dlp_linux":
			_, _ = w.Write(payload)
		case "/wrong":
			_, _ = w.Write([]byte("tampered"))
		default:
			http.NotFound(w, r)
		}
	}))
	defer server.Close()
	dest := filepath.Join(t.TempDir(), "yt-dlp_linux")
	if err := installRelease(context.Background(), server.URL, "yt-dlp_linux", dest, func(string) {}); err != nil {
		t.Fatal(err)
	}
	b, err := os.ReadFile(dest)
	if err != nil || !reflect.DeepEqual(b, payload) {
		t.Fatal("wrong installed binary")
	}
	if err := installRelease(context.Background(), server.URL, "wrong", dest, func(string) {}); err == nil {
		t.Error("tampered or unlisted file should be rejected")
	}
}
func TestDownloadIntegrationWithStub(t *testing.T) {
	if runtime.GOOS != "linux" || runtime.GOARCH != "amd64" {
		t.Skip("shell test for linux x64")
	}
	root := t.TempDir()
	d := &DownloadManager{root: root, work: filepath.Join(root, "xxx")}
	if err := d.ensureWorkspace(); err != nil {
		t.Fatal(err)
	}
	tool := filepath.Join(d.work, "_tools", "yt-dlp_linux")
	if err := os.MkdirAll(filepath.Dir(tool), 0755); err != nil {
		t.Fatal(err)
	}
	fixture := "#!/bin/sh\nout=''\nwhile [ $# -gt 0 ]; do\n if [ \"$1\" = '-P' ]; then shift; out=\"$1\"; fi\n shift\ndone\nmkdir -p \"$out\"\nprintf stub-audio > \"$out/DJ Foo - Song [123].mp3\"\necho '[download] done'\n"
	if err := os.WriteFile(tool, []byte(fixture), 0755); err != nil {
		t.Fatal(err)
	}
	err := d.startDownload(DownloadOptions{URLs: "https://soundcloud.com/artist/sets/one\nhttps://soundcloud.com/artist/sets/two", Quality: "native", Browser: "none", Scope: "playlist"})
	if err != nil {
		t.Fatal(err)
	}
	until := time.Now().Add(10 * time.Second)
	for d.status()["active"].(bool) && time.Now().Before(until) {
		time.Sleep(40 * time.Millisecond)
	}
	s := d.status()
	if s["active"].(bool) || s["exitCode"].(int) != 0 {
		t.Fatalf("unexpected download status: %v", s)
	}
	if _, err := os.Stat(filepath.Join(root, "_Incoming", "DJ Foo - Song [123].mp3")); err != nil {
		t.Fatal(err)
	}
	app, err := newApp(root)
	if err != nil {
		t.Fatal(err)
	}
	if err = app.scan(); err != nil {
		t.Fatal(err)
	}
	if len(app.state.Tracks) != 1 {
		t.Fatalf("expected 1 scanned incoming track, got %d", len(app.state.Tracks))
	}
	if !strings.Contains(app.state.Tracks[0].Original, "_Incoming") {
		t.Fatalf("wrong source: %v", app.state.Tracks[0].Original)
	}
	if err = app.stage(); err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(filepath.Join(root, "xxx", "_staging", "_Incoming", "DJ Foo - Song [123].mp3")); err != nil {
		t.Fatal(err)
	}
}
