package main

import (
	"slices"
	"strings"
	"testing"
)

func job(name, status, conclusion string) ciJob {
	return ciJob{Name: name, Status: status, Conclusion: conclusion}
}

func marker(conclusion string) ciJob {
	if conclusion == "" {
		return job(fullRunMarker, "queued", "")
	}
	return job(fullRunMarker, "completed", conclusion)
}

func TestJudgeCIFailsFastOnARedJobWhileOthersRun(t *testing.T) {
	run := ciRun{ID: 1, Status: "in_progress", Jobs: []ciJob{
		marker("success"),
		job("hygiene", "completed", "failure"),
		job("desktop-rust-macos", "in_progress", ""),
	}}
	state, failed := judgeCI(run)
	if state != ciRed {
		t.Fatalf("got %v, want red while the run is still going", state)
	}
	if len(failed) != 1 || failed[0].Name != "hygiene" {
		t.Fatalf("got failed jobs %v, want only hygiene", failed)
	}
}

func TestJudgeCI(t *testing.T) {
	cases := []struct {
		name string
		run  ciRun
		want ciState
	}{
		{"queued, no jobs yet", ciRun{Status: "queued"}, ciPending},
		{"running, nothing red", ciRun{Status: "in_progress", Jobs: []ciJob{marker("success"), job("a", "in_progress", "")}}, ciPending},
		{"green", ciRun{Status: "completed", Conclusion: "success", Jobs: []ciJob{marker("success"), job("a", "completed", "success"), job("b", "completed", "skipped")}}, ciGreen},
		{"a cancelled job", ciRun{Status: "in_progress", Jobs: []ciJob{job("a", "completed", "cancelled")}}, ciRed},
		{"a timed-out job", ciRun{Status: "in_progress", Jobs: []ciJob{job("a", "completed", "timed_out")}}, ciRed},
		{"run ended red with every job listed green", ciRun{Status: "completed", Conclusion: "failure", Jobs: []ciJob{job("a", "completed", "success")}}, ciRed},
		{"run cancelled", ciRun{Status: "completed", Conclusion: "cancelled"}, ciRed},
		{
			// `gh run rerun --failed` reopens the run: the failed job is queued again, so the wait
			// picks the new attempt up instead of reading the old red.
			"a re-run of the failed jobs",
			ciRun{Status: "queued", Jobs: []ciJob{marker("success"), job("a", "completed", "success"), job("hygiene", "queued", "")}},
			ciPending,
		},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			if got, _ := judgeCI(c.run); got != c.want {
				t.Fatalf("got %v, want %v", got, c.want)
			}
		})
	}
}

func TestNewestFullRun(t *testing.T) {
	partial := ciRun{ID: 3, CreatedAt: at(9), Jobs: []ciJob{marker("skipped")}}
	full := ciRun{ID: 1, CreatedAt: at(1), Jobs: []ciJob{marker("success")}}
	newerFull := ciRun{ID: 2, CreatedAt: at(5), Jobs: []ciJob{marker("success")}}
	// A run whose jobs aren't listed yet may still be the full one: it counts, so a gate waits on
	// it rather than passing over it.
	justQueued := ciRun{ID: 4, CreatedAt: at(10)}

	if got, ok := newestFullRun([]ciRun{full, partial, newerFull}); !ok || got.ID != 2 {
		t.Fatalf("got run %d (ok %v), want 2: a run_all-off dispatch never counts", got.ID, ok)
	}
	if got, ok := newestFullRun([]ciRun{full, justQueued}); !ok || got.ID != 4 {
		t.Fatalf("got run %d (ok %v), want the just-queued 4", got.ID, ok)
	}
	if _, ok := newestFullRun([]ciRun{partial}); ok {
		t.Fatal("a dispatch with run_all off isn't a full run")
	}
	// A finished run without the marker predates it, so it can't prove anything.
	if _, ok := newestFullRun([]ciRun{{ID: 5, Status: "completed", Conclusion: "success", Jobs: []ciJob{job("a", "completed", "success")}}}); ok {
		t.Fatal("a finished run without the marker job isn't a full run")
	}
}

func TestGateCommit(t *testing.T) {
	// The tagged commit's own full run decides whenever it has one, even when its parent has a
	// green one: a red or running run on the release commit must hold the release.
	if got := gateCommit(true, true); got != gateOwn {
		t.Fatalf("got %v, want the release commit's own run", got)
	}
	// The hand-made security-only release gates the fix commit, then bumps on top of it.
	if got := gateCommit(false, true); got != gateParent {
		t.Fatalf("got %v, want the parent's run", got)
	}
	// Any other commit needs a run of its own.
	if got := gateCommit(false, false); got != gateOwn {
		t.Fatalf("got %v, want the commit's own run", got)
	}
}

func TestJobChangesPrintsEachJobOncePerState(t *testing.T) {
	seen := map[string]string{}
	first := jobChanges(seen, []ciJob{job("a", "queued", ""), job("b", "in_progress", "")})
	if !slices.Equal(first, []string{"a: queued", "b: in_progress"}) {
		t.Fatalf("got %v", first)
	}
	second := jobChanges(seen, []ciJob{job("a", "queued", ""), job("b", "completed", "success")})
	if !slices.Equal(second, []string{"b: success"}) {
		t.Fatalf("got %v, want only b's change", second)
	}
}

func TestCIRedMessageSaysWhyAndWhatNext(t *testing.T) {
	failed := []ciJob{{ID: 77, Name: "hygiene", Status: "completed", Conclusion: "failure", URL: "https://example/job/77"}}
	msg := ciRedMessage("vdavid/cmdr", ciRun{ID: 42}, failed, "0.56.0")
	for _, want := range []string{
		"hygiene: failure https://example/job/77",
		// `gh run view --log-failed` refuses while the run goes on, which is when a fail-fast gate stops.
		"gh api --allow-escape-sequences repos/vdavid/cmdr/actions/jobs/77/logs | tail -100",
		"gh run rerun 42 --failed -R vdavid/cmdr",
		"./scripts/release.sh -abort 0.56.0",
	} {
		if !strings.Contains(msg, want) {
			t.Errorf("the message lacks %q:\n%s", want, msg)
		}
	}
}
