<script lang="ts">
    /**
     * The one question in front of Rollback.
     *
     * ⚠️ **The body has to match the operation.** Undoing a copy deletes what the copy
     * wrote, and a destination it OVERWROTE is one of those: no backup of the replaced
     * original is kept (`write_operations/transfer/CLAUDE.md` § "Overwrite isn't
     * reversible"). Undoing a move deletes NOTHING; the files travel home. Wording every
     * rollback as a delete would scare people off an operation that takes nothing away,
     * and wording every one as a restore would hide a real deletion. Hence `variant`.
     * Rationale and the surfaces that raise it: `DETAILS.md` § "Rollback asks first".
     *
     * The safe answer takes focus in every variant, so a reflex Enter never rolls back.
     * Presentational: each host owns the pending-rollback state and calls its own
     * rollback in `onConfirm`.
     */
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import Button from '$lib/ui/Button.svelte'
    import type { MessageKey } from '$lib/intl/keys.gen'
    import { tString } from '$lib/intl/messages.svelte'
    import type { RollbackConfirmVariant } from './reversal-wording'

    interface Props {
        variant: RollbackConfirmVariant
        /** Go ahead: reverse the operation. */
        onConfirm: () => void
        /** Leave the operation exactly as it is. Also what Escape and × do. */
        onCancel: () => void
    }

    const { variant, onConfirm, onCancel }: Props = $props()

    /**
     * Body + cancel wording + whether the confirming button reads as destructive.
     * Only the two deleting variants get `danger`: red on "put my files back" would
     * cry wolf, and this app spends that colour on operations that take something away.
     */
    function wording(v: RollbackConfirmVariant): { body: MessageKey; cancel: MessageKey; destructive: boolean } {
        switch (v) {
            case 'stopAndDelete':
                return {
                    body: 'fileOperations.rollbackConfirm.body',
                    cancel: 'fileOperations.rollbackConfirm.keep',
                    destructive: true,
                }
            case 'undoByDeleting':
                return {
                    body: 'fileOperations.rollbackConfirm.bodyUndoByDeleting',
                    cancel: 'fileOperations.rollbackConfirm.leaveAsIs',
                    destructive: true,
                }
            case 'undoByMovingBack':
                return {
                    body: 'fileOperations.rollbackConfirm.bodyUndoByMovingBack',
                    cancel: 'fileOperations.rollbackConfirm.leaveAsIs',
                    destructive: false,
                }
            case 'undoByRenamingBack':
                return {
                    body: 'fileOperations.rollbackConfirm.bodyUndoByRenamingBack',
                    cancel: 'fileOperations.rollbackConfirm.leaveAsIs',
                    destructive: false,
                }
        }
    }

    const words = $derived(wording(variant))
</script>

<ModalDialog
    dialogId="rollback-confirmation"
    titleId="rollback-confirmation-title"
    ariaDescribedby="rollback-confirmation-body"
    onclose={onCancel}
    containerStyle="max-width: 440px"
>
    {#snippet title()}{tString('fileOperations.rollbackConfirm.title')}{/snippet}

    <p id="rollback-confirmation-body" class="rollback-confirm-body">
        {tString(words.body)}
    </p>

    {#snippet footer()}
        <!-- The safe answer takes focus, so a reflex Enter keeps things as they are. -->
        <Button variant="secondary" autoFocus onclick={onCancel}>{tString(words.cancel)}</Button>
        <Button variant={words.destructive ? 'danger' : 'primary'} onclick={onConfirm}
            >{tString('fileOperations.rollbackConfirm.rollBack')}</Button
        >
    {/snippet}
</ModalDialog>

<style>
    .rollback-confirm-body {
        margin: 0;
        font-size: var(--font-size-md);
        color: var(--color-text-secondary);
    }
</style>
