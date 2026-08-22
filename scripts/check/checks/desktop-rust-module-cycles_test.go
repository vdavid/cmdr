package checks

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// dotFixture builds cargo-modules-shaped DOT from a node list and edge pairs, so
// the tests exercise the real parser rather than a hand-built graph.
func dotFixture(nodes []string, edges [][2]string) string {
	var sb strings.Builder
	sb.WriteString("digraph {\n    node [\n        shape=\"record\",\n    ];\n")
	for _, node := range nodes {
		kind := "mod"
		if !strings.Contains(node, "::") {
			kind = "crate"
		}
		sb.WriteString("    \"" + node + "\" [label=\"" + kind + "|" + node + "\"]; // \"" + kind + "\" node\n")
	}
	for _, edge := range edges {
		sb.WriteString("    \"" + edge[0] + "\" -> \"" + edge[1] + "\" [label=\"uses\"]; // \"uses\" edge\n")
	}
	sb.WriteString("}\n")
	return sb.String()
}

// bidirectional expands a parent ↔ child pair, the shape `mod.rs` and a submodule
// take when the parent defines a type the child implements against.
func bidirectional(a, b string) [][2]string {
	return [][2]string{{a, b}, {b, a}}
}

func TestParseModuleGraph(t *testing.T) {
	graph := parseModuleGraph("cmdr", dotFixture(
		[]string{"cmdr_lib", "cmdr_lib::a", "cmdr_lib::b"},
		[][2]string{{"cmdr_lib", "cmdr_lib::a"}, {"cmdr_lib::a", "cmdr_lib::b"}},
	))

	if graph.root != "cmdr_lib" {
		t.Errorf("root = %q, want the one node with no `::`", graph.root)
	}
	if len(graph.nodes) != 3 {
		t.Errorf("nodes = %v, want 3", graph.nodes)
	}
	if got := graph.edges["cmdr_lib::a"]; len(got) != 1 || got[0] != "cmdr_lib::b" {
		t.Errorf("edges from a = %v, want [cmdr_lib::b]", got)
	}
}

// The two shapes the check has to tell apart, plus the ways each one can hide.
func TestCollapseHubs(t *testing.T) {
	// A file split into submodules: one parent, several children, every edge
	// parent ↔ child. This MUST collapse to one, or the ratchet fires on the
	// refactor that produced `lifecycle/state/` and gets silenced.
	var splitEdges [][2]string
	splitNodes := []string{"k::state"}
	for _, child := range []string{"a", "b", "c", "d", "e", "f", "g", "h"} {
		node := "k::state::" + child
		splitNodes = append(splitNodes, node)
		splitEdges = append(splitEdges, bidirectional("k::state", node)...)
	}

	tests := []struct {
		name  string
		nodes []string
		edges [][2]string
		want  []string
	}{
		{
			name:  "a file split into submodules is one thing, however many children",
			nodes: splitNodes,
			edges: splitEdges,
			want:  []string{"k::state"},
		},
		{
			name:  "two subsystems welded together are two things",
			nodes: []string{"k::network", "k::backends::smb"},
			edges: bidirectional("k::network", "k::backends::smb"),
			want:  []string{"k::backends::smb", "k::network"},
		},
		{
			name:  "a whole subsystem welded to another collapses to the two subsystems",
			nodes: []string{"k::network", "k::network::mdns", "k::smb", "k::smb::state", "k::smb::session"},
			edges: concatEdges(
				bidirectional("k::network", "k::network::mdns"),
				bidirectional("k::smb", "k::smb::state"),
				bidirectional("k::smb", "k::smb::session"),
				bidirectional("k::network", "k::smb::state"),
			),
			want: []string{"k::network", "k::smb"},
		},
		{
			name:  "siblings in a circle with no parent in it stay their full size",
			nodes: []string{"k::ops::eta", "k::ops::state", "k::ops::manager"},
			edges: [][2]string{
				{"k::ops::eta", "k::ops::state"},
				{"k::ops::state", "k::ops::manager"},
				{"k::ops::manager", "k::ops::eta"},
			},
			want: []string{"k::ops::eta", "k::ops::manager", "k::ops::state"},
		},
		{
			// The hub-collapsing step must not become a hiding place: adding the
			// idiomatic parent → child edge to a sibling tangle would otherwise
			// report the whole thing as one hub and erase the signal for good.
			name:  "a sibling circle stays visible when its parent joins the component",
			nodes: []string{"k::ops", "k::ops::eta", "k::ops::state", "k::ops::manager"},
			edges: concatEdges(
				bidirectional("k::ops", "k::ops::eta"),
				[][2]string{
					{"k::ops::eta", "k::ops::state"},
					{"k::ops::state", "k::ops::manager"},
					{"k::ops::manager", "k::ops::eta"},
				},
			),
			want: []string{"k::ops::eta", "k::ops::manager", "k::ops::state"},
		},
		{
			// Deleting parent-child edges outright would break this cycle and miss
			// the welding. Folding members into their topmost in-component ancestor
			// keeps it.
			name:  "welding that routes through a parent-child edge is still welding",
			nodes: []string{"k::x", "k::x::y", "k::z"},
			edges: [][2]string{
				{"k::x", "k::x::y"},
				{"k::x::y", "k::z"},
				{"k::z", "k::x"},
			},
			want: []string{"k::x", "k::z"},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			graph := parseModuleGraph("k", dotFixture(append([]string{"k"}, tt.nodes...), tt.edges))
			components := stronglyConnectedComponents(graph.nodes, graph.edges, nil)
			if len(components) != 1 {
				t.Fatalf("fixture has %d components, want exactly 1", len(components))
			}
			got := collapseHubs(components[0], graph.edges)
			if strings.Join(got, ",") != strings.Join(tt.want, ",") {
				t.Errorf("collapseHubs = %v, want %v", got, tt.want)
			}
		})
	}
}

