package main

import (
	"fmt"
	"regexp"
	"strings"
)

// A red job's log, cut down to why it failed, so a red land says what broke without a trip to
// the browser. A finished job's log is readable while the rest of the run still goes (unlike
// `gh run view --log-failed`, which refuses until the run ends), and that's when an early red
// stops the wait.

var (
	// GitHub prefixes every log line with its timestamp.
	logTimestamp = regexp.MustCompile(`^\d{4}-\d\d-\d\dT[\d:.]+Z ?`)
	// CSI and OSC escape sequences: the check runner's colors, cargo's, Playwright's.
	ansiEscape = regexp.MustCompile(`\x1b(\[[0-9;?]*[ -/]*[@-~]|\][^\x07\x1b]*(\x07|\x1b\\))`)
	// Where a failure's own block starts. Each block runs on through every line indented deeper
	// than its first (blank lines too), which is how all three print a failure's detail.
	failureStarts = []*regexp.Regexp{
		// The check runner: `• Desktop: 🦀 Rust / clippy... FAILED (12.3s)`, its output indented below.
		regexp.MustCompile(`\.\.\. FAILED \(`),
		// Playwright's numbered failures: `  1) [cmdr] › tests/x.spec.ts:12:5 › title ───`.
		regexp.MustCompile(`^\s*\d+\) .*›`),
		// Go's test failures, subtests nested deeper.
		regexp.MustCompile(`^\s*--- FAIL: `),
	}
	// GitHub's own error annotations: the fallback when no block matched, a step that died before
	// any of the three printed anything (a setup step, a killed runner).
	errorAnnotation = regexp.MustCompile(`^##\[error\]`)
)

// failureSummary picks the lines of a job's log that say why it failed: every failure block, in
// log order, else GitHub's error annotations. Picked by structure, never cut by length: a block is
// as long as the failure it reports.
func failureSummary(log string) []string {
	var lines []string
	for line := range strings.SplitSeq(strings.ReplaceAll(log, "\r\n", "\n"), "\n") {
		lines = append(lines, strings.TrimRight(ansiEscape.ReplaceAllString(logTimestamp.ReplaceAllString(line, ""), ""), " \t\r"))
	}
	var out []string
	for i := 0; i < len(lines); {
		if !startsFailure(lines[i]) {
			i++
			continue
		}
		end := blockEnd(lines, i)
		out = append(out, lines[i:end]...)
		i = end
	}
	if len(out) > 0 {
		return out
	}
	for _, line := range lines {
		if errorAnnotation.MatchString(line) {
			out = append(out, line)
		}
	}
	return out
}

func startsFailure(line string) bool {
	for _, re := range failureStarts {
		if re.MatchString(line) {
			return true
		}
	}
	return false
}

// blockEnd is the index after the block starting at start: it takes every following line that's
// blank or indented deeper than start's, and leaves trailing blank lines out.
func blockEnd(lines []string, start int) int {
	depth := indentOf(lines[start])
	end := start + 1
	for i := start + 1; i < len(lines); i++ {
		if lines[i] == "" {
			continue
		}
		if indentOf(lines[i]) <= depth {
			break
		}
		end = i + 1
	}
	return end
}

func indentOf(line string) int {
	return len(line) - len(strings.TrimLeft(line, " \t"))
}

// jobLogSummaries reads each failed job's log and picks why it failed (`failureSummary`), keyed
// by job ID. A log that can't be read yet, or says nothing structural, maps to a one-line note,
// so the report still points at the job's page.
func jobLogSummaries(r runner, jobs []runJob) map[int64][]string {
	out := map[int64][]string{}
	for _, job := range jobs {
		if !job.failed() || job.ID == 0 {
			continue
		}
		// `--allow-escape-sequences` keeps gh from mangling the log's own escapes; the summary
		// strips them.
		log, err := r.ghRetrying(transientRetries, r.ghCall([]string{"api", "--allow-escape-sequences", fmt.Sprintf("repos/%s/actions/jobs/%d/logs", repo, job.ID)}))
		switch summary := failureSummary(log); {
		case err != nil:
			out[job.ID] = []string{"(couldn't read its log yet; the link above has it)"}
		case len(summary) == 0:
			out[job.ID] = []string{"(its log names no failure block; the link above has it all)"}
		default:
			out[job.ID] = summary
		}
	}
	return out
}
