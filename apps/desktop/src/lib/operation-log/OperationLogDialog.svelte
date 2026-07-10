<script lang="ts">
    /**
     * The alpha "Operation log" dialog (requirement 6b): the newest file operations,
     * newest first, each expandable to its per-item rows (a mass rename is one
     * collapsible group). Debugging/demo quality by design — it may become a sidebar
     * later — but fully i18n'd, style-guide compliant, and a11y-basic (ModalDialog's
     * focus trap, expandable rows as real buttons with `aria-expanded`).
     *
     * Every label comes from a typed enum via `operation-log-labels` (never a
     * backend-rendered string); the summary is an ICU plural formatted per viewer.
     */
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import Button from '$lib/ui/Button.svelte'
    import Spinner from '$lib/ui/Spinner.svelte'
    import Icon from '$lib/ui/Icon.svelte'
    import StatusBadge from '$lib/ui/StatusBadge.svelte'
    import { getBadgeStatus } from '$lib/feature-status'
    import { tString } from '$lib/intl/messages.svelte'
    import { formatInteger } from '$lib/intl/number-format'
    import { formatDateTime } from '$lib/settings/reactive-settings.svelte'
    import { getOperationLogDetail, type OperationItemView, type OperationRow } from '$lib/tauri-commands'
    import { getAppLogger } from '$lib/logging/logger'
    import { SvelteMap, SvelteSet } from 'svelte/reactivity'
    import { operationLogState, closeOperationLog, loadMoreOperations } from './operation-log-trigger.svelte'
    import {
        operationSummary,
        initiatorLabel,
        executionStatusLabel,
        rollbackStateLabel,
        itemOutcomeLabel,
    } from './operation-log-labels'

    const log = getAppLogger('operationLogDialog')

    // Alpha badge policy: the status comes from the repo-root feature-status.json.
    const badge = getBadgeStatus('operation-log')

    /** How many item rows one expansion fetches; enough for any realistic group. */
    const ITEM_PAGE = 200

    interface ItemsState {
        loading: boolean
        error: boolean
        items: OperationItemView[]
        total: number
    }

    // Per-operation expansion + lazily fetched items, keyed by opId. Fetched once
    // on first expand and cached for the dialog's lifetime. Reactive Map/Set (Svelte
    // 5 tracks their mutations) so a `.get(id)` is honestly `ItemsState | undefined`.
    const expanded = new SvelteSet<string>()
    const itemsByOp = new SvelteMap<string, ItemsState>()

    function handleClose() {
        closeOperationLog()
    }

    async function toggleOperation(op: OperationRow) {
        const id = op.opId
        const willOpen = !expanded.has(id)
        if (willOpen) expanded.add(id)
        else expanded.delete(id)
        if (!willOpen || itemsByOp.has(id)) return

        itemsByOp.set(id, { loading: true, error: false, items: [], total: 0 })
        try {
            const detail = await getOperationLogDetail(id, ITEM_PAGE, 0)
            itemsByOp.set(id, {
                loading: false,
                error: false,
                items: detail?.items ?? [],
                total: detail?.totalItems ?? 0,
            })
        } catch (e) {
            itemsByOp.set(id, { loading: false, error: true, items: [], total: 0 })
            log.warn("Couldn't load the operation's items: {error}", { error: String(e) })
        }
    }
</script>

<ModalDialog
    titleId="operation-log-title"
    dialogId="operation-log"
    role="dialog"
    onclose={handleClose}
    ariaDescribedby="operation-log-body"
    containerStyle="width: 620px; max-width: calc(100vw - 2 * var(--spacing-xl))"
