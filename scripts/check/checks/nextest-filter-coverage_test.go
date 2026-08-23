package checks

import (
	"strings"
	"testing"
)

func TestScanNextestFilters(t *testing.T) {
	tests := []struct {
		name   string
		config string
		names  []string
		want   []string // "atom → leaf's real home", or "atom → " when the leaf is gone too
	}{
		{
			name:   "a filter that still selects a test is silent",
			config: "filter = 'test(downloads::watcher_test::dropping_a_file_emits_one_event)'",
			names:  []string{"downloads::watcher_test::dropping_a_file_emits_one_event"},
		},
		{
			// The rot this check exists for: the tests moved to a sibling
			// `watcher_test.rs`, the filter kept the old inline-module path, and the
			// override silently stopped applying.
			name:   "a filter whose module path went stale names where the test went",
			config: "filter = 'test(downloads::watcher::tests::dropping_a_file_emits_one_event)'",
			names:  []string{"downloads::watcher_test::dropping_a_file_emits_one_event"},
			want: []string{
				"downloads::watcher::tests::dropping_a_file_emits_one_event → downloads::watcher_test::dropping_a_file_emits_one_event",
			},
		},
		{
			// A module split inserts a segment; the leaf is untouched, so only the
			// whole-path match can catch it.
			name:   "a filter that lost a module split is a finding",
			config: "filter = 'test(cover::cold_drive_tests::a_change_inside_a_walked_branch)'",
			names:  []string{"cover::cold_drive_tests::branches::a_change_inside_a_walked_branch_reaches_the_index"},
			want: []string{
				"cover::cold_drive_tests::a_change_inside_a_walked_branch → cover::cold_drive_tests::branches::a_change_inside_a_walked_branch_reaches_the_index",
			},
		},
		{
			// A module PREFIX filter (trailing `::`) rots the same way, and its leaf
			// is the module segment rather than a function name.
			name:   "a stale module prefix is a finding",
			config: "filter = 'test(indexing::external_drive_fixture::)'",
			names:  []string{"indexing::tests::external_drive_fixture::tests::parses_hdiutil_attach_output"},
			want: []string{
				"indexing::external_drive_fixture:: → indexing::tests::external_drive_fixture::tests::parses_hdiutil_attach_output",
			},
		},
		{
			name: "every atom of a multi-line filter is checked",
			config: `filter = '''
    test(file_system::listing::caching_reaper_test::)
  + test(downloads::commands::tests::go_to_latest_)
'''`,
			names: []string{
				"file_system::listing::caching_reaper_test::reaper_evicts_stale_listing",
				"downloads::commands::gone::go_to_latest_returns_empty",
			},
			want: []string{
				"downloads::commands::tests::go_to_latest_ → downloads::commands::gone::go_to_latest_returns_empty",
			},
		},
		{
			// A bare name (no `::`) is what the SMB integration filters use; it stays
			// valid however the module around it is rearranged.
			name:   "a bare test name matches wherever it lives",
			config: "filter = 'test(smb_integration_volume_id_is_per_mount)'",
			names:  []string{"network::mount::tests::smb_integration_volume_id_is_per_mount_not_per_path_shape"},
		},
		{
			// Nothing by that name anywhere: a deleted test, or one this platform
			// doesn't compile. Still a finding, but it can't point anywhere.
			name:   "a filter naming nothing at all is reported without a destination",
			config: "filter = 'test(a_test_that_was_deleted)'",
			names:  []string{"downloads::watcher_test::dropping_a_file_emits_one_event"},
			want:   []string{"a_test_that_was_deleted → "},
		},
		{
			name:   "a package atom is nextest's own business, not ours",
			config: "filter = 'package(cmdr-fsevent-stream) & test(tests::must_receive_fs_events)'",
			names:  []string{"tests::must_receive_fs_events_tokio"},
		},
		{
			name:   "the same stale atom in two overrides is reported once",
			config: "filter = 'test(gone::tests::a)'\nfilter = 'test(gone::tests::a)'",
			names:  []string{"here::a"},
			want:   []string{"gone::tests::a → here::a"},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			var got []string
			for _, f := range scanNextestFilters(tt.config, tt.names) {
				got = append(got, f.atom+" → "+f.movedTo)
			}
			if strings.Join(got, "\n") != strings.Join(tt.want, "\n") {
				t.Errorf("got:\n  %s\nwant:\n  %s", strings.Join(got, "\n  "), strings.Join(tt.want, "\n  "))
			}
		})
	}
}

