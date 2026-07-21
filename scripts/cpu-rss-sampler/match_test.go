package main

import "testing"

// The snapshot the real Mac produced while writing this tool: the one true app
// process is `/Applications/Cmdr.app/Contents/MacOS/Cmdr`, and four node
// processes carry the repo path `…/vdavid/cmdr/…` in their arguments. The old
// `pgrep -f cmdr` matched the four node processes (repo path) and MISSED the app
// (case-sensitive `cmdr` never matches `Cmdr`), then reported a node process's
// RSS as if it were the app's. These fixtures pin that this can't happen again.
func realWorldSnapshot() []Process {
	return []Process{
		{PID: 41049, RSSKiB: 47728, CPUPct: 1.2, Exe: "/Users/x/.local/share/mise/installs/node/lts/bin/node", Command: "node /Users/x/projects-git/vdavid/cmdr/apps/desktop/node_modules/oxlint/bin/oxlint --lsp"},
		{PID: 74022, RSSKiB: 20960, CPUPct: 0.1, Exe: "/Users/x/.local/share/mise/installs/node/24.18.0/bin/node", Command: "node /Users/x/projects-git/vdavid/cmdr/node_modules/.pnpm/tinypool@2.1.0/node_modules/tinypool/dist/entry/process.js"},
		{PID: 95430, RSSKiB: 166720, CPUPct: 3.4, Exe: "/Users/x/.local/share/mise/installs/node/22.22.3/bin/node", Command: "node .../codegraph.js serve --mcp --path /Users/x/projects-git/vdavid/cmdr"},
		{PID: 77902, RSSKiB: 1454288, CPUPct: 42.7, Exe: "/Applications/Cmdr.app/Contents/MacOS/Cmdr", Command: "/Applications/Cmdr.app/Contents/MacOS/Cmdr"},
	}
}

func TestSelectTargetPicksTheAppNotRepoPathTooling(t *testing.T) {
	got, cands, err := selectTarget(realWorldSnapshot(), "Cmdr", "", 0)
	if err != nil {
		t.Fatalf("unexpected error: %v (candidates: %v)", err, cands)
	}
	if got.PID != 77902 {
		t.Fatalf("picked PID %d (%s), want the app at 77902", got.PID, got.Exe)
	}
	if len(cands) != 1 {
		t.Fatalf("want exactly one candidate, got %d: %v", len(cands), cands)
	}
}

// A bare `cargo`/`tauri dev` build's binary is lowercase `cmdr`, not the bundled
// `Cmdr`. Matching is case-insensitive on the executable's basename so a dev
// build is watched just like a prod one.
func TestSelectTargetMatchesBareDevBinaryCaseInsensitively(t *testing.T) {
	procs := []Process{
		{PID: 200, Exe: "/Users/x/.local/share/mise/installs/node/lts/bin/node", Command: "node /Users/x/projects-git/vdavid/cmdr/x"},
		{PID: 201, Exe: "/Users/x/projects-git/vdavid/cmdr/apps/desktop/src-tauri/target/debug/cmdr", Command: "target/debug/cmdr"},
	}
	got, _, err := selectTarget(procs, "Cmdr", "", 0)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if got.PID != 201 {
		t.Fatalf("picked PID %d, want the dev binary at 201", got.PID)
	}
}

// The whole point of the rewrite: only argv[0] is matched, never the argument
// string. A process is NOT the app just because the repo path is in its args.
func TestSelectTargetIgnoresRepoPathInArguments(t *testing.T) {
	procs := []Process{
		{PID: 300, Exe: "/opt/homebrew/bin/rg", Command: "rg cmdr /Users/x/projects-git/vdavid/cmdr"},
		{PID: 301, Exe: "/Users/x/.local/share/mise/installs/node/lts/bin/node", Command: "node /Users/x/projects-git/vdavid/cmdr/apps/desktop/node_modules/oxlint/bin/oxlint --lsp"},
	}
	_, _, err := selectTarget(procs, "Cmdr", "", 0)
	if err == nil {
		t.Fatal("expected no-match error, but something was selected")
	}
}

