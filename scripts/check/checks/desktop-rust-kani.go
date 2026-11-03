package checks

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
)

// kaniVersion is the Kani this lane installs when the one on PATH differs.
// Pinned like every other tool install (`checks/CLAUDE.md`); Kani brings its own
// nightly and CBMC, so this one pin moves all three.
const kaniVersion = "0.68.0"

// kaniProofAttribute marks a harness; the `fn` after it is the proof.
const kaniProofAttribute = "#[kani::proof]"

// kaniProof is one `#[kani::proof]` harness in a library crate's `src/`.
type kaniProof struct {
	// pkg is the crate's package name, for `cargo kani -p`.
	pkg string
	// harness is the harness's path inside the crate, for `--harness … --exact`.
	harness string
	// file is the repo-relative file it's written in.
	file string
}

// property is the proof's name under the `kani` property kind: everything
// `cargo kani` needs to run it, so a record names exactly one proof.
func (p kaniProof) property() string {
	return p.pkg + "/" + p.harness
}

// parseKaniProperty splits a `kani` property name back into package and harness.
func parseKaniProperty(name string) (pkg, harness string, ok bool) {
	pkg, harness, ok = strings.Cut(name, "/")
	return pkg, harness, ok && pkg != "" && harness != ""
}

// kaniProofFnPattern reads the harness fn's name off its signature.
var kaniProofFnPattern = regexp.MustCompile(`^\s*(?:pub(?:\([^)]*\))?\s+)?fn\s+(\w+)`)

// kaniPackageNamePattern reads `name = "…"` in a manifest's `[package]` table.
var kaniPackageNamePattern = regexp.MustCompile(`^name\s*=\s*"([^"]+)"`)

// findKaniProofs lists every harness in the tree, sorted by property. It reads
// tracked and new `.rs` files outside `vendor/`, so the fast lane sees a proof
// before its first commit. Proofs live in a library crate's `src/` and follow
// the file layout (no `#[path]`), which is what lets a file name its module.
func findKaniProofs(rootDir string) ([]kaniProof, error) {
	// The record files' listing, so the fast lane asks git once.
	files, err := corpus.GitLsFiles(rootDir, "-co", "--exclude-standard")
	if err != nil {
		return nil, err
	}
	var proofs []kaniProof
	for _, rel := range files {
		if !strings.HasSuffix(rel, ".rs") || strings.HasPrefix(rel, "vendor/") {
			continue
		}
		path := filepath.Join(rootDir, filepath.FromSlash(rel))
		if !corpus.Exists(path) {
			continue
		}
		source, err := corpus.ReadFile(path)
		if err != nil {
			return nil, err
		}
		if !strings.Contains(source, kaniProofAttribute) {
			continue
		}
		found, err := kaniProofsIn(rootDir, rel, source)
		if err != nil {
			return nil, err
		}
		proofs = append(proofs, found...)
	}
	sort.Slice(proofs, func(i, j int) bool { return proofs[i].property() < proofs[j].property() })
	return proofs, nil
}

// kaniProofsIn reads the harnesses in one file.
func kaniProofsIn(rootDir, rel, source string) ([]kaniProof, error) {
	pkg, module, err := kaniModuleOf(rootDir, rel)
	if err != nil {
		return nil, err
	}
	lines := strings.Split(source, "\n")
	var proofs []kaniProof
	for i, line := range lines {
		if strings.TrimSpace(line) != kaniProofAttribute {
			continue
		}
		name := ""
		// Other attributes (`#[kani::unwind(…)]`) may sit between it and the fn.
		for _, next := range lines[i+1:] {
			if m := kaniProofFnPattern.FindStringSubmatch(next); m != nil {
				name = m[1]
				break
			}
			if !strings.HasPrefix(strings.TrimSpace(next), "#[") {
				break
			}
		}
		if name == "" {
			return nil, fmt.Errorf("%s:%d: `%s` isn't followed by a fn", rel, i+1, kaniProofAttribute)
		}
		harness := name
		if module != "" {
			harness = module + "::" + name
		}
		proofs = append(proofs, kaniProof{pkg: pkg, harness: harness, file: rel})
	}
	return proofs, nil
}

