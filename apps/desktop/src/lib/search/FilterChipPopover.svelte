<script lang="ts">
    /**
     * Generic popover that floats below (or above, on flip) an anchor element.
     *
     * Used by the filter-chip strip in `SearchFilterChips.svelte`. The look mirrors macOS
     * popovers: small radius, frosted-glass material (matching the tooltip primitive), hairline
     * border, soft drop shadow. The popover positions itself relative to the anchor's viewport
     * rect on open and re-runs on resize, auto-flipping above when there isn't room below.
     *
     * Focus contract:
     *   - On open, focus moves to the first focusable element inside the popover (slot content).
     *   - Tab cycles within the popover; Shift+Tab wraps in reverse. Focus never escapes while open.
     *   - Esc closes the popover and returns focus to the anchor. The keydown handler is on the
     *     popover itself and calls `stopPropagation`, so the dialog's capture-phase Escape doesn't
     *     also fire and close the whole dialog.
     *   - Enter inside the popover fires `onConfirm` (which the parent typically wires to "close").
     *     Native form controls (input, select, button) consume Enter themselves before this fires.
     *
     * Click-outside closes too. The check looks at the mousedown target rather than click, so a
     * mousedown inside followed by mouseup outside (a drag) doesn't accidentally close.
     */
    import { onMount, onDestroy, tick, type Snippet } from 'svelte'

    interface Props {
        /** The trigger element. Used for positioning and as the focus-return target. */
        anchor: HTMLElement
        /** Whether the popover is shown. Owned by the parent (controlled component). */
        open: boolean
        /** Fired when the popover wants to close (Esc, click outside, or Enter via `onConfirm`). */
        onClose: () => void
        /** Optional: aria-label for the popover region (defaults to "Filter options"). */
        ariaLabel?: string
        children: Snippet
    }

    const { anchor, open, onClose, ariaLabel = 'Filter options', children }: Props = $props()

    let popoverEl: HTMLDivElement | undefined = $state()
    let position = $state<{ left: number; top: number; flipped: boolean }>({ left: 0, top: 0, flipped: false })

    const VIEWPORT_MARGIN = 8
    const OFFSET = 6

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
            // Stop propagation so the dialog's capture-phase Escape handler doesn't also fire and
            // close the whole dialog. This is the contract documented in the search dialog's
            // CLAUDE.md.
            e.stopPropagation()
            onClose()
            // Focus returns to the anchor so the user keeps their place in the chip row.
            anchor.focus()
            return
        }
        if (e.key === 'Tab') {
            const focusables = focusableElements()
            if (focusables.length === 0) {
                e.preventDefault()
                return
            }
            const first = focusables[0]
            const last = focusables[focusables.length - 1]
            if (e.shiftKey && document.activeElement === first) {
                e.preventDefault()
                last.focus()
            } else if (!e.shiftKey && document.activeElement === last) {
                e.preventDefault()
                first.focus()
            }
        }
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
        class="filter-chip-popover"
        role="dialog"
        aria-label={ariaLabel}
        data-flipped={position.flipped}
        style:left="{position.left}px"
        style:top="{position.top}px"
        onkeydown={handleKeyDown}
        tabindex="-1"
    >
        {@render children()}
    </div>
{/if}

<style>
    .filter-chip-popover {
        position: fixed;
        z-index: var(--z-dropdown);

        /* Frosted-glass material via shared design tokens. Same translucency, blur, and hairline
           as the tooltip primitive — that's the contract the search-redesign-plan calls out:
           "Reuse the tooltip's frosted-glass material values exactly." See `app.css` §
           Frosted-glass material for the underlying values. */
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
</style>
