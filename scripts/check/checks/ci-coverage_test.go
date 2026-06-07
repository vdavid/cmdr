package checks

import (
	"os"
	"path/filepath"
	"reflect"
	"testing"
)

func TestExtractWorkflowCheckNames(t *testing.T) {
	cases := []struct {
		name    string
		content string
		want    []string
	}{
		{
			name: "plain step invocation",
			content: `      - name: Run clippy
        run: ./scripts/check/check --check desktop-rust-clippy --ci
`,
			want: []string{"desktop-rust-clippy"},
		},
		{
			name:    "comma-separated names",
			content: `        run: ./scripts/check/check --check rustfmt,clippy --ci`,
			want:    []string{"rustfmt", "clippy"},
		},
		{
			name:    "repeated flag on one line",
			content: `        run: go run ./scripts/check --check rustfmt --check clippy`,
			want:    []string{"rustfmt", "clippy"},
		},
		{
			name:    "equals form",
			content: `        run: ./scripts/check/check --check=oxfmt`,
			want:    []string{"oxfmt"},
		},
		{
			name: "comment mentioning --check without an invocation is ignored",
			content: `      # To rerun: --check desktop-rust-clippy
      - run: echo hello
`,
			want: nil,
		},
		{
			name:    "comment mentioning scripts/check AND --check counts (conservative)",
			content: `      # ./scripts/check/check --check clippy`,
			want:    []string{"clippy"},
		},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			got := extractWorkflowCheckNames(tc.content)
			if !reflect.DeepEqual(got, tc.want) {
				t.Errorf("got %v, want %v", got, tc.want)
			}
		})
	}
}

func TestExtractCIFilterPaths(t *testing.T) {
	ciYml := `jobs:
  changes:
    steps:
      - name: Detect file changes
        uses: dorny/paths-filter@abc
        id: filter
        with:
          filters: |
            rust:
              - 'apps/desktop/src-tauri/**'
              - 'Cargo.toml'
            svelte:
              - 'apps/desktop/src/**'

  other-job:
    steps:
      - run: echo "- 'not/a/filter'"
`
	want := []string{"apps/desktop/src-tauri/**", "Cargo.toml", "apps/desktop/src/**"}
	got := extractCIFilterPaths(ciYml)
	if !reflect.DeepEqual(got, want) {
		t.Errorf("got %v, want %v", got, want)
	}
}

func TestExtractCIFilterPathsEndsAtDedent(t *testing.T) {
	ciYml := `        with:
          filters: |
            rust:
              - 'Cargo.toml'
      - name: next step
        run: ./foo --bar 'apps/nonexistent/**'
`
	want := []string{"Cargo.toml"}
	got := extractCIFilterPaths(ciYml)
	if !reflect.DeepEqual(got, want) {
		t.Errorf("got %v, want %v", got, want)
	}
}

func TestStaticPathPrefix(t *testing.T) {
	cases := []struct {
		pattern string
		want    string
	}{
		{"apps/desktop/src/**", "apps/desktop/src"},
		{"Cargo.toml", "Cargo.toml"},
		{"**/package.json", ""},
		{"apps/*/static/**", "apps"},
		{".github/workflows/ci.yml", ".github/workflows/ci.yml"},
		{"crates/**", "crates"},
	}
	for _, tc := range cases {
		if got := staticPathPrefix(tc.pattern); got != tc.want {
			t.Errorf("staticPathPrefix(%q) = %q, want %q", tc.pattern, got, tc.want)
		}
	}
}

// TestRegistryCIContract runs the real check against the real repo. This is
// the test-time mirror of the CI hygiene step: it fails the Go test suite
// locally if someone adds a check without wiring it into a workflow, renames
// a check a workflow still invokes, or leaves a dangling filter path in
// ci.yml. Pre-fix, this would have flagged 12 never-run checks, the stale
// `desktop-svelte-eslint-typecheck` invocation, and the renamed
// `vite.config.ts` filter entry.
func TestRegistryCIContract(t *testing.T) {
	// Tests run with the package dir as cwd; the repo root is three levels up.
	root, err := filepath.Abs(filepath.Join("..", "..", ".."))
	if err != nil {
		t.Fatalf("failed to resolve repo root: %v", err)
	}
	if _, err := os.Stat(filepath.Join(root, ".github", "workflows", "ci.yml")); err != nil {
		t.Skipf("repo layout not found from %s: %v", root, err)
	}
	result, err := RunCICoverage(&CheckContext{RootDir: root})
	if err != nil {
		t.Fatalf("registry/CI contract broken:\n%v", err)
	}
	if result.Code == ResultSkipped {
		t.Fatalf("expected the check to run, got skipped: %s", result.Message)
	}
}
