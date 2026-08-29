<script lang="ts">
    /**
     * The conflict prompt for an operation with no progress dialog in front of
     * it. Chrome only: the body is the very same `TransferConflictDialog` the
     * progress dialog embeds, and every button routes through the same
     * `resolveWriteConflict` / `cancelWriteOperation` calls.
     *
     * Mounted unconditionally by the main page and gated on the host's queue, so
     * the component owns its own visibility and the page stays a one-liner.
     * Decisions and the whole flow: `operation-conflict.svelte.ts` and
     * `DETAILS.md`.
     */
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import type { ConflictResolution } from '$lib/file-explorer/types'
    import TransferConflictDialog from './transfer/TransferConflictDialog.svelte'
    import RollbackConfirmDialog from './RollbackConfirmDialog.svelte'
    import { getFolderName } from './transfer/transfer-dialog-utils'
    import {
        cancelConflictPrompt,
        getConflictPrompt,
        isCancellingConflictPrompt,
        isResolvingConflictPrompt,
        resolveConflictPrompt,
    } from './operation-conflict.svelte'

    /** Roomier than the buttons need (two 200 px columns), narrower than the
     *  progress dialog: that one is 580 px for its fixed-width readout columns,
     *  which this body doesn't have. */
    const DIALOG_WIDTH_STYLE = 'width: 520px; min-width: 520px'

    /** Rollback deletes everything the operation has written, and a file it
     *  overwrote has no backup, so the click raises the question rather than
     *  the deletion. `DETAILS.md` § "Rollback asks first". */
    let rollbackAsked = $state(false)

    const prompt = $derived(getConflictPrompt())
    const snapshot = $derived(prompt?.snapshot ?? null)
    const isCopy = $derived(snapshot?.operationType === 'copy')
    const isMove = $derived(snapshot?.operationType === 'move')
    /** The typed truth from the snapshot, rather than the progress dialog's own
     *  same-volume guess: a CROSS-volume move can't roll back either. */
    const rollbackUnavailable = $derived(snapshot !== null && !snapshot.supportsRollback)

    /** Which operation is asking. With several running at once, the buttons
     *  below are ambiguous without it. */
    const context = $derived(
        snapshot === null
            ? null
            : tString('fileOperations.operationConflict.context', {
                  type: snapshot.operationType,
                  hasDestination: snapshot.destination === null ? 'no' : 'yes',
                  destination: snapshot.destination === null ? '' : getFolderName(snapshot.destination),
              }),
    )
</script>

{#if prompt}
    <!-- No `onclose`: deliberately no × and no Escape. Every way out of this
         dialog is a decision about the user's files, and the reflex Escape on a
         modal that appeared over whatever they were doing would cancel a
         transfer they may not even have been watching. The conflict body's own
         Cancel / Rollback row is the way out. -->
    <ModalDialog
        titleId="operation-conflict-title"
        ariaDescribedby={context === null ? undefined : 'operation-conflict-context'}
        dialogId="operation-conflict"
        containerStyle={DIALOG_WIDTH_STYLE}
    >
        {#snippet title()}{tString('fileOperations.transferProgress.titleConflict')}{/snippet}

        {#if context !== null}
            <p id="operation-conflict-context" class="context">{context}</p>
        {/if}

        <TransferConflictDialog
            conflictEvent={prompt.event}
            {isCopy}
            {isMove}
            {rollbackUnavailable}
            isCancelling={isCancellingConflictPrompt()}
            isResolvingConflict={isResolvingConflictPrompt()}
            onResolve={(resolution: ConflictResolution, applyToAll: boolean) => {
                void resolveConflictPrompt(resolution, applyToAll)
            }}
            onCancel={(rollback: boolean) => {
                if (rollback) {
                    rollbackAsked = true
                    return
                }
                void cancelConflictPrompt(false)
            }}
        />

        {#if prompt.pausedOthers}
            <p class="paused-note">{tString('fileOperations.operationConflict.pausedNote')}</p>
        {/if}
    </ModalDialog>

    <!-- Stacked over the prompt that raised it: same subtree, so DOM order puts
         it on top and its focus trap takes over until it goes
         (`$lib/ui/DETAILS.md` § ModalDialog). -->
    {#if rollbackAsked}
        <RollbackConfirmDialog
            variant="stopAndDelete"
            onConfirm={() => {
                rollbackAsked = false
                void cancelConflictPrompt(true)
            }}
            onCancel={() => {
                rollbackAsked = false
            }}
        />
    {/if}
{/if}

<style>
    /* Sits between the title and the filename the body leads with, so the eye
       reads "which operation" before "which file". Matches the body's own
       horizontal padding. */
    .context {
        margin: 0;
        padding: var(--spacing-md) var(--spacing-xl) 0;
        text-align: center;
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }

    .paused-note {
        margin: 0;
        padding: 0 var(--spacing-xl) var(--spacing-lg);
        text-align: center;
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
    }
</style>
