<script lang="ts">
    /**
     * "Forget everything Ask Cmdr remembers?" confirmation, opened from
     * `AskCmdrSection`.
     *
     * Deleting is quick enough that there is no in-flight state to model, but both
     * buttons still go dead once the confirm has fired, so a double Enter can't
     * send a second delete at a folder the first one is walking.
     */
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import Button from '$lib/ui/Button.svelte'
    import { tString } from '$lib/intl/messages.svelte'

    interface Props {
        /** True while the delete is running. */
        isForgetting: boolean
        onConfirm: () => void
        onCancel: () => void
    }

    const { isForgetting, onConfirm, onCancel }: Props = $props()
</script>

<ModalDialog
    titleId="forget-memory-title"
    dialogId="forget-memory"
    role="alertdialog"
    onclose={() => {
        if (!isForgetting) onCancel()
    }}
    containerStyle="width: 420px"
    onkeydown={(e: KeyboardEvent) => {
        if (e.key === 'Enter' && !isForgetting) onConfirm()
    }}
>
    {#snippet title()}{tString('askCmdr.forget.title')}{/snippet}
    <p class="confirm-message">{tString('askCmdr.forget.message')}</p>
    {#snippet footer()}
        <Button variant="secondary" disabled={isForgetting} onclick={onCancel}>
            {tString('askCmdr.forget.cancel')}
        </Button>
        <Button variant="danger" disabled={isForgetting} onclick={onConfirm}>
            {tString('askCmdr.forget.confirm')}
        </Button>
    {/snippet}
</ModalDialog>

<style>
    .confirm-message {
        margin: 0;
        font-size: var(--font-size-md);
        line-height: var(--font-line-height-prose);
        color: var(--color-text-secondary);
    }
</style>
