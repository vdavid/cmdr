package checks

import (
	"fmt"
	"os"
	"sort"
	"strings"
)

// The workspace grew a second and third crate, and the checks did not follow. Every
// cargo lane was `cmd.Dir`-scoped to the app package and every Rust scanner walked a
// hardcoded `apps/desktop/src-tauri/src`, so anything under `crates/` was never
// tested, linted, formatted, or scanned. Nothing was red about that: a member no
// lane selects compiles fine and reports nothing at all.
//
// This check is what stops the next crate from re-opening the hole. It asserts that
// every workspace member is reachable by the cargo lanes and by the Rust source
// scanners, and that every Rust check has said which of the two it is.

// ScannerJurisdiction says which workspace members a Rust source scanner governs.
// Declared once here and read by the scanner itself through ScannerRoots, so a
// scanner can't drift from what this check believes about it.
type ScannerJurisdiction struct {
	// Kinds the scanner governs. Ignored when AppTreeOnly is set.
	Kinds []MemberKind
	// AppTreeOnly pins the scanner to `apps/desktop/src-tauri/src` regardless of the
	// member list. For the rare check whose remedy exists only inside the app crate,
	// where pointing it at a crate would report violations with no legal fix.
	AppTreeOnly bool
	// Why records the reason for anything narrower than every first-party member. A
	// narrowing with no reason is how coverage quietly erodes.
	Why string
}

// rustScannerJurisdictions is the single declaration of which members each Rust
// source scanner reaches. Adding a scanner without an entry fails this check.
var rustScannerJurisdictions = map[string]ScannerJurisdiction{
	"desktop-rust-lock-poison":        {Kinds: []MemberKind{KindApp, KindTool}},
	"desktop-rust-error-string-match": {Kinds: []MemberKind{KindApp, KindTool}},
	"desktop-rust-test-sleep":         {Kinds: []MemberKind{KindApp, KindTool}},
	"desktop-rust-fixed-temp-dir":     {Kinds: []MemberKind{KindApp, KindTool}},
	"desktop-rust-no-hand-rolled-fixture": {
		Kinds: []MemberKind{KindApp, KindTool},
		Why:   "a test that hand-builds a cross-boundary scan type is the same mistake wherever it lives",
	},
	"desktop-rust-derive-default-justified": {
		Kinds: []MemberKind{KindApp},
		Why:   "narrowed further to the two filesystem trees (`file_system/` and all of `cmdr-fs`), where a zero value is a claim about a disk; elsewhere a `Default` carries no filesystem fact and the rule would be churn",
	},
	"desktop-rust-probe-unwrap-justified": {
		Kinds: []MemberKind{KindApp},
		Why:   "narrowed further to `file_system/`, the only tree where `Volume::is_directory` is called at all",
	},
	"desktop-rust-discarded-outcome": {
		AppTreeOnly: true,
		Why:         "it guards the IPC and MCP surfaces, which only exist in the app tree; a standalone CLI has no surface that could report a success it didn't get",
	},
	"desktop-rust-ipc-enum-camelcase": {Kinds: []MemberKind{KindApp, KindTool}},
	"desktop-rust-jscpd":              {Kinds: []MemberKind{KindApp, KindTool}},
	"desktop-rust-write-ops-agent-isolation": {
		AppTreeOnly: true,
		Why:         "it fences `file_system/write_operations/`, which only exists in the app tree; no other member has a write engine to keep out of the agent module",
	},
	"desktop-rust-macos-availability": {
		Kinds: []MemberKind{KindApp, KindTool, KindVendored},
		Why:   "any member linked into a process on an old Mac can raise the unrecognized-selector exception, and the vendored fork calls into the same frameworks we do",
	},
	"desktop-rust-sqlite-open-direct": {
		Kinds: []MemberKind{KindApp},
		Why:   "the page-cache slab is process-wide, so a standalone CLI opening the first connection in its own process has nothing to protect",
	},
	"desktop-pluralize-noun": {
		Kinds: []MemberKind{KindApp},
		Why:   "`pluralize` is a private module of the app crate, so a tool crate can't reach the helper this check directs it to",
	},
	"desktop-rust-log-error-macro": {
		AppTreeOnly: true,
		Why:         "`log_error!` is a crate-root `macro_rules!` no separate crate can invoke, so a crate has no legal alternative to `log::error!`",
	},
	"desktop-rust-cfg-gate": {
		Kinds: []MemberKind{KindApp, KindTool, KindVendored},
		Why:   "it pairs each member's OWN manifest with that member's tree, so it governs every member that isn't already macOS-only at the crate level",
	},
	"desktop-rust-mtp-dropping-timeout": {
		AppTreeOnly: true,
		Why:         "scoped to `src/mtp/`, the app-side USB transport; the rule is about that one subsystem's wire protocol",
	},
	"desktop-rust-mtp-no-transport-reset": {
		AppTreeOnly: true,
		Why:         "scoped to `src/mtp/`, same subsystem as mtp-dropping-timeout",
	},
	"desktop-fixture-lane-coverage": {
		AppTreeOnly: true,
		Why:         "the name prefixes it enforces are only how the integration lane selects the APP crate's cells; the lane takes each backend crate by package, so a cell there needs no name and would have no legal fix",
	},
}

