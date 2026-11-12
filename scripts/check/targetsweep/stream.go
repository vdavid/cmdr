package targetsweep

import (
	"bytes"
	"encoding/json"
	"fmt"
	"regexp"
	"sort"
	"strings"
	"time"
)

// unitHashRE matches a cargo unit's entry name: anything, a dash, the unit's 16 lowercase
// hex digits, then nothing or an extension. Cargo names every hashed output this way
// (`libserde-<h>.rlib`, `serde-<h>.d`, `.fingerprint/serde-<h>/`, `build/ring-<h>/`).
var unitHashRE = regexp.MustCompile(`^.+-([0-9a-f]{16})(?:\..*)?$`)

// unitHash reads the unit hash off one path segment.
func unitHash(name string) (string, bool) {
	m := unitHashRE.FindStringSubmatch(name)
	if m == nil {
		return "", false
	}
	return m[1], true
}

// StreamFile is one file of `LiveDir`, as read off the volume.
type StreamFile struct {
	Name    string
	ModTime time.Time
	Content []byte
}

// streamSummary is what one cargo JSON message stream says.
type streamSummary struct {
	hashes map[string]bool
	// artifacts counts `compiler-artifact` messages: a stream with none named no units.
	artifacts int
	// complete is cargo's own `build-finished` verdict: the build graph ran to the end,
	// so the stream names every unit in it, fresh or rebuilt.
	complete bool
	// malformed counts lines that open like JSON and don't parse.
	malformed int
}

// cargoMessage is the slice of a `--message-format=json` line the sweep reads.
type cargoMessage struct {
	Reason     string   `json:"reason"`
	Filenames  []string `json:"filenames"`
	Executable *string  `json:"executable"`
	OutDir     string   `json:"out_dir"`
	Success    *bool    `json:"success"`
}

// parseStream reads a cargo JSON message stream. Lines that don't open with `{` are text
// (the doc lane saves cargo's stderr alongside), and are skipped.
func parseStream(data []byte) streamSummary {
	s := streamSummary{hashes: map[string]bool{}}
	for _, line := range bytes.Split(data, []byte("\n")) {
		line = bytes.TrimSpace(line)
		if len(line) == 0 || line[0] != '{' {
			continue
		}
		var msg cargoMessage
		if err := json.Unmarshal(line, &msg); err != nil {
			s.malformed++
			continue
		}
		switch msg.Reason {
		case "compiler-artifact":
			s.artifacts++
			for _, p := range msg.Filenames {
				addPathHashes(s.hashes, p)
			}
			if msg.Executable != nil {
				addPathHashes(s.hashes, *msg.Executable)
			}
		case "build-script-executed":
			addPathHashes(s.hashes, msg.OutDir)
		case "build-finished":
			s.complete = msg.Success != nil && *msg.Success
		}
	}
	return s
}

// addPathHashes adds every unit hash a path names: `build/ring-<h>/out` names one in a
// directory, `deps/libserde-<h>.rlib` one in the file name.
func addPathHashes(into map[string]bool, path string) {
	for _, seg := range strings.Split(path, "/") {
		if h, ok := unitHash(seg); ok {
			into[h] = true
		}
	}
}

// Live is the mark: every unit hash some lane's build graph holds.
type Live struct {
	Hashes map[string]bool
	// Problem says why the mark can't be trusted; non-empty means the sweep leaves every
	// build unit alone.
	Problem string
}

// assembleLive unions the streams into the live set. It is sound only when every lane
// has a complete stream: the union of those names every unit their last finished builds
// held. On top, each lane's partial streams NEWER than its complete one count too, since
// they're the only record of units a run built before its build stopped. A partial no
// newer than its lane's complete stream is superseded (that finished run listed whatever
// of it is still live), and it comes back in `superseded` for the caller to delete.
//
// A partial is read leniently (a killed run cuts its last line), and a stream of a lane
// this code doesn't know counts in full: both only ever add hashes.
func assembleLive(files []StreamFile, lanes []string) (Live, []string) {
	complete := map[string]StreamFile{}
	for _, f := range files {
		for _, lane := range lanes {
			if f.Name == CompleteStream(lane) {
				complete[lane] = f
			}
		}
	}

	live := Live{Hashes: map[string]bool{}}
	var problems []string
	for _, lane := range lanes {
		f, ok := complete[lane]
		if !ok {
			problems = append(problems, fmt.Sprintf("the %s lane has no finished build on record", lane))
			continue
		}
		s := parseStream(f.Content)
		if problem := completeStreamProblem(lane, s); problem != "" {
			problems = append(problems, problem)
		}
		for h := range s.hashes {
			live.Hashes[h] = true
		}
	}
	superseded := addOtherStreams(live.Hashes, files, complete, lanes)
	live.Problem = strings.Join(problems, "; ")
	return live, superseded
}

// completeStreamProblem says why a lane's complete stream can't vouch for its lane.
func completeStreamProblem(lane string, s streamSummary) string {
	switch {
	case s.malformed > 0:
		return fmt.Sprintf("the %s lane's record has %d unreadable lines", lane, s.malformed)
	case !s.complete:
		return fmt.Sprintf("the %s lane's record doesn't end in a finished build", lane)
	case s.artifacts == 0:
		return fmt.Sprintf("the %s lane's record names no units", lane)
	}
	return ""
}

// addOtherStreams adds the hashes of every stream that isn't a known lane's complete one:
// partials newer than their lane's complete stream, and streams of unknown lanes. It
// returns the superseded partials.
func addOtherStreams(into map[string]bool, files []StreamFile, complete map[string]StreamFile, lanes []string) []string {
	var superseded []string
	for _, f := range files {
		if !strings.HasSuffix(f.Name, ".jsonl") || isComplete(f.Name, lanes) {
			continue
		}
		if lane, ok := partialLane(f.Name, lanes); ok {
			if c, has := complete[lane]; has && !f.ModTime.After(c.ModTime) {
				superseded = append(superseded, f.Name)
				continue
			}
		}
		for h := range parseStream(f.Content).hashes {
			into[h] = true
		}
	}
	sort.Strings(superseded)
	return superseded
}

func isComplete(name string, lanes []string) bool {
	for _, lane := range lanes {
		if name == CompleteStream(lane) {
			return true
		}
	}
	return false
}

func partialLane(name string, lanes []string) (string, bool) {
	for _, lane := range lanes {
		if strings.HasPrefix(name, PartialStreamPrefix(lane)) {
			return lane, true
		}
	}
	return "", false
}
