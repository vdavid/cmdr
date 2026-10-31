package checks

import (
	"strings"
	"testing"
)

func TestSecurityAdvisoryProblems(t *testing.T) {
	const ghsa = "https://github.com/vdavid/cmdr/security/advisories/GHSA-aaaa-bbbb-cccc"
	entry := func(id, fixedIn, severity, url string) string {
		return `{"id":"` + id + `","fixedIn":"` + fixedIn + `","severity":"` + severity + `","url":"` + url + `"}`
	}
	list := func(entries ...string) string {
		return `{"advisories":[` + strings.Join(entries, ",") + `]}`
	}

	cases := []struct {
		name    string
		json    string
		wantErr string // "" = valid
	}{
		{name: "empty list", json: `{"advisories":[]}`},
		{name: "valid entries", json: list(
			entry("GHSA-aaaa-bbbb-cccc", "0.52.0", "high", ghsa),
			entry("2026-10-16-example", "0.53.0", "low", "https://getcmdr.com/changelog#v0.53.0"),
			entry("2026-10-16-prerelease", "0.53.0-rc.1", "critical", "https://getcmdr.com/changelog#v0.53.0-rc.1"),
			entry("GHSA-dddd-eeee-ffff", "0.52.0", "medium", "https://github.com/getcmdr/cmdr/security/advisories/GHSA-dddd-eeee-ffff"),
		)},
		{name: "another repo in the org", json: list(entry("a", "0.52.0", "low", "https://github.com/getcmdr/evil/security/advisories/GHSA-x")), wantErr: "allowed"},
		{name: "org advisories index, no id", json: list(entry("a", "0.52.0", "low", "https://github.com/getcmdr/cmdr/security/advisories/")), wantErr: "allowed"},
		{name: "not json", json: `{`, wantErr: "doesn't parse"},
		{name: "a bare array", json: `[]`, wantErr: "doesn't parse"},
		{name: "a top-level key the app refuses", json: `{"advisories":[],"extra":1}`, wantErr: "doesn't parse"},
		{name: "a summary field (dropped from the schema)", json: `{"advisories":[{"id":"a","fixedIn":"0.52.0","severity":"low","url":"https://getcmdr.com/changelog","summary":"x"}]}`, wantErr: "doesn't parse"},
		{name: "a missing field", json: `{"advisories":[{"id":"a","fixedIn":"0.52.0","severity":"low"}]}`, wantErr: "url"},
		{name: "an empty id", json: list(entry("", "0.52.0", "low", ghsa)), wantErr: "id"},
		{name: "a duplicate id", json: list(entry("dup", "0.52.0", "low", ghsa), entry("dup", "0.53.0", "high", ghsa)), wantErr: "duplicate"},
		{name: "fixedIn not semver", json: list(entry("a", "0.52", "low", ghsa)), wantErr: "semver"},
		{name: "fixedIn with a v prefix", json: list(entry("a", "v0.52.0", "low", ghsa)), wantErr: "semver"},
		{name: "fixedIn newer than the app", json: list(entry("a", "0.54.0", "low", ghsa)), wantErr: "newer than the app"},
		{name: "fixedIn a pre-release of the app version is fine", json: list(entry("a", "0.53.0-rc.1", "low", ghsa))},
		{name: "unknown severity", json: list(entry("a", "0.52.0", "severe", ghsa)), wantErr: "severity"},
		{name: "capitalized severity", json: list(entry("a", "0.52.0", "High", ghsa)), wantErr: "severity"},
		{name: "changelog link with no anchor", json: list(entry("a", "0.52.0", "low", "https://getcmdr.com/changelog")), wantErr: "#v0.52.0"},
		{name: "changelog anchor of another release", json: list(entry("a", "0.52.0", "low", "https://getcmdr.com/changelog#v0.53.0")), wantErr: "#v0.52.0"},
		{name: "changelog anchor without the v", json: list(entry("a", "0.52.0", "low", "https://getcmdr.com/changelog#0.52.0")), wantErr: "#v0.52.0"},
		{name: "changelog anchor below the page", json: list(entry("a", "0.52.0", "low", "https://getcmdr.com/changelog/#v0.52.0")), wantErr: "#v0.52.0"},
		{name: "http link", json: list(entry("a", "0.52.0", "low", "http://getcmdr.com/changelog")), wantErr: "allowed"},
		{name: "lookalike host", json: list(entry("a", "0.52.0", "low", "https://github.com.evil.example/vdavid/cmdr/security/advisories/GHSA-x")), wantErr: "allowed"},
		{name: "userinfo trick", json: list(entry("a", "0.52.0", "low", "https://github.com@evil.example/vdavid/cmdr/security/advisories/GHSA-x")), wantErr: "allowed"},
		{name: "advisories index, no id", json: list(entry("a", "0.52.0", "low", "https://github.com/vdavid/cmdr/security/advisories/")), wantErr: "allowed"},
		{name: "changelog lookalike path", json: list(entry("a", "0.52.0", "low", "https://getcmdr.com/changelog-evil")), wantErr: "allowed"},
		{name: "dot-dot path", json: list(entry("a", "0.52.0", "low", "https://github.com/vdavid/cmdr/security/advisories/../../../evil")), wantErr: "allowed"},
		{name: "explicit port", json: list(entry("a", "0.52.0", "low", "https://getcmdr.com:443/changelog")), wantErr: "allowed"},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			problems := securityAdvisoryProblems([]byte(tc.json), "0.53.0")
			joined := strings.Join(problems, "\n")
			if tc.wantErr == "" {
				if len(problems) != 0 {
					t.Fatalf("want valid, got problems:\n%s", joined)
				}
				return
			}
			if !strings.Contains(joined, tc.wantErr) {
				t.Fatalf("want a problem mentioning %q, got:\n%s", tc.wantErr, joined)
			}
		})
	}
}

func TestSecuritySemverOrdering(t *testing.T) {
	for _, tc := range []struct {
		a, b string
		want int
	}{
		{"0.53.0", "0.53.0", 0},
		{"0.9.0", "0.10.0", -1},
		{"1.0.0", "0.99.99", 1},
		{"0.53.0-rc.1", "0.53.0", -1},
		{"0.53.0-rc.2", "0.53.0-rc.10", -1},
		{"0.53.0-alpha", "0.53.0-alpha.1", -1},
		{"0.53.0-1", "0.53.0-alpha", -1},
	} {
		a, okA := parseStrictSemver(tc.a)
		b, okB := parseStrictSemver(tc.b)
		if !okA || !okB {
			t.Fatalf("%s or %s didn't parse", tc.a, tc.b)
		}
		if got := a.compare(b); got != tc.want {
			t.Errorf("compare(%s, %s) = %d, want %d", tc.a, tc.b, got, tc.want)
		}
	}
}
