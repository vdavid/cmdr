<script lang="ts">
    /**
     * A favorite row's "Set shortcut" capture, at the right of the row, wherever the row is
     * shown (the ⌃D menu, the switcher's favorites section). Renders nothing unless this
     * favorite's shortcut is being set.
     */
    import { tString } from '$lib/intl/messages.svelte'
    import type { VolumeInfo } from '../types'
    import type { FavoritesMenuController } from './favorites-menu.svelte'

    interface Props {
        favorites: FavoritesMenuController
        volume: VolumeInfo
        /** The capture `<input>`, which the controller focuses when the edit starts. */
        shortcutInput?: HTMLInputElement
    }

    /* eslint-disable prefer-const -- $bindable() requires `let` destructuring */
    let { favorites, volume, shortcutInput = $bindable() }: Props = $props()
    /* eslint-enable prefer-const */
</script>

{#if favorites.editingShortcutId === volume.id}
    <!-- eslint-disable-next-line cmdr/prefer-ui-primitive -- This read-only, one-key capture sits inside a menu row, not a framed text field. -->
    <input
        class="favorite-shortcut-input"
        bind:this={shortcutInput}
        readonly
        value={volume.favoriteShortcut ?? ''}
        placeholder={tString('fileExplorer.navigation.favoriteShortcutPrompt')}
        aria-label={tString('fileExplorer.navigation.favoriteShortcutAriaLabel', { name: volume.name })}
        onkeydown={(event: KeyboardEvent) => { favorites.handleShortcutKeyDown(event, volume) }}
        onblur={favorites.cancelShortcutEdit}
    />
{/if}

<style>
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
