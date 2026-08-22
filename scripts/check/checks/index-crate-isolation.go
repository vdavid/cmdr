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

// The app-free crates' boundary, made self-defending.
//
// `cmdr-index`, `cmdr-fs`, `cmdr-archive`, and `cmdr-smb` exist so the app can't reach
// into their internals and a change inside one can't have an unbounded blast radius. Two
// properties carry that, and both erode the same way — one convenient addition at a
// time, each individually reasonable:
//
//  1. **No `tauri` in the dependency tree.** The moment the index can name
//     `AppHandle`, every seam in `indexing/host/` is optional and the boundary is a
//     convention again. Machine-checkable against `cargo metadata`, which is the
//     point: the compiler already enforces "no reach into the app", but nothing
//     stops someone adding the dependency that would make the reach legal.
//  2. **A bounded public surface.** Making a subsystem a crate means everything the
//     app touches has to be `pub`, so the honest failure mode is 65 `pub(crate)`
//     items quietly becoming 65 `pub` items: the build-time split lands and the
//     encapsulation is abandoned, with everything still green. The item-by-item
//     audit behind the index's shape is
//     `crates/cmdr-index/src/indexing/handle/DETAILS.md`; this check is what stops
//     the next `pub` from being added without going through it.
//
// The ceilings below are what each crate actually exposes today, with no headroom.
// Raising one is a design decision that needs David's say-so, exactly like a
// `file-length` allowlist entry. Shrinking is always fine and never fails.

// guardedIndexCrates are the crates whose dependency trees must stay app-free.
var guardedIndexCrates = []string{"cmdr-index", "cmdr-fs", "cmdr-archive", "cmdr-smb", "cmdr-sftp"}

// forbiddenForIndexCrates are the packages that must not appear in a guarded
// crate's tree. `cmdr` is the app; `tauri` and `tauri-specta` are what would let
// the index talk to a host directly instead of through a seam. `specta` is
// deliberately absent: 58 data types derive `specta::Type` and the app is the only
// consumer, so it's a schema dependency on data, not a presentation one.
var forbiddenForIndexCrates = []string{"cmdr", "tauri", "tauri-specta"}

// surfaceCeilings caps each bucket of the public surface. See each crate's entry in
// surfaceGuardedCrates for what justifies its numbers.
type surfaceCeilings struct {
	// RootPromises is what `lib.rs` exports: a `pub` there says "a host may rely on
	// this forever".
	RootPromises int
	// HandleMethods is the count of `pub fn` on the crate's one handle type, when it
	// has one. Zero (with an empty HandleType) means the crate has no such type.
	HandleMethods int
	// PublicModules is how many modules a host can name a path into, across the
	// whole crate.
	PublicModules int
	// SubsystemItems is every `pub` item inside those modules — the surface the
	// root re-exports don't capture.
	SubsystemItems int
}

