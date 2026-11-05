<!--
    The toast stack, plus the two live regions it speaks through.

    ❗ The regions are always mounted, empty until a toast arrives (a region inserted with its
    first message announces unreliably in WebKit), and they're siblings: a polite one for
    status toasts and an assertive one for warnings and failures. The toasts themselves are
    no live regions, so nothing nests an `alert` inside a polite region and nothing speaks
    twice. `ui/DETAILS.md` § Toast system.
-->
<script lang="ts">
    import { onMount, untrack } from 'svelte'
    import { getToasts, dismissToast } from './toast-store.svelte'
    import ToastItem from './ToastItem.svelte'
    import type { ToastSpeech } from './toast-speech'
    import LiveAnnouncer from '../LiveAnnouncer.svelte'
    import { handleToastStackFocusOut, handleToastStackKeydown, registerToastStack } from './toast-focus'
    import { getKeepNotificationsUntilClosed, getShowFunctionKeyBar } from '#lib/settings/reactive-settings.svelte.js'
    import { dependOn } from '#lib/utils/reactivity.js'
    import { watchToastPlacement, type ToastEdge } from './toast-placement'

    const toasts = getToasts()

    // "Keep notifications until I close them": every toast leaves the way a persistent one
    // does (no timer, no ring). Only how it LEAVES changes; the store's `dismissal` still says
    // what kind of toast it is, so pane navigation keeps clearing a pane's own stale ones.
    const keepUntilClosed = $derived(getKeepNotificationsUntilClosed())

    type Announcer = { announce: (message: string) => void }
    let politeRegion: Announcer | undefined = $state()
    let assertiveRegion: Announcer | undefined = $state()

    // Toasts raised in the same turn speak as one message per region: the announcer keeps
    // only the newest of two messages asked within a frame, which would drop the first.
    const pending = { polite: [] as string[], assertive: [] as string[] }
    let flushQueued = false

    function speak({ text, urgent }: ToastSpeech): void {
        if (!text) return
        pending[urgent ? 'assertive' : 'polite'].push(text)
        if (flushQueued) return
        flushQueued = true
        queueMicrotask(() => {
            flushQueued = false
            if (pending.polite.length > 0) politeRegion?.announce(pending.polite.splice(0).join(' '))
            if (pending.assertive.length > 0) assertiveRegion?.announce(pending.assertive.splice(0).join(' '))
        })
    }

    let stackElement: HTMLDivElement | undefined = $state()

    // The keyboard walk (`toast-focus.ts`). Listeners rather than `onkeydown` attributes: the
    // stack is no widget of its own, only the place a walk across its toasts happens.
    onMount(() => {
        const unregister = registerToastStack({
            element: () => stackElement,
            announce: (message) => politeRegion?.announce(message),
        })
        const element = stackElement
        element?.addEventListener('keydown', handleToastStackKeydown)
        element?.addEventListener('focusout', handleToastStackFocusOut)
        return () => {
            unregister()
            element?.removeEventListener('keydown', handleToastStackKeydown)
            element?.removeEventListener('focusout', handleToastStackFocusOut)
        }
    })

    // Off the focused cursor row (`toast-placement.ts`): watched only while toasts are up, and
    // back at the top once the last one goes.
    let edge = $state<ToastEdge>('top')
    /** Height of the window chrome along the bottom (the function key bar), which the bottom edge sits above. */
    let bottomInset = $state(0)
    const hasToasts = $derived(toasts.length > 0)

    let remeasurePlacement: (() => void) | null = null

    $effect(() => {
        const element = stackElement
        if (!hasToasts || !element) {
            edge = 'top'
            bottomInset = 0
            return
        }
        const watch = untrack(() =>
            watchToastPlacement(element, (placement) => {
                edge = placement.edge
                bottomInset = placement.bottomInset
            }),
        )
        remeasurePlacement = watch.remeasure
        return () => {
            remeasurePlacement = null
            watch.stop()
        }
    })

    // The function key bar coming or going moves the floor the bottom edge sits on, and no
    // DOM event says so: follow its setting and measure again.
    $effect(() => {
        dependOn(getShowFunctionKeyBar())
        untrack(() => remeasurePlacement?.())
    })

    function handleUserDismiss(id: string): void {
        const toast = toasts.find((t) => t.id === id)
        toast?.onDismiss?.()
        dismissToast(id)
    }
</script>

<div class="toast-container" class:at-bottom={edge === 'bottom'} data-toast-stack data-edge={edge}
    style:bottom={edge === 'bottom' ? `calc(var(--spacing-lg) + ${String(bottomInset)}px)` : undefined} bind:this={stackElement}>
    {#each toasts as toast (toast.id)}
            <ToastItem
                id={toast.id}
                content={toast.content}
                level={toast.level}
                dismissal={keepUntilClosed ? 'persistent' : toast.dismissal}
                timeoutMs={toast.timeoutMs}
                postedAt={toast.postedAt}
                closeTooltip={toast.closeTooltip}
                contentProps={toast.props}
                widthPx={toast.widthPx}
                report={toast.report}
                onTimeout={dismissToast}
                onUserDismiss={handleUserDismiss}
                onSpeak={speak}
            />
    {/each}
</div>
<LiveAnnouncer bind:this={politeRegion} data-toast-announcer="polite" />
<LiveAnnouncer bind:this={assertiveRegion} politeness="assertive" data-toast-announcer="assertive" />

<style>
    .toast-container {
        position: fixed;
        /* Below the status corner's real bottom edge, which it publishes (`status-corner/toast-clearance.ts`), so
           a toast never covers the copy's progress chip at any zoom or text size. */
        top: max(var(--spacing-lg), calc(var(--titlebar-status-corner-bottom) + var(--spacing-sm)));
        right: var(--spacing-lg);
        z-index: var(--z-notification);
        display: flex;
        flex-direction: column-reverse;
        /* Right-align so a wider toast (a per-toast `widthPx`, or one grown to fit its action
           row) extends leftward while narrower toasts keep hugging the screen edge. Each
           ToastItem caps itself at 360 by default; the stack only keeps the same gutter on the
           left as on the right, which `--spacing-toast-actions-max` (`app.css`) mirrors. */
        align-items: flex-end;
        /* Room for each toast's shadow to fall before the next one starts. */
        gap: var(--spacing-md);
        max-width: calc(100vw - 2 * var(--spacing-lg));
        pointer-events: none;
    }

    /* Moved off the focused cursor row (`toast-placement.ts`). Newest still sits nearest the
       window edge. `bottom` is inline: the gap plus the measured function key bar. */
    .toast-container.at-bottom {
        top: auto;
        flex-direction: column;
    }

    .toast-container > :global(*) {
        pointer-events: auto;
    }
</style>
