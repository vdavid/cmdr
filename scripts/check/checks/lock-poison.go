package checks

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
)

// AllowLockPoisonComment is the magic comment that opts a single line out of the
// lock-poison check, in either lane. Place it on the line immediately above the
// flagged line, or as a trailing comment on the same line, with a short reason.
//
//	// allowed-lock-poison: nothing panics under this lock, proven by construction
//	let g = state.entries.lock().unwrap();
const AllowLockPoisonComment = "// allowed-lock-poison:"

// lockPoisonAllowlistRelPath is where the swallow lane's budget lives, next to
// the check source files.
const lockPoisonAllowlistRelPath = "scripts/check/checks/lock-poison-allowlist.json"

// lockBareUnwrapPattern matches a std-lock acquisition followed by a bare
// `.unwrap()`. The empty `()` between the method name and `.unwrap()` is what
// keeps `io::Read::read(&mut buf).unwrap()` / `io::Write::write(buf).unwrap()`
// (which carry arguments) and `mutex.lock().await` (tokio async, returns a
// future, no `.unwrap()`) out of scope. `try_lock` / `try_read` / `try_write`
// are out of scope by name (the `\b` before the verb won't match the `_`).
var lockBareUnwrapPattern = regexp.MustCompile(`\b(lock|read|write)\(\)\.unwrap\(\)`)

// lockExpectPattern captures the message argument of a `.lock().expect(<msg>)`
// (and read/write) so we can check whether it names "poison". Same empty-parens
// and verb-boundary constraints as the unwrap pattern. The message capture is
// non-greedy up to the next `"` so multiple expects on one line each get their
// own message checked by FindAllStringSubmatch.
var lockExpectPattern = regexp.MustCompile(`\b(lock|read|write)\(\)\.expect\(\s*"((?:[^"\\]|\\.)*)"`)

// The swallow lane's patterns. lockAcquirePattern finds the acquisitions
// themselves, under the same verb rules as the two above (empty parens, so
// `read(&mut buf)` is out; a leading `.`, so `try_lock()` is out). The two
// prefix patterns then say which construct is consuming the `Result`: they're
// anchored at the end of the text BEFORE an acquisition, and `[^;{}]*$` keeps
// them inside the same statement.
//
// lockOrDefaultPattern catches the combinator family that turns a failure into a
// value out of thin air: `unwrap_or`, `unwrap_or_default`, `unwrap_or_else`,
// `map_or`, `map_or_else`.
var (
	lockAcquirePattern   = regexp.MustCompile(`\.(?:lock|read|write)\(\)`)
	lockLetOkPrefix      = regexp.MustCompile(`\blet\s+Ok\s*\([^()]*\)\s*=\s*[^;{}]*$`)
	lockMatchPrefix      = regexp.MustCompile(`\bmatch\s+[^;{}]*$`)
	lockOrDefaultPattern = regexp.MustCompile(`\.(?:unwrap_or|map_or)`)
)

// lockChainLookahead bounds how many lines a combinator chain may wrap across
// before the parser gives up on it. Nothing in the tree comes close; the cap is
// there so a file with unbalanced brackets can't turn one site into a whole-file
// scan.
const lockChainLookahead = 12

// lockIntentMarkers are the fingerprints of the three outcomes the lock-poison
// policy sanctions when an acquisition fails (see the module doc of
// `crates/cmdr-fs/src/ignore_poison.rs`): recover the data, abort loudly, or
// hand the failure to the caller. A handler carrying none of them substitutes a
// default value, which is the silent swallow this lane exists to surface.
var lockIntentMarkers = []string{
	"into_inner", // recover: take the data anyway
	"ignore_poison",
	"panic!", // abort: loud, and the crash reporter sees it
	"unreachable!",
	"todo!",
	"expect(",
	"unwrap(",
	"Err(", // propagate: the caller decides
}

type lockPoisonSite struct {
	relPath string
	line    int
	text    string
}

// lockSwallowShape names which result-discarding form a site takes, so the
// report says what to look for rather than only where.
type lockSwallowShape string

