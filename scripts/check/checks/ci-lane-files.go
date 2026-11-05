package checks

import (
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
)

// Rules 6 and 7 of `ci-coverage`: what makes ci.yml's change filters honest about the lane
// files and the light lane's gated checks.
//
// Each lane's steps live in its own reusable `<lane>-lane.yml`, called by one ci.yml job that
// gates it on `changes`' filter outputs. Two silent failures follow from that split, and both
// had already happened before it: `macos-lane.yml` sat in no filter that started the macOS lane,
// and no filter watched `.github/actions/`, so an edit to a lane's steps or to a composite
// action they use ran nothing that would test it.
//
//   - Rule 6: every filter a lane's caller reads (in its `if:` or `with:`) lists the lane file,
//     every local composite action it uses (transitively), `ci.yml`, and `.mise.toml`. A lane file
//     named outright in another lane's filter fails too, since then an edit to one lane reruns
//     two, and a `*-lane.yml` no ci.yml job calls is dead.
//   - Rule 7: a filter that gates one check in the light lane (`ciFilterGatedChecks`) covers that
//     check's whole fingerprint (`Inputs`, `GlobalInputs`, and the runner files its `Run`
//     reaches), so a check that used to run on every change still runs whenever anything it
//     reads changes.

// ciFilterGatedChecks maps each change filter that gates single checks in the light lane to
// those checks. Each must cover its checks' fingerprints (rule 7).
var ciFilterGatedChecks = map[string][]string{
	"termbase":  {"desktop-i18n-termbase"},
	"changelog": {"changelog-commit-links"},
	"scorecard": {"scorecard-local"},
	"scripts": {
		"scripts-go-gofmt", "scripts-go-vet", "scripts-go-staticcheck", "scripts-go-ineffassign", "scripts-go-misspell",
		"scripts-go-gocyclo", "scripts-go-deadcode", "scripts-go-tests", "scripts-go-nilaway",
	},
	"api-server": {"api-server-worker-types", "api-server-eslint", "api-server-typecheck", "api-server-tests"},
	"dashboard": {
		"dashboard-eslint", "dashboard-stylelint", "dashboard-svelte-check", "dashboard-import-cycles", "dashboard-knip",
		"dashboard-tests", "dashboard-build",
	},
}

// ciFilterNarrowedInputs names the fingerprint patterns a gated filter deliberately doesn't
// carry, each with its reason.
// ciFilterNarrowedChecks names whole checks a gated filter deliberately doesn't cover, each
// with its reason. The light lane saves no lane marker, so a narrowing here can only skip the
// check on a change outside its filter, never vouch for a tree it didn't see.
var ciFilterNarrowedChecks = map[string]map[string]string{
	"scripts": {
		"scripts-go-tests": "its tests also read the real tree (crates, the app, the fixture stacks, ci.yml) to hold the check runner's rules to it (`goTestsInputs`); gating it on all of that would run the Go tests on nearly every PR. A tree change that breaks one surfaces on the next `scripts/` change and on the nightly full run",
	},
}

var ciFilterNarrowedInputs = map[string]map[string]string{
	"scorecard": {
		"**": "Scorecard's file checks read the paths in the filter plus every file's CONTENT for binaries and shell scripts, which the light lane checks on the PR's own files (`docs/tooling/ci.md` § The light lane)",
	},
}

var (
	ciFilterKeyRe      = regexp.MustCompile(`^(\s+)([A-Za-z0-9_-]+):\s*(?:&([A-Za-z0-9_-]+))?\s*(?:#.*)?$`)
	ciFilterAliasRe    = regexp.MustCompile(`^\s+- \*([A-Za-z0-9_-]+)\s*$`)
	jobUsesLaneRe      = regexp.MustCompile(`^    uses: \./\.github/workflows/([A-Za-z0-9_.-]+)\s*$`)
	changesOutputRefRe = regexp.MustCompile(`needs\.changes\.outputs\.([A-Za-z0-9_-]+)`)
	filterOutputRefRe  = regexp.MustCompile(`steps\.filter\.outputs\.([A-Za-z0-9_-]+)`)
	localActionUseRe   = regexp.MustCompile(`uses:\s+\./\.github/actions/([A-Za-z0-9_-]+)`)
)

// parseCIFilters returns each filter in ci.yml's `filters: |` block with its patterns, YAML
// aliases (`- *rust-inputs`) expanded to the anchored filter's patterns, in order.
func parseCIFilters(ciYml string) map[string][]string {
	block := scanCIFilterBlock(ciYml)
	filters := map[string][]string{}
	for _, name := range block.order {
		var out []string
		for _, p := range block.raw[name] {
			alias, isAlias := strings.CutPrefix(p, "*")
			if !isAlias {
				out = append(out, p)
				continue
			}
			for _, ap := range block.raw[block.anchors[alias]] {
				if !strings.HasPrefix(ap, "*") {
					out = append(out, ap)
				}
			}
		}
		filters[name] = out
	}
	return filters
}

