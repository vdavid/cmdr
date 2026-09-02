package checks

import (
	"bufio"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"runtime"
	"sort"
	"strconv"
	"strings"
)

// macOSAvailabilityCheckID is the registry ID, needed here because the scanner
// resolves its own jurisdiction.
const macOSAvailabilityCheckID = "desktop-rust-macos-availability"

// tauriConfRelPath holds the deployment floor this check enforces:
// `bundle.macOS.minimumSystemVersion` is the version the bundle claims to run on,
// and every Objective-C selector we call has to exist that far back.
const tauriConfRelPath = "apps/desktop/src-tauri/tauri.conf.json"

// macOSAvailabilitySelectorsFile caches what the SDK headers say, so the check
// still runs where the SDK doesn't exist. CI is Linux end to end, and a guard that
// only fires on the author's Mac isn't the gate this one has to be.
const macOSAvailabilitySelectorsFile = "macos-availability-selectors.json"

// AllowNewerSelectorComment opts a single call out of this check, for a selector
// that IS newer than the floor but only ever runs on an OS that has it. Put it on
// the line above the call (or trailing on the same line) with the gate as the
// reason, so a reader can find the guard without leaving the file:
//
//	if !macos_at_least(12, 0) { return fallback(); }
//	// allowed-newer-selector: guarded by the `macos_at_least(12, 0)` early return above
//	let apps = workspace.URLsForApplicationsToOpenURL(&url);
//
// A runtime gate is the ONLY thing this excuses. The check can't see the gate (it
// reads lines, not control flow), so the comment is the claim and the reason is
// where you make it checkable by a human. An opt-out that excuses no call is
// reported as an orphan, which is what keeps a stale one from rotting in place.
const AllowNewerSelectorComment = "// allowed-newer-selector:"

// nonFrameworkObjc2Crates are the `objc2-*` crates that bind no framework, so
// there are no headers to resolve them against. Everything else must map to a
// framework in the SDK or the check fails: an unmapped crate is a blind spot, and
// a blind spot that passes silently is how this check stops being one.
var nonFrameworkObjc2Crates = map[string]bool{
	"objc2":                  true,
	"objc2-encode":           true,
	"objc2-exception-helper": true,
}

// macOSVersion is a major.minor pair, the granularity Apple's availability macros
// and our `minimumSystemVersion` both write.
type macOSVersion struct {
	major int
	minor int
}

func (v macOSVersion) String() string { return fmt.Sprintf("%d.%d", v.major, v.minor) }

func (v macOSVersion) newerThan(other macOSVersion) bool {
	if v.major != other.major {
		return v.major > other.major
	}
	return v.minor > other.minor
}

// macOSSelectorIndex is the committed answer to "which selectors are newer than
// the floor", refreshed from the SDK on every macOS run and read as-is elsewhere.
// The floor rides along because the list only means anything against the one it
// was built for.
type macOSSelectorIndex struct {
	Floor string `json:"floor"`
	// SDK is the SDK the list was read from, so a reader can tell how current
	// the answer is. Nothing keys off it.
	SDK string `json:"sdk"`
	// Selectors maps a selector to the macOS version that introduced it,
	// `objc2`-spelled (keywords joined by underscores).
	Selectors map[string]string `json:"selectors"`
}

// availabilitySite is one call to a selector the deployment floor can't reach.
type availabilitySite struct {
	relPath  string
	line     int
	selector string
	needs    macOSVersion
	text     string
}

