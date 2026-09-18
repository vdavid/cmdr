package checks

import (
	"debug/macho"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"runtime"
	"sort"
	"strconv"
	"strings"
)

// macOSSymbolAllowlistFile names the symbols we deliberately import despite being
// newer than the floor, each with the runtime gate that makes it safe.
const macOSSymbolAllowlistFile = "macos-symbol-floor-allowlist.json"

// macOSSymbolFloorAllowlist is the committed set of justified exceptions.
type macOSSymbolFloorAllowlist struct {
	// Comment carries the file's own instructions, since whoever has to extend it
	// arrives from a failure message rather than from this source file.
	Comment string `json:"comment"`
	// Exempt maps a symbol to why importing it is safe below the floor. The only
	// honest reason is that it's weak-linked AND every call is version-gated, so
	// dyld binds it to null instead of refusing to start the process.
	Exempt map[string]string `json:"exempt"`
}

// symbolFloorViolation is one imported symbol the deployment floor can't reach.
type symbolFloorViolation struct {
	symbol string
	needs  macOSVersion
}

var (
	// sdkFunctionName matches the identifier a C function declaration names: the
	// one immediately before its parameter list.
	sdkFunctionName = regexp.MustCompile(`([A-Za-z_][A-Za-z0-9_]*)\s*\(`)
	// sdkInterfaceName matches an Objective-C class declaration's own name, and
	// deliberately NOT a category (`@interface NSString (MyAdditions)`), which
	// declares no class symbol of its own.
	sdkInterfaceName = regexp.MustCompile(`@interface\s+([A-Za-z_][A-Za-z0-9_]*)\s*(?::\s*[A-Za-z_][A-Za-z0-9_]*)?\s*(?:<[^>]*>)?\s*(?:\{|$|\n)`)
	// sdkNotALinkedSymbol marks a declaration that produces no dynamic symbol: a
	// type, a value inside one, or a preprocessor line.
	sdkNotALinkedSymbol = regexp.MustCompile(`\btypedef\b|\bNS_ENUM\b|\bNS_OPTIONS\b|\bNS_TYPED\b|\benum\b|\bstruct\b\s*\{|@protocol\b|^\s*#`)
)

