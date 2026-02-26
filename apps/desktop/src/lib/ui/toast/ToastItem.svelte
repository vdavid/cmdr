<script lang="ts">
    import { onMount, onDestroy } from 'svelte'
    import type { ToastContent, ToastLevel, ToastDismissal } from './toast-store.svelte'

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
    class:warn={level === 'warn'}
    class:error={level === 'error'}
    role={level === 'info' ? 'status' : 'alert'}
>
    <div class="toast-content">
        {#if typeof content === 'string'}
            <span class="toast-message">{content}</span>
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
