package checks

import (
	"regexp"
	"strings"
)

// logMacroPattern finds a log macro invocation in masked source: the `log` crate's
// five level macros and `log!` (bare or path-qualified), plus the app's
// `log_error!`. The `\b` keeps `eprintln!` and `debug_assert!` out, and the
// delimiter after the `!` keeps `macro_rules! log_error {` out.
var logMacroPattern = regexp.MustCompile(`\b(?:trace|debug|info|warn|error|log|log_error)\s*!\s*[(\[{]`)

// formatMacroPattern finds the formatting macros whose arguments a log macro's
// argument can nest (judged as part of it), and that turn a marked value into a
// `String` when they sit outside one.
var formatMacroPattern = regexp.MustCompile(`\bformat(?:_args)?\s*!\s*[(\[{]`)

// logWrapperPattern names the `cmdr_fs` log wrappers. An argument built through
// one of them already marks its values for the bundle's span pass.
var logWrapperPattern = regexp.MustCompile(`\b(?:LogPrivate|LogDetail|LoggedText|LogError)\b`)

// markedStringFormatPattern names the wrappers whose `format!` output is a marked
// string. `LoggedText` is left out on purpose: it re-quotes our own log text,
// which already carries its markers (the Flow B auto note).
var markedStringFormatPattern = regexp.MustCompile(`\b(?:LogPrivate|LogDetail|LogError)\b`)

// markedToStringPattern finds a `LogPrivate` / `LogError` constructor call, whose
// `.to_string()` would turn log text into data.
var markedToStringPattern = regexp.MustCompile(`\b(?:LogPrivate\s*::\s*[a-z_]+|LogError)\s*\(`)

// namedArgPattern reads a `name = expr` format argument.
var namedArgPattern = regexp.MustCompile(`^\s*[A-Za-z_][A-Za-z0-9_]*\s*=([^=]|$)`)

// lossyConversionPattern is the conversion that prints a path or OS string bare.
var lossyConversionPattern = regexp.MustCompile(`\.\s*(display|to_string_lossy)\s*\(\s*\)`)

// logPrivateFile is one file's source, masked twin, and the indexes the judging
// steps share.
type logPrivateFile struct {
	source     string
	masked     string
	literals   map[int]rustLiteral // keyed by start offset
	lineStarts []int
	// errors is the tree's path-carrying error index; errorBindings the names this
	// file declares with one of those types (`e: &VolumeError`).
	errors        *pathErrorIndex
	errorBindings map[string]bool
}

func (f *logPrivateFile) lineOf(offset int) int { return lineForOffset(f.lineStarts, offset) }

// snippet returns the source text of [start, end) on one line, for the report.
func (f *logPrivateFile) snippet(start, end int) string {
	return strings.Join(strings.Fields(f.source[start:end]), " ")
}

// scanRustSourceForLogPrivate scans one file's source: the log macros for
// unmarked values, the rest for marked strings, and the opt-out directives for
// orphans.
func scanRustSourceForLogPrivate(src rustSourceFile, index *pathErrorIndex) logPrivateFileFindings {
	source, masked, relPath := src.source, src.masked, src.relPath
	file := &logPrivateFile{
		source: source, masked: masked, literals: map[int]rustLiteral{}, lineStarts: lineStartOffsets(source),
		errors: index, errorBindings: index.bindingsIn(masked),
	}
	for _, lit := range src.literals {
		file.literals[lit.start] = lit
	}
	testRegions := findRustTestRegions(masked)
	lines := strings.Split(source, "\n")
	tracker := newDirectiveTracker(AllowLogPrivateComment, "//")
	for i, line := range lines {
		if !inRegions(testRegions, file.lineStarts[i]) {
			tracker.observe(i+1, line)
		}
	}
	excused := func(line int) bool {
		prev := ""
		if line >= 2 {
			prev = lines[line-2]
		}
		cur := lines[line-1]
		if !strings.Contains(prev, AllowLogPrivateComment) && !strings.Contains(cur, AllowLogPrivateComment) {
			return false
		}
		tracker.markUsed(line, cur, prev)
		return true
	}

	var findings logPrivateFileFindings
	var logSpans [][2]int
	for _, loc := range logMacroPattern.FindAllStringIndex(masked, -1) {
		open := loc[1] - 1
		closeAt := matchDelim(masked, open, masked[open], closingDelim(masked[open]))
		if closeAt < 0 || inRegions(testRegions, loc[0]) || insideSpans(logSpans, loc[0]) {
			continue
		}
		logSpans = append(logSpans, [2]int{open, closeAt})
		for _, s := range file.judgeFormatInvocation(open, closeAt) {
			if excused(s.line) {
				continue
			}
			s.relPath = relPath
			findings.sites = append(findings.sites, s)
		}
	}

	for _, line := range file.markedStringConversions(logSpans, testRegions) {
		if !excused(line) {
			findings.conversions = append(findings.conversions, markedStringSite{
				relPath: relPath, line: line, text: strings.TrimSpace(lines[line-1]),
			})
		}
	}
	findings.orphans = tracker.orphans(relPath)
	return findings
}

