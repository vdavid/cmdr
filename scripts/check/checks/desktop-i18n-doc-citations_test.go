package checks

import (
	"fmt"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
)

// testCitationKeys is a miniature English catalog: enough namespaces to exercise
// the gate, and the `servers.sheet.*` family that the dead `fileExplorer.network.login.*`
// citations were renamed into.
var testCitationKeys = []string{
	"settings.ai.endpoint.label",
	"settings.aiendpoint.label",
	"settings.section.advanced",
	"fileExplorer.mtp.tryAgain",
	"fileExplorer.network.connect",
	"servers.sheet.username",
	"servers.sheet.password",
	"servers.sheet.remember",
	"servers.sheet.connectAsGuest",
	"mtp.connectedToast.title",
	// `red` opens `rememberInKeychain`, so it's the shape that made an earlier
	// containment bonus rank tag colours above the real rename.
	"menu.tag.red",
}

func TestDottedKeyIndexMatchesSegmentAligned(t *testing.T) {
	ix := testCitationIndex(testCitationKeys)

	cases := []struct {
		name  string
		token string
		want  bool
	}{
		{"an exact key resolves", "settings.ai.endpoint.label", true},
		{"a prefix of a key resolves", "settings.ai", true},
		{"a suffix of a key resolves", "sheet.connectAsGuest", true},
		{"an infix of a key resolves", "ai.endpoint", true},
		{"a single segment is never a citation", "settings", false},
		{"a partial segment never matches", "settings.aiend", false},
		{"segments must align, not just concatenate", "ai.endpoint.label.extra", false},
		{"a dead key does not resolve", "fileExplorer.network.login.rememberInKeychain", false},
		{"the run has to be contiguous", "settings.endpoint", false},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			if got := ix.resolves(tc.token); got != tc.want {
				t.Errorf("resolves(%q) = %v, want %v", tc.token, got, tc.want)
			}
		})
	}
}

// `ai.endpoint` must not be satisfied by `settings.aiendpoint.label`: a raw
// substring matcher accepts it, and then a citation naming a key that no longer
// exists reads as verified.
func TestDottedKeyIndexRejectsRawSubstringMatches(t *testing.T) {
	ix := testCitationIndex([]string{"settings.aiendpoint.label"})
	if ix.resolves("ai.endpoint") {
		t.Error("`ai.endpoint` matched `settings.aiendpoint.label` as a raw substring")
	}
}

func TestScanDocForCitationsGatesOnLaneNamespaces(t *testing.T) {
	ix := testCitationIndex(testCitationKeys)
	content := strings.Join([]string{
		"Finder `LocalizableMerged.strings` `MR10.1` and NetAuthAgent `PHL-pS-ELV.title`.",
		"Dates render as `dd.MM.yyyy`, hosts as `example.com`.",
		"The catalog file is `settings.json`, the key is `settings.section.advanced`.",
		"Nothing backticked here: settings.section.advanced.",
	}, "\n")

	got := scanDocForCitations("docs/i18n/de/glossary.md", content, messageKeyCitationLane, ix)
	if len(got) != 1 {
		t.Fatalf("expected exactly the one catalog-namespaced token, got %v", citationTokens(got))
	}
	if got[0].token != "settings.section.advanced" || got[0].line != 3 {
		t.Errorf("got %q at line %d, want `settings.section.advanced` at line 3", got[0].token, got[0].line)
	}
}

// The gate and the matcher have to compose: a token whose first segment isn't a
// namespace is never even considered, so the elided shorthand the guides use for
// a LIVE key (`mtp.tryAgain` for `fileExplorer.mtp.tryAgain`) can't be reported,
// while the same shorthand under a real namespace still resolves as a suffix.
func TestScanDocForCitationsComposesGateWithSuffixMatching(t *testing.T) {
	ix := testCitationIndex(testCitationKeys)
	content := strings.Join([]string{
		"The retry button is `network.tryAgain`.",
		"The guest row is `sheet.connectAsGuest`.",
		"The dead one is `mtp.permissionDialog.title`.",
	}, "\n")

	got := scanDocForCitations("docs/i18n/de/glossary.md", content, messageKeyCitationLane, ix)
	tokens := citationTokens(got)
	// `network.tryAgain` is gated out (`network` is no namespace), and
	// `sheet.connectAsGuest` is gated out for the same reason even though it
	// would have resolved. Only the `mtp`-namespaced token is a citation.
	if len(got) != 1 || strings.Join(tokens, ",") != "mtp.permissionDialog.title" {
		t.Fatalf("got %v, want only `mtp.permissionDialog.title`", tokens)
	}
	if ix.resolves(got[0].token) {
		t.Error("`mtp.permissionDialog.title` should not resolve against the test catalog")
	}
}

