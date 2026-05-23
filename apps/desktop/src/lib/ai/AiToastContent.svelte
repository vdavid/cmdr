<script lang="ts">
    import Button from '$lib/ui/Button.svelte'
    import { getAiState, handleCancel, handleGotIt } from './ai-state.svelte'

    const aiState = getAiState()
</script>

{#if aiState.notificationState === 'downloading'}
    <div class="ai-content">
        <span class="ai-title">Downloading AI model...</span>
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
            <span class="ai-progress-text">Starting download...</span>
        {/if}
    </div>
    <div class="ai-actions">
        <Button variant="secondary" size="mini" onclick={() => void handleCancel()}>Cancel</Button>
    </div>
{:else if aiState.notificationState === 'installing'}
    <div class="ai-content">
        <span class="ai-title">Setting up AI...</span>
        <span class="ai-description">Starting server</span>
    </div>
{:else if aiState.notificationState === 'ready'}
    <div class="ai-content">
        <span class="ai-title">AI ready</span>
        <span class="ai-description">Try creating a new folder (F7) to see AI-powered name suggestions.</span>
    </div>
    <div class="ai-actions">
        <Button
            variant="primary"
            size="mini"
            onclick={() => {
                handleGotIt()
            }}>Got it</Button
        >
    </div>
{:else if aiState.notificationState === 'starting'}
    <div class="ai-content">
        <span class="ai-title">AI starting...</span>
        <span class="ai-description">Loading the model, this takes a few seconds</span>
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
        gap: var(--spacing-xs);
        justify-content: flex-end;
        margin-top: var(--spacing-xs);
    }
</style>
