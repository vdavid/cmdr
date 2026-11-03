package checks

import (
	"slices"
	"strings"
	"testing"
)

// A run Miri stopped mid-test, shaped like `cargo miri test`'s real output.
const miriStoppedRun = `running 3 tests
test attr_buf::tests::a_buffer_too_short_for_a_length_is_bad ... ok
test apple_double::tests::devfs_reports_no_native_xattrs ... ignored, calls ` + "`getattrlist`" + `, which Miri can't cross
test attr_buf::tests::a_record_in_a_word_buffer_decodes_through_its_bytes ... error: Undefined Behavior: constructing invalid value of type &[u8]
   --> crates/cmdr-fs/src/attr_buf.rs:326:14
error: aborting due to 1 previous error
`

func TestMiriLinesRecordEveryTestAndReturnOnlyThePassedOnes(t *testing.T) {
	records, passed := parseMiriTestLines("cmdr-fs", miriStoppedRun)

	want := map[string]TestOutcome{
		"cmdr-fs::attr_buf::tests::a_buffer_too_short_for_a_length_is_bad":              TestPassed,
		"cmdr-fs::apple_double::tests::devfs_reports_no_native_xattrs":                  TestSkipped,
		"cmdr-fs::attr_buf::tests::a_record_in_a_word_buffer_decodes_through_its_bytes": TestFailed,
	}
	if len(records) != len(want) {
		t.Fatalf("got %d records, want %d: %+v", len(records), len(want), records)
	}
	for _, record := range records {
		if want[record.ID] != record.Outcome {
			t.Errorf("%s: got %q, want %q", record.ID, record.Outcome, want[record.ID])
		}
	}
	if !slices.Equal(passed, []string{"attr_buf::tests::a_buffer_too_short_for_a_length_is_bad"}) {
		t.Errorf("passed = %v", passed)
	}
}

func TestMiriReportStartsAtTheTestMiriStoppedIn(t *testing.T) {
	report := miriReport(miriStoppedRun)
	if !strings.HasPrefix(report, "test attr_buf::tests::a_record_in_a_word_buffer_decodes_through_its_bytes ... error:") {
		t.Errorf("report starts with the wrong line:\n%s", report)
	}
	if strings.Contains(report, "a_buffer_too_short_for_a_length_is_bad") {
		t.Errorf("a passing test's line leaked into the report:\n%s", report)
	}
}

func TestMiriReportPassesAnOutputWithNoErrorThrough(t *testing.T) {
	output := "warning: something odd\nfinished"
	if got := miriReport(output); got != output {
		t.Errorf("got %q", got)
	}
}

// Every target's module ends in `::`, or its filter could match a sibling module that
// merely shares a prefix, and a renamed module would go on passing on someone else's tests.
func TestEveryMiriTargetPinsOneModule(t *testing.T) {
	for _, target := range miriTargets {
		if !strings.HasSuffix(target.module, "::") {
			t.Errorf("%s %q doesn't end in `::`", target.pkg, target.module)
		}
		if target.why == "" {
			t.Errorf("%s %q says nothing about why Miri runs it", target.pkg, target.module)
		}
	}
}
