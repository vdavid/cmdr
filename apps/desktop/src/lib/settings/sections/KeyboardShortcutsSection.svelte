<script lang="ts">
    import { onMount } from 'svelte'
    import { tooltip } from '$lib/tooltip/tooltip'
    import SettingsSection from '../components/SettingsSection.svelte'
    import Button from '$lib/ui/Button.svelte'
    import { commands } from '$lib/commands/command-registry'
    import { searchCommands } from '$lib/commands/fuzzy-search'
    import {
        getEffectiveShortcuts,
        isShortcutModified,
        setShortcut,
        addShortcut,
        removeShortcut,
        resetShortcut,
        resetAllShortcuts,
        onShortcutChange,
        isNativeShortcutCommand,
    } from '$lib/shortcuts'
    import {
        formatKeyCombo,
        isModifierKey,
        isMacOS,
        findConflictsForShortcut,
        getConflictingCommandIds,
        getConflictCount,
    } from '$lib/shortcuts'
    import { confirmDialog } from '$lib/utils/confirm-dialog'
    import GlobalShortcutRow from '$lib/downloads/GlobalShortcutRow.svelte'
    import { groupCommandsByScope } from './keyboard-shortcuts-grouping'
    import { classifyConflict, reservedByMacOsMessage, type ConflictKind } from './keyboard-shortcuts-banner'
    import { shortcutAnchorId } from '$lib/settings/settings-window'
    import {
        getPendingShortcutHighlight,
        clearPendingShortcutHighlight,
        registerShortcutFilterReset,
        unregisterShortcutFilterReset,
    } from '$lib/settings/pending-shortcut-highlight.svelte'

    interface Props {
        searchQuery: string
    }

    const { searchQuery }: Props = $props()

    // Use global search query if provided, otherwise use local search
    let localNameSearchQuery = $state('')
    const nameSearchQuery = $derived(searchQuery.trim() ? searchQuery : localNameSearchQuery)
    let keySearchQuery = $state('')
    let keyFilterInput: HTMLInputElement | null = $state(null)
    let activeFilter = $state<'all' | 'modified' | 'conflicts'>('all')
    let editingShortcut = $state<{ commandId: string; index: number } | null>(null)
    let pendingKey = $state('')
    let confirmTimeout = $state<ReturnType<typeof setTimeout> | null>(null)
    // The captured combo plus its classification (native → reserved-by-macOS,
    // Cancel-only; normal → resolvable Remove/Keep/Cancel). See keyboard-shortcuts-banner.ts.
    let conflictWarning = $state<{ shortcut: string; conflict: ConflictKind } | null>(null)

    // "Adding" is pure UI state: the add slot is the editing target one past the
    // end of the command's real shortcuts. Nothing is written to the store until a
    // key is captured and confirmed, so abandoning an add (Escape, clicking away,
    // clicking + elsewhere) leaks nothing. See CLAUDE.md § "The add slot is UI-only".
    const isAddingNewShortcut = $derived.by(() => {
        if (!editingShortcut) return false
        return editingShortcut.index === getEffectiveShortcuts(editingShortcut.commandId).length
    })

    // Reactivity trigger for shortcut changes
    let shortcutChangeCounter = $state(0)

    // Subscribe to shortcut changes
    $effect(() => {
        return onShortcutChange(() => {
            shortcutChangeCounter++
        })
    })

    // Deep-link arrival flash: the settings page sets the target command id in the
    // shared module (`pending-shortcut-highlight`) after scrolling its row into
    // view; we read it here to apply the `flash` class, then clear it once the
    // animation has played out. State-driven (not a direct DOM class) because the
    // rows re-key on `shortcutChangeCounter` — an imperative class would vanish on
    // the next re-render. ~1.6 s matches the CSS animation duration.
    const FLASH_DURATION_MS = 1600
    const highlightedCommandId = $derived(getPendingShortcutHighlight())
    $effect(() => {
        if (highlightedCommandId === null) return
        const timer = setTimeout(() => {
            clearPendingShortcutHighlight()
        }, FLASH_DURATION_MS)
        return () => {
            clearTimeout(timer)
        }
    })

    // Register a filter resetter so a deep link to a row that a leftover filter
    // would hide can clear the filters first (the settings page calls this before
    // its scroll). Unregister on unmount so the page no-ops when the section is gone.
    function resetFilters() {
        activeFilter = 'all'
        localNameSearchQuery = ''
        keySearchQuery = ''
    }
    $effect(() => {
        registerShortcutFilterReset(resetFilters)
        return () => {
            unregisterShortcutFilterReset(resetFilters)
        }
    })

    // Get conflict count for badge
    const conflictCount = $derived.by(() => {
        // Trigger on shortcut changes
        void shortcutChangeCounter
        return getConflictCount()
    })

    // Get conflicting command IDs for filtering
    const conflictingIds = $derived.by(() => {
        // Trigger on shortcut changes
        void shortcutChangeCounter
        return getConflictingCommandIds()
    })

    // Get commands filtered by search and filter
    const filteredCommands = $derived.by(() => {
        // Trigger on shortcut changes
        void shortcutChangeCounter

        let cmds = [...commands]

        // Filter by name search
        if (nameSearchQuery.trim()) {
            const results = searchCommands(nameSearchQuery)
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
            const conflict = classifyConflict(conflicts)
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
        const confirmed = await confirmDialog(
            'Reset all keyboard shortcuts to their defaults?',
            'Reset keyboard shortcuts',
        )
        if (confirmed) {
            await resetAllShortcuts()
        }
    }

    /** Check if a filter combo is a subset of a shortcut (all filter modifiers + key present in shortcut) */
    const macModifierSet = new Set(['⌘', '⌃', '⌥', '⇧'])
    const nonMacModifierSet = new Set(['Ctrl', 'Alt', 'Shift', 'Super'])

    function splitCombo(combo: string): { mods: Set<string>; key: string } {
        if (isMacOS()) {
            const chars = Array.from(combo)
            const mods = new Set(chars.filter((ch) => macModifierSet.has(ch)))
            const key = chars.filter((ch) => !macModifierSet.has(ch)).join('')
            return { mods, key }
        }
        const parts = combo.split('+')
        const mods = new Set(parts.filter((p) => nonMacModifierSet.has(p)))
        const key = parts.filter((p) => !nonMacModifierSet.has(p)).join('+')
        return { mods, key }
    }

    function keyFilterMatches(shortcut: string, filter: string): boolean {
        const s = splitCombo(shortcut)
        const f = splitCombo(filter)

        for (const mod of f.mods) {
            if (!s.mods.has(mod)) return false
        }
        if (f.key && s.key.toLowerCase() !== f.key.toLowerCase()) return false
        return true
    }

    // Key filter field: track modifiers and build combo string
    function formatModifiers(event: KeyboardEvent): string {
        const parts: string[] = []
        if (isMacOS()) {
            if (event.metaKey) parts.push('\u2318')
            if (event.ctrlKey) parts.push('\u2303')
            if (event.altKey) parts.push('\u2325')
            if (event.shiftKey) parts.push('\u21E7')
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
                    ? /^[\u2318\u2303\u2325\u21E7]*$/.test(currentValue)
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

    // Use capture phase listener to intercept key events before they reach +page.svelte's window listener
    // This allows us to stop ESC from closing the settings window when editing shortcuts
    onMount(() => {
        function captureKeyDown(event: KeyboardEvent) {
            if (!editingShortcut) return

            // Handle all key events during editing
            handleKeyDown(event)
        }

        document.addEventListener('keydown', captureKeyDown, true) // true = capture phase
        return () => {
            document.removeEventListener('keydown', captureKeyDown, true)
        }
    })
</script>

<SettingsSection title="Keyboard shortcuts">
    <div class="shortcuts-header">
        <div class="search-fields">
            <input
                type="text"
                class="search-input"
                placeholder="Search by action name..."
                value={searchQuery.trim() ? searchQuery : localNameSearchQuery}
                oninput={(e) => {
                    const target = e.target
                    if (target instanceof HTMLInputElement) localNameSearchQuery = target.value
                }}
                disabled={!!searchQuery.trim()}
                autocomplete="off"
                autocapitalize="off"
                spellcheck="false"
            />
            <div class="key-search-wrapper">
                <input
                    type="text"
                    class="search-input key-search"
                    placeholder="Filter by keys..."
                    bind:value={keySearchQuery}
                    bind:this={keyFilterInput}
                    onkeydown={handleKeyFilterKeyDown}
                    onkeyup={handleKeyFilterKeyUp}
                    autocomplete="off"
                    autocapitalize="off"
                    spellcheck="false"
                />
                <span class="key-search-hint" class:visible={!!keySearchQuery}>Press ESC to clear</span>
            </div>
        </div>

        <div class="filters">
            <button class="filter-chip" class:active={activeFilter === 'all'} onclick={() => (activeFilter = 'all')}>
                All
            </button>
            <button
                class="filter-chip"
                class:active={activeFilter === 'modified'}
                onclick={() => (activeFilter = 'modified')}
            >
                Modified
            </button>
            <button
                class="filter-chip"
                class:active={activeFilter === 'conflicts'}
                onclick={() => (activeFilter = 'conflicts')}
            >
                Conflicts
                {#if conflictCount > 0}
                    <span class="conflict-badge">{conflictCount}</span>
                {/if}
            </button>
        </div>
    </div>

    {#if conflictWarning}
        <div class="conflict-warning">
            <span class="warning-icon">⚠️</span>
            {#if conflictWarning.conflict.kind === 'native'}
                <!-- macOS owns this combo: it can never reach Cmdr, so we don't offer
                     "Remove from other" or "Keep both" (both would be a lie). -->
                <span class="warning-text">
                    {reservedByMacOsMessage(conflictWarning.shortcut, conflictWarning.conflict.command)}
                </span>
                <div class="warning-actions">
                    <Button variant="secondary" size="mini" onclick={cancelEdit}>Cancel</Button>
                </div>
            {:else}
                <span class="warning-text">
                    <strong>{conflictWarning.shortcut}</strong> is already bound to "{conflictWarning.conflict.command
                        .name}"
                </span>
                <div class="warning-actions">
                    <Button variant="secondary" size="mini" onclick={handleRemoveFromOther}>Remove from other</Button>
                    <Button variant="secondary" size="mini" onclick={handleKeepBoth}>Keep both</Button>
                    <Button variant="secondary" size="mini" onclick={cancelEdit}>Cancel</Button>
                </div>
            {/if}
        </div>
    {/if}

    <div class="commands-list">
        {#each groupedCommands as group (group.scope)}
            <div class="scope-group">
                <h3 class="scope-title">{group.title}</h3>
                {#each group.commands as command (`${command.id}-${String(shortcutChangeCounter)}`)}
                    {@const shortcuts = getEffectiveShortcuts(command.id)}
                    {@const isModified = isShortcutModified(command.id)}
                    {@const hasConflicts = conflictingIds.has(command.id)}
                    {@const isNative = isNativeShortcutCommand(command.id)}
                    {@const isAddingHere =
                        editingShortcut !== null &&
                        editingShortcut.commandId === command.id &&
                        editingShortcut.index === shortcuts.length}
                    <div
                        id={shortcutAnchorId(command.id)}
                        class="command-row"
                        class:has-conflicts={hasConflicts}
                        class:flash={command.id === highlightedCommandId}
                    >
                        <div class="command-info">
                            {#if isModified}
                                <span class="modified-dot" use:tooltip={'Modified from default'}></span>
                            {/if}
                            {#if hasConflicts}
                                <span class="conflict-icon" use:tooltip={'Has conflicting shortcuts'}>⚠️</span>
                            {/if}
                            <span class="command-name">{command.name}</span>
                        </div>
                        <div class="command-shortcuts">
                            {#if isNative}
                                <!-- macOS owns both the behavior and the accelerator (PredefinedMenuItem).
                                     Cmdr can neither rebind nor intercept it, so the row is read-only: plain
                                     pills, a "macOS" badge, no +/×/reset and no add slot. -->
                                {#if shortcuts.length > 0}
                                    {#each shortcuts as shortcut (shortcut)}
                                        <span class="shortcut-pill static">{shortcut}</span>
                                    {/each}
                                {:else}
                                    <span class="no-shortcut">(none)</span>
                                {/if}
                                <span
                                    class="macos-badge"
                                    use:tooltip={"macOS handles this shortcut. Cmdr can't change it."}>macOS</span
                                >
                            {:else}
                                {#if shortcuts.length > 0}
                                {#each shortcuts as shortcut, i (i)}
                                    {@const isEditing =
                                        editingShortcut !== null &&
                                        editingShortcut.commandId === command.id &&
                                        editingShortcut.index === i}
                                    <button
                                        class="shortcut-pill"
                                        class:editing={isEditing}
                                        class:pending-conflict={isEditing && conflictWarning !== null}
                                        class:empty={!shortcut && !isEditing}
                                        onclick={() => {
                                            editingShortcut = { commandId: command.id, index: i }
                                            pendingKey = ''
                                            conflictWarning = null
                                        }}
                                    >
                                        {#if isEditing}
                                            {pendingKey || 'Press keys...'}
                                        {:else if shortcut}
                                            {shortcut}
                                            <span
                                                class="remove-shortcut"
                                                use:tooltip={'Remove shortcut'}
                                                role="button"
                                                tabindex="-1"
                                                onclick={(e) => {
                                                    e.stopPropagation()
                                                    handleRemoveShortcutAtIndex(command.id, i)
                                                }}
                                                onkeydown={(e) => {
                                                    if (e.key === 'Enter' || e.key === ' ') {
                                                        e.stopPropagation()
                                                        handleRemoveShortcutAtIndex(command.id, i)
                                                    }
                                                }}>×</span
                                            >
                                        {:else}
                                            (none)
                                        {/if}
                                    </button>
                                {/each}
                            {:else if !isAddingHere}
                                <span class="no-shortcut">(none)</span>
                            {/if}
                            {#if isAddingHere}
                                <!-- Synthetic add-slot pill: UI-only until a key is captured and
                                     confirmed. Until then nothing reaches the store, so abandoning
                                     the add leaks no junk entry. -->
                                <button
                                    class="shortcut-pill editing"
                                    class:pending-conflict={conflictWarning !== null}
                                    onclick={() => {
                                        pendingKey = ''
                                        conflictWarning = null
                                    }}
                                >
                                    {pendingKey || 'Press keys...'}
                                </button>
                            {/if}
                            <button
                                class="add-shortcut"
                                aria-label="Add shortcut"
                                use:tooltip={'Add shortcut'}
                                onclick={() => {
                                    handleAddShortcut(command.id)
                                }}
                            >
                                +
                            </button>
                            {#if isModified}
                                <button
                                    class="reset-shortcut"
                                    aria-label="Reset to default"
                                    use:tooltip={'Reset to default'}
                                    onclick={(e) => {
                                        e.stopPropagation()
                                        handleResetShortcut(command.id)
                                    }}
                                >
                                    ↩
                                </button>
                            {/if}
                            {/if}
                        </div>
                    </div>
                {/each}
            </div>
        {/each}
        {#if showGlobalGoToLatestRow}
            <GlobalShortcutRow />
        {/if}
    </div>

    <div class="shortcuts-footer">
        <Button variant="secondary" size="mini" onclick={handleResetAll}>Reset all to defaults</Button>
    </div>
</SettingsSection>

<style>
    .shortcuts-header {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-sm);
        margin-bottom: var(--spacing-lg);
    }

    .search-fields {
        display: flex;
        gap: var(--spacing-sm);
    }

    .search-input {
        flex: 1;
        padding: var(--spacing-xs) var(--spacing-sm);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        background: var(--color-bg-primary);
        color: var(--color-text-primary);
        font-size: var(--font-size-sm);
    }

    .search-input:focus {
        outline: none;
        border-color: var(--color-accent);
        box-shadow: var(--shadow-focus);
    }

    .key-search-wrapper {
        flex: 0.5;
        position: relative;
    }

    .key-search {
        width: 100%;
    }

    .key-search-hint {
        position: absolute;
        top: 100%;
        left: 0;
        margin-top: var(--spacing-xxs);
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
        opacity: 0;
        transition: opacity 0.15s;
    }

    .key-search-hint.visible {
        opacity: 1;
    }

    .filters {
        display: flex;
        gap: var(--spacing-xs);
    }

    .filter-chip {
        padding: var(--spacing-xs) var(--spacing-sm);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-full);
        background: var(--color-bg-primary);
        color: var(--color-text-secondary);
        font-size: var(--font-size-xs);
        cursor: default;
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
    }

    .filter-chip.active {
        background: var(--color-accent);
        color: var(--color-accent-fg);
        border-color: var(--color-accent);
    }

    .filter-chip.active:hover {
        background: var(--color-accent-hover);
        border-color: var(--color-accent-hover);
    }

    .conflict-badge {
        background: var(--color-warning);
        color: var(--color-accent-fg);
        font-size: var(--font-size-xs);
        /* stylelint-disable-next-line declaration-property-value-disallowed-list */
        padding: 1px 5px;
        border-radius: var(--radius-lg);
        font-weight: 600;
    }

    .conflict-warning {
        background: var(--color-warning-bg);
        border: 1px solid var(--color-warning);
        border-radius: var(--radius-sm);
        padding: var(--spacing-sm);
        margin-bottom: var(--spacing-lg);
        display: flex;
        flex-wrap: wrap;
        align-items: center;
        gap: var(--spacing-sm);
    }

    .warning-icon {
        font-size: var(--font-size-lg);
    }

    .warning-text {
        flex: 1;
        font-size: var(--font-size-sm);
    }

    .warning-actions {
        display: flex;
        gap: var(--spacing-xs);
    }

    .commands-list {
        max-height: 400px;
        overflow-y: auto;
        /* Reserve the scrollbar gutter so the row's trailing controls (the + add
           button, the macOS badge) never sit under an overlay scrollbar when the
           list scrolls. `stable` keeps the layout steady whether or not the bar
           is showing, which reads cleaner than a forced always-on scrollbar. */
        scrollbar-gutter: stable;
    }

    .scope-group {
        margin-bottom: var(--spacing-lg);
    }

    .scope-title {
        font-size: var(--font-size-sm);
        font-weight: 600;
        color: var(--color-text-tertiary);
        margin: 0 0 var(--spacing-xs);
        text-transform: uppercase;
        letter-spacing: 0.5px;
    }

    .command-row {
        display: flex;
        align-items: center;
        justify-content: space-between;
        padding: var(--spacing-xs) 0;
        border-bottom: 1px solid var(--color-border-subtle);
    }

    .command-row:last-child {
        border-bottom: none;
    }

    .command-row.has-conflicts {
        background: var(--color-warning-bg);
    }

    /* Deep-link arrival flash: two gentle accent-subtle background pulses so the
       eye lands on the row the user clicked a shortcut chip to customize. The
       `border-radius` rounds the highlight so it reads as a deliberate pulse, not
       a full-bleed row-state change. State-driven via `class:flash` (the row
       re-keys on `shortcutChangeCounter`, so a direct DOM class wouldn't survive).
       The settings page clears the state after the animation; the class drops then. */
    .command-row.flash {
        border-radius: var(--radius-sm);
        animation: shortcut-flash 1.5s ease-in-out;
    }

    @keyframes shortcut-flash {
        0%,
        100% {
            background: transparent;
        }

        20%,
        65% {
            background: var(--color-accent-subtle);
        }

        42% {
            background: transparent;
        }
    }

    /* Reduced motion: skip the pulses, show a static highlight that fades out once. */
    @media (prefers-reduced-motion: reduce) {
        .command-row.flash {
            animation: shortcut-flash-static 1.5s ease-out;
        }

        @keyframes shortcut-flash-static {
            0% {
                background: var(--color-accent-subtle);
            }

            100% {
                background: transparent;
            }
        }
    }

    .command-info {
        flex: 1;
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
    }

    .modified-dot {
        width: 6px;
        height: 6px;
        border-radius: var(--radius-full);
        background: var(--color-accent);
    }

    .conflict-icon {
        font-size: var(--font-size-sm);
    }

    .command-name {
        font-size: var(--font-size-sm);
        color: var(--color-text-primary);
    }

    .command-shortcuts {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
    }

    .shortcut-pill {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xs);
        padding: var(--spacing-xxs) var(--spacing-sm);
        background: var(--color-bg-tertiary);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        font-size: var(--font-size-xs);
        font-family: var(--font-system) sans-serif;
        color: var(--color-text-primary);
        cursor: default;
        min-width: 40px;
        text-align: center;
    }

    .shortcut-pill.editing {
        background: var(--color-accent);
        color: var(--color-accent-fg);
        border-color: var(--color-accent);
    }

    .shortcut-pill.editing:hover {
        background: var(--color-accent-hover);
        border-color: var(--color-accent-hover);
    }

    /* A pending-decision pill: the user pressed a conflicting combo and the warning
       banner is up awaiting their choice. Tint it like the warning (also on hover,
       overriding the accent hover above) so it reads as "this combo is in question",
       not as a saved binding. */
    .shortcut-pill.editing.pending-conflict,
    .shortcut-pill.editing.pending-conflict:hover {
        background: var(--color-warning-bg);
        color: var(--color-text-primary);
        border-color: var(--color-warning);
    }

    .shortcut-pill.empty {
        color: var(--color-text-tertiary);
        border-style: dashed;
    }

    /* A read-only pill for macOS-native commands: same chip shape as an editable
       pill, but rendered as a plain span (no hover, no click) so it reads as
       "shown, not editable". */
    .shortcut-pill.static {
        color: var(--color-text-secondary);
    }

    /* "macOS" badge marking a row the OS owns. Tinted with the muted/secondary
       surface tokens so it reads as an informational tag, not an action. */
    .macos-badge {
        display: inline-flex;
        align-items: center;
        padding: var(--spacing-xxs) var(--spacing-xs);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        background: var(--color-bg-secondary);
        color: var(--color-text-tertiary);
        font-size: var(--font-size-xs);
        cursor: default;
    }

    .remove-shortcut {
        width: 12px;
        height: 12px;
        border-radius: var(--radius-full);
        background: var(--color-text-tertiary);
        color: var(--color-bg-primary);
        font-size: var(--font-size-xs);
        font-weight: 600;
        cursor: default;
        display: none;
        align-items: center;
        justify-content: center;
        line-height: 1;
        flex-shrink: 0;
    }

    .shortcut-pill:hover .remove-shortcut {
        display: flex;
    }

    .no-shortcut {
        color: var(--color-text-tertiary);
        font-size: var(--font-size-sm);
    }

    .add-shortcut,
    .reset-shortcut {
        width: 20px;
        height: 20px;
        padding: 0;
        border: 1px dashed var(--color-border);
        border-radius: var(--radius-sm);
        background: transparent;
        color: var(--color-text-tertiary);
        font-size: var(--font-size-md);
        cursor: default;
        display: flex;
        align-items: center;
        justify-content: center;
    }

    .reset-shortcut {
        font-size: var(--font-size-sm);
    }

    .reset-shortcut:hover {
        color: var(--color-accent-text);
        border-color: var(--color-accent);
    }

    .shortcuts-footer {
        margin-top: var(--spacing-lg);
        padding-top: var(--spacing-sm);
        border-top: 1px solid var(--color-border);
    }
</style>
