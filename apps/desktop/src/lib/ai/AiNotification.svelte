<script lang="ts">
    import { onMount } from 'svelte'
    import {
        getAiState,
        handleCancel,
        handleDismiss,
        handleDownload,
        handleGotIt,
        handleOptOut,
        initAiState,
    } from './ai-state.svelte'

    const aiState = getAiState()

    onMount(() => {
        let cleanup: (() => void) | undefined
        void initAiState().then((c) => {
            cleanup = c
        })
        return () => cleanup?.()
    })
</script>

{#if aiState.notificationState === 'offer'}
    <div class="ai-notification" role="alert">
        <div class="ai-content">
            <span class="ai-title">AI features available</span>
            <span class="ai-description"
                >Download the AI model ({aiState.modelInfo?.sizeFormatted ?? '~4 GB'}) to enable smart suggestions.</span
            >
            <span class="ai-hint">You can add or remove AI later in settings.</span>
        </div>
        <div class="ai-actions">
            <button class="ai-button primary" onclick={() => void handleDownload()}>Download</button>
            <button class="ai-button secondary" onclick={() => void handleDismiss()}>Not now</button>
            <button class="ai-button tertiary" onclick={() => void handleOptOut()}>I don't want AI</button>
        </div>
    </div>
{:else if aiState.notificationState === 'downloading'}
    <div class="ai-notification" role="status" aria-live="polite">
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
            <button class="ai-button secondary" onclick={() => void handleCancel()}>Cancel</button>
        </div>
    </div>
{:else if aiState.notificationState === 'installing'}
    <div class="ai-notification" role="status" aria-live="polite">
        <div class="ai-content">
            <span class="ai-title">Setting up AI...</span>
            <span class="ai-description">Starting inference server</span>
        </div>
    </div>
{:else if aiState.notificationState === 'ready'}
    <div class="ai-notification" role="alert">
        <div class="ai-content">
            <span class="ai-title">AI ready</span>
            <span class="ai-description">Try creating a new folder (F7) to see AI-powered name suggestions.</span>
        </div>
        <div class="ai-actions">
            <button class="ai-button primary" onclick={handleGotIt}>Got it</button>
        </div>
    </div>
{:else if aiState.notificationState === 'starting'}
    <div class="ai-notification" role="status" aria-live="polite">
        <div class="ai-content">
            <span class="ai-title">AI starting...</span>
            <span class="ai-description">Loading the model, this takes a few seconds</span>
        </div>
    </div>
{/if}

<style>
    .ai-notification {
        position: fixed;
        top: var(--spacing-lg);
        right: var(--spacing-lg);
        background: var(--color-bg-secondary);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-lg);
        padding: var(--spacing-sm) var(--spacing-md);
        display: flex;
        flex-direction: column;
        gap: var(--spacing-sm);
        box-shadow: var(--shadow-md);
        z-index: var(--z-notification);
        max-width: 320px;
    }

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
    }

    .ai-button {
        padding: var(--spacing-xs) var(--spacing-sm);
        border-radius: var(--radius-md);
        font-size: var(--font-size-sm);
        cursor: pointer;
        border: none;
    }

    .ai-button.primary {
        background: var(--color-accent);
        color: #fff;
    }

    .ai-button.primary:hover {
        filter: brightness(1.1);
    }

    .ai-button.secondary {
        background: transparent;
        color: var(--color-text-secondary);
    }

    .ai-button.secondary:hover {
        background: var(--color-bg-tertiary);
    }

    .ai-button.tertiary {
        background: transparent;
        color: var(--color-text-tertiary);
        font-size: var(--font-size-xs);
    }

    .ai-button.tertiary:hover {
        color: var(--color-text-secondary);
    }
</style>