// ciFilterBlock is the filter block as written: patterns per filter, with `*alias`
// placeholders, plus which filter each anchor names.
type ciFilterBlock struct {
	raw     map[string][]string
	anchors map[string]string
	order   []string
}

// scanCIFilterBlock reads the `filters: |` literal block line by line. It ends at the first
// non-blank line indented at or left of the `filters:` key; a filter key is any `name:` line at
// the indent of the first one.
func scanCIFilterBlock(ciYml string) ciFilterBlock {
	block := ciFilterBlock{raw: map[string][]string{}, anchors: map[string]string{}}
	lines := strings.Split(ciYml, "\n")
	start, blockIndent := -1, 0
	for i, line := range lines {
		if strings.TrimSpace(line) == "filters: |" {
			start, blockIndent = i+1, len(line)-len(strings.TrimLeft(line, " "))
			break
		}
	}
	if start < 0 {
		return block
	}
	keyIndent, current := -1, ""
	for _, line := range lines[start:] {
		trimmed := strings.TrimSpace(line)
		if trimmed == "" || strings.HasPrefix(trimmed, "#") {
			continue
		}
		if len(line)-len(strings.TrimLeft(line, " ")) <= blockIndent {
			break
		}
		if m := ciFilterKeyRe.FindStringSubmatch(line); m != nil && (keyIndent < 0 || len(m[1]) == keyIndent) {
			keyIndent, current = len(m[1]), m[2]
			block.addFilter(current, m[3])
			continue
		}
		block.addItem(current, line)
	}
	return block
}

func (b *ciFilterBlock) addFilter(name, anchor string) {
	b.raw[name] = []string{}
	b.order = append(b.order, name)
	if anchor != "" {
		b.anchors[anchor] = name
	}
}

// addItem records one `- 'pattern'` or `- *alias` line under the current filter.
func (b *ciFilterBlock) addItem(filter, line string) {
	if filter == "" {
		return
	}
	if m := filterPathRe.FindStringSubmatch(line); m != nil {
		b.raw[filter] = append(b.raw[filter], m[1])
	} else if m := ciFilterAliasRe.FindStringSubmatch(line); m != nil {
		b.raw[filter] = append(b.raw[filter], "*"+m[1])
	}
}

// laneRuleContext is what rule 6 reads from ci.yml once.
type laneRuleContext struct {
	rootDir        string
	filters        map[string][]string
	outputToFilter map[string]string // `changes` output -> the filter it's read from
}

// validateLaneFilters is rule 6.
func validateLaneFilters(rootDir, ciYml string) []string {
	jobs := extractWorkflowJobs(ciYml)
	ctx := laneRuleContext{rootDir: rootDir, filters: parseCIFilters(ciYml), outputToFilter: map[string]string{}}
	for k, v := range jobOutputs(jobs["changes"]) {
		if m := filterOutputRefRe.FindStringSubmatch(v); m != nil && !strings.HasSuffix(k, "-changed") {
			ctx.outputToFilter[k] = m[1]
		}
	}

	var violations []string
	laneFilters := map[string]map[string]bool{} // lane file -> the filters its callers read
	var ids []string
	for id := range jobs {
		ids = append(ids, id)
	}
	sort.Strings(ids)
	for _, id := range ids {
		lane := jobLaneFile(jobs[id])
		if lane == "" {
			continue
		}
		if laneFilters[lane] == nil {
			laneFilters[lane] = map[string]bool{}
		}
		violations = append(violations, ctx.checkCaller(id, lane, jobs[id], laneFilters[lane])...)
	}
	violations = append(violations, foreignLaneFiles(ctx.filters, laneFilters)...)
	return append(violations, uncalledLaneFiles(rootDir, laneFilters)...)
}

// jobLaneFile returns the local reusable workflow a job calls, or "".
func jobLaneFile(body []string) string {
	for _, line := range body {
		if m := jobUsesLaneRe.FindStringSubmatch(line); m != nil {
			return m[1]
		}
	}
	return ""
}

// checkCaller checks every filter one lane caller reads, recording them in readFilters.
func (ctx laneRuleContext) checkCaller(id, lane string, body []string, readFilters map[string]bool) []string {
	lanePath := ".github/workflows/" + lane
	required := []string{lanePath, ".github/workflows/ci.yml", ".mise.toml"}
	var violations []string
	actions, err := localActionsUsed(ctx.rootDir, lanePath)
	if err != nil {
		violations = append(violations, err.Error())
	}
	for _, a := range actions {
		required = append(required, ".github/actions/"+a+"/action.yml")
	}
	for _, line := range body {
		for _, m := range changesOutputRefRe.FindAllStringSubmatch(line, -1) {
			filter, isFilter := ctx.outputToFilter[m[1]]
			if !isFilter || readFilters[filter] {
				continue
			}
			readFilters[filter] = true
			for _, path := range required {
				if !matchesAny(path, ctx.filters[filter]) {
					violations = append(violations, fmt.Sprintf(
						"ci.yml: job `%s` runs `%s` on the `%s` filter, which doesn't match `%s`, so an edit there never reruns the lane",
						id, lane, filter, path))
				}
			}
		}
	}
	return violations
}

