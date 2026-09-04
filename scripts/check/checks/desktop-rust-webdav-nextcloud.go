package checks

import (
	"fmt"
	"os/exec"
	"regexp"
	"strconv"
	"time"
)

// The sabre/dav lane: `crates/cmdr-webdav`'s Nextcloud cells, against a real
// Nextcloud in Docker.
//
// Its own check rather than more cells in `desktop-rust-integration-tests`,
// because the server is an order of magnitude heavier than everything that lane
// talks to: a ~1 GB image against httpd's ~60 MB, and a first boot that installs
// Nextcloud before it binds a port (~25 s warm, longer on a loaded runner). The
// shared lane runs on every `pnpm check`; this one is slow-lane, so a default
// run never pays for it.
//
// What the cells are for: three claims `crates/cmdr-webdav/DETAILS.md` makes
// about real servers — what a ranged GET answers, what a chunked PUT answers,
// and whether RFC 4331 quota reports the account's numbers rather than the
// disk's. Apache `mod_dav` can answer none of them: it honours `Range` natively
// and omits the quota properties entirely.

// WebdavNextcloudTestAtom is the `test()` argument that selects exactly the
// sabre/dav cells, here and in the subtraction that keeps them OUT of the shared
// fixture lane (`fixture-lane-coverage.go`).
//
// ❗ A module path, not a name prefix: nextest matches `test()` as a substring
// of the whole test path, so the trailing `::` pins this to the one module and
// a cell elsewhere can never drift into (or out of) the lane by its name alone.
const WebdavNextcloudTestAtom = "volume::nextcloud_test::"

// RunWebdavNextcloudTests runs the `cmdr-webdav` cells that need a real
// Nextcloud.
//
// Container lifecycle is the orchestrator's, exactly as the shared lane's is:
// this check declares `webdav/nextcloud` in `NeedsContainers`, so the service is
// up by the time this runs and outlives it.
func RunWebdavNextcloudTests(ctx *CheckContext) (CheckResult, error) {
	if !CommandExists("docker") {
		return CheckResult{}, fmt.Errorf(
			"docker is required for the Nextcloud WebDAV cells; install Docker or run without this check",
		)
	}
	if _, err := RunCommand(exec.Command("docker", "info"), true); err != nil {
		return CheckResult{}, fmt.Errorf(
			"docker daemon is not running; start Docker or run without this check",
		)
	}

	// ❗ A longer wait than the other stacks get. Every other fixture container
	// binds its port within a second of starting; this one installs Nextcloud
	// first, and the port staying unbound IS the install still running.
	if err := waitForContainers("webdav-fixture", WebdavNextcloudServices(), 300*time.Second); err != nil {
		return CheckResult{}, err
	}

	laneArgs, err := HostCargoLaneArgs(ctx.RootDir)
	if err != nil {
		return CheckResult{}, err
	}
	if err := EnsureCargoNextest(); err != nil {
		return CheckResult{}, err
	}

	// Debug, and the same question `desktop-rust-tests` asks cargo, so this
	// reuses that lane's warm build instead of paying its own compile.
	baseArgs := append([]string{"--locked", "--run-ignored", "only"}, laneArgs...)
	cmd := exec.Command("cargo", append(append([]string{"nextest", "run"}, baseArgs...),
		"-E", "test("+WebdavNextcloudTestAtom+")")...)
	cmd.Dir = ctx.RootDir
	output, err := RunCommand(cmd, true)
	// See `desktop-rust-tests.go`: captured nextest output is not plain text.
	output = StripANSI(output)
	ctx.RecordTests(ParseNextestResults(output)...)
	if err != nil {
		return resolveRustFailure("Nextcloud WebDAV cells failed",
			nextestContentionRunner(ctx.RootDir, baseArgs), LoadPerCore, trimRustTestProgress(output))
	}

	count := -1
	message := "All Nextcloud WebDAV cells passed"
	if m := regexp.MustCompile(`(\d+) tests? run`).FindStringSubmatch(output); len(m) > 1 {
		count, _ = strconv.Atoi(m[1])
		message = fmt.Sprintf("%d sabre/dav %s passed", count, Pluralize(count, "cell", "cells"))
	}
	// ❗ Zero is a failure, not a pass. The filter is a module path, so a
	// renamed or moved module selects nothing and nextest calls that a clean
	// run — which would leave three unobserved claims looking observed.
	if count == 0 {
		return CheckResult{}, fmt.Errorf(
			"the filter test(%s) selected no cell; the module moved or was renamed, and the sabre/dav claims in crates/cmdr-webdav/DETAILS.md would go unchecked",
			WebdavNextcloudTestAtom,
		)
	}

	result := Success(message)
	result.Total = count
	return result, nil
}
