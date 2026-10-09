<script lang="ts">
    /**
     * The favorites menu (⌃D): the pane's favorites as a numbered list, with an add row
     * at the bottom. It hangs off the same chip as the volume switcher, and the chip
     * (`VolumeBreadcrumb.svelte`) keeps the two from ever being open at once.
     *
     * ❗ The house `Menu` primitive (`$lib/ui/Menu.svelte`) owns every interaction: open
     * and close, anchoring, the cursor, the keyboard (including the digit accelerators
     * and ⌥↑/⌥↓ reorder), keyboard-vs-pointer mode, drag, focus, and placement. This
     * component owns the rename field's markup and the two keys that swap menus; the rows
     * themselves come from `favorites-menu.svelte.ts`.
     */
    import { onDestroy } from 'svelte'
    import { getVolumes } from '$lib/stores/volume-store.svelte'
    import { getCachedIcon, iconCacheVersion } from '$lib/icon-cache'
    import { dependOn } from '$lib/utils/reactivity'
    import { eventMatchesCommand } from '$lib/shortcuts'
    import { tString } from '$lib/intl/messages.svelte'
    import Menu from '$lib/ui/Menu.svelte'
    import { createMenu } from '$lib/ui/menu-controller.svelte'
    import type { MenuRowContext } from '$lib/ui/menu-types'
    import type { PaneId } from '$lib/commands/types'
    import type { VolumeChangePayload } from '../pane/types'
    import { createFavoritesMenu, FAVORITES_SECTION_ID, type FavoritesRow } from './favorites-menu.svelte'
    import { reportFavoritesMenuOpened, type FavoritesMenuOpenTrigger } from './favorites-analytics'

    interface Props {
        /** Which pane this menu belongs to, so ⌥F1 / ⌥F2 swaps back to ITS switcher. */
        paneId: PaneId
        /** The pane's volume and folder: what the add row acts on, and what gates it. */
        volumeId: string
        currentPath: string
        /** The chip the list hangs under, read at open time. */
        getAnchor: () => HTMLElement | undefined
        /** The chip's whole control cluster: pressing a control in it doesn't close the list. */
        getChipCluster: () => HTMLElement | undefined
        onVolumeChange?: (change: VolumeChangePayload) => void
        /** ⌥F1 / ⌥F2 inside the menu: hand the header back to the volume switcher. */
        onShowVolumes: () => void
        /** So the chip can keep one header menu open at a time. */
        onOpenChange: (open: boolean) => void
    }

    const { paneId, volumeId, currentPath, getAnchor, getChipCluster, onVolumeChange, onShowVolumes, onOpenChange }: Props = $props()

    let renameInputRef: HTMLInputElement | undefined = $state()
    let shortcutInputRef: HTMLInputElement | undefined = $state()

    // Generic macOS folder icon, standing in for a favorite whose own icon isn't fetched
    // yet (FDA-gated paths aren't asked about, to avoid TCC popups). Reading
    // `$iconCacheVersion` re-evaluates this once the icon lands.
    const dirIconFallback = $derived.by(() => {
        dependOn($iconCacheVersion)
        return getCachedIcon('dir')
    })

    const favorites = createFavoritesMenu({
        getVolumes: () => getVolumes(),
        getPaneId: () => paneId,
        getPaneVolumeId: () => volumeId,
        getPaneCurrentPath: () => currentPath,
        getDirIconFallback: () => dirIconFallback,
        getRenameInputRef: () => renameInputRef,
        getShortcutInputRef: () => shortcutInputRef,
        go: (target) => { onVolumeChange?.(target) },
    })

    /** Where focus was when the menu opened, so closing it doesn't move the focused pane. */
    let focusBeforeOpen: HTMLElement | null = null

    /**
     * The two keys that swap menus. Central dispatch is suppressed while a header menu is
     * open, so nothing else would answer them; matching through `eventMatchesCommand`
     * means a rebind follows. ⌃D closes (the key that opened it toggles), and the pane's
     * OWN chooser key swaps back — ❗ `paneId`'s, not either one, so ⌥F2 pressed over the
     * left pane's favorites still means the right pane's switcher.
     */
    function handleKey(event: KeyboardEvent): boolean {
        if (eventMatchesCommand(event, 'favorites.open')) {
            menu.close()
            return true
        }
        if (eventMatchesCommand(event, paneId === 'left' ? 'pane.leftVolumeChooser' : 'pane.rightVolumeChooser')) {
            onShowVolumes()
            return true
        }
        return false
    }

    const menu = createMenu<FavoritesRow>({
        getSections: () => favorites.sections,
        onSelect: (item, source) => {
            void favorites.select(item, source)
        },
        onReorder: ({ sectionId, orderedValues }) => {
            if (sectionId === FAVORITES_SECTION_ID) favorites.applyReorder(orderedValues)
        },
        // While either inline editor is focused, its `<input>` owns every keystroke:
        // arrows and Home/End must not move the menu cursor.
        isEditing: favorites.isEditing,
        onKey: handleKey,
        onOpenChange,
        restoreFocus: () => {
            // ❗ Back to whatever held focus, ❌ not to this pane: the command opens the
            // FOCUSED pane's menu, and a swap from the other pane's switcher must not
            // move the focus across.
            focusBeforeOpen?.focus()
            focusBeforeOpen = null
        },
        // ❗ The chip's own controls sit BESIDE the anchor, and pressing one deliberately
        // leaves the menu open.
        keepOpenWithin: getChipCluster,
    })

    export function open(trigger: FavoritesMenuOpenTrigger): void {
        if (menu.isOpen) return
        const anchor = getAnchor()
        if (!anchor) return
        focusBeforeOpen = document.activeElement instanceof HTMLElement ? document.activeElement : null
        menu.openUnder(anchor)
        reportFavoritesMenuOpened({ trigger })
    }

    export function close(): void {
        menu.close()
    }

    export function toggle(trigger: FavoritesMenuOpenTrigger): void {
        if (menu.isOpen) menu.close()
        else open(trigger)
    }

    export function getIsOpen(): boolean {
        return menu.isOpen
    }

    onDestroy(() => {
        menu.destroy()
    })
