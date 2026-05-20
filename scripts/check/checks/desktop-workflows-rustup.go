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

// RunWorkflowsRustup ensures `rust-toolchain.toml` stays the single source of
// truth for which rustup targets / components the project needs. Workflows that
// also `rustup target add X` (or `rustup component add X`) duplicate that
// source — and the duplicate gets out of sync silently.
//
// The bug this guards against (May 2026, v0.20.0 release): the release workflow
// ran `rustup target add x86_64-apple-darwin` from the repo root, but
// `rust-toolchain.toml` was at `apps/desktop/`. So `rustup target add` touched
// the runner's default toolchain instead of the pinned channel
// `pnpm tauri build` used inside `apps/desktop/`. Result: every non-aarch64
// release build failed at "Target x86_64-apple-darwin is not installed".
//
// The structural fix is to move `rust-toolchain.toml` to the workspace root
// (done in commit `41e999ab`) AND declare `targets = [...]` there. This check
// adds the regression guard: any new `rustup target add` / `rustup component
// add` in a workflow re-introduces the divergence risk, so the check fails.
//
// `rustup install`, `rustup update`, `rustup toolchain install`, `rustup show`
// stay allowed — they don't add anything orthogonal to the toolchain file.
//
// Opt-out: append `# allowed-rustup-add: <reason>` to the line. Empty reasons
// are rejected.
func RunWorkflowsRustup(ctx *CheckContext) (CheckResult, error) {
	wfDir := filepath.Join(ctx.RootDir, ".github", "workflows")
	entries, err := os.ReadDir(wfDir)
	if err != nil {
		if os.IsNotExist(err) {
			return Skipped("no .github/workflows/"), nil
		}
		return CheckResult{}, fmt.Errorf("failed to read workflows dir: %w", err)
	}

	var files []string
	for _, e := range entries {
		if e.IsDir() {
			continue
		}
		name := e.Name()
		if strings.HasSuffix(name, ".yml") || strings.HasSuffix(name, ".yaml") {
			files = append(files, filepath.Join(wfDir, name))
		}
	}
	sort.Strings(files)

	var violations []string
	scanned := 0
	for _, f := range files {
		v, err := scanWorkflowForRustup(f, ctx.RootDir)
		if err != nil {
			return CheckResult{}, err
		}
		violations = append(violations, v...)
		scanned++
	}

	if len(violations) > 0 {
		return CheckResult{}, fmt.Errorf(
			"`rustup target add` / `rustup component add` found in workflows; declare in rust-toolchain.toml instead\n%s",
			indentOutput(strings.Join(violations, "\n")))
	}

	result := Success(fmt.Sprintf("%d %s, no `rustup target/component add` lines",
		scanned, Pluralize(scanned, "workflow", "workflows")))
	result.Total = scanned
	return result, nil
}

// Matches `rustup target add` or `rustup component add` anywhere on a line,
// catching both `run: rustup target add X` and shell continuations / `&&`
// chains. Whitespace between tokens is forgiving. The `#` lookahead is not
// strict — we strip trailing comments before the match in the scanner.
var rustupAddRE = regexp.MustCompile(`\brustup\s+(target|component)\s+add\b`)

// Opt-out marker. Must include a non-empty reason after the colon.
var rustupAddAllowedRE = regexp.MustCompile(`#\s*allowed-rustup-add:\s*(\S.*)`)

func scanWorkflowForRustup(path, repoRoot string) ([]string, error) {
	rel, _ := filepath.Rel(repoRoot, path)
	f, err := os.Open(path)
	if err != nil {
		return nil, fmt.Errorf("open %s: %w", rel, err)
	}
	defer f.Close()

	var violations []string
	scanner := bufio.NewScanner(f)
	// Workflow yaml lines can be long (compose-style chained runs); raise the
	// buffer above bufio's 64 KB default to cover the worst case.
	scanner.Buffer(make([]byte, 0, 64*1024), 1024*1024)
	lineNo := 0
	for scanner.Scan() {
		lineNo++
		line := scanner.Text()
		// Skip whole-line YAML comments. A line whose first non-whitespace
		// character is `#` is a comment and can mention `rustup target add` in
		// prose without being a command (for example, "the previous rustup
		// target add step was removed; see rust-toolchain.toml").
		trimmed := strings.TrimLeft(line, " \t")
		if strings.HasPrefix(trimmed, "#") {
			continue
		}
		if !rustupAddRE.MatchString(line) {
			continue
		}
		// Opt-out: line ends with `# allowed-rustup-add: <reason>`.
		if m := rustupAddAllowedRE.FindStringSubmatch(line); m != nil && strings.TrimSpace(m[1]) != "" {
			continue
		}
		violations = append(violations, fmt.Sprintf(
			"%s:%d: %s\n    declare the target/component in rust-toolchain.toml instead, OR add `# allowed-rustup-add: <reason>`",
			rel, lineNo, strings.TrimSpace(line)))
	}
	if err := scanner.Err(); err != nil {
		return nil, fmt.Errorf("scan %s: %w", rel, err)
	}
	return violations, nil
}
