package main

import (
	"fmt"
	"sort"
	"strings"
	"sync"
	"time"

	"cmdr/scripts/check/checks"
)

// unknownSelectorCSVFileName is the THIRD log beside ~/cmdr-check-log.csv and
// ~/cmdr-test-log.csv, and it's a separate file for the same reason those two
// are separate from each other: every CSV reader hard-errors on a field-count
// mismatch, so one schema per question is the only shape that keeps a long
// history readable.
//
// Why record this at all: a name the runner doesn't recognize is evidence that
// the checks aren't named the way people reach for them. The rows are the input
// to a naming review (collect for about a month, then look for patterns and
// rename or regroup the checks), which makes this pure data collection. The run
// itself behaves as it always has: it prints the error and exits 1, and a
// failure to write a row stays silent. Schema and example queries: `DETAILS.md`
// § "The unrecognized-name log".
const unknownSelectorCSVFileName = "cmdr-unknown-check-log.csv"

var (
	unknownSelectorCSVHeader = []string{"timestamp", "unknown", "args", "did_you_mean"}
	unknownSelectorCSVMu     sync.Mutex
)

// suggestionLimit caps how many accepted names one typo earns, in the row and in
// the error message. Three is enough to read the intent back later; more reads as
// noise and buries the pattern the log exists to surface.
const suggestionLimit = 3

// unknownSelectorError reports positional selectors the runner doesn't accept.
// It carries the WHOLE invocation rather than only the first bad token, because
// the naming review wants to see what the user got right beside what they got
// wrong. `noLog` rides along because parseFlags hands main no cliFlags on its
// error path, and this log honors `--no-log` / `--ci` like the other two.
type unknownSelectorError struct {
	names []string // every unrecognized token, verbatim and in the order typed
	args  []string // the full argument list, as passed
	noLog bool
}

func newUnknownSelectorError(names, args []string, noLog bool) *unknownSelectorError {
	return &unknownSelectorError{names: names, args: args, noLog: noLog}
}

// Error keeps the wording a single unknown name has always produced, and adds a
// suggestion line whenever the runner has a decent guess.
func (e *unknownSelectorError) Error() string {
	var b strings.Builder
	label := checks.Pluralize(len(e.names), "unknown check or group", "unknown checks or groups")
	fmt.Fprintf(&b, "%s: %s", label, strings.Join(e.names, ", "))
	for _, name := range e.names {
		if line := didYouMeanLine(name, len(e.names) > 1); line != "" {
			b.WriteString("\n" + line)
		}
	}
	b.WriteString("\nRun 'pnpm check --help' to see available checks and groups")
	return b.String()
}

// didYouMeanLine renders the suggestion for one unrecognized name. A lone bad
// token reads as a plain question; with several in one invocation each line names
// the token it belongs to, so the lines stay unambiguous.
func didYouMeanLine(name string, qualify bool) string {
	matches := suggestSelectors(name)
	if len(matches) == 0 {
		return ""
	}
	if qualify {
		return fmt.Sprintf("Did you mean %s instead of %s?", joinOr(matches), name)
	}
	return fmt.Sprintf("Did you mean %s?", joinOr(matches))
}

// joinOr renders a list as "a", "a or b", or "a, b, or c".
func joinOr(items []string) string {
	switch len(items) {
	case 0:
		return ""
	case 1:
		return items[0]
	case 2:
		return items[0] + " or " + items[1]
	default:
		return strings.Join(items[:len(items)-1], ", ") + ", or " + items[len(items)-1]
	}
}

// logUnknownSelectors appends one row per unrecognized name to
// ~/cmdr-unknown-check-log.csv. Best-effort like the other two logs: it's
// instrumentation, and a full disk or a read-only home must never color a run's
// verdict or print noise.
func logUnknownSelectors(e *unknownSelectorError) {
	if e == nil || e.noLog {
		return
	}
	appendCSVRows(&unknownSelectorCSVMu, unknownSelectorCSVFileName, unknownSelectorCSVHeader,
		unknownSelectorRows(e, time.Now()))
}

