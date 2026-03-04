<script lang="ts">
    import Button from '$lib/ui/Button.svelte'

    interface Props {
        originalPath: string
        retrying: boolean
        onRetry: () => void
        onOpenHome: () => void
    }

    const { originalPath, retrying, onRetry, onOpenHome }: Props = $props()
</script>

<div class="unreachable-banner" role="alert">
    <div class="banner-content">
        <div class="banner-header">
            <svg class="warning-icon" width="16" height="16" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
                <path
                    d="M8 1a.75.75 0 0 1 .65.375l6.25 10.75A.75.75 0 0 1 14.25 13H1.75a.75.75 0 0 1-.65-1.125L7.35 1.375A.75.75 0 0 1 8 1zm0 4a.75.75 0 0 0-.75.75v3a.75.75 0 0 0 1.5 0v-3A.75.75 0 0 0 8 5zm0 6.5a.75.75 0 1 0 0-1.5.75.75 0 0 0 0 1.5z"
                />
            </svg>
            <span class="banner-message">Couldn't reach {originalPath}</span>
        </div>
        <p class="banner-detail">
            The volume for this path didn't respond in time. It may be a network drive that's currently unavailable.
        </p>
        <div class="banner-actions">
            <Button size="mini" onclick={onRetry} disabled={retrying}>
                {retrying ? 'Retrying...' : 'Retry'}
            </Button>
            <Button size="mini" onclick={onOpenHome}>Open home folder</Button>
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
