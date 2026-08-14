package checks

import (
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
)

// The discarded-outcome scanner.
//
// The defect it catches, in one line: a function that returns NOTHING, calling
// one that returns a typed answer, and dropping it on the floor. By the time the
// value reaches an IPC command or an MCP tool it no longer exists, so the surface
// invents a success — "OK: Paused every running operation" for a queue that never
// stopped.
//
// Why a check rather than a review habit: the same shape shipped three times
// (`resolve_write_conflict`, `pause_operation`, `pause_all` / `resume_all`), each
// time through a thin delegating wrapper that looked correct on its own.
//
// ## What is (and isn't) flagged
//
// A call is a violation when ALL of these hold:
//
//   - it sits in statement position and its value is discarded (the statement is
//     `foo(args);`, not `let x = foo(..)`, `foo(..)?`, or `foo(..).bar()`);
//   - the enclosing function declares no return type;
//   - the callee is a free function defined somewhere in the scanned tree, and
//     EVERY definition of that name returns the same meaningful type.
//
// `Result<…>` and `Option<…>` returns are deliberately NOT flagged: `Result` is
// `#[must_use]`, so the compiler already warns, and an `Option` discard is
// usually a map/set idiom. This check fills exactly the gap the compiler leaves:
// a plain `bool` or a bare outcome enum, which carry no `#[must_use]` and vanish
// in silence.
//
// Ambiguity always resolves to "don't flag": an unresolvable name, a name with
// two definitions disagreeing on their return type, a method call, a macro. A
// check people learn to ignore is worse than no check.
const discardedOutcomeDirective = "// allowed-discarded-outcome:"

// rustFnRef is one definition of a name: where it lives and what it returns. The
// path matters because a thin IPC command routinely shares its name with the
// function it delegates to, importing that one under an alias.
type rustFnRef struct {
	path       string
	returnType string
}

// rustFnDecl is one parsed free-function definition: its name, its declared
// return type ("" for unit), and the byte range of its body.
type rustFnDecl struct {
	name       string
	returnType string
	bodyStart  int // index of the byte after the opening `{`
	bodyEnd    int // index of the closing `}`
	line       int // 1-based line of the `fn` keyword
}

// discardedOutcomeSite is one flagged call.
type discardedOutcomeSite struct {
	relPath    string
	line       int
	text       string
	caller     string
	callee     string
	returnType string
}

var (
	rustFnKeyword  = regexp.MustCompile(`\bfn\s+([A-Za-z_]\w*)`)
	rustUseAlias   = regexp.MustCompile(`\b([A-Za-z_]\w*)\s+as\s+([A-Za-z_]\w*)`)
	rustCallInStmt = regexp.MustCompile(`(?:^|[;{}])[\s]*((?:[A-Za-z_]\w*::)*)([a-z_]\w*)\s*\(`)
)

// rustStatementKeywords are the words that read like a call in statement
// position but aren't one.
var rustStatementKeywords = map[string]bool{
	"if": true, "match": true, "while": true, "for": true, "loop": true,
	"return": true, "else": true, "fn": true, "let": true, "unsafe": true,
	"move": true, "async": true, "await": true, "break": true, "continue": true,
	"yield": true, "impl": true, "mod": true, "use": true, "struct": true,
	"enum": true, "trait": true, "const": true, "static": true, "type": true,
	"where": true, "pub": true, "crate": true, "super": true, "self": true,
	"Self": true, "as": true, "in": true, "ref": true, "dyn": true,
}

