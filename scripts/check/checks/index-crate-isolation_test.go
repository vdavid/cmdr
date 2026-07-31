package checks

import (
	"strings"
	"testing"
)

// A fixture graph shaped like the real one: the app depends on the index, the index
// depends on the leaf crate, and nothing points back. `serde` stands in for an
// ordinary shared dependency, which must never read as a violation.
func cleanFixture() *cargoGraph {
	return newCargoGraph(map[string][]cargoEdge{
		"cmdr":       {{Name: "cmdr-index", Kind: depNormal}, {Name: "tauri", Kind: depNormal}},
		"cmdr-index": {{Name: "cmdr-fs", Kind: depNormal}, {Name: "serde", Kind: depNormal}},
		"cmdr-fs":    {{Name: "serde", Kind: depNormal}},
		"tauri":      {{Name: "serde", Kind: depNormal}},
		"serde":      nil,
	})
}

func TestIsolationPassesWhenTheGuardedCratesStayClean(t *testing.T) {
	violations := isolationViolations(cleanFixture(), []string{"cmdr-index", "cmdr-fs"}, forbiddenForIndexCrates)
	if len(violations) != 0 {
		t.Fatalf("a clean graph must report nothing, got %v", violations)
	}
}

func TestIsolationCatchesADirectTauriDependency(t *testing.T) {
	graph := cleanFixture()
	graph.addEdge("cmdr-index", cargoEdge{Name: "tauri", Kind: depNormal})

	violations := isolationViolations(graph, []string{"cmdr-index", "cmdr-fs"}, forbiddenForIndexCrates)
	if len(violations) != 1 {
		t.Fatalf("a manifest that depends on tauri must be caught, got %v", violations)
	}
	if !strings.Contains(violations[0], "cmdr-index") || !strings.Contains(violations[0], "tauri") {
		t.Fatalf("the violation must name the crate and the dependency, got %q", violations[0])
	}
}

func TestIsolationCatchesTauriArrivingTransitively(t *testing.T) {
	// The creep this check exists for: nobody writes `tauri` into the index's
	// manifest, they add a small helper crate that happens to pull it in.
	graph := cleanFixture()
	graph.addEdge("cmdr-index", cargoEdge{Name: "handy-helper", Kind: depNormal})
	graph.addEdge("handy-helper", cargoEdge{Name: "tauri-specta", Kind: depNormal})

	violations := isolationViolations(graph, []string{"cmdr-index", "cmdr-fs"}, forbiddenForIndexCrates)
	if len(violations) != 1 {
		t.Fatalf("a transitive reach must be caught, got %v", violations)
	}
	if !strings.Contains(violations[0], "handy-helper") {
		t.Fatalf("the violation must name the path that brought it in, got %q", violations[0])
	}
}

func TestIsolationCatchesADevDependencyBackOnTheApp(t *testing.T) {
	// Cargo permits a dev-dependency cycle, so `cmdr-index` CAN dev-depend on
	// `cmdr` and compile. That's exactly the reach the gated `testing` surface
	// exists to make unnecessary, so it has to fail here.
	graph := cleanFixture()
	graph.addEdge("cmdr-index", cargoEdge{Name: "cmdr", Kind: depDev})

	violations := isolationViolations(graph, []string{"cmdr-index", "cmdr-fs"}, forbiddenForIndexCrates)
	if len(violations) != 1 {
		t.Fatalf("a dev-dependency on the app must be caught, got %v", violations)
	}
	if !strings.Contains(violations[0], "dev-dependency") {
		t.Fatalf("the violation must say it came in as a dev-dependency, got %q", violations[0])
	}
}

func TestIsolationIgnoresWhatADevDependencyItselfPullsIn(t *testing.T) {
	// A dev-dependency's own tree never links into the shipped crate, so following
	// it transitively would report violations that don't exist.
	graph := cleanFixture()
	graph.addEdge("cmdr-index", cargoEdge{Name: "test-harness", Kind: depDev})
	graph.addEdge("test-harness", cargoEdge{Name: "tauri", Kind: depNormal})

	violations := isolationViolations(graph, []string{"cmdr-index", "cmdr-fs"}, forbiddenForIndexCrates)
	if len(violations) != 0 {
		t.Fatalf("a dev-dependency's own tree isn't the crate's, got %v", violations)
	}
}

