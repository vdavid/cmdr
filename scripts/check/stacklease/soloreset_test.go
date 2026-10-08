package stacklease

import (
	"errors"
	"os"
	"path/filepath"
	"strconv"
	"testing"
)

func TestSmbResetsTheGuestNotifyd(t *testing.T) {
	if SMB.soloResets["smb-consumer-guest"] == "" {
		t.Fatal("the SMB stack must reset the guest's notifyd: every cmdr-smb suite watches its `public` share, and a killed client leaks its watches there for the container's lifetime")
	}
}

func TestASoloAcquireRunsTheStacksResets(t *testing.T) {
	fake := withFake(t)
	if _, err := SMB.Acquire("manual", "e2e"); err != nil {
		t.Fatal(err)
	}
	want := execCall{"smb-consumer-guest", SMB.soloResets["smb-consumer-guest"]}
	if len(fake.execCalls) != 1 || fake.execCalls[0] != want {
		t.Fatalf("want exactly the guest's reset, got %v", fake.execCalls)
	}
}

func TestAnAcquireUnderAnotherLiveHolderResetsNothing(t *testing.T) {
	fake := withFake(t)
	if _, err := SMB.Acquire("manual", "e2e"); err != nil {
		t.Fatal(err)
	}
	before := len(fake.execCalls)
	// Our own PID is a live holder, so "manual" is never alone from here on.
	if _, err := SMB.Acquire(strconv.Itoa(os.Getpid()), "e2e"); err != nil {
		t.Fatal(err)
	}
	if _, err := SMB.Acquire("manual", "e2e"); err != nil {
		t.Fatal(err)
	}
	if len(fake.execCalls) != before {
		t.Fatalf("a reset under a live holder would cut its watches mid-suite; exec calls went %d -> %d", before, len(fake.execCalls))
	}
}

// The killed run that leaked the watches is exactly the holder the sweep reaps,
// so its stale lease must not keep the next run from cleaning up after it.
func TestAHolderThatDiedDoesNotBlockTheReset(t *testing.T) {
	fake := withFake(t)
	if err := os.MkdirAll(SMB.LeaseDir(), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(SMB.LeaseDir(), "2147480000"), []byte("dead"), 0o644); err != nil {
		t.Fatal(err)
	}
	if _, err := SMB.Acquire("manual", "e2e"); err != nil {
		t.Fatal(err)
	}
	if len(fake.execCalls) != 1 {
		t.Fatalf("want the reset once the dead holder is swept, got %v", fake.execCalls)
	}
}

func TestAResetThatFailsLeavesTheAcquireStanding(t *testing.T) {
	fake := withFake(t)
	fake.execErr = errors.New("container not running")
	if _, err := SMB.Acquire("manual", "e2e"); err != nil {
		t.Fatalf("a failed reset only costs speed, never the run: %v", err)
	}
	if got := leaseFiles(t); len(got) != 1 || got[0] != "manual" {
		t.Fatalf("want the lease written anyway, got %v", got)
	}
}

func TestAResetSkipsAServiceTheModeDoesNotBring(t *testing.T) {
	fakes := withFakes(t)
	s := testStack("alpha")
	s.soloResets = map[string]string{"not-in-any-mode": "true"}
	if _, err := s.Acquire("manual", ModeCore); err != nil {
		t.Fatal(err)
	}
	if got := fakes.forStack(s).execCalls; len(got) != 0 {
		t.Fatalf("exec into a service nobody asked for would fail or start nothing useful, got %v", got)
	}
}
