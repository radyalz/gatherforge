package main

import (
	"archive/zip"
	"bytes"
	"crypto/rand"
	"crypto/sha256"
	"encoding/binary"
	"encoding/hex"
	"encoding/json"
	"encoding/xml"
	"errors"
	"fmt"
	"io"
	"log"
	"net"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"sort"
	"strconv"
	"strings"
	"sync"
	"time"
	"unicode"
	"unicode/utf16"
	"unicode/utf8"
)

const version = "1.1.0"

var extSet = map[string]bool{".mp3": true, ".mp2": true, ".flac": true, ".wav": true, ".wave": true, ".m4a": true, ".m4b": true, ".aac": true, ".ogg": true, ".oga": true, ".opus": true, ".wma": true, ".aif": true, ".aiff": true, ".ape": true, ".wv": true, ".alac": true, ".tta": true, ".dsf": true, ".dff": true, ".webm": true, ".mka": true}
var losslessSet = map[string]bool{".flac": true, ".wav": true, ".wave": true, ".aiff": true, ".aif": true, ".ape": true, ".wv": true, ".alac": true, ".dsf": true, ".dff": true}

type Track struct {
	ID          int    `json:"id"`
	Original    string `json:"original"`
	Stage       string `json:"stage"`
	Filename    string `json:"filename"`
	Artist      string `json:"artist"`
	RawArtist   string `json:"rawArtist"`
	Title       string `json:"title"`
	Source      string `json:"source"`
	Format      string `json:"format"`
	Bytes       int64  `json:"bytes"`
	Action      string `json:"action"` // keep, skip, exact, possible
	DuplicateOf int    `json:"duplicateOf"`
	Published   string `json:"published"`
	Error       string `json:"error,omitempty"`
}

type History struct {
	Published string `json:"published"`
	Artist    string `json:"artist"`
	Title     string `json:"title"`
	Bytes     int64  `json:"bytes"`
	Hash      string `json:"hash"`
}

type State struct {
	Version int               `json:"version"`
	Phase   string            `json:"phase"` // review, staging, staged, published, finalized
	Created string            `json:"created"`
	Tracks  []*Track          `json:"tracks"`
	Aliases map[string]string `json:"aliases"`
}

type App struct {
	mu         sync.Mutex
	root       string
	work       string
	token      string
	state      State
	busy       bool
	message    string
	downloader *DownloadManager
}

func norm(s string) string {
	return strings.ToLower(strings.Join(strings.Fields(strings.TrimSpace(s)), " "))
}
func lowerExt(path string) string { return strings.ToLower(filepath.Ext(path)) }
func folderName(s string) string {
	s = strings.TrimSpace(s)
	if s == "" {
		s = "Unknown Artist"
	}
	var b strings.Builder
	for _, r := range s {
		if r < 32 || strings.ContainsRune("<>:\"/\\|?*", r) {
			b.WriteRune('_')
		} else {
			b.WriteRune(r)
		}
	}
	t := strings.Trim(b.String(), " .")
	if t == "" {
		t = "Unknown Artist"
	}
	if utf8.RuneCountInString(t) > 100 {
		t = string([]rune(t)[:100])
	}
	if runtime.GOOS == "windows" {
		switch strings.ToUpper(t) {
		case "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "LPT1", "LPT2":
			t = "_" + t
		}
	}
	if t == "xxx" {
		t = "_xxx_artist"
	}
	return t
}
func mainArtist(s string) string {
	s = strings.TrimSpace(s)
	if s == "" {
		return "Unknown Artist"
	}
	// Preserve known artist names containing commas/ampersands.
	switch norm(s) {
	case "earth, wind & fire", "simon & garfunkel", "tyler, the creator", "ac/dc", "hall & oates", "mumford & sons", "flora cash":
		return folderName(s)
	}
	s = strings.ReplaceAll(s, "（", "(")
	low := strings.ToLower(s)
	cut := len(s)
	for _, sep := range []string{" feat. ", " feat ", " ft. ", " ft ", " featuring ", " with ", " vs. ", " vs ", " versus ", " x ", " & ", " + ", " / ", ";", ", ", " (feat", " [feat", " (ft", " [ft"} {
		if i := strings.Index(low, sep); i >= 0 && i < cut {
			cut = i
		}
	}
	return folderName(s[:cut])
}
func cleanTitle(s string) string {
	s = strings.TrimSpace(s)
	if i := strings.LastIndex(s, " ["); i >= 0 && strings.HasSuffix(s, "]") {
		body := s[i+2 : len(s)-1]
		if len(body) >= 5 {
			valid := true
			for _, r := range body {
				if !unicode.IsDigit(r) && !unicode.IsLetter(r) {
					valid = false
					break
				}
			}
			if valid {
				s = s[:i]
			}
		}
	}
	if s == "" {
		return "Untitled"
	}
	return s
}
func fallback(rel string) (string, string) {
	stem := strings.TrimSuffix(filepath.Base(rel), filepath.Ext(rel))
	if i := strings.Index(stem, " - "); i > 0 {
		return stem[:i], cleanTitle(stem[i+3:])
	}
	parts := strings.Split(filepath.ToSlash(rel), "/")
	if len(parts) > 1 {
		parent := parts[len(parts)-2]
		if parent != "" && parent != "_staging" {
			return parent, cleanTitle(stem)
		}
	}
	return "Unknown Artist", cleanTitle(stem)
}

