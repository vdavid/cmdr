package checks

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// writeHeader puts one fake framework header into a temp SDK-shaped tree.
func writeHeader(t *testing.T, dir, name, body string) string {
	t.Helper()
	headers := filepath.Join(dir, "Headers")
	if err := os.MkdirAll(headers, 0o755); err != nil {
		t.Fatalf("mkdir: %v", err)
	}
	mustWrite(t, filepath.Join(headers, name), body)
	return headers
}

func TestParseHeaderDeclarations_ReadsPropertyAvailability(t *testing.T) {
	// The real shape of the declaration that aborted the app: Foundation spells
	// the platform `macosx`, and the version rides after the property name.
	found := parseHeaderDeclarations(`
@interface NSLocale : NSObject
@property (nullable, readonly, copy) NSString *regionCode API_AVAILABLE(macosx(14.0), ios(17.0));
@property (readonly, copy) NSString *languageCode API_AVAILABLE(macosx(10.12), ios(10.0));
@property (readonly, copy) NSString *localeIdentifier;
@end
`)
	if got, want := found["regionCode"], (macOSVersion{major: 14}); got != want {
		t.Errorf("regionCode: got %v, want %v", got, want)
	}
	if got, want := found["languageCode"], (macOSVersion{major: 10, minor: 12}); got != want {
		t.Errorf("languageCode: got %v, want %v", got, want)
	}
	// No attribute at all means "always been there", which is what keeps a
	// missing macro from reading as a violation.
	if got, want := found["localeIdentifier"], (macOSVersion{}); got != want {
		t.Errorf("localeIdentifier: got %v, want %v", got, want)
	}
}

func TestParseHeaderDeclarations_JoinsSelectorKeywordsTheWayObjc2Does(t *testing.T) {
	found := parseHeaderDeclarations(`
- (void)addObserverForName:(NSString *)name object:(id)obj queue:(NSOperationQueue *)queue API_AVAILABLE(macos(14.0));
- (void)refresh API_AVAILABLE(macos(15.0));
`)
	if got, want := found["addObserverForName_object_queue"], (macOSVersion{major: 14}); got != want {
		t.Errorf("multi-part selector: got %v, want %v", got, want)
	}
	if got, want := found["refresh"], (macOSVersion{major: 15}); got != want {
		t.Errorf("no-argument selector: got %v, want %v", got, want)
	}
}

func TestParseHeaderDeclarations_IgnoresProse(t *testing.T) {
	// A sentence with a colon in it reads as a selector keyword unless the
	// comments come out first, and Apple's headers are full of them.
	found := parseHeaderDeclarations(`
/// Returns the region code: for example, "GB".
// Note: this one is old.
@property (readonly, copy) NSString *regionCode API_AVAILABLE(macos(14.0));
`)
	if _, unwanted := found["example"]; unwanted {
		t.Error("prose inside a comment was read as a declaration")
	}
	if got, want := found["regionCode"], (macOSVersion{major: 14}); got != want {
		t.Errorf("regionCode: got %v, want %v", got, want)
	}
}

func TestSelectorsNewerThan_KeepsOnlyWhatTheFloorCantReach(t *testing.T) {
	dir := t.TempDir()
	headers := writeHeader(t, dir, "Fake.h", `
@property (readonly, copy) NSString *regionCode API_AVAILABLE(macos(14.0));
@property (readonly, copy) NSString *scriptCode API_AVAILABLE(macos(10.12));
@property (readonly) BOOL sinceTheFloorItself API_AVAILABLE(macos(12.0));
`)
	newer, err := selectorsNewerThan([]string{headers}, macOSVersion{major: 12})
	if err != nil {
		t.Fatalf("selectorsNewerThan: %v", err)
	}
	if _, ok := newer["regionCode"]; !ok {
		t.Error("a macOS 14 selector has to be listed")
	}
	if _, ok := newer["scriptCode"]; ok {
		t.Error("a selector older than the floor was listed")
	}
	// The floor itself is reachable: `minimumSystemVersion` is what we ship on.
	if _, ok := newer["sinceTheFloorItself"]; ok {
		t.Error("a selector introduced in the floor version was listed")
	}
}

func TestSelectorsNewerThan_TheOldestDeclarationWins(t *testing.T) {
	// The scan can't know a Rust receiver's class, so a name that some class has
	// carried since 10.x can't be called a violation on another class's account.
	dir := t.TempDir()
	headers := writeHeader(t, dir, "Fake.h", `
@property (readonly) NSString *sharedName API_AVAILABLE(macos(15.0));
@property (readonly) NSString *sharedName API_AVAILABLE(macos(10.13));
`)
	newer, err := selectorsNewerThan([]string{headers}, macOSVersion{major: 12})
	if err != nil {
		t.Fatalf("selectorsNewerThan: %v", err)
	}
	if _, ok := newer["sharedName"]; ok {
		t.Error("a name an older class also declares must not be listed")
	}
}

