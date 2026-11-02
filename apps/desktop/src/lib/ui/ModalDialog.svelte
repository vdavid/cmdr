<script lang="ts">
    import { onMount, onDestroy, tick, untrack } from 'svelte'
    import type { Snippet } from 'svelte'
    import { notifyDialogOpened, notifyDialogClosed } from '#lib/tauri-commands/index.js'
    import { focusFirstField, trapFocus } from './focus-trap'
    import { captureFocusOrigin, restoreFocus, type FocusOrigin } from './focus-restore'
    import { anchoredTopFor } from './modal-anchor'
    import { clampDragOffset, clampPointToArea, dragFrameOf } from './modal-drag'
    import {
        declaredStyle,
        observeFooterRow,
        panelMinWidth,
        resizeDirectionsFor,
        resizeLimitsOf,
        resizeStep,
        type DraggedSize,
        type ResizeDirection,
    } from './modal-resize'
    import type { SoftDialogId } from './dialog-registry'
    import { registerDialogClose, unregisterDialogClose } from './dialog-close-registry'
    import { markDialogOpen, markDialogClosed } from './open-dialogs.svelte'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { dependOn } from '#lib/utils/reactivity.js'
    import { providePortalTarget } from './portal-target'
    import {
        attachStartedElsewhereGuard,
        startedElsewhereLine,
        type StartedElsewhereGuard,
        type ToolInitiator,
    } from './started-elsewhere-guard'

    interface Props {
        titleId: string
        onkeydown?: (event: KeyboardEvent) => void
        title: Snippet
        children: Snippet
        /**
         * Action buttons, rendered in a right-aligned footer to match macOS.
         * Put the primary action last (rightmost). When omitted, no footer renders
         * (dialogs that own a custom button layout still place buttons in `children`).
         */
        footer?: Snippet
        /**
         * Content pinned to the LEFT of the footer row, on the same line as the
         * action buttons (a modifier toggle, a "don't ask again" switch). Renders
         * only alongside `footer`; the buttons stay right-aligned.
         */
        footerLeading?: Snippet
        /** MCP dialog tracking: sends notifyDialogOpened/Closed on mount/destroy */
        dialogId?: SoftDialogId
        role?: 'dialog' | 'alertdialog'
        draggable?: boolean
        /** Use blurred overlay (0.6 opacity + backdrop-filter) instead of plain 0.4 opacity */
        blur?: boolean
        ariaDescribedby?: string
        /** Inline style string for the dialog container (sizing, colors) */
        containerStyle?: string
        /**
         * Lets the user resize the dialog by dragging any edge or corner. The
         * body region grows and scrolls; the caller still passes the initial
         * size via `containerStyle`. Off by default. Combines with `fillBody`:
         * there the inner scroll region keeps ownership of the scrolling and
         * only the grab zones and the size floors come from here.
         *
         * `'horizontal'` is the right choice whenever the body has nothing to do
         * with extra height (a short form, a capped list): free vertical dragging
         * would just open a band of dead space above the footer. It exposes the
         * left and right edges only. Use `true` when a region inside actually
         * absorbs the slack, which in practice means `fillBody` with a scrolling
         * child.
         */
        resizable?: boolean | 'horizontal'
        /**
         * A height the user dragged this dialog to before, which the caller kept
         * (`onResizeEnd`). Taken once, the first time the dialog is vertically
         * resizable and the user hasn't dragged it yet, and fitted to the window.
         * May arrive after the dialog opened (it's usually read from disk).
         */
        restoreHeight?: number | null
        /** A resize drag ended at this size (`null` on an axis it never set). */
        onResizeEnd?: (size: DraggedSize) => void
        /**
         * Pins the dialog's TOP edge where centering first put it, so a body that
         * grows (a mode switch revealing extra controls) extends downward instead
         * of pushing the title up. The dialog still slides up if it would otherwise
         * run past the bottom. For dialogs whose height changes while open.
         */
        growDownward?: boolean
        /**
         * Where the dialog sits in the overlay. `'center'` is the default macOS
         * placement; `'top'` drops it 10vh from the top, the Spotlight-style
         * placement for a dialog the user types into and reads a long list from
         * (the query dialogs).
         */
        align?: 'center' | 'top'
        /**
         * Makes the panel a fixed-height frame: it becomes a flex column, the body
         * absorbs the vertical slack, and the whole panel clips to its radius. The
         * caller caps the height via `containerStyle` (`max-height: 80vh`). Use it
         * when a child region (a results list) should scroll while the title bar and
         * footer stay put. The body is a flex column too, so its own child can take
         * `flex: 1 1 auto` and own the scrolling. `resizable` brings its own version
         * of this (with a scrolling body); combined, `fillBody`'s inner region wins.
         */
        fillBody?: boolean
        /**
         * Hands the WHOLE keydown contract to the consumer: this component still
         * stops propagation (shielding the app behind the scrim), then forwards every
         * key to `onkeydown` — including Escape and Enter on a focused button, which
         * it otherwise handles itself. For dialogs that own dynamic Enter semantics or
         * an Escape that must defer to a nested popover (the query dialogs). `onclose`
         * still drives the × button, the focus-trap escape fallback, and the MCP close
         * registry.
         */
        ownsKeyboard?: boolean
        /**
         * Extra class on the overlay element. For a SHARED dialog that needs one
         * stable structural hook across several `dialogId`s (`QueryDialog` renders as
         * `.search-overlay` for all three of its ids; the E2E suite and the
         * overlay-dismissal safety net key on it). Not a styling hook.
         */
        overlayClass?: string
        /** Clicking the scrim closes the dialog. Off by default (macOS panels don't dismiss on backdrop). */
        closeOnOverlayClick?: boolean
        /**
         * Renders at `--z-modal-top`, above every other modal (they all share
         * `--z-modal` and stack by DOM order, which nothing the APP raises over
         * the user's current dialog can rely on). ❌ Not a "make mine important"
         * switch: two callers, the quit prompt and the old-macOS notice, ordered
         * against each other by DOM position. DETAILS § ModalDialog.
         */
        topmost?: boolean
        /**
         * Who started this, when a stray keystroke mustn't be able to touch it: it opened by itself,
         * maybe under someone's typing. It waits for the keyboard to go quiet, keeps focus on the
         * overlay until a click or Tab, and says who started it (`started-elsewhere-guard.ts`). Read
         * once, as the dialog opens.
         */
        startedBy?: ToolInitiator
        /** Renders × button and handles Escape key */
        onclose?: () => void
        /**
         * Escape runs this instead of `onclose`, for a dialog where Escape means
         * something other than closing (the progress dialog: Escape cancels the
         * operation, × only stops watching it). Takes effect only alongside
         * `onclose`: no `onclose` means Escape reaches `onkeydown` as usual.
         */
        onescape?: () => void
    }

    const {
        titleId,
        onkeydown,
        title,
        children,
        footer,
        footerLeading,
        dialogId,
        role = 'dialog',
        draggable = true,
        blur = false,
        ariaDescribedby,
        containerStyle = '',
        resizable = false,
        restoreHeight = null,
        onResizeEnd,
        growDownward = false,
        align = 'center',
        fillBody = false,
        ownsKeyboard = false,
        overlayClass = '',
        closeOnOverlayClick = false,
        topmost = false,
        startedBy,
        onclose,
        onescape,
    }: Props = $props()

    /** Read once: a host's prop getter may read state that's already gone as the dialog comes down. */
    const guardedFor = untrack(() => startedBy)

    /** What Escape does: `onescape` where the dialog gives one, else closing. */
    const escapeAction = $derived(onclose ? (onescape ?? onclose) : undefined)

    let overlayElement: HTMLDivElement | undefined = $state()
    // Menus in the body (`Select`, `Combobox`) portal into the overlay: it carries the modal
    // rung and the focus trap, and it doesn't clip the way the panel does.
    providePortalTarget(() => overlayElement)
    let dialogElement: HTMLDivElement | undefined = $state()
    let titleBarElement: HTMLDivElement | undefined = $state()
    let dialogPosition = $state({ x: 0, y: 0 })
    let isDragging = $state(false)
    /** Distance from the overlay's top to the dialog's top, once `growDownward` pins it. */
    let anchoredTop = $state<number | null>(null)
    /** Where centering put the `growDownward` dialog's top: the spot it returns to. Plain: nothing renders it. */
    let homeTop = 0
    /**
     * Element that had focus when the dialog opened. Restored on destroy so
     * keyboard input flows back to wherever it came from (typically a file
     * pane): without this, focus falls to <body> and arrow keys silently
     * no-op until the user clicks back into a pane.
     */
    let focusOrigin: FocusOrigin | null = null
    let heightObserver: ResizeObserver | null = null
    /** Re-clamps the drag offset when the window or the panel changes size (`keepTitleBarReachable`). */
    let reachObserver: ResizeObserver | null = null
    /** Size the user dragged the panel to. */
    let resizedSize = $state<DraggedSize>({ width: null, height: null })
    /** Tears down whichever pointer drag (move or resize) is running, so a dialog closed mid-drag leaves no listeners. */
    let stopActiveDrag: (() => void) | null = null

    const resizeDirections = $derived(resizeDirectionsFor(resizable))

    let footerActionsElement: HTMLDivElement | undefined = $state()
    /** How wide the panel must be to hold the footer's buttons on one row; `0` with no footer. */
    let footerRowPx = $state(0)

    /**
     * A footer's buttons never wrap, in any language: the panel's `min-width` rises to the
     * width they take on one row (`panelMinWidth`, written below), so a long translation
     * widens the dialog instead. Observed: a label can change while the dialog is open.
     */
    $effect(() => {
        const [panel, actions] = [dialogElement, footerActionsElement]
        if (!panel || !actions) {
            footerRowPx = 0
            return
        }
        return observeFooterRow(panel, actions, (px) => (footerRowPx = px))
    })

    /**
     * The drag offset rides on `left` / `top` against the panel's own `position: relative`,
     * NOT on a `transform`.
     *
     * ❌ Never move this back to `transform`, and never add `filter`, `backdrop-filter`,
     * `perspective`, `contain: paint|layout`, or `will-change` of any of those to the panel.
     * Each of them makes the panel the containing block for `position: fixed` DESCENDANTS,
     * which silently re-bases any floating layer rendered INSIDE the panel from the viewport
     * onto the panel's border box: it jumps down-right by exactly the panel's top-left.
     * `transform: translate(0px, 0px)` triggers it too, so an undragged dialog broke them
     * just as thoroughly as a dragged one.
     *
     * `Popover` is the exposed one (it positions `fixed` from `getBoundingClientRect()` and
     * deliberately does NOT portal, so the host dialog's Escape handler can find it in its own
     * subtree). `Menu`, `Select`, and `Combobox` portal into the overlay (`providePortalTarget`),
     * outside the panel, so all three are immune.
     *
     * `left` / `top` shift the panel visually without reflowing siblings (same as the
     * transform did) and establish no containing block. `will-change: transform` is the
     * tempting "smooth out the drag" change that would bring the bug straight back.
     *
     * They're written as inline PROPERTIES from an effect, not into the `style` attribute:
     * the dragged size lives in that same attribute, so re-rendering it (which a drag does
     * on every pointer frame) would snap a resized dialog back to its starting size
     * mid-drag. `containerStyle` is the only thing left on the attribute.
     */
    $effect(() => {
        const el = dialogElement
        if (!el) return
        // Read so that a `containerStyle` change — which rewrites the attribute and wipes
        // these properties with it — re-applies them.
        dependOn(containerStyle)
        // Only ever SET these: removing them would delete `containerStyle`'s own width or
        // height along with ours, since both land on the same inline declaration.
        if (resizedSize.width !== null) el.style.width = `${String(resizedSize.width)}px`
        if (resizedSize.height !== null) el.style.height = `${String(resizedSize.height)}px`
        el.style.left = `${String(dialogPosition.x)}px`
        el.style.top = `${String(dialogPosition.y)}px`
        el.style.minWidth = panelMinWidth(containerStyle, resizable !== false, footerRowPx)
        if (anchoredTop === null) {
            el.style.removeProperty('align-self')
            el.style.removeProperty('margin-top')
        } else {
            el.style.alignSelf = 'flex-start'
            el.style.marginTop = `${String(anchoredTop)}px`
        }
    })

    /** The height the user dragged to, set aside while vertical resizing is withdrawn. */
    let parkedHeight: number | null = null

    /**
     * A dialog that stops being vertically resizable goes back to its natural height,
     * and gets the user's height back once it's resizable again, if the window still
     * fits it. **Why**: the body that used the height has given way to one that doesn't
     * (the progress dialog's tree to a conflict prompt), and the bands that could undo a
     * tall, mostly empty panel are gone with it.
     */
    $effect(() => {
        const vertical = resizable === true
        const el = dialogElement
        if (!el) return
        untrack(() => {
            if (!vertical && resizedSize.height !== null) {
                parkedHeight = resizedSize.height
                resizedSize = { ...resizedSize, height: null }
                // Back to what `containerStyle` says (nothing, for most): the drag's
                // height shares that one inline declaration.
                el.style.height = declaredStyle(containerStyle, 'height')
            } else if (vertical && parkedHeight !== null) {
                const height = parkedHeight
                parkedHeight = null
                if (height <= resizeLimitsOf(el).maxHeight) {
                    resizedSize = { ...resizedSize, height }
                }
            }
        })
    })

    /** The caller's remembered height has been taken, or a drag made it moot. */
    let restoredHeightTaken = false

    /** The first time the dialog can take height, the one the caller remembered. */
    $effect(() => {
        const vertical = resizable === true
        const height = restoreHeight
        const el = dialogElement
        if (!el || !vertical || height === null) return
        untrack(() => {
            if (restoredHeightTaken || resizedSize.height !== null || parkedHeight !== null) return
            restoredHeightTaken = true
            const limits = resizeLimitsOf(el)
            resizedSize = { ...resizedSize, height: Math.max(limits.minHeight, Math.min(height, limits.maxHeight)) }
        })
    })

    /** Where flex centering puts the dialog's top right now. */
    function centeredTop(): number {
        if (!overlayElement || !dialogElement) return 0
        return Math.max(0, (overlayElement.clientHeight - dialogElement.offsetHeight) / 2)
    }

    /**
     * Re-centers on the CURRENT height. Used at mount (a no-op visually: it pins
     * exactly where flex already put the dialog) and on window resize, where
     * re-centering is what the user expects.
     */
    function anchorToCurrentCenter() {
        if (!growDownward) return
        homeTop = centeredTop()
        anchoredTop = homeTop
    }

    /**
     * Keeps a grown dialog on screen, and brings a shrunk one back: the top goes
     * where it opened, pulled up only by the overflow (`modal-anchor.ts`). A dialog
     * taller than the window stops at the top and its body scrolls (the
     * `.grow-downward` cap below).
     */
    function clampAnchorIntoView() {
        if (anchoredTop === null || !overlayElement || !dialogElement) return
        // The margin the `.grow-downward` height cap leaves on each side.
        const margin = parseFloat(getComputedStyle(dialogElement).getPropertyValue('--spacing-xl')) || 0
        anchoredTop = anchoredTopFor(homeTop, overlayElement.clientHeight, dialogElement.offsetHeight, margin)
    }

    /** `offset`, kept where the title bar stays grabbable (`modal-drag.ts`); as is with no layout to measure. */
    function reachableOffset(offset: { x: number; y: number }): { x: number; y: number } {
        if (!overlayElement || !dialogElement || !titleBarElement) return offset
        const frame = dragFrameOf(overlayElement, dialogElement, titleBarElement)
        return frame ? clampDragOffset(offset, frame) : offset
    }

    /**
     * Pulls the panel back after the window shrinks or the panel changes size, so
     * nothing but a drag can leave the title bar out of reach. A no-op when it's
     * already reachable, so it never fights a drag.
     */
    function keepTitleBarReachable() {
        const next = reachableOffset(dialogPosition)
        if (next.x !== dialogPosition.x || next.y !== dialogPosition.y) dialogPosition = next
    }

    /**
     * Moves the panel with the pointer, stopping it where the title bar would leave
     * the overlay. The pointer itself roams free (a webview can't confine it); the
     * panel picks it back up once it returns past where the panel stopped.
     */
    function handleTitlePointerDown(event: PointerEvent) {
        if (!draggable) return
        if ((event.target as HTMLElement).tagName === 'BUTTON') return

        event.preventDefault()
        // A second press takes over rather than leaving the first drag's listeners running.
        stopActiveDrag?.()
        isDragging = true

        const startX = event.clientX - dialogPosition.x
        const startY = event.clientY - dialogPosition.y

        const handlePointerMove = (moveEvent: PointerEvent) => {
            dialogPosition = reachableOffset({ x: moveEvent.clientX - startX, y: moveEvent.clientY - startY })
        }

        const handlePointerUp = () => {
            isDragging = false
            document.removeEventListener('pointermove', handlePointerMove)
            document.removeEventListener('pointerup', handlePointerUp)
            document.removeEventListener('pointercancel', handlePointerUp)
            document.body.style.cursor = ''
            document.body.style.userSelect = ''
            stopActiveDrag = null
        }

        document.addEventListener('pointermove', handlePointerMove)
        document.addEventListener('pointerup', handlePointerUp)
        document.addEventListener('pointercancel', handlePointerUp)
        document.body.style.cursor = 'move'
        document.body.style.userSelect = 'none'
        stopActiveDrag = handlePointerUp
    }

    /** Drags one edge or corner, keeping the OPPOSITE edge where it is (`resizeStep`). */
    function handleResizePointerDown(event: PointerEvent, direction: ResizeDirection) {
        const el = dialogElement
        if (!el) return
        // Stops the drag from selecting text across the dialog it passes over.
        event.preventDefault()
        // A second press (a second finger, a button chord) takes over rather than
        // leaving the first drag's listeners running alongside this one.
        stopActiveDrag?.()

        // The band's own `cursor` is the one source for which arrow this direction shows;
        // the body borrows it for the drag rather than repeating the mapping in script.
        const bandCursor =
            event.currentTarget instanceof HTMLElement ? getComputedStyle(event.currentTarget).cursor : ''
        const rect = el.getBoundingClientRect()
        const start = {
            direction,
            startWidth: rect.width,
            startHeight: rect.height,
            startPosition: { ...dialogPosition },
            limits: resizeLimitsOf(el),
            verticalDrift: align === 'center' && anchoredTop === null ? 0.5 : 0,
        }
        const [startX, startY] = [event.clientX, event.clientY]
        // An edge stops at the window's edges, so the north one can't carry the title bar
        // out of reach. `null` with no layout to measure (jsdom).
        const overlayRect = overlayElement?.getBoundingClientRect()
        const area = overlayRect && overlayRect.width > 0 && overlayRect.height > 0 ? overlayRect : null

        const handlePointerMove = (moveEvent: PointerEvent) => {
            const raw = { x: moveEvent.clientX, y: moveEvent.clientY }
            const pointer = area ? clampPointToArea(raw, area) : raw
            const next = resizeStep({
                ...start,
                dragged: resizedSize,
                dx: pointer.x - startX,
                dy: pointer.y - startY,
            })
            resizedSize = next.size
            dialogPosition = next.position
        }

        const handlePointerUp = () => {
            document.removeEventListener('pointermove', handlePointerMove)
            document.removeEventListener('pointerup', handlePointerUp)
            document.removeEventListener('pointercancel', handlePointerUp)
            document.body.style.cursor = ''
            document.body.style.userSelect = ''
            stopActiveDrag = null
            // The user's own size from here on, whatever the caller remembered.
            restoredHeightTaken = true
            onResizeEnd?.({ ...resizedSize })
        }

        document.addEventListener('pointermove', handlePointerMove)
        document.addEventListener('pointerup', handlePointerUp)
        document.addEventListener('pointercancel', handlePointerUp)
        // Hold the resize cursor for the whole drag, even where the pointer wanders off
        // the band and over the panel's own content.
        document.body.style.cursor = bandCursor
        document.body.style.userSelect = 'none'
        stopActiveDrag = handlePointerUp
    }

    function handleOverlayKeydown(event: KeyboardEvent) {
        event.stopPropagation()
        // The consumer owns Escape and Enter (see `ownsKeyboard`); forward untouched.
        if (ownsKeyboard) {
            onkeydown?.(event)
            return
        }
        if (event.key === 'Escape' && escapeAction) {
            // ❌ Never drop this. An Escape the webview leaves unprevented goes back to
            // AppKit, which reads it as `cancelOperation:` and takes a full-screen window
            // out of full screen, so closing the dialog would ALSO restore the window.
            event.preventDefault()
            escapeAction()
            return
        }
        // When a button is focused (user Tab'd to it), let the browser activate
        // that button on Enter instead of firing the dialog's default action.
        // Without this, Tab'ing to Cancel and pressing Enter would still trigger
        // Copy/Move/etc., which surprises users coming from Windows or the web.
        if (event.key === 'Enter' && event.target instanceof HTMLButtonElement) {
            return
        }
        onkeydown?.(event)
    }

    /**
     * Scrim click (never a click that bubbled up from the panel) closes, when opted in.
     *
     * ❌ Keep the target check FIRST, before any prop read. Every click inside the panel
     * bubbles here, including the one on ×, or on a button that just closed the dialog,
     * and by then the host may have torn down the data its props derive from; reading
     * `onclose` re-evaluates that getter and throws (the Selection dialog's `config`
     * did, on every commit click).
     */
    function handleOverlayClick(event: MouseEvent) {
        if (event.target !== event.currentTarget) return
        if (!closeOnOverlayClick || !onclose) return
        onclose()
    }

    /**
     * Keeps the MCP close registry pointing at the CURRENT `onclose`, so the
     * `dialog` tool's generic close runs the live primitive and the entry goes
     * away with the dialog.
     *
     * ❌ Never move this back to a register-on-mount / unregister-on-destroy
     * pair. `onclose` may be conditional (`TransferProgressDialog` withdraws it
     * while a conflict is on screen), which gives it a new identity on every
     * flip; the registry unregisters BY identity, so the mount-time arrow would
     * outlive the dialog and MCP would answer `true` for a dialog that isn't
     * there. An effect re-registers on each change and its cleanup runs on
     * destroy, covering both.
     *
     * No `onclose` means no dismiss affordance, so the dialog stays non-closable
     * (an honest tool failure over a silent no-op).
     */
    $effect(() => {
        if (!dialogId || !onclose) return
        const close = onclose
        registerDialogClose(dialogId, close)
        return () => { unregisterDialogClose(dialogId, close) }
    })

    let guard: StartedElsewhereGuard | null = null
    let armed = $state(false)

    // Its own hook, ahead of the one below, so no key slips in ahead of it.
    onMount(() => {
        if (!guardedFor) return
        guard = attachStartedElsewhereGuard({
            scope: () => overlayElement,
            parkFocus: () => overlayElement,
            tabMovesFocus: true,
            onArmedChange: (next) => {
                armed = next
            },
        })
    })

    onMount(async () => {
        focusOrigin = captureFocusOrigin()
        if (dialogId) {
            markDialogOpen(dialogId)
            void notifyDialogOpened(dialogId)
        }
        await tick()
        // Only if nothing inside claimed focus first. A child that autofocuses (the
        // name box in the New folder / New file dialogs) mounts BEFORE this component,
        // so its post-`tick()` `.focus()` resolves first; focusing the scrim
        // unconditionally would take it straight back and force the user to click the
        // field before typing (#84). Otherwise the first field a person can type in,
        // so a dialog opened from a context menu or a shortcut takes keys at once, and
        // the scrim only when there is no such field (Escape and Enter still land).
        //
        // A guarded dialog parks on the scrim whatever a child claimed: text that lands in a field
        // has to follow a click or Tab.
        if (guardedFor) {
            overlayElement?.focus()
        } else if (overlayElement && !overlayElement.contains(document.activeElement)) {
            if (!dialogElement || !focusFirstField(dialogElement)) overlayElement.focus()
        }

        // The overlay tracks the window; the panel grows with its body or a resize. Either
        // can strand a dragged panel's title bar, and both report after layout, so this
        // also runs after `anchorToCurrentCenter` has re-pinned a `growDownward` panel.
        if (overlayElement && dialogElement) {
            reachObserver = new ResizeObserver(keepTitleBarReachable)
            reachObserver.observe(overlayElement)
            reachObserver.observe(dialogElement)
        }

        if (!growDownward || !dialogElement) return
        anchorToCurrentCenter()
        // Height changes come from the body (a mode switch, an expanding section),
        // not from anything this component drives, so observe rather than react.
        heightObserver = new ResizeObserver(clampAnchorIntoView)
        heightObserver.observe(dialogElement)
        window.addEventListener('resize', anchorToCurrentCenter)
    })

    onDestroy(() => {
        guard?.detach()
        // A dialog can close mid-drag (Escape, or an operation finishing under it);
        // without this its pointer listeners would outlive it on `document`.
        stopActiveDrag?.()
        heightObserver?.disconnect()
        heightObserver = null
        reachObserver?.disconnect()
        reachObserver = null
        if (growDownward) window.removeEventListener('resize', anchorToCurrentCenter)
        if (dialogId) {
            markDialogClosed(dialogId)
            void notifyDialogClosed(dialogId)
        }
        // Restore focus to wherever it was before the dialog opened (`focus-restore.ts`). An
        // element unmounted while the dialog was up (a rename input) is skipped.
        restoreFocus(focusOrigin, { leaving: overlayElement })
    })
