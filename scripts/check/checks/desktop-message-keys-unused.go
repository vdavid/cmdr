package checks

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
)

// unusedKeyDynamicPrefixes is the closed, documented allowlist of message-key
// prefixes that are assembled at runtime, so their keys never appear as a literal
// in source and a naive scan would wrongly flag them as orphans. EVERY entry must
// map to a real runtime-construction site; a stale entry (no catalog key under
// it) fails the check (see unusedDynamicPrefixesWithoutKeys). Don't widen this to
// silence an orphan: a genuine orphan is dead translation work to delete, not to
// allowlist.
//
// Construction sites (apps/desktop/src/lib/), each building keys under one prefix
// from an enum/reason variable spliced into the dotted path:
//   - errors.git.: git-error-messages.ts builds getMessage(`errors.git.${kind}.{title,message,suggestion}`)
//   - errors.listing.: listing-error-messages.ts builds `errors.listing.${reason}.${part}`
//   - errors.provider.: provider-error-messages.ts builds getMessage(`errors.provider.${p}.…`) and `…appBased.${cat}`
//   - errors.write.: transfer-error-messages.ts builds getMessage(`errors.write.${key}`)
//
// The same dynamic-key set is the codegen's sanctioned non-fatal dead-key case
// (see docs/guides/i18n.md and apps/desktop/src/lib/intl/messages/DETAILS.md).
var unusedKeyDynamicPrefixes = []string{
	"errors.git.",
	"errors.listing.",
	"errors.provider.",
	"errors.write.",
}

// RunDesktopMessageKeysUnused fails if any key in the `messages/en/*.json`
// catalogs is never referenced anywhere in the frontend source: an orphan key is
// dead translation work that costs real money once human translators are
// involved. A key counts as referenced if its literal dotted string appears in
// any `.ts`/`.svelte` file under `apps/desktop/src/` (covering direct accessor
// calls, `<Trans key>`, and indirection like registry `*Key` fields and Record
// maps that store the literal). Keys under an allowlisted dynamic prefix
// (`unusedKeyDynamicPrefixes`) are treated as referenced, since they're built at
// runtime and never appear verbatim.
//
// Test references count as references on purpose: a key used only in a test is
// rare, and counting it avoids false positives that would block the build on a
// key that does have a (test) call site. The companion `desktop-message-keys-fresh`
// check owns the inverse direction (keys referenced in code but missing from the
// catalog).
func RunDesktopMessageKeysUnused(ctx *CheckContext) (CheckResult, error) {
	messagesDir := filepath.Join(ctx.RootDir, "apps", "desktop", "src", "lib", "intl", "messages", "en")
	srcDir := filepath.Join(ctx.RootDir, "apps", "desktop", "src")

	catalogKeys, err := collectUnusedCheckCatalogKeys(messagesDir)
	if err != nil {
		if os.IsNotExist(err) {
			return Success("no message catalogs yet"), nil
		}
		return CheckResult{}, err
	}

	// A stale dynamic-prefix allowlist entry (no catalog key under it) means the
	// allowlist drifted from the construction sites: surface it rather than let it
	// silently mask future orphans.
	if stale := unusedDynamicPrefixesWithoutKeys(catalogKeys, unusedKeyDynamicPrefixes); len(stale) > 0 {
		return CheckResult{}, fmt.Errorf(
			"%d dynamic-key %s in `unusedKeyDynamicPrefixes` no longer match any catalog key (stale allowlist; remove or fix):\n  %s",
			len(stale), Pluralize(len(stale), "prefix", "prefixes"), strings.Join(stale, "\n  "),
		)
	}

	sources, scanned, err := readUnusedCheckSources(srcDir)
	if err != nil {
		return CheckResult{}, err
	}

	unused := findUnusedMessageKeys(catalogKeys, sources, unusedKeyDynamicPrefixes)
	if len(unused) > 0 {
		var sb strings.Builder
		for _, k := range unused {
			sb.WriteString("  ")
			sb.WriteString(k)
			sb.WriteString("\n")
		}
		return CheckResult{}, fmt.Errorf(
			"found %d orphan message %s never referenced in `apps/desktop/src/` (dead translation work). "+
				"Remove the key and its `@key` metadata sibling from the catalog and run `pnpm intl:keys`. "+
				"If a key is intentionally unwired (a feature mid-build) or built at runtime, wire it to a call site "+
				"or add its prefix to `unusedKeyDynamicPrefixes` with a construction-site comment:\n%s",
			len(unused), Pluralize(len(unused), "key", "keys"), strings.TrimRight(sb.String(), "\n"),
		)
	}

	return Success(fmt.Sprintf(
		"%d message %s all referenced (%d source %s scanned)",
		len(catalogKeys), Pluralize(len(catalogKeys), "key", "keys"),
		scanned, Pluralize(scanned, "file", "files"),
	)), nil
}

