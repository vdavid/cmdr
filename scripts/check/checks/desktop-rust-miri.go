package checks

import (
	"fmt"
	"os"
	"os/exec"
	"regexp"
	"runtime"
	"strings"
)

// miriTarget is one slice of the workspace the Miri lane interprets: a member's
// library tests whose path starts with module, and why that module is worth Miri's
// time.
//
// Most of Cmdr's `unsafe` is FFI, which Miri can't cross, so the lane names the
// modules whose `unsafe` is pure Rust (pointer work, unchecked conversions) and
// whose tests reach it without a syscall. The classification behind the list is on
// GitHub issue #887. A test in a listed module that does reach FFI or the file
// system carries `#[cfg_attr(miri, ignore = "…")]`, so the rest of its module
// still runs here.
type miriTarget struct {
	pkg    string
	module string
	why    string
}

// miriTargets is the lane's whole scope. ❗ Module paths end in `::`: libtest
// matches a filter as a substring of the test path, so the trailing `::` pins
// each to one module, and a target selecting no test fails the lane rather than
// passing on nothing.
var miriTargets = []miriTarget{
	{
		pkg:    "cmdr-fs",
		module: "attr_buf::",
		why:    "the decoder for the records the kernel packs, and `words_as_bytes`, the one view of a word buffer as bytes",
	},
	{
		pkg:    "cmdr-fs",
		module: "apple_double::",
		why:    "`OsStr::from_encoded_bytes_unchecked` on a name split after its `._` prefix",
	},
	{
		pkg:    "cmdr-fs",
		module: "mapped_file_faults::",
		why:    "the Mach-O load-command walk the SIGBUS handler trusts, and its lock-free range table",
	},
}

// miriFlags turns on strict provenance: an integer-to-pointer cast is an error
// rather than a warning, since Miri can miss a pointer bug behind one.
const miriFlags = "-Zmiri-strict-provenance"

// libtestLineRE matches libtest's per-test line, `test <path> ... <result>`. A
// test Miri stopped in has no result word: Miri's report follows on that line.
var libtestLineRE = regexp.MustCompile(`^test (\S+) \.\.\. (ok|FAILED|ignored)?`)

// RunMiri interprets the [miriTargets] tests under Miri on the pinned nightly,
// which reports undefined behavior (an out-of-bounds or misaligned read, a
// reference outliving what it borrows, a data race) that a native run executes
// without complaint. macOS only: every target is the macOS build of its module.
func RunMiri(ctx *CheckContext) (CheckResult, error) {
	if runtime.GOOS != "darwin" {
		return Skipped("macOS only: the targets are the macOS builds of their modules"), nil
	}
	if err := ensureNightlyToolchain(); err != nil {
		return CheckResult{}, err
	}
	if err := ensureMiriComponents(); err != nil {
		return CheckResult{}, err
	}

	// One cargo invocation per package, so each library compiles under Miri once.
	var packages []string
	modulesByPackage := map[string][]string{}
	for _, target := range miriTargets {
		if _, seen := modulesByPackage[target.pkg]; !seen {
			packages = append(packages, target.pkg)
		}
		modulesByPackage[target.pkg] = append(modulesByPackage[target.pkg], target.module)
	}

	// Miri builds into `target/miri/` on its own toolchain, so it shares nothing
	// with the stable lanes and takes `-p` freely: no other lane's artifacts are
	// at stake.
	passed := 0
	var failures, empty []string
	for _, pkg := range packages {
		modules := modulesByPackage[pkg]
		args := append([]string{"+" + nightlyToolchain, "miri", "test", "--locked", "-p", pkg, "--lib", "--"}, modules...)
		cmd := exec.Command("cargo", args...)
		cmd.Dir = ctx.RootDir
		cmd.Env = append(os.Environ(), "MIRIFLAGS="+miriFlags)
		output, err := RunCommand(cmd, true)

		records, ran := parseMiriTestLines(pkg, output)
		ctx.RecordTests(records...)
		if err != nil {
			failures = append(failures, fmt.Sprintf("%s:\n%s", pkg, indentOutput(miriReport(output))))
			continue
		}
		for _, module := range modules {
			count := 0
			for _, name := range ran {
				if strings.HasPrefix(name, module) {
					count++
				}
			}
			if count == 0 {
				empty = append(empty, pkg+" "+module)
			}
			passed += count
		}
	}

	if len(failures) > 0 {
		return CheckResult{}, fmt.Errorf(
			"a Miri run stopped: undefined behavior, or a test reaching code Miri can't run (mark that one `#[cfg_attr(miri, ignore = \"…\")]`)\n%s",
			strings.Join(failures, "\n"))
	}
	if len(empty) > 0 {
		return CheckResult{}, fmt.Errorf(
			"no test ran under Miri in %s; the module moved, was renamed, or every test in it is ignored, so its `unsafe` goes unchecked",
			strings.Join(empty, ", "))
	}
	result := Success(fmt.Sprintf("%d %s across %d %s, no undefined behavior",
		passed, Pluralize(passed, "test", "tests"), len(miriTargets), Pluralize(len(miriTargets), "module", "modules")))
	result.Total = passed
	return result, nil
}

// parseMiriTestLines reads libtest's per-test lines into records (IDs shaped
// like nextest's, `<package>::<test path>`), and returns the paths of the tests
// that passed. A test Miri stopped in counts as failed.
func parseMiriTestLines(pkg, output string) ([]TestRecord, []string) {
	var records []TestRecord
	var passed []string
	for line := range strings.SplitSeq(StripANSI(output), "\n") {
		m := libtestLineRE.FindStringSubmatch(line)
		if m == nil {
			continue
		}
		outcome := TestFailed
		switch m[2] {
		case "ok":
			outcome = TestPassed
			passed = append(passed, m[1])
		case "ignored":
			outcome = TestSkipped
		}
		records = append(records, TestRecord{ID: pkg + "::" + m[1], Outcome: outcome, Seconds: -1, Attempt: 1})
	}
	return records, passed
}

// miriReport keeps what says why a run stopped: from the first `error` line
// (Miri's report, or a compile error) to the end. The passing tests' lines
// before it are noise. Without one, the whole output passes through.
func miriReport(output string) string {
	lines := strings.Split(StripANSI(output), "\n")
	for i, line := range lines {
		if strings.HasPrefix(line, "error") || strings.Contains(line, "... error:") {
			return strings.Join(lines[i:], "\n")
		}
	}
	return output
}

// ensureMiriComponents adds Miri and the standard library's source (which Miri
// builds its sysroot from) to the pinned nightly, which `ensureNightlyToolchain`
// installs with the minimal profile.
func ensureMiriComponents() error {
	listCmd := exec.Command("rustup", "component", "list", "--installed", "--toolchain", nightlyToolchain)
	listed, err := RunCommand(listCmd, true)
	if err != nil {
		return fmt.Errorf("failed to list %s's components\n%s", nightlyToolchain, indentOutput(listed))
	}
	installed := strings.Fields(listed)
	has := func(prefix string) bool {
		for _, component := range installed {
			if strings.HasPrefix(component, prefix) {
				return true
			}
		}
		return false
	}
	if has("miri") && has("rust-src") {
		return nil
	}
	addCmd := exec.Command("rustup", "component", "add", "miri", "rust-src", "--toolchain", nightlyToolchain)
	if output, err := RunCommand(addCmd, true); err != nil {
		return fmt.Errorf("failed to add Miri to %s\n%s", nightlyToolchain, indentOutput(output))
	}
	return nil
}
