<script lang="ts">
    import { onMount, onDestroy, tick } from 'svelte'
    import type { Snippet } from 'svelte'
    import { notifyDialogOpened, notifyDialogClosed } from '$lib/tauri-commands'
    import type { SoftDialogId } from './dialog-registry'

    interface Props {
        titleId: string
        onkeydown?: (event: KeyboardEvent) => void
        title: Snippet
        children: Snippet
        /** MCP dialog tracking: sends notifyDialogOpened/Closed on mount/destroy */
        dialogId?: SoftDialogId
        role?: 'dialog' | 'alertdialog'
        draggable?: boolean
        /** Use blurred overlay (0.6 opacity + backdrop-filter) instead of plain 0.4 opacity */
        blur?: boolean
        ariaDescribedby?: string
        /** Inline style string for the dialog container (sizing, colors) */
        containerStyle?: string
        /** Renders × button and handles Escape key */
        onclose?: () => void
    }

    const {
        titleId,
        onkeydown,
        title,
        children,
        dialogId,
        role = 'dialog',
        draggable = true,
        blur = false,
        ariaDescribedby,
        containerStyle = '',
        onclose,
    }: Props = $props()

    let overlayElement: HTMLDivElement | undefined = $state()
    let dialogPosition = $state({ x: 0, y: 0 })
    let isDragging = $state(false)

    const dialogStyle = $derived(
        `transform: translate(${String(dialogPosition.x)}px, ${String(dialogPosition.y)}px);${containerStyle ? ` ${containerStyle}` : ''}`,
    )

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
        onkeydown?.(event)
    }

    onMount(async () => {
        if (dialogId) {
            void notifyDialogOpened(dialogId)
        }
        await tick()
        overlayElement?.focus()
    })

    onDestroy(() => {
        if (dialogId) {
            void notifyDialogClosed(dialogId)
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
>
    <div class="modal-dialog" class:dragging={isDragging} style={dialogStyle}>
        {#if onclose}
            <button class="modal-close-button" onclick={onclose} aria-label="Close">×</button>
        {/if}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div class="dialog-title-bar" class:draggable onmousedown={handleTitleMouseDown}>
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
    }

    .modal-overlay.blur {
        background: rgba(0, 0, 0, 0.6);
        backdrop-filter: blur(4px);
    }

    .modal-dialog {
        background: var(--color-bg-secondary);
        border: 1px solid var(--color-border-primary);
        border-radius: 12px;
        box-shadow: 0 16px 48px rgba(0, 0, 0, 0.4);
        position: relative;
    }

    .modal-close-button {
        position: absolute;
        top: 12px;
        right: 12px;
        z-index: 1;
        background: none;
        border: none;
        color: var(--color-text-secondary);
        font-size: 20px;
        cursor: pointer;
        padding: 4px 8px;
        line-height: 1;
        border-radius: 4px;
    }

    .modal-close-button:hover {
        background: var(--color-button-hover);
        color: var(--color-text-primary);
    }

    .modal-dialog.dragging {
        cursor: move;
    }

    .dialog-title-bar {
        padding: 16px 24px 8px;
        user-select: none;
    }

    .dialog-title-bar.draggable {
        cursor: move;
    }

    h2 {
        margin: 0;
        font-size: 16px;
        font-weight: 600;
        color: var(--color-text-primary);
        text-align: center;
    }
</style>
