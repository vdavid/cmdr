package checks

import (
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"sort"
	"strings"
)

// The index crates' boundary, made self-defending.
//
// `cmdr-index` and `cmdr-fs` exist so the app can't reach into index internals and
// an index change can't have an unbounded blast radius. Two properties carry that,
// and both erode the same way — one convenient addition at a time, each individually
// reasonable:
//
//  1. **No `tauri` in the dependency tree.** The moment the index can name
//     `AppHandle`, every seam in `indexing/host/` is optional and the boundary is a
//     convention again. Machine-checkable against `cargo metadata`, which is the
//     point: the compiler already enforces "no reach into the app", but nothing
//     stops someone adding the dependency that would make the reach legal.
//  2. **A bounded public surface.** Making the index a crate means everything the
//     app touches has to be `pub`, so the honest failure mode is 65 `pub(crate)`
//     items quietly becoming 65 `pub` items: the build-time split lands and the
//     encapsulation is abandoned, with everything still green. The item-by-item
//     audit behind today's shape is
//     `crates/cmdr-index/src/indexing/handle/DETAILS.md`; this check is what stops
//     the next `pub` from being added without going through it.
//
// The ceilings below are the numbers that audit landed on. Raising one is a design
// decision that needs David's say-so, exactly like a `file-length` allowlist entry.
// Shrinking is always fine and never fails.

// guardedIndexCrates are the crates whose dependency trees must stay app-free.
var guardedIndexCrates = []string{"cmdr-index", "cmdr-fs"}

// forbiddenForIndexCrates are the packages that must not appear in a guarded
// crate's tree. `cmdr` is the app; `tauri` and `tauri-specta` are what would let
// the index talk to a host directly instead of through a seam. `specta` is
// deliberately absent: 58 data types derive `specta::Type` and the app is the only
// consumer, so it's a schema dependency on data, not a presentation one.
var forbiddenForIndexCrates = []string{"cmdr", "tauri", "tauri-specta"}

// surfaceCeilings caps each bucket of the public surface. See the audit for what
// justifies each number, item by item.
type surfaceCeilings struct {
	// RootPromises is what `lib.rs` exports: a `pub` there says "a host may rely on
	// this forever".
	RootPromises int
	// HandleMethods is the count of `pub fn` on `Index`. The plan's target was about
	// 25; the audit landed at 34 and justified the nine in writing.
	HandleMethods int
	// PublicModules is how many modules a host can name a path into, across the
	// whole crate.
	PublicModules int
	// SubsystemItems is every `pub` item inside those modules — the surface the
	// root re-exports don't capture, which is where `media_index` and `importance`
	// live.
	SubsystemItems int
}

// Measured 2026-07-31, and each number is where the audit landed rather than where
// the code happened to be. `HandleMethods` is 35 rather than the audit's headline 34
// because this count includes `Index::builder`, the constructor.
var indexCrateCeilings = surfaceCeilings{
	RootPromises:   44,
	HandleMethods:  35,
	PublicModules:  17,
	SubsystemItems: 159,
}

// ── The dependency graph ─────────────────────────────────────────────

type depKind int

const (
	depNormal depKind = iota
	depBuild
	depDev
)

// cargoEdge is one dependency of one package.
type cargoEdge struct {
	Name string
	Kind depKind
}

// cargoGraph is the resolved dependency graph, keyed by package name. Names rather
// than cargo's package ids, because a violation has to be readable and two versions
// of a forbidden crate are equally forbidden.
type cargoGraph struct {
	edges map[string][]cargoEdge
}

func newCargoGraph(edges map[string][]cargoEdge) *cargoGraph {
	if edges == nil {
		edges = map[string][]cargoEdge{}
	}
	return &cargoGraph{edges: edges}
}

func (g *cargoGraph) addEdge(from string, edge cargoEdge) {
	g.edges[from] = append(g.edges[from], edge)
}

func (g *cargoGraph) has(name string) bool {
	_, ok := g.edges[name]
	return ok
}

// isolationViolations reports every way a guarded crate reaches a forbidden one.
//
// Traversal rule: a guarded crate's DIRECT dependencies count whatever their kind
// (a dev-dependency on the app is precisely the reach the gated `testing` surface
// exists to make unnecessary, and cargo permits that cycle), but only normal and
// build edges are followed onward. A dev-dependency's own tree never links into the
// shipped crate, so following it would report violations that don't exist.
func isolationViolations(graph *cargoGraph, guarded []string, forbidden []string) []string {
	banned := make(map[string]bool, len(forbidden))
	for _, name := range forbidden {
		banned[name] = true
	}

	var violations []string
	for _, crate := range guarded {
		if !graph.has(crate) {
			violations = append(violations, fmt.Sprintf(
				"%q isn't in the workspace's dependency graph, so this check would guard nothing: "+
					"it was renamed or removed, and `guardedIndexCrates` has to follow", crate))
			continue
		}
		violations = append(violations, reachesForbidden(graph, crate, banned)...)
	}
	sort.Strings(violations)
	return violations
}

