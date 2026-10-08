package checks

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"strconv"
	"strings"
	"sync"
	"time"
)

// The Linux Docker lanes (`desktop-rust-clippy-linux`, `desktop-rust-tests-linux`) share
// everything in this file: one image, one pair of cache volumes, one container lifecycle.
// Both run cargo from a Mac against the same Linux target, so they share a warm build
// directory; `DependsOn` keeps them from overlapping within a run, and cargo's own
// build-directory lock serializes two runs in the same worktree (the volume lives in one
// VM kernel, so `flock` holds across containers).

// linuxImageRepo names the lanes' provisioned image. Tags are content hashes
// (`linuxImageTag`), so an edit to what the image installs builds a new one and nothing
// else ever does.
const linuxImageRepo = "cmdr-rust-linux"

// The container-side paths the two cache volumes mount at.
const (
	linuxContainerTargetDir = "/cargo-target"
	linuxContainerCargoHome = "/cargo-home"
)

// linuxCargoHomeVolume holds cargo's registry and git checkouts for every checkout on the
// machine. It's content-addressed by crate and version, so no checkout can poison another,
// and it's the one cache that costs a network download to refill. It mounts as the WHOLE
// `CARGO_HOME` rather than just `registry/`, because cargo's package-cache lock lives in
// `CARGO_HOME` itself: shared registry plus private locks would let two containers unpack
// the same crate at once.
const linuxCargoHomeVolume = "cmdr-rust-linux-cargo-home"

// The labels the E2E Linux lane puts on its per-checkout volumes
// (`apps/desktop/scripts/e2e-linux.sh`), reused verbatim so one family of reapers
// covers both lanes: `~/.claude/scripts/remove-worktree.sh` removes any volume whose
// label VALUE is the worktree's path, and the E2E script's `reap_stale_cache_volumes`
// removes any `e2e-linux-cache` volume whose checkout no longer exists.
const (
	linuxCacheLabel    = "com.cmdr.e2e-linux-cache"
	linuxCheckoutLabel = "com.cmdr.e2e-linux-checkout"
)

// linuxTargetVolumePrefix starts every per-checkout target volume's name.
const linuxTargetVolumePrefix = "cmdr-rust-linux-target-"

// linuxDockerfileTemplate provisions what Tauri's compile step and the test run need: the
// GTK/WebKit dev libraries, the `.mise.toml` Go (`build.rs` runs
// `go run scripts/download-llama-server.go`, and Debian's `golang-go` lags too far behind
// mise), the pinned cargo-nextest, and the repo's pinned toolchain with its components.
//
// The base is `rust:<channel>` from `rust-toolchain.toml`, so the image's default
// toolchain IS the pinned one; `rustup toolchain install` then adds the file's components
// and targets at build time, which a bare `rust:latest` otherwise re-downloads on every
// cold container.
//
// dpkg's architecture names (amd64 / arm64) line up with Go's download filenames AND with
// nextest's pre-built URLs (`https://get.nexte.st/<version>/linux` for x86, `…/linux-arm`
// for ARM), so one $(dpkg --print-architecture) covers both. Installing the wrong-arch
// nextest binary caused a silent OrbStack crash on Apple Silicon (`Dynamic loader not
// found: /lib64/ld-linux-x86-64.so.2`).
//
// nextest (vs raw `cargo test`) is required: a handful of tests (e.g.
// `ai::api_keys::tests::*`) rely on per-test process isolation because the secret-store
// backend caches `CMDR_DATA_DIR` in a `LazyLock` on first access; `cargo test` runs
// siblings as threads in one process and silently shares that cache. The precompiled
// binary from get.nexte.st avoids a `cargo install` recompile.
const linuxDockerfileTemplate = `FROM rust:%[1]s
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update -qq \
 && apt-get install -y -qq --no-install-recommends \
    libgtk-3-dev libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libacl1-dev \
    curl ca-certificates \
 && rm -rf /var/lib/apt/lists/*
RUN ARCH=$(dpkg --print-architecture) \
 && case "$ARCH" in \
      amd64) NEXTEST_PLATFORM=linux ;; \
      arm64) NEXTEST_PLATFORM=linux-arm ;; \
      *) echo "unsupported architecture: $ARCH" >&2; exit 1 ;; \
    esac \
 && curl -fsSL https://go.dev/dl/go%[2]s.linux-${ARCH}.tar.gz | tar -xz -C /usr/local \
 && curl -LsSf "https://get.nexte.st/%[3]s/${NEXTEST_PLATFORM}" | tar zxf - -C /usr/local/bin
COPY rust-toolchain.toml /toolchain/rust-toolchain.toml
RUN cd /toolchain && rustup toolchain install
ENV PATH=/usr/local/go/bin:$PATH
`

