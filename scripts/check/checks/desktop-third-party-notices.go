package checks

import (
	"bytes"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"slices"
	"sort"
	"strings"
)

// Pinned, like every other tool install here. `cargo-about`'s binary sits
// behind the `cli` feature, so the plain `cargo install` silently builds a
// library and installs nothing.
const cargoAboutVersion = "0.9.1"

// cargoAboutAsset is one host's prebuilt release tarball, plus the sha256 of
// that tarball.
type cargoAboutAsset struct {
	triple string
	sha256 string
}

// cargoAboutAssets maps `GOOS/GOARCH` to the upstream prebuilt binary for it.
// Fetching one takes ~2 s where `cargo install` spends ~3 min building from
// source, and that build is nearly the entire cost of this check on any machine
// that doesn't have the tool yet: a fresh clone, or a CI runner whose cache
// didn't restore.
//
// The checksums are the pin, exactly as `--version --locked` is for a
// `cargo install` (`CLAUDE.md` § Pin every tool install). We execute this
// binary, so a release asset that changed under us must fail loudly instead of
// running. Refresh with `curl -sSL <url> | shasum -a 256`, in the same commit
// that moves `cargoAboutVersion`.
//
// A host with no entry falls back to building from source; 0.9.1 publishes no
// `x86_64-apple-darwin` asset.
var cargoAboutAssets = map[string]cargoAboutAsset{
	"darwin/arm64": {"aarch64-apple-darwin", "6a38fe166d17a674269d4373256c0b6bd93acc2553e12de0517cb9ecc73c9c02"},
	"linux/amd64":  {"x86_64-unknown-linux-musl", "c0e7dc6f5d74b0beec5c0053d39ab24514c717d19acd91886907a22457ea9e98"},
	"linux/arm64":  {"aarch64-unknown-linux-musl", "d13ff19fedb566f859831c0b71c22120e7c598c7753d5f1018dd7353c6ced02a"},
}

// NoticesFileName is the generated attribution file at the repo root.
const NoticesFileName = "THIRD-PARTY-NOTICES.md"

// packagesJSONPath is the compact list the in-app Acknowledgements dialog
// loads. Same data, minus the license texts: 562 KB of legal text has no
// business in the app bundle, but the names do, so the dialog and the notices
// file can't disagree about what ships.
var packagesJSONPath = filepath.Join("apps", "desktop", "src", "lib", "licensing", "third-party-packages.gen.json")

// RunThirdPartyNotices regenerates `THIRD-PARTY-NOTICES.md` from the two
// lockfiles and fails in CI if the committed copy is stale.
//
// Cmdr ships hundreds of MIT / Apache-2.0 / BSD dependencies compiled into one
// binary. Those licenses require the copyright and permission notices to travel
// with the distribution, which a lockfile alone doesn't satisfy. This check
// produces the artifact that does, and the in-app Acknowledgements dialog reads
// the same data.
//
// **Cost is amortized by the runner's fingerprint cache, not by hand.** The
// `Inputs` list is the two lockfiles plus this check's own inputs, so on a run
// where no dependency moved the check is skipped before it ever shells out.
// That's why there's no bespoke staleness marker (contrast
// `desktop-bindings-fresh.go`, which predates leaning on the cache): declaring
// inputs is the supported way to say "only run when these change".
//
// Local runs rewrite the file, matching the auto-fix UX of oxfmt and clippy
// `--fix`. `--ci` never touches the working tree and fails on any drift.
func RunThirdPartyNotices(ctx *CheckContext) (CheckResult, error) {
	noticesPath := filepath.Join(ctx.RootDir, NoticesFileName)

	// Missing is a legitimate first-run state, not an error.
	original, _ := os.ReadFile(noticesPath)

	cargoAboutDir, err := ensureCargoAbout()
	if err != nil {
		return CheckResult{}, err
	}

	rust, err := collectRustCrates(ctx, cargoAboutDir)
	if err != nil {
		return CheckResult{}, err
	}
	npm, err := collectNpmPackages(ctx)
	if err != nil {
		return CheckResult{}, err
	}

	packagesPath := filepath.Join(ctx.RootDir, packagesJSONPath)
	originalPackages, _ := os.ReadFile(packagesPath)

	generated := renderNotices(rust, npm)
	generatedPackages, err := renderPackagesJSON(rust.packages, npm)
	if err != nil {
		return CheckResult{}, err
	}

	changed := !bytes.Equal(generated, original) || !bytes.Equal(generatedPackages, originalPackages)

	summary := fmt.Sprintf("%d Rust crates, %d npm packages, %d license texts",
		len(rust.packages), len(npm), len(rust.texts))

	if ctx.CI && changed {
		return CheckResult{}, fmt.Errorf(
			"%s is stale (%s). Run `pnpm check third-party-notices` and commit the result",
			NoticesFileName, summary)
	}
	if changed {
		if err := os.WriteFile(noticesPath, generated, 0o644); err != nil {
			return CheckResult{}, fmt.Errorf("couldn't write %s: %w", NoticesFileName, err)
		}
		if err := os.WriteFile(packagesPath, generatedPackages, 0o644); err != nil {
			return CheckResult{}, fmt.Errorf("couldn't write %s: %w", packagesJSONPath, err)
		}
		return SuccessWithChanges(fmt.Sprintf("%s regenerated (%s)", NoticesFileName, summary)), nil
	}
	return Success(fmt.Sprintf("%s in sync (%s)", NoticesFileName, summary)), nil
}