const (
	swallowIfLet     lockSwallowShape = "if let Ok"
	swallowLetElse   lockSwallowShape = "let-else"
	swallowMatch     lockSwallowShape = "match"
	swallowOk        lockSwallowShape = ".ok()"
	swallowOrDefault lockSwallowShape = "unwrap_or"
)

type lockSwallowSite struct {
	relPath string
	line    int
	shape   lockSwallowShape
	text    string
}

// lockPoisonFileFindings is one scanned file's contribution to both lanes.
type lockPoisonFileFindings struct {
	violations []lockPoisonSite
	swallows   []lockSwallowSite
	orphans    []orphanDirective
}

// lockPoisonAllowlist is the on-disk shape of lock-poison-allowlist.json. It
// keys on the file, valued in the number of result-discarding sites that file is
// allowed to hold. A per-site key would move on every edit above the site; a
// per-file count only moves when somebody writes or removes a swallow.
type lockPoisonAllowlist struct {
	Comment string         `json:"$comment,omitempty"`
	Files   map[string]int `json:"files"`
}

// RunLockPoison holds every std `Mutex`/`RwLock` acquisition in non-test Rust
// code to the poison-handling policy documented in the module doc of
// `crates/cmdr-fs/src/ignore_poison.rs`. It runs two lanes over one scan:
//
//   - The intent lane FAILS on an acquisition that records no choice at all: a
//     bare `.unwrap()`, or an `.expect(<msg>)` whose message doesn't name poison.
//   - The swallow lane WARNS on an acquisition whose failure is silently
//     discarded (`if let Ok(…)`, a `match` arm that returns, a let-else, or
//     `.ok()`), budgeted by `lock-poison-allowlist.json` so the count can only
//     go down. Two shipped bugs came from this shape, so it's tracked rather
//     than tolerated.
func RunLockPoison(ctx *CheckContext) (CheckResult, error) {
	// Every first-party tree, app and crates alike: a poisoned lock aborts the same
	// process wherever it was acquired. The vendored fork is out of jurisdiction.
	roots, err := ScannerRoots(ctx.RootDir, "desktop-rust-lock-poison")
	if err != nil {
		return CheckResult{}, err
	}

	var violations []lockPoisonSite
	var swallows []lockSwallowSite
	var orphans []orphanDirective
	scanned := 0
	for _, root := range roots {
		rootFindings, rootScanned, scanErr := scanForLockPoison(ctx.RootDir, root)
		if scanErr != nil {
			return CheckResult{}, fmt.Errorf("failed to scan Rust files: %w", scanErr)
		}
		violations = append(violations, rootFindings.violations...)
		swallows = append(swallows, rootFindings.swallows...)
		orphans = append(orphans, rootFindings.orphans...)
		scanned += rootScanned
	}

	allowlist := loadLockPoisonAllowlist(ctx.RootDir)
	counts := countSwallowsPerFile(swallows)
	staleChanges := shrinkwrapLockPoisonAllowlist(&allowlist, counts)
	madeChanges := false
	if len(staleChanges) > 0 && !ctx.CI {
		if err := writeJSONAllowlist(filepath.Join(ctx.RootDir, lockPoisonAllowlistRelPath), allowlist); err != nil {
			return CheckResult{}, err
		}
		reformatWithOxfmt(ctx.RootDir, lockPoisonAllowlistRelPath)
		madeChanges = true
	}
	overBudget := swallowsOverBudget(swallows, allowlist)

	var hardParts []string
	if len(violations) > 0 {
		hardParts = append(hardParts, formatLockPoisonViolations(violations))
	}
	if len(orphans) > 0 {
		hardParts = append(hardParts, formatOrphanDirectives(AllowLockPoisonComment, orphans))
	}

	swallowMsg := ""
	if len(overBudget) > 0 {
		swallowMsg = formatLockSwallows(overBudget, allowlist)
	}
	staleMsg := formatLockPoisonStaleness(staleChanges, ctx.CI)

	if len(hardParts) > 0 {
		// The warn lane's findings ride along with the failure rather than
		// waiting for the next run: a reader fixing one usually fixes both.
		hardParts = appendNonEmpty(hardParts, swallowMsg, staleMsg)
		return CheckResult{}, fmt.Errorf("%s", strings.Join(hardParts, "\n"))
	}

	allowlistedCount := 0
	for _, n := range allowlist.Files {
		allowlistedCount += n
	}
	if swallowMsg != "" {
		msg := strings.Join(appendNonEmpty([]string{swallowMsg}, staleMsg), "\n")
		return CheckResult{Code: ResultWarning, Message: msg, MadeChanges: madeChanges, Total: -1, Issues: -1, Changes: -1}, nil
	}

	okMsg := fmt.Sprintf(
		"%d Rust %s scanned, every std-lock acquisition records poison-handling intent",
		scanned, Pluralize(scanned, "file", "files"),
	)
	if allowlistedCount > 0 {
		okMsg += fmt.Sprintf(" (%d discarded %s allowlisted)",
			allowlistedCount, Pluralize(allowlistedCount, "result", "results"))
	}
	if staleMsg != "" {
		return SuccessWithChanges(okMsg + "; " + staleMsg), nil
	}
	return Success(okMsg), nil
}

