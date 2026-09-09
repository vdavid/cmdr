package checks

import (
	"encoding/json"
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
)

// The per-language translator guides under `docs/i18n/<locale>/` justify a term
// choice by citing the message catalog as evidence: "the shipped
// `settings.ai.endpoint.label` already says Endpunkt, so the sheet copies it".
// Nothing verified those keys, so a rename orphaned 83 of them in one afternoon
// and an agent invented a few more out of thin air. Both failure modes hand the
// next translator false authority: they either trust a value that was never
// shipped, or revert a correct fix because a doc says the old wording is settled.
//
// The check reads every backticked dotted token in `docs/i18n/**/*.md` and
// requires it to name a real key in the English catalogs.

const (
	i18nDocCitationsDocsRelDir   = "docs/i18n"
	i18nDocCitationsEnRelDir     = "apps/desktop/src/lib/intl/messages/en"
	i18nDocCitationsAllowlistRel = "scripts/check/checks/desktop-i18n-doc-citations-allowlist.json"
)

// i18nDocBacktickedToken matches one backtick-delimited span. Markdown fences and
// prose are irrelevant here: the guides put every key inside single backticks, and
// a token that isn't backticked isn't a citation.
var i18nDocBacktickedToken = regexp.MustCompile("`([^`\n]+)`")

// i18nDocIdentifierSegment is what a key segment looks like. Anything else (a
// space, a slash, a digit-leading macOS string id like `600218`) disqualifies the
// whole token, which is how `PHL-pS-ELV.title` survives but `MR10.1` doesn't.
var i18nDocIdentifierSegment = regexp.MustCompile(`^[A-Za-z_][A-Za-z0-9_-]*$`)

// docCitation is one backticked dotted token that passed the namespace gate, kept
// with where it sits so a finding can be opened directly.
type docCitation struct {
	relPath string
	line    int
	token   string
}

// catalogKeyIndex is the English catalogs, indexed for the two questions this
// check asks: "is this first segment a real namespace?" and "does this dotted
// token name part of a real key?".
type catalogKeyIndex struct {
	namespaces map[string]struct{}
	keys       []string
	// runs holds every contiguous segment run of every key, joined by dots. A
	// citation resolves exactly when it IS one of these runs, which makes exact,
	// prefix, suffix, and infix matching one map lookup and keeps the matching
	// segment-aligned: `ai.endpoint` can never be satisfied by
	// `settings.aiendpoint.label`.
	runs map[string]struct{}
}

// newCatalogKeyIndex builds the index from the full key list. The namespace set
// comes from the keys themselves rather than the catalog filenames, so there's one
// source of truth even if a catalog file is ever split or renamed.
func newCatalogKeyIndex(keys []string) *catalogKeyIndex {
	ix := &catalogKeyIndex{
		namespaces: make(map[string]struct{}, 64),
		keys:       append([]string(nil), keys...),
		runs:       make(map[string]struct{}, len(keys)*8),
	}
	sort.Strings(ix.keys)
	for _, key := range ix.keys {
		segs := strings.Split(key, ".")
		ix.namespaces[segs[0]] = struct{}{}
		for start := range segs {
			for end := start + 2; end <= len(segs); end++ {
				ix.runs[strings.Join(segs[start:end], ".")] = struct{}{}
			}
		}
	}
	return ix
}

// resolves reports whether the token names part of a real key: the whole key, a
// leading run of it (`settings.ai` for `settings.ai.endpoint.label`), a trailing
// one (the guides routinely elide the namespace), or a run in the middle.
func (ix *catalogKeyIndex) resolves(token string) bool {
	_, ok := ix.runs[token]
	return ok
}

// isNamespace reports whether the segment opens a real catalog namespace.
func (ix *catalogKeyIndex) isNamespace(segment string) bool {
	_, ok := ix.namespaces[segment]
	return ok
}

// i18nCitationLeafWeight is how much of a suggestion's score comes from the leaf
// segment, the rest coming from parent-path overlap. Heavily leaf-weighted on
// purpose: the surviving keys still live under the dead path's siblings, so a
// higher path weight buries the real answer under near-neighbours of the rename
// that happened (`servers.sheet.remember` loses to
// `fileExplorer.network.browser.refreshHint` at 0.75).
const i18nCitationLeafWeight = 0.85

