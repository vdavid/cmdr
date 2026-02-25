<script lang="ts">
    import type { Snippet } from 'svelte'
    import { onMount, onDestroy } from 'svelte'

    type ToastLevel = 'info' | 'warn' | 'error'
    type ToastDismissal = 'transient' | 'persistent'

    interface Props {
        id: string
        content: Snippet | string
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
            {@render content()}
        {/if}
    </div>
    <button
        class="toast-close"
        onclick={() => {
            ondismiss(id)
        }}
        aria-label="Dismiss notification"
    >
        &times;
    </button>
</div>

<style>
    .toast {
        background: var(--color-bg-secondary);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-lg);
        box-shadow: var(--shadow-md);
        padding: var(--spacing-sm) var(--spacing-md);
        font-size: var(--font-size-sm);
        max-width: 360px;
        display: flex;
        align-items: start;
        gap: var(--spacing-sm);
    }

    .toast.warn {
        border-color: color-mix(in srgb, var(--color-warning) 50%, transparent);
        background: var(--color-warning-bg);
    }

    .toast.error {
        border-color: var(--color-error-border);
        background: var(--color-error-bg);
    }

    .toast-message {
        color: var(--color-text-primary);
        line-height: 1.4;
    }

    .error .toast-message {
        color: var(--color-error);
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
        font-size: var(--font-size-md);
        cursor: pointer;
        padding: 2px var(--spacing-xs);
        border-radius: var(--radius-sm);
        line-height: 1;
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
