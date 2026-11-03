package checks

import (
	"bytes"
	"fmt"
	"os"
	"path/filepath"
	"slices"
	"strings"
)

// RunPropertyMutants applies every mutant record and runs its catcher, failing
// when a property stays green under the change its record says it catches (a
// vacuous property), goes red for some other reason, or is already red without
// the mutant (an input that proves nothing). It works in a mirror of the tree
// under `target/property-mutants/`, never in the checkout: a mutant left behind
// by an interrupted run must never reach a commit, and the lanes running beside
// this one must never compile one.
func RunPropertyMutants(ctx *CheckContext) (CheckResult, error) {
	return runPropertyMutants(ctx, propertyKinds, filepath.Join(ctx.RootDir, "target", "property-mutants"))
}

func runPropertyMutants(ctx *CheckContext, kinds []propertyKind, workDir string) (CheckResult, error) {
	records, problems, err := loadMutantRecords(ctx.RootDir, kinds)
	if err != nil {
		return CheckResult{}, err
	}
	if len(problems) > 0 {
		return CheckResult{}, fmt.Errorf("%d mutant %s can't be applied; `pnpm check mutant-records` lists them:\n  - %s",
			len(problems), Pluralize(len(problems), "record", "records"), strings.Join(problems, "\n  - "))
	}
	if len(records) == 0 {
		return Success("no mutant records to run"), nil
	}

	for _, kind := range kinds {
		if kind.prepare == nil || !slices.ContainsFunc(records, func(r mutantRecord) bool {
			name, _ := r.kindAndName()
			return name == kind.name
		}) {
			continue
		}
		if err := kind.prepare(); err != nil {
			return CheckResult{}, err
		}
	}

	tree := filepath.Join(workDir, "tree")
	if err := syncMutantTree(ctx.RootDir, tree, filepath.Join(workDir, "tree-files.txt")); err != nil {
		return CheckResult{}, fmt.Errorf("couldn't mirror the tree into %s: %w", tree, err)
	}
	failures := killMutants(tree, kinds, records)
	if len(failures) > 0 {
		return CheckResult{}, fmt.Errorf("%d of %d mutant %s didn't prove %s property can fail (`scripts/check/checks/DETAILS.md` § \"Property mutants\"):\n%s",
			len(failures), len(records), Pluralize(len(records), "record", "records"),
			Pluralize(len(failures), "its", "their"), strings.Join(failures, "\n"))
	}
	return Success(fmt.Sprintf("%d %s caught, each by the property it's recorded against",
		len(records), Pluralize(len(records), "mutant", "mutants"))), nil
}

// killMutants runs every catcher on the clean tree, which must pass, then once
// per mutant, which must fail printing the record's `expect`. It returns one
// message per record that didn't hold up, and leaves the tree clean.
func killMutants(tree string, kinds []propertyKind, records []mutantRecord) []string {
	var failures []string
	baselineGreen := make([]bool, len(records))
	for i, r := range records {
		kind, ok := r.kindIn(kinds)
		if !ok {
			failures = append(failures, fmt.Sprintf("  %s: no property kind runs it", r.label()))
			continue
		}
		run, err := kind.run(tree, r)
		switch {
		case err != nil:
			failures = append(failures, fmt.Sprintf("  %s: doesn't run without the mutant: %v", r.label(), err))
		case run.failed:
			failures = append(failures, fmt.Sprintf("  %s: is red WITHOUT the mutant, so its failure can't show the mutant was caught:\n%s",
				r.label(), indentOutput(run.output)))
		default:
			baselineGreen[i] = true
		}
	}
	for i, r := range records {
		kind, ok := r.kindIn(kinds)
		if !baselineGreen[i] || !ok {
			continue
		}
		if msg := killMutant(tree, kind, r); msg != "" {
			failures = append(failures, msg)
		}
	}
	return failures
}

