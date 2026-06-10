<script lang="ts">
    import { onMount, onDestroy } from 'svelte'
    import type { ToastContent, ToastLevel, ToastDismissal } from './toast-store.svelte'
    import { HOVER_LEAVE_GRACE_MS } from './toast-store.svelte'
    import { openErrorReportDialog } from '$lib/error-reporter/error-report-flow.svelte'
    import { tooltip } from '$lib/tooltip/tooltip'
    import Button from '$lib/ui/Button.svelte'

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
        onTimeout,
        onUserDismiss,
    }: Props = $props()

    // Auto-dismiss timer plus hover-pause bookkeeping.
    //
    // For a transient toast, `startTimer(ms)` arms the auto-dismiss timer and
    // records the duration (`armedForMs`) and the wall-clock moment it was
    // armed (`timerStartedAt`). `activeElapsedMs` accumulates the un-hovered
    // visibility time so we can tell the difference between "user hovered
    // during the natural visibility window" and "user hovered before the
    // toast got any unhovered moment."
    //
    // `pointerenter` clears the timer, captures how much time was left on it
    // (`pausedRemainingMs`), and adds the time that already passed to
    // `activeElapsedMs`. `pointerleave` decides what to do next:
    //  - If the timer made any progress before being paused (the user saw
    //    the toast for a moment before hovering), resume with the captured
    //    remainder. The natural visibility window is preserved across the
    //    hover.
    //  - If no progress was made (the pointer entered before any unhovered
    //    visibility), the toast already lived past its natural expiry while
    //    hovered: start a `HOVER_LEAVE_GRACE_MS` grace timer so an accidental
    //    cursor exit doesn't snap it away.
    //
    // Persistent toasts never get a timer; the hover handlers no-op for them.
    let timer: ReturnType<typeof setTimeout> | undefined
    let timerStartedAt: number | undefined
    let armedForMs: number | undefined
    let pausedRemainingMs: number | undefined
    let activeElapsedMs = 0
    let isHovered = false

    // Error-level toasts that carry a plain-text message get an inline "Send error
    // report…" action. Component-content toasts manage their own actions, so we don't
    // add a second button on top of them.
    const showSendErrorReport = $derived(level === 'error' && typeof content === 'string')

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
        timerStartedAt = undefined
        armedForMs = undefined
    }

    function startTimer(ms: number) {
        clearTimer()
        armedForMs = ms
        timerStartedAt = Date.now()
        timer = setTimeout(() => {
            onTimeout(id)
        }, ms)
    }

    function handlePointerEnter() {
        if (dismissal !== 'transient') return
        if (isHovered) return
        isHovered = true
        if (timer === undefined || timerStartedAt === undefined || armedForMs === undefined) {
            // Timer already fired (or never armed): nothing left to pause.
            pausedRemainingMs = 0
            return
        }
        const elapsed = Date.now() - timerStartedAt
        activeElapsedMs += elapsed
        pausedRemainingMs = Math.max(0, armedForMs - elapsed)
        clearTimer()
    }

    function handlePointerLeave() {
        if (dismissal !== 'transient') return
        if (!isHovered) return
        isHovered = false
        // The toast had a chance to be read unhovered iff `activeElapsedMs > 0`.
        // - Some unhovered time happened → resume with the captured remainder
        //   (preserves the natural visibility window across the hover).
        // - No unhovered time at all (hover started immediately) → the only
        //   reading window was during hover, so on leave give the grace
        //   period for an accidental cursor exit.
        if (activeElapsedMs > 0 && pausedRemainingMs !== undefined && pausedRemainingMs > 0) {
            startTimer(pausedRemainingMs)
        } else {
            startTimer(HOVER_LEAVE_GRACE_MS)
        }
        pausedRemainingMs = undefined
    }

    onMount(() => {
        if (dismissal === 'transient') {
            startTimer(timeoutMs)
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
                        Send error report&hellip;
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
        line-height: 1.4;
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
