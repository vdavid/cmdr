<script lang="ts">
    import { explorerState } from './explorer-state.svelte'
    import { getActiveTab } from '../tabs/tab-state-manager.svelte'

    interface Props {
        visible?: boolean
        /** Source-side actions (copy/move/delete). Always true on real folders. */
        canSourceOps?: boolean
        onRename?: () => void
        onView?: () => void
        onEdit?: () => void
        onCopy?: () => void
        onMove?: () => void
        onNewFile?: () => void
        onNewFolder?: () => void
        onDelete?: () => void
        onDeletePermanently?: () => void
    }

    const {
        visible = true,
        canSourceOps = true,
        onRename,
        onView,
        onEdit,
        onCopy,
        onMove,
        onNewFile,
        onNewFolder,
        onDelete,
        onDeletePermanently,
    }: Props = $props()

    /**
     * Destination-side capability flags for the focused pane, read straight off
     * the explorer store. A `search-results://` snapshot pane has no real folder
     * to create into / rename within, so mkdir / mkfile / rename / paste-into
     * render visibly disabled; per `docs/design-principles.md`, "disabled is
     * better than 'you did the wrong thing' toasts."
     *
     * Reading the focused pane's active-tab `volumeId` through the store keeps
     * this reactive without the old `onFocusedVolumeChange` callback → page
     * `$state` → prop chain (A9: a store getter inside a `$derived` is reactive;
     * a plain `explorerRef` method call isn't). Per-pane read only (P1): we touch
     * the focused pane's manager, never both.
     *
     * Known-transitional A6 exception: the `=== 'search-results'` string compare
     * stays here for now — Phase 4 replaces it with a capability check. Only its
     * input (the volumeId) has moved to a store read.
     */
    const isSearchResultsPane = $derived(
        getActiveTab(explorerState.getTabMgr(explorerState.getFocusedPane())).volumeId ===
            'search-results',
    )
    const canMkdir = $derived(!isSearchResultsPane)
    const canMkfile = $derived(!isSearchResultsPane)
    const canRename = $derived(!isSearchResultsPane)

    let shiftHeld = $state(false)

    function handleKeyDown(e: KeyboardEvent) {
        if (e.key === 'Shift') {
            shiftHeld = true
        }
    }

    function handleKeyUp(e: KeyboardEvent) {
        if (e.key === 'Shift') {
            shiftHeld = false
        }
    }
</script>

<svelte:document onkeydown={handleKeyDown} onkeyup={handleKeyUp} />

{#if visible}
    <div
        class="function-key-bar"
        role="toolbar"
        aria-label="Function keys"
        onmousedown={(e) => {
            e.preventDefault()
        }}
    >
        {#if shiftHeld}
            <button disabled tabindex={-1} aria-label="F2 (no shift action)">
                <kbd>F2</kbd>
            </button>
            <button disabled tabindex={-1} aria-label="F3 (no shift action)">
                <kbd>F3</kbd>
            </button>
            <button
                onclick={onNewFile}
                disabled={!canMkfile}
                tabindex={-1}
                aria-label="Create new file (Shift+F4)"
            >
                <kbd>⇧F4</kbd><span>New file</span>
            </button>
            <button disabled tabindex={-1} aria-label="F5 (no shift action)">
                <kbd>F5</kbd>
            </button>
            <button
                onclick={onRename}
                disabled={!canRename}
                tabindex={-1}
                aria-label="Rename (Shift+F6)"
            >
                <kbd>⇧F6</kbd><span>Rename</span>
            </button>
            <button disabled tabindex={-1} aria-label="F7 (no shift action)">
                <kbd>F7</kbd>
            </button>
            <button
                onclick={onDeletePermanently}
                disabled={!canSourceOps}
                tabindex={-1}
                aria-label="Delete permanently (Shift+F8)"
            >
                <kbd>⇧F8</kbd><span>Permanently</span>
            </button>
        {:else}
            <button
                onclick={onRename}
                disabled={!canRename}
                tabindex={-1}
                aria-label="Rename (F2)"
            >
                <kbd>F2</kbd><span>Rename</span>
            </button>
            <button onclick={onView} tabindex={-1} aria-label="View file (F3)">
                <kbd>F3</kbd><span>View</span>
            </button>
            <button onclick={onEdit} tabindex={-1} aria-label="Edit file (F4)">
                <kbd>F4</kbd><span>Edit</span>
            </button>
            <button
                onclick={onCopy}
                disabled={!canSourceOps}
                tabindex={-1}
                aria-label="Copy (F5)"
            >
                <kbd>F5</kbd><span>Copy</span>
            </button>
            <button
                onclick={onMove}
                disabled={!canSourceOps}
                tabindex={-1}
                aria-label="Move (F6)"
            >
                <kbd>F6</kbd><span>Move</span>
            </button>
            <button
                onclick={onNewFolder}
                disabled={!canMkdir}
                tabindex={-1}
                aria-label="New folder (F7)"
            >
                <kbd>F7</kbd><span>New folder</span>
            </button>
            <button
                onclick={onDelete}
                disabled={!canSourceOps}
                tabindex={-1}
                aria-label="Delete (F8)"
            >
                <kbd>F8</kbd><span>Delete</span>
            </button>
        {/if}
    </div>
{/if}

<style>
    .function-key-bar {
        display: flex;
        flex-shrink: 0;
        background: var(--color-bg-secondary);
    }

    button {
        flex: 1;
        display: flex;
        align-items: center;
        justify-content: center;
        gap: var(--spacing-xs);
        border: none;
        border-right: 1px solid var(--color-border-subtle);
        background: transparent;
        padding: var(--spacing-xs) 0;
        font-family: var(--font-system) sans-serif;
        font-size: var(--font-size-sm);
        color: var(--color-text-primary);
        transition: background-color var(--transition-fast);
    }

    button:last-child {
        border-right: none;
    }

    button:hover:not(:disabled) {
        background: var(--color-bg-tertiary);
    }

    button:disabled {
        opacity: 0.4;
        cursor: default;
    }

    kbd {
        font-family: var(--font-system) sans-serif;
        font-size: var(--font-size-xs);
        color: var(--color-text-secondary);
        /* stylelint-disable-next-line declaration-property-value-disallowed-list */
        padding: 1px var(--spacing-xs);
    }
</style>