func closingDelim(open byte) byte {
	switch open {
	case '[':
		return ']'
	case '{':
		return '}'
	}
	return ')'
}

func insideSpans(spans [][2]int, offset int) bool {
	for _, s := range spans {
		if offset > s[0] && offset < s[1] {
			return true
		}
	}
	return false
}

// judgeFormatInvocation judges the arguments of the formatting macro whose
// delimiters sit at open/closeAt. The format string is its first argument that is
// a bare string literal (after `target: …` or a level); a macro with none (a
// `macro_rules!` forwarding `$($arg)*`) has nothing to judge.
func (f *logPrivateFile) judgeFormatInvocation(open, closeAt int) []logPrivateSite {
	var fmtLit rustLiteral
	found := false
	var sites []logPrivateSite
	named := map[string]bool{}
	for _, a := range splitTopLevelArgs(f.masked, open+1, closeAt) {
		if !found {
			if lit, ok := f.literals[a[0]]; ok && lit.end == a[1] {
				fmtLit, found = lit, true
			}
			continue
		}
		arg := f.masked[a[0]:a[1]]
		if namedArgPattern.MatchString(arg) {
			name, _, _ := strings.Cut(arg, "=")
			named[strings.TrimSpace(name)] = true
		}
		sites = append(sites, f.judgeArg(a[0], a[1])...)
	}
	if !found {
		return nil
	}
	sites = append(sites, f.judgeCaptures(fmtLit, named)...)
	return sites
}

// judgeCaptures flags each inline capture (`{path:?}`) of a variable whose name
// reads as a path or identity. `{{` is a literal brace; a capture naming one of
// the macro's `name = …` arguments is judged through that argument instead, and a
// capitalized one is a constant of ours (`{STATE_FILE_PATH}`), never a user value.
func (f *logPrivateFile) judgeCaptures(lit rustLiteral, named map[string]bool) []logPrivateSite {
	var sites []logPrivateSite
	content := f.source[lit.contentStart:lit.contentEnd]
	for i := 0; i < len(content); i++ {
		if content[i] != '{' {
			continue
		}
		if i+1 < len(content) && content[i+1] == '{' {
			i++
			continue
		}
		end := strings.IndexByte(content[i:], '}')
		if end < 0 {
			break
		}
		spec := content[i+1 : i+end]
		name := spec
		if colon := strings.IndexByte(spec, ':'); colon >= 0 {
			name = spec[:colon]
		}
		name = strings.TrimSpace(name)
		if name != "" && !named[name] && !(name[0] >= 'A' && name[0] <= 'Z') {
			what := ""
			switch {
			case readsAsIdentity(name):
				what = "capture"
			case f.errorBindings[name]:
				what = "path-carrying error: " + name
			}
			if what != "" {
				at := lit.contentStart + i
				sites = append(sites, logPrivateSite{
					line: f.lineOf(at), what: what, text: content[i : i+end+1],
				})
			}
		}
		i += end
	}
	return sites
}

