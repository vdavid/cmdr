<script lang="ts">
    // The Suggested ops dialog: what Ask Cmdr proposed, laid out so the user can decide.
    //
    // The layout is the argument. Each group shows the agent's reason under a label that says
    // whose words they are, next to facts Cmdr holds by itself, so a hallucinated claim is
    // visible AS a claim rather than reading like a finding. Reversibility and a folder that
    // doesn't exist yet are DISCLOSED here and never block: once the user approves, it is
    // exactly as if they started the action.
    //
    // The file list is virtualized over a WINDOW the store fetches, because a group of 60,000
    // ops is legitimate and neither the list nor the wire may grow with it.
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import Button from '$lib/ui/Button.svelte'
    import Checkbox from '$lib/ui/Checkbox.svelte'
    import Size from '$lib/ui/Size.svelte'
    import DateLabel from '$lib/ui/DateLabel.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import { tooltip } from '$lib/tooltip/tooltip'
    import { calculateVirtualWindow } from '$lib/file-explorer/views/virtual-scroll'
    import {
        approvableCount,
        approveGroup,
        closeSuggestedOps,
        collapseGroup,
        ensureOpWindow,
        expandGroup,
        opAt,
        openGroup,
        refreshSuggestions,
        rejectGroup,
        suggestedOpsState,
        toggleOp,
    } from './suggested-ops-trigger.svelte'

    /** Row height in pixels, fixed so the virtual window stays plain arithmetic. */
    const ROW_HEIGHT = 30
    /** Rows kept above and below the viewport, so a fast scroll doesn't flash empty rows. */
    const ROW_BUFFER = 8

    let scrollTop = $state(0)
    let viewportHeight = $state(360)

    const group = $derived(openGroup())
    /** Every op row the open group has, from `COUNT(*)`: the scrollbar is right long before the
     *  rows themselves are read. */
    const totalRows = $derived(suggestedOpsState.window?.total ?? group?.totalOpCount ?? 0)

    const virtual = $derived(
        calculateVirtualWindow({
            direction: 'vertical',
            itemSize: ROW_HEIGHT,
            bufferSize: ROW_BUFFER,
            containerSize: viewportHeight,
            scrollOffset: scrollTop,
            totalItems: totalRows,
        }),
    )

    // Pull the window the viewport actually reaches. A request for rows already held is a
    // no-op, so scrolling inside a window costs nothing.
    $effect(() => {
        const open = group
        if (!open) return
        void ensureOpWindow(open.groupId, virtual.startIndex)
    })

    const visibleIndexes = $derived(
        Array.from({ length: Math.max(0, virtual.endIndex - virtual.startIndex) }, (_, i) => virtual.startIndex + i),
    )

    function reversibilityLabel(reversible: string): string {
        if (reversible === 'restoreMove') return tString('suggestedOps.reversibleRestoreMove')
        if (reversible === 'deleteWhatWasWritten') return tString('suggestedOps.reversibleDeleteWritten')
        return tString('suggestedOps.irreversible')
    }
</script>

<ModalDialog
    titleId="suggested-ops-title"
    dialogId="suggested-ops"
    role="dialog"
    onclose={closeSuggestedOps}
    containerStyle="width: 760px; max-width: calc(100vw - 2 * var(--spacing-xl))"
    resizable