// kaniModuleOf finds the crate a source file belongs to (the nearest manifest
// with a `[package]`) and the file's module path inside it: `src/read/name/
// proofs.rs` is `read::name::proofs`, `src/lib.rs` is the crate root.
func kaniModuleOf(rootDir, rel string) (string, string, error) {
	dir := filepath.Dir(filepath.FromSlash(rel))
	for dir != "." && dir != string(filepath.Separator) {
		pkg, err := kaniPackageName(filepath.Join(rootDir, dir, "Cargo.toml"))
		if err != nil {
			return "", "", err
		}
		if pkg == "" {
			dir = filepath.Dir(dir)
			continue
		}
		inCrate, err := filepath.Rel(filepath.Join(dir, "src"), filepath.FromSlash(rel))
		if err != nil || !filepath.IsLocal(inCrate) {
			return "", "", fmt.Errorf("%s: a Kani proof belongs in its crate's `src/`", rel)
		}
		parts := strings.Split(strings.TrimSuffix(filepath.ToSlash(inCrate), ".rs"), "/")
		if last := parts[len(parts)-1]; last == "mod" || (len(parts) == 1 && last == "lib") {
			parts = parts[:len(parts)-1]
		}
		if len(parts) == 1 && parts[0] == "main" {
			return "", "", fmt.Errorf("%s: a Kani proof belongs in a library crate; the lane runs `--lib`", rel)
		}
		return pkg, strings.Join(parts, "::"), nil
	}
	return "", "", fmt.Errorf("%s: no crate manifest above it", rel)
}

// kaniPackageName reads a manifest's package name; "" for no manifest, or one
// with no `[package]` (a virtual workspace root).
func kaniPackageName(manifest string) (string, error) {
	data, err := os.ReadFile(manifest)
	if os.IsNotExist(err) {
		return "", nil
	}
	if err != nil {
		return "", err
	}
	inPackage := false
	for line := range strings.SplitSeq(string(data), "\n") {
		line = strings.TrimSpace(line)
		if strings.HasPrefix(line, "[") {
			inPackage = line == "[package]"
			continue
		}
		if m := kaniPackageNamePattern.FindStringSubmatch(line); inPackage && m != nil {
			return m[1], nil
		}
	}
	return "", nil
}

// RunKani verifies every Kani proof, one `cargo kani` per crate that has any.
// Each proof states its own bound and assumptions in its file; how to write one:
// `docs/tooling/testing.md` § "Kani proofs".
func RunKani(ctx *CheckContext) (CheckResult, error) {
	proofs, err := findKaniProofs(ctx.RootDir)
	if err != nil {
		return CheckResult{}, err
	}
	if len(proofs) == 0 {
		return Skipped("no Kani proofs in the tree"), nil
	}
	if err := ensureKani(); err != nil {
		return CheckResult{}, err
	}
	perPkg := map[string]int{}
	for _, proof := range proofs {
		perPkg[proof.pkg]++
	}
	var failures []string
	for _, pkg := range sortedKeys(perPkg) {
		cmd := exec.Command("cargo", "kani", "-p", pkg, "--lib", "--output-format", "terse")
		cmd.Dir = ctx.RootDir
		output, err := RunCommand(cmd, true)
		if err != nil {
			failures = append(failures, fmt.Sprintf("%s:\n%s", pkg, indentOutput(kaniFindingReport(output))))
		}
	}
	if len(failures) > 0 {
		return CheckResult{}, fmt.Errorf("the Kani proofs failed in %d of %d crates\n%s",
			len(failures), len(perPkg), strings.Join(failures, "\n"))
	}
	return Success(fmt.Sprintf("%d Kani proofs verified across %d crates", len(proofs), len(perPkg))), nil
}

// kaniFindingReport keeps what a failed run says went wrong: each harness
// that ran, the checks that failed, and the verdicts. Without a verdict (the
// crate didn't build), the whole output passes through.
func kaniFindingReport(output string) string {
	if !strings.Contains(output, "VERIFICATION:-") {
		return trimBuildNoise(output)
	}
	var kept []string
	for _, line := range strings.Split(output, "\n") {
		trimmed := strings.TrimSpace(line)
		if strings.HasPrefix(trimmed, "Checking harness") || strings.HasPrefix(trimmed, "Failed Checks:") ||
			strings.HasPrefix(trimmed, "File:") || strings.HasPrefix(trimmed, "VERIFICATION:-") ||
			strings.HasPrefix(trimmed, "CBMC timed out") {
			kept = append(kept, line)
		}
	}
	return strings.Join(kept, "\n")
}

// ensureKani installs the pinned Kani and runs its setup (its nightly and
// CBMC) unless that exact version answers already.
func ensureKani() error {
	want := "Kani Rust Verifier " + kaniVersion + " (cargo plugin)"
	if CommandExists("cargo-kani") {
		output, err := RunCommand(exec.Command("cargo", "kani", "--version"), true)
		if err == nil && strings.HasPrefix(strings.TrimSpace(output), want) {
			return nil
		}
	}
	installCmd := exec.Command("cargo", "install", "kani-verifier", "--version", kaniVersion, "--locked")
	if output, err := RunCommand(installCmd, true); err != nil {
		return fmt.Errorf("failed to install Kani %s\n%s", kaniVersion, indentOutput(output))
	}
	if output, err := RunCommand(exec.Command("cargo", "kani", "setup"), true); err != nil {
		return fmt.Errorf("failed to set Kani %s up\n%s", kaniVersion, indentOutput(output))
	}
	return nil
}
