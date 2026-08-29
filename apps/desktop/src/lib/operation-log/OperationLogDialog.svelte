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
    import { tooltip } from '$lib/tooltip/tooltip'
    import { getBadgeStatus } from '$lib/feature-status'
    import { tString } from '$lib/intl/messages.svelte'
    import { formatInteger } from '$lib/intl/number-format'
    import { formatDateTime } from '$lib/settings/reactive-settings.svelte'
    import RollbackConfirmDialog from '$lib/file-operations/RollbackConfirmDialog.svelte'
    import type { MessageKey } from '$lib/intl/keys.gen'
    import {
        getOperationLogDetail,
        rollbackOperation,
        type OperationItemView,
        type OperationRow,
    } from '$lib/tauri-commands'
    import { getAppLogger } from '$lib/logging/logger'
    import { SvelteMap, SvelteSet } from 'svelte/reactivity'
    import { asRollbackRefusal } from './rollback-refusal'
    import {
        operationLogState,
        closeOperationLog,
        loadMoreOperations,
        markOperationRollingBack,
    } from './operation-log-trigger.svelte'
    import {
        operationSummary,
        initiatorLabel,
        executionStatusLabel,
        rollbackStateLabel,
        itemOutcomeLabel,
        rollbackConfirmVariant,
        rollbackRefusalNotice,
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

    // Which row is asking its rollback question, which rows have a dispatch in
    // flight, and the last refusal each row earned. Keyed by opId so the list can
    // reorder or grow under them.
    let rollbackAskedId = $state<string | null>(null)
    const dispatching = new SvelteSet<string>()
    const refusals = new SvelteMap<string, MessageKey>()

    /**
     * The row whose question is up, resolved fresh from the list each time. A row
     * that stops being rollbackable while the question is open (a reversal started
     * elsewhere) takes the question down with it, the way the queue row does: there's
     * nothing left for an answer to act on.
     */
    const rollbackAsked = $derived.by(() => {
        if (rollbackAskedId === null) return null
        const op = operationLogState.entries.find((entry) => entry.opId === rollbackAskedId)
        return op?.rollbackState === 'rollbackable' ? op : null
    })

    function handleClose() {
        closeOperationLog()
    }

    function askRollback(opId: string) {
        refusals.delete(opId)
        rollbackAskedId = opId
    }

    /**
     * Hand the reversal to the operation queue and let go of it. There's no progress
     * dialog here on purpose: the user is reading their history, not watching a
     * transfer, and the status corner already surfaces what's running.
     */
    async function confirmRollback(opId: string) {
        rollbackAskedId = null
        if (dispatching.has(opId)) return
        dispatching.add(opId)
        try {
            await rollbackOperation(opId)
            markOperationRollingBack(opId)
        } catch (e) {
            const refusal = asRollbackRefusal(e)
            refusals.set(opId, rollbackRefusalNotice(refusal))
            log.warn("Couldn't roll {opId} back: {reason}", { opId, reason: refusal?.kind ?? String(e) })
        } finally {
            dispatching.delete(opId)
        }
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
    fillBody
    resizable
>
    <!-- The title bar's `<h2>` is already the row (gap + badge alignment live there),
         so the words and the badge are its direct children. -->
    {#snippet title()}
        <span>{tString('operationLog.dialog.title')}</span>{#if badge}<StatusBadge status={badge} />{/if}
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
                        {@const refusal = refusals.get(op.opId)}
                        <li class="op">
                            <div class="op-row">
                                <button
                                    type="button"
                                    class="op-head"
                                    id="op-head-{op.opId}"
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

                                <!-- Only on a row the journal says can be reversed. The name stays
                                     "Roll back" for every row; `aria-describedby` is what tells a
                                     screen reader WHICH row this one belongs to. -->
                                {#if op.rollbackState === 'rollbackable'}
                                    <Button
                                        size="mini"
                                        disabled={dispatching.has(op.opId)}
                                        aria-describedby="op-head-{op.opId}"
                                        onclick={() => { askRollback(op.opId); }}
                                    >
                                        {tString('operationLog.dialog.rollBack')}
                                    </Button>
                                {/if}
                            </div>

                            {#if refusal != null}
                                <p class="op-refusal" role="status">{tString(refusal)}</p>
                            {/if}

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
                                                    <span
                                                        class="item-path"
                                                        use:tooltip={{ text: item.sourcePath, overflowOnly: true }}
                                                        >{item.sourcePath}</span
                                                    >
                                                    {#if item.destPath != null}
                                                        <Icon name="chevron-right" size={12} />
                                                        <span
                                                            class="item-path"
                                                            use:tooltip={{ text: item.destPath, overflowOnly: true }}
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

<!-- Stacked over the log: same subtree, so DOM order puts it on top and its focus
     trap takes over until it goes (`$lib/ui/DETAILS.md` § ModalDialog). -->
{#if rollbackAsked !== null}
    <RollbackConfirmDialog
        variant={rollbackConfirmVariant(rollbackAsked.kind)}
        onConfirm={() => void confirmRollback(rollbackAsked.opId)}
        onCancel={() => (rollbackAskedId = null)}
    />
{/if}

<style>
    /* Fills `fillBody`'s slot so an edge drag lands in the list, not in dead space
       above the Close button. The panel's own max-height does the capping. */
    .body {
        display: flex;
        flex-direction: column;
        flex: 1 1 auto;
        min-height: 0;
    }

    .scroll-area {
        flex: 1 1 auto;
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

    /* The head button and the row's action sit side by side: a button can't nest in
       a button, and the head has to stay the expand target on its own. */
    .op-row {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
        padding-right: var(--spacing-md);
    }

    .op-head {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
        flex: 1 1 auto;
        min-width: 0;
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

    .op-refusal {
        margin: 0;
        padding: 0 var(--spacing-md) var(--spacing-sm) var(--spacing-2xl);
        font-size: var(--font-size-xs);
        color: var(--color-text-secondary);
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