func TestSelectorsNewerThan_SkipsSingleLowercaseWords(t *testing.T) {
	// `bytes`, `close`, and `title` are selectors AND ordinary Rust method names.
	// Listing them would bury the real finding in noise.
	dir := t.TempDir()
	headers := writeHeader(t, dir, "Fake.h", `
@property (readonly) NSData *bytes API_AVAILABLE(macos(15.0));
@property (readonly) NSString *bytesRemaining API_AVAILABLE(macos(15.0));
`)
	newer, err := selectorsNewerThan([]string{headers}, macOSVersion{major: 12})
	if err != nil {
		t.Fatalf("selectorsNewerThan: %v", err)
	}
	if _, ok := newer["bytes"]; ok {
		t.Error("a single lowercase word must stay out of the list")
	}
	if _, ok := newer["bytesRemaining"]; !ok {
		t.Error("a camelCase selector has to stay in the list")
	}
}

func TestSelectorsNewerThan_EmptyParseIsAFailure(t *testing.T) {
	// An empty answer scans every source and finds nothing, which reads exactly
	// like a pass.
	if _, err := selectorsNewerThan([]string{t.TempDir()}, macOSVersion{major: 12}); err == nil {
		t.Fatal("expected an error when the headers yield no declarations")
	}
}

// availabilityScanFixture writes Rust files into a temp tree and scans them for the given
// too-new selectors.
func availabilityScanFixture(t *testing.T, newer map[string]macOSVersion, files map[string]string) []availabilitySite {
	t.Helper()
	root := t.TempDir()
	src := filepath.Join(root, "src")
	for rel, body := range files {
		mustWrite(t, filepath.Join(src, rel), body)
	}
	sites, scanned, err := scanForNewSelectors(root, src, newer)
	if err != nil {
		t.Fatalf("scanForNewSelectors: %v", err)
	}
	if scanned != len(files) {
		t.Errorf("scanned %d files, wrote %d", scanned, len(files))
	}
	return sites
}

func TestScanForNewSelectors_FindsTheCallAndNamesIt(t *testing.T) {
	newer := map[string]macOSVersion{"regionCode": {major: 14}}
	sites := availabilityScanFixture(t, newer, map[string]string{
		"intl.rs": "use objc2_foundation::NSLocale;\nfn f() {\n    let r = locale.regionCode();\n}\n",
	})
	if len(sites) != 1 {
		t.Fatalf("expected one site, got %d", len(sites))
	}
	if sites[0].line != 3 || sites[0].selector != "regionCode" {
		t.Errorf("got %+v, want regionCode on line 3", sites[0])
	}
}

func TestScanForNewSelectors_ReadsOnlyObjc2Files(t *testing.T) {
	// Elsewhere a matching name is a Rust method that happens to share it, and
	// the receiver can't be an Objective-C object.
	newer := map[string]macOSVersion{"regionCode": {major: 14}}
	sites := availabilityScanFixture(t, newer, map[string]string{
		"plain.rs": "fn f(profile: &Profile) -> String {\n    profile.regionCode()\n}\n",
	})
	if len(sites) != 0 {
		t.Fatalf("expected no sites in a file that never mentions objc2, got %+v", sites)
	}
}

func TestScanForNewSelectors_SkipsCommentsAndCatchesMsgSend(t *testing.T) {
	newer := map[string]macOSVersion{"regionCode": {major: 14}}
	sites := availabilityScanFixture(t, newer, map[string]string{
		"commented.rs": "use objc2::msg_send;\n// `regionCode()` is macOS 14+, so we don't call it\nfn f() {}\n",
		"hand.rs":      "use objc2::msg_send;\nfn f() {\n    let r: *mut NSString = unsafe { msg_send![locale, regionCode] };\n}\n",
	})
	if len(sites) != 1 {
		t.Fatalf("expected only the msg_send site, got %+v", sites)
	}
	if !strings.HasSuffix(sites[0].relPath, "hand.rs") {
		t.Errorf("got %s, want the msg_send file", sites[0].relPath)
	}
}

func TestMacOSDeploymentFloor_ReadsTheBundleFloor(t *testing.T) {
	root := t.TempDir()
	mustWrite(t, filepath.Join(root, filepath.FromSlash(tauriConfRelPath)),
		`{"bundle": {"macOS": {"minimumSystemVersion": "12.0"}}}`)
	floor, err := macOSDeploymentFloor(root)
	if err != nil {
		t.Fatalf("macOSDeploymentFloor: %v", err)
	}
	if floor.String() != "12.0" {
		t.Errorf("got %s, want 12.0", floor)
	}
}

