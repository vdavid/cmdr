<script lang="ts">
    /**
     * The alpha "Operation log" dialog: master–detail. The left list holds recent file
     * operations, newest first; its cursor picks the one the right pane shows: a header,
     * the sentence it carries on sight, and its item tree with a checkbox on everything a
     * rollback can still take. The action bar rolls back what's checked.
     *
     * Every label comes from a typed enum via `operation-log-labels` /
     * `operation-tree-labels` (never a backend-rendered string). The tree is the
     * backend's; `operation-tree-source.svelte.ts` holds only this view's toggles. Layout,
     * the tree source, and the selection flow: `DETAILS.md`.
     */
    import ModalDialog from '#lib/ui/ModalDialog.svelte'
    import Button from '#lib/ui/Button.svelte'
    import Spinner from '#lib/ui/Spinner.svelte'
    import StatusBadge from '#lib/ui/StatusBadge.svelte'
    import { getBadgeStatus } from '#lib/feature-status.js'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { getOperationLogRecording } from '#lib/settings/reactive-settings.svelte.js'
    import RollbackConfirmDialog from '#lib/file-operations/RollbackConfirmDialog.svelte'
    import type { RollbackAnswer } from '#lib/file-operations/rollback-confirmers.js'
    import { rollbackConfirmVariant } from '#lib/file-operations/reversal-wording.js'
    import type { MessageKey } from '#lib/intl/keys.gen.js'
    import type { RollbackSelection } from '#lib/ipc/bindings.js'
    import { rollbackOperation, type OperationRow } from '#lib/tauri-commands/index.js'
    import { getAppLogger } from '#lib/logging/logger.js'
    import { SvelteMap, SvelteSet } from 'svelte/reactivity'
    import { onDestroy, onMount } from 'svelte'
    import { claimMenuCommand } from '#lib/commands/menu-claims.js'
    import { claimKey } from '#lib/shortcuts/claim-key.js'
    import { eventMatchesCommand } from '#lib/shortcuts/index.js'
    import { useInlineSize } from '#lib/utils/inline-size-action.js'
    import RollbackControls from './RollbackControls.svelte'
    import OperationList from './OperationList.svelte'
    import OperationDetail from './OperationDetail.svelte'
    import { asRollbackRefusal } from './rollback-refusal'
    import { createOperationTreeSource } from './operation-tree-source.svelte'
    import {
        operationLogState,
        closeOperationLog,
        loadMoreOperations,
        markOperationRollingBack,
        refreshOperation,
    } from './operation-log-trigger.svelte'
    import {
        rollbackAction,
        rollbackActionLabel,
        rollbackRefusalNotice,
        rowStandingNotice,
        selectedCountText,
        type RollbackAction,
    } from './operation-log-labels'

    const log = getAppLogger('operationLogDialog')

    // Badge policy: the status comes from the repo-root feature-status.json.
    const badge = getBadgeStatus('operation-log')

    const tree = createOperationTreeSource()
    onDestroy(() => {
        tree.dispose()
    })

    /** The operation under the list's cursor, by id, so the list can grow and refresh under it. */
    let cursorOpId = $state<string | null>(null)
    const entries = $derived(operationLogState.entries)
    const cursorIndex = $derived(Math.max(0, entries.findIndex((entry) => entry.opId === cursorOpId)))
    const op = $derived<OperationRow | undefined>(entries[cursorIndex])

    function pickOperation(index: number): void {
        cursorOpId = entries.at(index)?.opId ?? null
    }

    // The tree follows the cursor; it reads once the cursor has rested (`SETTLE_MS`).
    $effect(() => {
        tree.show(op ?? null)
    })

    // Which operations have a dispatch in flight, and the last refusal each earned. Keyed
    // by opId so the list can reorder or grow under them.
    const dispatching = new SvelteSet<string>()
    const refusals = new SvelteMap<string, MessageKey>()

    /** The tree source holds this operation (its picks are this operation's, rows showing or not). */
    const treeOwned = $derived(op !== undefined && tree.opId === op.opId)
    /** The tree's answer counts only once it's this operation's and its rows are showing. */
    const treeIsOp = $derived(treeOwned && tree.blank === null && tree.view !== null)
    const tally = $derived(treeIsOp && tree.view !== null && !tree.view.readOnly ? tree.view.tally : null)
    /** Decoupled from `blank` on purpose: a tree that turns unreadable keeps the person's picks. */
    const narrowed = $derived(treeOwned && tree.narrowed)
    /**
     * The action bar's press for this operation, or `null` on a state the backend gate refuses.
     * Narrowed but with no rows showing (the tree turned unreadable, say), the press is
     * disabled: the picks can't be counted or confirmed, and ❌ falling back to the
     * whole-op press would reverse what the person left out.
     */
    const bar = $derived.by(() => {
        if (op === undefined) return null
        if (!narrowed || tally !== null) return rollbackAction(op.rollbackState, tally, narrowed)
        const lastTally = tree.view !== null && !tree.view.readOnly ? tree.view.tally : null
        const stale = rollbackAction(op.rollbackState, lastTally, true)
        return stale === null ? null : { ...stale, disabled: true }
    })
    /**
     * The detail pane shows its standing sentence (`#op-reason-…`), so the button can point
     * at it. Same rule `OperationDetail` renders by: a refusal takes the slot, unlabelled.
     */
    const reasonShown = $derived(
        op !== undefined &&
            !refusals.has(op.opId) &&
            rowStandingNotice(op.rollbackState, op.notRollbackableReason, op.kind) !== null,
    )

    /**
     * The question that's up: which operation, what the press does, and the selection it
     * sends, captured when asked. Resolved fresh from the list on every read, so an
     * operation that stops offering a rollback while the question is open (a reversal
     * started elsewhere) takes the question down with it, the way the queue row does.
     */
    let asked = $state<{ opId: string; action: RollbackAction; selection: RollbackSelection | null } | null>(null)
    const rollbackAsked = $derived.by(() => {
        if (asked === null) return null
        const question = asked
        const row = entries.find((entry) => entry.opId === question.opId)
        if (row === undefined || rollbackAction(row.rollbackState, null, false) === null) return null
        return { ...question, op: row }
    })

    function askRollback(): void {
        if (op === undefined || bar === null || bar.disabled) return
        refusals.delete(op.opId)
        // Untouched, every check is on: no selection says "everything" and works on an
        // operation whose tree can't be read too.
        asked = { opId: op.opId, action: bar.action, selection: narrowed ? tree.selection : null }
    }

    /**
     * Hand the reversal to the operation queue and let go of it. There's no progress
     * dialog here on purpose: the person is reading their history, not watching a
     * transfer, and the status corner already surfaces what's running.
     */
    async function confirmRollback(opId: string, selection: RollbackSelection | null): Promise<RollbackAnswer> {
        asked = null
        if (dispatching.has(opId)) return { status: 'refused', refusal: { kind: 'alreadyRollingBack' } }
        dispatching.add(opId)
        try {
            const dispatch = await rollbackOperation(opId, 'user', selection)
            markOperationRollingBack(opId, dispatch.inverseOpId)
            // The tree goes read-only while the reversal runs: the backend now says `rolling_back`.
            if (tree.opId === opId) tree.refresh()
            return { status: 'dispatched', inverseOpId: dispatch.inverseOpId }
        } catch (e) {
            const refusal = asRollbackRefusal(e)
            refusals.set(opId, rollbackRefusalNotice(refusal))
            // A typed refusal is an answer the dialog already words; only an untyped
            // throw is a diagnostic worth a warn.
            if (refusal) log.info("Couldn't roll {opId} back: {reason}", { opId, reason: refusal.kind })
            else log.warn("Couldn't roll {opId} back: {error}", { opId, error: e })
            return { status: 'refused', refusal: refusal ?? { kind: 'unknownOperation' } }
        } finally {
            dispatching.delete(opId)
        }
    }

    /**
     * The operation's reversal has ended: re-read its header, and its tree when it's the
     * one shown, since outcomes changed. The picks go with it: they described a state
     * that's gone, and "Roll back the rest" is the natural next press.
     */
    function handleReversalEnded(ended: OperationRow): void {
        void refreshOperation(ended.opId)
        if (tree.opId === ended.opId) tree.refresh({ resetSelection: true })
    }

    /** Narrower than this, the list stacks above the detail: side by side, the tree would be too thin to read. */
    const STACK_BELOW_PX = 640
    let splitWidth = $state(0)

    function focusOperationList(): void {
        document.querySelector<HTMLElement>('#operation-log-body .op-list [role="grid"]')?.focus()
    }

    /**
     * → or Enter in the list. A tree that has no rows yet (the cursor is settling, or it's
     * loading) remembers the ask for its operation and takes focus once its rows arrive;
     * the ask goes if the list cursor moves on first.
     */
    let treeFocusFor = $state<string | null>(null)

    function focusTree(): void {
        const grid = treeGrid()
        if (grid !== null && tree.opId === op?.opId && tree.blank === null) {
            treeFocusFor = null
            grid.focus()
            return
        }
        treeFocusFor = op?.opId ?? null
    }

    // Runs after the DOM catches up, so the rows' grid exists by the time it reads.
    $effect(() => {
        const wanted = treeFocusFor
        if (wanted === null) return
        if (op?.opId !== wanted) {
            treeFocusFor = null
            return
        }
        if (tree.opId !== wanted || tree.blank !== null) return
        treeFocusFor = null
        treeGrid()?.focus()
    })

    function treeGrid(): HTMLElement | null {
        return document.querySelector<HTMLElement>('#operation-log-body [role="treegrid"]')
    }

    /**
     * ⌘A and ⌘⇧A are Edit > Select all / Deselect all's accelerators, which fire before the
     * page sees the key, and the dialog gate refuses both over a dialog. So the dialog claims
     * those two menu commands, and takes them only while its tree holds focus; anywhere else
     * the fire passes on and the gate decides, as for any dialog. The tree's keydown answers
     * the same keys (`operationLog.selectAll` / `deselectAll`); both halves of a double fire
     * land as one pick, since `setAll` is idempotent.
     */
    function claimForTree(on: boolean): () => boolean {
        return () => {
            const grid = treeGrid()
            if (grid === null || document.activeElement !== grid) return false
            tree.setAll(on)
            return true
        }
    }

    onMount(() => {
        const releaseAll = claimMenuCommand('selection.selectAll', claimForTree(true))
        const releaseNone = claimMenuCommand('selection.deselectAll', claimForTree(false))
        return () => {
            releaseAll()
            releaseNone()
        }
    })

    /** ⌘↩ from anywhere in the dialog: the action bar's press, which asks first. */
    function handleKeydown(event: KeyboardEvent): void {
        if (event.isComposing || !eventMatchesCommand(event, 'operationLog.rollBack')) return
        claimKey(event)
        askRollback()
    }
