package checks

import "testing"

func TestIsPathShapedLinkText(t *testing.T) {
	tests := []struct {
		text string
		want bool
	}{
		{"`docs/architecture.md`", true},
		{"`DETAILS.md`", true},
		{"DETAILS.md", true},                   // unbackticked path is just as redundant
		{"`../reconcile/DETAILS.md`", true},    // relative paths count
		{"`apps/desktop/src-tauri/`", true},    // directory reference
		{"`Cargo.toml`", true},                 // known extension, no slash
		{"`scripts/check`", true},              // slash, no extension
		{"the subsystem map", false},           // prose
		{"`serde`", false},                     // crate name: no slash, no extension
		{"`--fast`", false},                    // flag
		{"`pnpm check`", false},                // space means prose, not a path
		{"docs", false},                        // bare word
		{"`cargo deny check`", false},          // command
		{".", false},                           // not a reference
		{"..", false},                          // not a reference
		{"", false},                            // empty
		{"`a` and `b`", false},                 // multiple spans, not one path
		{"v1.2.3", false},                      // version, unknown extension
		{"`getcmdr.com`", false},               // domain, unknown extension
		{"`routes/(main)/+page.svelte`", true}, // SvelteKit group dir + plus-prefixed file
		{"`@AGENTS.md`", true},                 // import marker
		{"`docs/style-guide.md`", true},        // hyphenated filename
		{"`.claude/rules/docs.md`", true},      // hidden dir
		{"`file-explorer/CLAUDE.md`", true},    // hyphenated dir
		{"read `docs/architecture.md`", false}, // prose wrapping a path
	}
	for _, tt := range tests {
		if got := isPathShapedLinkText(tt.text); got != tt.want {
			t.Errorf("isPathShapedLinkText(%q) = %v, want %v", tt.text, got, tt.want)
		}
	}
}

func TestScanLinkText(t *testing.T) {
	tests := []struct {
		name       string
		content    string
		wantHits   int
		wantLine   int    // line of the first hit (if any)
		wantTarget string // target of the first hit (if any)
	}{
		{
			name:       "exact duplicate is flagged",
			content:    "See [`docs/architecture.md`](docs/architecture.md) for the map.\n",
			wantHits:   1,
			wantLine:   1,
			wantTarget: "docs/architecture.md",
		},
		{
			name:       "path text with a relative target is flagged",
			content:    "Details in [`lib/query-ui/CLAUDE.md`](../query-ui/CLAUDE.md).\n",
			wantHits:   1,
			wantLine:   1,
			wantTarget: "../query-ui/CLAUDE.md",
		},
		{
			name:     "descriptive link text passes",
			content:  "See [the subsystem map](docs/architecture.md).\n",
			wantHits: 0,
		},
		{
			name:     "bare backtick path (no link) passes",
			content:  "See `docs/architecture.md` for the map.\n",
			wantHits: 0,
		},
		{
			name:     "external URL keeps its path-ish text",
			content:  "See [`serde_json`](https://docs.rs/serde_json/latest/).\n",
			wantHits: 0,
		},
		{
			name:     "link inside a fenced block is an example, not a reference",
			content:  "Bad:\n\n```md\nSee [`docs/x.md`](docs/x.md).\n```\n\nDone.\n",
			wantHits: 0,
		},
		{
			name:     "link inside a double-backtick span is an example",
			content:  "Write ``[`docs/x.md`](docs/x.md)`` instead of this.\n",
			wantHits: 0,
		},
		{
			name:     "whole link inside an inline-code span is a template, not a reference",
			content:  "The line reads `<what's inside>: [DETAILS.md](DETAILS.md). <trigger>` verbatim.\n",
			wantHits: 0,
		},
		{
			name:       "line number is reported",
			content:    "# Title\n\nProse.\n\nSee [`DETAILS.md`](DETAILS.md).\n",
			wantHits:   1,
			wantLine:   5,
			wantTarget: "DETAILS.md",
		},
		{
			name:     "two hits on one line are both reported",
			content:  "See [`a/CLAUDE.md`](a/CLAUDE.md) and [`b/CLAUDE.md`](b/CLAUDE.md).\n",
			wantHits: 2,
		},
		{
			name:     "in-page anchor target is skipped (no file to name)",
			content:  "See [`#docs`](#docs).\n",
			wantHits: 0,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			hits := scanLinkText("doc.md", tt.content)
			if len(hits) != tt.wantHits {
				t.Fatalf("got %d hits, want %d: %+v", len(hits), tt.wantHits, hits)
			}
			if tt.wantTarget != "" {
				if hits[0].line != tt.wantLine {
					t.Errorf("first hit line = %d, want %d", hits[0].line, tt.wantLine)
				}
				if hits[0].target != tt.wantTarget {
					t.Errorf("first hit target = %q, want %q", hits[0].target, tt.wantTarget)
				}
			}
		})
	}
}

func TestScanLinkTextFlagsAnchorTargets(t *testing.T) {
	hits := scanLinkText("doc.md", "See [`docs/testing.md`](docs/testing.md#e2e).\n")
	if len(hits) != 1 {
		t.Fatalf("got %d hits, want 1", len(hits))
	}
	if !hits[0].anchor {
		t.Error("expected anchor=true for a target with a #fragment")
	}
	if hits[0].sameStr {
		t.Error("expected sameStr=false when the target carries an anchor")
	}
}
