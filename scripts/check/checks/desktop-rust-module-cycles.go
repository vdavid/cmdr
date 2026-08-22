package checks

import (
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
)

// The module-cycle ratchet. A subsystem welded into a cycle with another one
// can't be understood, changed, or extracted into a crate alone, and it had
// re-welded twice before anything measured it. This is what measures it.
//
// What it does NOT measure is the thing that makes the obvious version of this
// check unusable: splitting a long file into submodules grows the maximum
// strongly-connected component without adding one line of coupling (`mod.rs`
// defines a type, the children implement against it, the parent calls them).
// `lifecycle/state.rs` becoming `lifecycle/state/` with eight children took
// `cmdr-index`'s largest component from six modules to 19, and that change
// improved the code. A ratchet that fires on it teaches people to silence
// ratchets, so this one collapses parent-child hubs before it counts.
//
// Warn-only, like the other allowlist gauges: raising a number needs David's OK
// (`.claude/rules/file-length-allowlist.md`), and the fix is to cut the edge.

// cargoModulesVersion pins the tool the baseline was measured with. The absolute
// numbers move between releases (0.26.0 reports `cmdr` at 128 of 528 modules in a
// cycle where 0.27.0 reports 126 of 522), so a floating version would drift the
// ratchet silently and fire on somebody else's machine for no reason at all. The
// allowlist records the same version, and a disagreement between the two is
// reported rather than papered over.
//
// To bump: edit this line, install it, run `pnpm check module-cycles`, and re-seed
// the allowlist deliberately with the numbers the new version reports.
const cargoModulesVersion = "0.27.0"

// moduleCyclesFlags is the measurement, and every flag earns its place:
// `--no-fns`/`--no-types`/`--no-traits` reduce the graph to modules, `--no-owns`
// drops the containment edges so only `use` dependencies remain, and
// `--no-externs`/`--no-sysroot` keep other crates out of one crate's picture.
// `--acyclic` is deliberately absent: it runs BEFORE the filters, so it trips on a
// type and its own method and can never pass. Cycle detection happens here instead.
// See `DETAILS.md` § "Rust module cycles" for the five traps behind those choices.
var moduleCyclesFlags = []string{
	"--no-fns", "--no-types", "--no-traits", "--no-owns", "--no-externs", "--no-sysroot",
}

var (
	// moduleDotEdgeRe matches `"a" -> "b" [...]`, one `use` dependency.
	moduleDotEdgeRe = regexp.MustCompile(`^\s*"([^"]+)"\s*->\s*"([^"]+)"`)
	// moduleDotNodeRe matches `"a" [label=...]`, one module.
	moduleDotNodeRe = regexp.MustCompile(`^\s*"([^"]+)"\s*\[label=`)
)

// moduleGraph is one crate's module dependency graph.
type moduleGraph struct {
	// pkg is the cargo package name, which is what a reader recognizes. It is not
	// always the lib name the graph uses: package `cmdr` has lib `cmdr_lib`.
	pkg string
	// root is the crate-root node, the one module path with no `::` in it.
	root string
	// nodes are module paths in the order cargo-modules declared them.
	nodes []string
	edges map[string][]string
}

// parseModuleGraph reads cargo-modules' DOT output.
func parseModuleGraph(pkg, dot string) moduleGraph {
	graph := moduleGraph{pkg: pkg, edges: map[string][]string{}}
	seen := map[string]bool{}
	for _, line := range strings.Split(dot, "\n") {
		if match := moduleDotEdgeRe.FindStringSubmatch(line); match != nil {
			graph.edges[match[1]] = append(graph.edges[match[1]], match[2])
			continue
		}
		if match := moduleDotNodeRe.FindStringSubmatch(line); match != nil && !seen[match[1]] {
			seen[match[1]] = true
			graph.nodes = append(graph.nodes, match[1])
			if !strings.Contains(match[1], "::") {
				graph.root = match[1]
			}
		}
	}
	return graph
}

