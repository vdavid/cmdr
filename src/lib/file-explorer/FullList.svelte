<script lang="ts">
    import type { FileEntry, SyncStatus } from './types'
    import { getCachedIcon, prefetchIcons, iconCacheVersion } from '$lib/icon-cache'
    import { calculateVirtualWindow, getScrollToPosition } from './virtual-scroll'

    interface Props {
        files: FileEntry[]
        selectedIndex: number
        isFocused?: boolean
        syncStatusMap?: Record<string, SyncStatus>
        onSelect: (index: number) => void
        onNavigate: (entry: FileEntry) => void
        onContextMenu?: (entry: FileEntry) => void
    }

    const {
        files,
        selectedIndex,
        isFocused = true,
        syncStatusMap = {},
        onSelect,
        onNavigate,
        onContextMenu,
    }: Props = $props()

    // Sync status icon paths - returns undefined if no icon should be shown
    function getSyncIconPath(status: SyncStatus | undefined): string | undefined {
        if (!status) return undefined
        const iconMap: Record<SyncStatus, string | undefined> = {
            synced: '/icons/sync-synced.svg',
            online_only: '/icons/sync-online-only.svg',
            uploading: '/icons/sync-uploading.svg',
            downloading: '/icons/sync-downloading.svg',
            unknown: undefined,
        }
        return iconMap[status]
    }

    // ==== Virtual scrolling constants ====
    // Row height in pixels - must match CSS (.file-entry height)
    // Current CSS: padding 2px top/bottom + ~16px line height = ~20px
    const ROW_HEIGHT = 20
    // Buffer items above/below viewport to reduce gaps during fast scrolling
    const BUFFER_SIZE = 20

    // Size tier colors for digit triads (indexed: 0=bytes, 1=kB, 2=MB, 3=GB, 4=TB+)
    const sizeTierClasses = ['size-bytes', 'size-kb', 'size-mb', 'size-gb', 'size-tb']

    // ==== Virtual scrolling state ====
    let scrollContainer: HTMLDivElement | undefined = $state()
    let containerHeight = $state(0)
    let scrollTop = $state(0)

    // ==== Virtual scrolling derived calculations ====
    const virtualWindow = $derived(
        calculateVirtualWindow({
            direction: 'vertical',
            itemSize: ROW_HEIGHT,
            bufferSize: BUFFER_SIZE,
            containerSize: containerHeight,
            scrollOffset: scrollTop,
            totalItems: files.length,
        }),
    )
    const visibleFiles = $derived(files.slice(virtualWindow.startIndex, virtualWindow.endIndex))

    function handleScroll(e: Event) {
        const target = e.target as HTMLDivElement
        scrollTop = target.scrollTop
    }

    // Track which icons we've prefetched to avoid redundant calls (module-level, non-reactive)
    // Using a plain Set outside the reactive system since we only add to it
    // eslint-disable-next-line svelte/prefer-svelte-reactivity
    const prefetchedSet: Set<string> = new Set()

    // Prefetch icons for visible files when they change
    $effect(() => {
        const newIconIds = visibleFiles.map((f) => f.iconId).filter((id) => id && !prefetchedSet.has(id))
        if (newIconIds.length > 0) {
            // Add to set first to avoid re-fetching during async
            newIconIds.forEach((id) => prefetchedSet.add(id))
            void prefetchIcons(newIconIds)
        }
    })

    // Subscribe to cache version - this makes getIconUrl reactive
    // When iconCacheVersion updates, this derived value triggers re-render

    const _cacheVersion = $derived($iconCacheVersion)

    function getIconUrl(file: FileEntry): string | undefined {
        // Read _cacheVersion to establish reactive dependency (it's used implicitly)
        void _cacheVersion

        // For directories, try path-based icon first (for custom folder icons)
        if (file.isDirectory) {
            const pathIcon = getCachedIcon(`path:${file.path}`)
            if (pathIcon) return pathIcon
        }

        // Fall back to generic icon ID
        return getCachedIcon(file.iconId)
    }

    function getFallbackEmoji(file: FileEntry): string {
        if (file.isSymlink) return '🔗'
        if (file.isDirectory) return '📁'
        return '📄'
    }

    /** Formats a number into digit triads with CSS classes for coloring */
    function formatSizeTriads(bytes: number): { value: string; tierClass: string }[] {
        const str = String(bytes)
        const triads: { value: string; tierClass: string }[] = []

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

        // Add thin space separator between triads
        return triads.map((t, i) => ({
            ...t,
            value: i < triads.length - 1 ? t.value + '\u2009' : t.value,
        }))
    }

    /** Formats bytes as human-readable (for tooltip) */
    function formatHumanReadable(bytes: number): string {
        const units = ['bytes', 'KB', 'MB', 'GB', 'TB']
        let value = bytes
        let unitIndex = 0
        while (value >= 1024 && unitIndex < units.length - 1) {
            value /= 1024
            unitIndex++
        }
        const valueStr = unitIndex === 0 ? String(value) : value.toFixed(2)
        return `${valueStr} ${units[unitIndex]}`
    }

    /** Formats timestamp as YYYY-MM-DD hh:mm */
    function formatDate(timestamp: number | undefined): string {
        if (timestamp === undefined) return ''
        const date = new Date(timestamp * 1000)
        const pad = (n: number) => String(n).padStart(2, '0')
        const year = date.getFullYear()
        const month = pad(date.getMonth() + 1)
        const day = pad(date.getDate())
        const hours = pad(date.getHours())
        const mins = pad(date.getMinutes())
        return `${String(year)}-${month}-${day} ${hours}:${mins}`
    }

    function handleClick(actualIndex: number) {
        onSelect(actualIndex)
    }

    // Keep selected item in view when container height changes (window resize)
    $effect(() => {
        // Track containerHeight to trigger on resize
        if (containerHeight > 0 && scrollContainer) {
            const newScrollTop = getScrollToPosition(selectedIndex, ROW_HEIGHT, scrollTop, containerHeight)
            if (newScrollTop !== undefined) {
                scrollContainer.scrollTop = newScrollTop
            }
        }
    })

    function handleDoubleClick(actualIndex: number) {
        onNavigate(files[actualIndex])
    }

    // Exported for parent to call when arrow keys change selection
    // noinspection JSUnusedGlobalSymbols -- it's actually used
    export function scrollToIndex(index: number) {
        if (!scrollContainer) return

        const newScrollTop = getScrollToPosition(index, ROW_HEIGHT, scrollTop, containerHeight)
        if (newScrollTop !== undefined) {
            scrollContainer.scrollTop = newScrollTop
        }
    }

    // Returns paths of currently visible files (for sync status polling)
    export function getVisiblePaths(): string[] {
        return visibleFiles.map((f) => f.path)
    }
