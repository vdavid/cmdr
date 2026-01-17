<script lang="ts">
    import type { FileEntry, ListingStats } from './types'
    import {
        formatHumanReadable,
        buildDateTooltip,
        getSizeDisplay,
        getDateDisplay,
        isBrokenSymlink as checkBrokenSymlink,
        isPermissionDenied as checkPermissionDenied,
        formatSizeTriads,
        pluralize,
        formatNumber,
        calculatePercentage,
    } from './selection-info-utils'

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
    }

    const { viewMode, entry, currentDirModifiedAt, stats, selectedCount }: Props = $props()

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
    const sizeTooltip = $derived(
        entry?.size !== undefined && !isDirectory ? formatHumanReadable(entry.size) : undefined,
    )
    const dateDisplay = $derived(getDateDisplay(entry, isBrokenSymlink, isPermissionDenied, currentDirModifiedAt))
    const dateTooltip = $derived(entry && !isBrokenSymlink && !isPermissionDenied ? buildDateTooltip(entry) : undefined)

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
        const availableWidth = containerWidth - sizeWidth - dateWidth - 24 // gaps

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

    const sizePercentage = $derived(calculatePercentage(selectedFileSize, totalFileSize))
    const filePercentage = $derived(calculatePercentage(selectedFiles, totalFiles))
    const dirPercentage = $derived(calculatePercentage(selectedDirs, totalDirs))

    // Size triads for selection summary
    const selectedSizeTriads = $derived(formatSizeTriads(selectedFileSize))
    const totalSizeTriads = $derived(formatSizeTriads(totalFileSize))

    // Tooltip with human-readable sizes
    const selectionSizeTooltip = $derived(
        hasFiles ? `${formatHumanReadable(selectedFileSize)} of ${formatHumanReadable(totalFileSize)}` : undefined,
    )
</script>

<div class="selection-info" bind:this={containerElement}>
    {#if displayMode === 'empty'}
        <span class="summary-text">Nothing in here.</span>
    {:else if displayMode === 'file-info' && entry}
        <!-- Brief mode without selection: show file info -->
        <span class="name" bind:this={nameElement} title={displayName}>{truncatedName}</span>
        <span class="size" title={sizeTooltip}>
            {#if sizeDisplay === 'DIR'}
                DIR
            {:else if sizeDisplay}
                {#each sizeDisplay as triad, i (i)}
                    <span class={triad.tierClass}>{triad.value}</span>
                {/each}
            {/if}
        </span>
        <span class="date" title={dateTooltip}>{dateDisplay}</span>
    {:else if displayMode === 'no-selection'}
        <!-- Full mode without selection: show totals -->
        <span class="summary-text">{noSelectionText}</span>
    {:else if displayMode === 'selection-summary' && stats}
        <!-- Selection summary -->
        <span class="summary-text" title={selectionSizeTooltip}>
            {#if hasOnlyDirs}
                <!-- Only dirs, no files: can't show size -->
                {formatNumber(selectedDirs)} of {formatNumber(totalDirs)}
                {pluralize(totalDirs, 'dir', 'dirs')} ({dirPercentage}%) selected.
            {:else if hasFiles}
                <!-- Has files: show full summary -->
                {#each selectedSizeTriads as triad, i (i)}<span class={triad.tierClass}>{triad.value}</span>{/each}
                of
                {#each totalSizeTriads as triad, i (i)}<span class={triad.tierClass}>{triad.value}</span>{/each}
                ({sizePercentage}%) selected in {formatNumber(selectedFiles)} of {formatNumber(totalFiles)}
                {pluralize(totalFiles, 'file', 'files')} ({filePercentage}%){#if hasDirs}
                    &nbsp;and {formatNumber(selectedDirs)} of {formatNumber(totalDirs)}
                    {pluralize(totalDirs, 'dir', 'dirs')} ({dirPercentage}%){/if}.
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
        border-top: 1px solid var(--color-border-primary);
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
        width: 140px;
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

    /* Size tier colors - bytes are default text color - these are used dynamically */
    /*noinspection CssUnusedSymbol*/
    .size-bytes {
        color: var(--color-text-secondary);
    }

    /*noinspection CssUnusedSymbol*/
    .size-kb {
        color: var(--color-size-kb);
    }

    /*noinspection CssUnusedSymbol*/
    .size-mb {
        color: var(--color-size-mb);
    }

    /*noinspection CssUnusedSymbol*/
    .size-gb {
        color: var(--color-size-gb);
    }

    /*noinspection CssUnusedSymbol*/
    .size-tb {
        color: var(--color-size-tb);
    }
</style>
