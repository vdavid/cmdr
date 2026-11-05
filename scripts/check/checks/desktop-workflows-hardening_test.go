package checks

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func writeWorkflow(t *testing.T, dir, name, content string) {
	t.Helper()
	if err := os.MkdirAll(dir, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, name), []byte(content), 0o644); err != nil {
		t.Fatal(err)
	}
}

func TestScanWorkflowFile(t *testing.T) {
	tmp := t.TempDir()
	wfDir := filepath.Join(tmp, ".github", "workflows")

	cases := []struct {
		name    string
		content string
		want    []string // substrings expected in each violation
	}{
		{
			name: "tag-pinned action flagged",
			content: `name: x
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
`,
			want: []string{"tag/branch-pinned action: actions/checkout@v4"},
		},
		{
			name: "SHA-pinned action accepted",
			content: `name: x
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@de0fac2e4500dabe0009e67214ff5f5447ce83dd # v6
`,
			want: nil,
		},
		{
			name: "branch ref flagged (same severity as tag)",
			content: `name: x
jobs:
  build:
    steps:
      - uses: dtolnay/rust-toolchain@master
`,
			want: []string{"dtolnay/rust-toolchain@master"},
		},
		{
			name: "local action exempt",
			content: `name: x
jobs:
  build:
    steps:
      - uses: ./.github/actions/my-action
`,
			want: nil,
		},
		{
			name: "pull_request_target block trigger flagged",
			content: `name: x
on:
  pull_request_target:
    paths: ['**']
jobs: {}
`,
			want: []string{"pull_request_target trigger"},
		},
		{
			name: "pull_request_target inline trigger flagged",
			content: `name: x
on: [push, pull_request_target]
jobs: {}
`,
			want: []string{"pull_request_target trigger"},
		},
		{
			name: "pull_request (without _target) accepted",
			content: `name: x
on:
  pull_request:
    paths: ['**']
jobs: {}
`,
			want: nil,
		},
		{
			name: "workflow-scoped id-token: write flagged",
			content: `name: x
permissions:
  id-token: write
  contents: read
jobs: {}
`,
			want: []string{"workflow-scoped 'id-token: write'"},
		},
		{
			name: "job-scoped id-token: write accepted",
			content: `name: x
jobs:
  publish:
    permissions:
      id-token: write
      contents: read
    runs-on: ubuntu-latest
    steps:
      - run: echo hi
`,
			want: nil,
		},
		{
			name: "comments and blank lines ignored",
			content: `# header
name: x

# inline note
on: [push]

jobs:
  build:
    steps:
      # tag pin would be flagged below, this comment is not
      - uses: actions/checkout@de0fac2e4500dabe0009e67214ff5f5447ce83dd # v6
`,
			want: nil,
		},
		{
			name: "multiple violations in one file",
			content: `name: x
on:
  pull_request_target:
permissions:
  id-token: write
jobs:
  build:
    steps:
      - uses: actions/cache@v5
`,
			want: []string{
				"pull_request_target trigger",
				"workflow-scoped 'id-token: write'",
				"actions/cache@v5",
			},
		},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			path := filepath.Join(wfDir, "test.yml")
			writeWorkflow(t, wfDir, "test.yml", tc.content)
			defer os.Remove(path)

			got, err := scanWorkflowFile(path, tmp)
			if err != nil {
				t.Fatalf("scanWorkflowFile error: %v", err)
			}
			if len(got) != len(tc.want) {
				t.Errorf("got %d violations, want %d:\n  got: %v\n  want: %v", len(got), len(tc.want), got, tc.want)
				return
			}
			for i, w := range tc.want {
				if !strings.Contains(got[i], w) {
					t.Errorf("violation %d: got %q, want substring %q", i, got[i], w)
				}
			}
		})
	}
}

func TestCheckWorkflowPermissions(t *testing.T) {
	cases := []struct {
		name    string
		content string
		want    []string // substrings expected in each violation
	}{
		{
			name: "read-only block accepted, job-level write allowed",
			content: `name: x
permissions:
  contents: read
jobs:
  release:
    permissions:
      contents: write
`,
			want: nil,
		},
		{
			name:    "read-all accepted",
			content: "name: x\npermissions: read-all\njobs: {}\n",
			want:    nil,
		},
		{
			name:    "empty map accepted",
			content: "name: x\npermissions: {}\njobs: {}\n",
			want:    nil,
		},
		{
			name: "missing block flagged",
			content: `name: x
jobs:
  build:
    permissions:
      contents: read
`,
			want: []string{"no workflow-level 'permissions:'"},
		},
		{
			name:    "write-all flagged",
			content: "name: x\npermissions: write-all\njobs: {}\n",
			want:    []string{"workflow-scoped 'permissions: write-all'"},
		},
		{
			name: "workflow-scoped write flagged, id-token left to its own check",
			content: `name: x
permissions:
  contents: write # comment
  id-token: write
  actions: read
jobs: {}
`,
			want: []string{"workflow-scoped 'contents: write'"},
		},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			got := checkWorkflowPermissions(strings.Split(tc.content, "\n"), "test.yml")
			if len(got) != len(tc.want) {
				t.Errorf("got %d violations, want %d:\n  got: %v\n  want: %v", len(got), len(tc.want), got, tc.want)
				return
			}
			for i, w := range tc.want {
				if !strings.Contains(got[i], w) {
					t.Errorf("violation %d: got %q, want substring %q", i, got[i], w)
				}
			}
		})
	}
}

