package checks

import (
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"sort"
	"strings"
)

// The copy-paste detector, shared by the Rust lane (`desktop-rust-jscpd.go`) and
// the frontend lane (`desktop-svelte-jscpd.go`).
//
// The product of this check is the CLONE LIST: which two files say the same thing,
// at which lines. An aggregate duplication percentage answers no question anybody
// has — nobody refactors a percentage — so the percentage rides along in the
// headline while the list carries the value.
//
// Warn-only, with a per-lane allowlist that turns the standing inventory (a wall
// nobody reads) into a delta (a handful of pairs somebody can act on today).

// jscpdVersion pins the jscpd CLI. It MUST stay pinned (repo policy:
// checks/CLAUDE.md § "every tool install pins --version"). An unpinned `npx
// jscpd` pulled the 5.x rewrite — whose CLI renamed/removed flags (`--ignore`,
// `--reporters` gone) — and the arg-parse error got misreported as a
// duplication failure, reddening CI for a no-Rust-change commit. Bump
// deliberately and re-validate the flags below against the new major.
const jscpdVersion = "4.2.3"

// jscpdPairSeparator joins the two paths of a file pair into one allowlist key.
// A path can't contain it, so the key round-trips by eye without needing to parse.
const jscpdPairSeparator = " ↔ "

// jscpdInventoryPairs is how many pairs a passing run lists. Enough to name the
// real targets, few enough that the message stays something a reader finishes.
// The complete inventory is the allowlist JSON itself.
const jscpdInventoryPairs = 10

// jscpdLane is one detector configuration: what it reads, how sensitive it is, and
// which allowlist holds its accepted duplication.
type jscpdLane struct {
	// checkID is the registry ID, used to resolve the lane's source roots.
	checkID string
	// allowlistName names `<name>-allowlist.json` beside this file.
	allowlistName string
	// what names the lane in prose ("Rust", "frontend").
	what string
	// roots returns the absolute source roots to scan.
	roots func(rootDir string) ([]string, error)
	// formats is jscpd's `--format` list. `.svelte` files tokenize into three
	// sub-formats (typescript, css, markup), so the frontend lane sees component
	// script, style, AND markup duplication from the one `svelte` entry.
	formats string
	// minLines and minTokens set the detection floor. Tokens is the real dial;
	// min-lines only keeps a dense one-liner pair out.
	minLines  int
	minTokens int
	// ignore is jscpd's comma-separated `--ignore` glob list. Comma-separated (not
	// a brace alternation) because jscpd splits on commas, which would break a
	// `{...}` group.
	ignore string
}

// jscpdLocation is one end of a clone: where the duplicated fragment sits.
type jscpdLocation struct {
	Path  string
	Start int
	End   int
}

func (l jscpdLocation) String() string {
	return fmt.Sprintf("%s:%d-%d", l.Path, l.Start, l.End)
}

// jscpdClone is one duplicated fragment, found in two places.
type jscpdClone struct {
	Format string
	Lines  int
	A      jscpdLocation
	B      jscpdLocation
}

// jscpdPair is every clone between one pair of files (or within one file, when
// both ends are the same path).
type jscpdPair struct {
	key    string
	clones []jscpdClone
	lines  int
}

// jscpdTotals is what jscpd measured overall, for the headline.
type jscpdTotals struct {
	sources         int
	clones          int
	duplicatedLines int
	percentage      float64
}

// jscpdReport is one measurement: the clones bucketed by file pair, worst first.
type jscpdReport struct {
	pairs  []jscpdPair
	clones []jscpdClone
	// linesByPair covers every pair found, so shrink-wrap can tell "this pair is
	// clean now" from "this pair never existed".
	linesByPair map[string]int
	totals      jscpdTotals
}