// renderPackagesJSON emits the dialog's data. Indented and newline-terminated
// so a dependency bump shows up as a readable diff rather than one long line.
func renderPackagesJSON(rust []attributedPackage, npm []attributedPackage) ([]byte, error) {
	payload := struct {
		Comment string              `json:"_comment"`
		Rust    []attributedPackage `json:"rust"`
		Npm     []attributedPackage `json:"npm"`
	}{
		Comment: "Generated by `pnpm check third-party-notices`. Don't edit by hand. Full license texts: THIRD-PARTY-NOTICES.md",
		Rust:    rust,
		Npm:     npm,
	}
	if rust == nil {
		payload.Rust = []attributedPackage{}
	}
	if npm == nil {
		payload.Npm = []attributedPackage{}
	}

	encoded, err := json.MarshalIndent(payload, "", "  ")
	if err != nil {
		return nil, fmt.Errorf("couldn't encode the package list: %w", err)
	}
	return append(encoded, '\n'), nil
}

// attributedPackage is one shipped dependency, in the form the notices file
// and the in-app dialog both want.
type attributedPackage struct {
	Name    string `json:"name"`
	Version string `json:"version"`
	License string `json:"license"`
	URL     string `json:"url"`
}

// licenseText is one distinct license text plus who it covers. Distinct means
// by content: 253 of the MIT texts differ only in their copyright line, and
// reproducing each is the entire point of the file.
type licenseText struct {
	ID   string
	Text string
	// SourcePath is the file the text was read from, relative to the crate root
	// (or to the repo, for a workspace crate). It goes into the notices file so
	// a crate that silently starts reporting a different license file shows up
	// as a readable diff line naming that file, instead of as a license count
	// that moved for no visible reason.
	SourcePath string
	UsedBy     []string
	sortKey    string
}

// crateClarification pins which file(s) a crate's notice text is read from.
type crateClarification struct {
	crate string
	// license is the crate's own declared expression, repeated verbatim. A
	// clarification REPLACES what cargo-about read from the crate, so narrowing
	// it here would make the Acknowledgements dialog claim a crate is MIT-only
	// when upstream actually offers a choice of three.
	license string
	files   []clarifiedFile
}

// clarifiedFile is one pinned license file and the SPDX id of the text in it,
// which is a single license even when the crate's own expression isn't.
type clarifiedFile struct {
	path    string
	license string
}

