package main

import (
	"context"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"time"

	"cmdr/scripts/check/checks"
)

// The Linux lanes' target volume moves between the main clone and its worktrees in two
// directions:
//
//   - Seed: a new worktree starts with a copy of the main clone's volume, so its first
//     Linux run finds every third-party crate fresh.
//   - Promote: a worktree merged into `main` hands its volume back as the main clone's,
//     since after the fast-forward its tree IS main's tree.
//
// The volume names, labels, and stale-channel sweep are the lanes' own
// (`checks/desktop-rust-linux-container.go`), so the two can't disagree on a key.
// Mechanism, races, and measurements: `checks/DETAILS.md` § "The Linux Docker lanes share
// an image and a build cache".

// Exit codes of `copyScript`, so the caller reads the outcome from a code rather than
// from the script's wording.
const (
	// exitBusy: a cargo build holds a lock on one side, so the copy didn't start.
	exitBusy = 75
	// exitNoReflink: the copy-on-write clone failed (not btrfs, or out of space).
	exitNoReflink = 76
)

// copyScript copies `/from` into `/to` inside a container, holding every cargo lock on
// both sides so no build writes either tree mid-copy (flock holds across containers: one
// VM kernel). Busy locks exit `exitBusy` rather than wait, since a hook must never stall
// worktree creation or teardown.
//
// The copy lands in `/to/.incoming` first, then each top-level entry swaps into place
// with `mv --exchange` (one `renameat2(RENAME_EXCHANGE)`), so an interrupted run leaves
// either the old tree or the new one at every path, never a half-copied `debug/`: cargo
// trusts any rlib it finds next to a matching fingerprint. Top-level entries the source
// doesn't have go too, so the result is exactly the source. A leftover `.incoming` from
// a killed run is ignored by cargo and cleared by the next handoff.
var copyScript = fmt.Sprintf(`set -uo pipefail
shopt -s nullglob dotglob
for lock in /from/*/.cargo*lock /to/*/.cargo*lock; do
  exec {fd}<"$lock" || exit 1
  flock -n "$fd" || { echo "a build holds $lock"; exit %[1]d; }
done
rm -rf /to/.incoming && mkdir /to/.incoming || exit 1
keep=" .incoming "
for entry in /from/*; do
  name=${entry##*/}
  [ "$name" = .incoming ] && continue
  cp -a --reflink=always "$entry" /to/.incoming/ || { rm -rf /to/.incoming; exit %[2]d; }
  keep="$keep$name "
done
for entry in /to/.incoming/*; do
  name=${entry##*/}
  if [ -e "/to/$name" ]; then
    mv -T --exchange "$entry" "/to/$name" || exit 1
  else
    mv -T "$entry" "/to/$name" || exit 1
  fi
done
for entry in /to/*; do
  case "$keep" in *" ${entry##*/} "*) ;; *) rm -rf "$entry" ;; esac
done
rm -rf /to/.incoming
`, exitBusy, exitNoReflink)

// copyTimeout bounds the copy container. A reflink copy takes about a second; anything
// near this is a wedged daemon, and a hook must not hang its caller.
const copyTimeout = 2 * time.Minute

// seed gives a new worktree a copy of the main clone's Linux target volume for the same
// toolchain channel. It returns the one line worth printing, empty when there was nothing
// to do (no Rust toolchain file, or no main-clone volume yet), and an error only for a
// copy that went wrong.
func seed(worktreeRoot, mainRoot string) (string, error) {
	channel, ok := toolchainChannel(worktreeRoot)
	if !ok {
		return "", nil
	}
	from := checks.LinuxTargetVolume(mainRoot, channel)
	to := checks.LinuxTargetVolume(worktreeRoot, channel)

	if reason, down := checks.DockerDownReason(); down {
		return "no Linux cache seeded: " + reason, nil
	}
	if !volumeExists(from) {
		return "", nil
	}
	if volumeExists(to) {
		return fmt.Sprintf("%s already exists; left it as it is", to), nil
	}
	image, ok := anyLinuxImage()
	if !ok {
		return "no Linux cache seeded: no Linux lane image to copy with", nil
	}
	if err := checks.CreateLinuxTargetVolume(worktreeRoot, to); err != nil {
		return "", err
	}

	start := time.Now()
	code, out, err := copyVolume(image, from, to)
	if err != nil || code != 0 {
		// The volume is ours and brand new: nothing else can have mounted it yet (the
		// check runner waits for `.warming-worktree`), so drop it rather than hand over
		// a partial one.
		_, _ = checks.RunCommand(exec.Command("docker", "volume", "rm", to), true)
		if code == exitBusy {
			return "no Linux cache seeded: a build in the main clone is using its cache", nil
		}
		return "", copyFailure(code, out, err)
	}
	return fmt.Sprintf("seeded %s from the main clone's Linux cache in %s", to, since(start)), nil
}