// RunDiscardedOutcome scans the desktop app's Rust tree for a unit-returning
// function that throws away a typed answer from the function it delegates to.
// Rationale and the full rule: the comment on `discardedOutcomeDirective`, and
// `apps/desktop/src-tauri/src/mcp/DETAILS.md` § Queue for the case that motivated
// it.
func RunDiscardedOutcome(ctx *CheckContext) (CheckResult, error) {
	roots, err := ScannerRoots(ctx.RootDir, "desktop-rust-discarded-outcome")
	if err != nil {
		return CheckResult{}, err
	}

	files, err := collectRustFiles(roots)
	if err != nil {
		return CheckResult{}, err
	}

	// Pass 1: index every free function's return type across the whole tree, so
	// a call site can be resolved without a compiler. The defining file rides
	// along: it's what lets an aliased import resolve past a same-named local
	// function.
	index := map[string][]rustFnRef{}
	parsed := make([]*parsedRustFile, 0, len(files))
	for _, path := range files {
		file, parseErr := parseRustFile(path)
		if parseErr != nil {
			return CheckResult{}, parseErr
		}
		parsed = append(parsed, file)
		for _, fn := range file.fns {
			index[fn.name] = append(index[fn.name], rustFnRef{path: path, returnType: fn.returnType})
		}
	}

	// Pass 2: walk the unit-returning functions and flag the discards.
	var violations []discardedOutcomeSite
	var orphans []orphanDirective
	for _, file := range parsed {
		relPath, relErr := filepath.Rel(ctx.RootDir, file.path)
		if relErr != nil {
			relPath = file.path
		}
		relPath = filepath.ToSlash(relPath)
		fileViolations, fileOrphans := scanFileForDiscardedOutcomes(file, relPath, index)
		violations = append(violations, fileViolations...)
		orphans = append(orphans, fileOrphans...)
	}

	if len(violations) > 0 || len(orphans) > 0 {
		var sb strings.Builder
		if len(violations) > 0 {
			sort.Slice(violations, func(i, j int) bool {
				if violations[i].relPath == violations[j].relPath {
					return violations[i].line < violations[j].line
				}
				return violations[i].relPath < violations[j].relPath
			})
			sb.WriteString(fmt.Sprintf(
				"found %d discarded %s: a function that returns nothing is throwing away a typed answer, so no surface above it can say what happened.\n"+
					"Return the value (the `PauseOutcome` / `ConflictResolutionOutcome` pattern), or opt out with `%s <reason>` on the line above:\n",
				len(violations), Pluralize(len(violations), "outcome", "outcomes"), discardedOutcomeDirective,
			))
			for _, v := range violations {
				sb.WriteString(fmt.Sprintf(
					"  %s:%d: `%s` drops `%s`'s `%s`\n    %s\n",
					v.relPath, v.line, v.caller, v.callee, v.returnType, v.text,
				))
			}
		}
		if len(orphans) > 0 {
			if sb.Len() > 0 {
				sb.WriteString("\n")
			}
			sb.WriteString(formatOrphanDirectives(discardedOutcomeDirective, orphans))
		}
		return CheckResult{}, fmt.Errorf("%s", strings.TrimRight(sb.String(), "\n"))
	}

	return Success(fmt.Sprintf(
		"%d Rust %s scanned, no function drops an answer its caller needs",
		len(files), Pluralize(len(files), "file", "files"),
	)), nil
}

// parsedRustFile is one file reduced to what the scanner needs: the masked
// source (comments and string literals blanked so a match is always real code),
// the original lines for reporting, its `use … as …` aliases, and its free
// functions.
type parsedRustFile struct {
	path        string
	masked      string
	lines       []string
	lineStarts  []int
	aliases     map[string]string // local alias → original name
	fns         []rustFnDecl
	testRegions [][2]int // byte ranges of `#[cfg(test)]` items, skipped wholesale
	directives  *directiveTracker
}

func collectRustFiles(roots []string) ([]string, error) {
	var files []string
	for _, root := range roots {
		err := filepath.WalkDir(root, func(path string, d os.DirEntry, err error) error {
			if err != nil {
				return err
			}
			if d.IsDir() || !strings.HasSuffix(d.Name(), ".rs") {
				return nil
			}
			files = append(files, path)
			return nil
		})
		if err != nil {
			return nil, fmt.Errorf("failed to walk %s: %w", root, err)
		}
	}
	sort.Strings(files)
	return files, nil
}

