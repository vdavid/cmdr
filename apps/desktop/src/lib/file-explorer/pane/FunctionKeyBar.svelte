<script lang="ts">
    import { explorerState } from './explorer-state.svelte'
    import { getActiveTab } from '../tabs/tab-state-manager.svelte'
    import { capabilitiesForPane } from './volume-capabilities'
    import {
        getFirstShiftShortcutReactive,
        getFirstShortcutReactive,
    } from '$lib/shortcuts/reactive-shortcuts.svelte'
    import { toPlatformShortcut } from '$lib/shortcuts/key-capture'
    import { fnKeyToCommand } from './function-key-commands'
    import { tString } from '$lib/intl/messages.svelte'
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
     * Capabilities for the focused PANE, read straight off the explorer store.
     * The button `disabled` flags branch on the `VolumeCapabilities` record
     * (capabilities, not a `volumeId === 'search-results'` string
     * compare), the same source the dispatch guard and the context menu read. A
     * `search-results://` snapshot pane has no real folder to create into /
     * rename within, so mkdir / mkfile / rename render visibly disabled; per
     * `docs/design-principles.md`, "disabled is better than 'you did the wrong
     * thing' toasts." Its rows are real files, so source ops (copy/move/delete)
     * stay enabled (`canBeSource: true`).
     *
     * ❌ `capabilitiesForPane`, never `capabilitiesFor`: the two ROUTED kinds are
     * kind-from-PATH on top of a parent drive whose own row is writable, so asking
     * the volume id alone offered New folder and Rename inside a `.git` snapshot and
     * inside a read-only tar, and the press then hit `readOnlyRefusal`'s alert — the
     * refusal dialog the principle above exists to avoid. A zip still enables both:
     * it's the one archive format the managed edit flow can write.
     *
     * Reading the focused pane's active tab through the store keeps this reactive
     * across the component boundary: a store getter inside a `$derived` is reactive,
     * a plain `explorerRef` method call isn't. Per-pane read only (P1): we touch the
     * focused pane's manager, never both.
     */
    const activeTab = $derived(getActiveTab(explorerState.getTabMgr(explorerState.getFocusedPane())))
    const caps = $derived(capabilitiesForPane(activeTab.volumeId, activeTab.path))
    const canMkdir = $derived(caps.canWrite)
    const canMkfile = $derived(caps.canWrite)
    const canRename = $derived(caps.canWrite)
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

    /** Slot 0 of the bar is F2, so slot `i` carries `F${i + FIRST_FN_KEY}`. */
    const FIRST_FN_KEY = 2

    /** One button. `shortcut` is a display-form chip; `undefined` renders no chip. */
    interface Slot {
        id: CommandId
        /** The short button text. */
        label: string
        /** The full spoken action name, for the aria-label. */
        action: string
        enabled: boolean
        shortcut: string | undefined
    }

    /**
     * The nine actions the bar can offer, without a chip: the two rows below compose
     * them, each supplying the binding that belongs to IT.
     */
    const actions = $derived({
        rename: {
            id: fnKeyToCommand.rename,
            label: tString('fileExplorer.functionKeyBar.renameLabel'),
            action: tString('fileExplorer.functionKeyBar.renameAction'),
            enabled: canRename,
        },
        view: {
            id: fnKeyToCommand.view,
            label: tString('fileExplorer.functionKeyBar.viewLabel'),
            action: tString('fileExplorer.functionKeyBar.viewAction'),
            enabled: true,
        },
        edit: {
            id: fnKeyToCommand.edit,
            label: tString('fileExplorer.functionKeyBar.editLabel'),
            action: tString('fileExplorer.functionKeyBar.editAction'),
            enabled: true,
        },
        copy: {
            id: fnKeyToCommand.copy,
            label: tString('fileExplorer.functionKeyBar.copyLabel'),
            action: tString('fileExplorer.functionKeyBar.copyAction'),
            enabled: canSourceOps,
        },
        move: {
            id: fnKeyToCommand.move,
            label: tString('fileExplorer.functionKeyBar.moveLabel'),
            action: tString('fileExplorer.functionKeyBar.moveAction'),
            enabled: canSourceOps,
        },
        newFolder: {
            id: fnKeyToCommand.newFolder,
            label: tString('fileExplorer.functionKeyBar.newFolderLabel'),
            action: tString('fileExplorer.functionKeyBar.newFolderAction'),
            enabled: canMkdir,
        },
        delete: {
            id: fnKeyToCommand.delete,
            label: tString('fileExplorer.functionKeyBar.deleteLabel'),
            action: tString('fileExplorer.functionKeyBar.deleteAction'),
            enabled: canSourceOps,
        },
        newFile: {
            id: fnKeyToCommand.newFile,
            label: tString('fileExplorer.functionKeyBar.newFileLabel'),
            action: tString('fileExplorer.functionKeyBar.newFileAction'),
            enabled: canMkfile,
        },
        deletePermanently: {
            id: fnKeyToCommand.deletePermanently,
            label: tString('fileExplorer.functionKeyBar.permanentlyLabel'),
            action: tString('fileExplorer.functionKeyBar.deletePermanentlyAction'),
            enabled: canSourceOps,
        },
    })

    /**
     * The plain row, F2…F8. Each chip shows its command's live effective FIRST
     * shortcut, never the hardcoded F-key: rebinding `file.copy` to `⌘C` in Settings
     * re-renders the F5 chip as `⌘C` immediately, so the bar can't lie about what the
     * keys do. The chip keeps the bar's quiet `<kbd>` look (a boxed `ShortcutChip` pill
     * repeated 7× fights the flat bar).
     */
    const defaultRow = $derived<Slot[]>(
        [
            actions.rename,
            actions.view,
            actions.edit,
            actions.copy,
            actions.move,
            actions.newFolder,
            actions.delete,
        ].map((action) => ({ ...action, shortcut: getFirstShortcutReactive(action.id) })),
    )

    /**
     * The Shift row, same seven fixed slots. WHICH slots carry a command is fixed; a
     * `null` slot is an F-key with no Shift action, and reads its label off its
     * POSITION so the row always spells one ⇧F2…⇧F8 ladder. A command slot reads its
     * command's SHIFTED binding, so Rename shows `⇧F6` here and `F2` in the plain row.
     */
    const shiftRow = $derived<(Slot | null)[]>(
        [null, null, actions.newFile, null, actions.rename, null, actions.deletePermanently].map(
            (action) =>
                action === null
                    ? null
                    : { ...action, shortcut: getFirstShiftShortcutReactive(action.id) },
        ),
    )
