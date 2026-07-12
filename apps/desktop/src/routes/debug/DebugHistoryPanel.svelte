<script lang="ts">
    import { onMount, onDestroy } from 'svelte'
    import { tooltip } from '$lib/tooltip/tooltip'

    interface HistoryEntry {
        volumeId: string
        path: string
        networkHost?: { name: string; hostname: string }
    }

    interface NavigationHistory {
        stack: HistoryEntry[]
        currentIndex: number
    }

    interface HistoryPayload {
        left: NavigationHistory
        right: NavigationHistory
        focusedPane: 'left' | 'right'
    }

    let leftHistory = $state<NavigationHistory | null>(null)
    let rightHistory = $state<NavigationHistory | null>(null)
    let focusedPane = $state<'left' | 'right'>('left')
    let unlisten: (() => void) | undefined

    onMount(async () => {
        try {
            const { listen } = await import('@tauri-apps/api/event')
            unlisten = await listen<HistoryPayload>('debug-history', (event) => {
                leftHistory = event.payload.left
                rightHistory = event.payload.right
                focusedPane = event.payload.focusedPane
            })
        } catch {
            // Not in Tauri environment
        }
    })

    onDestroy(() => {
        unlisten?.()
    })

    /** Format a history entry for display */
    function formatEntry(entry: HistoryEntry): string {
        if (entry.networkHost) {
            return `${entry.networkHost.name} (${entry.networkHost.hostname})`
        }
        const parts = entry.path.split('/')
        const lastPart = parts[parts.length - 1] || parts[parts.length - 2] || entry.path
        if (entry.volumeId !== 'root' && entry.volumeId !== 'network') {
            const volumeName = entry.volumeId.split('/').pop() ?? entry.volumeId
            return `[${volumeName}] ${lastPart}`
        }
        return lastPart || entry.path
    }
</script>

<section class="debug-section">
    <h2>Navigation history</h2>
    <div class="history-panes">
        <div class="history-pane" class:focused={focusedPane === 'left'}>
            <h3>Left pane</h3>
            {#if leftHistory}
                <ul class="history-list">
                    {#each leftHistory.stack as entry, i (i)}
                        {@const isCurrent = i === leftHistory.currentIndex}
                        {@const isFuture = i > leftHistory.currentIndex}
                        {@const arrow = isCurrent ? '→' : i < leftHistory.currentIndex ? '←' : '↓'}
                        <li class:current={isCurrent} class:future={isFuture}>
                            <span class="history-index">{arrow}</span>
                            <span class="history-path" use:tooltip={{ text: entry.path, overflowOnly: true }}
                                >{formatEntry(entry)}</span
                            >
                        </li>
                    {/each}
                </ul>
            {:else}
                <p class="no-history">No history yet</p>
            {/if}
        </div>
        <div class="history-pane" class:focused={focusedPane === 'right'}>
            <h3>Right pane</h3>
            {#if rightHistory}
                <ul class="history-list">
                    {#each rightHistory.stack as entry, i (i)}
                        {@const isCurrent = i === rightHistory.currentIndex}
                        {@const isFuture = i > rightHistory.currentIndex}
                        {@const arrow = isCurrent ? '→' : i < rightHistory.currentIndex ? '←' : '↓'}
                        <li class:current={isCurrent} class:future={isFuture}>
                            <span class="history-index">{arrow}</span>
                            <span class="history-path" use:tooltip={{ text: entry.path, overflowOnly: true }}
                                >{formatEntry(entry)}</span
                            >
                        </li>
                    {/each}
                </ul>
            {:else}
                <p class="no-history">No history yet</p>
            {/if}
        </div>
    </div>
</section>

<style>
    /* stylelint-disable declaration-property-value-disallowed-list -- Dev utility window */

    .history-panes {
        display: flex;
        gap: var(--spacing-md);
    }

    .history-pane {
        flex: 1;
        background: var(--color-bg-secondary);
        border-radius: var(--radius-md);
        padding: var(--spacing-sm);
        min-width: 0;
    }

    .history-pane.focused {
        outline: 2px solid var(--color-accent);
    }

    .history-pane h3 {
        margin: 0 0 var(--spacing-sm);
        font-size: var(--font-size-sm);
        font-weight: 600;
        color: var(--color-text-secondary);
        text-transform: uppercase;
    }

    .history-list {
        list-style: none;
        margin: 0;
        padding: 0;
        font-size: var(--font-size-sm);
        font-family: var(--font-mono);
    }

    .history-list li {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
        padding: 3px 4px;
        border-radius: var(--radius-sm);
        color: var(--color-text-secondary);
        min-width: 0;
    }

    .history-list li.current {
        background: var(--color-bg-tertiary);
        color: var(--color-text-primary);
        font-weight: 600;
    }

    .history-list li.future {
        opacity: 0.5;
    }

    .history-index {
        flex-shrink: 0;
        width: 12px;
        text-align: center;
    }

    .history-path {
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }
</style>