// appendNonEmpty appends the non-empty extras to parts, so the report builders
// can hand over optional sections without each one re-checking for "".
func appendNonEmpty(parts []string, extras ...string) []string {
	for _, e := range extras {
		if e != "" {
			parts = append(parts, e)
		}
	}
	return parts
}

func formatLockPoisonViolations(violations []lockPoisonSite) string {
	sort.Slice(violations, func(i, j int) bool {
		if violations[i].relPath == violations[j].relPath {
			return violations[i].line < violations[j].line
		}
		return violations[i].relPath < violations[j].relPath
	})
	var sb strings.Builder
	for _, v := range violations {
		sb.WriteString(fmt.Sprintf("  %s:%d: %s\n", v.relPath, v.line, v.text))
	}
	return fmt.Sprintf(
		"found %d std-lock %s acquired without recorded poison-handling intent "+
			"(use `lock_ignore_poison()` / `read_ignore_poison()` / `write_ignore_poison()` to recover, "+
			"or `.expect(\"<lock> poisoned: <why>\")` to abort deliberately; "+
			"add `%s <reason>` on the line above or as a trailing comment to opt a site out):\n%s",
		len(violations), Pluralize(len(violations), "site", "sites"), AllowLockPoisonComment,
		strings.TrimRight(sb.String(), "\n"),
	)
}

// formatLockSwallows lists the over-budget files, each followed by every
// discarding site it holds: the budget is per file, so which of its sites to fix
// is the reader's pick.
func formatLockSwallows(sites []lockSwallowSite, allowlist lockPoisonAllowlist) string {
	byFile := map[string][]lockSwallowSite{}
	for _, s := range sites {
		byFile[s.relPath] = append(byFile[s.relPath], s)
	}
	var sb strings.Builder
	for _, relPath := range sortedKeys(byFile) {
		fileSites := byFile[relPath]
		sort.Slice(fileSites, func(i, j int) bool { return fileSites[i].line < fileSites[j].line })
		sb.WriteString(fmt.Sprintf("  - %s: %d %s (allowlist: %d)\n",
			relPath, len(fileSites), Pluralize(len(fileSites), "site", "sites"), allowlist.Files[relPath]))
		for _, s := range fileSites {
			sb.WriteString(fmt.Sprintf("      %s:%d [%s] %s\n", relPath, s.line, s.shape, s.text))
		}
	}
	return fmt.Sprintf(
		"%d %s discard more poisoned-lock results than allowed "+
			"(on poison the block is skipped and the caller sees empty or default data; recover with "+
			"`lock_ignore_poison()`, hand the failure to the caller, or abort with `.expect(\"<lock> poisoned: <why>\")`; "+
			"`%s <reason>` opts one site out):\n%s",
		len(byFile), Pluralize(len(byFile), "file", "files"), AllowLockPoisonComment,
		strings.TrimRight(sb.String(), "\n"),
	)
}

func formatLockPoisonStaleness(changes []string, ci bool) string {
	if len(changes) == 0 {
		return ""
	}
	verb := "Shrink-wrapped allowlist"
	if ci {
		verb = "Stale allowlist entries (a local run shrink-wraps them)"
	}
	return fmt.Sprintf("%s:\n  - %s", verb, strings.Join(changes, "\n  - "))
}

