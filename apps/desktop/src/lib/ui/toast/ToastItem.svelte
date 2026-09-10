<script lang="ts">
    import { onMount, onDestroy } from 'svelte'
    import type { ToastContent, ToastLevel, ToastDismissal } from './toast-store.svelte'
    import { HOVER_LEAVE_GRACE_MS } from './toast-store.svelte'
    import { formatToastAge, msUntilToastAgeChanges } from './toast-age'
    import ToastLevelIcon from './ToastLevelIcon.svelte'
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
        /** When the toast's current content was posted (`Toast.postedAt`); the age label counts from it. */
        postedAt: number
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
        postedAt,
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

    // Age label ("2m ago"). `now` moves only when the label would change: one timer, armed for
    // the next whole minute (or hour) and re-armed each time it fires, so an idle toast costs one
    // wake-up a minute at most. `now` last moved at the previous tick, so a same-id re-add can
    // re-stamp `postedAt` past it; that counts as age zero, and the next tick lands a minute
    // after the new post.
    let now = $state(Date.now())
    const ageLabel = $derived(formatToastAge(now - postedAt))

    $effect(() => {
        const ageTimer = setTimeout(
            () => {
                now = Date.now()
            },
            msUntilToastAgeChanges(Math.max(now, postedAt) - postedAt),
        )
        return () => {
            clearTimeout(ageTimer)
        }
    })

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
    <ToastLevelIcon {level} />
    <div class="toast-main">
        <div class="toast-line">
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
            {#if ageLabel !== null}
                <!-- `aria-hidden`: the toast is a live region, so a label that changes every
                     minute would have a screen reader announce the whole toast again. -->
                <span class="toast-age" aria-hidden="true">{ageLabel}</span>
            {/if}
        </div>
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
        /* The containing block for the close button pinned to the corner. */
        position: relative;
        display: flex;
        align-items: flex-start;
        gap: var(--spacing-md);
        max-width: 360px;
        /* The right inset also clears the pinned close button. */
        padding: var(--spacing-toast) calc(var(--spacing-toast) + 30px) var(--spacing-toast) var(--spacing-toast);
        background: var(--color-toast-default-bg);
        border: 1px solid var(--color-toast-default-border);
        border-radius: var(--radius-toast);
        box-shadow: var(--shadow-toast);
        font-size: var(--font-size-sm);
    }

    .toast.info {
        background: var(--color-toast-info-bg);
        border-color: var(--color-toast-info-border);
    }

    .toast.success {
        background: var(--color-toast-success-bg);
        border-color: var(--color-toast-success-border);
    }

    .toast.warn {
        background: var(--color-toast-warn-bg);
        border-color: var(--color-toast-warn-border);
    }

    .toast.error {
        background: var(--color-toast-error-bg);
        border-color: var(--color-toast-error-border);
    }

    /* At least as tall as the level icon, with the text centered in that height, so a one-line
       toast sits level with its icon while a longer one starts at the icon's top. */
    .toast-main {
        flex: 1;
        min-width: 0;
        min-height: 28px;
        display: flex;
        flex-direction: column;
        justify-content: center;
    }

    /* The age label sits on the content's first line, whatever that content is. */
    .toast-line {
        display: flex;
        align-items: baseline;
        gap: var(--spacing-sm);
    }

    .toast-content {
        flex: 1;
        min-width: 0;
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

    .toast-age {
        flex-shrink: 0;
        color: var(--color-text-tertiary);
        font-variant-numeric: tabular-nums;
        white-space: nowrap;
    }

    /* Pinned the same distance from the top and right edges, centered on the first text line. */
    .toast-close {
        position: absolute;
        top: calc(var(--spacing-toast) - 3px);
        right: calc(var(--spacing-toast) - 3px);
        background: none;
        border: none;
        color: var(--color-text-tertiary);
        font-size: var(--font-size-sm);
        width: 22px;
        height: 22px;
        display: flex;
        align-items: center;
        justify-content: center;
        border-radius: var(--radius-full);
        line-height: var(--font-line-height-flat);
        transition:
            background var(--transition-fast),
            color var(--transition-fast);
    }

    .toast-close:hover {
        background: var(--color-tint-hover);
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
