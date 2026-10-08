package checks

import (
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"slices"
	"strings"
	"testing"
)

// repoLinuxDockerfile renders the Linux lanes' Dockerfile from the real repo's pins.
func repoLinuxDockerfile(t *testing.T, root string) string {
	t.Helper()
	toolchainFile, err := os.ReadFile(filepath.Join(root, "rust-toolchain.toml"))
	if err != nil {
		t.Fatalf("reading rust-toolchain.toml: %v", err)
	}
	channel, err := RustToolchainChannel(toolchainFile)
	if err != nil {
		t.Fatalf("rustToolchainChannel: %v", err)
	}
	goVersion, err := MiseGoVersion(root)
	if err != nil {
		t.Fatalf("MiseGoVersion: %v", err)
	}
	return linuxDockerfile(channel, goVersion)
}

// The image IS the provisioning: every tool pinned, the repo's own toolchain as the base
// (so the container compiles with what CI compiles with), and nothing that runs the work.
// The Go tarball assertion lives in TestLinuxContainerProvisionsTheMisePinnedGo, which owns
// the "container Go == .mise.toml" invariant.
func TestLinuxImageProvisionsThePinnedTools(t *testing.T) {
	root := repoRootForTest(t)
	toolchainFile, err := os.ReadFile(filepath.Join(root, "rust-toolchain.toml"))
	if err != nil {
		t.Fatalf("reading rust-toolchain.toml: %v", err)
	}
	channel, err := RustToolchainChannel(toolchainFile)
	if err != nil {
		t.Fatalf("rustToolchainChannel: %v", err)
	}
	dockerfile := repoLinuxDockerfile(t, root)

	for _, want := range []string{
		"FROM rust:" + channel + "\n",
		"get.nexte.st/" + containerNextestVersion + "/",
		"rustup toolchain install",
		// `build.rs` shells out to Go, and each lane's exec relies on the image's PATH.
		"ENV PATH=/usr/local/go/bin:$PATH",
	} {
		if !strings.Contains(dockerfile, want) {
			t.Errorf("the Linux image must contain %q, got:\n%s", want, dockerfile)
		}
	}
	for _, unwanted := range []string{"latest", "cargo nextest run", "cargo clippy"} {
		if strings.Contains(dockerfile, unwanted) {
			t.Errorf("the Linux image must not contain %q (unpinned, or running the work at build time):\n%s", unwanted, dockerfile)
		}
	}
}

func TestRustToolchainChannel(t *testing.T) {
	got, err := RustToolchainChannel([]byte("[toolchain]\n# a comment\nchannel = \"1.97.1\"\ncomponents = [\"clippy\"]\n"))
	if err != nil || got != "1.97.1" {
		t.Errorf("rustToolchainChannel = %q, %v; want 1.97.1", got, err)
	}
	if _, err := RustToolchainChannel([]byte("[toolchain]\ncomponents = [\"clippy\"]\n")); err == nil {
		t.Error("a toolchain file with no channel must be an error, not an empty tag")
	}
}

// The tag changes with anything that changes what's inside the image, and with nothing
// else, so an unchanged tree never rebuilds and a bumped pin always does.
func TestLinuxImageTagIsContentAddressed(t *testing.T) {
	toolchain := []byte("channel = \"1.97.1\"\n")
	base := linuxImageTag(linuxDockerfile("1.97.1", "1.27.1"), toolchain)

	if again := linuxImageTag(linuxDockerfile("1.97.1", "1.27.1"), toolchain); again != base {
		t.Errorf("same inputs gave two tags: %s, %s", base, again)
	}
	if !regexp.MustCompile(`^cmdr-rust-linux:[0-9a-f]{12}$`).MatchString(base) {
		t.Errorf("unexpected tag shape %q", base)
	}
	for name, other := range map[string]string{
		"a Go bump":        linuxImageTag(linuxDockerfile("1.97.1", "1.27.2"), toolchain),
		"a toolchain bump": linuxImageTag(linuxDockerfile("1.98.0", "1.27.1"), toolchain),
		"a new component":  linuxImageTag(linuxDockerfile("1.97.1", "1.27.1"), append(toolchain, []byte("components = [\"miri\"]\n")...)),
	} {
		if other == base {
			t.Errorf("%s must build a new image, but kept tag %s", name, base)
		}
	}
}

func TestCheckoutCacheKey(t *testing.T) {
	key := checkoutCacheKey("/Users/me/cmdr/.claude/worktrees/linux-lane-cache")
	if !regexp.MustCompile(`^linux-lane-cache-[0-9a-f]{8}$`).MatchString(key) {
		t.Errorf("unexpected key %q", key)
	}
	if other := checkoutCacheKey("/Users/me/other-clone/.claude/worktrees/linux-lane-cache"); other == key {
		t.Errorf("two checkouts with the same dir name must not share a key: %s", key)
	}
	long := checkoutCacheKey("/x/a very long worktree name with spaces and ü")
	if slug := long[:strings.LastIndex(long, "-")]; slug != "a-very-long-worktree-nam" {
		t.Errorf("slug must be Docker-safe and cut at 24 characters, got %q", slug)
	}
}