// RunMacOSSymbolFloor fails when the built binary imports a C symbol or an
// Objective-C class that the macOS version the bundle promises doesn't have.
//
// The third rung, beside `desktop-macos-framework-floor` (a whole framework that
// doesn't exist that far back) and `desktop-rust-macos-availability` (a selector
// the Objective-C runtime can't find). This one is the gap between them: a symbol
// INSIDE a framework that does exist. dyld resolves it before `main`, so the app
// aborts at launch with `Symbol not found` and no runtime gate can save it, which
// is what `kIOMainPortDefault` (macOS 12) did to every macOS 10.15 and 11 user in
// v0.46.0. `nm -u` and the SDK headers together are the whole answer, and neither
// of the sibling checks looks at either.
//
// Objective-C classes ride along for free: a class reference is an
// `_OBJC_CLASS_$_Name` import, so a class newer than the floor fails here rather
// than at the first `alloc`.
//
// Jurisdiction: the frameworks the binary actually loads. A symbol from a
// framework we don't link can't be imported, and the SDK's `usr/include` is
// deliberately out of scope (libSystem's surface predates every floor we could
// set; if that ever stops being true, this is where it would go).
func RunMacOSSymbolFloor(ctx *CheckContext) (CheckResult, error) {
	floor, err := macOSDeploymentFloor(ctx.RootDir)
	if err != nil {
		return CheckResult{}, err
	}

	binary, description, _ := desktopBinaryPath(ctx.RootDir)
	if binary == "" {
		return Skipped("no built macOS binary to read; a `pnpm dev`, the Playwright lane's release build, or the release workflow produces one"), nil
	}
	if runtime.GOOS != "darwin" || !CommandExists("xcrun") {
		return Skipped("needs the macOS SDK headers, which only exist on a Mac"), nil
	}

	sdkPath, err := macOSSDKPath(ctx)
	if err != nil {
		return CheckResult{}, err
	}
	headerDirs, unresolved, err := linkedFrameworkHeaderDirs(binary, sdkPath)
	if err != nil {
		return CheckResult{}, err
	}
	if len(headerDirs) == 0 {
		return CheckResult{}, fmt.Errorf(
			"found no SDK headers for any framework %s loads, so this check would pass by knowing nothing", description)
	}

	newer, err := symbolsNewerThan(headerDirs, floor)
	if err != nil {
		return CheckResult{}, err
	}

	imported, err := importedSymbolNames(binary)
	if err != nil {
		return CheckResult{}, err
	}

	allowlist, err := readSymbolFloorAllowlist(ctx.RootDir)
	if err != nil {
		return CheckResult{}, err
	}

	if violations := violatingSymbols(imported, newer, allowlist.Exempt); len(violations) > 0 {
		var sb strings.Builder
		for _, v := range violations {
			sb.WriteString(fmt.Sprintf("  %s, which arrived in macOS %s\n", v.symbol, v.needs))
		}
		return CheckResult{}, fmt.Errorf(
			"%s imports %s newer than the macOS %s it promises in %s, so dyld aborts the app at launch with `Symbol not found` before any of your code runs:\n%s\nUse an equivalent that reaches the floor (`kIOMainPortDefault` is documented as a synonym for NULL, so the literal 0 says the same thing on every version), or weak-link it AND gate every call, then record it in `%s` with that gate as the reason",
			description, Pluralize(len(violations), "a symbol", "symbols"), floor, tauriConfRelPath,
			strings.TrimRight(sb.String(), "\n"), macOSSymbolAllowlistFile)
	}

	unresolvedNote := ""
	if unresolved > 0 {
		unresolvedNote = fmt.Sprintf(" (%d without SDK headers)", unresolved)
	}
	return Success(fmt.Sprintf("%d imported %s judged against %d %s newer than macOS %s, read from %d %s%s",
		len(imported), Pluralize(len(imported), "symbol", "symbols"),
		len(newer), Pluralize(len(newer), "symbol", "symbols"), floor,
		len(headerDirs), Pluralize(len(headerDirs), "framework", "frameworks"), unresolvedNote)), nil
}

// linkedFrameworkHeaderDirs maps the frameworks the binary loads to their SDK
// header directories, and counts the ones the SDK ships no headers for (a
// framework with a private or headerless surface, which declares nothing we could
// judge).
func linkedFrameworkHeaderDirs(binary, sdkPath string) ([]string, int, error) {
	paths, err := loadedDylibPaths(binary)
	if err != nil {
		return nil, 0, err
	}
	seen := map[string]bool{}
	var dirs []string
	unresolved := 0
	for _, path := range paths {
		name, ok := systemFrameworkName(path)
		if !ok || seen[name] {
			continue
		}
		seen[name] = true
		headers := filepath.Join(sdkPath, systemFrameworkRoot, name+".framework", "Headers")
		if info, statErr := os.Stat(headers); statErr == nil && info.IsDir() {
			dirs = append(dirs, headers)
			continue
		}
		unresolved++
	}
	sort.Strings(dirs)
	return dirs, unresolved, nil
}

