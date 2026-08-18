package checks

import (
	"strings"
	"testing"
)

func TestJscpdPairKeySortsPathsSoOrderDoesNotMintASecondEntry(t *testing.T) {
	forward := jscpdPairKey("b.rs", "a.rs")
	backward := jscpdPairKey("a.rs", "b.rs")
	if forward != backward {
		t.Fatalf("pair key depends on argument order: %q vs %q", forward, backward)
	}
	if forward != "a.rs ↔ b.rs" {
		t.Fatalf("unexpected pair key %q", forward)
	}
}

func TestJscpdPairKeyCollapsesASelfPairToOnePath(t *testing.T) {
	if got := jscpdPairKey("a.rs", "a.rs"); got != "a.rs" {
		t.Fatalf("self-pair key = %q, want %q", got, "a.rs")
	}
}

func TestSummarizeJscpdClonesSumsLinesPerPairAndRanksWorstFirst(t *testing.T) {
	clones := []jscpdClone{
		{Format: "rust", Lines: 8, A: jscpdLocation{"a.rs", 1, 8}, B: jscpdLocation{"b.rs", 20, 27}},
		{Format: "rust", Lines: 40, A: jscpdLocation{"c.rs", 5, 44}, B: jscpdLocation{"d.rs", 9, 48}},
		{Format: "rust", Lines: 7, A: jscpdLocation{"b.rs", 60, 66}, B: jscpdLocation{"a.rs", 90, 96}},
	}
	report := summarizeJscpdClones(clones)

	if len(report.pairs) != 2 {
		t.Fatalf("pairs = %d, want 2", len(report.pairs))
	}
	if report.pairs[0].key != "c.rs ↔ d.rs" {
		t.Fatalf("worst pair = %q, want the 40-line one", report.pairs[0].key)
	}
	// The two a.rs/b.rs clones are reported in opposite orders; they must land in
	// one bucket, or an allowlist entry would only ever cover half the duplication.
	if got := report.linesByPair["a.rs ↔ b.rs"]; got != 15 {
		t.Fatalf("a.rs ↔ b.rs lines = %d, want 15", got)
	}
	if len(report.pairs[1].clones) != 2 {
		t.Fatalf("a.rs ↔ b.rs clones = %d, want 2", len(report.pairs[1].clones))
	}
}

func TestShrinkwrapJscpdAllowlistDropsGonePairsAndRatchetsShrunkOnes(t *testing.T) {
	report := summarizeJscpdClones([]jscpdClone{
		{Format: "rust", Lines: 12, A: jscpdLocation{"a.rs", 1, 12}, B: jscpdLocation{"b.rs", 1, 12}},
	})
	list := jscpdAllowlist{Pairs: map[string]int{
		"a.rs ↔ b.rs":    30,
		"gone.rs ↔ x.rs": 9,
	}}

	changes := shrinkwrapJscpdAllowlist(&list, report)

	if list.Pairs["a.rs ↔ b.rs"] != 12 {
		t.Fatalf("ratcheted entry = %d, want 12", list.Pairs["a.rs ↔ b.rs"])
	}
	if _, still := list.Pairs["gone.rs ↔ x.rs"]; still {
		t.Fatal("a pair with no clones left must be dropped")
	}
	if len(changes) != 2 {
		t.Fatalf("changes = %v, want one drop and one ratchet", changes)
	}
}

func TestFindJscpdRegressionsFlagsUnlistedPairsAndGrownOnes(t *testing.T) {
	report := summarizeJscpdClones([]jscpdClone{
		{Format: "rust", Lines: 50, A: jscpdLocation{"grew.rs", 1, 50}, B: jscpdLocation{"other.rs", 1, 50}},
		{Format: "rust", Lines: 20, A: jscpdLocation{"new.rs", 1, 20}, B: jscpdLocation{"fresh.rs", 1, 20}},
		{Format: "rust", Lines: 30, A: jscpdLocation{"ok.rs", 1, 30}, B: jscpdLocation{"fine.rs", 1, 30}},
	})
	list := jscpdAllowlist{Pairs: map[string]int{
		"grew.rs ↔ other.rs": 20,
		"fine.rs ↔ ok.rs":    30,
	}}

	regressions := findJscpdRegressions(report, list)

	if len(regressions) != 2 {
		t.Fatalf("regressions = %d, want 2 (one grown, one unlisted)", len(regressions))
	}
	// Worst overshoot first: the grown pair is +30, the unlisted one +20.
	if regressions[0].pair.key != "grew.rs ↔ other.rs" {
		t.Fatalf("first regression = %q, want the grown pair", regressions[0].pair.key)
	}
	if regressions[0].listed != true || regressions[0].allowed != 20 {
		t.Fatalf("grown pair should carry its allowlisted number, got listed=%v allowed=%d",
			regressions[0].listed, regressions[0].allowed)
	}
	if regressions[1].listed != false {
		t.Fatal("an unlisted pair must be reported as unlisted, not as allowed 0")
	}
}

