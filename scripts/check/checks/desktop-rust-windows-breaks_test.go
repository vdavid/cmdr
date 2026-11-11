package checks

import (
	"strings"
	"testing"
)

func TestWindowsBreaksRunsNativelyOnWindowsAndCrossChecksOnlyWhenAsked(t *testing.T) {
	cases := []struct {
		goos, env, wantTarget string
		wantOK                bool
	}{
		{"windows", "", "", true},
		{"windows", "x86_64-pc-windows-gnu", "", true},
		{"linux", "x86_64-pc-windows-gnu", "x86_64-pc-windows-gnu", true},
		{"darwin", "", "", false},
	}
	for _, c := range cases {
		target, ok := windowsBreaksTarget(c.goos, c.env)
		if target != c.wantTarget || ok != c.wantOK {
			t.Errorf("windowsBreaksTarget(%q, %q) = (%q, %v), want (%q, %v)",
				c.goos, c.env, target, ok, c.wantTarget, c.wantOK)
		}
	}
}

func TestWindowsPendingMembersAreLinuxMembersNotDeclaredForWindows(t *testing.T) {
	members := []WorkspaceMember{
		{Name: "portable"},
		{Name: "pending", Platforms: []string{"linux", "macos"}},
		{Name: "mac-only", Platforms: []string{"macos"}},
		{Name: "declared", Platforms: []string{"linux", "macos", "windows"}},
	}
	got := windowsPendingMembers(members)
	if len(got) != 1 || got[0].Name != "pending" {
		t.Fatalf("pending members = %v, want only `pending`", got)
	}
}

// A workspace of six members and one third-party crate, shaped so each status
// and each traversal rule shows up once:
//
//	broken: fails in its own code
//	above-broken: depends on broken            -> blocked
//	clean: depends on nothing                   -> compiles
//	needs-sys: depends on sys-crate 1.0.0, whose build script fails -> blocked
//	tests-broken: dev-depends on broken         -> blocked (its tests are checked)
//	above-tests-broken: depends on tests-broken, whose dev edge isn't followed -> compiles
const windowsFixtureMetadata = `{
  "packages": [
    {"id": "path+file:///w/broken#0.0.0", "name": "broken", "version": "0.0.0"},
    {"id": "path+file:///w/above-broken#0.0.0", "name": "above-broken", "version": "0.0.0"},
    {"id": "path+file:///w/clean#0.0.0", "name": "clean", "version": "0.0.0"},
    {"id": "path+file:///w/needs-sys#0.0.0", "name": "needs-sys", "version": "0.0.0"},
    {"id": "path+file:///w/tests-broken#0.0.0", "name": "tests-broken", "version": "0.0.0"},
    {"id": "path+file:///w/above-tests-broken#0.0.0", "name": "above-tests-broken", "version": "0.0.0"},
    {"id": "registry+https://github.com/rust-lang/crates.io-index#sys-crate@1.0.0", "name": "sys-crate", "version": "1.0.0"}
  ],
  "workspace_members": [
    "path+file:///w/broken#0.0.0", "path+file:///w/above-broken#0.0.0", "path+file:///w/clean#0.0.0",
    "path+file:///w/needs-sys#0.0.0", "path+file:///w/tests-broken#0.0.0", "path+file:///w/above-tests-broken#0.0.0"
  ],
  "resolve": {"nodes": [
    {"id": "path+file:///w/broken#0.0.0", "deps": []},
    {"id": "path+file:///w/above-broken#0.0.0", "deps": [
      {"pkg": "path+file:///w/broken#0.0.0", "dep_kinds": [{"kind": null}]}]},
    {"id": "path+file:///w/clean#0.0.0", "deps": []},
    {"id": "path+file:///w/needs-sys#0.0.0", "deps": [
      {"pkg": "registry+https://github.com/rust-lang/crates.io-index#sys-crate@1.0.0", "dep_kinds": [{"kind": null}]}]},
    {"id": "path+file:///w/tests-broken#0.0.0", "deps": [
      {"pkg": "path+file:///w/broken#0.0.0", "dep_kinds": [{"kind": "dev"}]}]},
    {"id": "path+file:///w/above-tests-broken#0.0.0", "deps": [
      {"pkg": "path+file:///w/tests-broken#0.0.0", "dep_kinds": [{"kind": null}]}]},
    {"id": "registry+https://github.com/rust-lang/crates.io-index#sys-crate@1.0.0", "deps": []}
  ]}
}`

