package main

import (
	"fmt"
	"strings"
)

// glob is one compiled `dorny/paths-filter` pattern. dorny compiles each pattern with picomatch
// (`{dot: true}`) and reads a leading `!` as a negation. We support the subset our filters use,
// and refuse everything else so a new syntax fails loudly instead of matching differently from
// dorny: `**` as a whole segment (zero or more segments), `*` (any run of characters within a
// segment, dotfiles included), `?` (one character within a segment), and literals.
type glob struct {
	raw     string
	negated bool
	segs    []string
}

// unsupportedGlobChars are picomatch features we don't reimplement: braces, brackets, extglobs,
// and escapes. A pattern using one is rejected.
const unsupportedGlobChars = "{}[]()\\"

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
	if len(pat) == 0 {
		return len(path) == 0
	}
	if pat[0] == "**" {
		for i := 0; i <= len(path); i++ {
			if matchSegs(pat[1:], path[i:]) {
				return true
			}
		}
		return false
	}
	if len(path) == 0 || !matchSegment(pat[0], path[0]) {
		return false
	}
	return matchSegs(pat[1:], path[1:])
}

// matchSegment matches one path segment against `*`, `?`, and literals.
func matchSegment(pat, s string) bool {
	if pat == "" {
		return s == ""
	}
	switch pat[0] {
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