// lockPoisonAllowlistPath returns the allowlist location for a repo root.
func lockPoisonAllowlistPath(rootDir string) string {
	return filepath.Join(rootDir, lockPoisonAllowlistRelPath)
}

// loadLockPoisonAllowlist reads the swallow-lane budget. A missing or unparsable
// file yields an empty allowlist, so every discarding site gets reported.
func loadLockPoisonAllowlist(rootDir string) lockPoisonAllowlist {
	var list lockPoisonAllowlist
	data, err := os.ReadFile(lockPoisonAllowlistPath(rootDir))
	if err != nil {
		return list
	}
	if err := json.Unmarshal(data, &list); err != nil {
		return lockPoisonAllowlist{}
	}
	return list
}

func countSwallowsPerFile(sites []lockSwallowSite) map[string]int {
	counts := map[string]int{}
	for _, s := range sites {
		counts[s.relPath]++
	}
	return counts
}

// shrinkwrapLockPoisonAllowlist drops entries whose file no longer discards
// anything and ratchets the rest down to the current count. There's no slack
// buffer: the number only moves when somebody writes or removes a swallow, so
// there's no drift to absorb.
func shrinkwrapLockPoisonAllowlist(list *lockPoisonAllowlist, counts map[string]int) []string {
	var changes []string
	for _, path := range sortedKeys(list.Files) {
		allowed := list.Files[path]
		current := counts[path]
		switch {
		case current == 0:
			delete(list.Files, path)
			changes = append(changes, fmt.Sprintf("removed %s (no discarded lock results left)", path))
		case current < allowed:
			list.Files[path] = current
			changes = append(changes, fmt.Sprintf("ratcheted %s: %d → %d", path, allowed, current))
		}
	}
	return changes
}

// swallowsOverBudget returns every site in a file holding more discarding sites
// than its allowlist entry permits. A file absent from the allowlist has a
// budget of zero, so a newly written swallow surfaces immediately.
func swallowsOverBudget(sites []lockSwallowSite, allowlist lockPoisonAllowlist) []lockSwallowSite {
	counts := countSwallowsPerFile(sites)
	var out []lockSwallowSite
	for _, s := range sites {
		if counts[s.relPath] > allowlist.Files[s.relPath] {
			out = append(out, s)
		}
	}
	return out
}

func scanForLockPoison(rootDir, srcDir string) (lockPoisonFileFindings, int, error) {
	var all lockPoisonFileFindings
	scanned := 0

	err := filepath.WalkDir(srcDir, func(path string, d os.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if d.IsDir() || !strings.HasSuffix(d.Name(), ".rs") {
			return nil
		}
		relPath, relErr := filepath.Rel(rootDir, path)
		if relErr != nil {
			relPath = path
		}
		relPath = filepath.ToSlash(relPath)

		// Skip dedicated test files, including a themed module under a `tests/`
		// directory. (Reuses isRustTestPath from the test-sleep check, which
		// lives in the same package.)
		if isRustTestPath(relPath, d.Name()) {
			return nil
		}
		scanned++

		findings, scanErr := scanRustFileForLockPoison(path, relPath)
		if scanErr != nil {
			return scanErr
		}
		all.violations = append(all.violations, findings.violations...)
		all.swallows = append(all.swallows, findings.swallows...)
		all.orphans = append(all.orphans, findings.orphans...)
		return nil
	})

	return all, scanned, err
}

// lockPoisonScanState tracks the in-file `#[cfg(test)]` mod skip across lines.
// Unlike error-string-match (which scans in-file test mods to flag
// stringly-typed assertions), test code may freely use bare `.lock().unwrap()`
// and discard a poisoned result: a poisoned lock in a test means the test
// already panicked, so aborting there is harmless. We detect a `#[cfg(test)]`
// attribute, arm on the next `mod ... {`, then skip until brace depth returns to
// the level where the mod opened.
type lockPoisonScanState struct {
	inTestMod      bool
	testModDepth   int
	pendingCfgTest bool
}

