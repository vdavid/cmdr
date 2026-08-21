<script lang="ts">
    /**
     * Persistent INFO notice: a share Cmdr tried to take over is staying on the
     * macOS kernel mount, so it works but at a fraction of the speed.
     *
     * The button runs the SAME flow the yellow dot and the breadcrumb's "Connect
     * directly" item run (`connectDirectly`), which owns its own progress and
     * failure toasts. So the three outcomes here are only about this notice:
     * connected or handed to the credential form means it has had its say and
     * goes; still on the OS mount means the button is worth pressing again once
     * the server or the password is fixed, and `connectDirectly` has already said
     * why it didn't work.
     */
    import type { Snippet } from 'svelte'
    import Button from '$lib/ui/Button.svelte'
    import Trans from '$lib/intl/Trans.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import { dismissToast } from '$lib/ui/toast'
    import { connectDirectly } from './direct-connect'
    import { promptForSmbCredentials } from './smb-login-hosts'

    interface Props {
        /** Dedup id of this toast; lets the notice retire itself once it's moot. */
        toastId: string
        /** The volume still on the kernel mount, handed straight to the upgrade flow. */
        volumeId: string
        /** The share's name, which is what the sentence names. */
        share: string
    }

    const { toastId, volumeId, share }: Props = $props()

    let connecting = $state(false)

    async function retry(): Promise<void> {
        if (connecting) return
        connecting = true
        try {
            const outcome = await connectDirectly(volumeId, promptForSmbCredentials)
            if (outcome !== 'stillOnOsMount') dismissToast(toastId)
        } finally {
            connecting = false
        }
    }
</script>

{#snippet shareName(children: Snippet)}
    <strong>{@render children()}</strong>
{/snippet}

<div class="content">
    <span class="message">
        <Trans key="fileExplorer.network.osMountFallback.message" snippets={{ shareName }} params={{ share }} />
    </span>
    <div class="actions">
        <Button variant="primary" size="mini" disabled={connecting} onclick={() => void retry()}>
            {tString('fileExplorer.network.osMountFallback.retry')}
        </Button>
    </div>
</div>

<style>
    .content {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
        font-size: var(--font-size-sm);
    }

    .message {
        color: var(--color-text-primary);
        overflow-wrap: anywhere;
    }

    .actions {
        display: flex;
        justify-content: flex-end;
        margin-top: var(--spacing-xxs);
    }
</style>
