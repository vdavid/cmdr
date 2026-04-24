<script lang="ts">
    import { onMount, onDestroy } from 'svelte'
    import type { ToastContent, ToastLevel, ToastDismissal } from './toast-store.svelte'
    import { openErrorReportDialog } from '$lib/error-reporter/error-report-flow.svelte'

    interface Props {
        id: string
        content: ToastContent
        level: ToastLevel
        dismissal: ToastDismissal
        timeoutMs: number
        ondismiss: (id: string) => void
    }

    const { id, content, level, dismissal, timeoutMs, ondismiss }: Props = $props()

    let timer: ReturnType<typeof setTimeout> | undefined

    // Error-level toasts that carry a plain-text message get an inline "Send error
    // report…" action. Component-content toasts manage their own actions, so we don't
    // add a second button on top of them.
    const showSendErrorReport = $derived(level === 'error' && typeof content === 'string')

    function handleSendErrorReport() {
        // Pre-fill the user note with the toast text so the user has something to
        // start from. They can edit before sending.
        const initialNote = typeof content === 'string' ? content : ''
        openErrorReportDialog(initialNote)
        ondismiss(id)
    }

    onMount(() => {
        if (dismissal === 'transient') {
            timer = setTimeout(() => {
                ondismiss(id)
            }, timeoutMs)
        }
    })

    onDestroy(() => {
        if (timer !== undefined) {
            clearTimeout(timer)
        }
    })
</script>

<div
    class="toast"
    class:success={level === 'success'}
    class:warn={level === 'warn'}
    class:error={level === 'error'}
    role={level === 'info' || level === 'success' ? 'status' : 'alert'}
>
    <div class="toast-content">
        {#if typeof content === 'string'}
            <span class="toast-message">{content}</span>
            {#if showSendErrorReport}
                <button class="toast-action" onclick={handleSendErrorReport}>
                    Send error report&hellip;
                </button>
            {/if}
        {:else}
            {@const ContentComponent = content}
            <ContentComponent />
        {/if}
    </div>
    <button
        class="toast-close"
        onclick={() => {
            ondismiss(id)
        }}
        aria-label="Dismiss notification"
    >
        <svg
            width="10"
            height="10"
            viewBox="0 0 10 10"
            fill="none"
            stroke="currentColor"
            stroke-width="1.5"
            stroke-linecap="round"
        >
            <path d="M1 1l8 8M9 1l-8 8" />
        </svg>
    </button>
</div>

<style>
    .toast {
        background: var(--color-bg-secondary);
        border: 1px solid var(--color-border-subtle);
        border-left: 3px solid var(--color-text-tertiary);
        border-radius: var(--radius-md);
        box-shadow: var(--shadow-md);
        padding: var(--spacing-md) var(--spacing-lg);
        font-size: var(--font-size-sm);
        max-width: 360px;
        display: flex;
        align-items: start;
        gap: var(--spacing-sm);
    }

    .toast.success {
        border-left-color: var(--color-toast-success-stripe);
        background: var(--color-toast-success-bg);
    }

    .toast.warn {
        border-left-color: var(--color-toast-warn-stripe);
        background: var(--color-toast-warn-bg);
    }

    .toast.error {
        border-left-color: var(--color-error);
        background: var(--color-toast-error-bg);
    }

    .toast-message {
        color: var(--color-text-primary);
        line-height: 1.4;
    }

    .toast-action {
        background: none;
        border: none;
        padding: 0;
        margin-top: var(--spacing-xs);
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
        cursor: default;
        display: block;
    }

    .toast-action:hover {
        color: var(--color-text-secondary);
    }

    .toast-content {
        flex: 1;
        min-width: 0;
    }

    .toast-close {
        flex-shrink: 0;
        background: none;
        border: none;
        color: var(--color-text-tertiary);
        font-size: var(--font-size-sm);
        width: 22px;
        height: 22px;
        display: flex;
        align-items: center;
        justify-content: center;
        border-radius: var(--radius-sm);
        line-height: 1;
        transition:
            background var(--transition-fast),
            color var(--transition-fast);
    }

    .toast-close:hover {
        background: var(--color-bg-tertiary);
        color: var(--color-text-primary);
    }

    @media (prefers-reduced-motion: no-preference) {
        .toast {
            animation: toast-slide-in 0.2s ease-out;
        }

        @keyframes toast-slide-in {
            from {
                opacity: 0;
                transform: translateX(20px);
            }
            to {
                opacity: 1;
                transform: translateX(0);
            }
        }
    }
</style>