func TestNearestKeysPointsAtTheRenamedFamily(t *testing.T) {
	ix := testCitationIndex(testCitationKeys)

	cases := []struct {
		token string
		want  string
	}{
		{"fileExplorer.network.login.connectAsGuest", "servers.sheet.connectAsGuest"},
		{"fileExplorer.network.login.username", "servers.sheet.username"},
		{"fileExplorer.network.login.rememberInKeychain", "servers.sheet.remember"},
	}

	for _, tc := range cases {
		t.Run(tc.token, func(t *testing.T) {
			got := ix.nearestKeys(tc.token, 3)
			if len(got) == 0 || got[0] != tc.want {
				t.Errorf("nearestKeys(%q) = %v, want %q first", tc.token, got, tc.want)
			}
		})
	}
}

// A leaf that merely opens another one is not a match: `red` is the first three
// runes of `rememberInKeychain`, and ranking it as a near miss buries the rename
// the reader is looking for.
func TestNearestKeysIgnoresAccidentalShortPrefixes(t *testing.T) {
	ix := testCitationIndex(testCitationKeys)
	got := ix.nearestKeys("fileExplorer.network.login.rememberInKeychain", 3)
	for _, key := range got {
		if key == "menu.tag.red" {
			t.Errorf("`menu.tag.red` ranked as near `rememberInKeychain`: %v", got)
		}
	}
}

func TestShrinkwrapDocCitationAllowlistDropsFixedEntries(t *testing.T) {
	list := docCitationAllowlist{
		Retired: map[string]map[string]string{
			"docs/i18n/zh/glossary.md": {"askCmdr.consent.noContents": "quoted as the retired key it replaced"},
		},
		Pending: map[string]map[string]string{
			"docs/i18n/de/glossary.md": {
				"fileExplorer.network.login.title":    "renamed to servers.sheet.*",
				"fileExplorer.network.login.username": "already fixed by the time this runs",
			},
			"docs/i18n/gone/glossary.md": {"settings.section.old": "file deleted"},
		},
	}
	live := map[string]map[string]bool{
		"docs/i18n/zh/glossary.md": {"askCmdr.consent.noContents": true},
		"docs/i18n/de/glossary.md": {"fileExplorer.network.login.title": true},
	}

	changes := shrinkwrapDocCitationAllowlist(&list, live)

	if _, still := list.Pending["docs/i18n/de/glossary.md"]["fileExplorer.network.login.username"]; still {
		t.Error("a citation that no longer appears should be dropped")
	}
	if _, still := list.Pending["docs/i18n/de/glossary.md"]["fileExplorer.network.login.title"]; !still {
		t.Error("a citation still present should be kept")
	}
	if _, still := list.Pending["docs/i18n/gone/glossary.md"]; still {
		t.Error("a file with nothing left should be dropped entirely")
	}
	if len(list.Retired["docs/i18n/zh/glossary.md"]) != 1 {
		t.Error("a live retired entry should be kept")
	}
	if len(changes) != 2 {
		t.Errorf("expected 2 shrink-wrap changes, got %d: %v", len(changes), changes)
	}
}

