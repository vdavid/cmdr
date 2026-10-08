<script lang="ts">
    /**
     * Persistent INFO notice: a share Cmdr tried to take over is staying on the
     * macOS kernel mount, so it works but at a fraction of the speed.
     *
     * The button runs the SAME flow the yellow dot and the breadcrumb's "Connect
     * directly" item run (`connectDirectly`), which owns its own progress and
     * failure toasts. So the outcomes here are only about this notice: connected,
     * handed to the sign-in sheet, or gone (the share unmounted before the press)
     * means it has had its say and goes; still on the OS mount means the button is
     * worth pressing again once the server or the password is fixed, and
     * `connectDirectly` has already said why it didn't work.
     *
     * When this Mac is what blocked the connection (`blockedServer`), the notice
     * says what to switch and leads with the button to it; the retry stays beside
     * it, because the switch works at once (ERR-XGS9X).
     */
    import type { Snippet } from 'svelte'
    import Button from '$lib/ui/Button.svelte'
    import Trans from '$lib/intl/Trans.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import { dismissToast } from '$lib/ui/toast'
    import { openLocalNetworkSettings } from '$lib/tauri-commands'
    import { connectDirectly } from './direct-connect'
    import { directConnectionUnavailableMessage, openLocalNetworkSettingsLabel } from './upgrade-messages'

    interface Props {
        /** Dedup id of this toast; lets the notice retire itself once it's moot. */
        toastId: string
        /** The volume still on the kernel mount, handed straight to the upgrade flow. */
        volumeId: string
        /** The share's name, which is what the sentence names. */
        share: string
        /**
         * Whether pressing the button again could ever work. False when the
         * server says it has no such share: the same ask gets the same answer,
         * so the notice explains instead of offering a retry.
         */
        retryable: boolean
        /**
         * The server's friendly name, set when something on this Mac is what
         * blocked the connection (`blockedByThisMac`). The notice then says what
         * to switch and offers the way there, beside the retry for after.
         */
        blockedServer?: string
    }

    const { toastId, volumeId, share, retryable, blockedServer }: Props = $props()

    let connecting = $state(false)

    async function retry(): Promise<void> {
        if (connecting) return
        connecting = true
        try {
            const outcome = await connectDirectly({ volumeId, shareName: share })
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
        {#if blockedServer !== undefined}
            {directConnectionUnavailableMessage('blockedByThisMac', blockedServer)}
        {:else if retryable}
            <Trans key="fileExplorer.network.osMountFallback.message" snippets={{ shareName }} params={{ share }} />
        {:else}
            <Trans
                key="fileExplorer.network.osMountFallback.shareNotOnServer"
                snippets={{ shareName }}
                params={{ share }}
            />
        {/if}
    </span>
    {#if retryable}
        <div class="actions">
            {#if blockedServer !== undefined}
                <Button variant="primary" size="mini" onclick={() => void openLocalNetworkSettings()}>
                    {openLocalNetworkSettingsLabel()}
                </Button>
            {/if}
            <Button
                variant={blockedServer !== undefined ? 'secondary' : 'primary'}
                size="mini"
                disabled={connecting}
                onclick={() => void retry()}
            >
                {tString('fileExplorer.network.osMountFallback.retry')}
            </Button>
        </div>
    {/if}
</div>

<style>
    .content {
        font-size: var(--font-size-sm);
    }

    .message {
        color: var(--color-text-primary);
        overflow-wrap: anywhere;
    }

    .actions {
        display: flex;
        justify-content: flex-end;
        gap: var(--spacing-xs);
        margin-top: calc(var(--spacing-xs) + var(--spacing-xxs));
    }
</style>
