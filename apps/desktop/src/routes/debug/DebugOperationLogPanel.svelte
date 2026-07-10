<script lang="ts">
    import { onMount } from 'svelte'
    import { commands, type OperationRow } from '$lib/ipc/bindings'
    import { formatInteger } from '$lib/intl/number-format'

    // Dev-only inspector for the operation log's read side (the Debug panel, requirement 6a).
    // Reads through the same typed `get_recent_operation_log_entries` IPC the alpha
    // dialog uses. Plain English on purpose: the Debug window is never shipped
    // to users, so it skips the i18n catalog (same as every other Debug*Panel).

    const PAGE = 50

    type Loadable = 'idle' | 'loading' | 'ready' | 'error'

    let rows = $state<OperationRow[]>([])
    let loadState: Loadable = $state('idle')
    let errorMessage = $state('')
    let hasMore = $state(false)

    onMount(() => void reload())

    async function fetchPage(offset: number): Promise<OperationRow[]> {
        const result = await commands.getRecentOperationLogEntries(PAGE, offset)
        // tauri-specta wraps Result<T, E> as { status, data | error }.
        if (typeof result === 'object' && 'status' in result) {
            if (result.status === 'ok') return result.data
            throw new Error(result.error)
        }
        return result
    }

    async function reload() {
        loadState = 'loading'
        try {
            const page = await fetchPage(0)
            rows = page
            hasMore = page.length === PAGE
            loadState = 'ready'
            errorMessage = ''
        } catch (e) {
            loadState = 'error'
            errorMessage = String(e)
        }
    }

    async function loadMore() {
        try {
            const page = await fetchPage(rows.length)
            rows = [...rows, ...page]
            hasMore = page.length === PAGE
        } catch (e) {
            loadState = 'error'
            errorMessage = String(e)
        }
    }

    function fmtTime(epochSecs: number | null): string {
        // Dev-only inspector: a stable ISO/UTC stamp is fine (and avoids the
        // locale-format lint the shipped UI must obey). Trim to whole seconds.
        if (epochSecs == null) return '–'
        return new Date(epochSecs * 1000).toISOString().replace('T', ' ').replace(/\.\d+Z$/, ' UTC')
    }

    function volumes(row: OperationRow): string {
        const src = row.sourceVolumeId ?? '–'
        if (row.destVolumeId == null || row.destVolumeId === row.sourceVolumeId) return src
        return `${src} → ${row.destVolumeId}`
    }
</script>

<div class="debug-section">
    <h2>Operation log</h2>
    <div class="oplog-actions">
        <button class="index-button" onclick={() => void reload()}>Refresh</button>
        <span class="index-message">
            {#if loadState === 'loading'}Loading…{:else}{formatInteger(rows.length)} shown{/if}
        </span>
    </div>

    {#if loadState === 'error'}
        <p class="oplog-error">Couldn't read the operation log: {errorMessage}</p>
    {:else if loadState === 'ready' && rows.length === 0}
        <p class="no-history">No operations logged yet.</p>
    {:else}
        <div class="oplog-list">
            {#each rows as row (row.opId)}
                <div class="oplog-row">
                    <div class="oplog-row-head">
                        <span class="oplog-kind">{row.kind}{row.archiveSubkind ? ` (${row.archiveSubkind})` : ''}</span>
                        <span class="oplog-badge">{row.initiator}</span>
                        <span class="oplog-badge">{row.executionStatus}</span>
                        <span class="oplog-badge oplog-rollback">{row.rollbackState}</span>
                        {#if row.notRollbackableReason}
                            <span class="oplog-reason">{row.notRollbackableReason}</span>
                        {/if}
                    </div>
                    <div class="oplog-meta">
                        <span>{formatInteger(row.itemsDone)} / {formatInteger(row.itemCount)} items</span>
                        <span>{volumes(row)}</span>
                        <span>coverage: {row.searchCoverage}{row.searchCoverageReason ? ` (${row.searchCoverageReason})` : ''}</span>
                    </div>
                    <div class="oplog-meta oplog-meta-dim">
                        <span>{fmtTime(row.startedAt)} → {fmtTime(row.endedAt)}</span>
                        <span class="oplog-id">{row.opId}</span>
                    </div>
                </div>
            {/each}
        </div>
        {#if hasMore}
            <button class="index-button" onclick={() => void loadMore()}>Load 50 more</button>
        {/if}
    {/if}
</div>

<style>
    .oplog-actions {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
        margin-bottom: var(--spacing-md);
    }

    .oplog-error {
        margin: 0;
        font-size: var(--font-size-sm);
        color: var(--color-error-text);
    }

    .oplog-list {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
    }

    .oplog-row {
        background: var(--color-bg-secondary);
        border-radius: var(--radius-md);
        padding: var(--spacing-sm) var(--spacing-md);
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xxs);
    }

    .oplog-row-head {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
        flex-wrap: wrap;
    }

    .oplog-kind {
        font-weight: 600;
        font-family: var(--font-mono);
    }

    .oplog-badge {
        font-size: var(--font-size-xs);
        padding: 1px var(--spacing-xs);
        border-radius: var(--radius-sm);
        background: var(--color-bg-tertiary);
        color: var(--color-text-secondary);
    }

    .oplog-rollback {
        background: var(--color-accent-subtle);
        color: var(--color-text-primary);
    }

    .oplog-reason {
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
        font-style: italic;
    }

    .oplog-meta {
        display: flex;
        gap: var(--spacing-md);
        font-size: var(--font-size-xs);
        color: var(--color-text-secondary);
        flex-wrap: wrap;
    }

    .oplog-meta-dim {
        color: var(--color-text-tertiary);
    }

    .oplog-id {
        font-family: var(--font-mono);
    }
</style>