// nearestKeys returns up to `want` real keys closest to a dead citation, best
// first. The leaf carries most of the weight: a rename usually moves a message to
// another parent and keeps (or barely edits) its last segment, which is exactly
// the case the suggestion has to solve. Path overlap only breaks ties, because the
// path is the part that died.
func (ix *catalogKeyIndex) nearestKeys(token string, want int) []string {
	tokenSegs := strings.Split(token, ".")
	tokenLeaf := tokenSegs[len(tokenSegs)-1]
	tokenPath := tokenSegs[:len(tokenSegs)-1]

	type scored struct {
		key      string
		score    float64
		distance int
	}
	ranked := make([]scored, 0, len(ix.keys))
	for _, key := range ix.keys {
		keySegs := strings.Split(key, ".")
		leafScore := citationSimilarity(tokenLeaf, keySegs[len(keySegs)-1])
		pathScore := citationPathOverlap(tokenPath, keySegs[:len(keySegs)-1])
		ranked = append(ranked, scored{
			key:      key,
			score:    i18nCitationLeafWeight*leafScore + (1-i18nCitationLeafWeight)*pathScore,
			distance: citationEditDistance(token, key),
		})
	}
	sort.Slice(ranked, func(i, j int) bool {
		if ranked[i].score != ranked[j].score {
			return ranked[i].score > ranked[j].score
		}
		if ranked[i].distance != ranked[j].distance {
			return ranked[i].distance < ranked[j].distance
		}
		return ranked[i].key < ranked[j].key
	})
	if want > len(ranked) {
		want = len(ranked)
	}
	out := make([]string, 0, want)
	for _, r := range ranked[:want] {
		out = append(out, r.key)
	}
	return out
}

// citationSimilarity scores two segments in [0, 1], taking the better of two
// readings. Edit distance over the longer segment handles a leaf that was tweaked;
// the shared-prefix reading handles the one edit distance is bad at, a leaf that
// was truncated or extended (`remember` for `rememberInKeychain`), where ten
// appended characters swamp eight identical ones.
func citationSimilarity(a, b string) float64 {
	ar, br := []rune(a), []rune(b)
	longest := max(len(ar), len(br))
	if longest == 0 {
		return 1
	}
	byDistance := 1 - float64(citationEditDistance(a, b))/float64(longest)

	shortest := min(len(ar), len(br))
	shared := 0
	for shared < shortest && ar[shared] == br[shared] {
		shared++
	}
	byContainment := 0.0
	// The whole shorter leaf has to be the longer one's opening, be at least four
	// runes, and be a real part of it rather than an accident: `red` opens
	// `rememberInKeychain` too. It caps below 1 so an exact leaf always wins.
	if shared == shortest && shortest >= 4 && float64(shortest)/float64(longest) >= 0.4 {
		byContainment = 0.8
	}
	return max(byDistance, byContainment)
}

// citationPathOverlap scores how much of the two keys' parent paths is shared,
// order-insensitively: a renamed key often keeps one or two of its ancestors.
func citationPathOverlap(a, b []string) float64 {
	longest := max(len(a), len(b))
	if longest == 0 {
		return 1
	}
	inA := make(map[string]struct{}, len(a))
	for _, seg := range a {
		inA[seg] = struct{}{}
	}
	shared := 0
	for _, seg := range b {
		if _, ok := inA[seg]; ok {
			shared++
		}
	}
	return float64(shared) / float64(longest)
}

// citationEditDistance is the Levenshtein distance between two strings, measured
// over runes.
func citationEditDistance(a, b string) int {
	ar, br := []rune(a), []rune(b)
	prev := make([]int, len(br)+1)
	cur := make([]int, len(br)+1)
	for j := range prev {
		prev[j] = j
	}
	for i := 1; i <= len(ar); i++ {
		cur[0] = i
		for j := 1; j <= len(br); j++ {
			cost := 1
			if ar[i-1] == br[j-1] {
				cost = 0
			}
			cur[j] = min(prev[j]+1, cur[j-1]+1, prev[j-1]+cost)
		}
		prev, cur = cur, prev
	}
	return prev[len(br)]
}

