package checks

import (
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

func TestCatalogKeyIndexMatchesSegmentAligned(t *testing.T) {
	ix := newCatalogKeyIndex(testCitationKeys)

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
func TestCatalogKeyIndexRejectsRawSubstringMatches(t *testing.T) {
	ix := newCatalogKeyIndex([]string{"settings.aiendpoint.label"})
	if ix.resolves("ai.endpoint") {
		t.Error("`ai.endpoint` matched `settings.aiendpoint.label` as a raw substring")
	}
}

func TestScanDocForCitationsGatesOnCatalogNamespaces(t *testing.T) {
	ix := newCatalogKeyIndex(testCitationKeys)
	content := strings.Join([]string{
		"Finder `LocalizableMerged.strings` `MR10.1` and NetAuthAgent `PHL-pS-ELV.title`.",
		"Dates render as `dd.MM.yyyy`, hosts as `example.com`.",
		"The catalog file is `settings.json`, the key is `settings.section.advanced`.",
		"Nothing backticked here: settings.section.advanced.",
	}, "\n")

	got := scanDocForCitations("docs/i18n/de/glossary.md", content, ix)
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
	ix := newCatalogKeyIndex(testCitationKeys)
	content := strings.Join([]string{
		"The retry button is `network.tryAgain`.",
		"The guest row is `sheet.connectAsGuest`.",
		"The dead one is `mtp.permissionDialog.title`.",
	}, "\n")

	got := scanDocForCitations("docs/i18n/de/glossary.md", content, ix)
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
	ix := newCatalogKeyIndex(testCitationKeys)

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
	ix := newCatalogKeyIndex(testCitationKeys)
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

func citationTokens(citations []docCitation) []string {
	out := make([]string, 0, len(citations))
	for _, c := range citations {
		out = append(out, c.token)
	}
	return out
}
