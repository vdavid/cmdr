// Re-anchor cost measurement: how long a full metadata walk of a huge directory takes via readdir, lstat, and getattrlistbulk.
//
// A Phase C re-anchor is a streaming one-level walk of a sealed directory that
// sums its children's sizes: no DB reads, no writer messages, no row writes.
// This tool times exactly that shape, three ways, so the spike can answer "at
// what cadence is a re-anchor affordable?" with numbers instead of judgment:
//
//	enumerate  readdir only, no attributes (the floor)
//	lstat      readdir + one lstat per entry (the naive re-anchor)
//	bulk       getattrlistbulk, names and sizes in batches (macOS only)
//
// Usage:
//
//	go run ./scripts/reanchor-cost [flags] <dir> [more dirs...]
//
// Flags: -runs (repeats per directory, default 2: run 1 is cold-ish, later runs
// are warm), -methods, -bufkb (getattrlistbulk buffer size), -md (markdown
// table output).
//
// It reports wall time, entries seen, bytes summed, and µs/entry per run, and
// cross-checks the methods against each other: a byte or entry mismatch between
// `lstat` and `bulk` means the bulk attribute layout is being misparsed, and is
// reported as a warning rather than hidden.
//
// Results and the go/no-go call: docs/notes/reanchor-cost-spike.md
package main

import (
	"flag"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"time"
)

func main() {
	runs := flag.Int("runs", 2, "measurement runs per directory (run 1 is cold-ish, the rest warm)")
	methods := flag.String("methods", "enumerate,lstat,bulk", "comma-separated subset of: enumerate, lstat, bulk")
	bufKB := flag.Int("bufkb", 128, "getattrlistbulk buffer size in KiB")
	md := flag.Bool("md", false, "print a markdown table instead of aligned text")
	generate := flag.Int("generate", 0, "instead of measuring, create this many empty files in each named directory (control corpora)")
	flag.Parse()

	dirs := flag.Args()
	if len(dirs) == 0 {
		fmt.Fprintln(os.Stderr, "usage: go run ./scripts/reanchor-cost [flags] <dir> [more dirs...]")
		os.Exit(2)
	}
	if *generate > 0 {
		for _, dir := range dirs {
			if err := generateControl(dir, *generate); err != nil {
				fmt.Fprintf(os.Stderr, "generating %s: %v\n", dir, err)
				os.Exit(1)
			}
		}
		return
	}
	selected, err := parseMethods(*methods)
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(2)
	}
	if *runs < 1 {
		fmt.Fprintln(os.Stderr, "-runs must be at least 1")
		os.Exit(2)
	}

	var all []Result
	var warnings []string
	for _, dir := range dirs {
		fmt.Fprintf(os.Stderr, "→ %s\n", dir)
		var perDir []Result
		for run := 1; run <= *runs; run++ {
			for _, m := range selected {
				res, err := Measure(m, dir, *bufKB*1024)
				if err != nil {
					warnings = append(warnings, fmt.Sprintf("%s [%s]: %v", dir, m, err))
					fmt.Fprintf(os.Stderr, "  run %d %-9s failed: %v\n", run, m, err)
					continue
				}
				res.Dir = dir
				res.Run = run
				perDir = append(perDir, res)
				all = append(all, res)
				fmt.Fprintf(os.Stderr, "  run %d %-9s %8.2fs  %9d entries  %6.1f µs/entry  %3.0f%% CPU\n",
					run, m, res.Duration.Seconds(), res.Entries, res.MicrosPerEntry(), res.CPUPercent())
			}
		}
		warnings = append(warnings, crossCheck(dir, perDir)...)
	}

	fmt.Print(report(all, *md))
	if len(warnings) > 0 {
		fmt.Printf("\nWarnings (%d):\n", len(warnings))
		for _, w := range warnings {
			fmt.Printf("  - %s\n", w)
		}
	}
}

// generateControl builds a flat directory of empty files, the control corpus the
// spike compares the pathological real directory against. Names mimic DriveFS's
// `fetch_temp` (`<digits>-<digits>-<digits>`) so name length is not a variable.
func generateControl(dir string, count int) error {
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return err
	}
	start := time.Now()
	for i := 0; i < count; i++ {
		name := filepath.Join(dir, fmt.Sprintf("70154-%d-%06d", i, i%1000000))
		f, err := os.OpenFile(name, os.O_CREATE|os.O_WRONLY|os.O_EXCL, 0o644)
		if err != nil && !os.IsExist(err) {
			return err
		}
		if f != nil {
			if err := f.Close(); err != nil {
				return err
			}
		}
	}
	fmt.Fprintf(os.Stderr, "created %d empty files in %s (%.1fs)\n", count, dir, time.Since(start).Seconds())
	return nil
}

func parseMethods(spec string) ([]Method, error) {
	var out []Method
	for _, raw := range strings.Split(spec, ",") {
		name := strings.TrimSpace(raw)
		if name == "" {
			continue
		}
		m := Method(name)
		switch m {
		case MethodEnumerate, MethodLstat, MethodBulk:
			out = append(out, m)
		default:
			return nil, fmt.Errorf("unknown method %q (want enumerate, lstat, or bulk)", name)
		}
	}
	if len(out) == 0 {
		return nil, fmt.Errorf("no methods selected")
	}
	return out, nil
}