// surfaceGuardedCrates are the guarded crates whose public surface is ALSO capped.
// Not every guarded crate is: `cmdr-fs` is deliberately absent, because it's shared
// vocabulary whose whole job is to be named from everywhere, so a count of its `pub`
// items would measure the wrong thing, and `cmdr-sftp`'s is still growing.
//
// HandleType names the one type whose methods get their own bucket, or is empty when
// the crate has no such type. A backend crate doesn't: its API is the `Volume` trait
// it implements, whose methods aren't a promise of its own.
var surfaceGuardedCrates = []struct {
	Name       string
	HandleType string
	Ceilings   surfaceCeilings
}{
	{
		Name: "cmdr-index",
		// Measured 2026-07-31, and each number is where the item-by-item audit
		// landed rather than where the code happened to be. `HandleMethods` is 35
		// rather than the audit's headline 34 because this count includes
		// `Index::builder`, the constructor.
		//
		// Raised once on 2026-08-05, with David's say-so, for ONE new concept:
		// COVERAGE — what the index can't answer for yet, and walking the rest.
		// He asked for the whole surface to be designed together and the ceilings
		// raised to match, rather than a bump per method, so both numbers below
		// carry the full concept:
		//
		//   - `RootPromises` 44 → 47: `CoverageMap` (the answer), `CoverageToken`
		//     (which state of the index it describes), `CoverageDimension` (the
		//     forward-compat axis content search will add itself to).
		//   - `HandleMethods` 35 → 38: `Index::coverage` and
		//     `Index::coverage_token`, both landed, plus ONE reserved slot for
		//     `Index::cover`, the walk half — it takes the frontier a coverage
		//     answer named and fills it in. That slot is spoken for; anything else
		//     arriving in it still has to be argued the same way these were.
		//
		// `Index::cover` landed on 2026-08-05 into its reserved slot, and brought
		// the three types its answer is made of. `RootPromises` 47 → 50:
		//
		//   - `CoverWalk` — the running walk: take batches off it, cancel it,
		//     finish it. A host can't drive a walk without a handle to it.
		//   - `CoveredEntry` — one entry the walk found, in the shape a result row
		//     needs. Decision 3 puts the matching host-side, so this type crossing
		//     the boundary is the whole point: it's what keeps a matcher out of
		//     this crate.
		//   - `CoverOutcome` — what the walk covered, and whether it was cancelled.
		//     The terminal state a search's UI phase reads.
		//
		// Nothing else is owed to the coverage concept. A fourth type here needs
		// the same argument these three did.
		//
		// Raised again on 2026-08-05, with David's say-so, for ONE more concept:
		// WHAT THE INDEX OCCUPIES ON DISK, and dropping all of it. Once a search
		// walks, a machine that indexes nothing still accumulates databases, and
		// the settings screen has to be able to show and reclaim them.
		// `HandleMethods` 38 → 40, and no new root promise (both answer in types
		// the crate already promises):
		//
		//   - `Index::disk_footprint` — the bytes every index database occupies,
		//     read off the FILES rather than the registry, which can't see the
		//     database a walk built and nothing re-registered after a restart.
		//   - `Index::forget_all_volumes` — the whole-index sibling of
		//     `forget_volume`, reaching those same unregistered databases.
		//
		// The concept is closed: measuring and clearing is all of it.
		//
		// Raised again on 2026-08-15, with David's say-so, for ONE item:
		// `CoveragePhase`, which phase of a drive's first index is running.
		// `RootPromises` 50 -> 51. The index owns the order and the path space that
		// classifies a root into it, so a host re-deriving the phase from that root
		// would need its own idea of firmlinks: right on one machine, wrong on the
		// next. It rides one event variant and the status response, which is what lets
		// a window that reloaded mid-index name the running phase instead of waiting
		// out the next boundary. Nothing else is owed to the concept: what each phase
		// is CALLED is the host's, and a second type here needs the same argument this
		// one got.
		//
		// ⚠️ WHICH BUCKET a grant lands in is not a choice, so read the right counter
		// before assuming you have headroom. `SubsystemItems` counts `pub` items in the
		// modules this walk can REACH, and it reaches them from `pub mod` declarations
		// in `lib.rs` — for `cmdr-index` that is `importance` and `media_index`, and
		// nothing else. `indexing` is a private module, so everything a host can name
		// from it arrives as a `pub use` in `lib.rs` and counts as a ROOT PROMISE. A
		// value an event carries has one sane home, `indexing/events/payload.rs` beside
		// `ScanRunKind` and `ActivityPhase` (anywhere else makes the event envelope
		// import its own parent), so a new payload enum ALWAYS spends a root promise.
		// A grant of "one item" for one of those is this line moving by one, and the
		// three counters below staying put.
		HandleType: "Index",
		Ceilings: surfaceCeilings{
			RootPromises:   51,
			HandleMethods:  40,
			PublicModules:  17,
			SubsystemItems: 156,
		},
	},
	{
		// Measured 2026-08-22, at the extraction, with no headroom. What each item is
		// for: `crates/cmdr-smb/DETAILS.md` § "The public surface is capped".
		Name: "cmdr-smb",
		Ceilings: surfaceCeilings{
			RootPromises:   15,
			PublicModules:  4,
			SubsystemItems: 18,
		},
	},
	{
		// Measured 2026-08-03, at the extraction, and set to exactly what the crate
		// exposes — no headroom, so the first addition has to be argued for.
		//
		// A backend's API is the `Volume` trait it implements, which is `cmdr-fs`'s
		// promise rather than this crate's, so everything counted here exists for one
		// of exactly three callers, and a new item should name which:
		//
		//   - the boundary detector the host routes with (`boundary`, 9 of the root
		//     promises),
		//   - the reading core the host's archive-edit driver and file viewer parse
		//     and stream through (`read`, 23 of them),
		//   - `ArchiveVolume` itself, plus `mutator` and `active_watch_count`.
		//
		// Four public modules is the whole module tree a host can name a path into:
		// `boundary`, `read`, `volume`, `watch`. `mutation` is private and reaches the
		// root only as the `mutator` re-export; `test_fixtures` is `testing`-gated and
		// counts apart.
		Name: "cmdr-archive",
		Ceilings: surfaceCeilings{
			RootPromises:   35,
			PublicModules:  4,
			SubsystemItems: 36,
		},
	},
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

func ceilingBreaches(crate string, counts surfaceCounts, ceilings surfaceCeilings, handleType string) []string {
	var problems []string
	check := func(bucket string, got, cap int, remedy string) {
		if got > cap {
			problems = append(problems, fmt.Sprintf(
				"%s: %s is %d, over the ceiling of %d. %s", crate, bucket, got, cap, remedy))
		}
	}
	check("root promises in `lib.rs`", counts.RootPromises, ceilings.RootPromises,
		"A `pub` there is a promise a host may rely on forever.")
	if handleType != "" {
		check(fmt.Sprintf("public methods on the `%s` handle", handleType), counts.HandleMethods, ceilings.HandleMethods,
			"Reach for a facade method, a fold into an existing call, or the `testing` gate first.")
	}
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
func countSurface(files map[string]string, rootFile, handleType string) surfaceCounts {
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
		counts.SubsystemItems += countInherentMethods(source, publicTypes, handleType)
	}

	// The handle's methods are also subsystem items; counting them in their own
	// bucket is what pins the index's target rather than letting it hide in a total.
	// A crate with no handle type skips this and keeps every method in the subsystem
	// bucket.
	if handleType != "" {
		for _, source := range files {
			counts.HandleMethods += countMethodsOn(source, handleType)
		}
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
// surface, not a new promise. `handleType`, when the crate has one, is skipped here
// because it's counted in its own bucket.
func countInherentMethods(source string, publicTypes map[string]bool, handleType string) int {
	total := 0
	for name := range publicTypes {
		if handleType != "" && name == handleType {
			continue
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

// RunIndexCrateIsolation enforces both halves of the app-free crates' boundary.
func RunIndexCrateIsolation(ctx *CheckContext) (CheckResult, error) {
	graph, err := readCargoGraph(ctx.RootDir)
	if err != nil {
		return CheckResult{}, err
	}
	problems := isolationViolations(graph, guardedIndexCrates, forbiddenForIndexCrates)

	var summaries []string
	for _, crate := range surfaceGuardedCrates {
		srcDir := filepath.Join(ctx.RootDir, "crates", crate.Name, "src")
		files, err := readCrateSources(srcDir)
		if err != nil {
			return CheckResult{}, fmt.Errorf("couldn't read `%s`'s sources: %w", crate.Name, err)
		}
		if _, ok := files["lib.rs"]; !ok {
			return CheckResult{}, fmt.Errorf("`%s/lib.rs` is missing, so the public surface can't be counted", srcDir)
		}
		counts := countSurface(files, "lib.rs", crate.HandleType)
		problems = append(problems, ceilingBreaches(crate.Name, counts, crate.Ceilings, crate.HandleType)...)
		summary := fmt.Sprintf("%s: %d root promises", crate.Name, counts.RootPromises)
		if crate.HandleType != "" {
			summary += fmt.Sprintf(", %d handle methods", counts.HandleMethods)
		}
		summaries = append(summaries, fmt.Sprintf("%s, %d public modules, %d items in them (+%d gated)",
			summary, counts.PublicModules, counts.SubsystemItems, counts.Gated))
	}

	if len(problems) > 0 {
		return CheckResult{}, fmt.Errorf(
			"%d crate-boundary %s:\n  %s\n\nThe audit behind the index's surface is `crates/cmdr-index/src/indexing/handle/DETAILS.md`; "+
				"raising a ceiling needs David's explicit say-so, like a `file-length` allowlist entry",
			len(problems), Pluralize(len(problems), "problem", "problems"), strings.Join(problems, "\n  "))
	}

	return Success(fmt.Sprintf("no tauri in %s; %s",
		strings.Join(guardedIndexCrates, ", "), strings.Join(summaries, "; "))), nil
}
