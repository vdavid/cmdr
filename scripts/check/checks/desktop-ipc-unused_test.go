package checks

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// ipcBindings renders a minimal `bindings.ts` with one `commands` entry per
// camelCase/snake_case pair, in the shape tauri-specta generates.
func ipcBindings(pairs ...[2]string) string {
	var sb strings.Builder
	sb.WriteString("import { invoke as __TAURI_INVOKE } from '@tauri-apps/api/core'\n\n/** Commands */\nexport const commands = {\n")
	for _, p := range pairs {
		sb.WriteString("  /** Doc. */\n")
		sb.WriteString("  " + p[0] + ": (path: string) =>\n")
		sb.WriteString("    typedError<Foo<'x'>, string>(__TAURI_INVOKE('" + p[1] + "', { path })),\n")
	}
	sb.WriteString("}\n\nexport const events = {}\n")
	return sb.String()
}

const ipcAllowlistEmpty = `{"wrappers":{},"commands":{}}`

// ipcFixture is a small but complete tree: two used wrappers, one wrapper used
// only by tests and a `test-*` harness, one command reached only through that
// dead wrapper, one called straight from production, one by a raw invoke, one
// from E2E, and one nobody calls.
func ipcFixture() map[string]string {
	return map[string]string{
		"apps/desktop/src/lib/ipc/bindings.ts": ipcBindings(
			[2]string{"listFiles", "list_files"},
			[2]string{"deleteThing", "delete_thing"},
			[2]string{"getStats", "get_stats"},
			[2]string{"directUse", "direct_use"},
			[2]string{"recordCrumb", "record_crumb"},
			[2]string{"getToken", "get_token"},
			[2]string{"orphan", "orphan"},
		),
		"apps/desktop/src/lib/tauri-commands/files.ts": `import { commands } from '$lib/ipc/bindings'

/** Lists files. */
export async function listFiles(path: string) {
  return commands.listFiles(path)
}

/**
 * Deletes a thing: nothing outside tests calls this.
 */
export async function deleteThing(path: string) {
  return commands.deleteThing(path)
}

export const getStats = async (path: string) => commands.getStats(path)
`,
		"apps/desktop/src/lib/tauri-commands/index.ts": `export { listFiles, deleteThing, getStats } from './files'
`,
		"apps/desktop/src/routes/Page.svelte": `<script lang="ts">
  import { listFiles } from '$lib/tauri-commands'
  // deleteThing is mentioned in a comment, which doesn't count
  void listFiles('/')
</script>
`,
		"apps/desktop/src/lib/stats.ts": `export async function load() {
  const { getStats } = await import('$lib/tauri-commands')
  return getStats('/')
}
`,
		"apps/desktop/src/lib/direct.ts": `import { commands } from '$lib/ipc/bindings'
export const run = () => commands.directUse('/')
`,
		"apps/desktop/src/lib/crumbs.ts": `import { invoke } from '@tauri-apps/api/core'
export const crumb = () => invoke<void>('record_crumb', { event: 'x' })
`,
		"apps/desktop/src/lib/files.test.ts": `import { deleteThing } from '$lib/tauri-commands'
vi.mock('$lib/tauri-commands', () => ({ deleteThing: vi.fn() }))
await invoke('orphan')
`,
		"apps/desktop/src/lib/test-harness.ts": `import { deleteThing } from '$lib/tauri-commands'
export const h = () => deleteThing('/')
`,
		"apps/desktop/test/e2e-shared/client.ts":                 "export const tok = (p) => p.evaluate(`window.__TAURI_INTERNALS__.invoke('get_token')`)\n",
		"apps/desktop/test/unit/other.ts":                        "await invoke('orphan')\n",
		"scripts/check/checks/desktop-ipc-unused-allowlist.json": ipcAllowlistEmpty,
	}
}

func TestIpcUnused_ReportsDeadWrapperAndTheCommandsBehindIt(t *testing.T) {
	tmp := setupGitRepo(t, ipcFixture())
	_, err := RunIpcUnused(&CheckContext{RootDir: tmp})
	if err == nil {
		t.Fatal("expected a failure for the dead wrapper and the unused commands")
	}
	msg := err.Error()
	for _, want := range []string{
		"deleteThing (apps/desktop/src/lib/tauri-commands/files.ts)",
		"commands.deleteThing (delete_thing)",
		"commands.orphan (orphan)",
	} {
		if !strings.Contains(msg, want) {
			t.Errorf("expected failure to name %q, got:\n%s", want, msg)
		}
	}
	for _, used := range []string{"listFiles", "getStats", "directUse", "recordCrumb", "getToken"} {
		if strings.Contains(msg, used) {
			t.Errorf("%s is used, but the failure names it:\n%s", used, msg)
		}
	}
}

func TestIpcUnused_PassesWhenEverythingHasACaller(t *testing.T) {
	files := ipcFixture()
	files["apps/desktop/src/lib/more.svelte"] = `<script lang="ts">
  import { commands } from '$lib/ipc/bindings'
  import {
    type Foo,
    deleteThing as removeIt,
  } from '$lib/tauri-commands'
  void removeIt
  void commands.orphan('/')
</script>
`
	tmp := setupGitRepo(t, files)
	result, err := RunIpcUnused(&CheckContext{RootDir: tmp})
	if err != nil {
		t.Fatalf("expected success, got: %v", err)
	}
	if !strings.Contains(result.Message, "3 wrappers") || !strings.Contains(result.Message, "7 commands") {
		t.Errorf("expected wrapper and command counts in the message, got: %s", result.Message)
	}
}

