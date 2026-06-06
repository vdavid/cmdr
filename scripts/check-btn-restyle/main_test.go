package main

import "testing"

func TestScanFlagsBtnPrimaryRestyle(t *testing.T) {
	svelte := `<style>
  .btn-primary {
    color: red;
    background: blue;
  }
</style>`
	vs, as, _ := scanFile("/root", "/root/apps/desktop/src/foo/Foo.svelte", svelte)
	if len(as) != 0 {
		t.Errorf("unexpected allowlist entries: %#v", as)
	}
	if len(vs) != 2 {
		t.Fatalf("want 2 violations (color + background); got %d: %#v", len(vs), vs)
	}
	if vs[0].Property != "color" || vs[1].Property != "background" {
		t.Errorf("unexpected props: %v / %v", vs[0].Property, vs[1].Property)
	}
}

func TestScanIgnoresLayoutOnly(t *testing.T) {
	svelte := `<style>
  .btn {
    flex: 1;
    max-width: 200px;
    margin: 0;
  }
</style>`
	vs, _, _ := scanFile("/root", "/root/x/Foo.svelte", svelte)
	if len(vs) != 0 {
		t.Errorf("layout-only override should pass, got: %#v", vs)
	}
}

func TestScanIgnoresSimilarlyNamedClasses(t *testing.T) {
	// `.btn-mini` and `.btn-regular` are size classes inside Button.svelte
	// itself, but if they appear in feature components they're not the
	// banned set.
	svelte := `<style>
  .btn-mini {
    color: red;
  }
  .btn-regular {
    background: blue;
  }
  .toggle-button {
    color: green;
  }
</style>`
	vs, _, _ := scanFile("/root", "/root/x/Foo.svelte", svelte)
	if len(vs) != 0 {
		t.Errorf("non-canonical class names should be ignored, got: %#v", vs)
	}
}

func TestScanRespectsAllowlistComment(t *testing.T) {
	svelte := `<style>
  /* allowed-btn-restyle: rendered inside the high-contrast pre-onboarding modal where we explicitly want the dimmed look */
  .btn-primary {
    color: red;
  }
</style>`
	vs, as, _ := scanFile("/root", "/root/x/Foo.svelte", svelte)
	if len(vs) != 0 {
		t.Errorf("allowlisted rule should not violate, got: %#v", vs)
	}
	if len(as) != 1 {
		t.Fatalf("want 1 allowlist entry, got %d", len(as))
	}
	if !contains(as[0].Rationale, "high-contrast pre-onboarding") {
		t.Errorf("rationale not captured: %q", as[0].Rationale)
	}
}

func TestScanEmptyRationaleNotAllowed(t *testing.T) {
	// Empty rationale defeats the purpose; treat as no allowlist comment.
	svelte := `<style>
  /* allowed-btn-restyle: */
  .btn-primary {
    color: red;
  }
</style>`
	vs, _, _ := scanFile("/root", "/root/x/Foo.svelte", svelte)
	if len(vs) != 1 {
		t.Errorf("empty-rationale comment shouldn't allowlist; got %d violations", len(vs))
	}
}

func TestScanAllowsCanonicalButtonFile(t *testing.T) {
	// The walker in main() filters by filename; scanFile is the inner
	// primitive and doesn't know about Button.svelte. The test confirms
	// the helper's contract: it surfaces violations regardless of path.
	// The file-level skip is exercised by run() under integration.
	svelte := `<style>
  .btn-primary { color: red; }
</style>`
	vs, _, _ := scanFile("/root", "/root/x/Button.svelte", svelte)
	if len(vs) != 1 {
		t.Errorf("scanFile should flag regardless of path; got %d", len(vs))
	}
}

func TestScanDescendsIntoMediaBlocks(t *testing.T) {
	svelte := `<style>
  @media (prefers-color-scheme: dark) {
    .btn-primary {
      color: red;
    }
  }
</style>`
	// Note: the current walker bumps depth on `@`-rule braces but doesn't
	// re-scan their inner rules for selector matches. This test pins
	// today's behavior (nested rules NOT flagged). If we extend coverage
	// later, update this test.
	vs, _, _ := scanFile("/root", "/root/x/Foo.svelte", svelte)
	if len(vs) != 0 {
		t.Errorf("nested @media rules currently not scanned; got %d", len(vs))
	}
}

func contains(s, sub string) bool {
	for i := 0; i+len(sub) <= len(s); i++ {
		if s[i:i+len(sub)] == sub {
			return true
		}
	}
	return false
}

func TestScanFlagsOrphanCommentOnNonBtnRule(t *testing.T) {
	svelte := `<style>
  /* allowed-btn-restyle: stale, this rule no longer touches a btn class */
  .toggle-button {
    color: red;
  }
</style>`
	vs, _, orphans := scanFile("/root", "/root/x/Foo.svelte", svelte)
	if len(vs) != 0 {
		t.Errorf("non-btn rule should not violate, got: %#v", vs)
	}
	if len(orphans) != 1 {
		t.Fatalf("want 1 orphaned allowlist comment, got %d: %#v", len(orphans), orphans)
	}
	if orphans[0].Line != 2 {
		t.Errorf("want orphan at line 2, got %d", orphans[0].Line)
	}
}

func TestScanFlagsOrphanCommentOnLayoutOnlyBtnRule(t *testing.T) {
	// Layout-only overrides aren't flagged at all, so they don't need (or
	// consume) a rationale.
	svelte := `<style>
  /* allowed-btn-restyle: stale, the color override below was removed */
  .btn-primary {
    flex: 1;
  }
</style>`
	_, _, orphans := scanFile("/root", "/root/x/Foo.svelte", svelte)
	if len(orphans) != 1 {
		t.Fatalf("want 1 orphaned allowlist comment, got %d: %#v", len(orphans), orphans)
	}
}

func TestScanUsedAllowlistCommentIsNotOrphan(t *testing.T) {
	svelte := `<style>
  /* allowed-btn-restyle: intentionally dimmed in the pre-onboarding modal */
  .btn-primary {
    color: red;
  }
</style>`
	vs, as, orphans := scanFile("/root", "/root/x/Foo.svelte", svelte)
	if len(vs) != 0 || len(as) != 1 {
		t.Fatalf("expected the comment to excuse the rule (vs=%d, as=%d)", len(vs), len(as))
	}
	if len(orphans) != 0 {
		t.Errorf("used allowlist comment should not be an orphan, got: %#v", orphans)
	}
}
