package checks

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"testing"
	"time"
)

// toyPropertyKind is a property family small enough to run in a unit test. Its
// "program" is `src/limits.txt`, one `<name>=<number>` per line, and it has two
// properties:
//   - `toy:cap_holds` reads `cap` and goes red, printing "cap exceeded", past 10.
//   - `toy:vacuous` reads nothing and always passes: the shape this whole
//     mechanism exists to catch, a property that can't fail.
var toyPropertyKind = propertyKind{
	name: "toy",
	properties: func(string) ([]string, error) {
		return []string{"cap_holds", "vacuous"}, nil
	},
	checkRecord: func(string, mutantRecord) []string { return nil },
	run: func(tree string, r mutantRecord) (mutantRun, error) {
		_, name := r.kindAndName()
		if name == "vacuous" {
			return mutantRun{output: "ok"}, nil
		}
		data, err := os.ReadFile(filepath.Join(tree, "src", "limits.txt"))
		if err != nil {
			return mutantRun{}, err
		}
		for line := range strings.SplitSeq(string(data), "\n") {
			value, ok := strings.CutPrefix(line, "cap=")
			if !ok {
				continue
			}
			n, err := strconv.Atoi(value)
			if err != nil {
				return mutantRun{failed: true, output: "panicked: cap isn't a number"}, nil
			}
			if n > 10 {
				return mutantRun{failed: true, output: "panicked: cap exceeded"}, nil
			}
		}
		return mutantRun{output: "ok"}, nil
	},
}

var toyKinds = []propertyKind{toyPropertyKind}

// newMutantRepo makes a git work tree holding the toy program and the given
// record file, with a private corpus so each test's listing is its own.
func newMutantRepo(t *testing.T, records []mutantRecord) string {
	t.Helper()
	previous := corpus
	corpus = newSourceCorpus()
	t.Cleanup(func() { corpus = previous })

	root := t.TempDir()
	gitRun(t, root, "init", "-q")
	writeTestFile(t, root, "src/limits.txt", "floor=1\ncap=10\n")
	writeTestFile(t, root, ".gitignore", "target/\n")
	if records != nil {
		data, err := json.Marshal(mutantRecordFile{Mutants: records})
		if err != nil {
			t.Fatal(err)
		}
		writeTestFile(t, root, "src/limits.mutants.json", string(data))
	}
	return root
}

func writeTestFile(t *testing.T, root, rel, content string) {
	t.Helper()
	path := filepath.Join(root, filepath.FromSlash(rel))
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, []byte(content), 0o644); err != nil {
		t.Fatal(err)
	}
}

func capRecord() mutantRecord {
	return mutantRecord{
		Property: "toy:cap_holds",
		Why:      "the cap is raised past what the property allows",
		File:     "src/limits.txt",
		Find:     "cap=10",
		Replace:  "cap=99",
		Expect:   "cap exceeded",
	}
}

func runToyMutants(t *testing.T, root string) (CheckResult, error) {
	t.Helper()
	return runPropertyMutants(&CheckContext{RootDir: root}, toyKinds, filepath.Join(root, "target", "property-mutants"))
}

func TestPropertyMutants_ACaughtMutantPassesAndLeavesTheMirrorClean(t *testing.T) {
	root := newMutantRepo(t, []mutantRecord{capRecord()})
	result, err := runToyMutants(t, root)
	if err != nil {
		t.Fatalf("expected the mutant to be caught, got: %v", err)
	}
	if !strings.Contains(result.Message, "1 mutant caught") {
		t.Errorf("unexpected message %q", result.Message)
	}
	mirrored, err := os.ReadFile(filepath.Join(root, "target", "property-mutants", "tree", "src", "limits.txt"))
	if err != nil {
		t.Fatal(err)
	}
	if string(mirrored) != "floor=1\ncap=10\n" {
		t.Errorf("the mirror kept the mutant: %q", mirrored)
	}
	original, _ := os.ReadFile(filepath.Join(root, "src", "limits.txt"))
	if string(original) != "floor=1\ncap=10\n" {
		t.Errorf("the checkout was touched: %q", original)
	}
}

