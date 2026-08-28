<script lang="ts">
    import { onMount, onDestroy } from 'svelte'
    import type { ToastContent, ToastLevel, ToastDismissal } from './toast-store.svelte'
    import { HOVER_LEAVE_GRACE_MS } from './toast-store.svelte'
    import { openErrorReportDialog } from '$lib/error-reporter/error-report-flow.svelte'
    import { tooltip } from '$lib/tooltip/tooltip'
    import Button from '$lib/ui/Button.svelte'
    import Icon from '$lib/ui/Icon.svelte'
    import { tString } from '$lib/intl/messages.svelte'

    interface Props {
        id: string
        content: ToastContent
        level: ToastLevel
        dismissal: ToastDismissal
        timeoutMs: number
        closeTooltip?: string
        /**
         * Props forwarded to a component-shaped `content`. Merged with the
         * toast id under `toastId` so the content component can self-dismiss
         * without a module-state bridge. Ignored for string content.
         */
        // eslint-disable-next-line @typescript-eslint/no-explicit-any -- mirrors ToastOptions.props
        contentProps?: Record<string, any>
        /** Optional per-toast max-width override in px (default 360). */
        widthPx?: number
        /**
         * Suppress the inline "Send error report…" action this toast would otherwise get.
         * Set on toasts that are themselves about error reporting (the send-failure toast),
         * so a failed send doesn't offer to re-run the same flow.
         */
        suppressErrorReportAction?: boolean
        /** Called when the auto-dismiss timer fires for transient toasts. */
        onTimeout: (id: string) => void
        /** Called when the user clicks the X button or the inline action. */
        onUserDismiss: (id: string) => void
    }

    const {
        id,
        content,
        level,
        dismissal,
        timeoutMs,
        closeTooltip,
        contentProps,
        widthPx,
        suppressErrorReportAction = false,
        onTimeout,
        onUserDismiss,
    }: Props = $props()

    // Auto-dismiss timer for transient toasts.
    //
    // The rule: a toast hides at `max(its natural deadline, the moment the
    // pointer left it + HOVER_LEAVE_GRACE_MS)`. The natural deadline is
    // `mountedAt + timeoutMs` in wall-clock time, and hovering neither pauses
    // nor extends that clock. What hovering guarantees is the grace tail after
    // the pointer leaves, so a toast can never vanish out from under a cursor
    // that's still on it, and never snaps away the instant the mouse drifts
    // off. Don't reintroduce a paused countdown: a toast hovered for a
    // minute is meant to go one second after the pointer leaves, not to get
    // its leftover seconds back.
    //
    // Persistent toasts never get a timer; the hover handlers no-op for them.
    let timer: ReturnType<typeof setTimeout> | undefined
    let naturalDeadline = 0

    // Error-level toasts that carry a plain-text message get an inline "Send error
    // report…" action. Component-content toasts manage their own actions, so we don't
    // add a second button on top of them. A toast that opts out via
    // `suppressErrorReportAction` (the send-failure toast itself) never shows it, so a
    // failed send doesn't offer to re-run the flow that just failed.
    const showSendErrorReport = $derived(
        !suppressErrorReportAction && level === 'error' && typeof content === 'string',
    )

    function handleSendErrorReport() {
        // Pre-fill the user note with the toast text so the user has something to
        // start from. They can edit before sending.
        const initialNote = typeof content === 'string' ? content : ''
        openErrorReportDialog(initialNote)
        onUserDismiss(id)
    }

    function clearTimer() {
        if (timer !== undefined) {
            clearTimeout(timer)
            timer = undefined
        }
    }

    function armTimer(ms: number) {
        clearTimer()
        timer = setTimeout(() => {
            onTimeout(id)
        }, ms)
    }

    function handlePointerEnter() {
        if (dismissal !== 'transient') return
        clearTimer()
    }

    function handlePointerLeave() {
        if (dismissal !== 'transient') return
        armTimer(Math.max(naturalDeadline - Date.now(), HOVER_LEAVE_GRACE_MS))
    }

    onMount(() => {
        if (dismissal === 'transient') {
            naturalDeadline = Date.now() + timeoutMs
            armTimer(timeoutMs)
        }
    })

    onDestroy(() => {
        clearTimer()
    })
</script>

<div
    class="toast"
    class:info={level === 'info'}
    class:success={level === 'success'}
    class:warn={level === 'warn'}
    class:error={level === 'error'}
    style={widthPx ? `max-width: ${String(widthPx)}px` : undefined}
    role={level === 'default' || level === 'info' || level === 'success' ? 'status' : 'alert'}
    onpointerenter={handlePointerEnter}
    onpointerleave={handlePointerLeave}
>
    <div class="toast-content">
        {#if typeof content === 'string'}
            <span class="toast-message">{content}</span>
            {#if showSendErrorReport}
                <div class="toast-actions">
                    <Button size="mini" variant="secondary" onclick={handleSendErrorReport}>
                        {tString('ui.toast.sendErrorReport')}
                    </Button>
                </div>
            {/if}
        {:else}
            {@const ContentComponent = content}
            {#if contentProps}
                <!-- Component toasts that opt into the prop-forwarding shape get
                     the toast id appended for self-dismiss. Existing toasts that
                     don't pass `props` to `addToast` keep their zero-prop shape so
                     they don't see Svelte's unknown-prop warning. -->
                <ContentComponent {...contentProps} toastId={id} />
            {:else}
                <ContentComponent />
            {/if}
        {/if}
    </div>
    <button
        class="toast-close"
        onclick={() => {
            onUserDismiss(id)
        }}
        use:tooltip={closeTooltip}
        aria-label={tString('ui.toast.dismissAria')}
    >
        <Icon name="x" size={10} />
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

    .toast.info {
        border-left-color: var(--color-toast-info-stripe);
        background: var(--color-toast-info-bg);
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
    }

    .toast-actions {
        display: flex;
        justify-content: flex-end;
        gap: var(--spacing-sm);
        margin-top: var(--spacing-md);
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
        line-height: var(--font-line-height-flat);
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