// stronglyConnectedComponents returns Tarjan's components of size two or more,
// each sorted. `within` restricts the walk to a subset (nil means the whole
// graph), which is what lets a component be re-examined with its hub removed.
func stronglyConnectedComponents(nodes []string, edges map[string][]string, within map[string]bool) [][]string {
	var (
		index    = map[string]int{}
		lowLink  = map[string]int{}
		onStack  = map[string]bool{}
		stack    []string
		counter  int
		out      [][]string
		included = func(n string) bool { return within == nil || within[n] }
	)

	var visit func(node string)
	visit = func(node string) {
		index[node] = counter
		lowLink[node] = counter
		counter++
		stack = append(stack, node)
		onStack[node] = true

		for _, next := range edges[node] {
			if !included(next) {
				continue
			}
			if _, known := index[next]; !known {
				visit(next)
				lowLink[node] = min(lowLink[node], lowLink[next])
			} else if onStack[next] {
				lowLink[node] = min(lowLink[node], index[next])
			}
		}

		if lowLink[node] != index[node] {
			return
		}
		var component []string
		for {
			top := stack[len(stack)-1]
			stack = stack[:len(stack)-1]
			onStack[top] = false
			component = append(component, top)
			if top == node {
				break
			}
		}
		if len(component) > 1 {
			sort.Strings(component)
			out = append(out, component)
		}
	}

	for _, node := range nodes {
		if !included(node) {
			continue
		}
		if _, known := index[node]; !known {
			visit(node)
		}
	}
	return out
}

// isModuleAncestor reports whether parent is a strict ancestor of child.
func isModuleAncestor(parent, child string) bool {
	return parent != child && strings.HasPrefix(child, parent+"::")
}

// collapseHubs answers the question the raw component size gets wrong: how many
// INDEPENDENT things are stuck together here, and which ones?
//
// A module and its descendants are one thing, however many files it was split
// into, so every member of the component folds into the topmost ancestor of its
// own that is also in the component. Two cases follow:
//
//   - More than one survivor: those are genuinely separate modules that depend on
//     each other in a circle. That's the welding this check exists to see, and the
//     count of survivors is the number this ratchet tracks.
//   - Exactly one survivor: the whole component hangs under a single module, so
//     the cycle is the idiomatic parent ↔ child relation and the parent is a hub,
//     not a coupling. Drop the hub and look again at what's left — siblings that
//     also depend on each other in a circle are still a tangle, and without this
//     step one idiomatic `parent → child` edge would hide them completely.
//
// EVERY circle left under a dropped hub is reported, not the biggest one. The
// allowlist keys a LIST of sizes per home exactly so a new tangle can't hide
// behind a bigger one; answering with a single set here would rebuild that hiding
// place one level down, where nothing else is watching.
//
// Each returned set is sorted and holds at least two modules; the sets come
// largest first. A component that is nothing but a hub and its children returns
// none: there is no tangle in it.
func collapseHubs(component []string, edges map[string][]string) [][]string {
	inComponent := make(map[string]bool, len(component))
	for _, module := range component {
		inComponent[module] = true
	}

	survivors := map[string]bool{}
	for _, module := range component {
		top := module
		for _, candidate := range component {
			if isModuleAncestor(candidate, top) {
				top = candidate
			}
		}
		// One pass finds the topmost only if the candidates happen to be ordered;
		// repeat until nothing higher is left. Depth is a module nesting depth, so
		// this converges in a handful of rounds.
		for {
			higher := top
			for _, candidate := range component {
				if isModuleAncestor(candidate, higher) {
					higher = candidate
				}
			}
			if higher == top {
				break
			}
			top = higher
		}
		survivors[top] = true
	}
	if len(survivors) > 1 {
		return [][]string{sortedKeys(survivors)}
	}

	hub := sortedKeys(survivors)[0]
	rest := make([]string, 0, len(component)-1)
	remaining := make(map[string]bool, len(component)-1)
	for _, module := range component {
		if module != hub {
			rest = append(rest, module)
			remaining[module] = true
		}
	}

	var tangles [][]string
	for _, sub := range stronglyConnectedComponents(rest, edges, remaining) {
		tangles = append(tangles, collapseHubs(sub, edges)...)
	}
	sortTanglesLargestFirst(tangles)
	return tangles
}

// sortTanglesLargestFirst orders collapsed sets the way the allowlist stores
// them, so a home's measured sizes line up with its accepted ones.
func sortTanglesLargestFirst(tangles [][]string) {
	sort.Slice(tangles, func(i, j int) bool {
		if len(tangles[i]) != len(tangles[j]) {
			return len(tangles[i]) > len(tangles[j])
		}
		return tangles[i][0] < tangles[j][0]
	})
}

