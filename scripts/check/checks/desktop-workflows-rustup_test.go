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
			got, err := scanWorkflowForRustup(filepath.Join(wfDir, "test.yml"), tmp)
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