func TestCheckCodeQLRunsOnPullRequests(t *testing.T) {
	const analyze = `jobs:
  analyze:
    steps:
      - uses: github/codeql-action/analyze@24c54180a607b1449ed407dd24f251e4e9147c8d # v4
`
	cases := []struct {
		name    string
		content string
		want    []string // substrings expected in each violation
	}{
		{
			name:    "push-only CodeQL flagged",
			content: "name: x\non:\n  push:\n    branches:\n      - main\n  schedule:\n    - cron: '1 2 * * 3'\n" + analyze,
			want:    []string{"runs CodeQL but not on 'pull_request'"},
		},
		{
			name:    "block-form pull_request accepted",
			content: "name: x\non:\n  push:\n  pull_request:\n    branches:\n      - main\n" + analyze,
			want:    nil,
		},
		{
			name:    "inline pull_request accepted",
			content: "name: x\non: [push, pull_request]\n" + analyze,
			want:    nil,
		},
		{
			name:    "pull_request_target doesn't count",
			content: "name: x\non: [push, pull_request_target]\n" + analyze,
			want:    []string{"runs CodeQL but not on 'pull_request'"},
		},
		{
			name:    "a nested pull_request key doesn't count",
			content: "name: x\non:\n  workflow_run:\n    types:\n      pull_request:\n" + analyze,
			want:    []string{"runs CodeQL but not on 'pull_request'"},
		},
		{
			name:    "workflow without CodeQL analysis ignored",
			content: "name: x\non:\n  push:\njobs:\n  up:\n    steps:\n      - uses: github/codeql-action/upload-sarif@24c54180a607b1449ed407dd24f251e4e9147c8d\n",
			want:    nil,
		},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			got := checkCodeQLRunsOnPullRequests(strings.Split(tc.content, "\n"), "test.yml")
			if len(got) != len(tc.want) {
				t.Errorf("got %d violations, want %d:\n  got: %v\n  want: %v", len(got), len(tc.want), got, tc.want)
				return
			}
			for i, w := range tc.want {
				if !strings.Contains(got[i], w) {
					t.Errorf("violation %d: got %q, want substring %q", i, got[i], w)
				}
			}
		})
	}
}

func TestCheckWorkflowRunGuard(t *testing.T) {
	const on = "name: x\non:\n  workflow_run:\n    workflows: [CI]\n    types: [completed]\n"
	const guard = "github.event.workflow_run.event == 'push' && github.event.workflow_run.head_branch == 'main' && " +
		"github.event.workflow_run.head_repository.full_name == github.repository"
	cases := []struct {
		name    string
		content string
		want    []string // substrings expected in each violation
	}{
		{
			name:    "every job guarded, inline and folded",
			content: on + "jobs:\n  a:\n    if: " + guard + "\n  b:\n    needs: a\n    if: >-\n      always() && " + guard + "\n",
			want:    nil,
		},
		{
			name:    "a job without an if is flagged",
			content: on + "jobs:\n  a:\n    if: " + guard + "\n  b:\n    runs-on: ubuntu-latest\n",
			want:    []string{"job `b`"},
		},
		{
			name:    "a guard without the event check is flagged",
			content: on + "jobs:\n  a:\n    if: github.event.workflow_run.head_branch == 'main'\n",
			want:    []string{"job `a`"},
		},
		{
			name:    "a workflow without workflow_run is ignored",
			content: "name: x\non:\n  push:\njobs:\n  a:\n    runs-on: ubuntu-latest\n",
			want:    nil,
		},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			got := checkWorkflowRunGuard(tc.content, "test.yml")
			if len(got) != len(tc.want) {
				t.Errorf("got %d violations, want %d:\n  got: %v\n  want: %v", len(got), len(tc.want), got, tc.want)
				return
			}
			for i, w := range tc.want {
				if !strings.Contains(got[i], w) {
					t.Errorf("violation %d: got %q, want substring %q", i, got[i], w)
				}
			}
		})
	}
}

func TestIsExemptUsesRef(t *testing.T) {
	cases := []struct {
		ref  string
		want bool
	}{
		{"./.github/actions/foo", true},
		{"../shared/bar", true},
		{"actions/checkout", false},
		{"github/codeql-action/init", false},
	}
	for _, tc := range cases {
		t.Run(tc.ref, func(t *testing.T) {
			if got := isExemptUsesRef(tc.ref); got != tc.want {
				t.Errorf("isExemptUsesRef(%q) = %v, want %v", tc.ref, got, tc.want)
			}
		})
	}
}