// Audio metadata readers use only the Go standard library; unsupported tags fall back to filenames.
func latin1(data []byte) string {
	rr := make([]rune, len(data))
	for i, b := range data {
		rr[i] = rune(b)
	}
	return string(rr)
}
func textTag(data []byte) string {
	if len(data) == 0 {
		return ""
	}
	encoding := data[0]
	b := data[1:]
	switch encoding {
	case 0:
		return strings.TrimSpace(strings.Trim(latin1(b), "\x00"))
	case 3:
		return strings.TrimSpace(strings.Trim(string(b), "\x00"))
	case 1, 2:
		le := false
		if encoding == 1 && len(b) >= 2 {
			if b[0] == 0xff && b[1] == 0xfe {
				le = true
				b = b[2:]
			} else if b[0] == 0xfe && b[1] == 0xff {
				b = b[2:]
			}
		}
		var code []uint16
		for i := 0; i+1 < len(b); i += 2 {
			var n uint16
			if le {
				n = binary.LittleEndian.Uint16(b[i:])
			} else {
				n = binary.BigEndian.Uint16(b[i:])
			}
			if n == 0 {
				break
			}
			code = append(code, n)
		}
		return strings.TrimSpace(string(utf16.Decode(code)))
	}
	return strings.TrimSpace(string(b))
}
func syncsafe(b []byte) int {
	if len(b) < 4 {
		return 0
	}
	return int(b[0]&0x7f)<<21 | int(b[1]&0x7f)<<14 | int(b[2]&0x7f)<<7 | int(b[3]&0x7f)
}
func id3Tags(data []byte) (string, string) {
	if len(data) < 10 || string(data[:3]) != "ID3" {
		return "", ""
	}
	ver := data[3]
	size := syncsafe(data[6:10])
	end := 10 + size
	if end > len(data) {
		end = len(data)
	}
	artist, title := "", ""
	pos := 10
	for pos+10 <= end {
		frame := string(data[pos : pos+4])
		if data[pos] == 0 {
			break
		}
		sz := int(binary.BigEndian.Uint32(data[pos+4 : pos+8]))
		if ver == 4 {
			sz = syncsafe(data[pos+4 : pos+8])
		}
		if sz < 1 || pos+10+sz > end {
			break
		}
		val := textTag(data[pos+10 : pos+10+sz])
		switch frame {
		case "TPE1":
			if artist == "" {
				artist = val
			}
		case "TIT2":
			if title == "" {
				title = val
			}
		case "TPE2":
			if artist == "" {
				artist = val
			}
		}
		pos += 10 + sz
	}
	return artist, title
}
func id3v1(path string) (string, string) {
	f, e := os.Open(path)
	if e != nil {
		return "", ""
	}
	defer f.Close()
	stat, e := f.Stat()
	if e != nil || stat.Size() < 128 {
		return "", ""
	}
	var data [128]byte
	if _, e = f.ReadAt(data[:], stat.Size()-128); e != nil || string(data[:3]) != "TAG" {
		return "", ""
	}
	return strings.TrimSpace(strings.Trim(latin1(data[33:63]), "\x00")), strings.TrimSpace(strings.Trim(latin1(data[3:33]), "\x00"))
}
func vorbisBlock(b []byte) (string, string) {
	if len(b) < 8 {
		return "", ""
	}
	off := 0
	readStr := func() string {
		if off+4 > len(b) {
			return ""
		}
		size := int(binary.LittleEndian.Uint32(b[off:]))
		off += 4
		if size < 0 || size > len(b)-off {
			off = len(b)
			return ""
		}
		out := string(b[off : off+size])
		off += size
		return out
	}
	_ = readStr()
	if off+4 > len(b) {
		return "", ""
	}
	count := binary.LittleEndian.Uint32(b[off:])
	off += 4
	artist, title := "", ""
	for i := uint32(0); i < count && i < 2500 && off+4 <= len(b); i++ {
		val := readStr()
		k, v, ok := strings.Cut(val, "=")
		if !ok {
			continue
		}
		switch strings.ToUpper(k) {
		case "ARTIST", "ALBUMARTIST":
			if artist == "" {
				artist = v
			}
		case "TITLE":
			if title == "" {
				title = v
			}
		}
	}
	return artist, title
}
func flacTags(data []byte) (string, string) {
	if len(data) < 8 || string(data[:4]) != "fLaC" {
		return "", ""
	}
	p := 4
	for p+4 <= len(data) {
		header := data[p]
		size := int(data[p+1])<<16 | int(data[p+2])<<8 | int(data[p+3])
		p += 4
		if p+size > len(data) {
			break
		}
		if header&0x7f == 4 {
			return vorbisBlock(data[p : p+size])
		}
		p += size
		if header&0x80 != 0 {
			break
		}
	}
	return "", ""
}
func oggTags(data []byte) (string, string) {
	if i := bytes.Index(data, []byte("OpusTags")); i >= 0 {
		return vorbisBlock(data[i+8:])
	}
	if i := bytes.Index(data, []byte("\x03vorbis")); i >= 0 {
		return vorbisBlock(data[i+7:])
	}
	return "", ""
}
func mp4Tags(data []byte) (string, string) {
	// Search common iTunes metadata atoms (©ART, aART, ©nam) under ilst.
	artist, title := "", ""
	for _, atom := range []struct {
		key string
		out *string
	}{{"\xa9ART", &artist}, {"aART", &artist}, {"\xa9nam", &title}} {
		if *atom.out != "" {
			continue
		}
		target := []byte(atom.key)
		for pos := 0; pos+12 < len(data); {
			i := bytes.Index(data[pos:], target)
			if i < 0 {
				break
			}
			i += pos
			if i >= 4 {
				boxSize := int(binary.BigEndian.Uint32(data[i-4 : i]))
				boxEnd := i - 4 + boxSize
				if boxSize >= 24 && boxEnd <= len(data) {
					j := bytes.Index(data[i+4:boxEnd], []byte("data"))
					if j >= 4 {
						d := i + 4 + j + 12
						if d <= boxEnd {
							txt := strings.TrimSpace(strings.Trim(string(data[d:boxEnd]), "\x00"))
							if utf8.ValidString(txt) && txt != "" {
								*atom.out = txt
								break
							}
						}
					}
				}
			}
			pos = i + 4
		}
	}
	return artist, title
}
func wavTags(data []byte) (string, string) {
	artist, title := "", ""
	for i := 0; i+8 < len(data); i++ {
		key := string(data[i : i+4])
		if key != "IART" && key != "INAM" {
			continue
		}
		size := int(binary.LittleEndian.Uint32(data[i+4 : i+8]))
		if size < 1 || size > 1024 || i+8+size > len(data) {
			continue
		}
		v := strings.TrimSpace(strings.Trim(string(data[i+8:i+8+size]), "\x00"))
		if key == "IART" {
			artist = v
		} else {
			title = v
		}
	}
	return artist, title
}
func metadata(path, rel string) (string, string, string) {
	f, e := os.Open(path)
	if e != nil {
		a, t := fallback(rel)
		return a, t, "filename"
	}
	defer f.Close()
	max := int64(8 << 20)
	if s, e := f.Stat(); e == nil && s.Size() < max {
		max = s.Size()
	}
	data, _ := io.ReadAll(io.LimitReader(f, max))
	ext := lowerExt(path)
	artist, title := "", ""
	source := "filename"
	switch ext {
	case ".mp3", ".mp2", ".aac":
		artist, title = id3Tags(data)
		if artist == "" || title == "" {
			a, t := id3v1(path)
			if artist == "" {
				artist = a
			}
			if title == "" {
				title = t
			}
		}
	case ".flac":
		artist, title = flacTags(data)
	case ".opus", ".ogg", ".oga":
		artist, title = oggTags(data)
	case ".m4a", ".m4b", ".alac":
		artist, title = mp4Tags(data)
	case ".wav", ".wave":
		artist, title = wavTags(data)
	}
	fa, ft := fallback(rel)
	if artist != "" {
		source = "metadata"
	} else {
		artist = fa
	}
	if title == "" {
		title = ft
	}
	return mainArtist(artist), cleanTitle(title), source
}

func (a *App) loadHistory() []History {
	b, e := os.ReadFile(filepath.Join(a.work, "history.json"))
	if e != nil {
		return nil
	}
	var records []History
	if json.Unmarshal(b, &records) != nil {
		return nil
	}
	return records
}
func (a *App) newBatch() error {
	if a.state.Phase != "finalized" {
		return errors.New("complete and finalize the current batch first")
	}
	if e := a.owned(); e != nil {
		return e
	}
	history := a.loadHistory()
	seen := map[string]bool{}
	for _, h := range history {
		seen[norm(filepath.ToSlash(h.Published))] = true
	}
	for _, t := range a.state.Tracks {
		if t.Published == "" {
			continue
		}
		key := norm(filepath.ToSlash(t.Published))
		if seen[key] {
			continue
		}
		p := filepath.Join(a.root, t.Published)
		stat, e := os.Stat(p)
		if e != nil {
			return fmt.Errorf("published file missing: %s", p)
		}
		digest, e := hash(p)
		if e != nil {
			return e
		}
		history = append(history, History{Published: t.Published, Artist: t.Artist, Title: t.Title, Bytes: stat.Size(), Hash: digest})
		seen[key] = true
	}
	b, e := json.MarshalIndent(history, "", "  ")
	if e != nil {
		return e
	}
	tmp := filepath.Join(a.work, "history.json.tmp")
	if e = os.WriteFile(tmp, b, 0644); e != nil {
		return e
	}
	if e = os.Rename(tmp, filepath.Join(a.work, "history.json")); e != nil {
		return e
	}
	archive := filepath.Join(a.work, "_reports", "Previous_Batches")
	if e = os.MkdirAll(archive, 0755); e != nil {
		return e
	}
	backup, e := json.MarshalIndent(a.state, "", "  ")
	if e != nil {
		return e
	}
	name := fmt.Sprintf("batch-%s.json", time.Now().UTC().Format("20060102-150405.000000000"))
	if e = os.WriteFile(filepath.Join(archive, name), backup, 0644); e != nil {
		return e
	}
	a.state = newState()
	return a.scan()
}