// reachesForbidden walks one guarded crate's tree and describes each forbidden crate
// it can reach, naming the path that brought it in.
func reachesForbidden(graph *cargoGraph, crate string, banned map[string]bool) []string {
	type step struct {
		name string
		path []string
		kind depKind
	}

	var found []string
	reported := map[string]bool{}
	seen := map[string]bool{crate: true}
	queue := []step{}
	for _, edge := range graph.edges[crate] {
		queue = append(queue, step{name: edge.Name, path: []string{crate, edge.Name}, kind: edge.Kind})
	}

	for len(queue) > 0 {
		current := queue[0]
		queue = queue[1:]

		if banned[current.name] && !reported[current.name] {
			reported[current.name] = true
			via := ""
			if len(current.path) > 2 {
				via = fmt.Sprintf(" (via %s)", strings.Join(current.path[1:len(current.path)-1], " → "))
			}
			if current.kind == depDev {
				found = append(found, fmt.Sprintf(
					"%s reaches %q as a dev-dependency%s; a test that needs the app is what the gated `testing` "+
						"surface is for", crate, current.name, via))
			} else {
				found = append(found, fmt.Sprintf(
					"%s depends on %q%s; the index must not be able to name it", crate, current.name, via))
			}
			continue
		}
		// A dev-dependency's own tree isn't the guarded crate's, so stop there.
		if current.kind == depDev || seen[current.name] {
			continue
		}
		seen[current.name] = true
		for _, edge := range graph.edges[current.name] {
			if edge.Kind == depDev {
				continue
			}
			queue = append(queue, step{name: edge.Name, path: append(append([]string{}, current.path...), edge.Name), kind: edge.Kind})
		}
	}
	return found
}

// cargoMetadataJSON is the slice of `cargo metadata` output this check reads.
type cargoMetadataJSON struct {
	Packages []struct {
		ID   string `json:"id"`
		Name string `json:"name"`
	} `json:"packages"`
	Resolve struct {
		Nodes []struct {
			ID   string `json:"id"`
			Deps []struct {
				Pkg      string `json:"pkg"`
				DepKinds []struct {
					Kind string `json:"kind"`
				} `json:"dep_kinds"`
			} `json:"deps"`
		} `json:"nodes"`
	} `json:"resolve"`
}

// readCargoGraph runs `cargo metadata` and folds it into a name-keyed graph.
// `--all-features` on purpose: a `tauri` dependency hidden behind an off-by-default
// feature is still a dependency someone can turn on, and the whole point of a
// machine check is that it doesn't depend on which configuration happened to build.
func readCargoGraph(rootDir string) (*cargoGraph, error) {
	cmd := exec.Command("cargo", "metadata", "--format-version", "1", "--all-features", "--locked")
	cmd.Dir = rootDir
	output, err := cmd.Output()
	if err != nil {
		return nil, fmt.Errorf("couldn't read the cargo dependency graph: %w", err)
	}
	var meta cargoMetadataJSON
	if err := json.Unmarshal(output, &meta); err != nil {
		return nil, fmt.Errorf("couldn't parse `cargo metadata` output: %w", err)
	}

	nameByID := make(map[string]string, len(meta.Packages))
	for _, pkg := range meta.Packages {
		nameByID[pkg.ID] = pkg.Name
	}

	graph := newCargoGraph(nil)
	for _, node := range meta.Resolve.Nodes {
		from := nameByID[node.ID]
		if from == "" {
			continue
		}
		if _, ok := graph.edges[from]; !ok {
			graph.edges[from] = nil
		}
		for _, dep := range node.Deps {
			to := nameByID[dep.Pkg]
			if to == "" || to == from {
				continue
			}
			for _, kind := range dep.DepKinds {
				graph.addEdge(from, cargoEdge{Name: to, Kind: parseDepKind(kind.Kind)})
			}
		}
	}
	return graph, nil
}

func parseDepKind(kind string) depKind {
	switch kind {
	case "dev":
		return depDev
	case "build":
		return depBuild
	default:
		return depNormal
	}
}

// ── The public-item ceiling ──────────────────────────────────────────

