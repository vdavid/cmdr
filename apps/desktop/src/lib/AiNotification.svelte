<script lang="ts">
    import { onMount } from 'svelte'
    import {
        getAiState,
        handleCancel,
        handleDismiss,
        handleDownload,
        handleGotIt,
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
            <span class="ai-description">Download the AI model (4.6 GB) to enable smart suggestions.</span>
        </div>
        <div class="ai-actions">
            <button class="ai-button primary" onclick={() => void handleDownload()}>Download</button>
            <button class="ai-button secondary" onclick={() => void handleDismiss()}>Not now</button>
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
{/if}

<style>
    .ai-notification {
        position: fixed;
        top: var(--spacing-md);
        right: var(--spacing-md);
        background: var(--color-bg-secondary);
        border: 1px solid var(--color-border);
        border-radius: 8px;
        padding: var(--spacing-sm) var(--spacing-md);
        display: flex;
        flex-direction: column;
        gap: var(--spacing-sm);
        box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
        z-index: 9998;
        max-width: 320px;
    }

    @media (prefers-color-scheme: dark) {
        .ai-notification {
            box-shadow: 0 4px 12px rgba(0, 0, 0, 0.4);
        }
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
        font-size: var(--font-size-xs);
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
        border-radius: 2px;
        overflow: hidden;
        margin-top: 4px;
    }

    .progress-bar-fill {
        height: 100%;
        background: var(--color-accent);
        border-radius: 2px;
        transition: width 0.2s ease;
    }

    .ai-actions {
        display: flex;
        gap: var(--spacing-xs);
        justify-content: flex-end;
    }

    .ai-button {
        padding: var(--spacing-xs) var(--spacing-sm);
        border-radius: 6px;
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
        background: var(--color-button-hover);
    }
</style>