func newState() State {
	return State{Version: 1, Phase: "review", Created: time.Now().Format(time.RFC3339), Aliases: map[string]string{}, Tracks: []*Track{}}
}
func newApp(root string) (*App, error) {
	var token [18]byte
	_, _ = rand.Read(token[:])
	a := &App{root: root, work: filepath.Join(root, "xxx"), token: hex.EncodeToString(token[:]), state: newState()}
	a.downloader = &DownloadManager{root: root, work: a.work}
	if _, e := os.Stat(filepath.Join(a.work, "state.json")); e == nil {
		b, e := os.ReadFile(filepath.Join(a.work, "state.json"))
		if e != nil {
			return nil, e
		}
		if e = json.Unmarshal(b, &a.state); e != nil {
			return nil, fmt.Errorf("invalid xxx/state.json: %w", e)
		}
		if a.state.Aliases == nil {
			a.state.Aliases = map[string]string{}
		}
	}
	return a, nil
}
func (a *App) owned() error {
	if f, e := os.Stat(a.work); e == nil {
		if !f.IsDir() {
			return errors.New("xxx exists as a file")
		}
		if _, e = os.Stat(filepath.Join(a.work, ".music-organizer-owned")); e != nil {
			return errors.New("existing xxx folder is not owned by this app; rename it before staging")
		}
	}
	return os.MkdirAll(a.work, 0755)
}
func (a *App) save() error {
	if e := a.owned(); e != nil {
		return e
	}
	marker := filepath.Join(a.work, ".music-organizer-owned")
	if _, e := os.Stat(marker); os.IsNotExist(e) {
		if e = os.WriteFile(marker, []byte("GatherForge: managed staging folder. Do not remove manually while a project is active.\n"), 0644); e != nil {
			return e
		}
	}
	b, e := json.MarshalIndent(a.state, "", "  ")
	if e != nil {
		return e
	}
	tmp := filepath.Join(a.work, "state.json.tmp")
	if e = os.WriteFile(tmp, b, 0644); e != nil {
		return e
	}
	return os.Rename(tmp, filepath.Join(a.work, "state.json"))
}
func (a *App) scan() error {
	if a.state.Phase != "review" {
		return errors.New("finish or restore the existing staged project first")
	}
	// Scan-only makes NO file changes; user can inspect before staging.
	var tracks []*Track
	history := a.loadHistory()
	prior := map[string]History{}
	for _, h := range history {
		prior[norm(filepath.ToSlash(h.Published))] = h
	}
	e := filepath.WalkDir(a.root, func(path string, entry os.DirEntry, err error) error {
		if err != nil {
			return nil
		}
		if entry.IsDir() {
			if path == a.work || entry.Name() == ".git" || entry.Name() == "node_modules" {
				return filepath.SkipDir
			}
			if entry.Type()&os.ModeSymlink != 0 {
				return filepath.SkipDir
			}
			return nil
		}
		if !entry.Type().IsRegular() || !extSet[lowerExt(entry.Name())] {
			return nil
		}
		rel, e := filepath.Rel(a.root, path)
		if e != nil {
			return nil
		}
		stat, e := entry.Info()
		if e != nil {
			return nil
		}
		if h, ok := prior[norm(filepath.ToSlash(rel))]; ok && h.Bytes == stat.Size() {
			return nil
		}
		artist, title, source := metadata(path, rel)
		tracks = append(tracks, &Track{ID: len(tracks) + 1, Original: rel, Stage: filepath.Join("_staging", rel), Filename: entry.Name(), Artist: artist, RawArtist: artist, Title: title, Source: source, Format: lowerExt(path), Bytes: stat.Size(), Action: "keep"})
		return nil
	})
	if e != nil {
		return e
	}
	sort.Slice(tracks, func(i, j int) bool { return strings.ToLower(tracks[i].Original) < strings.ToLower(tracks[j].Original) })
	for i, t := range tracks {
		t.ID = i + 1
		if folder, ok := a.state.Aliases[norm(t.Artist)]; ok {
			t.Artist = folder
		}
	}
	a.state.Tracks = tracks
	a.classify()
	if len(history) > 0 {
		byKey := map[string]History{}
		for _, h := range history {
			byKey[norm(h.Artist)+"\x00"+norm(h.Title)] = h
		}
		for _, t := range a.state.Tracks {
			if t.Action == "keep" {
				if h, ok := byKey[norm(t.Artist)+"\x00"+norm(t.Title)]; ok {
					t.Action = "possible"
					t.DuplicateOf = -1
					t.Error = "Also in published library: " + h.Published
				}
			}
		}
	}
	// Normalize case variants into a single folder, even on case-sensitive filesystems.
	canon := map[string]string{}
	for _, t := range a.state.Tracks {
		key := norm(t.Artist)
		if prev, ok := canon[key]; ok {
			t.Artist = prev
		} else {
			canon[key] = t.Artist
		}
	}
	return a.save()
}
func (a *App) classify() {
	tracks := a.state.Tracks
	// Prefer lossless, then file size when two tracks have matching normalized artist/title.
	rank := func(t *Track) int64 {
		n := t.Bytes
		if losslessSet[t.Format] {
			n += (1 << 50)
		}
		return n
	}
	byKey := map[string][]*Track{}
	for _, t := range tracks {
		t.DuplicateOf = 0
		if t.Action != "skip" {
			t.Action = "keep"
		}
		key := norm(t.Artist) + "\x00" + norm(t.Title)
		byKey[key] = append(byKey[key], t)
	}
	for _, group := range byKey {
		if len(group) < 2 {
			continue
		}
		sort.Slice(group, func(i, j int) bool { return rank(group[i]) > rank(group[j]) })
		for _, t := range group[1:] {
			t.Action = "possible"
			t.DuplicateOf = group[0].ID
		}
	}
	// For equal sized files, SHA256 is sufficient to prove exact file duplicates.
	bySize := map[int64][]*Track{}
	for _, t := range tracks {
		bySize[t.Bytes] = append(bySize[t.Bytes], t)
	}
	digests := map[string]*Track{}
	for _, group := range bySize {
		if len(group) < 2 {
			continue
		}
		sort.Slice(group, func(i, j int) bool { return group[i].ID < group[j].ID })
		for _, t := range group {
			path := filepath.Join(a.root, t.Original)
			if a.state.Phase != "review" {
				path = filepath.Join(a.work, t.Stage)
			}
			digest, e := hash(path)
			if e != nil {
				continue
			}
			if orig, ok := digests[digest]; ok {
				t.Action = "exact"
				t.DuplicateOf = orig.ID
			} else {
				digests[digest] = t
			}
		}
	}
}
func hash(path string) (string, error) {
	f, e := os.Open(path)
	if e != nil {
		return "", e
	}
	defer f.Close()
	h := sha256.New()
	_, e = io.CopyBuffer(h, f, make([]byte, 1024*1024))
	if e != nil {
		return "", e
	}
	return hex.EncodeToString(h.Sum(nil)), nil
}
func exists(path string) bool { _, e := os.Stat(path); return e == nil }
func (a *App) stage() error {
	if a.state.Phase != "review" && a.state.Phase != "staging" {
		return errors.New("stage is only allowed from review")
	}
	if len(a.state.Tracks) == 0 {
		return errors.New("scan music files first")
	}
	if e := a.owned(); e != nil {
		return e
	}
	a.state.Phase = "staging"
	if e := a.save(); e != nil {
		return e
	}
	for _, t := range a.state.Tracks {
		source := filepath.Join(a.root, t.Original)
		target := filepath.Join(a.work, t.Stage)
		if exists(target) {
			if exists(source) {
				return fmt.Errorf("both original and staging file exist for %s; resolve manually", t.Original)
			}
			continue
		}
		if e := os.MkdirAll(filepath.Dir(target), 0755); e != nil {
			return e
		}
		if e := os.Rename(source, target); e != nil {
			// Moves across partitions require copy+verify. Within same root this is typically atomic.
			return fmt.Errorf("could not stage %s: %w", t.Original, e)
		}
	}
	a.state.Phase = "staged"
	return a.save()
}
func (a *App) restore() error {
	if a.state.Phase != "staged" && a.state.Phase != "staging" {
		return errors.New("restore only allowed before publishing")
	}
	for _, t := range a.state.Tracks {
		source := filepath.Join(a.work, t.Stage)
		target := filepath.Join(a.root, t.Original)
		if !exists(source) {
			if exists(target) {
				continue
			}
			return fmt.Errorf("missing staged file %s", t.Stage)
		}
		if exists(target) {
			return fmt.Errorf("original path is occupied: %s", target)
		}
		if e := os.MkdirAll(filepath.Dir(target), 0755); e != nil {
			return e
		}
		if e := os.Rename(source, target); e != nil {
			return e
		}
	}
	a.state.Phase = "review"
	return a.save()
}
func makeUnique(target string) (string, error) {
	dir := filepath.Dir(target)
	if e := os.MkdirAll(dir, 0755); e != nil {
		return "", e
	}
	candidate := target
	for i := 2; exists(candidate); i++ {
		ext := filepath.Ext(target)
		candidate = strings.TrimSuffix(target, ext) + fmt.Sprintf(" (%d)", i) + ext
	}
	return candidate, nil
}
func copyAtomic(src, dst string) error {
	in, e := os.Open(src)
	if e != nil {
		return e
	}
	defer in.Close()
	var random [8]byte
	_, _ = rand.Read(random[:])
	tmp := dst + ".part-" + hex.EncodeToString(random[:])
	out, e := os.OpenFile(tmp, os.O_WRONLY|os.O_CREATE|os.O_EXCL, 0644)
	if e != nil {
		return e
	}
	_, e = io.CopyBuffer(out, in, make([]byte, 1<<20))
	if e2 := out.Sync(); e == nil {
		e = e2
	}
	if e2 := out.Close(); e == nil {
		e = e2
	}
	if e != nil {
		_ = os.Remove(tmp)
		return e
	}
	if e = os.Rename(tmp, dst); e != nil {
		_ = os.Remove(tmp)
		return e
	}
	return nil
}
func (a *App) publish() error {
	if a.state.Phase != "staged" && a.state.Phase != "published" {
		return errors.New("stage the files first")
	}
	for _, t := range a.state.Tracks {
		if t.Action == "possible" {
			return fmt.Errorf("possible duplicate #%d needs a Keep or Skip decision before publishing", t.ID)
		}
	}
	for _, t := range a.state.Tracks {
		if t.Action != "keep" {
			continue
		}
		source := filepath.Join(a.work, t.Stage)
		if !exists(source) {
			return fmt.Errorf("staged file missing: %s", t.Stage)
		}
		if t.Published != "" {
			p := filepath.Join(a.root, t.Published)
			if exists(p) {
				srcH, e1 := hash(source)
				dstH, e2 := hash(p)
				if e1 == nil && e2 == nil && srcH == dstH {
					continue
				}
			}
			return fmt.Errorf("published file is missing or changed: %s", t.Published)
		}
		baseFolder := folderName(t.Artist)
		if baseFolder == "xxx" {
			return errors.New("invalid destination folder")
		}
		targetDir := filepath.Join(a.root, baseFolder)
		filename := folderName(strings.TrimSuffix(t.Filename, filepath.Ext(t.Filename))) + t.Format
		target, e := makeUnique(filepath.Join(targetDir, filename))
		if e != nil {
			return e
		}
		if e = copyAtomic(source, target); e != nil {
			return e
		}
		rel, _ := filepath.Rel(a.root, target)
		t.Published = rel
		if e = a.save(); e != nil {
			return e
		}
	}
	a.state.Phase = "published"
	return a.save()
}
func (a *App) finalize() error {
	if a.state.Phase != "published" && a.state.Phase != "finalizing" {
		return errors.New("publish first; cleanup is never automatic")
	}
	// Do a complete SHA-256 verification BEFORE cleanup starts. The recorded
	// "finalizing" phase makes a partially completed cleanup safely resumable.
	for _, t := range a.state.Tracks {
		if t.Action != "keep" {
			continue
		}
		if t.Published == "" {
			return fmt.Errorf("track %d not published", t.ID)
		}
		dst := filepath.Join(a.root, t.Published)
		if !exists(dst) {
			return fmt.Errorf("published file missing: %s", dst)
		}
		src := filepath.Join(a.work, t.Stage)
		if !exists(src) {
			if a.state.Phase != "finalizing" {
				return fmt.Errorf("staged file missing before verification: %s", t.Stage)
			}
			continue
		}
		h1, e := hash(src)
		if e != nil {
			return e
		}
		h2, e := hash(dst)
		if e != nil {
			return e
		}
		if h1 != h2 {
			return fmt.Errorf("published file differs from staging: %s", t.Published)
		}
	}
	a.state.Phase = "finalizing"
	if e := a.save(); e != nil {
		return e
	}
	// Published songs are already verified; only these copies are removed.
	// Everything not published is archived without deletion.
	for _, t := range a.state.Tracks {
		if strings.HasPrefix(filepath.ToSlash(t.Stage), "_Not_Published/") {
			continue
		}
		src := filepath.Join(a.work, t.Stage)
		if !exists(src) {
			continue
		}
		if t.Action == "keep" {
			if e := os.Remove(src); e != nil {
				return e
			}
		} else {
			dst, e := makeUnique(filepath.Join(a.work, "_Not_Published", folderName(t.Action), t.Filename))
			if e != nil {
				return e
			}
			if e = os.Rename(src, dst); e != nil {
				return e
			}
			t.Stage = dst[len(a.work)+1:]
		}
		if e := a.save(); e != nil {
			return e
		}
	}
	a.state.Phase = "finalized"
	return a.save()
}
func (a *App) update(changes []map[string]any, aliases map[string]string) error {
	if a.state.Phase == "finalized" {
		return errors.New("project already finalized")
	}
	if a.state.Phase == "published" || a.state.Phase == "finalizing" {
		return errors.New("folder names and choices cannot change after publishing")
	}
	for old, newName := range aliases {
		if strings.TrimSpace(old) == "" || strings.TrimSpace(newName) == "" {
			continue
		}
		newName = folderName(newName)
		a.state.Aliases[norm(old)] = newName
		for _, t := range a.state.Tracks {
			if norm(t.Artist) == norm(old) || norm(t.RawArtist) == norm(old) {
				t.Artist = newName
			}
		}
	}
	for _, change := range changes {
		id, e := parseID(change["id"])
		if e != nil {
			continue
		}
		for _, t := range a.state.Tracks {
			if t.ID != id {
				continue
			}
			if v, ok := change["artist"].(string); ok && strings.TrimSpace(v) != "" {
				t.Artist = folderName(v)
			}
			if v, ok := change["action"].(string); ok && (v == "keep" || v == "skip" || v == "possible" || v == "exact") {
				t.Action = v
			}
			break
		}
	}
	return a.save()
}
func parseID(v any) (int, error) {
	switch x := v.(type) {
	case float64:
		return int(x), nil
	case int:
		return x, nil
	case string:
		return strconv.Atoi(x)
	}
	return 0, errors.New("not an id")
}
func (a *App) status() any {
	folders := map[string]int{}
	counts := map[string]int{"keep": 0, "skip": 0, "possible": 0, "exact": 0, "flagged": 0}
	for _, t := range a.state.Tracks {
		counts[t.Action]++
		if t.DuplicateOf != 0 {
			counts["flagged"]++
		}
		if t.Action == "keep" {
			folders[t.Artist]++
		}
	}
	return map[string]any{"root": a.root, "work": a.work, "phase": a.state.Phase, "tracks": a.state.Tracks, "folders": folders, "counts": counts, "busy": a.busy, "message": a.message, "version": version}
}

