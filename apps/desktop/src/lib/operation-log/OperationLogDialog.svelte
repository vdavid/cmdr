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
    import LinkButton from '$lib/ui/LinkButton.svelte'
    import { tooltip } from '$lib/tooltip/tooltip'
    import { getBadgeStatus } from '$lib/feature-status'
    import { tString } from '$lib/intl/messages.svelte'
    import Trans from '$lib/intl/Trans.svelte'
    import { formatInteger } from '$lib/intl/number-format'
    import { formatDateTime } from '$lib/settings/reactive-settings.svelte'
    import RollbackConfirmDialog from '$lib/file-operations/RollbackConfirmDialog.svelte'
    import RollbackControls from './RollbackControls.svelte'
    import type { MessageKey } from '$lib/intl/keys.gen'
    import {
        getOperationLogDetail,
        rollbackOperation,
        type OperationItemView,
        type OperationRow,
    } from '$lib/tauri-commands'
    import { getAppLogger } from '$lib/logging/logger'
    import { SvelteMap, SvelteSet } from 'svelte/reactivity'
    import type { Snippet } from 'svelte'
    import { asRollbackRefusal } from './rollback-refusal'
    import {
        operationLogState,
        closeOperationLog,
        loadMoreOperations,
        markOperationRollingBack,
        refreshOperation,
    } from './operation-log-trigger.svelte'
    import {
        operationSummary,
        initiatorLabel,
        executionStatusLabel,
        rollbackStateLabel,
        itemOutcomeLabel,
        rollbackRefusalNotice,
        rowRollbackAction,
        rowRollbackActionLabel,
        rowStandingNotice,
    } from './operation-log-labels'
    import { rollbackConfirmVariant } from '$lib/file-operations/reversal-wording'

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

    // Per-operation expansion + lazily fetched items, keyed by opId. Fetched on
    // first expand and cached for the dialog's lifetime, except a read that threw,
    // which the next expand tries again. Reactive Map/Set (Svelte
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
     * The row whose question is up, resolved fresh from the list each time, paired
     * with the action its button offered. A row that stops offering one while the
     * question is open (a reversal started elsewhere) takes the question down with
     * it, the way the queue row does: there's nothing left for an answer to act on.
     */
    const rollbackAsked = $derived.by(() => {
        if (rollbackAskedId === null) return null
        const op = operationLogState.entries.find((entry) => entry.opId === rollbackAskedId)
        if (op === undefined) return null
        const action = rowRollbackAction(op.rollbackState)
        return action === null ? null : { op, action }
    })

    /** Every listed row by id, so a rollback row can find the operation it undid
     *  (and the other way round) without a scan per row. */
    const entriesById = $derived(new Map(operationLogState.entries.map((entry) => [entry.opId, entry])))

    /** How long a row we jumped to stays tinted, so the eye can find it. */
    const HIGHLIGHT_MS = 2000
    let highlightedId = $state<string | null>(null)
    let highlightTimer: ReturnType<typeof setTimeout> | undefined

    $effect(() => () => { clearTimeout(highlightTimer); })

    /**
     * Take the user to a linked row: scroll it into view, move focus to its head so a
     * keyboard or screen-reader user lands where a sighted one looks, and tint it for a
     * moment.
     */
    function goToOperation(opId: string) {
        const head = document.getElementById(`op-head-${opId}`)
        if (head === null) return
        head.scrollIntoView({ block: 'nearest' })
        head.focus({ preventScroll: true })
        highlightedId = opId
        clearTimeout(highlightTimer)
        highlightTimer = setTimeout(() => (highlightedId = null), HIGHLIGHT_MS)
    }

    /** The row's reversal has ended: re-read it, and its rollback's own row when
     *  that's listed too, since its status drifted the same way. */
    function handleReversalEnded(op: OperationRow) {
        void refreshOperation(op.opId)
        if (op.inverseOpId !== null) void refreshOperation(op.inverseOpId)
    }

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
            const dispatch = await rollbackOperation(opId)
            markOperationRollingBack(opId, dispatch.inverseOpId)
        } catch (e) {
            const refusal = asRollbackRefusal(e)
            refusals.set(opId, rollbackRefusalNotice(refusal))
            // A typed refusal is an answer the dialog already words; only an untyped
            // throw is a diagnostic worth a warn.
            if (refusal) log.info("Couldn't roll {opId} back: {reason}", { opId, reason: refusal.kind })
            else log.warn("Couldn't roll {opId} back: {error}", { opId, error: String(e) })
        } finally {
            dispatching.delete(opId)
        }
    }

    async function toggleOperation(op: OperationRow) {
        const id = op.opId
        const willOpen = !expanded.has(id)
        if (willOpen) expanded.add(id)
        else expanded.delete(id)
        const cached = itemsByOp.get(id)
        // A failed read isn't kept: expanding the row again is the retry.
        if (!willOpen || (cached !== undefined && !cached.error)) return

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
                        <!-- One expression drives both the line and the `aria-describedby` that
                             points at it, so the two can't drift into an orphaned reference. A
                             refusal the user just earned outranks the standing explanation. -->
                        {@const reasonNotice =
                            refusal == null
                                ? rowStandingNotice(op.rollbackState, op.notRollbackableReason)
                                : null}
                        {@const action = rowRollbackAction(op.rollbackState)}
                        <!-- The journal's two links between a rollback and what it undid.
                             A link only goes to a row that's listed; an original that isn't
                             (an older page, or pruned) is named without one, and a pointer to
                             a rollback that isn't listed yet (one this dialog just started)
                             stays quiet until the next read. -->
                        {@const undid = op.rollsBackOpId !== null ? entriesById.get(op.rollsBackOpId) : undefined}
                        {@const undoneBy = op.inverseOpId !== null ? entriesById.get(op.inverseOpId) : undefined}
                        <li class="op" class:op-highlighted={highlightedId === op.opId}>
                            <div class="op-row">
                                <button
                                    type="button"
                                    class="op-head"
                                    id="op-head-{op.opId}"
                                    aria-expanded={isOpen}
                                    aria-controls="op-items-{op.opId}"
                                    aria-describedby={reasonNotice != null ? `op-reason-${op.opId}` : undefined}
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

                                <!-- Only on a row the backend's own gate would let through, and
                                     worded by what the press does: "Roll back" starts one,
                                     "Finish rolling back" picks a stopped one up.
                                     `aria-describedby` tells a screen reader WHICH row this
                                     button belongs to, and on a row that carries a standing
                                     explanation, why it says what it says. -->
                                {#if action !== null}
                                    <Button
                                        size="mini"
                                        disabled={dispatching.has(op.opId)}
                                        aria-describedby={reasonNotice != null
                                            ? `op-head-${op.opId} op-reason-${op.opId}`
                                            : `op-head-${op.opId}`}
                                        onclick={() => { askRollback(op.opId); }}
                                    >
                                        {rowRollbackActionLabel(action)}
                                    </Button>
                                {/if}

                                <!-- The row's reversal is a live operation of its own, so it can be
                                     parked and stopped from here. It commands that operation, never
                                     this row's; `RollbackControls.svelte` holds the reasoning. The
                                     id is journal truth on every read, so a reversal this dialog
                                     didn't start gets the same buttons. -->
                                {#if op.rollbackState === 'rollingBack' && op.inverseOpId !== null}
                                    <RollbackControls
                                        inverseOpId={op.inverseOpId}
                                        describedBy="op-head-{op.opId}"
                                        onEnded={() => { handleReversalEnded(op); }}
                                    />
                                {/if}
                            </div>

                            {#if op.rollsBackOpId !== null}
                                <p class="op-relation" id="op-relation-{op.opId}">
                                    {#if undid}
                                        {#snippet undidLink(children: Snippet)}
                                            <LinkButton onclick={() => { goToOperation(undid.opId); }}
                                                >{@render children()}</LinkButton
                                            >
                                        {/snippet}
                                        <Trans
                                            key="operationLog.dialog.rollbackOf"
                                            snippets={{ operationLink: undidLink }}
                                            params={{
                                                operation: operationSummary(
                                                    undid.kind,
                                                    undid.archiveSubkind,
                                                    undid.itemCount,
                                                ),
                                                time: formatDateTime(undid.endedAt ?? undid.startedAt),
                                            }}
                                        />
                                    {:else}
                                        {tString('operationLog.dialog.rollbackOfUnlisted')}
                                    {/if}
                                </p>
                            {:else if undoneBy}
                                <p class="op-relation" id="op-relation-{op.opId}">
                                    {#snippet undoneByLink(children: Snippet)}
                                        <LinkButton onclick={() => { goToOperation(undoneBy.opId); }}
                                            >{@render children()}</LinkButton
                                        >
                                    {/snippet}
                                    <Trans
                                        key="operationLog.dialog.latestRollback"
                                        snippets={{ operationLink: undoneByLink }}
                                        params={{
                                            operation: operationSummary(
                                                undoneBy.kind,
                                                undoneBy.archiveSubkind,
                                                undoneBy.itemCount,
                                            ),
                                            time: formatDateTime(undoneBy.endedAt ?? undoneBy.startedAt),
                                        }}
                                    />
                                </p>
                            {/if}

                            {#if refusal != null}
                                <p class="op-refusal" role="status">{tString(refusal)}</p>
                            {:else if reasonNotice != null}
                                <!-- The badge names the state; this says what it means for the
                                     user's files. A `notRollbackable` row can't earn the sentence
                                     from a refusal (it offers no button), and a partly-reversed one
                                     would otherwise leave someone who cancelled a reversal guessing.
                                     Which states speak, and when they stay quiet:
                                     `rowStandingNotice`. -->
                                <p class="op-reason" id="op-reason-{op.opId}">{tString(reasonNotice)}</p>
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
        variant={rollbackConfirmVariant(rollbackAsked.op.kind)}
        finishing={rollbackAsked.action === 'finish'}
        onConfirm={() => void confirmRollback(rollbackAsked.op.opId)}
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

    /* A row a relation link jumped to. A plain tint, no animation, so it needs no
       reduced-motion branch; focus on its head carries the same news for a keyboard. */
    .op-highlighted {
        background: var(--color-accent-subtle);
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

    .op-relation,
    .op-refusal,
    .op-reason {
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