// A locale's glossary split into `terms.json` + `decisions.md` carries its
// deliberate citations along: an entry whose doc stopped citing the key follows it
// to the successor doc when that one cites it, rather than being dropped and
// leaving the moved citation unexcused.
func TestShrinkwrapDocCitationAllowlistFollowsACitationToItsSuccessorDoc(t *testing.T) {
	list := docCitationAllowlist{
		Retired: map[string]map[string]string{
			"docs/i18n/de/glossary.md": {"askCmdr.consent.noContents": "carried over verbatim"},
			"docs/i18n/fr/glossary.md": {"askCmdr.consent.noContents": "carried over verbatim"},
		},
	}
	live := map[string]map[string]bool{
		"docs/i18n/de/decisions.md": {"askCmdr.consent.noContents": true},
	}

	changes := shrinkwrapDocCitationAllowlist(&list, live)

	if got := list.Retired["docs/i18n/de/decisions.md"]["askCmdr.consent.noContents"]; got != "carried over verbatim" {
		t.Errorf("the de entry should have moved to decisions.md with its reason, got %q", got)
	}
	if _, still := list.Retired["docs/i18n/de/glossary.md"]; still {
		t.Error("the de glossary.md entry should be gone after the move")
	}
	if _, still := list.Retired["docs/i18n/fr/glossary.md"]; still {
		t.Error("the fr entry, cited nowhere, should be dropped")
	}
	if dead := (docCitationAllowlist{Retired: list.Retired}).judge([]docCitation{
		{relPath: "docs/i18n/de/decisions.md", line: 3, token: "askCmdr.consent.noContents"},
	}); len(dead.reported) != 0 {
		t.Errorf("the moved entry should excuse the decisions.md citation: %+v", dead)
	}
	if len(changes) != 2 {
		t.Errorf("expected a move and a drop, got %d: %v", len(changes), changes)
	}
}

// The termbase files are guides too: a concept note or a term's `exceptions`
// reason cites keys in backticks exactly like the markdown does, so they're
// scanned; other JSON under `docs/i18n/` isn't a translator guide.
func TestScanDocsForCitationsReadsTheTermbaseJSON(t *testing.T) {
	root := t.TempDir()
	write := func(rel, content string) {
		path := filepath.Join(root, rel)
		if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(path, []byte(content), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	write("docs/i18n/concepts.json", "{\n  \"a\": { \"note\": \"see `servers.sheet.gone`\" }\n}\n")
	write("docs/i18n/nl/terms.json", "{\n  \"a\": {\n    \"note\": \"like `servers.sheet.remember`\"\n  }\n}\n")
	write("docs/i18n/nl/concepts-proposed.json", "{ \"b\": { \"sense\": \"`servers.sheet.password`\" } }\n")
	write("docs/i18n/reference-pile/inventory.json", "{ \"x\": \"`servers.sheet.nope`\" }\n")

	citations, docs, err := scanDocsForCitations(root, messageKeyCitationLane, testCitationIndex(testCitationKeys))
	if err != nil {
		t.Fatal(err)
	}
	got := map[string]string{}
	for _, c := range citations {
		got[c.token] = fmt.Sprintf("%s:%d", c.relPath, c.line)
	}
	want := map[string]string{
		"servers.sheet.gone":     "docs/i18n/concepts.json:2",
		"servers.sheet.remember": "docs/i18n/nl/terms.json:3",
		"servers.sheet.password": "docs/i18n/nl/concepts-proposed.json:1",
	}
	if !reflect.DeepEqual(got, want) || docs != 3 {
		t.Errorf("got %v across %d docs, want %v across 3", got, docs, want)
	}
}

// An entry with a blank reason silences nothing: the allowlist exists to get the
// judgement written down, and a blank one records no judgement.
func TestDocCitationAllowlistIgnoresBlankReasons(t *testing.T) {
	list := docCitationAllowlist{
		Retired: map[string]map[string]string{
			"docs/i18n/zh/glossary.md": {"askCmdr.consent.noContents": "  "},
		},
	}
	dead := []docCitation{{relPath: "docs/i18n/zh/glossary.md", line: 886, token: "askCmdr.consent.noContents"}}
	if verdict := list.judge(dead); len(verdict.reported) != 1 || verdict.retired != 0 {
		t.Errorf("a blank reason allowlisted a citation: %+v", verdict)
	}
}

// testCitationIndex builds an index the way the message-key lane does, so the
// tests exercise the real namespace rule rather than a hand-written set.
func testCitationIndex(keys []string) *dottedKeyIndex {
	return newDottedKeyIndex(keys, messageKeyCitationLane.namespaces(keys))
}

func citationTokens(citations []docCitation) []string {
	out := make([]string, 0, len(citations))
	for _, c := range citations {
		out = append(out, c.token)
	}
	return out
}
