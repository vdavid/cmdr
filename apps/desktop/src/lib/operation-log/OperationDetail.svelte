<script lang="ts">
    /**
     * The Operation log's right pane: the selected operation's header (summary, who and
     * when, its execution and rollback badges, the route it took), the one sentence it
     * carries on sight, and its item tree.
     *
     * The header renders from the list's row at once; the tree catches up once the
     * cursor settles (`operation-tree-source.svelte.ts`). The route and the partial
     * notice's counts come from the tree's answer, so they appear with it.
     */
    import { tString } from '#lib/intl/messages.svelte.js'
    import type { MessageKey } from '#lib/intl/keys.gen.js'
    import { formatDateTime } from '#lib/settings/reactive-settings.svelte.js'
    import type { OperationRow } from '#lib/tauri-commands/index.js'
    import {
        coverageNotice,
        executionStatusLabel,
        initiatorLabel,
        operationSummary,
        partiallyRolledBackSummary,
        rollbackStateLabel,
        rowStandingNotice,
    } from './operation-log-labels'
    import { operationRoute } from './operation-tree-labels'
    import type { OperationTreeSource } from './operation-tree-source.svelte'
    import OperationTree from './OperationTree.svelte'

    interface Props {
        op: OperationRow
        source: OperationTreeSource
        /** The refusal the last press on this operation earned; it outranks the standing notice. */
        refusal: MessageKey | null
        /** ← on a top-level tree row. */
        onLeaveTree?: () => void
    }

    const { op, source, refusal, onLeaveTree }: Props = $props()

    /** The tree's answer, once it's this operation's. */
    const view = $derived(source.opId === op.opId && source.blank === null ? source.view : null)
    const route = $derived(view === null ? null : operationRoute(op.kind, view.rootPath, view.otherRoot))
    /**
     * The sentence this operation carries on sight. A partly rolled-back one says it with
     * the tree's counts when it has them; every other state, and that one before the tree
     * answers, takes `rowStandingNotice`'s.
     */
    const notice = $derived.by(() => {
        if (op.rollbackState === 'partiallyRolledBack' && view !== null) {
            const counted = partiallyRolledBackSummary(view.tally)
            if (counted !== null) return counted
        }
        const key = rowStandingNotice(op.rollbackState, op.notRollbackableReason, op.kind)
        return key === null ? null : tString(key)
    })
    /** What the journal may be missing of this operation (a crash cut it off), beside the notice. */
    const coverage = $derived(coverageNotice(op.searchCoverageReason))
</script>

<section class="detail" aria-labelledby="op-head-{op.opId}">
    <header class="detail-head">
        <div class="head-line">
            <h3 class="op-title" id="op-head-{op.opId}">{operationSummary(op)}</h3>
            <span class="op-meta">
                <span>{initiatorLabel(op.initiator)}</span>
                <span aria-hidden="true">·</span>
                <span>{formatDateTime(op.endedAt ?? op.startedAt)}</span>
            </span>
            <span class="op-badges">
                <span class="op-badge">{executionStatusLabel(op.executionStatus)}</span>
                <span class="op-badge op-badge-rollback" data-state={op.rollbackState}
                    >{rollbackStateLabel(op.rollbackState)}</span
                >
            </span>
        </div>
        {#if route !== null}<p class="op-route">{route}</p>{/if}
        {#if refusal !== null}
            <p class="op-refusal" role="status">{tString(refusal)}</p>
        {:else if notice !== null}
            <!-- The badge names the state; this says what it means for the person's files.
                 Which states speak, and when they stay quiet: `rowStandingNotice`. -->
            <p class="op-reason" id="op-reason-{op.opId}">{notice}</p>
        {/if}
        {#if coverage !== null}<p class="op-coverage">{tString(coverage)}</p>{/if}
    </header>
    <OperationTree {source} kind={op.kind} onLeave={onLeaveTree} />
</section>

<style>
    .detail {
        display: flex;
        flex-direction: column;
        flex: 1 1 auto;
        min-width: 0;
        min-height: 0;
    }

    .detail-head {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
        padding: var(--spacing-sm) var(--spacing-md) var(--spacing-md);
        border-bottom: 1px solid var(--color-border-subtle);
    }

    .head-line {
        display: flex;
        flex-wrap: wrap;
        align-items: baseline;
        gap: var(--spacing-xs) var(--spacing-sm);
    }

    .op-title {
        margin: 0;
        font-size: var(--font-size-lg);
        font-weight: 600;
    }

    /* Wraps between its parts (who, the dot, when), never inside one. */
    .op-meta {
        display: inline-flex;
        flex-wrap: wrap;
        align-items: center;
        gap: var(--spacing-xs);
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
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

    .op-badge-rollback[data-state='partiallyRolledBack'],
    .op-badge-rollback[data-state='rollingBack'] {
        background: var(--color-warning-bg);
    }

    .op-badge-rollback[data-state='notRollbackable'],
    .op-badge-rollback[data-state='rolledBack'] {
        background: var(--color-bg-tertiary);
    }

    .op-route,
    .op-coverage,
    .op-reason,
    .op-refusal {
        margin: 0;
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }

    .op-reason,
    .op-refusal {
        color: var(--color-text-primary);
    }
</style>
