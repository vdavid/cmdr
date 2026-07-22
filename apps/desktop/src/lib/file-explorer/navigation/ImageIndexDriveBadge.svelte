<script lang="ts">
    /**
     * Per-drive IMAGE-search status dot: a small colored dot immediately after the
     * filesystem `DriveIndexBadge` in `VolumeBreadcrumb.svelte` (both the active-drive
     * breadcrumb and the volume-dropdown rows). Three states: gray `off`, pulsing yellow
     * `indexing`, green `done`. Non-interactive — a labeled status glyph (`role="img"` +
     * `aria-label`), NOT focusable: unlike the sibling `DriveIndexBadge` (a menu button),
     * it has no action, so it stays out of the tab order rather than adding a dead tab stop
     * per drive. Screen readers announce the aria-label; the mouse-hover tooltip carries the
     * live "N of M images indexed on this drive" phrase.
     *
     * State + tooltip counts derive from the pure `image-index-drive-state.ts`; the dot's
     * dimensions and pulse mirror the sibling `DriveIndexBadge`. Hides itself entirely on
     * drives with nothing to index, so unrelated drives stay clean.
     */
    import type { MediaIndexVolumeState } from '$lib/tauri-commands'
    import { tooltip } from '$lib/tooltip/tooltip'
    import { tString } from '$lib/intl/messages.svelte'
    import { formatInteger } from '$lib/intl/number-format'
    import { getMediaIndexEnabled } from '$lib/settings/reactive-settings.svelte'
    import { getVolumeEnrichActivity } from '$lib/indexing/media-enrich-state.svelte'
    import { imageIndexDriveState, imageIndexDriveCoverage } from './image-index-drive-state'

    interface Props {
        /** The drive this dot describes. */
        volumeId: string
        /** The honest per-volume image-index state (fetched + refreshed by the parent). */
        volumeState: MediaIndexVolumeState
        /**
         * Small left margin for the always-visible breadcrumb placement (matches the
         * filesystem dot's `.breadcrumb-drive-index-badge`). Off for dropdown rows.
         */
        breadcrumb?: boolean
    }

    const { volumeId, volumeState, breadcrumb = false }: Props = $props()

    // Live enrichment activity is reactive (the `media-enrich-state` SvelteMap); the master
    // toggle is a reactive getter. `volumeState` is refetched by the parent on this volume's
    // enrich events, so the counts stay live without a poll.
    const enrichActivity = $derived(getVolumeEnrichActivity(volumeId))
    const state = $derived(imageIndexDriveState({ enabled: getMediaIndexEnabled(), volumeState, enrichActivity }))

    // Hide entirely on drives with nothing to index (or not scored yet), so unrelated drives
    // stay clean. `qualifyingCount === 0` = no images qualify; `null` = the index can't say yet.
    const hidden = $derived(volumeState.qualifyingCount == null || volumeState.qualifyingCount === 0)

    const coverage = $derived(imageIndexDriveCoverage(volumeState))

    const tooltipText = $derived.by(() => {
        if (state === 'off' || coverage === null) return tString('fileExplorer.imageIndex.drive.off')
        if (state === 'done') {
            return tString('fileExplorer.imageIndex.drive.done', {
                total: coverage.total,
                totalText: formatInteger(coverage.total),
            })
        }
        return tString('fileExplorer.imageIndex.drive.indexing', {
            done: coverage.done,
            doneText: formatInteger(coverage.done),
            total: coverage.total,
            totalText: formatInteger(coverage.total),
        })
    })
</script>

{#if !hidden}
    <span
        class="image-index-drive-badge image-index-drive-badge-{state}"
        class:breadcrumb-image-index-drive-badge={breadcrumb}
        role="img"
        aria-label={`${tString('fileExplorer.imageIndex.drive.ariaLabel')}: ${tooltipText}`}
        use:tooltip={tooltipText}
    ></span>
{/if}

<style>
    /* Mirrors `.drive-index-badge`: same 10px round dot, same flex sizing. Non-interactive
       and non-focusable (a labeled status glyph, not a control), so a <span> (role="img")
       rather than a <button>. */
    .image-index-drive-badge {
        width: 10px;
        height: 10px;
        border-radius: 50%;
        flex-shrink: 0;
        opacity: 0.8;
        background-color: var(--color-text-tertiary);
    }

    /*noinspection CssUnusedSymbol*/
    .image-index-drive-badge-off {
        background-color: var(--color-text-tertiary);
    }

    /*noinspection CssUnusedSymbol*/
    .image-index-drive-badge-indexing {
        background-color: var(--color-warning);
    }

    /*noinspection CssUnusedSymbol*/
    .image-index-drive-badge-done {
        background-color: var(--color-allow);
    }

    /* The indexing dot pulses to signal live work, like the filesystem scanning dot and the
       corner hourglass. Gated behind reduced-motion. */
    @media (prefers-reduced-motion: no-preference) {
        /*noinspection CssUnusedSymbol*/
        .image-index-drive-badge-indexing {
            animation: image-index-drive-pulse 2s ease-in-out infinite;
        }
    }

    @keyframes image-index-drive-pulse {
        0%,
        100% {
            opacity: 0.5;
        }
        50% {
            opacity: 1;
        }
    }

    /* Closed-breadcrumb placement: a small left margin so it sits next to the filesystem
       drive-index dot instead of jamming against it. */
    .breadcrumb-image-index-drive-badge {
        margin-left: var(--spacing-xs);
    }

    /* In a dropdown row it follows the right-aligned filesystem dot; a small gap after it. */
    :global(.volume-item) .image-index-drive-badge {
        margin-left: var(--spacing-sm);
    }
</style>
