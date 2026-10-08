<script lang="ts">
    /**
     * "Connect directly"'s answer when something on this Mac refused Cmdr's own
     * connection to a server the Mac itself reaches (`blockedByThisMac`): what to
     * switch, and a button that opens it.
     *
     * The fix in ERR-XGS9X was a switch in System Settings, so a sentence alone
     * would leave the person hunting for the pane.
     */
    import Button from '$lib/ui/Button.svelte'
    import { openLocalNetworkSettings } from '$lib/tauri-commands'
    import { directConnectionUnavailableMessage, openLocalNetworkSettingsLabel } from './upgrade-messages'

    interface Props {
        /** The server's friendly name, which the sentence names. */
        server: string
    }

    const { server }: Props = $props()
</script>

<div class="content">
    <span class="message">{directConnectionUnavailableMessage('blockedByThisMac', server)}</span>
    <div class="actions">
        <Button variant="primary" size="mini" onclick={() => void openLocalNetworkSettings()}>
            {openLocalNetworkSettingsLabel()}
        </Button>
    </div>
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
        margin-top: calc(var(--spacing-xs) + var(--spacing-xxs));
    }
</style>
