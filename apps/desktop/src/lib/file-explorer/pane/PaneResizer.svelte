<script lang="ts">
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
    aria-label="Resize panes"
>
    <div class="handle"></div>
</div>

<style>
    .pane-resizer {
        width: 5px;
        cursor: col-resize;
        display: flex;
        align-items: center;
        justify-content: center;
        background: var(--color-border-strong);
        flex-shrink: 0;
        transition: background-color var(--transition-base);
    }

    .pane-resizer:hover,
    .pane-resizer.dragging {
        background: var(--color-accent);
    }

    .handle {
        width: 3px;
        height: 24px;
        border-radius: 2px;
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
