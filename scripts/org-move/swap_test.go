package main

import (
	"strings"
	"testing"
)

func TestSwapContent(t *testing.T) {
	cases := []struct {
		name, in, want string
		swapped        int
	}{
		{"web URL", "see https://github.com/vdavid/cmdr/issues/12.", "see https://github.com/getcmdr/cmdr/issues/12.", 1},
		{"API URL", "fetch('https://api.github.com/repos/vdavid/cmdr/releases')", "fetch('https://api.github.com/repos/getcmdr/cmdr/releases')", 1},
		{"gh api path", "`gh api repos/vdavid/cmdr/hooks`", "`gh api repos/getcmdr/cmdr/hooks`", 1},
		{"gh -R", "gh release list -R vdavid/cmdr", "gh release list -R getcmdr/cmdr", 1},
		{"gh --repo=", "scorecard --repo=github.com/vdavid/cmdr --show-details", "scorecard --repo=github.com/getcmdr/cmdr --show-details", 1},
		{"issue ref", "(vdavid/cmdr#605)", "(getcmdr/cmdr#605)", 1},
		{"bare id in backticks", "lives on `vdavid/cmdr`, and", "lives on `getcmdr/cmdr`, and", 1},
		{"line start", "vdavid/cmdr: 300", "getcmdr/cmdr: 300", 1},
		{"clone URL", "git clone https://github.com/vdavid/cmdr.git /opt/cmdr", "git clone https://github.com/getcmdr/cmdr.git /opt/cmdr", 1},
		{"SSH remote", "git@github.com:vdavid/cmdr.git", "git@github.com:getcmdr/cmdr.git", 1},
		{"Scorecard badge", "api.scorecard.dev/projects/github.com/vdavid/cmdr/badge", "api.scorecard.dev/projects/github.com/getcmdr/cmdr/badge", 1},
		{"runner service name", "look for `actions.runner.vdavid-cmdr.*`", "look for `actions.runner.getcmdr-cmdr.*`", 1},
		{"demo image", `ref="ghcr.io/vdavid/cmdr-demo:$TAG"`, `ref="ghcr.io/getcmdr/cmdr-demo:$TAG"`, 1},
		{"E2E base image", "ghcr.io/vdavid/cmdr-e2e-base:3f2a", "ghcr.io/getcmdr/cmdr-e2e-base:3f2a", 1},
		{"owner-level registry", "(E2E_BASE_REGISTRY, for example ghcr.io/vdavid) under", "(E2E_BASE_REGISTRY, for example ghcr.io/getcmdr) under", 1},
		{"demo package settings", "github.com/users/vdavid/packages/container/cmdr-demo/settings", "github.com/orgs/getcmdr/packages/container/cmdr-demo/settings", 1},
		{"regex-escaped API path", `/^\/repos\/vdavid\/cmdr\/actions\/runs$/`, `/^\/repos\/getcmdr\/cmdr\/actions\/runs$/`, 1},
		{"regex-escaped other repo", `/^\/repos\/vdavid\/cmdr-reports\/issues$/`, `/^\/repos\/vdavid\/cmdr-reports\/issues$/`, 0},
		{"other repo: reports", "vdavid/cmdr-reports#55", "vdavid/cmdr-reports#55", 0},
		{"other repo: fsevent fork", "https://github.com/vdavid/cmdr-fsevent-stream", "https://github.com/vdavid/cmdr-fsevent-stream", 0},
		{"local path", "~/projects-git/vdavid/cmdr/target", "~/projects-git/vdavid/cmdr/target", 0},
		{"elided local path", "`…/vdavid/cmdr/…`", "`…/vdavid/cmdr/…`", 0},
		{"lookalike host", "https://evil.example/vdavid/cmdr/x", "https://evil.example/vdavid/cmdr/x", 0},
		{"profile link", "https://github.com/vdavid", "https://github.com/vdavid", 0},
		{"tap", "brew install --cask vdavid/tap/cmdr", "brew install --cask vdavid/tap/cmdr", 0},
		{"advisory link keeps the old name", "url: 'https://github.com/vdavid/cmdr/security/advisories/GHSA-aaaa-bbbb-cccc'", "url: 'https://github.com/vdavid/cmdr/security/advisories/GHSA-aaaa-bbbb-cccc'", 0},
		{"report-a-vulnerability link moves", "https://github.com/vdavid/cmdr/security/advisories/new", "https://github.com/getcmdr/cmdr/security/advisories/new", 1},
		{"pre-reusable-workflow signer", "`-signer-workflow vdavid/cmdr/.github/workflows/release.yml`", "`-signer-workflow vdavid/cmdr/.github/workflows/release.yml`", 0},
		{"current signer moves", "`--signer-workflow vdavid/cmdr/.github/workflows/release-pipeline.yml`", "`--signer-workflow getcmdr/cmdr/.github/workflows/release-pipeline.yml`", 1},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			got, res := swapContent(c.in, nil)
			if got != c.want {
				t.Errorf("got  %q\nwant %q", got, c.want)
			}
			if res.swapped != c.swapped {
				t.Errorf("swapped %d, want %d", res.swapped, c.swapped)
			}
			again, res2 := swapContent(got, nil)
			if again != got || res2.swapped != 0 {
				t.Errorf("not idempotent: second pass gave %q (%d swapped)", again, res2.swapped)
			}
		})
	}
}

func TestSwapContentReportsWhatItKeeps(t *testing.T) {
	_, res := swapContent("a ~/projects-git/vdavid/cmdr b https://github.com/vdavid/cmdr/security/advisories/GHSA-x", nil)
	if res.kept[keptLocalPath] != 1 {
		t.Errorf("local path kept %d, want 1 (%v)", res.kept[keptLocalPath], res.kept)
	}
	if len(res.kept) != 2 {
		t.Errorf("want two keep reasons, got %v", res.kept)
	}
}

