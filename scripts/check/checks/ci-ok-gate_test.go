package checks

import (
	"reflect"
	"strings"
	"testing"
)

const ciOKGateFixture = `name: CI

on:
  pull_request:
  push:
    branches:
      - main

jobs:
  changes:
    name: Detect changes
    runs-on: ubuntu-latest
    outputs:
      verified: ${{ steps.verified.outputs.verified }}
      lint: ${{ steps.verified.outputs.verified != 'true' && steps.filter.outputs.lint || 'false' }}
      lint-changed: ${{ steps.filter.outputs.lint }}

  lint:
    name: Lint
    needs: changes
    if: needs.changes.outputs.lint == 'true'

  deploy-website:
    needs: lint

  warm-e2e-linux-cache:
    needs: changes

  warm-rust-cache:
    needs: changes

  warm-rust-macos-cache:
    needs: changes

  prune-superseded-caches:
    needs: [warm-rust-cache, warm-rust-macos-cache]

  ci-ok:
    name: CI OK
    needs:
      [
        changes,
        lint, # trailing comment
      ]
    if: always()
    steps:
      - run: echo ok
`

func TestCIOKGateAcceptsAFullGate(t *testing.T) {
	if got := validateCIOKGate(ciOKGateFixture); len(got) != 0 {
		t.Errorf("want no violations, got:\n%s", strings.Join(got, "\n"))
	}
}

// The regression anchor: `changes` outside the gate's needs is how a failed change
// detection skipped every lane and still yielded a green `CI OK`.
func TestCIOKGateFlagsAJobOutsideItsNeeds(t *testing.T) {
	yml := strings.Replace(ciOKGateFixture, "        changes,\n", "", 1)
	got := validateCIOKGate(yml)
	if len(got) != 1 || !strings.Contains(got[0], "`changes`") {
		t.Errorf("want one violation naming `changes`, got:\n%s", strings.Join(got, "\n"))
	}
}

func TestCIOKGateFlagsASkippableGate(t *testing.T) {
	yml := strings.Replace(ciOKGateFixture, "    if: always()", "    if: ${{ !cancelled() }}", 1)
	got := validateCIOKGate(yml)
	if len(got) != 1 || !strings.Contains(got[0], "always()") {
		t.Errorf("want one violation about always(), got:\n%s", strings.Join(got, "\n"))
	}
}

func TestCIOKGateFlagsAPullRequestPathFilter(t *testing.T) {
	yml := strings.Replace(ciOKGateFixture, "  pull_request:\n", "  pull_request:\n    paths-ignore:\n      - 'docs/**'\n", 1)
	got := validateCIOKGate(yml)
	if len(got) != 1 || !strings.Contains(got[0], "paths-ignore") {
		t.Errorf("want one violation about paths-ignore, got:\n%s", strings.Join(got, "\n"))
	}
}

func TestCIOKGateFlagsAStaleExemption(t *testing.T) {
	yml := strings.Replace(ciOKGateFixture, "  deploy-website:\n    needs: lint\n", "", 1)
	got := validateCIOKGate(yml)
	if len(got) != 1 || !strings.Contains(got[0], "deploy-website") {
		t.Errorf("want one violation naming the stale exemption, got:\n%s", strings.Join(got, "\n"))
	}
}

// A landed SHA already carries a green `CI OK` from its PR, so the `main` push skips every
// lane. A lane that doesn't gate on `changes` would re-run the suite on every land.
func TestCIOKGateFlagsALaneThatIgnoresChangeDetection(t *testing.T) {
	yml := strings.Replace(ciOKGateFixture, "    if: needs.changes.outputs.lint == 'true'\n", "", 1)
	got := validateCIOKGate(yml)
	if len(got) != 1 || !strings.Contains(got[0], "`lint`") {
		t.Errorf("want one violation naming `lint`, got:\n%s", strings.Join(got, "\n"))
	}
}

func TestCIOKGateFlagsAFilterOutputNotGatedOnVerified(t *testing.T) {
	yml := strings.Replace(ciOKGateFixture,
		"lint: ${{ steps.verified.outputs.verified != 'true' && steps.filter.outputs.lint || 'false' }}",
		"lint: ${{ steps.filter.outputs.lint }}", 1)
	got := validateCIOKGate(yml)
	if len(got) != 1 || !strings.Contains(got[0], "`lint`") {
		t.Errorf("want one violation naming output `lint`, got:\n%s", strings.Join(got, "\n"))
	}
}

func TestJobNeedsForms(t *testing.T) {
	cases := []struct {
		name string
		body []string
		want []string
	}{
		{"scalar", []string{"    needs: changes", "    if: always()"}, []string{"changes"}},
		{"inline flow", []string{"    needs: [a, b-c]", "    steps:"}, []string{"a", "b-c"}},
		{"block sequence", []string{"    needs:", "      - a", "      - b", "    if: always()"}, []string{"a", "b"}},
		{"multi-line flow", []string{"    needs:", "      [", "        a,", "        b,", "      ]", "    if: always()"}, []string{"a", "b"}},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			if got := jobNeeds(tc.body); !reflect.DeepEqual(got, tc.want) {
				t.Errorf("got %v, want %v", got, tc.want)
			}
		})
	}
}
