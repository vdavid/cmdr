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

func TestCrateRelativeSourceStripsTheRegistryPrefix(t *testing.T) {
	// The whole point: two machines must render the same line. An absolute path
	// would bake in a home directory and a registry index hash, so the file
	// could never match between a Mac and the Linux runner.
	cases := []struct {
		name  string
		given string
		want  string
	}{
		{
			name:  "registry crate",
			given: "/Users/someone/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/miniz_oxide-0.8.9/LICENSE-MIT.md",
			want:  "LICENSE-MIT.md",
		},
		{
			name:  "vendored file nested inside a registry crate",
			given: "/home/runner/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/libmimalloc-sys-0.1.49/c_src/mimalloc/v2/LICENSE",
			want:  "c_src/mimalloc/v2/LICENSE",
		},
		{
			name:  "workspace crate resolves against the repo instead",
			given: "/repo/crates/fsevent-stream/LICENSE",
			want:  "crates/fsevent-stream/LICENSE",
		},
		{
			name:  "a clarified crate already reports crate-relative",
			given: "LICENSE.txt",
			want:  "LICENSE.txt",
		},
		{
			name:  "no source file at all",
			given: "",
			want:  "",
		},
	}
	for _, testCase := range cases {
		t.Run(testCase.name, func(t *testing.T) {
			if got := crateRelativeSource(testCase.given, "/repo"); got != testCase.want {
				t.Errorf("got %q, want %q", got, testCase.want)
			}
		})
	}
}

func TestCrateRelativeSourceFallsBackToTheFileName(t *testing.T) {
	// Somewhere neither under the repo nor in a registry: still no absolute
	// path in the generated file.
	if got := crateRelativeSource("/somewhere/else/LICENSE", "/repo"); got != "LICENSE" {
		t.Errorf("got %q, want %q", got, "LICENSE")
	}
}

func TestVerifyClarificationsAcceptsThePinnedFiles(t *testing.T) {
	sources := map[string][]string{}
	for _, clarification := range licenseClarifications {
		for _, file := range clarification.files {
			sources[clarification.crate] = append(sources[clarification.crate], file.path)
		}
	}
	if err := verifyClarifications(sources); err != nil {
		t.Errorf("the real pins should verify against their own files, got: %v", err)
	}
}

func TestVerifyClarificationsCatchesASilentFallBackToScanning(t *testing.T) {
	// What a stale checksum looks like from here: cargo-about warned, exited 0,
	// scanned the crate, and picked a different file. Left unchecked, the
	// notices file goes back to differing between macOS and CI.
	sources := map[string][]string{}
	for _, clarification := range licenseClarifications {
		for _, file := range clarification.files {
			sources[clarification.crate] = append(sources[clarification.crate], file.path)
		}
	}
	sources["miniz_oxide"] = []string{"LICENSE-MIT.md"}

	err := verifyClarifications(sources)
	if err == nil {
		t.Fatal("expected a failure when a pinned crate's text came from another file")
	}
	if !strings.Contains(err.Error(), "miniz_oxide") {
		t.Errorf("the failure should name the crate to fix, got: %v", err)
	}
}

func TestVerifyClarificationsCatchesACrateThatDroppedAFile(t *testing.T) {
	// libmimalloc-sys pins two files because two copyright holders ship code in
	// the binary. Crediting only one of them is a regression, not a rounding.
	sources := map[string][]string{
		"libmimalloc-sys": {"LICENSE.txt"},
		"miniz_oxide":     {"LICENSE"},
	}
	if err := verifyClarifications(sources); err == nil {
		t.Error("expected a failure when a pinned file stopped contributing a text")
	}
}

func TestRenderClarificationsCarriesEveryPinnedChecksum(t *testing.T) {
	// A file with no checksum would be written without one, and cargo-about
	// rejects that outright — but silently missing a checksum in the map is the
	// kind of thing a rename does quietly.
	rendered := renderClarifications()
	for _, clarification := range licenseClarifications {
		for _, file := range clarification.files {
			checksum, ok := clarificationChecksums[clarification.crate+"/"+file.path]
			if !ok || checksum == "" {
				t.Errorf("no checksum pinned for %s/%s", clarification.crate, file.path)
				continue
			}
			if !strings.Contains(rendered, checksum) {
				t.Errorf("rendered config omits the checksum for %s/%s", clarification.crate, file.path)
			}
		}
		if !strings.Contains(rendered, "["+clarification.crate+".clarify]") {
			t.Errorf("rendered config omits the [%s.clarify] section", clarification.crate)
		}
	}
}

func TestRenderNoticesNamesTheFileEachTextCameFrom(t *testing.T) {
	rust := rustCollection{
		texts: []licenseText{
			{ID: "MIT", Text: "Copyright", SourcePath: "c_src/mimalloc/v2/LICENSE", UsedBy: []string{"libmimalloc-sys 0.1.49"}},
			{ID: "MIT", Text: "Other", UsedBy: []string{"synthesized 1.0"}},
		},
	}
	out := string(renderNotices(rust, nil))

	if !strings.Contains(out, "Text from: `c_src/mimalloc/v2/LICENSE`") {
		t.Errorf("expected the source file to be named, got:\n%s", out)
	}
	// A text cargo-about synthesized rather than read has no file to name.
	if strings.Contains(out, "Text from: ``") {
		t.Error("rendered an empty source path")
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