func parseRustFile(path string) (*parsedRustFile, error) {
	raw, err := os.ReadFile(path)
	if err != nil {
		return nil, fmt.Errorf("failed to read %s: %w", path, err)
	}
	source := string(raw)
	file := &parsedRustFile{
		path:       path,
		masked:     maskRustNonCode(source),
		lines:      strings.Split(source, "\n"),
		aliases:    map[string]string{},
		directives: newDirectiveTracker(discardedOutcomeDirective, "//"),
	}
	file.lineStarts = lineStartOffsets(source)

	for _, match := range rustUseAlias.FindAllStringSubmatch(file.masked, -1) {
		file.aliases[match[2]] = match[1]
	}
	file.testRegions = findRustTestRegions(file.masked)
	file.fns = findRustFns(file.masked, file.lineStarts)
	return file, nil
}

// maskRustNonCode replaces every comment and string/char literal with spaces,
// keeping byte offsets and newlines intact. Everything downstream can then match
// on real code without re-deriving "is this inside a comment".
func maskRustNonCode(source string) string {
	out := []byte(source)
	for i := 0; i < len(out); i++ {
		end := endOfMaskedRun(out, i)
		if end <= i {
			continue
		}
		blankRun(out, i, end)
		i = end - 1
	}
	return string(out)
}

// endOfMaskedRun returns the exclusive end of the comment or literal starting at
// `i`, or `i` when nothing starts there.
func endOfMaskedRun(out []byte, i int) int {
	switch {
	case hasPairAt(out, i, '/', '/'):
		if end := indexOfByteFrom(out, i, '\n'); end >= 0 {
			return end
		}
		return len(out)
	case hasPairAt(out, i, '/', '*'):
		return endOfBlockComment(out, i)
	case out[i] == '"':
		return endOfStringLiteral(out, i)
	}
	return i
}

// blankRun replaces [from, to) with spaces, leaving newlines in place so byte
// offsets and line numbers both survive.
func blankRun(out []byte, from, to int) {
	for i := from; i < to && i < len(out); i++ {
		if out[i] != '\n' {
			out[i] = ' '
		}
	}
}

func hasPairAt(out []byte, i int, first, second byte) bool {
	return out[i] == first && i+1 < len(out) && out[i+1] == second
}

func indexOfByteFrom(out []byte, from int, b byte) int {
	for i := from; i < len(out); i++ {
		if out[i] == b {
			return i
		}
	}
	return -1
}

// endOfBlockComment handles Rust's NESTED `/* /* */ */`, which C-style scanning
// would close one level early.
func endOfBlockComment(out []byte, start int) int {
	depth := 1
	for i := start + 2; i < len(out); {
		switch {
		case hasPairAt(out, i, '/', '*'):
			depth++
			i += 2
		case hasPairAt(out, i, '*', '/'):
			depth--
			i += 2
			if depth == 0 {
				return i
			}
		default:
			i++
		}
	}
	return len(out)
}

func endOfStringLiteral(out []byte, start int) int {
	for i := start + 1; i < len(out); {
		switch out[i] {
		case '\\':
			i += 2
		case '"':
			return i + 1
		default:
			i++
		}
	}
	return len(out)
}

func lineStartOffsets(source string) []int {
	starts := []int{0}
	for i := 0; i < len(source); i++ {
		if source[i] == '\n' {
			starts = append(starts, i+1)
		}
	}
	return starts
}

// lineOf maps a byte offset to a 1-based line number.
func (f *parsedRustFile) lineOf(offset int) int {
	lo, hi := 0, len(f.lineStarts)-1
	for lo < hi {
		mid := (lo + hi + 1) / 2
		if f.lineStarts[mid] <= offset {
			lo = mid
		} else {
			hi = mid - 1
		}
	}
	return lo + 1
}

// findRustTestRegions returns the byte ranges of `#[cfg(test)]` items. Test code
// discards values on purpose all the time, and a test that drops an outcome
// misleads nobody.
func findRustTestRegions(masked string) [][2]int {
	var regions [][2]int
	for _, marker := range []string{"#[cfg(test)]", "#[test]", "#[tokio::test"} {
		search := 0
		for {
			idx := strings.Index(masked[search:], marker)
			if idx < 0 {
				break
			}
			start := search + idx
			open := strings.IndexByte(masked[start:], '{')
			if open < 0 {
				break
			}
			end := matchBrace(masked, start+open)
			if end < 0 {
				break
			}
			regions = append(regions, [2]int{start, end})
			search = start + len(marker)
		}
	}
	return regions
}

