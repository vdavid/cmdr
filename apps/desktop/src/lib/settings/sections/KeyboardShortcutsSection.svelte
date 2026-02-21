<script lang="ts">
    import { onMount } from 'svelte'
    import { commands } from '$lib/commands/command-registry'
    import type { Command } from '$lib/commands/types'
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
    } from '$lib/shortcuts'
    import {
        formatKeyCombo,
        isModifierKey,
        isMacOS,
        findConflictsForShortcut,
        getConflictingCommandIds,
        getConflictCount,
        type CommandScope,
    } from '$lib/shortcuts'
    import { confirmDialog } from '$lib/utils/confirm-dialog'

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
    let conflictWarning = $state<{ shortcut: string; conflictingCommand: Command } | null>(null)

    // Track if we're in "add shortcut" mode (new shortcut with empty value)
    const isAddingNewShortcut = $derived.by(() => {
        if (!editingShortcut) return false
        const shortcuts = getEffectiveShortcuts(editingShortcut.commandId)
        return editingShortcut.index === shortcuts.length - 1 && shortcuts[editingShortcut.index] === ''
    })

    // Reactivity trigger for shortcut changes
    let shortcutChangeCounter = $state(0)

    // Subscribe to shortcut changes
    $effect(() => {
        const unsubscribe = onShortcutChange(() => {
            shortcutChangeCounter++
        })
        return unsubscribe
    })

    // Group commands by scope
    const scopes = ['App', 'Main window', 'File list', 'Navigation', 'Selection', 'Edit', 'View', 'Help']

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

        // Filter by key search (exact match on effective shortcuts)
        if (keySearchQuery.trim()) {
            cmds = cmds.filter((c) => {
                const shortcuts = getEffectiveShortcuts(c.id)
                return shortcuts.some((s) => s.toLowerCase().includes(keySearchQuery.toLowerCase()))
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

    // Group filtered commands by scope
    const groupedCommands = $derived.by(() => {
        const groups: Record<string, Command[]> = {}
        for (const scope of scopes) {
            const scopeCommands = filteredCommands.filter((c) => c.scope === scope)
            if (scopeCommands.length > 0) {
                groups[scope] = scopeCommands
            }
        }
        return groups
    })

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
            const conflicts = findConflictsForShortcut(combo, command.scope as CommandScope, command.id)
            if (conflicts.length > 0) {
                conflictWarning = { shortcut: combo, conflictingCommand: conflicts[0] }
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

        // Check for duplicates on the same action
        const currentShortcuts = getEffectiveShortcuts(currentCommandId)
        const isDuplicate = currentShortcuts.some((s, i) => s === pendingKey && i !== currentIndex)
        if (isDuplicate) {
            // Shortcut already exists on this action - just cancel
            if (isAddingNewShortcut) {
                removeShortcut(currentCommandId, currentIndex)
            }
            cancelEdit()
            return
        }

        setShortcut(currentCommandId, currentIndex, pendingKey)
        cancelEdit()
    }

    function handleRemoveFromOther() {
        if (!conflictWarning || !editingShortcut) return

        // Find the index of the shortcut in the conflicting command
        const conflictShortcuts = getEffectiveShortcuts(conflictWarning.conflictingCommand.id)
        const conflictIndex = conflictShortcuts.indexOf(conflictWarning.shortcut)
        if (conflictIndex >= 0) {
            removeShortcut(conflictWarning.conflictingCommand.id, conflictIndex)
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
            // If we're adding a new shortcut and canceling, remove the empty entry
            if (isAddingNewShortcut) {
                removeShortcut(editingShortcut.commandId, editingShortcut.index)
            }
            cancelEdit()
            return
        }

        // Handle Backspace/Delete to remove shortcut
        if (event.key === 'Backspace' || event.key === 'Delete') {
            if (!pendingKey) {
                event.preventDefault()
                event.stopPropagation()
                removeShortcut(editingShortcut.commandId, editingShortcut.index)
                cancelEdit()
                return
            }
        }

        handleKeyCapture(event)
    }

    function handleAddShortcut(commandId: string) {
        addShortcut(commandId, '')
        const shortcuts = getEffectiveShortcuts(commandId)
        editingShortcut = { commandId, index: shortcuts.length - 1 }
        pendingKey = ''
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

<div class="section">
    <h2 class="section-title">Keyboard shortcuts</h2>

    <div class="shortcuts-header">
        <div class="search-fields">
            <input
                type="text"
                class="search-input"
                placeholder="Search by action name..."
                value={searchQuery.trim() ? searchQuery : localNameSearchQuery}
                oninput={(e) => (localNameSearchQuery = (e.target as HTMLInputElement).value)}
                disabled={!!searchQuery.trim()}
                autocomplete="off"
                autocapitalize="off"
                spellcheck="false"
            />
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
            <span class="warning-text">
                <strong>{conflictWarning.shortcut}</strong> is already bound to "{conflictWarning.conflictingCommand
                    .name}"
            </span>
            <div class="warning-actions">
                <button class="warning-btn" onclick={handleRemoveFromOther}>Remove from other</button>
                <button class="warning-btn" onclick={handleKeepBoth}>Keep both</button>
                <button class="warning-btn secondary" onclick={cancelEdit}>Cancel</button>
            </div>
        </div>
    {/if}

    <div class="commands-list">
        {#each Object.entries(groupedCommands) as [scope, scopeCommands] (scope)}
            <div class="scope-group">
                <h3 class="scope-title">{scope}</h3>
                {#each scopeCommands as command (`${command.id}-${String(shortcutChangeCounter)}`)}
                    {@const shortcuts = getEffectiveShortcuts(command.id)}
                    {@const isModified = isShortcutModified(command.id)}
                    {@const hasConflicts = conflictingIds.has(command.id)}
                    <div class="command-row" class:has-conflicts={hasConflicts}>
                        <div class="command-info">
                            {#if isModified}
                                <span class="modified-dot" title="Modified from default"></span>
                            {/if}
                            {#if hasConflicts}
                                <span class="conflict-icon" title="Has conflicting shortcuts">⚠️</span>
                            {/if}
                            <span class="command-name">{command.name}</span>
                        </div>
                        <div class="command-shortcuts">
                            {#if shortcuts.length > 0}
                                {#each shortcuts as shortcut, i (i)}
                                    {@const isEditing =
                                        editingShortcut !== null &&
                                        editingShortcut.commandId === command.id &&
                                        editingShortcut.index === i}
                                    <button
                                        class="shortcut-pill"
                                        class:editing={isEditing}
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
                                                title="Remove shortcut"
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
                                            —
                                        {/if}
                                    </button>
                                {/each}
                            {:else}
                                <span class="no-shortcut">—</span>
                            {/if}
                            <button
                                class="add-shortcut"
                                title="Add shortcut"
                                onclick={() => {
                                    handleAddShortcut(command.id)
                                }}
                            >
                                +
                            </button>
                            {#if isModified}
                                <button
                                    class="reset-shortcut"
                                    title="Reset to default"
                                    onclick={(e) => {
                                        e.stopPropagation()
                                        handleResetShortcut(command.id)
                                    }}
                                >
                                    ↩
                                </button>
                            {/if}
                        </div>
                    </div>
                {/each}
            </div>
        {/each}
    </div>

    <div class="shortcuts-footer">
        <button class="reset-button" onclick={handleResetAll}>Reset all to defaults</button>
    </div>
</div>

<style>
    .section {
        margin-bottom: var(--spacing-lg);
    }

    .section-title {
        font-size: var(--font-size-lg);
        font-weight: 600;
        color: var(--color-text-primary);
        margin: 0 0 var(--spacing-sm);
        padding-bottom: var(--spacing-xs);
        border-bottom: 1px solid var(--color-border);
    }

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
    }

    .key-search {
        flex: 0.5;
    }

    .filters {
        display: flex;
        gap: var(--spacing-xs);
    }

    .filter-chip {
        padding: var(--spacing-xs) var(--spacing-sm);
        border: 1px solid var(--color-border);
        border-radius: 12px;
        background: var(--color-bg-primary);
        color: var(--color-text-secondary);
        font-size: var(--font-size-xs);
        cursor: default;
        display: flex;
        align-items: center;
        gap: 4px;
    }

    .filter-chip.active {
        background: var(--color-accent);
        color: white;
        border-color: var(--color-accent);
    }

    .conflict-badge {
        background: var(--color-warning);
        color: white;
        font-size: var(--font-size-xs);
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

    .warning-btn {
        padding: var(--spacing-xs) var(--spacing-sm);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        background: var(--color-bg-primary);
        color: var(--color-text-primary);
        font-size: var(--font-size-sm);
        cursor: default;
    }

    .warning-btn.secondary {
        color: var(--color-text-secondary);
    }

    .commands-list {
        max-height: 400px;
        overflow-y: auto;
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
        gap: 4px;
        padding: 2px 8px;
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
        color: white;
        border-color: var(--color-accent);
    }

    .shortcut-pill.empty {
        color: var(--color-text-tertiary);
        border-style: dashed;
    }

    .remove-shortcut {
        width: 12px;
        height: 12px;
        border-radius: var(--radius-full);
        background: var(--color-text-tertiary);
        color: var(--color-bg-primary);
        font-size: var(--font-size-xs);
        font-weight: bold;
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
        color: var(--color-accent);
        border-color: var(--color-accent);
    }

    .shortcuts-footer {
        margin-top: var(--spacing-lg);
        padding-top: var(--spacing-sm);
        border-top: 1px solid var(--color-border);
    }

    .reset-button {
        padding: var(--spacing-xs) var(--spacing-sm);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        background: var(--color-bg-secondary);
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
        cursor: default;
    }
</style>
