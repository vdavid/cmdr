package checks

import (
	"debug/macho"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
)

// macOSFrameworkVersionsFile records when each macOS framework shipped.
//
// Hand-written, because Apple publishes that in prose and nothing in the SDK
// carries it: a framework's headers annotate only what is NEWER than the framework
// itself, so `UniformTypeIdentifiers` reads as 12.0 from its own headers (its most
// recent addition) and `PDFKit` as 13.0, when they arrived in 11.0 and 10.4.
// Deriving the number is therefore not on the table. What IS enforced is that
// nothing goes unrecorded: a framework the binary loads and this file doesn't name
// fails the check.
const macOSFrameworkVersionsFile = "macos-framework-versions.json"

// macOSBinaryEnvVar points the check at a specific Mach-O, for the release
// workflow, which has the real signed binary and is the one run that must never be
// skipped. Unset everywhere else, where the check finds a local build itself.
const macOSBinaryEnvVar = "CMDR_MACOS_BINARY"

// systemFrameworkRoot is the only prefix this check judges. Everything else a Mach-O
// loads is either `/usr/lib` (the dyld shared cache basics, all of them older than
// any floor we could set) or `@rpath` / `@executable_path` (something shipping
// inside the bundle, which is present by construction).
const systemFrameworkRoot = "/System/Library/Frameworks/"

// macOSFrameworkIndex is the committed answer to "when did each framework ship".
type macOSFrameworkIndex struct {
	// Comment carries the file's own instructions, since whoever has to extend it
	// arrives from a failure message rather than from this source file.
	Comment string `json:"comment"`
	// Floor is the `minimumSystemVersion` the list was last read against, so a floor
	// that moves forces a re-read instead of a silently stale verdict.
	Floor string `json:"floor"`
	// Frameworks maps a framework's name to the macOS version that introduced it.
	Frameworks map[string]string `json:"frameworks"`
}

// loadedFramework is one framework the binary carries a hard load command for.
type loadedFramework struct {
	name  string
	since macOSVersion
}

// RunMacOSFrameworkFloor fails when the desktop binary loads a framework that
// doesn't exist on the oldest macOS the bundle promises.
//
// dyld resolves every `LC_LOAD_DYLIB` before `main` runs and refuses to start the
// process if one file is missing, so this is not a call that might not happen and
// no runtime version gate can save it. v0.42.0 shipped that way and could not open
// at all on Catalina: `objc2-quick-look-ui` was declared without
// `default-features = false`, its default set turns on
// `objc2-uniform-type-identifiers`, and that crate's `#[link]` is unconditional.
// Nothing in Cmdr called `UTType`; the reference alone was enough.
//
// Decision/Why: it reads the built Mach-O rather than the dependency graph, because
// the graph can't answer this. A `#[link]` name is not the load command it produces
// (`objc2-quick-look-ui` asks for `QuickLookUI` and the linker resolves it through
// the `Quartz` umbrella), a `build.rs` can emit a link flag no manifest mentions,
// and the crate that broke Catalina appears in no manifest in this repo at all.
// The load commands are the only place the truth is complete.
//
// `desktop-rust-macos-availability` is the sibling rung, for the other failure
// mode: a selector newer than the floor inside a framework that does exist.
func RunMacOSFrameworkFloor(ctx *CheckContext) (CheckResult, error) {
	floor, err := macOSDeploymentFloor(ctx.RootDir)
	if err != nil {
		return CheckResult{}, err
	}

	index, err := readFrameworkIndex(ctx.RootDir)
	if err != nil {
		return CheckResult{}, err
	}
	if index.Floor != floor.String() {
		return CheckResult{}, fmt.Errorf(
			"%s records the floor as macOS %s but %s now says %s; re-check every entry against the new floor, then update the file's `floor`",
			macOSFrameworkVersionsFile, index.Floor, tauriConfRelPath, floor)
	}

	binary, description := desktopBinaryPath(ctx.RootDir)
	if binary == "" {
		return Skipped("no built macOS binary to read; a `pnpm dev`, the Playwright lane's release build, or the release workflow produces one"), nil
	}

	paths, err := loadedDylibPaths(binary)
	if err != nil {
		return CheckResult{}, err
	}

	loaded, unknown, err := frameworksFrom(paths, index)
	if err != nil {
		return CheckResult{}, err
	}
	if len(unknown) > 0 {
		sort.Strings(unknown)
		return CheckResult{}, fmt.Errorf(
			"%s loads %s this check knows nothing about (%s). An unrecorded framework is a blind spot, and a blind spot that passes silently is how this check stops being one. Look up when Apple shipped each one and add it to `%s`; if one turns out to be newer than macOS %s, the dependency that pulls it in has to go instead",
			description, Pluralize(len(unknown), "a framework", "frameworks"), strings.Join(unknown, ", "),
			filepath.Join(filepath.Join(runnerChecksDirParts...), macOSFrameworkVersionsFile), floor)
	}

	var tooNew []loadedFramework
	for _, f := range loaded {
		if f.since.newerThan(floor) {
			tooNew = append(tooNew, f)
		}
	}
	if len(tooNew) > 0 {
		var sb strings.Builder
		for _, f := range tooNew {
			sb.WriteString(fmt.Sprintf("  %s.framework, which arrived in macOS %s\n", f.name, f.since))
		}
		return CheckResult{}, fmt.Errorf(
			"%s loads %s newer than the macOS %s it promises in %s, so dyld turns the app away on that OS before any of your code runs:\n%s\nRun `cargo tree -e normal --target aarch64-apple-darwin -i <the objc2 crate that binds it>` to find who pulls it in. A feature default reaching a framework nothing calls is how this last happened, and `default-features = false` was the whole fix. Failing that, raise `minimumSystemVersion`",
			description, Pluralize(len(tooNew), "a framework", "frameworks"), floor, tauriConfRelPath,
			strings.TrimRight(sb.String(), "\n"))
	}

	return Success(fmt.Sprintf("%d %s loaded by %s, every one of them present on macOS %s",
		len(loaded), Pluralize(len(loaded), "framework", "frameworks"), description, floor)), nil
}

