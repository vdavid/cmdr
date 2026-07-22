package checks

import (
	"strings"
	"testing"
)

func TestParseTransUsagesReadsTheShapesTheAppActuallyUses(t *testing.T) {
	source := `<script lang="ts">
    import Trans from '$lib/intl/Trans.svelte'
</script>

<p><Trans key="a.simple" snippets={{ dir }} /></p>
<p><Trans key="b.renamed" snippets={{ kindName: kindSnippet, quickLookKey, openKey }} /></p>
<p><Trans key="c.withParams" params={{ name: currentDirName }} snippets={{ dir }} /></p>
<p>
    <Trans
        key="d.multiline"
        snippets={{ req: pathCode, land: pathCode }}
    />
</p>
<p><Trans key="e.noSnippets" params={{ count }} /></p>
<p><Trans key={messageKey} snippets={{ settingsLink }} /></p>
<p><Trans key="f.shorthandSnippets" {snippets} /></p>
<p><Trans key="g.snippetsVariable" snippets={builtElsewhere} /></p>
`

	usages, unresolvable := parseTransUsages(source)
	// The computed key, the `{snippets}` shorthand, and the variable snippets prop
	// are all unresolvable here. Guessing at them would produce false positives,
	// which would cost this check its only real asset: that a failure means a bug.
	if unresolvable != 3 {
		t.Errorf("expected 3 unresolvable usages, got %d", unresolvable)
	}
	if len(usages) != 5 {
		t.Fatalf("expected 5 literal-key usages, got %d: %+v", len(usages), usages)
	}

	want := map[string]string{
		"a.simple":     "dir",
		"b.renamed":    "kindName,openKey,quickLookKey",
		"c.withParams": "dir",
		"d.multiline":  "land,req",
		"e.noSnippets": "",
	}
	for _, u := range usages {
		got := strings.Join(u.snippets, ",")
		if want[u.key] != got {
			t.Errorf("%s: snippets = %q, want %q", u.key, got, want[u.key])
		}
	}
}

func TestMessageTagsIgnoresParamsAndClosingTags(t *testing.T) {
	cases := []struct {
		message string
		want    string
	}{
		{"Actual <kindName>{kind}</kindName> instead.", "kindName"},
		{"Press <quickLookKey></quickLookKey> or <openKey></openKey>.", "openKey,quickLookKey"},
		{"Moved {count} files.", ""},
		{"A <b>bold</b> {thing} and <i>italic</i>.", "b,i"},
		// A self-closing tag still needs a snippet.
		{"Restart <icon/> now.", "icon"},
	}
	for _, tc := range cases {
		got := strings.Join(messageTags(tc.message), ",")
		if got != tc.want {
			t.Errorf("messageTags(%q) = %q, want %q", tc.message, got, tc.want)
		}
	}
}

func TestDiffNamesReportsBothDirections(t *testing.T) {
	// A tag with no snippet is the dangerous one: `Trans` renders NOTHING for it,
	// so the tag's inner text silently vanishes from the UI.
	missing, extra := diffNames([]string{"kindName", "openKey"}, []string{"kind", "openKey"})
	if strings.Join(missing, ",") != "kindName" {
		t.Errorf("missing = %v, want [kindName]", missing)
	}
	// A snippet with no tag is dead weight, and usually the other half of a rename.
	if strings.Join(extra, ",") != "kind" {
		t.Errorf("extra = %v, want [kind]", extra)
	}

	missing, extra = diffNames([]string{"a"}, []string{"a"})
	if len(missing) != 0 || len(extra) != 0 {
		t.Errorf("expected no drift, got missing=%v extra=%v", missing, extra)
	}
}

// The exact regression this check exists for: renaming a catalog tag without
// renaming the component's snippet key. Both halves of the rename are reported.
func TestARenamedTagWithoutItsSnippetIsCaught(t *testing.T) {
	missing, extra := diffNames(messageTags("view the actual <kindName>{kind}</kindName>"), []string{"kind"})
	if len(missing) != 1 || missing[0] != "kindName" {
		t.Errorf("expected kindName reported missing, got %v", missing)
	}
	if len(extra) != 1 || extra[0] != "kind" {
		t.Errorf("expected the stale kind snippet reported, got %v", extra)
	}
}