// surfaceCounts is what the crate currently promises, by bucket.
type surfaceCounts struct {
	RootPromises   int
	HandleMethods  int
	PublicModules  int
	SubsystemItems int
	// Gated is the `testing` / `tooling` surface, counted apart because it isn't
	// the API: it exists so a consumer's tests and the measurement binaries have
	// one door instead of a widened module.
	Gated int
}

func ceilingBreaches(counts surfaceCounts, ceilings surfaceCeilings) []string {
	var problems []string
	check := func(bucket string, got, cap int, remedy string) {
		if got > cap {
			problems = append(problems, fmt.Sprintf(
				"%s: %d, over the ceiling of %d. %s", bucket, got, cap, remedy))
		}
	}
	check("root promises in `lib.rs`", counts.RootPromises, ceilings.RootPromises,
		"A `pub` there is a promise a host may rely on forever.")
	check("public methods on the `Index` handle", counts.HandleMethods, ceilings.HandleMethods,
		"Reach for a facade method, a fold into an existing call, or the `testing` gate first.")
	check("public modules", counts.PublicModules, ceilings.PublicModules,
		"A `pub mod` exposes everything `pub` inside it, including items nobody meant to promise.")
	check("public items in those modules", counts.SubsystemItems, ceilings.SubsystemItems,
		"Check the item against the four dispositions in the audit before widening it.")
	return problems
}

// countSurface walks the crate's public module tree and counts each bucket.
//
// Source-level rather than rustdoc JSON: rustdoc's JSON output is nightly-only, and
// a check that needs a second toolchain is a check CI skips. What's counted is
// deliberately coarse — the number has to be stable and has to MOVE when the surface
// does, which a line-shaped count over rustfmt-normalized source does reliably.
//
// `files` maps a crate-root-relative path to that file's contents.
func countSurface(files map[string]string, rootFile string) surfaceCounts {
	var counts surfaceCounts
	publicTypes := map[string]bool{}

	// Walk the module tree, following only publicly-reachable declarations.
	type modRef struct {
		file   string
		isRoot bool
	}
	queue := []modRef{{file: rootFile, isRoot: true}}
	visited := map[string]bool{rootFile: true}
	var publicFiles []string

	for len(queue) > 0 {
		current := queue[0]
		queue = queue[1:]
		source, ok := files[current.file]
		if !ok {
			continue
		}
		if !current.isRoot {
			counts.PublicModules++
		}
		publicFiles = append(publicFiles, current.file)

		for _, decl := range publicModDecls(source) {
			child := resolveModFile(files, current.file, decl)
			if child == "" || visited[child] {
				continue
			}
			visited[child] = true
			queue = append(queue, modRef{file: child})
		}
	}

	sort.Strings(publicFiles)
	for _, file := range publicFiles {
		for _, name := range publicTypeNames(files[file]) {
			publicTypes[name] = true
		}
	}

	for _, file := range publicFiles {
		source := files[file]
		items, gated := countModuleItems(source, file == rootFile)
		counts.Gated += gated
		if file == rootFile {
			counts.RootPromises += items
		} else {
			counts.SubsystemItems += items
		}
		counts.SubsystemItems += countInherentMethods(source, publicTypes, file == rootFile)
	}

	// `Index`'s methods are also subsystem items; counting them in their own bucket
	// is what pins Decision 3's target rather than letting it hide in a total.
	for _, source := range files {
		counts.HandleMethods += countMethodsOn(source, "Index")
	}
	return counts
}

// publicModDecls returns the module names a file declares as `pub mod`, skipping
// `pub(crate) mod`, plain `mod`, and anything behind a `testing` / `tooling` gate.
func publicModDecls(source string) []string {
	var names []string
	lines := strings.Split(source, "\n")
	for i, line := range lines {
		if !strings.HasPrefix(line, "pub mod ") {
			continue
		}
		if isGated(lines, i) {
			continue
		}
		name := strings.TrimSuffix(strings.TrimSpace(strings.TrimPrefix(line, "pub mod ")), ";")
		if name != "" && !strings.Contains(name, "{") {
			names = append(names, name)
		}
	}
	return names
}

// resolveModFile maps a `pub mod X` inside `parent` to `X.rs` or `X/mod.rs`.
func resolveModFile(files map[string]string, parent, name string) string {
	dir := filepath.Dir(parent)
	if strings.HasSuffix(parent, "/mod.rs") || parent == "lib.rs" || !strings.Contains(parent, "/") {
		if parent == "lib.rs" {
			dir = "."
		}
	} else {
		// A non-`mod.rs` file's children live in a directory named after it.
		dir = strings.TrimSuffix(parent, ".rs")
	}
	for _, candidate := range []string{name + ".rs", name + "/mod.rs"} {
		full := candidate
		if dir != "." && dir != "" {
			full = dir + "/" + candidate
		}
		if _, ok := files[full]; ok {
			return full
		}
	}
	return ""
}