// jscpdAllowlist is the on-disk shape of `<lane>-allowlist.json`. `Pairs` maps a
// file pair to the duplicated line count it may carry.
//
// **The key is the file PAIR, not the clone.** A clone's location moves the moment
// anything above it changes, and its content changes the moment somebody renames a
// variable in both copies, so both would churn the JSON on edits that changed no
// duplication. A pair of paths only changes when a file is renamed or deleted.
//
// **The value is duplicated LINES, not a clone count.** One number then catches
// both regressions that matter: a new duplicated block between the same two files
// raises it, and an existing block growing raises it too. A clone count would miss
// the second entirely.
type jscpdAllowlist struct {
	Comment string         `json:"$comment,omitempty"`
	Pairs   map[string]int `json:"pairs"`
}

func jscpdAllowlistRelPath(name string) string {
	return filepath.ToSlash(filepath.Join("scripts", "check", "checks", name+"-allowlist.json"))
}

func jscpdAllowlistPath(rootDir, name string) string {
	return filepath.Join(rootDir, filepath.FromSlash(jscpdAllowlistRelPath(name)))
}

// loadJscpdAllowlist reads a lane's allowlist. A missing or unparsable file yields
// an empty allowlist, which reports every pair as unlisted.
func loadJscpdAllowlist(rootDir, name string) jscpdAllowlist {
	list := jscpdAllowlist{Pairs: map[string]int{}}
	data, err := os.ReadFile(jscpdAllowlistPath(rootDir, name))
	if err != nil {
		return list
	}
	if err := json.Unmarshal(data, &list); err != nil {
		return jscpdAllowlist{Pairs: map[string]int{}}
	}
	if list.Pairs == nil {
		list.Pairs = map[string]int{}
	}
	return list
}

// jscpdPairKey is a clone's stable identity: the two paths, sorted so the order
// jscpd happened to report them in doesn't mint a second entry. A clone with both
// ends in one file keys on that single path.
func jscpdPairKey(a, b string) string {
	if a == b {
		return a
	}
	if a > b {
		a, b = b, a
	}
	return a + jscpdPairSeparator + b
}

// summarizeJscpdClones buckets clones by file pair, worst (most duplicated lines)
// first, ties broken by key so the output is stable run to run.
func summarizeJscpdClones(clones []jscpdClone) jscpdReport {
	report := jscpdReport{clones: clones, linesByPair: map[string]int{}}
	buckets := map[string]*jscpdPair{}
	for _, clone := range clones {
		key := jscpdPairKey(clone.A.Path, clone.B.Path)
		bucket, ok := buckets[key]
		if !ok {
			bucket = &jscpdPair{key: key}
			buckets[key] = bucket
		}
		bucket.clones = append(bucket.clones, clone)
		bucket.lines += clone.Lines
		report.linesByPair[key] += clone.Lines
	}
	for _, bucket := range buckets {
		sort.Slice(bucket.clones, func(i, j int) bool {
			return bucket.clones[i].Lines > bucket.clones[j].Lines
		})
		report.pairs = append(report.pairs, *bucket)
	}
	sort.Slice(report.pairs, func(i, j int) bool {
		if report.pairs[i].lines != report.pairs[j].lines {
			return report.pairs[i].lines > report.pairs[j].lines
		}
		return report.pairs[i].key < report.pairs[j].key
	})
	return report
}

// jscpdRawFile is one end of a clone as jscpd's JSON reporter writes it.
type jscpdRawFile struct {
	Name  string `json:"name"`
	Start int    `json:"start"`
	End   int    `json:"end"`
}

// jscpdRawReport is the JSON reporter's output. The console reporter prints only
// the aggregate, which is why this lane asks for JSON.
type jscpdRawReport struct {
	Statistics struct {
		Total struct {
			Sources         int     `json:"sources"`
			Clones          int     `json:"clones"`
			DuplicatedLines int     `json:"duplicatedLines"`
			Percentage      float64 `json:"percentage"`
		} `json:"total"`
	} `json:"statistics"`
	Duplicates []struct {
		Format     string       `json:"format"`
		Lines      int          `json:"lines"`
		FirstFile  jscpdRawFile `json:"firstFile"`
		SecondFile jscpdRawFile `json:"secondFile"`
	} `json:"duplicates"`
}