// moduleTangle is one collapsed component: two or more modules that depend on
// each other in a circle after the parent-child hubs are folded away.
type moduleTangle struct {
	// home is where the tangle lives, in package-qualified form
	// (`cmdr::file_system::write_operations`): the longest module path all its
	// members share. It's the allowlist key, so it has to read like a place rather
	// than like a list of modules.
	home string
	// members are the collapsed modules, the ones a cut has to separate.
	members []string
	// rawSize is how many modules the underlying component held before collapsing,
	// which is the number a raw `cargo modules` reading would have shown.
	rawSize int
}

// tangleHome computes the package-qualified longest common module path.
func tangleHome(graph moduleGraph, members []string) string {
	shared := strings.Split(members[0], "::")
	for _, member := range members[1:] {
		segments := strings.Split(member, "::")
		for len(shared) > len(segments) || !equalSegments(shared, segments[:len(shared)]) {
			shared = shared[:len(shared)-1]
		}
	}
	// The first segment is the lib name; the package name is what people call it.
	return strings.Join(append([]string{graph.pkg}, shared[1:]...), "::")
}

func equalSegments(a, b []string) bool {
	for i := range a {
		if a[i] != b[i] {
			return false
		}
	}
	return true
}

// crateModuleCycles is one crate's measurement.
type crateModuleCycles struct {
	pkg      string
	modules  int
	inCycle  int
	maxRaw   int
	tangles  []moduleTangle
	maxSize  int
	rawCount int
}

// measureModuleCycles turns one crate's DOT output into its tangles, worst first.
func measureModuleCycles(pkg, dot string) crateModuleCycles {
	graph := parseModuleGraph(pkg, dot)
	measurement := crateModuleCycles{pkg: pkg, modules: len(graph.nodes)}
	for _, component := range stronglyConnectedComponents(graph.nodes, graph.edges, nil) {
		measurement.rawCount++
		measurement.inCycle += len(component)
		measurement.maxRaw = max(measurement.maxRaw, len(component))
		for _, collapsed := range collapseHubs(component, graph.edges) {
			if len(collapsed) < 2 {
				continue
			}
			measurement.tangles = append(measurement.tangles, moduleTangle{
				home:    tangleHome(graph, collapsed),
				members: collapsed,
				rawSize: len(component),
			})
			measurement.maxSize = max(measurement.maxSize, len(collapsed))
		}
	}
	sort.Slice(measurement.tangles, func(i, j int) bool {
		a, b := measurement.tangles[i], measurement.tangles[j]
		if len(a.members) != len(b.members) {
			return len(a.members) > len(b.members)
		}
		return a.home < b.home
	})
	return measurement
}

// moduleCyclesAllowlist is the on-disk shape of the baseline. `Tangles` maps a
// home to the accepted sizes of the tangles living there, largest first: a list
// rather than one number because two independent tangles can share a home, and
// collapsing them to a maximum would let a new one hide behind a bigger one.
type moduleCyclesAllowlist struct {
	Comment string `json:"$comment,omitempty"`
	// ToolVersion is the cargo-modules release the numbers were measured with.
	ToolVersion string           `json:"toolVersion"`
	Tangles     map[string][]int `json:"tangles"`
}

func moduleCyclesAllowlistPath(rootDir string) string {
	return filepath.Join(rootDir, "scripts", "check", "checks", "module-cycles-allowlist.json")
}

func loadModuleCyclesAllowlist(rootDir string) moduleCyclesAllowlist {
	var list moduleCyclesAllowlist
	data, err := os.ReadFile(moduleCyclesAllowlistPath(rootDir))
	if err != nil {
		return list
	}
	if err := json.Unmarshal(data, &list); err != nil {
		return moduleCyclesAllowlist{}
	}
	return list
}

// tangleSizesByHome groups the measured tangles by home, largest first.
func tangleSizesByHome(measurements []crateModuleCycles) map[string][]int {
	sizes := map[string][]int{}
	for _, measurement := range measurements {
		for _, tangle := range measurement.tangles {
			sizes[tangle.home] = append(sizes[tangle.home], len(tangle.members))
		}
	}
	for _, list := range sizes {
		sort.Sort(sort.Reverse(sort.IntSlice(list)))
	}
	return sizes
}

