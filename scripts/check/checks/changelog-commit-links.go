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

// changelogTrailingRefsPattern matches a bullet entry's trailing `(hash, hash, …)`
// group, the only place the changelog carries commit references. Group 1 captures
// the comma-separated hashes. It runs against a logical entry (wrapped source lines
// already joined), so `$` really is the end of the entry.
//
// Anchoring to the end is what keeps ordinary prose safe: plenty of entries close on
// a parenthetical like `(~40x speed-up!)` or `(smb2 0.8.0)`, and a hex-looking word
// mid-sentence is never even considered.
//
// Recognition stays deliberately wider than the changelogRefLength rule the check
// enforces. Narrowing it to {8} would make a stray 7-character ref stop being a ref:
// it'd be read as prose, silently skip SHA validation, and quietly fail to render in
// anything matching the convention. Recognize loosely, then fail loudly on the length.
var changelogTrailingRefsPattern = regexp.MustCompile(`\(([0-9a-f]{6,40}(?:,\s*[0-9a-f]{6,40})*)\)$`)

// changelogRefLength is the exact length every commit ref must have. `release.md`
// produces it with `git log --abbrev=8`, and the whole file is normalized to it.
const changelogRefLength = 8

// changelogCommitURLPattern matches the deprecated `…/commit/<sha>` URL form in any
// shape (bare, or wrapped in a markdown link). The changelog stores bare hashes and
// lets the renderers linkify them, so any URL here is a regression to catch.
var changelogCommitURLPattern = regexp.MustCompile(`https://github\.com/vdavid/cmdr/commit/`)

// changelogBulletMarkers are the list markers that open a logical entry at column zero.
var changelogBulletMarkers = []string{"- ", "* ", "+ "}

// changelogCommitLinkFinding records a problem with a specific line.
type changelogCommitLinkFinding struct {
	line    int
	message string
}

// changelogScanResult holds what the scan collected.
type changelogScanResult struct {
	findings   []changelogCommitLinkFinding
	uniqueSHAs map[string]int // sha -> first line seen
	totalRefs  int            // count of all (non-unique) commit refs
}

// RunChangelogCommitLinks validates that every commit hash referenced in
// CHANGELOG.md is exactly changelogRefLength characters and resolves to a real
// commit reachable from HEAD, and that nobody has reintroduced the deprecated
// `[sha](url)` link form. If CHANGELOG.md is missing, the check succeeds with 0 SHAs
// validated; no CHANGELOG means no risk of bad refs.
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

	resolved, badSHAs, err := resolveShasWithBatch(ctx.RootDir, shas)
	if err != nil {
		return CheckResult{}, err
	}
	for _, sha := range badSHAs {
		scan.findings = append(scan.findings, changelogCommitLinkFinding{
			line:    scan.uniqueSHAs[sha],
			message: fmt.Sprintf("SHA does not resolve in this repo: %s", sha),
		})
	}

	// Reachability: existence in the object DB isn't enough. An abbreviated
	// SHA of a rebased-away commit still resolves locally via reflog, but CI
	// does a clean clone with no reflog and fails. Require every SHA to be
	// reachable from HEAD so both environments agree.
	if len(resolved) > 0 {
		reachable, err := collectReachableFromHEAD(ctx.RootDir)
		if err != nil {
			return CheckResult{}, err
		}
		for inputSHA, fullSHA := range resolved {
			if _, ok := reachable[fullSHA]; !ok {
				scan.findings = append(scan.findings, changelogCommitLinkFinding{
					line:    scan.uniqueSHAs[inputSHA],
					message: fmt.Sprintf("SHA resolves but is not reachable from HEAD (likely rebased away): %s", inputSHA),
				})
			}
		}
	}

	if len(scan.findings) > 0 {
		return CheckResult{}, formatFindingsError(scan.findings)
	}

	count := len(shas)
	if count == 0 {
		return Success("No commit refs to validate"), nil
	}
	result := Success(fmt.Sprintf("%d unique %s resolved (%d %s)",
		count, Pluralize(count, "SHA", "SHAs"),
		scan.totalRefs, Pluralize(scan.totalRefs, "reference", "references")))
	result.Total = count
	return result, nil
}

