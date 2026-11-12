package main

import (
	"context"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"time"

	"cmdr/scripts/check/checks"
)

// The handoff sweeps the copy INSIDE the copy container, under its locks, so the sweep is
// a Linux binary: `target-sweep`, cross-built here for the Docker VM's architecture and
// mounted at `/sweep`. Its logic is `targetsweep`, unit-tested on the host.

// sweeper is a built `target-sweep` in a temp dir; the zero value is "no sweeper".
type sweeper struct{ dir string }

func (s sweeper) remove() {
	if s.dir != "" {
		_ = os.RemoveAll(s.dir)
	}
}

// sweeperBuildTimeout bounds the cross-build. Warm, it takes about a second; the first
// one on a machine also compiles the standard library for Linux.
const sweeperBuildTimeout = 90 * time.Second

// buildSweeper cross-builds `target-sweep` from the checkout whose hook is running (the
// main clone's, `scripts/worktree-hooks/DETAILS.md`). It never fails the handoff: no
// sweeper means the copy goes ahead unswept, and the note says why.
func buildSweeper(checkoutRoot string) (sweeper, string) {
	arch, ok := dockerGOARCH()
	if !ok {
		return sweeper{}, "didn't sweep the Linux cache: Docker's architecture isn't one the sweep builds for"
	}
	dir, err := os.MkdirTemp("", "cmdr-target-sweep-")
	if err != nil {
		return sweeper{}, fmt.Sprintf("didn't sweep the Linux cache: %v", err)
	}
	ctx, cancel := context.WithTimeout(context.Background(), sweeperBuildTimeout)
	defer cancel()
	cmd := exec.CommandContext(ctx, "go", "build", "-o", filepath.Join(dir, "target-sweep"), "./target-sweep")
	cmd.Dir = filepath.Join(checkoutRoot, "scripts", "check")
	cmd.Env = append(os.Environ(), "GOOS=linux", "GOARCH="+arch, "CGO_ENABLED=0")
	if out, err := checks.RunCommand(cmd, true); err != nil {
		_ = os.RemoveAll(dir)
		return sweeper{}, fmt.Sprintf("didn't sweep the Linux cache: building the sweep didn't work: %v: %s", err, strings.TrimSpace(out))
	}
	return sweeper{dir: dir}, ""
}

// dockerGOARCH maps the Docker VM's architecture to Go's name for it.
func dockerGOARCH() (string, bool) {
	out, err := checks.RunCommand(exec.Command("docker", "info", "--format", "{{.Architecture}}"), true)
	if err != nil {
		return "", false
	}
	return goarchFor(strings.TrimSpace(out))
}

func goarchFor(dockerArch string) (string, bool) {
	switch dockerArch {
	case "aarch64", "arm64":
		return "arm64", true
	case "x86_64", "amd64":
		return "amd64", true
	}
	return "", false
}

// withSweepLine appends the sweep's outcome to a handoff's line: the copy script's
// output on success (`copyScript` prints nothing else), or why no sweep ran.
func withSweepLine(msg, copyOutput, sweepNote string) string {
	line := strings.TrimSpace(copyOutput)
	if sweepNote != "" {
		line = sweepNote
	}
	if line == "" {
		return msg
	}
	return msg + "\n" + line
}
