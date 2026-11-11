package checks

import (
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
)

// Rule 9 of `ci-coverage`: a standalone lane file. Every `*-lane.yml` is a reusable
// workflow `ci.yml` calls, except one that triggers itself and declares no `workflow_call`
// (`windows-lane.yml`, which has to stay outside `CI OK` until it's green). Nothing in `ci.yml` gates it, so its own
// `push.paths` is its filter, and it answers to the same contract a caller's filter does
// (rules 6 and 8): it lists the lane file, every local action the lane uses, and
// `.mise.toml`, and it covers the fingerprint of every check the lane runs. Without that, a
// change to a file only its checks read would never start it.

var (
	workflowOnInlineRe = regexp.MustCompile(`^on:\s*(\S.*)$`)
	workflowKeyRe      = regexp.MustCompile(`^( *)([A-Za-z_][A-Za-z0-9_-]*):`)
	workflowListItemRe = regexp.MustCompile(`^ *- +['"]?([^'"#]+?)['"]?\s*(?:#.*)?$`)
)

// workflowTriggers reads a workflow's top-level `on:`: the events it names, and the
// `push.paths` list (nil when push has none, which means every push).
func workflowTriggers(content string) (events []string, pushPaths []string) {
	lines := strings.Split(content, "\n")
	for i, line := range lines {
		if m := workflowOnInlineRe.FindStringSubmatch(line); m != nil {
			return strings.FieldsFunc(m[1], func(r rune) bool { return strings.ContainsRune("[], ", r) }), nil
		}
		if strings.TrimRight(line, " ") == "on:" {
			return parseOnBlock(lines[i+1:])
		}
	}
	return nil, nil
}

// parseOnBlock reads the indented block under `on:`, up to the next top-level key.
func parseOnBlock(lines []string) (events []string, pushPaths []string) {
	event, inPaths := "", false
	for _, l := range lines {
		trimmed := strings.TrimSpace(l)
		if trimmed == "" || strings.HasPrefix(trimmed, "#") {
			continue
		}
		if !strings.HasPrefix(l, " ") {
			break
		}
		if m := workflowKeyRe.FindStringSubmatch(l); m != nil {
			if len(m[1]) == 2 {
				event, inPaths = m[2], false
				events = append(events, event)
			} else if event == "push" {
				inPaths = m[2] == "paths"
				if inPaths {
					pushPaths = []string{}
				}
			}
			continue
		}
		if m := workflowListItemRe.FindStringSubmatch(l); m != nil && inPaths {
			pushPaths = append(pushPaths, m[1])
		}
	}
	return events, pushPaths
}

// isStandaloneWorkflow says whether a workflow starts itself and can't be called: it names
// at least one trigger and none of them is `workflow_call`. A lane that declares
// `workflow_call` is built to be called, so it needs its `ci.yml` caller whatever else it
// lists: a `workflow_dispatch` beside it runs the lane by hand, never on a PR, and counting
// that as "triggers itself" would keep the check green on a gating lane that lost its caller.
func isStandaloneWorkflow(content string) bool {
	events, _ := workflowTriggers(content)
	for _, e := range events {
		if e == "workflow_call" {
			return false
		}
	}
	return len(events) > 0
}

// validateStandaloneLanes is rule 9, for every lane file no `ci.yml` job calls.
func validateStandaloneLanes(rootDir string, called map[string]map[string]bool) []string {
	entries, err := os.ReadDir(filepath.Join(rootDir, ".github", "workflows"))
	if err != nil {
		return nil
	}
	sources := LoadRunnerSources(rootDir)
	var violations []string
	for _, e := range entries {
		name := e.Name()
		if _, isCalled := called[name]; isCalled || !strings.HasSuffix(name, "-lane.yml") {
			continue
		}
		rel := ".github/workflows/" + name
		content, readErr := os.ReadFile(filepath.Join(rootDir, filepath.FromSlash(rel)))
		if readErr != nil || !isStandaloneWorkflow(string(content)) {
			continue // `uncalledLaneFiles` reports a lane file that runs nowhere
		}
		_, paths := workflowTriggers(string(content))
		if paths == nil {
			continue // every push starts it, so nothing can slip past
		}
		violations = append(violations, standaloneLaneMisses(rootDir, name, rel, paths, sources)...)
	}
	sort.Strings(violations)
	return violations
}

func standaloneLaneMisses(rootDir, name, rel string, paths []string, sources *RunnerSourceIndex) []string {
	var violations []string
	need := []string{rel, ".mise.toml"}
	actions, err := localActionsUsed(rootDir, rel)
	if err != nil {
		return []string{fmt.Sprintf("%s: %v", rel, err)}
	}
	for _, a := range actions {
		need = append(need, ".github/actions/"+a+"/action.yml")
	}
	for _, in := range need {
		if !filterCoversInput(paths, in) {
			violations = append(violations, fmt.Sprintf(
				"%s: its `push.paths` misses `%s`, so an edit there never starts the lane; add it", rel, in))
		}
	}
	missing, err := laneInputsMissingFrom(rootDir, name, paths, sources)
	if err != nil {
		return append(violations, fmt.Sprintf("couldn't read %s: %v", rel, err))
	}
	for _, in := range sortedKeys(missing) {
		readers := missing[in]
		sort.Strings(readers)
		violations = append(violations, fmt.Sprintf(
			"%s: its `push.paths` misses `%s` (read by %s), so a change there never starts the lane; add it",
			rel, in, strings.Join(compactStrings(readers), ", ")))
	}
	return violations
}