>
    {#snippet title()}
        <span class="title-row">
            {tString('operationLog.dialog.title')}
            {#if badge}<StatusBadge status={badge} />{/if}
        </span>
    {/snippet}

    <div class="body" id="operation-log-body">
        <div class="scroll-area">
            {#if operationLogState.loading}
                <div class="centered"><Spinner size="md" label={tString('operationLog.dialog.loading')} /></div>
            {:else if operationLogState.loadError}
                <p class="notice">{tString('operationLog.dialog.loadError')}</p>
            {:else if operationLogState.entries.length === 0}
                <p class="notice">{tString('operationLog.dialog.empty')}</p>
            {:else}
                <ul class="op-list">
                    {#each operationLogState.entries as op (op.opId)}
                        {@const isOpen = expanded.has(op.opId)}
                        {@const items = itemsByOp.get(op.opId)}
                        <li class="op">
                            <button
                                type="button"
                                class="op-head"
                                aria-expanded={isOpen}
                                aria-controls="op-items-{op.opId}"
                                onclick={() => void toggleOperation(op)}
                            >
                                <Icon name={isOpen ? 'chevron-down' : 'chevron-right'} size={16} />
                                <span class="op-summary"
                                    >{operationSummary(op.kind, op.archiveSubkind, op.itemCount)}</span
                                >
                                <span class="op-meta">
                                    <span>{initiatorLabel(op.initiator)}</span>
                                    <span aria-hidden="true">·</span>
                                    <span>{formatDateTime(op.endedAt ?? op.startedAt)}</span>
                                </span>
                                <span class="op-badges">
                                    <span class="op-badge">{executionStatusLabel(op.executionStatus)}</span>
                                    <span class="op-badge op-badge-rollback"
                                        >{rollbackStateLabel(op.rollbackState)}</span
                                    >
                                </span>
                            </button>

                            {#if isOpen}
                                <div class="op-items" id="op-items-{op.opId}">
                                    {#if items?.loading}
                                        <div class="centered-sm">
                                            <Spinner size="sm" label={tString('operationLog.dialog.loading')} />
                                        </div>
                                    {:else if items?.error}
                                        <p class="notice-sm">{tString('operationLog.dialog.itemsError')}</p>
                                    {:else if items && items.items.length === 0}
                                        <p class="notice-sm">{tString('operationLog.dialog.noItems')}</p>
                                    {:else if items}
                                        <ul class="item-list">
                                            {#each items.items as item (item.seq)}
                                                <li class="item">
                                                    <span class="item-path" title={item.sourcePath}
                                                        >{item.sourcePath}</span
                                                    >
                                                    {#if item.destPath != null}
                                                        <Icon name="chevron-right" size={12} />
                                                        <span class="item-path" title={item.destPath}
                                                            >{item.destPath}</span
                                                        >
                                                    {/if}
                                                    <span class="item-outcome">{itemOutcomeLabel(item.outcome)}</span>
                                                </li>
                                            {/each}
                                        </ul>
                                        {#if items.total > items.items.length}
                                            <p class="more-items">
                                                {tString('operationLog.dialog.moreItems', {
                                                    count: items.total - items.items.length,
                                                    countText: formatInteger(items.total - items.items.length),
                                                })}
                                            </p>
                                        {/if}
                                    {/if}
                                </div>
                            {/if}
                        </li>
                    {/each}
                </ul>

                {#if operationLogState.hasMore}
                    <div class="load-more">
                        <Button
                            variant="secondary"
                            disabled={operationLogState.loadingMore}
                            onclick={() => void loadMoreOperations()}
                        >
                            {tString('operationLog.dialog.loadMore')}
                        </Button>
                    </div>
                {/if}
            {/if}
        </div>

        <div class="footer">
            <Button variant="primary" onclick={handleClose}>{tString('operationLog.dialog.close')}</Button>
        </div>
    </div>
</ModalDialog>

<style>
    .title-row {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-sm);
    }

    .body {
        display: flex;
        flex-direction: column;
        padding: 0 var(--spacing-xl) var(--spacing-xl);
        max-height: calc(100vh - 2 * var(--spacing-2xl) - var(--titlebar-height));
        min-height: 0;
    }

    .scroll-area {
        overflow-y: auto;
        min-height: 0;
        padding-right: var(--spacing-xs);
    }

    .centered {
        display: flex;
        justify-content: center;
        padding: var(--spacing-2xl) 0;
    }

    .centered-sm {
        display: flex;
        justify-content: center;
        padding: var(--spacing-sm) 0;
    }

    .notice {
        margin: var(--spacing-md) 0;
        font-size: var(--font-size-md);
        color: var(--color-text-secondary);
        line-height: 1.5;
    }

    .notice-sm {
        margin: var(--spacing-xs) 0;
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }

    .op-list,
    .item-list {
        list-style: none;
        margin: 0;
        padding: 0;
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
    }

    .op {
        background: var(--color-bg-secondary);
        border-radius: var(--radius-md);
    }

    .op-head {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
        width: 100%;
        padding: var(--spacing-sm) var(--spacing-md);
        background: transparent;
        border: none;
        border-radius: var(--radius-md);
        text-align: left;
        color: var(--color-text-primary);
        font-size: var(--font-size-sm);
    }

    .op-head:hover {
        background: var(--color-bg-tertiary);
    }

    .op-head:focus-visible {
        outline: 2px solid var(--color-accent);
        outline-offset: -2px;
    }

    .op-summary {
        font-weight: 600;
    }

    .op-meta {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xs);
        color: var(--color-text-tertiary);
        font-size: var(--font-size-xs);
    }

    .op-badges {
        margin-left: auto;
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xs);
        flex-shrink: 0;
    }

    .op-badge {
        font-size: var(--font-size-xs);
        padding: 1px var(--spacing-xs);
        border-radius: var(--radius-sm);
        background: var(--color-bg-tertiary);
        color: var(--color-text-secondary);
        white-space: nowrap;
    }

    .op-badge-rollback {
        background: var(--color-accent-subtle);
        color: var(--color-text-primary);
    }

    .op-items {
        padding: 0 var(--spacing-md) var(--spacing-sm) var(--spacing-2xl);
    }

    .item {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
        font-size: var(--font-size-xs);
        color: var(--color-text-secondary);
        padding: var(--spacing-xxs) 0;
    }

    .item-path {
        font-family: var(--font-mono);
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        max-width: 40%;
    }

    .item-outcome {
        margin-left: auto;
        color: var(--color-text-tertiary);
        flex-shrink: 0;
    }

    .more-items {
        margin: var(--spacing-xs) 0 0;
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
    }

    .load-more {
        display: flex;
        justify-content: center;
        margin-top: var(--spacing-md);
    }

    .footer {
        display: flex;
        align-items: center;
        justify-content: flex-end;
        gap: var(--spacing-md);
        margin-top: var(--spacing-lg);
        padding-top: var(--spacing-md);
        border-top: 1px solid var(--color-border);
    }
</style>
