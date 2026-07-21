<script lang="ts">
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import Button from '$lib/ui/Button.svelte'
    import Icon from '$lib/ui/Icon.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import { onDirectoryDiff } from '$lib/tauri-commands'
    import { onMount } from 'svelte'
    import { tooltip } from '$lib/tooltip/tooltip'
    import { useShortenMiddle } from '$lib/utils/shorten-middle-action'
    import {
        applyRenameReview,
        allowAllRenameRows,
        askCmdrState,
        cancelRenameReview,
        denyAllRenameRows,
        renameReviewListingChanged,
        setRenameRowAllowed,
    } from './ask-cmdr-trigger.svelte'

    const review = $derived(askCmdrState.renameReview)
    const allowedCount = $derived(review?.rows.filter((row) => row.allowed && !row.blockedReason).length ?? 0)
    const blockedCount = $derived(review?.rows.filter((row) => row.blockedReason).length ?? 0)
    const renameLabel = $derived(tString('askCmdr.renameReview.rename', { count: allowedCount }))

    function toggleRow(rowId: string, checked: boolean): void {
        setRenameRowAllowed(rowId, checked)
    }

    onMount(() => {
        const listener = onDirectoryDiff((diff) => {
            void renameReviewListingChanged(diff.changes)
        })
        return () => {
            void listener.then((unlisten) => { unlisten(); }).catch(() => {})
        }
    })
</script>