func xmlEscape(s string) string {
	var clean strings.Builder
	for _, r := range s {
		if r == 9 || r == 10 || r == 13 || r >= 32 {
			clean.WriteRune(r)
		}
	}
	var b bytes.Buffer
	_ = xml.EscapeText(&b, []byte(clean.String()))
	return b.String()
}
func excelCol(i int) string {
	s := ""
	for i > 0 {
		i--
		s = string(rune('A'+i%26)) + s
		i /= 26
	}
	return s
}

type Sheet struct {
	name    string
	headers []string
	rows    [][]string
}

func sheetXML(s Sheet) []byte {
	var b strings.Builder
	b.WriteString(`<?xml version="1.0" encoding="UTF-8" standalone="yes"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetViews><sheetView workbookViewId="0"><pane ySplit="1" topLeftCell="A2" activePane="bottomLeft" state="frozen"/></sheetView></sheetViews><cols>`)
	for i, h := range s.headers {
		w := len(h) + 4
		if w < 14 {
			w = 14
		}
		if w > 56 {
			w = 56
		}
		b.WriteString(fmt.Sprintf(`<col min="%d" max="%d" width="%d" customWidth="1"/>`, i+1, i+1, w))
	}
	b.WriteString(`</cols><sheetData>`)
	rows := append([][]string{s.headers}, s.rows...)
	for y, row := range rows {
		b.WriteString(fmt.Sprintf(`<row r="%d">`, y+1))
		for x, v := range row {
			style := 0
			if y == 0 {
				style = 1
			}
			addr := fmt.Sprintf("%s%d", excelCol(x+1), y+1)
			b.WriteString(fmt.Sprintf(`<c r="%s" s="%d" t="inlineStr"><is><t xml:space="preserve">%s</t></is></c>`, addr, style, xmlEscape(v)))
		}
		b.WriteString(`</row>`)
	}
	b.WriteString(`</sheetData>`)
	if len(s.headers) > 0 {
		b.WriteString(fmt.Sprintf(`<autoFilter ref="A1:%s%d"/>`, excelCol(len(s.headers)), len(rows)))
	}
	b.WriteString(`</worksheet>`)
	return []byte(b.String())
}
func writeWorkbook(path string, sheets []Sheet) error {
	if e := os.MkdirAll(filepath.Dir(path), 0755); e != nil {
		return e
	}
	f, e := os.Create(path)
	if e != nil {
		return e
	}
	z := zip.NewWriter(f)
	add := func(path string, body []byte) error {
		w, e := z.Create(path)
		if e != nil {
			return e
		}
		_, e = w.Write(body)
		return e
	}
	names := []string{}
	for i, s := range sheets {
		if len(s.name) > 31 {
			s.name = s.name[:31]
		}
		names = append(names, fmt.Sprintf(`<sheet name="%s" sheetId="%d" r:id="rId%d"/>`, xmlEscape(s.name), i+1, i+1))
	}
	workbook := `<?xml version="1.0" encoding="UTF-8" standalone="yes"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets>` + strings.Join(names, "") + `</sheets></workbook>`
	rels := []string{}
	for i := range sheets {
		rels = append(rels, fmt.Sprintf(`<Relationship Id="rId%d" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet%d.xml"/>`, i+1, i+1))
	}
	rels = append(rels, fmt.Sprintf(`<Relationship Id="rId%d" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>`, len(sheets)+1))
	content := []string{`<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>`, `<Default Extension="xml" ContentType="application/xml"/>`, `<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>`, `<Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/>`}
	for i := range sheets {
		content = append(content, fmt.Sprintf(`<Override PartName="/xl/worksheets/sheet%d.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>`, i+1))
	}
	styles := `<?xml version="1.0" encoding="UTF-8" standalone="yes"?><styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><fonts count="2"><font><sz val="11"/><name val="Aptos"/></font><font><b/><color rgb="FFFFFFFF"/><sz val="11"/><name val="Aptos"/></font></fonts><fills count="3"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill><fill><patternFill patternType="solid"><fgColor rgb="FF183153"/><bgColor indexed="64"/></patternFill></fill></fills><borders count="1"><border><left/><right/><top/><bottom/><diagonal/></border></borders><cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs><cellXfs count="2"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/><xf numFmtId="0" fontId="1" fillId="2" borderId="0" xfId="0" applyFont="1" applyFill="1"/></cellXfs></styleSheet>`
	entries := map[string][]byte{"[Content_Types].xml": []byte(`<?xml version="1.0" encoding="UTF-8"?>` + `<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">` + strings.Join(content, "") + `</Types>`), "_rels/.rels": []byte(`<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>`), "xl/workbook.xml": []byte(workbook), "xl/_rels/workbook.xml.rels": []byte(`<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">` + strings.Join(rels, "") + `</Relationships>`), "xl/styles.xml": []byte(styles)}
	for i, s := range sheets {
		entries[fmt.Sprintf("xl/worksheets/sheet%d.xml", i+1)] = sheetXML(s)
	}
	keys := make([]string, 0, len(entries))
	for k := range entries {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	for _, k := range keys {
		if e = add(k, entries[k]); e != nil {
			_ = z.Close()
			_ = f.Close()
			return e
		}
	}
	if e = z.Close(); e != nil {
		_ = f.Close()
		return e
	}
	return f.Close()
}
func (a *App) reports() (string, error) {
	if e := a.owned(); e != nil {
		return "", e
	}
	if e := a.save(); e != nil {
		return "", e
	}
	dir := filepath.Join(a.work, "_reports")
	sheetTracks := Sheet{"Tracks", []string{"ID", "Primary artist / folder", "Song title", "Action", "Duplicate of ID", "Format", "Size (bytes)", "Tag source", "Original relative path", "Staging full path", "Published relative path", "Notes"}, [][]string{}}
	sheetDup := Sheet{"Duplicates", []string{"ID", "Primary artist", "Song", "Duplicate status", "Compare with ID", "Original relative path", "Staging full path", "Selected action", "Notes"}, [][]string{}}
	sheetUncertain := Sheet{"Needs review", []string{"ID", "Reason", "Primary artist", "Song", "Original relative path", "Staging full path"}, [][]string{}}
	byArtist := map[string][]*Track{}
	counts := map[string][3]int{}
	for _, t := range a.state.Tracks {
		stage := filepath.Join(a.work, t.Stage)
		sheetTracks.rows = append(sheetTracks.rows, []string{strconv.Itoa(t.ID), t.Artist, t.Title, t.Action, strconv.Itoa(t.DuplicateOf), t.Format, strconv.FormatInt(t.Bytes, 10), t.Source, t.Original, stage, t.Published, t.Error})
		if t.DuplicateOf != 0 || t.Action == "exact" || t.Action == "possible" {
			sheetDup.rows = append(sheetDup.rows, []string{strconv.Itoa(t.ID), t.Artist, t.Title, t.Action, strconv.Itoa(t.DuplicateOf), t.Original, stage, t.Action, t.Error})
		}
		if t.Source == "filename" || t.Action == "possible" {
			reason := "artist inferred from filename/folder"
			if t.Action == "possible" {
				reason = "possible duplicate (review before publishing)"
			}
			sheetUncertain.rows = append(sheetUncertain.rows, []string{strconv.Itoa(t.ID), reason, t.Artist, t.Title, t.Original, stage})
		}
		byArtist[t.Artist] = append(byArtist[t.Artist], t)
		x := counts[t.Artist]
		x[0]++
		if t.Action == "keep" {
			x[1]++
		}
		if t.Action == "exact" || t.Action == "possible" {
			x[2]++
		}
		counts[t.Artist] = x
	}
	sheetArtists := Sheet{"Artists", []string{"Artist folder", "All tracks", "Selected to publish", "Potential/exact duplicates", "Staging directory"}, [][]string{}}
	artists := make([]string, 0, len(byArtist))
	for k := range byArtist {
		artists = append(artists, k)
	}
	sort.Slice(artists, func(i, j int) bool { return norm(artists[i]) < norm(artists[j]) })
	for _, artist := range artists {
		v := counts[artist]
		sheetArtists.rows = append(sheetArtists.rows, []string{artist, strconv.Itoa(v[0]), strconv.Itoa(v[1]), strconv.Itoa(v[2]), filepath.Join(a.work, "_staging")})
	}
	path := filepath.Join(dir, "Music_Review.xlsx")
	if e := writeWorkbook(path, []Sheet{sheetArtists, sheetTracks, sheetDup, sheetUncertain}); e != nil {
		return "", e
	}
	// One Excel file per artist for convenient focused review.
	for i, artist := range artists {
		ts := byArtist[artist]
		rows := [][]string{}
		for _, t := range ts {
			rows = append(rows, []string{strconv.Itoa(t.ID), t.Title, t.Action, t.Format, t.Original, filepath.Join(a.work, t.Stage), strconv.Itoa(t.DuplicateOf)})
		}
		name := fmt.Sprintf("%03d_%s.xlsx", i+1, folderName(artist))
		if e := writeWorkbook(filepath.Join(dir, "Artists", name), []Sheet{{"Tracks", []string{"ID", "Song", "Action", "Format", "Original path", "Staging source", "Duplicate of ID"}, rows}}); e != nil {
			return "", e
		}
	}
	return path, nil
}

func apiError(w http.ResponseWriter, code int, msg string) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(code)
	_ = json.NewEncoder(w).Encode(map[string]any{"ok": false, "error": msg})
}
func apiJSON(w http.ResponseWriter, result any) {
	w.Header().Set("Content-Type", "application/json")
	_ = json.NewEncoder(w).Encode(result)
}
func (a *App) serve() error {
	mux := http.NewServeMux()
	mux.HandleFunc("/", func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/" {
			http.NotFound(w, r)
			return
		}
		w.Header().Set("Content-Type", "text/html; charset=utf-8")
		_, _ = io.WriteString(w, strings.ReplaceAll(htmlUI, "__TOKEN__", a.token))
	})
	mux.HandleFunc("/api/state", func(w http.ResponseWriter, r *http.Request) { a.mu.Lock(); defer a.mu.Unlock(); apiJSON(w, a.status()) })
	// Downloader actions run asynchronously so long playlist downloads never block the UI.
	mux.HandleFunc("/api/downloader", func(w http.ResponseWriter, r *http.Request) {
		if r.Method != "GET" {
			apiError(w, 405, "GET required")
			return
		}
		apiJSON(w, a.downloader.status())
	})
	mux.HandleFunc("/api/downloader/action", func(w http.ResponseWriter, r *http.Request) {
		if r.Method != "POST" {
			apiError(w, 405, "POST required")
			return
		}
		if r.Header.Get("X-Organizer-Token") != a.token {
			apiError(w, 403, "Invalid session token")
			return
		}
		r.Body = http.MaxBytesReader(w, r.Body, 1<<18)
		var req struct {
			Command string          `json:"command"`
			Options DownloadOptions `json:"options"`
		}
		if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
			apiError(w, 400, "Invalid request")
			return
		}
		var err error
		switch req.Command {
		case "install":
			err = a.downloader.install()
		case "start":
			a.mu.Lock()
			if a.busy {
				err = errors.New("Wait for the organizer operation to finish")
			} else {
				err = a.downloader.startDownload(req.Options)
			}
			a.mu.Unlock()
		case "stop":
			err = a.downloader.stop()
		default:
			err = errors.New("Unknown downloader command")
		}
		if err != nil {
			apiError(w, 400, err.Error())
			return
		}
		apiJSON(w, a.downloader.status())
	})
	mux.HandleFunc("/api/do", func(w http.ResponseWriter, r *http.Request) {
		if r.Method != "POST" {
			apiError(w, 405, "POST required")
			return
		}
		if r.Header.Get("X-Organizer-Token") != a.token {
			apiError(w, 403, "Invalid session token")
			return
		}
		var req struct {
			Command string            `json:"command"`
			Changes []map[string]any  `json:"changes"`
			Aliases map[string]string `json:"aliases"`
		}
		r.Body = http.MaxBytesReader(w, r.Body, 1<<20)
		if e := json.NewDecoder(r.Body).Decode(&req); e != nil {
			apiError(w, 400, "Invalid request")
			return
		}
		a.mu.Lock()
		if req.Command != "update" && req.Command != "export" && a.downloader.status()["active"].(bool) {
			a.mu.Unlock()
			apiError(w, 409, "Wait for the download to finish before changing the library")
			return
		}
		if a.busy {
			a.mu.Unlock()
			apiError(w, 409, "Another operation is in progress")
			return
		}
		a.busy = true
		a.mu.Unlock()
		var err error
		var info string
		a.mu.Lock()
		switch req.Command {
		case "scan":
			err = a.scan()
			info = "Scan completed"
		case "newbatch":
			err = a.newBatch()
			info = "New batch started; previously published songs excluded"
		case "stage":
			err = a.stage()
			info = "Original tracks are safely staged"
		case "restore":
			err = a.restore()
			info = "Staged tracks restored to original paths"
		case "publish":
			err = a.publish()
			info = "Approved tracks copied into artist folders; staging preserved"
		case "finalize":
			err = a.finalize()
			info = "Published tracks verified; leftover duplicates archived"
		case "export":
			info, err = a.reports()
		case "update":
			err = a.update(req.Changes, req.Aliases)
			info = "Saved changes"
		default:
			err = errors.New("Unknown command")
		}
		a.busy = false
		if err != nil {
			a.message = err.Error()
		} else {
			a.message = info
		}
		result := a.status()
		a.mu.Unlock()
		if err != nil {
			apiError(w, 400, err.Error())
			return
		}
		apiJSON(w, result)
	})
	listener, e := net.Listen("tcp", "127.0.0.1:0")
	if e != nil {
		return e
	}
	url := "http://" + listener.Addr().String() + "/"
	fmt.Printf("\nGatherForge %s\nFolder: %s\nOpen: %s\nClose this terminal / Ctrl+C to stop.\n\n", version, a.root, url)
	openBrowser(url)
	return http.Serve(listener, mux)
}
func openBrowser(url string) {
	var c *exec.Cmd
	switch runtime.GOOS {
	case "windows":
		c = exec.Command("rundll32", "url.dll,FileProtocolHandler", url)
	case "darwin":
		c = exec.Command("open", url)
	default:
		c = exec.Command("xdg-open", url)
	}
	if e := c.Start(); e != nil {
		fmt.Println("Could not open browser automatically; visit", url)
	}
}
func printUsage() {
	fmt.Print(`GatherForge — self-contained, no Python/pip/FFmpeg needed

COPY THIS EXECUTABLE into any music folder and launch it.
It operates on that executable's folder, not your current terminal directory.

Commands:
  (no arguments)       launch the browser-based organizer
  --scan               scan the folder (no music moved)
  --new-batch          start another batch after finalizing the current one
  --stage              move originals into xxx/_staging
  --export             create xxx/_reports/Music_Review.xlsx + per-artist workbooks
  --publish            copy selected songs to top-level artist folders (staging stays)
  --restore            undo staging (only before publishing)
  --finalize           verify copies, remove published staging copies, archive unused
  --status             print track counts
  --install-yt-dlp     install official, SHA256-verified yt-dlp into xxx/_tools
  --download URL ...   download one track or multiple playlists (best native audio)
  --help               print this help

KEYS IN BROWSER: / search, S scan, G stage, E export, P publish, ? shortcuts.
SAFETY: all operations require manual initiation; stage & publish are separated.
`)
}
func waitForDownloader(d *DownloadManager) {
	var shown int64
	for {
		s := d.status()
		logs := s["logs"].([]string)
		count := s["logCounter"].(int64)
		start := shown - (count - int64(len(logs)))
		if start < 0 {
			start = 0
		}
		if start > int64(len(logs)) {
			start = int64(len(logs))
		}
		for _, line := range logs[start:] {
			fmt.Println(line)
		}
		shown = count
		if !s["active"].(bool) {
			fmt.Println(s["message"])
			if s["exitCode"].(int) != 0 {
				os.Exit(1)
			}
			return
		}
		time.Sleep(300 * time.Millisecond)
	}
}