// skipInTestMod advances the brace-depth tracking while inside a test module and
// reports whether this line is inside one.
func (s *lockPoisonScanState) skipInTestMod(line string) bool {
	if !s.inTestMod {
		return false
	}
	s.testModDepth += strings.Count(line, "{") - strings.Count(line, "}")
	if s.testModDepth <= 0 {
		s.inTestMod = false
	}
	return true
}

// armTestMod arms on a `#[cfg(test)]` attribute and opens the skip on the
// `mod ... {` that follows (which may be the same line or a later one).
func (s *lockPoisonScanState) armTestMod(line string) bool {
	if strings.Contains(line, "#[cfg(test)]") {
		s.pendingCfgTest = true
	}
	if !s.pendingCfgTest || !strings.Contains(line, "mod ") || !strings.Contains(line, "{") {
		return false
	}
	s.pendingCfgTest = false
	s.testModDepth = strings.Count(line, "{") - strings.Count(line, "}")
	// One-line mod (`mod tests { ... }`) leaves nothing to skip.
	s.inTestMod = s.testModDepth > 0
	return true
}

// scanRustFileForLockPoison scans a single Rust file for both lanes, plus
// orphaned opt-out directives. The whole file is read up front because the
// swallow lane classifies a site by the block that follows it, which routinely
// sits several lines below.
func scanRustFileForLockPoison(path, relPath string) (lockPoisonFileFindings, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return lockPoisonFileFindings{}, err
	}
	lines := strings.Split(string(data), "\n")

	var findings lockPoisonFileFindings
	var state lockPoisonScanState
	tracker := newDirectiveTracker(AllowLockPoisonComment, "//")

	for i, line := range lines {
		lineNum := i + 1
		if state.skipInTestMod(line) {
			continue
		}
		tracker.observe(lineNum, line)

		// Comment-only lines never carry code; they still count for the
		// previous-line opt-out lookup below.
		if strings.HasPrefix(strings.TrimLeft(line, " \t"), "//") {
			continue
		}
		if state.armTestMod(line) {
			continue
		}

		prev := ""
		if i > 0 {
			prev = lines[i-1]
		}
		excused := hasAllowLockPoisonComment(prev) || hasAllowLockPoisonComment(line)

		if lineHasLockPoisonViolation(line) {
			if excused {
				tracker.markUsed(lineNum, line, prev)
				continue
			}
			findings.violations = append(findings.violations, lockPoisonSite{
				relPath: relPath, line: lineNum, text: strings.TrimSpace(line),
			})
			continue
		}

		swallows := classifyLockSwallows(lines, i)
		if len(swallows) == 0 {
			continue
		}
		if excused {
			tracker.markUsed(lineNum, line, prev)
			continue
		}
		for _, s := range swallows {
			s.relPath = relPath
			findings.swallows = append(findings.swallows, s)
		}
	}

	findings.orphans = tracker.orphans(relPath)
	return findings, nil
}

// lineHasLockPoisonViolation reports whether a line acquires a std lock without
// recorded intent: a bare `.unwrap()`, or an `.expect(<msg>)` whose message
// does not name "poison" (case-insensitive).
func lineHasLockPoisonViolation(line string) bool {
	if lockBareUnwrapPattern.MatchString(line) {
		return true
	}
	for _, m := range lockExpectPattern.FindAllStringSubmatch(line, -1) {
		msg := m[2]
		if !strings.Contains(strings.ToLower(msg), "poison") {
			return true
		}
	}
	return false
}

func hasAllowLockPoisonComment(line string) bool {
	return strings.Contains(line, AllowLockPoisonComment)
}

// classifyLockSwallows returns the result-discarding acquisitions on lines[idx],
// one site per acquisition. The `relPath` of each site is left to the caller.
//
// An acquisition whose failure handler records intent is not a site, and neither
// is one whose shape the parser couldn't resolve — an unreadable site is left
// alone rather than guessed at.
func classifyLockSwallows(lines []string, idx int) []lockSwallowSite {
	line := blankLineComment(lines[idx])
	text := strings.TrimSpace(lines[idx])
	var out []lockSwallowSite

	for _, m := range lockAcquirePattern.FindAllStringIndex(line, -1) {
		if shape, discards := classifyLockAcquisition(lines, line, idx, m[0], m[1]); discards {
			out = append(out, lockSwallowSite{line: idx + 1, shape: shape, text: text})
		}
	}
	return out
}

