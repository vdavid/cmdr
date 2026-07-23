<script lang="ts">
    /**
     * The chosen-folders list: the folders the user picked for image indexing. Rendered
     * inside the "Image indexing" card in `ImageIndexingSection.svelte`, under the scope
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
     * Each folder shows a live "N of M indexed" coverage line via `mediaIndexFolderCoverage`
     * (reusing the file list's `getFolderCoverageBadge`), polled lightly while open so it
     * stays honest as a fresh folder indexes. Coverage is queried against the local root
     * volume; a chosen folder on another local drive simply shows no line (its paths don't
     * match root), which is honest rather than wrong.
     */
    import { onMount } from 'svelte'
    import { open as openFolderPicker } from '@tauri-apps/plugin-dialog'
    import { tString } from '$lib/intl/messages.svelte'
    import { onSpecificSettingChange } from '$lib/settings'
    import { getChosenFolders, isFolderChosen, setFolderChosen } from '$lib/media-index/always-index-folders'
    import { getAppLogger } from '$lib/logging/logger'
    import { ROOT_VOLUME_ID } from '$lib/indexing'
    import { mediaIndexFolderCoverage, type FolderCoverage } from '$lib/tauri-commands'
    import { getFolderCoverageBadge } from '$lib/file-explorer/views/file-list-utils'
    import Button from '$lib/ui/Button.svelte'
    import Icon from '$lib/ui/Icon.svelte'

    const log = getAppLogger('media-index')

    let folders = $state<string[]>(getChosenFolders())
    // Per-folder subtree coverage (path → eligible/accounted), for the "N of M indexed" line.
    let coverage = $state<Record<string, FolderCoverage>>({})

    async function refreshCoverage(): Promise<void> {
        if (folders.length === 0) {
            coverage = {}
            return
        }
        try {
            const results = await mediaIndexFolderCoverage(ROOT_VOLUME_ID, folders)
            const next: Record<string, FolderCoverage> = {}
            for (const cov of results) next[cov.path] = cov
            coverage = next
        } catch (err) {
            log.warn('folder-coverage query failed: {err}', { err: String(err) })
        }
    }

    onMount(() => {
        void refreshCoverage()
        // Keep the list in sync when a folder is added or removed in another window.
        const unsub = onSpecificSettingChange('mediaIndex.alwaysIndexFolders', () => {
            folders = getChosenFolders()
            void refreshCoverage()
        })
        // Light poll so a freshly-added folder's "N of M indexed" line advances while open.
        const timer = setInterval(() => void refreshCoverage(), 3000)
        return () => {
            unsub()
            clearInterval(timer)
        }
    })

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
        void refreshCoverage()
    }

    async function handleRemove(folder: string): Promise<void> {
        try {
            await setFolderChosen(folder, false)
        } catch {
            // Rolled back and logged inside; re-read below.
        }
        folders = getChosenFolders()
        void refreshCoverage()
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
                {@const badge = getFolderCoverageBadge(coverage[folder], tString)}
                <li class="mi-folders-row">
                    <span class="mi-folders-icon" aria-hidden="true"><Icon name="folder" size={16} /></span>
                    <span class="mi-folders-path">
                        <span class="mi-folders-name">{folderName(folder)}</span>
                        <span class="mi-folders-full">{folder}</span>
                        {#if badge}
                            <span class="mi-folders-coverage">
                                <Icon name={badge.icon} size={12} aria-hidden="true" />
                                <span>{badge.tooltip}</span>
                            </span>
                        {/if}
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

    .mi-folders-coverage {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xxs);
        margin-top: var(--spacing-xxs);
        color: var(--color-text-tertiary);
        font-size: var(--font-size-sm);
    }

    .mi-folders-actions {
        display: flex;
    }
</style>
