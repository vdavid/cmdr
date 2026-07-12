<script lang="ts">
    import { onMount, onDestroy } from 'svelte'
    import { tooltip } from '$lib/tooltip/tooltip'
    import { useShortenMiddle } from '$lib/utils/shorten-middle-action'
    import type { ClosedTab } from '$lib/file-explorer/tabs/tab-state-manager.svelte'

    interface ClosedTabsPayload {
        left: ClosedTab[]
        right: ClosedTab[]
        focusedPane: 'left' | 'right'
    }

    let leftStack = $state<ClosedTab[]>([])
    let rightStack = $state<ClosedTab[]>([])
    let focusedPane = $state<'left' | 'right'>('left')
    let unlisten: (() => void) | undefined

    onMount(async () => {
        try {
            const { listen } = await import('@tauri-apps/api/event')
            unlisten = await listen<ClosedTabsPayload>('debug-closed-tabs', (event) => {
                leftStack = event.payload.left
                rightStack = event.payload.right
                focusedPane = event.payload.focusedPane
            })
        } catch {
            // Not in Tauri environment
        }
    })

    onDestroy(() => {
        unlisten?.()
    })

    /** Multi-line tooltip text for one closed tab (tooltip CSS is `white-space: pre-line`). */
    function tabDetails(entry: ClosedTab): string {
        const t = entry.tab
        const sort = `${t.sortBy} ${t.sortOrder}`
        const cursor = t.cursorFilename ?? '(none)'
        const lines = [
            `Path: ${t.path}`,
            `Volume: ${t.volumeId}`,
            `Sort: ${sort}`,
            `View: ${t.viewMode}`,
            `Pinned: ${t.pinned ? 'yes' : 'no'}`,
            `Cursor: ${cursor}`,
            `Original index: ${String(entry.originalIndex)}`,
            `Tab id: ${t.id}`,
        ]
        return lines.join('\n')
    }
</script>

<section class="debug-section">
    <h2>Closed tabs</h2>
    <div class="closed-tabs-panes">
        <div class="closed-tabs-pane" class:focused={focusedPane === 'left'}>
            <h3>Left pane</h3>
            {#if leftStack.length > 0}
                <ul class="closed-tabs-list">
                    {#each leftStack as entry, i (`${entry.tab.id}-${String(i)}`)}
                        {@const isTop = i === leftStack.length - 1}
                        <li class:top={isTop} use:tooltip={tabDetails(entry)}>
                            <span class="closed-tab-marker">{isTop ? '↑' : '·'}</span>
                            <span
                                class="closed-tab-path"
                                use:useShortenMiddle={{ text: entry.tab.path, preferBreakAt: '/' }}
                            ></span>
                        </li>
                    {/each}
                </ul>
            {:else}
                <p class="no-closed-tabs">No recently closed tabs</p>
            {/if}
        </div>
        <div class="closed-tabs-pane" class:focused={focusedPane === 'right'}>
            <h3>Right pane</h3>
            {#if rightStack.length > 0}
                <ul class="closed-tabs-list">
                    {#each rightStack as entry, i (`${entry.tab.id}-${String(i)}`)}
                        {@const isTop = i === rightStack.length - 1}
                        <li class:top={isTop} use:tooltip={tabDetails(entry)}>
                            <span class="closed-tab-marker">{isTop ? '↑' : '·'}</span>
                            <span
                                class="closed-tab-path"
                                use:useShortenMiddle={{ text: entry.tab.path, preferBreakAt: '/' }}
                            ></span>
                        </li>
                    {/each}
                </ul>
            {:else}
                <p class="no-closed-tabs">No recently closed tabs</p>
            {/if}
        </div>
    </div>
</section>

<style>
    /* stylelint-disable declaration-property-value-disallowed-list -- Dev utility window */

    .closed-tabs-panes {
        display: flex;
        gap: var(--spacing-md);
    }

    .closed-tabs-pane {
        flex: 1;
        background: var(--color-bg-secondary);
        border-radius: var(--radius-md);
        padding: var(--spacing-sm);
        min-width: 0;
    }

    .closed-tabs-pane.focused {
        outline: 2px solid var(--color-accent);
    }

    .closed-tabs-pane h3 {
        margin: 0 0 var(--spacing-sm);
        font-size: var(--font-size-sm);
        font-weight: 600;
        color: var(--color-text-secondary);
        text-transform: uppercase;
    }

    .closed-tabs-list {
        list-style: none;
        margin: 0;
        padding: 0;
        font-size: var(--font-size-sm);
        font-family: var(--font-mono);
    }

    .closed-tabs-list li {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
        padding: 3px 4px;
        border-radius: var(--radius-sm);
        color: var(--color-text-secondary);
        min-width: 0;
    }

    .closed-tabs-list li.top {
        background: var(--color-bg-tertiary);
        color: var(--color-text-primary);
        font-weight: 600;
    }

    .closed-tab-path {
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    /* Row marker prefix (↑ for the top entry, · otherwise). */
    .closed-tab-marker {
        flex-shrink: 0;
        width: 12px;
        text-align: center;
        color: var(--color-text-tertiary);
    }

    .no-closed-tabs {
        margin: 0;
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
        font-style: italic;
    }
</style>
