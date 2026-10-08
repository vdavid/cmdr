<script lang="ts">
    import { onDestroy, onMount, untrack } from 'svelte'
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
         * Props forwarded to a component-shaped `content`. The component always
         * gets the toast id as `toastId` too, with or without these, so it can
         * close itself. Ignored for string content.
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
    // The clock counts from the later of mount and the latest post, so a same-id
    // re-raise (a new `postedAt`, which can also change `dismissal` and
    // `timeoutMs`) restarts it. Persistent toasts never get a timer, and a toast
    // re-raised under the pointer waits for the pointer to leave, same as one
    // hovered the whole time.
    const mountedAt = Date.now()
    let timer: ReturnType<typeof setTimeout> | undefined
    let naturalDeadline = 0
    let hovered = false
    let toastElement: HTMLDivElement | undefined = $state()
    let focusReturnTarget: HTMLElement | null = null
    let ownedFocus = false

    onMount(() => {
        const active = document.activeElement
        focusReturnTarget = active instanceof HTMLElement && active !== document.body ? active : null
        document.addEventListener('focusin', observeDocumentFocus)
        return () => { document.removeEventListener('focusin', observeDocumentFocus); }
    })

    function rememberFocusOrigin(event: FocusEvent): void {
        ownedFocus = true
        const origin = event.relatedTarget
        if (origin instanceof HTMLElement && origin !== document.body && !toastElement?.contains(origin)) {
            focusReturnTarget = origin
        }
    }

    function rememberPointerFocusOrigin(): void {
        const active = document.activeElement
        if (active instanceof HTMLElement && active !== document.body && !toastElement?.contains(active)) {
            focusReturnTarget = active
            ownedFocus = true
        }
    }

    function observeDocumentFocus(event: FocusEvent): void {
        const destination = event.target
        if (ownedFocus && destination instanceof HTMLElement && !toastElement?.contains(destination)) ownedFocus = false
    }

    onDestroy(() => {
        if (!ownedFocus) return
        const target = focusReturnTarget
        queueMicrotask(() => {
            if (target?.isConnected && document.activeElement === document.body) target.focus()
        })
    })

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

    // Countdown ring around the X: it empties exactly when the dismiss timer fires. Under the
    // pointer it freezes where it is (the toast won't go while hovered), and on leave it drains
    // what's left over whatever time the timer was re-armed for, so a ring frozen at 75% still
    // reaches zero right as the toast goes, even when the one-second grace tail decides that.
    // CSS does the drawing: the dash offset starts at the emptied share (`1 - ringFrom`) and a
    // keyframe drains it to the full circumference over `ringMs`; `ringRun` restarts the animation.
    const RING_RADIUS = 10
    const RING_CIRCUMFERENCE = 2 * Math.PI * RING_RADIUS
    let ringFrom = $state(1)
    let ringMs = $state(0)
    let ringRunning = $state(false)
    let ringRun = $state(0)
    let ringStartedAt = 0

    function ringFraction(): number {
        if (!ringRunning) return ringFrom
        if (ringMs <= 0) return 0
        return ringFrom * Math.max(0, 1 - (Date.now() - ringStartedAt) / ringMs)
    }

    function startRing(from: number, ms: number) {
        ringFrom = from
        ringMs = ms
        ringStartedAt = Date.now()
        ringRunning = true
        ringRun++
    }

    function pauseRing() {
        ringFrom = ringFraction()
        ringRunning = false
    }

    function handlePointerEnter() {
        hovered = true
        if (dismissal !== 'transient') return
        clearTimer()
        pauseRing()
    }

    function handlePointerLeave() {
        hovered = false
        if (dismissal !== 'transient') return
        const ms = Math.max(naturalDeadline - Date.now(), HOVER_LEAVE_GRACE_MS)
        armTimer(ms)
        startRing(ringFraction(), ms)
    }

    $effect(() => {
        if (dismissal !== 'transient') return
        // Reading `postedAt` is also what restarts the clock on a same-id re-raise
        // that leaves dismissal and timeout unchanged.
        naturalDeadline = Math.max(mountedAt, postedAt) + timeoutMs
        const ms = Math.max(naturalDeadline - Date.now(), 0)
        const from = timeoutMs > 0 ? Math.min(1, ms / timeoutMs) : 0
        untrack(() => {
            if (hovered) {
                ringFrom = from
                ringRunning = false
            } else {
                armTimer(ms)
                startRing(from, ms)
            }
        })
        return clearTimer
    })
</script>

<div
    bind:this={toastElement}
    class="toast"
    class:info={level === 'info'}
    class:success={level === 'success'}
    class:warn={level === 'warn'}
    class:error={level === 'error'}
    style={widthPx ? `max-width: ${String(widthPx)}px` : undefined}
    role={level === 'default' || level === 'info' || level === 'success' ? 'status' : 'alert'}
    onpointerenter={handlePointerEnter}
    onpointerleave={handlePointerLeave}
    onpointerdown={rememberPointerFocusOrigin}
    onfocusin={rememberFocusOrigin}