var (
	// sdkAvailabilityPattern reads the macOS version out of an availability
	// attribute. Apple writes the platform both ways (`macos(14.0)` in newer
	// headers, `macosx(14.0)` in Foundation's older ones).
	sdkAvailabilityPattern = regexp.MustCompile(`API_AVAILABLE\s*\(\s*macosx?\((\d+)\.(\d+)`)
	// sdkAttributeCut is where a declaration stops being the declaration: every
	// availability, Swift-naming, and audit macro Apple appends after the name.
	sdkAttributeCut = regexp.MustCompile(`API_AVAILABLE|API_DEPRECATED|API_UNAVAILABLE|NS_[A-Z]`)
	// sdkMethodStart marks an Objective-C method declaration (`- (void)foo`).
	sdkMethodStart = regexp.MustCompile(`^[-+]\s*\(`)
	// sdkSelectorPart matches each keyword of a multi-part selector, the
	// identifier immediately before a colon.
	sdkSelectorPart = regexp.MustCompile(`([A-Za-z_][A-Za-z0-9_]*)\s*:`)
	// sdkIdentifier matches any C identifier, used to take the last one in a
	// property declaration (which is the property's name).
	sdkIdentifier = regexp.MustCompile(`[A-Za-z_][A-Za-z0-9_]*`)
	// sdkLineComment and sdkBlockComment strip the prose out of a header before
	// it's parsed; a sentence with a colon in it would otherwise read as a
	// selector keyword.
	sdkLineComment  = regexp.MustCompile(`(?m)//.*$`)
	sdkBlockComment = regexp.MustCompile(`(?s)/\*.*?\*/`)
	// rustMethodCall matches a Rust method or associated-function call, which is
	// how `objc2`'s generated bindings spell a selector
	// (`locale.regionCode()`, `NSLocale::autoupdatingCurrentLocale()`).
	rustMethodCall = regexp.MustCompile(`(?:\.|::)([A-Za-z_][A-Za-z0-9_]*)\s*\(`)
	// rustIdentifier picks the selector out of a hand-written `msg_send!`, where
	// it isn't a method call at all.
	rustIdentifier = regexp.MustCompile(`[A-Za-z_][A-Za-z0-9_]*`)
	// objc2CrateName finds the `objc2-*` dependencies in a member's manifest,
	// which is what tells this check which frameworks we bind.
	objc2CrateName = regexp.MustCompile(`\bobjc2(?:-[a-z0-9]+)*\b`)
)

// RunMacOSAvailability fails when we call an Objective-C selector newer than the
// macOS version the bundle claims to run on.
//
// `objc2` carries no availability information: every binding compiles on every
// deployment target, and a selector the running OS doesn't have raises
// `NSInvalidArgumentException` instead. Raised on the main thread inside the Tauri
// setup hook (or any other `extern "C"` callback the OS calls us through), that
// exception unwinds out of a `nounwind` frame and aborts the process before a
// window opens, which is what shipping `NSLocale.regionCode` (macOS 14+) did to
// every macOS 12 and 13 user in v0.39.0 and v0.40.0 (GitHub issue #54). Nothing
// else catches it: it compiles clean, the tests pass on a current Mac, and
// GitHub's oldest hosted macOS runner is newer than the floor.
//
// The SDK headers are the source of truth. They only exist on macOS, so a macOS
// run refreshes `macos-availability-selectors.json` from them and every run scans
// against that.
func RunMacOSAvailability(ctx *CheckContext) (CheckResult, error) {
	floor, err := macOSDeploymentFloor(ctx.RootDir)
	if err != nil {
		return CheckResult{}, err
	}

	newer, refreshed, err := selectorsNewerThanFloor(ctx, floor)
	if err != nil {
		return CheckResult{}, err
	}

	roots, err := ScannerRoots(ctx.RootDir, macOSAvailabilityCheckID)
	if err != nil {
		return CheckResult{}, err
	}

	var violations []availabilitySite
	var orphans []orphanDirective
	scanned := 0
	for _, root := range roots {
		rootViolations, rootOrphans, rootScanned, scanErr := scanForNewSelectors(ctx.RootDir, root, newer)
		if scanErr != nil {
			return CheckResult{}, fmt.Errorf("failed to scan Rust files: %w", scanErr)
		}
		violations = append(violations, rootViolations...)
		orphans = append(orphans, rootOrphans...)
		scanned += rootScanned
	}

	if len(violations) > 0 {
		sort.Slice(violations, func(i, j int) bool {
			if violations[i].relPath == violations[j].relPath {
				return violations[i].line < violations[j].line
			}
			return violations[i].relPath < violations[j].relPath
		})
		var sb strings.Builder
		for _, v := range violations {
			sb.WriteString(fmt.Sprintf("  %s:%d: `%s` needs macOS %s\n    %s\n", v.relPath, v.line, v.selector, v.needs, v.text))
		}
		return CheckResult{}, fmt.Errorf(
			"found %d Objective-C %s newer than the macOS %s this bundle claims to run on (%s). On an older Mac the selector doesn't exist, and the unrecognized-selector exception aborts the app rather than returning an error. Use an older API that answers the same question, or gate the call on the running version (`crate::platform::macos_at_least`) and mark it with `%s <reason>`:\n%s",
			len(violations), Pluralize(len(violations), "call", "calls"), floor, tauriConfRelPath,
			AllowNewerSelectorComment,
			strings.TrimRight(sb.String(), "\n"),
		)
	}

	if len(orphans) > 0 {
		return CheckResult{}, fmt.Errorf("%s", formatOrphanDirectives(AllowNewerSelectorComment, orphans))
	}

	message := fmt.Sprintf(
		"%d Rust %s scanned against %d %s newer than macOS %s, every call reaches the floor",
		scanned, Pluralize(scanned, "file", "files"),
		len(newer), Pluralize(len(newer), "selector", "selectors"), floor,
	)
	if refreshed {
		return SuccessWithChanges(message + fmt.Sprintf(", and %s now matches the installed SDK", macOSAvailabilitySelectorsFile)), nil
	}
	return Success(message), nil
}