</script>

<ModalDialog
    titleId="operation-log-title"
    dialogId="operation-log"
    role="dialog"
    onclose={closeOperationLog}
    onkeydown={handleKeydown}
    ariaDescribedby="operation-log-body"
    containerStyle="width: 900px; height: 640px; max-width: calc(100vw - 2 * var(--spacing-xl)); max-height: calc(100vh - 2 * var(--spacing-xl))"
    fillBody
    resizable
>
    <!-- The title bar's `<h2>` is already the row (gap + badge alignment live there),
         so the words and the badge are its direct children. -->
    {#snippet title()}
        <span>{tString('operationLog.dialog.title')}</span>{#if badge}<StatusBadge status={badge} />{/if}
    {/snippet}

    <div class="body" id="operation-log-body">
        <!-- Recording off: new operations never reach the journal, so the list is empty
             or stops at the point it was turned off. Say why, above whatever shows. -->
        {#if !getOperationLogRecording()}
            <p class="notice recording-off" role="status">{tString('operationLog.dialog.recordingOff')}</p>
        {/if}
        {#if operationLogState.loading}
            <!-- A row like the notices around it, so it lines up with the title (`dialog-inset.spec.ts`). -->
            <div class="notice loading" role="status">
                <Spinner size="sm" />
                <span>{tString('operationLog.dialog.loading')}</span>
            </div>
        {:else if operationLogState.loadError}
            <p class="notice">{tString('operationLog.dialog.loadError')}</p>
        {:else if entries.length === 0}
            <!-- "Copy something and it'll show up here" would be untrue while recording is off. -->
            {#if getOperationLogRecording()}
                <p class="notice">{tString('operationLog.dialog.empty')}</p>
            {/if}
        {:else}
            <div
                class="split"
                class:is-stacked={splitWidth > 0 && splitWidth < STACK_BELOW_PX}
                use:useInlineSize={{ onResize: (px) => (splitWidth = px) }}
            >
                <div class="master">
                    <OperationList
                        {entries}
                        {cursorIndex}
                        onCursorChange={pickOperation}
                        hasMore={operationLogState.hasMore}
                        loadingMore={operationLogState.loadingMore}
                        onLoadMore={() => void loadMoreOperations()}
                        onGoToItems={focusTree}
                    />
                </div>
                {#if op !== undefined}
                    <OperationDetail
                        {op}
                        source={tree}
                        refusal={refusals.get(op.opId) ?? null}
                        onLeaveTree={focusOperationList}
                    />
                {/if}
            </div>
        {/if}
    </div>

    {#snippet footerLeading()}
        {#if tally !== null && tally.selectable > 0 && bar?.action.kind === 'selected'}
            <span class="selected-count">{selectedCountText(tally)}</span>
        {/if}
    {/snippet}

    {#snippet footer()}
        <span class="op-actions">
            <!-- The operation's reversal is a live operation of its own, so it can be parked
                 and stopped from here. It commands that operation, never this one;
                 `RollbackControls.svelte` holds the reasoning. -->
            {#if op !== undefined && op.rollbackState === 'rollingBack' && op.inverseOpId !== null}
                {@const shown = op}
                <RollbackControls
                    inverseOpId={op.inverseOpId}
                    describedBy="op-head-{op.opId}"
                    onEnded={() => {
                        handleReversalEnded(shown)
                    }}
                />
            {/if}
            <Button variant={bar === null ? 'primary' : 'secondary'} onclick={closeOperationLog}
                >{tString('operationLog.dialog.close')}</Button
            >
            <!-- Only on an operation the backend's own gate would let through (`rollbackAction`),
                 worded by what the press does. `aria-describedby` names the operation it acts
                 on, and the sentence that explains it. -->
            {#if op !== undefined && bar !== null}
                <Button
                    variant="primary"
                    keyHint={{ commandId: 'operationLog.rollBack' }}
                    disabled={bar.disabled || dispatching.has(op.opId)}
                    aria-describedby={reasonShown ? `op-head-${op.opId} op-reason-${op.opId}` : `op-head-${op.opId}`}
                    onclick={askRollback}
                >
                    {rollbackActionLabel(bar.action)}
                </Button>
            {/if}
        </span>
    {/snippet}
</ModalDialog>

<!-- Stacked over the log: same subtree, so DOM order puts it on top and its focus
     trap takes over until it goes (`#lib/ui/DETAILS.md` § ModalDialog). -->
{#if rollbackAsked !== null}
    {@const question = rollbackAsked}
    <RollbackConfirmDialog
        variant={rollbackConfirmVariant(question.op.kind)}
        rest={question.op.rollbackState === 'partiallyRolledBack'}
        subset={question.action.kind === 'selected' && question.action.subset}
        onConfirm={() => confirmRollback(question.opId, question.selection)}
        onCancel={() => (asked = null)}
    />
{/if}

<style>
    /* Fills `fillBody`'s slot, so the two panes take the height and each owns one scroller. */
    .body {
        display: flex;
        flex-direction: column;
        flex: 1 1 auto;
        min-height: 0;
    }

    /* Side by side; under `STACK_BELOW_PX` the list stacks above the detail. Measured with
       `useInlineSize`, since WebKit's floor here predates container queries. */
    .split {
        display: flex;
        flex: 1 1 auto;
        min-height: 0;
        border-top: 1px solid var(--color-border-subtle);
        border-bottom: 1px solid var(--color-border-subtle);
    }

    .master {
        display: flex;
        flex-direction: column;
        flex: 0 0 300px;
        min-height: 0;
        border-right: 1px solid var(--color-border-subtle);
        background: var(--color-bg-secondary);
    }

    .split.is-stacked {
        flex-direction: column;
    }

    .split.is-stacked .master {
        flex: 0 0 35%;
        border-right: none;
        border-bottom: 1px solid var(--color-border-subtle);
    }

    .loading {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
    }

    .notice {
        margin: var(--spacing-md) 0;
        font-size: var(--font-size-md);
        color: var(--color-text-secondary);
    }

    /* It leads the list rather than replacing it, so it needs no space above. */
    .recording-off {
        margin-top: 0;
    }

    .selected-count {
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }

    .op-actions {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-sm);
    }
</style>
