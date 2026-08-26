package checks

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"runtime"
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
//
// A reason may OPEN with a platform scope (`macos-only`, `linux-only`,
// `windows-only`), and that scope is what makes the opt-out safe: the filter is
// excused everywhere except the platform it names, so the lane where the test
// actually compiles still fails if a rename rots the filter. Without the scope
// an opt-out would silence the check on every platform, which is the whole
// failure this check exists to catch. A scoped opt-out is never an orphan: it
// works for the OTHER platform's lane.
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
	for _, match := range nextestTestAtomPattern.FindAllStringSubmatch(filterDeclarationsIn(config), -1) {
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

// nextestFilterDeclarationStart matches the opening of a `filter = '...'` line,
// which is the only place an atom is DECLARED.
var nextestFilterDeclarationStart = regexp.MustCompile(`^\s*filter\s*=`)

// filterDeclarationsIn returns only the config's filter declarations, so the
// atom scan can't read one out of a comment.
//
// This file's own prose explains the substring trap by writing `test(x)`, and
// comments quote filters while discussing them. Scanning those invented atoms
// that answer to nobody — harmless only because a one-letter atom matches
// something by luck, and a phantom `test(y)` in a future comment would fail the
// check with nothing to fix. A declaration's value can span lines, so it runs
// until its quote closes.
func filterDeclarationsIn(config string) string {
	var kept []string
	inDeclaration := false
	for _, line := range strings.Split(config, "\n") {
		if !inDeclaration {
			if strings.HasPrefix(strings.TrimLeft(line, " \t"), "#") {
				continue
			}
			if !nextestFilterDeclarationStart.MatchString(line) {
				continue
			}
			// TOML's multi-line literal opens with three quotes and runs to the
			// next three; the single-quoted shape opens and closes on its own line.
			value := line[strings.Index(line, "=")+1:]
			if strings.Contains(value, "'''") {
				inDeclaration = strings.Count(value, "'''") == 1
			} else {
				inDeclaration = strings.Count(value, "'") < 2
			}
			kept = append(kept, line)
			continue
		}
		kept = append(kept, line)
		if strings.Contains(line, "'''") {
			inDeclaration = false
		}
	}
	return strings.Join(kept, "\n")
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

	// This check LISTS tests rather than running them, but listing is still
	// `cargo nextest`, and CI reaches this step before any lane that runs tests.
	// Without this it passes only while the Rust cache carries the binary.
	if err := EnsureCargoNextest(); err != nil {
		return CheckResult{}, err
	}

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

	unexcused, orphanDirectives := excuseNextestFindings(config, names, runtime.GOOS)

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
	if orphans := orphanDirectives; len(orphans) > 0 {
		return CheckResult{}, fmt.Errorf("%s", formatOrphanDirectives(AllowUnmatchedNextestFilterComment, orphans))
	}

	atoms := len(nextestTestAtomPattern.FindAllString(config, -1))
	return Success(fmt.Sprintf("%d nextest %s, all selecting a live test over %s tests",
		atoms, Pluralize(atoms, "filter", "filters"), formatThousands(len(names)))), nil
}

// nextestFilterPlatformScopes maps the scope token an opt-out reason may open
// with to the `runtime.GOOS` it names. A scoped opt-out excuses the filter
// everywhere EXCEPT that platform, because that platform is the one where the
// test compiles, the override does its work, and a rename can rot the filter.
var nextestFilterPlatformScopes = map[string]string{
	"macos-only":   "darwin",
	"linux-only":   "linux",
	"windows-only": "windows",
}

// nextestFilterScopeOf returns the `runtime.GOOS` an opt-out reason scopes
// itself to, or "" when it is unconditional. The token opens the reason, so
// `macos-only, hdiutil has no Linux counterpart` scopes and a reason merely
// mentioning macOS in prose does not.
func nextestFilterScopeOf(reason string) string {
	for token, goos := range nextestFilterPlatformScopes {
		if strings.HasPrefix(reason, token) {
			return goos
		}
	}
	return ""
}

// excuseNextestFindings decides, for the platform named by goos, which stale
// filters are real rot and which opt-out comments are orphans.
//
// Split out of [RunNextestFilterCoverage] and pure so the OTHER platform's
// behaviour is testable from this one: the Linux lane is where a macOS-only
// filter looks deleted, and that lane can't be reached from a macOS test run.
func excuseNextestFindings(config string, names []string, goos string) ([]nextestFilterFinding, []orphanDirective) {
	tracker := newDirectiveTracker(AllowUnmatchedNextestFilterComment, "#")
	for i, line := range strings.Split(config, "\n") {
		tracker.observe(i+1, line)
		// A scoped opt-out exists for the OTHER platform's lane, so it is never an
		// orphan on this one: on the platform it names the filter matches (no
		// finding to excuse), and everywhere else it does the excusing. Reporting
		// it unused on either side would mean deleting the reason on one OS and
		// re-adding it on the other, which is how an opt-out ends up with no
		// reason at all. The cost is that a scoped opt-out outlives its filter;
		// the atom itself still has to name a real test on its own platform.
		if marker := strings.Index(line, AllowUnmatchedNextestFilterComment); marker >= 0 {
			reason := strings.TrimSpace(line[marker+len(AllowUnmatchedNextestFilterComment):])
			if nextestFilterScopeOf(reason) != "" {
				tracker.markLineUsed(i + 1)
			}
		}
	}

	var unexcused []nextestFilterFinding
	for _, f := range scanNextestFilters(config, names) {
		at, reason, found := nextestFilterOptOutFor(config, f.atom)
		// On the platform a scope names, the test is supposed to be here, so the
		// opt-out does not apply and a filter selecting nothing is genuine rot.
		if found && nextestFilterScopeOf(reason) != goos {
			tracker.markLineUsed(at)
			continue
		}
		unexcused = append(unexcused, f)
	}
	return unexcused, tracker.orphans(".config/nextest.toml")
}

// nextestFilterOptOutFor finds the reasoned opt-out for an atom, looked for on
// the ten comment lines above the `filter =` line that carries it. Returns the
// opt-out's 1-based line and its reason text.
func nextestFilterOptOutFor(config, atom string) (int, string, bool) {
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
			if reason := strings.TrimSpace(lines[j][marker+len(AllowUnmatchedNextestFilterComment):]); reason != "" {
				return j + 1, reason, true
			}
		}
	}
	return 0, "", false
}
