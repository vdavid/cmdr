<script lang="ts">
    /**
     * CommandPalette - VS Code-style command palette modal.
     *
     * Features:
     * - Fuzzy search with highlighted matches
     * - Keyboard navigation (↑/↓/Enter/Escape)
     * - Empty-query view lists recently executed commands, most-recent first
     * - Blocks keyboard events from propagating to file explorer
     */
    import { onDestroy, onMount, tick } from 'svelte'
    import { searchCommands, getPaletteCommands, type CommandMatch, type CommandId } from '$lib/commands'
    import { pruneRecentCommands, pushRecentCommand } from '$lib/app-status-store'

    interface Props {
        /** Called when user selects a command */
        onExecute: (commandId: CommandId) => void
        /** Called when palette is closed */
        onClose: () => void
    }

    const { onExecute, onClose }: Props = $props()

    let query = $state('')
    let recentCommandIds = $state<string[]>([])
    let cursorIndex = $state(0)
    let hoveredIndex = $state<number | null>(null)
    let inputElement: HTMLInputElement | undefined = $state()
    let resultsContainer: HTMLDivElement | undefined = $state()
    /**
     * Element that had focus when the palette opened. We restore focus to it on
     * destroy so the focused pane (or whichever element triggered the palette)
     * gets keyboard input again. Without this, focus falls to <body> and arrow
     * keys silently no-op until the user clicks back into a pane.
     */
    let previousActiveElement: HTMLElement | null = null

    // Derived: filtered and ranked results. When the query is empty, recents
    // lead the list (most-recent first) so the cursor at index 0 lands on the
    // user's last-executed command (Enter re-runs it).
    const results = $derived(searchCommands(query, recentCommandIds))

    // Boundary between recents and the rest in the empty-query view. Used to
    // render the "Recent" / "All commands" subheaders. Always 0 when the query
    // is non-empty (no grouping during search).
    const recentCount = $derived.by(() => {
        if (query.trim() || recentCommandIds.length === 0) return 0
        const recentSet = new Set(recentCommandIds)
        let n = 0
        for (const r of results) {
            if (recentSet.has(r.command.id)) n++
            else break
        }
        return n
    })

    // Stable per-option IDs let the combobox input announce the active option
    // to assistive tech via aria-activedescendant. DOM focus stays in the input.
    function optionId(commandId: string): string {
        return `palette-option-${commandId}`
    }
    const activeDescendantId = $derived(
        results[cursorIndex] ? optionId(results[cursorIndex].command.id) : undefined,
    )

    // Reset cursor position when query changes
    $effect(() => {
        void query // Track
        cursorIndex = 0
        hoveredIndex = null
    })

    onMount(() => {
        previousActiveElement = document.activeElement instanceof HTMLElement ? document.activeElement : null
        // Load recents and prune any IDs that no longer correspond to a valid palette
        // command (renamed or removed since last use). Self-heals the persisted list.
        const validIds = new Set(getPaletteCommands().map((c) => c.id))
        void pruneRecentCommands(validIds).then((ids) => {
            recentCommandIds = ids
        })
        inputElement?.focus()
    })

    onDestroy(() => {
        // Restore focus to whatever had it before we opened, if it's still in the DOM.
        // Without this, focus falls to <body> after close and keyboard nav stops working
        // for the previously focused pane until the user clicks back into it.
        if (previousActiveElement?.isConnected) {
            previousActiveElement.focus()
        }
    })

    function handleKeyDown(e: KeyboardEvent) {
        // CRITICAL: Stop propagation to prevent file explorer from handling these
        e.stopPropagation()

        switch (e.key) {
            case 'Escape':
                e.preventDefault()
                onClose()
                break
            case 'ArrowDown':
                e.preventDefault()
                cursorIndex = Math.min(cursorIndex + 1, results.length - 1)
                hoveredIndex = null
                scrollCursorIntoView()
                break
            case 'ArrowUp':
                e.preventDefault()
                cursorIndex = Math.max(cursorIndex - 1, 0)
                hoveredIndex = null
                scrollCursorIntoView()
                break
            case 'Enter':
                e.preventDefault()
                if (results[cursorIndex]) {
                    const id = results[cursorIndex].command.id
                    void pushRecentCommand(id)
                    onExecute(id)
                }
                break
        }
    }

    function scrollCursorIntoView() {
        void tick().then(() => {
            const cursor = resultsContainer?.querySelector('.result-item.is-under-cursor')
            cursor?.scrollIntoView({ block: 'nearest' })
        })
    }

    function handleResultClick(index: number) {
        const id = results[index].command.id
        void pushRecentCommand(id)
        onExecute(id)
    }

    function handleOverlayClick(e: MouseEvent) {
        // Only close if clicking the overlay itself, not the modal content
        if (e.target === e.currentTarget) {
            onClose()
        }
    }

    /** Render command name with matched characters highlighted */
    function highlightMatches(match: CommandMatch): { text: string; highlighted: boolean }[] {
        const { name } = match.command
        const indices = new Set(match.matchedIndices)
        const segments: { text: string; highlighted: boolean }[] = []

        let currentSegment = ''
        let currentHighlighted = false

        for (let i = 0; i < name.length; i++) {
            const isHighlighted = indices.has(i)
            if (isHighlighted !== currentHighlighted && currentSegment) {
                segments.push({ text: currentSegment, highlighted: currentHighlighted })
                currentSegment = ''
            }
            currentSegment += name[i]
            currentHighlighted = isHighlighted
        }

        if (currentSegment) {
            segments.push({ text: currentSegment, highlighted: currentHighlighted })
        }

        return segments
    }

    /** Format shortcuts for display */
    function formatShortcuts(shortcuts: string[]): string {
        return shortcuts.slice(0, 2).join(' / ')
    }
