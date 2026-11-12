/**
 * ESLint rule: feature code never reads a shortcut as text through the store's raw
 * readers.
 *
 * Rationale: on screen, a shortcut renders through `<ShortcutChip commandId="…" />`
 * (`src/lib/ui/ShortcutChip.svelte`), placed in a sentence by a `<Trans>` tag. A shortcut
 * spliced into prose as a `{shortcut}` param reads as punctuation: the empty-folder hint
 * said "press ⌘⇧. or choose…", where the period looked like the sentence's. A chip also
 * follows a rebind live and renders nothing when unbound, so the sentence can't name a
 * dead key.
 *
 * Feature code reads shortcuts through two accessors in
 * `src/lib/shortcuts/reactive-shortcuts.svelte.ts` instead:
 *   - `commandHasShortcut(commandId)`: the yes/no a sentence around a chip needs.
 *   - `shortcutAsText(commandId, sink)`: for a sink that can only hold text (a screen
 *     reader announcement, an `aria-label`, a tooltip's `shortcut`, a string toast, an
 *     error string, a menu or select label). The named sink keeps each exception explicit.
 *
 * Exempt by path: the shortcuts module itself, `ShortcutChip`, and the surfaces whose job
 * is editing or listing bindings (the shortcut settings section, the downloads global
 * shortcut row, the F-key bar's key legend, the MCP settings dump). The per-site
 * decisions: `src/lib/shortcuts/DETAILS.md` § Shortcuts as text.
 *
 * Opt out per-line only with a reason:
 *
 *   // eslint-disable-next-line cmdr/no-raw-shortcut-text -- <reason>
 */

/** The raw readers: each hands back a binding as text (or a list of them). */
const RAW_READERS = new Set([
  'getEffectiveShortcuts',
  'getLiveShortcuts',
  'getDefaultShortcuts',
  'getLiveShortcutsReactive',
  'getFirstShortcutReactive',
  'getFirstShiftShortcutReactive',
  'toDisplayShortcut',
])

const allowedPathFragments = [
  '/lib/shortcuts/',
  '/lib/ui/ShortcutChip.svelte',
  // Editing and listing bindings is these surfaces' job.
  '/lib/settings/sections/KeyboardShortcutsSection',
  '/lib/downloads/GlobalShortcutRow.svelte',
  '/lib/file-explorer/pane/FunctionKeyBar.svelte',
  '/lib/settings/mcp-main-bridge.ts',
]

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: {
      description:
        'Render a shortcut with `<ShortcutChip>`; read it as text only through `shortcutAsText` for a text-only sink.',
      recommended: true,
    },
    messages: {
      rawShortcutText:
        'Don\'t read a shortcut with `{{ name }}` in feature code. On screen, render `<ShortcutChip commandId="…" />` ' +
        '(`#lib/ui/ShortcutChip.svelte`) through a `<Trans>` tag; for a sentence around it, `commandHasShortcut`. ' +
        'For a sink that can only hold text (announcement, aria-label, tooltip, string toast), `shortcutAsText(id, sink)` ' +
        'from `#lib/shortcuts/reactive-shortcuts.svelte`.',
    },
    schema: [],
  },
  create(context) {
    const filename = context.filename || context.getFilename() || ''
    if (allowedPathFragments.some((fragment) => filename.includes(fragment))) {
      return {}
    }

    return {
      ImportSpecifier(node) {
        const name = node.imported.type === 'Identifier' ? node.imported.name : node.imported.value
        if (!RAW_READERS.has(name)) return
        context.report({ node, messageId: 'rawShortcutText', data: { name } })
      },
    }
  },
}
