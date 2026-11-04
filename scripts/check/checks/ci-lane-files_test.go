package checks

import (
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
)

// A trimmed ci.yml in the real layout: a shared anchor, two lanes on their own filters, one
// lane reading a second filter through `with:`, and a filter that gates a single check.
const ciLaneFilesFixture = `name: CI

jobs:
  changes:
    outputs:
      verified: ${{ steps.verified.outputs.verified }}
      rust: ${{ steps.verified.outputs.verified != 'true' && steps.filter.outputs.rust || 'false' }}
      extra:
        ${{ steps.verified.outputs.verified != 'true' && steps.filter.outputs.extra || 'false' }}
      docs: ${{ steps.verified.outputs.verified != 'true' && steps.filter.outputs.docs || 'false' }}
      rust-changed: ${{ steps.filter.outputs.rust }}
    steps:
      - name: Detect file changes
        uses: dorny/paths-filter@ceb8a2b8f2d89434be7ff52d3de7ec3738c5cc9d # v4.0.3
        id: filter
        with:
          filters: |
            # Shared.
            shared: &shared
              - 'crates/**'
              - '.mise.toml'
              - '.github/workflows/ci.yml'
              - '!**/CLAUDE.md'
            rust:
              - *shared
              - '.github/workflows/rust-lane.yml'
              - '.github/actions/tool/**'
              - '.github/actions/nested/**'
            extra:
              - 'extra/**'
              - '.mise.toml'
              - '.github/workflows/ci.yml'
              - '.github/workflows/rust-lane.yml'
              - '.github/actions/tool/**'
              - '.github/actions/nested/**'
            docs:
              - 'docs/**'
              - '.mise.toml'
              - '.github/workflows/ci.yml'
              - '.github/workflows/docs-lane.yml'

  rust:
    name: Rust
    needs: changes
    if: needs.changes.outputs.rust == 'true'
    uses: ./.github/workflows/rust-lane.yml
    with:
      extra: ${{ needs.changes.outputs.extra == 'true' }}

  docs:
    name: Docs
    needs: changes
    if: needs.changes.outputs.verified != 'true'
    uses: ./.github/workflows/docs-lane.yml
    with:
      docs: ${{ needs.changes.outputs.docs == 'true' }}
`