// normalizeJscpdPath makes the reporter's path match what a repo-relative
// allowlist key looks like. jscpd relativizes against the process working
// directory, which the lane pins to the repo root; the `./` prefix is the only
// variation left, and left alone it would key a second entry for the same file.
func normalizeJscpdPath(name string) string {
	return strings.TrimPrefix(filepath.ToSlash(name), "./")
}

// jscpdSpan builds a location, ordering the two ends. jscpd reports a handful of
// intra-file clones backwards (`start` 105, `end` 51, with the byte positions
// agreeing), and printed verbatim that reads like the tool is broken.
func jscpdSpan(file jscpdRawFile) jscpdLocation {
	start, end := file.Start, file.End
	if start > end {
		start, end = end, start
	}
	return jscpdLocation{Path: normalizeJscpdPath(file.Name), Start: start, End: end}
}

// parseJscpdReport turns the JSON reporter's output into clones plus the totals
// for the headline.
func parseJscpdReport(data []byte) ([]jscpdClone, jscpdTotals, error) {
	var raw jscpdRawReport
	if err := json.Unmarshal(data, &raw); err != nil {
		return nil, jscpdTotals{}, fmt.Errorf("parse jscpd report: %w", err)
	}
	clones := make([]jscpdClone, 0, len(raw.Duplicates))
	for _, dup := range raw.Duplicates {
		clones = append(clones, jscpdClone{
			Format: dup.Format,
			Lines:  dup.Lines,
			A:      jscpdSpan(dup.FirstFile),
			B:      jscpdSpan(dup.SecondFile),
		})
	}
	totals := jscpdTotals{
		sources:         raw.Statistics.Total.Sources,
		clones:          raw.Statistics.Total.Clones,
		duplicatedLines: raw.Statistics.Total.DuplicatedLines,
		percentage:      raw.Statistics.Total.Percentage,
	}
	return clones, totals, nil
}

// shrinkwrapJscpdAllowlist drops pairs that carry no duplication any more and
// ratchets every surviving entry down to what's actually there. No slack buffer:
// a duplicated-line count only moves when duplication is added or removed, so
// there's no drift for a buffer to absorb — and a buffer would let a clone grow
// for free. It mutates list in place and returns one line per change.
func shrinkwrapJscpdAllowlist(list *jscpdAllowlist, report jscpdReport) []string {
	var changes []string
	for _, key := range sortedKeys(list.Pairs) {
		allowed := list.Pairs[key]
		current := report.linesByPair[key]
		switch {
		case current == 0:
			delete(list.Pairs, key)
			changes = append(changes, fmt.Sprintf("removed %s (no duplication left)", key))
		case current < allowed:
			list.Pairs[key] = current
			changes = append(changes, fmt.Sprintf("ratcheted %s: %d → %d duplicated lines", key, allowed, current))
		}
	}
	return changes
}

// jscpdRegression is one file pair carrying more duplication than it's allowed.
type jscpdRegression struct {
	pair    jscpdPair
	allowed int
	listed  bool
}

// findJscpdRegressions returns every pair over its allowed line count, worst
// overshoot first. A pair missing from the allowlist is a regression too: entries
// are added deliberately, never by a check.
func findJscpdRegressions(report jscpdReport, list jscpdAllowlist) []jscpdRegression {
	var out []jscpdRegression
	for _, pair := range report.pairs {
		allowed, listed := list.Pairs[pair.key]
		if listed && pair.lines <= allowed {
			continue
		}
		out = append(out, jscpdRegression{pair: pair, allowed: allowed, listed: listed})
	}
	sort.Slice(out, func(i, j int) bool {
		a, b := out[i].pair.lines-out[i].allowed, out[j].pair.lines-out[j].allowed
		if a != b {
			return a > b
		}
		return out[i].pair.key < out[j].pair.key
	})
	return out
}

