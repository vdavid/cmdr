package checks

import (
	"os"
	"path/filepath"
	"reflect"
	"regexp"
	"sort"
	"strings"
	"testing"
)

// The filter is a parenthesised union with every hand-run atom subtracted, so
// nextest itself keeps the FAT/exFAT fixtures out whatever a later atom spells.
func TestDiskImageLaneFilterSubtractsTheHandRunFixtures(t *testing.T) {
	got := diskImageLaneFilter(
		[]string{"a::real::", "b::pins::"},
		[]string{"c::fat_fixture::"},
	)
	want := "(test(a::real::) + test(b::pins::)) - test(c::fat_fixture::)"
	if got != want {
		t.Errorf("diskImageLaneFilter = %q, want %q", got, want)
	}
}

// The lane runs exactly the M1 real-image paths, and never the FAT/exFAT
// fixtures. A later milestone adds its path to both lists at once.
func TestDiskImageLaneRunsTheRealImagePaths(t *testing.T) {
	want := []string{
		"testing::disk_images::real_images::",
		"file_system::volume::eject::real_image::",
		"file_system::index_provider::real_image::",
		"file_system::write_operations::transfer::real_image::",
		"indexing::tests::vanish_tests::",
		"volumes::unmount_approver::real_image::",
		"volumes::rename_real_image::",
	}
	if !reflect.DeepEqual(diskImageLaneTestAtoms, want) {
		t.Errorf("diskImageLaneTestAtoms = %q, want %q", diskImageLaneTestAtoms, want)
	}
	if !reflect.DeepEqual(diskImageHandRunTestAtoms, []string{"indexing::tests::external_drive_fixture::"}) {
		t.Errorf("diskImageHandRunTestAtoms = %q, want only the FAT/exFAT fixture", diskImageHandRunTestAtoms)
	}
	for _, lane := range diskImageLaneTestAtoms {
		for _, handRun := range diskImageHandRunTestAtoms {
			// `test(x)` is a substring match, so a lane atom inside a hand-run
			// path would select the hand-run tests too (the subtraction still
			// drops them, but the union would be lying about what it names).
			if strings.Contains(handRun, lane) || strings.Contains(lane, handRun) {
				t.Errorf("lane atom %q overlaps hand-run atom %q", lane, handRun)
			}
		}
	}
	filter := diskImageLaneFilter(diskImageLaneTestAtoms, diskImageHandRunTestAtoms)
	if !strings.HasSuffix(filter, " - test(indexing::tests::external_drive_fixture::)") {
		t.Errorf("the lane's filter %q doesn't subtract the FAT/exFAT fixture", filter)
	}
}

// The overrides that give a real-image test its serialization and 30 s cap, and
// the lane that runs it, must name the same paths: a path in the lane with no
// override runs under the global 8 s cap in parallel with other image tests, and
// an override the lane doesn't name is a real-image test nothing runs.
func TestDiskImageLaneMatchesTheDiskImageNextestGroup(t *testing.T) {
	raw, err := os.ReadFile(filepath.Join(repoRootForTest(t), ".config", "nextest.toml"))
	if err != nil {
		t.Fatalf("read .config/nextest.toml: %v", err)
	}
	group := diskImageGroupAtoms(string(raw))
	if len(group) == 0 {
		t.Fatal("found no `disk-image` override in .config/nextest.toml; the parse is broken, not the config")
	}

	named := append(append([]string{}, diskImageLaneTestAtoms...), diskImageHandRunTestAtoms...)
	sort.Strings(named)
	sort.Strings(group)
	if !reflect.DeepEqual(named, group) {
		t.Errorf("the lane and the hand-run list name %q, but the `disk-image` overrides name %q", named, group)
	}
}

func TestDiskImageGroupAtomsReadsOnlyDiskImageOverrides(t *testing.T) {
	config := `
[test-groups]
disk-image = { max-threads = 1 }

[[profile.default.overrides]]
# filter = 'test(commented::out::)'
filter = 'test(one::real::)'
test-group = 'disk-image'
slow-timeout = { period = "30s", terminate-after = 1 }

[[profile.default.overrides]]
filter = 'test(watcher::tests::)'
test-group = 'real-notify'

[[profile.default.overrides]]
filter = 'test(two::pins::) | test(three::pins::)'
test-group = "disk-image"
`
	got := diskImageGroupAtoms(config)
	want := []string{"one::real::", "two::pins::", "three::pins::"}
	if !reflect.DeepEqual(got, want) {
		t.Errorf("diskImageGroupAtoms = %q, want %q", got, want)
	}
}

// nextestTestGroupLine matches an override's `test-group = '<name>'` line.
var nextestTestGroupLine = regexp.MustCompile(`^\s*test-group\s*=\s*['"]([^'"]+)['"]`)

// diskImageGroupAtoms returns the `test(...)` atoms of every
// `[[profile.default.overrides]]` block that puts its tests in the `disk-image`
// group, in declaration order.
func diskImageGroupAtoms(config string) []string {
	var atoms []string
	for _, block := range strings.Split(config, "[[profile.") {
		inGroup := false
		for _, line := range strings.Split(block, "\n") {
			if m := nextestTestGroupLine.FindStringSubmatch(line); m != nil && m[1] == "disk-image" {
				inGroup = true
			}
		}
		if !inGroup {
			continue
		}
		for _, m := range nextestTestAtomPattern.FindAllStringSubmatch(filterDeclarationsIn(block), -1) {
			atoms = append(atoms, strings.TrimSpace(m[1]))
		}
	}
	return atoms
}