func main() {
	exe, e := os.Executable()
	if e != nil {
		log.Fatal(e)
	}
	// NOTE: Resolve symlink in order to identify the real directory holding the executable.
	exe, e = filepath.EvalSymlinks(exe)
	if e != nil {
		log.Fatal(e)
	}
	a, e := newApp(filepath.Dir(exe))
	if e != nil {
		log.Fatal(e)
	}
	if len(os.Args) > 1 {
		cmd := os.Args[1]
		if cmd == "--help" || cmd == "-h" {
			printUsage()
			return
		}
		var e error
		switch cmd {
		case "--scan":
			e = a.scan()
		case "--new-batch":
			e = a.newBatch()
		case "--stage":
			e = a.stage()
		case "--export":
			var p string
			p, e = a.reports()
			if e == nil {
				fmt.Println("Excel report:", p)
			}
		case "--publish":
			e = a.publish()
		case "--restore":
			e = a.restore()
		case "--finalize":
			e = a.finalize()
		case "--status":
			fmt.Printf("Folder: %s\nStage: %s\nTracks: %d\n", a.root, a.state.Phase, len(a.state.Tracks))
			return
		case "--install-yt-dlp":
			e = a.downloader.install()
			if e == nil {
				waitForDownloader(a.downloader)
			}
			return
		case "--download":
			if len(os.Args) < 3 {
				log.Fatal("pass one or more URLs")
			}
			e = a.downloader.startDownload(DownloadOptions{URLs: strings.Join(os.Args[2:], "\n"), Quality: "native", Browser: "none", Scope: "playlist"})
			if e == nil {
				waitForDownloader(a.downloader)
			}
			return
		default:
			printUsage()
			os.Exit(2)
		}
		if e != nil {
			log.Fatal(e)
		}
		fmt.Printf("Success. Phase: %s; Tracks: %d\n", a.state.Phase, len(a.state.Tracks))
		return
	}
	if e = a.serve(); e != nil {
		log.Fatal(e)
	}
}