// formatJscpdRegressions renders the warn body: what gained duplication, and every
// clone behind it with `file:line`. This is the actionable half of the check, so
// nothing here is summarized away — a regression is a handful of pairs, and the
// reader needs the exact spans to go extract them.
func formatJscpdRegressions(regressions []jscpdRegression) string {
	var sb strings.Builder
	fmt.Fprintf(&sb, "%d file %s gained duplication:", len(regressions),
		Pluralize(len(regressions), "pair", "pairs"))
	for _, regression := range regressions {
		against := fmt.Sprintf("allowlist: %d, +%d", regression.allowed, regression.pair.lines-regression.allowed)
		if !regression.listed {
			against = "not in the allowlist"
		}
		fmt.Fprintf(&sb, "\n  - %s%d duplicated %s%s in %d %s (%s)",
			ansiYellow, regression.pair.lines, Pluralize(regression.pair.lines, "line", "lines"), ansiReset,
			len(regression.pair.clones), Pluralize(len(regression.pair.clones), "clone", "clones"), against)
		for _, clone := range regression.pair.clones {
			fmt.Fprintf(&sb, "\n      %s  ↔  %s", clone.A, clone.B)
		}
	}
	sb.WriteString("\nExtract the shared code, or get David's OK to raise the number " +
		"(`.claude/rules/file-length-allowlist.md`).")
	return sb.String()
}

// formatJscpdHeadline is the one-line scale of the lane's standing duplication.
func formatJscpdHeadline(report jscpdReport, what string) string {
	var sb strings.Builder
	fmt.Fprintf(&sb, "%s %s clones, %s duplicated %s (%.2f%%) across %s %s in %s file %s",
		formatThousands(report.totals.clones), what,
		formatThousands(report.totals.duplicatedLines),
		Pluralize(report.totals.duplicatedLines, "line", "lines"),
		report.totals.percentage,
		formatThousands(report.totals.sources), Pluralize(report.totals.sources, "file", "files"),
		formatThousands(len(report.pairs)), Pluralize(len(report.pairs), "pair", "pairs"))
	return sb.String()
}

// formatJscpdInventory renders the headline plus the worst pairs. It's what a
// PASSING run says under `pnpm check -v`: a standing list of where this lane's
// duplication actually lives, ranked, with a location to open. A warn deliberately
// doesn't print it — the delta is the message there, and burying three new lines
// under ten standing ones is how a check teaches people to skip it. The complete
// inventory is the allowlist JSON: every pair is a line in it.
func formatJscpdInventory(report jscpdReport, what string) string {
	var sb strings.Builder
	sb.WriteString(formatJscpdHeadline(report, what))
	for _, pair := range report.pairs[:min(jscpdInventoryPairs, len(report.pairs))] {
		widest := pair.clones[0]
		fmt.Fprintf(&sb, "\n  %4d %s in %2d %s  %s  ↔  %s",
			pair.lines, Pluralize(pair.lines, "line", "lines"),
			len(pair.clones), Pluralize(len(pair.clones), "clone", "clones"),
			widest.A, widest.B)
	}
	return sb.String()
}

