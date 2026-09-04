package checks

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// writeJscpdSourceFile plants a file the shrink-wrap can see, so an exempt entry
// pointing at it reads as live rather than dead.
func writeJscpdSourceFile(t *testing.T, rootDir, relPath string) {
	t.Helper()
	if err := os.WriteFile(filepath.Join(rootDir, filepath.FromSlash(relPath)), []byte("// present\n"), 0o644); err != nil {
		t.Fatalf("write %s: %v", relPath, err)
	}
}

// seedJscpdAllowlistFile plants a lane allowlist at the path loadJscpdAllowlist
// resolves, so a test can exercise the on-disk shape rather than the Go struct.
func seedJscpdAllowlistFile(t *testing.T, rootDir, body string) {
	t.Helper()
	path := jscpdAllowlistPath(rootDir, "test-lane")
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatalf("mkdir: %v", err)
	}
	if err := os.WriteFile(path, []byte(body), 0o644); err != nil {
		t.Fatalf("write %s: %v", path, err)
	}
}

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
	list := jscpdAllowlist{Pairs: map[string]jscpdPairLimit{
		"a.rs ↔ b.rs":    {Lines: 30},
		"gone.rs ↔ x.rs": {Lines: 9},
	}}

	changes := shrinkwrapJscpdAllowlist(t.TempDir(), &list, report)

	if list.Pairs["a.rs ↔ b.rs"].Lines != 12 {
		t.Fatalf("ratcheted entry = %d, want 12", list.Pairs["a.rs ↔ b.rs"].Lines)
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
	list := jscpdAllowlist{Pairs: map[string]jscpdPairLimit{
		"grew.rs ↔ other.rs": {Lines: 20},
		"fine.rs ↔ ok.rs":    {Lines: 30},
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
	list := jscpdAllowlist{Pairs: map[string]jscpdPairLimit{"a.rs ↔ b.rs": {Lines: 40}}}
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
	regressions := findJscpdRegressions(report, jscpdAllowlist{Pairs: map[string]jscpdPairLimit{}})

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

func TestFindJscpdRegressionsSkipsAnExemptPair(t *testing.T) {
	report := summarizeJscpdClones([]jscpdClone{
		{Format: "rust", Lines: 39, A: jscpdLocation{"gen.rs", 128, 166}, B: jscpdLocation{"gen.rs", 88, 126}},
		{Format: "rust", Lines: 12, A: jscpdLocation{"a.rs", 1, 12}, B: jscpdLocation{"b.rs", 1, 12}},
	})
	list := jscpdAllowlist{
		Pairs:  map[string]jscpdPairLimit{"a.rs ↔ b.rs": {Lines: 12}},
		Exempt: map[string]string{"gen.rs": "generated; the duplication is the generator's, not a hand-written copy"},
	}

	if got := findJscpdRegressions(report, list); len(got) != 0 {
		t.Fatalf("regressions = %v, want none (the only over-limit pair is exempt)", got)
	}
}

func TestFindJscpdRegressionsStillFlagsAnUnlistedPairBesideAnExemptOne(t *testing.T) {
	report := summarizeJscpdClones([]jscpdClone{
		{Format: "rust", Lines: 39, A: jscpdLocation{"gen.rs", 128, 166}, B: jscpdLocation{"gen.rs", 88, 126}},
		{Format: "rust", Lines: 12, A: jscpdLocation{"a.rs", 1, 12}, B: jscpdLocation{"b.rs", 1, 12}},
	})
	list := jscpdAllowlist{Exempt: map[string]string{"gen.rs": "generated"}}

	got := findJscpdRegressions(report, list)
	if len(got) != 1 || got[0].pair.key != "a.rs ↔ b.rs" {
		t.Fatalf("regressions = %v, want only a.rs ↔ b.rs", got)
	}
}

func TestShrinkwrapJscpdAllowlistKeepsAnExemptPairWithNoDuplicationLeft(t *testing.T) {
	rootDir := t.TempDir()
	writeJscpdSourceFile(t, rootDir, "gen.rs")
	// A `pairs` entry with no duplication left is stale and gets dropped. An
	// `exempt` one is a standing decision about a generated file, so it survives a
	// regeneration that happens to leave no clone this time.
	list := jscpdAllowlist{Exempt: map[string]string{"gen.rs": "generated"}}

	changes := shrinkwrapJscpdAllowlist(rootDir, &list, summarizeJscpdClones(nil))

	if _, ok := list.Exempt["gen.rs"]; !ok {
		t.Fatalf("dropped a live exempt entry; changes = %v", changes)
	}
}

func TestShrinkwrapJscpdAllowlistDropsAnExemptPairWhoseFileIsGone(t *testing.T) {
	rootDir := t.TempDir()
	writeJscpdSourceFile(t, rootDir, "kept.rs")
	list := jscpdAllowlist{Exempt: map[string]string{
		"kept.rs":              "generated",
		"gone.rs":              "generated",
		"kept.rs ↔ missing.rs": "generated",
	}}

	changes := shrinkwrapJscpdAllowlist(rootDir, &list, summarizeJscpdClones(nil))

	if _, ok := list.Exempt["gone.rs"]; ok {
		t.Fatalf("kept an exempt entry whose file is gone; changes = %v", changes)
	}
	if _, ok := list.Exempt["kept.rs ↔ missing.rs"]; ok {
		t.Fatalf("kept an exempt pair whose second file is gone; changes = %v", changes)
	}
	if _, ok := list.Exempt["kept.rs"]; !ok {
		t.Fatalf("dropped a live exempt entry; changes = %v", changes)
	}
}

func TestShrinkwrapJscpdAllowlistDropsAPairsEntryTheExemptSectionAlreadyCovers(t *testing.T) {
	rootDir := t.TempDir()
	writeJscpdSourceFile(t, rootDir, "gen.rs")
	report := summarizeJscpdClones([]jscpdClone{
		{Format: "rust", Lines: 39, A: jscpdLocation{"gen.rs", 128, 166}, B: jscpdLocation{"gen.rs", 88, 126}},
	})
	list := jscpdAllowlist{
		Pairs:  map[string]jscpdPairLimit{"gen.rs": {Lines: 39}},
		Exempt: map[string]string{"gen.rs": "generated"},
	}

	changes := shrinkwrapJscpdAllowlist(rootDir, &list, report)

	if _, ok := list.Pairs["gen.rs"]; ok {
		t.Fatalf("kept a redundant pairs entry beside an exempt one; changes = %v", changes)
	}
}

func TestLoadJscpdAllowlistReadsBothPairValueShapes(t *testing.T) {
	rootDir := t.TempDir()
	seedJscpdAllowlistFile(t, rootDir, `{
	  "pairs": {
	    "a.rs ↔ b.rs": 14,
	    "c.rs ↔ d.rs": {"lines": 62, "reason": "trait-method signatures; a macro would cost more than it saves"}
	  }
	}`)

	list := loadJscpdAllowlist(rootDir, "test-lane")

	if list.Pairs["a.rs ↔ b.rs"].Lines != 14 || list.Pairs["a.rs ↔ b.rs"].Reason != "" {
		t.Fatalf("a bare number must read as a reasonless limit, got %+v", list.Pairs["a.rs ↔ b.rs"])
	}
	if list.Pairs["c.rs ↔ d.rs"].Lines != 62 {
		t.Fatalf("object entry lines = %d, want 62", list.Pairs["c.rs ↔ d.rs"].Lines)
	}
	if !strings.Contains(list.Pairs["c.rs ↔ d.rs"].Reason, "trait-method signatures") {
		t.Fatalf("object entry lost its reason, got %q", list.Pairs["c.rs ↔ d.rs"].Reason)
	}
}

func TestJscpdAllowlistWritesABareNumberUnlessThePairCarriesAReason(t *testing.T) {
	rootDir := t.TempDir()
	seedJscpdAllowlistFile(t, rootDir, "{}")
	list := jscpdAllowlist{Pairs: map[string]jscpdPairLimit{
		"a.rs ↔ b.rs": {Lines: 14},
		"c.rs ↔ d.rs": {Lines: 62, Reason: "trait-method signatures"},
	}}

	if err := writeJSONAllowlist(jscpdAllowlistPath(rootDir, "test-lane"), list); err != nil {
		t.Fatalf("write allowlist: %v", err)
	}

	data, err := os.ReadFile(jscpdAllowlistPath(rootDir, "test-lane"))
	if err != nil {
		t.Fatalf("read allowlist: %v", err)
	}
	written := string(data)
	if !strings.Contains(written, `"a.rs ↔ b.rs": 14`) {
		t.Fatalf("a reasonless entry must stay a bare number, got:\n%s", written)
	}
	if !strings.Contains(written, `"lines": 62`) || !strings.Contains(written, `"reason": "trait-method signatures"`) {
		t.Fatalf("an entry with a reason must write as an object, got:\n%s", written)
	}
}

func TestShrinkwrapJscpdAllowlistKeepsAPairsReasonWhenItRatchets(t *testing.T) {
	report := summarizeJscpdClones([]jscpdClone{
		{Format: "rust", Lines: 40, A: jscpdLocation{"a.rs", 1, 40}, B: jscpdLocation{"b.rs", 1, 40}},
	})
	list := jscpdAllowlist{Pairs: map[string]jscpdPairLimit{
		"a.rs ↔ b.rs": {Lines: 62, Reason: "trait-method signatures"},
	}}

	changes := shrinkwrapJscpdAllowlist(t.TempDir(), &list, report)

	if list.Pairs["a.rs ↔ b.rs"].Lines != 40 {
		t.Fatalf("ratcheted entry = %d, want 40 (changes = %v)", list.Pairs["a.rs ↔ b.rs"].Lines, changes)
	}
	if list.Pairs["a.rs ↔ b.rs"].Reason != "trait-method signatures" {
		t.Fatalf("ratcheting dropped the reason, got %q", list.Pairs["a.rs ↔ b.rs"].Reason)
	}
}

func TestJscpdAllowlistReasonSurvivesAWriteLoadRoundTrip(t *testing.T) {
	rootDir := t.TempDir()
	seedJscpdAllowlistFile(t, rootDir, `{
	  "pairs": {"a.rs ↔ b.rs": {"lines": 62, "reason": "trait-method signatures"}}
	}`)
	report := summarizeJscpdClones([]jscpdClone{
		{Format: "rust", Lines: 40, A: jscpdLocation{"a.rs", 1, 40}, B: jscpdLocation{"b.rs", 1, 40}},
	})

	list := loadJscpdAllowlist(rootDir, "test-lane")
	shrinkwrapJscpdAllowlist(rootDir, &list, report)
	if err := writeJSONAllowlist(jscpdAllowlistPath(rootDir, "test-lane"), list); err != nil {
		t.Fatalf("write allowlist: %v", err)
	}

	reloaded := loadJscpdAllowlist(rootDir, "test-lane")
	if reloaded.Pairs["a.rs ↔ b.rs"] != (jscpdPairLimit{Lines: 40, Reason: "trait-method signatures"}) {
		t.Fatalf("round trip lost the ratchet or the reason, got %+v", reloaded.Pairs["a.rs ↔ b.rs"])
	}
}

func TestFormatJscpdRegressionsPrintsTheAllowlistedReason(t *testing.T) {
	report := summarizeJscpdClones([]jscpdClone{
		{Format: "rust", Lines: 70, A: jscpdLocation{"a.rs", 1, 70}, B: jscpdLocation{"b.rs", 1, 70}},
	})
	list := jscpdAllowlist{Pairs: map[string]jscpdPairLimit{
		"a.rs ↔ b.rs": {Lines: 62, Reason: "trait-method signatures"},
	}}

	out := formatJscpdRegressions(findJscpdRegressions(report, list))

	if !strings.Contains(out, "trait-method signatures") {
		t.Fatalf("a grown pair must show why it was accepted, got:\n%s", out)
	}
}
