package checks

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestWorkflowTriggersReadsEventsAndPushPaths(t *testing.T) {
	yml := `name: X

on:
  push:
    branches: [main]
    # a comment between keys
    paths:
      - 'crates/**'
      - "Cargo.lock" # trailing comment
  workflow_dispatch:

permissions:
  contents: read
`
	events, paths := workflowTriggers(yml)
	if strings.Join(events, ",") != "push,workflow_dispatch" {
		t.Errorf("events = %v", events)
	}
	if strings.Join(paths, ",") != "crates/**,Cargo.lock" {
		t.Errorf("push paths = %v", paths)
	}
	if !isStandaloneWorkflow(yml) {
		t.Error("a workflow with push and dispatch triggers itself")
	}
}

func TestAReusableLaneIsNotStandalone(t *testing.T) {
	yml := "name: X\non:\n  workflow_call:\n    inputs:\n      paths:\n        type: string\n"
	if isStandaloneWorkflow(yml) {
		t.Error("a workflow_call-only lane file is called, not standalone")
	}
	if _, paths := workflowTriggers(yml); paths != nil {
		t.Errorf("an input named `paths` isn't a push filter, got %v", paths)
	}
	if !isStandaloneWorkflow("name: X\non: [push, workflow_dispatch]\n") {
		t.Error("the inline `on:` list form triggers itself too")
	}
	if isStandaloneWorkflow("name: X\non:\n  workflow_call:\n  workflow_dispatch:\n") {
		t.Error("a lane that declares workflow_call is reusable, whatever else starts it")
	}
	if isStandaloneWorkflow("name: X\n") {
		t.Error("a workflow with no trigger starts nowhere, so it doesn't stand alone")
	}
}

// The real Windows lane passes, and dropping a path its checks read is caught with the
// reader named, the same contract a ci.yml caller's filter answers to.
func TestStandaloneLaneFlagsAMissingPushPath(t *testing.T) {
	root := repoRootForTest(t)
	raw, err := os.ReadFile(filepath.Join(root, ".github", "workflows", "windows-lane.yml"))
	if err != nil {
		t.Fatal(err)
	}
	sources := LoadRunnerSources(root)
	rel := ".github/workflows/windows-lane.yml"
	_, paths := workflowTriggers(string(raw))
	if got := standaloneLaneMisses(root, "windows-lane.yml", rel, paths, sources); len(got) != 0 {
		t.Fatalf("the real windows-lane.yml should pass, got:\n%s", strings.Join(got, "\n"))
	}

	var narrowed []string
	for _, p := range paths {
		if p != "clippy.toml" && p != ".github/actions/install-mise/**" {
			narrowed = append(narrowed, p)
		}
	}
	got := strings.Join(standaloneLaneMisses(root, "windows-lane.yml", rel, narrowed, sources), "\n")
	for _, want := range []string{
		"misses `clippy.toml` (read by desktop-rust-clippy)",
		"misses `.github/actions/install-mise/action.yml`",
	} {
		if !strings.Contains(got, want) {
			t.Errorf("want a violation containing %q, got:\n%s", want, got)
		}
	}
}
