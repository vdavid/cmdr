package checks

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
)

// `<Trans>` snippet parity: every `<tag>` in a message has a matching snippet at
// the call site, and every snippet matches a tag.
//
// A tag with no snippet renders NOTHING. `Trans.svelte`'s renderer is
// `{#if childSnippet}…{/if}` with no else, so an unmatched tag drops its inner
// text on the floor — silently, at runtime, with every other check green. The
// catalog is valid, `i18n-parity` is happy (it compares locales to each other,
// not to the component), and nothing throws.
//
// The realistic way in is a RENAME. Renaming a tag in the catalog without
// renaming the snippet key (or vice versa) is a two-file edit that looks done
// after one file, and the failure is invisible in review. Both halves are
// reported, so a half-finished rename names its own other half.
//
// Mechanics:
//   - Scans `<Trans … />` elements in tracked `.svelte` files. Extracts a LITERAL
//     `key="…"` and the keys of an object-literal `snippets={{ … }}` (both
//     `name: value` and shorthand `name`).
//   - Compares against the ENGLISH catalog's tags for that key. English is the
//     canonical tag set: `i18n-parity` already errors when a locale's tag set
//     diverges from it, so checking one locale here covers all of them.
//   - Usages with a computed key (`key={someVar}`) can't be resolved statically
//     and are skipped, as are keys absent from the catalog (`message-keys-fresh`
//     owns those). Both counts are REPORTED rather than silently dropped, so the
//     check never overstates its coverage.

const transEnMessagesDir = "apps/desktop/src/lib/intl/messages/en"

// transElementPattern captures one `<Trans …/>` element's attribute text. `[^>]*`
// stops at the element's own `>`, which is safe because attribute values here are
// quoted strings or `{…}` expressions that contain no bare `>`.
var transElementPattern = regexp.MustCompile(`<Trans\b([^>]*?)/>`)

// transLiteralKeyPattern captures a literal `key="…"` attribute.
var transLiteralKeyPattern = regexp.MustCompile(`\bkey="([^"]+)"`)

// transDynamicKeyPattern matches a computed `key={…}` attribute.
var transDynamicKeyPattern = regexp.MustCompile(`\bkey=\{`)

// transSnippetsPattern captures the inside of `snippets={{ … }}`.
var transSnippetsPattern = regexp.MustCompile(`\bsnippets=\{\{([^}]*)\}\}`)

// transOpaqueSnippetsPattern matches a snippets prop whose keys can't be read
// here: the Svelte shorthand `{snippets}`, or `snippets={expr}` naming a variable
// instead of an inline object. `NetworkBrowser` uses the shorthand form.
var transOpaqueSnippetsPattern = regexp.MustCompile(`\{snippets\}|\bsnippets=\{[^{]`)

// transTagInMessage matches an opening or self-closing tag in a catalog message.
var transTagInMessage = regexp.MustCompile(`<([A-Za-z_][A-Za-z0-9_]*)\s*/?>`)

// transUsage is one resolvable `<Trans>` call site.
type transUsage struct {
	key      string
	snippets []string
}

// parseTransUsages returns every `<Trans>` usage this check can resolve, plus the
// count it deliberately skipped: a computed `key={…}`, or a snippets prop that
// names a variable instead of an inline object. Both are unresolvable without
// evaluating the component, and a guess would be a false positive — the worst
// outcome for a check whose whole value is that a failure means a real bug.
func parseTransUsages(source string) (usages []transUsage, unresolvable int) {
	for _, m := range transElementPattern.FindAllStringSubmatch(source, -1) {
		attrs := m[1]
		keyMatch := transLiteralKeyPattern.FindStringSubmatch(attrs)
		if keyMatch == nil {
			if transDynamicKeyPattern.MatchString(attrs) {
				unresolvable++
			}
			continue
		}
		if transOpaqueSnippetsPattern.MatchString(attrs) {
			unresolvable++
			continue
		}
		usages = append(usages, transUsage{key: keyMatch[1], snippets: parseSnippetNames(attrs)})
	}
	return usages, unresolvable
}

// parseSnippetNames returns the sorted keys of a `snippets={{ … }}` object
// literal, handling both `name: value` and shorthand `name`.
func parseSnippetNames(attrs string) []string {
	m := transSnippetsPattern.FindStringSubmatch(attrs)
	if m == nil {
		return nil
	}
	var names []string
	for _, entry := range strings.Split(m[1], ",") {
		name := strings.TrimSpace(entry)
		if colon := strings.Index(name, ":"); colon >= 0 {
			name = strings.TrimSpace(name[:colon])
		}
		if name != "" {
			names = append(names, name)
		}
	}
	sort.Strings(names)
	return names
}