// classifyLockAcquisition decides what one `.lock()` / `.read()` / `.write()`
// does with a failure. What precedes the call says which construct is consuming
// the `Result` (`let Ok(…) = …`, `match …`, or a plain combinator chain), and
// each construct's own handler decides whether the failure is recorded or
// dropped.
func classifyLockAcquisition(lines []string, line string, idx, start, end int) (lockSwallowShape, bool) {
	before := line[:start]
	switch {
	case consumesDirectly(lockLetOkPrefix, before):
		return lockBindingDiscards(lines, idx, end)
	case consumesDirectly(lockMatchPrefix, before):
		return lockMatchDiscards(lines, idx, end)
	default:
		return lockChainDiscards(lines, idx, end)
	}
}

// consumesDirectly reports whether the construct the prefix pattern found is the
// one consuming this acquisition's `Result`. It has to sit at the same bracket
// depth: in `match watched.and_then(|w| w.lock().ok())` the `match` reads what
// the closure returned, so the closure's own chain is what discards the failure.
func consumesDirectly(pattern *regexp.Regexp, before string) bool {
	loc := pattern.FindStringIndex(before)
	if loc == nil {
		return false
	}
	depth := 0
	for _, c := range before[loc[0]:] {
		switch c {
		case '(', '[':
			depth++
		case ')', ']':
			depth--
		}
	}
	return depth == 0
}

// lockBindingDiscards resolves what happens when a `let Ok(…) = <lock>()`
// binding fails. col is the offset just past the acquisition.
func lockBindingDiscards(lines []string, idx, col int) (lockSwallowShape, bool) {
	next, nextIdx, nextCol := nextMeaningful(lines, idx, col)
	if startsWithWord(next, "else") {
		body, _, _, ok := findBraceBlock(lines, nextIdx, nextCol)
		return swallowLetElse, !ok || !handlerRecordsIntent(body)
	}

	// A plain `if let` (or the `&& let` link of a let-chain): the only handler
	// it can have is an `else` after the block it guards.
	_, endIdx, endCol, ok := findBraceBlock(lines, idx, col)
	if !ok {
		return swallowIfLet, true
	}
	after, afterIdx, afterCol := nextMeaningful(lines, endIdx, endCol)
	if !startsWithWord(after, "else") {
		return swallowIfLet, true
	}
	body, _, _, ok := findBraceBlock(lines, afterIdx, afterCol)
	return swallowIfLet, !ok || !handlerRecordsIntent(body)
}

// lockMatchDiscards reads the `match` block the acquisition feeds and judges its
// `Err` arm. An unparsable block or a missing `Err` arm stays quiet.
func lockMatchDiscards(lines []string, idx, col int) (lockSwallowShape, bool) {
	body, _, _, ok := findBraceBlock(lines, idx, col)
	if !ok {
		return swallowMatch, false
	}
	arm, found := lockMatchErrArm(body)
	return swallowMatch, found && !handlerRecordsIntent(arm)
}

// lockChainDiscards judges the combinator chain hanging off the acquisition. The
// chain is read to the end of its statement, so one that wraps across lines
// (`.lock()` … `.map(…)` … `.unwrap_or_default()`) is still seen whole.
func lockChainDiscards(lines []string, idx, col int) (lockSwallowShape, bool) {
	chain := lockChainAfter(lines, idx, col)
	if strings.Contains(chain, "into_inner") {
		return "", false
	}
	if strings.Contains(chain, ".ok()") {
		return swallowOk, true
	}
	if lockOrDefaultPattern.MatchString(chain) {
		return swallowOrDefault, true
	}
	return "", false
}

