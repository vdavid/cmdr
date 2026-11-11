package checks

import (
	"debug/macho"
	"encoding/binary"
	"reflect"
	"testing"
	"time"
)

// dyldEnvironmentLoad builds the raw bytes of one `LC_DYLD_ENVIRONMENT` command the way
// `ld -dyld_env` writes it: an 8-byte header, a 4-byte offset to the string, then the
// NUL-terminated string padded to 8 bytes.
func dyldEnvironmentLoad(value string) macho.LoadBytes {
	const headerSize = 12
	size := headerSize + len(value) + 1
	size = (size + 7) &^ 7
	raw := make([]byte, size)
	binary.LittleEndian.PutUint32(raw[0:4], loadCmdDyldEnvironment)
	binary.LittleEndian.PutUint32(raw[4:8], uint32(size))
	binary.LittleEndian.PutUint32(raw[8:12], headerSize)
	copy(raw[headerSize:], value)
	return raw
}

func TestDyldEnvironmentEntriesReadsTheLinkerFlag(t *testing.T) {
	file := &macho.File{
		ByteOrder: binary.LittleEndian,
		Loads: []macho.Load{
			&macho.Dylib{Name: "/System/Library/Frameworks/WebKit.framework/Versions/A/WebKit"},
			dyldEnvironmentLoad(stagedWebKitDyldEnvironment),
			dyldEnvironmentLoad("DYLD_LIBRARY_PATH=/opt/lib"),
		},
	}
	got := dyldEnvironmentEntries(file)
	want := []string{stagedWebKitDyldEnvironment, "DYLD_LIBRARY_PATH=/opt/lib"}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("dyldEnvironmentEntries = %q, want %q", got, want)
	}
}

func TestDyldEnvironmentEntriesSurvivesAMalformedCommand(t *testing.T) {
	// An offset past the end of the command must not panic the whole check run.
	broken := dyldEnvironmentLoad(stagedWebKitDyldEnvironment)
	binary.LittleEndian.PutUint32(broken[8:12], uint32(len(broken)+40))
	file := &macho.File{ByteOrder: binary.LittleEndian, Loads: []macho.Load{broken}}
	if got := dyldEnvironmentEntries(file); len(got) != 0 {
		t.Fatalf("dyldEnvironmentEntries = %q, want nothing from a malformed command", got)
	}
}

func TestJudgeStagedWebKit(t *testing.T) {
	older := time.Date(2026, 9, 12, 23, 14, 0, 0, time.UTC)
	newer := older.Add(time.Hour)
	tests := []struct {
		name               string
		missing            bool
		isNamedBinary      bool
		binaryBuilt        time.Time
		buildScriptChanged time.Time
		want               stagedWebKitVerdict
	}{
		{"a binary carrying the entry passes however old it is", false, false, older, newer, stagedWebKitPresent},
		{"a local build newer than build.rs is held to it", true, false, newer, older, stagedWebKitMissing},
		// A worktree's cloned target/ predates its own checkout, so this is every fresh one.
		{"a local build older than build.rs answers for older source", true, false, older, newer, stagedWebKitUnjudgedStaleBuild},
		// The release binary is the one that ships; its mtime excuses nothing.
		{"the release workflow's binary is always judged", true, true, older, newer, stagedWebKitMissing},
		{"an unreadable build.rs leaves a local build judged", true, false, older, time.Time{}, stagedWebKitMissing},
	}
	for _, tt := range tests {
		if got := judgeStagedWebKit(tt.missing, tt.isNamedBinary, tt.binaryBuilt, tt.buildScriptChanged); got != tt.want {
			t.Errorf("%s: judgeStagedWebKit = %d, want %d", tt.name, got, tt.want)
		}
	}
}

func TestArchesMissingStagedWebKit(t *testing.T) {
	// A universal binary is two Mach-Os. Catalina only ever runs the Intel one, so a
	// flag that landed in one slice and not the other is still a broken Catalina build.
	entries := map[string][]string{
		"x86_64": {"DYLD_LIBRARY_PATH=/opt/lib"},
		"arm64":  {stagedWebKitDyldEnvironment},
	}
	if got, want := archesMissingStagedWebKit(entries), []string{"x86_64"}; !reflect.DeepEqual(got, want) {
		t.Fatalf("archesMissingStagedWebKit = %q, want %q", got, want)
	}

	entries["x86_64"] = append(entries["x86_64"], stagedWebKitDyldEnvironment)
	if got := archesMissingStagedWebKit(entries); len(got) != 0 {
		t.Fatalf("archesMissingStagedWebKit = %q, want none once every slice carries it", got)
	}
}

