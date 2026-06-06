package checks

import (
	"path/filepath"
	"strings"
	"testing"
)

func TestScanWorkflowForRustup(t *testing.T) {
	tmp := t.TempDir()
	wfDir := filepath.Join(tmp, ".github", "workflows")

	cases := []struct {
		name    string
		content string
		want    []string // substrings expected in each violation
	}{
		{
			name: "rustup target add flagged",
			content: `name: x
jobs:
  build:
    steps:
      - run: rustup target add x86_64-apple-darwin
`,
			want: []string{"rustup target add x86_64-apple-darwin"},
		},
		{
			name: "rustup component add flagged",
			content: `name: x
jobs:
  build:
    steps:
      - run: rustup component add clippy
`,
			want: []string{"rustup component add clippy"},
		},
		{
			name: "rustup target add inside chained command flagged",
			content: `name: x
jobs:
  build:
    steps:
      - run: |
          rustup update stable && rustup target add x86_64-unknown-linux-gnu
`,
			want: []string{"rustup target add x86_64-unknown-linux-gnu"},
		},
		{
			name: "rustup install accepted (whole toolchains, not redundant with toml)",
			content: `name: x
jobs:
  build:
    steps:
      - run: rustup install 1.95.0
`,
			want: nil,
		},
		{
			name: "rustup update accepted",
			content: `name: x
jobs:
  build:
    steps:
      - run: rustup update stable
`,
			want: nil,
		},
		{
			name: "rustup toolchain install accepted",
			content: `name: x
jobs:
  build:
    steps:
      - run: rustup toolchain install nightly
`,
			want: nil,
		},
		{
			name: "rustup show accepted",
			content: `name: x
jobs:
  build:
    steps:
      - run: rustup show
`,
			want: nil,
		},
		{
			name: "opt-out with reason accepted",
			content: `name: x
jobs:
  build:
    steps:
      - run: rustup target add wasm32-unknown-unknown # allowed-rustup-add: wasm builds only happen here, separate from main toolchain
`,
			want: nil,
		},
		{
			name: "opt-out with empty reason still flagged",
			content: `name: x
jobs:
  build:
    steps:
      - run: rustup target add wasm32-unknown-unknown # allowed-rustup-add:
`,
			want: []string{"rustup target add wasm32-unknown-unknown"},
		},
		{
			name: "comment mentioning rustup target add (not an actual command) not flagged",
			content: `name: x
jobs:
  build:
    steps:
      # The previous rustup target add step was removed; see rust-toolchain.toml
      - run: echo ok
`,
			want: nil,
		},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			writeWorkflow(t, wfDir, "test.yml", tc.content)
			got, _, err := scanWorkflowForRustup(filepath.Join(wfDir, "test.yml"), tmp)
			if err != nil {
				t.Fatalf("unexpected error: %v", err)
			}
			if len(got) != len(tc.want) {
				t.Fatalf("got %d violations, want %d:\n  got: %v\n  want: %v",
					len(got), len(tc.want), got, tc.want)
			}
			for i, expected := range tc.want {
				if !strings.Contains(got[i], expected) {
					t.Errorf("violation %d does not contain %q:\n  %s", i, expected, got[i])
				}
			}
		})
	}
}

func TestScanWorkflowForRustup_FlagsOrphanedOptOut(t *testing.T) {
	tmp := t.TempDir()
	wfDir := filepath.Join(tmp, ".github", "workflows")
	writeWorkflow(t, wfDir, "test.yml", `name: x
jobs:
  build:
    steps:
      # allowed-rustup-add: floating directive that excuses nothing
      - run: echo ok
      - run: rustup update stable # allowed-rustup-add: stale, this line no longer adds a target
`)

	_, orphans, err := scanWorkflowForRustup(filepath.Join(wfDir, "test.yml"), tmp)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(orphans) != 2 {
		t.Fatalf("expected 2 orphaned directives (pure-comment line 5 + trailing line 7), got %d: %v", len(orphans), orphans)
	}
	if orphans[0].line != 5 || orphans[1].line != 7 {
		t.Errorf("expected orphans at lines 5 and 7, got %d and %d", orphans[0].line, orphans[1].line)
	}
}

func TestScanWorkflowForRustup_UsedOptOutIsNotOrphan(t *testing.T) {
	tmp := t.TempDir()
	wfDir := filepath.Join(tmp, ".github", "workflows")
	writeWorkflow(t, wfDir, "test.yml", `name: x
jobs:
  build:
    steps:
      - run: rustup target add wasm32-unknown-unknown # allowed-rustup-add: wasm builds only happen here
`)

	violations, orphans, err := scanWorkflowForRustup(filepath.Join(wfDir, "test.yml"), tmp)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(violations) != 0 {
		t.Errorf("expected no violations, got %v", violations)
	}
	if len(orphans) != 0 {
		t.Errorf("expected no orphans for a used opt-out, got %v", orphans)
	}
}
