<script lang="ts">
    import { commands } from '$lib/commands/command-registry'
    import type { Command } from '$lib/commands/types'
    import { searchCommands } from '$lib/commands/fuzzy-search'

    interface Props {
        searchQuery: string
    }

    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    const { searchQuery }: Props = $props()

    let nameSearchQuery = $state('')
    let keySearchQuery = $state('')
    let activeFilter = $state<'all' | 'modified' | 'conflicts'>('all')
    let editingShortcut = $state<{ commandId: string; index: number } | null>(null)
    let pendingKey = $state('')

    // Group commands by scope
    const scopes = ['App', 'Main window', 'File list', 'Navigation', 'Selection', 'Edit', 'View', 'Help']

    // Get commands filtered by search and filter
    const filteredCommands = $derived.by(() => {
        let cmds = [...commands]

        // Filter by name search
        if (nameSearchQuery.trim()) {
            const results = searchCommands(nameSearchQuery)
            const matchedIds = new Set(results.map((r) => r.command.id))
            cmds = cmds.filter((c) => matchedIds.has(c.id))
        }

        // Filter by key search (exact match)
        if (keySearchQuery.trim()) {
            cmds = cmds.filter((c) => c.shortcuts.some((s) => s.toLowerCase().includes(keySearchQuery.toLowerCase())))
        }

        // Filter by modified/conflicts (future: when we have shortcut customization)
        // For now, all shortcuts are defaults

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

        // Build key string
        const parts: string[] = []
        if (event.metaKey) parts.push('⌘')
        if (event.ctrlKey) parts.push('⌃')
        if (event.altKey) parts.push('⌥')
        if (event.shiftKey) parts.push('⇧')

        const key = event.key
        if (!['Meta', 'Control', 'Alt', 'Shift'].includes(key)) {
            parts.push(key.length === 1 ? key.toUpperCase() : key)
            pendingKey = parts.join('')
        }
    }

    function resetAllToDefaults() {
        if (confirm('Reset all keyboard shortcuts to their defaults?')) {
            // Future: reset shortcuts
        }
    }
</script>

<svelte:window onkeydown={editingShortcut ? handleKeyCapture : undefined} />

<div class="section">
    <h2 class="section-title">Keyboard shortcuts</h2>

    <div class="shortcuts-header">
        <div class="search-fields">
            <input
                type="text"
                class="search-input"
                placeholder="Search by action name..."
                bind:value={nameSearchQuery}
            />
            <input
                type="text"
                class="search-input key-search"
                placeholder="Press keys..."
                bind:value={keySearchQuery}
                readonly
                onfocus={() => {
                    /* Future: capture key combo */
                }}
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
            </button>
        </div>
    </div>

    <div class="commands-list">
        {#each Object.entries(groupedCommands) as [scope, scopeCommands] (scope)}
            <div class="scope-group">
                <h3 class="scope-title">{scope}</h3>
                {#each scopeCommands as command (command.id)}
                    <div class="command-row">
                        <div class="command-info">
                            <span class="command-name">{command.name}</span>
                        </div>
                        <div class="command-shortcuts">
                            {#if command.shortcuts.length > 0}
                                {#each command.shortcuts as shortcut, i (shortcut)}
                                    {@const isEditing =
                                        editingShortcut !== null &&
                                        editingShortcut.commandId === command.id &&
                                        editingShortcut.index === i}
                                    <button
                                        class="shortcut-pill"
                                        class:editing={isEditing}
                                        onclick={() => {
                                            editingShortcut = { commandId: command.id, index: i }
                                            pendingKey = ''
                                        }}
                                    >
                                        {isEditing ? pendingKey || 'Press keys...' : shortcut}
                                    </button>
                                {/each}
                            {:else}
                                <span class="no-shortcut">—</span>
                            {/if}
                            <button class="add-shortcut" title="Add shortcut">+</button>
                        </div>
                    </div>
                {/each}
            </div>
        {/each}
    </div>

    <div class="shortcuts-footer">
        <button class="reset-button" onclick={resetAllToDefaults}> Reset all to defaults </button>
    </div>
</div>

<style>
    .section {
        margin-bottom: var(--spacing-md);
    }

    .section-title {
        font-size: 16px;
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
        margin-bottom: var(--spacing-md);
    }

    .search-fields {
        display: flex;
        gap: var(--spacing-sm);
    }

    .search-input {
        flex: 1;
        padding: var(--spacing-xs) var(--spacing-sm);
        border: 1px solid var(--color-border);
        border-radius: 4px;
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
        cursor: pointer;
    }

    .filter-chip:hover {
        background: var(--color-bg-hover);
    }

    .filter-chip.active {
        background: var(--color-accent);
        color: white;
        border-color: var(--color-accent);
    }

    .commands-list {
        max-height: 400px;
        overflow-y: auto;
    }

    .scope-group {
        margin-bottom: var(--spacing-md);
    }

    .scope-title {
        font-size: var(--font-size-sm);
        font-weight: 600;
        color: var(--color-text-muted);
        margin: 0 0 var(--spacing-xs);
        text-transform: uppercase;
        letter-spacing: 0.5px;
    }

    .command-row {
        display: flex;
        align-items: center;
        justify-content: space-between;
        padding: var(--spacing-xs) 0;
        border-bottom: 1px solid var(--color-border-secondary);
    }

    .command-row:last-child {
        border-bottom: none;
    }

    .command-info {
        flex: 1;
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
        padding: 2px 8px;
        background: var(--color-bg-tertiary);
        border: 1px solid var(--color-border);
        border-radius: 4px;
        font-size: var(--font-size-xs);
        font-family: var(--font-system);
        color: var(--color-text-primary);
        cursor: pointer;
    }

    .shortcut-pill:hover {
        background: var(--color-bg-hover);
    }

    .shortcut-pill.editing {
        background: var(--color-accent);
        color: white;
        border-color: var(--color-accent);
    }

    .no-shortcut {
        color: var(--color-text-muted);
        font-size: var(--font-size-sm);
    }

    .add-shortcut {
        width: 20px;
        height: 20px;
        padding: 0;
        border: 1px dashed var(--color-border);
        border-radius: 4px;
        background: transparent;
        color: var(--color-text-muted);
        font-size: 14px;
        cursor: pointer;
        display: flex;
        align-items: center;
        justify-content: center;
    }

    .add-shortcut:hover {
        border-color: var(--color-accent);
        color: var(--color-accent);
    }

    .shortcuts-footer {
        margin-top: var(--spacing-md);
        padding-top: var(--spacing-sm);
        border-top: 1px solid var(--color-border);
    }

    .reset-button {
        padding: var(--spacing-xs) var(--spacing-sm);
        border: 1px solid var(--color-border);
        border-radius: 4px;
        background: var(--color-bg-secondary);
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
        cursor: pointer;
    }

    .reset-button:hover {
        background: var(--color-bg-tertiary);
    }
</style>