// licenseClarifications pin the license text for crates that ship more than one
// candidate file.
//
// **Without them this check can't be green on macOS and in CI at the same
// time.** When a crate carries several license files, cargo-about takes
// whichever the filesystem hands it first, and APFS and ext4 don't enumerate in
// the same order, so the notices file generated on a Mac and the one generated
// on the Linux runner differ permanently. Naming the file removes the choice.
//
// Checksums live in `clarificationChecksums` and are verified by
// `verifyClarifications` after the run, not trusted: cargo-about answers a
// stale checksum with a warning and a silent fall back to scanning.
var licenseClarifications = []crateClarification{
	{
		crate:   "libmimalloc-sys",
		license: "MIT",
		// Ships the Rust wrapper's own MIT (`LICENSE.txt`, Octavian Oncescu) AND
		// the vendored mimalloc C sources' MIT (Microsoft, Daan Leijen), and
		// compiles the latter into the binary, so both copyright holders have to
		// be credited. Naming both files also settles which one wins, since
		// letting the filesystem decide credited only one of them, and which one
		// depended on the machine. The v2 and v3 vendored trees carry a
		// byte-identical text, so v2 stands for both.
		files: []clarifiedFile{
			{"LICENSE.txt", "MIT"},
			{"c_src/mimalloc/v2/LICENSE", "MIT"},
		},
	},
	{
		crate:   "miniz_oxide",
		license: "MIT OR Zlib OR Apache-2.0",
		// `LICENSE` and `LICENSE-MIT.md` are the same text apart from one blank
		// line, so either is correct and only the ambiguity is a problem.
		files: []clarifiedFile{
			{"LICENSE", "MIT"},
		},
	},
}

// clarificationChecksums maps a clarified file to the sha256 cargo-about wants
// alongside it. Refresh one with:
//
//	shasum -a 256 ~/.cargo/registry/src/*/<crate>-<version>/<path>
var clarificationChecksums = map[string]string{
	"libmimalloc-sys/LICENSE.txt":               "a08554fab028af3af047e8588d32d019662c1b1bf07959a195a2f75fe340726b",
	"libmimalloc-sys/c_src/mimalloc/v2/LICENSE": "82ade5d7d9b029044b5fbbc326207fc47c2d44ee4dd71e8a559c36d728217de9",
	"miniz_oxide/LICENSE":                       "4108245a1f2df9d4e94df8abed5b4ba0759bb2f9b40a6b939f1be141077ae50b",
}

// verifyClarifications fails when a pinned crate's text didn't actually come
// from the pinned file.
//
// A clarification that's merely present proves nothing: cargo-about warns about
// a checksum mismatch, exits 0, and falls back to scanning the crate directory,
// which is the filesystem-order-dependent behavior the pins exist to remove. So
// the result is what gets checked, and only a crate bump that rewrites one of
// these files can trip it.
func verifyClarifications(sourcesByCrate map[string][]string) error {
	for _, clarification := range licenseClarifications {
		expected := make([]string, 0, len(clarification.files))
		for _, file := range clarification.files {
			expected = append(expected, file.path)
		}
		sort.Strings(expected)
		actual := slices.Clone(sourcesByCrate[clarification.crate])
		sort.Strings(actual)

		if slices.Equal(expected, actual) {
			continue
		}
		return fmt.Errorf(
			"the license pin for `%s` didn't take: its notice text came from %v, not the pinned %v.\n"+
				"cargo-about only warns when a pinned checksum goes stale, then falls back to scanning the crate,\n"+
				"and that scan follows filesystem order: %s would then generate differently on macOS and in CI.\n"+
				"A dependency bump probably rewrote the file. Refresh `clarificationChecksums` in\n"+
				"`scripts/check/checks/desktop-third-party-notices.go` with:\n"+
				"  shasum -a 256 ~/.cargo/registry/src/*/%s-*/<path>",
			clarification.crate, actual, expected, NoticesFileName, clarification.crate)
	}
	return nil
}

type rustCollection struct {
	packages []attributedPackage
	texts    []licenseText
}

// --- installing cargo-about ---