func TestSystemFrameworkName(t *testing.T) {
	tests := []struct {
		path string
		want string
	}{
		{"/System/Library/Frameworks/AppKit.framework/Versions/C/AppKit", "AppKit"},
		{"/System/Library/Frameworks/UniformTypeIdentifiers.framework/Versions/A/UniformTypeIdentifiers", "UniformTypeIdentifiers"},
		// A subframework is judged as itself, not as the umbrella it sits under:
		// the two ship and disappear on their own schedules.
		{"/System/Library/Frameworks/Quartz.framework/Frameworks/QuickLookUI.framework/Versions/A/QuickLookUI", "QuickLookUI"},
		{"/usr/lib/libSystem.B.dylib", ""},                        // the shared-cache basics
		{"/usr/lib/libc++.1.dylib", ""},                           // same
		{"@rpath/libcmdr_helper.dylib", ""},                       // ships inside the bundle
		{"@executable_path/../Frameworks/Foo.framework/Foo", ""},  // same
		{"/Library/Frameworks/Sparkle.framework/Sparkle", ""},     // not a system framework
		{"/System/Library/PrivateFrameworks/Apple.framework", ""}, // not the judged root
	}
	for _, tt := range tests {
		got, ok := systemFrameworkName(tt.path)
		if tt.want == "" {
			if ok {
				t.Errorf("systemFrameworkName(%q) = %q, want no framework", tt.path, got)
			}
			continue
		}
		if !ok || got != tt.want {
			t.Errorf("systemFrameworkName(%q) = %q (ok=%v), want %q", tt.path, got, ok, tt.want)
		}
	}
}

func TestFrameworksFromSortsAndDeduplicates(t *testing.T) {
	index := macOSFrameworkIndex{
		Floor:      "10.15",
		Frameworks: map[string]string{"AppKit": "10.0", "Vision": "10.13"},
	}
	loaded, unknown, err := frameworksFrom([]string{
		"/System/Library/Frameworks/Vision.framework/Versions/A/Vision",
		"/System/Library/Frameworks/AppKit.framework/Versions/C/AppKit",
		"/System/Library/Frameworks/AppKit.framework/Versions/C/AppKit",
		"/usr/lib/libSystem.B.dylib",
	}, index)
	if err != nil {
		t.Fatalf("frameworksFrom: %v", err)
	}
	if len(unknown) != 0 {
		t.Fatalf("unknown = %v, want none", unknown)
	}
	if len(loaded) != 2 || loaded[0].name != "AppKit" || loaded[1].name != "Vision" {
		t.Fatalf("loaded = %+v, want AppKit then Vision", loaded)
	}
}

func TestFrameworksFromReportsWhatItCannotJudge(t *testing.T) {
	// The load command that broke Catalina, against a list that doesn't name it.
	// Reporting it is the point: a framework nobody recorded a version for has to
	// fail, or the check silently stops covering whatever gets added next.
	index := macOSFrameworkIndex{Floor: "10.15", Frameworks: map[string]string{"AppKit": "10.0"}}
	_, unknown, err := frameworksFrom([]string{
		"/System/Library/Frameworks/UniformTypeIdentifiers.framework/Versions/A/UniformTypeIdentifiers",
	}, index)
	if err != nil {
		t.Fatalf("frameworksFrom: %v", err)
	}
	if len(unknown) != 1 || unknown[0] != "UniformTypeIdentifiers" {
		t.Fatalf("unknown = %v, want [UniformTypeIdentifiers]", unknown)
	}
}

func TestFrameworksFromRefusesAnUnreadableVersion(t *testing.T) {
	index := macOSFrameworkIndex{Floor: "10.15", Frameworks: map[string]string{"AppKit": "ancient"}}
	if _, _, err := frameworksFrom([]string{
		"/System/Library/Frameworks/AppKit.framework/Versions/C/AppKit",
	}, index); err == nil {
		t.Fatal("want an error for a version the check can't compare, got none")
	}
}

// The committed list is what every run is judged against, so a floor that has moved
// past it, or an entry above the floor, has to be caught here rather than at the
// next release.
func TestCommittedFrameworkVersionsMatchTheBundleFloor(t *testing.T) {
	rootDir := repoRootForTest(t)

	floor, err := macOSDeploymentFloor(rootDir)
	if err != nil {
		t.Fatalf("macOSDeploymentFloor: %v", err)
	}
	index, err := readFrameworkIndex(rootDir)
	if err != nil {
		t.Fatalf("readFrameworkIndex: %v", err)
	}
	if index.Floor != floor.String() {
		t.Fatalf("%s says floor %s, %s says %s", macOSFrameworkVersionsFile, index.Floor, tauriMacOSConfRelPath, floor)
	}
	for name, raw := range index.Frameworks {
		if _, ok := parseMacOSVersion(raw); !ok {
			t.Errorf("%s: %q is not a readable macOS version", name, raw)
		}
	}
}