// writeLaneFixtureRepo lays out the lane files and actions the fixture names.
func writeLaneFixtureRepo(t *testing.T) string {
	t.Helper()
	root := t.TempDir()
	files := map[string]string{
		".github/workflows/rust-lane.yml":   "jobs:\n  lane:\n    steps:\n      - uses: ./.github/actions/tool\n",
		".github/workflows/docs-lane.yml":   "jobs:\n  lane:\n    steps:\n      - run: echo docs\n",
		".github/actions/tool/action.yml":   "runs:\n  steps:\n    - uses: ./.github/actions/nested\n",
		".github/actions/nested/action.yml": "runs:\n  steps:\n    # uses: ./.github/actions/commented-out\n    - run: echo\n",
		".github/workflows/ci.yml":          ciLaneFilesFixture,
		".github/workflows/unrelated.yml":   "name: other\n",
	}
	for rel, content := range files {
		path := filepath.Join(root, rel)
		if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(path, []byte(content), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	return root
}

func TestParseCIFiltersExpandsAnchors(t *testing.T) {
	filters := parseCIFilters(ciLaneFilesFixture)
	want := []string{
		"crates/**", ".mise.toml", ".github/workflows/ci.yml", "!**/CLAUDE.md",
		".github/workflows/rust-lane.yml", ".github/actions/tool/**", ".github/actions/nested/**",
	}
	if !reflect.DeepEqual(filters["rust"], want) {
		t.Errorf("rust = %v, want %v", filters["rust"], want)
	}
	if len(filters) != 4 {
		t.Errorf("want 4 filters (shared, rust, extra, docs), got %v", filters)
	}
}

func TestLaneFiltersAcceptTheFixture(t *testing.T) {
	root := writeLaneFixtureRepo(t)
	if got := validateLaneFilters(root, ciLaneFilesFixture); len(got) != 0 {
		t.Errorf("want no violations, got:\n%s", strings.Join(got, "\n"))
	}
}

// The regression anchor: `macos-lane.yml` sat only in a filter that gated one of its steps, so
// an edit to the lane never started it.
func TestLaneFiltersFlagAMissingLaneFile(t *testing.T) {
	root := writeLaneFixtureRepo(t)
	yml := strings.Replace(ciLaneFilesFixture, "              - '.github/workflows/rust-lane.yml'\n              - '.github/actions/tool/**'\n              - '.github/actions/nested/**'\n            extra:", "              - '.github/actions/tool/**'\n              - '.github/actions/nested/**'\n            extra:", 1)
	got := validateLaneFilters(root, yml)
	if len(got) != 1 || !strings.Contains(got[0], "`rust` filter") || !strings.Contains(got[0], "rust-lane.yml") {
		t.Errorf("want one violation naming the rust filter and its lane file, got:\n%s", strings.Join(got, "\n"))
	}
}

// A filter read only through `with:` must list the lane's files too, and so must every
// composite action the lane reaches, nested ones included.
func TestLaneFiltersFlagAMissingNestedActionInAWithFilter(t *testing.T) {
	root := writeLaneFixtureRepo(t)
	yml := strings.Replace(ciLaneFilesFixture, "              - '.github/actions/nested/**'\n            docs:", "            docs:", 1)
	got := validateLaneFilters(root, yml)
	if len(got) != 1 || !strings.Contains(got[0], "`extra` filter") || !strings.Contains(got[0], "nested/action.yml") {
		t.Errorf("want one violation naming the extra filter and the nested action, got:\n%s", strings.Join(got, "\n"))
	}
}

func TestLaneFiltersFlagAnotherLanesFile(t *testing.T) {
	root := writeLaneFixtureRepo(t)
	yml := strings.Replace(ciLaneFilesFixture, "              - 'docs/**'\n", "              - 'docs/**'\n              - '.github/workflows/rust-lane.yml'\n", 1)
	got := validateLaneFilters(root, yml)
	if len(got) != 1 || !strings.Contains(got[0], "`docs` filter lists `.github/workflows/rust-lane.yml`") {
		t.Errorf("want one violation about docs listing the rust lane file, got:\n%s", strings.Join(got, "\n"))
	}
}

func TestLaneFiltersFlagAnUncalledLaneFile(t *testing.T) {
	root := writeLaneFixtureRepo(t)
	if err := os.WriteFile(filepath.Join(root, ".github/workflows/orphan-lane.yml"), []byte("name: x\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	got := validateLaneFilters(root, ciLaneFilesFixture)
	if len(got) != 1 || !strings.Contains(got[0], "orphan-lane.yml") {
		t.Errorf("want one violation naming the orphan lane, got:\n%s", strings.Join(got, "\n"))
	}
}

// The gated-check rule runs against the real repo, the only place its registry and runner
// sources exist; the real ci.yml must pass, and dropping one input must fail by name.
func TestGatedCheckFiltersCoverTheirChecks(t *testing.T) {
	root := repoRootForTest(t)
	content, err := os.ReadFile(filepath.Join(root, ".github", "workflows", "ci.yml"))
	if err != nil {
		t.Fatal(err)
	}
	ciYml := string(content)
	if got := validateGatedCheckFilters(root, ciYml); len(got) != 0 {
		t.Fatalf("want the real ci.yml to pass, got:\n%s", strings.Join(got, "\n"))
	}
	broken := strings.Replace(ciYml, "              - 'scripts/check/checks/desktop-i18n-termbase.go'\n", "", 1)
	if broken == ciYml {
		t.Fatal("the termbase filter no longer lists desktop-i18n-termbase.go; update this test")
	}
	got := validateGatedCheckFilters(root, broken)
	if len(got) != 1 || !strings.Contains(got[0], "desktop-i18n-termbase.go") {
		t.Errorf("want one violation naming the dropped input, got:\n%s", strings.Join(got, "\n"))
	}
}

func TestFilterCoversInput(t *testing.T) {
	filter := []string{"docs/i18n/**", "scripts/check/*.go", "CHANGELOG.md", "!**/CLAUDE.md"}
	cases := map[string]bool{
		"docs/i18n/**":            true, // listed as is
		"docs/i18n/en/terms.json": true, // inside a listed dir
		"docs/i18n/en/**":         true, // a glob inside a listed dir
		"scripts/check/main.go":   true, // a concrete path the glob matches
		"CHANGELOG.md":            true,
		"scripts/check/**":        false, // wider than what's listed
		"docs/CLAUDE.md":          false,
		"apps/desktop/*.ts":       false,
	}
	for input, want := range cases {
		if got := filterCoversInput(filter, input); got != want {
			t.Errorf("filterCoversInput(%q) = %v, want %v", input, got, want)
		}
	}
}

// A long output the formatter folds onto the next line still reaches the verified-gate rule.
func TestJobOutputsJoinsFoldedValues(t *testing.T) {
	body := []string{
		"    outputs:",
		"      short: ${{ steps.a.outputs.x }}",
		"      # a comment",
		"      long:",
		"        ${{ steps.verified.outputs.verified != 'true' && steps.filter.outputs.long",
		"        || 'false' }}",
		"    steps:",
	}
	got := jobOutputs(body)
	want := map[string]string{
		"short": "${{ steps.a.outputs.x }}",
		"long":  "${{ steps.verified.outputs.verified != 'true' && steps.filter.outputs.long || 'false' }}",
	}
	if !reflect.DeepEqual(got, want) {
		t.Errorf("got %v, want %v", got, want)
	}
}