// ensureCargoAbout makes a `cargo-about` matching `cargoAboutVersion` available
// and returns a directory to put in front of PATH for the generate call (empty
// when the one already on PATH is the pinned build).
//
// The version is verified rather than assumed from mere presence: a stale local
// binary harvests license files by its own rules, which hands David a diff that
// CI can't reproduce. That's the same class of drift the clarifications below
// exist to remove, so the tool that produces the file is pinned as hard as the
// file's inputs are.
func ensureCargoAbout() (string, error) {
	if installedCargoAboutVersion() == cargoAboutVersion {
		return "", nil
	}

	binDir, err := cargoBinDir()
	if err != nil {
		return "", err
	}

	asset, ok := cargoAboutAssets[runtime.GOOS+"/"+runtime.GOARCH]
	if !ok {
		installCmd := exec.Command("cargo", "install", "cargo-about",
			"--version", cargoAboutVersion, "--locked", "--features", "cli")
		if output, err := RunCommand(installCmd, true); err != nil {
			return "", fmt.Errorf("failed to install cargo-about\n%s", indentOutput(output))
		}
		return binDir, nil
	}

	if err := downloadCargoAbout(asset, binDir); err != nil {
		return "", err
	}
	return binDir, nil
}

// installedCargoAboutVersion returns the version of the `cargo-about` on PATH,
// or "" when there is none or it can't say.
func installedCargoAboutVersion() string {
	if !CommandExists("cargo-about") {
		return ""
	}
	output, err := exec.Command("cargo-about", "--version").Output()
	if err != nil {
		return ""
	}
	// `cargo-about 0.9.1`
	fields := strings.Fields(string(output))
	if len(fields) < 2 {
		return ""
	}
	return fields[1]
}

// cargoBinDir is where cargo puts installed binaries, which is also where this
// check drops the downloaded one: it's already on PATH for the user, and CI's
// `rust-cache` carries `${CARGO_HOME}/bin` between runs, so the download
// happens once per cache generation rather than once per run.
func cargoBinDir() (string, error) {
	if cargoHome := os.Getenv("CARGO_HOME"); cargoHome != "" {
		return filepath.Join(cargoHome, "bin"), nil
	}
	home, err := os.UserHomeDir()
	if err != nil {
		return "", fmt.Errorf("couldn't locate the home directory to install cargo-about into: %w", err)
	}
	return filepath.Join(home, ".cargo", "bin"), nil
}

// downloadCargoAbout installs the pinned prebuilt binary for this host.
func downloadCargoAbout(asset cargoAboutAsset, binDir string) error {
	url := fmt.Sprintf(
		"https://github.com/EmbarkStudios/cargo-about/releases/download/%s/cargo-about-%s-%s.tar.gz",
		cargoAboutVersion, cargoAboutVersion, asset.triple)
	return InstallPinnedBinary(url, asset.sha256, "cargo-about", filepath.Join(binDir, "cargo-about"))
}

// --- cargo-about ---

type aboutPackage struct {
	Name       string `json:"name"`
	Version    string `json:"version"`
	License    string `json:"license"`
	Repository string `json:"repository"`
	Homepage   string `json:"homepage"`
}

type aboutOutput struct {
	Licenses []struct {
		ID   string `json:"id"`
		Text string `json:"text"`
		// SourcePath is the file the text was read from: absolute for a crate
		// cargo-about scanned, crate-relative for one a clarification pinned.
		SourcePath string `json:"source_path"`
		UsedBy     []struct {
			Crate aboutPackage `json:"crate"`
		} `json:"used_by"`
	} `json:"licenses"`
	Crates []struct {
		Package aboutPackage `json:"package"`
		License string       `json:"license"`
	} `json:"crates"`
}

