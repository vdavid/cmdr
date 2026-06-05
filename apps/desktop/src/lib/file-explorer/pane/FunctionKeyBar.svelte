<script lang="ts">
    import { explorerState } from './explorer-state.svelte'
    import { getActiveTab } from '../tabs/tab-state-manager.svelte'
    import { capabilitiesFor } from './volume-capabilities'
    import type { CommandId } from '$lib/commands'

    interface Props {
        visible?: boolean
        /**
         * Dispatches a `file.*` command for the clicked F-key onto the command
         * bus. The buttons carry the same user intent as the keyboard / palette /
         * menu paths, so they route through the one typed dispatch spine instead
         * of calling `explorerRef` directly. Wired to `handleCommandExecute` in
         * `+page.svelte`.
         */
        onCommand?: (id: CommandId) => void
    }

    const { visible = true, onCommand }: Props = $props()

    /**
     * Each F-key button's command id. Held in a typed map (not inlined as a
     * string literal at the `onCommand?.(…)` call site) so the `CommandId` type
     * is checked and `cmdr/no-raw-command-dispatch` stays satisfied: the call
     * site passes a typed value, never a magic string.
     */
    const fnKeyToCommand = {
        view: 'file.view',
        edit: 'file.edit',
        copy: 'file.copy',
        move: 'file.move',
        rename: 'file.rename',
        newFile: 'file.newFile',
        newFolder: 'file.newFolder',
        delete: 'file.delete',
        deletePermanently: 'file.deletePermanently',
    } as const satisfies Record<string, CommandId>

    /**
     * Capabilities for the focused pane, read straight off the explorer store.
     * The button `disabled` flags branch on the `VolumeCapabilities` record
     * (invariant A6 — capabilities, not a `volumeId === 'search-results'` string
     * compare), the same source the dispatch guard and the context menu read. A
     * `search-results://` snapshot pane has no real folder to create into /
     * rename within, so mkdir / mkfile / rename render visibly disabled; per
     * `docs/design-principles.md`, "disabled is better than 'you did the wrong
     * thing' toasts." Its rows are real files, so source ops (copy/move/delete)
     * stay enabled (`canBeSource: true`).
     *
     * Reading the focused pane's active-tab `volumeId` through the store keeps
     * this reactive without the old `onFocusedVolumeChange` callback → page
     * `$state` → prop chain (A9: a store getter inside a `$derived` is reactive;
     * a plain `explorerRef` method call isn't). Per-pane read only (P1): we touch
     * the focused pane's manager, never both. `capabilitiesFor` resolves the
     * `fsType`/`category` from the volume store, so we pass just the volumeId.
     */
    const caps = $derived(
        capabilitiesFor(
            getActiveTab(explorerState.getTabMgr(explorerState.getFocusedPane())).volumeId,
        ),
    )
    const canMkdir = $derived(caps.canCreateChild)
    const canMkfile = $derived(caps.canCreateChild)
    const canRename = $derived(caps.canRenameInPlace)
    /** Source-side actions (copy/move/delete). The snapshot pane's rows are real files. */
    const canSourceOps = $derived(caps.canBeSource)

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
                onclick={() => onCommand?.(fnKeyToCommand.newFile)}
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
                onclick={() => onCommand?.(fnKeyToCommand.rename)}
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
                onclick={() => onCommand?.(fnKeyToCommand.deletePermanently)}
                disabled={!canSourceOps}
                tabindex={-1}
                aria-label="Delete permanently (Shift+F8)"
            >
                <kbd>⇧F8</kbd><span>Permanently</span>
            </button>
        {:else}
            <button
                onclick={() => onCommand?.(fnKeyToCommand.rename)}
                disabled={!canRename}
                tabindex={-1}
                aria-label="Rename (F2)"
            >
                <kbd>F2</kbd><span>Rename</span>
            </button>
            <button
                onclick={() => onCommand?.(fnKeyToCommand.view)}
                tabindex={-1}
                aria-label="View file (F3)"
            >
                <kbd>F3</kbd><span>View</span>
            </button>
            <button
                onclick={() => onCommand?.(fnKeyToCommand.edit)}
                tabindex={-1}
                aria-label="Edit file (F4)"
            >
                <kbd>F4</kbd><span>Edit</span>
            </button>
            <button
                onclick={() => onCommand?.(fnKeyToCommand.copy)}
                disabled={!canSourceOps}
                tabindex={-1}
                aria-label="Copy (F5)"
            >
                <kbd>F5</kbd><span>Copy</span>
            </button>
            <button
                onclick={() => onCommand?.(fnKeyToCommand.move)}
                disabled={!canSourceOps}
                tabindex={-1}
                aria-label="Move (F6)"
            >
                <kbd>F6</kbd><span>Move</span>
            </button>
            <button
                onclick={() => onCommand?.(fnKeyToCommand.newFolder)}
                disabled={!canMkdir}
                tabindex={-1}
                aria-label="New folder (F7)"
            >
                <kbd>F7</kbd><span>New folder</span>
            </button>
            <button
                onclick={() => onCommand?.(fnKeyToCommand.delete)}
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
