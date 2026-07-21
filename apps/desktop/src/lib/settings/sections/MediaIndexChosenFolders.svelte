<script lang="ts">
    /**
     * The chosen-folders list: the folders the user picked for image indexing. Rendered
     * inside the "Image search" card in `ImageSearchSection.svelte`, under the scope
     * control, once the master `mediaIndex.enabled` toggle is on.
     *
     * These folders are indexed whatever the importance slider says, so in the "only
     * folders I choose" scope they ARE the coverage, and in the automatic scope they're the
     * escape hatch for a folder importance ranks too low to reach. Empty by default: on a
     * fresh install, image indexing covers nothing until the user names a folder here.
     *
     * Adding goes through the native folder picker (an absolute path is the only thing the
     * backend can match on, so there's no free-text field to typo), and the backend kicks an
     * indexing pass the moment a folder lands. Removing stops future indexing but deletes
     * nothing: the folder's existing rows stay searchable until the user reclaims the space.
     *
     * No per-folder progress line: the backend has no cheap per-folder count (it would mean
     * a prefix scan of `media.db` per folder per poll), so progress is voiced once for the
     * whole drive by the slider's own line rather than faked per row.
     */
    import { onMount } from 'svelte'
    import { open as openFolderPicker } from '@tauri-apps/plugin-dialog'
    import { tString } from '$lib/intl/messages.svelte'
    import { onSpecificSettingChange } from '$lib/settings'
    import { getChosenFolders, isFolderChosen, setFolderChosen } from '$lib/media-index/always-index-folders'
    import { getAppLogger } from '$lib/logging/logger'
    import Button from '$lib/ui/Button.svelte'
    import Icon from '$lib/ui/Icon.svelte'

    const log = getAppLogger('media-index')

    let folders = $state<string[]>(getChosenFolders())

    onMount(() =>
        // Keep the list in sync when a folder is added or removed in another window.
        onSpecificSettingChange('mediaIndex.alwaysIndexFolders', () => {
            folders = getChosenFolders()
        }),
    )

    async function handleAdd(): Promise<void> {
        let picked: string | string[] | null
        try {
            picked = await openFolderPicker({ directory: true, multiple: false })
        } catch (err) {
            log.warn('The folder picker did not open: {err}', { err: String(err) })
            return
        }
        // The user cancelled, or picked something we can't resolve to one path.
        if (typeof picked !== 'string') return
        // Adding the same folder twice would be a no-op backend-side (it's a set) but would
        // show a duplicate row, so drop it here.
        if (isFolderChosen(picked)) return
        try {
            await setFolderChosen(picked, true)
        } catch {
            // `setFolderChosen` rolled the persisted value back and logged; re-read so the
            // list shows what actually stuck.
        }
        folders = getChosenFolders()
    }

    async function handleRemove(folder: string): Promise<void> {
        try {
            await setFolderChosen(folder, false)
        } catch {
            // Rolled back and logged inside; re-read below.
        }
        folders = getChosenFolders()
    }

    /** The last path segment, as the readable name; the full path is the second line. */
    function folderName(path: string): string {
        const trimmed = path.replace(/\/+$/, '')
        return trimmed.slice(trimmed.lastIndexOf('/') + 1) || trimmed
    }
</script>

<div class="mi-folders">
    <h4 class="mi-folders-title">{tString('settings.mediaIndex.chosenFolders.title')}</h4>
    <p class="mi-folders-help">{tString('settings.mediaIndex.chosenFolders.help')}</p>

    {#if folders.length === 0}
        <p class="mi-folders-empty">{tString('settings.mediaIndex.chosenFolders.empty')}</p>
    {:else}
        <ul class="mi-folders-list">
            {#each folders as folder (folder)}
                <li class="mi-folders-row">
                    <span class="mi-folders-icon" aria-hidden="true"><Icon name="folder" size={16} /></span>
                    <span class="mi-folders-path">
                        <span class="mi-folders-name">{folderName(folder)}</span>
                        <span class="mi-folders-full">{folder}</span>
                    </span>
                    <Button
                        size="mini"
                        aria-label={tString('settings.mediaIndex.chosenFolders.removeAria', { folder })}
                        onclick={() => void handleRemove(folder)}
                    >
                        {tString('settings.mediaIndex.chosenFolders.remove')}
                    </Button>
                </li>
            {/each}
        </ul>
    {/if}

    <div class="mi-folders-actions">
        <Button onclick={() => void handleAdd()}>{tString('settings.mediaIndex.chosenFolders.add')}</Button>
    </div>
</div>

<style>
    .mi-folders {
        margin-top: var(--spacing-sm);
        padding-top: var(--spacing-sm);
        border-top: 1px solid var(--color-border-subtle);
    }

    .mi-folders-title {
        margin: 0;
        font-size: var(--font-size-md);
        font-weight: 600;
        color: var(--color-text-primary);
    }

    .mi-folders-help {
        margin: var(--spacing-xxs) 0 var(--spacing-sm);
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
        line-height: 1.4;
    }

    .mi-folders-empty {
        margin: 0 0 var(--spacing-sm);
        color: var(--color-text-tertiary);
        font-size: var(--font-size-sm);
        line-height: 1.4;
    }

    .mi-folders-list {
        list-style: none;
        margin: 0 0 var(--spacing-sm);
        padding: 0;
    }

    .mi-folders-row {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
        padding: var(--spacing-xs) 0;
        border-bottom: 1px solid var(--color-border-subtle);
    }

    .mi-folders-row:last-child {
        border-bottom: none;
    }

    .mi-folders-icon {
        display: flex;
        color: var(--color-text-tertiary);
    }

    .mi-folders-path {
        display: flex;
        flex-direction: column;
        min-width: 0;
        flex: 1;
    }

    .mi-folders-name {
        color: var(--color-text-primary);
        font-size: var(--font-size-sm);
        font-weight: 500;
    }

    .mi-folders-full {
        color: var(--color-text-tertiary);
        font-size: var(--font-size-sm);
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .mi-folders-actions {
        display: flex;
    }
</style>