func TestIsolationReportsAGuardedCrateThatIsNotInTheGraph(t *testing.T) {
	// A renamed or removed crate must fail loudly. Silently checking nothing is
	// how this check would pass forever over a boundary that no longer exists.
	violations := isolationViolations(cleanFixture(), []string{"cmdr-index", "cmdr-ghost"}, forbiddenForIndexCrates)
	if len(violations) != 1 || !strings.Contains(violations[0], "cmdr-ghost") {
		t.Fatalf("a missing guarded crate must be reported, got %v", violations)
	}
}

// ── The public-item ceiling ──────────────────────────────────────────

func TestCeilingCountsTheHandleMethodsAndRootPromises(t *testing.T) {
	source := map[string]string{
		"lib.rs": `
pub mod host;

/// Docs.
pub use indexing::handle::{Index, IndexError};
pub use indexing::store::IndexFailure;

#[cfg(any(test, feature = "testing"))]
pub use indexing::testing;
`,
		"indexing/handle/mod.rs": `
pub struct Index {}

impl Index {
    pub fn start_volume(&self) {}
    pub fn stop_volume(&self) {}
    fn private_helper(&self) {}
    pub(crate) fn crate_only(&self) {}
}

impl std::fmt::Debug for Index {
    pub fn fmt(&self) {}
}
`,
	}
	counts := countSurface(source, "lib.rs")
	if counts.RootPromises != 4 {
		t.Fatalf("root promises: want 4 (host, Index, IndexError, IndexFailure), got %d", counts.RootPromises)
	}
	if counts.HandleMethods != 2 {
		t.Fatalf("handle methods: want 2, got %d", counts.HandleMethods)
	}
	if counts.Gated != 1 {
		t.Fatalf("the gated surface is counted apart: want 1, got %d", counts.Gated)
	}
}

func TestCeilingFollowsPublicModulesAndStopsAtCrateOnlyOnes(t *testing.T) {
	source := map[string]string{
		"lib.rs": `
pub mod media_index;
`,
		"media_index/mod.rs": `
pub mod read;
pub(crate) mod writer;
mod private;

pub use read::MediaIndex;
`,
		"media_index/read.rs": `
pub struct MediaIndex {}

impl MediaIndex {
    pub fn open() {}
}

pub fn helper() {}
`,
		"media_index/writer.rs": `
pub fn never_reachable() {}
pub struct AlsoNotReachable {}
`,
		"media_index/private.rs": `
pub fn also_never() {}
`,
	}
	counts := countSurface(source, "lib.rs")
	// `media_index` + `media_index::read` are public; the other two aren't.
	if counts.PublicModules != 2 {
		t.Fatalf("public modules: want 2, got %d", counts.PublicModules)
	}
	// `MediaIndex` (struct), `MediaIndex::open`, `helper`, and the `pub use` of
	// `MediaIndex`. Nothing from `writer` or `private`.
	if counts.SubsystemItems != 4 {
		t.Fatalf("subsystem items: want 4, got %d", counts.SubsystemItems)
	}
}

func TestCeilingFailsOnGrowthAndNotesShrink(t *testing.T) {
	over := surfaceCounts{RootPromises: 5, HandleMethods: 40, PublicModules: 10, SubsystemItems: 90}
	problems := ceilingBreaches(over, surfaceCeilings{RootPromises: 5, HandleMethods: 34, PublicModules: 10, SubsystemItems: 90})
	if len(problems) != 1 || !strings.Contains(problems[0], "handle") {
		t.Fatalf("growth past a ceiling must fail and name the bucket, got %v", problems)
	}

	exact := surfaceCounts{RootPromises: 5, HandleMethods: 34, PublicModules: 10, SubsystemItems: 90}
	if problems := ceilingBreaches(exact, surfaceCeilings{RootPromises: 5, HandleMethods: 34, PublicModules: 10, SubsystemItems: 90}); len(problems) != 0 {
		t.Fatalf("sitting exactly at the ceiling is fine, got %v", problems)
	}

	under := surfaceCounts{RootPromises: 4, HandleMethods: 30, PublicModules: 9, SubsystemItems: 80}
	if problems := ceilingBreaches(under, surfaceCeilings{RootPromises: 5, HandleMethods: 34, PublicModules: 10, SubsystemItems: 90}); len(problems) != 0 {
		t.Fatalf("shrinking is never a failure, got %v", problems)
	}
}
