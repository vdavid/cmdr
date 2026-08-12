package main

import (
	"errors"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"cmdr/scripts/check/checks"
)

// errChecked stands in for whatever a red lane returns; only its non-nil-ness
// matters here.
var errChecked = errors.New("checked and red")

// overlapProbe records whether two or more checks were ever inside their Run
// function at the same moment. Concurrency is the thing under test, so it's
// observed directly rather than inferred from durations.
type overlapProbe struct {
	mu       sync.Mutex
	running  map[string]bool
	overlaps atomic.Int32
}

func newOverlapProbe() *overlapProbe {
	return &overlapProbe{running: map[string]bool{}}
}

// runFunc returns a CheckFunc that stays inside Run long enough for a second
// admission attempt to happen, and flags any co-tenancy it sees.
func (p *overlapProbe) runFunc(id string) checks.CheckFunc {
	return func(*checks.CheckContext) (checks.CheckResult, error) {
		p.mu.Lock()
		if len(p.running) > 0 {
			p.overlaps.Add(1)
		}
		p.running[id] = true
		p.mu.Unlock()

		time.Sleep(50 * time.Millisecond)

		p.mu.Lock()
		delete(p.running, id)
		p.mu.Unlock()
		return checks.Success("ok"), nil
	}
}

func exclusiveDef(id, resource string, run checks.CheckFunc) checks.CheckDefinition {
	return checks.CheckDefinition{
		ID:          id,
		Nickname:    id,
		DisplayName: id,
		App:         checks.AppDesktop,
		Tech:        "🦀 Rust",
		CpuWeight:   1,
		Exclusive:   resource,
		Inputs:      []string{"**/*.rs"},
		Run:         run,
	}
}

// Two checks naming the same resource must never run at once, however much CPU
// budget is free. That's what keeps cargo lanes off each other's build-directory
// lock, where the loser would block invisibly while still holding its weight.
func TestRunner_SameExclusiveResourceNeverOverlaps(t *testing.T) {
	probe := newOverlapProbe()
	defs := []checks.CheckDefinition{
		exclusiveDef("a", "cargo-build-dir", probe.runFunc("a")),
		exclusiveDef("b", "cargo-build-dir", probe.runFunc("b")),
		exclusiveDef("c", "cargo-build-dir", probe.runFunc("c")),
	}

	r := NewRunner(&checks.CheckContext{}, defs, nil, false, true, true)
	if failed, _ := r.Run(); failed {
		t.Fatal("expected every check to pass")
	}
	if got := probe.overlaps.Load(); got != 0 {
		t.Errorf("checks sharing a resource overlapped %d time(s), want 0", got)
	}
}

// The control: without a resource, the weight budget alone decides, so light
// checks still pack in together. A resource must not become a global lock.
func TestRunner_ChecksWithoutAResourceStillOverlap(t *testing.T) {
	probe := newOverlapProbe()
	defs := []checks.CheckDefinition{
		exclusiveDef("a", "", probe.runFunc("a")),
		exclusiveDef("b", "", probe.runFunc("b")),
	}

	r := NewRunner(&checks.CheckContext{}, defs, nil, false, true, true)
	if failed, _ := r.Run(); failed {
		t.Fatal("expected every check to pass")
	}
	if probe.overlaps.Load() == 0 {
		t.Error("independent light checks never ran concurrently; the gate is over-serializing")
	}
}

// Different resources are independent, so a doc build in its own target
// directory runs beside a lane holding the shared one.
func TestRunner_DifferentResourcesRunConcurrently(t *testing.T) {
	probe := newOverlapProbe()
	defs := []checks.CheckDefinition{
		exclusiveDef("a", "cargo-build-dir", probe.runFunc("a")),
		exclusiveDef("b", "cargo-doc-dir", probe.runFunc("b")),
	}

	r := NewRunner(&checks.CheckContext{}, defs, nil, false, true, true)
	if failed, _ := r.Run(); failed {
		t.Fatal("expected every check to pass")
	}
	if probe.overlaps.Load() == 0 {
		t.Error("checks holding different resources never ran concurrently")
	}
}

// A resource is released even when its holder fails, so one red lane can't
// strand every other lane naming the same resource.
func TestRunner_ResourceIsReleasedAfterAFailure(t *testing.T) {
	var ran atomic.Int32
	failing := func(*checks.CheckContext) (checks.CheckResult, error) {
		ran.Add(1)
		return checks.CheckResult{}, errChecked
	}
	passing := func(*checks.CheckContext) (checks.CheckResult, error) {
		ran.Add(1)
		return checks.Success("ok"), nil
	}

	defs := []checks.CheckDefinition{
		exclusiveDef("a", "cargo-build-dir", failing),
		exclusiveDef("b", "cargo-build-dir", passing),
	}

	r := NewRunner(&checks.CheckContext{}, defs, nil, false, true, true)
	if failed, _ := r.Run(); !failed {
		t.Fatal("expected the run to be red")
	}
	if got := ran.Load(); got != 2 {
		t.Errorf("%d of 2 checks ran; a failure stranded the resource", got)
	}
}