// rustCargoLanes are the Rust checks that drive cargo rather than scanning source
// trees themselves. Each says how it reaches the workspace, so "covered by the
// package selection" can't be assumed of a lane that never had one.
var rustCargoLanes = map[string]string{
	"desktop-rust-rustfmt":           "`cargo fmt --all`",
	"desktop-rust-clippy":            "`--workspace` via CargoSelectionArgs",
	"desktop-rust-rustdoc":           "every first-party member named explicitly; the vendored fork is skipped, since `--all-features` turns on two mutually exclusive arms there",
	"desktop-rust-cargo-deny":        "reads the whole `cargo metadata` graph from the workspace root",
	"desktop-rust-cargo-audit":       "reads the workspace `Cargo.lock`",
	"desktop-rust-cargo-machete":     "handed each member's directory (it walks dirs, not the cargo graph)",
	"desktop-rust-cargo-udeps":       "`--workspace` via CargoSelectionArgs",
	"desktop-rust-module-cycles":     "every first-party library member (`kind = app` with a `src/lib.rs`), one `--lib` graph each; a bin-only tool has no library graph and the vendored fork's module layout isn't ours to ratchet",
	"desktop-rust-tests":             "`--workspace` via HostCargoLaneArgs",
	"nextest-filter-coverage":        "`--workspace` via HostCargoLaneArgs, listing rather than running, so it sees exactly the tests `desktop-rust-tests` does",
	"desktop-rust-integration-tests": "`--workspace` via HostCargoLaneArgs, narrowed by a filter expression",
	"desktop-rust-webdav-nextcloud":  "`--workspace` via HostCargoLaneArgs, narrowed to one module of `cmdr-webdav`; the cells the shared fixture lane subtracts",
	"desktop-rust-tests-linux":       "`--workspace` computed for `linux`, since cargo runs in a container",
	"desktop-bindings-fresh":         "hashes every member's sources and manifest to decide whether to regenerate; the regen itself is `--workspace` via `pnpm bindings:regen`",
	// Not coverage lanes: a handful of named tests against a live endpoint, each
	// self-skipping without its key. They reach the app crate on purpose and nothing else.
	"desktop-rust-groq-smoke":      "one `--lib` test module in the app crate; a targeted smoke, not a sweep (selected via HostCargoLaneArgs so it shares the other lanes' artifacts)",
	"desktop-rust-fireworks-smoke": "one `--lib` test module in the app crate; same shape as the Groq smoke",
	"desktop-rust-anthropic-smoke": "one `--lib` test module in the app crate; same shape as the Groq smoke",
	"desktop-rust-openai-smoke":    "one `--lib` test module in the app crate; same shape as the Groq smoke",
	"desktop-rust-gemini-smoke":    "one `--lib` test module in the app crate; same shape as the Groq smoke",
}

// memberCoverageRegistry is assigned in init() rather than read from AllChecks
// directly: AllChecks's initializer references RunWorkspaceMemberCoverage (this
// check is registered there), and Go rejects the resulting initialization cycle.
// Same pattern as `ci-coverage`.
var memberCoverageRegistry []CheckDefinition

func init() { memberCoverageRegistry = AllChecks }

// rustMetaChecks reason ABOUT the workspace rather than compiling or scanning it,
// so "which members do you reach?" isn't a question they answer. Each entry says
// what it does instead.
var rustMetaChecks = map[string]string{
	"workspace-member-coverage":     "this check; it reads the member list and the registry, not the sources",
	"index-crate-isolation":         "it reads the `cargo metadata` graph and counts `cmdr-index`'s public surface; both are about two named crates, not a sweep",
	"desktop-shipped-locales-fresh": "it regenerates ONE file in the app crate from the message-catalog dirs and diffs it; the inputs are catalog directories, not workspace sources",
	"desktop-native-strings-fresh":  "same shape as shipped-locales-fresh: it regenerates ONE file in the app crate from the message catalogs and diffs it, so its inputs are catalog files, not workspace sources",
}

