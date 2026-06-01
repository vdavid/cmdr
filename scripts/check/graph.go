package main

import (
	"fmt"
	"runtime"
	"sort"
	"strings"

	"cmdr/scripts/check/checks"
)

// renderGraph prints the DependsOn graph of the given checks, annotated with
// each check's CPU weight and size lane (fast / normal / slow), in one of three
// formats: "tree" (colored terminal tree, the default), "mermaid", or "dot".
//
// Edges point from a dependency to its dependent (formatter → linter → tests),
// so the graph reads top-down in execution order.
func renderGraph(defs []checks.CheckDefinition, format string, isTTY bool) error {
	switch format {
	case "", "tree":
		renderGraphTree(defs, isTTY)
	case "mermaid":
		renderGraphMermaid(defs)
	case "dot":
		renderGraphDot(defs)
	default:
		return fmt.Errorf("unknown graph format %q (want: tree, mermaid, dot)", format)
	}
	return nil
}

// graphModel holds the lookup structures shared by the renderers.
type graphModel struct {
	defs     []checks.CheckDefinition
	byID     map[string]*checks.CheckDefinition
	children map[string][]string // dependency ID -> dependent IDs, in registry order
	roots    []string            // IDs with no in-set dependency, in registry order
	capacity int
}

func buildGraphModel(defs []checks.CheckDefinition) *graphModel {
	m := &graphModel{
		defs:     defs,
		byID:     make(map[string]*checks.CheckDefinition, len(defs)),
		children: make(map[string][]string),
		capacity: runtime.NumCPU(),
	}
	for i := range defs {
		m.byID[defs[i].ID] = &defs[i]
	}
	for i := range defs {
		d := &defs[i]
		hasParent := false
		for _, dep := range d.DependsOn {
			if _, ok := m.byID[dep]; ok {
				m.children[dep] = append(m.children[dep], d.ID)
				hasParent = true
			}
		}
		if !hasParent {
			m.roots = append(m.roots, d.ID)
		}
	}
	return m
}

// sizeTag returns the size lane of a check: "fast", "slow", or "normal".
func sizeTag(d *checks.CheckDefinition) string {
	switch {
	case d.IsSlow:
		return "slow"
	case d.IsFast:
		return "fast"
	default:
		return "normal"
	}
}

func weightOf(d *checks.CheckDefinition) int {
	if d.CpuWeight < 1 {
		return 1
	}
	return d.CpuWeight
}

// ── Terminal tree ────────────────────────────────────────────────────────────

// colorLightGreen (bright green) marks weight 2 in the gradient between green
// (w1) and yellow (w3-5). Not in colors.go since it's graph-only.
const colorLightGreen = "\033[92m"

// laneColor maps a size lane to its color: fast green, normal yellow, slow red.
var laneColor = map[string]string{"fast": colorGreen, "normal": colorYellow, "slow": colorRed}

// weightColor returns the gradient color for a CPU weight: w1 green, w2 light
// green, w3-5 yellow, w6-8 orange, w9+ red.
// laneCounts tallies how many checks fall in each size lane and the summed weight.
func laneCounts(defs []checks.CheckDefinition) (fast, normal, slow, totalWeight int) {
	for i := range defs {
		d := &defs[i]
		totalWeight += weightOf(d)
		switch sizeTag(d) {
		case "fast":
			fast++
		case "slow":
			slow++
		default:
			normal++
		}
	}
	return fast, normal, slow, totalWeight
}

func weightColor(w int) string {
	switch {
	case w <= 1:
		return colorGreen
	case w == 2:
		return colorLightGreen
	case w <= 5:
		return colorYellow
	case w <= 8:
		return colorOrange
	default:
		return colorRed
	}
}

