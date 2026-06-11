package checks

import (
	"os"
	"path/filepath"
	"reflect"
	"testing"
)

// writeDocFile creates rootDir/relPath (with parents) holding content.
func writeDocFile(t *testing.T, rootDir, relPath, content string) {
	t.Helper()
	full := filepath.Join(rootDir, filepath.FromSlash(relPath))
	if err := os.MkdirAll(filepath.Dir(full), 0o755); err != nil {
		t.Fatalf("mkdir for %s: %v", relPath, err)
	}
	if err := os.WriteFile(full, []byte(content), 0o644); err != nil {
		t.Fatalf("write %s: %v", relPath, err)
	}
}

// buildFixtureGraph lays out a small doc tree and returns its analyzed graph.
//
//	CLAUDE.md            the root: @imports AGENTS.md (mirrors the real entry point)
//	AGENTS.md            mentions docs/guide.md (file ref) and `sub/` (dir ref)
//	docs/guide.md        reachable via file ref, and links docs/notes/note.md
//	docs/orphan.md       mentioned nowhere -> orphan
//	docs/notes/note.md   under docs/, now enforced: reachable here (linked from guide)
//	sub/CLAUDE.md        reachable via the `sub/` directory reference
//	sub/DETAILS.md       linked from sub/CLAUDE.md, and links back (cycle)
//	lonely/CLAUDE.md     its dir is never mentioned -> orphan
func buildFixtureGraph(t *testing.T) *DocGraph {
	t.Helper()
	root := t.TempDir()
	writeDocFile(t, root, "CLAUDE.md", "@AGENTS.md")
	writeDocFile(t, root, "AGENTS.md", "See [the guide](docs/guide.md), the `sub/` subsystem, and the `(main)/` route.")
	writeDocFile(t, root, "routes/(main)/CLAUDE.md", "A SvelteKit group dir, reached via the `(main)/` dir reference.")
	writeDocFile(t, root, "docs/guide.md", "A reachable guide. See [a note](notes/note.md).")
	writeDocFile(t, root, "docs/orphan.md", "Nobody links here.")
	writeDocFile(t, root, "docs/notes/note.md", "A note, now enforced like any docs/ file.")
	writeDocFile(t, root, "sub/CLAUDE.md", "Subsystem must-knows. Detail: [DETAILS.md](DETAILS.md).")
	writeDocFile(t, root, "sub/DETAILS.md", "Back to [CLAUDE.md](CLAUDE.md) (cycle guard).")
	writeDocFile(t, root, "lonely/CLAUDE.md", "Nobody mentions my directory.")

	g, err := BuildDocGraph(root)
	if err != nil {
		t.Fatalf("BuildDocGraph: %v", err)
	}
	return g
}

func TestBuildDocGraphReportsOrphans(t *testing.T) {
	g := buildFixtureGraph(t)
	want := []string{"docs/orphan.md", "lonely/CLAUDE.md"}
	if !reflect.DeepEqual(g.Orphans, want) {
		t.Errorf("orphans = %v, want %v", g.Orphans, want)
	}
}

func TestBuildDocGraphReachesViaFileAndDirRefs(t *testing.T) {
	g := buildFixtureGraph(t)

	guide, ok := g.Reached["docs/guide.md"]
	if !ok {
		t.Fatal("docs/guide.md should be reachable")
	}
	if guide.Parent != "AGENTS.md" {
		t.Errorf("guide parent = %q, want AGENTS.md", guide.Parent)
	}
	if guide.ViaDir {
		t.Error("guide reached via a file ref, ViaDir should be false")
	}

	claude, ok := g.Reached["sub/CLAUDE.md"]
	if !ok {
		t.Fatal("sub/CLAUDE.md should be reachable via the `sub/` dir reference")
	}
	if !claude.ViaDir {
		t.Error("sub/CLAUDE.md reached via a directory reference, ViaDir should be true")
	}

	if _, ok := g.Reached["routes/(main)/CLAUDE.md"]; !ok {
		t.Error("a paren group dir like routes/(main)/ should resolve from a `(main)/` dir reference")
	}

	details, ok := g.Reached["sub/DETAILS.md"]
	if !ok {
		t.Fatal("sub/DETAILS.md should be reachable from its sibling CLAUDE.md")
	}
	if details.Parent != "sub/CLAUDE.md" {
		t.Errorf("details parent = %q, want sub/CLAUDE.md", details.Parent)
	}
}

func TestBuildDocGraphEnforcesNotesUnderDocs(t *testing.T) {
	g := buildFixtureGraph(t)
	if _, ok := g.Reached["docs/notes/note.md"]; !ok {
		t.Error("docs/notes/ is under docs/ and now enforced; a linked note should be reachable")
	}
}

func TestBuildDocGraphRootsAtRootClaudeMd(t *testing.T) {
	g := buildFixtureGraph(t)
	if g.Root != "CLAUDE.md" {
		t.Errorf("root = %q, want CLAUDE.md", g.Root)
	}
	agents, ok := g.Reached["AGENTS.md"]
	if !ok {
		t.Fatal("AGENTS.md should be reached via the root CLAUDE.md @import")
	}
	if agents.Parent != "CLAUDE.md" {
		t.Errorf("AGENTS.md parent = %q, want CLAUDE.md", agents.Parent)
	}
}