// selectorsNewerThanFloor answers which selectors the floor can't reach, and
// whether the committed list had to be rewritten to say so.
//
// On macOS the SDK decides and the file follows. Everywhere else the file IS the
// answer: it's committed precisely so the Linux CI lanes enforce this too. A file
// built against a different floor is an error rather than a silent wrong answer,
// since it lists only what was above the floor at the time.
func selectorsNewerThanFloor(ctx *CheckContext, floor macOSVersion) (map[string]macOSVersion, bool, error) {
	path := filepath.Join(ctx.RootDir, filepath.Join(runnerChecksDirParts...), macOSAvailabilitySelectorsFile)

	if runtime.GOOS != "darwin" || !CommandExists("xcrun") {
		stored, err := readSelectorIndex(path)
		if err != nil {
			return nil, false, err
		}
		if stored.Floor != floor.String() {
			return nil, false, fmt.Errorf(
				"%s was built for macOS %s but %s now says %s; re-run this check on a Mac to rebuild it from the SDK",
				macOSAvailabilitySelectorsFile, stored.Floor, tauriConfRelPath, floor)
		}
		versions, err := parseSelectorVersions(stored)
		return versions, false, err
	}

	sdkPath, err := macOSSDKPath(ctx)
	if err != nil {
		return nil, false, err
	}
	frameworks, err := boundFrameworkHeaderDirs(ctx.RootDir, sdkPath)
	if err != nil {
		return nil, false, err
	}
	newer, err := selectorsNewerThan(frameworks, floor)
	if err != nil {
		return nil, false, err
	}

	fresh := macOSSelectorIndex{
		Floor:     floor.String(),
		SDK:       filepath.Base(sdkPath),
		Selectors: make(map[string]string, len(newer)),
	}
	for name, version := range newer {
		fresh.Selectors[name] = version.String()
	}

	stored, readErr := readSelectorIndex(path)
	if readErr == nil && sameSelectorIndex(stored, fresh) {
		return newer, false, nil
	}
	if ctx.CI {
		return nil, false, fmt.Errorf(
			"%s doesn't match the installed SDK (%s). Run `pnpm check macos-availability` locally and commit the rewrite",
			macOSAvailabilitySelectorsFile, fresh.SDK)
	}
	if err := writeSelectorIndex(path, fresh); err != nil {
		return nil, false, err
	}
	return newer, true, nil
}