func inRegions(regions [][2]int, offset int) bool {
	for _, r := range regions {
		if offset >= r[0] && offset <= r[1] {
			return true
		}
	}
	return false
}

// findRustFns parses every `fn` in the masked source into a declaration with its
// return type and body range. A signature that can't be parsed is skipped rather
// than guessed at.
func findRustFns(masked string, lineStarts []int) []rustFnDecl {
	var fns []rustFnDecl
	for _, loc := range rustFnKeyword.FindAllStringSubmatchIndex(masked, -1) {
		name := masked[loc[2]:loc[3]]
		// Step over the generic parameter list before hunting for the parameter
		// list's `(`. A bound can contain one (`fn f<F: FnOnce(&mut S) -> bool>(…)`),
		// and taking the first `(` there parses the bound as the signature.
		afterGenerics := skipRustGenerics(masked, loc[3])
		if afterGenerics < 0 {
			continue
		}
		paren := strings.IndexByte(masked[afterGenerics:], '(')
		if paren < 0 {
			continue
		}
		openParen := afterGenerics + paren
		closeParen := matchDelim(masked, openParen, '(', ')')
		if closeParen < 0 {
			continue
		}
		// Everything between the parameter list and the body opener is the return
		// type plus an optional `where` clause.
		bodyOpen := indexOfBodyBrace(masked, closeParen+1)
		if bodyOpen < 0 {
			continue
		}
		// Cut the `where` clause BEFORE looking for `->`: its bounds carry arrows of
		// their own (`where F: FnOnce(&mut S) -> bool`), and reading one of those as
		// the function's return type turns a unit function into a `bool` one.
		tail := trimRustWhereClause(masked[closeParen+1 : bodyOpen])
		returnType := ""
		if arrow := strings.Index(tail, "->"); arrow >= 0 {
			returnType = strings.Join(strings.Fields(tail[arrow+2:]), " ")
		}
		bodyEnd := matchBrace(masked, bodyOpen)
		if bodyEnd < 0 {
			continue
		}
		fns = append(fns, rustFnDecl{
			name:       name,
			returnType: returnType,
			bodyStart:  bodyOpen + 1,
			bodyEnd:    bodyEnd,
			line:       lineForOffset(lineStarts, loc[0]),
		})
	}
	return fns
}

// skipRustGenerics returns the offset just past a `<…>` generic parameter list
// starting at (or after whitespace from) `from`, or `from` itself when there
// isn't one. Arrows inside bounds are stepped over so `-> bool>` doesn't close
// the list one `>` early.
func skipRustGenerics(masked string, from int) int {
	start := skipRustSpace(masked, from)
	if start >= len(masked) || masked[start] != '<' {
		return from
	}
	depth := 0
	for i := start; i < len(masked); i++ {
		if masked[i] == '-' && i+1 < len(masked) && masked[i+1] == '>' {
			i++ // an arrow, not a closing bracket
			continue
		}
		switch masked[i] {
		case '<':
			depth++
		case '>':
			depth--
			if depth == 0 {
				return i + 1
			}
		case '{', ';':
			return -1 // ran past the signature; don't guess
		}
	}
	return -1
}

func skipRustSpace(masked string, from int) int {
	i := from
	for i < len(masked) && isRustSpaceByte(masked[i]) {
		i++
	}
	return i
}

func isRustSpaceByte(b byte) bool {
	return b == ' ' || b == '\t' || b == '\n' || b == '\r'
}

// trimRustWhereClause cuts a trailing `where` clause off a signature tail,
// matching the keyword as a whole word so a type named `somewhere` survives.
func trimRustWhereClause(tail string) string {
	search := 0
	for {
		idx := strings.Index(tail[search:], "where")
		if idx < 0 {
			return tail
		}
		at := search + idx
		beforeOK := at == 0 || !isRustIdentByte(tail[at-1])
		afterAt := at + len("where")
		afterOK := afterAt >= len(tail) || !isRustIdentByte(tail[afterAt])
		if beforeOK && afterOK {
			return tail[:at]
		}
		search = afterAt
	}
}

