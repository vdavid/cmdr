<script lang="ts">
    import { tString } from '#lib/intl/messages.svelte.js'
    import { clampSplitPercent } from './pane-split'

    interface Props {
        onResize: (widthPercent: number) => void
        onResizeEnd: () => void
        onReset: () => void
    }

    const { onResize, onResizeEnd, onReset }: Props = $props()

    let isDragging = $state(false)

    function handleMouseDown(event: MouseEvent) {
        // A secondary press is a context menu, which swallows the mouseup that ends a drag.
        if (event.button !== 0) return
        event.preventDefault()
        isDragging = true

        // Capture the container reference at drag start (not during mousemove)
        const container = (event.target as HTMLElement).closest('.dual-pane-explorer')
        if (!container) return

        // How far from the line the press landed, so grabbing the wide pad doesn't jump the divider.
        const grabOffset = event.clientX - (event.currentTarget as HTMLElement).getBoundingClientRect().left

        const handleMouseMove = (moveEvent: MouseEvent) => {
            const rect = container.getBoundingClientRect()
            const mouseX = moveEvent.clientX - grabOffset - rect.left
            const widthPercent = (mouseX / rect.width) * 100

            onResize(clampSplitPercent(widthPercent))
        }

        const handleMouseUp = () => {
            isDragging = false
            onResizeEnd()
            document.removeEventListener('mousemove', handleMouseMove)
            document.removeEventListener('mouseup', handleMouseUp)
            document.body.style.cursor = ''
        }

        document.addEventListener('mousemove', handleMouseMove)
        document.addEventListener('mouseup', handleMouseUp)
        document.body.style.cursor = 'col-resize'
    }

    /**
     * The strip lies over the right pane's edge, and a wheel over it would stop here. It scrolls
     * what's under the strip instead: the wheel's default action only looks at this element's own
     * ancestors, and the list isn't one. The line itself has its own 1px column in the layout, so
     * no pane lies under it when the split lands on a whole pixel (any odd window width at 50%):
     * it reads the point just past the line, the same pane the strip it belongs to scrolls.
     */
    function handleWheel(event: WheelEvent) {
        const line = (event.currentTarget as HTMLElement).getBoundingClientRect()
        const isOnLine = event.clientX >= Math.floor(line.left) && event.clientX < line.right
        const under = document
            .elementsFromPoint(isOnLine ? Math.ceil(line.right) : event.clientX, event.clientY)
            .find((element) => !element.closest('.pane-resizer'))
        for (let element = under ?? null; element; element = element.parentElement) {
            const style = getComputedStyle(element)
            const left = /auto|scroll/.test(style.overflowX) && element.scrollWidth > element.clientWidth
            const top = /auto|scroll/.test(style.overflowY) && element.scrollHeight > element.clientHeight
            if (left || top) {
                element.scrollBy(left ? event.deltaX : 0, top ? event.deltaY : 0)
                return
            }
        }
    }
</script>

<!-- The keys live in commands, not here: `pane.moveDividerLeft` / `Right` (⌃⌥← / ⌃⌥→) and `pane.resetDivider`. No Tab stop, since Tab switches panes. -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
    class="pane-resizer"
    class:dragging={isDragging}
    onmousedown={handleMouseDown}
    ondblclick={onReset}
    onwheel={handleWheel}
    role="separator"
    aria-orientation="vertical"
    aria-label={tString('fileExplorer.pane.resizePanesAriaLabel')}
>
    <div class="handle"></div>
</div>

<style>
    /* A 1px divider: the element itself is the hairline, and two unpainted pseudo-elements
       widen the hit area past it. Doing it this way (rather
       than a 4px strip with a 1px line drawn inside) keeps the LAYOUT at 1px, so the
       panes stay flush against the line and no strip of a third color appears between
       them. */
    .pane-resizer {
        width: 1px;
        cursor: col-resize;
        display: flex;
        align-items: center;
        justify-content: center;
        /* `--color-border`, not `--color-border-strong`: at 1px the divider only has to
           separate the panes, not assert itself. Both themes step down together. */
        background: var(--color-border);
        flex-shrink: 0;
        /* Sit above the per-pane tab-bar's active-tab stacking context
           (`.tab.active` uses `z-index: 1` so its `margin-bottom: -1px`
           overlap with the path bar paints on top). The active tab's
           shoulders extend past its left/right edges and would otherwise
           sit over this divider; raising the resizer's z-index keeps it
           on top. */
        position: relative;
        z-index: var(--z-sticky);
        transition: background-color var(--transition-fast);
    }

    /* The strip along the whole divider, never painted. 7px of overhang to the RIGHT makes it 8px
       wide, stopping short of the right pane's file icons (12px in). Only rightward: the left
       pane's vertical scrollbar runs along the divider and must stay fully grabbable.
       `pane-resizer-hit-area.test.ts` pins both. Events on it bubble to the resizer, so the drag
       handlers need no change. */
    .pane-resizer::before {
        content: '';
        position: absolute;
        top: 0;
        bottom: 0;
        left: 0;
        right: -7px;
        cursor: col-resize;
    }

    /* The 24-by-24 target (WCAG 2.5.8): a grab pad at the divider's foot, centered on the line.
       A full-height strip that wide would cover the right pane's file icons or the left pane's
       scrollbar, so only here, where both panes' status footers have 12px of blank inset
       (`.selection-info`), does the target grow without taking a click from anything. Never
       painted, so the line stays a line. Pinned by `pane-resizer-hit-area.test.ts`. */
    .pane-resizer::after {
        content: '';
        position: absolute;
        bottom: 0;
        left: calc(-1 * var(--spacing-md));
        right: -11px;
        height: var(--spacing-xl);
        cursor: col-resize;
    }

    .pane-resizer:hover,
    .pane-resizer.dragging {
        background: var(--color-accent);
    }

    /* Wider than the 1px divider, so it overhangs symmetrically; `flex-shrink: 0`
       stops the 1px track from squeezing it. `position: relative` keeps it above the
       grab layers, which would otherwise paint over it. */
    .handle {
        position: relative;
        flex-shrink: 0;
        width: 3px;
        height: 24px;
        border-radius: var(--radius-xs);
        background: var(--color-text-tertiary);
        opacity: 0;
        transition: opacity var(--transition-base);
    }

    .pane-resizer:hover .handle,
    .pane-resizer.dragging .handle {
        opacity: 1;
        background: white;
    }
</style>