// collectUnusedCheckCatalogKeys reads every `en/*.json` catalog and returns the
// sorted, deduped set of renderable message keys, dropping ARB-style `@key`
// metadata entries, matching `gen-message-keys-lib.js`'s collectCatalogKeys.
func collectUnusedCheckCatalogKeys(messagesDir string) ([]string, error) {
	entries, err := os.ReadDir(messagesDir)
	if err != nil {
		return nil, err
	}
	seen := map[string]bool{}
	for _, entry := range entries {
		if entry.IsDir() || !strings.HasSuffix(entry.Name(), ".json") {
			continue
		}
		path := filepath.Join(messagesDir, entry.Name())
		data, readErr := os.ReadFile(path)
		if readErr != nil {
			return nil, fmt.Errorf("couldn't read %s: %w", path, readErr)
		}
		var parsed map[string]json.RawMessage
		if jsonErr := json.Unmarshal(data, &parsed); jsonErr != nil {
			return nil, fmt.Errorf("couldn't parse %s: %w", path, jsonErr)
		}
		for key := range parsed {
			if strings.HasPrefix(key, "@") {
				continue
			}
			seen[key] = true
		}
	}
	keys := make([]string, 0, len(seen))
	for k := range seen {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	return keys, nil
}

// readUnusedCheckSources reads every `.ts`/`.svelte` file under srcDir (skipping
// node_modules / build dirs and the generated `keys.gen.ts`, whose content lists
// the catalog keys rather than using them) and returns their contents plus the
// count scanned. Test files ARE included (test references count as references).
func readUnusedCheckSources(srcDir string) ([]string, int, error) {
	var sources []string
	scanned := 0
	err := filepath.WalkDir(srcDir, func(path string, d os.DirEntry, walkErr error) error {
		if walkErr != nil {
			return walkErr
		}
		if d.IsDir() {
			if d.Name() == "node_modules" || d.Name() == "target" || d.Name() == ".svelte-kit" {
				return filepath.SkipDir
			}
			return nil
		}
		ext := filepath.Ext(d.Name())
		if ext != ".ts" && ext != ".svelte" {
			return nil
		}
		if d.Name() == "keys.gen.ts" {
			return nil
		}
		data, readErr := os.ReadFile(path)
		if readErr != nil {
			return readErr
		}
		sources = append(sources, string(data))
		scanned++
		return nil
	})
	if err != nil {
		return nil, 0, fmt.Errorf("couldn't scan %s: %w", srcDir, err)
	}
	return sources, scanned, nil
}

// findUnusedMessageKeys returns the sorted catalog keys whose literal text
// appears in no source file and that aren't covered by an allowlisted dynamic
// prefix. Using a substring scan (not a strict accessor-call regex) is
// deliberate: it credits any indirection that stores the literal (registry
// `*Key` fields, Record maps), matching the codegen's findCatalogKeyMentions
// dead-key suppression, so the only keys flagged are genuinely absent ones.
func findUnusedMessageKeys(catalogKeys, sources, dynamicPrefixes []string) []string {
	var unused []string
	for _, key := range catalogKeys {
		if hasAnyPrefix(key, dynamicPrefixes) {
			continue
		}
		if !mentionedInAny(key, sources) {
			unused = append(unused, key)
		}
	}
	sort.Strings(unused)
	return unused
}

// unusedDynamicPrefixesWithoutKeys returns the allowlisted prefixes that no
// catalog key starts with: a stale entry that should be removed so the dynamic
// allowlist stays honest (each prefix tied to a live construction site).
func unusedDynamicPrefixesWithoutKeys(catalogKeys, dynamicPrefixes []string) []string {
	var stale []string
	for _, prefix := range dynamicPrefixes {
		covered := false
		for _, key := range catalogKeys {
			if strings.HasPrefix(key, prefix) {
				covered = true
				break
			}
		}
		if !covered {
			stale = append(stale, prefix)
		}
	}
	sort.Strings(stale)
	return stale
}

func hasAnyPrefix(s string, prefixes []string) bool {
	for _, p := range prefixes {
		if strings.HasPrefix(s, p) {
			return true
		}
	}
	return false
}

func mentionedInAny(key string, sources []string) bool {
	for _, src := range sources {
		if strings.Contains(src, key) {
			return true
		}
	}
	return false
}
