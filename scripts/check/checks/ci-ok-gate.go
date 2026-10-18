package checks

import (
	"fmt"
	"regexp"
	"sort"
	"strings"
)

// ciOKJobID is the job behind the `CI OK` status check that `main`'s ruleset requires.
const ciOKJobID = "ci-ok"

// ciOKGateExempt names the ci.yml jobs `ci-ok` deliberately doesn't wait for, each with
// its reason. Everything else must be in its `needs`.
var ciOKGateExempt = map[string]string{
	ciOKJobID:        "the gate itself",
	"deploy-website": "a post-land deploy on `main` pushes, not a check; a PR never runs it",
}

// validateCIOKGate is rule 5: `ci-ok` is the one required check, so it must see every
// job's result. A job outside its `needs` can fail while `CI OK` is green: `changes`
// once could, and a failed `changes` skips every lane, which the gate counted as a pass.
// It also must run with `always()`: GitHub reports a skipped required check as passing,
// so a gate that skips itself on cancellation (`!cancelled()`) would wave a cancelled
// run through. And the `pull_request` trigger takes no path filter, or a PR outside it
// never reports `CI OK` and the required check waits forever.
func validateCIOKGate(ciYml string) []string {
	jobs := extractWorkflowJobs(ciYml)
	gate, ok := jobs[ciOKJobID]
	if !ok {
		return []string{"ci.yml has no `" + ciOKJobID + "` job; `main`'s ruleset requires its `CI OK` check"}
	}

	var violations []string
	needs := map[string]bool{}
	for _, n := range jobNeeds(gate) {
		needs[n] = true
	}
	var ids []string
	for id := range jobs {
		ids = append(ids, id)
	}
	sort.Strings(ids)
	for _, id := range ids {
		if _, exempt := ciOKGateExempt[id]; !exempt && !needs[id] {
			violations = append(violations, fmt.Sprintf(
				"ci.yml: job `%s` is not in `%s`'s needs, so it can fail while `CI OK` stays green; add it, or exempt it with a reason in ciOKGateExempt",
				id, ciOKJobID))
		}
	}
	var exempt []string
	for id := range ciOKGateExempt {
		exempt = append(exempt, id)
	}
	sort.Strings(exempt)
	for _, id := range exempt {
		if _, exists := jobs[id]; !exists {
			violations = append(violations, fmt.Sprintf("ciOKGateExempt names `%s`, which ci.yml no longer has; drop the stale entry", id))
		}
	}

	if cond := jobScalar(gate, "if"); cond != "always()" && cond != "${{ always() }}" {
		violations = append(violations, fmt.Sprintf(
			"ci.yml: `%s` must run with `if: always()` (has %q); a skipped required check counts as passing",
			ciOKJobID, cond))
	}
	if key := pullRequestPathFilter(ciYml); key != "" {
		violations = append(violations, fmt.Sprintf(
			"ci.yml: the `pull_request` trigger has `%s`, so a PR outside it never reports `CI OK` and the required check hangs; gate jobs with the `changes` filter instead",
			key))
	}
	return violations
}

var (
	workflowJobKeyRe = regexp.MustCompile(`^  ([A-Za-z0-9_-]+):\s*(#.*)?$`)
	jobNeedsNameRe   = regexp.MustCompile(`[A-Za-z0-9_][A-Za-z0-9_-]*`)
)

// extractWorkflowJobs maps each job ID under the top-level `jobs:` to its body lines
// (everything indented under its key). Line-based, like the rest of this package's
// workflow scanning: ci.yml keeps the standard two-space layout.
func extractWorkflowJobs(yml string) map[string][]string {
	jobs := map[string][]string{}
	inJobs := false
	current := ""
	for _, line := range strings.Split(yml, "\n") {
		if strings.TrimSpace(line) == "" {
			continue
		}
		if !strings.HasPrefix(line, " ") && !strings.HasPrefix(line, "#") {
			inJobs = strings.HasPrefix(line, "jobs:")
			current = ""
			continue
		}
		if !inJobs {
			continue
		}
		if m := workflowJobKeyRe.FindStringSubmatch(line); m != nil {
			current = m[1]
			jobs[current] = nil
			continue
		}
		if current != "" && strings.HasPrefix(line, "    ") {
			jobs[current] = append(jobs[current], line)
		}
	}
	return jobs
}

// jobNeeds returns the job IDs in a job's `needs`, in any of its three forms: a scalar
// (`needs: changes`), a flow sequence on one or several lines (`needs: [a, b]`), or a
// block sequence (`- a`).
func jobNeeds(body []string) []string {
	var names []string
	collecting := false
	for _, line := range body {
		trimmed := strings.TrimSpace(line)
		if strings.HasPrefix(trimmed, "#") {
			continue
		}
		if !collecting {
			if strings.HasPrefix(line, "    needs:") {
				collecting = true
				names = append(names, jobNeedsNameRe.FindAllString(strings.TrimPrefix(trimmed, "needs:"), -1)...)
			}
			continue
		}
		// A sibling key at the job's own indent ends the value.
		if strings.HasPrefix(line, "    ") && !strings.HasPrefix(line, "     ") && !strings.HasPrefix(trimmed, "-") && !strings.HasPrefix(trimmed, "]") {
			break
		}
		if i := strings.Index(trimmed, "#"); i >= 0 {
			trimmed = trimmed[:i]
		}
		names = append(names, jobNeedsNameRe.FindAllString(trimmed, -1)...)
	}
	return names
}

// jobScalar returns a job-level `key: value` scalar, unquoted, or "" when absent.
func jobScalar(body []string, key string) string {
	prefix := "    " + key + ":"
	for _, line := range body {
		if strings.HasPrefix(line, prefix) {
			return strings.Trim(strings.TrimSpace(strings.TrimPrefix(line, prefix)), `'"`)
		}
	}
	return ""
}

// pullRequestPathFilter returns `paths` or `paths-ignore` when the top-level `on:` gives
// `pull_request` one, else "".
func pullRequestPathFilter(yml string) string {
	inOn, inPR := false, false
	for _, line := range strings.Split(yml, "\n") {
		if strings.TrimSpace(line) == "" || strings.HasPrefix(strings.TrimSpace(line), "#") {
			continue
		}
		if !strings.HasPrefix(line, " ") {
			inOn = strings.HasPrefix(line, "on:")
			inPR = false
			continue
		}
		if !inOn {
			continue
		}
		if strings.HasPrefix(line, "  ") && !strings.HasPrefix(line, "   ") {
			inPR = strings.HasPrefix(strings.TrimSpace(line), "pull_request:")
			continue
		}
		if inPR {
			key := strings.TrimSuffix(strings.Fields(line)[0], ":")
			if key == "paths" || key == "paths-ignore" {
				return key
			}
		}
	}
	return ""
}
