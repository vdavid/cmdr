package checks

import (
	"encoding/json"
	"fmt"
	"math"
	"os"
	"path"
	"path/filepath"
	"sort"
	"strings"
)

// The invariant-density gauge. Every ❌ rule in an agent doc is an invariant the
// type system doesn't hold: it costs tokens in every session that loads the doc,
// it fails open (nothing stops the next author from ignoring it), and it rots
// silently when the code moves on. Counting the markers per subsystem, normalized
// by the size of the code the docs sit beside, turns "this module is tangled" into
// a number that can be driven down by making the invariant unrepresentable.
//
// Warn-only. Local runs lower an entry to the current count and drop a subsystem
// that reaches zero. ❗ Unlike every other allowlist in this package, RAISING an
// entry here needs no sign-off: a rule earns its place on whether the invariant is
// worth stating, which the number can't judge
// (`.claude/rules/file-length-allowlist.md`).

const (
	// invariantRuleMarker is the house convention for "never do X" in an agent doc.
	// Counting the marker rather than the prose keeps the gauge out of the business
	// of parsing English; unmarked prohibitions are undercounted, which is one more
	// reason to keep marking them.
	invariantRuleMarker = "❌"
	// invariantCautionMarker is the ⚠️ base rune. Matching the rune rather than the
	// full emoji sequence counts a marker written with or without the U+FE0F
	// variation selector.
	invariantCautionMarker = "\u26a0"
)

// invariantDocNames are the docs this gauge reads: the colocated push/pull tier
// pair, plus `AGENTS.md`, which is the root `CLAUDE.md` in all but name (the root
// `CLAUDE.md` is a bare `@AGENTS.md` import). Leaving `AGENTS.md` out would let the
// repo's most-read rule list grow untracked.
var invariantDocNames = map[string]bool{
	"CLAUDE.md":  true,
	"DETAILS.md": true,
	"AGENTS.md":  true,
}

// invariantSubsystemManifests name a directory as a subsystem root. A directory
// that declares itself a build unit is a boundary somebody owns, it has a stable
// name that survives refactors inside it, and it comes with a natural denominator
// (the source under it). Everything outside one lands in the repo-root bucket.
var invariantSubsystemManifests = map[string]bool{
	"Cargo.toml":   true,
	"package.json": true,
}

// invariantExtraSubsystemRoots name directories that become subsystem roots without
// declaring a manifest. The escape hatch exists because a build unit is sometimes
// coarser than the boundary somebody owns: one `apps/desktop/package.json` covers
// the Svelte frontend, the test harness, and the app's build scripts, so without a
// split the frontend's rules and the E2E harness's rules share a row and neither
// number answers a question anybody has.
//
// Keep the list short, and only add a directory a reader would name out loud. It's
// a Go constant rather than a section of the allowlist JSON on purpose: that file is
// a self-rewriting record of accepted counts, and bucket geometry is not something a
// shrink-wrap pass may move. An entry whose directory is gone is inert (its bucket
// stays empty, and shrink-wrap drops the allowlist entry on the next local run).
var invariantExtraSubsystemRoots = []string{
	"apps/desktop/src",
	"apps/desktop/test",
}

// invariantRepoRootBucket is the catch-all subsystem: docs and source that sit
// under no build unit (`docs/`, `scripts/`, `brand/`, the root `AGENTS.md`).
const invariantRepoRootBucket = "."

// invariantSubsystem is one bucket of the gauge.
type invariantSubsystem struct {
	root        string
	rules       int
	cautions    int
	docs        int
	sourceLines int
}

// rulesPerKiloLine is the comparable number: rules per 1,000 source lines of the
// code the docs sit beside. A big subsystem is allowed more rules than a small
// one; carrying three times the rules per line of code is what stands out.
//
// Rules with no source under them at all have no denominator, so they report as
// infinite: that ranks the subsystem first and prints "n/a" instead of a
// flattering 0.00.
func (s invariantSubsystem) rulesPerKiloLine() float64 {
	if s.sourceLines == 0 {
		return math.Inf(1)
	}
	return float64(s.rules) * 1000 / float64(s.sourceLines)
}

