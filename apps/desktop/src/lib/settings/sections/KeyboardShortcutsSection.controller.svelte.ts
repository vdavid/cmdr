/**
 * Business logic for `KeyboardShortcutsSection.svelte`.
 *
 * The component stays thin (markup + scoped styles + lifecycle): it owns only
 * the capture-phase `keydown` listener (runes lifecycle must live in `.svelte`),
 * the deep-link highlight wiring through `pending-shortcut-highlight.svelte.ts`,
 * and DOM refs. Everything else — the keyboard-capture/conflict engine, shortcut
 * CRUD, the filtering/search derivations, and the key-filter field helpers —
 * lives here behind a `createKeyboardShortcutsController()` factory, mirroring the
 * project's other reactive `.svelte.ts` modules (`reactive-settings.svelte.ts`).
 *
 * Why a factory (per-instance state) rather than module-level `$state` like
 * `reactive-settings`: the section can mount/unmount as the settings sidebar
 * routes between sections, and its editing/filter state is per-component, not
 * app-global. The factory keeps each mount's state isolated.
 *
 * Why `.svelte.ts`, not `.ts`: the `$state`/`$derived` runes need the extension.
 *
 * Pure helpers stay where they already live and are imported here, not
 * duplicated: command grouping in `keyboard-shortcuts-grouping`, conflict
 * classification in `keyboard-shortcuts-banner`.
 */
import { commands } from '$lib/commands/command-registry'
import { searchAllCommands } from '$lib/commands/fuzzy-search'
import {
  getEffectiveShortcuts,
  isShortcutModified,
  setShortcut,
  addShortcut,
  removeShortcut,
  resetShortcut,
  resetAllShortcuts,
  formatKeyCombo,
  isModifierKey,
  isMacOS,
  findConflictsForShortcut,
  getConflictingCommandIds,
  getConflictCount,
} from '$lib/shortcuts'
import { confirmDialog } from '$lib/utils/confirm-dialog'
import { groupCommandsByScope } from './keyboard-shortcuts-grouping'
import { classifyConflict, classifySystemShortcut, type ConflictKind } from './keyboard-shortcuts-banner'

/** The combo being captured plus its classification (drives the conflict banner). */
export interface ConflictWarning {
  shortcut: string
  conflict: ConflictKind
}

/** Which command + slot index is currently being edited (or added, at index === length). */
export interface EditingShortcut {
  commandId: string
  index: number
}

// Plain `Set`, not `SvelteSet`: these are immutable module-level constants read
// imperatively inside pure helpers, never reactive state to subscribe to.

const MAC_MODIFIER_SET = new Set(['⌘', '⌃', '⌥', '⇧'])

const NON_MAC_MODIFIER_SET = new Set(['Ctrl', 'Alt', 'Shift', 'Super'])

/**
 * Build the controller for one `KeyboardShortcutsSection` mount.
 *
 * @param getSearchQuery Reads the section's `searchQuery` prop live. Passing an
 *   accessor (not a snapshot) keeps the `nameSearchQuery` derivation reactive to
 *   the parent-driven global search.
 */
