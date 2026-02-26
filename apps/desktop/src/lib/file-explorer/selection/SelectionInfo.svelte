<script lang="ts">
    import type { FileEntry, ListingStats } from '../types'
    import {
        buildDateTooltip,
        getSizeDisplay,
        isBrokenSymlink as checkBrokenSymlink,
        isPermissionDenied as checkPermissionDenied,
        formatSizeTriads,
        pluralize,
        formatNumber,
        calculatePercentage,
    } from './selection-info-utils'
    import { measureDateColumnWidth } from '../views/full-list-utils'
    import { formatFileSize, formatDateTime } from '$lib/settings/reactive-settings.svelte'
    import { isScanning } from '$lib/indexing/index-state.svelte'
    import { tooltip } from '$lib/tooltip/tooltip'
    import type { VolumeSpaceInfo } from '$lib/tauri-commands/storage'
    import { formatDiskSpaceStatus } from '../disk-space-utils'

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

    const displayName = $derived(entry?.name ?? '')
    const isDirectory = $derived(entry?.isDirectory ?? false)
    const isBrokenSymlink = $derived(checkBrokenSymlink(entry))
    const isPermissionDenied = $derived(checkPermissionDenied(entry))
    const sizeDisplay = $derived(getSizeDisplay(entry, isBrokenSymlink, isPermissionDenied))
    const sizeTooltip = $derived(entry?.size !== undefined && !isDirectory ? formatFileSize(entry.size) : undefined)
    // Use formatDateTime from reactive-settings for consistent date formatting with Full mode
    const dateDisplay = $derived.by(() => {
        if (!entry) return ''
        if (isBrokenSymlink) return '(broken symlink)'
        if (isPermissionDenied) return '(permission denied)'
        // For ".." entry, use the current directory's modified time
        const timestamp = entry.name === '..' ? currentDirModifiedAt : entry.modifiedAt
        return formatDateTime(timestamp)
    })
    const dateTooltip = $derived(entry && !isBrokenSymlink && !isPermissionDenied ? buildDateTooltip(entry) : undefined)
    // Calculate date column width using measured text width (same utility as FullList)
    const dateColumnWidth = $derived(measureDateColumnWidth(formatDateTime))

    // Middle-truncate long filenames
    let nameElement: HTMLSpanElement | undefined = $state()
    let containerElement: HTMLDivElement | undefined = $state()

    // Use a separate state for truncated name, initialized lazily
    const getTruncatedName = $derived.by(() => {
        // This runs on every displayName change
        if (!nameElement || !containerElement || !entry) {
            return displayName
        }

        const containerWidth = containerElement.clientWidth
        // Account for size and date widths plus gaps
        const sizeEl = containerElement.querySelector('.size')
        const dateEl = containerElement.querySelector('.date')
        const sizeWidth = sizeEl instanceof HTMLElement ? sizeEl.offsetWidth : 0
        const dateWidth = dateEl instanceof HTMLElement ? dateEl.offsetWidth : 0
        const diskSpaceEl = containerElement.querySelector('.disk-space-text')
        const diskSpaceWidth = diskSpaceEl instanceof HTMLElement ? diskSpaceEl.offsetWidth : 0
        const availableWidth = containerWidth - sizeWidth - dateWidth - diskSpaceWidth - 24 // gaps

        // Create a temporary span to measure (avoids direct DOM manipulation)
        const measureSpan = document.createElement('span')
        measureSpan.style.cssText = 'position:absolute;visibility:hidden;white-space:nowrap;'
        measureSpan.style.font = getComputedStyle(nameElement).font
        document.body.appendChild(measureSpan)

        measureSpan.textContent = displayName
        const fullWidth = measureSpan.offsetWidth

        if (fullWidth <= availableWidth) {
            document.body.removeChild(measureSpan)
            return displayName
        }

        // Binary search for the right truncation point
        const extension = displayName.includes('.') ? displayName.slice(displayName.lastIndexOf('.')) : ''
        const baseName = displayName.includes('.') ? displayName.slice(0, displayName.lastIndexOf('.')) : displayName

        // Keep at least 4 chars of the base name visible
        const minPrefix = 4
        const ellipsis = '…'

        let low = minPrefix
        let high = baseName.length
        let bestFit = minPrefix

        while (low <= high) {
            const mid = Math.floor((low + high) / 2)
            measureSpan.textContent = baseName.slice(0, mid) + ellipsis + extension

            if (measureSpan.offsetWidth <= availableWidth) {
                bestFit = mid
                low = mid + 1
            } else {
                high = mid - 1
            }
        }

        document.body.removeChild(measureSpan)
        return baseName.slice(0, bestFit) + ellipsis + extension
    })

    // Track container width for reactivity
    let containerWidth = $state(0)

    // ResizeObserver for responsive truncation
    $effect(() => {
        if (!containerElement) return

        const observer = new ResizeObserver((entries) => {
            for (const e of entries) {
                containerWidth = e.contentRect.width
            }
        })

        observer.observe(containerElement)
        containerWidth = containerElement.clientWidth

        return () => {
            observer.disconnect()
        }
    })

    // Derive truncated name based on containerWidth (for reactivity)
    const truncatedName = $derived.by(() => {
        void containerWidth // Dependency trigger for resize
        return getTruncatedName
    })

    // ========================================================================
    // No-selection mode (Full mode without selection)
    // ========================================================================

    const noSelectionText = $derived.by(() => {
        if (!stats) return ''
        const { totalFiles, totalDirs } = stats

        const filesPart = `${formatNumber(totalFiles)} ${pluralize(totalFiles, 'file', 'files')}`
        const dirsPart = totalDirs > 0 ? ` and ${formatNumber(totalDirs)} ${pluralize(totalDirs, 'dir', 'dirs')}` : ''

        return `No selection, ${filesPart}${dirsPart}.`
    })

    // ========================================================================
    // Selection summary mode
    // ========================================================================

    // Drive index scanning state — used for stale indicator when dirs are selected
    const scanning = $derived(isScanning())

    // Computed values for selection summary
    const selectedFiles = $derived(stats?.selectedFiles ?? 0)
    const selectedDirs = $derived(stats?.selectedDirs ?? 0)
    const selectedFileSize = $derived(stats?.selectedFileSize ?? 0)
    const totalFiles = $derived(stats?.totalFiles ?? 0)
    const totalDirs = $derived(stats?.totalDirs ?? 0)
    const totalFileSize = $derived(stats?.totalFileSize ?? 0)

    const hasFiles = $derived(totalFiles > 0)
    const hasDirs = $derived(totalDirs > 0)
    const hasOnlyDirs = $derived(!hasFiles && hasDirs)

    // When directories are selected during scanning, sizes might be incomplete
    const showSelectionStale = $derived(scanning && selectedDirs > 0)

    const sizePercentage = $derived(calculatePercentage(selectedFileSize, totalFileSize))
    const filePercentage = $derived(calculatePercentage(selectedFiles, totalFiles))
    const dirPercentage = $derived(calculatePercentage(selectedDirs, totalDirs))

    // Size triads for selection summary
    const selectedSizeTriads = $derived(formatSizeTriads(selectedFileSize))
    const totalSizeTriads = $derived(formatSizeTriads(totalFileSize))

    // Tooltip with human-readable sizes
    const selectionSizeTooltip = $derived(
        hasFiles ? `${formatFileSize(selectedFileSize)} of ${formatFileSize(totalFileSize)}` : undefined,
    )