// withinAllowance reports whether a home's current tangle sizes are covered by
// the accepted ones: no more tangles than allowed, and none bigger, comparing
// largest against largest.
func withinAllowance(current, allowed []int) bool {
	if len(allowed) == 0 {
		return len(current) == 0
	}
	if len(current) > len(allowed) {
		return false
	}
	for i, size := range current {
		if size > allowed[i] {
			return false
		}
	}
	return true
}

func sameSizes(a, b []int) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i] != b[i] {
			return false
		}
	}
	return true
}

// shrinkwrapModuleCyclesAllowlist drops homes whose cycles are gone and ratchets
// the rest down to what's measured now. It mutates list and returns one line per
// change. There's no slack: a tangle size only moves when somebody adds or cuts a
// dependency, so it doesn't need the buffer a line count does.
func shrinkwrapModuleCyclesAllowlist(list *moduleCyclesAllowlist, current map[string][]int) []string {
	var changes []string
	for _, home := range sortedKeys(list.Tangles) {
		allowed := list.Tangles[home]
		measured, still := current[home]
		switch {
		case !still:
			delete(list.Tangles, home)
			changes = append(changes, fmt.Sprintf("removed %s (no tangles left there)", home))
		case withinAllowance(measured, allowed) && !sameSizes(measured, allowed):
			list.Tangles[home] = measured
			changes = append(changes, fmt.Sprintf("ratcheted %s: %s → %s", home, formatSizes(allowed), formatSizes(measured)))
		}
	}
	return changes
}

func formatSizes(sizes []int) string {
	parts := make([]string, len(sizes))
	for i, size := range sizes {
		parts[i] = fmt.Sprintf("%d", size)
	}
	return "[" + strings.Join(parts, ", ") + "]"
}

// moduleCyclesRegression is one home carrying more or bigger tangles than agreed.
type moduleCyclesRegression struct {
	home    string
	current []int
	allowed []int
	listed  bool
}

// findModuleCyclesRegressions returns every home over its allowance, worst first.
// A home missing from the allowlist is a regression too: a new tangle is exactly
// the event this check was built to catch, so it can never be a silent pass.
func findModuleCyclesRegressions(current map[string][]int, list moduleCyclesAllowlist) []moduleCyclesRegression {
	var out []moduleCyclesRegression
	for _, home := range sortedKeys(current) {
		allowed, listed := list.Tangles[home]
		if listed && withinAllowance(current[home], allowed) {
			continue
		}
		out = append(out, moduleCyclesRegression{home: home, current: current[home], allowed: allowed, listed: listed})
	}
	sort.Slice(out, func(i, j int) bool { return out[i].current[0] > out[j].current[0] })
	return out
}

// installedCargoModulesVersion reports the version on PATH, and whether the tool
// is there at all.
func installedCargoModulesVersion() (string, bool) {
	if !CommandExists("cargo-modules") {
		return "", false
	}
	output, err := RunCommand(exec.Command("cargo-modules", "--version"), true)
	if err != nil {
		return "", false
	}
	fields := strings.Fields(strings.TrimSpace(output))
	if len(fields) == 0 {
		return "", false
	}
	return fields[len(fields)-1], true
}

// cargoModulesInstallCommand is what a developer runs to opt in.
func cargoModulesInstallCommand() string {
	return fmt.Sprintf("cargo install cargo-modules --version %s --locked", cargoModulesVersion)
}

// cargoModulesSkipReason returns why this run can't measure anything, or "" when
// the pinned tool is there.
//
// A missing or differently-versioned tool is a SKIP, never an install and never a
// failure. Building cargo-modules pulls in rust-analyzer and costs minutes, so
// starting that behind a check nobody asked to install anything for would be worse
// than saying what's missing. A wrong version skips for a different reason: its
// numbers aren't comparable with the baseline's, and warning on incomparable
// numbers is noise that teaches people to ignore the check.
func cargoModulesSkipReason() string {
	installed, present := installedCargoModulesVersion()
	switch {
	case !present:
		return fmt.Sprintf("cargo-modules isn't installed; `%s` enables this check", cargoModulesInstallCommand())
	case installed != cargoModulesVersion:
		return fmt.Sprintf("cargo-modules %s is installed and the baseline is measured with %s, whose numbers differ; `%s` enables this check",
			installed, cargoModulesVersion, cargoModulesInstallCommand())
	}
	return ""
}