// The card's acceptance test: a property that can't fail is reported, even
// though its record is well-formed and applies cleanly.
func TestPropertyMutants_AVacuousPropertyIsCaught(t *testing.T) {
	record := capRecord()
	record.Property = "toy:vacuous"
	root := newMutantRepo(t, []mutantRecord{record})
	_, err := runToyMutants(t, root)
	if err == nil {
		t.Fatal("a property that passes under its mutant must fail the check")
	}
	if !strings.Contains(err.Error(), "SURVIVED") || !strings.Contains(err.Error(), "toy:vacuous") {
		t.Errorf("the failure should name the surviving mutant, got: %v", err)
	}
}

func TestPropertyMutants_RedForTheWrongReasonFails(t *testing.T) {
	record := capRecord()
	record.Replace = "cap=ten"
	root := newMutantRepo(t, []mutantRecord{record})
	_, err := runToyMutants(t, root)
	if err == nil || !strings.Contains(err.Error(), "without printing \"cap exceeded\"") {
		t.Fatalf("a mutant caught by some other failure must be reported, got: %v", err)
	}
}

func TestPropertyMutants_RedWithoutTheMutantFails(t *testing.T) {
	root := newMutantRepo(t, []mutantRecord{capRecord()})
	writeTestFile(t, root, "src/limits.txt", "floor=1\ncap=10\ncap=11\n")
	_, err := runToyMutants(t, root)
	if err == nil || !strings.Contains(err.Error(), "red WITHOUT the mutant") {
		t.Fatalf("a baseline that's already red proves nothing and must be reported, got: %v", err)
	}
}

func TestPropertyMutants_RefusesRecordsThatDontApply(t *testing.T) {
	record := capRecord()
	record.Find = "cap=12"
	root := newMutantRepo(t, []mutantRecord{record})
	_, err := runToyMutants(t, root)
	if err == nil || !strings.Contains(err.Error(), "no longer occurs") {
		t.Fatalf("expected the stale record to stop the run, got: %v", err)
	}
}

func recordProblems(t *testing.T, root string) []string {
	t.Helper()
	_, problems, err := loadMutantRecords(root, toyKinds)
	if err != nil {
		t.Fatal(err)
	}
	return problems
}

func TestMutantRecords_ValidationNamesEachProblem(t *testing.T) {
	cases := []struct {
		name   string
		edit   func(r *mutantRecord)
		expect string
	}{
		{"ambiguous find", func(r *mutantRecord) { r.Find = "=1" }, "occurs 2 times"},
		{"vanished find", func(r *mutantRecord) { r.Find = "cap=11" }, "no longer occurs"},
		{"unknown kind", func(r *mutantRecord) { r.Property = "kani:cap_holds" }, "known kind"},
		{"gone property", func(r *mutantRecord) { r.Property = "toy:renamed" }, "no toy property named"},
		{"multi-line", func(r *mutantRecord) { r.Replace = "cap=1\ncap=2" }, "spans lines"},
		{"no-op", func(r *mutantRecord) { r.Replace = r.Find }, "changes nothing"},
		{"no expect", func(r *mutantRecord) { r.Expect = "" }, "`expect` is empty"},
		{"escaping path", func(r *mutantRecord) { r.File = "../outside.txt" }, "isn't a repo-relative path"},
		{"absolute path", func(r *mutantRecord) { r.File = "/etc/hosts" }, "isn't a repo-relative path"},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			record := capRecord()
			tc.edit(&record)
			problems := recordProblems(t, newMutantRepo(t, []mutantRecord{record}))
			if len(problems) == 0 || !strings.Contains(strings.Join(problems, "\n"), tc.expect) {
				t.Errorf("expected a problem mentioning %q, got %q", tc.expect, problems)
			}
		})
	}
}