// linuxDockerfile fills in the toolchain channel, the Go version, and the nextest version.
func linuxDockerfile(channel, goVersion string) string {
	return fmt.Sprintf(linuxDockerfileTemplate, channel, goVersion, containerNextestVersion)
}

var toolchainChannelRE = regexp.MustCompile(`(?m)^\s*channel\s*=\s*"([^"]+)"`)

// RustToolchainChannel reads the pinned channel from `rust-toolchain.toml`.
func RustToolchainChannel(toolchainFile []byte) (string, error) {
	m := toolchainChannelRE.FindSubmatch(toolchainFile)
	if m == nil {
		return "", fmt.Errorf("rust-toolchain.toml has no `channel = \"…\"` line")
	}
	return string(m[1]), nil
}

// linuxImageTag content-addresses the image by everything that goes into it: the
// Dockerfile (which carries the channel, Go, and nextest versions) and the toolchain file
// (which adds components and targets).
func linuxImageTag(dockerfile string, toolchainFile []byte) string {
	h := sha256.New()
	h.Write([]byte(dockerfile))
	h.Write([]byte{0})
	h.Write(toolchainFile)
	return linuxImageRepo + ":" + hex.EncodeToString(h.Sum(nil))[:12]
}

var volumeNameUnsafeRE = regexp.MustCompile(`[^a-zA-Z0-9_.-]`)

// checkoutCacheKey is the per-checkout suffix the E2E Linux lane gives its volumes, in Go:
// the checkout's directory name (Docker's volume-name charset, first 24 characters) for a
// human reading `docker volume ls`, plus 8 hex digits of its absolute path's SHA-256 so
// two worktrees named the same in different clones can't collide. The formula lives in
// `apps/desktop/scripts/e2e-linux.sh` (`CHECKOUT_KEY`);
// `TestCheckoutCacheKeyMatchesTheE2ELinuxScript` runs that script's lines to keep the two
// in step.
func checkoutCacheKey(rootDir string) string {
	slug := []rune(volumeNameUnsafeRE.ReplaceAllString(filepath.Base(rootDir), "-"))
	if len(slug) > 24 {
		slug = slug[:24]
	}
	sum := sha256.Sum256([]byte(rootDir))
	return string(slug) + "-" + hex.EncodeToString(sum[:])[:8]
}

// LinuxTargetVolume names a checkout's target volume. The toolchain channel is part of
// the name because cargo keys artifacts by rustc: after a bump every artifact in the old
// volume is dead weight, so a bump starts a fresh volume and `staleLinuxTargetVolumes`
// drops the old one.
func LinuxTargetVolume(rootDir, channel string) string {
	return linuxTargetVolumePrefix + checkoutCacheKey(rootDir) + "-" + channel
}

// staleLinuxTargetVolumes picks, from the volumes labelled with this checkout, the target
// volumes of other toolchains. It never names a volume from another lane (the E2E lane
// shares the label) or another checkout (a sibling whose key merely extends this one's).
func staleLinuxTargetVolumes(labelled []string, rootDir, current string) []string {
	prefix := linuxTargetVolumePrefix + checkoutCacheKey(rootDir) + "-"
	var stale []string
	for _, name := range labelled {
		if name != current && strings.HasPrefix(name, prefix) {
			stale = append(stale, name)
		}
	}
	return stale
}

