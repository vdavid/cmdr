package checks

import (
	"regexp"
	"testing"
)

func TestRequiredLiteralFindsTheLongestRunEveryMatchMustContain(t *testing.T) {
	cases := map[string]string{
		`\bmessage\.contains\(`:                       "message.contains(",
		`\b(error|err|msg|lower)\.starts_with\(`:      ".starts_with(",
		`\.to_string\(\)\.contains\(`:                 ".to_string().contains(",
		`Connection::open(_in_memory|_with_flags)?\(`: "Connection::open",
		`(?:std::)?thread::sleep\(`:                   "thread::sleep(",
		`(foo)+bar`:                                   "foo",
		`a|b`:                                         "",
		`(?i)Sleep\(`:                                 "",
		`\w+`:                                         "",
		`x?y*`:                                        "",
	}
	for expr, want := range cases {
		if got := requiredLiteral(expr); got != want {
			t.Errorf("requiredLiteral(%q) = %q, want %q", expr, got, want)
		}
	}
}

// A gate with no literal runs the regex on every line, so wrapping a pattern
// that has none buys nothing: reword the pattern or drop the wrapper.
func TestEveryScannerGateHasALiteral(t *testing.T) {
	gates := map[string][]gatedRegexp{
		"errorStringMatchPatterns": errorStringMatchPatterns,
		"scanner patterns": {
			rawSqliteOpenPattern, testSleepRegex, testGatedCfgRegex, fixedTempDirRegex, pluralizeNounSuffixRe,
			crateRefPattern, e2eSelDatasetRegex, lockBareUnwrapPattern, lockExpectPattern, handRolledFixtureRegex,
			languageCountRe,
		},
	}
	for name, list := range gates {
		for _, g := range list {
			if g.literal == "" {
				t.Errorf("%s: %q has no required literal to gate on", name, g.re.String())
			}
		}
	}
}

// The gate may only ever skip lines the regex would have rejected anyway.
func TestGatedRegexpAgreesWithItsRegexp(t *testing.T) {
	exprs := []string{
		`\bstderr\.contains\(`, `\b(error|err)\.contains\(`, `(?i)sleep\(`, `a|b`, `(foo)+bar`,
	}
	lines := []string{
		"", "if stderr.contains(\"x\")", "err.contains(", "my_err.contains(", "Sleep(1)", "a", "foofoobar",
		"stderr .contains(", "foo bar", "b",
	}
	for _, expr := range exprs {
		re := regexp.MustCompile(expr)
		gated := gateRegexp(re)
		for _, line := range lines {
			if gated.MatchString(line) != re.MatchString(line) {
				t.Errorf("%q on %q: gated %v, regexp %v", expr, line, gated.MatchString(line), re.MatchString(line))
			}
			if (gated.FindAllStringSubmatch(line, -1) == nil) != (re.FindAllStringSubmatch(line, -1) == nil) {
				t.Errorf("%q on %q: FindAllStringSubmatch disagrees", expr, line)
			}
		}
	}
}
