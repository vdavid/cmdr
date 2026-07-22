package main

import (
	"fmt"

	"cmdr/scripts/check/checks"
)

// renderDocsGraph draws the doc-discoverability tree rooted at the repo-root
// CLAUDE.md: every CLAUDE.md, DETAILS.md, and docs/ file reachable by walking
// references between docs, in the same box-drawing style as the check dependency
// graph (--graph). A node reached through a directory reference (CLAUDE.md only)
// is tagged "(dir reference)". Orphans (enforced docs not reachable) are listed
// in red at the bottom: the same set the docs-reachable check fails on.
//
// Closest-to-root wins: each doc appears once, under the shortest reference path
// from the root (BFS), so the tree shows the most direct way to discover it.
func renderDocsGraph(rootDir string, useColor bool) error {
	g, err := checks.BuildDocGraph(rootDir)
	if err != nil {
		return err
	}
	c := func(code, s string) string {
		if !useColor {
			return s
		}
		return code + s + colorReset
	}

	usage := computeDocUsage(rootDir, g)
	nodes := make([]string, 0, len(g.Reached))
	for n := range g.Reached {
		nodes = append(nodes, n)
	}
	readColor := readColorBuckets(usage, nodes)

	fmt.Printf("%s\n", c(colorDim, fmt.Sprintf(
		"Doc reachability from %s — %d reachable, %d orphaned. Edge = \"is referenced by\".",
		g.Root, len(g.Reached), len(g.Orphans))))
	if usage.available {
		fmt.Printf("%s\n", c(colorDim, fmt.Sprintf(
			"Usage over ~30 days across %d agent sessions (read = doc loaded or explicitly read; %% of sessions). Read %% color: ",
			usage.totalSessions))+
			c(colorRed, "0 unread")+c(colorDim, ", ")+c(colorYellow, "≤20%")+c(colorDim, ", ")+
			c(colorGreen, ">20%")+c(colorDim, "."))
	} else {
		fmt.Printf("%s\n", c(colorDim, "Usage unavailable (no transcripts found in ~/.claude/projects)."))
	}
	fmt.Println()

	var printSubtree func(docPath, prefix string, isLast, isRoot bool)
	printSubtree = func(docPath, prefix string, isLast, isRoot bool) {
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
		label := docPath
		if r := g.Reached[docPath]; r != nil && r.ViaDir {
			label += " " + c(colorDim, "(dir reference)")
		}
		if usage.available {
			label += " " + usageAnnotation(usage, docPath, readColor[docPath], c)
		}
		fmt.Printf("%s%s%s\n", prefix, connector, label)
		children := g.Children(docPath)
		for i, child := range children {
			printSubtree(child, childPrefix, i == len(children)-1, false)
		}
	}
	printSubtree(g.Root, "", true, true)

	if len(g.Orphans) > 0 {
		fmt.Printf("\n%s\n", c(colorRed, fmt.Sprintf(
			"Orphans (%d) — not reachable from %s:", len(g.Orphans), g.Root)))
		for _, o := range g.Orphans {
			fmt.Printf("  %s\n", c(colorRed, o))
		}
		fmt.Printf("\n%s\n", c(colorDim,
			"Link each orphan from a doc that's already reachable. A CLAUDE.md also counts as reached when a reachable doc mentions its directory."))
	}
	return nil
}