// runJscpd invokes the pinned jscpd over the lane's roots and reads its JSON
// report back. The report goes to a per-invocation temp dir: two lanes run
// concurrently, and a fixed output path would have each read the other's file.
func runJscpd(ctx *CheckContext, lane jscpdLane) ([]jscpdClone, jscpdTotals, error) {
	roots, err := lane.roots(ctx.RootDir)
	if err != nil {
		return nil, jscpdTotals{}, err
	}
	if len(roots) == 0 {
		return nil, jscpdTotals{}, fmt.Errorf("%s has no source roots to scan", lane.checkID)
	}
	relRoots := make([]string, 0, len(roots))
	for _, root := range roots {
		rel, relErr := filepath.Rel(ctx.RootDir, root)
		if relErr != nil {
			return nil, jscpdTotals{}, relErr
		}
		relRoots = append(relRoots, filepath.ToSlash(rel))
	}

	jscpdSpec := "jscpd@" + jscpdVersion
	probe := exec.Command("npx", jscpdSpec, "--version")
	probe.Dir = ctx.RootDir
	if _, probeErr := RunCommand(probe, true); probeErr != nil {
		install := exec.Command("npm", "install", "-g", jscpdSpec)
		if _, installErr := RunCommand(install, true); installErr != nil {
			return nil, jscpdTotals{}, fmt.Errorf("failed to install %s: %w", jscpdSpec, installErr)
		}
	}

	outDir, err := os.MkdirTemp("", "jscpd-report-")
	if err != nil {
		return nil, jscpdTotals{}, err
	}
	defer func() { _ = os.RemoveAll(outDir) }()

	args := append([]string{jscpdSpec}, relRoots...)
	args = append(args,
		"--format", lane.formats,
		"--min-lines", fmt.Sprintf("%d", lane.minLines),
		"--min-tokens", fmt.Sprintf("%d", lane.minTokens),
		// The allowlist is the gate, so jscpd's own percentage gate is off. 100 is
		// the "never exit non-zero for duplication" setting; a real tool error still
		// exits non-zero and surfaces verbatim below.
		"--threshold", "100",
		"--ignore", lane.ignore,
		"--reporters", "json",
		"--output", outDir,
		"--silent",
	)
	cmd := exec.Command("npx", args...)
	cmd.Dir = ctx.RootDir
	output, err := RunCommand(cmd, true)
	if err != nil {
		return nil, jscpdTotals{}, fmt.Errorf("jscpd failed\n%s", indentOutput(output))
	}

	data, err := os.ReadFile(filepath.Join(outDir, "jscpd-report.json"))
	if err != nil {
		return nil, jscpdTotals{}, fmt.Errorf("jscpd wrote no JSON report\n%s", indentOutput(output))
	}
	return parseJscpdReport(data)
}

// runJscpdLane is the whole check body, shared by both lanes: measure, shrink-wrap
// the allowlist, and either report the delta (warn) or the standing inventory
// (pass).
func runJscpdLane(ctx *CheckContext, lane jscpdLane) (CheckResult, error) {
	clones, totals, err := runJscpd(ctx, lane)
	if err != nil {
		return CheckResult{}, err
	}
	report := summarizeJscpdClones(clones)
	report.totals = totals

	allowlist := loadJscpdAllowlist(ctx.RootDir, lane.allowlistName)
	staleChanges := shrinkwrapJscpdAllowlist(&allowlist, report)
	madeChanges := false
	if len(staleChanges) > 0 && !ctx.CI {
		if err := writeJSONAllowlist(jscpdAllowlistPath(ctx.RootDir, lane.allowlistName), allowlist); err != nil {
			return CheckResult{}, err
		}
		reformatWithOxfmt(ctx.RootDir, jscpdAllowlistRelPath(lane.allowlistName))
		madeChanges = true
	}

	var staleMsg string
	if len(staleChanges) > 0 {
		verb := "Shrink-wrapped allowlist"
		if ctx.CI {
			verb = "Stale allowlist entries (a local run shrink-wraps them)"
		}
		staleMsg = fmt.Sprintf("%s:\n  - %s", verb, strings.Join(staleChanges, "\n  - "))
	}

	regressions := findJscpdRegressions(report, allowlist)

	if len(regressions) == 0 {
		inventory := formatJscpdInventory(report, lane.what)
		if staleMsg != "" {
			msg := inventory + "\n" + staleMsg
			if ctx.CI {
				return CheckResult{Code: ResultWarning, Message: msg, Total: totals.clones, Issues: 0, Changes: -1}, nil
			}
			return SuccessWithChanges(msg), nil
		}
		return Success(inventory), nil
	}

	msg := formatJscpdRegressions(regressions) + "\n" + formatJscpdHeadline(report, lane.what)
	if staleMsg != "" {
		msg += "\n" + staleMsg
	}
	return CheckResult{
		Code:        ResultWarning,
		Message:     msg,
		MadeChanges: madeChanges,
		Total:       totals.clones,
		Issues:      len(regressions),
		Changes:     -1,
	}, nil
}