func concatEdges(sets ...[][2]string) [][2]string {
	var out [][2]string
	for _, set := range sets {
		out = append(out, set...)
	}
	return out
}

func TestTangleHome(t *testing.T) {
	graph := moduleGraph{pkg: "cmdr", root: "cmdr_lib"}
	tests := []struct {
		name    string
		members []string
		want    string
	}{
		{
			name:    "siblings are homed at their shared parent, under the package name",
			members: []string{"cmdr_lib::file_system::write_operations::eta", "cmdr_lib::file_system::write_operations::state"},
			want:    "cmdr::file_system::write_operations",
		},
		{
			name:    "modules sharing nothing but the crate are homed at the crate",
			members: []string{"cmdr_lib::drag_image_detection", "cmdr_lib::drag_image_swap"},
			want:    "cmdr",
		},
		{
			name:    "differing depths home at the shallower shared path",
			members: []string{"cmdr_lib::file_system::listing::operations", "cmdr_lib::file_system::watcher"},
			want:    "cmdr::file_system",
		},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := tangleHome(graph, tt.members); got != tt.want {
				t.Errorf("tangleHome = %q, want %q", got, tt.want)
			}
		})
	}
}

func TestMeasureModuleCycles(t *testing.T) {
	// One healthy split (invisible) and one genuine two-subsystem weld (counted).
	measurement := measureModuleCycles("cmdr", dotFixture(
		[]string{
			"cmdr_lib",
			"cmdr_lib::state", "cmdr_lib::state::a", "cmdr_lib::state::b",
			"cmdr_lib::network", "cmdr_lib::backends::smb",
		},
		concatEdges(
			bidirectional("cmdr_lib::state", "cmdr_lib::state::a"),
			bidirectional("cmdr_lib::state", "cmdr_lib::state::b"),
			bidirectional("cmdr_lib::network", "cmdr_lib::backends::smb"),
		),
	))

	if measurement.modules != 6 {
		t.Errorf("modules = %d, want 6", measurement.modules)
	}
	if measurement.maxRaw != 3 {
		t.Errorf("maxRaw = %d, want 3 (the parent plus its two children)", measurement.maxRaw)
	}
	if len(measurement.tangles) != 1 {
		t.Fatalf("tangles = %v, want only the network/smb weld", measurement.tangles)
	}
	tangle := measurement.tangles[0]
	if tangle.home != "cmdr" || len(tangle.members) != 2 {
		t.Errorf("tangle = %+v, want two members homed at `cmdr`", tangle)
	}
}

func TestWithinAllowance(t *testing.T) {
	tests := []struct {
		name    string
		current []int
		allowed []int
		want    bool
	}{
		{"identical", []int{11, 2}, []int{11, 2}, true},
		{"smaller", []int{9, 2}, []int{11, 2}, true},
		{"fewer", []int{11}, []int{11, 2}, true},
		{"one grew", []int{12, 2}, []int{11, 2}, false},
		{"a new tangle appeared at the same home", []int{11, 2, 2}, []int{11, 2}, false},
		{"the small one grew past the allowance for the second slot", []int{11, 3}, []int{11, 2}, false},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := withinAllowance(tt.current, tt.allowed); got != tt.want {
				t.Errorf("withinAllowance(%v, %v) = %v, want %v", tt.current, tt.allowed, got, tt.want)
			}
		})
	}
}