func collectRustCrates(ctx *CheckContext, cargoAboutDir string) (rustCollection, error) {
	tauriDir := filepath.Join(ctx.RootDir, "apps", "desktop", "src-tauri")

	configPath, err := writeAboutConfig(ctx.RootDir, tauriDir)
	if err != nil {
		return rustCollection{}, err
	}
	defer func() { _ = os.Remove(configPath) }()

	outPath := filepath.Join(os.TempDir(), "cmdr-about.json")
	defer func() { _ = os.Remove(outPath) }()

	cmd := exec.Command("cargo", "about", "generate", "-c", configPath, "--format", "json", "-o", outPath)
	cmd.Dir = tauriDir
	if cargoAboutDir != "" {
		// `cargo about` resolves the subcommand off PATH, so the build we just
		// installed has to come first: a different `cargo-about` earlier on PATH
		// would silently generate the file instead.
		cmd.Env = append(os.Environ(),
			"PATH="+cargoAboutDir+string(os.PathListSeparator)+os.Getenv("PATH"))
	}
	if output, err := RunCommand(cmd, true); err != nil {
		return rustCollection{}, fmt.Errorf(
			"`cargo about generate` failed. A dependency may carry a license that isn't in `deny.toml`'s allow list;"+
				" that needs a human decision, not an automatic addition.\n%s", indentOutput(output))
	}

	raw, err := os.ReadFile(outPath)
	if err != nil {
		return rustCollection{}, fmt.Errorf("couldn't read cargo-about output: %w", err)
	}
	var parsed aboutOutput
	if err := json.Unmarshal(raw, &parsed); err != nil {
		return rustCollection{}, fmt.Errorf("couldn't parse cargo-about output: %w", err)
	}

	collection := rustCollection{}
	for _, crate := range parsed.Crates {
		collection.packages = append(collection.packages, attributedPackage{
			Name:    crate.Package.Name,
			Version: crate.Package.Version,
			License: crate.License,
			URL:     firstNonEmpty(crate.Package.Repository, crate.Package.Homepage),
		})
	}
	sourcesByCrate := map[string][]string{}
	for _, license := range parsed.Licenses {
		users := make([]string, 0, len(license.UsedBy))
		source := crateRelativeSource(license.SourcePath, ctx.RootDir)
		for _, used := range license.UsedBy {
			users = append(users, fmt.Sprintf("%s %s", used.Crate.Name, used.Crate.Version))
			sourcesByCrate[used.Crate.Name] = append(sourcesByCrate[used.Crate.Name], source)
		}
		sort.Strings(users)
		collection.texts = append(collection.texts, licenseText{
			ID: license.ID,
			// Normalize line endings. Some upstream license files ship CRLF, and
			// git would rewrite them to LF on checkout: the committed file would
			// then never match a fresh regeneration, so the check would report
			// drift forever on any clean clone.
			Text:       normalizeNewlines(license.Text),
			SourcePath: source,
			UsedBy:     users,
			sortKey:    license.ID + "\x00" + strings.Join(users, ","),
		})
	}

	if err := verifyClarifications(sourcesByCrate); err != nil {
		return rustCollection{}, err
	}

	sortPackages(collection.packages)
	sort.Slice(collection.texts, func(i, j int) bool {
		return collection.texts[i].sortKey < collection.texts[j].sortKey
	})
	return collection, nil
}

// registrySrcMarker is the path segment that precedes
// `<index>/<crate>-<version>/` in an extracted registry crate's location.
const registrySrcMarker = "/registry/src/"

// crateRelativeSource turns cargo-about's absolute license-file path into one
// that means the same thing on every machine: relative to the crate root for a
// registry crate, relative to the repo for a workspace crate. The raw path
// would bake `/Users/<whoever>` and a registry index hash into a committed
// file. A clarified crate already reports a crate-relative path.
func crateRelativeSource(sourcePath, rootDir string) string {
	if sourcePath == "" || !filepath.IsAbs(sourcePath) {
		return sourcePath
	}
	if index := strings.Index(sourcePath, registrySrcMarker); index >= 0 {
		rest := sourcePath[index+len(registrySrcMarker):]
		// `<index>/<crate>-<version>/<path within the crate>`
		if parts := strings.SplitN(rest, string(os.PathSeparator), 3); len(parts) == 3 {
			return parts[2]
		}
	}
	if relative, err := filepath.Rel(rootDir, sourcePath); err == nil && !strings.HasPrefix(relative, "..") {
		return relative
	}
	return filepath.Base(sourcePath)
}