</script>

<div
    class="full-list"
    class:is-focused={isFocused}
    bind:this={scrollContainer}
    bind:clientHeight={containerHeight}
    onscroll={handleScroll}
    tabindex="-1"
    role="listbox"
    aria-activedescendant={files[selectedIndex] ? `file-${String(selectedIndex)}` : undefined}
>
    <!-- Spacer div provides accurate scrollbar for full list size -->
    <div class="virtual-spacer" style="height: {virtualWindow.totalSize}px;">
        <!-- Visible window positioned with translateY -->
        <div class="virtual-window" style="transform: translateY({virtualWindow.offset}px);">
            {#each visibleFiles as file, localIndex (file.path)}
                {@const actualIndex = virtualWindow.startIndex + localIndex}
                {@const syncIcon = getSyncIconPath(syncStatusMap[file.path])}
                <!-- svelte-ignore a11y_click_events_have_key_events a11y_interactive_supports_focus -->
                <div
                    id={`file-${String(actualIndex)}`}
                    class="file-entry"
                    class:is-directory={file.isDirectory}
                    class:is-selected={actualIndex === selectedIndex}
                    onclick={() => {
                        handleClick(actualIndex)
                    }}
                    ondblclick={() => {
                        handleDoubleClick(actualIndex)
                    }}
                    oncontextmenu={(e) => {
                        e.preventDefault()
                        onSelect(actualIndex)
                        onContextMenu?.(files[actualIndex])
                    }}
                    role="option"
                    aria-selected={actualIndex === selectedIndex}
                >
                    <span class="col-icon">
                        {#if getIconUrl(file)}
                            <img class="icon" src={getIconUrl(file)} alt="" width="16" height="16" />
                        {:else}
                            <span class="icon-emoji">{getFallbackEmoji(file)}</span>
                        {/if}
                        {#if file.isSymlink}
                            <span class="symlink-badge">🔗</span>
                        {/if}
                    </span>
                    <span class="col-name">
                        <span class="name-text">{file.name}</span>
                        {#if syncIcon}
                            <img
                                class="sync-icon"
                                src={syncIcon}
                                alt={syncStatusMap[file.path] ?? ''}
                                width="12"
                                height="12"
                            />
                        {/if}
                    </span>
                    <span class="col-size" title={file.size !== undefined ? formatHumanReadable(file.size) : ''}>
                        {#if file.isDirectory}
                            <span class="size-dir">DIR</span>
                        {:else if file.size !== undefined && file.name !== '..'}
                            {#each formatSizeTriads(file.size) as triad, i (i)}
                                <span class={triad.tierClass}>{triad.value}</span>
                            {/each}
                        {/if}
                    </span>
                    <span class="col-date">{formatDate(file.modifiedAt)}</span>
                </div>
            {/each}
        </div>
    </div>
</div>

<style>
    .full-list {
        margin: 0;
        padding: 0;
        overflow-y: auto;
        font-family: var(--font-system) sans-serif;
        font-size: var(--font-size-sm);
        flex: 1;
        outline: none;
    }

    /* Virtual scrolling container - sets total height for accurate scrollbar */
    .virtual-spacer {
        position: relative;
        width: 100%;
    }

    /* Visible window - positioned with translateY for smooth scrolling */
    .virtual-window {
        will-change: transform;
    }

    .file-entry {
        padding: var(--spacing-xxs) var(--spacing-sm);
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
        height: 20px;
        box-sizing: border-box;
    }

    .file-entry.is-selected {
        background-color: rgba(204, 228, 247, 0.1);
    }

    .full-list.is-focused .file-entry.is-selected {
        background-color: var(--color-selection-bg);
    }

    /* Column layout */
    .col-icon {
        position: relative;
        width: 16px;
        height: 16px;
        flex-shrink: 0;
    }

    .col-name {
        flex: 1;
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
        overflow: hidden;
        min-width: 0;
    }

    .name-text {
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        flex: 1;
        min-width: 0;
    }

    .sync-icon {
        flex-shrink: 0;
        opacity: 0.9;
    }

    .is-directory .col-name .name-text {
        font-weight: 600;
    }

    .col-size {
        flex-shrink: 0;
        width: 90px;
        text-align: right;
        font-variant-numeric: tabular-nums;
    }

    .col-date {
        flex-shrink: 0;
        width: 120px;
        font-variant-numeric: tabular-nums;
        opacity: 0.8;
    }

    .icon {
        width: 16px;
        height: 16px;
        object-fit: contain;
    }

    .icon-emoji {
        font-size: var(--font-size-sm);
        width: 16px;
        text-align: center;
        display: block;
    }

    .symlink-badge {
        position: absolute;
        bottom: -2px;
        right: -2px;
        font-size: 8px;
        line-height: 1;
    }

    /* Size tier colors - bytes are default text color - these are used dynamically */
    /*noinspection CssUnusedSymbol*/
    .size-bytes {
        color: var(--color-text-tertiary);
    }
    /*noinspection CssUnusedSymbol*/
    .size-kb {
        color: var(--color-text-secondary);
    }
    /*noinspection CssUnusedSymbol*/
    .size-mb {
        color: var(--color-text-primary);
    }
    /*noinspection CssUnusedSymbol*/
    .size-gb {
        color: var(--color-warning);
    }
    /*noinspection CssUnusedSymbol*/
    .size-tb {
        color: var(--color-error);
    }

    .size-dir {
        color: var(--color-text-tertiary);
    }

    /* Dark mode: 10% of dark selection color #0a50d0 */
    @media (prefers-color-scheme: dark) {
        .file-entry.is-selected {
            background-color: rgba(10, 80, 208, 0.1);
        }
    }
</style>
