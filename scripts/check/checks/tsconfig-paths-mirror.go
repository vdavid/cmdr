package checks

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
)

// tsconfigPathsMirrorApps are the SvelteKit apps whose svelte-check lane runs
// `--tsgo`. Each mirrors its package.json `imports` in tsconfig `paths`.
var tsconfigPathsMirrorApps = []string{"apps/desktop", "apps/analytics-dashboard"}

// tsconfigPathsMirrorInputs is exactly the files the check reads.
var tsconfigPathsMirrorInputs = []string{
	"apps/desktop/package.json",
	"apps/desktop/tsconfig.json",
	"apps/analytics-dashboard/package.json",
	"apps/analytics-dashboard/tsconfig.json",
}

// RunTsconfigPathsMirror fails when an app's tsconfig `paths` stop mirroring
// its package.json `imports`.
//
// Why the duplicate exists: `svelte-check --tsgo` type-checks transpiled
// components it writes to disk, and rebases tsconfig `paths` onto them but not
// subpath `imports` (sveltejs/language-tools#3142). Without the mirror, every
// `#lib/*.svelte` import resolves to the ambient `*.svelte` shim: false errors
// on named type imports, and prop checks on those components silently gone.
// So each `imports` entry needs a `paths` entry with the same key and target,
// and a `#` alias in `paths` with no `imports` twin is stale.
func RunTsconfigPathsMirror(ctx *CheckContext) (CheckResult, error) {
	var problems []string
	mirrored := 0
	for _, app := range tsconfigPathsMirrorApps {
		pkgData, err := os.ReadFile(filepath.Join(ctx.RootDir, app, "package.json"))
		if err != nil {
			return CheckResult{}, fmt.Errorf("reading %s/package.json: %w", app, err)
		}
		tsData, err := os.ReadFile(filepath.Join(ctx.RootDir, app, "tsconfig.json"))
		if err != nil {
			return CheckResult{}, fmt.Errorf("reading %s/tsconfig.json: %w", app, err)
		}
		n, appProblems, err := tsconfigPathsMirrorProblems(pkgData, tsData)
		if err != nil {
			return CheckResult{}, fmt.Errorf("%s: %w", app, err)
		}
		mirrored += n
		for _, p := range appProblems {
			problems = append(problems, app+": "+p)
		}
	}
	if len(problems) > 0 {
		return CheckResult{}, fmt.Errorf("tsconfig `paths` must mirror package.json `imports` for `svelte-check --tsgo` (sveltejs/language-tools#3142):\n%s",
			indentOutput(strings.Join(problems, "\n")))
	}
	return Success(fmt.Sprintf("%d %s mirrored across %d apps",
		mirrored, Pluralize(mirrored, "subpath import", "subpath imports"), len(tsconfigPathsMirrorApps))), nil
}

// tsconfigPathsMirrorProblems compares one app's package.json `imports` with
// its tsconfig `compilerOptions.paths` (JSONC). Returns how many imports are
// mirrored correctly and one line per mismatch.
func tsconfigPathsMirrorProblems(pkgData, tsconfigData []byte) (int, []string, error) {
	var pkg struct {
		Imports map[string]json.RawMessage `json:"imports"`
	}
	if err := json.Unmarshal(pkgData, &pkg); err != nil {
		return 0, nil, fmt.Errorf("parsing package.json: %w", err)
	}
	var ts struct {
		CompilerOptions struct {
			Paths map[string][]string `json:"paths"`
		} `json:"compilerOptions"`
	}
	if err := json.Unmarshal(stripJSONC(tsconfigData), &ts); err != nil {
		return 0, nil, fmt.Errorf("parsing tsconfig.json: %w", err)
	}

	var problems []string
	mirrored := 0
	keys := make([]string, 0, len(pkg.Imports))
	for k := range pkg.Imports {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	for _, key := range keys {
		var target string
		if err := json.Unmarshal(pkg.Imports[key], &target); err != nil {
			problems = append(problems, fmt.Sprintf("`imports[%q]` is a conditional map, which tsconfig `paths` can't mirror; use a plain string target", key))
			continue
		}
		got, ok := ts.CompilerOptions.Paths[key]
		switch {
		case !ok:
			problems = append(problems, fmt.Sprintf("`paths[%q]` is missing; add `%q: [%q]`", key, key, target))
		case len(got) != 1 || got[0] != target:
			problems = append(problems, fmt.Sprintf("`paths[%q]` is %q, but package.json `imports` says %q", key, got, target))
		default:
			mirrored++
		}
	}
	for key := range ts.CompilerOptions.Paths {
		if _, ok := pkg.Imports[key]; !ok && strings.HasPrefix(key, "#") {
			problems = append(problems, fmt.Sprintf("`paths[%q]` has no package.json `imports` twin; remove it or add the import", key))
		}
	}
	sort.Strings(problems)
	return mirrored, problems, nil
}

// stripJSONC turns tsconfig-flavored JSON into plain JSON: drops `//` and
// `/* */` comments outside strings and trailing commas before `}` or `]`.
func stripJSONC(data []byte) []byte {
	out := make([]byte, 0, len(data))
	for i := 0; i < len(data); i++ {
		c := data[i]
		switch {
		case c == '"':
			end := jsonStringEnd(data, i)
			out = append(out, data[i:end]...)
			i = end - 1
		case c == '/' && i+1 < len(data) && (data[i+1] == '/' || data[i+1] == '*'):
			// Keep the line break a `//` comment ends on; drop the comment itself.
			i = jsonCommentEnd(data, i) - 1
		case c == '}' || c == ']':
			out = append(dropTrailingComma(out), c)
		default:
			out = append(out, c)
		}
	}
	return out
}

// jsonStringEnd returns the index just past the string literal opening at start.
func jsonStringEnd(data []byte, start int) int {
	for i := start + 1; i < len(data); i++ {
		switch data[i] {
		case '\\':
			i++
		case '"':
			return i + 1
		}
	}
	return len(data)
}

// jsonCommentEnd returns the index just past the comment opening at start; a
// `//` comment ends before its newline.
func jsonCommentEnd(data []byte, start int) int {
	if data[start+1] == '/' {
		for i := start; i < len(data); i++ {
			if data[i] == '\n' {
				return i
			}
		}
		return len(data)
	}
	for i := start + 2; i+1 < len(data); i++ {
		if data[i] == '*' && data[i+1] == '/' {
			return i + 2
		}
	}
	return len(data)
}

// dropTrailingComma removes a comma that is the last non-space byte of out.
func dropTrailingComma(out []byte) []byte {
	j := len(out) - 1
	for j >= 0 && (out[j] == ' ' || out[j] == '\t' || out[j] == '\n' || out[j] == '\r') {
		j--
	}
	if j >= 0 && out[j] == ',' {
		return append(out[:j], out[j+1:]...)
	}
	return out
}