var publicItemPrefixes = []string{
	"pub fn ", "pub async fn ", "pub unsafe fn ", "pub extern ",
	"pub struct ", "pub enum ", "pub trait ", "pub union ",
	"pub const ", "pub static ", "pub type ",
}

// countModuleItems counts a file's module-level `pub` items and `pub use` names,
// returning the ungated count and the gated one separately.
//
// `countMods` is set only for the crate root, where a `pub mod` IS one of the names
// a host may write. Everywhere else modules live in their own bucket, so counting
// them here would double them.
func countModuleItems(source string, countMods bool) (items, gated int) {
	lines := strings.Split(source, "\n")
	for i, line := range lines {
		n := 0
		switch {
		case strings.HasPrefix(line, "pub use "):
			n = countUseTreeLeaves(lines, i)
		case countMods && strings.HasPrefix(line, "pub mod "):
			n = 1
		case hasAnyPrefix(line, publicItemPrefixes):
			n = 1
		default:
			continue
		}
		if isGated(lines, i) {
			gated += n
		} else {
			items += n
		}
	}
	return items, gated
}

// countUseTreeLeaves counts the names one `pub use` brings in, following a braced
// list across lines. `pub use a::b::{c, d};` is two promises, not one.
func countUseTreeLeaves(lines []string, start int) int {
	statement := lines[start]
	for i := start; !strings.Contains(statement, ";") && i+1 < len(lines); i++ {
		statement += " " + strings.TrimSpace(lines[i+1])
	}
	open := strings.Index(statement, "{")
	if open < 0 {
		return 1
	}
	close := strings.LastIndex(statement, "}")
	if close < open {
		return 1
	}
	inner := statement[open+1 : close]
	count := 0
	for _, part := range strings.Split(inner, ",") {
		if strings.TrimSpace(part) != "" {
			count++
		}
	}
	if count == 0 {
		return 1
	}
	return count
}

// publicTypeNames returns the names a file declares as public types, so an `impl`
// block's methods can be attributed to something a host can actually name.
func publicTypeNames(source string) []string {
	var names []string
	for _, line := range strings.Split(source, "\n") {
		for _, prefix := range []string{"pub struct ", "pub enum ", "pub trait ", "pub union ", "pub type "} {
			if !strings.HasPrefix(line, prefix) {
				continue
			}
			name := strings.TrimPrefix(line, prefix)
			name = strings.TrimSpace(strings.FieldsFunc(name, func(r rune) bool {
				return r == '<' || r == '{' || r == '(' || r == ' ' || r == ';' || r == '='
			})[0])
			if name != "" {
				names = append(names, name)
			}
		}
	}
	return names
}

// countInherentMethods counts `pub fn` / `pub const` in inherent `impl` blocks for
// types a host can name. Trait impls are skipped: their methods are the trait's
// surface, not a new promise. `skipIndex` keeps the handle's methods out of the
// subsystem bucket, where they'd be counted twice.
func countInherentMethods(source string, publicTypes map[string]bool, skipIndex bool) int {
	total := 0
	for name := range publicTypes {
		if skipIndex && name == "Index" {
			continue
		}
		if name == "Index" {
			continue // counted in its own bucket
		}
		total += countMethodsOn(source, name)
	}
	return total
}

// countMethodsOn counts the public associated items in `impl <typeName>` blocks in
// one file. Block extent comes from column-0 `impl` … column-0 `}`, which holds
// because the tree is rustfmt-normalized.
func countMethodsOn(source, typeName string) int {
	lines := strings.Split(source, "\n")
	count := 0
	inBlock := false
	for i, line := range lines {
		if !inBlock {
			if isInherentImplOf(line, typeName) {
				inBlock = true
			}
			continue
		}
		if line == "}" {
			inBlock = false
			continue
		}
		trimmed := strings.TrimSpace(line)
		if !strings.HasPrefix(line, "    ") {
			continue
		}
		if hasAnyPrefix(trimmed, []string{"pub fn ", "pub async fn ", "pub unsafe fn ", "pub const fn ", "pub const "}) &&
			!isGated(lines, i) {
			count++
		}
	}
	return count
}

