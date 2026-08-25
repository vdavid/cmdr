<script lang="ts">
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import Button from '$lib/ui/Button.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import type { BundleWriteBlocker } from '$lib/tauri-commands'

    interface Props {
        /** Which arrangement is keeping the update out of the bundle. */
        blocker: BundleWriteBlocker
        /** Called when the dialog is closed. */
        onClose: () => void
    }

    const { blocker, onClose }: Props = $props()

    // Both arrangements ask the same thing of the user, so only the first paragraph differs:
    // naming the one they're actually in is what makes the instruction land.
    const reason = $derived(
        blocker === 'translocated'
            ? tString('updates.moveToApplicationsDialog.translocated')
            : tString('updates.moveToApplicationsDialog.readOnlyVolume'),
    )
</script>

<ModalDialog
    titleId="dialog-title"
    blur
    dialogId="move-to-applications"
    onclose={onClose}
    containerStyle="min-width: 440px; max-width: 520px"
>
    {#snippet title()}{tString('updates.moveToApplicationsDialog.title')}{/snippet}

    <div class="dialog-body">
        <p class="reason">{reason}</p>
        <p class="how-to">{tString('updates.moveToApplicationsDialog.howTo')}</p>
    </div>

    {#snippet footer()}
        <Button variant="primary" onclick={onClose}>{tString('updates.moveToApplicationsDialog.gotIt')}</Button>
    {/snippet}
</ModalDialog>

<style>
    .reason {
        margin: 0 0 var(--spacing-md);
        font-size: var(--font-size-md);
        color: var(--color-text-secondary);
        line-height: var(--font-line-height-prose);
    }

    .how-to {
        margin: 0 0 var(--spacing-xl);
        font-size: var(--font-size-md);
        color: var(--color-text-primary);
        line-height: var(--font-line-height-prose);
    }
</style>
