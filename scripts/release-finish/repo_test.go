package main

import "testing"

func TestParseRepoName(t *testing.T) {
	for _, tc := range []struct {
		in, want string
		ok       bool
	}{
		{"vdavid/cmdr\n", "vdavid/cmdr", true},
		// The org the repo moved to (#476): `gh repo view` names it even through a remote
		// that still says `vdavid/cmdr`, since GitHub redirects the old name.
		{"getcmdr/cmdr", "getcmdr/cmdr", true},
		{"", "", false},
		{"cmdr", "", false},
		{"vdavid/cmdr/extra", "", false},
		{"https://github.com/vdavid/cmdr", "", false},
		{"vdavid/cmdr --repo evil/x", "", false},
	} {
		got, err := parseRepoName(tc.in)
		if (err == nil) != tc.ok || got != tc.want {
			t.Errorf("parseRepoName(%q) = %q, %v; want %q, ok=%v", tc.in, got, err, tc.want, tc.ok)
		}
	}
}

func TestPipelineSignerFor(t *testing.T) {
	for repo, want := range map[string]string{
		"vdavid/cmdr":  "vdavid/cmdr/.github/workflows/release-pipeline.yml",
		"getcmdr/cmdr": "getcmdr/cmdr/.github/workflows/release-pipeline.yml",
	} {
		if got := pipelineSignerFor(repo); got != want {
			t.Errorf("pipelineSignerFor(%q) = %q, want %q", repo, got, want)
		}
	}
}

// An attestation names the repo that built it, forever, so a release made before the move
// verifies against `vdavid/cmdr` and one made after against `getcmdr/cmdr`: the repo to verify
// against comes from the signer workflow, never from where the repo lives today.
func TestSignerRepo(t *testing.T) {
	for _, tc := range []struct {
		signer, want string
		ok           bool
	}{
		{"vdavid/cmdr/.github/workflows/release-pipeline.yml", "vdavid/cmdr", true},
		{"getcmdr/cmdr/.github/workflows/release-pipeline.yml", "getcmdr/cmdr", true},
		{"vdavid/cmdr/.github/workflows/release.yml", "vdavid/cmdr", true},
		{"release-pipeline.yml", "", false},
		{"vdavid/cmdr", "", false},
		{"vdavid/cmdr/release-pipeline.yml", "", false},
	} {
		got, err := signerRepo(tc.signer)
		if (err == nil) != tc.ok || got != tc.want {
			t.Errorf("signerRepo(%q) = %q, %v; want %q, ok=%v", tc.signer, got, err, tc.want, tc.ok)
		}
	}
}