// writeAboutConfig derives cargo-about's accepted-license list from
// `deny.toml`.
//
// Single-sourced on purpose: `deny.toml` already decides which licenses Cmdr
// may depend on, and it shrink-wraps itself (`unused-allowed-license = "deny"`),
// so it can't drift. A second hand-maintained list here would answer the same
// question with a different answer sooner or later.
func writeAboutConfig(rootDir, tauriDir string) (string, error) {
	denyPath := DenyConfigPath(rootDir)
	denyRaw, err := os.ReadFile(denyPath)
	if err != nil {
		return "", fmt.Errorf("couldn't read %s: %w", denyPath, err)
	}

	accepted := parseDenyAllowList(string(denyRaw))
	if len(accepted) == 0 {
		return "", fmt.Errorf("no `allow = [...]` licenses found in %s", denyPath)
	}

	quoted := make([]string, 0, len(accepted))
	for _, id := range accepted {
		// Our own crate is excluded by `[private] ignore`, so its BSL entry
		// would only ever be dead weight in cargo-about's list.
		if strings.HasPrefix(id, "LicenseRef-") {
			continue
		}
		quoted = append(quoted, fmt.Sprintf("%q", id))
	}

	config := fmt.Sprintf(`# Generated by the `+"`third-party-notices`"+` check. Do not edit; see third-party-notices.go.
accepted = [%s]
targets = ["aarch64-apple-darwin", "x86_64-apple-darwin"]
ignore-dev-dependencies = true
workarounds = ["ring"]

[private]
ignore = true
%s`, strings.Join(quoted, ", "), renderClarifications())

	configPath := filepath.Join(os.TempDir(), "cmdr-about-config.toml")
	if err := os.WriteFile(configPath, []byte(config), 0o644); err != nil {
		return "", fmt.Errorf("couldn't write cargo-about config: %w", err)
	}
	return configPath, nil
}

// renderClarifications emits the `[<crate>.clarify]` sections. A file with no
// entry in `clarificationChecksums` is written without one, which cargo-about
// rejects outright: better a hard failure naming the crate than a silent fall
// back to scanning it.
func renderClarifications() string {
	var b strings.Builder
	for _, clarification := range licenseClarifications {
		fmt.Fprintf(&b, "\n[%s.clarify]\nlicense = %q\n", clarification.crate, clarification.license)
		for _, file := range clarification.files {
			fmt.Fprintf(&b, "\n[[%s.clarify.files]]\npath = %q\nlicense = %q\nchecksum = %q\n",
				clarification.crate, file.path, file.license,
				clarificationChecksums[clarification.crate+"/"+file.path])
		}
	}
	return b.String()
}

// parseDenyAllowList pulls the SPDX ids out of deny.toml's `[licenses] allow`
// array. Kept to a line scan rather than a TOML dependency: the array is one
// quoted id per line with optional trailing comments, and a scanner that
// returns nothing fails loudly at the call site.
func parseDenyAllowList(contents string) []string {
	var ids []string
	inAllow := false
	for _, line := range strings.Split(contents, "\n") {
		trimmed := strings.TrimSpace(line)
		if !inAllow {
			if strings.HasPrefix(trimmed, "allow") && strings.Contains(trimmed, "[") {
				inAllow = true
			}
			continue
		}
		if strings.HasPrefix(trimmed, "]") {
			break
		}
		if start := strings.Index(trimmed, `"`); start >= 0 {
			if end := strings.Index(trimmed[start+1:], `"`); end >= 0 {
				ids = append(ids, trimmed[start+1:start+1+end])
			}
		}
	}
	return ids
}

// --- pnpm ---

type pnpmPackage struct {
	Name     string   `json:"name"`
	Versions []string `json:"versions"`
	License  string   `json:"license"`
	Homepage string   `json:"homepage"`
}

