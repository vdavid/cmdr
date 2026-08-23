package checks

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
)

// A per-test override in `.config/nextest.toml` that no longer selects anything,
// made impossible to ship quietly.
//
// Every override there buys a test something it needs: a bigger cap over an
// in-test deadline it would otherwise die under, or a `test-group` that stops
// several live FSEvents watches from starving each other. `test(x)` is a
// SUBSTRING match on the test's full path, so a filter that spells a module path
// keeps working only as long as that path does. Move the tests to a sibling
// `*_test.rs`, split a test module into a directory, and the filter matches
// NOTHING — nextest says nothing about it, the tests fall back to the global 8 s
// cap and full parallelism, and the only symptom is a flake weeks later that
// reads like host contention.
//
// That happened three times over five weeks: `downloads::watcher::tests::` (five
// self-healing FSEvents tests, `real-notify` + 20 s), the cold-drive
// branch-watch test (`real-notify` + 40 s over its own 30 s wait), and the
// disk-image fixtures (`disk-image` serialization + 30 s over hdiutil).
//
// The config's own comments already warn about this ("a stale prefix here
// matches NOTHING and silently drops the pair back to the 8 s global cap"). A
// warning is what was there before, and this is what replaces it.

// AllowUnmatchedNextestFilterComment marks a filter that deliberately selects
// nothing here: a test only one platform compiles, named so the override is
// ready wherever it does. It carries a reason after the colon, or it isn't an
// opt-out. An orphaned one fails, like every other opt-out in this package.
const AllowUnmatchedNextestFilterComment = "# allowed-unmatched-nextest-filter:"

// nextestTestAtomPattern pulls the argument out of every `test(...)` atom in a
// filterset. `package(...)`, `binary(...)`, and the rest are left alone: nextest
// fails to PARSE an unknown package, so those rot loudly on their own.
var nextestTestAtomPattern = regexp.MustCompile(`\btest\(([^)]*)\)`)

// nextestFilterFinding is one filter atom that selects no test.
type nextestFilterFinding struct {
	// atom is the text inside `test(...)`, which is the string to edit.
	atom string
	// movedTo is the full path of a test whose leaf segment matches, so the
	// finding says where the atom's target went. Empty when nothing by that name
	// exists at all (a deleted test, or one this platform doesn't compile).
	movedTo string
}

// scanNextestFilters reports every `test(...)` atom in `config` that no name in
// `testNames` contains, in the order the config declares them, each atom once.
//
// The leaf lookup is what makes a finding actionable. An atom's last `::`
// segment is the function name (or, for a module prefix, the module), and it
// almost always still exists — the whole failure mode is that the path AROUND it
// changed. Naming where it went turns "this matches nothing" into the edit.
func scanNextestFilters(config string, testNames []string) []nextestFilterFinding {
	var findings []nextestFilterFinding
	seen := map[string]bool{}
	for _, match := range nextestTestAtomPattern.FindAllStringSubmatch(config, -1) {
		atom := strings.TrimSpace(match[1])
		if atom == "" || seen[atom] {
			continue
		}
		seen[atom] = true
		if anyContains(testNames, atom) {
			continue
		}
		findings = append(findings, nextestFilterFinding{atom: atom, movedTo: leafHome(atom, testNames)})
	}
	return findings
}

// anyContains reports whether any name carries `needle` as a substring, which is
// exactly what `test(needle)` asks nextest.
func anyContains(names []string, needle string) bool {
	for _, name := range names {
		if strings.Contains(name, needle) {
			return true
		}
	}
	return false
}

// leafHome returns the first test whose path carries the atom's last `::`
// segment, or "" when nothing does.
func leafHome(atom string, testNames []string) string {
	leaf := atom
	if trimmed := strings.TrimSuffix(atom, "::"); trimmed != atom {
		leaf = trimmed
	}
	if at := strings.LastIndex(leaf, "::"); at >= 0 {
		leaf = leaf[at+len("::"):]
	}
	if leaf == "" {
		return ""
	}
	for _, name := range testNames {
		if strings.Contains(name, leaf) {
			return name
		}
	}
	return ""
}

// ParseNextestListNames pulls the test paths out of `cargo nextest list -T
// human`, which indents each name under its binary-id header.
func ParseNextestListNames(output string) []string {
	var names []string
	for _, line := range strings.Split(output, "\n") {
		if !strings.HasPrefix(line, " ") {
			continue
		}
		if name := strings.TrimSpace(line); name != "" {
			names = append(names, name)
		}
	}
	return names
}

