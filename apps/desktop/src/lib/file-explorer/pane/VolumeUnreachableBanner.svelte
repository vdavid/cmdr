<script lang="ts">
    import Button from '$lib/ui/Button.svelte'
    import Icon from '$lib/ui/Icon.svelte'

    interface Props {
        originalPath: string
        retrying: boolean
        onRetry: () => void
        onOpenHome?: () => void
        /**
         * SMB give-up variant: shown when the per-volume reconnect manager
         * exhausted its backoff cycle. Replaces "Open home folder" with
         * "Disconnect" (the connection stays alive by default; the user
         * explicitly drops it). The detail line also adapts.
         */
        smbGaveUp?: boolean
        onDisconnect?: () => void
    }

    const {
        originalPath,
        retrying,
        onRetry,
        onOpenHome,
        smbGaveUp = false,
        onDisconnect,
    }: Props = $props()
</script>

<div class="unreachable-banner" role="alert">
    <div class="banner-content">
        <div class="banner-header">
            <span class="warning-icon"><Icon name="triangle-alert" size={16} aria-hidden="true" /></span>
            <span class="banner-message">Couldn't reach {originalPath}</span>
        </div>
        <p class="banner-detail">
            {#if smbGaveUp}
                Cmdr tried reconnecting several times but the server didn't come back. The connection stays available
                for now. Try again, or disconnect to drop it.
            {:else}
                The volume for this path didn't respond in time. It may be a network drive that's currently unavailable.
            {/if}
        </p>
        <div class="banner-actions">
            <Button size="mini" onclick={onRetry} disabled={retrying}>
                {retrying ? 'Retrying...' : 'Retry'}
            </Button>
            {#if smbGaveUp}
                {#if onDisconnect}
                    <Button size="mini" onclick={onDisconnect}>Disconnect</Button>
                {/if}
            {:else if onOpenHome}
                <Button size="mini" onclick={onOpenHome}>Open home folder</Button>
            {/if}
        </div>
    </div>
</div>

<style>
    .unreachable-banner {
        display: flex;
        align-items: center;
        justify-content: center;
        height: 100%;
        padding: var(--spacing-xl);
    }

    .banner-content {
        max-width: 400px;
        padding: var(--spacing-lg);
        background-color: var(--color-warning-bg);
        border: 1px solid color-mix(in srgb, var(--color-warning) 30%, transparent);
        border-radius: var(--radius-lg);
    }

    .banner-header {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
        margin-bottom: var(--spacing-sm);
    }

    .warning-icon {
        display: inline-flex;
        flex-shrink: 0;
        color: var(--color-warning);
    }

    .banner-message {
        font-size: var(--font-size-md);
        font-weight: 500;
        color: var(--color-text-primary);
        word-break: break-all;
    }

    .banner-detail {
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
        line-height: 1.4;
        margin: 0 0 var(--spacing-md) 0;
    }

    .banner-actions {
        display: flex;
        gap: var(--spacing-sm);
    }
</style>