func TestMacOSDeploymentFloor_MissingFloorIsAFailure(t *testing.T) {
	root := t.TempDir()
	mustWrite(t, filepath.Join(root, filepath.FromSlash(tauriConfRelPath)), `{"bundle": {"macOS": {}}}`)
	if _, err := macOSDeploymentFloor(root); err == nil {
		t.Fatal("expected an error when the bundle names no minimum system version")
	}
}

func TestParseMacOSVersion(t *testing.T) {
	for input, want := range map[string]macOSVersion{
		"12":     {major: 12},
		"12.0":   {major: 12},
		"13.7":   {major: 13, minor: 7},
		"13.7.8": {major: 13, minor: 7},
	} {
		got, ok := parseMacOSVersion(input)
		if !ok || got != want {
			t.Errorf("%q: got %v (ok=%v), want %v", input, got, ok, want)
		}
	}
	for _, input := range []string{"", "sonoma", "."} {
		if _, ok := parseMacOSVersion(input); ok {
			t.Errorf("%q: expected no version", input)
		}
	}
}

func TestSelectorIndexRoundTrips(t *testing.T) {
	path := filepath.Join(t.TempDir(), macOSAvailabilitySelectorsFile)
	index := macOSSelectorIndex{Floor: "12.0", SDK: "MacOSX26.5.sdk", Selectors: map[string]string{"regionCode": "14.0"}}
	if err := writeSelectorIndex(path, index); err != nil {
		t.Fatalf("writeSelectorIndex: %v", err)
	}
	stored, err := readSelectorIndex(path)
	if err != nil {
		t.Fatalf("readSelectorIndex: %v", err)
	}
	if !sameSelectorIndex(stored, index) {
		t.Errorf("round trip changed the index: %+v", stored)
	}
	// The SDK name is deliberately not compared: an Xcode update that adds no API
	// to the frameworks we bind shouldn't rewrite the file.
	index.SDK = "MacOSX27.0.sdk"
	if !sameSelectorIndex(stored, index) {
		t.Error("a new SDK name alone must not count as a change")
	}
	index.Selectors["somethingElse"] = "15.0"
	if sameSelectorIndex(stored, index) {
		t.Error("a new selector has to count as a change")
	}
}

func TestReadSelectorIndex_EmptyIsAFailure(t *testing.T) {
	path := filepath.Join(t.TempDir(), macOSAvailabilitySelectorsFile)
	mustWrite(t, path, `{"floor": "12.0", "sdk": "MacOSX26.5.sdk", "selectors": {}}`)
	if _, err := readSelectorIndex(path); err == nil {
		t.Fatal("expected an error for a list that can't fail")
	}
}

func TestBoundFrameworkHeaderDirs_UnmappedCrateIsAFailure(t *testing.T) {
	root := t.TempDir()
	seedAppFixtureWorkspace(t, root)
	mustWrite(t, filepath.Join(root, "apps", "desktop", "src-tauri", "Cargo.toml"),
		"[package]\nname = \"cmdr\"\nversion = \"0.0.0\"\n\n[dependencies]\nobjc2-foundation = \"0.3\"\nobjc2-nosuchthing = \"0.3\"\n")

	sdk := t.TempDir()
	writeHeader(t, filepath.Join(sdk, "System", "Library", "Frameworks", "Foundation.framework"), "NSLocale.h", "@end\n")

	_, err := boundFrameworkHeaderDirs(root, sdk)
	if err == nil {
		t.Fatal("expected an error for a binding crate with no framework")
	}
	if !strings.Contains(err.Error(), "objc2-nosuchthing") {
		t.Errorf("expected the unmapped crate to be named, got: %v", err)
	}
}

func TestBoundFrameworkHeaderDirs_MapsCrateNamesToFrameworks(t *testing.T) {
	root := t.TempDir()
	seedAppFixtureWorkspace(t, root)
	mustWrite(t, filepath.Join(root, "apps", "desktop", "src-tauri", "Cargo.toml"),
		"[package]\nname = \"cmdr\"\nversion = \"0.0.0\"\n\n[dependencies]\nobjc2 = \"0.6\"\nobjc2-app-kit = \"0.3\"\n")

	frameworks := filepath.Join(sdkFixture(t), "System", "Library", "Frameworks")
	writeHeader(t, filepath.Join(frameworks, "AppKit.framework"), "NSColor.h", "@end\n")

	dirs, err := boundFrameworkHeaderDirs(root, filepath.Dir(filepath.Dir(filepath.Dir(frameworks))))
	if err != nil {
		t.Fatalf("boundFrameworkHeaderDirs: %v", err)
	}
	if len(dirs) != 1 || !strings.Contains(dirs[0], "AppKit.framework") {
		t.Errorf("got %v, want the AppKit headers (and nothing for the runtime crate)", dirs)
	}
}

// sdkFixture is a temp dir standing in for an SDK root.
func sdkFixture(t *testing.T) string {
	t.Helper()
	return t.TempDir()
}
