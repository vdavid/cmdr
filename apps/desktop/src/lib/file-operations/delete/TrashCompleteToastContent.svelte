<script lang="ts">
    /**
     * The toast after files go to the Trash, with the two things a user wants
     * next: put them back, or go look at them.
     *
     * Undo is the one that matters. Trashing by accident is cheap to do and, until
     * this existed, expensive to reverse: the rollback engine could always restore
     * a trash, but only the operation log and the queue row offered it, and neither
     * is open three seconds after a mis-keyed F8.
     *
     * ❌ No "delete permanently" here, deliberately. This toast renders after EVERY
     * trash, including the ones the user is glad they can take back, and a
     * one-click irreversible action on a transient surface that appears that often
     * is a misclick away from the one thing the journal can never reverse. The
     * delete dialog is where permanent is chosen (Shift flips it in place).
     */
    import Button from '$lib/ui/Button.svelte'
    import { dismissToast } from '$lib/ui/toast'
    import { tString } from '$lib/intl/messages.svelte'
    import { runTrashUndo } from './trash-undo'
    import { goToTrashedItems } from './go-to-trash'
    import type { PaneRevealAPI } from '$lib/file-explorer/navigation/navigate-and-select'

    interface Props {
        /** Dedup id of this toast; lets the component get out of its own way. */
        toastId: string
        /** The composed "Moved N files to trash" sentence, already localized. */
        message: string
        /**
         * The journaled trash operation. Both actions read from it: Undo reverses
         * it, and "Go to trash" reads where its items actually landed.
         */
        operationId: string
        /**
         * A directory on the volume the items came FROM, so "Go to trash" can still
         * open the right volume's trash when the journal recorded no in-trash
         * location. Snapshotted at toast time; the pane may have moved on since.
         */
        sourceFolderPath: string
        /**
         * Snapshot of the explorer handle at toast-creation time. Both actions are
         * no-ops without it (HMR or pre-mount).
         */
        explorer: PaneRevealAPI | undefined
    }

    const { toastId, message, operationId, sourceFolderPath, explorer }: Props = $props()

    /**
     * Both actions hand the conversation to something else (the undo raises its own
     * progress toast, the navigation moves the panes), so this toast steps aside
     * rather than sitting on top of what it started.
     */
    function handleUndo() {
        dismissToast(toastId)
        void runTrashUndo(operationId)
    }

    function handleGoToTrash() {
        dismissToast(toastId)
        void goToTrashedItems(explorer, operationId, sourceFolderPath)
    }
</script>

<div class="toast-body">
    <span class="message">{message}</span>
    <div class="actions">
        <Button size="mini" variant="secondary" onclick={handleGoToTrash}>
            {tString('fileOperations.trash.goToTrashAction')}
        </Button>
        <Button size="mini" variant="primary" onclick={handleUndo}>
            {tString('fileOperations.trash.undoAction')}
        </Button>
    </div>
</div>

<style>
    .toast-body {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
        font-size: var(--font-size-sm);
    }

    .message {
        color: var(--color-text-primary);
    }

    /* Undo sits rightmost, where the eye lands last and the pointer travels least:
       it's the action this toast exists for. */
    .actions {
        display: flex;
        justify-content: flex-end;
        gap: var(--spacing-xs);
        margin-top: var(--spacing-xs);
    }
</style>