// symbolsNewerThan reads every header under the given directories and answers
// which linked symbols need a macOS newer than the floor.
func symbolsNewerThan(headerDirs []string, floor macOSVersion) (map[string]macOSVersion, error) {
	oldest := map[string]macOSVersion{}
	for _, dir := range headerDirs {
		// ❗ Resolve first: in the SDK a framework's `Headers` is a SYMLINK to
		// `Versions/A/Headers`, and `filepath.Walk` doesn't follow one, so walking the
		// link itself visits a single non-directory and reads no headers at all.
		resolved, resolveErr := filepath.EvalSymlinks(dir)
		if resolveErr != nil {
			return nil, fmt.Errorf("couldn't resolve %s: %w", dir, resolveErr)
		}
		walkErr := filepath.Walk(resolved, func(path string, info os.FileInfo, err error) error {
			if err != nil {
				// A dangling symlink inside the SDK is not this check's problem.
				return nil //nolint:nilerr
			}
			if info.IsDir() || filepath.Ext(path) != ".h" {
				return nil
			}
			raw, readErr := os.ReadFile(path)
			if readErr != nil {
				return fmt.Errorf("couldn't read %s: %w", path, readErr)
			}
			for name, version := range parseHeaderSymbols(string(raw)) {
				if known, seen := oldest[name]; !seen || known.newerThan(version) {
					oldest[name] = version
				}
			}
			return nil
		})
		if walkErr != nil {
			return nil, walkErr
		}
	}

	newer := map[string]macOSVersion{}
	for name, version := range oldest {
		if version.newerThan(floor) {
			newer[name] = version
		}
	}
	return newer, nil
}

// parseHeaderSymbols reads one header's C functions, C globals, and Objective-C
// class declarations, answering the macOS version each one needs.
//
// Selectors and properties are deliberately absent: the Objective-C runtime
// resolves those, so a too-new one raises an exception rather than refusing to
// launch, which is `desktop-rust-macos-availability`'s half of the problem.
func parseHeaderSymbols(header string) map[string]macOSVersion {
	stripped := sdkLineComment.ReplaceAllString(sdkBlockComment.ReplaceAllString(header, " "), " ")
	found := map[string]macOSVersion{}
	note := func(name string, version macOSVersion) {
		if name == "" {
			return
		}
		if known, seen := found[name]; !seen || known.newerThan(version) {
			found[name] = version
		}
	}

	// Declarations end at a semicolon and often span lines, so the chunks are
	// statements rather than lines.
	for _, chunk := range strings.Split(stripped, ";") {
		if name, version, ok := interfaceSymbol(chunk); ok {
			note(name, version)
			continue
		}

		declaration := strings.Join(strings.Fields(chunk), " ")
		if declaration == "" || sdkNotALinkedSymbol.MatchString(declaration) || sdkMethodStart.MatchString(declaration) ||
			strings.Contains(declaration, "@property") || strings.Contains(declaration, "@interface") {
			continue
		}
		version, ok := availabilityIn(declaration)
		if !ok {
			continue
		}
		note(cSymbolName(declaration), version)
	}
	return found
}

// interfaceSymbol reads an Objective-C class declaration, whose exported symbol is
// `OBJC_CLASS_$_<name>`.
//
// Only the declaration's OWN line counts for availability: a class statement runs
// to the first semicolon, which is usually the end of its first method, and taking
// that method's attribute would date the class by one of its members.
func interfaceSymbol(chunk string) (string, macOSVersion, bool) {
	at := strings.Index(chunk, "@interface")
	if at < 0 {
		return "", macOSVersion{}, false
	}
	match := sdkInterfaceName.FindStringSubmatch(chunk[at:])
	if match == nil {
		return "", macOSVersion{}, false
	}
	head := chunk[:at]
	if line := chunk[at:]; true {
		if end := strings.IndexByte(line, '\n'); end >= 0 {
			head += line[:end]
		} else {
			head += line
		}
	}
	version, ok := availabilityIn(head)
	if !ok {
		return "", macOSVersion{}, false
	}
	return "OBJC_CLASS_$_" + match[1], version, true
}

// availabilityIn reads the macOS version out of a declaration's availability
// attribute. A declaration without one reads as "always there", which is what
// keeps the answer conservative.
func availabilityIn(declaration string) (macOSVersion, bool) {
	match := sdkAvailabilityPattern.FindStringSubmatch(declaration)
	if match == nil {
		return macOSVersion{}, false
	}
	major, err := strconv.Atoi(match[1])
	if err != nil {
		return macOSVersion{}, false
	}
	minor, err := strconv.Atoi(match[2])
	if err != nil {
		return macOSVersion{}, false
	}
	return macOSVersion{major: major, minor: minor}, true
}