func TestIpcUnused_SiblingImportCountsForFolderPlumbing(t *testing.T) {
	files := ipcFixture()
	files["apps/desktop/src/lib/tauri-commands/helpers.ts"] = `export function unwrapIt(x: unknown): never {
  throw x
}
`
	files["apps/desktop/src/lib/tauri-commands/other.ts"] = `import { unwrapIt } from './helpers'
export const noop = () => unwrapIt(1)
`
	tmp := setupGitRepo(t, files)
	_, err := RunIpcUnused(&CheckContext{RootDir: tmp})
	if err == nil {
		t.Fatal("expected the fixture's dead wrapper to still fail")
	}
	if strings.Contains(err.Error(), "unwrapIt") {
		t.Errorf("a helper a sibling file imports is folder plumbing, but the failure names it:\n%s", err)
	}
	if !strings.Contains(err.Error(), "noop") {
		t.Errorf("an import from a dead sibling must not hide that sibling, got:\n%s", err)
	}
}

func TestIpcUnused_AllowlistedWrapperCoversItsCommand(t *testing.T) {
	files := ipcFixture()
	files["scripts/check/checks/desktop-ipc-unused-allowlist.json"] = `{
  "wrappers": {"deleteThing": "gap: no UI removes a thing yet"},
  "commands": {"orphan": "awaiting David's call"}
}`
	tmp := setupGitRepo(t, files)
	result, err := RunIpcUnused(&CheckContext{RootDir: tmp})
	if err != nil {
		t.Fatalf("expected the allowlist to cover both, got: %v", err)
	}
	if !strings.Contains(result.Message, "2 allowlisted") {
		t.Errorf("expected '2 allowlisted', got: %s", result.Message)
	}
}

func TestIpcUnused_RejectsAnEntryWithoutAReason(t *testing.T) {
	files := ipcFixture()
	files["scripts/check/checks/desktop-ipc-unused-allowlist.json"] = `{
  "wrappers": {"deleteThing": "  "},
  "commands": {"orphan": "awaiting David's call"}
}`
	tmp := setupGitRepo(t, files)
	_, err := RunIpcUnused(&CheckContext{RootDir: tmp})
	if err == nil || !strings.Contains(err.Error(), "deleteThing") || !strings.Contains(err.Error(), "reason") {
		t.Fatalf("expected a missing-reason failure naming deleteThing, got: %v", err)
	}
}

func TestIpcUnused_ShrinkWrapsStaleEntriesLocally(t *testing.T) {
	files := ipcFixture()
	files["scripts/check/checks/desktop-ipc-unused-allowlist.json"] = `{
  "wrappers": {"deleteThing": "gap: x", "listFiles": "now used", "goneWrapper": "deleted"},
  "commands": {"orphan": "awaiting", "direct_use": "now used", "gone_command": "deleted"}
}`
	tmp := setupGitRepo(t, files)
	result, err := RunIpcUnused(&CheckContext{RootDir: tmp})
	if err != nil {
		t.Fatalf("expected success with a shrink-wrap, got: %v", err)
	}
	if result.Code != ResultSuccess || !result.MadeChanges {
		t.Errorf("expected SuccessWithChanges, got code %d: %s", result.Code, result.Message)
	}
	data, readErr := os.ReadFile(filepath.Join(tmp, "scripts/check/checks/desktop-ipc-unused-allowlist.json"))
	if readErr != nil {
		t.Fatal(readErr)
	}
	got := string(data)
	for _, gone := range []string{"listFiles", "goneWrapper", "direct_use", "gone_command"} {
		if strings.Contains(got, gone) {
			t.Errorf("expected %s to be shrink-wrapped away, allowlist is now:\n%s", gone, got)
		}
	}
	for _, kept := range []string{"deleteThing", "orphan"} {
		if !strings.Contains(got, kept) {
			t.Errorf("expected %s to stay, allowlist is now:\n%s", kept, got)
		}
	}
}

func TestIpcUnused_StaleEntriesWarnInCIWithoutRewriting(t *testing.T) {
	files := ipcFixture()
	allowlist := `{
  "wrappers": {"deleteThing": "gap: x", "listFiles": "now used"},
  "commands": {"orphan": "awaiting"}
}`
	files["scripts/check/checks/desktop-ipc-unused-allowlist.json"] = allowlist
	tmp := setupGitRepo(t, files)
	result, err := RunIpcUnused(&CheckContext{RootDir: tmp, CI: true})
	if err != nil {
		t.Fatalf("expected a warning, got: %v", err)
	}
	if result.Code != ResultWarning || !strings.Contains(result.Message, "listFiles") {
		t.Errorf("expected a warning naming listFiles, got code %d: %s", result.Code, result.Message)
	}
	data, _ := os.ReadFile(filepath.Join(tmp, "scripts/check/checks/desktop-ipc-unused-allowlist.json"))
	if string(data) != allowlist {
		t.Errorf("CI must not rewrite the allowlist, got:\n%s", data)
	}
}