// linuxContainer is what a Linux lane's container runs from.
type linuxContainer struct {
	Image        string
	TargetVolume string
	// Built is how long the image build took, zero when the image was already there.
	Built time.Duration
}

// buildNote is the suffix a lane's result message carries when this run paid for the
// image, so a slow run explains itself.
func (c linuxContainer) buildNote() string {
	if c.Built == 0 {
		return ""
	}
	return fmt.Sprintf(" (built %s in %s)", c.Image, c.Built.Round(time.Second))
}

// runArgs is the `docker run` flags the container needs: the repo, both cache volumes,
// and the environment pointing cargo at them. The whole repo is mounted so cargo finds
// the workspace root (and its Cargo.lock, and `.config/nextest.toml`, where the contention
// profiles live).
func (c linuxContainer) runArgs(rootDir string) []string {
	return []string{
		"-v", rootDir + ":/repo",
		"-v", c.TargetVolume + ":" + linuxContainerTargetDir,
		"-v", linuxCargoHomeVolume + ":" + linuxContainerCargoHome,
		"-w", "/repo",
		"-e", "CARGO_TARGET_DIR=" + linuxContainerTargetDir,
		"-e", "CARGO_HOME=" + linuxContainerCargoHome,
	}
}

// prepareLinuxContainer makes sure the image and the cache volumes exist.
func prepareLinuxContainer(rootDir string) (linuxContainer, error) {
	toolchainFile, err := os.ReadFile(filepath.Join(rootDir, "rust-toolchain.toml"))
	if err != nil {
		return linuxContainer{}, err
	}
	channel, err := RustToolchainChannel(toolchainFile)
	if err != nil {
		return linuxContainer{}, err
	}
	goVersion, err := MiseGoVersion(rootDir)
	if err != nil {
		return linuxContainer{}, err
	}
	dockerfile := linuxDockerfile(channel, goVersion)
	c := linuxContainer{
		Image:        linuxImageTag(dockerfile, toolchainFile),
		TargetVolume: LinuxTargetVolume(rootDir, channel),
	}

	if !dockerSucceeds("image", "inspect", c.Image) {
		start := time.Now()
		if err := buildLinuxImage(c.Image, dockerfile, toolchainFile); err != nil {
			return linuxContainer{}, err
		}
		c.Built = time.Since(start)
		pruneLinuxImages(c.Image)
	}

	if err := ensureLinuxVolumes(rootDir, c.TargetVolume); err != nil {
		return linuxContainer{}, err
	}
	return c, nil
}

// buildLinuxImage builds the image from a throwaway context holding only the toolchain
// file, the one thing the Dockerfile copies in.
func buildLinuxImage(tag, dockerfile string, toolchainFile []byte) error {
	contextDir, err := os.MkdirTemp("", "cmdr-rust-linux-image-")
	if err != nil {
		return err
	}
	defer func() { _ = os.RemoveAll(contextDir) }()
	if err := os.WriteFile(filepath.Join(contextDir, "rust-toolchain.toml"), toolchainFile, 0o644); err != nil {
		return err
	}
	cmd := exec.Command("docker", "build", "--progress=plain", "-t", tag, "-f", "-", contextDir)
	cmd.Stdin = strings.NewReader(dockerfile)
	if out, err := RunCommand(cmd, true); err != nil {
		return fmt.Errorf("building the Linux container image %s failed\n%s", tag, indentOutput(out))
	}
	return nil
}

// pruneLinuxImages drops every other tag of the lanes' image (each holds a few GB).
// Best-effort: a tag a running container still uses refuses to go, and stays until the
// next build.
func pruneLinuxImages(keep string) {
	for _, image := range LinuxImageTags() {
		if image != keep {
			_, _ = RunCommand(exec.Command("docker", "image", "rm", image), true)
		}
	}
}

// LinuxImageTags lists every local tag of the lanes' image, as `repo:tag`.
func LinuxImageTags() []string {
	out, err := RunCommand(exec.Command("docker", "image", "ls", linuxImageRepo, "--format", "{{.Repository}}:{{.Tag}}"), true)
	if err != nil {
		return nil
	}
	return strings.Fields(out)
}