func renderGraphTree(defs []checks.CheckDefinition, useColor bool) {
	m := buildGraphModel(defs)
	c := func(code, s string) string {
		if !useColor {
			return s
		}
		return code + s + colorReset
	}

	// annot renders the colored "(lane, wN[, smb][, ci-only])" suffix. Only the
	// lane and weight carry color; the name and tech stay plain (coloring names
	// read as confusing).
	annot := func(d *checks.CheckDefinition) string {
		w := weightOf(d)
		parts := []string{
			c(laneColor[sizeTag(d)], sizeTag(d)),
			c(weightColor(w), fmt.Sprintf("w%d", w)),
		}
		if d.NeedsSmb != "" {
			parts = append(parts, "smb")
		}
		if d.CIOnly {
			parts = append(parts, "ci-only")
		}
		return "(" + strings.Join(parts, ", ") + ")"
	}
	nodeLabel := func(id string) string {
		d := m.byID[id]
		if d == nil {
			return id // unreachable: ids come from the model built off defs
		}
		return fmt.Sprintf("%s %s - %s", d.CLIName(), annot(d), d.Tech)
	}

	var printSubtree func(id, prefix string, isLast, isRoot bool)
	printSubtree = func(id, prefix string, isLast, isRoot bool) {
		connector := ""
		childPrefix := prefix
		if !isRoot {
			if isLast {
				connector = "└─ "
				childPrefix = prefix + "   "
			} else {
				connector = "├─ "
				childPrefix = prefix + "│  "
			}
		}
		fmt.Printf("%s%s%s\n", prefix, connector, nodeLabel(id))
		for i, k := range m.children[id] {
			printSubtree(k, childPrefix, i == len(m.children[id])-1, false)
		}
	}

	// Split roots into trees (have dependents) and standalone checks (no deps in
	// either direction). A standalone check would be a lonely one-node tree, so
	// list those compactly at the end instead of cluttering the graph.
	var trees, standalone []string
	for _, root := range m.roots {
		if len(m.children[root]) > 0 {
			trees = append(trees, root)
		} else {
			standalone = append(standalone, root)
		}
	}

	fmt.Printf("%s\n\n", c(colorDim, fmt.Sprintf(
		"Check dependency graph — %d checks, core budget %d. Edge = runs-after.", len(defs), m.capacity)))
	for _, root := range trees {
		printSubtree(root, "", true, true)
		fmt.Println()
	}

	if len(standalone) > 0 {
		items := make([]string, len(standalone))
		for i, id := range standalone {
			d := m.byID[id]
			if d == nil {
				items[i] = id // unreachable: ids come from the model built off defs
				continue
			}
			items[i] = fmt.Sprintf("%s %s", d.CLIName(), annot(d))
		}
		fmt.Printf("%s\n  %s\n\n",
			c(colorDim, fmt.Sprintf("Standalone (no dependencies, %d):", len(standalone))),
			strings.Join(items, ", "))
	}

	// Summary + legend.
	fastN, normalN, slowN, totalW := laneCounts(defs)
	fmt.Printf("%s\n", c(colorDim, fmt.Sprintf(
		"Legend: lane = %s / %s / %s; weight w1 %s, w2 %s, w3-5 %s, w6-8 %s, w9+ %s.",
		c(colorGreen, "fast"), c(colorYellow, "normal"), c(colorRed, "slow"),
		c(colorGreen, "green"), c(colorLightGreen, "light"), c(colorYellow, "yellow"), c(colorOrange, "orange"), c(colorRed, "red"))))
	fmt.Printf("%s\n", c(colorDim, fmt.Sprintf(
		"Totals: %d fast, %d normal, %d slow · summed weight %d vs %d-core budget.",
		fastN, normalN, slowN, totalW, m.capacity)))
}

// ── Mermaid ──────────────────────────────────────────────────────────────────

// mermaidID sanitizes a check ID into a Mermaid-safe node identifier.
func mermaidID(id string) string {
	return strings.NewReplacer("-", "_", ".", "_", "/", "_").Replace(id)
}

func renderGraphMermaid(defs []checks.CheckDefinition) {
	m := buildGraphModel(defs)
	fmt.Println("%% Cmdr check dependency graph. Paste into https://mermaid.live or a Markdown ```mermaid block.")
	fmt.Println("graph TD")
	fmt.Println("  classDef fast fill:#d9f2d9,stroke:#3a7a3a,color:#1a3a1a;")
	fmt.Println("  classDef normal fill:#eef2f7,stroke:#5a6b80,color:#1a2230;")
	fmt.Println("  classDef slow fill:#ffe0c2,stroke:#c87a32,color:#5a3410;")
	for i := range defs {
		d := &defs[i]
		tags := sizeTag(d)
		if d.CIOnly {
			tags += " ci"
		}
		label := fmt.Sprintf("%s<br/>w%d · %s", d.CLIName(), weightOf(d), tags)
		fmt.Printf("  %s[\"%s\"]:::%s\n", mermaidID(d.ID), label, sizeTag(d))
	}
	// Edges in registry order for stable output.
	for i := range defs {
		d := &defs[i]
		for _, child := range m.children[d.ID] {
			fmt.Printf("  %s --> %s\n", mermaidID(d.ID), mermaidID(child))
		}
	}
}

// ── Graphviz DOT ───────────────────────────────────────────────────────────

func renderGraphDot(defs []checks.CheckDefinition) {
	m := buildGraphModel(defs)
	fill := map[string]string{"fast": "#d9f2d9", "normal": "#eef2f7", "slow": "#ffe0c2"}
	fmt.Println("// Cmdr check dependency graph. Render: dot -Tpng -o checks.png this.dot")
	fmt.Println("digraph checks {")
	fmt.Println("  rankdir=TB;")
	fmt.Println("  node [shape=box, style=\"filled,rounded\", fontname=\"Helvetica\"];")
	// Group nodes into per-app clusters for readability.
	apps := []checks.App{checks.AppOther, checks.AppDesktop, checks.AppWebsite, checks.AppApiServer, checks.AppScripts}
	byApp := map[checks.App][]*checks.CheckDefinition{}
	for i := range defs {
		byApp[defs[i].App] = append(byApp[defs[i].App], &defs[i])
	}
	for _, app := range apps {
		group := byApp[app]
		if len(group) == 0 {
			continue
		}
		sort.SliceStable(group, func(a, b int) bool { return group[a].ID < group[b].ID })
		fmt.Printf("  subgraph \"cluster_%s\" {\n", app)
		fmt.Printf("    label=%q; style=dashed; color=\"#aaaaaa\";\n", checks.AppDisplayName(app))
		for _, d := range group {
			style := ""
			if d.CIOnly {
				style = ", style=\"filled,rounded,dashed\""
			}
			label := fmt.Sprintf("%s\\nw%d · %s", d.CLIName(), weightOf(d), sizeTag(d))
			fmt.Printf("    %q [label=%q, fillcolor=%q%s];\n", d.ID, label, fill[sizeTag(d)], style)
		}
		fmt.Println("  }")
	}
	for i := range defs {
		d := &defs[i]
		for _, child := range m.children[d.ID] {
			fmt.Printf("  %q -> %q;\n", d.ID, child)
		}
	}
	fmt.Println("}")
}
