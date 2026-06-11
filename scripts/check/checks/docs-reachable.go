package checks

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
)

// docsReachableAllowlist is the on-disk shape of docs-reachable-allowlist.json.
// `Files` maps a repo-relative doc path to the reason it's intentionally NOT
// reachable from AGENTS.md. The goal is an empty allowlist: every doc connected.
type docsReachableAllowlist struct {
	Comment string            `json:"$comment,omitempty"`
	Files   map[string]string `json:"files"`
}

func docsReachableAllowlistPath(rootDir string) string {
	return filepath.Join(rootDir, "scripts", "check", "checks", "docs-reachable-allowlist.json")
}

// loadDocsReachableAllowlist reads the allowlist JSON. A missing or unparsable
// file yields an empty allowlist (every orphan gets reported).
func loadDocsReachableAllowlist(rootDir string) docsReachableAllowlist {
	list := docsReachableAllowlist{Files: map[string]string{}}
	data, err := os.ReadFile(docsReachableAllowlistPath(rootDir))
	if err != nil {
		return list
	}
	if err := json.Unmarshal(data, &list); err != nil {
		return docsReachableAllowlist{Files: map[string]string{}}
	}
	if list.Files == nil {
		list.Files = map[string]string{}
	}
	return list
}

// shrinkwrapDocsReachableAllowlist drops allowlist entries that no longer earn
// their place: the doc is gone, or it's now reachable (so it needs no exemption).
// Mutates list in place and returns one human-readable line per change.
func shrinkwrapDocsReachableAllowlist(rootDir string, list *docsReachableAllowlist, orphanSet map[string]bool) []string {
	var changes []string
	for _, docPath := range sortedKeys(list.Files) {
		switch {
		case !fileExists(filepath.Join(rootDir, filepath.FromSlash(docPath))):
			delete(list.Files, docPath)
			changes = append(changes, fmt.Sprintf("removed %s (file no longer exists)", docPath))
		case !orphanSet[docPath]:
			delete(list.Files, docPath)
			changes = append(changes, fmt.Sprintf("removed %s (now reachable from AGENTS.md)", docPath))
		}
	}
	return changes
}

// RunDocsReachable fails when any enforced doc (CLAUDE.md, DETAILS.md, or a
// docs/ file outside the ephemeral scratch dirs) can't be reached from AGENTS.md
// by walking references between docs. A CLAUDE.md counts as reached when a
// reachable doc mentions its directory; everything else must be named. Allowlist
// entries (intentionally-unreachable docs) are suppressed; stale ones shrink-wrap
// away outside CI. Unlike the length scanners this is an error, not a warning:
// the doc tree must stay connected.
func RunDocsReachable(ctx *CheckContext) (CheckResult, error) {
	g, err := BuildDocGraph(ctx.RootDir)
	if err != nil {
		return CheckResult{}, fmt.Errorf("failed to build doc graph: %w", err)
	}

	orphanSet := make(map[string]bool, len(g.Orphans))
	for _, o := range g.Orphans {
		orphanSet[o] = true
	}

	allowlist := loadDocsReachableAllowlist(ctx.RootDir)
	staleChanges := shrinkwrapDocsReachableAllowlist(ctx.RootDir, &allowlist, orphanSet)
	if len(staleChanges) > 0 && !ctx.CI {
		if err := writeJSONAllowlist(docsReachableAllowlistPath(ctx.RootDir), allowlist); err != nil {
			return CheckResult{}, err
		}
		reformatWithOxfmt(ctx.RootDir, "scripts/check/checks/docs-reachable-allowlist.json")
	}

	var reported []string
	for _, o := range g.Orphans {
		if _, ok := allowlist.Files[o]; !ok {
			reported = append(reported, o)
		}
	}

	staleMsg := formatDocsStaleMsg(ctx.CI, staleChanges)
	if len(reported) == 0 {
		okMsg := fmt.Sprintf("All docs reachable from AGENTS.md (%d in graph)", len(g.Reached))
		if len(allowlist.Files) > 0 {
			okMsg = fmt.Sprintf("%s (%d allowlisted)", okMsg, len(allowlist.Files))
		}
		if staleMsg != "" {
			if ctx.CI {
				return CheckResult{Code: ResultWarning, Message: okMsg + "; " + staleMsg, Total: -1, Issues: -1, Changes: -1}, nil
			}
			return SuccessWithChanges(okMsg + "; " + staleMsg), nil
		}
		return Success(okMsg), nil
	}

	body := formatOrphans(reported, len(allowlist.Files))
	if staleMsg != "" {
		body += "\n" + staleMsg
	}
	return CheckResult{}, fmt.Errorf("%s", body)
}

// formatDocsStaleMsg renders the shrink-wrap note (or the CI-mode equivalent).
func formatDocsStaleMsg(ci bool, staleChanges []string) string {
	if len(staleChanges) == 0 {
		return ""
	}
	verb := "Shrink-wrapped allowlist"
	if ci {
		verb = "Stale allowlist entries (a local run shrink-wraps them)"
	}
	return fmt.Sprintf("%s:\n  - %s", verb, strings.Join(staleChanges, "\n  - "))
}

// formatOrphans builds the failure body listing the unreachable docs.
func formatOrphans(orphans []string, allowlisted int) string {
	suffix := ""
	if allowlisted > 0 {
		suffix = fmt.Sprintf(" (%d allowlisted)", allowlisted)
	}
	var sb strings.Builder
	for _, o := range orphans {
		sb.WriteString("  - ")
		sb.WriteString(o)
		sb.WriteString("\n")
	}
	return fmt.Sprintf(
		"%d %s unreachable from AGENTS.md%s. Link each from a doc that's already reachable (a CLAUDE.md also counts as reached when a reachable doc mentions its directory):\n%s",
		len(orphans), Pluralize(len(orphans), "doc", "docs"), suffix,
		strings.TrimRight(sb.String(), "\n"))
}