func TestShrinkwrapModuleCyclesAllowlist(t *testing.T) {
	list := moduleCyclesAllowlist{Tangles: map[string][]int{
		"cmdr::gone":    {3},
		"cmdr::shrunk":  {5, 2},
		"cmdr::steady":  {2},
		"cmdr::regrown": {2},
	}}
	current := map[string][]int{
		"cmdr::shrunk":  {3},
		"cmdr::steady":  {2},
		"cmdr::regrown": {4},
		"cmdr::new":     {2},
	}

	changes := shrinkwrapModuleCyclesAllowlist(&list, current)

	if _, still := list.Tangles["cmdr::gone"]; still {
		t.Error("a home with no tangles left should be dropped")
	}
	if got := list.Tangles["cmdr::shrunk"]; len(got) != 1 || got[0] != 3 {
		t.Errorf("shrunk = %v, want ratcheted to [3]", got)
	}
	if got := list.Tangles["cmdr::regrown"]; len(got) != 1 || got[0] != 2 {
		t.Errorf("regrown = %v, want the allowance left alone so the regression reports", got)
	}
	if _, added := list.Tangles["cmdr::new"]; added {
		t.Error("shrink-wrap must never add a home; that needs David's OK")
	}
	if len(changes) != 2 {
		t.Errorf("changes = %v, want one removal and one ratchet", changes)
	}
}

func TestFindModuleCyclesRegressions(t *testing.T) {
	list := moduleCyclesAllowlist{Tangles: map[string][]int{
		"cmdr::known": {4},
		"cmdr::clean": {2},
	}}
	current := map[string][]int{
		"cmdr::known": {6},
		"cmdr::clean": {2},
		"cmdr::fresh": {3},
	}

	regressions := findModuleCyclesRegressions(current, list)

	if len(regressions) != 2 {
		t.Fatalf("regressions = %+v, want the grown one and the unlisted one", regressions)
	}
	if regressions[0].home != "cmdr::known" || !regressions[0].listed {
		t.Errorf("worst regression = %+v, want the grown listed home first", regressions[0])
	}
	if regressions[1].home != "cmdr::fresh" || regressions[1].listed {
		t.Errorf("second regression = %+v, want the unlisted home reported too", regressions[1])
	}
}

// The baseline is only meaningful next to the tool that produced it, so the two
// can't drift apart unnoticed.
func TestModuleCyclesAllowlistMatchesPinnedVersion(t *testing.T) {
	list := loadModuleCyclesAllowlist(repoRootForTest(t))
	if list.ToolVersion != cargoModulesVersion {
		t.Errorf("allowlist toolVersion = %q, but the check pins cargo-modules %q; re-seed the baseline on the pinned version",
			list.ToolVersion, cargoModulesVersion)
	}
	if len(list.Tangles) == 0 {
		t.Errorf("allowlist at %s has no entries", moduleCyclesAllowlistPath(repoRootForTest(t)))
	}
}

func TestCargoModulesInstallCommandPinsTheVersion(t *testing.T) {
	command := cargoModulesInstallCommand()
	if !strings.Contains(command, "--version "+cargoModulesVersion) || !strings.Contains(command, "--locked") {
		t.Errorf("install command = %q, want a pinned --version and --locked", command)
	}
}

func TestModuleCyclesPackagesAreTheLibraryMembers(t *testing.T) {
	root := repoRootForTest(t)
	packages, err := moduleCyclesPackages(root)
	if err != nil {
		t.Fatal(err)
	}
	if len(packages) == 0 {
		t.Fatal("no library members found")
	}
	for _, pkg := range packages {
		if pkg == "index-query" || pkg == "operation-log-dump" {
			t.Errorf("%s is a bin-only tool with no `--lib` graph", pkg)
		}
		if pkg == "cmdr-fsevent-stream" {
			t.Errorf("%s is vendored; its module layout isn't ours to ratchet", pkg)
		}
	}
}

// fakeCargoModules puts a stand-in `cargo-modules` on PATH reporting the given
// version, or nothing at all when version is empty.
func fakeCargoModules(t *testing.T, version string) {
	t.Helper()
	dir := t.TempDir()
	if version != "" {
		script := "#!/bin/sh\necho \"cargo-modules " + version + "\"\n"
		if err := os.WriteFile(filepath.Join(dir, "cargo-modules"), []byte(script), 0o755); err != nil {
			t.Fatal(err)
		}
	}
	t.Setenv("PATH", dir)
}

// The version guard is the whole reason this check can be trusted on somebody
// else's machine, so both halves of it are pinned down.
func TestCargoModulesSkipReason(t *testing.T) {
	t.Run("the pinned version runs the check", func(t *testing.T) {
		fakeCargoModules(t, cargoModulesVersion)
		if reason := cargoModulesSkipReason(); reason != "" {
			t.Errorf("skip reason = %q, want the check to run", reason)
		}
	})

	t.Run("a missing tool skips with the install command", func(t *testing.T) {
		fakeCargoModules(t, "")
		reason := cargoModulesSkipReason()
		if !strings.Contains(reason, cargoModulesInstallCommand()) {
			t.Errorf("skip reason = %q, want it to name the install command", reason)
		}
	})

	t.Run("a different version skips rather than comparing numbers that don't compare", func(t *testing.T) {
		fakeCargoModules(t, "0.1.0")
		reason := cargoModulesSkipReason()
		if !strings.Contains(reason, "0.1.0") || !strings.Contains(reason, cargoModulesVersion) {
			t.Errorf("skip reason = %q, want it to name both versions", reason)
		}
	})
}
