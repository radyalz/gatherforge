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
	"os"
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

const version = "2.0.0"

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
