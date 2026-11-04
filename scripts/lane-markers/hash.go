package main

import (
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"strings"
)

// hashVersion goes into every hash. Bump it when what a hash covers changes in a way the file
// set alone wouldn't show (say, a new kind of definition input), so older markers stop counting.
const hashVersion = "lane-markers/v1"

// markerPrefix starts every marker's cache key: `lane-green-<lane>-<hash>`.
const markerPrefix = "lane-green-"

// alwaysInputs are part of every lane's definition, whatever its filters say: the toolchain pins
// (they change what every compiler and linter does) and the check runner (most lanes run their
// checks through it). Over-inclusion only costs a rerun; a missing input would cost a lane
// reported green for code it never tested.
var alwaysInputs = mustFilter("always", []string{
	".mise.toml",
	"rust-toolchain.toml",
	"scripts/check/**",
	"!**/CLAUDE.md",
	"!**/DETAILS.md",
})

func mustFilter(name string, patterns []string) filter {
	f := filter{name: name}
	for _, p := range patterns {
		g, err := compileGlob(p)
		if err != nil {
			panic(err)
		}
		f.patterns = append(f.patterns, g)
	}
	return f
}

// laneInputs is exactly the tracked files a lane's verdict depends on: what any of its filters
// matches (with the workflow's quantifier, the same way dorny decides), its definition files and
// directories, and `alwaysInputs`. Sorted by path, each file once.
func laneInputs(t tree, c *config, l lane) []entry {
	defFiles := map[string]bool{}
	for _, f := range l.defFiles {
		defFiles[f] = true
	}
	var out []entry
	for _, e := range t.entries() {
		if defFiles[e.path] || alwaysInputs.matches(e.path, quantSomeWithExcludes) || underAny(e.path, l.defDirs) || matchesAnyFilter(e.path, c, l) {
			out = append(out, e)
		}
	}
	return out
}

func matchesAnyFilter(p string, c *config, l lane) bool {
	for _, name := range l.filters {
		if c.filters[name].matches(p, c.quant) {
			return true
		}
	}
	return false
}

func underAny(p string, dirs []string) bool {
	for _, d := range dirs {
		if strings.HasPrefix(p, d+"/") {
			return true
		}
	}
	return false
}

// laneHash digests the lane id and its inputs' paths, modes, and object IDs. Git object IDs are
// content hashes, so two trees give the same lane hash exactly when every input byte, every
// input's executable bit, and the input set itself are the same.
func laneHash(laneID string, inputs []entry) string {
	h := sha256.New()
	fmt.Fprintf(h, "%s\nlane %s\n", hashVersion, laneID)
	for _, e := range inputs {
		fmt.Fprintf(h, "%s %s %s\t%s\x00", e.mode, e.typ, e.object, e.path)
	}
	return hex.EncodeToString(h.Sum(nil))
}

func markerKey(laneID, hash string) string {
	return markerPrefix + laneID + "-" + hash
}