</script>

<div class="selection-info" bind:this={containerElement}>
    {#if displayMode === 'empty'}
        <span class="summary-text">Nothing in here.</span>
        {#if volumeSpace}
            <span class="disk-space-text">{formatDiskSpaceStatus(volumeSpace, formatFileSize)}</span>
        {/if}
    {:else if displayMode === 'file-info' && entry}
        <!-- Brief mode without selection: show file info -->
        <span class="name" bind:this={nameElement} use:tooltip={displayName}>{truncatedName}</span>
        <span class="size" use:tooltip={sizeTooltip}>
            {#if sizeDisplay === 'DIR'}
                DIR
            {:else if sizeDisplay}
                {#each sizeDisplay as triad, i (i)}
                    <span class={triad.tierClass}>{triad.value}</span>
                {/each}
            {/if}
        </span>
        <span class="date" style="width: {dateColumnWidth}px;" use:tooltip={dateTooltip}>{dateDisplay}</span>
        {#if volumeSpace}
            <span class="disk-space-text">{formatDiskSpaceStatus(volumeSpace, formatFileSize)}</span>
        {/if}
    {:else if displayMode === 'no-selection'}
        <!-- Full mode without selection: show totals -->
        <span class="summary-text">{noSelectionText}</span>
        {#if volumeSpace}
            <span class="disk-space-text">{formatDiskSpaceStatus(volumeSpace, formatFileSize)}</span>
        {/if}
    {:else if displayMode === 'selection-summary' && stats}
        <!-- Selection summary -->
        <span class="summary-text" use:tooltip={selectionSizeTooltip}>
            {#if hasOnlyDirs}
                <!-- Only dirs, no files -->
                {formatNumber(selectedDirs)} of {formatNumber(totalDirs)}
                {pluralize(totalDirs, 'dir', 'dirs')} ({dirPercentage}%) selected.
                {#if showSelectionStale}
                    <span class="stale-indicator" use:tooltip={'Might be outdated. Currently scanning...'}>⚠️</span>
                {/if}
            {:else if hasFiles}
                <!-- Has files: show full summary -->
                {#each selectedSizeTriads as triad, i (i)}<span class={triad.tierClass}>{triad.value}</span>{/each}
                of
                {#each totalSizeTriads as triad, i (i)}<span class={triad.tierClass}>{triad.value}</span>{/each}
                ({sizePercentage}%) selected in {formatNumber(selectedFiles)} of {formatNumber(totalFiles)}
                {pluralize(totalFiles, 'file', 'files')} ({filePercentage}%){#if hasDirs}
                    &nbsp;and {formatNumber(selectedDirs)} of {formatNumber(totalDirs)}
                    {pluralize(totalDirs, 'dir', 'dirs')} ({dirPercentage}%){/if}.
                {#if showSelectionStale}
                    <span class="stale-indicator" use:tooltip={'Might be outdated. Currently scanning...'}>⚠️</span>
                {/if}
            {/if}
        </span>
    {/if}
</div>

<style>
    .selection-info {
        display: flex;
        align-items: center;
        gap: 8px;
        padding: var(--spacing-xs) var(--spacing-sm);
        font-family: var(--font-system), sans-serif;
        font-size: calc(var(--font-size-sm) * 0.95);
        color: var(--color-text-secondary);
        background-color: var(--color-bg-secondary);
        border-top: 1px solid var(--color-border-strong);
        min-height: 1.5em;
    }

    .name {
        flex: 1 1 0;
        min-width: 0;
        overflow: hidden;
        white-space: nowrap;
        text-overflow: clip; /* We handle truncation manually */
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
        font-size: 10px;
        cursor: help;
    }
</style>
