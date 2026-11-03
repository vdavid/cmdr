<script lang="ts">
    /**
     * One refused access attempt, told to the person (#457): what Cmdr turned away, that nothing
     * got in, the innocent cause beside the serious one, and the move they have. One component for
     * every `AccessNotice` kind, switched on the kind, so the family reads alike; the update case
     * shares the update toasts' look (`#lib/updates/UpdateToastContent.svelte`).
     *
     * ❌ Never names the web page's origin (a site the person visits) or a path: those are the
     * security log's.
     */
    import type { AccessNotice } from '#lib/ipc/bindings.js'
    import type { ReportIncident } from '#lib/error-reporter/report-incident.js'
    import Button from '#lib/ui/Button.svelte'
    import ToastActions from '#lib/ui/toast/ToastActions.svelte'
    import ReportOffer from '#lib/error-reporter/ReportOffer.svelte'
    import { dismissToast } from '#lib/ui/toast/index.js'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { formatInteger } from '#lib/intl/number-format.js'
    import { getAppLogger } from '#lib/logging/logger.js'
    import { openSettingsWindow } from '#lib/settings/settings-window.js'
    import { getVolumes } from '#lib/stores/volume-store.svelte.js'
    import { disconnectSmbVolume, openExternalUrl, reconnectVolume } from '#lib/tauri-commands/index.js'
    import { dismissSecurityNotice } from './dismissed-notices'

    interface Props {
        /** Dedup id of this toast; lets the notice get out of its own way. */
        toastId: string
        notice: AccessNotice
        /** How many times since Cmdr started. The backend re-raises as it grows. */
        count: number
        /** The update case's report offer, handed down by the toast frame. */
        report?: ReportIncident
    }

    const { toastId, notice, count, report }: Props = $props()

    const log = getAppLogger('securityNotices')

    const MCP_SETTINGS_SECTION = ['AI', 'MCP server']
    const WEBSITE = 'https://getcmdr.com'

    /** The share's name as the volume list shows it, or `null` once it's gone from the list. */
    const serverName = $derived(
        notice.kind === 'smbSignatureRefused'
            ? (getVolumes().find((volume) => volume.id === notice.volumeId)?.name ?? null)
            : null,
    )

    const title = $derived.by(() => {
        switch (notice.kind) {
            case 'mcpForeignOrigin':
                return tString('securityNotices.mcpForeignOrigin.title')
            case 'smbSignatureRefused':
                return serverName === null
                    ? tString('securityNotices.smbSignatureRefused.titleUnnamed')
                    : tString('securityNotices.smbSignatureRefused.title', { name: serverName })
            case 'updateNotGenuine':
                return tString('securityNotices.updateNotGenuine.title')
        }
    })

    const body = $derived.by(() => {
        switch (notice.kind) {
            case 'mcpForeignOrigin':
                return tString('securityNotices.mcpForeignOrigin.body')
            case 'smbSignatureRefused':
                return tString('securityNotices.smbSignatureRefused.body')
            case 'updateNotGenuine':
                return tString('securityNotices.updateNotGenuine.body')
        }
    })

    async function openMcpSettings(): Promise<void> {
        try {
            await openSettingsWindow('security-notice-toast', MCP_SETTINGS_SECTION)
        } catch (error) {
            // The toast stays up, so the button is still there to try again.
            log.warn("Couldn't open Settings from a security notice: {error}", { error })
            return
        }
        dismissToast(toastId)
    }

    /** Builds a fresh, freshly signed session: the refused one was already dropped. */
    function reconnect(volumeId: string): void {
        dismissToast(toastId)
        void reconnectVolume(volumeId).catch((error: unknown) => {
            log.warn('Reconnecting {volumeId} from a security notice didn’t take: {error}', {
                volumeId,
                error,
            })
        })
    }

    function disconnect(volumeId: string): void {
        dismissToast(toastId)
        void disconnectSmbVolume(volumeId).catch((error: unknown) => {
            log.warn('Disconnecting {volumeId} from a security notice didn’t take: {error}', {
                volumeId,
                error,
            })
        })
    }

    async function openWebsite(): Promise<void> {
        try {
            await openExternalUrl(WEBSITE)
        } catch (error) {
            log.warn("Couldn't open getcmdr.com from a security notice: {error}", { error })
        }
    }
</script>

<div class="notice">
    <span class="notice-headline">{title}</span>
    <span class="notice-detail">{body}</span>
    {#if count > 1}
        <span class="notice-count">
            {tString('securityNotices.count', { count, countText: formatInteger(count) })}
        </span>
    {/if}
    <ToastActions>
        {#if notice.kind === 'mcpForeignOrigin'}
            <Button
                variant="secondary"
                size="mini"
                onclick={() => {
                    dismissSecurityNotice(notice)
                }}>{tString('securityNotices.dismiss')}</Button
            >
            <Button variant="primary" size="mini" onclick={() => void openMcpSettings()}>
                {tString('securityNotices.mcpForeignOrigin.openSettings')}
            </Button>
        {:else if notice.kind === 'smbSignatureRefused'}
            {@const volumeId = notice.volumeId}
            <Button
                variant="secondary"
                size="mini"
                onclick={() => {
                    disconnect(volumeId)
                }}>{tString('securityNotices.smbSignatureRefused.disconnect')}</Button
            >
            <Button
                variant="primary"
                size="mini"
                onclick={() => {
                    reconnect(volumeId)
                }}>{tString('securityNotices.smbSignatureRefused.reconnect')}</Button
            >
        {:else}
            {#if report}
                <ReportOffer
                    incident={report}
                    size="mini"
                    onOpen={() => {
                        dismissToast(toastId)
                    }}
                />
            {/if}
            <Button variant="primary" size="mini" onclick={() => void openWebsite()}>
                {tString('securityNotices.updateNotGenuine.openWebsite')}
            </Button>
        {/if}
    </ToastActions>
</div>

<style>
    /* The update toasts' look (`UpdateToastContent.svelte`), so the family reads alike. */
    .notice-headline {
        font-size: var(--font-size-sm);
        font-weight: 600;
        color: var(--color-text-primary);
    }

    .notice-detail {
        font-size: var(--font-size-sm);
        color: var(--color-text-primary);
    }

    .notice-count {
        font-size: var(--font-size-xs);
        color: var(--color-text-secondary);
        font-variant-numeric: tabular-nums;
    }
</style>
