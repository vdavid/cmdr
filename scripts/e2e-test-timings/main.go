// Command e2e-test-timings reads Playwright JSON reports from one or more E2E
// runs (macOS and/or Linux) and emits a per-test timing comparison.
//
// macOS sharded run produces three JSON files at /tmp/cmdr-e2e-report-{mtp,
// nonmtp1,nonmtp2}.json (set by scripts/check/checks/desktop-svelte-e2e-playwright.go's
// planShards). The Linux docker run produces /tmp/cmdr-e2e-report-linux.json
// (set by apps/desktop/scripts/e2e-linux.sh). Both follow Playwright's standard
// JSON reporter shape.
//
// The comparison joins macOS shards' tests on `{spec}::{describe chain}::{title}`
// and matches them against the Linux run. Output is a markdown or CSV table
// sorted by (linux/macos) ratio, surfacing tests that are disproportionately
// slow on Linux. See README.md for usage.
package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
)

// playwrightReport is the relevant subset of Playwright's JSON reporter
// output. We unmarshal only the fields we use; extra fields are ignored.
type playwrightReport struct {
	Suites []suite `json:"suites"`
}

type suite struct {
	Title  string `json:"title"`
	File   string `json:"file"`
	Specs  []spec `json:"specs"`
	Suites []suite `json:"suites"`
}

type spec struct {
	Title string `json:"title"`
	Tests []test `json:"tests"`
}

type test struct {
	Results []result `json:"results"`
}

type result struct {
	DurationMs int    `json:"duration"`
	Status     string `json:"status"`
}

// row is the per-test record after parsing one or more reports. macOS and
// Linux durations stay separate; either may be zero when the test ran only
// in one environment.
type row struct {
	spec    string
	full    string // describe chain + title
	macosMs int
	linuxMs int
}

func (r row) ratio() float64 {
	if r.macosMs == 0 || r.linuxMs == 0 {
		return 0
	}
	return float64(r.linuxMs) / float64(r.macosMs)
}

func main() {
	macPaths := flag.String("macos", "/tmp/cmdr-e2e-report-mtp.json,/tmp/cmdr-e2e-report-nonmtp1.json,/tmp/cmdr-e2e-report-nonmtp2.json",
		"comma-separated macOS report paths")
	linuxPath := flag.String("linux", "/tmp/cmdr-e2e-report-linux.json",
		"Linux report path")
	sortBy := flag.String("sort", "ratio", "sort by: ratio | linux | macos | delta")
	top := flag.Int("top", 0, "show only the top N rows (0 = all)")
	format := flag.String("format", "md", "output format: md | csv")
	minLinuxMs := flag.Int("min-linux-ms", 0,
		"hide tests faster than this on Linux (filter out the cheap tail)")
	flag.Parse()

	rows := map[string]*row{}

	for p := range strings.SplitSeq(*macPaths, ",") {
		p = strings.TrimSpace(p)
		if p == "" {
			continue
		}
		if err := loadInto(rows, p, true); err != nil {
			fmt.Fprintf(os.Stderr, "warn: %v\n", err)
		}
	}
	if *linuxPath != "" {
		if err := loadInto(rows, *linuxPath, false); err != nil {
			fmt.Fprintf(os.Stderr, "warn: %v\n", err)
		}
	}

	if len(rows) == 0 {
		fmt.Fprintln(os.Stderr, "no test data loaded — check --macos / --linux paths")
		os.Exit(1)
	}

	list := make([]*row, 0, len(rows))
	for _, r := range rows {
		if r.linuxMs >= *minLinuxMs {
			list = append(list, r)
		}
	}

	sortRows(list, *sortBy)
	if *top > 0 && len(list) > *top {
		list = list[:*top]
	}

	switch *format {
	case "csv":
		emitCSV(list)
	default:
		emitMarkdown(list)
	}
}