// moduleCyclesPackages are the crates this check measures: every first-party
// library member. A bin-only tool has no `--lib` graph to ask for, and the
// vendored fork's module layout isn't ours to hold to a ratchet.
func moduleCyclesPackages(rootDir string) ([]string, error) {
	members, err := MembersOfKind(rootDir, KindApp)
	if err != nil {
		return nil, err
	}
	var packages []string
	for _, member := range members {
		if fileExists(filepath.Join(member.SrcDir, "lib.rs")) {
			packages = append(packages, member.Name)
		}
	}
	return packages, nil
}

// runCargoModules prints one crate's module graph as DOT. cargo-modules takes no
// `--locked`: it loads the workspace rust-analyzer-style rather than resolving
// dependencies, so there's no resolution for a lockfile to pin.
func runCargoModules(rootDir, pkg string) (string, error) {
	args := append([]string{"modules", "dependencies", "--lib", "--package", pkg}, moduleCyclesFlags...)
	cmd := exec.Command("cargo", args...)
	cmd.Dir = rootDir
	output, err := RunCommand(cmd, true)
	if err != nil {
		return "", fmt.Errorf("cargo-modules couldn't read %s\n%s", pkg, indentOutput(output))
	}
	return output, nil
}

// formatModuleCyclesGauge renders the per-crate table. This IS the gauge: it
// prints under `pnpm check -v` and on every warn.
func formatModuleCyclesGauge(measurements []crateModuleCycles) string {
	var sb strings.Builder
	total, tangled := 0, 0
	for _, measurement := range measurements {
		total += measurement.modules
		tangled += len(measurement.tangles)
	}
	fmt.Fprintf(&sb, "%s %s across %d %s hold %s %s once parent-child hubs collapse",
		formatThousands(total), Pluralize(total, "module", "modules"),
		len(measurements), Pluralize(len(measurements), "crate", "crates"),
		formatThousands(tangled), Pluralize(tangled, "tangle", "tangles"))

	width := len("crate")
	for _, measurement := range measurements {
		width = max(width, len(measurement.pkg))
	}
	const row = "\n  %-*s  %8s  %9s  %8s  %8s"
	fmt.Fprintf(&sb, row, width, "crate", "modules", "in cycles", "max raw", "tangles")
	for _, measurement := range measurements {
		fmt.Fprintf(&sb, row, width, measurement.pkg,
			formatThousands(measurement.modules), formatThousands(measurement.inCycle),
			formatThousands(measurement.maxRaw), formatThousands(len(measurement.tangles)))
	}
	return sb.String()
}

// formatModuleCyclesRegressions renders the warn body: which home gained or grew a
// tangle, and which modules are in it. Naming the members matters more than the
// number, because the fix is always one specific edge.
func formatModuleCyclesRegressions(regressions []moduleCyclesRegression, measurements []crateModuleCycles) string {
	byHome := map[string][]moduleTangle{}
	for _, measurement := range measurements {
		for _, tangle := range measurement.tangles {
			byHome[tangle.home] = append(byHome[tangle.home], tangle)
		}
	}

	var sb strings.Builder
	fmt.Fprintf(&sb, "%d %s gained module cycles:", len(regressions),
		Pluralize(len(regressions), "place", "places"))
	for _, regression := range regressions {
		against := fmt.Sprintf("allowlist: %s", formatSizes(regression.allowed))
		if !regression.listed {
			against = "not in the allowlist"
		}
		fmt.Fprintf(&sb, "\n  - %s: %s%s%s (%s)", regression.home,
			ansiYellow, formatSizes(regression.current), ansiReset, against)
		for _, tangle := range byHome[regression.home] {
			fmt.Fprintf(&sb, "\n    %d modules depend on each other in a circle (%d before collapsing): %s",
				len(tangle.members), tangle.rawSize, strings.Join(shortenModules(tangle.members), ", "))
		}
	}
	sb.WriteString("\nCut one edge and the tangle opens. ⚠️ Read the direction carefully first: cargo-modules attributes " +
		"an `impl` to the module defining the type it PRODUCES, so an edge can point the opposite way from the code " +
		"(`scripts/check/checks/DETAILS.md` § \"Rust module cycles\"). Raising a number needs David's OK " +
		"(`.claude/rules/file-length-allowlist.md`).")
	return sb.String()
}