// cSymbolName picks the identifier a C declaration names: the one before a
// parameter list for a function, the last one otherwise for a global.
func cSymbolName(declaration string) string {
	// `cutAtAttribute` cuts at `API_AVAILABLE`, and Apple writes `__API_AVAILABLE` as
	// often as the bare macro, so the cut can leave the macro's own leading
	// underscores behind as the last "identifier".
	body := strings.TrimRight(strings.TrimSpace(cutAtAttribute(declaration)), "_ \t")
	if body == "" {
		return ""
	}
	if match := sdkFunctionName.FindStringSubmatch(body); match != nil {
		return match[1]
	}
	identifiers := sdkIdentifier.FindAllString(body, -1)
	if len(identifiers) == 0 {
		return ""
	}
	return identifiers[len(identifiers)-1]
}

// importedSymbolNames reads the dynamic symbols the binary needs resolved, across
// every architecture slice, with the leading underscore of the C ABI removed so
// they match what the headers declare.
func importedSymbolNames(path string) ([]string, error) {
	names := map[string]bool{}
	collect := func(file *macho.File) error {
		imported, err := file.ImportedSymbols()
		if err != nil {
			// A slice with no dynamic symbol table imports nothing.
			return nil //nolint:nilerr
		}
		for _, symbol := range imported {
			names[strings.TrimPrefix(symbol, "_")] = true
		}
		return nil
	}

	file, err := macho.Open(path)
	if err != nil {
		fat, fatErr := macho.OpenFat(path)
		if fatErr != nil {
			return nil, fmt.Errorf("couldn't read %s as a Mach-O binary: %w", path, err)
		}
		defer fat.Close()
		for _, arch := range fat.Arches {
			if collectErr := collect(arch.File); collectErr != nil {
				return nil, collectErr
			}
		}
		return sortedKeys(names), nil
	}
	defer file.Close()
	if collectErr := collect(file); collectErr != nil {
		return nil, collectErr
	}
	return sortedKeys(names), nil
}

// violatingSymbols names the imported symbols the floor can't reach, newest need
// first so the worst offender leads the failure.
func violatingSymbols(imported []string, newer map[string]macOSVersion, exempt map[string]string) []symbolFloorViolation {
	var violations []symbolFloorViolation
	for _, symbol := range imported {
		name := strings.TrimPrefix(symbol, "_")
		needs, tooNew := newer[name]
		if !tooNew {
			continue
		}
		if _, excused := exempt[name]; excused {
			continue
		}
		violations = append(violations, symbolFloorViolation{symbol: name, needs: needs})
	}
	sort.Slice(violations, func(i, j int) bool {
		if violations[i].needs.newerThan(violations[j].needs) {
			return true
		}
		if violations[j].needs.newerThan(violations[i].needs) {
			return false
		}
		return violations[i].symbol < violations[j].symbol
	})
	return violations
}

// readSymbolFloorAllowlist reads the committed exceptions. A missing file is an
// empty allowlist: nothing is excused, which is the safe direction.
func readSymbolFloorAllowlist(rootDir string) (macOSSymbolFloorAllowlist, error) {
	path := filepath.Join(rootDir, filepath.Join(runnerChecksDirParts...), macOSSymbolAllowlistFile)
	raw, err := os.ReadFile(path)
	if os.IsNotExist(err) {
		return macOSSymbolFloorAllowlist{}, nil
	}
	if err != nil {
		return macOSSymbolFloorAllowlist{}, fmt.Errorf("couldn't read %s: %w", macOSSymbolAllowlistFile, err)
	}
	var allowlist macOSSymbolFloorAllowlist
	if err := json.Unmarshal(raw, &allowlist); err != nil {
		return macOSSymbolFloorAllowlist{}, fmt.Errorf("couldn't parse %s: %w", macOSSymbolAllowlistFile, err)
	}
	return allowlist, nil
}