// rustCheckClassification is the partition of the Rust checks: each is a cargo
// lane, a source scanner with a declared jurisdiction, or a meta-check. It's a
// parameter so the tests can drive the logic with a fixture partition.
type rustCheckClassification struct {
	cargoLanes map[string]string
	scanners   map[string]ScannerJurisdiction
	metaChecks map[string]string
}

func liveRustCheckClassification() rustCheckClassification {
	return rustCheckClassification{
		cargoLanes: rustCargoLanes,
		scanners:   rustScannerJurisdictions,
		metaChecks: rustMetaChecks,
	}
}

// ScannerRoots returns the source roots the named Rust scanner should walk. Every
// scanner resolves its roots through here, which is what keeps the jurisdiction
// table honest: an undeclared check is an error rather than an empty list, because
// an empty list reads as "scanned nothing" and passes.
func ScannerRoots(rootDir, checkID string) ([]string, error) {
	jurisdiction, ok := rustScannerJurisdictions[checkID]
	if !ok {
		return nil, fmt.Errorf(
			"%s has no entry in `rustScannerJurisdictions`; declare which workspace member kinds it governs "+
				"(scripts/check/checks/workspace-member-coverage.go)", checkID)
	}
	if jurisdiction.AppTreeOnly {
		return existingDirs([]string{appRustSrcDir(rootDir)}), nil
	}
	if len(jurisdiction.Kinds) == 0 {
		return nil, fmt.Errorf(
			"%s declares neither member kinds nor `AppTreeOnly`, so it would scan nothing and pass", checkID)
	}
	return RustSrcRoots(rootDir, jurisdiction.Kinds...)
}

// ScannerMemberKinds returns the member kinds the named scanner governs, for the
// scanners that need the members themselves rather than just their `src/` trees.
func ScannerMemberKinds(checkID string) ([]MemberKind, error) {
	jurisdiction, ok := rustScannerJurisdictions[checkID]
	if !ok {
		return nil, fmt.Errorf(
			"%s has no entry in `rustScannerJurisdictions`; declare which workspace member kinds it governs "+
				"(scripts/check/checks/workspace-member-coverage.go)", checkID)
	}
	if jurisdiction.AppTreeOnly {
		return nil, fmt.Errorf(
			"%s is pinned to the app tree, so it has no member kinds; use ScannerRoots instead", checkID)
	}
	if len(jurisdiction.Kinds) == 0 {
		return nil, fmt.Errorf(
			"%s declares neither member kinds nor `AppTreeOnly`, so it would govern nothing and pass", checkID)
	}
	return jurisdiction.Kinds, nil
}

// RunWorkspaceMemberCoverage verifies that nothing in the workspace is invisible to
// the checks.
func RunWorkspaceMemberCoverage(ctx *CheckContext) (CheckResult, error) {
	members, err := WorkspaceMembers(ctx.RootDir)
	if err != nil {
		return CheckResult{}, err
	}
	classification := liveRustCheckClassification()

	var problems []string
	problems = append(problems, findMemberCoverageGaps(members, classification)...)
	problems = append(problems, findUnclassifiedRustChecks(memberCoverageRegistry, classification)...)
	problems = append(problems, findStaleJurisdictions(memberCoverageRegistry, classification)...)
	problems = append(problems, findMalformedJurisdictions(classification.scanners)...)

	if len(problems) > 0 {
		sort.Strings(problems)
		return CheckResult{}, fmt.Errorf(
			"%d workspace coverage %s:\n  %s",
			len(problems), Pluralize(len(problems), "gap", "gaps"), strings.Join(problems, "\n  "),
		)
	}

	return Success(fmt.Sprintf(
		"%d workspace %s covered by %d cargo %s and %d source %s",
		len(members), Pluralize(len(members), "member", "members"),
		len(classification.cargoLanes), Pluralize(len(classification.cargoLanes), "lane", "lanes"),
		len(classification.scanners), Pluralize(len(classification.scanners), "scanner", "scanners"),
	)), nil
}

// findMemberCoverageGaps reports members nothing reaches.
func findMemberCoverageGaps(members []WorkspaceMember, c rustCheckClassification) []string {
	var problems []string
	for _, m := range members {
		// The cargo lanes run on macOS (locally and for the macOS-only bits) and on
		// Linux (CI's "Desktop (Rust)" job, plus the Docker lane). A member neither
		// can build is a member no lane ever compiles or tests.
		if !m.BuildsOn("macos") && !m.BuildsOn("linux") {
			problems = append(problems, fmt.Sprintf(
				"member %q declares platforms %v, so no cargo lane can ever select it: its tests never run",
				m.Name, m.Platforms))
		}

		governed := false
		for _, j := range c.scanners {
			if j.AppTreeOnly {
				continue
			}
			for _, k := range j.Kinds {
				if k == m.Kind {
					governed = true
					break
				}
			}
			if governed {
				break
			}
		}
		if !governed {
			problems = append(problems, fmt.Sprintf(
				"member %q is kind %q, which no Rust source scanner governs: its sources are scanned by nothing",
				m.Name, m.Kind))
		}
	}
	return problems
}

