package checks

import (
	"strings"
	"testing"
)

func changelogSecurityProblemsIn(t *testing.T, changelog string) []string {
	t.Helper()
	entries, err := collectChangelogSecurityEntries(strings.NewReader(changelog))
	if err != nil {
		t.Fatal(err)
	}
	var problems []string
	for _, entry := range entries {
		problems = append(problems, changelogSecurityEntryProblems(entry)...)
	}
	return problems
}

func TestChangelogSecurity_AcceptsBothForms(t *testing.T) {
	changelog := `# Changelog

## [Unreleased]

### Security

- Stop a malicious ` + "`.git/config`" + ` from switching off path protections: ` + "`gix`" + `
  GHSA-q54r-jf2p-rfr2 (high), Cmdr 0.52.0–0.56.0 (70ae9eb83)
- Keep S3 access key IDs out of logs: CVSS v4.0 2.1 (low), Cmdr 0.50.0–0.56.0 (1981c788b)
- Close a timing leak: ` + "`crypto-bigint`" + ` RUSTSEC-2026-0387 (unrated), Cmdr 0.3.0–0.10.0 and 0.40.0–0.56.0 (70ae9eb83)
- Fix a flaw that shipped in one release only: CVE-2026-12345 (medium), Cmdr 0.56.0 (70ae9eb83)

### Non-app

- Anything goes here (70ae9eb83)
`
	if problems := changelogSecurityProblemsIn(t, changelog); len(problems) > 0 {
		t.Fatalf("want no problems, got %q", problems)
	}
}

func TestChangelogSecurity_SkipsReleasesBefore057(t *testing.T) {
	changelog := `## [0.56.0] - 2026-10-23

### Security

- Update the SMB library for an upstream fix to its signature checks (169ffc336)
`
	entries, err := collectChangelogSecurityEntries(strings.NewReader(changelog))
	if err != nil {
		t.Fatal(err)
	}
	if len(entries) != 0 {
		t.Fatalf("0.56.0 predates the rule, want no entries, got %d", len(entries))
	}
}

func TestChangelogSecurity_ChecksVersionedReleasesFrom057(t *testing.T) {
	changelog := `## [0.58.0] - 2026-11-01

### Fixed

- A plain fix, not a Security line (70ae9eb83)

### Security

- Update the TLS library for upstream security fixes (0246c7b34)
`
	problems := changelogSecurityProblemsIn(t, changelog)
	if len(problems) != 2 {
		t.Fatalf("want the missing ID and the missing versions, got %q", problems)
	}
	if !strings.Contains(problems[0], "upstream advisory ID") || !strings.Contains(problems[1], "affected versions") {
		t.Fatalf("unexpected problems: %q", problems)
	}
}

func TestChangelogSecurity_Problems(t *testing.T) {
	cases := []struct {
		name  string
		entry string
		want  string
	}{
		{"id without severity", "- Fix it: `gix` GHSA-q54r-jf2p-rfr2, Cmdr 0.52.0–0.56.0 (70ae9eb83)", "severity in parens"},
		{"old CVSS form", "- Fix it: high (CVSS 8.2), Cmdr 0.31.0–0.53.0 (70ae9eb83)", "CVSS v4.0 <score> (<rating>)"},
		{"rating off its band", "- Fix it: CVSS v4.0 8.2 (medium), Cmdr 0.31.0–0.53.0 (70ae9eb83)", "is rated (high), not (medium)"},
		{"unrated with our own score", "- Fix it: CVSS v4.0 2.1 (unrated), Cmdr 0.50.0–0.56.0 (70ae9eb83)", "is rated (low)"},
		{"no versions", "- Fix it: `gix` GHSA-q54r-jf2p-rfr2 (high) (70ae9eb83)", "affected versions"},
		{"hyphen range", "- Fix it: RUSTSEC-2026-0387 (unrated), Cmdr 0.40.0-0.56.0 (70ae9eb83)", "en dash"},
		{"backwards range", "- Fix it: RUSTSEC-2026-0387 (unrated), Cmdr 0.56.0–0.40.0 (70ae9eb83)", "runs backwards"},
		{"range reaching the fix", "- Fix it: RUSTSEC-2026-0387 (unrated), Cmdr 0.40.0–0.58.0 (70ae9eb83)", "the release that fixes it"},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			changelog := "## [0.58.0] - 2026-11-01\n\n### Security\n\n" + tc.entry + "\n"
			problems := changelogSecurityProblemsIn(t, changelog)
			for _, problem := range problems {
				if strings.Contains(problem, tc.want) {
					return
				}
			}
			t.Fatalf("want a problem containing %q, got %q", tc.want, problems)
		})
	}
}

func TestCVSSV4Rating_Bands(t *testing.T) {
	cases := map[float64]string{0.0: "none", 0.1: "low", 3.9: "low", 4.0: "medium", 6.9: "medium", 7.0: "high",
		8.9: "high", 9.0: "critical", 10.0: "critical"}
	for score, want := range cases {
		if got := cvssV4Rating(score); got != want {
			t.Errorf("cvssV4Rating(%.1f) = %q, want %q", score, got, want)
		}
	}
}
