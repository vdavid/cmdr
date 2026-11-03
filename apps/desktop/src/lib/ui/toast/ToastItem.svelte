<script lang="ts">
    import { onDestroy, onMount, untrack } from 'svelte'
    import type { ToastContent, ToastLevel, ToastDismissal } from './toast-store.svelte'
    import { HOVER_LEAVE_GRACE_MS } from './toast-store.svelte'
    import { formatToastAge, msUntilToastAgeChanges } from './toast-age'
    import ToastLevelIcon from './ToastLevelIcon.svelte'
    import ToastActions from './ToastActions.svelte'
    import ReportOffer from '#lib/error-reporter/ReportOffer.svelte'
    import type { ReportIncident } from '#lib/error-reporter/report-incident.js'
    import CloseButton from '#lib/ui/CloseButton.svelte'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { captureFocusOrigin, restoreFocus, type FocusOrigin } from '#lib/ui/focus-restore.js'
    import { toastSpeech, type ToastSpeech } from './toast-speech'
    import { dependOn } from '#lib/utils/reactivity.js'
    import { continueWalkAfterClose, isInToastStack } from './toast-focus'

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
         * What this toast's "Send error report…" opens (`Toast.report`, resolved by the store),
         * or null for none. The frame renders the offer under string content; a component
         * gets it as its `report` prop and places `ReportOffer` in its own action row.
         */
        report?: ReportIncident | null
        /** Called when the auto-dismiss timer fires for transient toasts. */
        onTimeout: (id: string) => void
        /** Called when the user clicks the X button or the inline action. */
        onUserDismiss: (id: string) => void
        /**
         * Called after each post (mount and every same-id re-raise) with what the toast says.
         * The toast is no live region itself: the stack speaks it (`ToastContainer.svelte`).
         */
        onSpeak?: (speech: ToastSpeech) => void
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
        report = null,
        onTimeout,
        onUserDismiss,
        onSpeak,
    }: Props = $props()

    // Auto-dismiss timer for transient toasts.
    //
    // The rule: a toast hides at `max(its natural deadline, the moment it was
    // let go + HOVER_LEAVE_GRACE_MS)`. It's HELD while the pointer is on it or
    // keyboard focus is inside it, and let go when neither is true any more. The
    // natural deadline is `mountedAt + timeoutMs` in wall-clock time, and holding
    // neither pauses nor extends that clock. What holding guarantees is the grace
    // tail after letting go, so a toast can never vanish out from under a cursor
    // or a focused button, and never snaps away the instant either drifts off.
    // Don't reintroduce a paused countdown: a toast held for a minute is meant to
    // go one second after it's let go, not to get its leftover seconds back.
    //
    // The clock counts from the later of mount and the latest post, so a same-id
    // re-raise (a new `postedAt`, which can also change `dismissal` and
    // `timeoutMs`) restarts it. Persistent toasts never get a timer, and a toast
    // re-raised while held waits to be let go, same as one held the whole time.
    const mountedAt = Date.now()
    let timer: ReturnType<typeof setTimeout> | undefined
    let naturalDeadline = 0
    let hovered = false
    let focusWithin = false
    let toastElement: HTMLDivElement | undefined = $state()
    let focusReturnTarget: FocusOrigin | null = null
    let ownedFocus = false
    /** What a press inside the toast may focus; a press anywhere else in it keeps focus put. */
    const FOCUSABLE_IN_TOAST = 'button, a[href], input, select, textarea, [tabindex], [contenteditable="true"]'

    onMount(() => {
        focusReturnTarget = captureFocusOrigin()
        document.addEventListener('focusin', observeDocumentFocus)
        return () => { document.removeEventListener('focusin', observeDocumentFocus); }
    })

    function rememberFocusOrigin(event: FocusEvent): void {
        ownedFocus = true
        const origin = event.relatedTarget
        // From another toast, the walk is on and it knows where focus came from.
        if (isInToastStack(origin)) return
        if (origin instanceof HTMLElement && origin !== document.body && !toastElement?.contains(origin)) {
            focusReturnTarget = captureFocusOrigin(origin)
        }
    }

    function rememberPointerFocusOrigin(): void {
        const active = document.activeElement
        if (active instanceof HTMLElement && active !== document.body && !toastElement?.contains(active)) {
            focusReturnTarget = captureFocusOrigin(active)
            ownedFocus = true
        }
    }

    /**
     * A click on the toast's text keeps focus where it is. WebKit otherwise drops it to `<body>`
     * (the frame isn't focusable), and every pane key dies until Tab or a click. Text selection
     * is off app-wide, so cancelling the press costs nothing; a control still takes its click.
     */
    function keepFocusOnTextPress(event: MouseEvent): void {
        if (event.target instanceof Element && !event.target.closest(FOCUSABLE_IN_TOAST)) event.preventDefault()
    }

    function observeDocumentFocus(event: FocusEvent): void {
        const destination = event.target
        if (ownedFocus && destination instanceof HTMLElement && !toastElement?.contains(destination)) ownedFocus = false
    }

    onDestroy(() => {
        if (!ownedFocus) return
        const target = focusReturnTarget
        queueMicrotask(() => {
            if (document.activeElement !== document.body || continueWalkAfterClose()) return
            restoreFocus(target)
        })
    })

    // What the toast says, read off the rendered body once each post has rendered, so a
    // component toast speaks the same way a string one does.
    let contentElement: HTMLDivElement | undefined = $state()
    /** The frame's accessible name, so a keyboard walk landing on it hears what the toast says. */
    let spokenText = $state('')

    $effect(() => {
        // Each post is news, even with the same words: `postedAt` moves on every re-raise.
        dependOn(postedAt, content)
        const element = contentElement
        if (!element) return
        const urgent = level === 'warn' || level === 'error'
        const text = toastSpeech(element)
        spokenText = text
        untrack(() => onSpeak?.({ text, urgent }))
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

    function isHeld(): boolean {
        return hovered || focusWithin
    }

    /** Runs `change` (flipping `hovered` or `focusWithin`), then starts or stops the clock if being held flipped. */
    function updateHold(change: () => void) {
        const wasHeld = isHeld()
        change()
        const held = isHeld()
        if (held === wasHeld || dismissal !== 'transient') return
        if (held) {
            clearTimer()
            pauseRing()
            return
        }
        const ms = Math.max(naturalDeadline - Date.now(), HOVER_LEAVE_GRACE_MS)
        armTimer(ms)
        startRing(ringFraction(), ms)
    }

    function handlePointerEnter() {
        updateHold(() => {
            hovered = true
        })
    }

    function handlePointerLeave() {
        updateHold(() => {
            hovered = false
        })
    }

    function handleFocusIn(event: FocusEvent) {
        rememberFocusOrigin(event)
        updateHold(() => {
            focusWithin = true
        })
    }

    function handleFocusOut(event: FocusEvent) {
        const next = event.relatedTarget
        if (next instanceof Node && toastElement?.contains(next)) return
        updateHold(() => {
            focusWithin = false
        })
    }

    $effect(() => {
        if (dismissal !== 'transient') return
        // Reading `postedAt` is also what restarts the clock on a same-id re-raise
        // that leaves dismissal and timeout unchanged.
        naturalDeadline = Math.max(mountedAt, postedAt) + timeoutMs
        const ms = Math.max(naturalDeadline - Date.now(), 0)
        const from = timeoutMs > 0 ? Math.min(1, ms / timeoutMs) : 0
        untrack(() => {
            if (isHeld()) {
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

<!-- `onmousedown` only keeps a press on the text from moving focus (`keepFocusOnTextPress`); the
     frame does nothing on it, so it's no interaction the keyboard is missing. -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
    bind:this={toastElement}
    class="toast"
    class:info={level === 'info'}
    class:success={level === 'success'}
    class:warn={level === 'warn'}
    class:error={level === 'error'}
    style={widthPx ? `max-width: ${String(widthPx)}px` : undefined}
    data-toast-frame
    role="group"
    aria-label={spokenText || undefined}
    onpointerenter={handlePointerEnter}
    onpointerleave={handlePointerLeave}
    onpointerdown={rememberPointerFocusOrigin}
    onmousedown={keepFocusOnTextPress}
    onfocusin={handleFocusIn}
    onfocusout={handleFocusOut}
>
    <ToastLevelIcon {level} />
    <!-- Before the content though pinned to the corner: focus order follows reading order, and it
         reads on the first line, above any action row (`toast-focus.ts` walks in DOM order). -->
    <CloseButton
        class="toast-close"
        onclick={() => {
            onUserDismiss(id)
        }}
        tooltipText={closeTooltip}
        ariaLabel={tString('ui.toast.dismissAria')}
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
    </CloseButton>
    <div class="toast-main">
        <div class="toast-content" bind:this={contentElement}>
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
                <!-- Inline, right beside the corner, ❌ never inside a block row: a browser leaves a
                     float out of a block sibling's width, so a short message got a box as narrow as
                     its text and broke mid-word around the corner ("Copie / d 1 / file."). -->
                <span class="toast-message">{content}</span>
                {#if report}
                    <ToastActions>
                        <ReportOffer
                            incident={report}
                            size="mini"
                            onOpen={() => {
                                onUserDismiss(id)
                            }}
                        />
                    </ToastActions>
                {/if}
            {:else}
                {@const ContentComponent = content}
                <!-- `toastId` goes to EVERY component toast, props or not: a body that
                     closes itself calls `dismissToast(toastId)`, and without the id that's
                     a silent no-op that leaves the toast up. -->
                <ContentComponent {...contentProps} toastId={id} {...report ? { report } : {}} />
            {/if}
        </div>
    </div>
</div>

<style>
    .toast {
        /* The containing block for the close button pinned to the corner. */
        position: relative;
        display: flex;
        align-items: flex-start;
        gap: var(--spacing-md);
        max-width: 360px;
        /* Wins over `max-width` (and an inline `widthPx`): a `ToastActions` row's one-line width is
           its minimum, so a toast whose buttons don't fit grows leftward until they do. Text can't
           push it, since `overflow-wrap: anywhere` gives text a near-zero minimum. */
        min-width: min-content;
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

    /* Pinned the same distance from the top and right edges, centered on the first text line. */
    .toast :global(.toast-close) {
        position: absolute;
        top: calc(var(--spacing-toast) - 3px);
        right: calc(var(--spacing-toast) - 3px);
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