// lockChainAfter returns the text from an acquisition to the end of its
// statement: the next `;` at bracket depth zero, the close of the expression it
// sits inside, or the lookahead limit, whichever comes first.
func lockChainAfter(lines []string, idx, col int) string {
	var sb strings.Builder
	depth := 0
	for i := idx; i < len(lines) && i < idx+lockChainLookahead; i++ {
		line := blankLineComment(lines[i])
		start := 0
		if i == idx {
			start = col
		}
		for j := start; j < len(line); j++ {
			switch line[j] {
			case '(', '[', '{':
				depth++
			case ')', ']', '}':
				depth--
			case ';':
				if depth == 0 {
					return sb.String()
				}
			}
			if depth < 0 {
				// The enclosing call or block closed: the chain ended with it.
				return sb.String()
			}
			sb.WriteByte(line[j])
		}
		sb.WriteByte('\n')
	}
	return sb.String()
}

// startsWithWord reports whether s opens with the given keyword as a whole word,
// so `elsewhere` isn't read as `else`.
func startsWithWord(s, word string) bool {
	if !strings.HasPrefix(s, word) {
		return false
	}
	if len(s) == len(word) {
		return true
	}
	c := s[len(word)]
	return !(c == '_' || ('a' <= c && c <= 'z') || ('A' <= c && c <= 'Z') || ('0' <= c && c <= '9'))
}

// handlerRecordsIntent reports whether a failure handler (a `match` Err arm, a
// let-else block, or an `if let Ok` else branch) does one of the three things
// the policy sanctions. Anything else silently substitutes a default value.
func handlerRecordsIntent(handler string) bool {
	for _, marker := range lockIntentMarkers {
		if strings.Contains(handler, marker) {
			return true
		}
	}
	return false
}

// findBraceBlock scans forward from lines[idx] at byte offset col for the next
// `{`, and returns the block's body (braces excluded, newlines preserved) plus
// the position just past its matching `}`. ok is false when the file ends first,
// which the callers read as "no handler here".
//
// Comments are blanked, not removed, so a brace or a `panic!` mentioned in prose
// neither unbalances the depth count nor reads as recorded intent, while every
// byte offset still points at the same place in the original line.
func findBraceBlock(lines []string, idx, col int) (body string, endIdx, endCol int, ok bool) {
	depth := 0
	var sb strings.Builder
	for i := idx; i < len(lines); i++ {
		line := blankLineComment(lines[i])
		start := 0
		if i == idx {
			start = col
		}
		for j := start; j < len(line); j++ {
			switch line[j] {
			case '{':
				depth++
				if depth == 1 {
					continue
				}
			case '}':
				depth--
				if depth == 0 {
					return sb.String(), i, j + 1, true
				}
			}
			if depth > 0 {
				sb.WriteByte(line[j])
			}
		}
		if depth > 0 {
			sb.WriteByte('\n')
		}
	}
	return "", 0, 0, false
}

// nextMeaningful returns the text at the next non-blank, non-comment position at
// or after lines[idx][col], with the position it was found at.
func nextMeaningful(lines []string, idx, col int) (string, int, int) {
	for i := idx; i < len(lines); i++ {
		line := blankLineComment(lines[i])
		start := 0
		if i == idx {
			start = col
		}
		if start > len(line) {
			continue
		}
		rest := line[start:]
		trimmed := strings.TrimLeft(rest, " \t")
		if strings.TrimRight(trimmed, " \t") == "" {
			continue
		}
		return trimmed, i, start + len(rest) - len(trimmed)
	}
	return "", idx, col
}

// blankLineComment replaces a line's `//` comment with spaces, leaving every
// other byte where it was so offsets stay comparable across the two scanners.
func blankLineComment(line string) string {
	at := lineCommentStart(line)
	if at < 0 {
		return line
	}
	return line[:at] + strings.Repeat(" ", len(line)-at)
}

// lineCommentStart returns the offset where a line's `//` comment begins, or -1
// when it has none. A `//` inside a string or char literal doesn't count.
func lineCommentStart(line string) int {
	for i := 0; i < len(line); i++ {
		switch {
		case line[i] == '"':
			i = skipLiteral(line, i, '"')
		case line[i] == '\'' && opensCharLiteral(line, i):
			i = skipLiteral(line, i, '\'')
		case line[i] == '/' && i+1 < len(line) && line[i+1] == '/':
			return i
		}
	}
	return -1
}

