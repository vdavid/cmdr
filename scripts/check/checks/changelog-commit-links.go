package checks

import (
	"bufio"
	"bytes"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
)

// changelogCommitURLPattern matches valid commit URLs with a 6–40 char hex SHA.
// Group 1 captures the SHA.
var changelogCommitURLPattern = regexp.MustCompile(`https://github\.com/vdavid/cmdr/commit/([0-9a-f]{6,40})`)

// changelogAnyCommitURLPattern matches any commit URL with a hex-looking SHA,
// so we can also surface URLs whose SHA is shorter than 6 or longer than 40.
var changelogAnyCommitURLPattern = regexp.MustCompile(`https://github\.com/vdavid/cmdr/commit/([0-9a-f]+)`)

// changelogPairedLinkPattern matches `[hash](https://github.com/vdavid/cmdr/commit/hash2)`.
// Group 1: the bracketed text (expected to be a hex SHA prefix-compatible with group 2).
// Group 2: the SHA in the URL.
var changelogPairedLinkPattern = regexp.MustCompile(`\[([0-9a-fA-F]+)\]\(https://github\.com/vdavid/cmdr/commit/([0-9a-f]+)\)`)

// changelogCommitLinkFinding records a problem with a specific line.
type changelogCommitLinkFinding struct {
	line    int
	message string
}

// changelogScanResult holds what the line-by-line scan collected.
type changelogScanResult struct {
	findings   []changelogCommitLinkFinding
	uniqueSHAs map[string]int // sha -> first line seen
	totalLinks int            // count of all (non-unique) commit URLs
}

// RunChangelogCommitLinks validates that every GitHub commit URL referenced in
// CHANGELOG.md resolves to a real commit in the repo, and that any `[sha](url)`
// pair has matching SHAs. If CHANGELOG.md is missing, the check succeeds with
// 0 SHAs validated — no CHANGELOG means no risk of bad links.
func RunChangelogCommitLinks(ctx *CheckContext) (CheckResult, error) {
	path := filepath.Join(ctx.RootDir, "CHANGELOG.md")
	file, err := os.Open(path)
	if err != nil {
		if os.IsNotExist(err) {
			return Success("No CHANGELOG.md, nothing to validate"), nil
		}
		return CheckResult{}, fmt.Errorf("failed to open CHANGELOG.md: %w", err)
	}
	defer file.Close()

	scan, err := scanChangelogForCommitLinks(file)
	if err != nil {
		return CheckResult{}, err
	}

	// Resolve each unique URL SHA against the repo.
	shas := make([]string, 0, len(scan.uniqueSHAs))
	for sha := range scan.uniqueSHAs {
		shas = append(shas, sha)
	}
	sort.Strings(shas)

	badSHAs, err := resolveShasWithBatch(ctx.RootDir, shas)
	if err != nil {
		return CheckResult{}, err
	}
	for _, sha := range badSHAs {
		scan.findings = append(scan.findings, changelogCommitLinkFinding{
			line:    scan.uniqueSHAs[sha],
			message: fmt.Sprintf("SHA does not resolve in this repo: %s", sha),
		})
	}

	if len(scan.findings) > 0 {
		return CheckResult{}, formatFindingsError(scan.findings)
	}

	count := len(shas)
	if count == 0 {
		return Success("No commit links to validate"), nil
	}
	result := Success(fmt.Sprintf("%d unique %s resolved (%d %s)",
		count, Pluralize(count, "SHA", "SHAs"),
		scan.totalLinks, Pluralize(scan.totalLinks, "link", "links")))
	result.Total = count
	return result, nil
}

// scanChangelogForCommitLinks walks the file line by line, collecting unique
// URL SHAs and flagging per-line structural issues (short/long SHAs, text/URL
// mismatches).
func scanChangelogForCommitLinks(r io.Reader) (changelogScanResult, error) {
	var result changelogScanResult
	result.uniqueSHAs = make(map[string]int)

	scanner := bufio.NewScanner(r)
	scanner.Buffer(make([]byte, 1024*1024), 1024*1024)
	lineNum := 0
	for scanner.Scan() {
		lineNum++
		line := scanner.Text()
		result.findings = append(result.findings, scanLineForLinkIssues(line, lineNum)...)
		for _, m := range changelogCommitURLPattern.FindAllStringSubmatch(line, -1) {
			sha := strings.ToLower(m[1])
			result.totalLinks++
			if _, exists := result.uniqueSHAs[sha]; !exists {
				result.uniqueSHAs[sha] = lineNum
			}
		}
	}
	if err := scanner.Err(); err != nil {
		return result, fmt.Errorf("failed to read CHANGELOG.md: %w", err)
	}
	return result, nil
}