// Two live instances (say prod plus an E2E build, or a dev build plus prod) must
// fail loudly, not silently pick one. Benchmark numbers pinned to the wrong
// instance are worse than an error.
func TestSelectTargetFailsLoudlyOnMultipleCandidates(t *testing.T) {
	procs := []Process{
		{PID: 400, Exe: "/Applications/Cmdr.app/Contents/MacOS/Cmdr", Command: "/Applications/Cmdr.app/Contents/MacOS/Cmdr"},
		{PID: 401, Exe: "/Users/x/projects-git/vdavid/cmdr/apps/desktop/src-tauri/target/release/cmdr", Command: "target/release/cmdr"},
	}
	_, cands, err := selectTarget(procs, "Cmdr", "", 0)
	if err == nil {
		t.Fatal("expected an ambiguity error for two live instances")
	}
	if len(cands) != 2 {
		t.Fatalf("the error should carry both candidates, got %d", len(cands))
	}
}

// `-path-contains` disambiguates two instances down to one deliberately.
func TestSelectTargetPathContainsNarrowsToOne(t *testing.T) {
	procs := []Process{
		{PID: 500, Exe: "/Applications/Cmdr.app/Contents/MacOS/Cmdr", Command: "/Applications/Cmdr.app/Contents/MacOS/Cmdr"},
		{PID: 501, Exe: "/Users/x/projects-git/vdavid/cmdr/apps/desktop/src-tauri/target/release/cmdr", Command: "target/release/cmdr"},
	}
	got, _, err := selectTarget(procs, "Cmdr", "/Applications/", 0)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if got.PID != 500 {
		t.Fatalf("picked PID %d, want the /Applications/ instance at 500", got.PID)
	}
}

// The sampler must never watch itself, even if some day it is renamed to
// something that basename-matches.
func TestSelectTargetExcludesSelf(t *testing.T) {
	procs := []Process{
		{PID: 600, Exe: "/tmp/go-build/cmdr", Command: "cpu-rss-sampler"},
		{PID: 601, Exe: "/Applications/Cmdr.app/Contents/MacOS/Cmdr", Command: "/Applications/Cmdr.app/Contents/MacOS/Cmdr"},
	}
	got, _, err := selectTarget(procs, "Cmdr", "", 600)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if got.PID != 601 {
		t.Fatalf("picked PID %d, want 601 (600 is self)", got.PID)
	}
}

func TestSelectTargetNoMatchIsAnError(t *testing.T) {
	procs := []Process{
		{PID: 700, Exe: "/usr/bin/ssh", Command: "ssh nas"},
	}
	if _, _, err := selectTarget(procs, "Cmdr", "", 0); err == nil {
		t.Fatal("expected a no-match error")
	}
}

func TestParsePSLine(t *testing.T) {
	// pid, rss (KiB), %cpu, then the full command; ps right-pads the numbers.
	line := "  77902 1454288  42.7 /Applications/Cmdr.app/Contents/MacOS/Cmdr"
	p, ok := parsePSLine(line)
	if !ok {
		t.Fatal("expected the line to parse")
	}
	if p.PID != 77902 || p.RSSKiB != 1454288 || p.CPUPct != 42.7 {
		t.Fatalf("parsed pid=%d rss=%d cpu=%v", p.PID, p.RSSKiB, p.CPUPct)
	}
	if p.Exe != "/Applications/Cmdr.app/Contents/MacOS/Cmdr" {
		t.Fatalf("exe = %q", p.Exe)
	}
}

func TestParsePSLineSkipsHeaderAndJunk(t *testing.T) {
	for _, line := range []string{"", "   ", "PID RSS %CPU COMMAND", "notanumber 1 2 /bin/x"} {
		if _, ok := parsePSLine(line); ok {
			t.Fatalf("line %q should not have parsed", line)
		}
	}
}
