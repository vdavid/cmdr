package checks

import "testing"

// testDynamicPrefixes stands in for the real allowlist so the pure logic can be
// exercised independently of the production prefix set.
var testDynamicPrefixes = []string{"errors.git.", "errors.listing."}

func TestFindUnusedMessageKeys_FlagsKeyWithNoMention(t *testing.T) {
	catalog := []string{"common.cancel", "settings.fsWatch.title", "menu.open.never"}
	// Only the first two literals appear anywhere in source.
	sources := []string{
		`const x = t('common.cancel')`,
		`<button>{t('settings.fsWatch.title')}</button>`,
	}
	unused := findUnusedMessageKeys(catalog, sources, nil)
	if len(unused) != 1 || unused[0] != "menu.open.never" {
		t.Fatalf("got %#v, want exactly [menu.open.never]", unused)
	}
}

func TestFindUnusedMessageKeys_AllReferencedIsClean(t *testing.T) {
	catalog := []string{"common.cancel", "common.ok"}
	sources := []string{`t('common.cancel'); t('common.ok')`}
	if unused := findUnusedMessageKeys(catalog, sources, nil); len(unused) != 0 {
		t.Fatalf("got %#v, want none", unused)
	}
}

func TestFindUnusedMessageKeys_CountsIndirectMention(t *testing.T) {
	// A key reached through a registry map (its literal stored in a `*Key` field)
	// or any other indirection still has its literal text in source, so a
	// substring mention counts as a reference. Mirrors the codegen's dead-key
	// suppression (findCatalogKeyMentions).
	catalog := []string{"settings.tint.red"}
	sources := []string{`const tintKeys = { red: 'settings.tint.red' }`}
	if unused := findUnusedMessageKeys(catalog, sources, nil); len(unused) != 0 {
		t.Fatalf("got %#v, want none (indirect mention counts)", unused)
	}
}

func TestFindUnusedMessageKeys_AllowlistedDynamicPrefixNotFlagged(t *testing.T) {
	// Keys built at runtime never appear as a literal, so a naive scan would flag
	// them. An allowlisted dynamic prefix treats every key under it as referenced.
	catalog := []string{
		"errors.git.notARepo.title",
		"errors.listing.notFound.title",
		"menu.open.never", // not under any allowlisted prefix → still flagged
	}
	sources := []string{`const noop = true`}
	unused := findUnusedMessageKeys(catalog, sources, testDynamicPrefixes)
	if len(unused) != 1 || unused[0] != "menu.open.never" {
		t.Fatalf("got %#v, want exactly [menu.open.never]", unused)
	}
}

func TestFindUnusedMessageKeys_ResultIsSorted(t *testing.T) {
	catalog := []string{"zeta.b.leaf", "alpha.a.leaf", "mid.c.leaf"}
	unused := findUnusedMessageKeys(catalog, []string{`noop`}, nil)
	want := []string{"alpha.a.leaf", "mid.c.leaf", "zeta.b.leaf"}
	if len(unused) != len(want) {
		t.Fatalf("got %#v, want %#v", unused, want)
	}
	for i := range want {
		if unused[i] != want[i] {
			t.Fatalf("got %#v, want sorted %#v", unused, want)
		}
	}
}

// Guards against a prefix that no longer corresponds to any catalog key: a stale
// allowlist entry must surface so the dynamic-prefix list stays honest (each
// entry tied to a real runtime-construction site that still has keys).
func TestUnusedKeyPrefixCoverage_ReportsStaleEntry(t *testing.T) {
	catalog := []string{"errors.git.notARepo.title"}
	prefixes := []string{"errors.git.", "errors.gone."}
	stale := unusedDynamicPrefixesWithoutKeys(catalog, prefixes)
	if len(stale) != 1 || stale[0] != "errors.gone." {
		t.Fatalf("got %#v, want exactly [errors.gone.]", stale)
	}
}

func TestUnusedKeyPrefixCoverage_AllPrefixesCovered(t *testing.T) {
	catalog := []string{"errors.git.notARepo.title", "errors.listing.notFound.title"}
	if stale := unusedDynamicPrefixesWithoutKeys(catalog, testDynamicPrefixes); len(stale) != 0 {
		t.Fatalf("got %#v, want none", stale)
	}
}
