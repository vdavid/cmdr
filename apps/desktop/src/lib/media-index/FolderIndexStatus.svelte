<script lang="ts">
    /**
     * The pane status bar's image-search readout for the folder you're standing in.
     * Rendered by `SelectionInfo.svelte` in every display mode.
     *
     * Informational, never a call to action: the actions (choose this folder, exclude it)
     * live in the folder's right-click menu, and a button down here would pull the status
     * bar into a control surface it isn't.
     *
     * Everything shown comes from state the frontend already holds (the master toggle, the
     * scope, the two folder lists, and the live per-volume enrichment activity). There's no
     * per-folder query and no poll: `media.db` has no cheap per-folder count, so the states
     * describe COVERAGE, never completion, and the automatic scope gets its own "can't say"
     * state rather than a guess. See `folder-index-state.ts`.
     */
    import { onMount } from 'svelte'
    import Icon from '$lib/ui/Icon.svelte'
    import type { IconName } from '$lib/ui/icons/icon-map'
    import { tooltip } from '$lib/tooltip/tooltip'
    import { tString } from '$lib/intl/messages.svelte'
    import { getSetting, onSpecificSettingChange } from '$lib/settings'
    import { getVolumeEnrichActivity } from '$lib/indexing/media-enrich-state.svelte'
    import { deriveFolderIndexState, type FolderIndexState } from './folder-index-state'

    interface Props {
        /** The pane's volume id, so a pass on another drive never lights this one up. */
        volumeId: string
        /** The pane's current folder as an absolute OS path (empty while it's landing). */
        folderPath: string
    }

    const { volumeId, folderPath }: Props = $props()

    // The settings this reads are all live-applied, so they can change in the settings
    // window while a pane is open; subscribe rather than snapshot at mount.
    let enabled = $state(getSetting('mediaIndex.enabled'))
    let scope = $state(getSetting('mediaIndex.scope'))
    let chosenFolders = $state(getSetting('mediaIndex.alwaysIndexFolders'))
    let excludedFolders = $state(getSetting('mediaIndex.excludedFolders'))

    onMount(() => {
        const unsubs = [
            onSpecificSettingChange('mediaIndex.enabled', (_id, value) => (enabled = value)),
            onSpecificSettingChange('mediaIndex.scope', (_id, value) => (scope = value)),
            onSpecificSettingChange('mediaIndex.alwaysIndexFolders', (_id, value) => (chosenFolders = value)),
            onSpecificSettingChange('mediaIndex.excludedFolders', (_id, value) => (excludedFolders = value)),
        ]
        return () => {
            for (const unsub of unsubs) unsub()
        }
    })

    // A paused pass is not active work, so it doesn't read as "indexing" (matching
    // `isAnyVolumeEnriching`'s rule for the corner hourglass).
    const activity = $derived(getVolumeEnrichActivity(volumeId))
    const enriching = $derived(activity !== undefined && activity.paused === null)

    const indexState = $derived(
        deriveFolderIndexState({ enabled, scope, chosenFolders, excludedFolders, folderPath, enriching }),
    )

    // Drive-wide progress, shown only in the tooltip and only once a tick has landed: the
    // figure is for the whole drive, never for this folder.
    const percent = $derived(
        activity && activity.total > 0 ? Math.min(100, Math.round((activity.done / activity.total) * 100)) : null,
    )

    const icons: Record<Exclude<FolderIndexState, 'off'>, IconName> = {
        indexing: 'hourglass',
        indexed: 'circle-check',
        notIndexed: 'circle',
        excluded: 'eye-off',
        automatic: 'sparkles',
    }

    const label = $derived.by(() => {
        switch (indexState) {
            case 'indexing':
                return tString('fileExplorer.imageIndex.indexing')
            case 'indexed':
                return tString('fileExplorer.imageIndex.indexed')
            case 'notIndexed':
                return tString('fileExplorer.imageIndex.notIndexed')
            case 'excluded':
                return tString('fileExplorer.imageIndex.excluded')
            case 'automatic':
                return tString('fileExplorer.imageIndex.automatic')
            case 'off':
                return ''
        }
    })

    const hint = $derived.by(() => {
        switch (indexState) {
            case 'indexing':
                return percent === null
                    ? tString('fileExplorer.imageIndex.indexingTooltipNoProgress')
                    : tString('fileExplorer.imageIndex.indexingTooltip', { percent })
            case 'indexed':
                return tString('fileExplorer.imageIndex.indexedTooltip')
            case 'notIndexed':
                return tString('fileExplorer.imageIndex.notIndexedTooltip')
            case 'excluded':
                return tString('fileExplorer.imageIndex.excludedTooltip')
            case 'automatic':
                return tString('fileExplorer.imageIndex.automaticTooltip')
            case 'off':
                return ''
        }
    })
</script>

{#if indexState !== 'off'}
    <span class="folder-index-status" data-state={indexState} use:tooltip={hint}>
        <Icon name={icons[indexState]} size={12} />
        {label}
    </span>
{/if}

<style>
    .folder-index-status {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xs);
        flex-shrink: 0;
        /* Between the pane's own content and the free-space readout, whatever the
           status bar's display mode puts in the DOM before it. */
        order: 1;
        padding-left: var(--spacing-md);
        color: var(--color-text-tertiary);
        white-space: nowrap;
        cursor: help;
    }

    /* A running pass is the one state that's happening right now, so it borrows the
       accent the other indexing indicators use. */
    .folder-index-status[data-state='indexing'] {
        /* stylelint-disable-next-line declaration-property-value-disallowed-list -- small status indicator, not body text */
        color: var(--color-accent);
    }
</style>