func TestMutantRecords_UnknownFieldsAreRefused(t *testing.T) {
	root := newMutantRepo(t, nil)
	writeTestFile(t, root, "src/limits.mutants.json", `{"mutants": [{"property": "toy:cap_holds", "expected": "x"}]}`)
	problems := recordProblems(t, root)
	if len(problems) != 1 || !strings.Contains(problems[0], "unknown field") {
		t.Errorf("expected the misspelled field to be refused, got %q", problems)
	}
}

func TestMutantRecordsCheck_FailsOnAnUnrecordedProperty(t *testing.T) {
	root := newMutantRepo(t, []mutantRecord{capRecord()})
	_, err := checkPropertyMutantRecords(&CheckContext{RootDir: root, CI: true}, toyKinds)
	if err == nil || !strings.Contains(err.Error(), "toy:vacuous") {
		t.Fatalf("expected the unrecorded property to be named, got: %v", err)
	}
}

func TestMutantRecordsCheck_AllowlistCoversAndShrinkWraps(t *testing.T) {
	root := newMutantRepo(t, []mutantRecord{capRecord()})
	list := propertyMutantsAllowlist{Unrecorded: map[string]unrecordedProperty{
		"toy:vacuous":   {Reason: "crash-only for now"},
		"toy:cap_holds": {Reason: "recorded since"},
		"toy:gone":      {Reason: "deleted since"},
	}}
	path := filepath.Join(root, filepath.FromSlash(propertyMutantsAllowlistRelPath))
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := writeJSONAllowlist(path, list); err != nil {
		t.Fatal(err)
	}

	result, err := checkPropertyMutantRecords(&CheckContext{RootDir: root}, toyKinds)
	if err != nil {
		t.Fatalf("an allowlisted property should pass, got: %v", err)
	}
	if !result.MadeChanges {
		t.Error("expected the stale entries to be shrink-wrapped")
	}
	after, err := loadPropertyMutantsAllowlist(root)
	if err != nil {
		t.Fatal(err)
	}
	if len(after.Unrecorded) != 1 || after.Unrecorded["toy:vacuous"].Reason == "" {
		t.Errorf("expected only toy:vacuous to stay, got %v", after.Unrecorded)
	}
}

func TestMutantRecordsCheck_AReasonlessEntryFails(t *testing.T) {
	root := newMutantRepo(t, []mutantRecord{capRecord()})
	path := filepath.Join(root, filepath.FromSlash(propertyMutantsAllowlistRelPath))
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := writeJSONAllowlist(path, propertyMutantsAllowlist{Unrecorded: map[string]unrecordedProperty{"toy:vacuous": {}}}); err != nil {
		t.Fatal(err)
	}
	_, err := checkPropertyMutantRecords(&CheckContext{RootDir: root, CI: true}, toyKinds)
	if err == nil || !strings.Contains(err.Error(), "no `reason`") {
		t.Fatalf("expected the reasonless entry to fail, got: %v", err)
	}
}