>
    <ToastLevelIcon {level} />
    <div class="toast-main">
        <div class="toast-content">
            <!-- The corner, floated so the body's first line wraps around the age label and the
                 close button pinned over it, while everything below runs to the right edge. -->
            <span class="toast-corner">
                {#if ageLabel !== null}
                    <!-- `aria-hidden`: the toast is a live region, so a label that changes every
                         minute would have a screen reader announce the whole toast again. -->
                    <span class="toast-age" aria-hidden="true">{ageLabel}</span>
                {/if}
            </span>
            {#if typeof content === 'string'}
                <div>
                    <span class="toast-message">{content}</span>
                    {#if showSendErrorReport}
                        <div class="toast-actions">
                            <Button size="mini" variant="secondary" onclick={handleSendErrorReport}>
                                {tString('ui.toast.sendErrorReport')}
                            </Button>
                        </div>
                    {/if}
                </div>
            {:else}
                {@const ContentComponent = content}
                <!-- `toastId` goes to EVERY component toast, props or not: a body that
                     closes itself calls `dismissToast(toastId)`, and without the id that's
                     a silent no-op that leaves the toast up. -->
                <ContentComponent {...contentProps} toastId={id} />
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
        {#if dismissal === 'transient'}
            {#key ringRun}
                <svg
                    class="toast-timer"
                    data-state={ringRunning ? 'running' : 'paused'}
                    viewBox="0 0 22 22"
                    aria-hidden="true"
                >
                    <circle
                        cx="11"
                        cy="11"
                        r={RING_RADIUS}
                        style:stroke-dasharray="{RING_CIRCUMFERENCE}px"
                        style:stroke-dashoffset="{RING_CIRCUMFERENCE * (1 - ringFrom)}px"
                        style:animation-duration="{ringMs}ms"
                    />
                </svg>
            {/key}
        {/if}
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
        padding: var(--spacing-toast);
        /* Glass: the level's tint, partly see-through and blurred (`app.css` § Toasts). Each
           level only swaps `--color-toast-bg`. */
        --color-toast-bg: var(--color-toast-default-bg);

        background: color-mix(in srgb, var(--color-toast-bg) var(--glass-toast-opacity), transparent);
        -webkit-backdrop-filter: var(--glass-backdrop);
        backdrop-filter: var(--glass-backdrop);
        border: 1px solid var(--color-toast-default-border);
        border-radius: var(--radius-toast);
        box-shadow: var(--shadow-toast), var(--shadow-glass-rim);
        font-size: var(--font-size-sm);
    }

    .toast.info {
        --color-toast-bg: var(--color-toast-info-bg);

        border-color: var(--color-toast-info-border);
    }

    .toast.success {
        --color-toast-bg: var(--color-toast-success-bg);

        border-color: var(--color-toast-success-border);
    }

    .toast.warn {
        --color-toast-bg: var(--color-toast-warn-bg);

        border-color: var(--color-toast-warn-border);
    }

    .toast.error {
        --color-toast-bg: var(--color-toast-error-bg);

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

    /* A long unbroken run (an S3 access key ID, a path, a URL) breaks inside the toast rather
       than running out past its edge. Inherited, so component bodies get it too. Words still
       break at spaces first. */
    .toast-content {
        overflow-wrap: anywhere;
    }

    /* The close button is pinned over the content box's top-right corner, reaching 19px into it
       both down and in. The float claims that corner (plus a gap, plus the age label when there is
       one), so text flows around it instead of under it. */
    .toast-corner {
        float: right;
        height: 19px;
        min-height: 1lh;
        margin-left: var(--spacing-md);
        padding-right: calc(19px + var(--spacing-sm));
    }

    .toast-age {
        color: var(--color-text-tertiary);
        font-variant-numeric: tabular-nums;
        white-space: nowrap;
    }

    /* The toast body contract (`ui/DETAILS.md` § Toast system): a body renders ONE root, in block
       flow. A flex or grid root would be its own formatting context, which the corner float can't
       reach into, so its lines couldn't wrap around the corner. The frame stacks the root's rows
       instead, every row a block `--spacing-xs` below the last. Zero specificity (`:where`), so a
       row's own `display` or `margin-top` still wins; a row that sets `margin-top` sets its whole
       distance from the row above. Unlayered, so it also beats the `typography` layer's margins
       on a `<p>` row. `toast-body-layout.test.ts` holds bodies to the one-root, no-flex-root half. */
    :where(.toast-content) > :global(*) > :global(*) {
        display: block;
        margin-block: 0;
    }

    :where(.toast-content) > :global(*) > :global(* + *) {
        margin-block-start: var(--spacing-xs);
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

    /* The countdown ring hugs the X button's round edge, starting at 12 o'clock. The circle's inline
       dash array and offset come from `RING_CIRCUMFERENCE`; the keyframe's empty offset repeats
       that number (2π·10) because a dash-offset percentage measures the viewport, not the dash. */
    .toast-timer {
        position: absolute;
        inset: 0;
        width: 100%;
        height: 100%;
        transform: rotate(-90deg);
        pointer-events: none;
    }

    .toast-timer circle {
        fill: none;
        stroke: currentcolor;
        stroke-width: 1.5;
        opacity: 0.6;
    }

    /* The duration is inline (`animation-duration`): the time the dismiss timer was armed for. */
    .toast-timer[data-state='running'] circle {
        animation-name: toast-timer-drain;
        animation-timing-function: linear;
        animation-fill-mode: forwards;
    }

    @keyframes toast-timer-drain {
        to {
            stroke-dashoffset: 62.8319px;
        }
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
