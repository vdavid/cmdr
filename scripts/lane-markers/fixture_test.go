package main

import (
	"crypto/sha1"
	"encoding/hex"
	"fmt"
	"sort"
	"strings"
	"testing"
)

// memTree is an in-memory commit tree: object IDs are content hashes, like git's.
type memTree struct {
	files map[string]string
	modes map[string]string
}

func newMemTree(files map[string]string) *memTree {
	cp := map[string]string{}
	for k, v := range files {
		cp[k] = v
	}
	return &memTree{files: cp, modes: map[string]string{}}
}

func (m *memTree) entries() []entry {
	var out []entry
	for p, c := range m.files {
		sum := sha1.Sum([]byte(c))
		mode := m.modes[p]
		if mode == "" {
			mode = "100644"
		}
		out = append(out, entry{mode: mode, typ: "blob", object: hex.EncodeToString(sum[:]), path: p})
	}
	sort.Slice(out, func(i, j int) bool { return out[i].path < out[j].path })
	return out
}

func (m *memTree) read(p string) ([]byte, error) {
	c, ok := m.files[p]
	if !ok {
		return nil, fmt.Errorf("%s isn't tracked", p)
	}
	return []byte(c), nil
}

// fixtureCI is ci.yml in the shape the wiring takes: the filters in `changes`, an inline lane, a
// lane calling a reusable `<lane>-lane.yml`, and jobs that aren't lanes.
const fixtureCI = `name: CI
on: [pull_request]
jobs:
  changes:
    runs-on: ubuntu-latest
    outputs:
      rust: ${{ steps.filter.outputs.rust }}
      svelte: ${{ steps.filter.outputs.svelte }}
      finder-extension: ${{ steps.filter.outputs.finder-extension }}
      lanes: ${{ steps.lanes.outputs.lanes }}
    steps:
      - uses: actions/checkout@0000000000000000000000000000000000000000 # v7
      - name: Detect file changes
        uses: dorny/paths-filter@ceb8a2b8f2d89434be7ff52d3de7ec3738c5cc9d # v4.0.3
        id: filter
        with:
          predicate-quantifier: some-with-excludes
          filters: |
            rust:
              - 'apps/desktop/src-tauri/**'
              - 'crates/**'
              - 'Cargo.lock'
              - '.github/workflows/ci.yml'
              - '!**/CLAUDE.md'
              - '!**/DETAILS.md'
            finder-extension:
              - 'apps/desktop/finder-extension/**'
              - '.github/workflows/macos-lane.yml'
            svelte:
              - 'apps/desktop/src/**'
              - 'pnpm-lock.yaml'
              - '!**/CLAUDE.md'
  desktop-rust:
    needs: changes
    if: inputs.run_all || (needs.changes.outputs.rust == 'true' && !fromJSON(needs.changes.outputs.lanes)['desktop-rust'].verified)
    runs-on: ubuntu-latest
    steps:
      - uses: ./.github/actions/pinned-rust-toolchain
      - run: ./scripts/check/check --check desktop-rust-clippy --ci
      - run: echo "$GITHUB_RUN_ID" > "$RUNNER_TEMP/lane-green"
      - uses: actions/cache/save@0000000000000000000000000000000000000000 # v5
        with:
          path: ${{ runner.temp }}/lane-green
          key: ${{ fromJSON(needs.changes.outputs.lanes)['desktop-rust'].key }}
  desktop-rust-macos:
    needs: changes
    if: inputs.run_all || (needs.changes.outputs.rust == 'true' && !fromJSON(needs.changes.outputs.lanes)['desktop-rust-macos'].verified)
    uses: ./.github/workflows/macos-lane.yml
    with:
      compile-finder-extension: ${{ inputs.run_all || needs.changes.outputs.finder-extension == 'true' }}
      marker-key: ${{ fromJSON(needs.changes.outputs.lanes)['desktop-rust-macos'].key }}
  e2e-base-image:
    needs: changes
    if: inputs.run_all || (needs.changes.outputs.svelte == 'true' && !fromJSON(needs.changes.outputs.lanes)['desktop-e2e'].verified)
    runs-on: ubuntu-latest
    steps:
      - uses: ./.github/actions/base-image
  desktop-e2e:
    needs: [changes, e2e-base-image]
    if: inputs.run_all || (needs.changes.outputs.svelte == 'true' && !fromJSON(needs.changes.outputs.lanes)['desktop-e2e'].verified)
    runs-on: ubuntu-latest
    steps:
      - run: ./run-e2e.sh
      - uses: actions/cache/save@0000000000000000000000000000000000000000 # v5
        with:
          path: ${{ runner.temp }}/lane-green
          key: ${{ fromJSON(needs.changes.outputs.lanes)['desktop-e2e'].key }}
  warm-rust-cache:
    needs: changes
    if: needs.changes.outputs.rust != 'true'
    runs-on: ubuntu-latest
    steps:
      - uses: ./.github/actions/pinned-rust-toolchain
`