// scanDocForCitations pulls the citations out of one guide.
//
// ❗ The namespace gate is what makes this check viable, not a nicety. The guides
// are built out of mined evidence from other products, so without it every macOS
// and Total Commander string id (`MR10.1`, `PHL-pS-ELV.title`), every date pattern
// (`dd.MM.yyyy`), every hostname, and every reference-pile filename reads as a
// citation: 1,924 findings to surface 10 real problems. Requiring the first
// segment to be a real catalog namespace cuts that to the ~90 that are actually
// about our catalog, and unlike a denylist of foreign id shapes it needs no
// feeding as translators mine new bundles.
//
// The cost is that a namespace-less shorthand for a live key (`mtp.tryAgain` for
// `fileExplorer.mtp.tryAgain`) is never considered. That's the right trade: those
// are unverifiable either way, and a wrong one misleads nobody about which key
// shipped.
func scanDocForCitations(relPath, content string, ix *catalogKeyIndex) []docCitation {
	var found []docCitation
	for i, line := range strings.Split(content, "\n") {
		for _, match := range i18nDocBacktickedToken.FindAllStringSubmatch(line, -1) {
			token := match[1]
			if !isCitationShaped(token) || namesCatalogFile(token) {
				continue
			}
			if !ix.isNamespace(strings.Split(token, ".")[0]) {
				continue
			}
			found = append(found, docCitation{relPath: relPath, line: i + 1, token: token})
		}
	}
	return found
}

// isCitationShaped reports whether the token looks like a dotted message key: at
// least two segments, each one an identifier.
func isCitationShaped(token string) bool {
	segs := strings.Split(token, ".")
	if len(segs) < 2 {
		return false
	}
	for _, seg := range segs {
		if !i18nDocIdentifierSegment.MatchString(seg) {
			return false
		}
	}
	return true
}

// namesCatalogFile reports whether the token is a catalog FILENAME (`errors.json`)
// rather than a key. The guides name those constantly when they say where a key
// lives, and every catalog file is `<namespace>.json`, so the shape is exact
// enough to recognise without a growing extension denylist.
func namesCatalogFile(token string) bool {
	segs := strings.Split(token, ".")
	return len(segs) == 2 && segs[1] == "json"
}

// docCitationAllowlist is the on-disk shape of
// `desktop-i18n-doc-citations-allowlist.json`. Both sections map a doc to the dead
// citations it may carry, valued in the reason it may carry them; an entry with a
// blank reason silences nothing, so the judgement always ends up written down.
//
// `retired` is permanent: a guide sometimes has to name a key that's GONE, because
// what it's recording is where a translation came from ("the two sentences are
// carried over verbatim from the retired `askCmdr.consent.noContents`"). Naming it
// is the point, and no live key can replace it.
//
// `pending` is temporary: citations that broke when keys were renamed, kept green
// while each locale's fix lands. Local runs drop an entry the moment its doc stops
// citing the dead key, so the section drains itself and can't quietly become the
// permanent one.
type docCitationAllowlist struct {
	Comment string                       `json:"$comment,omitempty"`
	Retired map[string]map[string]string `json:"retired"`
	Pending map[string]map[string]string `json:"pending"`
}

// allowingSection names the section that excuses this doc from citing this key,
// or "" when neither does. A blank reason excuses nothing.
func (list docCitationAllowlist) allowingSection(relPath, token string) string {
	if strings.TrimSpace(list.Retired[relPath][token]) != "" {
		return "retired"
	}
	if strings.TrimSpace(list.Pending[relPath][token]) != "" {
		return "pending"
	}
	return ""
}

// deadDocCitationVerdict splits the dead citations by what excuses them.
type deadDocCitationVerdict struct {
	reported []docCitation
	retired  int
	pending  int
}

// judge sorts every dead citation into the section that excuses it, or into the
// report when nothing does.
func (list docCitationAllowlist) judge(dead []docCitation) deadDocCitationVerdict {
	var verdict deadDocCitationVerdict
	for _, citation := range dead {
		switch list.allowingSection(citation.relPath, citation.token) {
		case "retired":
			verdict.retired++
		case "pending":
			verdict.pending++
		default:
			verdict.reported = append(verdict.reported, citation)
		}
	}
	return verdict
}