>
    {#snippet title()}
        <span>{tString('suggestedOps.title')}</span>
    {/snippet}

    <p class="lede">{tString('suggestedOps.description')}</p>

    {#if suggestedOpsState.loading}
        <p class="notice" role="status">{tString('suggestedOps.loadingFiles')}</p>
    {:else if suggestedOpsState.loadError}
        <p class="notice notice-problem" role="alert">{tString('suggestedOps.loadFailed')}</p>
    {:else if suggestedOpsState.sweeps.length === 0}
        <p class="notice" role="status">{tString('suggestedOps.empty')}</p>
    {:else}
        {#each suggestedOpsState.sweeps as sweep (sweep.sweepId)}
            <section class="sweep">
                <header class="sweep-header">
                    <DateLabel modifiedAt={sweep.createdAt} />
                    {#if sweep.rationale}
                        <p class="agent-words">
                            <span class="agent-label">{tString('suggestedOps.agentReason')}</span>
                            <span class="agent-text">{sweep.rationale}</span>
                        </p>
                    {/if}
                </header>

                {#each sweep.groups as g (g.groupId)}
                    <article class="group" class:expanded={g.groupId === suggestedOpsState.openGroupId}>
                        <div class="group-head">
                            <h3 class="group-name">{g.displayName}</h3>
                            <span class="count">{tString('suggestedOps.fileCount', { count: g.liveOpCount })}</span>
                        </div>

                        <ul class="markers">
                            <li class="marker" class:danger={g.reversible === 'irreversible'}>
                                {reversibilityLabel(g.reversible)}
                            </li>
                            {#if g.destinationState === 'willBeCreated'}
                                <li class="marker" use:tooltip={tString('suggestedOps.folderWillBeCreatedTooltip')}>
                                    {tString('suggestedOps.folderWillBeCreated')}
                                </li>
                            {:else if g.destinationState === 'unknown'}
                                <li class="marker">{tString('suggestedOps.destinationUnknown')}</li>
                            {/if}
                            {#if g.fromSelector}
                                <li class="marker">{tString('suggestedOps.fromPattern')}</li>
                            {/if}
                        </ul>

                        {#if g.rationale}
                            <p class="agent-words">
                                <span class="agent-label">{tString('suggestedOps.agentReason')}</span>
                                <span class="agent-text">{g.rationale}</span>
                            </p>
                        {/if}

                        <div class="group-actions">
                            {#if g.groupId === suggestedOpsState.openGroupId}
                                <Button variant="secondary" onclick={collapseGroup}>
                                    {tString('suggestedOps.collapse')}
                                </Button>
                            {:else}
                                <Button variant="secondary" onclick={() => void expandGroup(g.groupId)}>
                                    {tString('suggestedOps.review')}
                                </Button>
                            {/if}
                            <Button
                                variant="primary"
                                disabled={suggestedOpsState.busyGroupId !== null}
                                onclick={() => void approveGroup(g.groupId)}
                            >
                                {g.groupId === suggestedOpsState.openGroupId
                                    ? tString('suggestedOps.approveCount', { count: approvableCount() })
                                    : tString('suggestedOps.approve')}
                            </Button>
                            <Button
                                variant="secondary"
                                disabled={suggestedOpsState.busyGroupId !== null}
                                onclick={() => void rejectGroup(g.groupId)}
                            >
                                {tString('suggestedOps.reject')}
                            </Button>
                        </div>

                        {#if g.groupId === suggestedOpsState.openGroupId}
                            {#if suggestedOpsState.changedUnderReview}
                                <!-- The rows stay exactly where they are. Re-ordering a list
                                     somebody is halfway through deciding on is how a wrong row
                                     gets approved. -->
                                <div class="changed" role="status">
                                    <span>{tString('suggestedOps.changedUnderReview')}</span>
                                    <Button variant="secondary" onclick={() => void refreshSuggestions()}>
                                        {tString('suggestedOps.showNewVersion')}
                                    </Button>
                                </div>
                            {/if}

                            <!-- The other half of the juxtaposition: the agent's reason is
                                 labelled as its words above, and these are the facts Cmdr holds
                                 by itself, so a claim can be checked against something the agent
                                 could not invent. -->
                            <p class="facts-label">{tString('suggestedOps.cmdrFacts')}</p>

                            <div class="col-heads" aria-hidden="true">
                                <span>{tString('suggestedOps.columnFile')}</span>
                                {#if g.verb === 'rename'}
                                    <span>{tString('suggestedOps.columnNewName')}</span>
                                {/if}
                                <span>{tString('suggestedOps.columnSize')}</span>
                                <span>{tString('suggestedOps.columnChanged')}</span>
                            </div>

                            <div
                                class="op-list"
                                onscroll={(e) => (scrollTop = e.currentTarget.scrollTop)}
                                bind:clientHeight={viewportHeight}
                            >
                                <div class="op-spacer" style:height="{virtual.totalSize}px">
                                    <div class="op-window" style:transform="translateY({virtual.offset}px)">
                                        {#each visibleIndexes as index (index)}
                                            {@const op = opAt(index)}
                                            <div class="op-row" style:height="{ROW_HEIGHT}px">
                                                {#if op}
                                                    <Checkbox
                                                        checked={!suggestedOpsState.deselected.has(op.opId)}
                                                        onCheckedChange={() => { toggleOp(op.opId); }}
                                                        ariaLabel={tString('suggestedOps.includeFile')}
                                                    />
                                                    <span class="cell path" use:tooltip={op.sourcePath}>
                                                        {op.sourcePath}
                                                    </span>
                                                    {#if g.verb === 'rename'}
                                                        <span class="cell">{op.newName ?? ''}</span>
                                                    {/if}
                                                    <span class="cell">
                                                        <Size
                                                            bytes={op.snapshotSize}
                                                            fallback={tString('suggestedOps.noIndexData')}
                                                        />
                                                    </span>
                                                    <span class="cell">
                                                        {#if op.snapshotModified === null}
                                                            {tString('suggestedOps.noIndexData')}
                                                        {:else}
                                                            <DateLabel modifiedAt={op.snapshotModified} />
                                                        {/if}
                                                    </span>
                                                {:else}
                                                    <span class="op-pending">{tString('suggestedOps.loadingFiles')}</span>
                                                {/if}
                                            </div>
                                        {/each}
                                    </div>
                                </div>
                            </div>
                        {/if}
                    </article>
                {/each}
            </section>
        {/each}
    {/if}
</ModalDialog>

<style>
    .lede {
        margin: 0 0 var(--spacing-md);
        color: var(--color-text-secondary);
    }

    .notice {
        margin: var(--spacing-lg) 0;
        text-align: center;
        color: var(--color-text-secondary);
    }

    .sweep {
        margin-bottom: var(--spacing-lg);
    }

    .sweep-header {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xxs);
        margin-bottom: var(--spacing-sm);
    }

    .group {
        border: 1px solid var(--color-border);
        border-radius: var(--radius-md);
        padding: var(--spacing-sm);
        margin-bottom: var(--spacing-sm);
    }

    .group-head {
        display: flex;
        align-items: baseline;
        gap: var(--spacing-sm);
    }

    .group-name {
        margin: 0;
        font-size: var(--font-size-md);
        font-weight: 600;
    }

    .count {
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
    }

    .markers {
        display: flex;
        flex-wrap: wrap;
        gap: var(--spacing-xxs);
        margin: var(--spacing-xxs) 0;
        padding: 0;
        list-style: none;
    }

    .marker {
        padding: 0 var(--spacing-xxs);
        border-radius: var(--radius-sm);
        background: var(--color-bg-tertiary);
        color: var(--color-text-secondary);
        font-size: var(--font-size-xs);
    }

    /* The agent's words are visibly ITS words. The label isn't decoration: an unlabelled
       reason reads as something Cmdr checked. */
    .agent-words {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xxs);
        margin: var(--spacing-xxs) 0;
    }

    .agent-label {
        color: var(--color-text-tertiary);
        font-size: var(--font-size-xs);
    }

    .agent-text {
        font-style: italic;
        color: var(--color-text-secondary);
    }

    .group-actions {
        display: flex;
        gap: var(--spacing-xxs);
        margin-top: var(--spacing-xs);
    }

    .changed {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: var(--spacing-sm);
        margin-top: var(--spacing-xs);
        padding: var(--spacing-xxs) var(--spacing-xs);
        border-radius: var(--radius-sm);
        background: var(--color-bg-tertiary);
    }

    .col-heads,
    .op-row {
        display: grid;
        grid-template-columns: auto minmax(0, 3fr) minmax(0, 1fr) minmax(0, 1fr);
        align-items: center;
        gap: var(--spacing-xs);
    }

    .facts-label {
        margin: var(--spacing-sm) 0 0;
        color: var(--color-text-tertiary);
        font-size: var(--font-size-xs);
    }

    .col-heads {
        grid-template-columns: minmax(0, 3fr) minmax(0, 1fr) minmax(0, 1fr);
        margin-top: var(--spacing-xs);
        padding: 0 var(--spacing-xxs);
        color: var(--color-text-tertiary);
        font-size: var(--font-size-xs);
    }

    .op-list {
        max-height: 360px;
        overflow-y: auto;
        border-top: 1px solid var(--color-border);
    }

    .op-spacer {
        position: relative;
    }

    .op-window {
        position: absolute;
        inset-inline: 0;
        top: 0;
    }

    .op-row {
        padding: 0 var(--spacing-xxs);
        font-size: var(--font-size-sm);
    }

    .cell {
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .op-pending {
        grid-column: 1 / -1;
        color: var(--color-text-tertiary);
    }
</style>
