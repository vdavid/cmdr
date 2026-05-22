<script lang="ts">
    /**
     * RecentSearchesPopover: fuzzy-searchable list over the full recent-searches history.
     *
     * Opens via `⌘H` or the "All searches…" footer chip. Reuses `FilterChipPopover` for
     * positioning, focus trap, and Esc-only-closes-the-popover behavior — same contract as
     * the filter chips (so the dialog's capture-phase Escape never closes the whole dialog
     * while this is open).
     *
     * The list is fuzzy-searched via `@leeoniya/ufuzzy`, the same library the command palette
     * uses. The haystack is `"{mode-badge} {query}"` per entry, so users can also filter by
     * mode (`"AI screenshots"`, `".*temp"`).
     *
     * Keyboard: ↑/↓ moves the cursor, Enter activates, Esc closes (via the popover wrapper).
     */
    import uFuzzy from '@leeoniya/ufuzzy'
    import FilterChipPopover from './FilterChipPopover.svelte'
    import type { HistoryEntry } from '$lib/tauri-commands'
    import { chipTooltip, modeBadge } from './recent-searches-utils'

    interface Props {
        anchor: HTMLElement
        open: boolean
        entries: HistoryEntry[]
        onClose: () => void
        onPick: (entry: HistoryEntry) => void
        onRemove: (entry: HistoryEntry) => void
    }

    const { anchor, open, entries, onClose, onPick, onRemove }: Props = $props()

    // Tuned the same way as the command palette's fuzzy search.
    const fuzzy = new uFuzzy({ intraMode: 1, interIns: 3 })

    let query = $state('')
    let cursor = $state(0)
    let inputEl: HTMLInputElement | undefined = $state()

    // Reset state every time the popover re-opens so users land on a clean view.
    $effect(() => {
        if (open) {
            query = ''
            cursor = 0
        }
    })

    // Build the haystack lazily; cheap relative to the user's typing speed.
    const haystack = $derived(entries.map((e) => `${modeBadge(e.mode)} ${e.query}`))

    interface Match {
        entry: HistoryEntry
        indices: number[]
        haystackText: string
    }

    const matches = $derived.by<Match[]>(() => {
        const trimmed = query.trim()
        if (!trimmed) {
            // Empty query: show everything in original order (newest first).
            return entries.map((entry, i) => ({
                entry,
                indices: [],
                haystackText: haystack[i],
            }))
        }
        const [idxs, info, order] = fuzzy.search(haystack, trimmed)
        if (!idxs || !order) return []
        return order.map((orderIdx) => {
            const haystackIdx = idxs[orderIdx]
            const entry = entries[haystackIdx]
            const ranges = info.ranges[orderIdx]
            const indices: number[] = []
            for (let i = 0; i < ranges.length; i += 2) {
                const start = ranges[i]
                const end = ranges[i + 1]
                for (let j = start; j < end; j++) indices.push(j)
            }
            return { entry, indices, haystackText: haystack[haystackIdx] }
        })
    })

    /** Clamp cursor whenever the match list shrinks below it. */
    $effect(() => {
        if (cursor >= matches.length) {
            cursor = Math.max(0, matches.length - 1)
        }
    })

    /** Highlight matched characters in the haystack text for the active match. */
    function renderHighlights(text: string, indices: number[]): { ch: string; matched: boolean }[] {
        const set = new Set(indices)
        return Array.from(text).map((ch, i) => ({ ch, matched: set.has(i) }))
    }

    function handleKeydown(e: KeyboardEvent): void {
        if (e.key === 'ArrowDown') {
            e.preventDefault()
            cursor = Math.min(cursor + 1, Math.max(0, matches.length - 1))
        } else if (e.key === 'ArrowUp') {
            e.preventDefault()
            cursor = Math.max(0, cursor - 1)
        } else if (e.key === 'Enter') {
            e.preventDefault()
            // `cursor` is clamped against `matches.length` above; runtime bounds
            // guard for the empty-matches case (no row to activate).
            const m = matches[cursor]
            // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard for empty matches
            if (m) {
                onPick(m.entry)
            }
        }
    }

    function handleContextMenu(e: MouseEvent, entry: HistoryEntry): void {
        e.preventDefault()
        onRemove(entry)
    }
</script>

<FilterChipPopover {anchor} {open} {onClose} ariaLabel="All recent searches">
    <div class="recent-popover" onkeydown={handleKeydown} role="search">
        <input
            bind:this={inputEl}
            type="text"
            class="search-field"
            placeholder="Filter recent searches"
            bind:value={query}
            aria-label="Filter recent searches"
        />
        <div class="results" role="listbox" aria-label="Recent searches">
            {#if matches.length === 0}
                <div class="empty">No recent searches match that filter.</div>
            {:else}
                {#each matches as match, index (match.entry.id)}
                    <button
                        type="button"
                        class="result-row"
                        class:is-cursor={index === cursor}
                        role="option"
                        aria-selected={index === cursor}
                        title={chipTooltip(match.entry)}
                        onclick={() => {
                            onPick(match.entry)
                        }}
                        oncontextmenu={(e) => {
                            handleContextMenu(e, match.entry)
                        }}
                        onmousemove={() => {
                            cursor = index
                        }}
                    >
                        <span class="row-mode">{modeBadge(match.entry.mode)}</span>
                        <span class="row-query">
                            {#each renderHighlights(match.haystackText.slice(modeBadge(match.entry.mode).length + 1), match.indices.filter((i) => i >= modeBadge(match.entry.mode).length + 1).map((i) => i - modeBadge(match.entry.mode).length - 1)) as part, i (i)}
                                {#if part.matched}
                                    <strong>{part.ch}</strong>
                                {:else}
                                    {part.ch}
                                {/if}
                            {/each}
                        </span>
                    </button>
                {/each}
            {/if}
        </div>
        <div class="hint">↑↓ to move · Enter to run · right-click to remove</div>
    </div>
</FilterChipPopover>

<style>
    .recent-popover {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-sm);
        width: 480px;
        max-width: 90vw;
    }

    .search-field {
        padding: var(--spacing-xs) var(--spacing-sm);
        font-size: var(--font-size-sm);
        background: var(--color-bg-secondary);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        color: var(--color-text-primary);
    }

    .search-field:focus {
        outline: 2px solid var(--color-accent);
        outline-offset: -2px;
    }

    .results {
        display: flex;
        flex-direction: column;
        max-height: 360px;
        overflow-y: auto;
        scrollbar-width: thin;
    }

    .empty {
        padding: var(--spacing-md);
        color: var(--color-text-tertiary);
        font-size: var(--font-size-sm);
        text-align: center;
    }

    .result-row {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
        padding: var(--spacing-xs) var(--spacing-sm);
        background: transparent;
        border: 0;
        text-align: left;
        font-size: var(--font-size-sm);
        color: var(--color-text-primary);
        border-radius: var(--radius-xs);
    }

    .result-row.is-cursor {
        background: var(--color-accent-subtle);
    }

    .row-mode {
        font-size: var(--font-size-xs);
        font-family: var(--font-mono);
        font-weight: 600;
        color: var(--color-text-secondary);
        flex-shrink: 0;
        width: 24px;
    }

    .row-query {
        flex: 1;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .row-query strong {
        font-weight: 600;
        color: var(--color-text-primary);
        background: var(--color-accent-subtle);
        border-radius: var(--radius-xs);
    }

    .hint {
        color: var(--color-text-tertiary);
        font-size: var(--font-size-xs);
        padding-top: var(--spacing-xxs);
        border-top: 1px solid var(--color-border-subtle);
    }
</style>
