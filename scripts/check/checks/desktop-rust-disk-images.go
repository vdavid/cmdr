package checks

import (
	"fmt"
	"regexp"
	"runtime"
	"strconv"
	"strings"
)

// The opt-in lane for real-macOS-OS pins: mostly the `#[ignore]`d tests that
// attach real synthetic APFS and HFS+ images through
// `cmdr_fs::testing::disk_images`, plus any pin on real OS behavior too slow
// under load for the default lane (today a real `fseventsd` replay). So the
// eject, drive-safety, and OS pins run from `pnpm check` instead of by hand.
//
// Slow-lane and never in CI: `hdiutil` has no Linux counterpart, and CI's one
// macOS job (`desktop-rust-macos`) leaves it out until that job has a green
// record. Off macOS it answers OK without touching cargo.
//
// ❗ The lane takes no lock of its own. Each real-image test holds the
// machine-wide `flock` (`$TMPDIR/cmdr-disk-image-tests.lock`) for its whole
// body, which is what keeps two worktrees' runs apart; the runner holding the
// same lock would block its own nextest processes. Within one run, the
// `disk-image` nextest group serializes them.

// diskImageLaneTestAtoms are the real-image module paths the lane runs, one
// `test()` atom each. A milestone that adds a real-image test module adds its
// path here AND a `disk-image` override in `.config/nextest.toml`;
// `TestDiskImageLaneMatchesTheDiskImageNextestGroup` fails until both name it.
//
// ❗ Module paths ending in `::`, never name prefixes: nextest matches `test()`
// as a substring of the whole test path, so the trailing `::` pins each atom to
// one module.
var diskImageLaneTestAtoms = []string{
	"testing::disk_images::real_images::",
	"file_system::volume::eject::real_image::",
	"file_system::index_provider::real_image::",
	"transfer::real_image::",
	"delete::real_image::",
	"indexing::tests::vanish_tests::",
	"volumes::unmount_approver::real_image::",
	"volumes::rename_real_image::",
	// No image: a real `fseventsd` replay at a renamed directory, too slow under load
	// for the default lane (`crates/cmdr-index/src/indexing/lifecycle/cover/DETAILS.md`
	// § "What the cold-drive cells pin about FSEvents").
	"cold_drive_tests::moves::real_journal::",
	"mapped_file_faults::real_image::",
	"fault_guard_real_image::",
}

// diskImageHandRunTestAtoms are the `disk-image` group's modules this lane
// never runs. The FAT/exFAT fixtures stay hand-run: an FSKit `msdos` unmount
// kernel-panicked a Mac (`crates/cmdr-index/src/indexing/tests/CLAUDE.md`), so
// nothing automated cycles one. The lane's filter SUBTRACTS them, so nextest
// keeps them out whatever a later atom in the union happens to spell.
var diskImageHandRunTestAtoms = []string{
	"indexing::tests::external_drive_fixture::",
}

// diskImageLaneFilter builds the lane's nextest filterset: the union of
// `laneAtoms`, minus every `handRunAtoms` entry.
func diskImageLaneFilter(laneAtoms, handRunAtoms []string) string {
	union := make([]string, len(laneAtoms))
	for i, atom := range laneAtoms {
		union[i] = "test(" + atom + ")"
	}
	// ❗ Parenthesised: `-` binds tighter than `+` in a nextest filterset, so
	// without the grouping the subtraction would apply to the last atom alone.
	filter := "(" + strings.Join(union, " + ") + ")"
	for _, atom := range handRunAtoms {
		filter += " - test(" + atom + ")"
	}
	return filter
}

// RunDiskImageTests runs the real-image tests on synthetic APFS and HFS+ disk
// images.
func RunDiskImageTests(ctx *CheckContext) (CheckResult, error) {
	if runtime.GOOS != "darwin" {
		return Success("skipped: macOS only"), nil
	}

	// The same question `desktop-rust-tests` asks cargo (`buildNextestLane`), so this
	// reuses that lane's warm build instead of paying its own compile.
	filter := diskImageLaneFilter(diskImageLaneTestAtoms, diskImageHandRunTestAtoms)
	lane, output, _, err := runNextestLane(ctx.RootDir, nextestRunOptions{RunIgnoredOnly: true}, filter)
	if lane == nil {
		return CheckResult{}, err
	}
	defer lane.remove()
	ctx.RecordTests(ParseNextestResults(output)...)
	if err != nil {
		return resolveRustFailure("disk-image tests failed",
			lane.contentionRunner(), LoadPerCore, trimRustTestProgress(output))
	}

	count := -1
	message := "All disk-image tests passed"
	if m := regexp.MustCompile(`(\d+) tests? run`).FindStringSubmatch(output); len(m) > 1 {
		count, _ = strconv.Atoi(m[1])
		message = fmt.Sprintf("%d disk-image %s passed", count, Pluralize(count, "test", "tests"))
	}
	// ❗ Zero is a failure, not a pass. Every atom is a module path, so a moved
	// or renamed module selects nothing, and nextest calls that a clean run.
	if count == 0 {
		return CheckResult{}, fmt.Errorf(
			"the filter %s selected no test; a real-image module moved or was renamed, so its pins would go unchecked",
			filter,
		)
	}

	// A retry-rescued run still exits 0: report it as a warning, as `desktop-rust-tests` does.
	if flaky := ParseFlakyTests(output); len(flaky) > 0 {
		return CheckResult{
			Code:    ResultWarning,
			Message: message + "; " + FlakySummary(flaky),
			Total:   count,
			Issues:  len(flaky),
			Changes: -1,
		}, nil
	}

	result := Success(message)
	result.Total = count
	return result, nil
}
