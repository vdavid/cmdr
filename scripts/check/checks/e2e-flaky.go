package checks

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
)

// Playwright flaky-pass reporting.
//
// `playwright.config.ts` sets `retries: process.env.CI ? 1 : 0`, and the Linux lane
// runs that exact config with `CI=true`. A spec rescued by its retry is reported by
// Playwright as PASSED (the process exits 0 and it lands in `stats.expected`), so
// without this the check reports a clean green on the one lane that actually has
// retries: the same silent-retry hole closed on the Rust side in
// `rust-test-diagnostics.go`.
//
// The retry carve-out is only defensible while retry-passes stay visible
// (`docs/testing.md`), so a flaky run is downgraded from pass to WARN and every
// rescued spec is named.
//
// Source is the structured JSON report the suite already writes
// (`CMDR_E2E_JSON_REPORT`), not stdout: `stats.flaky` and a per-test `status` of
// expected/unexpected/flaky/skipped are stable reporter fields, whereas the `list`
// reporter's tally is presentation text.

// parsePlaywrightFlaky reads one Playwright JSON report and returns the keys of every
// spec that needed a retry to pass. Key format matches the duration allowlist's
// (`<file>::<describe chain>::<title>`), so the two read alike.
func parsePlaywrightFlaky(path string) ([]string, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, fmt.Errorf("read %s: %w", path, err)
	}
	var report e2eJSONReport
	if err := json.Unmarshal(data, &report); err != nil {
		return nil, fmt.Errorf("parse %s: %w", path, err)
	}

	seen := map[string]bool{}
	for _, s := range report.Suites {
		collectSuiteFlaky(s, s.File, nil, seen)
	}
	return sortedKeys(seen), nil
}

// collectSuiteFlaky mirrors collectSuiteDurations' walk of Playwright's nested suite
// tree, accumulating the describe chain. Only `flaky` counts: `unexpected` is a real
// failure the check already raises, and reporting it here would double-count it.
func collectSuiteFlaky(s e2eJSONSuite, file string, describe []string, seen map[string]bool) {
	scope := describe
	if s.Title != "" && s.Title != filepath.Base(file) && s.Title != file {
		scope = append(append([]string{}, describe...), s.Title)
	}
	for _, sp := range s.Specs {
		for _, t := range sp.Tests {
			if t.Status == "flaky" {
				seen[file+"::"+strings.Join(scope, " › ")+"::"+sp.Title] = true
			}
		}
	}
	for _, child := range s.Suites {
		collectSuiteFlaky(child, file, scope, seen)
	}
}

// applyE2EFlakyWarning layers the flaky verdict onto a result. A flake downgrades a
// pass to a warn and names every rescued spec; an unreadable report is disclosed
// rather than treated as clean, since "no flakes found" and "couldn't look" must never
// read the same.
func applyE2EFlakyWarning(result CheckResult, reportPaths []string) CheckResult {
	seen := map[string]bool{}
	for _, path := range reportPaths {
		flaky, err := parsePlaywrightFlaky(path)
		if err != nil {
			result.Message += fmt.Sprintf("; flaky check skipped (%v)", err)
			return result
		}
		for _, key := range flaky {
			seen[key] = true
		}
	}

	keys := sortedKeys(seen)
	if len(keys) == 0 {
		return result
	}

	result.Code = ResultWarning
	result.Issues = len(keys)
	result.Message += fmt.Sprintf("\n%d %s passed only on retry (a retry-pass is a flake, not a pass; see docs/testing.md): %s",
		len(keys), Pluralize(len(keys), "spec", "specs"),
		strings.Join(formatE2ETestKeys(keys), ", "))
	return result
}
