package checks

import (
	"bufio"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"regexp"
	"strconv"
	"strings"
)

// changelogSecurityFirstVersion is the first release held to the form. Older
// sections predate the two-form rule in SECURITY.md § How fixed vulnerabilities are
// published and stay as they shipped; `[Unreleased]` is always checked.
var changelogSecurityFirstVersion = strictSemver{core: [3]int{0, 57, 0}}

// changelogSectionHeadingRE matches a release heading, `## [0.57.0] - 2026-10-28` or
// `## [Unreleased]`, capturing what's in the brackets.
var changelogSectionHeadingRE = regexp.MustCompile(`^## \[([^\]]+)\]`)

// changelogAdvisoryIDRE matches an upstream advisory ID: GitHub's, RustSec's, a
// CVE, or the Go vulnerability database's. A dependency's flaw names one of these.
var changelogAdvisoryIDRE = regexp.MustCompile(`\b(?:GHSA(?:-[0-9a-z]{4}){3}|RUSTSEC-\d{4}-\d{4}|CVE-\d{4}-\d{4,}|GO-\d{4}-\d{4,})\b`)

// changelogCVSSRE matches our own score, `CVSS v4.0 8.2 (high)`. A flaw in Cmdr's
// own code carries one.
var changelogCVSSRE = regexp.MustCompile(`\bCVSS v4\.0 (\d{1,2}\.\d) \(([a-z]+)\)`)

// changelogSeverityRE matches an upstream severity in parens. `unrated` is for an
// advisory that gives none (most RustSec ones), and only a dependency's line may use it.
var changelogSeverityRE = regexp.MustCompile(`\((?:low|medium|high|critical|unrated)\)`)

// changelogAffectedRE matches the affected versions, `Cmdr 0.52.0–0.56.0` or a single
// `Cmdr 0.56.0`. Further windows may follow without the prefix, `and 0.40.0–0.56.0`.
var changelogAffectedRE = regexp.MustCompile(`\bCmdr \d+\.\d+\.\d+`)

// changelogVersionRangeRE matches every en-dash version range in an entry, so each
// window's ends can be checked against each other and the release.
var changelogVersionRangeRE = regexp.MustCompile(`\b(\d+\.\d+\.\d+)–(\d+\.\d+\.\d+)\b`)

// changelogHyphenRangeRE catches a range written with a hyphen, which reads as a
// pre-release version (`0.52.0-0.56.0` is ONE semver) and slips past the range rule.
var changelogHyphenRangeRE = regexp.MustCompile(`\b\d+\.\d+\.\d+-\d+\.\d+\.\d+\b`)

// changelogSecurityEntry is one logical `### Security` bullet, wrapped lines joined
// and its trailing commit-ref group dropped.
type changelogSecurityEntry struct {
	line    int
	release string        // the bracketed heading text, like "0.57.0" or "Unreleased"
	version *strictSemver // nil for `[Unreleased]`
	text    string
}

// changelogSecurityFinding is a problem with one entry.
type changelogSecurityFinding struct {
	line    int
	message string
}

// RunChangelogSecurity fails on any `### Security` line, from 0.57.0 on, that doesn't
// carry the fields SECURITY.md promises: an upstream advisory ID with its severity (a
// dependency's flaw) or our own CVSS v4.0 score and rating (a flaw in Cmdr's code),
// plus the affected Cmdr versions.
func RunChangelogSecurity(ctx *CheckContext) (CheckResult, error) {
	file, err := os.Open(filepath.Join(ctx.RootDir, "CHANGELOG.md"))
	if err != nil {
		if os.IsNotExist(err) {
			return Success("No CHANGELOG.md, nothing to validate"), nil
		}
		return CheckResult{}, fmt.Errorf("failed to open CHANGELOG.md: %w", err)
	}
	defer file.Close()

	entries, err := collectChangelogSecurityEntries(file)
	if err != nil {
		return CheckResult{}, err
	}

	var findings []changelogSecurityFinding
	for _, entry := range entries {
		for _, problem := range changelogSecurityEntryProblems(entry) {
			findings = append(findings, changelogSecurityFinding{line: entry.line, message: problem})
		}
	}
	if len(findings) > 0 {
		var sb strings.Builder
		for _, f := range findings {
			sb.WriteString(fmt.Sprintf("CHANGELOG.md:%d %s\n", f.line, f.message))
		}
		return CheckResult{}, fmt.Errorf("%d %s in CHANGELOG.md Security lines. Each line is a fixed vulnerability, in "+
			"one of two forms (docs/guides/releasing.md § Writing the CHANGELOG):\n"+
			"  dependency: `...: `gix` GHSA-q54r-jf2p-rfr2 (high), Cmdr 0.52.0–0.56.0 (<hash>)`\n"+
			"  our own:    `...: CVSS v4.0 2.1 (low), Cmdr 0.50.0–0.56.0 (<hash>)`\n"+
			"Hardening that fixes no flaw goes under Changed.\n%s",
			len(findings), Pluralize(len(findings), "problem", "problems"),
			indentOutput(strings.TrimRight(sb.String(), "\n")))
	}

	if len(entries) == 0 {
		return Success("No Security lines from 0.57.0 on"), nil
	}
	releases := map[string]struct{}{}
	for _, entry := range entries {
		releases[entry.release] = struct{}{}
	}
	result := Success(fmt.Sprintf("%d Security %s in %d %s carry their advisory fields",
		len(entries), Pluralize(len(entries), "line", "lines"),
		len(releases), Pluralize(len(releases), "release", "releases")))
	result.Total = len(entries)
	return result, nil
}