// crossCheck compares the totals the methods produced for one directory. The
// entry counts and byte sums must agree; when they don't, either the directory
// changed under us (likely on a churny target) or the bulk parser is wrong, and
// both are worth surfacing rather than averaging away.
func crossCheck(dir string, results []Result) []string {
	byMethod := map[Method][]Result{}
	for _, r := range results {
		byMethod[r.Method] = append(byMethod[r.Method], r)
	}
	var warnings []string
	spread := func(name string, wantsAttrs bool, pick func(Result) int64) {
		var lo, hi int64
		first := true
		for method, rs := range byMethod {
			// `enumerate` reads no attributes and reports zero bytes by
			// construction, so it must not drag a byte comparison to zero:
			// that would silently disable the check that the bulk parser agrees
			// with lstat, which is the one cross-check that matters.
			if wantsAttrs && method == MethodEnumerate {
				continue
			}
			for _, r := range rs {
				v := pick(r)
				if first || v < lo {
					lo = v
				}
				if first || v > hi {
					hi = v
				}
				first = false
			}
		}
		if first || lo == hi || hi == 0 {
			return
		}
		warnings = append(warnings, fmt.Sprintf("%s: %s differ across runs/methods (%d .. %d, %.4f%% spread)",
			dir, name, lo, hi, 100*float64(hi-lo)/float64(hi)))
	}
	spread("entries", false, func(r Result) int64 { return r.Entries })
	spread("logical bytes", true, func(r Result) int64 { return r.LogicalBytes })
	spread("physical bytes", true, func(r Result) int64 { return r.PhysicalBytes })
	return warnings
}

func report(results []Result, md bool) string {
	if len(results) == 0 {
		return "no results\n"
	}
	sort.SliceStable(results, func(i, j int) bool {
		if results[i].Dir != results[j].Dir {
			return results[i].Dir < results[j].Dir
		}
		if results[i].Method != results[j].Method {
			return results[i].Method < results[j].Method
		}
		return results[i].Run < results[j].Run
	})

	header := []string{"directory", "method", "run", "wall", "cpu", "cpu %", "entries", "dirs", "logical bytes", "physical bytes", "µs/entry"}
	rows := [][]string{}
	for _, r := range results {
		rows = append(rows, []string{
			shortenPath(r.Dir),
			string(r.Method),
			fmt.Sprintf("%d", r.Run),
			fmt.Sprintf("%.2fs", r.Duration.Seconds()),
			fmt.Sprintf("%.2fs", r.CPUTime.Seconds()),
			fmt.Sprintf("%.0f%%", r.CPUPercent()),
			group(r.Entries),
			group(r.Dirs),
			group(r.LogicalBytes),
			group(r.PhysicalBytes),
			fmt.Sprintf("%.1f", r.MicrosPerEntry()),
		})
	}
	if md {
		return renderMarkdown(header, rows)
	}
	return renderText(header, rows)
}

func renderMarkdown(header []string, rows [][]string) string {
	var b strings.Builder
	b.WriteString("| " + strings.Join(header, " | ") + " |\n")
	b.WriteString("|" + strings.Repeat(" --- |", len(header)) + "\n")
	for _, row := range rows {
		b.WriteString("| " + strings.Join(row, " | ") + " |\n")
	}
	return b.String()
}

func renderText(header []string, rows [][]string) string {
	widths := make([]int, len(header))
	for i, h := range header {
		widths[i] = len(h)
	}
	for _, row := range rows {
		for i, cell := range row {
			if len(cell) > widths[i] {
				widths[i] = len(cell)
			}
		}
	}
	var b strings.Builder
	writeRow := func(cells []string) {
		for i, cell := range cells {
			if i > 0 {
				b.WriteString("  ")
			}
			b.WriteString(fmt.Sprintf("%-*s", widths[i], cell))
		}
		b.WriteString("\n")
	}
	writeRow(header)
	sep := make([]string, len(header))
	for i := range sep {
		sep[i] = strings.Repeat("-", widths[i])
	}
	writeRow(sep)
	for _, row := range rows {
		writeRow(row)
	}
	return b.String()
}

// shortenPath keeps the table readable without losing which target a row is.
func shortenPath(p string) string {
	parts := strings.Split(strings.TrimSuffix(p, "/"), "/")
	if len(parts) <= 2 {
		return p
	}
	return ".../" + strings.Join(parts[len(parts)-2:], "/")
}

// group formats an integer with thin thousands separators.
func group(n int64) string {
	s := fmt.Sprintf("%d", n)
	neg := strings.HasPrefix(s, "-")
	s = strings.TrimPrefix(s, "-")
	var out []string
	for len(s) > 3 {
		out = append([]string{s[len(s)-3:]}, out...)
		s = s[:len(s)-3]
	}
	out = append([]string{s}, out...)
	joined := strings.Join(out, " ")
	if neg {
		return "-" + joined
	}
	return joined
}
