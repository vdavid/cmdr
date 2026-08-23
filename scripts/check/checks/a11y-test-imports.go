package checks

import (
	"path"
	"regexp"
	"strings"
)

// Resolves which components a `*.a11y.test.ts` file actually imports, so the
// a11y coverage check can accept one directory-level test file covering several
// components instead of demanding one file per component.
//
// "Actually imports" has to be exact. A substring search for the component name
// would accept `SearchSection.svelte` as coverage for `Section.svelte`, or a
// name that only appears in a doc comment or a `describe()` title, and the check
// would silently stop enforcing anything. So: strip comments, match real import
// statements, and resolve each specifier to a repo-relative path that must equal
// the component's path.

// svelteLibPrefix is what the `$lib` alias resolves to (SvelteKit's default,
// and `apps/desktop/svelte.config.js` doesn't override it).
const svelteLibPrefix = "apps/desktop/src/lib"

// quotedSpecifier matches a single- or double-quoted module specifier. RE2 has
// no backreferences, so the two quote styles are spelled out.
const quotedSpecifier = `('[^'\n]*'|"[^"\n]*")`

// staticImportRe matches `import ... from 'spec'` in all its shapes (default,
// named, namespace, type-only) plus the side-effect form `import 'spec'`. The
// char class excludes quotes, parens, and `;` so a match can never run across a
// statement boundary or swallow a string literal.
var staticImportRe = regexp.MustCompile(`\bimport\b(?:[^'"();]*\bfrom\s*)?\s*` + quotedSpecifier)

// dynamicImportRe matches `import('spec')`.
var dynamicImportRe = regexp.MustCompile(`\bimport\s*\(\s*` + quotedSpecifier)

// importedPathsIn returns the repo-relative paths that `source` imports, for the
// specifiers it can resolve: relative (`./`, `../`) and `$lib`-rooted ones.
// Anything else (a bare package, an unknown alias) resolves to nothing, which
// keeps an unresolvable specifier a coverage gap rather than a free pass.
//
// `testRelPath` is the importing file's own repo-relative path.
func importedPathsIn(testRelPath, source string) map[string]bool {
	code := stripTSComments(source)
	dir := path.Dir(testRelPath)

	resolved := map[string]bool{}
	for _, re := range []*regexp.Regexp{staticImportRe, dynamicImportRe} {
		for _, m := range re.FindAllStringSubmatch(code, -1) {
			spec := strings.Trim(m[1], `'"`)
			if p, ok := resolveSpecifier(dir, spec); ok {
				resolved[p] = true
			}
		}
	}
	return resolved
}

// resolveSpecifier turns one module specifier into a repo-relative path.
func resolveSpecifier(fromDir, spec string) (string, bool) {
	switch {
	case strings.HasPrefix(spec, "./"), strings.HasPrefix(spec, "../"):
		return path.Clean(path.Join(fromDir, spec)), true
	case spec == "$lib" || strings.HasPrefix(spec, "$lib/"):
		return path.Clean(svelteLibPrefix + strings.TrimPrefix(spec, "$lib")), true
	default:
		return "", false
	}
}

// stripTSComments blanks out `//` and `/* */` comments while leaving string and
// template literals alone, so a commented-out import can't count as an import
// and an apostrophe inside a comment can't unbalance the quote tracking.
//
// It does not know regex literals from division, so `/'/` would open a string.
// The cost of getting that wrong is a missed import, which reads as a coverage
// gap the author has to fix in the open. Never a false pass.
func stripTSComments(source string) string {
	var out strings.Builder
	out.Grow(len(source))

	for i := 0; i < len(source); {
		switch {
		case strings.HasPrefix(source[i:], "//"):
			i = skipLineComment(source, i)
		case strings.HasPrefix(source[i:], "/*"):
			i = skipBlockComment(source, i, &out)
		case source[i] == '\'' || source[i] == '"' || source[i] == '`':
			i = copyStringLiteral(source, i, &out)
		default:
			out.WriteByte(source[i])
			i++
		}
	}
	return out.String()
}

// skipLineComment returns the index of the comment's terminating newline, which
// the caller then copies: line structure has to survive so a regex can't join
// two statements into one.
func skipLineComment(source string, i int) int {
	for i < len(source) && source[i] != '\n' {
		i++
	}
	return i
}

// skipBlockComment returns the index just past `*/`, emitting the newlines the
// comment spanned.
func skipBlockComment(source string, i int, out *strings.Builder) int {
	for i += 2; i < len(source); i++ {
		if strings.HasPrefix(source[i:], "*/") {
			return i + 2
		}
		if source[i] == '\n' {
			out.WriteByte('\n')
		}
	}
	return i
}

// copyStringLiteral copies a quoted literal verbatim (escapes included) and
// returns the index just past its closing quote.
func copyStringLiteral(source string, i int, out *strings.Builder) int {
	quote := source[i]
	out.WriteByte(source[i])
	for i++; i < len(source); {
		c := source[i]
		out.WriteByte(c)
		i++
		if c == '\\' && i < len(source) {
			out.WriteByte(source[i])
			i++
			continue
		}
		if c == quote {
			break
		}
	}
	return i
}
