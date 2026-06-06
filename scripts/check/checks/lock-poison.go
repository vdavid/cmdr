package checks

import (
	"bufio"
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
)

// AllowLockPoisonComment is the magic comment that opts a single line out of the
// lock-poison check. Place it on the line immediately above the flagged line, or
// as a trailing comment on the same line, with a short reason.
//
//	// allowed-lock-poison: nothing panics under this lock, proven by construction
//	let g = state.entries.lock().unwrap();
const AllowLockPoisonComment = "// allowed-lock-poison:"

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

type lockPoisonSite struct {
	relPath string
	line    int
	text    string
}

// RunLockPoison fails the build if any non-test Rust file acquires a std
// `Mutex`/`RwLock` without recording deliberate poison-handling intent. The
// policy is documented in the module doc of
// `apps/desktop/src-tauri/src/ignore_poison.rs` and in `AGENTS.md` § "No bare
// `.lock().unwrap()`".
func RunLockPoison(ctx *CheckContext) (CheckResult, error) {
	rustSrcDir := filepath.Join(ctx.RootDir, "apps", "desktop", "src-tauri", "src")

	violations, orphans, scanned, err := scanForLockPoison(ctx.RootDir, rustSrcDir)
	if err != nil {
		return CheckResult{}, fmt.Errorf("failed to scan Rust files: %w", err)
	}

	var parts []string
	if len(violations) > 0 {
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
		parts = append(parts, fmt.Sprintf(
			"found %d std-lock %s acquired without recorded poison-handling intent "+
				"(use `lock_ignore_poison()` / `read_ignore_poison()` / `write_ignore_poison()` to recover, "+
				"or `.expect(\"<lock> poisoned: <why>\")` to abort deliberately; "+
				"add `%s <reason>` on the line above or as a trailing comment to opt a site out):\n%s",
			len(violations), Pluralize(len(violations), "site", "sites"), AllowLockPoisonComment, strings.TrimRight(sb.String(), "\n"),
		))
	}
	if len(orphans) > 0 {
		parts = append(parts, formatOrphanDirectives(AllowLockPoisonComment, orphans))
	}
	if len(parts) > 0 {
		return CheckResult{}, fmt.Errorf("%s", strings.Join(parts, "\n"))
	}

	return Success(fmt.Sprintf(
		"%d Rust %s scanned, every std-lock acquisition records poison-handling intent",
		scanned, Pluralize(scanned, "file", "files"),
	)), nil
}

func scanForLockPoison(rootDir, srcDir string) ([]lockPoisonSite, []orphanDirective, int, error) {
	var violations []lockPoisonSite
	var orphans []orphanDirective
	scanned := 0

	err := filepath.WalkDir(srcDir, func(path string, d os.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if d.IsDir() || !strings.HasSuffix(d.Name(), ".rs") {
			return nil
		}
		// Skip dedicated test files. (Reuses isRustTestFile from the
		// error-string-match check, which lives in the same package.)
		if isRustTestFile(d.Name()) {
			return nil
		}
		scanned++

		relPath, relErr := filepath.Rel(rootDir, path)
		if relErr != nil {
			relPath = path
		}

		fileViolations, fileOrphans, scanErr := scanRustFileForLockPoison(path, relPath)
		if scanErr != nil {
			return scanErr
		}
		violations = append(violations, fileViolations...)
		orphans = append(orphans, fileOrphans...)
		return nil
	})

	return violations, orphans, scanned, err
}

// lockPoisonScanState tracks the in-file `#[cfg(test)]` mod skip across lines.
// Unlike error-string-match (which scans in-file test mods to flag
// stringly-typed assertions), test code may freely use bare `.lock().unwrap()`:
// a poisoned lock in a test means the test already panicked, so aborting there
// is harmless. We detect a `#[cfg(test)]` attribute, arm on the next `mod ... {`,
// then skip until brace depth returns to the level where the mod opened.
type lockPoisonScanState struct {
	inTestMod      bool
	testModDepth   int
	pendingCfgTest bool
}

// scanRustFileForLockPoison scans a single Rust file for std-lock acquisitions
// that record no poison-handling intent, plus orphaned opt-out directives.
func scanRustFileForLockPoison(path, relPath string) ([]lockPoisonSite, []orphanDirective, error) {
	f, openErr := os.Open(path)
	if openErr != nil {
		return nil, nil, openErr
	}
	defer f.Close()

	scanner := bufio.NewScanner(f)
	scanner.Buffer(make([]byte, 64*1024), 1024*1024)

	var violations []lockPoisonSite
	var state lockPoisonScanState
	tracker := newDirectiveTracker(AllowLockPoisonComment, "//")
	var prev string
	lineNum := 0

	for scanner.Scan() {
		lineNum++
		line := scanner.Text()

		if site := classifyLockPoisonLine(line, prev, relPath, lineNum, &state, tracker); site != nil {
			violations = append(violations, *site)
		}
		prev = line
	}
	return violations, tracker.orphans(relPath), scanner.Err()
}

// classifyLockPoisonLine evaluates one line against the lock-poison policy,
// advancing the test-mod skip state. It returns a non-nil site only when the
// line is a real violation (a std-lock acquisition with no recorded intent and
// no opt-out comment). Directive sites are recorded on the tracker; test-mod
// lines are outside the check's jurisdiction and never tracked.
func classifyLockPoisonLine(line, prev, relPath string, lineNum int, state *lockPoisonScanState, tracker *directiveTracker) *lockPoisonSite {
	if state.inTestMod {
		state.testModDepth += strings.Count(line, "{") - strings.Count(line, "}")
		if state.testModDepth <= 0 {
			state.inTestMod = false
		}
		return nil
	}

	tracker.observe(lineNum, line)

	// Comment-only lines never carry code; the caller still records them for
	// the previous-line opt-out lookup.
	if strings.HasPrefix(strings.TrimLeft(line, " \t"), "//") {
		return nil
	}

	// Arm on a `#[cfg(test)]` attribute; the `mod ... {` that opens the test
	// module may be on this line or a following one.
	if strings.Contains(line, "#[cfg(test)]") {
		state.pendingCfgTest = true
	}
	if state.pendingCfgTest && strings.Contains(line, "mod ") && strings.Contains(line, "{") {
		state.pendingCfgTest = false
		state.testModDepth = strings.Count(line, "{") - strings.Count(line, "}")
		// One-line mod (`mod tests { ... }`) leaves nothing to skip.
		state.inTestMod = state.testModDepth > 0
		return nil
	}

	if !lineHasLockPoisonViolation(line) {
		return nil
	}

	// Opt-out: `// allowed-lock-poison: <reason>` on the previous line OR as a
	// trailing comment on the same line.
	if hasAllowLockPoisonComment(prev) || hasAllowLockPoisonComment(line) {
		tracker.markUsed(lineNum, line, prev)
		return nil
	}

	return &lockPoisonSite{
		relPath: relPath,
		line:    lineNum,
		text:    strings.TrimSpace(line),
	}
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
