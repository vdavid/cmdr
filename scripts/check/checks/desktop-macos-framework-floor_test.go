package checks

import "testing"

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
		t.Fatalf("%s says floor %s, %s says %s", macOSFrameworkVersionsFile, index.Floor, tauriConfRelPath, floor)
	}
	for name, raw := range index.Frameworks {
		if _, ok := parseMacOSVersion(raw); !ok {
			t.Errorf("%s: %q is not a readable macOS version", name, raw)
		}
	}
}
