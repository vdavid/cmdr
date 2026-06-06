<script lang="ts">
    import IconHourglass from '~icons/lucide/hourglass'
    import IconInfo from '~icons/lucide/info'
    import type { FileEntry, ListingStats } from '../types'
    import {
        buildDateTooltip,
        getSizeDisplay,
        isBrokenSymlink as checkBrokenSymlink,
        isPermissionDenied as checkPermissionDenied,
        formatSizeForDisplay,
        formatNumber,
        calculatePercentage,
    } from './selection-info-utils'
    import { pluralize } from '$lib/utils/pluralize'
    import { measureDateColumnWidth } from '../views/full-list-utils'
    import {
        formatFileSize,
        formatDateTime,
        formattedDate,
        getSizeDisplayMode,
        getFileSizeUnit,
        getFileSizeFormat,
    } from '$lib/settings/reactive-settings.svelte'
    import DateLabel from '$lib/ui/DateLabel.svelte'
    import {
        getDisplaySize,
        getDirSizeDisplayState,
        buildFileSizeTooltip,
        buildDirSizeTooltip,
        buildSelectionSizeTooltip,
    } from '../views/full-list-utils'
    import { isScanning, isAggregating } from '$lib/indexing/index-state.svelte'
    import { tooltip } from '$lib/tooltip/tooltip'
    import { useShortenMiddle } from '$lib/utils/shorten-middle-action'
    import type { VolumeSpaceInfo } from '$lib/tauri-commands'
    import { formatDiskSpaceStatus } from '../disk-space-utils'
    import { formatFileSizeWithFormat } from '$lib/settings/format-utils'

    // Free-space text is intentionally uncolored: red GB would falsely signal "low space".
    function diskSpaceStatusText(space: VolumeSpaceInfo): string {
        const format = getFileSizeFormat()
        return formatDiskSpaceStatus(space, (b) => formatFileSizeWithFormat(b, format))
    }

    interface Props {
        /** View mode: 'brief' or 'full' */
        viewMode: 'brief' | 'full'
        /** Entry under cursor (for Brief mode without selection) */
        entry: FileEntry | null
        /** Modified timestamp of the current directory (for ".." entry) */
        currentDirModifiedAt?: number
        /** Listing statistics from backend */
        stats: ListingStats | null
        /** Number of selected items */
        selectedCount: number
        /** Disk space info for current volume (null when unavailable) */
        volumeSpace?: VolumeSpaceInfo | null
    }

    const { viewMode, entry, currentDirModifiedAt, stats, selectedCount, volumeSpace }: Props = $props()

    // ========================================================================
    // Display mode determination
    // ========================================================================

    type DisplayMode = 'file-info' | 'no-selection' | 'selection-summary' | 'empty'

    const displayMode = $derived.by((): DisplayMode => {
        // Empty directory
        if (stats && stats.totalFiles === 0 && stats.totalDirs === 0) {
            return 'empty'
        }

        // Has selection → show selection summary
        if (selectedCount > 0) {
            return 'selection-summary'
        }

        // Full mode without selection → show totals
        if (viewMode === 'full') {
            return 'no-selection'
        }

        // Brief mode without selection → show file info (existing behavior)
        return 'file-info'
    })

    // ========================================================================
    // File info mode (Brief mode without selection)
    // ========================================================================

    // Drive index scanning state (used for the selection-summary stale indicator).
    const scanning = $derived(isScanning())
    // Full index activity (scan OR aggregation) for the per-folder file-info
    // readout, matching FullList. `scanning` alone misses the aggregation phase.
    const indexing = $derived(isScanning() || isAggregating())

    const sizeDisplayMode = $derived(getSizeDisplayMode())
    const sizeFormatOpts = $derived({
        unit: getFileSizeUnit(),
        format: getFileSizeFormat(),
    })
    const displayName = $derived(entry?.name ?? '')
    const isDirectory = $derived(entry?.isDirectory ?? false)
    const isBrokenSymlink = $derived(checkBrokenSymlink(entry))
    const isPermissionDenied = $derived(checkPermissionDenied(entry))
    const displaySize = $derived(
        entry
            ? getDisplaySize(
                  isDirectory ? entry.recursiveSize : entry.size,
                  isDirectory ? entry.recursivePhysicalSize : entry.physicalSize,
                  sizeDisplayMode,
              )
            : undefined,
    )
    const sizeDisplay = $derived(
        getSizeDisplay(entry, isBrokenSymlink, isPermissionDenied, displaySize, sizeFormatOpts),
    )
    // Per-folder size-column state, shared with FullList via getDirSizeDisplayState.
    // `dirActive` = the folder's size is unsettled: a full scan/aggregation is
    // running, OR this folder has live index writes in flight (recursiveSizePending).
    const dirActive = $derived(isDirectory && (indexing || (entry?.recursiveSizePending ?? false)))
    const dirSizeState = $derived(
        isDirectory ? getDirSizeDisplayState(displaySize, indexing, entry?.recursiveSizePending) : null,
    )
    const sizeTooltip = $derived(
        entry
            ? isDirectory
                ? buildDirSizeTooltip(
                      entry.recursiveSize,
                      entry.recursivePhysicalSize,
                      entry.recursiveFileCount ?? 0,
                      entry.recursiveDirCount ?? 0,
                      dirActive,
                      formatFileSize,
                      formatNumber,
                      pluralize,
                  ) || undefined
                : buildFileSizeTooltip(entry.size, entry.physicalSize, formatFileSize)
            : undefined,
    )
    /**
     * `placeholder` is a string for the special states (broken/permission) and
     * `null` when we should render the actual timestamp via `<DateLabel>`.
     * `dateTimestamp` carries the value for that case (the parent dir's
     * modifiedAt covers the `..` row).
     */
    const datePlaceholder = $derived.by(() => {
        if (!entry) return ''
        if (isBrokenSymlink) return '(broken symlink)'
        if (isPermissionDenied) return '(permission denied)'
        return null
    })
    const dateTimestamp = $derived(entry?.name === '..' ? currentDirModifiedAt : entry?.modifiedAt)
    const dateTooltip = $derived(
        entry && !isBrokenSymlink && !isPermissionDenied ? buildDateTooltip(entry, formattedDate) : undefined,
    )
    // Show an info hint next to a directory's size when its subtree contains
    // symlinks: their content is intentionally excluded from the recursive
    // size (matching `du`/Finder), but that can be surprising for folders that
    // are mostly symlinks.
    const showSymlinkHint = $derived(
        entry !== null && isDirectory && entry.recursiveHasSymlinks === true && !isBrokenSymlink && !isPermissionDenied,
    )
    const symlinkHintTooltip =
        'This folder contains symlinks. Symlinked content is not counted in the total to avoid double counting.'
    // Calculate date column width using measured text width (same utility as FullList)
    const dateColumnWidth = $derived(measureDateColumnWidth(formatDateTime))

    // ========================================================================
    // No-selection mode (Full mode without selection)
    // ========================================================================

    const noSelectionText = $derived.by(() => {
        if (!stats) return ''
        const { totalFiles, totalDirs } = stats

        const filesPart = `${formatNumber(totalFiles)} ${pluralize(totalFiles, 'file')}`
        const dirsPart = totalDirs > 0 ? ` and ${formatNumber(totalDirs)} ${pluralize(totalDirs, 'dir')}` : ''

        return `No selection, ${filesPart}${dirsPart}.`
    })

    // ========================================================================
    // Selection summary mode
    // ========================================================================

    // Computed values for selection summary
    const selectedFiles = $derived(stats?.selectedFiles ?? 0)
    const selectedDirs = $derived(stats?.selectedDirs ?? 0)
    const selectedLogicalSize = $derived(stats?.selectedSize ?? 0)
    const selectedPhysicalSize = $derived(stats?.selectedPhysicalSize ?? 0)
    const totalFiles = $derived(stats?.totalFiles ?? 0)
    const totalDirs = $derived(stats?.totalDirs ?? 0)
    const totalLogicalSize = $derived(stats?.totalSize ?? 0)
    const totalPhysicalSize = $derived(stats?.totalPhysicalSize ?? 0)

    // Apply the user's size display preference to selection totals
    const selectedSize = $derived(
        getDisplaySize(selectedLogicalSize, selectedPhysicalSize, sizeDisplayMode) ?? selectedLogicalSize,
    )
    const totalSize = $derived(getDisplaySize(totalLogicalSize, totalPhysicalSize, sizeDisplayMode) ?? totalLogicalSize)

    const hasFiles = $derived(totalFiles > 0)
    const hasDirs = $derived(totalDirs > 0)
    const hasOnlyDirs = $derived(!hasFiles && hasDirs)

    // When directories are selected during scanning, sizes might be incomplete
    const showSelectionStale = $derived(scanning && selectedDirs > 0)

    const sizePercentage = $derived(calculatePercentage(selectedSize, totalSize))

    // Size display for selection summary (triads in raw mode, single span in human-friendly mode)
    const selectedSizeTriads = $derived(formatSizeForDisplay(selectedSize, sizeFormatOpts))
    const totalSizeTriads = $derived(formatSizeForDisplay(totalSize, sizeFormatOpts))

    // Tooltip shows human-readable sizes; includes both content and on-disk when they differ
    const selectionSizeTooltip = $derived(
        buildSelectionSizeTooltip(
            selectedLogicalSize,
            selectedPhysicalSize,
            totalLogicalSize,
            totalPhysicalSize,
            formatFileSize,
        ),
    )
