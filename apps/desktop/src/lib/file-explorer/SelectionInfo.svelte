<script lang="ts">
    import type { FileEntry } from './types'

    interface Props {
        entry: FileEntry | null
        /** Modified timestamp of the current directory (for ".." entry) */
        currentDirModifiedAt?: number
    }

    const { entry, currentDirModifiedAt }: Props = $props()

    // Size tier colors for digit triads (indexed: 0=bytes, 1=kB, 2=MB, 3=GB, 4=TB+)
    const sizeTierClasses = ['size-bytes', 'size-kb', 'size-mb', 'size-gb', 'size-tb']

    /** Formats a number into digit triads with separate styled spans */
    function formatSizeTriads(bytes: number): { value: string; tierClass: string }[] {
        const str = String(bytes)
        const triads: { value: string; tierClass: string }[] = []

        // Split into triads from right to left
        let remaining = str
        let tierIndex = 0
        while (remaining.length > 0) {
            const start = Math.max(0, remaining.length - 3)
            const triad = remaining.slice(start)
            remaining = remaining.slice(0, start)

            triads.unshift({
                value: triad,
                tierClass: sizeTierClasses[Math.min(tierIndex, sizeTierClasses.length - 1)],
            })
            tierIndex++
        }

        // Add thousand separators between triads (space)
        return triads.map((t, i) => ({
            ...t,
            value: i < triads.length - 1 ? t.value + '\u2009' : t.value, // thin space separator
        }))
    }

    /** Formats bytes as human-readable (for tooltip) */
    function formatHumanReadable(bytes: number): string {
        const units = ['bytes', 'KB', 'MB', 'GB', 'TB', 'PB']
        let value = bytes
        let unitIndex = 0
        while (value >= 1024 && unitIndex < units.length - 1) {
            value /= 1024
            unitIndex++
        }
        const valueStr = unitIndex === 0 ? String(value) : value.toFixed(2)
        return `${valueStr} ${units[unitIndex]}`
    }

    /** Formats timestamp as YYYY-MM-DD hh:mm:ss */
    function formatDate(timestamp: number | undefined): string {
        if (timestamp === undefined) return ''
        const date = new Date(timestamp * 1000)
        const pad = (n: number) => String(n).padStart(2, '0')
        const year = date.getFullYear()
        const month = pad(date.getMonth() + 1)
        const day = pad(date.getDate())
        const hours = pad(date.getHours())
        const mins = pad(date.getMinutes())
        const secs = pad(date.getSeconds())
        return `${String(year)}-${month}-${day} ${hours}:${mins}:${secs}`
    }

    /** Builds date tooltip content */
    function buildDateTooltip(e: FileEntry): string {
        const lines: string[] = []
        if (e.createdAt !== undefined) lines.push(`Created: ${formatDate(e.createdAt)}`)
        if (e.openedAt !== undefined) lines.push(`Last opened: ${formatDate(e.openedAt)}`)
        if (e.addedAt !== undefined) lines.push(`Last moved ("added"): ${formatDate(e.addedAt)}`)
        if (e.modifiedAt !== undefined) lines.push(`Last modified: ${formatDate(e.modifiedAt)}`)
        return lines.join('\n')
    }

    // Computed values
    const displayName = $derived(entry?.name ?? '')

    const isDirectory = $derived(entry?.isDirectory ?? false)

    const isBrokenSymlink = $derived(entry !== null && entry.isSymlink && entry.iconId === 'symlink-broken')

    const isPermissionDenied = $derived(
        entry !== null && !entry.isSymlink && entry.permissions === 0 && entry.size === undefined,
    )

    const sizeDisplay = $derived.by(() => {
        if (!entry || isBrokenSymlink || isPermissionDenied) return null
        if (isDirectory) return 'DIR'
        if (entry.size === undefined) return null
        return formatSizeTriads(entry.size)
    })

    const sizeTooltip = $derived(
        entry?.size !== undefined && !isDirectory ? formatHumanReadable(entry.size) : undefined,
    )

    const dateDisplay = $derived.by(() => {
        if (!entry) return ''
        if (isBrokenSymlink) return '(broken symlink)'
        if (isPermissionDenied) return '(permission denied)'
        // For ".." entry, use the current directory's modified time
        const timestamp = entry.name === '..' ? currentDirModifiedAt : entry.modifiedAt
        return formatDate(timestamp)
    })

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
            const testName = baseName.slice(0, mid) + ellipsis + extension
            measureSpan.textContent = testName

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
</script>

<div class="selection-info" bind:this={containerElement}>
    {#if entry}
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
