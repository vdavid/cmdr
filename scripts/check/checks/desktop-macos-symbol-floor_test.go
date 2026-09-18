package checks

import (
	"os"
	"path/filepath"
	"reflect"
	"runtime"
	"testing"
)

// symbolHeaderDir drops one fake SDK header into a temp framework tree and
// returns the `Headers` directory the parser walks.
func symbolHeaderDir(t *testing.T, name, body string) string {
	t.Helper()
	return writeHeader(t, t.TempDir(), name, body)
}

func TestSymbolsNewerThanReadsEachDeclarationKind(t *testing.T) {
	// The shapes Apple actually writes, including the one that broke Catalina: the
	// declaration spans three lines and the name sits alone on the middle one.
	dir := symbolHeaderDir(t, "Shapes.h", `
extern
const mach_port_t kIOMainPortDefault
__API_AVAILABLE(macos(12.0), ios(15.0));

extern
const mach_port_t kIOMasterPortDefault
__API_AVAILABLE(macCatalyst(1.0))
__API_DEPRECATED_WITH_REPLACEMENT("kIOMainPortDefault", macos(10.0, 12.0));

kern_return_t IOMainPort( mach_port_t bootstrapPort, mach_port_t * mainPort )
__API_AVAILABLE(macos(12.0));

API_AVAILABLE(macos(13.0)) @interface NSFancyThing : NSObject
- (void)oldEnoughMethod API_AVAILABLE(macos(14.0));
@property (readonly) NSInteger someProperty API_AVAILABLE(macos(15.0));
@end

extern const CFStringRef kSomeNewKey API_AVAILABLE(macos(11.0));
`)

	got, err := symbolsNewerThan([]string{dir}, macOSVersion{major: 10, minor: 15})
	if err != nil {
		t.Fatalf("symbolsNewerThan: %v", err)
	}

	want := map[string]macOSVersion{
		"kIOMainPortDefault":        {major: 12, minor: 0},
		"IOMainPort":                {major: 12, minor: 0},
		"OBJC_CLASS_$_NSFancyThing": {major: 13, minor: 0},
		"kSomeNewKey":               {major: 11, minor: 0},
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("symbolsNewerThan = %v, want %v", got, want)
	}
}

func TestSymbolsNewerThanIgnoresWhatIsNotALinkedSymbol(t *testing.T) {
	// Selectors and properties belong to `desktop-rust-macos-availability`: they're
	// resolved by the Objective-C runtime, never by dyld, so a too-new one raises an
	// exception rather than refusing to launch. Enums and typedefs are values and
	// types, and a mention inside a comment is prose.
	dir := symbolHeaderDir(t, "NotSymbols.h", `
/* Discussion: kFakeCommentedSymbol API_AVAILABLE(macos(12.0)) is only prose. */
// Another mention of kLineCommentSymbol API_AVAILABLE(macos(13.0)).

typedef NS_ENUM(NSInteger, NSFancyMode) {
    NSFancyModeQuiet API_AVAILABLE(macos(12.0)) = 0,
} API_AVAILABLE(macos(12.0));

@interface NSOldThing (FancyAdditions)
- (void)methodOnACategory API_AVAILABLE(macos(12.0));
@end
`)

	got, err := symbolsNewerThan([]string{dir}, macOSVersion{major: 10, minor: 15})
	if err != nil {
		t.Fatalf("symbolsNewerThan: %v", err)
	}
	if len(got) != 0 {
		t.Fatalf("symbolsNewerThan = %v, want nothing", got)
	}
}

func TestSymbolsNewerThanKeepsTheOldestAnswer(t *testing.T) {
	// A symbol redeclared across headers (or across platform blocks) is only as new
	// as its oldest macOS annotation; taking the newest would invent a violation.
	dir := symbolHeaderDir(t, "Twice.h", `
extern const int kTwiceDeclared API_AVAILABLE(macos(13.0));
extern const int kTwiceDeclared API_AVAILABLE(macos(11.0));
`)
	got, err := symbolsNewerThan([]string{dir}, macOSVersion{major: 10, minor: 15})
	if err != nil {
		t.Fatalf("symbolsNewerThan: %v", err)
	}
	if want := (macOSVersion{major: 11, minor: 0}); got["kTwiceDeclared"] != want {
		t.Fatalf("kTwiceDeclared = %v, want %v", got["kTwiceDeclared"], want)
	}
}

func TestViolatingSymbolsIntersectsAndSorts(t *testing.T) {
	newer := map[string]macOSVersion{
		"kIOMainPortDefault": {major: 12, minor: 0},
		"NSFancyFunction":    {major: 13, minor: 0},
		"kNeverImported":     {major: 14, minor: 0},
	}
	imported := []string{"_kIOMainPortDefault", "_NSFancyFunction", "_CFRelease"}

	got := violatingSymbols(imported, newer, nil)
	want := []symbolFloorViolation{
		{symbol: "NSFancyFunction", needs: macOSVersion{major: 13, minor: 0}},
		{symbol: "kIOMainPortDefault", needs: macOSVersion{major: 12, minor: 0}},
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("violatingSymbols = %+v, want %+v", got, want)
	}
}

func TestViolatingSymbolsHonorsTheAllowlist(t *testing.T) {
	newer := map[string]macOSVersion{"kIOMainPortDefault": {major: 12, minor: 0}}
	imported := []string{"_kIOMainPortDefault"}
	exempt := map[string]string{"kIOMainPortDefault": "weak-linked and gated on macos_at_least(12, 0)"}
	if got := violatingSymbols(imported, newer, exempt); len(got) != 0 {
		t.Fatalf("violatingSymbols = %+v, want nothing once exempt", got)
	}
}

// TestRealSDKDeclaresTheSymbolThatBrokeCatalina holds the parser against the real
// headers: a rewrite that stops understanding Apple's declaration shapes would
// otherwise pass every synthetic test above and find nothing in the SDK.
func TestRealSDKDeclaresTheSymbolThatBrokeCatalina(t *testing.T) {
	if runtime.GOOS != "darwin" || !CommandExists("xcrun") {
		t.Skip("needs the macOS SDK")
	}
	sdkPath, err := macOSSDKPath(&CheckContext{RootDir: "."})
	if err != nil {
		t.Skipf("no SDK: %v", err)
	}
	headers := filepath.Join(sdkPath, "System", "Library", "Frameworks", "IOKit.framework", "Headers")
	if _, statErr := os.Stat(headers); statErr != nil {
		t.Skipf("no IOKit headers: %v", statErr)
	}

	newer, err := symbolsNewerThan([]string{headers}, macOSVersion{major: 10, minor: 15})
	if err != nil {
		t.Fatalf("symbolsNewerThan: %v", err)
	}
	if want := (macOSVersion{major: 12, minor: 0}); newer["kIOMainPortDefault"] != want {
		t.Fatalf("kIOMainPortDefault = %v, want %v", newer["kIOMainPortDefault"], want)
	}
	// Its deprecated twin has been there since 10.0, so it must NOT be flagged: the
	// whole fix for the v0.46.0 launch abort is passing 0 instead of the new one.
	if _, flagged := newer["kIOMasterPortDefault"]; flagged {
		t.Fatalf("kIOMasterPortDefault was flagged as newer than the floor")
	}
}
