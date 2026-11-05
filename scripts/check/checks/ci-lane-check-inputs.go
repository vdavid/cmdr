package checks

import (
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
)

// Rule 8 of `ci-coverage`: every path-filtered lane's filters cover the fingerprint of every
// check its lane file runs (`Inputs`, `GlobalInputs`, and the runner files its `Run` reaches),
// plus what the lane runs outside the check runner (`laneExtraInputs`).
//
// Why it matters twice over: a filter that misses a file a lane's check reads skips the lane
// when only that file changes, and lane markers hash a lane's FILTERS, so the miss also lets a
// follow-up push skip the lane on a marker that never saw the file (the CI gate review of
// 2026-11-05, F1: `clippy.toml`, `deny.toml`, `knip.json`, and more).
// The light lane is rule 7's (`ciFilterGatedChecks`): it gates checks section by section.

// laneExtraInputs are files a lane reads outside any `--check`: its own setup steps and
// scripts. Keyed by lane file.
var laneExtraInputs = map[string][]string{
	// `pnpm install --frozen-lockfile` before the checks.
	"svelte-lane.yml":  {"pnpm-lock.yaml", "pnpm-workspace.yaml", "package.json"},
	"website-lane.yml": {"pnpm-lock.yaml", "pnpm-workspace.yaml", "package.json"},
	// `e2e-linux.sh` builds the app in Docker from the lockfiles and the E2E harness.
	"e2e-linux-lane.yml": {"pnpm-lock.yaml", "pnpm-workspace.yaml", "package.json", "apps/desktop/test/e2e-linux/**"},
}

// laneCheckNarrowedInputs names fingerprint patterns a lane's filters deliberately don't
// carry, per lane file, each with its reason. Every entry is a place a marker can vouch for
// a tree it didn't see, so keep it short.
var laneCheckNarrowedInputs = map[string]map[string]string{}

// laneFileCheckIDs lists the registry checks a lane file runs (`--check` on any line that
// mentions `scripts/check`, the same parse as rule 1).
func laneFileCheckIDs(rootDir, lane string) ([]string, error) {
	content, err := os.ReadFile(filepath.Join(rootDir, ".github", "workflows", lane))
	if err != nil {
		return nil, err
	}
	return extractWorkflowCheckNames(string(content)), nil
}

func validateLaneCheckInputs(rootDir, ciYml string) []string {
	jobs := extractWorkflowJobs(ciYml)
	filters := parseCIFilters(ciYml)
	outputToFilter := map[string]string{}
	for k, v := range jobOutputs(jobs["changes"]) {
		if m := filterOutputRefRe.FindStringSubmatch(v); m != nil && !strings.HasSuffix(k, "-changed") {
			outputToFilter[k] = m[1]
		}
	}
	sources := LoadRunnerSources(rootDir)

	var ids []string
	for id := range jobs {
		ids = append(ids, id)
	}
	sort.Strings(ids)
	var violations []string
	for _, id := range ids {
		lane := jobLaneFile(jobs[id])
		if lane == "" || lane == "light-lane.yml" {
			continue
		}
		var patterns []string
		for _, line := range jobs[id] {
			for _, m := range changesOutputRefRe.FindAllStringSubmatch(line, -1) {
				if f, ok := outputToFilter[m[1]]; ok {
					patterns = append(patterns, filters[f]...)
				}
			}
		}
		missing, err := laneInputsMissingFrom(rootDir, lane, patterns, sources)
		if err != nil {
			violations = append(violations, fmt.Sprintf("couldn't read %s: %v", lane, err))
			continue
		}
		violations = append(violations, describeMissingLaneInputs(id, lane, missing)...)
	}
	return violations
}

// laneInputsMissingFrom maps each input a lane reads that its filter patterns don't cover to
// whoever reads it (its checks by ID, or the lane's own steps).
func laneInputsMissingFrom(rootDir, lane string, patterns []string, sources *RunnerSourceIndex) (map[string][]string, error) {
	checkIDs, err := laneFileCheckIDs(rootDir, lane)
	if err != nil {
		return nil, err
	}
	missing := map[string][]string{}
	need := func(in, reader string) {
		if strings.HasPrefix(in, "!") {
			return // an exclusion only narrows the set
		}
		if _, narrowed := laneCheckNarrowedInputs[lane][in]; narrowed {
			return
		}
		if !filterCoversInput(patterns, in) {
			missing[in] = append(missing[in], reader)
		}
	}
	for _, in := range laneExtraInputs[lane] {
		need(in, "the lane's own steps")
	}
	for _, cid := range checkIDs {
		def := ciCoverageLookup(cid)
		if def == nil {
			continue // rule 1 reports an unknown name
		}
		for _, in := range inputs(def.Inputs, GlobalInputs, sources.For(def.ID)) {
			need(in, def.ID)
		}
	}
	return missing, nil
}

func describeMissingLaneInputs(job, lane string, missing map[string][]string) []string {
	var ins []string
	for in := range missing {
		ins = append(ins, in)
	}
	sort.Strings(ins)
	var out []string
	for _, in := range ins {
		readers := missing[in]
		sort.Strings(readers)
		out = append(out, fmt.Sprintf(
			"ci.yml: job `%s` runs `%s`, whose filters miss `%s` (read by %s), so a change there skips the lane, and a lane marker can vouch for a tree it never saw; add it to the filter",
			job, lane, in, strings.Join(compactStrings(readers), ", ")))
	}
	return out
}

func compactStrings(s []string) []string {
	var out []string
	for i, v := range s {
		if i == 0 || v != s[i-1] {
			out = append(out, v)
		}
	}
	return out
}