// scanLineForLinkIssues checks a single line for text/URL mismatches and SHA
// length violations. It does NOT resolve SHAs against the repo.
func scanLineForLinkIssues(line string, lineNum int) []changelogCommitLinkFinding {
	var findings []changelogCommitLinkFinding

	// Paired `[hash](url)` — flag mismatches where neither is a prefix of the other.
	for _, m := range changelogPairedLinkPattern.FindAllStringSubmatch(line, -1) {
		textSHA := strings.ToLower(m[1])
		urlSHA := strings.ToLower(m[2])
		if textSHA != urlSHA && !strings.HasPrefix(textSHA, urlSHA) && !strings.HasPrefix(urlSHA, textSHA) {
			findings = append(findings, changelogCommitLinkFinding{
				line:    lineNum,
				message: fmt.Sprintf("text/URL SHA mismatch: [%s] vs /commit/%s", m[1], m[2]),
			})
		}
	}

	// Any URL — flag SHAs below 6 or above 40 chars.
	for _, m := range changelogAnyCommitURLPattern.FindAllStringSubmatch(line, -1) {
		sha := strings.ToLower(m[1])
		if len(sha) < 6 {
			findings = append(findings, changelogCommitLinkFinding{
				line:    lineNum,
				message: fmt.Sprintf("SHA too short (%d chars, need ≥6): %s", len(sha), sha),
			})
		} else if len(sha) > 40 {
			findings = append(findings, changelogCommitLinkFinding{
				line:    lineNum,
				message: fmt.Sprintf("SHA too long (%d chars, max 40): %s", len(sha), sha),
			})
		}
	}
	return findings
}

// formatFindingsError builds the aggregated error message listing every finding,
// sorted by line number then alphabetically for deterministic output.
func formatFindingsError(findings []changelogCommitLinkFinding) error {
	sort.Slice(findings, func(i, j int) bool {
		if findings[i].line != findings[j].line {
			return findings[i].line < findings[j].line
		}
		return findings[i].message < findings[j].message
	})
	var sb strings.Builder
	for _, f := range findings {
		sb.WriteString(fmt.Sprintf("  CHANGELOG.md:%d %s\n", f.line, f.message))
	}
	return fmt.Errorf("found %d %s in CHANGELOG.md commit links:\n%s",
		len(findings), Pluralize(len(findings), "issue", "issues"),
		strings.TrimRight(sb.String(), "\n"))
}

// resolveShasWithBatch pipes all SHAs through a single `git cat-file --batch-check`
// process and returns the subset that didn't resolve. A SHA is considered bad if
// git reports "missing", "ambiguous", or any non-"commit" type — we want the URL
// to point at a real commit, not a tree/blob/tag (none of which should appear in
// CHANGELOG.md anyway). Returns an error only on I/O failure; unresolved SHAs are
// data, not errors.
func resolveShasWithBatch(rootDir string, shas []string) ([]string, error) {
	if len(shas) == 0 {
		return nil, nil
	}

	cmd := exec.Command("git", "cat-file", "--batch-check=%(objectname) %(objecttype)")
	cmd.Dir = rootDir
	var stderr bytes.Buffer
	cmd.Stderr = &stderr

	stdin, err := cmd.StdinPipe()
	if err != nil {
		return nil, fmt.Errorf("failed to open stdin for git cat-file: %w", err)
	}
	stdout, err := cmd.StdoutPipe()
	if err != nil {
		return nil, fmt.Errorf("failed to open stdout for git cat-file: %w", err)
	}

	if err := cmd.Start(); err != nil {
		return nil, fmt.Errorf("failed to start git cat-file: %w", err)
	}

	writeErr := feedShasAsync(stdin, shas)
	bad, readErr := collectBatchResults(stdout, shas)
	if readErr != nil {
		_ = cmd.Process.Kill()
		return nil, readErr
	}

	if err := cmd.Wait(); err != nil {
		// Non-zero exit can happen when some SHAs are missing — that's expected
		// and already captured above. Only fail if the stdin writer itself broke,
		// or if we got no resolutions at all and git wrote to stderr.
		if w := *writeErr; w != nil {
			return nil, fmt.Errorf("failed to write SHAs to git cat-file: %w", w)
		}
		if len(bad) == len(shas) && stderr.Len() > 0 {
			return nil, fmt.Errorf("git cat-file failed: %s", strings.TrimSpace(stderr.String()))
		}
	}
	return bad, nil
}

// feedShasAsync writes all SHAs to stdin in a goroutine and returns a pointer to
// the write error for the caller to check after cmd.Wait(). stdin is always closed.
func feedShasAsync(stdin io.WriteCloser, shas []string) *error {
	var writeErr error
	go func() {
		w := bufio.NewWriter(stdin)
		for _, sha := range shas {
			if _, err := fmt.Fprintln(w, sha); err != nil {
				writeErr = err
				break
			}
		}
		if err := w.Flush(); err != nil && writeErr == nil {
			writeErr = err
		}
		_ = stdin.Close()
	}()
	return &writeErr
}

// collectBatchResults reads exactly len(shas) lines from stdout (git emits one
// line per input SHA) and returns the SHAs whose second field isn't "commit".
// git output format per the --batch-check format string:
//
//	"<fullsha> <type>"       (resolved)
//	"<sha> missing"          (not found)
//	"<sha> ambiguous"        (multiple object matches)
func collectBatchResults(stdout io.Reader, shas []string) ([]string, error) {
	var bad []string
	reader := bufio.NewReader(stdout)
	for i, sha := range shas {
		line, err := reader.ReadString('\n')
		if err != nil && err != io.EOF {
			return nil, fmt.Errorf("failed to read git cat-file output at SHA %d (%s): %w", i, sha, err)
		}
		line = strings.TrimRight(line, "\n")
		fields := strings.Fields(line)
		if len(fields) < 2 || fields[1] != "commit" {
			bad = append(bad, sha)
		}
	}
	return bad, nil
}
