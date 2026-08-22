package main

import (
	"testing"

	"cmdr/scripts/check/checks"
	"cmdr/scripts/check/stacklease"
)

// A check declares its fixture needs as two strings, so nothing but this test
// stands between a typo and a run that dies at bring-up time — after planning,
// after `pnpm install`, minutes in.
func TestEveryDeclaredStackModeResolves(t *testing.T) {
	for _, def := range checks.AllChecks {
		for _, want := range def.NeedsContainers {
			stack, err := stacklease.Lookup(want.Stack)
			if err != nil {
				t.Errorf("%s asks for %s: %v", def.ID, want, err)
				continue
			}
			if !contains(stack.Modes(), want.Mode) {
				t.Errorf("%s asks for %s, but the %s stack serves %v",
					def.ID, want, stack.Name, stack.Modes())
			}
		}
	}
}

// The pairs a check can name must each resolve too, so one is ready to be
// declared rather than debugged.
func TestDeclarableStackModesResolve(t *testing.T) {
	for _, want := range []checks.StackMode{checks.SmbCore, checks.SmbE2E, checks.SftpCore} {
		stack, err := stacklease.Lookup(want.Stack)
		if err != nil {
			t.Errorf("%s: %v", want, err)
			continue
		}
		if !contains(stack.Modes(), want.Mode) {
			t.Errorf("%s names a mode the %s stack doesn't serve (%v)", want, stack.Name, stack.Modes())
		}
	}
}

// Every port-env applier must name a registered stack, or a stack quietly comes
// up on whatever ports the ambient environment happens to carry.
func TestPortEnvAppliersNameRegisteredStacks(t *testing.T) {
	for name := range portEnvAppliers {
		if _, err := stacklease.Lookup(name); err != nil {
			t.Errorf("port-env applier for %q: %v", name, err)
		}
	}
}

func contains(haystack []string, needle string) bool {
	for _, s := range haystack {
		if s == needle {
			return true
		}
	}
	return false
}

// The orchestrator starts the union of what the planned checks need, once each.
// Two checks on one stack must not acquire twice, and a check needing two stacks
// must get both.
func TestCollectStackModesTakesTheUnionOnce(t *testing.T) {
	smbA := checks.CheckDefinition{ID: "a", NeedsContainers: []checks.StackMode{checks.SmbCore}}
	smbB := checks.CheckDefinition{ID: "b", NeedsContainers: []checks.StackMode{checks.SmbCore}}
	e2e := checks.CheckDefinition{ID: "c", NeedsContainers: []checks.StackMode{checks.SmbE2E}}
	both := checks.CheckDefinition{ID: "d", NeedsContainers: []checks.StackMode{checks.SmbCore, checks.SftpCore}}
	none := checks.CheckDefinition{ID: "e"}

	got := collectStackModes([]checks.CheckDefinition{smbB, none, smbA, both, e2e})
	want := []string{"sftp/core", "smb/core", "smb/e2e"}
	if len(got) != len(want) {
		t.Fatalf("collectStackModes = %v, want %v", got, want)
	}
	for i, pair := range got {
		if pair.String() != want[i] {
			t.Fatalf("collectStackModes = %v, want %v (order is what makes the logs reproducible)", got, want)
		}
	}
}

// A check needing two stacks annotates as both, so `--graph` says what a run
// will bring up.
func TestFixtureStackNamesListsEveryDistinctStack(t *testing.T) {
	cases := []struct {
		def  checks.CheckDefinition
		want string
	}{
		{checks.CheckDefinition{}, ""},
		{checks.CheckDefinition{NeedsContainers: []checks.StackMode{checks.SmbCore}}, "smb"},
		{checks.CheckDefinition{NeedsContainers: []checks.StackMode{checks.SmbCore, checks.SmbE2E}}, "smb"},
		{checks.CheckDefinition{NeedsContainers: []checks.StackMode{checks.SmbCore, checks.SftpCore}}, "smb+sftp"},
	}
	for _, c := range cases {
		if got := fixtureStackNames(&c.def); got != c.want {
			t.Errorf("fixtureStackNames(%v) = %q, want %q", c.def.NeedsContainers, got, c.want)
		}
	}
}