func TestFindJscpdRegressionsIgnoresAPairThatShrank(t *testing.T) {
	report := summarizeJscpdClones([]jscpdClone{
		{Format: "rust", Lines: 10, A: jscpdLocation{"a.rs", 1, 10}, B: jscpdLocation{"b.rs", 1, 10}},
	})
	list := jscpdAllowlist{Pairs: map[string]int{"a.rs ↔ b.rs": 40}}
	if regressions := findJscpdRegressions(report, list); len(regressions) != 0 {
		t.Fatalf("a shrinking pair must not warn, got %v", regressions)
	}
}

func TestParseJscpdReportReadsEveryCloneAndTheTotals(t *testing.T) {
	raw := `{
	  "statistics": {
	    "total": {"lines": 1000, "sources": 12, "clones": 2, "duplicatedLines": 21, "percentage": 2.1}
	  },
	  "duplicates": [
	    {"format": "rust", "lines": 14, "firstFile": {"name": "./a.rs", "start": 5, "end": 18},
	     "secondFile": {"name": "b.rs", "start": 40, "end": 53}},
	    {"format": "typescript", "lines": 7, "firstFile": {"name": "c.ts", "start": 1, "end": 7},
	     "secondFile": {"name": "c.ts", "start": 30, "end": 36}}
	  ]
	}`
	clones, totals, err := parseJscpdReport([]byte(raw))
	if err != nil {
		t.Fatalf("parse: %v", err)
	}
	if len(clones) != 2 {
		t.Fatalf("clones = %d, want 2", len(clones))
	}
	// A `./`-prefixed path must normalize, or it keys a second allowlist entry for
	// the same file.
	if clones[0].A.Path != "a.rs" {
		t.Fatalf("first clone path = %q, want %q", clones[0].A.Path, "a.rs")
	}
	if clones[0].A.Start != 5 || clones[0].B.End != 53 {
		t.Fatalf("clone locations lost: %+v", clones[0])
	}
	if totals.sources != 12 || totals.duplicatedLines != 21 || totals.percentage != 2.1 {
		t.Fatalf("totals = %+v", totals)
	}
}

func TestParseJscpdReportStraightensABackwardsSpan(t *testing.T) {
	// jscpd reports a few intra-file clones with the two ends swapped (`start` 105,
	// `end` 51, positions agreeing). Printed verbatim it reads like the tool is
	// broken, so the parser orders the pair.
	raw := `{"statistics": {"total": {}}, "duplicates": [
	  {"format": "rust", "lines": 19, "firstFile": {"name": "a.rs", "start": 134, "end": 152},
	   "secondFile": {"name": "a.rs", "start": 105, "end": 51}}]}`
	clones, _, err := parseJscpdReport([]byte(raw))
	if err != nil {
		t.Fatalf("parse: %v", err)
	}
	if got := clones[0].B.String(); got != "a.rs:51-105" {
		t.Fatalf("backwards span = %q, want %q", got, "a.rs:51-105")
	}
}

func TestFormatJscpdRegressionsNamesEveryCloneWithFileAndLines(t *testing.T) {
	report := summarizeJscpdClones([]jscpdClone{
		{Format: "rust", Lines: 55, A: jscpdLocation{"commands/volumes.rs", 120, 175}, B: jscpdLocation{"commands/volumes_linux.rs", 88, 143}},
		{Format: "rust", Lines: 12, A: jscpdLocation{"commands/volumes.rs", 300, 312}, B: jscpdLocation{"commands/volumes_linux.rs", 12, 24}},
	})
	regressions := findJscpdRegressions(report, jscpdAllowlist{Pairs: map[string]int{}})

	out := formatJscpdRegressions(regressions)

	for _, want := range []string{
		"commands/volumes.rs:120-175",
		"commands/volumes_linux.rs:88-143",
		"commands/volumes.rs:300-312",
		"commands/volumes_linux.rs:12-24",
	} {
		if !strings.Contains(out, want) {
			t.Fatalf("regression output is missing %q:\n%s", want, out)
		}
	}
}

func TestFormatJscpdInventoryShowsTheWorstPairsWithALocation(t *testing.T) {
	var clones []jscpdClone
	for i := range 15 {
		clones = append(clones, jscpdClone{
			Format: "rust", Lines: 30 - i,
			A: jscpdLocation{Path: "a" + string(rune('a'+i)) + ".rs", Start: 1, End: 30 - i},
			B: jscpdLocation{Path: "b" + string(rune('a'+i)) + ".rs", Start: 1, End: 30 - i},
		})
	}
	report := summarizeJscpdClones(clones)

	out := formatJscpdInventory(report, "Rust")

	if strings.Count(out, "↔") != jscpdInventoryPairs {
		t.Fatalf("inventory should list %d pairs, got:\n%s", jscpdInventoryPairs, out)
	}
	if !strings.Contains(out, "aa.rs:1-30") {
		t.Fatalf("inventory must carry a file:line for each pair:\n%s", out)
	}
}