// changelogSourceLine is one physical line of the file, kept with its 1-based
// number so a finding can cite where a hash actually sits.
type changelogSourceLine struct {
	num  int
	text string
}

// scanChangelogForCommitLinks reads the file, rebuilds each bullet entry from its
// wrapped source lines, and collects the hashes in each entry's trailing ref group.
// It also flags any leftover commit URL, the deprecated form.
func scanChangelogForCommitLinks(r io.Reader) (changelogScanResult, error) {
	var result changelogScanResult
	result.uniqueSHAs = make(map[string]int)

	scanner := bufio.NewScanner(r)
	scanner.Buffer(make([]byte, 1024*1024), 1024*1024)
	lineNum := 0
	var entry []changelogSourceLine
	for scanner.Scan() {
		lineNum++
		line := scanner.Text()

		if changelogCommitURLPattern.MatchString(line) {
			result.findings = append(result.findings, changelogCommitLinkFinding{
				line:    lineNum,
				message: "commit URL found; write the bare hash instead and let the renderers link it",
			})
		}

		switch {
		case startsChangelogEntry(line):
			result.collectEntryRefs(entry)
			entry = []changelogSourceLine{{num: lineNum, text: line}}
		case len(entry) > 0 && isChangelogContinuation(line):
			entry = append(entry, changelogSourceLine{num: lineNum, text: line})
		default:
			result.collectEntryRefs(entry)
			entry = nil
		}
	}
	result.collectEntryRefs(entry)

	if err := scanner.Err(); err != nil {
		return result, fmt.Errorf("failed to read CHANGELOG.md: %w", err)
	}
	return result, nil
}

// startsChangelogEntry reports whether the line opens a bullet entry: a list marker
// at column zero. An indented marker is a nested bullet, part of the entry above it.
func startsChangelogEntry(line string) bool {
	for _, marker := range changelogBulletMarkers {
		if strings.HasPrefix(line, marker) {
			return true
		}
	}
	return false
}

// isChangelogContinuation reports whether the line is a wrapped continuation of the
// entry above it: indented and non-blank.
func isChangelogContinuation(line string) bool {
	return strings.TrimSpace(line) != "" && (strings.HasPrefix(line, " ") || strings.HasPrefix(line, "\t"))
}

// collectEntryRefs joins an entry's source lines, pulls the hashes out of its
// trailing ref group (if it has one), and records each with the line it sits on.
func (result *changelogScanResult) collectEntryRefs(entry []changelogSourceLine) {
	if len(entry) == 0 {
		return
	}
	parts := make([]string, 0, len(entry))
	for _, line := range entry {
		parts = append(parts, strings.TrimSpace(line.text))
	}
	joined := strings.TrimSpace(strings.Join(parts, " "))

	match := changelogTrailingRefsPattern.FindStringSubmatch(joined)
	if match == nil {
		return
	}
	for _, sha := range strings.Split(match[1], ",") {
		sha = strings.TrimSpace(sha)
		result.totalRefs++
		line := lineOfSHA(entry, sha)
		if len(sha) != changelogRefLength {
			// Flagged per occurrence, not per unique ref: a wrong-length hash usually
			// repeats, and one finding per site means one pass fixes them all.
			result.findings = append(result.findings, changelogCommitLinkFinding{
				line: line,
				message: fmt.Sprintf("commit ref must be exactly %d characters, this one is %d: %s",
					changelogRefLength, len(sha), sha),
			})
		}
		if _, exists := result.uniqueSHAs[sha]; !exists {
			result.uniqueSHAs[sha] = line
		}
	}
}

// lineOfSHA returns the number of the line a hash sits on. The ref group trails the
// entry, so the search runs backwards; it falls back to the entry's first line if
// the hash somehow isn't found verbatim.
func lineOfSHA(entry []changelogSourceLine, sha string) int {
	for i := len(entry) - 1; i >= 0; i-- {
		if strings.Contains(entry[i].text, sha) {
			return entry[i].num
		}
	}
	return entry[0].num
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
	return fmt.Errorf("found %d %s in CHANGELOG.md commit refs:\n%s",
		len(findings), Pluralize(len(findings), "issue", "issues"),
		strings.TrimRight(sb.String(), "\n"))
}

