import { RuleTester } from 'eslint'
import rule from './no-raw-key-match.js'

// Flat-config RuleTester (ESLint 9+): module source, latest ECMA. RuleTester
// auto-detects Vitest's `describe`/`it` globals and emits one test per case, so
// `run` is called at the top level (it can't be nested inside our own `it`).
//
// The rule is registered under its real `cmdr/` name (and `run` is called with that
// name) because several cases carry an `eslint-disable-next-line cmdr/no-raw-key-match`
// comment: without the plugin registration ESLint reports "Definition for rule … was
// not found" instead of honoring the directive.
const ruleTester = new RuleTester({
  languageOptions: { ecmaVersion: 'latest', sourceType: 'module' },
  plugins: { cmdr: { rules: { 'no-raw-key-match': rule } } },
})

ruleTester.run('cmdr/no-raw-key-match', rule, {
  valid: [
    // The intended path: the whole combo, resolved through the registry.
    {
      code: `if (eventMatchesCommand(e, 'selection.selectAll')) selectAll()`,
      filename: 'src/lib/file-explorer/pane/selection-keys.ts',
    },
    // REJECTING modifiers is the safe direction — a bare `W`, not a superset.
    // (`routes/viewer/viewer-keyboard.ts` § handleToggleKey.)
    {
      code: `function handleToggleKey(e) { if (e.key.toLowerCase() === 'w' && !e.metaKey && !e.ctrlKey && !e.altKey) return true }`,
      filename: 'src/routes/viewer/viewer-keyboard.ts',
    },
    // A bail-out guard ahead of the key tests: the guarded body holds no key test.
    // (`pane/selection-dialog-keys.ts`, verbatim — a documented class-of-key matcher.)
    {
      code: `function classifySelectionDialogKey(e) {
        if (e.metaKey || e.altKey || e.ctrlKey) return null
        if (e.key === '+') return 'open-add'
        if (e.key === '-' || e.code === 'Minus') return 'open-remove'
        return null
      }`,
      filename: 'src/lib/file-explorer/pane/selection-dialog-keys.ts',
    },
    // The other class-of-key matcher: modifier reads, but no literal key test at all.
    // (`pane/type-to-jump-keys.ts`, verbatim.)
    {
      code: `function isTypeToJumpChar(e) {
        if (e.metaKey || e.ctrlKey || e.altKey) return false
        if (e.key.length !== 1) return false
        return /^[a-zA-Z0-9]$/.test(e.key)
      }`,
      filename: 'src/lib/file-explorer/pane/type-to-jump-keys.ts',
    },
    // Routing on "carries a modifier" with the key tests in the OTHER branch is the
    // shape that makes the viewer exact. (`viewer-keyboard.ts` § handleKeyDown.)
    {
      code: `function handleKeyDown(e) {
        if (e.metaKey || e.ctrlKey || e.altKey) {
          handleModifiedKey(e)
          return
        }
        if (e.key === 'Escape') { close(); return }
      }`,
      filename: 'src/routes/viewer/viewer-keyboard.ts',
    },
    // Mouse / click handlers forward modifier flags with no key test anywhere — the
    // big false-positive source. (`views/BriefList.svelte`, `views/FullList.svelte`.)
    {
      code: `function onRowClick(e, index) { handleSelect(index, e.shiftKey, e.metaKey) }`,
      filename: 'src/lib/file-explorer/views/BriefList.svelte',
    },
    // Drag handlers do the same.
    {
      code: `const wantsCopy = e.altKey || e.metaKey`,
      filename: 'src/lib/file-explorer/drag/drag-drop.ts',
    },
    // A modifier-only extraction with no key comparison in the expression.
    // (`navigation/keyboard-shortcuts.ts` § hasExtraModifier.)
    {
      code: `function hasExtraModifier(event, { alt = false } = {}) { return event.metaKey || event.ctrlKey || event.altKey !== alt }`,
      filename: 'src/lib/file-explorer/navigation/keyboard-shortcuts.ts',
    },
    // The combo FORMATTER reads every flag on purpose — exempt by path.
    {
      code: `if (event.metaKey && event.key === 'a') parts.push('⌘')`,
      filename: 'src/lib/shortcuts/key-capture.ts',
    },
    // The held-modifier tracker — exempt by path.
    {
      code: `if (e.metaKey && e.key === 'Meta') cmdKeyHeld = true`,
      filename: 'src/lib/file-explorer/modifier-key-tracker.svelte.ts',
    },
    // The Settings capture field must read raw modifiers to build a combo — exempt by path.
    {
      code: `if (event.metaKey && event.key === 'Escape') parts.push('⌘')`,
      filename: 'src/lib/settings/sections/KeyboardShortcutsSection.controller.svelte.ts',
    },
    // A key compared to a VARIABLE is a parameterized matcher, not a hardcoded combo.
    // (`query-ui/QueryDialog.svelte`.)
    {
      code: `const matches = e.metaKey && e.key === key`,
      filename: 'src/lib/query-ui/QueryDialog.svelte',
    },
    // All four modifiers pinned = one exact combo, a superset of nothing. The
    // dialogs' ⌘/⌃Enter (`feedback/FeedbackDialog.svelte`,
    // `error-reporter/ErrorReportDialog.svelte`), verbatim.
    {
      code: `function isSendCombo(event) { return (event.metaKey || event.ctrlKey) && !event.altKey && !event.shiftKey && event.key === 'Enter' }`,
      filename: 'src/lib/feedback/FeedbackDialog.svelte',
    },
    // Same shape, ⌘R. (`file-explorer/network/NetworkBrowser.svelte`, verbatim.)
    {
      code: `function isRefreshShortcut(e) { return e.key === 'r' && e.metaKey && !e.shiftKey && !e.altKey && !e.ctrlKey }`,
      filename: 'src/lib/file-explorer/network/NetworkBrowser.svelte',
    },
    // Same shape inside an `else if` chain. (`settings/components/SettingsSidebar.svelte`.)
    {
      code: `if (event.key === 'a' && (event.metaKey || event.ctrlKey) && !event.altKey && !event.shiftKey) searchInput?.select()`,
      filename: 'src/lib/settings/components/SettingsSidebar.svelte',
    },
    // A bare key test with no modifier read is the Tier 2 fixed-key vocabulary.
    {
      code: `if (e.key === 'Escape') close()`,
      filename: 'src/lib/ui/ModalDialog.svelte',
    },
    // A stated reason licenses the opt-out. (The violating line itself is elided:
    // RuleTester registers the rule under its own `rule-to-test/` prefix, so a
    // `cmdr/`-named directive can't actually suppress anything here. What's under
    // test is the rule's own demand for a reason, which is independent of that.)
    {
      code: `// eslint-disable-next-line cmdr/no-raw-key-match -- combo builder, not a matcher\nexport const combo = buildCombo(e)`,
      filename: 'src/lib/whatever/combo-builder.ts',
    },
  ],
  invalid: [
    // THE regression: `⌥⌘A` (Ask Cmdr) also satisfied this and selected every file.
    {
      code: `if (e.key === 'a' && e.metaKey) { selectAll(); e.preventDefault() }`,
      filename: 'src/lib/file-explorer/pane/FilePane.svelte',
      errors: [{ messageId: 'rawKeyMatch' }],
    },
    // Same bug, modifier first.
    {
      code: `if (e.metaKey && e.key === 'a') selectAll()`,
      filename: 'src/lib/file-explorer/pane/FilePane.svelte',
      errors: [{ messageId: 'rawKeyMatch' }],
    },
    // Partial exactness is still a superset: nothing rejects ⌃ here.
    {
      code: `const isSelectAll = e.key === 'a' && e.metaKey && !e.altKey`,
      filename: 'src/lib/file-explorer/pane/FilePane.svelte',
      errors: [{ messageId: 'rawKeyMatch' }],
    },
    // Case-insensitive key reads are the same combo.
    {
      code: `if (e.key.toLowerCase() === 'c' && e.metaKey) copy()`,
      filename: 'src/routes/viewer/+page.svelte',
      errors: [{ messageId: 'rawKeyMatch' }],
    },
    // `.code` is no different.
    {
      code: `if (e.code === 'KeyA' && e.ctrlKey) selectAll()`,
      filename: 'src/lib/file-explorer/pane/FilePane.svelte',
      errors: [{ messageId: 'rawKeyMatch' }],
    },
    // Nested: the modifier guards a body that holds the literal key test.
    {
      code: `if (e.altKey) { if (e.key === 'ArrowDown') goToEnd() }`,
      filename: 'src/lib/file-explorer/pane/cursor-nav-keys.ts',
      errors: [{ messageId: 'rawKeyMatch' }],
    },
    // Two required modifiers, one report each.
    {
      code: `if (e.key === 'v' && e.metaKey && e.altKey) pasteSpecial()`,
      filename: 'src/lib/file-explorer/pane/DualPaneExplorer.svelte',
      errors: [{ messageId: 'rawKeyMatch' }, { messageId: 'rawKeyMatch' }],
    },
    // Shift is a modifier too: `⇧Enter` hand-rolled is still a hand-rolled combo.
    {
      code: `const isShiftEnter = e.key === 'Enter' && e.shiftKey`,
      filename: 'src/lib/query-ui/QueryDialog.svelte',
      errors: [{ messageId: 'rawKeyMatch' }],
    },
    // Real catch #1 on the first repo-wide run: the favorites keyboard reorder pinned
    // only ⌥, so ⌥⌘↑ / ⌃⌥↑ reordered a favorite too.
    {
      code: `if (e.altKey && (e.key === 'ArrowUp' || e.key === 'ArrowDown')) reorderHighlighted()`,
      filename: 'src/lib/file-explorer/navigation/VolumeBreadcrumb.svelte',
      errors: [{ messageId: 'rawKeyMatch' }],
    },
    // Real catch #2: Quick Look's close gesture pinned only ⇧, so ⌥⇧Space closed the
    // panel on its way elsewhere.
    {
      code: `if (payload.shiftKey && (payload.key === ' ' || payload.code === 'Space')) close()`,
      filename: 'src/lib/file-explorer/quick-look/quick-look-state.svelte.ts',
      errors: [{ messageId: 'rawKeyMatch' }],
    },
    // Template literal with no interpolation is still a constant key.
    {
      code: 'if (e.key === `a` && e.metaKey) selectAll()',
      filename: 'src/lib/file-explorer/pane/FilePane.svelte',
      errors: [{ messageId: 'rawKeyMatch' }],
    },
    // An opt-out with no stated reason is reported on the COMMENT's own line, which
    // `eslint-disable-next-line` doesn't cover — so the directive can never silence
    // the demand for a reason.
    {
      code: `// eslint-disable-next-line cmdr/no-raw-key-match\nexport const combo = buildCombo(e)`,
      filename: 'src/lib/whatever/combo-builder.ts',
      errors: [{ messageId: 'missingReason', line: 1 }],
    },
    // A whole-file disable needs a reason too.
    {
      code: `/* eslint-disable cmdr/no-raw-key-match */\nexport const combo = buildCombo(e)`,
      filename: 'src/lib/whatever/combo-builder.ts',
      errors: [{ messageId: 'missingReason', line: 1 }],
    },
  ],
})
