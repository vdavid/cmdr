package checks

import (
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
)

// detailsRefRe matches a Markdown link or backtick path whose target ends in
// `DETAILS.md`. We follow docs-reachable's syntax-agnostic stance ("a reference
// is any mention: Markdown link, backtick path, or bare path token"), and accept
// a reference to any DETAILS.md, not strictly the sibling: the structural
// guarantee is the sibling-exists half (checked separately); the reference half
// only confirms the CLAUDE.md author knows the C/D pull tier exists. So a
// Markdown link `[…](…/DETAILS.md)` or a backtick path “ `…/DETAILS.md` “ all
// count. The leading path segment before `DETAILS.md` is unconstrained.
var detailsRefRe = regexp.MustCompile(
	"\\]\\([^)]*DETAILS\\.md(?:#[^)]*)?\\)" + // markdown link to any …/DETAILS.md
		"|`[^`]*DETAILS\\.md`") // backtick `…/DETAILS.md`

type detailsSiblingViolation struct {
	claudeMd string // repo-relative path to the offending CLAUDE.md
	reason   string // why it failed (no sibling, or no reference)
}

// RunClaudeMdDetailsSibling enforces the C/D pair contract: every non-root
// CLAUDE.md must have a sibling DETAILS.md in its directory AND reference a
// DETAILS.md (a Markdown link or a backtick path). This makes the "should this
// area have a DETAILS.md?" decision a one-time yes, so it never recurs per area:
// the pull tier always exists, and the push-tier doc acknowledges it. The
// repo-root CLAUDE.md is exempt (it's the @-import manifest, not an area doc, and
// has no area DETAILS.md). An error, not a warning: the pair is structural, like
// docs-reachable.
func RunClaudeMdDetailsSibling(ctx *CheckContext) (CheckResult, error) {
	claudeFiles, err := findClaudeMdFiles(ctx.RootDir)
	if err != nil {
		return CheckResult{}, fmt.Errorf("failed to find CLAUDE.md files: %w", err)
	}

	var violations []detailsSiblingViolation
	checked := 0
	for _, rel := range claudeFiles {
		// The repo-root CLAUDE.md is the @-import manifest, not an area doc.
		if filepath.Dir(rel) == "." {
			continue
		}
		checked++
		dir := filepath.Dir(rel)
		siblingDetails := filepath.Join(dir, "DETAILS.md")
		if !fileExists(filepath.Join(ctx.RootDir, siblingDetails)) {
			violations = append(violations, detailsSiblingViolation{
				claudeMd: rel,
				reason:   "no sibling DETAILS.md in its directory",
			})
			continue
		}
		referencesDetails, readErr := claudeMdReferencesDetails(filepath.Join(ctx.RootDir, rel))
		if readErr != nil {
			return CheckResult{}, fmt.Errorf("failed to read %s: %w", rel, readErr)
		}
		if !referencesDetails {
			violations = append(violations, detailsSiblingViolation{
				claudeMd: rel,
				reason:   "doesn't reference a DETAILS.md",
			})
		}
	}

	if len(violations) == 0 {
		return Success(fmt.Sprintf("%d non-root CLAUDE.md %s paired with a linked DETAILS.md",
			checked, Pluralize(checked, "file", "files"))), nil
	}

	return CheckResult{}, fmt.Errorf("%s", formatDetailsSiblingViolations(violations))
}

// claudeMdReferencesDetails reports whether the CLAUDE.md at path references a
// DETAILS.md (Markdown link or backtick path).
func claudeMdReferencesDetails(path string) (bool, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return false, err
	}
	return detailsRefRe.Match(data), nil
}

// formatDetailsSiblingViolations builds the failure body listing each CLAUDE.md
// missing its DETAILS.md pair, with the fix.
func formatDetailsSiblingViolations(violations []detailsSiblingViolation) string {
	sort.Slice(violations, func(i, j int) bool { return violations[i].claudeMd < violations[j].claudeMd })

	var sb strings.Builder
	for _, v := range violations {
		sb.WriteString(fmt.Sprintf("  - %s: %s\n", v.claudeMd, v.reason))
	}
	return fmt.Sprintf(
		"%d non-root CLAUDE.md %s without a referenced sibling DETAILS.md:\n%s"+
			"Every area CLAUDE.md needs a colocated DETAILS.md (the pull tier) and a reference to it. "+
			"Create the DETAILS.md and point at it, for example `See [DETAILS.md](DETAILS.md).`",
		len(violations), Pluralize(len(violations), "file", "files"),
		strings.TrimRight(sb.String(), "\n")+"\n")
}
