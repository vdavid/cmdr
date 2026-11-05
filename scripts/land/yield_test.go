package main

import (
	"strings"
	"testing"
	"time"
)

func TestDefaultYieldAfter(t *testing.T) {
	if got := defaultYieldAfter(""); got != 0 {
		t.Errorf("outside Claude Code: got %s, want no yield", got)
	}
	if got := defaultYieldAfter("1"); got != claudeYieldAfter {
		t.Errorf("in Claude Code: got %s, want %s", got, claudeYieldAfter)
	}
}

func TestShouldYield(t *testing.T) {
	cases := []struct {
		name       string
		yieldAfter time.Duration
		running    time.Duration
		want       bool
	}{
		{"disabled", 0, 3 * time.Hour, false},
		{"early", 50 * time.Minute, 49 * time.Minute, false},
		{"at the threshold", 50 * time.Minute, 50 * time.Minute, true},
		{"past it", 50 * time.Minute, 51 * time.Minute, true},
	}
	for _, c := range cases {
		if got := shouldYield(c.yieldAfter, c.running); got != c.want {
			t.Errorf("%s: got %v, want %v", c.name, got, c.want)
		}
	}
}

func TestYieldMessage(t *testing.T) {
	now := at(10)
	est := ciEstimate{typical: at(20), high: at(25)}
	msg := yieldMessage(est, now, "pnpm land")
	if !strings.HasPrefix(msg, "Still waiting for CI OK (expect it around ") || !strings.Contains(msg, "in about 10 min, up to 15 min") || !strings.HasSuffix(msg, "Run `pnpm land` again to keep waiting.") {
		t.Errorf("with an estimate: %q", msg)
	}
	est.queueWait, est.queuedAhead = 6*time.Minute, 12
	if msg := yieldMessage(est, now, "pnpm land"); !strings.Contains(msg, "; 12 jobs ahead of ours in the Linux queue") {
		t.Errorf("queued: %q", msg)
	}
	if msg := yieldMessage(ciEstimate{}, now, "pnpm land -record 5"); msg != "Still waiting for CI OK (no ETA yet). Run `pnpm land -record 5` again to keep waiting." {
		t.Errorf("no estimate: %q", msg)
	}
}