{#if review}
    <ModalDialog
        titleId="bulk-rename-review-title"
        dialogId="bulk-rename-review"
        containerStyle="width: min(640px, calc(100vw - 48px))"
        onclose={cancelRenameReview}
    >
        {#snippet title()}{tString('askCmdr.renameReview.title')}{/snippet}

        <div class="dialog-body">
            <p class="description">{tString('askCmdr.renameReview.description')}</p>
            {#if review.expired}
                <p class="notice" role="status">{tString('askCmdr.renameReview.expired')}</p>
            {:else}
                <div class="bulk-actions">
                    <Button size="mini" onclick={allowAllRenameRows} disabled={review.preflighting}>
                        {tString('askCmdr.renameReview.allowAll')}
                    </Button>
                    <Button size="mini" onclick={denyAllRenameRows} disabled={review.preflighting}>
                        {tString('askCmdr.renameReview.denyAll')}
                    </Button>
                    <span class="summary" role="status" aria-live="polite">
                        {tString('askCmdr.renameReview.status', { allowed: allowedCount, blocked: blockedCount })}
                    </span>
                </div>
                <div class="rows" aria-busy={review.preflighting}>
                    <table>
                        <thead>
                            <tr>
                                <th scope="col" class="allow-col">{tString('askCmdr.renameReview.allow')}</th>
                                <th scope="col" class="current-col">{tString('askCmdr.renameReview.originalName')}</th>
                                <th scope="col" class="arrow-col" aria-hidden="true"></th>
                                <th scope="col">{tString('askCmdr.renameReview.newName')}</th>
                            </tr>
                        </thead>
                        <tbody>
                            {#each review.rows as row (row.rowId)}
                                <tr class:blocked={row.blockedReason}>
                                    <td class="allow-cell">
                                        <input
                                            type="checkbox"
                                            checked={row.allowed}
                                            disabled={Boolean(row.blockedReason) || review.preflighting}
                                            aria-label={row.allowed
                                                ? `${tString('askCmdr.renameReview.deny')}: ${row.sourceName}`
                                                : `${tString('askCmdr.renameReview.allow')}: ${row.sourceName}`}
                                            onchange={(event) => { toggleRow(row.rowId, event.currentTarget.checked); }}
                                        />
                                    </td>
                                    <td class="name current">
                                        <span class="fname" use:useShortenMiddle={{ text: row.sourceName, preferBreakAt: '.', startRatio: 0.7 }}></span>
                                    </td>
                                    <td class="arrow"><Icon name="arrow-right" size={14} aria-hidden="true" /></td>
                                    <td class="name new">
                                        <span class="fname" use:useShortenMiddle={{ text: row.destinationName, preferBreakAt: '.', startRatio: 0.7 }}></span>
                                        <span class="badges">
                                            {#if row.warnings.includes('extensionChanged')}
                                                <span
                                                    class="warning-badge"
                                                    data-rename-warning="extensionChanged"
                                                    tabindex="0"
                                                    aria-label={tString('askCmdr.renameReview.extensionTooltip')}
                                                    use:tooltip={tString('askCmdr.renameReview.extensionTooltip')}
                                                >{tString('askCmdr.renameReview.extensionBadge')}</span>
                                            {/if}
                                            {#if row.warnings.includes('cycle')}
                                                <span
                                                    class="warning-badge"
                                                    data-rename-warning="cycle"
                                                    tabindex="0"
                                                    aria-label={tString('askCmdr.renameReview.cycleTooltip')}
                                                    use:tooltip={tString('askCmdr.renameReview.cycleTooltip')}
                                                >{tString('askCmdr.renameReview.cycleBadge')}</span>
                                            {/if}
                                            {#if row.blockedReason === 'targetExists'}
                                                <span
                                                    class="danger-badge"
                                                    data-warning="overwrite"
                                                    tabindex="0"
                                                    aria-label={tString('askCmdr.renameReview.overwriteTooltip')}
                                                    use:tooltip={tString('askCmdr.renameReview.overwriteTooltip')}
                                                >{tString('askCmdr.renameReview.overwriteBadge')}</span>
                                            {/if}
                                            {#if row.blockedReason === 'sourceMissing'}
                                                <span
                                                    class="danger-badge"
                                                    data-warning="source-missing"
                                                    tabindex="0"
                                                    aria-label={tString('askCmdr.renameReview.sourceMissingTooltip')}
                                                    use:tooltip={tString('askCmdr.renameReview.sourceMissingTooltip')}
                                                >{tString('askCmdr.renameReview.sourceMissingBadge')}</span>
                                            {/if}
                                        </span>
                                        {#if row.blockedReason}
                                            <small>{tString('askCmdr.renameReview.blocked')}</small>
                                        {/if}
                                    </td>
                                </tr>
                            {/each}
                        </tbody>
                    </table>
                </div>
            {/if}
        </div>

        {#snippet footer()}
            <Button onclick={cancelRenameReview}>{tString('askCmdr.renameReview.cancel')}</Button>
            <Button
                variant="primary"
                onclick={applyRenameReview}
                disabled={review.preflighting || review.expired || allowedCount === 0}
                aria-label={renameLabel}
            >{renameLabel}</Button>
        {/snippet}
    </ModalDialog>
{/if}

<style>
    .dialog-body {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-md);
        /* Dialog body reads at 14px; without this the list inherits the 16px root
           and every filename feels oversized against the chrome. */
        font-size: var(--font-size-md);
        line-height: 1.4;
    }

    .description,
    .notice {
        margin: 0;
        color: var(--color-text-secondary);
    }

    .notice {
        padding: var(--spacing-sm);
        background: var(--color-bg-tertiary);
        border-radius: var(--radius-sm);
    }

    .bulk-actions {
        display: flex;
        align-items: center;
        flex-wrap: wrap;
        gap: var(--spacing-xs);
    }

    .summary {
        margin-left: auto;
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }

    .rows {
        max-height: min(52vh, 560px);
        overflow: auto;
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
    }

    table {
        width: 100%;
        border-collapse: collapse;
        table-layout: fixed;
    }

    /* Column headers are chrome: quiet, not the bold black the browser defaults to. */
    th {
        padding: var(--spacing-sm) var(--spacing-md);
        font-size: var(--font-size-sm);
        font-weight: 500;
        color: var(--color-text-secondary);
        text-align: left;
        border-bottom: 1px solid var(--color-border-subtle);
        background: var(--color-bg-secondary);
        position: sticky;
        top: 0;
        z-index: var(--z-sticky);
    }

    td {
        padding: var(--spacing-sm) var(--spacing-md);
        vertical-align: middle;
        border-bottom: 1px solid var(--color-border-subtle);
    }

    tbody tr:last-child td {
        border-bottom: none;
    }

    /* The checkbox column: fixed and centered. */
    .allow-col,
    .allow-cell {
        width: 56px;
        text-align: center;
    }

    input[type='checkbox'] {
        /* Interactive chrome follows the system accent, never a fixed hue. */
        accent-color: var(--color-accent);
        width: 15px;
        height: 15px;
        margin: 0;
        vertical-align: middle;
    }

    /* The two names flank a centered arrow so each row reads as one rename,
       instead of drifting apart across the dialog width. */
    .current-col,
    .name.current {
        text-align: right;
    }

    .arrow-col {
        width: 32px;
    }

    .arrow {
        width: 32px;
        text-align: center;
        color: var(--color-text-tertiary);
    }

    .arrow :global(svg) {
        vertical-align: middle;
    }

    .name .fname {
        display: block;
    }

    .name.new .badges {
        display: inline-flex;
        flex-wrap: wrap;
        gap: var(--spacing-xs);
    }

    .name.new .badges:not(:empty) {
        margin-top: var(--spacing-xs);
    }

    tr.blocked .name {
        color: var(--color-text-secondary);
    }

    small {
        display: block;
        margin-top: var(--spacing-xxs);
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
    }

    .warning-badge,
    .danger-badge {
        display: inline-flex;
        width: fit-content;
        padding: 0 var(--spacing-xs);
        border-radius: var(--radius-sm);
        font-size: var(--font-size-xs);
        white-space: nowrap;
    }

    .warning-badge {
        color: var(--color-warning-text);
        background: var(--color-warning-bg);
    }

    .danger-badge {
        color: var(--color-error-text);
        background: var(--color-error-bg);
    }
</style>
