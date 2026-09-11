package checks

import "strings"

// The TypeScript lexer and CSS selector parser behind `desktop-svelte-e2e-stale-selector`:
// literals in, checkable class and data-* attribute names out.

// e2eSelInterpolation stands in for each `${…}` in a template literal's text. It must
// differ from the parser's end-of-input byte (0), or every loop that consumes it spins
// at the end of a string.
const e2eSelInterpolation = '\x01'

type e2eSelLiteral struct {
	line int
	text string
}

type e2eSelToken struct {
	kind string // "class" or "attribute"
	name string
}

// ---- lexer ----

// e2eSelLexer extracts every string literal from TypeScript source in one pass,
// including the literals inside template interpolations and inside a template's own
// text, which is often JavaScript for `evaluate`.
type e2eSelLexer struct {
	src  string
	i    int
	line int
	out  []e2eSelLiteral
}

// e2eSelKeywordsBeforeRegex are the words after which a `/` starts a regex literal.
var e2eSelKeywordsBeforeRegex = e2eSelWordSet("return typeof case in of yield await void delete instanceof new throw else do")

func lexE2ESelLiterals(src string) []e2eSelLiteral {
	return lexE2ESelLiteralsFrom(src, 1)
}

func lexE2ESelLiteralsFrom(src string, line int) []e2eSelLiteral {
	lx := &e2eSelLexer{src: src, line: line}
	lx.code(false)
	return lx.out
}

func (lx *e2eSelLexer) peekAt(offset int) byte {
	if j := lx.i + offset; j < len(lx.src) {
		return lx.src[j]
	}
	return 0
}

// code lexes JavaScript from lx.i. Inside an interpolation it stops on the `}` that
// closes the `${`, leaving lx.i on it.
func (lx *e2eSelLexer) code(insideInterpolation bool) {
	depth := 0
	regexAllowed := true
	for lx.i < len(lx.src) {
		c := lx.src[lx.i]
		if c == '}' && depth == 0 && insideInterpolation {
			return
		}
		if consumed, value := lx.skipped(c, regexAllowed); consumed {
			if value {
				regexAllowed = false
			}
			continue
		}
		if isE2ESelIdentByte(c) {
			regexAllowed = e2eSelKeywordsBeforeRegex[lx.word()]
			continue
		}
		switch c {
		case '{':
			depth++
		case '}':
			depth--
		}
		// After an operator or an opener a `/` starts a regex; after a closer it divides.
		regexAllowed = c != ')' && c != ']'
		lx.i++
	}
}

// skipped consumes whitespace, a comment, or a string, template, or regex literal at
// lx.i. It reports whether it consumed anything, and whether that was a value, after
// which a `/` divides.
func (lx *e2eSelLexer) skipped(c byte, regexAllowed bool) (consumed, value bool) {
	switch c {
	case '\n':
		lx.line++
		lx.i++
	case ' ', '\t', '\r':
		lx.i++
	case '/':
		return lx.slash(regexAllowed)
	case '\'', '"':
		lx.quoted(c)
		return true, true
	case '`':
		lx.template()
		return true, true
	default:
		return false, false
	}
	return true, false
}

// slash consumes a comment or a regex literal at lx.i. A `/` that's neither stays for
// code() as a division.
func (lx *e2eSelLexer) slash(regexAllowed bool) (consumed, value bool) {
	switch {
	case lx.peekAt(1) == '/':
		for lx.i < len(lx.src) && lx.src[lx.i] != '\n' {
			lx.i++
		}
		return true, false
	case lx.peekAt(1) == '*':
		lx.blockComment()
		return true, false
	case regexAllowed:
		lx.regex()
		return true, true
	}
	return false, false
}

func (lx *e2eSelLexer) word() string {
	start := lx.i
	for lx.i < len(lx.src) && isE2ESelIdentByte(lx.src[lx.i]) {
		lx.i++
	}
	return lx.src[start:lx.i]
}

func (lx *e2eSelLexer) blockComment() {
	lx.i += 2
	for lx.i < len(lx.src) && !(lx.src[lx.i] == '*' && lx.peekAt(1) == '/') {
		if lx.src[lx.i] == '\n' {
			lx.line++
		}
		lx.i++
	}
	lx.i += 2
}