// The lib's error arrives twice (lib, then its test build), plus rustc's own
// "aborting" summary, which isn't an error of the code.
const windowsFixtureCheckStdout = `{"reason":"compiler-message","package_id":"path+file:///w/broken#0.0.0","message":{"level":"error","message":"cannot find ` + "`unix`" + ` in ` + "`os`" + `","rendered":"error[E0433]: cannot find unix\n","spans":[{"file_name":"crates/broken/src/lib.rs","line_start":3,"is_primary":true}]}}
{"reason":"compiler-message","package_id":"path+file:///w/broken#0.0.0","message":{"level":"error","message":"cannot find ` + "`unix`" + ` in ` + "`os`" + `","rendered":"error[E0433]: cannot find unix\n","spans":[{"file_name":"crates/broken/src/lib.rs","line_start":3,"is_primary":true}]}}
{"reason":"compiler-message","package_id":"path+file:///w/broken#0.0.0","message":{"level":"error","message":"mismatched types","rendered":"error[E0308]: mismatched types\n","spans":[]}}
{"reason":"compiler-message","package_id":"path+file:///w/broken#0.0.0","message":{"level":"error","message":"aborting due to 2 previous errors","rendered":"error: aborting\n","spans":[]}}
{"reason":"compiler-message","package_id":"path+file:///w/clean#0.0.0","message":{"level":"warning","message":"unused variable","rendered":"warning\n","spans":[]}}
{"reason":"compiler-artifact","package_id":"path+file:///w/clean#0.0.0"}
{"reason":"compiler-artifact","package_id":"path+file:///w/above-tests-broken#0.0.0"}
not json, which cargo never prints on stdout but the parser shrugs off
{"reason":"build-finished","success":false}
`

const windowsFixtureCheckStderr = "   Compiling sys-crate v1.0.0\n" +
	"error: failed to run custom build command for `sys-crate v1.0.0`\n" +
	"error: could not compile `broken` (lib) due to 2 previous errors\n"

func windowsFixtureReport(t *testing.T) windowsBreakReport {
	t.Helper()
	graph, err := parseWindowsDepGraph([]byte(windowsFixtureMetadata))
	if err != nil {
		t.Fatalf("parseWindowsDepGraph: %v", err)
	}
	outcome := parseCargoCheckOutput([]byte(windowsFixtureCheckStdout), windowsFixtureCheckStderr, graph)
	report := windowsBreakReport{Target: "x86_64-pc-windows-gnu", FailedDeps: map[string]string{}}
	names := []string{"above-broken", "above-tests-broken", "broken", "clean", "needs-sys", "tests-broken"}
	report.assemble(names, graph, outcome, windowsFixtureCheckStderr)
	return report
}

func TestWindowsBreaksClassifiesEachMember(t *testing.T) {
	report := windowsFixtureReport(t)
	want := map[string]struct {
		status   windowsMemberStatus
		errors   int
		blockers string
	}{
		"broken":             {windowsFails, 2, ""},
		"above-broken":       {windowsBlocked, 0, "broken"},
		"clean":              {windowsCompiles, 0, ""},
		"needs-sys":          {windowsBlocked, 0, "sys-crate 1.0.0"},
		"tests-broken":       {windowsBlocked, 0, "broken"},
		"above-tests-broken": {windowsCompiles, 0, ""},
	}
	for _, m := range report.Members {
		w := want[m.Name]
		if m.Status != w.status || len(m.Errors) != w.errors || strings.Join(m.Blockers, ", ") != w.blockers {
			t.Errorf("%s = (%s, %d errors, blocked by %q), want (%s, %d errors, blocked by %q)",
				m.Name, m.Status, len(m.Errors), strings.Join(m.Blockers, ", "), w.status, w.errors, w.blockers)
		}
	}
	if got := report.FailedDeps["sys-crate 1.0.0"]; got != "failed to run custom build command for `sys-crate v1.0.0`" {
		t.Errorf("the failing dependency's line = %q", got)
	}
	if len(report.FailedDeps) != 1 {
		t.Errorf("only the third-party crate belongs in FailedDeps, got %v", report.FailedDeps)
	}
}

