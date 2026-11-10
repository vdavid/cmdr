package checks

import "testing"

func TestStylelintReportHasRule(t *testing.T) {
	cases := []struct {
		name    string
		report  string
		want    bool
		wantErr bool
	}{
		{"finding present", `[{"warnings":[{"rule":"color-no-hex"},{"rule":"declaration-no-important"}]}]`, true, false},
		// The #1056 shape: postcss-html parses the `.css` as HTML and reports nothing.
		{"file linted as HTML, no findings", `[{"errored":false,"warnings":[]}]`, false, false},
		{"other rule only", `[{"warnings":[{"rule":"color-no-hex"}]}]`, false, false},
		{"not JSON", `Error: No configuration provided`, false, true},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			got, err := stylelintReportHasRule([]byte(c.report), "declaration-no-important")
			if (err != nil) != c.wantErr {
				t.Fatalf("err = %v, wantErr %v", err, c.wantErr)
			}
			if got != c.want {
				t.Fatalf("got %v, want %v", got, c.want)
			}
		})
	}
}