</script>

<div class="selection-info">
    {#if displayMode === 'empty'}
        <span class="summary-text">Nothing in here.</span>
        {#if volumeSpace}
            <span class="disk-space-text">{diskSpaceStatusText(volumeSpace)}</span>
        {/if}
    {:else if displayMode === 'file-info' && entry}
        <!-- Brief mode without selection: show file info -->
        <span class="name" use:tooltip={displayName} use:useShortenMiddle={{ text: displayName, preferBreakAt: '.', startRatio: 0.7 }}></span>
        <span class="size" use:tooltip={sizeTooltip}>
            {#if sizeDisplay === 'DIR'}
                DIR
                {#if dirSizeState === 'scanning'}
                    <span
                        class="stale-indicator stale-icon"
                        role="img"
                        aria-label="Size not ready yet"
                        use:tooltip={'Sizes appear as the scan progresses'}
                    >
                        <IconHourglass width="12" height="12" />
                    </span>
                {/if}
            {:else if sizeDisplay}
                {#each sizeDisplay as triad, i (i)}
                    <span class={triad.tierClass}>{triad.value}</span>
                {/each}
                {#if dirSizeState === 'size-stale'}
                    <span
                        class="stale-indicator stale-icon"
                        role="img"
                        aria-label="Size updating"
                        use:tooltip={'Updating index, size may change.'}
                    >
                        <IconHourglass width="12" height="12" />
                    </span>
                {/if}
            {/if}
            {#if showSymlinkHint}
                <span
                    class="symlink-hint symlink-hint-icon"
                    role="img"
                    aria-label={symlinkHintTooltip}
                    use:tooltip={symlinkHintTooltip}
                >
                    <IconInfo width="12" height="12" />
                </span>
            {/if}
        </span>
        <span class="date" style="width: {dateColumnWidth}px;" use:tooltip={dateTooltip}>
            {#if datePlaceholder !== null}{datePlaceholder}{:else}<DateLabel modifiedAt={dateTimestamp} />{/if}
        </span>
        {#if volumeSpace}
            <span class="disk-space-text">{diskSpaceStatusText(volumeSpace)}</span>
        {/if}
    {:else if displayMode === 'no-selection'}
        <!-- Full mode without selection: show totals -->
        <span class="summary-text">{noSelectionText}</span>
        {#if volumeSpace}
            <span class="disk-space-text">{diskSpaceStatusText(volumeSpace)}</span>
        {/if}
    {:else if displayMode === 'selection-summary' && stats}
        <!-- Selection summary -->
        <span class="summary-text" use:tooltip={selectionSizeTooltip}>
            {#if hasOnlyDirs}
                <!-- Only dirs, no files -->
                {#if totalSize > 0}
                    {#each selectedSizeTriads as triad, i (i)}<span class={triad.tierClass}>{triad.value}</span>{/each}
                    of
                    {#each totalSizeTriads as triad, i (i)}<span class={triad.tierClass}>{triad.value}</span>{/each}
                    ({sizePercentage}%) selected in
                {/if}
                {formatNumber(selectedDirs)} of {formatNumber(totalDirs)}
                {pluralize(totalDirs, 'dir')}{#if totalSize === 0}
                    selected{/if}.
                {#if showSelectionStale}
                    <span class="stale-indicator stale-icon" use:tooltip={'Updating index, size may change.'}
                        ><IconHourglass width="12" height="12" /></span
                    >
                {/if}
            {:else if hasFiles}
                <!-- Has files: show full summary -->
                {#each selectedSizeTriads as triad, i (i)}<span class={triad.tierClass}>{triad.value}</span>{/each}
                of
                {#each totalSizeTriads as triad, i (i)}<span class={triad.tierClass}>{triad.value}</span>{/each}
                ({sizePercentage}%) selected in {formatNumber(selectedFiles)} of {formatNumber(totalFiles)}
                {pluralize(totalFiles, 'file')}{#if hasDirs}
                    &nbsp;and {formatNumber(selectedDirs)} of {formatNumber(totalDirs)}
                    {pluralize(totalDirs, 'dir')}{/if}.
                {#if showSelectionStale}
                    <span class="stale-indicator stale-icon" use:tooltip={'Updating index, size may change.'}
                        ><IconHourglass width="12" height="12" /></span
                    >
                {/if}
            {/if}
        </span>
    {/if}
</div>

<style>
    .selection-info {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
        padding: var(--spacing-xs) var(--spacing-sm);
        font-family: var(--font-system), sans-serif;
        font-size: calc(var(--font-size-sm) * 0.95);
        color: var(--color-text-secondary);
        background-color: var(--color-bg-info-bar);
        min-height: 1.5em;
    }

    .name {
        flex: 1 1 0;
        min-width: 0;
        overflow: hidden;
        white-space: nowrap;
    }

    .size {
        flex-shrink: 0;
        text-align: right;
        font-size: calc(var(--font-size-sm) * 0.9);
    }

    .date {
        flex-shrink: 0;
        /* width is set via inline style based on formatted date length */
        text-align: right;
        font-size: calc(var(--font-size-sm) * 0.9);
    }

    .summary-text {
        flex: 1 1 0;
        min-width: 0;
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
    }

    .disk-space-text {
        flex-shrink: 0;
        margin-left: auto;
        padding-left: var(--spacing-md);
        color: var(--color-text-tertiary);
        white-space: nowrap;
    }

    .stale-indicator {
        display: inline-flex;
        align-items: center;
        vertical-align: middle;
        cursor: help;
    }

    .stale-icon {
        /* stylelint-disable-next-line declaration-property-value-disallowed-list -- small icon indicator, not body text */
        color: var(--color-accent);
    }

    .symlink-hint {
        display: inline-flex;
        align-items: center;
        vertical-align: middle;
        margin-left: var(--spacing-xs);
        cursor: help;
    }

    .symlink-hint-icon {
        color: var(--color-text-tertiary);
    }
</style>
