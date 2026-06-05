package smblease

import (
	"bytes"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
)

// dockerComposer is the real Composer backed by `docker compose`. It mirrors
// start.sh's invocation: project `smb-consumer`, and for `up` the two layered
// files (the vendored compose + the cmdr-owned override). The bare ps/down
// calls reconstruct config from container labels, so they don't need the `-f`
// flags (matching start.sh / stop.sh).
type dockerComposer struct{}

// composeFileArgs returns the `-f` flags for `up`, resolving the .compose dir.
// If the dir can't be found we return no `-f` flags; `docker compose` then
// falls back to its default lookup, which is wrong for our project — but the
// only caller (Up) is the reconcile path, and a missing compose dir already
// means a broken checkout where every SMB path fails loudly. We log it.
func composeFileArgs() []string {
	cd := composeDir()
	if cd == "" {
		Logf("WARN: could not resolve the .compose dir; `up` will use docker's default file lookup")
		return nil
	}
	return []string{
		"-f", filepath.Join(cd, "docker-compose.yml"),
		"-f", filepath.Join(cd, "docker-compose.override.yml"),
	}
}

func runDocker(args ...string) (string, error) {
	cmd := exec.Command("docker", args...)
	var out bytes.Buffer
	cmd.Stdout = &out
	cmd.Stderr = &out
	// Inherit the environment so SMB_CONSUMER_*_PORT (set by the orchestrator's
	// ApplySmbPortEnv or by the bash caller) flows into compose's `${...}` port
	// substitution.
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
func (dockerComposer) Status() (running map[string]bool, healthy map[string]bool, err error) {
	out, err := runDocker("compose", "-p", ProjectName, "ps", "--format", "json")
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

// RunningServices lists the project's running services (used for `all` mode).
func (dockerComposer) RunningServices() ([]string, error) {
	out, err := runDocker("compose", "-p", ProjectName, "ps", "--services", "--filter", "status=running")
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

// Up brings the named services up (empty = all defined), layering the override.
func (dockerComposer) Up(services []string) error {
	args := []string{"compose", "-p", ProjectName}
	args = append(args, composeFileArgs()...)
	args = append(args, "up", "-d")
	args = append(args, services...)
	out, err := runDocker(args...)
	if err != nil {
		return fmt.Errorf("docker compose up: %w\n%s", err, out)
	}
	return nil
}

// Down tears the whole project down (matches stop.sh).
func (dockerComposer) Down() error {
	out, err := runDocker("compose", "-p", ProjectName, "down")
	if err != nil {
		return fmt.Errorf("docker compose down: %w\n%s", err, out)
	}
	return nil
}
