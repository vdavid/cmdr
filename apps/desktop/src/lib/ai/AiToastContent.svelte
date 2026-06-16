<script lang="ts">
    import Button from '$lib/ui/Button.svelte'
    import { getAiState, handleCancel, handleGotIt } from './ai-state.svelte'
    import { tString } from '$lib/intl/messages.svelte'

    const aiState = getAiState()
</script>

{#if aiState.notificationState === 'downloading'}
    <div class="ai-content">
        <span class="ai-title">{tString('ai.toast.downloadingTitle')}</span>
        {#if aiState.downloadProgress && aiState.downloadProgress.totalBytes > 0}
            <div class="progress-bar-container">
                <div
                    class="progress-bar-fill"
                    style="width: {String(
                        Math.round(
                            (aiState.downloadProgress.bytesDownloaded / aiState.downloadProgress.totalBytes) * 100,
                        ),
                    )}%"
                ></div>
            </div>
            <!-- eslint-disable-next-line svelte/no-at-html-tags -- progressText is built from typed numbers via formatBytes + tier classes; no user input. -->
            <span class="ai-progress-text">{@html aiState.progressText}</span>
        {:else}
            <span class="ai-progress-text">{tString('ai.toast.startingDownload')}</span>
        {/if}
    </div>
    <div class="ai-actions">
        <Button variant="secondary" size="mini" onclick={() => void handleCancel()}>{tString('ai.toast.cancel')}</Button>
    </div>
{:else if aiState.notificationState === 'installing'}
    <div class="ai-content">
        <span class="ai-title">{tString('ai.toast.installingTitle')}</span>
        <span class="ai-description">{tString('ai.toast.installingDescription')}</span>
    </div>
{:else if aiState.notificationState === 'ready'}
    <div class="ai-content">
        <span class="ai-title">{tString('ai.toast.readyTitle')}</span>
        <span class="ai-description">{tString('ai.toast.readyDescription')}</span>
    </div>
    <div class="ai-actions">
        <Button
            variant="primary"
            size="mini"
            onclick={() => {
                handleGotIt()
            }}>{tString('ai.toast.gotIt')}</Button
        >
    </div>
{:else if aiState.notificationState === 'starting'}
    <div class="ai-content">
        <span class="ai-title">{tString('ai.toast.startingTitle')}</span>
        <span class="ai-description">{tString('ai.toast.startingDescription')}</span>
    </div>
{/if}

<style>
    .ai-content {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
    }

    .ai-title {
        font-size: var(--font-size-sm);
        font-weight: 600;
        color: var(--color-text-primary);
    }

    .ai-description {
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
        line-height: 1.4;
    }

    .ai-progress-text {
        font-size: var(--font-size-xs);
        color: var(--color-text-secondary);
        font-variant-numeric: tabular-nums;
    }

    .progress-bar-container {
        width: 100%;
        height: 4px;
        background: var(--color-bg-tertiary);
        border-radius: var(--radius-xs);
        overflow: hidden;
        margin-top: var(--spacing-xs);
    }

    .progress-bar-fill {
        height: 100%;
        background: var(--color-accent);
        border-radius: var(--radius-xs);
        transition: width var(--transition-slow);
    }

    .ai-actions {
        display: flex;
        justify-content: flex-end;
        gap: var(--spacing-sm);
        margin-top: var(--spacing-md);
    }
</style>