func collectNpmPackages(ctx *CheckContext) ([]attributedPackage, error) {
	cmd := exec.Command("pnpm", "licenses", "list", "--json", "--prod", "--filter", "@cmdr/desktop")
	cmd.Dir = ctx.RootDir
	output, err := RunCommand(cmd, true)
	if err != nil {
		return nil, fmt.Errorf("`pnpm licenses list` failed\n%s", indentOutput(output))
	}

	// pnpm prints the JSON object last; anything before it is progress noise.
	start := strings.Index(output, "{")
	if start < 0 {
		return nil, fmt.Errorf("`pnpm licenses list` produced no JSON\n%s", indentOutput(output))
	}

	byLicense := map[string][]pnpmPackage{}
	if err := json.Unmarshal([]byte(output[start:]), &byLicense); err != nil {
		return nil, fmt.Errorf("couldn't parse `pnpm licenses list` output: %w", err)
	}

	var packages []attributedPackage
	for license, entries := range byLicense {
		for _, entry := range entries {
			for _, version := range entry.Versions {
				packages = append(packages, attributedPackage{
					Name:    entry.Name,
					Version: version,
					License: firstNonEmpty(entry.License, license),
					URL:     entry.Homepage,
				})
			}
		}
	}
	sortPackages(packages)
	return packages, nil
}

// --- rendering ---

func renderNotices(rust rustCollection, npm []attributedPackage) []byte {
	var b strings.Builder

	b.WriteString("# Third-party notices\n\n")
	b.WriteString("Cmdr is built on open-source software. This file lists every third-party package that ships inside\n")
	b.WriteString("the app, with its license, and reproduces the license texts in full.\n\n")
	b.WriteString("Generated from `Cargo.lock` and `pnpm-lock.yaml` by `pnpm check third-party-notices`. Don't edit it\n")
	b.WriteString("by hand; edit the generator at `scripts/check/checks/desktop-third-party-notices.go` instead.\n\n")

	fmt.Fprintf(&b, "- Rust crates: %d\n", len(rust.packages))
	fmt.Fprintf(&b, "- npm packages: %d\n", len(npm))
	fmt.Fprintf(&b, "- Distinct license texts: %d\n\n", len(rust.texts))

	writePackageSection(&b, "Rust crates", rust.packages)
	writePackageSection(&b, "npm packages", npm)

	b.WriteString("## License texts\n\n")
	b.WriteString("Each distinct text is reproduced once, with the packages it covers and the file it was read from.\n")
	b.WriteString("Many differ only in their copyright line, which is exactly what these licenses require to be\n")
	b.WriteString("carried along.\n\n")
	for _, text := range rust.texts {
		fmt.Fprintf(&b, "### %s\n\n", text.ID)
		fmt.Fprintf(&b, "Covers: %s\n\n", strings.Join(text.UsedBy, ", "))
		if text.SourcePath != "" {
			fmt.Fprintf(&b, "Text from: `%s`\n\n", text.SourcePath)
		}
		b.WriteString("```text\n")
		b.WriteString(strings.TrimRight(text.Text, "\n"))
		b.WriteString("\n```\n\n")
	}

	return []byte(b.String())
}

func writePackageSection(b *strings.Builder, title string, packages []attributedPackage) {
	fmt.Fprintf(b, "## %s\n\n", title)
	for _, pkg := range packages {
		fmt.Fprintf(b, "- **%s** %s, %s", pkg.Name, pkg.Version, pkg.License)
		if pkg.URL != "" {
			fmt.Fprintf(b, ", <%s>", pkg.URL)
		}
		b.WriteString("\n")
	}
	b.WriteString("\n")
}

func sortPackages(packages []attributedPackage) {
	sort.Slice(packages, func(i, j int) bool {
		if packages[i].Name != packages[j].Name {
			return strings.ToLower(packages[i].Name) < strings.ToLower(packages[j].Name)
		}
		return packages[i].Version < packages[j].Version
	})
}

// normalizeNewlines makes the generated file LF-only, so what git stores and
// what a regeneration produces can't disagree.
func normalizeNewlines(text string) string {
	return strings.ReplaceAll(strings.ReplaceAll(text, "\r\n", "\n"), "\r", "\n")
}

func firstNonEmpty(values ...string) string {
	for _, value := range values {
		if value != "" {
			return value
		}
	}
	return ""
}