// skipLiteral returns the index of the literal's closing quote, or the line
// length when it never closes, so the caller's loop resumes past it.
func skipLiteral(line string, start int, quote byte) int {
	for i := start + 1; i < len(line); i++ {
		if line[i] == '\\' {
			i++
			continue
		}
		if line[i] == quote {
			return i
		}
	}
	return len(line)
}

// opensCharLiteral tells a char literal from a lifetime: `'a'` and `'\n'` close
// within two bytes, `&'a str` never does.
func opensCharLiteral(line string, i int) bool {
	return (i+1 < len(line) && line[i+1] == '\\') || (i+2 < len(line) && line[i+2] == '\'')
}

// matchArm is one `<pattern> => <body>` pair of a match expression.
type matchArm struct {
	pattern string
	body    string
}

// lockMatchErrArm returns the body of the arm that handles a failed
// acquisition — the `Err(…)` arm, or the wildcard standing in for it. found is
// false when the arms couldn't be resolved, which keeps a shape the parser
// doesn't understand out of the report.
func lockMatchErrArm(matchBody string) (string, bool) {
	for _, arm := range splitMatchArms(matchBody) {
		pattern := strings.TrimSpace(arm.pattern)
		if strings.HasPrefix(pattern, "Err") || pattern == "_" {
			return arm.body, true
		}
	}
	return "", false
}

// splitMatchArms breaks a match body into its arms, tracking bracket depth so a
// nested match, tuple, or struct literal doesn't end an arm early. An arm ends
// at its top-level `,`, or at the closing brace of a block-bodied arm (which
// carries no comma).
func splitMatchArms(body string) []matchArm {
	s := matchArmSplitter{body: body, arrow: -1}
	for i := 0; i < len(body); i++ {
		s.step(i)
	}
	s.finish()
	return s.arms
}

// matchArmSplitter carries splitMatchArms' cursor: the depth of the brackets
// we're nested in, where the current arm started, where its `=>` sits (-1 before
// we've seen one), and whether its body is a brace block rather than an
// expression.
type matchArmSplitter struct {
	body     string
	arms     []matchArm
	depth    int
	armStart int
	arrow    int
	blockArm bool
}

func (s *matchArmSplitter) step(i int) {
	switch s.body[i] {
	case '(', '[':
		s.depth++
	case ')', ']':
		s.depth--
	case '{':
		s.openBrace(i)
	case '}':
		s.closeBrace(i)
	case '=':
		s.arrowAt(i)
	case ',':
		s.commaAt(i)
	}
}

// openBrace opens a block-bodied arm when the brace is the first thing after the
// arrow; otherwise it's a struct literal or a nested block, and only the depth
// moves.
func (s *matchArmSplitter) openBrace(i int) {
	if s.arrow >= 0 && s.depth == 0 && strings.TrimSpace(s.body[s.arrow+2:i]) == "" {
		s.blockArm = true
	}
	s.depth++
}

func (s *matchArmSplitter) closeBrace(i int) {
	s.depth--
	if s.depth == 0 && s.blockArm {
		s.emit(i+1, i+1)
	}
}

func (s *matchArmSplitter) arrowAt(i int) {
	if s.depth == 0 && s.arrow < 0 && i+1 < len(s.body) && s.body[i+1] == '>' {
		s.arrow = i
	}
}

func (s *matchArmSplitter) commaAt(i int) {
	if s.depth == 0 && s.arrow >= 0 {
		s.emit(i, i+1)
	}
}

// emit closes the current arm at bodyEnd and reopens the cursor at next.
func (s *matchArmSplitter) emit(bodyEnd, next int) {
	s.arms = append(s.arms, matchArm{pattern: s.body[s.armStart:s.arrow], body: s.body[s.arrow+2 : bodyEnd]})
	s.armStart, s.arrow, s.blockArm = next, -1, false
}

// finish closes a trailing arm that ended with neither a comma nor a block.
func (s *matchArmSplitter) finish() {
	if s.arrow >= 0 {
		s.emit(len(s.body), len(s.body))
	}
}