func isRustIdentByte(b byte) bool {
	return b == '_' || (b >= 'a' && b <= 'z') || (b >= 'A' && b <= 'Z') || (b >= '0' && b <= '9')
}

func lineForOffset(lineStarts []int, offset int) int {
	lo, hi := 0, len(lineStarts)-1
	for lo < hi {
		mid := (lo + hi + 1) / 2
		if lineStarts[mid] <= offset {
			lo = mid
		} else {
			hi = mid - 1
		}
	}
	return lo + 1
}

// indexOfBodyBrace finds the `{` that opens a function body, skipping over the
// braces a `where` clause or a return type can contain (`-> impl Fn() {`, an
// associated-type bound, a closure default). It stops at a `;`, which means the
// item is a trait method declaration with no body.
func indexOfBodyBrace(masked string, from int) int {
	depth := 0
	for i := from; i < len(masked); i++ {
		switch masked[i] {
		case ';':
			if depth == 0 {
				return -1
			}
		case '<', '(', '[':
			depth++
		case '>', ')', ']':
			if depth > 0 {
				depth--
			}
		case '{':
			return i
		}
	}
	return -1
}

func matchBrace(masked string, open int) int {
	return matchDelim(masked, open, '{', '}')
}

func matchDelim(masked string, open int, openCh, closeCh byte) int {
	if open >= len(masked) || masked[open] != openCh {
		return -1
	}
	depth := 0
	for i := open; i < len(masked); i++ {
		switch masked[i] {
		case openCh:
			depth++
		case closeCh:
			depth--
			if depth == 0 {
				return i
			}
		}
	}
	return -1
}

// scanFileForDiscardedOutcomes walks the file's unit-returning functions and
// reports each statement-position call whose typed answer is dropped.
func scanFileForDiscardedOutcomes(
	file *parsedRustFile, relPath string, index map[string][]rustFnRef,
) ([]discardedOutcomeSite, []orphanDirective) {
	var violations []discardedOutcomeSite

	for lineNum, line := range file.lines {
		if !inRegions(file.testRegions, file.lineStarts[lineNum]) {
			file.directives.observe(lineNum+1, line)
		}
	}

	for _, fn := range file.fns {
		if fn.returnType != "" || inRegions(file.testRegions, fn.bodyStart) {
			continue
		}
		body := file.masked[fn.bodyStart:fn.bodyEnd]
		for _, loc := range rustCallInStmt.FindAllStringSubmatchIndex(body, -1) {
			site, flagged := inspectCallSite(file, relPath, index, fn, loc)
			if flagged {
				violations = append(violations, site)
			}
		}
	}

	return violations, file.directives.orphans(relPath)
}

// inspectCallSite decides one candidate call: is it really a free-function call
// in this function's own body, is its value dropped, does the callee resolve to
// something worth keeping, and has somebody already justified the discard.
func inspectCallSite(
	file *parsedRustFile, relPath string, index map[string][]rustFnRef, fn rustFnDecl, loc []int,
) (discardedOutcomeSite, bool) {
	body := file.masked[fn.bodyStart:fn.bodyEnd]
	callee := body[loc[4]:loc[5]]
	if rustStatementKeywords[callee] {
		return discardedOutcomeSite{}, false
	}
	// A nested `fn` inside this body belongs to itself; its own declaration is
	// scanned separately.
	callOffset := fn.bodyStart + loc[4]
	if offsetIsInsideNestedFn(file.fns, fn, callOffset) {
		return discardedOutcomeSite{}, false
	}
	if !callValueIsDiscarded(file.masked, fn.bodyStart+loc[1]-1) {
		return discardedOutcomeSite{}, false
	}
	returnType, ok := resolveCalleeReturn(index, file, callee)
	if !ok || !isMeaningfulReturn(returnType) {
		return discardedOutcomeSite{}, false
	}

	lineNum := file.lineOf(callOffset)
	text, prev := file.lineAndPrev(lineNum)
	if strings.Contains(text, discardedOutcomeDirective) || strings.Contains(prev, discardedOutcomeDirective) {
		file.directives.markUsed(lineNum, text, prev)
		return discardedOutcomeSite{}, false
	}
	return discardedOutcomeSite{
		relPath:    relPath,
		line:       lineNum,
		text:       strings.TrimSpace(text),
		caller:     fn.name,
		callee:     callee,
		returnType: returnType,
	}, true
}

