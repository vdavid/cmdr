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
	for _, want := range []checks.StackMode{checks.SmbCore, checks.SmbE2E} {
		stack, err := stacklease.Lookup(want.Stack)
		if err != nil {
			t.Errorf("%s: %v", want, err)
			continue
		}
		if !contains(stack.Modes(), want.Mode) {
			t.Errorf("%s names a mode the %s stack doesn't serve (%v)", want, stack.Name, stack.Modes())
		}
	}
	// `checks.SftpCore` is deliberately absent: the SFTP stack's service table
	// stays empty until its fixture lands, and this is the test that goes red the
	// day someone points a check at it without filling the table in.
	if _, err := stacklease.Lookup(checks.SftpCore.Stack); err != nil {
		t.Errorf("the SFTP stack must be registered even before its fixture lands: %v", err)
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