// formatDensity renders a density cell, spelling out the no-denominator case.
func formatDensity(density float64) string {
	if math.IsInf(density, 1) {
		return "n/a"
	}
	return fmt.Sprintf("%.2f", density)
}

// invariantDoc is one agent doc's marker count, for naming the heaviest docs in a
// regressed subsystem.
type invariantDoc struct {
	relPath string
	rules   int
}

// invariantDensityReport is one measurement of the whole repo.
type invariantDensityReport struct {
	// subsystems carry at least one rule, worst density first.
	subsystems []invariantSubsystem
	// rulesByRoot covers EVERY discovered root, including the ones at zero, so
	// shrink-wrap can tell "this subsystem is clean now" from "this subsystem is
	// gone".
	rulesByRoot map[string]int
	// docsByRoot lists each subsystem's docs, rule-heaviest first.
	docsByRoot    map[string][]invariantDoc
	totalRules    int
	totalCautions int
	totalDocs     int
	totalLines    int
}

// invariantDensityAllowlist is the on-disk shape of
// invariant-density-allowlist.json. `Subsystems` maps a subsystem root to the
// accepted ❌ count: the contract that subsystem may not silently grow past.
type invariantDensityAllowlist struct {
	Comment    string         `json:"$comment,omitempty"`
	Subsystems map[string]int `json:"subsystems"`
}

func invariantDensityAllowlistPath(rootDir string) string {
	return filepath.Join(rootDir, "scripts", "check", "checks", "invariant-density-allowlist.json")
}

// loadInvariantDensityAllowlist reads the allowlist JSON. A missing or unparsable
// file yields an empty allowlist, which reports every subsystem as unlisted.
func loadInvariantDensityAllowlist(rootDir string) invariantDensityAllowlist {
	var list invariantDensityAllowlist
	data, err := os.ReadFile(invariantDensityAllowlistPath(rootDir))
	if err != nil {
		return list
	}
	if err := json.Unmarshal(data, &list); err != nil {
		return invariantDensityAllowlist{}
	}
	return list
}

// invariantSubsystemRoots collects every directory holding a subsystem manifest,
// except the repo root itself (which is the catch-all bucket, not a subsystem), plus
// the manifest-less roots named in invariantExtraSubsystemRoots.
func invariantSubsystemRoots(relPaths []string) []string {
	seen := make(map[string]bool)
	for _, root := range invariantExtraSubsystemRoots {
		seen[root] = true
	}
	for _, rel := range relPaths {
		if !invariantSubsystemManifests[path.Base(rel)] {
			continue
		}
		if dir := path.Dir(rel); dir != invariantRepoRootBucket {
			seen[dir] = true
		}
	}
	return sortedKeys(seen)
}

// invariantSubsystemFor returns the longest root that contains rel, or the
// repo-root bucket. Longest wins so a Rust member nested inside a JS package
// (`apps/desktop/src-tauri`) is its own subsystem rather than folding into its
// parent.
func invariantSubsystemFor(rel string, roots []string) string {
	best := invariantRepoRootBucket
	bestLen := -1
	for _, root := range roots {
		if len(root) <= bestLen {
			continue
		}
		if rel == root || strings.HasPrefix(rel, root+"/") {
			best, bestLen = root, len(root)
		}
	}
	return best
}

// countInvariantMarkers returns a doc's ❌ and ⚠️ counts, ignoring markers inside
// fenced blocks and inline code spans. That's the use/mention line: a rule is
// imposed in prose ("❌ Never do X"), while a marker in backticks is being talked
// ABOUT. Without it, the docs that explain the convention would be charged for
// explaining it.
func countInvariantMarkers(absPath string) (rules, cautions int, err error) {
	data, err := os.ReadFile(absPath)
	if err != nil {
		return 0, 0, err
	}
	prose := inlineCodeRe.ReplaceAllString(fencedCodeBlockRe.ReplaceAllString(string(data), ""), "")
	return strings.Count(prose, invariantRuleMarker), strings.Count(prose, invariantCautionMarker), nil
}

