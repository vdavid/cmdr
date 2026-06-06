<script lang="ts">
    import { explorerState } from './explorer-state.svelte'
    import { getActiveTab } from '../tabs/tab-state-manager.svelte'
    import { capabilitiesFor } from './volume-capabilities'
    import { getFirstShortcutReactive } from '$lib/shortcuts/reactive-shortcuts.svelte'
    import { fnKeyToCommand } from './function-key-commands'
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
     * Each visible button's CHIP shows its command's live effective first shortcut
     * (`getFirstShortcutReactive`), not the hardcoded F-key. Rebinding `file.copy`
     * to `⌘C` in Settings re-renders the F5 button's chip as `⌘C` immediately, so
     * the bar never lies about what the keys do. The chip keeps the bar's quiet
     * `<kbd>` look (a boxed `ShortcutChip` pill repeated 8× fights the flat bar);
     * truthfulness is the must, the chip style is the want (see the migration plan).
     *
     * The Shift fork stays presentational: WHICH buttons appear on Shift is fixed,
     * but each shown button reads ITS command's effective FIRST binding. Both Rename
     * buttons (F2 and the Shift-revealed one) therefore show `file.rename`'s first
     * binding — slightly odd, but truthful, which is the whole point.
     *
     * When a command has no binding the chip renders nothing (the button stays
     * clickable and keeps its label); an empty `<kbd>` would read as broken.
     */
    function shortcutFor(id: CommandId): string | undefined {
        return getFirstShortcutReactive(id)
    }

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

<!--
  One command button. The chip reads the command's live effective first shortcut;
  the aria-label interpolates the same dynamic combo so screen readers hear what
  actually triggers the action ("Copy (F5)" → "Copy (⌘C)" after a rebind). When
  unbound, both the chip and the parenthetical drop — the label alone, still clickable.
-->
{#snippet commandButton(id: CommandId, label: string, action: string, enabled: boolean)}
    {@const shortcut = shortcutFor(id)}
    <button
        onclick={() => onCommand?.(id)}
        disabled={!enabled}
        tabindex={-1}
        aria-label={shortcut ? `${action} (${shortcut})` : action}
    >
        {#if shortcut}<kbd>{shortcut}</kbd>{/if}<span>{label}</span>
    </button>
{/snippet}

<!-- A fixed F-key slot with no Shift action. Presentational only (not a command). -->
{#snippet emptySlot(fnKey: string)}
    <button disabled tabindex={-1} aria-label="{fnKey} (no shift action)">
        <kbd>{fnKey}</kbd>
    </button>
{/snippet}

{#if visible}
    <div
        class="function-key-bar"
        role="toolbar"
        aria-label="Function keys"
        onmousedown={(e) => {
            e.preventDefault()
        }}
    >
        <!-- eslint-disable @typescript-eslint/no-confusing-void-expression -- Svelte {@render} syntax -->
        {#if shiftHeld}
            {@render emptySlot('F2')}
            {@render emptySlot('F3')}
            {@render commandButton(fnKeyToCommand.newFile, 'New file', 'Create new file', canMkfile)}
            {@render emptySlot('F5')}
            {@render commandButton(fnKeyToCommand.rename, 'Rename', 'Rename', canRename)}
            {@render emptySlot('F7')}
            {@render commandButton(
                fnKeyToCommand.deletePermanently,
                'Permanently',
                'Delete permanently',
                canSourceOps,
            )}
        {:else}
            {@render commandButton(fnKeyToCommand.rename, 'Rename', 'Rename', canRename)}
            {@render commandButton(fnKeyToCommand.view, 'View', 'View file', true)}
            {@render commandButton(fnKeyToCommand.edit, 'Edit', 'Edit file', true)}
            {@render commandButton(fnKeyToCommand.copy, 'Copy', 'Copy', canSourceOps)}
            {@render commandButton(fnKeyToCommand.move, 'Move', 'Move', canSourceOps)}
            {@render commandButton(fnKeyToCommand.newFolder, 'New folder', 'New folder', canMkdir)}
            {@render commandButton(fnKeyToCommand.delete, 'Delete', 'Delete', canSourceOps)}
        {/if}
        <!-- eslint-enable @typescript-eslint/no-confusing-void-expression -->
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
        /* min-width: 0 lets a button shrink below its content size so a long custom
           binding (e.g. ⌘⇧⌥K) can't force the bar wider than the window: the label
           truncates instead. Without it, flex items refuse to shrink past content. */
        min-width: 0;
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

    /* The label truncates before the chip does: a long binding keeps the key
       readable (the chip is the truthful claim) while the word gives way. */
    button > span {
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        min-width: 0;
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
        white-space: nowrap;
        flex-shrink: 0;
    }
</style>
