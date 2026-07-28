package checks

import (
	"strings"
	"testing"
)

func TestParseDenyAllowList(t *testing.T) {
	// Shaped like the real deny.toml: trailing comments, an unrelated array
	// before the one we want, and a following section that must terminate it.
	deny := `
[licenses]
version = 2
unused-allowed-license = "deny"
allow = [
    "MIT",
    "Apache-2.0",
    "LicenseRef-BSL-1.1",       # Our own crate
    "bzip2-1.0.6",              # permissive BSD-style
]

[bans]
multiple-versions = "allow"
`
	got := parseDenyAllowList(deny)
	want := []string{"MIT", "Apache-2.0", "LicenseRef-BSL-1.1", "bzip2-1.0.6"}

	if len(got) != len(want) {
		t.Fatalf("got %d ids %v, want %d %v", len(got), got, len(want), want)
	}
	for i := range want {
		if got[i] != want[i] {
			t.Errorf("id %d: got %q, want %q", i, got[i], want[i])
		}
	}
}

func TestParseDenyAllowListStopsAtTheClosingBracket(t *testing.T) {
	// A quoted string after the array must not be swept in; `[bans]` values
	// are not licenses.
	deny := `
allow = [
    "MIT",
]

[sources]
allow-git = ["https://github.com/vdavid/smb2"]
`
	got := parseDenyAllowList(deny)
	if len(got) != 1 || got[0] != "MIT" {
		t.Errorf("got %v, want exactly [MIT]", got)
	}
}

func TestParseDenyAllowListReturnsNothingWhenAbsent(t *testing.T) {
	// The caller turns this into a loud failure rather than generating a
	// notices file that silently credits nobody.
	if got := parseDenyAllowList("[bans]\nwildcards = \"deny\"\n"); len(got) != 0 {
		t.Errorf("got %v, want none", got)
	}
}

func TestSortPackagesIsCaseInsensitiveAndVersionStable(t *testing.T) {
	// Output has to be byte-stable or the check reports spurious drift.
	packages := []attributedPackage{
		{Name: "zlib", Version: "1.0"},
		{Name: "Inflector", Version: "0.11.4"},
		{Name: "adler2", Version: "2.0.1"},
		{Name: "Inflector", Version: "0.10.0"},
	}
	sortPackages(packages)

	want := []string{"adler2 2.0.1", "Inflector 0.10.0", "Inflector 0.11.4", "zlib 1.0"}
	for i, expected := range want {
		got := packages[i].Name + " " + packages[i].Version
		if got != expected {
			t.Errorf("position %d: got %q, want %q", i, got, expected)
		}
	}
}

func TestRenderNoticesIncludesEveryPackageAndText(t *testing.T) {
	rust := rustCollection{
		packages: []attributedPackage{
			{Name: "serde", Version: "1.0.228", License: "MIT OR Apache-2.0", URL: "https://github.com/serde-rs/serde"},
		},
		texts: []licenseText{
			{ID: "MIT", Text: "Copyright (c) Somebody\n\nPermission is hereby granted...", UsedBy: []string{"serde 1.0.228"}},
		},
	}
	npm := []attributedPackage{
		{Name: "@ark-ui/svelte", Version: "5.22.1", License: "MIT", URL: "https://ark-ui.com"},
	}

	out := string(renderNotices(rust, npm))

	for _, needle := range []string{
		"**serde** 1.0.228, MIT OR Apache-2.0, <https://github.com/serde-rs/serde>",
		"**@ark-ui/svelte** 5.22.1, MIT, <https://ark-ui.com>",
		"Copyright (c) Somebody",
		"Covers: serde 1.0.228",
		"- Rust crates: 1",
		"- npm packages: 1",
	} {
		if !strings.Contains(out, needle) {
			t.Errorf("rendered notices missing %q", needle)
		}
	}
}

func TestRenderNoticesOmitsTheUrlWhenUnknown(t *testing.T) {
	// One crate in the real graph has no repository; a bare `<>` would be a
	// dead link and `dead-links` would be right to complain.
	rust := rustCollection{packages: []attributedPackage{{Name: "mystery", Version: "1.0", License: "MIT"}}}
	out := string(renderNotices(rust, nil))

	if !strings.Contains(out, "- **mystery** 1.0, MIT\n") {
		t.Errorf("expected a URL-less entry, got:\n%s", out)
	}
	if strings.Contains(out, "<>") {
		t.Error("rendered an empty link target")
	}
}

func TestRenderNoticesIsDeterministic(t *testing.T) {
	rust := rustCollection{
		packages: []attributedPackage{{Name: "b", Version: "1", License: "MIT"}, {Name: "a", Version: "1", License: "MIT"}},
	}
	sortPackages(rust.packages)
	first := string(renderNotices(rust, nil))
	second := string(renderNotices(rust, nil))
	if first != second {
		t.Error("two renders of the same input differ")
	}
}

func TestFirstNonEmpty(t *testing.T) {
	if got := firstNonEmpty("", "second", "third"); got != "second" {
		t.Errorf("got %q, want %q", got, "second")
	}
	if got := firstNonEmpty("", ""); got != "" {
		t.Errorf("got %q, want empty", got)
	}
}