// ensureLinuxVolumes creates both cache volumes, labelling the per-checkout one, then
// drops this checkout's target volumes of other toolchains.
func ensureLinuxVolumes(rootDir, targetVolume string) error {
	if err := CreateLinuxTargetVolume(rootDir, targetVolume); err != nil {
		return err
	}
	if out, err := RunCommand(exec.Command("docker", "volume", "create", linuxCargoHomeVolume), true); err != nil {
		return fmt.Errorf("creating the cargo home volume %s failed\n%s", linuxCargoHomeVolume, indentOutput(out))
	}
	DropStaleLinuxTargetVolumes(rootDir, targetVolume)
	return nil
}

// CreateLinuxTargetVolume creates a checkout's target volume with the labels the reapers
// key on; a no-op for one that exists.
//
// The labels go on at `docker volume create`, which has to come BEFORE any `docker run -v`:
// that flag also creates a missing volume, but WITHOUT labels, and `docker volume create`
// silently declines to add labels to a volume that already exists (verified on Docker
// 29.4.0, 2026-09-02, by the E2E lane) — an unlabelled cache is one no reaper finds.
func CreateLinuxTargetVolume(rootDir, targetVolume string) error {
	create := exec.Command("docker", "volume", "create",
		"--label", linuxCacheLabel+"=1",
		"--label", linuxCheckoutLabel+"="+rootDir,
		targetVolume)
	if out, err := RunCommand(create, true); err != nil {
		return fmt.Errorf("creating the Linux target volume %s failed\n%s", targetVolume, indentOutput(out))
	}
	return nil
}

// DropStaleLinuxTargetVolumes removes the checkout's target volumes of other toolchain
// channels and returns the ones it removed. Safe against a concurrent run by
// construction: Docker refuses to remove a volume a container is using, and that one
// stays for the next sweep.
func DropStaleLinuxTargetVolumes(rootDir, current string) []string {
	out, err := RunCommand(exec.Command("docker", "volume", "ls",
		"--filter", "label="+linuxCheckoutLabel+"="+rootDir, "--format", "{{.Name}}"), true)
	if err != nil {
		return nil
	}
	var dropped []string
	for _, stale := range staleLinuxTargetVolumes(strings.Fields(out), rootDir, current) {
		if _, err := RunCommand(exec.Command("docker", "volume", "rm", stale), true); err == nil {
			dropped = append(dropped, stale)
		}
	}
	return dropped
}

// dockerSucceeds reports whether a docker command exits 0, discarding its output.
func dockerSucceeds(args ...string) bool {
	_, err := RunCommand(exec.Command("docker", args...), true)
	return err == nil
}

// dockerUnavailable returns the skip result when Docker can't run a container here.
func dockerUnavailable() (CheckResult, bool) {
	if reason, down := DockerDownReason(); down {
		return Skipped(reason), true
	}
	return CheckResult{}, false
}

// DockerDownReason says why Docker can't run a container here, if it can't.
func DockerDownReason() (string, bool) {
	if !CommandExists("docker") {
		return "Docker not installed", true
	}
	if !dockerSucceeds("info") {
		return "Docker not running", true
	}
	return "", false
}

// containerKeepAlive bounds the idle container's lifetime. The container outlives each
// exec on purpose (the tests lane's contention re-run execs back into it, warm), so PID 1
// is a sleep rather than the work. The normal exit path is the deferred `docker rm -f`;
// this cap is what stops a hard-killed check runner (SIGKILL, so no defer) from leaving a
// container parked forever. Generous because a cold run compiles the whole workspace, and
// a worst-case re-run adds ~12 minutes on top.
const containerKeepAlive = 4 * time.Hour