// messageTags returns the sorted, deduplicated tag names in a catalog message.
func messageTags(message string) []string {
	seen := make(map[string]bool)
	var tags []string
	for _, m := range transTagInMessage.FindAllStringSubmatch(message, -1) {
		if !seen[m[1]] {
			seen[m[1]] = true
			tags = append(tags, m[1])
		}
	}
	sort.Strings(tags)
	return tags
}

// diffNames returns the tags with no snippet and the snippets with no tag.
func diffNames(tags, snippets []string) (missing, extra []string) {
	has := func(list []string, name string) bool {
		for _, item := range list {
			if item == name {
				return true
			}
		}
		return false
	}
	for _, tag := range tags {
		if !has(snippets, tag) {
			missing = append(missing, tag)
		}
	}
	for _, snippet := range snippets {
		if !has(tags, snippet) {
			extra = append(extra, snippet)
		}
	}
	return missing, extra
}

// loadEnglishMessages flattens every `en/*.json` into one key → message map,
// skipping the `@key` metadata entries.
func loadEnglishMessages(rootDir string) (map[string]string, error) {
	dir := filepath.Join(rootDir, transEnMessagesDir)
	files, err := os.ReadDir(dir)
	if err != nil {
		return nil, fmt.Errorf("couldn't read the English catalog at %s: %w", transEnMessagesDir, err)
	}
	messages := make(map[string]string)
	for _, file := range files {
		if file.IsDir() || !strings.HasSuffix(file.Name(), ".json") {
			continue
		}
		raw, err := os.ReadFile(filepath.Join(dir, file.Name()))
		if err != nil {
			return nil, fmt.Errorf("couldn't read %s: %w", file.Name(), err)
		}
		var entries map[string]any
		if err := json.Unmarshal(raw, &entries); err != nil {
			return nil, fmt.Errorf("couldn't parse %s: %w", file.Name(), err)
		}
		for key, value := range entries {
			if message, ok := value.(string); ok && !strings.HasPrefix(key, "@") {
				messages[key] = message
			}
		}
	}
	return messages, nil
}

// RunI18nTransSnippetParity fails when a message's tags and its call site's
// snippets disagree.
func RunI18nTransSnippetParity(ctx *CheckContext) (CheckResult, error) {
	messages, err := loadEnglishMessages(ctx.RootDir)
	if err != nil {
		return CheckResult{}, err
	}

	files, err := listTrackedFiles(ctx.RootDir, "apps/desktop/src/**/*.svelte")
	if err != nil {
		return CheckResult{}, fmt.Errorf("couldn't list tracked .svelte files: %w", err)
	}

	var findings []string
	checked, dynamicTotal, unknownTotal := 0, 0, 0

	for _, rel := range files {
		source, err := os.ReadFile(filepath.Join(ctx.RootDir, rel))
		if err != nil {
			return CheckResult{}, fmt.Errorf("couldn't read %s: %w", rel, err)
		}
		usages, dynamic := parseTransUsages(string(source))
		dynamicTotal += dynamic

		for _, usage := range usages {
			message, ok := messages[usage.key]
			if !ok {
				unknownTotal++
				continue
			}
			checked++
			missing, extra := diffNames(messageTags(message), usage.snippets)
			if len(missing) == 0 && len(extra) == 0 {
				continue
			}
			var problems []string
			if len(missing) > 0 {
				problems = append(problems, fmt.Sprintf(
					"%s has no snippet, so its content renders as NOTHING", tagList(missing)))
			}
			if len(extra) > 0 {
				problems = append(problems, fmt.Sprintf("%s passed but the message has no such tag", tagList(extra)))
			}
			findings = append(findings, fmt.Sprintf("%s: %s\n    %s", rel, usage.key, strings.Join(problems, "; ")))
		}
	}

	if len(findings) > 0 {
		return CheckResult{}, fmt.Errorf(
			"%d <Trans> %s whose tags and snippets disagree. A tag with no snippet drops its inner text "+
				"silently at runtime, so nothing else catches this. Usually a rename finished on one side only: "+
				"make the catalog tag and the `snippets={{ … }}` key match.\n%s",
			len(findings), Pluralize(len(findings), "usage", "usages"), indentOutput(strings.Join(findings, "\n")),
		)
	}

	return Success(fmt.Sprintf(
		"%d <Trans> %s agree with the catalog (%d unresolvable, %d unknown-key, both skipped)",
		checked, Pluralize(checked, "usage", "usages"), dynamicTotal, unknownTotal)), nil
}

func tagList(names []string) string {
	quoted := make([]string, len(names))
	for i, name := range names {
		quoted[i] = fmt.Sprintf("`<%s>`", name)
	}
	return strings.Join(quoted, ", ")
}
