<script lang="ts">
    /**
     * CommandPalette - VS Code-style command palette modal.
     *
     * Features:
     * - Fuzzy search with highlighted matches
     * - Keyboard navigation (↑/↓/Enter/Escape)
     * - Persists last query across app restarts
     * - Blocks keyboard events from propagating to file explorer
     */
    import { onMount, tick } from 'svelte'
    import { searchCommands, type CommandMatch } from '$lib/commands'
    import { loadPaletteQuery, savePaletteQuery } from '$lib/app-status-store'

    interface Props {
        /** Called when user selects a command */
        onExecute: (commandId: string) => void
        /** Called when palette is closed */
        onClose: () => void
    }

    const { onExecute, onClose }: Props = $props()

    let query = $state('')
    let cursorIndex = $state(0)
    let hoveredIndex = $state<number | null>(null)
    let inputElement: HTMLInputElement | undefined = $state()
    let resultsContainer: HTMLDivElement | undefined = $state()

    // Derived: filtered and ranked results
    const results = $derived(searchCommands(query))

    // Reset cursor position when query changes
    $effect(() => {
        void query // Track
        cursorIndex = 0
        hoveredIndex = null
    })

    onMount(() => {
        // Load persisted query and focus input
        void loadPaletteQuery().then((savedQuery) => {
            query = savedQuery
            void tick().then(() => {
                inputElement?.focus()
                inputElement?.select()
            })
        })
    })

    function handleKeyDown(e: KeyboardEvent) {
        // CRITICAL: Stop propagation to prevent file explorer from handling these
        e.stopPropagation()

        switch (e.key) {
            case 'Escape':
                e.preventDefault()
                void savePaletteQuery(query)
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
                    void savePaletteQuery(query)
                    onExecute(results[cursorIndex].command.id)
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
        void savePaletteQuery(query)
        onExecute(results[index].command.id)
    }

    function handleOverlayClick(e: MouseEvent) {
        // Only close if clicking the overlay itself, not the modal content
        if (e.target === e.currentTarget) {
            void savePaletteQuery(query)
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
            autocorrect="off"
            autocapitalize="off"
        />

        <div class="results-container" bind:this={resultsContainer}>
            {#if results.length === 0 && query.trim()}
                <div class="no-results">No commands found</div>
            {:else}
                {#each results as match, index (match.command.id)}
                    <div
                        class="result-item"
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
                        tabindex="-1"
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
            {/if}
        </div>
    </div>
</div>

<style>
    .palette-overlay {
        position: fixed;
        inset: 0;
        background: rgba(0, 0, 0, 0.5);
        backdrop-filter: blur(2px);
        display: flex;
        justify-content: center;
        align-items: flex-start;
        padding-top: 15vh;
        z-index: 10000;
    }

    .palette-modal {
        background: var(--color-bg-secondary);
        border: 1px solid var(--color-border-primary);
        border-radius: 8px;
        width: 500px;
        display: flex;
        flex-direction: column;
        box-shadow: 0 16px 48px rgba(0, 0, 0, 0.4);
        overflow: hidden;
    }

    .search-input {
        padding: 12px 16px;
        font-size: 16px;
        border: none;
        border-bottom: 1px solid var(--color-border-primary);
        background: var(--color-bg-primary);
        color: var(--color-text-primary);
        outline: none;
        flex-shrink: 0;
    }

    .search-input::placeholder {
        color: var(--color-text-muted);
    }

    .results-container {
        overflow-y: scroll; /* Always show scrollbar */
        max-height: 320px;
    }

    .no-results {
        padding: 16px;
        text-align: center;
        color: var(--color-text-muted);
        font-size: 14px;
    }

    .result-item {
        display: flex;
        justify-content: space-between;
        align-items: center;
        padding: 8px 16px;
        cursor: pointer;
        font-size: 14px;
        color: var(--color-text-primary);
    }

    /* Hover state - just a subtle overlay */
    .result-item.is-hovered {
        background: rgba(255, 255, 255, 0.06);
    }

    /* Cursor state - a full-on highlight */
    .result-item.is-under-cursor {
        background: var(--color-cursor-focused-bg);
        color: var(--color-cursor-focused-fg);
    }

    .result-item.is-under-cursor .shortcuts {
        color: var(--color-cursor-focused-fg);
        opacity: 0.8;
    }

    .command-name {
        flex: 1;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    /* Match highlight - macOS Spotlight-style with visible background */
    .match-highlight {
        background: rgba(255, 255, 255, 0.25);
        color: inherit;
        border-radius: 3px;
        padding: 1px 2px;
        margin: 0 -2px;
    }

    /* When item is under cursor, make the match highlight even more visible */
    .result-item.is-under-cursor .match-highlight {
        background: rgba(255, 255, 255, 0.35);
    }

    .shortcuts {
        margin-left: 16px;
        font-size: 12px;
        color: var(--color-text-muted);
        flex-shrink: 0;
    }

    /* Support light mode */
    @media (prefers-color-scheme: light) {
        .result-item.is-hovered {
            background: rgba(0, 0, 0, 0.04);
        }

        .match-highlight {
            background: rgba(0, 0, 0, 0.15);
        }

        .result-item.is-under-cursor .match-highlight {
            background: rgba(255, 255, 255, 0.4);
        }
    }
</style>
