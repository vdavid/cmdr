package checks

import "testing"

func TestNextestVersionIs(t *testing.T) {
	cases := []struct {
		name   string
		output string
		want   string
		match  bool
	}{
		{"the pinned version", "cargo-nextest 0.9.136 (1d5bf1ec9 2026-05-16)\nrelease: 0.9.136\n", "0.9.136", true},
		{"an older install", "cargo-nextest 0.9.120 (abc 2026-01-02)\n", "0.9.136", false},
		{"a version with the pin as its prefix", "cargo-nextest 0.9.1360 (abc 2026-01-02)\n", "0.9.136", false},
		{"no output", "", "0.9.136", false},
		{"cargo's unknown-command error", "error: no such command: `nextest`\n", "0.9.136", false},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			if got := nextestVersionIs(tc.output, tc.want); got != tc.match {
				t.Errorf("nextestVersionIs(%q, %q) = %v, want %v", tc.output, tc.want, got, tc.match)
			}
		})
	}
}
