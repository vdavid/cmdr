<!--
  Minimal context menu for the viewer's `.file-content`. Floats at the right-click
  position; offers Copy when there's a selection, and Select all otherwise. Closes on
  outside click, Escape, blur, or after an action runs.

  Why an in-app HTML menu instead of the OS-native one (`showContextMenu`): the OS
  menu would interrupt the gesture (it pops up over the webview), and tying a single
  Copy action to the heavyweight IPC isn't worth it. The two items here are pure
  UI-level actions; the gesture stays inside the webview.
-->

<script lang="ts">
    import { onMount, tick } from 'svelte'

    interface Props {
        /** Viewport-relative coordinates of the right-click. */
        x: number
        y: number
        /** Whether a non-empty selection currently exists. Controls which item is enabled. */
        hasSelection: boolean
        onCopy: () => void
        onSelectAll: () => void
        onClose: () => void
    }

    const { x, y, hasSelection, onCopy, onSelectAll, onClose }: Props = $props()

    let menuRef: HTMLDivElement | undefined = $state()
    /** Which item the keyboard has focused (0 = Copy, 1 = Select all). */
    let focusedIndex = $state(0)
    let firstItemRef: HTMLButtonElement | undefined = $state()
    let secondItemRef: HTMLButtonElement | undefined = $state()

    onMount(() => {
        // Move focus into the menu so Escape/Enter/arrows route here without the user
        // having to mouse over an item first.
        void tick().then(() => {
            firstItemRef?.focus()
        })
    })

    function handleKey(e: KeyboardEvent): void {
        if (e.key === 'Escape') {
            e.preventDefault()
            onClose()
            return
        }
        if (e.key === 'ArrowDown') {
            e.preventDefault()
            focusedIndex = (focusedIndex + 1) % 2
            ;(focusedIndex === 0 ? firstItemRef : secondItemRef)?.focus()
            return
        }
        if (e.key === 'ArrowUp') {
            e.preventDefault()
            focusedIndex = (focusedIndex + 1) % 2
            ;(focusedIndex === 0 ? firstItemRef : secondItemRef)?.focus()
        }
    }

    function handleOutsideClick(e: MouseEvent): void {
        if (menuRef && e.target instanceof Node && !menuRef.contains(e.target)) {
            onClose()
        }
    }

    function chooseCopy(): void {
        onCopy()
        onClose()
    }

    function chooseSelectAll(): void {
        onSelectAll()
        onClose()
    }
</script>

<svelte:window onmousedown={handleOutsideClick} onblur={onClose} onkeydown={handleKey} />

<div
    bind:this={menuRef}
    role="menu"
    aria-label="Viewer actions"
    class="viewer-context-menu"
    style="left: {x}px; top: {y}px"
>
    <button
        bind:this={firstItemRef}
        type="button"
        role="menuitem"
        class="menu-item"
        disabled={!hasSelection}
        onclick={chooseCopy}
    >
        Copy
        <span class="shortcut">⌘C</span>
    </button>
    <button
        bind:this={secondItemRef}
        type="button"
        role="menuitem"
        class="menu-item"
        onclick={chooseSelectAll}
    >
        Select all
        <span class="shortcut">⌘A</span>
    </button>
</div>

<style>
    .viewer-context-menu {
        position: fixed;
        z-index: var(--z-dropdown);
        min-width: 160px;
        padding: var(--spacing-xs);
        background: var(--color-bg-secondary);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-md);
        box-shadow: var(--shadow-md);
        font-size: var(--font-size-sm);
    }

    .menu-item {
        display: flex;
        align-items: center;
        justify-content: space-between;
        width: 100%;
        padding: var(--spacing-xs) var(--spacing-sm);
        border: none;
        background: transparent;
        color: var(--color-text-primary);
        font-size: var(--font-size-sm);
        text-align: left;
        border-radius: var(--radius-sm);
        transition: background var(--transition-fast);
    }

    .menu-item:hover:not(:disabled),
    .menu-item:focus-visible:not(:disabled) {
        background: var(--color-accent-subtle);
        outline: none;
    }

    .menu-item:disabled {
        color: var(--color-text-tertiary);
        opacity: 0.6;
    }

    .shortcut {
        color: var(--color-text-tertiary);
        font-family: var(--font-mono);
        font-size: var(--font-size-xs);
        margin-left: var(--spacing-md);
    }
</style>