</script>

<div
    bind:this={overlayElement}
    class="modal-overlay {overlayClass}"
    class:blur
    class:topmost
    class:align-top={align === 'top'}
    {role}
    aria-modal="true"
    aria-labelledby={titleId}
    aria-describedby={ariaDescribedby}
    data-dialog-id={dialogId}
    data-armed={guardedFor ? armed : undefined}
    tabindex="-1"
    onkeydown={handleOverlayKeydown}
    onclick={handleOverlayClick}
    use:trapFocus={{ onEscape: escapeAction }}
>
    <div
        bind:this={dialogElement}
        class="modal-dialog"
        class:dragging={isDragging}
        class:resizable={resizable !== false}
        class:fill-body={fillBody}
        class:grow-downward={growDownward}
        style={containerStyle}
    >
        {#if onclose}
            <!--
                tabindex=-1 keeps the × out of the tab cycle. The dialog's action buttons
                should be the only tab stops; × is a mouse / Escape-key affordance. Without
                this, Tab from the overlay lands on × first, which surprises keyboard users
                expecting the primary or first action to be the entry point.
            -->
            <button class="modal-close-button" onclick={onclose} aria-label={tString('ui.modalDialog.close')} tabindex="-1">×</button>
        {/if}
        <!--
            The panel's own content, in the element that clips it to the rounded
            corners. The clip can't live on the panel: the resize bands are panel
            children that deliberately hang over its edge, and `overflow: hidden`
            up there would cut them off.
        -->
        <div class="modal-content">
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div class="dialog-title-bar" class:draggable bind:this={titleBarElement} onpointerdown={handleTitlePointerDown}>
                <h2 id={titleId}>
                    {@render title()}
                </h2>
            </div>
            <div class="modal-body" class:no-footer={!footer}>
                {@render children()}
            </div>
            {#if guardedFor}
                <p class="started-elsewhere" role="status">{startedElsewhereLine(guardedFor, armed)}</p>
            {/if}
            {#if footer}
                <div class="modal-footer">
                    {#if footerLeading}<div class="modal-footer-leading">{@render footerLeading()}</div>{/if}
                    <div class="modal-footer-actions" bind:this={footerActionsElement}>{@render footer()}</div>
                </div>
            {/if}
        </div>
        {#each resizeDirections as direction (direction)}
            <!-- The direction rides on an attribute, not a class per edge: one static
                 `.resize-band` class keeps both the CSS scoping and `css-unused` honest. -->
            <div
                class="resize-band"
                data-direction={direction}
                aria-hidden="true"
                onpointerdown={(event) => {
                    handleResizePointerDown(event, direction)
                }}
            ></div>
        {/each}
    </div>
</div>

<style>
    .modal-overlay {
        position: fixed;
        /* Start below the title bar so the scrim never covers the OS window-drag
           region: the user can still drag the window while a dialog is open.
           `--titlebar-height` is per-window (see app.css § Window chrome). */
        inset: var(--titlebar-height) 0 0 0;
        background: var(--color-overlay-light);
        display: flex;
        align-items: center;
        justify-content: center;
        z-index: var(--z-modal);
    }

    /* The scrim takes focus on mount so Escape and the key handlers have somewhere to
       land. It's a focus-management container, never a keyboard target the user aims
       at, so the UA ring on it says nothing, and it paints as a full-width line under
       the title bar: the scrim is pinned to the viewport on three sides, so its TOP
       edge is the only ring edge that falls inside the window. WebKit draws that
       ring in `-webkit-focus-ring-color`, the macOS SYSTEM accent, so it also ignores
       the user's "App color" choice: a green line in a gold app. `aria-modal` plus
       `use:trapFocus` carry the a11y here. */
    .modal-overlay:focus {
        outline: none;
    }

    /* One rung above every other modal, so the panel AND its scrim cover a
       dialog that's already open. See the `topmost` prop. */
    .modal-overlay.topmost {
        z-index: var(--z-modal-top);
    }

    /* `align="top"`: the Spotlight placement. 10vh reads as "near the top" at any
       window height, and leaves room for an 80vh panel below it. */
    .modal-overlay.align-top {
        align-items: flex-start;
        padding-top: 10vh;
    }

    .modal-overlay.blur {
        background: var(--color-overlay-heavy);
        backdrop-filter: blur(4px);
    }

    /* Drop the scrim blur when the OS asks for reduced transparency; the dimming
       background still does its job. */
    :global(html.reduce-transparency) .modal-overlay.blur {
        backdrop-filter: none;
        -webkit-backdrop-filter: none;
    }

    /* macOS panel edge: the `border` is the darker OUTER hairline, the inset ring
       the lighter INNER one. The inset ring rides the padding-box corner, so it
       stays concentric with the border at any radius. */
    .modal-dialog {
        background: var(--color-bg-dialog);
        border: 1px solid var(--color-dialog-border-outer);
        border-radius: var(--radius-dialog);
        box-shadow:
            inset 0 0 0 1px var(--color-dialog-border-inner),
            var(--shadow-dialog);
        position: relative;
    }

    /* Opt-in user resizing: the grab bands render per exposed edge (see `.resize-band`),
       and `'horizontal'` simply exposes fewer of them, so the height stays content-driven
       and the panel can't be dragged into a band of empty space above the footer.
       Flex column so the body owns the slack and scrolls while title bar and
       footer keep their intrinsic height.
       max-* keep it inside the viewport (the overlay starts below the OS title bar).
       The width floor is the script's (`panelMinWidth`: 360px, the caller's own, or
       the footer's button row), since it has to take the larger of them. There's
       deliberately no `min-height`: every dialog opens at its natural height instead
       of padding itself out to a number, and the drag floors itself on the measured
       chrome (see `minimumPanelHeight`). The caller's `containerStyle` still sets the
       initial width/height. */
    .modal-dialog.resizable {
        display: flex;
        flex-direction: column;
        max-width: calc(100vw - 2 * var(--spacing-xl));
        max-height: calc(100vh - var(--titlebar-height) - 2 * var(--spacing-xl));
    }

    /* A `growDownward` dialog grows with its body up to the window's height, then its
       body scrolls: an Add sheet with Advanced open hid its buttons below the window. */
    .modal-dialog.grow-downward {
        display: flex;
        flex-direction: column;
        max-height: calc(100vh - var(--titlebar-height) - 2 * var(--spacing-xl));
    }

    .modal-dialog.grow-downward > .modal-content {
        display: flex;
        flex-direction: column;
        flex: 1 1 auto;
        min-height: 0;
    }

    .modal-dialog.grow-downward .modal-body {
        flex: 1 1 auto;
        min-height: 0;
        overflow: auto;
    }

    /* The clipping layer, one level below the panel so the bands can hang over the
       panel's edge. Only the two modes that own their height clip: a plain dialog is
       sized by its content and has nothing to cut off. The panel's 1px border sits
       outside this box, so its corner curve is a pixel tighter than the panel's. */
    .modal-dialog.resizable > .modal-content,
    .modal-dialog.fill-body > .modal-content {
        display: flex;
        flex-direction: column;
        flex: 1 1 auto;
        min-height: 0;
        overflow: hidden;
        border-radius: calc(var(--radius-dialog) - 1px);
    }

    .modal-dialog.resizable .modal-body {
        flex: 1 1 auto;
        min-height: 0;
        overflow: auto;
    }

    /* `fillBody`: a fixed-height frame (the caller caps it via `containerStyle`).
       Same flex column as `resizable`, and `.modal-content` clips full-bleed bands
       to the dialog's radius — but the body does NOT scroll here: it's a
       column whose own child takes the slack and owns the scrolling. */
    .modal-dialog.fill-body {
        display: flex;
        flex-direction: column;
    }

    .modal-dialog.fill-body .modal-body {
        flex: 1 1 auto;
        min-height: 0;
        display: flex;
        flex-direction: column;
    }

    /* Both at once: the bands, the floors, and the viewport caps come from `resizable`,
       the body layout from `fillBody`. The body must NOT scroll here — its child region
       already does, and two nested scrollers means the user drags one scrollbar and the
       other one moves. */
    .modal-dialog.resizable.fill-body .modal-body {
        overflow: hidden;
    }

    /* The resize grab zones: one per exposed edge, plus the four corners when both axes
       are free. Each straddles the panel edge (most of the band in the scrim, a sliver
       inside), which is how a macOS window behaves and what keeps a scrollbar at the
       body's right edge grabbable. Corners come last in the DOM so they win the hit test
       where they overlap the two edges they join. The `cursor` here is also what the
       body wears for the duration of the drag. */
    .resize-band {
        position: absolute;
    }

    .resize-band[data-direction='n'],
    .resize-band[data-direction='s'] {
        left: 0;
        right: 0;
        height: 7px;
        cursor: ns-resize;
    }

    .resize-band[data-direction='n'] {
        top: -4px;
    }

    .resize-band[data-direction='s'] {
        bottom: -4px;
    }

    .resize-band[data-direction='w'],
    .resize-band[data-direction='e'] {
        top: 0;
        bottom: 0;
        width: 7px;
        cursor: ew-resize;
    }

    .resize-band[data-direction='w'] {
        left: -4px;
    }

    .resize-band[data-direction='e'] {
        right: -4px;
    }

    .resize-band[data-direction='nw'],
    .resize-band[data-direction='ne'],
    .resize-band[data-direction='sw'],
    .resize-band[data-direction='se'] {
        width: var(--spacing-xl);
        height: var(--spacing-xl);
    }

    .resize-band[data-direction='nw'],
    .resize-band[data-direction='se'] {
        cursor: nwse-resize;
    }

    .resize-band[data-direction='ne'],
    .resize-band[data-direction='sw'] {
        cursor: nesw-resize;
    }

    .resize-band[data-direction='nw'] {
        top: -14px;
        left: -14px;
    }

    .resize-band[data-direction='ne'] {
        top: -14px;
        right: -14px;
    }

    .resize-band[data-direction='sw'] {
        bottom: -14px;
        left: -14px;
    }

    .resize-band[data-direction='se'] {
        bottom: -14px;
        right: -14px;
    }

    /* Fixed square + `--radius-full` so the hover fill is a circle around the glyph,
       not a rounded rectangle. Sized rather than padded, because the `×` glyph's
       own metrics aren't symmetric enough for padding alone to stay round. */
    .modal-close-button {
        position: absolute;
        top: var(--spacing-md);
        right: var(--spacing-md);
        z-index: 1;
        display: flex;
        align-items: center;
        justify-content: center;
        width: 26px;
        height: 26px;
        padding: 0;
        background: none;
        border: none;
        color: var(--color-text-secondary);
        font-size: var(--font-size-xl);
        line-height: var(--font-line-height-flat);
        border-radius: var(--radius-full);
    }

    .modal-close-button:hover {
        background: var(--color-bg-tertiary);
        color: var(--color-text-primary);
    }

    .modal-dialog.dragging {
        cursor: move;
    }

    /* One inset all round: the gap below the title and above the footer match the
       dialog's own `--spacing-dialog`, so nothing crowds the title or the
       action row. Bodies may add more, never less. */
    .dialog-title-bar {
        padding: var(--spacing-dialog);
        user-select: none;
    }

    .dialog-title-bar.draggable {
        cursor: move;
    }

    /* The title is a ROW, not a line of inline content: the words, plus whatever a
       dialog puts beside them (a `StatusBadge`, an icon). Flex is what gives the
       badge a real gap from the last word and puts its BOX on the title's optical
       middle. Left inline, the badge shares the title's text baseline, so its
       padded background hangs below the words. */
    h2 {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
        margin: 0;
        font-size: var(--font-size-lg);
        font-weight: 600;
        color: var(--color-text-primary);
        text-align: left;
    }

    /* Body padding, owned here and NOT opt-outable: every dialog's content lines up
       with the title and the action row, and a new section can't forget the inset.
       The title bar's bottom padding supplies the top gap, the footer the bottom.
       A block that needs to reach the panel edge (a full-bleed divider or scroll
       region) cancels it locally with a negative inline margin. */
    .modal-body {
        padding: 0 var(--spacing-dialog);
        /* A path, a URL, or a long filename has no break opportunity inside its last
           segment, so without this it overflows the inset and runs to the panel edge.
           ❌ Not `anywhere`: that also drops every flex/grid item's minimum to one
           character, so a squeezed row broke "You" into "Yo / u" (#700). `break-word`
           breaks only a word that alone outgrows the line, but it leaves the
           min-content width alone, so a flex/grid item holding a path must shrink on
           its own terms (`min-width: 0`, plus an ellipsis or the middle truncation). */
        overflow-wrap: break-word;
    }

    /* Footerless dialogs: the body owns the bottom padding the footer would give. */
    .modal-body.no-footer {
        padding-bottom: var(--spacing-dialog);
    }

    .started-elsewhere {
        margin: var(--spacing-md) var(--spacing-dialog) 0;
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }

    /* The action a person is about to take reads as not ready yet while the keyboard can't reach it.
       Opacity would drop the label below AA, so it trades the fill for tokens that hold it. A click
       still works: only `dialog_confirm` and the mouse get here, and neither is a stray key. */
    /* allowed-btn-restyle: the one unarmed look, for every dialog this primitive hosts */
    .modal-overlay[data-armed='false'] .modal-footer-actions :global(.btn-primary),
    .modal-overlay[data-armed='false'] .modal-footer-actions :global(.btn-danger) {
        background: var(--color-bg-tertiary);
        color: var(--color-text-primary);
        border-color: var(--color-border-strong);
    }

    /* Right-aligned action footer (macOS convention: primary action rightmost).
       Owns the dialog's bottom padding so callers don't repeat per-dialog button-row CSS. */
    .modal-footer {
        display: flex;
        flex-wrap: wrap;
        align-items: center;
        justify-content: flex-end;
        gap: var(--spacing-md);
        padding: var(--spacing-dialog);
    }

    /* `margin-right: auto` eats the slack, so the buttons stay hard right no matter
       how wide the leading content is. */
    .modal-footer-leading {
        margin-right: auto;
        min-width: 0;
    }

    /* The buttons are one row that never shrinks, never wraps, and never wraps a
       label: the panel's floor rises to fit it (`footerRowPx` in the script). When
       the footer can't also hold the leading control beside them (a longer
       translation, a third button), the whole group drops to a row of its own, still
       hard right, rather than squeezing each label onto two lines. */
    .modal-footer-actions {
        display: flex;
        flex-shrink: 0;
        flex-wrap: nowrap;
        justify-content: flex-end;
        align-items: center;
        gap: var(--spacing-md);
        margin-left: auto;
        white-space: nowrap;
    }
</style>