// readSelectorIndex reads the committed list. Its absence is an error: an empty
// list would scan every source and find nothing, which reads exactly like a pass.
func readSelectorIndex(path string) (macOSSelectorIndex, error) {
	raw, err := os.ReadFile(path)
	if err != nil {
		return macOSSelectorIndex{}, fmt.Errorf(
			"couldn't read %s (%w); without it this check would pass by knowing nothing", macOSAvailabilitySelectorsFile, err)
	}
	var index macOSSelectorIndex
	if err := json.Unmarshal(raw, &index); err != nil {
		return macOSSelectorIndex{}, fmt.Errorf("couldn't parse %s: %w", macOSAvailabilitySelectorsFile, err)
	}
	if len(index.Selectors) == 0 {
		return macOSSelectorIndex{}, fmt.Errorf(
			"%s lists no selectors; that's a check that can't fail, so it's treated as a broken file", macOSAvailabilitySelectorsFile)
	}
	return index, nil
}

// parseSelectorVersions turns the stored strings back into comparable versions.
func parseSelectorVersions(index macOSSelectorIndex) (map[string]macOSVersion, error) {
	out := make(map[string]macOSVersion, len(index.Selectors))
	for name, raw := range index.Selectors {
		version, ok := parseMacOSVersion(raw)
		if !ok {
			return nil, fmt.Errorf("%s gives `%s` the unreadable version %q", macOSAvailabilitySelectorsFile, name, raw)
		}
		out[name] = version
	}
	return out, nil
}

// sameSelectorIndex compares what the SDK says against what's committed. The SDK
// NAME is deliberately not part of it: an Xcode update that adds no API to the
// frameworks we bind shouldn't rewrite the file.
func sameSelectorIndex(stored, fresh macOSSelectorIndex) bool {
	if stored.Floor != fresh.Floor || len(stored.Selectors) != len(fresh.Selectors) {
		return false
	}
	for name, version := range fresh.Selectors {
		if stored.Selectors[name] != version {
			return false
		}
	}
	return true
}

// writeSelectorIndex rewrites the committed list, sorted by `json.Marshal`'s map
// key order so the diff is readable.
func writeSelectorIndex(path string, index macOSSelectorIndex) error {
	raw, err := json.MarshalIndent(index, "", "  ")
	if err != nil {
		return fmt.Errorf("couldn't encode %s: %w", macOSAvailabilitySelectorsFile, err)
	}
	if err := os.WriteFile(path, append(raw, '\n'), 0o644); err != nil {
		return fmt.Errorf("couldn't write %s: %w", macOSAvailabilitySelectorsFile, err)
	}
	return nil
}

// macOSDeploymentFloor reads `bundle.macOS.minimumSystemVersion`, the version the
// shipped bundle promises to run on.
func macOSDeploymentFloor(rootDir string) (macOSVersion, error) {
	path := filepath.Join(rootDir, tauriConfRelPath)
	raw, err := os.ReadFile(path)
	if err != nil {
		return macOSVersion{}, fmt.Errorf("couldn't read %s: %w", tauriConfRelPath, err)
	}
	var conf struct {
		Bundle struct {
			MacOS struct {
				MinimumSystemVersion string `json:"minimumSystemVersion"`
			} `json:"macOS"`
		} `json:"bundle"`
	}
	if err := json.Unmarshal(raw, &conf); err != nil {
		return macOSVersion{}, fmt.Errorf("couldn't parse %s: %w", tauriConfRelPath, err)
	}
	version, ok := parseMacOSVersion(conf.Bundle.MacOS.MinimumSystemVersion)
	if !ok {
		return macOSVersion{}, fmt.Errorf(
			"%s has no usable `bundle.macOS.minimumSystemVersion` (got %q); this check has no floor to enforce without it",
			tauriConfRelPath, conf.Bundle.MacOS.MinimumSystemVersion)
	}
	return version, nil
}

// parseMacOSVersion reads `12` or `12.0` or `12.0.1`; the patch level is dropped,
// since availability macros don't carry one.
func parseMacOSVersion(s string) (macOSVersion, bool) {
	parts := strings.Split(strings.TrimSpace(s), ".")
	if parts[0] == "" {
		return macOSVersion{}, false
	}
	major, err := strconv.Atoi(parts[0])
	if err != nil {
		return macOSVersion{}, false
	}
	minor := 0
	if len(parts) > 1 {
		if minor, err = strconv.Atoi(parts[1]); err != nil {
			return macOSVersion{}, false
		}
	}
	return macOSVersion{major: major, minor: minor}, true
}

