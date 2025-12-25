package main

import (
	"fmt"
	"os/exec"
	"path/filepath"
)

// HugoBuildCheck checks that the Hugo site builds without errors.
type HugoBuildCheck struct{}

func (c *HugoBuildCheck) Name() string {
	return "Hugo build"
}

func (c *HugoBuildCheck) Run(ctx *CheckContext) error {
	docsiteDir := filepath.Join(ctx.RootDir, "docsite")

	// Run hugo build with --quiet flag to suppress output unless there are errors
	cmd := exec.Command("hugo", "--quiet")
	cmd.Dir = docsiteDir
	output, err := runCommand(cmd, true)
	if err != nil {
		fmt.Println()
		fmt.Print(indentOutput(output, "      "))
		return fmt.Errorf("hugo build failed")
	}
	return nil
}

// DocsLinksCheck checks for broken links in the documentation.
// This uses lychee if available, otherwise skips silently.
type DocsLinksCheck struct{}

func (c *DocsLinksCheck) Name() string {
	return "Docs links"
}

func (c *DocsLinksCheck) Run(ctx *CheckContext) error {
	docsiteDir := filepath.Join(ctx.RootDir, "docsite")

	// Check if lychee is available
	if !commandExists("lychee") {
		// Warn but don't fail - lychee is optional
		fmt.Println("      (lychee not installed, skipping link check)")
		return nil
	}

	// Build the site first (lychee needs HTML output)
	buildCmd := exec.Command("hugo", "--quiet")
	buildCmd.Dir = docsiteDir
	if err := buildCmd.Run(); err != nil {
		return fmt.Errorf("failed to build site for link checking: %w", err)
	}

	// Run lychee on the built site
	// Use --offline for local files, --no-progress for cleaner output
	publicDir := filepath.Join(docsiteDir, "public")
	// Check all HTML files in the public directory
	// --offline: check local files without making HTTP requests
	// --no-progress: suppress progress output
	// Note: --offline mode doesn't need --accept flag for file:// URLs
	cmd := exec.Command("lychee", "--no-progress", "--offline", publicDir)
	cmd.Dir = docsiteDir
	output, err := runCommand(cmd, true)
	if err != nil {
		fmt.Println()
		fmt.Print(indentOutput(output, "      "))
		return fmt.Errorf("broken links found")
	}
	return nil
}