// invariantTally accumulates one measurement, bucketing every file it's handed
// into the nearest subsystem root.
type invariantTally struct {
	rootDir string
	roots   []string
	buckets map[string]*invariantSubsystem
	report  invariantDensityReport
}

func newInvariantTally(rootDir string, roots []string) *invariantTally {
	report := invariantDensityReport{
		rulesByRoot: make(map[string]int, len(roots)+1),
		docsByRoot:  make(map[string][]invariantDoc),
	}
	// Every known root starts at zero so shrink-wrap can tell "this subsystem is
	// clean now" (drop the entry) from "this subsystem is gone" (drop it too, but
	// say so differently).
	report.rulesByRoot[invariantRepoRootBucket] = 0
	for _, root := range roots {
		report.rulesByRoot[root] = 0
	}
	return &invariantTally{rootDir: rootDir, roots: roots, buckets: map[string]*invariantSubsystem{}, report: report}
}

func (t *invariantTally) bucketFor(rel string) *invariantSubsystem {
	root := invariantSubsystemFor(rel, t.roots)
	if bucket, ok := t.buckets[root]; ok {
		return bucket
	}
	bucket := &invariantSubsystem{root: root}
	t.buckets[root] = bucket
	return bucket
}

// addSourceFile grows the subsystem's denominator. A file that can't be read is
// skipped: a tracked file deleted locally isn't this gauge's business.
func (t *invariantTally) addSourceFile(rel string) {
	lines, err := countLines(filepath.Join(t.rootDir, filepath.FromSlash(rel)))
	if err != nil {
		return
	}
	t.bucketFor(rel).sourceLines += lines
	t.report.totalLines += lines
}

// addDoc counts one agent doc's markers into its subsystem.
func (t *invariantTally) addDoc(rel string) {
	ruleCount, cautionCount, err := countInvariantMarkers(filepath.Join(t.rootDir, filepath.FromSlash(rel)))
	if err != nil {
		return
	}
	bucket := t.bucketFor(rel)
	bucket.rules += ruleCount
	bucket.cautions += cautionCount
	bucket.docs++
	t.report.rulesByRoot[bucket.root] += ruleCount
	t.report.totalRules += ruleCount
	t.report.totalCautions += cautionCount
	t.report.totalDocs++
	if ruleCount > 0 {
		t.report.docsByRoot[bucket.root] = append(t.report.docsByRoot[bucket.root], invariantDoc{relPath: rel, rules: ruleCount})
	}
}

// finish ranks the subsystems worst density first, and each subsystem's docs
// rule-heaviest first. Subsystems with no rules are left out: the table is a list
// of places to look, and a clean subsystem isn't one.
func (t *invariantTally) finish() invariantDensityReport {
	for _, bucket := range t.buckets {
		if bucket.rules > 0 {
			t.report.subsystems = append(t.report.subsystems, *bucket)
		}
	}
	sort.Slice(t.report.subsystems, func(i, j int) bool {
		a, b := t.report.subsystems[i], t.report.subsystems[j]
		if a.rulesPerKiloLine() != b.rulesPerKiloLine() {
			return a.rulesPerKiloLine() > b.rulesPerKiloLine()
		}
		return a.root < b.root
	})
	for _, docs := range t.report.docsByRoot {
		sort.Slice(docs, func(i, j int) bool {
			if docs[i].rules != docs[j].rules {
				return docs[i].rules > docs[j].rules
			}
			return docs[i].relPath < docs[j].relPath
		})
	}
	return t.report
}

// measureInvariantDensity walks the repo once for source lines and once for agent
// docs, attributing each file to its nearest subsystem root.
func measureInvariantDensity(rootDir string) (invariantDensityReport, error) {
	relPaths, err := repoFiles(rootDir)
	if err != nil {
		return invariantDensityReport{}, err
	}
	docs, err := findMarkdownDocs(rootDir)
	if err != nil {
		return invariantDensityReport{}, err
	}

	tally := newInvariantTally(rootDir, invariantSubsystemRoots(relPaths))
	for _, rel := range relPaths {
		if fileLengthSourceExtensions[path.Ext(rel)] {
			tally.addSourceFile(rel)
		}
	}
	for _, rel := range docs {
		if invariantDocNames[path.Base(rel)] {
			tally.addDoc(rel)
		}
	}
	return tally.finish(), nil
}