// macOSSDKPath asks xcrun where the current macOS SDK is, rather than guessing a
// path that moves with every Xcode.
func macOSSDKPath(ctx *CheckContext) (string, error) {
	cmd := exec.Command("xcrun", "--sdk", "macosx", "--show-sdk-path")
	cmd.Dir = ctx.RootDir
	out, err := RunCommand(cmd, true)
	if err != nil {
		return "", fmt.Errorf("couldn't locate the macOS SDK:\n%s", indentOutput(out))
	}
	path := strings.TrimSpace(out)
	if path == "" {
		return "", fmt.Errorf("xcrun named no macOS SDK path")
	}
	return path, nil
}

// boundFrameworkHeaderDirs maps every `objc2-*` dependency in the workspace to the
// SDK framework whose headers describe it, so adding a binding crate extends this
// check's reach without anyone remembering to widen a list.
func boundFrameworkHeaderDirs(rootDir, sdkPath string) ([]string, error) {
	crates, err := objc2Crates(rootDir)
	if err != nil {
		return nil, err
	}
	frameworks, err := sdkFrameworkHeaderDirs(sdkPath)
	if err != nil {
		return nil, err
	}

	var dirs []string
	var unmapped []string
	for _, crate := range crates {
		if nonFrameworkObjc2Crates[crate] {
			continue
		}
		// `objc2-app-kit` binds `AppKit.framework`: same name, minus the crate
		// prefix and the kebab-case separators the crate name is spelled in.
		key := strings.ReplaceAll(strings.TrimPrefix(crate, "objc2-"), "-", "")
		dir, ok := frameworks[key]
		if !ok {
			unmapped = append(unmapped, crate)
			continue
		}
		dirs = append(dirs, dir)
	}
	if len(unmapped) > 0 {
		sort.Strings(unmapped)
		return nil, fmt.Errorf(
			"no SDK framework matches %s (%s); map it in `nonFrameworkObjc2Crates` if it binds no framework, so this check keeps covering every binding we depend on",
			Pluralize(len(unmapped), "crate", "crates"), strings.Join(unmapped, ", "))
	}
	sort.Strings(dirs)
	return dirs, nil
}

// objc2Crates lists the `objc2-*` dependencies named across the workspace's
// manifests, deduplicated.
func objc2Crates(rootDir string) ([]string, error) {
	members, err := WorkspaceMembers(rootDir)
	if err != nil {
		return nil, err
	}
	manifests := []string{filepath.Join(rootDir, "Cargo.toml")}
	for _, m := range members {
		manifests = append(manifests, m.ManifestPath)
	}

	seen := map[string]bool{}
	for _, manifest := range manifests {
		raw, readErr := os.ReadFile(manifest)
		if readErr != nil {
			if os.IsNotExist(readErr) {
				continue
			}
			return nil, fmt.Errorf("couldn't read %s: %w", manifest, readErr)
		}
		for _, name := range objc2CrateName.FindAllString(string(raw), -1) {
			seen[name] = true
		}
	}

	crates := make([]string, 0, len(seen))
	for name := range seen {
		crates = append(crates, name)
	}
	sort.Strings(crates)
	return crates, nil
}

