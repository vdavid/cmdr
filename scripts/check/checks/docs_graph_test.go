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
//	AGENTS.md            mentions docs/guide.md (file ref) and `sub/` (dir ref)
//	docs/guide.md        reachable via file ref from the root
//	docs/orphan.md       mentioned nowhere -> orphan
//	sub/CLAUDE.md        reachable via the `sub/` directory reference
//	sub/DETAILS.md       linked from sub/CLAUDE.md, and links back (cycle)
//	lonely/CLAUDE.md     its dir is never mentioned -> orphan
//	docs/notes/scratch.md ephemeral, excluded from candidates entirely
func buildFixtureGraph(t *testing.T) *DocGraph {
	t.Helper()
	root := t.TempDir()
	writeDocFile(t, root, "AGENTS.md", "See [the guide](docs/guide.md) and the `sub/` subsystem.")
	writeDocFile(t, root, "docs/guide.md", "A reachable guide. No further links.")
	writeDocFile(t, root, "docs/orphan.md", "Nobody links here.")
	writeDocFile(t, root, "sub/CLAUDE.md", "Subsystem must-knows. Detail: [DETAILS.md](DETAILS.md).")
	writeDocFile(t, root, "sub/DETAILS.md", "Back to [CLAUDE.md](CLAUDE.md) (cycle guard).")
	writeDocFile(t, root, "lonely/CLAUDE.md", "Nobody mentions my directory.")
	writeDocFile(t, root, "docs/notes/scratch.md", "Ephemeral scratch, not enforced.")

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

	details, ok := g.Reached["sub/DETAILS.md"]
	if !ok {
		t.Fatal("sub/DETAILS.md should be reachable from its sibling CLAUDE.md")
	}
	if details.Parent != "sub/CLAUDE.md" {
		t.Errorf("details parent = %q, want sub/CLAUDE.md", details.Parent)
	}
}

func TestBuildDocGraphExcludesEphemeralDirs(t *testing.T) {
	g := buildFixtureGraph(t)
	if _, ok := g.Reached["docs/notes/scratch.md"]; ok {
		t.Error("docs/notes/ is ephemeral and must not be a graph node")
	}
	for _, o := range g.Orphans {
		if o == "docs/notes/scratch.md" {
			t.Error("docs/notes/ must be excluded from orphan candidates")
		}
	}
}