// shortenModules drops the shared prefix so the names that differ are readable.
func shortenModules(members []string) []string {
	shared := strings.Split(members[0], "::")
	for _, member := range members[1:] {
		segments := strings.Split(member, "::")
		for len(shared) > len(segments) || !equalSegments(shared, segments[:len(shared)]) {
			shared = shared[:len(shared)-1]
		}
	}
	prefix := strings.Join(shared, "::") + "::"
	out := make([]string, len(members))
	for i, member := range members {
		out[i] = strings.TrimPrefix(member, prefix)
	}
	return out
}

// RunRustModuleCycles measures how much of each crate's module graph depends on
// itself in a circle, with parent-child hubs collapsed so a file split into
// submodules doesn't read as new coupling. Warn-only: it reports a home whose
// tangles grew past its allowlisted sizes (or that isn't listed yet), and outside
// CI it shrink-wraps the allowlist so every cut edge shows up as a number falling.
func RunRustModuleCycles(ctx *CheckContext) (CheckResult, error) {
	if skipReason := cargoModulesSkipReason(); skipReason != "" {
		return Skipped(skipReason), nil
	}

	packages, err := moduleCyclesPackages(ctx.RootDir)
	if err != nil {
		return CheckResult{}, err
	}
	measurements := make([]crateModuleCycles, 0, len(packages))
	for _, pkg := range packages {
		dot, runErr := runCargoModules(ctx.RootDir, pkg)
		if runErr != nil {
			return CheckResult{}, runErr
		}
		measurements = append(measurements, measureModuleCycles(pkg, dot))
	}

	gauge := formatModuleCyclesGauge(measurements)
	allowlist := loadModuleCyclesAllowlist(ctx.RootDir)
	if allowlist.ToolVersion != cargoModulesVersion {
		return CheckResult{
			Code: ResultWarning,
			Message: fmt.Sprintf(
				"The baseline was measured with cargo-modules %s and this check now pins %s, so the two aren't comparable.\n"+
					"Re-seed `scripts/check/checks/module-cycles-allowlist.json` from a run on %s, deliberately.\n%s",
				allowlist.ToolVersion, cargoModulesVersion, cargoModulesVersion, gauge),
			Total: len(packages), Issues: 1, Changes: -1,
		}, nil
	}

	current := tangleSizesByHome(measurements)
	staleChanges := shrinkwrapModuleCyclesAllowlist(&allowlist, current)
	madeChanges := false
	if len(staleChanges) > 0 && !ctx.CI {
		if err := writeJSONAllowlist(moduleCyclesAllowlistPath(ctx.RootDir), allowlist); err != nil {
			return CheckResult{}, err
		}
		reformatWithOxfmt(ctx.RootDir, "scripts/check/checks/module-cycles-allowlist.json")
		madeChanges = true
	}

	var staleMsg string
	if len(staleChanges) > 0 {
		verb := "Shrink-wrapped allowlist"
		if ctx.CI {
			verb = "Stale allowlist entries (a local run shrink-wraps them)"
		}
		staleMsg = fmt.Sprintf("%s:\n  - %s", verb, strings.Join(staleChanges, "\n  - "))
	}

	regressions := findModuleCyclesRegressions(current, allowlist)
	if len(regressions) == 0 {
		if staleMsg != "" {
			msg := gauge + "\n" + staleMsg
			if ctx.CI {
				return CheckResult{Code: ResultWarning, Message: msg, Total: len(packages), Issues: 0, Changes: -1}, nil
			}
			return SuccessWithChanges(msg), nil
		}
		return Success(gauge), nil
	}

	msg := formatModuleCyclesRegressions(regressions, measurements) + "\n" + gauge
	if staleMsg != "" {
		msg += "\n" + staleMsg
	}
	return CheckResult{
		Code:        ResultWarning,
		Message:     msg,
		MadeChanges: madeChanges,
		Total:       len(packages),
		Issues:      len(regressions),
		Changes:     -1,
	}, nil
}
