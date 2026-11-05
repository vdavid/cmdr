package main

import (
	"fmt"
	"strings"
)

// glob is one compiled `dorny/paths-filter` pattern. dorny compiles each pattern with picomatch
// (`{dot: true}`) and reads a leading `!` as a negation. We support the subset our filters use,
// and refuse everything else so a new syntax fails loudly instead of matching differently from
// dorny: `**` as a whole segment (zero or more segments), `*` (any run of characters within a
// segment, dotfiles included), `?` (one character within a segment), a plain character class
// (`[Dd]`: one character from a listed set, no ranges or negation; the scorecard filter spells
// case-insensitive names that way), and literals.
type glob struct {
	raw     string
	negated bool
	segs    []string
}

// unsupportedGlobChars are picomatch features we don't reimplement: braces, extglobs, and
// escapes. A pattern using one is rejected; brackets pass only as a plain class (`checkClasses`).
const unsupportedGlobChars = "{}()\\"

// checkClasses accepts a segment's `[...]` only as a non-empty set of literal characters: no
// range (`-`), negation (`!`, `^`), nesting, POSIX class, or stray `]`.
func checkClasses(seg string) error {
	for i := 0; i < len(seg); i++ {
		switch seg[i] {
		case ']':
			return fmt.Errorf("a `]` outside a character class")
		case '[':
			end := strings.IndexByte(seg[i+1:], ']')
			if end <= 0 {
				return fmt.Errorf("an empty or unclosed character class")
			}
			if set := seg[i+1 : i+1+end]; strings.ContainsAny(set, "[-!^:") {
				return fmt.Errorf("a character class with a range, negation, or nesting (%q)", set)
			}
			i += end + 1
		}
	}
	return nil
}

func compileGlob(pattern string) (glob, error) {
	g := glob{raw: pattern}
	body := pattern
	if strings.HasPrefix(body, "!") {
		g.negated = true
		body = body[1:]
	}
	switch {
	case body == "":
		return g, fmt.Errorf("pattern %q is empty", pattern)
	case strings.ContainsAny(body, unsupportedGlobChars):
		return g, fmt.Errorf("pattern %q uses glob syntax lane-markers doesn't support (%s); extend match.go and its tests", pattern, unsupportedGlobChars)
	case strings.Contains(body, "!"):
		return g, fmt.Errorf("pattern %q has a `!` past its first character", pattern)
	case strings.HasPrefix(body, "/") || strings.HasPrefix(body, "./") || strings.HasSuffix(body, "/"):
		return g, fmt.Errorf("pattern %q must be a plain repo-relative path", pattern)
	}
	for seg := range strings.SplitSeq(body, "/") {
		if seg == "" {
			return g, fmt.Errorf("pattern %q has an empty segment", pattern)
		}
		if seg != "**" && strings.Contains(seg, "**") {
			return g, fmt.Errorf("pattern %q uses `**` inside a segment", pattern)
		}
		if err := checkClasses(seg); err != nil {
			return g, fmt.Errorf("pattern %q uses glob syntax lane-markers doesn't support: %v; extend match.go and its tests", pattern, err)
		}
		g.segs = append(g.segs, seg)
	}
	return g, nil
}

// matchesBody reports whether path matches the pattern, ignoring its negation.
func (g glob) matchesBody(path string) bool {
	return matchSegs(g.segs, strings.Split(path, "/"))
}

// matches is picomatch's verdict: a negated pattern matches every path its body doesn't.
func (g glob) matches(path string) bool {
	return g.matchesBody(path) != g.negated
}

func matchSegs(pat, path []string) bool {
	return matchSegsAfter(pat, path, "")
}

// matchSegsAfter is matchSegs with the pattern segment just before pat. picomatch quirk: a
// trailing `**` matches zero segments (`crates/**` matches `crates`) EXCEPT after a segment that
// ends in `*`, where it needs one (`a*/**` doesn't match `ab`; pinned in `goldenClassGlobs`).
func matchSegsAfter(pat, path []string, prev string) bool {
	if len(pat) == 0 {
		return len(path) == 0
	}
	if pat[0] == "**" {
		from := 0
		if len(pat) == 1 && prev != "" && prev != "**" && strings.HasSuffix(prev, "*") {
			from = 1
		}
		for i := from; i <= len(path); i++ {
			if matchSegsAfter(pat[1:], path[i:], pat[0]) {
				return true
			}
		}
		return false
	}
	if len(path) == 0 || !matchSegment(pat[0], path[0]) {
		return false
	}
	return matchSegsAfter(pat[1:], path[1:], pat[0])
}

// matchSegment matches one path segment against `*`, `?`, a plain `[...]` class, and literals.
func matchSegment(pat, s string) bool {
	if pat == "" {
		return s == ""
	}
	switch pat[0] {
	case '[':
		end := strings.IndexByte(pat, ']') // well-formed: `checkClasses` vetted it
		return s != "" && strings.IndexByte(pat[1:end], s[0]) >= 0 && matchSegment(pat[end+1:], s[1:])
	case '*':
		for i := 0; i <= len(s); i++ {
			if matchSegment(pat[1:], s[i:]) {
				return true
			}
		}
		return false
	case '?':
		return s != "" && matchSegment(pat[1:], s[1:])
	default:
		return s != "" && s[0] == pat[0] && matchSegment(pat[1:], s[1:])
	}
}

// quantifier mirrors dorny's `predicate-quantifier` input.
type quantifier string

const (
	quantSome             quantifier = "some"
	quantEvery            quantifier = "every"
	quantSomeWithExcludes quantifier = "some-with-excludes"
)

func parseQuantifier(s string) (quantifier, error) {
	switch q := quantifier(s); q {
	case "":
		return quantSome, nil
	case quantSome, quantEvery, quantSomeWithExcludes:
		return q, nil
	default:
		return "", fmt.Errorf("unknown predicate-quantifier %q", s)
	}
}

// filter is one named dorny filter: the patterns in file order.
type filter struct {
	name     string
	patterns []glob
}

// matches is dorny's `Filter.isMatch` for one file (verified against dorny/paths-filter v4.0.3's
// `src/filter.ts`, 2026-11-05):
//   - `some`: any pattern matches, a negated one by NOT matching its body.
//   - `every`: every pattern matches, in that same sense.
//   - `some-with-excludes`: a negated pattern whose body matches excludes the file for good;
//     otherwise any plain pattern includes it. Negated patterns alone include nothing.
func (f filter) matches(path string, q quantifier) bool {
	switch q {
	case quantEvery:
		for _, g := range f.patterns {
			if !g.matches(path) {
				return false
			}
		}
		return true
	case quantSomeWithExcludes:
		included := false
		for _, g := range f.patterns {
			if g.negated {
				if g.matchesBody(path) {
					return false
				}
			} else if g.matchesBody(path) {
				included = true
			}
		}
		return included
	default:
		for _, g := range f.patterns {
			if g.matches(path) {
				return true
			}
		}
		return false
	}
}