// sdkFrameworkHeaderDirs indexes every framework in the SDK by its name, lowercased
// and separator-free, so a crate name can find it. Walks rather than reads one
// level: `QuickLookUI` lives inside `Quartz.framework`.
func sdkFrameworkHeaderDirs(sdkPath string) (map[string]string, error) {
	frameworks := map[string]string{}
	root := filepath.Join(sdkPath, "System", "Library", "Frameworks")
	err := filepath.WalkDir(root, func(path string, d os.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if !d.IsDir() || !strings.HasSuffix(d.Name(), ".framework") {
			return nil
		}
		// `Headers` is a symlink into `Versions/`, and a walk doesn't follow one,
		// so the link is resolved here rather than silently walking nothing.
		headers, resolveErr := filepath.EvalSymlinks(filepath.Join(path, "Headers"))
		if resolveErr != nil {
			return nil
		}
		if info, statErr := os.Stat(headers); statErr != nil || !info.IsDir() {
			return nil
		}
		key := strings.ToLower(strings.TrimSuffix(d.Name(), ".framework"))
		// A nested framework shadowing a top-level one would be the wrong
		// headers; first one found (the shallower) wins.
		if _, taken := frameworks[key]; !taken {
			frameworks[key] = headers
		}
		return nil
	})
	if err != nil {
		return nil, fmt.Errorf("couldn't walk the SDK frameworks at %s: %w", root, err)
	}
	return frameworks, nil
}

// selectorsNewerThan returns every selector whose OLDEST declaration in these
// frameworks is newer than the floor, keyed the way `objc2` spells it (selector
// keywords joined by underscores).
//
// Oldest declaration, because the scan below can't know which class a Rust
// receiver is: a name that some class has carried since 10.x is a name we can't
// call a violation. Same reason a selector has to carry an uppercase letter to
// count: `bytes`, `close`, and `title` are Objective-C selectors AND ordinary Rust
// method names, and flagging those would drown the real finding. Both are
// deliberate blind spots; `DETAILS.md` § "macOS availability" has the reasoning.
func selectorsNewerThan(headerDirs []string, floor macOSVersion) (map[string]macOSVersion, error) {
	oldest := map[string]macOSVersion{}
	note := func(name string, version macOSVersion) {
		if name == "" {
			return
		}
		if known, seen := oldest[name]; !seen || known.newerThan(version) {
			oldest[name] = version
		}
	}

	for _, dir := range headerDirs {
		err := filepath.WalkDir(dir, func(path string, d os.DirEntry, err error) error {
			if err != nil {
				return err
			}
			if d.IsDir() || !strings.HasSuffix(d.Name(), ".h") {
				return nil
			}
			raw, readErr := os.ReadFile(path)
			if readErr != nil {
				return readErr
			}
			for name, version := range parseHeaderDeclarations(string(raw)) {
				note(name, version)
			}
			return nil
		})
		if err != nil {
			return nil, fmt.Errorf("couldn't read the SDK headers in %s: %w", dir, err)
		}
	}

	if len(oldest) == 0 {
		return nil, fmt.Errorf(
			"read no declarations out of %d SDK %s; the header parse is broken, and an empty answer would pass by knowing nothing",
			len(headerDirs), Pluralize(len(headerDirs), "framework", "frameworks"))
	}

	newer := map[string]macOSVersion{}
	for name, version := range oldest {
		if version.newerThan(floor) && strings.ToLower(name) != name {
			newer[name] = version
		}
	}
	return newer, nil
}

// parseHeaderDeclarations reads one header's property and method declarations,
// answering the macOS version each one needs. A declaration with no availability
// attribute reads as "always there", which is what keeps the result conservative.
func parseHeaderDeclarations(header string) map[string]macOSVersion {
	stripped := sdkLineComment.ReplaceAllString(sdkBlockComment.ReplaceAllString(header, " "), " ")
	found := map[string]macOSVersion{}
	// Declarations end at a semicolon and often span lines, so the chunks are
	// statements rather than lines.
	for _, chunk := range strings.Split(stripped, ";") {
		declaration := strings.Join(strings.Fields(chunk), " ")
		if declaration == "" {
			continue
		}
		version := macOSVersion{}
		if match := sdkAvailabilityPattern.FindStringSubmatch(declaration); match != nil {
			major, _ := strconv.Atoi(match[1])
			minor, _ := strconv.Atoi(match[2])
			version = macOSVersion{major: major, minor: minor}
		}

		var name string
		switch {
		case strings.Contains(declaration, "@property"):
			body := cutAtAttribute(declaration[strings.Index(declaration, "@property")+len("@property"):])
			identifiers := sdkIdentifier.FindAllString(body, -1)
			if len(identifiers) > 0 {
				name = identifiers[len(identifiers)-1]
			}
		case sdkMethodStart.MatchString(declaration):
			body := cutAtAttribute(declaration)
			if parts := sdkSelectorPart.FindAllStringSubmatch(body, -1); len(parts) > 0 {
				keywords := make([]string, 0, len(parts))
				for _, part := range parts {
					keywords = append(keywords, part[1])
				}
				name = strings.Join(keywords, "_")
			} else if identifiers := sdkIdentifier.FindAllString(body, -1); len(identifiers) > 0 {
				name = identifiers[len(identifiers)-1]
			}
		}
		if name == "" {
			continue
		}
		if known, seen := found[name]; !seen || known.newerThan(version) {
			found[name] = version
		}
	}
	return found
}

