package main

import (
	"slices"
	"testing"
	"time"
)

func at(minute int) time.Time {
	return time.Date(2026, 10, 6, 12, minute, 0, 0, time.UTC)
}

func TestNextFinishAction(t *testing.T) {
	cases := []struct {
		name   string
		runs   []workflowRun
		want   finishAction
		wantID int64
	}{
		{"no dispatch yet", nil, actionDispatch, 0},
		{"one still running", []workflowRun{{ID: 1, Status: "in_progress", CreatedAt: at(1)}}, actionWait, 1},
		{"one queued", []workflowRun{{ID: 1, Status: "queued", CreatedAt: at(1)}}, actionWait, 1},
		{"one green", []workflowRun{{ID: 1, Status: "completed", Conclusion: "success", CreatedAt: at(1)}}, actionDone, 1},
		{"one red", []workflowRun{{ID: 1, Status: "completed", Conclusion: "failure", CreatedAt: at(1)}}, actionRerunFailed, 1},
		{"one cancelled", []workflowRun{{ID: 1, Status: "completed", Conclusion: "cancelled", CreatedAt: at(1)}}, actionRerunFailed, 1},
		{
			// The newest run decides, whatever order `gh run list` returns them in.
			"an older red, a newer green",
			[]workflowRun{
				{ID: 1, Status: "completed", Conclusion: "failure", CreatedAt: at(1)},
				{ID: 2, Status: "completed", Conclusion: "success", CreatedAt: at(5)},
			},
			actionDone, 2,
		},
		{
			"a newer red after an older green",
			[]workflowRun{
				{ID: 2, Status: "completed", Conclusion: "failure", CreatedAt: at(5)},
				{ID: 1, Status: "completed", Conclusion: "success", CreatedAt: at(1)},
			},
			actionRerunFailed, 2,
		},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			got, run := nextFinishAction(c.runs)
			if got != c.want || run.ID != c.wantID {
				t.Fatalf("got action %v on run %d, want %v on run %d", got, run.ID, c.want, c.wantID)
			}
		})
	}
}

func TestUpdateArchiveNames(t *testing.T) {
	got := updateArchiveNames("0.51.0")
	want := []string{"Cmdr_0.51.0_aarch64.app.tar.gz", "Cmdr_0.51.0_x64.app.tar.gz", "Cmdr_0.51.0_universal.app.tar.gz"}
	if !slices.Equal(got, want) {
		t.Fatalf("got %v, want %v", got, want)
	}
}

func TestMissingAssets(t *testing.T) {
	rel := releaseInfo{Assets: []releaseAsset{{Name: "Cmdr_0.51.0_aarch64.app.tar.gz"}, {Name: "Cmdr_0.51.0_aarch64.dmg"}}}
	got := rel.missing(updateArchiveNames("0.51.0"))
	want := []string{"Cmdr_0.51.0_x64.app.tar.gz", "Cmdr_0.51.0_universal.app.tar.gz"}
	if !slices.Equal(got, want) {
		t.Fatalf("got %v, want %v", got, want)
	}
}

func TestParseVersion(t *testing.T) {
	for _, ok := range []string{"0.51.0", "1.0.12"} {
		if err := checkVersion(ok); err != nil {
			t.Errorf("%s: %v", ok, err)
		}
	}
	for _, bad := range []string{"v0.51.0", "0.51", "0.51.0-beta", "", "0.51.0 "} {
		if err := checkVersion(bad); err == nil {
			t.Errorf("%q: want an error", bad)
		}
	}
}