// collectChangelogSecurityEntries walks the file and returns every bullet under a
// `### Security` heading of a release the rule covers.
func collectChangelogSecurityEntries(r io.Reader) ([]changelogSecurityEntry, error) {
	var entries []changelogSecurityEntry
	scanner := bufio.NewScanner(r)
	scanner.Buffer(make([]byte, 1024*1024), 1024*1024)

	var release string
	var version *strictSemver
	inScope, inSecurity := false, false
	var current *changelogSecurityEntry
	var parts []string
	flush := func() {
		if current != nil {
			current.text = strings.TrimSpace(changelogTrailingRefsPattern.ReplaceAllString(strings.Join(parts, " "), ""))
			entries = append(entries, *current)
		}
		current, parts = nil, nil
	}

	lineNum := 0
	for scanner.Scan() {
		lineNum++
		line := scanner.Text()

		if m := changelogSectionHeadingRE.FindStringSubmatch(line); m != nil {
			flush()
			release, version, inScope = m[1], nil, false
			if strings.EqualFold(release, "Unreleased") {
				inScope = true
			} else if v, ok := parseStrictSemver(release); ok {
				version = &v
				inScope = v.compare(changelogSecurityFirstVersion) >= 0
			}
			inSecurity = false
			continue
		}
		if strings.HasPrefix(line, "### ") {
			flush()
			inSecurity = strings.TrimSpace(strings.TrimPrefix(line, "### ")) == "Security"
			continue
		}
		if !inScope || !inSecurity {
			continue
		}

		switch {
		case startsChangelogEntry(line):
			flush()
			current = &changelogSecurityEntry{line: lineNum, release: release, version: version}
			parts = []string{strings.TrimSpace(line[2:])}
		case current != nil && isChangelogContinuation(line):
			parts = append(parts, strings.TrimSpace(line))
		default:
			flush()
		}
	}
	flush()

	if err := scanner.Err(); err != nil {
		return nil, fmt.Errorf("failed to read CHANGELOG.md: %w", err)
	}
	return entries, nil
}

// changelogSecurityEntryProblems lists what one entry is missing or gets wrong.
func changelogSecurityEntryProblems(entry changelogSecurityEntry) []string {
	var problems []string
	text := entry.text
	hasID := changelogAdvisoryIDRE.MatchString(text)
	cvss := changelogCVSSRE.FindStringSubmatch(text)

	switch {
	case cvss != nil:
		if problem := cvssRatingProblem(cvss[1], cvss[2]); problem != "" {
			problems = append(problems, problem)
		}
	case strings.Contains(text, "CVSS"):
		problems = append(problems, "write our own score as `CVSS v4.0 <score> (<rating>)`, like `CVSS v4.0 8.2 (high)`")
	case hasID:
		if !changelogSeverityRE.MatchString(text) {
			problems = append(problems, "add the upstream advisory's severity in parens after its ID: "+
				"(low), (medium), (high), (critical), or (unrated) when it gives none")
		}
	default:
		problems = append(problems, "name the upstream advisory ID (GHSA-…, RUSTSEC-…, CVE-…, GO-…) for a dependency's "+
			"flaw, or `CVSS v4.0 <score> (<rating>)` for a flaw in Cmdr's own code")
	}

	if changelogHyphenRangeRE.MatchString(text) {
		problems = append(problems, "write the version range with an en dash (–), not a hyphen")
	}
	if !changelogAffectedRE.MatchString(text) {
		problems = append(problems, "name the affected versions, like `Cmdr 0.52.0–0.56.0`")
	}
	for _, m := range changelogVersionRangeRE.FindAllStringSubmatch(text, -1) {
		first, firstOK := parseStrictSemver(m[1])
		last, lastOK := parseStrictSemver(m[2])
		if !firstOK || !lastOK {
			continue
		}
		if first.compare(last) > 0 {
			problems = append(problems, fmt.Sprintf("the range %s–%s runs backwards", m[1], m[2]))
		}
		if entry.version != nil && last.compare(*entry.version) >= 0 {
			problems = append(problems, fmt.Sprintf("the range %s–%s reaches %s, the release that fixes it",
				m[1], m[2], entry.release))
		}
	}
	return problems
}

// cvssRatingProblem checks a CVSS v4.0 score against the rating written beside it,
// using the specification's bands. Returns "" when they agree.
func cvssRatingProblem(rawScore, rating string) string {
	score, err := strconv.ParseFloat(rawScore, 64)
	if err != nil || score < 0 || score > 10 {
		return fmt.Sprintf("CVSS v4.0 score %s isn't between 0.0 and 10.0", rawScore)
	}
	want := cvssV4Rating(score)
	if rating != want {
		return fmt.Sprintf("CVSS v4.0 %s is rated (%s), not (%s)", rawScore, want, rating)
	}
	return ""
}

// cvssV4Rating maps a score to its qualitative rating per CVSS v4.0 § 5.
func cvssV4Rating(score float64) string {
	switch {
	case score >= 9.0:
		return "critical"
	case score >= 7.0:
		return "high"
	case score >= 4.0:
		return "medium"
	case score >= 0.1:
		return "low"
	}
	return "none"
}