</script>

<svelte:document onkeydown={handleKeyDown} onkeyup={handleKeyUp} />

<!--
  One command button. The aria-label interpolates the same dynamic combo the chip
  shows, so screen readers hear what actually triggers the action ("Copy (F5)" →
  "Copy (⌘C)" after a rebind). With no chip, both it and the parenthetical drop —
  the label alone, still clickable; an empty `<kbd>` would read as broken.
-->
{#snippet commandButton(slot: Slot)}
    <button
        onclick={() => onCommand?.(slot.id)}
        disabled={!slot.enabled}
        tabindex={-1}
        aria-label={slot.shortcut
            ? tString('fileExplorer.functionKeyBar.actionWithShortcut', {
                  action: slot.action,
                  shortcut: slot.shortcut,
              })
            : slot.action}
    >
        {#if slot.shortcut}<kbd>{slot.shortcut}</kbd>{/if}<span>{slot.label}</span>
    </button>
{/snippet}

<!--
  A fixed F-key slot with no Shift action. Presentational only (not a command). The
  chip carries Shift so the whole row reads as the Shift row; the aria-label names the
  bare key, which is what "{fnKey} (no shift action)" is worded around, and spares a
  screen reader a modifier glyph.
-->
{#snippet emptySlot(index: number)}
    {@const fnKey = `F${String(index + FIRST_FN_KEY)}`}
    <button disabled tabindex={-1} aria-label={tString('fileExplorer.functionKeyBar.noShiftAction', { fnKey })}>
        <kbd>{toPlatformShortcut(`⇧${fnKey}`)}</kbd>
    </button>
{/snippet}

{#if visible}
    <div
        class="function-key-bar"
        role="toolbar"
        aria-label={tString('fileExplorer.functionKeyBar.toolbarAriaLabel')}
        onmousedown={(e) => {
            e.preventDefault()
        }}
    >
        <!-- eslint-disable @typescript-eslint/no-confusing-void-expression -- Svelte {@render} syntax -->
        {#each shiftHeld ? shiftRow : defaultRow as slot, index (index)}
            {#if slot}
                {@render commandButton(slot)}
            {:else}
                {@render emptySlot(index)}
            {/if}
        {/each}
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
        /* Vertical padding lives on the BUTTON, not the bar, so the hover fill still
           spans the band's full height instead of floating inside an inset strip. */
        padding: var(--spacing-sm) 0;
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
        padding: 1px var(--spacing-xs);
        white-space: nowrap;
        flex-shrink: 0;
    }
</style>