// RunDesktopI18nDocCitations holds every message key the translator guides cite as
// evidence to being a key that actually exists.
//
// ERROR class, on David's call: a broken citation is not a maintenance signal, it
// is a doc asserting something false about the shipped app, and the next
// translator acts on it. The one legitimate exception (naming a retired key on
// purpose) is a written-down allowlist entry, not a softer verdict.
func RunDesktopI18nDocCitations(ctx *CheckContext) (CheckResult, error) {
	keys, err := readEnglishCatalogKeys(filepath.Join(ctx.RootDir, i18nDocCitationsEnRelDir))
	if err != nil {
		return CheckResult{}, err
	}
	if len(keys) == 0 {
		return Skipped("no English catalogs to cite"), nil
	}
	index := newCatalogKeyIndex(keys)

	citations, docsScanned, err := scanI18nDocsForCitations(ctx.RootDir, index)
	if err != nil {
		return CheckResult{}, err
	}

	dead, live := splitDeadDocCitations(citations, index)

	allowlist, staleChanges, madeChanges, err := syncDocCitationAllowlist(ctx, live)
	if err != nil {
		return CheckResult{}, err
	}
	verdict := allowlist.judge(dead)
	staleMsg := formatDocCitationStaleness(staleChanges, ctx.CI)

	if len(verdict.reported) > 0 {
		msg := formatDeadDocCitations(verdict.reported, index)
		if staleMsg != "" {
			msg += "\n" + staleMsg
		}
		return CheckResult{}, fmt.Errorf("%s", msg)
	}

	guides := fmt.Sprintf("%d translator %s", docsScanned, Pluralize(docsScanned, "guide", "guides"))
	okMsg := fmt.Sprintf("%d catalog %s in %s all name real keys",
		len(citations), Pluralize(len(citations), "citation", "citations"), guides)
	if len(dead) > 0 {
		okMsg = fmt.Sprintf("%d of %d catalog citations in %s name a real key; %d allowlisted (%d deliberate, %d pending a fix)",
			len(citations)-len(dead), len(citations), guides, len(dead), verdict.retired, verdict.pending)
	}
	if staleMsg != "" {
		return SuccessWithChanges(okMsg + "; " + staleMsg), nil
	}
	result := Success(okMsg)
	result.Total = len(citations)
	result.MadeChanges = madeChanges
	return result, nil
}

// splitDeadDocCitations returns the citations that name no real key, plus the
// (doc, token) set the allowlist shrink-wraps against.
func splitDeadDocCitations(citations []docCitation, index *catalogKeyIndex) ([]docCitation, map[string]map[string]bool) {
	var dead []docCitation
	live := map[string]map[string]bool{}
	for _, citation := range citations {
		if index.resolves(citation.token) {
			continue
		}
		dead = append(dead, citation)
		if live[citation.relPath] == nil {
			live[citation.relPath] = map[string]bool{}
		}
		live[citation.relPath][citation.token] = true
	}
	return dead, live
}

// syncDocCitationAllowlist loads the allowlist, drops what the tree no longer
// needs, and (outside CI) writes the shrunk file back.
func syncDocCitationAllowlist(ctx *CheckContext, live map[string]map[string]bool) (docCitationAllowlist, []string, bool, error) {
	allowlist := loadDocCitationAllowlist(ctx.RootDir)
	staleChanges := shrinkwrapDocCitationAllowlist(&allowlist, live)
	if len(staleChanges) == 0 || ctx.CI {
		return allowlist, staleChanges, false, nil
	}
	if err := writeJSONAllowlist(filepath.Join(ctx.RootDir, i18nDocCitationsAllowlistRel), allowlist); err != nil {
		return allowlist, staleChanges, false, err
	}
	reformatWithOxfmt(ctx.RootDir, i18nDocCitationsAllowlistRel)
	return allowlist, staleChanges, true, nil
}

// readEnglishCatalogKeys collects every message key in the English catalogs.
// `@`-prefixed entries are the translator metadata attached to a key, not keys.
func readEnglishCatalogKeys(enDir string) ([]string, error) {
	entries, err := os.ReadDir(enDir)
	if err != nil {
		if os.IsNotExist(err) {
			return nil, nil
		}
		return nil, fmt.Errorf("couldn't read the English catalogs: %w", err)
	}
	var keys []string
	for _, entry := range entries {
		if entry.IsDir() || !strings.HasSuffix(entry.Name(), ".json") {
			continue
		}
		path := filepath.Join(enDir, entry.Name())
		data, err := os.ReadFile(path)
		if err != nil {
			return nil, fmt.Errorf("couldn't read %s: %w", entry.Name(), err)
		}
		var catalog map[string]json.RawMessage
		if err := json.Unmarshal(data, &catalog); err != nil {
			return nil, fmt.Errorf("couldn't parse %s: %w", entry.Name(), err)
		}
		for key := range catalog {
			if !strings.HasPrefix(key, "@") {
				keys = append(keys, key)
			}
		}
	}
	return keys, nil
}

