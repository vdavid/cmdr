package checks

import (
	"testing"
)

func TestValidateCheckNames_NoCollisions(t *testing.T) {
	// The actual AllChecks should have no collisions
	if err := ValidateCheckNames(); err != nil {
		t.Errorf("ValidateCheckNames() failed on actual registry: %v", err)
	}
}

// TestEveryCheckDeclaresInputs guards the input-fingerprint cache contract: a
// check with no Inputs is fingerprinted on the global inputs alone, so it would
// be cache-skipped on any change to its own files — a correctness hole. Every
// check must declare what it reads (use wholeRepoInputs for whole-tree scanners).
func TestEveryCheckDeclaresInputs(t *testing.T) {
	for _, c := range AllChecks {
		if len(c.Inputs) == 0 {
			t.Errorf("check %q declares no Inputs; the cache can't tell what it reads (see checks/inputs.go)", c.ID)
		}
	}
}

func TestValidateCheckNames_DetectsNicknameIDCollision(t *testing.T) {
	// Save original and restore after test
	original := AllChecks
	defer func() { AllChecks = original }()

	// Create a test case where a nickname conflicts with another check's ID
	AllChecks = []CheckDefinition{
		{ID: "check-a", Nickname: "short-a", DisplayName: "A", App: AppDesktop, Tech: "Test"},
		{ID: "short-a", DisplayName: "B", App: AppDesktop, Tech: "Test"}, // ID collides with check-a's nickname
	}

	err := ValidateCheckNames()
	if err == nil {
		t.Error("ValidateCheckNames() should detect nickname-ID collision")
	}
}

func TestValidateCheckNames_DetectsDuplicateNicknames(t *testing.T) {
	original := AllChecks
	defer func() { AllChecks = original }()

	AllChecks = []CheckDefinition{
		{ID: "check-a", Nickname: "short", DisplayName: "A", App: AppDesktop, Tech: "Test"},
		{ID: "check-b", Nickname: "short", DisplayName: "B", App: AppDesktop, Tech: "Test"}, // Same nickname
	}

	err := ValidateCheckNames()
	if err == nil {
		t.Error("ValidateCheckNames() should detect duplicate nicknames")
	}
}

func TestValidateCheckNames_RejectsReservedSelectorNames(t *testing.T) {
	original := AllChecks
	defer func() { AllChecks = original }()

	AllChecks = []CheckDefinition{
		{ID: "rust", DisplayName: "A", App: AppDesktop, Tech: "Test"},
	}
	if err := ValidateCheckNames("rust", "svelte"); err == nil {
		t.Error("ValidateCheckNames() should reject a check ID that shadows a reserved selector keyword")
	}

	AllChecks = []CheckDefinition{
		{ID: "check-a", Nickname: "svelte", DisplayName: "A", App: AppDesktop, Tech: "Test"},
	}
	if err := ValidateCheckNames("rust", "svelte"); err == nil {
		t.Error("ValidateCheckNames() should reject a nickname that shadows a reserved selector keyword")
	}
}

func TestCLIName(t *testing.T) {
	tests := []struct {
		name     string
		def      CheckDefinition
		expected string
	}{
		{
			name:     "returns nickname when set",
			def:      CheckDefinition{ID: "full-id", Nickname: "short"},
			expected: "short",
		},
		{
			name:     "returns ID when nickname is empty",
			def:      CheckDefinition{ID: "full-id", Nickname: ""},
			expected: "full-id",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := tt.def.CLIName(); got != tt.expected {
				t.Errorf("CLIName() = %v, want %v", got, tt.expected)
			}
		})
	}
}

func TestGetCheckByID_MatchesNickname(t *testing.T) {
	original := AllChecks
	defer func() { AllChecks = original }()

	AllChecks = []CheckDefinition{
		{ID: "desktop-svelte-check", Nickname: "svelte-check", DisplayName: "svelte-check", App: AppDesktop, Tech: "Test"},
	}

	// Should find by ID
	if check := GetCheckByID("desktop-svelte-check"); check == nil {
		t.Error("GetCheckByID() should find check by ID")
	}

	// Should find by nickname
	if check := GetCheckByID("svelte-check"); check == nil {
		t.Error("GetCheckByID() should find check by nickname")
	}

	// Should not find unknown
	if check := GetCheckByID("unknown"); check != nil {
		t.Error("GetCheckByID() should return nil for unknown check")
	}
}

// TestFilterDisabledChecks_ExcludedUnlessNamed pins the mothball contract: a
// check carrying a Disabled reason leaves every suite, and the ONLY way back is
// naming it. There's deliberately no bulk flag, so a disabled check can't
// rejoin a lane by accident.
func TestFilterDisabledChecks_ExcludedUnlessNamed(t *testing.T) {
	defs := []CheckDefinition{
		{ID: "live-one"},
		{ID: "mothballed", Disabled: "noisy and low value"},
	}

	kept := FilterDisabledChecks(defs, nil)
	if len(kept) != 1 || kept[0].ID != "live-one" {
		t.Errorf("unnamed run kept %v; want just [live-one]", idsOf(kept))
	}
}

// The named-check escape hatch resolves through GetCheckByID, so it has to work
// off the real registry rather than a synthetic slice.
func TestFilterDisabledChecks_NamedCheckRunsAnyway(t *testing.T) {
	var disabled *CheckDefinition
	for i := range AllChecks {
		if AllChecks[i].Disabled != "" {
			disabled = &AllChecks[i]
			break
		}
	}
	if disabled == nil {
		t.Skip("no check is currently mothballed; nothing to assert")
	}

	if kept := FilterDisabledChecks(AllChecks, nil); containsID(kept, disabled.ID) {
		t.Errorf("check %q is Disabled but survived an unnamed run", disabled.ID)
	}
	if kept := FilterDisabledChecks(AllChecks, []string{disabled.ID}); !containsID(kept, disabled.ID) {
		t.Errorf("check %q was named explicitly but was still filtered out", disabled.ID)
	}
}

// A mothballed check must not also claim a lane it can no longer be part of: a
// leftover IsFast/IsSlow/CIOnly reads as "runs in that lane" to anyone skimming
// the registry, and would come back the moment Disabled is lifted.
func TestDisabledChecksClaimNoLane(t *testing.T) {
	for _, c := range AllChecks {
		if c.Disabled == "" {
			continue
		}
		if c.IsFast || c.IsSlow || c.CIOnly {
			t.Errorf("check %q is Disabled but still marked IsFast/IsSlow/CIOnly; clear the lane flags", c.ID)
		}
		if c.NotInCI == "" {
			t.Errorf("check %q is Disabled but has no NotInCI reason; a mothballed check is never in a workflow", c.ID)
		}
	}
}

func idsOf(defs []CheckDefinition) []string {
	ids := make([]string, 0, len(defs))
	for _, d := range defs {
		ids = append(ids, d.ID)
	}
	return ids
}

func containsID(defs []CheckDefinition, id string) bool {
	for _, d := range defs {
		if d.ID == id {
			return true
		}
	}
	return false
}
