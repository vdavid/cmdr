package checks

import (
	"path/filepath"
	"strings"
	"testing"
)

// The hole this check exists to close: a workspace member that no lane and no
// scanner reaches still compiles, still looks green, and quietly takes its tests
// out of the suite. Every assertion below has to be able to FAIL, or the check is
// decoration.

func TestMemberCoverage_FailsWhenNoLaneCanSelectAMember(t *testing.T) {
	// A member no lane's target OS can build is a member nothing ever tests.
	members := []WorkspaceMember{
		{Name: "cmdr", Kind: KindApp},
		{Name: "cmdr-exotic", Kind: KindApp, Platforms: []string{"freebsd"}},
	}

	problems := findMemberCoverageGaps(members, rustCheckClassification{
		cargoLanes: map[string]string{"desktop-rust-tests": "the whole workspace"},
		scanners:   map[string]ScannerJurisdiction{"desktop-rust-lock-poison": {Kinds: []MemberKind{KindApp}}},
	})

	if !containsSubstring(problems, "cmdr-exotic") {
		t.Fatalf("expected the unbuildable member to be reported, got: %v", problems)
	}
	if containsSubstring(problems, "\"cmdr\"") {
		t.Errorf("the portable member should not be reported, got: %v", problems)
	}
}

func TestMemberCoverage_FailsWhenNoScannerGovernsAMemberKind(t *testing.T) {
	members := []WorkspaceMember{
		{Name: "cmdr", Kind: KindApp},
		{Name: "cmdr-tool", Kind: KindTool},
	}

	problems := findMemberCoverageGaps(members, rustCheckClassification{
		cargoLanes: map[string]string{"desktop-rust-tests": "the whole workspace"},
		// Every scanner governs KindApp only, so the tool member is scanned by nothing.
		scanners: map[string]ScannerJurisdiction{"desktop-rust-lock-poison": {Kinds: []MemberKind{KindApp}}},
	})

	if !containsSubstring(problems, "cmdr-tool") {
		t.Fatalf("expected the unscanned member to be reported, got: %v", problems)
	}
}

func TestMemberCoverage_PassesWhenEveryMemberIsReached(t *testing.T) {
	members := []WorkspaceMember{
		{Name: "cmdr", Kind: KindApp},
		{Name: "cmdr-tool", Kind: KindTool},
		{Name: "cmdr-fsevent-stream", Kind: KindVendored, Platforms: []string{"macos"}},
	}

	problems := findMemberCoverageGaps(members, rustCheckClassification{
		cargoLanes: map[string]string{"desktop-rust-tests": "the whole workspace"},
		scanners: map[string]ScannerJurisdiction{
			"desktop-rust-lock-poison": {Kinds: []MemberKind{KindApp, KindTool}},
			"desktop-rust-jscpd":       {Kinds: []MemberKind{KindApp, KindTool, KindVendored}},
		},
	})

	if len(problems) != 0 {
		t.Fatalf("expected no gaps, got: %v", problems)
	}
}

// A Rust check that's neither a cargo lane nor a declared scanner is the shape of
// "someone added a scanner and hardcoded a path in it".
func TestMemberCoverage_FailsOnAnUnclassifiedRustCheck(t *testing.T) {
	defs := []CheckDefinition{
		{ID: "desktop-rust-tests", Tech: "🦀 Rust"},
		{ID: "desktop-rust-brand-new-scanner", Tech: "🦀 Rust"},
		{ID: "desktop-svelte-eslint", Tech: "🎨 Svelte"},
	}

	problems := findUnclassifiedRustChecks(defs, rustCheckClassification{
		cargoLanes: map[string]string{"desktop-rust-tests": "the whole workspace"},
		scanners:   map[string]ScannerJurisdiction{},
	})

	if !containsSubstring(problems, "desktop-rust-brand-new-scanner") {
		t.Fatalf("expected the unclassified Rust check to be reported, got: %v", problems)
	}
	if containsSubstring(problems, "desktop-svelte-eslint") {
		t.Errorf("a non-Rust check is out of scope, got: %v", problems)
	}
}

// A jurisdiction entry naming a check that no longer exists is a stale excuse.
func TestMemberCoverage_FailsOnAJurisdictionForANonexistentCheck(t *testing.T) {
	defs := []CheckDefinition{{ID: "desktop-rust-tests", Tech: "🦀 Rust"}}

	problems := findStaleJurisdictions(defs, rustCheckClassification{
		cargoLanes: map[string]string{"desktop-rust-tests": "the whole workspace"},
		scanners:   map[string]ScannerJurisdiction{"desktop-rust-renamed-away": {Kinds: []MemberKind{KindApp}}},
	})

	if !containsSubstring(problems, "desktop-rust-renamed-away") {
		t.Fatalf("expected the stale jurisdiction entry to be reported, got: %v", problems)
	}
}

// The real repo has to satisfy its own contract.
func TestMemberCoverage_RealRepoIsCovered(t *testing.T) {
	root, err := filepath.Abs(filepath.Join("..", "..", ".."))
	if err != nil {
		t.Fatalf("failed to resolve repo root: %v", err)
	}
	result, err := RunWorkspaceMemberCoverage(&CheckContext{RootDir: root})
	if err != nil {
		t.Fatalf("workspace member coverage is broken:\n%v", err)
	}
	if result.Code == ResultSkipped {
		t.Fatalf("expected the check to run, got skipped: %s", result.Message)
	}
}

// Every Rust source scanner must resolve its roots through the declared
// jurisdiction, so an unknown check ID has to be an error rather than an empty list
// (which reads as "scanned nothing" and passes).
func TestScannerRootsRejectsAnUndeclaredCheck(t *testing.T) {
	root, err := filepath.Abs(filepath.Join("..", "..", ".."))
	if err != nil {
		t.Fatalf("failed to resolve repo root: %v", err)
	}
	if _, err := ScannerRoots(root, "desktop-rust-not-a-real-check"); err == nil {
		t.Fatal("an undeclared check must fail rather than silently scan nothing")
	}
}

func containsSubstring(items []string, needle string) bool {
	for _, s := range items {
		if strings.Contains(s, needle) {
			return true
		}
	}
	return false
}
