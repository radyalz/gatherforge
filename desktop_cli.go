package main

import (
	"encoding/json"
	"fmt"
	"log"
	"os"
	"path/filepath"
	"strings"
	"time"
)

func waitForDownloader(d *DownloadManager) {
	var last int64
	for {
		s := d.status()
		lines := s["logs"].([]string)
		count := s["logCounter"].(int64)
		start := last - (count - int64(len(lines)))
		if start < 0 {
			start = 0
		}
		if start > int64(len(lines)) {
			start = int64(len(lines))
		}
		for _, line := range lines[start:] {
			fmt.Println(line)
		}
		last = count
		if !s["active"].(bool) {
			if s["exitCode"].(int) != 0 {
				log.Fatal(s["message"])
			}
			return
		}
		time.Sleep(300 * time.Millisecond)
	}
}
func main() {
	args := os.Args[1:]
	if len(args) < 3 || args[0] != "--root" {
		log.Fatal("usage: gatherforge-core --root LIBRARY --status | --action ACTION | --edit | --download URL...")
	}
	root, e := filepath.Abs(args[1])
	if e != nil {
		log.Fatal(e)
	}
	a, e := newApp(root)
	if e != nil {
		log.Fatal(e)
	}
	cmd := args[2]
	switch cmd {
	case "--status":
		e = json.NewEncoder(os.Stdout).Encode(a.status())
	case "--edit":
		var q struct {
			Changes []map[string]any  `json:"changes"`
			Aliases map[string]string `json:"aliases"`
		}
		e = json.NewDecoder(os.Stdin).Decode(&q)
		if e == nil {
			e = a.update(q.Changes, q.Aliases)
		}
	case "--action":
		if len(args) < 4 {
			log.Fatal("missing action")
		}
		switch args[3] {
		case "scan":
			e = a.scan()
		case "stage":
			e = a.stage()
		case "restore":
			e = a.restore()
		case "publish":
			e = a.publish()
		case "finalize":
			e = a.finalize()
		case "export":
			_, e = a.reports()
		case "newbatch":
			e = a.newBatch()
		default:
			log.Fatal("unknown action")
		}
	case "--install":
		e = a.downloader.install()
		if e == nil {
			waitForDownloader(a.downloader)
		}
	case "--download":
		if len(args) < 4 {
			log.Fatal("missing URLs")
		}
		e = a.downloader.startDownload(DownloadOptions{URLs: strings.Join(args[3:], "\n"), Quality: "native", Browser: "none", Scope: "playlist"})
		if e == nil {
			waitForDownloader(a.downloader)
		}
	default:
		e = fmt.Errorf("unknown option: %s", cmd)
	}
	if e != nil {
		log.Fatal(e)
	}
}