// cutAtAttribute drops everything from the first trailing macro onward, so the
// declaration's own name is the last identifier left.
func cutAtAttribute(declaration string) string {
	if at := sdkAttributeCut.FindStringIndex(declaration); at != nil {
		return declaration[:at[0]]
	}
	return declaration
}

// scanForNewSelectors walks one source root and returns every call to a selector
// in `newer`, the opt-out directives that excused nothing, and the count of files
// scanned.
//
// Only files that mention `objc2` are read for calls: elsewhere a matching name is
// a Rust method that happens to share it, and the receiver can't be an
// Objective-C object.
func scanForNewSelectors(
	rootDir, srcDir string, newer map[string]macOSVersion,
) ([]availabilitySite, []orphanDirective, int, error) {
	var violations []availabilitySite
	var orphans []orphanDirective
	scanned := 0

	err := filepath.WalkDir(srcDir, func(path string, d os.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if d.IsDir() || !strings.HasSuffix(d.Name(), ".rs") {
			return nil
		}

		raw, readErr := os.ReadFile(path)
		if readErr != nil {
			return readErr
		}
		scanned++
		if !strings.Contains(string(raw), "objc2") {
			return nil
		}

		relPath, relErr := filepath.Rel(rootDir, path)
		if relErr != nil {
			relPath = path
		}

		tracker := newDirectiveTracker(AllowNewerSelectorComment, "//")
		scanner := bufio.NewScanner(strings.NewReader(string(raw)))
		scanner.Buffer(make([]byte, 64*1024), 1024*1024)
		lineNum := 0
		prev := ""
		for scanner.Scan() {
			lineNum++
			line := scanner.Text()
			tracker.observe(lineNum, line)
			if strings.HasPrefix(strings.TrimLeft(line, " \t"), "//") {
				prev = line
				continue
			}
			for _, name := range calledSelectors(line) {
				version, isNew := newer[name]
				if !isNew {
					continue
				}
				// Opt-out: the directive on the line above, or trailing on this one.
				if strings.Contains(prev, AllowNewerSelectorComment) || strings.Contains(line, AllowNewerSelectorComment) {
					tracker.markUsed(lineNum, line, prev)
					continue
				}
				violations = append(violations, availabilitySite{
					relPath:  filepath.ToSlash(relPath),
					line:     lineNum,
					selector: name,
					needs:    version,
					text:     strings.TrimSpace(line),
				})
			}
			prev = line
		}
		orphans = append(orphans, tracker.orphans(filepath.ToSlash(relPath))...)
		return scanner.Err()
	})
	if err != nil {
		return nil, nil, 0, err
	}

	return violations, orphans, scanned, nil
}

// calledSelectors returns the names one line of Rust could be sending as a
// selector: every method and associated-function call, plus every bare identifier
// when the line hand-writes a `msg_send!`.
func calledSelectors(line string) []string {
	var names []string
	for _, match := range rustMethodCall.FindAllStringSubmatch(line, -1) {
		names = append(names, match[1])
	}
	if strings.Contains(line, "msg_send!") {
		names = append(names, rustIdentifier.FindAllString(line, -1)...)
	}
	return names
}