const htmlUI = `<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>GatherForge</title>
<style>
:root{--bg:#f4f6fb;--panel:#fff;--ink:#172238;--muted:#67768e;--line:#e1e6ef;--navy:#142a4a;--blue:#2b68ec;--soft:#ebf2ff;--amber:#9b6518;--red:#aa3333;--good:#23765c}*{box-sizing:border-box}body{margin:0;background:var(--bg);font-family:Inter,ui-sans-serif,system-ui,-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif;color:var(--ink);font-size:14px}header{background:var(--navy);color:white;padding:20px max(26px,calc((100vw - 1260px)/2));display:flex;align-items:center;justify-content:space-between;gap:16px;flex-wrap:wrap}header h1{font-size:20px;margin:0;font-weight:750;letter-spacing:-.4px}.header-sub{opacity:.75;font-size:12px;margin-top:5px}.version{font-size:12px;border:1px solid #5b6884;border-radius:40px;padding:5px 11px}.wrap{max-width:1300px;padding:27px 26px 70px;margin:auto}.intro{display:flex;gap:18px;justify-content:space-between;align-items:flex-end;flex-wrap:wrap}.intro h2{font-size:24px;letter-spacing:-.7px;margin:0 0 6px}.intro p{margin:0;color:var(--muted);line-height:1.55}.folder-path{background:#e8edf6;color:#32435e;border-radius:9px;padding:10px 15px;font-family:ui-monospace,monospace;font-size:12px;max-width:100%;overflow-wrap:anywhere}.stats{display:grid;grid-template-columns:repeat(4,minmax(0,1fr));gap:14px;margin:22px 0}.card{background:var(--panel);border:1px solid var(--line);border-radius:14px;box-shadow:0 4px 14px rgba(22,36,65,.025)}.stat{padding:18px 20px}.stat .label{color:var(--muted);font-size:12px}.stat .value{font-size:27px;font-weight:780;line-height:1.5}.stat .small{font-size:12px;color:var(--muted)}.workflow{padding:19px 20px;margin-bottom:17px}.workflow h3{margin:0 0 9px;font-size:15px}.flow{display:flex;gap:8px;flex-wrap:wrap;align-items:center}.step{background:#f3f6fa;border-radius:8px;padding:9px 11px;font-size:12px;color:#55657e}.step.active{background:#e7eeff;color:#1d52a7;font-weight:700}.arrow{color:#90a0b9}.buttons{display:flex;flex-wrap:wrap;gap:8px;margin:16px 0}.btn{font:inherit;font-weight:650;background:white;border:1px solid #c9d3e2;color:#23334c;border-radius:8px;padding:10px 15px;cursor:pointer;transition:background .15s;white-space:nowrap}.btn:hover{background:#f0f4fb}.btn.primary{background:var(--blue);color:white;border-color:var(--blue)}.btn.primary:hover{background:#1c52c5}.btn.warning{color:#934820;border-color:#e8c3a5}.btn.danger{color:#b43838;border-color:#ecc3c3}.btn:disabled{opacity:.38;cursor:not-allowed}.info{margin-top:12px;font-size:12px;color:#60718e;line-height:1.65}.alert{margin:15px 0;padding:13px 16px;background:#e9f5f0;border:1px solid #bfe1d3;border-radius:9px;color:#21634e;display:none}.alert.bad{background:#fff0f0;border-color:#e7b2b2;color:#a13a3a}.tabs{display:flex;gap:7px;margin:23px 0 13px;border-bottom:1px solid var(--line);padding-bottom:10px}.tab{border:0;background:transparent;padding:9px 14px;border-radius:8px;font-size:13px;cursor:pointer;color:#59677d;font-weight:700}.tab.active{background:var(--soft);color:#2459ba}.tab .count{margin-left:4px;background:#e0e7f1;border-radius:10px;padding:2px 6px;font-size:10px}.panel{padding:18px 18px 22px}.panel-head{display:flex;gap:12px;justify-content:space-between;align-items:center;flex-wrap:wrap;margin-bottom:14px}.panel-head h3{font-size:16px;margin:0}.panel-head .hint{color:var(--muted);font-size:12px;margin-top:5px}.search{font:inherit;width:min(370px,100%);border:1px solid #ccd5e3;border-radius:8px;padding:10px 12px;outline-color:#7ba0f5}.table-box{width:100%;overflow:auto;max-height:67vh}table{width:100%;border-collapse:collapse;text-align:left;font-size:12px}thead{position:sticky;top:0;background:#f1f4fa;z-index:2}th,td{padding:10px 11px;border-bottom:1px solid #e6eaf0;vertical-align:middle}th{color:#495b76;font-weight:730;white-space:nowrap}td.filename{max-width:350px;min-width:190px;word-break:break-word}td.source{max-width:265px;word-break:break-all;color:#748199;font-size:11px}.pill{display:inline-block;border-radius:18px;background:#e9edf5;color:#52637d;font-size:11px;padding:4px 8px;white-space:nowrap}.pill.exact{background:#f5e8e7;color:#9d5151}.pill.possible{background:#fff2db;color:#936317}.pill.keep{background:#e5f4e9;color:#387650}.pill.skip{background:#e6e6ef;color:#6b6372}.form-input{font:inherit;border:1px solid #d1d9e5;border-radius:7px;padding:7px 9px;max-width:220px;width:100%;min-width:135px}.form-select{font:inherit;border:1px solid #d1d9e5;border-radius:7px;padding:7px 5px;background:#fff}.muted{color:var(--muted)}.grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(265px,1fr));gap:12px}.artist{padding:15px 16px;min-width:0}.artist .row{display:flex;align-items:center;justify-content:space-between;gap:10px}.artist .name{font-weight:750;word-break:break-word}.artist .sub{font-size:12px;color:var(--muted);margin-top:7px}.artist input{margin-top:10px;width:100%}.artist button{margin-top:9px}.footer{font-size:12px;color:#738198;margin-top:19px;line-height:1.6}.shortcut{display:inline-block;border:1px solid #ced5e1;border-radius:4px;padding:1px 5px;font-family:monospace;color:#455674;background:#fff}.modal-bg{position:fixed;inset:0;background:#14233d99;display:flex;justify-content:center;align-items:center;z-index:10;padding:16px}.modal{width:min(560px,100%);padding:24px;border-radius:14px;background:white;box-shadow:0 20px 70px #0003}.modal h3{margin-top:0}.modal p{line-height:1.75;color:#5a6a83}.modal-actions{display:flex;gap:10px;justify-content:flex-end;margin-top:18px}.processing{opacity:.65;pointer-events:none}.download-card{padding:20px;margin-bottom:18px}.download-card h3{margin:0 0 8px;font-size:17px}.download-card p{color:var(--muted);line-height:1.6;margin:4px 0 15px}.downloads-grid{display:grid;grid-template-columns:1fr 1fr 1fr;gap:12px;margin:12px 0}.downloads-grid label{display:block;color:var(--muted);font-size:12px}.downloads-grid select{display:block;width:100%;margin-top:5px;padding:9px;border:1px solid #cbd5e2;border-radius:8px;background:white;color:var(--ink)}textarea.urls{width:100%;height:112px;border-radius:8px;border:1px solid #cbd5e2;padding:12px;resize:vertical;font:12px ui-monospace,monospace}.logs{height:205px;overflow:auto;background:#111d2c;color:#e1e8f4;padding:13px;white-space:pre-wrap;overflow-wrap:anywhere;font:12px ui-monospace,monospace;border-radius:8px}.download-state{font-size:12px;color:var(--muted);margin:10px 0 12px}@media(max-width:760px){.downloads-grid{grid-template-columns:1fr}}@media(max-width:760px){.stats{grid-template-columns:repeat(2,minmax(0,1fr))}.wrap{padding:20px 13px}.panel{padding:11px}.step{padding:7px 6px}.intro h2{font-size:21px}.btn{padding:9px 11px}}
</style></head><body><header><div><h1>GatherForge</h1><div class="header-sub">Local downloader + reviewed artist library</div></div><div class="version">v1.1 · local application</div></header><main class="wrap"><div class="intro"><div><h2>Organize your collection</h2><p>Scan everything underneath this folder. Review artists and duplicates before changing any files.</p></div><div class="folder-path" id="root"></div></div><section class="card download-card"><h3>Download tracks & playlists</h3><p>Paste one track, playlist, likes page, or multiple URLs (one per line). Files are saved to <b>_Incoming</b>, separate from your staging project. Review them with Scan / rescan when finished.</p><textarea class="urls" id="download-urls" placeholder="https://soundcloud.com/USERNAME/likes&#10;https://soundcloud.com/USERNAME/sets/PLAYLIST&#10;https://soundcloud.com/ARTIST/TRACK" aria-label="Audio URLs, one per line"></textarea><div class="downloads-grid"><label>Audio quality<select id="download-quality"><option value="native">Best available original (no conversion)</option><option value="mp3">Convert to MP3 (requires FFmpeg)</option></select></label><label>Login cookies (optional)<select id="download-browser"><option value="none">None / public</option><option value="firefox">Firefox</option><option value="chrome">Chrome</option><option value="chromium">Chromium</option><option value="edge">Edge</option><option value="brave">Brave</option></select></label><label>Link behavior<select id="download-scope"><option value="playlist">Entire playlists + single tracks</option><option value="single">Single track only (no playlists)</option></select></label></div><div class="buttons"><button class="btn" id="dl-install" onclick="downloaderAction('install')">Install / update yt-dlp</button><button class="btn primary" id="dl-start" onclick="downloaderAction('start')">Start download</button><button class="btn danger" id="dl-stop" onclick="downloaderAction('stop')" disabled>Stop</button></div><div class="download-state" id="download-state">Checking yt-dlp...</div><div class="logs" id="download-log" role="log" aria-live="off">Download log will appear here.</div><div class="info">First install downloads the official yt-dlp standalone binary and checks its SHA-256. Internet is required only for downloading/installing. yt-dlp is not included in this Git repository. FFmpeg is optional unless converting to MP3 or required by a particular stream. Download only tracks you are authorized to save.</div></section><section class="stats"><div class="card stat"><div class="label">Tracks found</div><div class="value" id="total">0</div><div class="small">Across subfolders</div></div><div class="card stat"><div class="label">Artist folders</div><div class="value" id="artists">0</div><div class="small">Editable before publish</div></div><div class="card stat"><div class="label">Selected tracks</div><div class="value" id="selected">0</div><div class="small">Will be published</div></div><div class="card stat"><div class="label">Duplicate flags</div><div class="value" id="dupes">0</div><div class="small">Exact and possible</div></div></section><section class="card workflow"><h3>Safe workflow</h3><div class="flow"><span class="step" data-phase="review">1. Review / edit</span><span class="arrow">›</span><span class="step" data-phase="staged">2. Stage originals</span><span class="arrow">›</span><span class="step" data-phase="published">3. Publish copies</span><span class="arrow">›</span><span class="step" data-phase="finalized">4. Optional cleanup</span></div><div class="buttons"><button class="btn" id="scan" onclick="run('scan')">Scan / rescan <span class="shortcut">S</span></button><button class="btn primary" id="stage" onclick="confirmTask('stage')">Move to staging <span class="shortcut">G</span></button><button class="btn" id="export" onclick="run('export')">Export Excel <span class="shortcut">E</span></button><button class="btn primary" id="publish" onclick="confirmTask('publish')">Publish artist folders <span class="shortcut">P</span></button><button class="btn" id="restore" onclick="confirmTask('restore')">Restore originals</button><button class="btn danger" id="finalize" onclick="confirmTask('finalize')">Finalize / clean staging</button><button class="btn" id="newbatch" onclick="confirmTask('newbatch')">Start new batch</button></div><div class="info">Staging lives in <b>xxx/_staging</b>. Publishing copies tracks to the artist folders and keeps staging intact. Finalize verifies each copy before removing its staged counterpart; skipped/duplicate files are moved to <b>xxx/_Not_Published</b>, not deleted. The organizer does not transcode audio; MP3 conversion is optional in the downloader.</div></section><div id="alert" class="alert"></div><div class="tabs"><button class="tab active" data-tab="tracks" onclick="tab('tracks')">Tracks</button><button class="tab" data-tab="artists" onclick="tab('artists')">Artist folders</button><button class="tab" data-tab="duplicates" onclick="tab('duplicates')">Duplicates <span id="dupetab" class="count">0</span></button><button class="tab" data-tab="uncertain" onclick="tab('uncertain')">Inferred artists</button></div><section class="card panel"><div class="panel-head"><div><h3 id="panel-title">All tracks</h3><div class="hint" id="panel-hint">Edit artists or choose Keep / Skip. Changes save immediately.</div></div><input class="search" id="search" placeholder="Search artist, song, source path..." oninput="renderList()" aria-label="Search tracks"></div><div id="content"></div><div class="footer">Local app · no uploads · no cloud services · music never modified during review. <span class="shortcut">/</span> search · <span class="shortcut">?</span> shortcuts</div></section></main><div id="modal-root"></div>
<script>
const token='__TOKEN__';let state={tracks:[],folders:{},counts:{},phase:'review'};let active='tracks',busy=false;const byid=id=>document.getElementById(id);const esc=x=>String(x??'').replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
function notify(msg,bad=false){const el=byid('alert');el.textContent=msg;el.style.display='block';el.className='alert'+(bad?' bad':'')}
async function refresh(){try{const r=await fetch('/api/state');state=await r.json();render()}catch(e){notify('Unable to reach local app: '+e,true)}}
async function run(command,more={}){if(busy)return;busy=true;document.body.classList.add('processing');try{const r=await fetch('/api/do',{method:'POST',headers:{'Content-Type':'application/json','X-Organizer-Token':token},body:JSON.stringify({command,...more})});const data=await r.json();if(!r.ok)throw Error(data.error||'Operation failed');state=data;notify(data.message||'Finished');render()}catch(e){notify(e.message,true)}finally{busy=false;document.body.classList.remove('processing')}}
const prompts={stage:{title:'Move all original audio into staging?',body:'This moves each source file, keeping its original folder path underneath xxx/_staging. No audio is deleted. You can restore the files before publishing.'},publish:{title:'Publish approved tracks?',body:'Selected Keep tracks will be COPIED from staging to top-level artist folders. Staging remains intact. You can still inspect and compare all originals.'},restore:{title:'Restore the original folder layout?',body:'This moves files from staging to their original paths. Restore is only available before publishing and refuses to overwrite files.'},newbatch:{title:'Start another organization batch?',body:'The completed batch will be archived in xxx/_reports/Previous_Batches, and known published songs will be excluded from the next scan. Any newly downloaded tracks can be reviewed and sorted separately.'},finalize:{title:'Finalize this library?',body:'All published songs will be SHA-256 verified. Only then will their staged copies be deleted. Duplicates, skipped items and songs not published are preserved in xxx/_Not_Published. This action cannot be undone automatically.'}};
function confirmTask(command){const p=prompts[command];let extra='';if(command==='publish'&&state.counts.possible>0){extra='<p style="color:#ad6023">There are '+state.counts.possible+' possible duplicates not chosen yet. Review them and select Keep or Skip before publishing.</p>'}
 byid('modal-root').innerHTML='<div class="modal-bg" onclick="closeModal(event)"><div class="modal" onclick="event.stopPropagation()"><h3>'+p.title+'</h3><p>'+p.body+'</p>'+extra+'<div class="modal-actions"><button class="btn" onclick="byid(\'modal-root\').innerHTML=\'\'">Cancel</button><button class="btn '+(command==='finalize'?'danger':'primary')+'" onclick="byid(\'modal-root\').innerHTML=\'\';run(\''+command+'\')">Confirm</button></div></div></div>'}
function closeModal(e){if(e.target.classList.contains('modal-bg'))byid('modal-root').innerHTML=''}
function render(){byid('root').textContent=state.root||'';byid('total').textContent=state.tracks.length;byid('artists').textContent=Object.keys(state.folders).length;byid('selected').textContent=state.counts.keep||0;byid('dupes').textContent=(state.counts.flagged||0);byid('dupetab').textContent=(state.counts.flagged||0);
 const phase=state.phase;document.querySelectorAll('.step').forEach(el=>el.classList.toggle('active',el.dataset.phase===phase||(phase==='staging'&&el.dataset.phase==='staged')));
 byid('scan').disabled=phase!=='review';byid('stage').disabled=phase!=='review'||!state.tracks.length;byid('export').disabled=!state.tracks.length;byid('publish').disabled=!(phase==='staged'||phase==='published')||(state.counts.possible||0)>0;byid('restore').disabled=!(phase==='staged'||phase==='staging');byid('finalize').disabled=!(phase==='published'||phase==='finalizing');byid('newbatch').disabled=phase!=='finalized';
 renderList()}
function tab(name){active=name;document.querySelectorAll('.tab').forEach(el=>el.classList.toggle('active',el.dataset.tab===active));renderList()}
function itemAction(t){const options=['keep','skip'];if(t.action==='exact')options.unshift('exact');if(t.action==='possible')options.unshift('possible');return '<select class="form-select" '+(state.phase==='published'||state.phase==='finalized'?'disabled':'')+' onchange="setTrack('+t.id+',\'action\',this.value)">'+options.map(x=>'<option '+(t.action===x?'selected':'')+' value="'+x+'">'+(x==='exact'?'Exact duplicate':x==='possible'?'Review needed':x==='keep'?'Keep':'Skip')+'</option>').join('')+'</select>'}
function rowHTML(t){return '<tr><td>'+t.id+'</td><td class="filename"><b>'+esc(t.title)+'</b><div class="muted">'+esc(t.filename)+'</div></td><td><input class="form-input" aria-label="Artist for track '+t.id+'" value="'+esc(t.artist)+'" '+(state.phase==='published'||state.phase==='finalized'?'disabled':'')+' onchange="setTrack('+t.id+',\'artist\',this.value)"></td><td>'+itemAction(t)+'</td><td><span class="pill '+esc(t.action)+'">'+esc(t.action)+(t.duplicateOf?(t.duplicateOf<0?' · existing library':' #'+t.duplicateOf):'')+'</span></td><td>'+esc(t.format)+'</td><td class="source" title="'+esc(t.original)+'">'+esc(t.original)+'</td><td class="source" title="'+esc(t.stage)+'">'+esc(t.stage)+'</td></tr>'}
function renderList(){const query=byid('search').value.toLowerCase().trim();let arr=state.tracks||[];if(active==='duplicates')arr=arr.filter(t=>t.duplicateOf||t.action==='exact'||t.action==='possible');if(active==='uncertain')arr=arr.filter(t=>t.source==='filename');if(query)arr=arr.filter(t=>(t.artist+' '+t.title+' '+t.original+' '+t.action).toLowerCase().includes(query));
 const names={tracks:'All tracks',artists:'Artist folders',duplicates:'Duplicate review',uncertain:'Inferred artist names'};byid('panel-title').textContent=names[active];byid('panel-hint').textContent=active==='artists'?'Rename or merge entire artist folders; editable per track in Tracks.':active==='duplicates'?'Exact duplicates are excluded automatically; decide Keep or Skip for each possible duplicate.':active==='uncertain'?'These artist names were inferred from filenames or folders because readable artist metadata was unavailable.':'Edit the first credited artist and Keep/Skip. Changes are saved immediately.';
 if(active==='artists'){renderArtists(query);return}
 const showing=arr.slice(0,250);byid('content').innerHTML='<div class="table-box"><table><thead><tr><th>#</th><th>Track</th><th>Main artist / folder</th><th>Decision</th><th>Flag</th><th>Format</th><th>Original source</th><th>Staging path</th></tr></thead><tbody>'+showing.map(rowHTML).join('')+'</tbody></table></div><div class="footer">Showing '+showing.length+' of '+arr.length+' matching tracks. Narrow the search to edit additional items.</div>';
}
function renderArtists(q){const grouped=new Map;for(const t of state.tracks){if(!grouped.has(t.artist))grouped.set(t.artist,[]);grouped.get(t.artist).push(t)}const entries=[...grouped.entries()].sort((a,b)=>a[0].localeCompare(b[0])).filter(([name])=>!q||name.toLowerCase().includes(q));byid('content').innerHTML='<div class="grid">'+entries.map(([name,ts],i)=>{const kept=ts.filter(t=>t.action==='keep').length;const dupe=ts.filter(t=>t.action==='possible'||t.action==='exact').length;return '<div class="card artist"><div class="row"><div class="name">'+esc(name)+'</div><span class="pill">'+ts.length+' tracks</span></div><div class="sub">'+kept+' selected · '+dupe+' duplicates flagged</div><input class="form-input" id="artist-'+i+'" value="'+esc(name)+'" '+(state.phase==='published'||state.phase==='finalized'?'disabled':'')+' aria-label="Rename artist '+esc(name)+'"><button class="btn" '+(state.phase==='published'||state.phase==='finalized'?'disabled':'')+' onclick="renameArtist('+JSON.stringify(name).replace(/"/g,'&quot;')+',byid(\'artist-'+i+'\').value)">Apply to all tracks</button></div>'}).join('')+'</div><div class="footer">'+entries.length+' artist groups · Same exact folder name will merge groups on publish.</div>'}
async function setTrack(id,field,value){const patch={id};patch[field]=value;await run('update',{changes:[patch]})}
async function renameArtist(oldName,newName){if(!newName.trim())return notify('Artist folder name cannot be empty',true);await run('update',{aliases:{[oldName]:newName}})}
document.addEventListener('keydown',e=>{if(e.ctrlKey||e.metaKey||e.altKey)return;const activeInput=['INPUT','TEXTAREA','SELECT'].includes(document.activeElement?.tagName);if(e.key==='Escape'){byid('modal-root').innerHTML='';return}if(e.key==='/'&&!activeInput){e.preventDefault();byid('search').focus();return}if(e.key==='?'&&!activeInput){notify('Shortcuts: S scan, G stage, E export, P publish, / search, Esc cancel');return}if(activeInput)return;switch(e.key.toLowerCase()){case 's':if(!byid('scan').disabled)run('scan');break;case 'g':if(!byid('stage').disabled)confirmTask('stage');break;case 'e':if(!byid('export').disabled)run('export');break;case 'p':if(!byid('publish').disabled)confirmTask('publish');break}});
let previousDownloadLogs=-1;
async function downloadRefresh(){
 try{
  const r=await fetch('/api/downloader');if(!r.ok)return;
  const d=await r.json();
  byid('dl-install').disabled=d.active;byid('dl-start').disabled=d.active||!d.toolInstalled;byid('dl-stop').disabled=!d.active;
  byid('download-state').textContent=(d.toolInstalled?'yt-dlp ready':'yt-dlp not installed')+' · '+(d.ffmpegInstalled?'FFmpeg available':'FFmpeg unavailable')+' · '+(d.active?'Running '+d.mode:d.message||'Idle')+' · '+d.incoming;
  const lines=(d.logs||[]).join('\n');
  if(previousDownloadLogs!==lines){const el=byid('download-log');el.textContent=lines||'Download log will appear here.';el.scrollTop=el.scrollHeight;previousDownloadLogs=lines}
 }catch(e){byid('download-state').textContent='Downloader not reachable: '+e.message}
}
async function downloaderAction(command){
 const opts={urls:byid('download-urls').value,quality:byid('download-quality').value,browser:byid('download-browser').value,scope:byid('download-scope').value};
 if(command==='install'&&!confirm('Install or update the official yt-dlp executable from GitHub?'))return;
 if(command==='stop'&&!confirm('Cancel the running job? Already downloaded tracks will be kept.'))return;
 try{const r=await fetch('/api/downloader/action',{method:'POST',headers:{'Content-Type':'application/json','X-Organizer-Token':token},body:JSON.stringify({command,options:opts})});const d=await r.json();if(!r.ok)throw Error(d.error||'Downloader operation failed');notify(command==='start'?'Download started. Progress is shown below.':command==='install'?'Installation started. Progress is shown below.':'Cancellation requested.');await downloadRefresh()}
 catch(e){notify(e.message,true)}
}
refresh();downloadRefresh();setInterval(downloadRefresh,1200);
</script></body></html>`