export function createKeyboardShortcutsController(getSearchQuery: () => string) {
  // ── Filter state ──────────────────────────────────────────────────────────
  let localNameSearchQuery = $state('')
  let keySearchQuery = $state('')
  let activeFilter = $state<'all' | 'modified' | 'conflicts'>('all')

  // ── Edit/capture state ────────────────────────────────────────────────────
  let editingShortcut = $state<EditingShortcut | null>(null)
  let pendingKey = $state('')
  let confirmTimeout = $state<ReturnType<typeof setTimeout> | null>(null)
  // The captured combo plus its classification (native → reserved-by-macOS,
  // Cancel-only; normal → resolvable Remove/Keep/Cancel). See keyboard-shortcuts-banner.ts.
  let conflictWarning = $state<ConflictWarning | null>(null)

  // Reactivity trigger for shortcut changes. The component subscribes to
  // `onShortcutChange` in an `$effect` and bumps this so the derivations below
  // (and the keyed `{#each}` in the markup) re-run.
  let shortcutChangeCounter = $state(0)

  // Use the global search query if provided, otherwise the local search.
  const nameSearchQuery = $derived(getSearchQuery().trim() ? getSearchQuery() : localNameSearchQuery)

  // "Adding" is pure UI state: the add slot is the editing target one past the
  // end of the command's real shortcuts. Nothing is written to the store until a
  // key is captured and confirmed, so abandoning an add (Escape, clicking away,
  // clicking + elsewhere) leaks nothing. See CLAUDE.md § "The add slot is UI-only".
  const isAddingNewShortcut = $derived.by(() => {
    if (!editingShortcut) return false
    return editingShortcut.index === getEffectiveShortcuts(editingShortcut.commandId).length
  })

  // Conflict count for the badge.
  const conflictCount = $derived.by(() => {
    void shortcutChangeCounter // Trigger on shortcut changes
    return getConflictCount()
  })

  // Conflicting command ids for filtering and the per-row warning icon.
  const conflictingIds = $derived.by(() => {
    void shortcutChangeCounter // Trigger on shortcut changes
    return getConflictingCommandIds()
  })

  // Commands filtered by search and the active filter chip.
  const filteredCommands = $derived.by(() => {
    void shortcutChangeCounter // Trigger on shortcut changes

    let cmds = [...commands]

    // Filter by name search. `searchAllCommands`, not `searchCommands`: this section
    // renders the full registry, so the search must cover the same set (palette-only
    // search made non-palette commands like "Open command palette" unfindable here).
    if (nameSearchQuery.trim()) {
      const results = searchAllCommands(nameSearchQuery)
      // eslint-disable-next-line svelte/prefer-svelte-reactivity -- local lookup set inside a pure $derived computation, not reactive state
      const matchedIds = new Set(results.map((r) => r.command.id))
      cmds = cmds.filter((c) => matchedIds.has(c.id))
    }

    // Filter by key search (subset match: filter's modifiers + key must all be present in shortcut)
    if (keySearchQuery.trim()) {
      cmds = cmds.filter((c) => {
        const shortcuts = getEffectiveShortcuts(c.id)
        return shortcuts.some((s) => keyFilterMatches(s, keySearchQuery))
      })
    }

    // Filter by modified/conflicts
    if (activeFilter === 'modified') {
      cmds = cmds.filter((c) => isShortcutModified(c.id))
    } else if (activeFilter === 'conflicts') {
      cmds = cmds.filter((c) => conflictingIds.has(c.id))
    }

    return cmds
  })

  // The global go-to-latest hotkey is a bespoke row (its binding lives in
  // settings.json, not shortcuts.json — see `GlobalShortcutRow.svelte`). It
  // shows whenever there's no active filter, or the name filter matches its
  // label. The key filter doesn't apply (we don't index its combo here).
  const showGlobalGoToLatestRow = $derived.by(() => {
    if (activeFilter === 'modified' || activeFilter === 'conflicts') return false
    if (keySearchQuery.trim()) return false
    if (!nameSearchQuery.trim()) return true
    return 'go to latest download global'.includes(nameSearchQuery.trim().toLowerCase())
  })

  // Group filtered commands by scope into the fixed display order. Every command
  // renders in exactly one group (keyed by its scope), so all are rebindable here.
  const groupedCommands = $derived(groupCommandsByScope(filteredCommands))

  function handleKeyCapture(event: KeyboardEvent) {
    if (!editingShortcut) return

    event.preventDefault()
    event.stopPropagation()

    // Ignore pure modifier key presses
    if (isModifierKey(event.key)) return

    // Format the key combo
    const combo = formatKeyCombo(event)
    pendingKey = combo

    // Clear any existing timeout
    if (confirmTimeout) {
      clearTimeout(confirmTimeout)
    }

    // Check for conflicts (editingShortcut is guaranteed non-null here due to early return)
    const currentEditCommandId = editingShortcut.commandId
    const command = commands.find((c) => c.id === currentEditCommandId)
    if (command) {
      const conflicts = findConflictsForShortcut(combo, command.scope, command.id)
      // In-app conflicts take priority; with none, still warn when macOS
      // itself usually owns the combo (Spotlight, Mission Control, …).
      const conflict = classifyConflict(conflicts) ?? classifySystemShortcut(combo)
      if (conflict) {
        conflictWarning = { shortcut: combo, conflict }
        return // Don't auto-save, wait for user decision
      }
    }

    // No conflicts - set 500ms confirmation delay
    confirmTimeout = setTimeout(() => {
      saveShortcut()
    }, 500)
  }

  function saveShortcut() {
    if (!editingShortcut || !pendingKey) return

    // Capture values before calling any functions that might change state
    const currentCommandId = editingShortcut.commandId
    const currentIndex = editingShortcut.index
    const addingNew = isAddingNewShortcut

    // Already bound to this same action? Nothing to do (no store touch on the
    // add slot means there's nothing to clean up either) - just exit.
    const currentShortcuts = getEffectiveShortcuts(currentCommandId)
    const isDuplicate = currentShortcuts.some((s, i) => s === pendingKey && i !== currentIndex)
    if (isDuplicate) {
      cancelEdit()
      return
    }

    if (addingNew) {
      // First time the store hears about this binding: append it.
      addShortcut(currentCommandId, pendingKey)
    } else {
      setShortcut(currentCommandId, currentIndex, pendingKey)
    }
    cancelEdit()
  }

  function handleRemoveFromOther() {
    // Only valid for a normal (resolvable) conflict; the native banner never
    // renders this action.
    if (!conflictWarning || conflictWarning.conflict.kind !== 'normal' || !editingShortcut) return

    // Find the index of the shortcut in the conflicting command
    const other = conflictWarning.conflict.command
    const conflictShortcuts = getEffectiveShortcuts(other.id)
    const conflictIndex = conflictShortcuts.indexOf(conflictWarning.shortcut)
    if (conflictIndex >= 0) {
      removeShortcut(other.id, conflictIndex)
    }

    // Now save our shortcut
    saveShortcut()
  }

  function handleKeepBoth() {
    // Just save without removing from other
    saveShortcut()
  }

  function cancelEdit() {
    if (confirmTimeout) {
      clearTimeout(confirmTimeout)
      confirmTimeout = null
    }
    editingShortcut = null
    pendingKey = ''
    conflictWarning = null
  }

  function handleKeyDown(event: KeyboardEvent) {
    if (!editingShortcut) return

    // Handle Escape to cancel - MUST stop immediate propagation to prevent closing settings window
    // (stopPropagation alone doesn't work because both listeners are on the same window element)
    if (event.key === 'Escape') {
      event.preventDefault()
      event.stopImmediatePropagation()
      // The add slot is UI-only, so canceling an add just drops edit state.
      cancelEdit()
      return
    }

    // Backspace/Delete on an empty capture removes the shortcut being edited.
    // On the add slot there's no real entry to remove, so it just cancels.
    if (event.key === 'Backspace' || event.key === 'Delete') {
      if (!pendingKey) {
        event.preventDefault()
        event.stopPropagation()
        if (!isAddingNewShortcut) {
          removeShortcut(editingShortcut.commandId, editingShortcut.index)
        }
        cancelEdit()
        return
      }
    }

    handleKeyCapture(event)
  }

  function handleAddShortcut(commandId: string) {
    // Don't materialize a store entry - the add slot is one past the end and
    // stays UI-only until a key is confirmed. Starting a fresh add also
    // dismisses any pending conflict decision from a previous edit.
    editingShortcut = { commandId, index: getEffectiveShortcuts(commandId).length }
    pendingKey = ''
    conflictWarning = null
  }

  function handleRemoveShortcutAtIndex(commandId: string, index: number) {
    removeShortcut(commandId, index)
  }

  function handleResetShortcut(commandId: string) {
    resetShortcut(commandId)
  }

  async function handleResetAll() {
    const confirmed = await confirmDialog('Reset all keyboard shortcuts to their defaults?', 'Reset keyboard shortcuts')
    if (confirmed) {
      await resetAllShortcuts()
    }
  }

  /** Begin editing an existing shortcut slot (clears any pending conflict). */
  function startEditingShortcut(commandId: string, index: number) {
    editingShortcut = { commandId, index }
    pendingKey = ''
    conflictWarning = null
  }

  /** Clicking the synthetic add-slot pill resets the in-progress capture. */
  function resetPendingCapture() {
    pendingKey = ''
    conflictWarning = null
  }

  // ── Key-filter field helpers ──────────────────────────────────────────────

  /**
   * Split a combo into its modifier set and base key (platform-aware). The
   * returned `Set`s are throwaway locals used imperatively in `keyFilterMatches`,
   * not reactive state, so plain `Set` (not `SvelteSet`) is correct.
   */
  function splitCombo(combo: string): { mods: Set<string>; key: string } {
    if (isMacOS()) {
      const chars = Array.from(combo)
      // eslint-disable-next-line svelte/prefer-svelte-reactivity -- throwaway local, not reactive state
      const mods = new Set(chars.filter((ch) => MAC_MODIFIER_SET.has(ch)))
      const key = chars.filter((ch) => !MAC_MODIFIER_SET.has(ch)).join('')
      return { mods, key }
    }
    const parts = combo.split('+')
    // eslint-disable-next-line svelte/prefer-svelte-reactivity -- throwaway local, not reactive state
    const mods = new Set(parts.filter((p) => NON_MAC_MODIFIER_SET.has(p)))
    const key = parts.filter((p) => !NON_MAC_MODIFIER_SET.has(p)).join('+')
    return { mods, key }
  }

  /** Check if a filter combo is a subset of a shortcut (all filter modifiers + key present in shortcut). */
  function keyFilterMatches(shortcut: string, filter: string): boolean {
    const s = splitCombo(shortcut)
    const f = splitCombo(filter)

    for (const mod of f.mods) {
      if (!s.mods.has(mod)) return false
    }
    if (f.key && s.key.toLowerCase() !== f.key.toLowerCase()) return false
    return true
  }

  /** Build the display string for the modifiers currently held (platform-aware). */
  function formatModifiers(event: KeyboardEvent): string {
    const parts: string[] = []
    if (isMacOS()) {
      if (event.metaKey) parts.push('⌘')
      if (event.ctrlKey) parts.push('⌃')
      if (event.altKey) parts.push('⌥')
      if (event.shiftKey) parts.push('⇧')
    } else {
      if (event.ctrlKey) parts.push('Ctrl')
      if (event.altKey) parts.push('Alt')
      if (event.shiftKey) parts.push('Shift')
      if (event.metaKey) parts.push('Win')
    }
    return isMacOS() ? parts.join('') : parts.join('+')
  }

  function handleKeyFilterKeyDown(event: KeyboardEvent) {
    // Let Tab through for focus navigation
    if (event.key === 'Tab') return

    // ESC clears the filter when it has a value; otherwise let it bubble (closes window)
    if (event.key === 'Escape') {
      if (keySearchQuery) {
        event.preventDefault()
        event.stopImmediatePropagation()
        keySearchQuery = ''
      }
      return
    }

    event.preventDefault()
    event.stopPropagation()

    // If it's only a modifier key, show it temporarily
    if (isModifierKey(event.key)) {
      keySearchQuery = formatModifiers(event)
      return
    }

    // It's a complete combo - format and keep it
    keySearchQuery = formatKeyCombo(event)
  }

  function handleKeyFilterKeyUp(event: KeyboardEvent) {
    // If we only have modifiers showing (no complete combo), check if all modifiers released
    if (isModifierKey(event.key)) {
      // Check if any modifier is still held
      const stillHasModifier = event.metaKey || event.ctrlKey || event.altKey || event.shiftKey
      if (!stillHasModifier) {
        // All modifiers released - check if current value looks like only modifiers
        const currentValue = keySearchQuery
        // If the value is only modifiers (no regular key), clear it
        const isOnlyModifiers = isMacOS()
          ? /^[⌘⌃⌥⇧]*$/.test(currentValue)
          : /^(Ctrl\+?|Alt\+?|Shift\+?|Win\+?)*$/.test(currentValue)
        if (isOnlyModifiers) {
          keySearchQuery = ''
        }
      } else {
        // Still have some modifiers held - update the display
        keySearchQuery = formatModifiers(event)
      }
    }
  }

  /** Reset all filters (name search, key filter, and the modified/conflicts chip). */
  function resetFilters() {
    activeFilter = 'all'
    localNameSearchQuery = ''
    keySearchQuery = ''
  }

  return {
    // Filter state (two-way: the markup binds these)
    get localNameSearchQuery() {
      return localNameSearchQuery
    },
    set localNameSearchQuery(value: string) {
      localNameSearchQuery = value
    },
    get keySearchQuery() {
      return keySearchQuery
    },
    set keySearchQuery(value: string) {
      keySearchQuery = value
    },
    get activeFilter() {
      return activeFilter
    },
    set activeFilter(value: 'all' | 'modified' | 'conflicts') {
      activeFilter = value
    },
    get nameSearchQuery() {
      return nameSearchQuery
    },

    // Edit/capture state
    get editingShortcut() {
      return editingShortcut
    },
    get pendingKey() {
      return pendingKey
    },
    get conflictWarning() {
      return conflictWarning
    },
    get isAddingNewShortcut() {
      return isAddingNewShortcut
    },
    get shortcutChangeCounter() {
      return shortcutChangeCounter
    },
    /** Bumped by the component's `onShortcutChange` subscription to re-run derivations and re-key rows. */
    bumpShortcutChangeCounter() {
      shortcutChangeCounter++
    },

    // Derived views
    get conflictCount() {
      return conflictCount
    },
    get conflictingIds() {
      return conflictingIds
    },
    get filteredCommands() {
      return filteredCommands
    },
    get showGlobalGoToLatestRow() {
      return showGlobalGoToLatestRow
    },
    get groupedCommands() {
      return groupedCommands
    },

    // Handlers
    handleKeyDown,
    handleRemoveFromOther,
    handleKeepBoth,
    cancelEdit,
    handleAddShortcut,
    handleRemoveShortcutAtIndex,
    handleResetShortcut,
    handleResetAll,
    startEditingShortcut,
    resetPendingCapture,
    handleKeyFilterKeyDown,
    handleKeyFilterKeyUp,
    resetFilters,
  }
}
