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

	violations, scanned, err := scanForLockPoison(ctx.RootDir, rustSrcDir)
	if err != nil {
		return CheckResult{}, fmt.Errorf("failed to scan Rust files: %w", err)
	}

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
		return CheckResult{}, fmt.Errorf(
			"found %d std-lock %s acquired without recorded poison-handling intent "+
				"(use `lock_ignore_poison()` / `read_ignore_poison()` / `write_ignore_poison()` to recover, "+
				"or `.expect(\"<lock> poisoned: <why>\")` to abort deliberately; "+
				"add `%s <reason>` on the line above or as a trailing comment to opt a site out):\n%s",
			len(violations), Pluralize(len(violations), "site", "sites"), AllowLockPoisonComment, sb.String(),
		)
	}

	return Success(fmt.Sprintf(
		"%d Rust %s scanned, every std-lock acquisition records poison-handling intent",
		scanned, Pluralize(scanned, "file", "files"),
	)), nil
}

func scanForLockPoison(rootDir, srcDir string) ([]lockPoisonSite, int, error) {
	var violations []lockPoisonSite
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

		f, openErr := os.Open(path)
		if openErr != nil {
			return openErr
		}
		defer f.Close()

		scanner := bufio.NewScanner(f)
		scanner.Buffer(make([]byte, 64*1024), 1024*1024)
		var prev string
		lineNum := 0

		// Track entry into an in-file `#[cfg(test)]` mod and skip its body.
		// Unlike error-string-match (which scans in-file test mods to flag
		// stringly-typed assertions), test code may freely use bare
		// `.lock().unwrap()`: a poisoned lock in a test means the test already
		// panicked, so aborting there is harmless. We detect a `#[cfg(test)]`
		// attribute, arm on the next `mod ... {`, then skip until brace depth
		// returns to the level where the mod opened.
		inTestMod := false
		testModDepth := 0
		pendingCfgTest := false

		for scanner.Scan() {
			lineNum++
			line := scanner.Text()

			if inTestMod {
				testModDepth += strings.Count(line, "{") - strings.Count(line, "}")
				if testModDepth <= 0 {
					inTestMod = false
				}
				prev = line
				continue
			}

			trimmed := strings.TrimLeft(line, " \t")

			// Comment-only lines never carry code; remember them for the
			// previous-line opt-out lookup and move on.
			if strings.HasPrefix(trimmed, "//") {
				prev = line
				continue
			}

			// Arm on a `#[cfg(test)]` attribute; the `mod ... {` that opens the
			// test module may be on this line or a following one.
			if strings.Contains(line, "#[cfg(test)]") {
				pendingCfgTest = true
			}
			if pendingCfgTest && strings.Contains(line, "mod ") && strings.Contains(line, "{") {
				inTestMod = true
				pendingCfgTest = false
				testModDepth = strings.Count(line, "{") - strings.Count(line, "}")
				if testModDepth <= 0 {
					// One-line mod (`mod tests { ... }`): nothing left to skip.
					inTestMod = false
				}
				prev = line
				continue
			}

			if !lineHasLockPoisonViolation(line) {
				prev = line
				continue
			}

			// Opt-out: `// allowed-lock-poison: <reason>` on the previous line
			// OR as a trailing comment on the same line.
			if hasAllowLockPoisonComment(prev) || hasAllowLockPoisonComment(line) {
				prev = line
				continue
			}

			violations = append(violations, lockPoisonSite{
				relPath: relPath,
				line:    lineNum,
				text:    strings.TrimSpace(line),
			})
			prev = line
		}
		return scanner.Err()
	})

	return violations, scanned, err
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