// findUnclassifiedRustChecks reports Rust checks that are neither a cargo lane nor a
// declared source scanner. That's the shape of "someone added a scanner and
// hardcoded a path inside it".
func findUnclassifiedRustChecks(defs []CheckDefinition, c rustCheckClassification) []string {
	var problems []string
	for _, def := range defs {
		if !strings.Contains(def.Tech, "Rust") {
			continue
		}
		if _, ok := c.cargoLanes[def.ID]; ok {
			continue
		}
		if _, ok := c.scanners[def.ID]; ok {
			continue
		}
		if _, ok := c.metaChecks[def.ID]; ok {
			continue
		}
		problems = append(problems, fmt.Sprintf(
			"check %q is a Rust check but is in none of `rustCargoLanes`, `rustScannerJurisdictions`, "+
				"or `rustMetaChecks`: say which workspace members it reaches", def.ID))
	}
	return problems
}

// findStaleJurisdictions reports declarations for checks that no longer exist, the
// same way `ci-coverage` refuses to let an excuse outlive its check.
func findStaleJurisdictions(defs []CheckDefinition, c rustCheckClassification) []string {
	known := make(map[string]bool, len(defs))
	for _, def := range defs {
		known[def.ID] = true
	}
	var problems []string
	for id := range c.scanners {
		if !known[id] {
			problems = append(problems, fmt.Sprintf(
				"`rustScannerJurisdictions` names %q, which is not a registered check", id))
		}
	}
	for id := range c.cargoLanes {
		if !known[id] {
			problems = append(problems, fmt.Sprintf(
				"`rustCargoLanes` names %q, which is not a registered check", id))
		}
	}
	for id := range c.metaChecks {
		if !known[id] {
			problems = append(problems, fmt.Sprintf(
				"`rustMetaChecks` names %q, which is not a registered check", id))
		}
	}
	return problems
}

// findMalformedJurisdictions reports entries that don't actually say anything
// enforceable. The empty declaration is the dangerous one: it makes `ScannerRoots`
// hand back no roots, and a scanner with no roots scans nothing and passes.
//
// The default breadth is app + tool, i.e. every first-party member. Anything
// narrower carries a `Why`, so a future reader can tell a deliberate exception from
// coverage that eroded one convenient omission at a time.
func findMalformedJurisdictions(scanners map[string]ScannerJurisdiction) []string {
	var problems []string
	for id, j := range scanners {
		switch {
		case j.AppTreeOnly && len(j.Kinds) > 0:
			problems = append(problems, fmt.Sprintf(
				"jurisdiction %q declares both `AppTreeOnly` and member kinds; it can only be one", id))
		case !j.AppTreeOnly && len(j.Kinds) == 0:
			problems = append(problems, fmt.Sprintf(
				"jurisdiction %q declares neither member kinds nor `AppTreeOnly`, so the scanner would walk nothing and pass", id))
		case j.AppTreeOnly && j.Why == "":
			problems = append(problems, fmt.Sprintf(
				"jurisdiction %q pins the app tree but records no `Why`; say what remedy only exists in the app crate", id))
		}
		if j.AppTreeOnly || j.Why != "" {
			continue
		}
		for _, kind := range []MemberKind{KindApp, KindTool} {
			if !jurisdictionCovers(j, kind) {
				problems = append(problems, fmt.Sprintf(
					"jurisdiction %q leaves out kind %q without a `Why`; a narrowing has to say why", id, kind))
			}
		}
	}
	return problems
}

func jurisdictionCovers(j ScannerJurisdiction, kind MemberKind) bool {
	for _, k := range j.Kinds {
		if k == kind {
			return true
		}
	}
	return false
}

// existingDirs drops paths that aren't directories on disk. Walking a missing root
// is an error in `filepath.WalkDir`, and a fresh worktree or a not-yet-created crate
// legitimately has none.
func existingDirs(paths []string) []string {
	out := make([]string, 0, len(paths))
	for _, p := range paths {
		if info, err := os.Stat(p); err == nil && info.IsDir() {
			out = append(out, p)
		}
	}
	return out
}
