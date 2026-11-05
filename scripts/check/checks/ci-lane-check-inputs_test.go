package checks

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// Rule 8 on the real tree is in TestRegistryCIContract; these prove it bites.
func TestLaneCheckInputsFlagAMissingCheckInput(t *testing.T) {
	root := repoRootForTest(t)
	raw, err := os.ReadFile(filepath.Join(root, ".github", "workflows", "ci.yml"))
	if err != nil {
		t.Fatal(err)
	}
	yml := string(raw)
	if got := validateLaneCheckInputs(root, yml); len(got) != 0 {
		t.Fatalf("the real ci.yml should pass, got:\n%s", strings.Join(got, "\n"))
	}
	broken := strings.Replace(yml, "              - 'clippy.toml'\n", "", 1)
	if broken == yml {
		t.Fatal("ci.yml no longer lists clippy.toml; update this test")
	}
	got := strings.Join(validateLaneCheckInputs(root, broken), "\n")
	for _, want := range []string{"`desktop-rust` runs `rust-lane.yml`", "`desktop-rust-macos` runs `macos-lane.yml`", "`clippy.toml` (read by desktop-rust-clippy)"} {
		if !strings.Contains(got, want) {
			t.Errorf("want a violation containing %q, got:\n%s", want, got)
		}
	}
}

func TestLaneCheckInputsFlagAMissingLaneStepInput(t *testing.T) {
	root := repoRootForTest(t)
	raw, err := os.ReadFile(filepath.Join(root, ".github", "workflows", "ci.yml"))
	if err != nil {
		t.Fatal(err)
	}
	yml := string(raw)
	// The `website` filter's only listing of the workspace file `pnpm install` reads.
	broken := strings.Replace(yml, "              - 'eslint-plugins/**'\n              - 'pnpm-workspace.yaml'\n", "              - 'eslint-plugins/**'\n", 1)
	if broken == yml {
		t.Fatal("the website filter's pnpm-workspace.yaml line moved; update this test")
	}
	got := strings.Join(validateLaneCheckInputs(root, broken), "\n")
	if !strings.Contains(got, "`website` runs `website-lane.yml`, whose filters miss `pnpm-workspace.yaml` (read by the lane's own steps)") {
		t.Errorf("want the website lane flagged for pnpm-workspace.yaml, got:\n%s", got)
	}
}