// isInherentImplOf reports whether the line opens an inherent `impl` for typeName.
// `impl Trait for Type` is not one: those methods belong to the trait.
func isInherentImplOf(line, typeName string) bool {
	if !strings.HasPrefix(line, "impl") {
		return false
	}
	if strings.Contains(line, " for ") {
		return false
	}
	head := strings.TrimPrefix(line, "impl")
	// Drop a generic parameter list: `impl<T> Foo<T> {`.
	if strings.HasPrefix(head, "<") {
		depth := 0
		for i, r := range head {
			switch r {
			case '<':
				depth++
			case '>':
				depth--
				if depth == 0 {
					head = head[i+1:]
					goto trimmed
				}
			}
		}
		return false
	}
trimmed:
	head = strings.TrimSpace(head)
	head = strings.TrimSuffix(strings.TrimSpace(strings.TrimSuffix(head, "{")), " ")
	if idx := strings.IndexAny(head, "<"); idx >= 0 {
		head = head[:idx]
	}
	head = strings.TrimSpace(head)
	// `impl super::Index` and `impl Index` are the same block to a host.
	if idx := strings.LastIndex(head, "::"); idx >= 0 {
		head = head[idx+2:]
	}
	return head == typeName
}

// isGated reports whether the item at `index` sits under a `testing` / `tooling`
// feature gate or a `cfg(test)`, walking back over the contiguous attribute and doc
// lines above it.
func isGated(lines []string, index int) bool {
	indent := len(lines[index]) - len(strings.TrimLeft(lines[index], " "))
	for i := index - 1; i >= 0; i-- {
		trimmed := strings.TrimSpace(lines[i])
		if trimmed == "" {
			return false
		}
		if strings.HasPrefix(trimmed, "///") || strings.HasPrefix(trimmed, "//") {
			continue
		}
		if !strings.HasPrefix(trimmed, "#[") && !strings.HasPrefix(trimmed, "#!") {
			return false
		}
		if len(lines[i])-len(strings.TrimLeft(lines[i], " ")) != indent {
			return false
		}
		if strings.Contains(trimmed, `feature = "testing"`) ||
			strings.Contains(trimmed, `feature = "tooling"`) ||
			strings.Contains(trimmed, "cfg(test)") {
			return true
		}
	}
	return false
}

// readCrateSources loads every `.rs` file under a crate's `src/`, keyed by its path
// relative to that directory.
func readCrateSources(srcDir string) (map[string]string, error) {
	files := map[string]string{}
	err := filepath.WalkDir(srcDir, func(path string, entry os.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if entry.IsDir() || !strings.HasSuffix(path, ".rs") {
			return nil
		}
		rel, relErr := filepath.Rel(srcDir, path)
		if relErr != nil {
			return relErr
		}
		content, readErr := os.ReadFile(path)
		if readErr != nil {
			return readErr
		}
		files[filepath.ToSlash(rel)] = string(content)
		return nil
	})
	return files, err
}

// RunIndexCrateIsolation enforces both halves of the index crates' boundary.
func RunIndexCrateIsolation(ctx *CheckContext) (CheckResult, error) {
	graph, err := readCargoGraph(ctx.RootDir)
	if err != nil {
		return CheckResult{}, err
	}
	problems := isolationViolations(graph, guardedIndexCrates, forbiddenForIndexCrates)

	srcDir := filepath.Join(ctx.RootDir, "crates", "cmdr-index", "src")
	files, err := readCrateSources(srcDir)
	if err != nil {
		return CheckResult{}, fmt.Errorf("couldn't read `cmdr-index`'s sources: %w", err)
	}
	if _, ok := files["lib.rs"]; !ok {
		return CheckResult{}, fmt.Errorf("`%s/lib.rs` is missing, so the public surface can't be counted", srcDir)
	}
	counts := countSurface(files, "lib.rs")
	problems = append(problems, ceilingBreaches(counts, indexCrateCeilings)...)

	if len(problems) > 0 {
		return CheckResult{}, fmt.Errorf(
			"%d index-boundary %s:\n  %s\n\nThe audit behind the surface is `crates/cmdr-index/src/indexing/handle/DETAILS.md`; "+
				"raising a ceiling needs David's explicit say-so, like a `file-length` allowlist entry",
			len(problems), Pluralize(len(problems), "problem", "problems"), strings.Join(problems, "\n  "))
	}

	return Success(fmt.Sprintf(
		"no tauri in %s; %d root promises, %d handle methods, %d public modules, %d items in them (+%d gated)",
		strings.Join(guardedIndexCrates, " or "),
		counts.RootPromises, counts.HandleMethods, counts.PublicModules, counts.SubsystemItems, counts.Gated)), nil
}