func TestSyncMutantTree_KeepsUnchangedFilesAndDropsDeletedOnes(t *testing.T) {
	root := newMutantRepo(t, nil)
	writeTestFile(t, root, "src/doomed.txt", "bye\n")
	tree := filepath.Join(root, "target", "property-mutants", "tree")
	manifest := filepath.Join(root, "target", "property-mutants", "tree-files.txt")
	if err := syncMutantTree(root, tree, manifest); err != nil {
		t.Fatal(err)
	}
	kept := filepath.Join(tree, "src", "limits.txt")
	old := time.Now().Add(-time.Hour)
	if err := os.Chtimes(kept, old, old); err != nil {
		t.Fatal(err)
	}
	// Build output in the mirror isn't listed, so it must survive the sync.
	writeTestFile(t, tree, "target/artifact", "built")
	// A mutant an interrupted run left behind must be rewritten.
	writeTestFile(t, tree, "src/limits.mutants.json", "stale")
	writeTestFile(t, root, "src/limits.mutants.json", `{"mutants": []}`)
	if err := os.Remove(filepath.Join(root, "src", "doomed.txt")); err != nil {
		t.Fatal(err)
	}

	corpus = newSourceCorpus()
	if err := syncMutantTree(root, tree, manifest); err != nil {
		t.Fatal(err)
	}
	if info, err := os.Stat(kept); err != nil || !info.ModTime().Equal(old) {
		t.Errorf("an unchanged file was rewritten, which costs cargo a rebuild")
	}
	if fileExists(filepath.Join(tree, "src", "doomed.txt")) {
		t.Error("a deleted file stayed in the mirror")
	}
	if !fileExists(filepath.Join(tree, "target", "artifact")) {
		t.Error("the sync deleted build output it never listed")
	}
	if data, _ := os.ReadFile(filepath.Join(tree, "src", "limits.mutants.json")); string(data) != `{"mutants": []}` {
		t.Errorf("a changed file wasn't re-copied: %q", data)
	}
}

func TestFuzzTargetNames_ReadsEveryBinTable(t *testing.T) {
	root := t.TempDir()
	writeTestFile(t, root, "fuzz/Cargo.toml", `[package]
name = "cmdr-fuzz"

[dependencies]
name-like = "1"

[[bin]]
name = "adb_sync"
path = "fuzz_targets/adb_sync.rs"

[[bin]]
path = "fuzz_targets/pdf.rs"
name = "pdf"
`)
	names, err := fuzzTargetNames(root)
	if err != nil {
		t.Fatal(err)
	}
	if strings.Join(names, ",") != "adb_sync,pdf" {
		t.Errorf("got %v", names)
	}
}

func TestFuzzMutantRecord_NeedsAnInputFile(t *testing.T) {
	root := t.TempDir()
	writeTestFile(t, root, "fuzz/seeds/t/a.txt", "a")
	record := mutantRecord{Property: "fuzz:t"}
	if p := checkFuzzMutantRecord(root, record); len(p) != 1 || !strings.Contains(p[0], "`input` is empty") {
		t.Errorf("got %q", p)
	}
	record.Input = "fuzz/seeds/t"
	if p := checkFuzzMutantRecord(root, record); len(p) != 1 || !strings.Contains(p[0], "isn't a file") {
		t.Errorf("got %q", p)
	}
	record.Input = "fuzz/seeds/t/a.txt"
	if p := checkFuzzMutantRecord(root, record); len(p) != 0 {
		t.Errorf("got %q", p)
	}
}

// The catching input has to sit in the target's corpus, so the mutant run also
// proves a fuzzing run starts from an input that reaches the property.
func TestFuzzMutantRecord_InputLivesInTheTargetsCorpus(t *testing.T) {
	root := t.TempDir()
	writeTestFile(t, root, "fuzz/seeds/t/a.txt", "a")
	writeTestFile(t, root, "fuzz/seeds/other/b.txt", "b")
	writeTestFile(t, root, "fuzz/large-seeds/t/c.bin.gz", "c")
	writeTestFile(t, root, "fuzz/elsewhere/d.txt", "d")
	record := mutantRecord{Property: "fuzz:t"}
	for _, input := range []string{"fuzz/seeds/other/b.txt", "fuzz/elsewhere/d.txt"} {
		record.Input = input
		if p := checkFuzzMutantRecord(root, record); len(p) != 1 || !strings.Contains(p[0], "isn't in the target's corpus") {
			t.Errorf("%s: got %q", input, p)
		}
	}
	for _, input := range []string{"fuzz/seeds/t/a.txt", "fuzz/large-seeds/t/c.bin.gz"} {
		record.Input = input
		if p := checkFuzzMutantRecord(root, record); len(p) != 0 {
			t.Errorf("%s: got %q", input, p)
		}
	}
}