// lineAndPrev returns the (1-based) line's text and the one above it, which is
// where an opt-out directive is allowed to sit.
func (f *parsedRustFile) lineAndPrev(lineNum int) (string, string) {
	text, prev := "", ""
	if lineNum-1 >= 0 && lineNum-1 < len(f.lines) {
		text = f.lines[lineNum-1]
	}
	if lineNum-2 >= 0 && lineNum-2 < len(f.lines) {
		prev = f.lines[lineNum-2]
	}
	return text, prev
}

// offsetIsInsideNestedFn reports whether the offset falls inside a DIFFERENT
// function declared within the outer one, so an inner helper's body isn't
// attributed to its enclosing unit-returning function.
func offsetIsInsideNestedFn(fns []rustFnDecl, outer rustFnDecl, offset int) bool {
	for _, other := range fns {
		if other.bodyStart == outer.bodyStart {
			continue
		}
		if other.bodyStart > outer.bodyStart && other.bodyEnd < outer.bodyEnd &&
			offset >= other.bodyStart && offset <= other.bodyEnd {
			return true
		}
	}
	return false
}

// callValueIsDiscarded reports whether the call's value goes nowhere: the next
// thing after its closing paren is a `;`. A `?`, a `.method()` chain, an
// `.await`, or an operator all mean the value is being used.
func callValueIsDiscarded(masked string, openParen int) bool {
	closeParen := matchDelim(masked, openParen, '(', ')')
	if closeParen < 0 {
		return false
	}
	for i := closeParen + 1; i < len(masked); i++ {
		switch masked[i] {
		case ' ', '\t', '\n', '\r':
			continue
		case ';':
			return true
		default:
			return false
		}
	}
	return false
}

// resolveCalleeReturn looks the callee up in the crate-wide index, following a
// local `use … as …` alias. It answers only when the name is UNAMBIGUOUS: two
// definitions that disagree about their return type resolve to "don't know",
// which resolves to "don't flag".
//
// An ALIASED name drops the definitions in the calling file first. The alias
// exists BECAUSE the plain name is taken locally (`pause_all as ops_pause_all`,
// inside the command that is itself called `pause_all`), so the local definition
// is the one thing the call certainly isn't. Without that step the check would
// miss precisely the thin-command shape it was written for.
func resolveCalleeReturn(index map[string][]rustFnRef, file *parsedRustFile, callee string) (string, bool) {
	name := callee
	original, aliased := file.aliases[callee]
	if aliased {
		name = original
	}
	candidates := index[name]
	if aliased {
		var elsewhere []rustFnRef
		for _, candidate := range candidates {
			if candidate.path != file.path {
				elsewhere = append(elsewhere, candidate)
			}
		}
		candidates = elsewhere
	}
	returnTypes := map[string]bool{}
	for _, candidate := range candidates {
		returnTypes[candidate.returnType] = true
	}
	if len(returnTypes) != 1 {
		return "", false
	}
	for returnType := range returnTypes {
		return returnType, true
	}
	return "", false
}

// isMeaningfulReturn reports whether dropping this return type loses something.
//
// `Result` and `Option` are excluded on purpose. `Result` is `#[must_use]`, so
// the compiler already warns about the discard; `Option` is the shape of the
// map/set idioms (`insert` returning the displaced value) where dropping it is
// the normal thing to do. What is left — `bool` and the named outcome types — is
// exactly the gap: no `#[must_use]`, no warning, no answer.
func isMeaningfulReturn(returnType string) bool {
	trimmed := strings.TrimSpace(returnType)
	switch {
	case trimmed == "", trimmed == "()", trimmed == "!":
		return false
	case strings.HasPrefix(trimmed, "Result<"), strings.HasPrefix(trimmed, "Result <"):
		return false
	case strings.HasPrefix(trimmed, "Option<"), strings.HasPrefix(trimmed, "Option <"):
		return false
	// A generic parameter tells us nothing about what the caller loses.
	case len(trimmed) == 1:
		return false
	}
	return true
}