// The config's own prose explains the substring trap by writing `test(x)`, and
// the file's comments name filters while discussing them. Scanning those would
// invent atoms that answer to nobody; only a real `filter =` line declares one.
func TestScanNextestFiltersIgnoresProse(t *testing.T) {
	config := `# ` + "`test(gone::from::the::tree)`" + ` is a substring match, so a stale prefix matches nothing.
# An older filter = 'test(also::not::real)' shape is quoted here as an example.
filter = 'test(downloads::watcher_test::eligible_create_emits)'
`
	names := []string{"downloads::watcher_test::eligible_create_emits"}
	if findings := scanNextestFilters(config, names); len(findings) != 0 {
		t.Errorf("prose invented %d atom(s): %v", len(findings), findings)
	}
}

func TestParseNextestListNames(t *testing.T) {
	// `cargo nextest list -T human` groups the names under a binary-id header.
	out := `cmdr:
    downloads::watcher_test::dropping_a_file_emits_one_event
    downloads::watcher_test::eligible_create_emits
cmdr-index:
    indexing::lifecycle::cover::cold_drive_tests::branches::a_change_inside
`
	names := ParseNextestListNames(out)
	if len(names) != 3 {
		t.Fatalf("got %d names, want 3: %v", len(names), names)
	}
	if names[0] != "downloads::watcher_test::dropping_a_file_emits_one_event" {
		t.Errorf("first name is %q", names[0])
	}
	if names[2] != "indexing::lifecycle::cover::cold_drive_tests::branches::a_change_inside" {
		t.Errorf("third name is %q", names[2])
	}
}

// A filter for a test only one platform compiles selects nothing everywhere
// else, which is not rot. The opt-out therefore carries a platform scope, and
// the scope decides where it applies: on the named platform the filter must
// still select something (that's where the override does its work and where a
// rename can rot it), and everywhere else it's excused.
func TestExcuseNextestFindings(t *testing.T) {
	const scopedConfig = `[[profile.default.overrides]]
# The disk-image fixtures attach a real image via hdiutil.
# allowed-unmatched-nextest-filter: macos-only, hdiutil has no Linux counterpart
filter = 'test(indexing::tests::external_drive_fixture::)'
test-group = 'disk-image'
`
	const unscopedConfig = `[[profile.default.overrides]]
# allowed-unmatched-nextest-filter: parked while the rewrite lands
filter = 'test(some::parked::test)'
`

	tests := []struct {
		name          string
		config        string
		names         []string
		goos          string
		wantUnexcused []string
		wantOrphans   int
	}{
		{
			name:   "a macos-only filter selecting nothing on linux is excused",
			config: scopedConfig,
			names:  []string{"downloads::watcher_test::eligible_create_emits"},
			goos:   "linux",
		},
		{
			// The whole point of scoping: on macOS the test exists, so the filter
			// must still find it. A rename there is exactly the rot this check was
			// built for, and the scope must not hide it.
			name:          "the same filter selecting nothing ON macos is still rot",
			config:        scopedConfig,
			names:         []string{"downloads::watcher_test::eligible_create_emits"},
			goos:          "darwin",
			wantUnexcused: []string{"indexing::tests::external_drive_fixture::"},
		},
		{
			name:   "a matching macos-only filter on macos is silent and not an orphan",
			config: scopedConfig,
			names:  []string{"indexing::tests::external_drive_fixture::tests::exfat_attaches"},
			goos:   "darwin",
		},
		{
			// Inert here, and reporting it as unused would force the reason to be
			// deleted on Linux and re-added on macOS.
			name:   "a scoped opt-out is not an orphan on the platform it excludes",
			config: scopedConfig,
			names:  []string{"indexing::tests::external_drive_fixture::tests::exfat_attaches"},
			goos:   "linux",
		},
		{
			// Unscoped opt-outs keep their old contract: once the filter matches
			// again, the reason has to go.
			name:        "an unscoped opt-out over a filter that matches is an orphan",
			config:      unscopedConfig,
			names:       []string{"some::parked::test"},
			goos:        "darwin",
			wantOrphans: 1,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			unexcused, orphans := excuseNextestFindings(tt.config, tt.names, tt.goos)
			var got []string
			for _, f := range unexcused {
				got = append(got, f.atom)
			}
			if strings.Join(got, "\n") != strings.Join(tt.wantUnexcused, "\n") {
				t.Errorf("unexcused: got %v, want %v", got, tt.wantUnexcused)
			}
			if len(orphans) != tt.wantOrphans {
				t.Errorf("got %d orphan(s), want %d: %v", len(orphans), tt.wantOrphans, orphans)
			}
		})
	}
}
