<script lang="ts">
    import { tString } from '$lib/intl/messages.svelte'

    interface Props {
        onResize: (widthPercent: number) => void
        onResizeEnd: () => void
        onReset: () => void
    }

    const { onResize, onResizeEnd, onReset }: Props = $props()

    let isDragging = $state(false)

    function handleMouseDown(event: MouseEvent) {
        event.preventDefault()
        isDragging = true

        // Capture the container reference at drag start (not during mousemove)
        const container = (event.target as HTMLElement).closest('.dual-pane-explorer')
        if (!container) return

        const handleMouseMove = (moveEvent: MouseEvent) => {
            const rect = container.getBoundingClientRect()
            const mouseX = moveEvent.clientX - rect.left
            const widthPercent = (mouseX / rect.width) * 100

            // Clamp to 25-75%
            const clampedPercent = Math.max(25, Math.min(75, widthPercent))
            onResize(clampedPercent)
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
</script>

<div
    class="pane-resizer"
    class:dragging={isDragging}
    onmousedown={handleMouseDown}
    ondblclick={onReset}
    role="separator"
    aria-orientation="vertical"
    aria-label={tString('fileExplorer.pane.resizePanesAriaLabel')}
>
    <div class="handle"></div>
</div>

<style>
    /* A 1px divider with a 4px grab target: the element itself is the hairline, and
       `::before` widens the hit area past it on both sides. Doing it this way (rather
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

    /* Grab target only, never painted. 1.5px of overhang each side puts the total at
       the 4px the divider used to occupy. Events on it bubble to the resizer, so the
       drag handlers need no change. */
    .pane-resizer::before {
        content: '';
        position: absolute;
        top: 0;
        bottom: 0;
        /* stylelint-disable-next-line declaration-property-value-disallowed-list -- half-pixel overhang, no matching token */
        left: -1.5px;
        /* stylelint-disable-next-line declaration-property-value-disallowed-list -- half-pixel overhang, no matching token */
        right: -1.5px;
        cursor: col-resize;
    }

    .pane-resizer:hover,
    .pane-resizer.dragging {
        background: var(--color-accent);
    }

    /* Wider than the 1px divider, so it overhangs symmetrically; `flex-shrink: 0`
       stops the 1px track from squeezing it. `position: relative` keeps it above the
       `::before` grab layer, which would otherwise paint over it. */
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
