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
