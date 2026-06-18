<script lang="ts">
    /**
     * Popover: a generic popover that floats below (or above, on flip) an anchor element.
     *
     * The look mirrors macOS popovers: small radius, frosted-glass material (matching the
     * tooltip primitive), hairline border, soft drop shadow. The popover positions itself
     * relative to the anchor's viewport rect on open and re-runs on resize, auto-flipping
     * above when there isn't room below.
     *
     * The query dialogs' filter chips (`query-ui/filter-chips/`) and the recent-items popover
     * (`query-ui/recent-items/`) both float their surfaces through this. `FilterPopover.svelte`
     * composes it for the labelled-grid filter surface.
     *
     * Focus contract:
     *   - On open, focus moves to the first focusable element inside the popover (slot content).
     *   - Tab cycles within the popover; Shift+Tab wraps in reverse. Focus never escapes while open.
     *   - Esc closes the popover and returns focus to the anchor. The keydown handler is on the
     *     popover itself and calls `stopPropagation`, so a host dialog's capture-phase Escape doesn't
     *     also fire and close the whole dialog. Host dialogs detect an open popover by the
     *     `.ui-popover` class and defer Escape to it (see `query-ui/QueryDialog.svelte`).
     *
     * Click-outside closes too. The check looks at the mousedown target rather than click, so a
     * mousedown inside followed by mouseup outside (a drag) doesn't accidentally close.
     */
    import { onMount, onDestroy, tick, type Snippet } from 'svelte'
    import { trapFocus } from '$lib/ui/focus-trap'
    import { tString } from '$lib/intl/messages.svelte'

    interface Props {
        /** The trigger element. Used for positioning and as the focus-return target. */
        anchor: HTMLElement
        /** Whether the popover is shown. Owned by the parent (controlled component). */
        open: boolean
        /** Fired when the popover wants to close (Esc, click outside, or Enter via `onConfirm`). */
        onClose: () => void
        /** Optional: aria-label for the popover region (defaults to "Options"). */
        ariaLabel?: string
        children: Snippet
    }

    const { anchor, open, onClose, ariaLabel, children }: Props = $props()
    const resolvedAriaLabel = $derived(ariaLabel ?? tString('ui.popover.defaultAriaLabel'))

    let popoverEl: HTMLDivElement | undefined = $state()
    let position = $state<{ left: number; top: number; flipped: boolean }>({ left: 0, top: 0, flipped: false })

    const VIEWPORT_MARGIN = 8
    const OFFSET = 2

    /** Repositions the popover relative to its anchor. Auto-flips above when needed. */
    function reposition(): void {
        // anchor is a required prop (HTMLElement), so no anchor-null check.
        if (!popoverEl) return
        const anchorRect = anchor.getBoundingClientRect()
        const popRect = popoverEl.getBoundingClientRect()

        let left = anchorRect.left
        let top = anchorRect.bottom + OFFSET
        let flipped = false

        // Flip above if there's not enough room below.
        if (top + popRect.height > window.innerHeight - VIEWPORT_MARGIN) {
            const flippedTop = anchorRect.top - popRect.height - OFFSET
            if (flippedTop >= VIEWPORT_MARGIN) {
                top = flippedTop
                flipped = true
            } else {
                // Pin to the bottom of the viewport if neither fits cleanly.
                top = Math.max(VIEWPORT_MARGIN, window.innerHeight - popRect.height - VIEWPORT_MARGIN)
            }
        }

        // Clamp horizontally so the popover stays on screen.
        left = Math.max(VIEWPORT_MARGIN, Math.min(left, window.innerWidth - popRect.width - VIEWPORT_MARGIN))
        position = { left, top, flipped }
    }

    /** Lists every focusable element currently inside the popover, in DOM order. */
    function focusableElements(): HTMLElement[] {
        if (!popoverEl) return []
        return Array.from(
            popoverEl.querySelectorAll<HTMLElement>(
                'input:not([disabled]), select:not([disabled]), textarea:not([disabled]), button:not([disabled]), [tabindex]:not([tabindex="-1"])',
            ),
        )
    }

    /** Moves focus to the first focusable element inside the popover. */
    async function focusFirst(): Promise<void> {
        await tick()
        const focusables = focusableElements()
        focusables[0]?.focus()
    }

    function handleKeyDown(e: KeyboardEvent): void {
        if (e.key === 'Escape') {
            e.preventDefault()
            // Stop propagation so a host dialog's capture-phase Escape handler doesn't also fire
            // and close the whole dialog. This is the contract documented in the query dialog's
            // CLAUDE.md.
            e.stopPropagation()
            closeAndReturnFocus()
        }
        // Tab cycling is handled by `use:trapFocus` on the popover element. The popover's
        // trap mounts above the host dialog's, so enforcement is scoped here while open.
    }

    function closeAndReturnFocus(): void {
        onClose()
        // Focus returns to the anchor so the user keeps their place.
        anchor.focus()
    }

    function handleDocumentMouseDown(e: MouseEvent): void {
        if (!open || !popoverEl) return
        const target = e.target as Node | null
        if (!target) return
        if (popoverEl.contains(target)) return
        if (anchor.contains(target)) return
        onClose()
    }

    function handleWindowResize(): void {
        if (open) reposition()
    }

    $effect(() => {
        if (!open) return
        // Re-run on every open: measure after the popover renders, then position and focus.
        void tick().then(() => {
            reposition()
            void focusFirst()
        })
    })

    onMount(() => {
        document.addEventListener('mousedown', handleDocumentMouseDown, true)
        window.addEventListener('resize', handleWindowResize)
    })

    onDestroy(() => {
        document.removeEventListener('mousedown', handleDocumentMouseDown, true)
        window.removeEventListener('resize', handleWindowResize)
    })
</script>

{#if open}
    <div
        bind:this={popoverEl}
        class="ui-popover"
        role="dialog"
        aria-label={resolvedAriaLabel}
        data-flipped={position.flipped}
        style:left="{position.left}px"
        style:top="{position.top}px"
        onkeydown={handleKeyDown}
        tabindex="-1"
        use:trapFocus={{ onEscape: closeAndReturnFocus }}
    >
        {@render children()}
    </div>
{/if}

<style>
    .ui-popover {
        position: fixed;
        z-index: var(--z-dropdown);

        /* Frosted-glass material via shared design tokens. Same translucency, blur, and hairline
           as the tooltip primitive — reuse the tooltip's frosted-glass material values exactly.
           See `app.css` § Frosted-glass material for the underlying values. */
        background: var(--color-bg-glass);
        -webkit-backdrop-filter: saturate(180%) blur(20px);
        backdrop-filter: saturate(180%) blur(20px);
        border: 0.5px solid var(--color-border-glass);
        border-radius: var(--radius-md);
        box-shadow: var(--shadow-md);
        padding: var(--spacing-sm);
        min-width: 220px;
        color: var(--color-text-primary);
        font-size: var(--font-size-sm);
        line-height: 1.3;
    }

    /* Reduced transparency: `--color-bg-glass` flips to opaque (in `app.css`); drop the blur here. */
    @media (prefers-reduced-transparency: reduce) {
        .ui-popover {
            -webkit-backdrop-filter: none;
            backdrop-filter: none;
        }
    }
</style>