// RunNextestFilterCoverage fails when a per-test override in
// `.config/nextest.toml` selects no test.
func RunNextestFilterCoverage(ctx *CheckContext) (CheckResult, error) {
	configPath := filepath.Join(ctx.RootDir, ".config", "nextest.toml")
	raw, err := os.ReadFile(configPath)
	if err != nil {
		return CheckResult{}, fmt.Errorf("couldn't read .config/nextest.toml: %w", err)
	}
	config := string(raw)

	laneArgs, err := HostCargoLaneArgs(ctx.RootDir)
	if err != nil {
		return CheckResult{}, err
	}
	// `--run-ignored all`, or every `#[ignore]`-gated cell looks deleted: the SMB
	// and SFTP fixtures and the disk-image ones are exactly where the caps live.
	// Same lane args as every other cargo lane, so this shares their `target/`
	// rather than taking turns invalidating it.
	args := append([]string{"nextest", "list", "--locked"}, laneArgs...)
	args = append(args, "--run-ignored", "all", "-T", "human")
	cmd := exec.Command("cargo", args...)
	cmd.Dir = ctx.RootDir
	output, runErr := RunCommand(cmd, true)
	if runErr != nil {
		return CheckResult{}, fmt.Errorf("couldn't list the workspace's tests:\n%s", indentOutput(StripANSI(output)))
	}
	names := ParseNextestListNames(StripANSI(output))
	if len(names) == 0 {
		return CheckResult{}, fmt.Errorf("`cargo nextest list` named no tests, so nothing here can be judged")
	}

	tracker := newDirectiveTracker(AllowUnmatchedNextestFilterComment, "#")
	for i, line := range strings.Split(config, "\n") {
		tracker.observe(i+1, line)
	}

	findings := scanNextestFilters(config, names)
	var unexcused []nextestFilterFinding
	for _, f := range findings {
		if at, found := nextestFilterOptOutFor(config, f.atom); found {
			tracker.markLineUsed(at)
			continue
		}
		unexcused = append(unexcused, f)
	}

	if len(unexcused) > 0 {
		lines := make([]string, len(unexcused))
		for i, f := range unexcused {
			if f.movedTo == "" {
				lines[i] = fmt.Sprintf("test(%s) — no test carries that name any more", f.atom)
				continue
			}
			lines[i] = fmt.Sprintf("test(%s) — the name lives at `%s` now", f.atom, f.movedTo)
		}
		sort.Strings(lines)
		return CheckResult{}, fmt.Errorf(
			"%d nextest %s in `.config/nextest.toml` select no test, so the cap and test-group they grant apply to nothing:\n  %s\n\n"+
				"`test(x)` is a substring match on a test's FULL path, so a filter spelling a module path dies when the module moves. "+
				"Repoint each one at where the test lives. A filter that deliberately selects nothing on this platform says so with an `%s <why>` comment above it",
			len(unexcused), Pluralize(len(unexcused), "filter", "filters"), strings.Join(lines, "\n  "),
			AllowUnmatchedNextestFilterComment)
	}
	if orphans := tracker.orphans(".config/nextest.toml"); len(orphans) > 0 {
		return CheckResult{}, fmt.Errorf("%s", formatOrphanDirectives(AllowUnmatchedNextestFilterComment, orphans))
	}

	atoms := len(nextestTestAtomPattern.FindAllString(config, -1))
	return Success(fmt.Sprintf("%d nextest %s, all selecting a live test over %s tests",
		atoms, Pluralize(atoms, "filter", "filters"), formatThousands(len(names)))), nil
}

// nextestFilterOptOutFor finds the reasoned opt-out for an atom, looked for on
// the ten comment lines above the `filter =` line that carries it. Returns the
// opt-out's 1-based line.
func nextestFilterOptOutFor(config, atom string) (int, bool) {
	lines := strings.Split(config, "\n")
	for i, line := range lines {
		if !strings.Contains(line, "test("+atom+")") {
			continue
		}
		for j := max(0, i-10); j < i; j++ {
			marker := strings.Index(lines[j], AllowUnmatchedNextestFilterComment)
			if marker < 0 {
				continue
			}
			if strings.TrimSpace(lines[j][marker+len(AllowUnmatchedNextestFilterComment):]) != "" {
				return j + 1, true
			}
		}
	}
	return 0, false
}
