<script lang="ts">
    /**
     * AiSearchRow — AI prompt input + Ask AI button + caveat + status/error display.
     *
     * Shown when AI search is enabled. The parent orchestrator owns the actual AI search
     * execution; this component just renders the input UI and fires callbacks.
     */
    interface Props {
        /** Bindable ref to the AI prompt input element (parent needs it for focus management). */
        inputElement: HTMLInputElement | undefined
        aiPrompt: string
        /** Pre-built input handler for the AI prompt field. */
        onPromptInput: (e: Event) => void
        onAiSearch: (query: string) => void
        disabled: boolean
        caveatText: string
        aiStatus: string
        aiError: string
    }

    /* eslint-disable prefer-const -- $bindable() requires `let` destructuring */
    let {
        inputElement = $bindable(),
        aiPrompt,
        onPromptInput,
        onAiSearch,
        disabled,
        caveatText,
        aiStatus,
        aiError,
    }: Props = $props()
    /* eslint-enable prefer-const */
</script>

<div class="input-row ai-prompt-row">
    <span class="row-label ai-label">AI</span>
    <input
        bind:this={inputElement}
        type="text"
        class="name-input"
        placeholder="Describe what you're looking for..."
        value={aiPrompt}
        oninput={onPromptInput}
        {disabled}
        aria-label="Natural language search query"
        spellcheck="false"
        autocomplete="off"
        autocapitalize="off"
    />
    <button
        class="action-button ai-active"
        onclick={() => {
            onAiSearch(aiPrompt)
        }}
        disabled={disabled || !aiPrompt.trim()}
        title="Ask AI (⌘Enter)"
    >
        Ask AI
    </button>
</div>
{#if caveatText}
    <div class="caveat-row">{caveatText}</div>
{/if}

{#if aiStatus}
    <div class="ai-status">{aiStatus}</div>
{/if}
{#if aiError}
    <div class="ai-error">{aiError}</div>
{/if}

<style>
    .input-row {
        display: flex;
        align-items: center;
        padding: var(--spacing-sm) var(--spacing-md);
        border-bottom: 1px solid var(--color-border-strong);
        background: var(--color-bg-primary);
        gap: var(--spacing-sm);
    }

    /* AI prompt row styling — subtle left accent border */
    .ai-prompt-row {
        border-left: 2px solid var(--color-accent);
        background: var(--color-bg-secondary);
        animation: slide-down 150ms ease-out;
    }

    @keyframes slide-down {
        from {
            max-height: 0;
            opacity: 0;
            padding-top: 0;
            padding-bottom: 0;
        }

        to {
            max-height: 60px;
            opacity: 1;
        }
    }

    @media (prefers-reduced-motion: reduce) {
        .ai-prompt-row {
            animation: none;
        }
    }

    .row-label {
        flex-shrink: 0;
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
        font-weight: 500;
        user-select: none;
    }

    .ai-label {
        color: var(--color-accent-text);
    }

    .name-input {
        flex: 1;
        font-size: var(--font-size-md);
        border: 1px solid transparent;
        background: transparent;
        color: var(--color-text-primary);
        outline: none;
        min-width: 0;
    }

    .name-input:focus {
        border-color: var(--color-accent);
        box-shadow: var(--shadow-focus);
    }

    .name-input::placeholder {
        color: var(--color-text-tertiary);
    }

    /* Shared button style for Ask AI */
    .action-button {
        flex-shrink: 0;
        padding: var(--spacing-xxs) var(--spacing-sm);
        font-size: var(--font-size-sm);
        border: 1px solid var(--color-border-strong);
        border-radius: var(--radius-sm);
        background: var(--color-bg-secondary);
        color: var(--color-text-secondary);
        white-space: nowrap;
    }

    .action-button:disabled {
        opacity: 0.5;
        cursor: not-allowed;
    }

    .action-button:not(:disabled):hover {
        background: var(--color-bg-tertiary);
    }

    .action-button.ai-active {
        background: var(--color-accent-subtle);
        border-color: var(--color-accent);
        color: var(--color-text-primary);
    }

    .caveat-row {
        padding: var(--spacing-xs) var(--spacing-md);
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
        overflow: hidden;
        white-space: nowrap;
        text-overflow: ellipsis;
    }

    .ai-status {
        padding: var(--spacing-xs) var(--spacing-md);
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }

    .ai-error {
        padding: var(--spacing-xs) var(--spacing-md);
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }
</style>