</script>

<div
    class="palette-overlay"
    role="dialog"
    aria-modal="true"
    aria-labelledby="palette-title"
    tabindex="-1"
    onclick={handleOverlayClick}
    onkeydown={handleKeyDown}
>
    <div class="palette-modal">
        <input
            bind:this={inputElement}
            type="text"
            class="search-input"
            placeholder="Search commands..."
            bind:value={query}
            aria-label="Search commands"
            id="palette-title"
            spellcheck="false"
            autocomplete="off"
            autocapitalize="off"
            role="combobox"
            aria-controls="palette-listbox"
            aria-expanded={results.length > 0}
            aria-autocomplete="list"
            aria-activedescendant={activeDescendantId}
        />

        {#if results.length === 0 && query.trim()}
            <div class="no-results">No commands found</div>
        {:else}
            <div
                id="palette-listbox"
                class="results-container"
                bind:this={resultsContainer}
                role="listbox"
                aria-label="Commands"
                tabindex="-1"
            >
                {#each results as match, index (match.command.id)}
                    {#if recentCount > 0 && index === 0}
                        <div class="group-heading">Recent</div>
                    {/if}
                    {#if recentCount > 0 && index === recentCount}
                        <div class="group-heading">All commands</div>
                    {/if}
                    <div
                        class="result-item"
                        id={optionId(match.command.id)}
                        class:is-under-cursor={index === cursorIndex}
                        class:is-hovered={hoveredIndex === index && index !== cursorIndex}
                        onclick={() => {
                            handleResultClick(index)
                        }}
                        onmouseenter={() => {
                            hoveredIndex = index
                        }}
                        onmouseleave={() => {
                            hoveredIndex = null
                        }}
                        role="option"
                        tabindex={index === cursorIndex ? 0 : -1}
                        aria-selected={index === cursorIndex}
                    >
                        <span class="command-name">
                            {#each highlightMatches(match) as segment, segIdx (segIdx)}
                                {#if segment.highlighted}
                                    <mark class="match-highlight">{segment.text}</mark>
                                {:else}
                                    {segment.text}
                                {/if}
                            {/each}
                        </span>
                        {#if match.command.shortcuts.length > 0}
                            <span class="shortcuts">{formatShortcuts(match.command.shortcuts)}</span>
                        {/if}
                    </div>
                {/each}
            </div>
        {/if}
    </div>
</div>

<style>
    .palette-overlay {
        position: fixed;
        /* Start below the title bar so the scrim never covers the OS window-drag
           region: the user can still drag the window while a dialog is open.
           `--titlebar-height` is per-window (see app.css § Window chrome). */
        inset: var(--titlebar-height) 0 0 0;
        background: var(--color-overlay);
        backdrop-filter: blur(2px);
        display: flex;
        justify-content: center;
        align-items: flex-start;
        padding-top: 15vh;
        z-index: var(--z-modal);
    }

    .palette-modal {
        background: var(--color-bg-secondary);
        border: 1px solid var(--color-border-strong);
        border-radius: var(--radius-lg);
        width: 500px;
        display: flex;
        flex-direction: column;
        box-shadow: var(--shadow-lg);
        overflow: hidden;
    }

    .search-input {
        padding: var(--spacing-md) var(--spacing-lg);
        font-size: var(--font-size-lg);
        border: none;
        border-bottom: 1px solid var(--color-border-strong);
        background: var(--color-bg-primary);
        color: var(--color-text-primary);
        outline: none;
        flex-shrink: 0;
    }

    .search-input:focus {
        border-bottom-color: var(--color-accent);
        box-shadow: var(--shadow-focus);
    }

    .search-input::placeholder {
        color: var(--color-text-tertiary);
    }

    .results-container {
        overflow-y: scroll; /* Always show scrollbar */
        max-height: 320px;
    }

    .no-results {
        padding: var(--spacing-lg);
        text-align: center;
        color: var(--color-text-tertiary);
        font-size: var(--font-size-md);
    }

    /* Section headers between recents and the rest of the palette */
    .group-heading {
        padding: var(--spacing-sm) var(--spacing-lg) var(--spacing-xxs);
        font-size: var(--font-size-xs);
        font-weight: 600;
        color: var(--color-text-tertiary);
        text-transform: uppercase;
        letter-spacing: 0.05em;
        border-top: 1px solid var(--color-border);
    }

    /* First heading sits right after the input, no top border */
    .group-heading:first-child {
        border-top: none;
    }

    .result-item {
        display: flex;
        justify-content: space-between;
        align-items: center;
        padding: var(--spacing-sm) var(--spacing-lg);
        font-size: var(--font-size-md);
        color: var(--color-text-primary);
    }

    /* Hover state - just a subtle overlay */
    .result-item.is-hovered {
        background: var(--color-tint-hover);
    }

    /* Cursor state - accent-tinted highlight */
    .result-item.is-under-cursor {
        background: var(--color-accent-subtle);
    }

    .result-item.is-under-cursor .shortcuts {
        color: var(--color-text-secondary);
    }

    .command-name {
        flex: 1;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    /* Match highlight - underline style that doesn't compromise text contrast */
    .match-highlight {
        color: inherit;
        background: none;
        text-decoration: underline;
        text-decoration-color: var(--color-accent);
        text-underline-offset: 2px;
        text-decoration-thickness: 2px;
    }

    /* When item is under cursor, make the match highlight even more visible */
    .result-item.is-under-cursor .match-highlight {
        text-decoration-color: var(--color-text-primary);
    }

    .shortcuts {
        margin-left: var(--spacing-lg);
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
        flex-shrink: 0;
    }

    /* Light mode overrides handled by design tokens (--color-tint-hover) */
</style>
