<script lang="ts">
    import { onMount, tick } from 'svelte'
    import type { Snippet } from 'svelte'

    interface Props {
        titleId: string
        onkeydown: (event: KeyboardEvent) => void
        title: Snippet
        children: Snippet
    }

    const { titleId, onkeydown, title, children }: Props = $props()

    let overlayElement: HTMLDivElement | undefined = $state()
    let dialogPosition = $state({ x: 0, y: 0 })
    let isDragging = $state(false)

    function handleTitleMouseDown(event: MouseEvent) {
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

    onMount(async () => {
        await tick()
        overlayElement?.focus()
    })
</script>

<div
    bind:this={overlayElement}
    class="modal-overlay"
    role="dialog"
    aria-modal="true"
    aria-labelledby={titleId}
    tabindex="-1"
    {onkeydown}
>
    <div
        class="draggable-dialog"
        class:dragging={isDragging}
        style="transform: translate({dialogPosition.x}px, {dialogPosition.y}px)"
    >
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div class="dialog-title-bar" onmousedown={handleTitleMouseDown}>
            <h2 id={titleId}>
                {@render title()}
            </h2>
        </div>
        {@render children()}
    </div>
</div>

<style>
    .modal-overlay {
        position: fixed;
        inset: 0;
        background: rgba(0, 0, 0, 0.4);
        display: flex;
        align-items: center;
        justify-content: center;
        z-index: 9999;
        /* No backdrop-filter blur — user needs to see content behind */
    }

    .draggable-dialog {
        background: var(--color-bg-secondary);
        border: 1px solid var(--color-border-primary);
        border-radius: 12px;
        min-width: 420px;
        max-width: 500px;
        box-shadow: 0 16px 48px rgba(0, 0, 0, 0.4);
        position: relative;
    }

    .draggable-dialog.dragging {
        cursor: move;
    }

    .dialog-title-bar {
        padding: 16px 24px 8px;
        cursor: move;
        user-select: none;
    }

    h2 {
        margin: 0;
        font-size: 16px;
        font-weight: 600;
        color: var(--color-text-primary);
        text-align: center;
    }
</style>