// unknownSelectorRows builds the rows for one rejected invocation: the typed name
// verbatim, the whole argument list beside it (so a later read sees what was
// right as well as what was wrong), and the runner's best guess at the intent.
// The timestamp format matches the sibling logs, so the three read the same way
// in a query.
func unknownSelectorRows(e *unknownSelectorError, now time.Time) [][]string {
	timestamp := now.Format("2006-01-02 15:04:05")
	args := strings.Join(e.args, " ")
	rows := make([][]string, 0, len(e.names))
	for _, name := range e.names {
		rows = append(rows, []string{timestamp, name, args, strings.Join(suggestSelectors(name), " ")})
	}
	return rows
}

// selectorCandidates lists every name a positional selector may carry: each
// check's ID and nickname, plus the app and tech-group keywords.
func selectorCandidates() []string {
	names := make([]string, 0, 2*len(checks.AllChecks)+len(reservedSelectorNames))
	for i := range checks.AllChecks {
		names = append(names, checks.AllChecks[i].ID)
		if nickname := checks.AllChecks[i].Nickname; nickname != "" {
			names = append(names, nickname)
		}
	}
	return append(names, reservedSelectorNames...)
}

// suggestSelectors ranks the accepted names nearest to what was typed, closest
// first, capped at suggestionLimit. Nearness is edit distance within a
// length-scaled budget, plus one special case: a typed name that's a FRAGMENT of
// an accepted one ("rust-test" for "rust-tests", "smb" for "smb-e2e") is a
// near-miss however far its edit distance runs.
func suggestSelectors(name string) []string {
	typed := strings.ToLower(strings.TrimSpace(name))
	if typed == "" {
		return nil
	}
	budget := suggestionDistanceBudget(typed)

	type candidate struct {
		name string
		rank int
	}
	var hits []candidate
	for _, cand := range selectorCandidates() {
		lower := strings.ToLower(cand)
		switch dist := editDistance(typed, lower); {
		case dist <= budget:
			hits = append(hits, candidate{cand, dist})
		case len(typed) >= 3 && strings.Contains(lower, typed):
			hits = append(hits, candidate{cand, 1})
		}
	}

	// Closest first, then the shortest name (the least the user would have had to
	// add), then alphabetically so the output is stable.
	sort.Slice(hits, func(i, j int) bool {
		if hits[i].rank != hits[j].rank {
			return hits[i].rank < hits[j].rank
		}
		if len(hits[i].name) != len(hits[j].name) {
			return len(hits[i].name) < len(hits[j].name)
		}
		return hits[i].name < hits[j].name
	})

	matches := make([]string, 0, suggestionLimit)
	for _, hit := range hits {
		if len(matches) == suggestionLimit {
			break
		}
		matches = append(matches, hit.name)
	}
	return matches
}

// suggestionDistanceBudget scales the edit-distance tolerance with the length of
// what was typed: three edits away from a four-character token is a different
// word, not a typo.
func suggestionDistanceBudget(typed string) int {
	return min(max(len([]rune(typed))/2, 1), 3)
}

// editDistance is the Levenshtein distance between two strings, measured over
// runes so a non-ASCII paste scores by characters rather than bytes.
func editDistance(a, b string) int {
	ar, br := []rune(a), []rune(b)
	prev := make([]int, len(br)+1)
	cur := make([]int, len(br)+1)
	for j := range prev {
		prev[j] = j
	}
	for i := 1; i <= len(ar); i++ {
		cur[0] = i
		for j := 1; j <= len(br); j++ {
			cost := 1
			if ar[i-1] == br[j-1] {
				cost = 0
			}
			cur[j] = min(prev[j]+1, cur[j-1]+1, prev[j-1]+cost)
		}
		prev, cur = cur, prev
	}
	return prev[len(br)]
}
