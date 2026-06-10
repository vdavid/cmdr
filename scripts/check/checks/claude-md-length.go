package checks

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
)

const (
	// claudeMdWarnWords is the push-tier budget. CLAUDE.md is auto-injected into
	// every agent session that touches its directory, so each word costs tokens
	// repeatedly. The pull-tier DETAILS.md (read on demand) is deliberately
	// unlimited and is NOT checked. See AGENTS.md § File structure.
	claudeMdWarnWords = 600

	// Tolerate this much growth above each allowlisted file's recorded word count
	// before warning, so small incremental edits don't churn the allowlist.
	claudeMdAllowlistBufferPct = 10
)

type longClaudeMd struct {
	relPath string
	words   int
}

// claudeMdLengthAllowlist is the on-disk shape of claude-md-length-allowlist.json.
// `Files` maps relative paths to accepted word counts (the contract a CLAUDE.md
// may not silently grow past).
type claudeMdLengthAllowlist struct {
	Comment string         `json:"$comment,omitempty"`
	Files   map[string]int `json:"files"`
}

// claudeMdLengthAllowlistPath returns the allowlist location (next to the check
// source files).
func claudeMdLengthAllowlistPath(rootDir string) string {
	return filepath.Join(rootDir, "scripts", "check", "checks", "claude-md-length-allowlist.json")
}

// loadClaudeMdLengthAllowlist reads the allowlist JSON. A missing or unparsable
// file yields an empty allowlist (all long CLAUDE.md files get reported).
func loadClaudeMdLengthAllowlist(rootDir string) claudeMdLengthAllowlist {
	var list claudeMdLengthAllowlist
	data, err := os.ReadFile(claudeMdLengthAllowlistPath(rootDir))
	if err != nil {
		return list
	}
	if err := json.Unmarshal(data, &list); err != nil {
		return claudeMdLengthAllowlist{}
	}
	return list
}

// countWords returns the whitespace-separated word count of a file. Matches
// `wc -w` so seeded counts and the check agree.
func countWords(path string) (int, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return 0, err
	}
	return len(strings.Fields(string(data))), nil
}

// shrinkwrapClaudeMdLengthAllowlist computes the stale-entry verdicts: dead
// entries (file gone), satisfied entries (file under the warn threshold), and
// slack entries (file more than the growth buffer below its allowed count,
// ratcheted down to the current count). It mutates list in place and returns one
// human-readable line per change.
func shrinkwrapClaudeMdLengthAllowlist(rootDir string, list *claudeMdLengthAllowlist) []string {
	var changes []string
	for _, path := range sortedKeys(list.Files) {
		allowed := list.Files[path]
		wordCount, err := countWords(filepath.Join(rootDir, path))
		switch {
		case err != nil:
			delete(list.Files, path)
			changes = append(changes, fmt.Sprintf("removed %s (file no longer exists)", path))
		case wordCount <= claudeMdWarnWords:
			delete(list.Files, path)
			changes = append(changes, fmt.Sprintf("removed %s (now %d words, under the %d threshold)", path, wordCount, claudeMdWarnWords))
		case wordCount <= allowed*(100-claudeMdAllowlistBufferPct)/100:
			list.Files[path] = wordCount
			changes = append(changes, fmt.Sprintf("ratcheted %s: %d → %d words", path, allowed, wordCount))
		}
	}
	return changes
}

type claudeMdLengthScanResult struct {
	longFiles        []longClaudeMd
	allowlistedCount int
}

// scanClaudeMdLengths finds every CLAUDE.md over the word threshold (excluding
// allowlisted ones still within their buffer). DETAILS.md files are intentionally
// not scanned: the pull tier is unlimited.
func scanClaudeMdLengths(rootDir string, allowlist claudeMdLengthAllowlist) (claudeMdLengthScanResult, error) {
	var result claudeMdLengthScanResult

	claudeFiles, err := findClaudeMdFiles(rootDir)
	if err != nil {
		return result, err
	}

	for _, relPath := range claudeFiles {
		wordCount, err := countWords(filepath.Join(rootDir, relPath))
		if err != nil || wordCount <= claudeMdWarnWords {
			continue
		}
		if allowedWords, ok := allowlist.Files[relPath]; ok && wordCount <= allowedWords*(100+claudeMdAllowlistBufferPct)/100 {
			result.allowlistedCount++
			continue
		}
		result.longFiles = append(result.longFiles, longClaudeMd{relPath: relPath, words: wordCount})
	}
	return result, nil
}

