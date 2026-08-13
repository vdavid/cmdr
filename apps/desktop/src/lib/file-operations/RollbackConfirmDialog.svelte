<script lang="ts">
    /**
     * The one question in front of Rollback.
     *
     * Rollback deletes every destination the operation has written, and a
     * destination it OVERWROTE is one of those: no backup of the replaced
     * original is kept (`write_operations/transfer/CLAUDE.md` § "Overwrite isn't
     * reversible"). So the button that sits beside a harmless Cancel can take
     * away a file the user had before the operation started, and that is what
     * this asks about. Rationale and the surfaces that raise it:
     * `DETAILS.md` § "Rollback asks first".
     *
     * Presentational: each host owns the pending-rollback state and calls its
     * own rollback in `onConfirm`.
     */
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import Button from '$lib/ui/Button.svelte'
    import { tString } from '$lib/intl/messages.svelte'

    interface Props {
        /** Go ahead: delete what the operation wrote. */
        onConfirm: () => void
        /** Leave the operation exactly as it is. Also what Escape and × do. */
        onCancel: () => void
    }

    const { onConfirm, onCancel }: Props = $props()
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
        {tString('fileOperations.rollbackConfirm.body')}
    </p>

    {#snippet footer()}
        <!-- The safe answer takes focus, so a reflex Enter keeps the files. -->
        <Button variant="secondary" autoFocus onclick={onCancel}
            >{tString('fileOperations.rollbackConfirm.keep')}</Button
        >
        <Button variant="danger" onclick={onConfirm}>{tString('fileOperations.rollbackConfirm.rollBack')}</Button>
    {/snippet}
</ModalDialog>

<style>
    .rollback-confirm-body {
        margin: 0;
        font-size: var(--font-size-md);
        color: var(--color-text-secondary);
    }
</style>
