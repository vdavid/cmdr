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
