package checks

import (
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"slices"
	"strings"
)

// The Linux E2E base image copies its Rust toolchain out of the official `rust` image and selects
// it with `ENV RUSTUP_TOOLCHAIN`, and both must name the version `rust-toolchain.toml` pins. The
// container mounts only the cargo registry, so a stale image makes rustup download the pinned
// toolchain again in every fresh container. That ran unnoticed from the 1.99.1 pin bump until
// 2026-10-22, with the image still on 1.99.0, because nothing tied the two together.
//
// Fails when any `rust:<version>-...` image or `RUSTUP_TOOLCHAIN=<version>` in `Dockerfile.base`
// differs from the pinned channel. Bump them together; the image tag also needs its new digest.
// Renovate leaves this image alone (`renovate.json`) so the bump always comes with the pin's.
const (
	e2eRustToolchainFile = "rust-toolchain.toml"
	e2eRustDockerfile    = "apps/desktop/test/e2e-linux/docker/Dockerfile.base"
)

// e2eRustImagePinInputs are the two files the check compares.
var e2eRustImagePinInputs = []string{e2eRustToolchainFile, e2eRustDockerfile}

var rustImageTagRE = regexp.MustCompile(`(?:\brust:|\bRUSTUP_TOOLCHAIN=)(\d+\.\d+(?:\.\d+)?)\b`)

func RunE2ERustImagePin(ctx *CheckContext) (CheckResult, error) {
	toolchain, err := os.ReadFile(filepath.Join(ctx.RootDir, e2eRustToolchainFile))
	if err != nil {
		return CheckResult{}, fmt.Errorf("read %s: %w", e2eRustToolchainFile, err)
	}
	dockerfile, err := os.ReadFile(filepath.Join(ctx.RootDir, e2eRustDockerfile))
	if err != nil {
		return CheckResult{}, fmt.Errorf("read %s: %w", e2eRustDockerfile, err)
	}
	pinned, tags, err := e2eRustVersions(string(toolchain), string(dockerfile))
	if err != nil {
		return CheckResult{}, err
	}
	var stale []string
	for _, tag := range tags {
		if tag != pinned && !slices.Contains(stale, tag) {
			stale = append(stale, tag)
		}
	}
	if len(stale) > 0 {
		return CheckResult{}, fmt.Errorf("%s copies Rust %s, but %s pins %s\n%s", e2eRustDockerfile,
			strings.Join(stale, ", "), e2eRustToolchainFile, pinned,
			indentOutput("bump the `rust:` image tag (and its digest) to the pinned version"))
	}
	return Success(fmt.Sprintf("E2E base image copies the pinned Rust %s", pinned)), nil
}

// e2eRustVersions returns the pinned channel and every Rust version the Dockerfile names.
func e2eRustVersions(toolchain, dockerfile string) (string, []string, error) {
	pinned, err := RustToolchainChannel([]byte(toolchain))
	if err != nil {
		return "", nil, err
	}
	var tags []string
	for _, line := range strings.Split(dockerfile, "\n") {
		if strings.HasPrefix(strings.TrimSpace(line), "#") {
			continue
		}
		for _, t := range rustImageTagRE.FindAllStringSubmatch(line, -1) {
			tags = append(tags, t[1])
		}
	}
	if len(tags) == 0 {
		return "", nil, fmt.Errorf("%s names no `rust:<version>` image or `RUSTUP_TOOLCHAIN`; update this check if the toolchain comes from elsewhere now", e2eRustDockerfile)
	}
	return pinned, tags, nil
}
