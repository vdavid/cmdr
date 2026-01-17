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
