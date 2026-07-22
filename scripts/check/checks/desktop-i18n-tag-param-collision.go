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

// i18n tag/param collision check: no message may name a `<tag>` and a `{param}`
// alike.
//
// `Trans.svelte` merges the two into ONE lookup — `{...params}` first, then a
// handler per snippet key — so a tag name overwrites a same-named param and the
// param renders as a stringified handler function, straight into the UI:
//
//	"The device is in use by <process>{process}</process>."
//	→ "The device is in use by (chunks) => ({ __trans: true, tag, chunks })."
//
// Nothing else catches it. It type-checks, every ICU/parity check passes (the
// message is valid ICU and the placeholders match across locales), and the
// component renders without throwing. The MTP `ptpcamerad` dialog shipped this
// way and only the named-process branch was affected, so it went unnoticed.
//
// Error, not warn: the output is garbage in the user's face, and the fix is
// always the same one-liner (rename the TAG, since the param name is usually the
// meaningful one).
//
// Mechanics:
//   - Scans every `<locale>/<area>.json` under the message catalog. Locale files
//     are checked too, not just `en`: a translator can introduce the collision
//     while localizing a message English got right.
//   - Skips `@`-prefixed metadata keys (translator descriptions, not messages).
//   - Compares tag names against ICU argument names. An ICU argument is the
//     identifier before `,` or `}` inside a brace, which makes `{count, plural,
//     …}` an argument named `count` while its nested `{# files}` cases are not
//     arguments at all.

const i18nMessagesDir = "apps/desktop/src/lib/intl/messages"

// icuArgumentPattern matches an ICU placeholder's ARGUMENT name: `{name}` or the
// `name` in `{name, plural, …}`. The `[^{}]*` guard keeps it from matching a
// nested plural case body, whose braces contain further braces.
var icuArgumentPattern = regexp.MustCompile(`\{\s*([A-Za-z_][A-Za-z0-9_]*)\s*[,}]`)

// transTagPattern matches an opening or self-closing tag: `<name>` or `<name/>`.
// Closing tags are ignored — an opening tag always accompanies them, so counting
// both would just duplicate.
var transTagPattern = regexp.MustCompile(`<([A-Za-z_][A-Za-z0-9_]*)\s*/?>`)

// collidingNames returns the names used as BOTH a tag and an ICU argument in one
// message, sorted, deduplicated. Empty when the message is well-formed.
func collidingNames(message string) []string {
	args := make(map[string]bool)
	for _, m := range icuArgumentPattern.FindAllStringSubmatch(message, -1) {
		args[m[1]] = true
	}
	if len(args) == 0 {
		return nil
	}

	seen := make(map[string]bool)
	var collisions []string
	for _, m := range transTagPattern.FindAllStringSubmatch(message, -1) {
		tag := m[1]
		if args[tag] && !seen[tag] {
			seen[tag] = true
			collisions = append(collisions, tag)
		}
	}
	sort.Strings(collisions)
	return collisions
}

// scanCatalogFile reports every colliding message in one `<locale>/<area>.json`,
// plus how many messages it looked at.
func scanCatalogFile(path, rel string) (findings []string, messageCount int, err error) {
	raw, err := os.ReadFile(path)
	if err != nil {
		return nil, 0, fmt.Errorf("couldn't read %s: %w", rel, err)
	}
	// Only string values are messages; a `@key` metadata entry is an object, so
	// decoding into `any` and type-switching skips it.
	var entries map[string]any
	if err := json.Unmarshal(raw, &entries); err != nil {
		return nil, 0, fmt.Errorf("couldn't parse %s: %w", rel, err)
	}

	keys := make([]string, 0, len(entries))
	for key := range entries {
		keys = append(keys, key)
	}
	sort.Strings(keys)

	for _, key := range keys {
		message, ok := entries[key].(string)
		if !ok || strings.HasPrefix(key, "@") {
			continue
		}
		messageCount++
		collisions := collidingNames(message)
		if len(collisions) > 0 {
			findings = append(findings, fmt.Sprintf(
				"%s: %s names %s as both a tag and a param\n    %s",
				rel, key, strings.Join(quoteEach(collisions), " and "), message,
			))
		}
	}
	return findings, messageCount, nil
}

// RunI18nTagParamCollision fails when any catalog message names a tag and a param
// alike.
func RunI18nTagParamCollision(ctx *CheckContext) (CheckResult, error) {
	messagesDir := filepath.Join(ctx.RootDir, i18nMessagesDir)
	locales, err := os.ReadDir(messagesDir)
	if err != nil {
		return CheckResult{}, fmt.Errorf("couldn't read the message catalog at %s: %w", i18nMessagesDir, err)
	}

	var findings []string
	messageCount := 0

	for _, locale := range locales {
		// `screenshots/` sits beside the locales but holds capture reports, not
		// messages (same exclusion as `nonEnLocaleCount`).
		if !locale.IsDir() || locale.Name() == "screenshots" {
			continue
		}
		localeDir := filepath.Join(messagesDir, locale.Name())
		areas, err := os.ReadDir(localeDir)
		if err != nil {
			return CheckResult{}, fmt.Errorf("couldn't read locale %s: %w", locale.Name(), err)
		}
		for _, area := range areas {
			if area.IsDir() || !strings.HasSuffix(area.Name(), ".json") {
				continue
			}
			rel := filepath.Join(i18nMessagesDir, locale.Name(), area.Name())
			fileFindings, count, err := scanCatalogFile(filepath.Join(localeDir, area.Name()), rel)
			if err != nil {
				return CheckResult{}, err
			}
			findings = append(findings, fileFindings...)
			messageCount += count
		}
	}

	if len(findings) > 0 {
		return CheckResult{}, fmt.Errorf(
			"%d %s where a <tag> and a {param} share a name; `Trans` lets the tag win, so the param renders "+
				"as a stringified function in the UI. Rename the TAG (the param name is usually the meaningful "+
				"one), in every locale, and refresh each `@key.sourceHash`.\n%s",
			len(findings), Pluralize(len(findings), "message", "messages"), indentOutput(strings.Join(findings, "\n")),
		)
	}

	return Success(fmt.Sprintf("no tag/param name collisions across %d %s",
		messageCount, Pluralize(messageCount, "message", "messages"))), nil
}

func quoteEach(names []string) []string {
	quoted := make([]string, len(names))
	for i, name := range names {
		quoted[i] = fmt.Sprintf("`%s`", name)
	}
	return quoted
}