</script>

<!-- The name this surface carries in Settings > Keyboard shortcuts, so screen readers and
     the shortcut scope say the same thing. -->
<Menu {menu} ariaLabel={tString('shortcuts.scope.favoritesMenu')}>
    {#snippet label(ctx: MenuRowContext<FavoritesRow>)}
        {@const row = ctx.item.data}
        {#if row?.kind === 'favorite' && favorites.renamingFavoriteId === row.volume.id}
            <!-- eslint-disable-next-line cmdr/prefer-ui-primitive -- Dense inline editor inside a menu row: it inherits the row's font and sits at row height with 2px side padding, which the framed `TextInput`'s padding would blow past, and it carries a resting accent border to read as "editing" rather than one that appears on focus. -->
            <input
                class="favorite-rename-input"
                bind:this={renameInputRef}
                bind:value={favorites.renameDraft}
                onkeydown={(e: KeyboardEvent) => { favorites.handleRenameKeyDown(e, row.volume) }}
                onblur={() => { void favorites.commitRename(row.volume) }}
                aria-label={tString('fileExplorer.navigation.renameFavoriteAriaLabel')}
            />
        {:else}
            <!-- A row a pick can't open right away reads quiet, like the switcher's undialed saved
                 place. ❌ Not `aria-disabled`: picking it is what dials it. Its tooltip says why. -->
            <span class="favorite-label" class:is-unreachable={row?.kind === 'favorite' && favorites.isDimmed(row.volume)}>{ctx.item.label}</span>
        {/if}
    {/snippet}
    {#snippet trailing(ctx: MenuRowContext<FavoritesRow>)}
        {@const row = ctx.item.data}
        {#if row?.kind === 'favorite' && favorites.editingShortcutId === row.volume.id}
            <!-- eslint-disable-next-line cmdr/prefer-ui-primitive -- This read-only, one-key capture sits inside a menu row, not a framed text field. -->
            <input
                class="favorite-shortcut-input"
                bind:this={shortcutInputRef}
                readonly
                value={row.volume.favoriteShortcut ?? ''}
                placeholder={tString('fileExplorer.navigation.favoriteShortcutPrompt')}
                aria-label={tString('fileExplorer.navigation.favoriteShortcutAriaLabel', { name: row.volume.name })}
                onkeydown={(event: KeyboardEvent) => { favorites.handleShortcutKeyDown(event, row.volume) }}
                onblur={favorites.cancelShortcutEdit}
            />
        {/if}
    {/snippet}
</Menu>

<style>
    .favorite-label {
        flex: 1;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    /*noinspection CssUnusedSymbol*/
    .favorite-label.is-unreachable {
        color: var(--color-text-quiet);
    }

    .favorite-rename-input {
        flex: 1;
        min-width: 0;
        font: inherit;
        color: var(--color-text-primary);
        background-color: var(--color-bg-primary);
        border: 1px solid var(--color-accent);
        border-radius: var(--radius-sm);
        padding: 0 var(--spacing-xxs);
    }

    .favorite-rename-input:focus {
        outline: none;
    }

    .favorite-shortcut-input {
        width: 112px;
        margin-left: auto;
        font: inherit;
        text-align: right;
        color: var(--color-text-primary);
        background-color: var(--color-bg-primary);
        border: 1px solid var(--color-accent);
        border-radius: var(--radius-sm);
        padding: 0 var(--spacing-xxs);
    }

    .favorite-shortcut-input:focus {
        outline: none;
    }
</style>
