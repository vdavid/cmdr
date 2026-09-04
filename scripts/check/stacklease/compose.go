package stacklease

import (
	"bytes"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
)

// dockerComposer is the real Composer backed by `docker compose`, scoped to one
// stack. It mirrors the stack's start.sh invocation: the stack's project name,
// and for `up` the stack's compose files layered in order. The bare ps/down
// calls reconstruct config from container labels, so they don't need the `-f`
// flags (matching start.sh / stop.sh).
type dockerComposer struct{ stack *Stack }

// composeFileArgs returns the `-f` flags for `up`, resolving the stack's compose
// dir. A stack whose compose dir can't be found has no legal `up`: docker's
// default file lookup would bring up whatever compose file happens to sit near
// the cwd, under our project name. So this reports instead of guessing.
func (d dockerComposer) composeFileArgs() ([]string, error) {
	cd := d.stack.composeDir()
	if cd == "" {
		return nil, fmt.Errorf(
			"can't find the %s stack's compose dir (%s); set %s or run from a checkout that has it",
			d.stack.Name, d.stack.composeDirRel, d.stack.composeDirEnv)
	}
	args := make([]string, 0, 2*len(d.stack.composeFiles))
	for _, f := range d.stack.composeFiles {
		args = append(args, "-f", filepath.Join(cd, f))
	}
	return args, nil
}

func runDocker(args ...string) (string, error) {
	cmd := exec.Command("docker", args...)
	var out bytes.Buffer
	cmd.Stdout = &out
	cmd.Stderr = &out
	// Inherit the environment so the stack's port env (set by the orchestrator or
	// by the bash caller) flows into compose's `${...}` port substitution.
	cmd.Env = os.Environ()
	err := cmd.Run()
	return out.String(), err
}

// composePsLine is the subset of `docker compose ps --format json` we read.
// Compose emits one JSON object per line (NDJSON).
type composePsLine struct {
	Service string `json:"Service"`
	State   string `json:"State"`
	Health  string `json:"Health"`
}

// Status returns the running and healthy service sets for the project.
func (d dockerComposer) Status() (running map[string]bool, healthy map[string]bool, err error) {
	out, err := runDocker("compose", "-p", d.stack.ProjectName, "ps", "--format", "json")
	if err != nil {
		return nil, nil, fmt.Errorf("docker compose ps: %w\n%s", err, out)
	}
	running = map[string]bool{}
	healthy = map[string]bool{}
	for _, line := range strings.Split(strings.TrimSpace(out), "\n") {
		line = strings.TrimSpace(line)
		if line == "" {
			continue
		}
		var p composePsLine
		if err := json.Unmarshal([]byte(line), &p); err != nil {
			// Tolerate a stray non-JSON line rather than failing the whole
			// status read; log and skip.
			Logf("WARN: unparseable `compose ps` line skipped: %q", line)
			continue
		}
		if p.Service == "" {
			continue
		}
		if p.State == "running" {
			running[p.Service] = true
			// Health is empty for images without a healthcheck; "healthy" only
			// when the healthcheck passed.
			if p.Health == "healthy" {
				healthy[p.Service] = true
			}
		}
	}
	return running, healthy, nil
}

// RunningServices lists the project's running services (used for the
// all-services mode).
func (d dockerComposer) RunningServices() ([]string, error) {
	out, err := runDocker("compose", "-p", d.stack.ProjectName, "ps", "--services", "--filter", "status=running")
	if err != nil {
		return nil, fmt.Errorf("docker compose ps --services: %w\n%s", err, out)
	}
	var svcs []string
	for _, line := range strings.Split(strings.TrimSpace(out), "\n") {
		if line = strings.TrimSpace(line); line != "" {
			svcs = append(svcs, line)
		}
	}
	return svcs, nil
}

// Up brings the named services up (empty = all defined), layering the stack's
// compose files.
func (d dockerComposer) Up(services []string) error {
	fileArgs, err := d.composeFileArgs()
	if err != nil {
		return err
	}
	out, err := runDocker(d.upArgs(services, fileArgs)...)
	if err != nil {
		return fmt.Errorf("docker compose up: %w\n%s", err, out)
	}
	return nil
}

// Restart stops and starts the named services, which re-runs each container's
// entrypoint. Bare (no `-f`): compose reconstructs config from container labels,
// and a restart changes nothing about the config anyway.
func (d dockerComposer) Restart(services []string) error {
	if len(services) == 0 {
		return nil
	}
	args := append([]string{"compose", "-p", d.stack.ProjectName, "restart"}, services...)
	out, err := runDocker(args...)
	if err != nil {
		return fmt.Errorf("docker compose restart: %w\n%s", err, out)
	}
	return nil
}

// upArgs builds the `docker compose … up` argv, split out so the one decision in
// it is testable without a daemon.
func (d dockerComposer) upArgs(services, fileArgs []string) []string {
	args := []string{"compose", "-p", d.stack.ProjectName}
	args = append(args, fileArgs...)
	args = append(args, "up", "-d")
	// ❗ A first-party image gets rebuilt on every up. Without it, an edit to the
	// Dockerfile or the entrypoint never reaches a container that's already
	// running, and the stack quietly keeps serving the old one. Docker's layer
	// cache makes the no-change case a few hundred milliseconds.
	if len(d.stack.buildContextsRel) != 0 {
		args = append(args, "--build")
	}
	return append(args, services...)
}

// Down tears the whole project down (matches stop.sh).
func (d dockerComposer) Down() error {
	out, err := runDocker("compose", "-p", d.stack.ProjectName, "down")
	if err != nil {
		return fmt.Errorf("docker compose down: %w\n%s", err, out)
	}
	return nil
}