// resolveShasWithBatch pipes all SHAs through a single `git cat-file --batch-check`
// process. Returns (resolved, bad, err): `resolved` maps each input SHA (abbreviated
// or full) to its full 40-char SHA when the object is a commit; `bad` lists the
// inputs that didn't resolve as a commit (missing, ambiguous, or wrong type: tree,
// blob, tag). Returns an error only on I/O failure; unresolved SHAs are data, not
// errors.
func resolveShasWithBatch(rootDir string, shas []string) (map[string]string, []string, error) {
	if len(shas) == 0 {
		return nil, nil, nil
	}

	cmd := exec.Command("git", "cat-file", "--batch-check=%(objectname) %(objecttype)")
	cmd.Dir = rootDir
	var stderr bytes.Buffer
	cmd.Stderr = &stderr

	stdin, err := cmd.StdinPipe()
	if err != nil {
		return nil, nil, fmt.Errorf("failed to open stdin for git cat-file: %w", err)
	}
	stdout, err := cmd.StdoutPipe()
	if err != nil {
		return nil, nil, fmt.Errorf("failed to open stdout for git cat-file: %w", err)
	}

	if err := cmd.Start(); err != nil {
		return nil, nil, fmt.Errorf("failed to start git cat-file: %w", err)
	}

	writeErr := feedShasAsync(stdin, shas)
	resolved, bad, readErr := collectBatchResults(stdout, shas)
	if readErr != nil {
		_ = cmd.Process.Kill()
		return nil, nil, readErr
	}

	if err := cmd.Wait(); err != nil {
		// Non-zero exit can happen when some SHAs are missing, which is expected
		// and already captured above. Only fail if the stdin writer itself broke,
		// or if we got no resolutions at all and git wrote to stderr.
		if w := *writeErr; w != nil {
			return nil, nil, fmt.Errorf("failed to write SHAs to git cat-file: %w", w)
		}
		if len(bad) == len(shas) && stderr.Len() > 0 {
			return nil, nil, fmt.Errorf("git cat-file failed: %s", strings.TrimSpace(stderr.String()))
		}
	}
	return resolved, bad, nil
}

// collectReachableFromHEAD runs `git rev-list HEAD` and returns the set of all
// full-40-char commit SHAs reachable from HEAD. Used to catch SHAs that resolve
// in the local object DB (via reflog or dangling objects) but aren't merged
// into HEAD, which would fail in CI's fresh clone.
func collectReachableFromHEAD(rootDir string) (map[string]struct{}, error) {
	cmd := exec.Command("git", "rev-list", "HEAD")
	cmd.Dir = rootDir
	var stderr bytes.Buffer
	cmd.Stderr = &stderr
	out, err := cmd.Output()
	if err != nil {
		return nil, fmt.Errorf("git rev-list HEAD failed: %s: %w", strings.TrimSpace(stderr.String()), err)
	}
	set := make(map[string]struct{}, 8192)
	for _, line := range strings.Split(strings.TrimRight(string(out), "\n"), "\n") {
		if line != "" {
			set[line] = struct{}{}
		}
	}
	return set, nil
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
// line per input SHA) and returns (resolved, bad, err). `resolved` maps each
// input SHA to its full 40-char SHA when the object is a commit; `bad` lists
// inputs whose second field isn't "commit" (missing, ambiguous, or wrong type).
// git output format per the --batch-check format string:
//
//	"<fullsha> <type>"       (resolved)
//	"<sha> missing"          (not found)
//	"<sha> ambiguous"        (multiple object matches)
func collectBatchResults(stdout io.Reader, shas []string) (map[string]string, []string, error) {
	resolved := make(map[string]string, len(shas))
	var bad []string
	reader := bufio.NewReader(stdout)
	for i, sha := range shas {
		line, err := reader.ReadString('\n')
		if err != nil && err != io.EOF {
			return nil, nil, fmt.Errorf("failed to read git cat-file output at SHA %d (%s): %w", i, sha, err)
		}
		line = strings.TrimRight(line, "\n")
		fields := strings.Fields(line)
		if len(fields) < 2 || fields[1] != "commit" {
			bad = append(bad, sha)
			continue
		}
		resolved[sha] = fields[0]
	}
	return resolved, bad, nil
}