// shrinkwrapInvariantDensityAllowlist drops entries for subsystems that are gone
// or clean, and ratchets every entry down to the current count. Unlike the
// length allowlists there's no slack buffer: a line count drifts with every edit,
// so it needs one, while a rule count only moves when somebody writes or deletes
// a rule. It mutates list in place and returns one line per change.
func shrinkwrapInvariantDensityAllowlist(list *invariantDensityAllowlist, report invariantDensityReport) []string {
	var changes []string
	for _, root := range sortedKeys(list.Subsystems) {
		allowed := list.Subsystems[root]
		current, known := report.rulesByRoot[root]
		switch {
		case !known:
			delete(list.Subsystems, root)
			changes = append(changes, fmt.Sprintf("removed %s (subsystem no longer exists)", root))
		case current == 0:
			delete(list.Subsystems, root)
			changes = append(changes, fmt.Sprintf("removed %s (no ❌ rules left)", root))
		case current < allowed:
			list.Subsystems[root] = current
			changes = append(changes, fmt.Sprintf("ratcheted %s: %d → %d rules", root, allowed, current))
		}
	}
	return changes
}

// invariantRegression is one subsystem carrying more rules than it's allowed.
type invariantRegression struct {
	subsystem invariantSubsystem
	allowed   int
	listed    bool
}

// findInvariantRegressions returns every subsystem over its allowed count, worst
// overshoot first. A subsystem missing from the allowlist is a regression too:
// entries are added deliberately, never by a check.
func findInvariantRegressions(report invariantDensityReport, list invariantDensityAllowlist) []invariantRegression {
	var out []invariantRegression
	for _, subsystem := range report.subsystems {
		allowed, listed := list.Subsystems[subsystem.root]
		if listed && subsystem.rules <= allowed {
			continue
		}
		out = append(out, invariantRegression{subsystem: subsystem, allowed: allowed, listed: listed})
	}
	sort.Slice(out, func(i, j int) bool {
		return out[i].subsystem.rules-out[i].allowed > out[j].subsystem.rules-out[j].allowed
	})
	return out
}

// formatInvariantGauge renders the headline plus the per-subsystem table, worst
// density first. This IS the gauge: it prints under `pnpm check -v` and on every
// warn.
func formatInvariantGauge(report invariantDensityReport) string {
	density := float64(0)
	if report.totalLines > 0 {
		density = float64(report.totalRules) * 1000 / float64(report.totalLines)
	}
	var sb strings.Builder
	fmt.Fprintf(&sb, "%s ❌ %s across %s agent %s: %.2f per 1,000 source lines (plus %s ⚠️)",
		formatThousands(report.totalRules), Pluralize(report.totalRules, "rule", "rules"),
		formatThousands(report.totalDocs), Pluralize(report.totalDocs, "doc", "docs"),
		density, formatThousands(report.totalCautions))

	width := len("subsystem")
	for _, subsystem := range report.subsystems {
		width = max(width, len(subsystem.root))
	}
	// Header cells stay emoji-free: a column padded to a rune count renders one
	// cell too narrow when that rune is double-width.
	const row = "\n  %-*s  %10s  %7s  %8s  %12s  %5s"
	fmt.Fprintf(&sb, row, width, "subsystem", "rules/kloc", "rules", "cautions", "src lines", "docs")
	for _, subsystem := range report.subsystems {
		fmt.Fprintf(&sb, row, width, subsystem.root,
			formatDensity(subsystem.rulesPerKiloLine()),
			formatThousands(subsystem.rules), formatThousands(subsystem.cautions),
			formatThousands(subsystem.sourceLines), formatThousands(subsystem.docs))
	}
	return sb.String()
}