func TestWindowsBreaksReportNamesWhatToDoNext(t *testing.T) {
	report := windowsFixtureReport(t)
	summary := report.summaryMarkdown()
	for _, want := range []string{
		"6 members not yet declared for Windows, checked on x86_64-pc-windows-gnu: 2 compile, 1 fails with 2 errors, 3 blocked",
		"| `broken` | fails | 2 | crates/broken/src/lib.rs:3: cannot find `unix` in `os` |",
		"| `above-broken` | blocked | 0 | blocked by broken |",
		"Ready for the must-compile list",
		"`above-tests-broken`, `clean`",
		"- `sys-crate 1.0.0`: failed to run custom build command",
		"### `broken`: 2 errors",
	} {
		if !strings.Contains(summary, want) {
			t.Errorf("summary is missing %q:\n%s", want, summary)
		}
	}
	errors := report.errorsMarkdown()
	if !strings.Contains(errors, "error[E0308]: mismatched types") || strings.Count(errors, "error[E0433]") != 1 {
		t.Errorf("errors.md should hold each distinct error once:\n%s", errors)
	}
}

func TestCargoErrorLinesKeepsEachErrorWithItsExplanation(t *testing.T) {
	stderr := "    Updating crates.io index\n" +
		"error: failed to select a version for `windows-sys`.\n" +
		"    ... required by package `foo v0.1.0`\n" +
		"\n" +
		"warning: something unrelated\n"
	got := cargoErrorLines(stderr)
	want := []string{"error: failed to select a version for `windows-sys`.", "    ... required by package `foo v0.1.0`"}
	if strings.Join(got, "\n") != strings.Join(want, "\n") {
		t.Fatalf("cargoErrorLines = %q, want %q", got, want)
	}
}

func TestWindowsRatchetRefusesAnUnlistedRestriction(t *testing.T) {
	members := []WorkspaceMember{
		{Name: "listed", Platforms: []string{"linux", "macos"}},
		{Name: "slipped", Platforms: []string{"linux", "macos"}},
		{Name: "no-reason", Platforms: []string{"linux", "macos"}},
		{Name: "promoted"},
	}
	list := windowsPendingAllowlist{Members: map[string]string{
		"listed": "blocked by `cmdr-fs`", "no-reason": " ", "promoted": "fails in its own code", "gone": "renamed",
	}}
	problems, promoted := checkWindowsRatchet(members, &list)

	if len(problems) != 2 || !containsSubstring(problems, `"slipped"`) || !containsSubstring(problems, `"no-reason" with no reason`) {
		t.Errorf("problems = %v, want one for `slipped` and one for `no-reason`", problems)
	}
	if strings.Join(promoted, ",") != "gone,promoted" {
		t.Errorf("promoted = %v, want [gone promoted]", promoted)
	}
	if _, still := list.Members["promoted"]; still {
		t.Error("a member that now builds on Windows should leave the allowlist")
	}
	if _, kept := list.Members["listed"]; !kept {
		t.Error("a still-restricted member should keep its entry")
	}
}

func TestSharedFeaturesFollowTheSelection(t *testing.T) {
	if got := SharedTargetFeatureArgsFor(map[string]bool{"cmdr": true}); strings.Join(got, " ") != "--features cmdr/virtual-mtp" {
		t.Errorf("with the app selected = %v", got)
	}
	// A Windows host today: cargo refuses `cmdr/virtual-mtp` when `cmdr` isn't selected.
	if got := SharedTargetFeatureArgsFor(map[string]bool{"cmdr": false, "cmdr-sort": true}); got != nil {
		t.Errorf("without the app selected = %v, want no --features", got)
	}
}