// loadInto parses a Playwright JSON report and merges per-test rows into the
// rows map keyed by `{spec}::{describe chain}::{title}`. `isMacos` decides
// which column the durations land in.
func loadInto(rows map[string]*row, path string, isMacos bool) error {
	data, err := os.ReadFile(path)
	if err != nil {
		return fmt.Errorf("read %s: %w", path, err)
	}
	var rep playwrightReport
	if err := json.Unmarshal(data, &rep); err != nil {
		return fmt.Errorf("parse %s: %w", path, err)
	}
	for _, s := range rep.Suites {
		walk(rows, s, s.File, nil, isMacos)
	}
	return nil
}

// walk recurses Playwright's nested suite tree, accumulating describe-chain
// context, and records each spec's total duration (summed across retry
// attempts so the row reflects real wall-clock cost).
func walk(rows map[string]*row, s suite, file string, describe []string, isMacos bool) {
	scope := describe
	// The top-level suite is the file itself (no describe). Nested suites
	// have describe titles.
	if s.Title != "" && s.Title != filepath.Base(file) && s.Title != file {
		scope = append(append([]string{}, describe...), s.Title)
	}
	for _, sp := range s.Specs {
		key := file + "::" + strings.Join(scope, " › ") + "::" + sp.Title
		fullTitle := strings.Join(append(scope, sp.Title), " › ")
		duration := 0
		for _, t := range sp.Tests {
			for _, r := range t.Results {
				duration += r.DurationMs
			}
		}
		r := rows[key]
		if r == nil {
			r = &row{spec: file, full: fullTitle}
			rows[key] = r
		}
		if isMacos {
			r.macosMs += duration
		} else {
			r.linuxMs += duration
		}
	}
	for _, child := range s.Suites {
		walk(rows, child, file, scope, isMacos)
	}
}

func sortRows(list []*row, by string) {
	switch by {
	case "linux":
		sort.SliceStable(list, func(i, j int) bool { return list[i].linuxMs > list[j].linuxMs })
	case "macos":
		sort.SliceStable(list, func(i, j int) bool { return list[i].macosMs > list[j].macosMs })
	case "delta":
		sort.SliceStable(list, func(i, j int) bool {
			return (list[i].linuxMs - list[i].macosMs) > (list[j].linuxMs - list[j].macosMs)
		})
	default: // ratio
		sort.SliceStable(list, func(i, j int) bool { return list[i].ratio() > list[j].ratio() })
	}
}

func emitMarkdown(list []*row) {
	fmt.Println("| Spec | Test | macOS | Linux | ratio |")
	fmt.Println("|------|------|-------|-------|-------|")
	for _, r := range list {
		fmt.Printf("| %s | %s | %s | %s | %s |\n",
			short(r.spec), escape(r.full), formatMs(r.macosMs), formatMs(r.linuxMs), formatRatio(r.ratio()))
	}
}

func emitCSV(list []*row) {
	fmt.Println("spec,test,macos_ms,linux_ms,ratio")
	for _, r := range list {
		fmt.Printf("%q,%q,%d,%d,%.2f\n", r.spec, r.full, r.macosMs, r.linuxMs, r.ratio())
	}
}

// short trims the well-known test path prefix so the table column doesn't
// repeat it on every row. Falls back to the basename if the prefix isn't found.
func short(specFile string) string {
	const prefix = "test/e2e-playwright/"
	if idx := strings.Index(specFile, prefix); idx >= 0 {
		return specFile[idx+len(prefix):]
	}
	return filepath.Base(specFile)
}

// escape sanitises titles for markdown table cells: replaces `|` (which would
// split the cell) and collapses newlines.
func escape(s string) string {
	s = strings.ReplaceAll(s, "|", "\\|")
	s = strings.ReplaceAll(s, "\n", " ")
	return s
}

// formatMs renders milliseconds as "—" if zero (clear "no data" signal),
// "Xms" if sub-second, or "X.Ys" otherwise.
func formatMs(ms int) string {
	if ms == 0 {
		return "—"
	}
	if ms < 1000 {
		return fmt.Sprintf("%dms", ms)
	}
	return fmt.Sprintf("%.1fs", float64(ms)/1000.0)
}

// formatRatio shows "—" when either side is missing (no comparison possible),
// otherwise the multiplier as "X.Y×".
func formatRatio(r float64) string {
	if r == 0 {
		return "—"
	}
	return fmt.Sprintf("%.1f×", r)
}
