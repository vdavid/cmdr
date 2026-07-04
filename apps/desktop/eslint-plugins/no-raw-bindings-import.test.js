import { RuleTester } from 'eslint'
import tseslint from 'typescript-eslint'
import rule from './no-raw-bindings-import.js'

// The rule distinguishes type imports from value imports via `importKind`, which
// only the TypeScript parser emits (espree can't even parse `import type`), so the
// RuleTester runs on the typescript-eslint parser. Flat-config RuleTester (ESLint
// 9+) auto-detects Vitest's `describe`/`it` globals and emits one test per case,
// so `run` is called at the top level.
const ruleTester = new RuleTester({
  languageOptions: { parser: tseslint.parser, ecmaVersion: 'latest', sourceType: 'module' },
})

ruleTester.run('no-raw-bindings-import', rule, {
  valid: [
    // Type imports from bindings are the source of truth — never reported.
    {
      code: `import type { RepoInfo } from '$lib/ipc/bindings'`,
      filename: 'src/lib/file-explorer/git/git-store.svelte.ts',
    },
    // Inline type specifier alongside nothing runtime — fine.
    {
      code: `import { type RepoInfo, type TagRef } from '$lib/ipc/bindings'`,
      filename: 'src/lib/file-explorer/types.ts',
    },
    // Importing the typed wrapper is the whole point.
    {
      code: `import { getRepoInfo } from '$lib/tauri-commands'`,
      filename: 'src/lib/file-explorer/git/git-store.svelte.ts',
    },
    // The wrapper layer itself legitimately imports the raw bindings.
    {
      code: `import { commands } from '$lib/ipc/bindings'`,
      filename: 'src/lib/tauri-commands/git.ts',
    },
    // The bindings module + typed-events plumbing may import `./bindings`.
    {
      code: `import { events } from './bindings'`,
      filename: 'src/lib/ipc/typed-events.ts',
    },
    // Tests mock the bindings on purpose — exempt by path.
    {
      code: `import { commands } from '$lib/ipc/bindings'`,
      filename: 'src/lib/file-explorer/git/git-store.test.ts',
    },
    // Dev-only debug routes are exempt (same stance as no-raw-tauri-invoke).
    {
      code: `import { commands } from '$lib/ipc/bindings'`,
      filename: 'src/routes/debug/DebugSmbDiagnosticsPanel.svelte',
    },
    // A same-named import from an unrelated module is not our concern.
    {
      code: `import { commands } from '$lib/command-palette/registry'`,
      filename: 'src/lib/whatever.ts',
    },
  ],
  invalid: [
    // Bare `commands` value import in feature code.
    {
      code: `import { commands } from '$lib/ipc/bindings'`,
      filename: 'src/lib/accent-color.ts',
      errors: [{ messageId: 'rawBindingsImport' }],
    },
    // `events` value import in feature code.
    {
      code: `import { events } from '$lib/ipc/bindings'`,
      filename: 'src/lib/settings-store.ts',
      errors: [{ messageId: 'rawBindingsImport' }],
    },
    // Aliased value import still reaches past the seam.
    {
      code: `import { commands as ipcCommands } from '$lib/ipc/bindings'`,
      filename: 'src/lib/shortcuts/shortcuts-store.ts',
      errors: [{ messageId: 'rawBindingsImport' }],
    },
    // Value `commands` mixed with a type import: only the value specifier is flagged.
    {
      code: `import { commands, type RepoInfo } from '$lib/ipc/bindings'`,
      filename: 'src/lib/file-explorer/git/git-store.svelte.ts',
      errors: [{ messageId: 'rawBindingsImport' }],
    },
    // Both runtime bindings in one import → one report each.
    {
      code: `import { commands, events } from '$lib/ipc/bindings'`,
      filename: 'src/lib/downloads/event-bridge.svelte.ts',
      errors: [{ messageId: 'rawBindingsImport' }, { messageId: 'rawBindingsImport' }],
    },
  ],
})
