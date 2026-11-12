import { RuleTester } from 'eslint'
import rule from './no-raw-shortcut-text.js'

// Flat-config RuleTester (ESLint 9+): module source, latest ECMA. RuleTester
// auto-detects Vitest's `describe`/`it` globals and emits one test per case.
const ruleTester = new RuleTester({
  languageOptions: { ecmaVersion: 'latest', sourceType: 'module' },
})

ruleTester.run('no-raw-shortcut-text', rule, {
  valid: [
    // The two feature-facing accessors are the intended path.
    {
      code: `import { commandHasShortcut, shortcutAsText } from '#lib/shortcuts/reactive-shortcuts.svelte.js'`,
      filename: 'src/lib/file-explorer/views/EmptyFolderState.svelte',
    },
    // The chip itself reads the binding.
    {
      code: `import { getFirstShortcutReactive } from '#lib/shortcuts/reactive-shortcuts.svelte.js'`,
      filename: 'src/lib/ui/ShortcutChip.svelte',
    },
    // The shortcuts module reads its own store.
    {
      code: `import { getEffectiveShortcuts } from './shortcuts-store'`,
      filename: 'src/lib/shortcuts/shortcut-dispatch.ts',
    },
    // Surfaces that edit or list bindings.
    {
      code: `import { getEffectiveShortcuts, toDisplayShortcut } from '#lib/shortcuts/index.js'`,
      filename: 'src/lib/settings/sections/KeyboardShortcutsSection.svelte',
    },
    {
      code: `import { getEffectiveShortcuts } from '#lib/shortcuts/index.js'`,
      filename: 'src/lib/settings/sections/KeyboardShortcutsSection.controller.svelte.ts',
    },
    {
      code: `import { toDisplayShortcut } from '#lib/shortcuts/index.js'`,
      filename: 'src/lib/downloads/GlobalShortcutRow.svelte',
    },
    {
      code: `import { getFirstShiftShortcutReactive } from '#lib/shortcuts/reactive-shortcuts.svelte.js'`,
      filename: 'src/lib/file-explorer/pane/FunctionKeyBar.svelte',
    },
    // Keys no command owns: the global hotkey's helper text, and the search dialog's key table.
    {
      code: `import { toDisplayShortcut } from '#lib/shortcuts/key-capture.js'`,
      filename: 'src/lib/downloads/global-shortcut-description.ts',
    },
    {
      code: `import { toDisplayShortcut } from '#lib/shortcuts/key-capture.js'`,
      filename: 'src/lib/query-ui/query-keys.ts',
    },
    // Unrelated shortcut-module imports stay fine anywhere.
    {
      code: `import { eventMatchesCommand } from '#lib/shortcuts/index.js'`,
      filename: 'src/lib/file-explorer/pane/FilePane.svelte',
    },
  ],
  invalid: [
    // Reading a binding as text for prose.
    {
      code: `import { getFirstShortcutReactive } from '#lib/shortcuts/reactive-shortcuts.svelte.js'`,
      filename: 'src/lib/file-explorer/views/EmptyFolderState.svelte',
      errors: [{ messageId: 'rawShortcutText' }],
    },
    // A one-off read plus display formatting, both flagged.
    {
      code: `import { getEffectiveShortcuts, toDisplayShortcut } from '#lib/shortcuts/index.js'`,
      filename: 'src/lib/file-explorer/pane/clipboard-operations.ts',
      errors: [{ messageId: 'rawShortcutText' }, { messageId: 'rawShortcutText' }],
    },
    // The live-key reader is still a raw reader: it hands back text.
    {
      code: `import { getLiveShortcuts } from '#lib/shortcuts/index.js'`,
      filename: 'src/lib/file-explorer/pane/default-app-open.ts',
      errors: [{ messageId: 'rawShortcutText' }],
    },
    // Any import path counts: the leaf module too.
    {
      code: `import { toDisplayShortcut } from '#lib/shortcuts/key-capture.js'`,
      filename: 'src/routes/viewer/ViewerStatusBar.svelte',
      errors: [{ messageId: 'rawShortcutText' }],
    },
    // A renamed import is still the raw reader.
    {
      code: `import { getLiveShortcutsReactive as shortcutsOf } from '#lib/shortcuts/reactive-shortcuts.svelte.js'`,
      filename: 'src/lib/command-palette/CommandPalette.svelte',
      errors: [{ messageId: 'rawShortcutText' }],
    },
  ],
})
