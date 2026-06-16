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

// messageKeyShapeRe enforces the semantic, prefix-scoped key shape from
// Decision 5: a lowerCamel first segment, then one or more lowerCamel segments,
// each separated by a dot (`area.feature.leaf`). At least two segments, so a
// bare `area` is rejected (a key must be scoped). Digits are allowed mid-word
// (`fsWatch`, `md5`), never as a segment's first char.
var messageKeyShapeRe = regexp.MustCompile(`^[a-z][a-zA-Z0-9]*(\.[a-z][a-zA-Z0-9]*)+$`)

// messageKeyKnownAreas is the closed set of first segments a key may use,
// matching the catalog file layout (Decision 4: key prefix ↔ filename, 1:1).
// A key's area is also its `messages/en/<area>.json` home. Adding an area means
// adding both a catalog file and an entry here, so structure can't drift.
var messageKeyKnownAreas = map[string]bool{
	"common":         true,
	"transfer":       true,
	"settings":       true,
	"errors":         true,
	"search":         true,
	"viewer":         true,
	"menu":           true,
	"commands":       true,
	"onboarding":     true,
	"fileExplorer":   true,
	"fileOperations": true,
	"queryUi":        true,
	"licensing":      true,
	"downloads":      true,
	"ai":             true,
	"goToPath":       true,
	"mtp":            true,
	"ui":             true,
	"updates":        true,
	"whatsNew":       true,
	"commandPalette": true,
	"shortcuts":      true,
	"crashReporter":  true,
	"errorReporter":  true,
	"feedback":       true,
	"indexing":       true,
	"lowDiskSpace":   true,
	"notifications":  true,
	"main":           true,
}

// messageKeyNamingViolation is one bad catalog key with why it's bad.
type messageKeyNamingViolation struct {
	file   string
	key    string
	reason string
}

// validateMessageKey reports why a key is invalid, or "" if it's fine. A key is
// valid when it matches the shape regex AND its first segment is a known area.
func validateMessageKey(key string) string {
	if !messageKeyShapeRe.MatchString(key) {
		return "doesn't match the `area.feature.leaf` shape (lowerCamel segments, dot-separated, at least two)"
	}
	area := key[:strings.IndexByte(key, '.')]
	if !messageKeyKnownAreas[area] {
		return fmt.Sprintf("unknown area %q (first segment must be a known catalog area)", area)
	}
	return ""
}

// RunDesktopMessageKeyNaming fails if any message key in the `messages/en/*.json`
// catalogs violates the naming contract: the `area.feature.leaf` shape and a
// known first-segment area. ARB-style `@key` metadata entries are validated by
// stripping the leading `@` first (so `@transfer.trash` is checked as
// `transfer.trash`), since a metadata entry for a misnamed key is also a bug.
func RunDesktopMessageKeyNaming(ctx *CheckContext) (CheckResult, error) {
	messagesDir := filepath.Join(ctx.RootDir, "apps", "desktop", "src", "lib", "intl", "messages", "en")

	entries, err := os.ReadDir(messagesDir)
	if err != nil {
		if os.IsNotExist(err) {
			return Success("no message catalogs yet"), nil
		}
		return CheckResult{}, fmt.Errorf("couldn't read %s: %w", messagesDir, err)
	}

	var violations []messageKeyNamingViolation
	keyCount := 0
	for _, entry := range entries {
		if entry.IsDir() || !strings.HasSuffix(entry.Name(), ".json") {
			continue
		}
		fileViolations, count, scanErr := scanMessageKeyNaming(filepath.Join(messagesDir, entry.Name()), entry.Name())
		if scanErr != nil {
			return CheckResult{}, scanErr
		}
		violations = append(violations, fileViolations...)
		keyCount += count
	}

	if len(violations) > 0 {
		sort.Slice(violations, func(i, j int) bool {
			if violations[i].file == violations[j].file {
				return violations[i].key < violations[j].key
			}
			return violations[i].file < violations[j].file
		})
		var sb strings.Builder
		for _, v := range violations {
			sb.WriteString(fmt.Sprintf("  %s: %q — %s\n", v.file, v.key, v.reason))
		}
		return CheckResult{}, fmt.Errorf(
			"found %d malformed message %s (shape `^[a-z][a-zA-Z0-9]*(\\.[a-z][a-zA-Z0-9]*)+$`, known area first segment):\n%s",
			len(violations), Pluralize(len(violations), "key", "keys"), sb.String(),
		)
	}

	return Success(fmt.Sprintf("%d message %s well-formed", keyCount, Pluralize(keyCount, "key", "keys"))), nil
}

// scanMessageKeyNaming validates every key in one catalog file, returning the
// violations and the count of (non-metadata) keys checked.
func scanMessageKeyNaming(path, name string) ([]messageKeyNamingViolation, int, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, 0, fmt.Errorf("couldn't read %s: %w", path, err)
	}
	var parsed map[string]json.RawMessage
	if err := json.Unmarshal(data, &parsed); err != nil {
		return nil, 0, fmt.Errorf("couldn't parse %s: %w", path, err)
	}

	var violations []messageKeyNamingViolation
	count := 0
	for rawKey := range parsed {
		// A `@key` metadata entry names the message key it describes; validate
		// the underlying key (drop the leading `@`).
		key := strings.TrimPrefix(rawKey, "@")
		// Count message keys once (the `@` twin isn't a separate message).
		if !strings.HasPrefix(rawKey, "@") {
			count++
		}
		if reason := validateMessageKey(key); reason != "" {
			violations = append(violations, messageKeyNamingViolation{file: name, key: rawKey, reason: reason})
		}
	}
	return violations, count, nil
}
