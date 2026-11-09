package main

import "testing"

func TestParseRepoName(t *testing.T) {
	for _, tc := range []struct {
		in, want string
		ok       bool
	}{
		{"vdavid/cmdr\n", "vdavid/cmdr", true},
		// The org the repo moved to (#476).
		{"getcmdr/cmdr", "getcmdr/cmdr", true},
		{"", "", false},
		{"cmdr", "", false},
		{"vdavid/cmdr/extra", "", false},
		{"vdavid/cmdr -R evil/x", "", false},
	} {
		got, err := parseRepoName(tc.in)
		if (err == nil) != tc.ok || got != tc.want {
			t.Errorf("parseRepoName(%q) = %q, %v; want %q, ok=%v", tc.in, got, err, tc.want, tc.ok)
		}
	}
}
