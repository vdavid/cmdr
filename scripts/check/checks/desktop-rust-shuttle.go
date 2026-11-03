package checks

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
)

// The interleaving tests: `#[test] fn shuttle_*` cells that run a crate's real
// concurrent code under Shuttle's scheduler, which picks who runs next at every
// lock and send. Each crate that has them carries a `shuttle` feature swapping
// its primitives for Shuttle's, so they build and run only here, one crate at a
// time, and never in `desktop-rust-tests`. `scripts/check/checks/DETAILS.md`
// § "Interleaving tests (Shuttle)".

// shuttleCrates are the members with interleaving tests, by package name and
// directory. A new one adds its `shuttle` feature and a line here.
var shuttleCrates = []shuttleCrate{{pkg: "cmdr-operations", dir: "crates/cmdr-operations"}}

type shuttleCrate struct {
	pkg string
	dir string
}

// shuttleTestPattern finds an interleaving test's name in a source file.
var shuttleTestPattern = regexp.MustCompile(`(?m)^\s*fn (shuttle_\w+)\(`)

// shuttleTests maps every interleaving test's name to the crate it lives in.
// Names are unique across crates, which is what lets a property ID be just the
// name; a duplicate is an error.
func shuttleTests(rootDir string) (map[string]shuttleCrate, error) {
	tests := map[string]shuttleCrate{}
	for _, c := range shuttleCrates {
		err := filepath.WalkDir(filepath.Join(rootDir, c.dir, "src"), func(path string, d os.DirEntry, err error) error {
			if err != nil || d.IsDir() || !strings.HasSuffix(path, ".rs") {
				return err
			}
			data, err := os.ReadFile(path)
			if err != nil {
				return err
			}
			for _, m := range shuttleTestPattern.FindAllStringSubmatch(string(data), -1) {
				if other, dup := tests[m[1]]; dup {
					return fmt.Errorf("two interleaving tests are named %s (%s and %s)", m[1], other.pkg, c.pkg)
				}
				tests[m[1]] = c
			}
			return nil
		})
		if err != nil && !os.IsNotExist(err) {
			return nil, err
		}
	}
	return tests, nil
}

// shuttleCargo runs cargo the way every interleaving build does: one crate, its
// `shuttle` feature on, in a build directory of its own under root's target
// dir. The feature changes that crate's fingerprint, so sharing `target/` would
// rebuild it, and everything above it, for the next lane that asks without it.
func shuttleCargo(root string, pkg string, subcommand []string, args ...string) *exec.Cmd {
	full := append(append(subcommand, "--locked", "-p", pkg, "--features", "shuttle", "--lib"), args...)
	cmd := exec.Command("cargo", full...)
	cmd.Dir = root
	cmd.Env = append(cmd.Environ(), "CARGO_TARGET_DIR="+filepath.Join(cargoTargetDir(root), "shuttle"))
	return cmd
}

// shuttleNextest runs pkg's interleaving tests that filter selects.
func shuttleNextest(root string, pkg string, filter string) *exec.Cmd {
	return shuttleCargo(root, pkg, []string{"nextest", "run"}, "--no-fail-fast", "-E", filter)
}

// RunShuttle runs every crate's interleaving tests.
func RunShuttle(ctx *CheckContext) (CheckResult, error) {
	tests, err := shuttleTests(ctx.RootDir)
	if err != nil {
		return CheckResult{}, err
	}
	if len(tests) == 0 {
		return CheckResult{}, fmt.Errorf("found no `fn shuttle_*` test in %d crate(s); the lane would prove nothing", len(shuttleCrates))
	}
	if err := EnsureCargoNextest(); err != nil {
		return CheckResult{}, err
	}
	counts := map[string]int{}
	for _, c := range tests {
		counts[c.pkg]++
	}
	pkgs := make([]string, 0, len(counts))
	for pkg := range counts {
		pkgs = append(pkgs, pkg)
	}
	sort.Strings(pkgs)
	for _, pkg := range pkgs {
		// Clippy first: the interleaving tests and the code under the swap
		// compile only here, so no other lane lints them.
		if output, err := RunCommand(shuttleCargo(ctx.RootDir, pkg, []string{"clippy"}, "--tests", "--", "-D", "warnings"), true); err != nil {
			return CheckResult{}, fmt.Errorf("clippy found issues in %s's interleaving build\n%s", pkg, indentOutput(output))
		}
		output, err := RunCommand(shuttleNextest(ctx.RootDir, pkg, `test(/::shuttle_/)`), true)
		if err != nil {
			return CheckResult{}, fmt.Errorf(
				"an interleaving test in %s failed; its output names the schedule, and the test file's docs say how to replay it\n%s",
				pkg, indentOutput(output))
		}
	}
	return Success(fmt.Sprintf("%d interleaving %s passed", len(tests), Pluralize(len(tests), "test", "tests"))), nil
}
