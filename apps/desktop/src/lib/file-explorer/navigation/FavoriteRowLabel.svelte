<script lang="ts">
    /**
     * A favorite row's label, wherever the row is shown (the ⌃D menu, the switcher's
     * favorites section): its name, read quiet when a pick can't open it right away, or the
     * inline rename field while a rename is on. One component, so the two menus can't drift.
     */
    import { tString } from '$lib/intl/messages.svelte'
    import type { VolumeInfo } from '../types'
    import type { FavoritesMenuController } from './favorites-menu.svelte'

    interface Props {
        favorites: FavoritesMenuController
        volume: VolumeInfo
        label: string
        /** The rename `<input>`, which the controller focuses and selects when a rename starts. */
        renameInput?: HTMLInputElement
    }

    /* eslint-disable prefer-const -- $bindable() requires `let` destructuring */
    let { favorites, volume, label, renameInput = $bindable() }: Props = $props()
    /* eslint-enable prefer-const */
</script>

{#if favorites.renamingFavoriteId === volume.id}
    <!-- eslint-disable-next-line cmdr/prefer-ui-primitive -- Dense inline editor inside a menu row: it inherits the row's font and sits at row height with 2px side padding, which the framed `TextInput`'s padding would blow past, and it carries a resting accent border to read as "editing" rather than one that appears on focus. -->
    <input
        class="favorite-rename-input"
        bind:this={renameInput}
        bind:value={favorites.renameDraft}
        onkeydown={(e: KeyboardEvent) => { favorites.handleRenameKeyDown(e, volume) }}
        onblur={() => { void favorites.commitRename(volume) }}
        aria-label={tString('fileExplorer.navigation.renameFavoriteAriaLabel')}
    />
{:else}
    <!-- A row a pick can't open right away reads quiet, like the switcher's undialed saved
         place. ❌ Not `aria-disabled`: picking it is what dials it. Its tooltip says why. -->
    <span class="favorite-label" class:is-unreachable={favorites.isDimmed(volume)}>{label}</span>
{/if}

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
</style>