// regex skips a regex literal on the current line, so a quote inside one (`/it's/`)
// can't open a string. Without a closing `/` on the line it was a division after all.
func (lx *e2eSelLexer) regex() {
	inClass := false
	for j := lx.i + 1; j < len(lx.src) && lx.src[j] != '\n'; j++ {
		switch lx.src[j] {
		case '\\':
			j++
		case '[':
			inClass = true
		case ']':
			inClass = false
		case '/':
			if !inClass {
				lx.i = j + 1
				return
			}
		}
	}
	lx.i++
}

// quoted reads a single- or double-quoted string. One that doesn't close on its line
// (an apostrophe in a template's prose) is dropped along with the rest of the line.
func (lx *e2eSelLexer) quoted(quote byte) {
	var sb strings.Builder
	j := lx.i + 1
	for ; j < len(lx.src) && lx.src[j] != '\n'; j++ {
		c := lx.src[j]
		if c == '\\' && j+1 < len(lx.src) {
			j++
			sb.WriteByte(lx.src[j])
			continue
		}
		if c == quote {
			lx.out = append(lx.out, e2eSelLiteral{line: lx.line, text: sb.String()})
			lx.i = j + 1
			return
		}
		sb.WriteByte(c)
	}
	lx.i = j
}

// template reads a template literal. Each `${…}` becomes e2eSelInterpolation in its
// text, and the literals inside one are lexed as code exactly once. The static text is
// then lexed as code too, for the strings in an `evaluate` body.
func (lx *e2eSelLexer) template() {
	startLine := lx.line
	var sb strings.Builder
	lx.i++
	for lx.i < len(lx.src) {
		c := lx.src[lx.i]
		switch {
		case c == '\\' && lx.i+1 < len(lx.src):
			if lx.src[lx.i+1] == '\n' {
				lx.line++
			}
			sb.WriteByte(lx.src[lx.i+1])
			lx.i += 2
		case c == '`':
			lx.i++
			lx.addTemplate(startLine, sb.String())
			return
		case c == '$' && lx.peekAt(1) == '{':
			lx.i += 2
			lx.code(true)
			lx.i++ // the closing brace
			sb.WriteRune(e2eSelInterpolation)
		default:
			if c == '\n' {
				lx.line++
			}
			sb.WriteByte(c)
			lx.i++
		}
	}
	lx.addTemplate(startLine, sb.String())
}

func (lx *e2eSelLexer) addTemplate(line int, text string) {
	lx.out = append(lx.out, e2eSelLiteral{line: line, text: text})
	lx.out = append(lx.out, lexE2ESelLiteralsFrom(text, line)...)
}

