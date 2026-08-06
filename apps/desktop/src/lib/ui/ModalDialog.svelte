<script lang="ts">
    import { onMount, onDestroy, tick } from 'svelte'
    import type { Snippet } from 'svelte'
    import { notifyDialogOpened, notifyDialogClosed } from '$lib/tauri-commands'
    import { trapFocus } from './focus-trap'
    import type { SoftDialogId } from './dialog-registry'
    import { registerDialogClose, unregisterDialogClose } from './dialog-close-registry'
    import { tString } from '$lib/intl/messages.svelte'

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
         * Lets the user drag the bottom-right corner to resize the dialog. The
         * body region grows and scrolls; the caller still passes the initial
         * size via `containerStyle`. Off by default.
         */
        resizable?: boolean
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
         * of this (with a scrolling body); don't combine the two.
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
        /** Renders × button and handles Escape key */
        onclose?: () => void
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
        growDownward = false,
        align = 'center',
        fillBody = false,
        ownsKeyboard = false,
        overlayClass = '',
        closeOnOverlayClick = false,
        onclose,
    }: Props = $props()

    let overlayElement: HTMLDivElement | undefined = $state()
    let dialogElement: HTMLDivElement | undefined = $state()
    let dialogPosition = $state({ x: 0, y: 0 })
    let isDragging = $state(false)
    /** Distance from the overlay's top to the dialog's top, once `growDownward` pins it. */
    let anchoredTop = $state<number | null>(null)
    /**
     * Element that had focus when the dialog opened. Restored on destroy so
     * keyboard input flows back to wherever it came from (typically a file
     * pane): without this, focus falls to <body> and arrow keys silently
     * no-op until the user clicks back into a pane.
     */
    let previousActiveElement: HTMLElement | null = null
    let heightObserver: ResizeObserver | null = null

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
     * subtree). `Menu` and `Select` portal to `document.body` and are immune.
     *
     * `left` / `top` shift the panel visually without reflowing siblings (same as the
     * transform did) and establish no containing block. `will-change: transform` is the
     * tempting "smooth out the drag" change that would bring the bug straight back.
     */
    const dialogStyle = $derived(
        `left: ${String(dialogPosition.x)}px; top: ${String(dialogPosition.y)}px;` +
            (anchoredTop === null ? '' : ` align-self: flex-start; margin-top: ${String(anchoredTop)}px;`) +
            (containerStyle ? ` ${containerStyle}` : ''),
    )

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
        anchoredTop = centeredTop()
    }

    /**
     * Keeps a grown dialog on screen. The pin is a top edge, so a body that grows
     * past the overlay's bottom would be clipped; pull it up by exactly the
     * overflow, never past the top.
     */
    function clampAnchorIntoView() {
        if (anchoredTop === null || !overlayElement || !dialogElement) return
        const maxTop = Math.max(0, overlayElement.clientHeight - dialogElement.offsetHeight)
        if (anchoredTop > maxTop) anchoredTop = maxTop
    }

    function handleTitleMouseDown(event: MouseEvent) {
        if (!draggable) return
        if ((event.target as HTMLElement).tagName === 'BUTTON') return

        event.preventDefault()
        isDragging = true

        const startX = event.clientX - dialogPosition.x
        const startY = event.clientY - dialogPosition.y

        const handleMouseMove = (e: MouseEvent) => {
            dialogPosition = {
                x: e.clientX - startX,
                y: e.clientY - startY,
            }
        }

        const handleMouseUp = () => {
            isDragging = false
            document.removeEventListener('mousemove', handleMouseMove)
            document.removeEventListener('mouseup', handleMouseUp)
            document.body.style.cursor = ''
        }

        document.addEventListener('mousemove', handleMouseMove)
        document.addEventListener('mouseup', handleMouseUp)
        document.body.style.cursor = 'move'
    }

    function handleOverlayKeydown(event: KeyboardEvent) {
        event.stopPropagation()
        // The consumer owns Escape and Enter (see `ownsKeyboard`); forward untouched.
        if (ownsKeyboard) {
            onkeydown?.(event)
            return
        }
        if (event.key === 'Escape' && onclose) {
            onclose()
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

    /** Scrim click (never a click that bubbled up from the panel) closes, when opted in. */
    function handleOverlayClick(event: MouseEvent) {
        if (!closeOnOverlayClick || !onclose) return
        if (event.target !== event.currentTarget) return
        onclose()
    }

    onMount(async () => {
        previousActiveElement = document.activeElement instanceof HTMLElement ? document.activeElement : null
        if (dialogId) {
            void notifyDialogOpened(dialogId)
            // Register the close primitive so the MCP `dialog` tool's generic close can
            // dismiss this dialog by id. Only when `onclose` exists — a dialog with no
            // dismiss affordance stays non-closable (an honest tool failure over a
            // silent no-op).
            if (onclose) registerDialogClose(dialogId, onclose)
        }
        await tick()
        overlayElement?.focus()

        if (!growDownward || !dialogElement) return
        anchorToCurrentCenter()
        // Height changes come from the body (a mode switch, an expanding section),
        // not from anything this component drives, so observe rather than react.
        heightObserver = new ResizeObserver(clampAnchorIntoView)
        heightObserver.observe(dialogElement)
        window.addEventListener('resize', anchorToCurrentCenter)
    })

    onDestroy(() => {
        heightObserver?.disconnect()
        heightObserver = null
        if (growDownward) window.removeEventListener('resize', anchorToCurrentCenter)
        if (dialogId) {
            void notifyDialogClosed(dialogId)
            if (onclose) unregisterDialogClose(dialogId, onclose)
        }
        // Restore focus to whatever had it before the dialog opened. The connected-check
        // skips elements that were unmounted while the dialog was up (e.g., a rename input).
        if (previousActiveElement?.isConnected) {
            previousActiveElement.focus()
        }
    })
</script>

<div
    bind:this={overlayElement}
    class="modal-overlay {overlayClass}"
    class:blur
    class:align-top={align === 'top'}
    {role}
    aria-modal="true"
    aria-labelledby={titleId}
    aria-describedby={ariaDescribedby}
    data-dialog-id={dialogId}
    tabindex="-1"
    onkeydown={handleOverlayKeydown}
    onclick={handleOverlayClick}
    use:trapFocus={{ onEscape: onclose }}
>
    <div
        bind:this={dialogElement}
        class="modal-dialog"
        class:dragging={isDragging}
        class:resizable
        class:fill-body={fillBody}
        style={dialogStyle}
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
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div class="dialog-title-bar" class:draggable onmousedown={handleTitleMouseDown}>
            <h2 id={titleId}>
                {@render title()}
            </h2>
        </div>
        <div class="modal-body" class:no-footer={!footer}>
            {@render children()}
        </div>
        {#if footer}
            <div class="modal-footer">
                {#if footerLeading}<div class="modal-footer-leading">{@render footerLeading()}</div>{/if}
                {@render footer()}
            </div>
        {/if}
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

    /* Opt-in user resizing: the native corner grip lives at the bottom-right.
       Flex column so the body owns the slack and scrolls while title bar and
       footer keep their intrinsic height. `overflow: hidden` both clips the
       rounded corners and gives `resize` a scroll container to grab.
       min-* keep the dialog usable when dragged small; max-* keep it inside the
       viewport (the overlay starts below the OS title bar). The caller's
       `containerStyle` still sets the initial width/height. */
    .modal-dialog.resizable {
        display: flex;
        flex-direction: column;
        resize: both;
        overflow: hidden;
        /* No design token for these floors; they're layout minimums, not spacing. */
        min-width: 360px;
        min-height: 240px;
        max-width: calc(100vw - 2 * var(--spacing-xl));
        max-height: calc(100vh - var(--titlebar-height) - 2 * var(--spacing-xl));
    }

    .modal-dialog.resizable .modal-body {
        flex: 1 1 auto;
        min-height: 0;
        overflow: auto;
    }

    /* `fillBody`: a fixed-height frame (the caller caps it via `containerStyle`).
       Same flex column as `resizable`, and `overflow: hidden` so full-bleed bands
       clip to the dialog's radius — but the body does NOT scroll here: it's a
       column whose own child takes the slack and owns the scrolling. */
    .modal-dialog.fill-body {
        display: flex;
        flex-direction: column;
        overflow: hidden;
    }

    .modal-dialog.fill-body .modal-body {
        flex: 1 1 auto;
        min-height: 0;
        display: flex;
        flex-direction: column;
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
        line-height: 1;
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
           `anywhere` (not `break-word`) also lets the token shrink the min-content
           width, so a flex/grid child can't be widened by it either. */
        overflow-wrap: anywhere;
    }

    /* Footerless dialogs: the body owns the bottom padding the footer would give. */
    .modal-body.no-footer {
        padding-bottom: var(--spacing-dialog);
    }

    /* Right-aligned action footer (macOS convention: primary action rightmost).
       Owns the dialog's bottom padding so callers don't repeat per-dialog button-row CSS. */
    .modal-footer {
        display: flex;
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
</style>
