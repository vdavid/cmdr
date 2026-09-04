package main

import (
	"encoding/csv"
	"errors"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"testing"
)

// readUnknownLog returns the rows of ~/cmdr-unknown-check-log.csv under a
// redirected HOME, header included. A missing file yields no rows.
func readUnknownLog(t *testing.T, home string) [][]string {
	t.Helper()
	f, err := os.Open(filepath.Join(home, unknownSelectorCSVFileName))
	if os.IsNotExist(err) {
		return nil
	}
	if err != nil {
		t.Fatalf("open unknown-name log: %v", err)
	}
	defer f.Close()
	rows, err := csv.NewReader(f).ReadAll()
	if err != nil {
		t.Fatalf("read unknown-name log: %v", err)
	}
	return rows
}

func TestLogUnknownSelectorsRecordsTheNameTheArgsAndTheGuess(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)

	logUnknownSelectors(newUnknownSelectorError(
		[]string{"clipy"}, []string{"clipy", "--fast", "website"}, false))

	rows := readUnknownLog(t, home)
	if len(rows) != 2 {
		t.Fatalf("expected a header plus 1 row, got %d: %v", len(rows), rows)
	}
	if !slices.Equal(rows[0], unknownSelectorCSVHeader) {
		t.Fatalf("unexpected header: %v", rows[0])
	}
	row := rows[1]
	if row[1] != "clipy" {
		t.Errorf("unknown name = %q, want the typed name verbatim", row[1])
	}
	// The whole invocation is the point: a naming review reads what the user got
	// right beside what they got wrong.
	if row[2] != "clipy --fast website" {
		t.Errorf("args = %q, want the full argument list", row[2])
	}
	if !strings.Contains(row[3], "clippy") {
		t.Errorf("did_you_mean = %q, want it to name clippy", row[3])
	}
}

func TestLogUnknownSelectorsWritesOneRowPerUnknownName(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)

	logUnknownSelectors(newUnknownSelectorError(
		[]string{"clipy", "rustfmtt"}, []string{"clipy,rustfmtt"}, false))

	rows := readUnknownLog(t, home)
	if len(rows) != 3 {
		t.Fatalf("expected a header plus 2 rows, got %d: %v", len(rows), rows)
	}
	if rows[1][1] != "clipy" || rows[2][1] != "rustfmtt" {
		t.Errorf("expected both names in the order typed, got %q and %q", rows[1][1], rows[2][1])
	}
}

func TestLogUnknownSelectorsHonorsNoLog(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)

	logUnknownSelectors(newUnknownSelectorError([]string{"clipy"}, []string{"clipy"}, true))

	if rows := readUnknownLog(t, home); len(rows) != 0 {
		t.Fatalf("--no-log / --ci must write no file at all, got %v", rows)
	}
}

func TestParseFlagsReportsEveryUnknownNameWithTheInvocation(t *testing.T) {
	args := []string{"clipy", "--fast", "website", "rustfmtt"}
	_, err := parseFlags(args)

	var unknown *unknownSelectorError
	if !errors.As(err, &unknown) {
		t.Fatalf("parseFlags() = %v, want an *unknownSelectorError", err)
	}
	if !slices.Equal(unknown.names, []string{"clipy", "rustfmtt"}) {
		t.Errorf("names = %v, want every unrecognized token", unknown.names)
	}
	if !slices.Equal(unknown.args, args) {
		t.Errorf("args = %v, want the invocation as typed", unknown.args)
	}
}

func TestParseFlagsCarriesTheLoggingFlagOntoTheError(t *testing.T) {
	for _, args := range [][]string{{"--no-log", "clipy"}, {"--ci", "clipy"}, {"clipy", "--no-log"}} {
		var unknown *unknownSelectorError
		if _, err := parseFlags(args); !errors.As(err, &unknown) {
			t.Fatalf("parseFlags(%q) = %v, want an *unknownSelectorError", args, err)
		} else if !unknown.noLog {
			t.Errorf("parseFlags(%q): noLog = false, want the log suppressed", args)
		}
	}

	var unknown *unknownSelectorError
	if _, err := parseFlags([]string{"clipy"}); !errors.As(err, &unknown) {
		t.Fatalf("parseFlags() = %v, want an *unknownSelectorError", err)
	} else if unknown.noLog {
		t.Error("a plain run should log the unrecognized name")
	}
}

