<script lang="ts">
    /**
     * "Connect directly"'s answer when something on this machine refused Cmdr's own
     * connection to a server the machine itself reaches (`blockedByThisMac`): what to
     * check, and the report offer the toast frame hands down.
     *
     * Where the OS has a Local Network permission (macOS), it also carries a button
     * that opens it: the fix in ERR-XGS9X was a switch in System Settings, so a
     * sentence alone would leave the person hunting for the pane. Elsewhere the
     * sentence names the firewall and there's no pane to open.
     */
    import { hasCapability } from '#lib/platform/index.js'
    import Button from '#lib/ui/Button.svelte'
    import ToastActions from '#lib/ui/toast/ToastActions.svelte'
    import ReportOffer from '#lib/error-reporter/ReportOffer.svelte'
    import type { ReportIncident } from '#lib/error-reporter/report-incident.js'
    import { dismissToast } from '#lib/ui/toast/index.js'
    import { openLocalNetworkSettings } from '#lib/tauri-commands/index.js'
    import { directConnectionUnavailableMessage, openLocalNetworkSettingsLabel } from './upgrade-messages'

    interface Props {
        /** The server's friendly name, which the sentence names. */
        server: string
        toastId: string
        /** The toast's report offer, from the frame (`direct-connect.ts` raises it). */
        report?: ReportIncident
    }

    const { server, toastId, report }: Props = $props()

    const offersLocalNetworkSettings = hasCapability('localNetworkPermission')
</script>

<div class="content">
    <span class="message">{directConnectionUnavailableMessage('blockedByThisMac', server)}</span>
    {#if report || offersLocalNetworkSettings}
        <ToastActions>
            {#if report}
                <ReportOffer
                    incident={report}
                    size="mini"
                    onOpen={() => {
                        dismissToast(toastId)
                    }}
                />
            {/if}
            {#if offersLocalNetworkSettings}
                <Button variant="primary" size="mini" onclick={() => void openLocalNetworkSettings()}>
                    {openLocalNetworkSettingsLabel()}
                </Button>
            {/if}
        </ToastActions>
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
</style>
