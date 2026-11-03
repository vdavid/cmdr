package checks

import (
	"slices"
	"strings"
	"testing"
)

// newKaniRepo makes a git work tree with a virtual workspace root and the given
// files, with a private corpus so each test's listing is its own.
func newKaniRepo(t *testing.T, files map[string]string) string {
	t.Helper()
	previous := corpus
	corpus = newSourceCorpus()
	t.Cleanup(func() { corpus = previous })

	root := t.TempDir()
	gitRun(t, root, "init", "-q")
	writeTestFile(t, root, "Cargo.toml", "[workspace]\nmembers = [\"crates/*\"]\n")
	for rel, content := range files {
		writeTestFile(t, root, rel, content)
	}
	return root
}

const kaniCrateManifest = "[package]\nname = \"cmdr-thing\"\nversion = \"0.0.0\"\n\n[dependencies]\nname = \"not-this\"\n"

func TestFindKaniProofs_NamesEachHarnessByItsPathInTheCrate(t *testing.T) {
	root := newKaniRepo(t, map[string]string{
		"crates/thing/Cargo.toml": kaniCrateManifest,
		"crates/thing/src/lib.rs": "#[cfg(kani)]\nmod proofs {\n    #[kani::proof]\n    fn at_the_root() {}\n}\n",
		"crates/thing/src/read/name/proofs.rs": "#[kani::proof]\n#[kani::unwind(14)]\nfn kept_names_are_plain() {}\n\n" +
			"#[kani::proof]\npub(crate) fn the_verdict_holds() {}\n",
		"crates/thing/src/walk/mod.rs": "    #[kani::proof]\n    fn in_a_mod_file() {}\n",
		"crates/thing/src/plain.rs":    "fn not_a_proof() {}\n",
		"vendor/fork/src/lib.rs":       "#[kani::proof]\nfn theirs() {}\n",
	})
	proofs, err := findKaniProofs(root)
	if err != nil {
		t.Fatal(err)
	}
	var got []string
	for _, proof := range proofs {
		got = append(got, proof.property())
	}
	want := []string{
		"cmdr-thing/at_the_root",
		"cmdr-thing/read::name::proofs::kept_names_are_plain",
		"cmdr-thing/read::name::proofs::the_verdict_holds",
		"cmdr-thing/walk::in_a_mod_file",
	}
	if !slices.Equal(got, want) {
		t.Errorf("got %v, want %v", got, want)
	}
}

func TestFindKaniProofs_RefusesAProofOutsideALibrary(t *testing.T) {
	for name, files := range map[string]map[string]string{
		"a binary's main": {
			"crates/thing/Cargo.toml":  kaniCrateManifest,
			"crates/thing/src/main.rs": "#[kani::proof]\nfn proof() {}\n",
		},
		"outside `src/`": {
			"crates/thing/Cargo.toml":      kaniCrateManifest,
			"crates/thing/tests/proofs.rs": "#[kani::proof]\nfn proof() {}\n",
		},
		"an attribute with no fn": {
			"crates/thing/Cargo.toml": kaniCrateManifest,
			"crates/thing/src/lib.rs": "#[kani::proof]\nconst X: u8 = 1;\n",
		},
	} {
		t.Run(name, func(t *testing.T) {
			if _, err := findKaniProofs(newKaniRepo(t, files)); err == nil {
				t.Error("expected an error")
			}
		})
	}
}

func TestParseKaniProperty_RoundTripsAProofsName(t *testing.T) {
	proof := kaniProof{pkg: "cmdr-thing", harness: "read::proofs::holds"}
	pkg, harness, ok := parseKaniProperty(proof.property())
	if !ok || pkg != proof.pkg || harness != proof.harness {
		t.Errorf("got (%q, %q, %v)", pkg, harness, ok)
	}
	for _, bad := range []string{"cmdr-thing", "/holds", "cmdr-thing/"} {
		if _, _, ok := parseKaniProperty(bad); ok {
			t.Errorf("%q parsed", bad)
		}
	}
}

func TestKaniFindingReport_KeepsTheFailedChecksAndVerdicts(t *testing.T) {
	output := strings.Join([]string{
		"   Compiling cmdr-thing v0.0.0",
		"Checking harness proofs::holds...",
		"VERIFICATION RESULT:",
		" ** 1 of 68 failed",
		`Failed Checks: "a Match that doesn't mean unchanged"`,
		` File: "src/proofs.rs", line 42, in proofs::holds`,
		"VERIFICATION:- FAILED",
		"Verification Time: 0.1s",
	}, "\n")
	report := kaniFindingReport(output)
	for _, kept := range []string{"Checking harness proofs::holds", "a Match that doesn't mean unchanged", "line 42", "VERIFICATION:- FAILED"} {
		if !strings.Contains(report, kept) {
			t.Errorf("report lost %q:\n%s", kept, report)
		}
	}
	if strings.Contains(report, "Compiling") || strings.Contains(report, "Verification Time") {
		t.Errorf("report kept noise:\n%s", report)
	}
}

func TestKaniRecords_TakeNoInput(t *testing.T) {
	r := mutantRecord{Property: "kani:cmdr-thing/proofs::holds", Input: "fuzz/seeds/x"}
	if problems := kaniPropertyKind.checkRecord("", r); len(problems) != 1 {
		t.Errorf("expected one problem, got %v", problems)
	}
	r.Input = ""
	if problems := kaniPropertyKind.checkRecord("", r); len(problems) != 0 {
		t.Errorf("expected none, got %v", problems)
	}
}