func TestUnknownSelectorErrorKeepsTheOriginalWording(t *testing.T) {
	got := newUnknownSelectorError([]string{"clipy"}, []string{"clipy"}, false).Error()
	lines := strings.Split(got, "\n")
	if lines[0] != "unknown check or group: clipy" {
		t.Errorf("first line = %q, want the wording a single bad name has always produced", lines[0])
	}
	if lines[len(lines)-1] != "Run 'pnpm check --help' to see available checks and groups" {
		t.Errorf("last line = %q, want the help pointer", lines[len(lines)-1])
	}
	if !strings.Contains(got, "Did you mean clippy") {
		t.Errorf("error = %q, want a suggestion naming clippy", got)
	}
}

func TestUnknownSelectorErrorNamesEachBadTokenWhenThereAreSeveral(t *testing.T) {
	got := newUnknownSelectorError([]string{"clipy", "rustfmtt"}, nil, false).Error()
	if !strings.HasPrefix(got, "unknown checks or groups: clipy, rustfmtt") {
		t.Errorf("error = %q, want both names in the first line", got)
	}
	if !strings.Contains(got, "instead of clipy?") || !strings.Contains(got, "instead of rustfmtt?") {
		t.Errorf("error = %q, want each suggestion tied to its token", got)
	}
}

func TestSuggestSelectors(t *testing.T) {
	tests := []struct {
		name  string
		typed string
		want  string // the accepted name the guess must offer
	}{
		{"a one-character typo", "clipy", "clippy"},
		{"a doubled character", "rustfmtt", "rustfmt"},
		{"a fragment of the real name", "rust-test", "rust-tests"},
		{"a near-miss group keyword", "svelt", "svelte"},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			got := suggestSelectors(tc.typed)
			if !slices.Contains(got, tc.want) {
				t.Errorf("suggestSelectors(%q) = %v, want it to offer %q", tc.typed, got, tc.want)
			}
			if len(got) > suggestionLimit {
				t.Errorf("suggestSelectors(%q) returned %d names, want at most %d", tc.typed, len(got), suggestionLimit)
			}
		})
	}
}

func TestSuggestSelectorsStaysQuietOnGibberish(t *testing.T) {
	// A row with no guess is still worth logging; a wrong guess is worse than none.
	for _, typed := range []string{"", "qqqqqqqqqqqqqqqq"} {
		if got := suggestSelectors(typed); len(got) != 0 {
			t.Errorf("suggestSelectors(%q) = %v, want no guess", typed, got)
		}
	}
}

func TestEditDistance(t *testing.T) {
	tests := []struct {
		a, b string
		want int
	}{
		{"", "", 0},
		{"clippy", "clippy", 0},
		{"clipy", "clippy", 1},
		{"", "clippy", 6},
		{"kitten", "sitting", 3},
	}
	for _, tc := range tests {
		if got := editDistance(tc.a, tc.b); got != tc.want {
			t.Errorf("editDistance(%q, %q) = %d, want %d", tc.a, tc.b, got, tc.want)
		}
	}
}

func TestJoinOr(t *testing.T) {
	tests := []struct {
		items []string
		want  string
	}{
		{nil, ""},
		{[]string{"a"}, "a"},
		{[]string{"a", "b"}, "a or b"},
		{[]string{"a", "b", "c"}, "a, b, or c"},
	}
	for _, tc := range tests {
		if got := joinOr(tc.items); got != tc.want {
			t.Errorf("joinOr(%v) = %q, want %q", tc.items, got, tc.want)
		}
	}
}