func TestProtectedSpansKeepTheOldName(t *testing.T) {
	text := "verify with --repo getcmdr/cmdr, or --repo vdavid/cmdr for releases up to v0.55.0."
	got, res := swapContent(text, []span{{start: strings.Index(text, "or --repo"), end: len(text), why: "dual-name text"}})
	if got != text {
		t.Errorf("protected text changed: %q", got)
	}
	if res.kept["dual-name text"] != 1 {
		t.Errorf("kept %v", res.kept)
	}
}

func TestApplyEdit(t *testing.T) {
	e := edit{
		files:   []string{"x.md"},
		find:    "verify with\n`--repo vdavid/cmdr`.",
		replace: "verify with `--repo getcmdr/cmdr`, or `--repo vdavid/cmdr` up to and including {cutoff}.",
		why:     "both names",
	}
	in := "Then verify with\n  `--repo vdavid/cmdr`. Done."
	out, status := applyEdit(in, e, "v0.55.0")
	if status != editApplied {
		t.Fatalf("status %v", status)
	}
	want := "Then verify with `--repo getcmdr/cmdr`, or `--repo vdavid/cmdr` up to and including v0.55.0. Done."
	if out != want {
		t.Fatalf("got  %q\nwant %q", out, want)
	}
	spans := editSpans(out, e, "v0.55.0")
	if len(spans) != 1 {
		t.Fatalf("spans %v", spans)
	}
	swapped, res := swapContent(out, spans)
	if swapped != out || res.swapped != 0 {
		t.Errorf("the generic swap touched the edit's own text: %q", swapped)
	}

	// A formatter rewrapping the inserted text doesn't make the edit apply twice.
	rewrapped := strings.Replace(out, "or `--repo", "or\n`--repo", 1)
	out2, status2 := applyEdit(rewrapped, e, "v0.55.0")
	if status2 != editAlreadyApplied || out2 != rewrapped || len(editSpans(out2, e, "v0.55.0")) != 1 {
		t.Errorf("second run: status %v, text %q", status2, out2)
	}

	if _, status3 := applyEdit("nothing here", e, "v0.55.0"); status3 != editAnchorMissing {
		t.Errorf("missing anchor: status %v", status3)
	}
}

// A later edit earlier in the file shifts every offset after it; the protected spans must
// still cover the first edit's text.
func TestProcessFileProtectsEveryEditAfterAllOfThem(t *testing.T) {
	saved := edits
	t.Cleanup(func() { edits = saved })
	edits = []edit{
		{files: []string{"a.md"}, find: "TAIL", replace: "check `--repo vdavid/cmdr` up to {cutoff}", why: "late"},
		{files: []string{"a.md"}, find: "HEAD", replace: "a much longer head line than before", why: "early"},
	}
	r := report{changed: map[string]int{}, kept: map[string]map[string]int{}, unclassified: map[string]int{}}
	out := r.processFile("a.md", "HEAD then https://github.com/vdavid/cmdr then TAIL", "v0.55.0")
	want := "a much longer head line than before then https://github.com/getcmdr/cmdr then check `--repo vdavid/cmdr` up to v0.55.0"
	if out != want {
		t.Fatalf("got  %q\nwant %q", out, want)
	}
	r2 := report{changed: map[string]int{}, kept: map[string]map[string]int{}, unclassified: map[string]int{}}
	if again := r2.processFile("a.md", out, "v0.55.0"); again != out {
		t.Errorf("second run changed it: %q", again)
	}
	for _, e := range r2.edits {
		if e.status != editAlreadyApplied {
			t.Errorf("second run: %s is %v", e.why, e.status)
		}
	}
}

func TestTheToolExcludesItself(t *testing.T) {
	for _, p := range []string{"scripts/org-move.sh", "scripts/org-move/rules.go"} {
		if _, ok := exclusionFor(p); !ok {
			t.Errorf("%s isn't excluded", p)
		}
	}
}

func TestGlobs(t *testing.T) {
	cases := []struct {
		glob, path string
		want       bool
	}{
		{"**/*.md", "README.md", true},
		{"**/*.md", "docs/guides/releasing.md", true},
		{"**/*.md", "docs/guides/releasing.mdx", false},
		{"docs/notes/**", "docs/notes/a.md", true},
		{"docs/notes/**", "docs/notes/performance/b.md", true},
		{"docs/notes/**", "docs/notesy.md", false},
		{"CHANGELOG.md", "CHANGELOG.md", true},
		{"CHANGELOG.md", "vendor/zip/CHANGELOG.md", false},
		{"scripts/check/checks/desktop-security-advisories*.go", "scripts/check/checks/desktop-security-advisories_test.go", true},
		{".github/**", ".github/workflows/ci.yml", true},
	}
	for _, c := range cases {
		if got := globMatch(c.glob, c.path); got != c.want {
			t.Errorf("globMatch(%q, %q) = %v, want %v", c.glob, c.path, got, c.want)
		}
	}
}

func TestRulesAreWellFormed(t *testing.T) {
	for _, x := range excludedPaths {
		if x.why == "" {
			t.Errorf("exclusion %q has no reason", x.glob)
		}
	}
	for _, k := range keepPatterns {
		if k.why == "" {
			t.Errorf("keep pattern %q has no reason", k.re)
		}
	}
	for _, e := range edits {
		if e.why == "" || len(e.files) == 0 || e.find == "" || e.replace == "" {
			t.Errorf("incomplete edit %+v", e)
		}
		if flexRE(e.find).MatchString(e.replace) {
			t.Errorf("edit's replacement contains its anchor, so it would re-apply: %q", e.find)
		}
	}
}
