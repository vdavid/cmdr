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
         * Standard body padding (`--spacing-dialog`, horizontal). ModalDialog owns it so
         * dialogs don't hand-roll their own. Set `false` for full-bleed bodies
         * that manage their own padding (e.g. edge-to-edge lists).
         */
        padded?: boolean
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
        padded = true,
        resizable = false,
        growDownward = false,
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

    const dialogStyle = $derived(
        `transform: translate(${String(dialogPosition.x)}px, ${String(dialogPosition.y)}px);` +
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
    class="modal-overlay"
    class:blur
    {role}
    aria-modal="true"
    aria-labelledby={titleId}
    aria-describedby={ariaDescribedby}
    data-dialog-id={dialogId}
    tabindex="-1"
    onkeydown={handleOverlayKeydown}
    use:trapFocus={{ onEscape: onclose }}
>
    <div
        bind:this={dialogElement}
        class="modal-dialog"
        class:dragging={isDragging}
        class:resizable
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
        <div class="modal-body" class:no-footer={!footer} class:flush={!padded}>
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
        background: var(--color-bg-secondary);
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

    h2 {
        margin: 0;
        font-size: var(--font-size-lg);
        font-weight: 600;
        color: var(--color-text-primary);
        text-align: left;
    }

    /* Standard body padding, owned here so dialogs don't hand-roll it. The
       horizontal inset matches the title bar and footer; the title bar's bottom
       padding supplies the top gap and the footer supplies the bottom. */
    .modal-body {
        padding: 0 var(--spacing-dialog);
    }

    /* Footerless dialogs: the body owns the bottom padding the footer would give. */
    .modal-body.no-footer {
        padding-bottom: var(--spacing-dialog);
    }

    /* Full-bleed opt-out (`padded={false}`): the body manages its own padding. */
    .modal-body.flush {
        padding: 0;
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