func isE2ESelIdentByte(c byte) bool {
	return c == '_' || c == '$' || (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9')
}

// ---- selector parser ----

// e2eSelTokens parses text as a CSS selector list and returns the static class names
// and data-* attribute names it checks, or nil when the text isn't a selector or names
// nothing checkable. Attribute values aren't checked: most are runtime data (file names,
// operation ids), and a stale value like a dialog id can survive in a comment anyway.
func e2eSelTokens(text string) []e2eSelToken {
	s := strings.TrimSpace(text)
	if s == "" || strings.ContainsAny(s, "\n\r") {
		return nil
	}
	p := &e2eSelParser{s: s}
	if !p.selectorList(false) || p.i != len(p.s) || p.rejected {
		return nil
	}
	// A lone `tag.word` or `${base}.word` is a dotted id (`nav.back`) or a file name
	// (`a.txt`, `${name}.png`) far more often than a selector.
	if p.complexes == 1 && len(p.top) == 1 {
		if shape := string(p.top[0]); shape == "tc" || shape == "ic" {
			return nil
		}
	}
	return p.tokens
}

type e2eSelParser struct {
	s         string
	i         int
	depth     int // nesting inside :not() / :is() / :where() / :has()
	complexes int // top-level complex selectors
	top       [][]byte
	tokens    []e2eSelToken
	rejected  bool
}

var e2eSelKnownTags = e2eSelWordSet("a abbr address article aside audio b blockquote body br button canvas caption " +
	"code col dd details dialog div dl dt em fieldset figure footer form h1 h2 h3 h4 h5 h6 header hr html i iframe " +
	"img input kbd label legend li main mark menu nav ol optgroup option output p pre progress section select small " +
	"span strong sub summary sup table tbody td textarea tfoot th thead time tr u ul video svg path circle rect g " +
	"line polyline polygon use")

var e2eSelKnownPseudos = e2eSelWordSet("not is where has matches hover focus focus-visible focus-within active " +
	"checked disabled enabled first-child last-child only-child nth-child nth-last-child nth-of-type " +
	"nth-last-of-type first-of-type last-of-type only-of-type empty root visible hidden has-text text-is text scope " +
	"placeholder-shown read-only read-write required optional invalid valid indeterminate target before after " +
	"placeholder selection")

func e2eSelWordSet(words string) map[string]bool {
	set := make(map[string]bool)
	for _, w := range strings.Fields(words) {
		set[w] = true
	}
	return set
}

func (p *e2eSelParser) peek() byte {
	if p.i < len(p.s) {
		return p.s[p.i]
	}
	return 0
}

func (p *e2eSelParser) ws() bool {
	start := p.i
	for p.i < len(p.s) && (p.s[p.i] == ' ' || p.s[p.i] == '\t') {
		p.i++
	}
	return p.i > start
}

func (p *e2eSelParser) selectorList(relative bool) bool {
	for {
		p.ws()
		if !p.complex(relative) {
			return false
		}
		if p.depth == 0 {
			p.complexes++
		}
		p.ws()
		if p.peek() != ',' {
			return true
		}
		p.i++
	}
}

func (p *e2eSelParser) complex(relative bool) bool {
	if c := p.peek(); relative && (c == '>' || c == '+' || c == '~') {
		p.i++
		p.ws()
	}
	if !p.compound() {
		return false
	}
	for {
		save := p.i
		hadWS := p.ws()
		c := p.peek()
		switch {
		case c == '>' || c == '+' || c == '~':
			p.i++
			p.ws()
		case hadWS && c != 0 && c != ',' && c != ')':
		default:
			p.i = save
			return true
		}
		if !p.compound() {
			return false
		}
	}
}

// compound parses one compound selector, recording the kind of each simple selector in
// it: `t` tag, `i` interpolation, `c` class, `#` id, `a` attribute, `:` pseudo, `*`.
func (p *e2eSelParser) compound() bool {
	var kinds []byte
	switch c := p.peek(); {
	case c == '*':
		p.i++
		kinds = append(kinds, '*')
	case c == e2eSelInterpolation:
		p.i++
		kinds = append(kinds, 'i')
	case isE2ESelNameStart(c):
		start := p.i
		if !e2eSelKnownTags[p.ident()] {
			p.i = start
			return false
		}
		kinds = append(kinds, 't')
	}
	for {
		kind, ok := p.simple()
		if !ok {
			return false
		}
		if kind == 0 {
			break
		}
		kinds = append(kinds, kind)
	}
	if len(kinds) == 0 {
		return false
	}
	if p.depth == 0 {
		p.top = append(p.top, kinds)
	}
	return true
}

// simple parses one class, id, attribute, pseudo, or interpolation. It returns kind 0
// when none starts here.
func (p *e2eSelParser) simple() (byte, bool) {
	switch p.peek() {
	case '.':
		return 'c', p.class()
	case '#':
		return '#', p.id()
	case '[':
		return 'a', p.attribute()
	case ':':
		return ':', p.pseudo()
	case e2eSelInterpolation:
		p.i++
		return 'i', true
	}
	return 0, true
}

func (p *e2eSelParser) class() bool {
	p.i++
	if p.peek() == e2eSelInterpolation {
		p.skipDynamicName()
		return true
	}
	name := p.ident()
	if name == "" {
		return false
	}
	if p.peek() == e2eSelInterpolation {
		// `.prefix-${x}`: built at runtime, so nothing static to check.
		p.skipDynamicName()
		return true
	}
	p.addName("class", name)
	return true
}

func (p *e2eSelParser) id() bool {
	p.i++
	if p.peek() != e2eSelInterpolation && p.ident() == "" {
		return false
	}
	p.skipDynamicName()
	return true
}

func (p *e2eSelParser) attribute() bool {
	p.i++
	p.ws()
	start := p.i
	for isE2ESelNameByte(p.peek()) || p.peek() == ':' {
		p.i++
	}
	name := p.s[start:p.i]
	if name == "" || !isE2ESelNameStart(name[0]) {
		return false
	}
	dynamic := p.peek() == e2eSelInterpolation
	p.skipDynamicName()
	p.ws()
	if p.peek() != ']' && !p.attributeMatcher() {
		return false
	}
	if p.peek() != ']' {
		return false
	}
	p.i++
	if !dynamic {
		p.addName("attribute", name)
	}
	return true
}

// attributeMatcher parses `op value [i|s]` after an attribute name.
func (p *e2eSelParser) attributeMatcher() bool {
	if c := p.peek(); c != 0 && strings.IndexByte("~|^$*", c) >= 0 {
		p.i++
	}
	if p.peek() != '=' {
		return false
	}
	p.i++
	p.ws()
	if quote := p.peek(); quote == '"' || quote == '\'' {
		end := strings.IndexByte(p.s[p.i+1:], quote)
		if end < 0 {
			return false
		}
		p.i += end + 2
	} else {
		start := p.i
		for p.i < len(p.s) && p.s[p.i] != ']' && p.s[p.i] != ' ' {
			p.i++
		}
		if p.i == start {
			return false
		}
	}
	p.ws()
	if c := p.peek(); c == 'i' || c == 's' {
		p.i++
		p.ws()
	}
	return true
}

func (p *e2eSelParser) pseudo() bool {
	p.i++
	if p.peek() == ':' {
		p.i++
	}
	name := p.ident()
	if !e2eSelKnownPseudos[name] {
		return false
	}
	if p.peek() != '(' {
		return true
	}
	p.i++
	switch name {
	case "not", "is", "where", "has", "matches":
		p.depth++
		ok := p.selectorList(true)
		p.depth--
		p.ws()
		if !ok || p.peek() != ')' {
			return false
		}
		p.i++
		return true
	}
	return p.skipParens()
}

// skipParens skips a pseudo's argument (`nth-child(2n+1)`, `has-text("a (b)")`), whose
// contents name no class.
func (p *e2eSelParser) skipParens() bool {
	depth := 1
	for ; p.i < len(p.s); p.i++ {
		switch c := p.s[p.i]; c {
		case '(':
			depth++
		case ')':
			depth--
			if depth == 0 {
				p.i++
				return true
			}
		case '"', '\'':
			end := strings.IndexByte(p.s[p.i+1:], c)
			if end < 0 {
				return false
			}
			p.i += end + 1
		}
	}
	return false
}

// addName records a class or attribute name. The app's classes and attributes are
// lowercase kebab-case, so an uppercase letter (`nav.goToPath`, `[Content_Types].xml`)
// or a trailing hyphen (`.cmdr-temp-`, a prefix match) means the text isn't a selector.
func (p *e2eSelParser) addName(kind, name string) {
	if strings.ToLower(name) != name || strings.HasSuffix(name, "-") {
		p.rejected = true
		return
	}
	if kind == "attribute" && !strings.HasPrefix(name, "data-") {
		return
	}
	p.tokens = append(p.tokens, e2eSelToken{kind: kind, name: name})
}

func (p *e2eSelParser) skipDynamicName() {
	for c := p.peek(); c == e2eSelInterpolation || isE2ESelNameByte(c); c = p.peek() {
		p.i++
	}
}

func (p *e2eSelParser) ident() string {
	start := p.i
	if p.peek() == '-' {
		p.i++
	}
	if !isE2ESelNameStart(p.peek()) {
		p.i = start
		return ""
	}
	for isE2ESelNameByte(p.peek()) {
		p.i++
	}
	return p.s[start:p.i]
}

func isE2ESelNameStart(c byte) bool {
	return c == '_' || (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z')
}

func isE2ESelNameByte(c byte) bool {
	return isE2ESelNameStart(c) || c == '-' || (c >= '0' && c <= '9')
}
