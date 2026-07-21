<script lang="ts">
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import Button from '$lib/ui/Button.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import { onDirectoryDiff } from '$lib/tauri-commands'
    import { onMount } from 'svelte'
    import { tooltip } from '$lib/tooltip/tooltip'
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
        containerStyle="width: min(820px, calc(100vw - 48px))"
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
                                <th scope="col">{tString('askCmdr.renameReview.allow')}</th>
                                <th scope="col">{tString('askCmdr.renameReview.originalName')}</th>
                                <th scope="col">{tString('askCmdr.renameReview.newName')}</th>
                            </tr>
                        </thead>
                        <tbody>
                            {#each review.rows as row (row.rowId)}
                                <tr class:blocked={row.blockedReason}>
                                    <td>
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
                                    <td title={row.sourceName}><span>{row.sourceName}</span></td>
                                    <td title={row.destinationName}>
                                        <span>{row.destinationName}</span>
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

    th,
    td {
        padding: var(--spacing-sm);
        text-align: left;
        border-bottom: 1px solid var(--color-border-subtle);
    }

    th:first-child,
    td:first-child {
        width: 64px;
        text-align: center;
    }

    td span {
        display: block;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    tr.blocked {
        color: var(--color-text-secondary);
        background: var(--color-bg-tertiary);
    }

    small {
        display: block;
        margin-top: var(--spacing-xxs);
        color: var(--color-text-secondary);
    }

    .warning-badge {
        display: inline-flex;
        width: fit-content;
        margin-top: var(--spacing-xxs);
        padding: 0 var(--spacing-xs);
        border-radius: var(--radius-sm);
        color: var(--color-warning-text);
        background: var(--color-warning-bg);
        font-size: var(--font-size-xs);
        white-space: nowrap;
    }

    .danger-badge {
        display: inline-flex;
        width: fit-content;
        margin-top: var(--spacing-xxs);
        padding: 0 var(--spacing-xs);
        border-radius: var(--radius-sm);
        color: var(--color-error-text);
        background: var(--color-error-bg);
        font-size: var(--font-size-xs);
        white-space: nowrap;
    }
</style>
