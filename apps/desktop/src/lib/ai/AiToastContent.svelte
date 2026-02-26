<script lang="ts">
    import {
        getAiState,
        handleCancel,
        handleDismiss,
        handleDownload,
        handleGotIt,
        handleOptOut,
    } from './ai-state.svelte'
    import Button from '$lib/ui/Button.svelte'

    const aiState = getAiState()
</script>

{#if aiState.notificationState === 'offer'}
    <div class="ai-content">
        <span class="ai-title">AI features available</span>
        <span class="ai-description"
            >Download the AI model ({aiState.modelInfo?.sizeFormatted ?? '~4 GB'}) to enable smart suggestions.</span
        >
        <span class="ai-hint">You can add or remove AI later in settings.</span>
    </div>
    <div class="ai-actions">
        <Button variant="primary" size="mini" onclick={() => void handleDownload()}>Download</Button>
        <Button variant="secondary" size="mini" onclick={() => void handleDismiss()}>Not now</Button>
        <button class="tertiary-link" onclick={() => void handleOptOut()}>I don't want AI</button>
    </div>
{:else if aiState.notificationState === 'downloading'}
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
            <span class="ai-progress-text">{aiState.progressText}</span>
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
        <span class="ai-description">Starting inference server</span>
    </div>
{:else if aiState.notificationState === 'ready'}
    <div class="ai-content">
        <span class="ai-title">AI ready</span>
        <span class="ai-description">Try creating a new folder (F7) to see AI-powered name suggestions.</span>
    </div>
    <div class="ai-actions">
        <Button variant="primary" size="mini" onclick={handleGotIt}>Got it</Button>
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
        gap: 4px;
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

    .ai-hint {
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
        margin-top: 4px;
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
        border-radius: 2px;
        overflow: hidden;
        margin-top: 4px;
    }

    .progress-bar-fill {
        height: 100%;
        background: var(--color-accent);
        border-radius: 2px;
        transition: width var(--transition-slow);
    }

    .ai-actions {
        display: flex;
        gap: var(--spacing-xs);
        justify-content: flex-end;
        margin-top: var(--spacing-xs);
    }

    .tertiary-link {
        background: transparent;
        color: var(--color-text-tertiary);
        font-size: var(--font-size-xs);
        border: none;
        padding: var(--spacing-xs) var(--spacing-sm);
        transition: all var(--transition-base);
    }

    .tertiary-link:hover {
        color: var(--color-text-secondary);
    }

    .tertiary-link:focus-visible {
        outline: 2px solid var(--color-accent);
        outline-offset: 1px;
    }
</style>