// The key and labels are the E2E Linux lane's, reused so one family of reapers covers both
// lanes. This runs the script's OWN lines, so the two can't drift apart silently.
func TestCheckoutCacheKeyMatchesTheE2ELinuxScript(t *testing.T) {
	if _, err := exec.LookPath("bash"); err != nil {
		t.Skip("bash not available")
	}
	script, err := os.ReadFile(filepath.Join(repoRootForTest(t), "apps", "desktop", "scripts", "e2e-linux.sh"))
	if err != nil {
		t.Fatalf("reading e2e-linux.sh: %v", err)
	}
	text := string(script)

	for _, label := range []string{
		`CACHE_LABEL="` + linuxCacheLabel + `"`,
		`CHECKOUT_LABEL="` + linuxCheckoutLabel + `"`,
	} {
		if !strings.Contains(text, label) {
			t.Errorf("e2e-linux.sh no longer defines %s; the reapers would stop finding this lane's volumes", label)
		}
	}

	fn := regexp.MustCompile(`(?ms)^sha256_stdin\(\) \{.*?^\}`).FindString(text)
	keyLines := regexp.MustCompile(`(?m)^CHECKOUT_(?:SLUG|KEY)=.*$`).FindAllString(text, -1)
	if fn == "" || len(keyLines) != 3 {
		t.Fatalf("couldn't find `sha256_stdin` and the three CHECKOUT_ lines in e2e-linux.sh (found %d); update this test with the script", len(keyLines))
	}
	program := fn + "\n" + strings.Join(keyLines, "\n") + "\nprintf '%s' \"$CHECKOUT_KEY\"\n"

	for _, root := range []string{
		"/Users/me/projects/cmdr",
		"/Users/me/cmdr/.claude/worktrees/linux-lane-cache",
		"/tmp/a very long worktree name with spaces",
	} {
		cmd := exec.Command("bash", "-c", program)
		cmd.Env = append(os.Environ(), "REPO_ROOT="+root, "LC_ALL=C")
		out, err := cmd.Output()
		if err != nil {
			t.Fatalf("running the script's key lines for %s: %v", root, err)
		}
		if got, want := checkoutCacheKey(root), string(out); got != want {
			t.Errorf("checkoutCacheKey(%q) = %q, e2e-linux.sh says %q", root, got, want)
		}
	}
}

// A toolchain bump starts a fresh volume, and the old one is the only thing pruned: never
// the E2E lane's volumes (same label), never a sibling checkout whose key extends ours.
func TestStaleLinuxTargetVolumes(t *testing.T) {
	root := "/Users/me/cmdr/.claude/worktrees/foo"
	current := LinuxTargetVolume(root, "1.98.0")
	old := LinuxTargetVolume(root, "1.97.1")
	key := checkoutCacheKey(root)

	labelled := []string{
		current,
		old,
		"cmdr-target-cache-" + key, // the E2E lane's, same checkout label
		"cmdr-root-node-modules-cache-" + key,
		linuxTargetVolumePrefix + key + "x-1.97.1", // not this checkout: a longer key
	}
	if got := staleLinuxTargetVolumes(labelled, root, current); !slices.Equal(got, []string{old}) {
		t.Errorf("staleLinuxTargetVolumes = %v, want only %s", got, old)
	}
	if !strings.HasSuffix(current, "-1.98.0") {
		t.Errorf("the target volume must carry the toolchain, got %s", current)
	}
}

// Both caches reach the container, and cargo is pointed at them. CARGO_HOME is the whole
// directory, so cargo's package-cache lock is shared along with the registry.
func TestLinuxContainerRunArgsMountBothCaches(t *testing.T) {
	c := linuxContainer{Image: "cmdr-rust-linux:abc", TargetVolume: "cmdr-rust-linux-target-foo-1.97.1"}
	joined := strings.Join(c.runArgs("/repo/root"), " ")
	for _, want := range []string{
		"-v /repo/root:/repo",
		"-v cmdr-rust-linux-target-foo-1.97.1:" + linuxContainerTargetDir,
		"-v " + linuxCargoHomeVolume + ":" + linuxContainerCargoHome,
		"-e CARGO_TARGET_DIR=" + linuxContainerTargetDir,
		"-e CARGO_HOME=" + linuxContainerCargoHome,
		"-w /repo",
	} {
		if !strings.Contains(joined, want) {
			t.Errorf("run args must contain %q, got: %s", want, joined)
		}
	}
}