// promote hands a merged worktree's Linux target volume to the main clone, replacing the
// main clone's volume's contents and dropping its volumes of other toolchain channels. It
// does nothing unless the worktree is merged (`mergedIntoMain`). It copies rather than
// moves, so the worktree keeps its volume until its own teardown reaps it, and a teardown
// that then refuses leaves nothing broken.
func promote(worktreeRoot, mainRoot string, dryRun bool) (string, error) {
	channel, ok := toolchainChannel(worktreeRoot)
	if !ok {
		return "", nil
	}
	from := checks.LinuxTargetVolume(worktreeRoot, channel)
	to := checks.LinuxTargetVolume(mainRoot, channel)

	if reason, down := checks.DockerDownReason(); down {
		return "no Linux cache promoted: " + reason, nil
	}
	if !volumeExists(from) {
		return "", nil
	}
	merged, why, err := mergedIntoMain(worktreeRoot, mainRoot)
	if err != nil {
		return "", err
	}
	if !merged {
		return fmt.Sprintf("not promoting %s: %s", from, why), nil
	}
	if dryRun {
		return fmt.Sprintf("would promote %s to the main clone's %s", from, to), nil
	}
	image, ok := anyLinuxImage()
	if !ok {
		return "no Linux cache promoted: no Linux lane image to copy with", nil
	}
	if err := checks.CreateLinuxTargetVolume(mainRoot, to); err != nil {
		return "", err
	}

	start := time.Now()
	code, out, err := copyVolume(image, from, to)
	if code == exitBusy {
		return fmt.Sprintf("not promoting %s: a build is using it or the main clone's cache; kept the older one", from), nil
	}
	if err != nil || code != 0 {
		return "", copyFailure(code, out, err)
	}
	msg := fmt.Sprintf("promoted %s to the main clone's %s in %s", from, to, since(start))
	if dropped := checks.DropStaleLinuxTargetVolumes(mainRoot, to); len(dropped) > 0 {
		msg += fmt.Sprintf(" (dropped %s)", strings.Join(dropped, ", "))
	}
	return msg, nil
}

// toolchainChannel reads the checkout's pinned channel. A checkout without a
// `rust-toolchain.toml` has no Linux lanes, so there's nothing to hand over.
func toolchainChannel(rootDir string) (string, bool) {
	toolchainFile, err := os.ReadFile(filepath.Join(rootDir, "rust-toolchain.toml"))
	if err != nil {
		return "", false
	}
	channel, err := checks.RustToolchainChannel(toolchainFile)
	return channel, err == nil
}

func volumeExists(name string) bool {
	_, err := checks.RunCommand(exec.Command("docker", "volume", "inspect", name), true)
	return err == nil
}

// anyLinuxImage picks any tag of the lanes' image: the copy needs only bash, GNU `cp`,
// `mv`, and `flock`, which every tag has, so a stale tag beats a pull or a 38 s build.
func anyLinuxImage() (string, bool) {
	tags := checks.LinuxImageTags()
	if len(tags) == 0 {
		return "", false
	}
	return tags[0], true
}

// copyVolume runs `copyScript` and returns its exit code (-1 when the container never
// ran it) with its output.
func copyVolume(image, from, to string) (int, string, error) {
	ctx, cancel := context.WithTimeout(context.Background(), copyTimeout)
	defer cancel()
	cmd := exec.CommandContext(ctx, "docker", "run", "--rm",
		"-v", from+":/from:ro", "-v", to+":/to", image, "bash", "-c", copyScript)
	out, err := checks.RunCommand(cmd, true)
	var exitErr *exec.ExitError
	if errors.As(err, &exitErr) {
		return exitErr.ExitCode(), out, nil
	}
	if err != nil {
		return -1, out, err
	}
	return 0, out, nil
}

// copyFailure words a copy that went wrong for the one line the hook prints.
func copyFailure(code int, out string, err error) error {
	out = strings.TrimSpace(out)
	if err != nil {
		return fmt.Errorf("the Linux cache copy didn't run: %w: %s", err, out)
	}
	if code == exitNoReflink {
		return fmt.Errorf("the Linux cache copy-on-write clone didn't work (not btrfs, or out of space?): %s", out)
	}
	return fmt.Errorf("the Linux cache copy exited %d: %s", code, out)
}

func since(start time.Time) time.Duration {
	return time.Since(start).Round(100 * time.Millisecond)
}

// mergedIntoMain decides by git ancestry whether a worktree's build cache describes the
// main clone's tree: its HEAD must be an ancestor of (or equal to) the main clone's HEAD,
// and it must have no uncommitted changes to tracked files. The second half matters
// because cargo judges freshness by mtime: an rlib built from an uncommitted edit, next to
// the main clone's older, unedited copy of that file, would pass as fresh.
func mergedIntoMain(worktreeRoot, mainRoot string) (bool, string, error) {
	wtHead, err := gitOut(worktreeRoot, "rev-parse", "HEAD")
	if err != nil {
		return false, "", err
	}
	mainHead, err := gitOut(mainRoot, "rev-parse", "HEAD")
	if err != nil {
		return false, "", err
	}
	ancestor := exec.Command("git", "merge-base", "--is-ancestor", wtHead, mainHead)
	ancestor.Dir = mainRoot
	if err := ancestor.Run(); err != nil {
		var exitErr *exec.ExitError
		if errors.As(err, &exitErr) && exitErr.ExitCode() == 1 {
			return false, "its branch isn't merged into the main clone's HEAD", nil
		}
		return false, "", fmt.Errorf("git merge-base --is-ancestor: %w", err)
	}
	dirty, err := gitOut(worktreeRoot, "status", "--porcelain", "--untracked-files=no")
	if err != nil {
		return false, "", err
	}
	if dirty != "" {
		return false, "it has uncommitted changes, so its cache isn't the main clone's tree", nil
	}
	return true, "", nil
}

// gitOut runs git in dir and returns its trimmed stdout.
func gitOut(dir string, args ...string) (string, error) {
	cmd := exec.Command("git", args...)
	cmd.Dir = dir
	out, err := cmd.Output()
	if err != nil {
		return "", fmt.Errorf("git %s in %s: %w", strings.Join(args, " "), dir, err)
	}
	return strings.TrimSpace(string(out)), nil
}
