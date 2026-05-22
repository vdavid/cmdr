<script lang="ts">
    /**
     * RecentSearchesFooter: chip strip at the bottom of the dialog showing the latest 6
     * recent searches plus an "All searches…" trailing chip that opens the popover.
     *
     * Each chip carries a small mode badge (`AI` / `Aa` / `.*`). Clicking a chip loads the
     * entry into the dialog's state and runs it. For AI entries, the click counts as the
     * user's explicit "yes, please run this" (search-redesign-plan §3.4 / §3.5). Right-click
     * opens a context menu with "Remove from history".
     *
     * Hidden when there are zero entries (the empty state already covers the discoverability
     * gap there).
     */
    import { tooltip } from '$lib/tooltip/tooltip'
    import type { HistoryEntry } from '$lib/tauri-commands'
    import { chipTooltip, modeBadge } from './recent-searches-utils'

    interface Props {
        entries: HistoryEntry[]
        /** True when the index isn't ready; chips render disabled to avoid no-op clicks. */
        disabled: boolean
        /** Called when a chip is activated. Parent loads + runs the entry. */
        onPick: (entry: HistoryEntry) => void
        /** Called when the user wants to remove an entry via right-click. */
        onRemove: (entry: HistoryEntry) => void
        /** Called when the user clicks "All searches…" or activates it via keyboard. */
        onOpenAll: () => void
    }

    const { entries, disabled, onPick, onRemove, onOpenAll }: Props = $props()

    const VISIBLE_CHIPS = 6
    const visible = $derived(entries.slice(0, VISIBLE_CHIPS))

    function handleContextMenu(e: MouseEvent, entry: HistoryEntry): void {
        e.preventDefault()
        onRemove(entry)
    }
</script>

{#if visible.length > 0}
    <div class="recent-footer" role="region" aria-label="Recent searches">
        {#each visible as entry (entry.id)}
            <button
                type="button"
                class="recent-chip"
                {disabled}
                onclick={() => {
                    onPick(entry)
                }}
                oncontextmenu={(e) => {
                    handleContextMenu(e, entry)
                }}
                use:tooltip={chipTooltip(entry)}
                aria-label={`Run recent search: ${entry.query}`}
            >
                <span class="chip-badge">{modeBadge(entry.mode)}</span>
                <span class="chip-query">{entry.query}</span>
            </button>
        {/each}
        <button
            type="button"
            class="all-searches"
            {disabled}
            onclick={onOpenAll}
            use:tooltip={'Show all recent searches (⌘H)'}
            aria-label="All recent searches"
        >
            All searches…
        </button>
    </div>
{/if}

<style>
    .recent-footer {
        display: flex;
        flex-wrap: nowrap;
        align-items: center;
        gap: var(--spacing-xs);
        padding: var(--spacing-sm) var(--spacing-lg);
        background: var(--color-bg-primary);
        border-top: 1px solid var(--color-border-subtle);
        overflow-x: auto;
        scrollbar-width: thin;
    }

    .recent-chip,
    .all-searches {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xs);
        padding: var(--spacing-xxs) var(--spacing-sm);
        font-size: var(--font-size-sm);
        font-weight: 500;
        line-height: 1;
        color: var(--color-text-secondary);
        background: transparent;
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        white-space: nowrap;
        max-width: 240px;
        transition:
            background var(--transition-base),
            border-color var(--transition-base),
            color var(--transition-base);
    }

    .recent-chip:not(:disabled):hover,
    .all-searches:not(:disabled):hover {
        background: var(--color-accent-subtle);
        border-color: var(--color-accent);
        color: var(--color-text-primary);
    }

    .recent-chip:disabled,
    .all-searches:disabled {
        opacity: 0.5;
        cursor: not-allowed;
    }

    .chip-badge {
        font-size: var(--font-size-xs);
        font-family: var(--font-mono);
        font-weight: 600;
        letter-spacing: 0.04em;
        padding: var(--spacing-xxs) var(--spacing-xs);
        background: var(--color-accent-subtle);
        color: var(--color-text-primary);
        border-radius: var(--radius-xs);
        line-height: 1;
    }

    .chip-query {
        line-height: 1;
        overflow: hidden;
        text-overflow: ellipsis;
        max-width: 180px;
    }

    .all-searches {
        font-style: italic;
        color: var(--color-text-tertiary);
    }
</style>