// startLinuxContainer brings up the detached container every phase execs into.
//
// PID 1 is a bounded `sleep`, not the work: the container has to outlive the first exec
// for the contention re-run to reuse it. `--rm` plus the deferred removal is the normal
// path; the sleep is the backstop for a check runner that never gets to run its defers.
func startLinuxContainer(name, rootDir string, c linuxContainer) error {
	args := append([]string{"run", "-d", "--rm", "--name", name}, c.runArgs(rootDir)...)
	args = append(args, c.Image, "sleep", strconv.Itoa(int(containerKeepAlive.Seconds())))
	cmd := exec.Command("docker", args...)

	// Tracked BEFORE the start returns: a container that came up while the command was
	// being interrupted is exactly the one that would otherwise be orphaned.
	containerTracker.mu.Lock()
	containerTracker.names[name] = struct{}{}
	containerTracker.mu.Unlock()

	if out, err := RunCommand(cmd, true); err != nil {
		return fmt.Errorf("failed to start the Linux container: %w\n%s", err, indentOutput(out))
	}
	return nil
}

// linuxContainerName is unique per run, so two worktrees (or two runs) never collide.
func linuxContainerName(lane string) string {
	return fmt.Sprintf("cmdr-rust-%s-linux-%d-%d", lane, os.Getpid(), time.Now().UnixNano())
}

// containerTracker holds the containers a check has running, so `KillAllProcesses` can
// remove them on Ctrl+C. The runner `os.Exit`s there, so a `defer` alone isn't enough.
var containerTracker = struct {
	mu    sync.Mutex
	names map[string]struct{}
}{names: make(map[string]struct{})}

// RemoveTrackedContainers force-removes every container a check still has running.
func RemoveTrackedContainers() {
	containerTracker.mu.Lock()
	names := make([]string, 0, len(containerTracker.names))
	for name := range containerTracker.names {
		names = append(names, name)
	}
	containerTracker.mu.Unlock()

	for _, name := range names {
		removeLinuxContainer(name)
	}
}

// removeLinuxContainer tears the container down on every exit path, pass or fail. Bounded
// so a wedged daemon can't turn cleanup into the thing that hangs the check.
func removeLinuxContainer(name string) {
	rmCtx, cancel := context.WithTimeout(context.Background(), dockerControlTimeout)
	defer cancel()
	_, _ = RunCommand(exec.CommandContext(rmCtx, "docker", "rm", "-f", name), true)

	containerTracker.mu.Lock()
	delete(containerTracker.names, name)
	containerTracker.mu.Unlock()
}

// dockerExec runs a shell script inside the live container and returns its combined output.
func dockerExec(container, script string) (string, error) {
	return RunCommand(exec.Command("docker", "exec", container, "sh", "-c", script), true)
}

// dockerControlTimeout bounds the small housekeeping docker calls (load sampling,
// teardown). The cargo execs themselves stay unbounded: their deadlines are nextest's,
// not the wall clock's.
const dockerControlTimeout = 30 * time.Second

// containerCargoScript is the ONE place a cargo command line is built for the container,
// so the lanes and the contention re-run can't drift apart in quoting or output capture.
// Every argument is single-quoted: the re-run passes a nextest filter expression
// (`test(=a::b) + test(=c::d)`) whose spaces and parens `sh -c` would otherwise split.
// Stderr joins stdout so diagnostics interleave with the progress they belong to.
func containerCargoScript(args ...string) string {
	quoted := make([]string, 0, len(args))
	for _, a := range args {
		quoted = append(quoted, shellQuote(a))
	}
	return "cargo " + strings.Join(quoted, " ") + " 2>&1"
}

// shellQuote wraps an argument for `sh -c`, escaping any embedded single quote.
func shellQuote(s string) string {
	return "'" + strings.ReplaceAll(s, "'", `'\''`) + "'"
}

// linuxSelectionArgs computes the cargo package selection for `linux`, NOT for this
// machine: the container is always Linux even though the host running the check is a Mac.
// Getting that wrong leaves `cmdr-fsevent-stream` in the selection set, where it fails at
// `cargo check` with `E0455: link kind 'framework' is only supported on Apple targets`.
func linuxSelectionArgs(rootDir string) ([]string, error) {
	members, err := WorkspaceMembers(rootDir)
	if err != nil {
		return nil, err
	}
	return CargoSelectionArgs(members, "linux"), nil
}