// formatInvariantRegressions renders the warn body: what grew, by how much, and
// which docs in that subsystem carry the most rules (the places to look first).
func formatInvariantRegressions(regressions []invariantRegression, report invariantDensityReport) string {
	var sb strings.Builder
	fmt.Fprintf(&sb, "%d %s gained ❌ rules:", len(regressions),
		Pluralize(len(regressions), "subsystem", "subsystems"))
	for _, regression := range regressions {
		against := fmt.Sprintf("allowlist: %d, +%d", regression.allowed, regression.subsystem.rules-regression.allowed)
		if !regression.listed {
			against = "not in the allowlist"
		}
		fmt.Fprintf(&sb, "\n  - %s: %s%d %s%s (%s)", regression.subsystem.root,
			ansiYellow, regression.subsystem.rules,
			Pluralize(regression.subsystem.rules, "rule", "rules"), ansiReset, against)
		if heaviest := formatHeaviestDocs(report.docsByRoot[regression.subsystem.root]); heaviest != "" {
			fmt.Fprintf(&sb, "\n    heaviest docs: %s", heaviest)
		}
	}
	sb.WriteString("\nEach ❌ is an invariant the type system could hold instead. Encode it, delete a stale one, " +
		"or get David's OK to raise the number (`.claude/rules/file-length-allowlist.md`).")
	return sb.String()
}

// formatHeaviestDocs names the three rule-heaviest docs of a subsystem.
func formatHeaviestDocs(docs []invariantDoc) string {
	if len(docs) == 0 {
		return ""
	}
	parts := make([]string, 0, 3)
	for _, doc := range docs[:min(3, len(docs))] {
		parts = append(parts, fmt.Sprintf("%s (%d)", doc.relPath, doc.rules))
	}
	return strings.Join(parts, ", ")
}

// formatThousands renders n with thousands separators.
func formatThousands(n int) string {
	digits := fmt.Sprintf("%d", n)
	sign := ""
	if strings.HasPrefix(digits, "-") {
		sign, digits = "-", digits[1:]
	}
	var sb strings.Builder
	for i, r := range digits {
		if i > 0 && (len(digits)-i)%3 == 0 {
			sb.WriteByte(',')
		}
		sb.WriteRune(r)
	}
	return sign + sb.String()
}

// RunInvariantDensity gauges how many ❌ "never do X" rules each subsystem's agent
// docs carry, absolute and per 1,000 source lines. Warn-only: it reports a
// subsystem whose count rose above its allowlisted number (or that isn't listed
// yet), and outside CI it shrink-wraps the allowlist so every rule that got
// encoded in a type shows up as a number going down.
func RunInvariantDensity(ctx *CheckContext) (CheckResult, error) {
	report, err := measureInvariantDensity(ctx.RootDir)
	if err != nil {
		return CheckResult{}, fmt.Errorf("failed to measure invariant density: %w", err)
	}

	allowlist := loadInvariantDensityAllowlist(ctx.RootDir)
	staleChanges := shrinkwrapInvariantDensityAllowlist(&allowlist, report)
	madeChanges := false
	if len(staleChanges) > 0 && !ctx.CI {
		if err := writeJSONAllowlist(invariantDensityAllowlistPath(ctx.RootDir), allowlist); err != nil {
			return CheckResult{}, err
		}
		reformatWithOxfmt(ctx.RootDir, "scripts/check/checks/invariant-density-allowlist.json")
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

	gauge := formatInvariantGauge(report)
	regressions := findInvariantRegressions(report, allowlist)

	if len(regressions) == 0 {
		if staleMsg != "" {
			msg := gauge + "\n" + staleMsg
			if ctx.CI {
				return CheckResult{Code: ResultWarning, Message: msg, Total: report.totalDocs, Issues: 0, Changes: -1}, nil
			}
			return SuccessWithChanges(msg), nil
		}
		return Success(gauge), nil
	}

	msg := formatInvariantRegressions(regressions, report) + "\n" + gauge
	if staleMsg != "" {
		msg += "\n" + staleMsg
	}
	return CheckResult{
		Code:        ResultWarning,
		Message:     msg,
		MadeChanges: madeChanges,
		Total:       report.totalDocs,
		Issues:      len(regressions),
		Changes:     -1,
	}, nil
}