// killMutant applies one record, runs its catcher, and puts the file back. It
// returns what went wrong, or "" when the property caught its mutant.
func killMutant(tree string, kind propertyKind, r mutantRecord) string {
	restore, err := applyMutant(tree, r)
	if err != nil {
		return fmt.Sprintf("  %s: couldn't apply the mutant: %v", r.label(), err)
	}
	run, runErr := kind.run(tree, r)
	if err := restore(); err != nil {
		return fmt.Sprintf("  %s: couldn't restore %s in the mirror (the next run re-copies it): %v", r.label(), r.File, err)
	}
	switch {
	case runErr != nil:
		return fmt.Sprintf("  %s: the mutant doesn't run: %v", r.label(), runErr)
	case !run.failed:
		return fmt.Sprintf("  %s: SURVIVED. With `%s` turned into `%s` in %s, the property still passes, so it doesn't catch what the record says (%s). "+
			"Make the property assert it, or aim the record at code the property does guard.",
			r.label(), r.Find, r.Replace, r.File, r.Why)
	case !strings.Contains(run.output, r.Expect):
		return fmt.Sprintf("  %s: went red without printing %q, so something other than this property caught the mutant:\n%s",
			r.label(), r.Expect, indentOutput(run.output))
	}
	return ""
}

// syncMutantTree makes tree a copy of every file git lists in rootDir, tracked
// or not yet added, writing only the files whose bytes differ. A file left
// alone keeps its mtime, so cargo's fingerprints stay warm across runs; a file
// written gets a fresh one, so nothing built from a mutant reads as up to date.
// Files listed last run (manifest) and gone now are deleted; build output the
// listing never names (each workspace's `target/`) stays.
func syncMutantTree(rootDir, tree, manifest string) error {
	files, err := corpus.GitLsFiles(rootDir, "-co", "--exclude-standard")
	if err != nil {
		return err
	}
	current := map[string]bool{}
	for _, rel := range files {
		copied, err := mirrorFile(filepath.Join(rootDir, filepath.FromSlash(rel)), filepath.Join(tree, filepath.FromSlash(rel)))
		if err != nil {
			return err
		}
		if copied {
			current[rel] = true
		}
	}
	if previous, err := os.ReadFile(manifest); err == nil {
		for rel := range strings.SplitSeq(string(previous), "\n") {
			if rel != "" && !current[rel] && isRepoRelative(rel) {
				if err := os.Remove(filepath.Join(tree, filepath.FromSlash(rel))); err != nil && !os.IsNotExist(err) {
					return err
				}
			}
		}
	}
	listed := sortedKeys(current)
	if err := os.MkdirAll(filepath.Dir(manifest), 0o755); err != nil {
		return err
	}
	return os.WriteFile(manifest, []byte(strings.Join(listed, "\n")+"\n"), 0o644)
}

// mirrorFile makes dst match src, a regular file or a symlink. It reports
// false for a source that's gone (a deletion git hasn't staged yet) or is
// neither, which the mirror then doesn't hold.
func mirrorFile(src, dst string) (bool, error) {
	info, err := os.Lstat(src)
	if os.IsNotExist(err) {
		return false, nil
	}
	if err != nil {
		return false, err
	}
	switch {
	case info.Mode()&os.ModeSymlink != 0:
		return true, mirrorSymlink(src, dst)
	case info.Mode().IsRegular():
		return true, mirrorRegularFile(src, dst, info.Mode().Perm())
	default:
		return false, nil
	}
}

func mirrorSymlink(src, dst string) error {
	target, err := os.Readlink(src)
	if err != nil {
		return err
	}
	if existing, err := os.Readlink(dst); err == nil && existing == target {
		return nil
	}
	if err := replaceInMirror(dst); err != nil {
		return err
	}
	return os.Symlink(target, dst)
}

// mirrorRegularFile leaves dst alone, mtime and all, when its bytes and mode
// already match.
func mirrorRegularFile(src, dst string, perm os.FileMode) error {
	want, err := os.ReadFile(src)
	if err != nil {
		return err
	}
	if info, err := os.Lstat(dst); err == nil && info.Mode().IsRegular() && info.Mode().Perm() == perm {
		if have, err := os.ReadFile(dst); err == nil && bytes.Equal(have, want) {
			return nil
		}
	}
	if err := replaceInMirror(dst); err != nil {
		return err
	}
	return os.WriteFile(dst, want, perm)
}

// replaceInMirror clears dst's spot, whatever holds it, and makes its parent.
func replaceInMirror(dst string) error {
	if err := os.MkdirAll(filepath.Dir(dst), 0o755); err != nil {
		return err
	}
	if err := os.RemoveAll(dst); err != nil && !os.IsNotExist(err) {
		return err
	}
	return nil
}