// scanI18nDocsForCitations walks the translator guides and returns every citation
// they make, plus how many guides carried at least one.
func scanI18nDocsForCitations(rootDir string, index *catalogKeyIndex) ([]docCitation, int, error) {
	docsDir := filepath.Join(rootDir, i18nDocCitationsDocsRelDir)
	var citations []docCitation
	docsScanned := 0
	err := filepath.WalkDir(docsDir, func(path string, entry fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if entry.IsDir() || !strings.HasSuffix(entry.Name(), ".md") {
			return nil
		}
		data, err := os.ReadFile(path)
		if err != nil {
			return err
		}
		relPath, err := filepath.Rel(rootDir, path)
		if err != nil {
			return err
		}
		found := scanDocForCitations(filepath.ToSlash(relPath), string(data), index)
		if len(found) > 0 {
			docsScanned++
		}
		citations = append(citations, found...)
		return nil
	})
	if err != nil {
		if os.IsNotExist(err) {
			return nil, 0, nil
		}
		return nil, 0, fmt.Errorf("couldn't scan the translator guides: %w", err)
	}
	return citations, docsScanned, nil
}

// formatDeadDocCitations reports every unexcused citation with the nearest real
// keys, so the reader can tell a rename from an invention without a search.
func formatDeadDocCitations(citations []docCitation, index *catalogKeyIndex) string {
	sort.Slice(citations, func(i, j int) bool {
		if citations[i].relPath != citations[j].relPath {
			return citations[i].relPath < citations[j].relPath
		}
		if citations[i].line != citations[j].line {
			return citations[i].line < citations[j].line
		}
		return citations[i].token < citations[j].token
	})
	// One suggestion lookup per distinct token: the same dead key is usually cited
	// by every locale, and the answer doesn't depend on who cited it.
	suggestions := map[string][]string{}
	var sb strings.Builder
	for _, citation := range citations {
		if _, done := suggestions[citation.token]; !done {
			suggestions[citation.token] = index.nearestKeys(citation.token, 3)
		}
		sb.WriteString(fmt.Sprintf("  %s:%d cites `%s`, which is not a message key\n",
			citation.relPath, citation.line, citation.token))
		if nearest := suggestions[citation.token]; len(nearest) > 0 {
			sb.WriteString(fmt.Sprintf("      nearest real keys: %s\n", strings.Join(nearest, ", ")))
		}
	}
	return fmt.Sprintf(
		"%d translator-guide %s name a message key that doesn't exist. A citation is the evidence a term "+
			"choice rests on, so a dead one either invents authority or points at a wording that shipped under "+
			"another name. Repoint each to the live key, or record a deliberate reference to a retired key in "+
			"%s:\n%s",
		len(citations), Pluralize(len(citations), "citation", "citations"),
		i18nDocCitationsAllowlistRel, strings.TrimRight(sb.String(), "\n"))
}

func formatDocCitationStaleness(changes []string, ci bool) string {
	if len(changes) == 0 {
		return ""
	}
	verb := "Shrink-wrapped allowlist"
	if ci {
		verb = "Stale allowlist entries (a local run shrink-wraps them)"
	}
	return fmt.Sprintf("%s:\n  - %s", verb, strings.Join(changes, "\n  - "))
}

// loadDocCitationAllowlist reads the allowlist. A missing or unparsable file
// yields an empty one, so every dead citation gets reported.
func loadDocCitationAllowlist(rootDir string) docCitationAllowlist {
	var list docCitationAllowlist
	data, err := os.ReadFile(filepath.Join(rootDir, i18nDocCitationsAllowlistRel))
	if err != nil {
		return list
	}
	if err := json.Unmarshal(data, &list); err != nil {
		return docCitationAllowlist{}
	}
	return list
}

// shrinkwrapDocCitationAllowlist drops every entry whose doc no longer cites the
// dead key, in both sections, and reports what it dropped. That's what keeps the
// `pending` section a burn-down list rather than a second permanent one.
func shrinkwrapDocCitationAllowlist(list *docCitationAllowlist, live map[string]map[string]bool) []string {
	var changes []string
	for _, section := range []struct {
		name    string
		entries map[string]map[string]string
	}{{"retired", list.Retired}, {"pending", list.Pending}} {
		for _, relPath := range sortedKeys(section.entries) {
			for _, token := range sortedKeys(section.entries[relPath]) {
				if live[relPath][token] {
					continue
				}
				delete(section.entries[relPath], token)
				changes = append(changes, fmt.Sprintf("removed %s / %s from %s (no longer cited)",
					relPath, token, section.name))
			}
			if len(section.entries[relPath]) == 0 {
				delete(section.entries, relPath)
			}
		}
	}
	return changes
}