// foreignLaneFiles flags a lane file named outright in a filter none of its callers read:
// an edit to that lane would rerun another one too.
func foreignLaneFiles(filters map[string][]string, laneFilters map[string]map[string]bool) []string {
	var names []string
	for name := range filters {
		names = append(names, name)
	}
	sort.Strings(names)
	var violations []string
	for _, name := range names {
		for _, p := range filters[name] {
			lane, ok := strings.CutPrefix(p, ".github/workflows/")
			if read, called := laneFilters[lane]; ok && called && !read[name] {
				violations = append(violations, fmt.Sprintf(
					"ci.yml: the `%s` filter lists `%s`, which no caller of that lane reads, so an edit to one lane reruns another; drop it",
					name, p))
			}
		}
	}
	return violations
}

// uncalledLaneFiles flags a `*-lane.yml` no ci.yml job calls.
func uncalledLaneFiles(rootDir string, laneFilters map[string]map[string]bool) []string {
	entries, err := os.ReadDir(filepath.Join(rootDir, ".github", "workflows"))
	if err != nil {
		return nil
	}
	var violations []string
	for _, e := range entries {
		if _, called := laneFilters[e.Name()]; strings.HasSuffix(e.Name(), "-lane.yml") && !called {
			violations = append(violations, fmt.Sprintf(
				".github/workflows/%s: no ci.yml job calls this lane file, so it runs nowhere", e.Name()))
		}
	}
	return violations
}

// localActionsUsed returns the local composite actions a workflow or action file uses,
// following each action's own `uses:` too, sorted.
func localActionsUsed(rootDir, relPath string) ([]string, error) {
	found := map[string]bool{}
	queue := []string{relPath}
	for len(queue) > 0 {
		path := queue[0]
		queue = queue[1:]
		content, err := os.ReadFile(filepath.Join(rootDir, path))
		if err != nil {
			return nil, fmt.Errorf("couldn't read %s: %w", path, err)
		}
		for _, line := range strings.Split(string(content), "\n") {
			if strings.HasPrefix(strings.TrimSpace(line), "#") {
				continue
			}
			if m := localActionUseRe.FindStringSubmatch(line); m != nil && !found[m[1]] {
				found[m[1]] = true
				queue = append(queue, ".github/actions/"+m[1]+"/action.yml")
			}
		}
	}
	var out []string
	for a := range found {
		out = append(out, a)
	}
	sort.Strings(out)
	return out, nil
}

// validateGatedCheckFilters is rule 7.
func validateGatedCheckFilters(rootDir, ciYml string) []string {
	filters := parseCIFilters(ciYml)
	sources := LoadRunnerSources(rootDir)
	var violations []string
	var names []string
	for name := range ciFilterGatedChecks {
		names = append(names, name)
	}
	sort.Strings(names)
	for _, name := range names {
		patterns, ok := filters[name]
		if !ok {
			violations = append(violations, fmt.Sprintf("ci.yml has no `%s` filter, which ciFilterGatedChecks names", name))
			continue
		}
		for _, id := range ciFilterGatedChecks[name] {
			if _, narrowed := ciFilterNarrowedChecks[name][id]; narrowed {
				continue
			}
			def := ciCoverageLookup(id)
			if def == nil {
				violations = append(violations, fmt.Sprintf("ciFilterGatedChecks names `%s`, which isn't a registry check", id))
				continue
			}
			var inputs []string
			inputs = append(inputs, def.Inputs...)
			inputs = append(inputs, GlobalInputs...)
			inputs = append(inputs, sources.For(id)...)
			for _, in := range inputs {
				if strings.HasPrefix(in, "!") {
					continue // an exclusion only narrows the set
				}
				if _, narrowed := ciFilterNarrowedInputs[name][in]; narrowed {
					continue
				}
				if !filterCoversInput(patterns, in) {
					violations = append(violations, fmt.Sprintf(
						"ci.yml: the `%s` filter gates `%s` but misses its input `%s`, so a change there skips the check; add it",
						name, id, in))
				}
			}
		}
	}
	return violations
}

// filterCoversInput reports whether a filter's patterns cover one fingerprint pattern: listed
// as is, matched as a concrete path, or inside a listed `dir/**`.
func filterCoversInput(filter []string, input string) bool {
	for _, p := range filter {
		if p == input {
			return true
		}
		if dir, ok := strings.CutSuffix(p, "/**"); ok && !strings.ContainsAny(dir, "*?[") && strings.HasPrefix(input, dir+"/") {
			return true
		}
	}
	return !strings.ContainsAny(input, "*?[") && matchesAny(input, filter)
}