// formatLongClaudeMd builds the warning message listing oversized CLAUDE.md files.
func formatLongClaudeMd(files []longClaudeMd, allowlist claudeMdLengthAllowlist, allowlistedCount int) string {
	sort.Slice(files, func(i, j int) bool { return files[i].relPath < files[j].relPath })

	var sb strings.Builder
	for _, f := range files {
		detail := fmt.Sprintf("(%d words)", f.words)
		if allowedWords, ok := allowlist.Files[f.relPath]; ok {
			growthPct := (f.words - allowedWords) * 100 / allowedWords
			detail = fmt.Sprintf("(%d words, allowlist: %d, +%d%% growth)", f.words, allowedWords, growthPct)
		}
		sb.WriteString(fmt.Sprintf("  - %s %s%s%s\n", f.relPath, ansiYellow, detail, ansiReset))
	}

	suffix := ""
	if allowlistedCount > 0 {
		suffix = fmt.Sprintf(" (%d allowlisted)", allowlistedCount)
	}
	return fmt.Sprintf("%d new CLAUDE.md %s over %d words%s (move depth into DETAILS.md):\n%s",
		len(files), Pluralize(len(files), "file", "files"),
		claudeMdWarnWords, suffix, strings.TrimRight(sb.String(), "\n"))
}

// RunClaudeMdLength scans every CLAUDE.md (the push tier) for word counts over
// the threshold, keeping the auto-injected docs lean. DETAILS.md (the pull tier)
// is deliberately unlimited and not scanned. Allowlisted files are suppressed up
// to their recorded count plus a 10% buffer. Stale allowlist entries are
// shrink-wrapped: outside CI the check removes dead/satisfied entries and ratchets
// slack ones down; in CI it only reports them. Always succeeds (warn-only).
func RunClaudeMdLength(ctx *CheckContext) (CheckResult, error) {
	allowlist := loadClaudeMdLengthAllowlist(ctx.RootDir)

	staleChanges := shrinkwrapClaudeMdLengthAllowlist(ctx.RootDir, &allowlist)
	madeChanges := false
	if len(staleChanges) > 0 && !ctx.CI {
		if err := writeJSONAllowlist(claudeMdLengthAllowlistPath(ctx.RootDir), allowlist); err != nil {
			return CheckResult{}, err
		}
		reformatWithOxfmt(ctx.RootDir, "scripts/check/checks/claude-md-length-allowlist.json")
		madeChanges = true
	}

	result, err := scanClaudeMdLengths(ctx.RootDir, allowlist)
	if err != nil {
		return CheckResult{}, fmt.Errorf("failed to scan CLAUDE.md files: %w", err)
	}

	var staleMsg string
	if len(staleChanges) > 0 {
		verb := "Shrink-wrapped allowlist"
		if ctx.CI {
			verb = "Stale allowlist entries (a local run shrink-wraps them)"
		}
		staleMsg = fmt.Sprintf("%s:\n  - %s", verb, strings.Join(staleChanges, "\n  - "))
	}

	if len(result.longFiles) == 0 {
		okMsg := "All CLAUDE.md files under threshold"
		if result.allowlistedCount > 0 {
			okMsg = fmt.Sprintf("No new long CLAUDE.md files (%d allowlisted)", result.allowlistedCount)
		}
		if staleMsg != "" {
			if ctx.CI {
				return CheckResult{Code: ResultWarning, Message: okMsg + "; " + staleMsg, Total: -1, Issues: -1, Changes: -1}, nil
			}
			return SuccessWithChanges(okMsg + "; " + staleMsg), nil
		}
		return Success(okMsg), nil
	}

	msg := formatLongClaudeMd(result.longFiles, allowlist, result.allowlistedCount)
	if staleMsg != "" {
		msg += "\n" + staleMsg
	}
	return CheckResult{Code: ResultWarning, Message: msg, MadeChanges: madeChanges, Total: -1, Issues: -1, Changes: -1}, nil
}
