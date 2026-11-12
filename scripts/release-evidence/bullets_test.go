package main

import (
	"strings"
	"testing"
)

// v0.55.0's facts render to the bullets its release log entry carries (minus the judgment the
// agent adds, like "hardening only").
func TestReleaseLogBullets(t *testing.T) {
	f := releaseFacts{
		repo: "vdavid/cmdr", version: "0.55.0", tag: "v0.55.0",
		taggedCommit: "6829d88ff5fae2692bfc684c2fe94d3a58389877",
		ci:           ciCandidate{commit: "8124e1f14129625cfc92b896de4295e249ed9093", describe: "the parent of the `chore(release)` commit"},
		ciRun:        workflowRun{ID: 39684718106, Conclusion: "success"},
		codeQLRun:    workflowRun{ID: 39682395589, Conclusion: "success"},
		releaseRuns: []workflowRun{
			{ID: 39692184518, Event: "push", Conclusion: "success"},
			{ID: 39698680663, Event: "workflow_dispatch", Conclusion: "success"},
		},
		lanes: []testLane{
			{job: "Desktop (Svelte)", name: "svelte-tests", status: "OK", summary: "17121 tests passed"},
			{job: "Desktop (Rust, fixtures)", name: "rust-integration-tests", status: "warn", summary: "1 test failed under load"},
		},
		hasSecurity:  true,
		archivePath:  "documents/Company AB/evidence/v0.55.0",
		archiveFiles: 15,
	}
	want := strings.Join([]string{
		"    - **CI**: full `ci.yml` run 39684718106 (success) on `8124e1f14129625cfc92b896de4295e249ed9093`, the parent of the `chore(release)` commit; `codeql.yml` run 39682395589 (success) on the same commit.",
		"    - **Release runs**: `release.yml` 39692184518 (tag push, success) and 39698680663 (dispatch, success); tagged commit `6829d88ff5fae2692bfc684c2fe94d3a58389877`.",
		"    - **SBOMs and provenance**: https://github.com/vdavid/cmdr/releases/tag/v0.55.0 (three CycloneDX SBOMs and `Cmdr_0.55.0.intoto.jsonl`); `gh attestation verify` on `Cmdr_0.55.0_aarch64.dmg` passed for provenance (signer `release-pipeline.yml` at `refs/tags/v0.55.0`) and the SBOM binding.",
		"    - **Test evidence**: `svelte-tests` (Desktop (Svelte)): 17,121 tests passed; `rust-integration-tests` (Desktop (Rust, fixtures), warn): 1 test failed under load. The old-macOS smoke test didn't run.",
		"    - **Security content**: the CHANGELOG has a `### Security` section; no advisory or CVE ids.",
		"    - **NAS copy**: `documents/Company AB/evidence/v0.55.0/` (15 files, listed back after upload).",
		"",
	}, "\n")
	if got := releaseLogBullets(f); got != want {
		t.Errorf("got:\n%s\nwant:\n%s", got, want)
	}
}

func TestWithThousands(t *testing.T) {
	for in, want := range map[string]string{
		"17121 tests passed":   "17,121 tests passed",
		"1234567 and 999 runs": "1,234,567 and 999 runs",
		"12 cells passed":      "12 cells passed",
	} {
		if got := withThousands(in); got != want {
			t.Errorf("withThousands(%q) = %q, want %q", in, got, want)
		}
	}
}
