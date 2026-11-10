package checks

import (
	"encoding/json"
	"strings"
	"testing"
)

func TestTsconfigPathsMirrorProblems(t *testing.T) {
	pkg := []byte(`{"imports": {"#lib/*": "./src/lib/*"}}`)
	cases := []struct {
		name     string
		pkg      []byte
		tsconfig string
		mirrored int
		problem  string // substring of the single expected problem, "" for none
	}{
		{"mirrored, with comments and a trailing comma", pkg,
			"{\n  // why\n  \"compilerOptions\": { /* x */ \"paths\": { \"#lib/*\": [\"./src/lib/*\"] }, },\n}", 1, ""},
		{"missing paths", pkg, `{"compilerOptions": {}}`, 0, "is missing"},
		{"wrong target", pkg, `{"compilerOptions": {"paths": {"#lib/*": ["./src/*"]}}}`, 0, "but package.json `imports` says"},
		{"stale alias", []byte(`{}`), `{"compilerOptions": {"paths": {"#lib/*": ["./src/lib/*"]}}}`, 0, "no package.json `imports` twin"},
		{"conditional import", []byte(`{"imports": {"#x": {"node": "./a.js"}}}`), `{}`, 0, "conditional map"},
		{"non-# paths are left alone", []byte(`{}`), `{"compilerOptions": {"paths": {"$x/*": ["./x/*"]}}}`, 0, ""},
		{"comment markers inside strings survive", []byte(`{"imports": {"#a": "./a//b/*c*/"}}`),
			`{"compilerOptions": {"paths": {"#a": ["./a//b/*c*/"]}}}`, 1, ""},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			n, problems, err := tsconfigPathsMirrorProblems(tc.pkg, []byte(tc.tsconfig))
			if err != nil {
				t.Fatalf("unexpected error: %v", err)
			}
			if n != tc.mirrored {
				t.Errorf("mirrored = %d, want %d", n, tc.mirrored)
			}
			if tc.problem == "" {
				if len(problems) != 0 {
					t.Errorf("want no problems, got %q", problems)
				}
				return
			}
			if len(problems) != 1 || !strings.Contains(problems[0], tc.problem) {
				t.Errorf("want one problem containing %q, got %q", tc.problem, problems)
			}
		})
	}
}

func TestStripJSONCYieldsValidJSON(t *testing.T) {
	in := "{\n  // a\n  \"a\": [1, 2,],\n  /* b */ \"b\": \"//not a comment\",\n}\n"
	var v map[string]any
	if err := json.Unmarshal(stripJSONC([]byte(in)), &v); err != nil {
		t.Fatalf("stripped output isn't JSON: %v\n%s", err, stripJSONC([]byte(in)))
	}
	if v["b"] != "//not a comment" {
		t.Errorf("string content mangled: %v", v["b"])
	}
}