// judgeArg judges one format argument, [start, end) in the file. A wrapped value
// is clean; otherwise a bare conversion, or an identifier that reads as a path or
// identity and isn't reduced to a scalar, makes it one unmarked value. A
// `format!` nested inside is judged as its own invocation, once.
func (f *logPrivateFile) judgeArg(start, end int) []logPrivateSite {
	start, end = trimSpan(f.masked, start, end)
	if start >= end {
		return nil
	}
	exprStart := start
	if m := namedArgPattern.FindStringIndex(f.masked[start:end]); m != nil {
		exprStart = start + strings.IndexByte(f.masked[start:end], '=') + 1
	}
	expr := []byte(f.masked[exprStart:end])
	if logWrapperPattern.Match(expr) {
		return nil
	}

	var sites []logPrivateSite
	for _, loc := range formatMacroPattern.FindAllIndex(expr, -1) {
		open := exprStart + loc[1] - 1
		closeAt := matchDelim(f.masked, open, f.masked[open], closingDelim(f.masked[open]))
		if closeAt < 0 || closeAt >= end {
			continue
		}
		sites = append(sites, f.judgeFormatInvocation(open, closeAt)...)
		blankRun(expr, loc[0], closeAt+1-exprStart)
	}

	line := f.lineOf(start)
	text := f.snippet(start, end)
	if m := lossyConversionPattern.FindSubmatch(expr); m != nil {
		return append(sites, logPrivateSite{line: line, what: string(m[1]), text: text})
	}
	if isScalarTail(lastChainCall(string(expr))) {
		return sites
	}
	if ident := firstIdentityIdent(string(expr)); ident != "" {
		return append(sites, logPrivateSite{line: line, what: "name: " + ident, text: text})
	}
	if name := f.pathCarryingError(string(expr)); name != "" {
		sites = append(sites, logPrivateSite{line: line, what: "path-carrying error: " + name, text: text})
	}
	return sites
}

func trimSpan(s string, start, end int) (int, int) {
	for start < end && isRustSpaceByte(s[start]) {
		start++
	}
	for end > start && isRustSpaceByte(s[end-1]) {
		end--
	}
	return start, end
}

// splitTopLevelArgs splits masked[from:to) at the commas that sit outside every
// bracket, including a turbofish's `<…>` (`collect::<HashMap<_, _>>()`).
func splitTopLevelArgs(masked string, from, to int) [][2]int {
	var args [][2]int
	depth, angle := 0, 0
	argStart := from
	for i := from; i < to; i++ {
		c := masked[i]
		switch {
		case strings.IndexByte("([{", c) >= 0:
			depth++
		case strings.IndexByte(")]}", c) >= 0:
			depth--
		case c == '<' && (angle > 0 || opensTurbofish(masked[from:i])):
			angle++
		case c == '>' && angle > 0 && masked[i-1] != '-':
			angle--
		case c == ',' && depth == 0 && angle == 0:
			args = append(args, [2]int{argStart, i})
			argStart = i + 1
		}
	}
	if s, e := trimSpan(masked, argStart, to); s < e {
		args = append(args, [2]int{argStart, to})
	}
	return trimSpans(masked, args)
}

// opensTurbofish reports whether a `<` right after `before` opens a turbofish.
func opensTurbofish(before string) bool {
	return strings.HasSuffix(strings.TrimRight(before, " \t\n"), "::")
}

func trimSpans(s string, spans [][2]int) [][2]int {
	for i, a := range spans {
		start, end := trimSpan(s, a[0], a[1])
		spans[i] = [2]int{start, end}
	}
	return spans
}

// markedStringConversions returns the lines outside every log macro (and every
// test region) where a marked value becomes a `String`: a `format!` whose
// arguments hold a `LogPrivate` or `LogDetail`, or a `LogPrivate::…(…)` call
// followed by `.to_string()`.
func (f *logPrivateFile) markedStringConversions(logSpans, testRegions [][2]int) []int {
	var lines []int
	outside := func(offset int) bool {
		return !insideSpans(logSpans, offset) && !inRegions(testRegions, offset)
	}
	for _, loc := range formatMacroPattern.FindAllStringIndex(f.masked, -1) {
		if strings.HasPrefix(f.masked[loc[0]:], "format_args") {
			continue // a borrowed `Arguments`, never a `String`
		}
		open := loc[1] - 1
		closeAt := matchDelim(f.masked, open, f.masked[open], closingDelim(f.masked[open]))
		if closeAt < 0 || !outside(loc[0]) {
			continue
		}
		if markedStringFormatPattern.MatchString(f.masked[open:closeAt]) {
			lines = append(lines, f.lineOf(loc[0]))
		}
	}
	for _, loc := range markedToStringPattern.FindAllStringIndex(f.masked, -1) {
		open := loc[1] - 1
		closeAt := matchDelim(f.masked, open, '(', ')')
		if closeAt < 0 || !outside(loc[0]) {
			continue
		}
		after := strings.TrimLeft(f.masked[closeAt+1:], " \t\n")
		if strings.HasPrefix(after, ".to_string()") {
			lines = append(lines, f.lineOf(loc[0]))
		}
	}
	return lines
}