const fixtureMacOSLane = `name: macOS lane
on:
  workflow_call:
    inputs:
      compile-finder-extension:
        type: boolean
jobs:
  lane:
    runs-on: macos-26
    steps:
      - uses: ./.github/actions/pinned-rust-toolchain
      - uses: ./.github/actions/mac-only/
      - run: ./scripts/check/check --check desktop-rust-tests --ci
`

func fixtureFiles() map[string]string {
	return map[string]string{
		".github/workflows/ci.yml":         fixtureCI,
		".github/workflows/macos-lane.yml": fixtureMacOSLane,
		// Like codeql.yml: its own dorny gate, in a job that isn't `changes`, so it defines no lanes.
		".github/workflows/other.yml":                        "name: other\non: push\njobs:\n  x:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: dorny/paths-filter@x\n        with:\n          filters: |\n            rust:\n              - '**/*.rs'\n",
		".github/actions/pinned-rust-toolchain/action.yml":   "runs:\n  using: composite\n  steps:\n    - uses: ./.github/actions/shared-helper\n",
		".github/actions/shared-helper/action.yml":           "runs:\n  using: composite\n  steps:\n    - run: echo hi\n      shell: bash\n",
		".github/actions/shared-helper/script.sh":            "echo hi\n",
		".github/actions/mac-only/action.yml":                "runs:\n  using: composite\n  steps:\n    - run: echo mac\n      shell: bash\n",
		".github/actions/unused/action.yml":                  "runs:\n  using: composite\n  steps: []\n",
		".github/actions/base-image/action.yml":              "runs:\n  using: composite\n  steps:\n    - run: docker build .\n      shell: bash\n",
		"apps/desktop/src-tauri/src/main.rs":                 "fn main() {}\n",
		"apps/desktop/src-tauri/CLAUDE.md":                   "# Agent docs\n",
		"apps/desktop/src-tauri/DETAILS.md":                  "# Details\n",
		"apps/desktop/src-tauri/src/agent/prompts/system.md": "You are Cmdr.\n",
		"apps/desktop/finder-extension/Ext.swift":            "import Foundation\n",
		"apps/desktop/src/App.svelte":                        "<p>hi</p>\n",
		"crates/cmdr-fs/src/lib.rs":                          "pub fn f() {}\n",
		"Cargo.lock":                                         "# lock\n",
		".mise.toml":                                         "[tools]\ngo = \"1.27.3\"\n",
		"rust-toolchain.toml":                                "[toolchain]\nchannel = \"1.91.0\"\n",
		"scripts/check/main.go":                              "package main\n",
		"scripts/check/CLAUDE.md":                            "# Check runner\n",
		"docs/tooling/ci.md":                                 "# CI\n",
		"pnpm-lock.yaml":                                     "lockfileVersion: '9.0'\n",
	}
}

func mustConfig(t *testing.T, tr tree) *config {
	t.Helper()
	c, err := loadConfig(tr)
	if err != nil {
		t.Fatal(err)
	}
	return c
}

func laneByID(t *testing.T, c *config, id string) lane {
	t.Helper()
	for _, l := range c.lanes {
		if l.id == id {
			return l
		}
	}
	t.Fatalf("no lane %s among %v", id, c.lanes)
	return lane{}
}

func hashOf(t *testing.T, files map[string]string, laneID string) string {
	t.Helper()
	tr := newMemTree(files)
	c := mustConfig(t, tr)
	return laneHash(laneID, laneInputs(tr, c, laneByID(t, c, laneID)))
}

func inputPaths(t *testing.T, files map[string]string, laneID string) string {
	t.Helper()
	tr := newMemTree(files)
	c := mustConfig(t, tr)
	var ps []string
	for _, e := range laneInputs(tr, c, laneByID(t, c, laneID)) {
		ps = append(ps, e.path)
	}
	return strings.Join(ps, "\n")
}
