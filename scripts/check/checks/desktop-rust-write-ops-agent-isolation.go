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

// writeOpsSubtree is the engine this check fences. Relative to a member's `src/`.
const writeOpsSubtree = "file_system/write_operations"

// agentPathPattern matches a Rust path naming the `agent` module: `crate::agent::`,
// `super::agent::`, a bare `agent::`, or a `use` of any of them. The `\b` keeps
// identifiers that merely END in "agent" (a `user_agent::` helper) out of it.
var agentPathPattern = regexp.MustCompile(`\bagent::`)

type agentReachSite struct {
	relPath string
	line    int
	text    string
}

// RunWriteOpsAgentIsolation fails the build if the write engine names the `agent`
// module.
//
// The engine executes what a person approved, and it must not know or care who
// proposed it: an approved operation is an ordinary operation, and the moment the
// engine can see the agent's world it grows a second execution path that drifts
// from the real one. The seam is the injected `OperationEventSink` — a caller that
// wants per-source outcomes recorded wraps the sink it passes in, and the engine
// reports through it either way.
//
// This replaces a written rule. It was a `❌` line in
// `file_system/write_operations/DETAILS.md`, which cost tokens in every session
// that loaded the doc and stopped nobody; a scanner holds the same boundary and
// costs nothing to read.
//
// Jurisdiction is the whole engine subtree INCLUDING its tests: a test that
// reaches for `agent::` is a test proving the engine knows about the agent, which
// is the thing being forbidden. Comments are exempt — the docs discuss the agent
// constantly, and naming the module you must not call is how you explain it.
func RunWriteOpsAgentIsolation(ctx *CheckContext) (CheckResult, error) {
	roots, err := ScannerRoots(ctx.RootDir, "desktop-rust-write-ops-agent-isolation")
	if err != nil {
		return CheckResult{}, err
	}

	var violations []agentReachSite
	scanned := 0
	for _, root := range roots {
		engineRoot := filepath.Join(root, writeOpsSubtree)
		if _, statErr := os.Stat(engineRoot); statErr != nil {
			continue
		}
		rootViolations, rootScanned, scanErr := scanForAgentReach(ctx.RootDir, engineRoot)
		if scanErr != nil {
			return CheckResult{}, fmt.Errorf("failed to scan Rust files: %w", scanErr)
		}
		violations = append(violations, rootViolations...)
		scanned += rootScanned
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
			"the write engine names the `agent` module in %d %s. An approved operation is an ordinary operation, "+
				"so the engine must not know who proposed it; reporting flows out through the injected "+
				"`OperationEventSink`, which a caller wraps when it wants per-source outcomes recorded:\n%s",
			len(violations), Pluralize(len(violations), "place", "places"),
			strings.TrimRight(sb.String(), "\n"),
		)
	}

	return Success(fmt.Sprintf(
		"%d Rust %s scanned, the write engine knows nothing about the agent",
		scanned, Pluralize(scanned, "file", "files"),
	)), nil
}

// scanForAgentReach walks the engine subtree and returns every line naming the
// `agent` module outside a comment, plus the count of files scanned.
func scanForAgentReach(rootDir, engineDir string) ([]agentReachSite, int, error) {
	var violations []agentReachSite
	scanned := 0

	err := filepath.WalkDir(engineDir, func(path string, d os.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if d.IsDir() || !strings.HasSuffix(d.Name(), ".rs") {
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
		// Allow long lines (default is 64 KB; some generated/test files exceed it).
		scanner.Buffer(make([]byte, 64*1024), 1024*1024)
		lineNum := 0
		for scanner.Scan() {
			lineNum++
			line := scanner.Text()
			if !agentPathPattern.MatchString(line) {
				continue
			}
			// Prose may name the module freely; only code may not reach it.
			trimmed := strings.TrimLeft(line, " \t")
			if strings.HasPrefix(trimmed, "//") {
				continue
			}
			violations = append(violations, agentReachSite{
				relPath: relPath,
				line:    lineNum,
				text:    strings.TrimSpace(line),
			})
		}
		return scanner.Err()
	})
	if err != nil {
		return nil, 0, err
	}

	return violations, scanned, nil
}