// readFrameworkIndex loads the committed framework-to-version list.
func readFrameworkIndex(rootDir string) (macOSFrameworkIndex, error) {
	var index macOSFrameworkIndex
	path := filepath.Join(rootDir, filepath.Join(runnerChecksDirParts...), macOSFrameworkVersionsFile)
	raw, err := os.ReadFile(path)
	if err != nil {
		return index, fmt.Errorf("couldn't read %s: %w", macOSFrameworkVersionsFile, err)
	}
	if err := json.Unmarshal(raw, &index); err != nil {
		return index, fmt.Errorf("couldn't parse %s: %w", macOSFrameworkVersionsFile, err)
	}
	if len(index.Frameworks) == 0 {
		return index, fmt.Errorf("%s lists no frameworks at all, so it would wave anything through", macOSFrameworkVersionsFile)
	}
	return index, nil
}

// desktopBinaryPath finds a Mach-O to read, plus a phrase naming it for the
// messages. The release workflow points at its own via the env var; locally the
// release build wins over the debug one, since that's the shape that ships.
func desktopBinaryPath(rootDir string) (string, string) {
	if named := strings.TrimSpace(os.Getenv(macOSBinaryEnvVar)); named != "" {
		return named, fmt.Sprintf("the binary %s names", macOSBinaryEnvVar)
	}
	for _, profile := range []string{"release", "debug"} {
		candidate := filepath.Join(rootDir, "target", profile, "Cmdr")
		if info, err := os.Stat(candidate); err == nil && !info.IsDir() {
			return candidate, fmt.Sprintf("the %s build", profile)
		}
	}
	return "", ""
}

// loadedDylibPaths reads the install names the binary hard-loads.
//
// Only `LC_LOAD_DYLIB` comes back: Go's parser turns exactly that command into a
// `*macho.Dylib`, and leaves `LC_LOAD_WEAK_DYLIB` as raw bytes. That's the
// distinction this check wants anyway, since dyld tolerates a missing weak one and
// binds its symbols to null.
func loadedDylibPaths(path string) ([]string, error) {
	file, err := macho.Open(path)
	if err != nil {
		fat, fatErr := macho.OpenFat(path)
		if fatErr != nil {
			return nil, fmt.Errorf("couldn't read %s as a Mach-O binary: %w", path, err)
		}
		defer fat.Close()
		var all []string
		for _, arch := range fat.Arches {
			all = append(all, dylibNames(arch.File)...)
		}
		return all, nil
	}
	defer file.Close()
	return dylibNames(file), nil
}

func dylibNames(file *macho.File) []string {
	var names []string
	for _, load := range file.Loads {
		if dylib, ok := load.(*macho.Dylib); ok {
			names = append(names, dylib.Name)
		}
	}
	return names
}

// frameworksFrom turns install-name paths into the system frameworks among them,
// each paired with the version that introduced it. The second return names the ones
// the index doesn't cover.
func frameworksFrom(paths []string, index macOSFrameworkIndex) ([]loadedFramework, []string, error) {
	var loaded []loadedFramework
	var unknown []string
	seen := map[string]bool{}

	for _, path := range paths {
		name, ok := systemFrameworkName(path)
		if !ok || seen[name] {
			continue
		}
		seen[name] = true

		raw, listed := index.Frameworks[name]
		if !listed {
			unknown = append(unknown, name)
			continue
		}
		since, parsed := parseMacOSVersion(raw)
		if !parsed {
			return nil, nil, fmt.Errorf("%s gives `%s` the unreadable version %q", macOSFrameworkVersionsFile, name, raw)
		}
		loaded = append(loaded, loadedFramework{name: name, since: since})
	}

	sort.Slice(loaded, func(i, j int) bool { return loaded[i].name < loaded[j].name })
	return loaded, unknown, nil
}

// systemFrameworkName picks the framework out of an install name, or reports that
// the path isn't a system framework at all.
//
// It takes the LAST `.framework` segment, so a subframework's own install name
// (`…/Quartz.framework/Frameworks/QuickLookUI.framework/…`) is judged as the
// subframework rather than as its umbrella. Those ship and disappear on their own
// schedules, which is the thing being judged.
func systemFrameworkName(path string) (string, bool) {
	if !strings.HasPrefix(path, systemFrameworkRoot) {
		return "", false
	}
	name := ""
	for _, segment := range strings.Split(path, "/") {
		if trimmed, found := strings.CutSuffix(segment, ".framework"); found {
			name = trimmed
		}
	}
	return name, name != ""
}
