package checks

import (
	"bytes"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"sort"
)

// This file holds the shared plumbing for allowlist shrink-wrapping: checks
// that own a JSON allowlist verify their own entries are still needed (dead
// files, satisfied constraints, slack numeric limits) and — outside CI —
// rewrite the allowlist to drop what's stale. The verdict logic stays in each
// check, because "is this entry needed?" IS the check's domain logic; only the
// rewrite and reporting plumbing is shared.

// writeJSONAllowlist marshals v (2-space indent, no HTML escaping, sorted map
// keys, trailing newline) and writes it to path atomically via temp+rename.
func writeJSONAllowlist(path string, v any) error {
	var buf bytes.Buffer
	enc := json.NewEncoder(&buf)
	enc.SetEscapeHTML(false)
	enc.SetIndent("", "  ")
	if err := enc.Encode(v); err != nil {
		return fmt.Errorf("marshal allowlist: %w", err)
	}
	tmp := path + ".tmp"
	if err := os.WriteFile(tmp, buf.Bytes(), 0o644); err != nil {
		return fmt.Errorf("write allowlist: %w", err)
	}
	if err := os.Rename(tmp, path); err != nil {
		return fmt.Errorf("rename allowlist: %w", err)
	}
	return nil
}

// reformatWithOxfmt runs oxfmt on a single just-rewritten file so the JSON
// style (collapsed short objects, print width) matches what the formatter
// enforces, instead of leaving an intermediate Go-marshaled shape for the
// next oxfmt run to churn. Best-effort: a missing pnpm/oxfmt is not an error.
func reformatWithOxfmt(rootDir, relPath string) {
	if !CommandExists("pnpm") {
		return
	}
	cmd := exec.Command("pnpm", "exec", "oxfmt", relPath)
	cmd.Dir = rootDir
	_, _ = RunCommand(cmd, true)
}

// fileExists reports whether path exists (as any kind of file).
func fileExists(path string) bool {
	_, err := os.Stat(path)
	return err == nil
}

// sortedKeys returns the map's keys in sorted order, for deterministic output.
func sortedKeys[V any](m map[string]V) []string {
	keys := make([]string, 0, len(m))
	for k := range m {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	return keys
}
