<script lang="ts">
    import type { FileEntry } from './types'
    import { getCachedIcon, prefetchIcons, iconCacheVersion } from '$lib/icon-cache'

    interface Props {
        files: FileEntry[]
        selectedIndex: number
        isFocused?: boolean
        onSelect: (index: number) => void
        onNavigate: (entry: FileEntry) => void
        onContextMenu?: (entry: FileEntry) => void
    }

    const { files, selectedIndex, isFocused = true, onSelect, onNavigate, onContextMenu }: Props = $props()

    // ==== Virtual scrolling constants ====
    // Row height in pixels - must match CSS (.file-entry height)
    // Current CSS: padding 2px top/bottom + ~16px line height = ~20px
    const ROW_HEIGHT = 20
    // Buffer items above/below viewport to reduce gaps during fast scrolling
    const BUFFER_SIZE = 20

    // ==== Virtual scrolling state ====
    let scrollContainer: HTMLDivElement | undefined = $state()
    let containerHeight = $state(0)
    let scrollTop = $state(0)

    // ==== Virtual scrolling derived calculations ====
    const startIndex = $derived(Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - BUFFER_SIZE))
    const visibleCount = $derived(Math.ceil(containerHeight / ROW_HEIGHT) + BUFFER_SIZE * 2)
    const endIndex = $derived(Math.min(startIndex + visibleCount, files.length))
    const visibleFiles = $derived(files.slice(startIndex, endIndex))
    const totalHeight = $derived(files.length * ROW_HEIGHT)
    const offsetY = $derived(startIndex * ROW_HEIGHT)

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

    function formatName(entry: FileEntry): string {
        return entry.name
    }

    function handleClick(actualIndex: number) {
        onSelect(actualIndex)
    }

    function handleDoubleClick(actualIndex: number) {
        onNavigate(files[actualIndex])
    }

    // Exported for parent to call when arrow keys change selection
    // With virtual scrolling, we calculate the target scroll position mathematically
    export function scrollToIndex(index: number) {
        if (!scrollContainer) return

        const targetTop = index * ROW_HEIGHT
        const targetBottom = targetTop + ROW_HEIGHT
        const viewportBottom = scrollTop + containerHeight

        if (targetTop < scrollTop) {
            // Item is above viewport - scroll up to show it
            scrollContainer.scrollTop = targetTop
        } else if (targetBottom > viewportBottom) {
            // Item is below viewport - scroll down to show it
            scrollContainer.scrollTop = targetBottom - containerHeight
        }
        // else: item already visible, no scroll needed
    }
</script>

<div
    class="file-list"
    class:is-focused={isFocused}
    bind:this={scrollContainer}
    bind:clientHeight={containerHeight}
    onscroll={handleScroll}
    tabindex="-1"
    role="listbox"
    aria-activedescendant={files[selectedIndex] ? `file-${String(selectedIndex)}` : undefined}
>
    <!-- Spacer div provides accurate scrollbar for full list size -->
    <div class="virtual-spacer" style="height: {totalHeight}px;">
        <!-- Visible window positioned with translateY -->
        <div class="virtual-window" style="transform: translateY({offsetY}px);">
            {#each visibleFiles as file, localIndex (file.path)}
                {@const actualIndex = startIndex + localIndex}
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
                    <span class="icon-wrapper">
                        {#if getIconUrl(file)}
                            <img class="icon" src={getIconUrl(file)} alt="" width="16" height="16" />
                        {:else}
                            <span class="icon-emoji">{getFallbackEmoji(file)}</span>
                        {/if}
                        {#if file.isSymlink}
                            <span class="symlink-badge">🔗</span>
                        {/if}
                    </span>
                    <span class="name">{formatName(file)}</span>
                </div>
            {/each}
        </div>
    </div>
</div>

<style>
    .file-list {
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
    }

    .file-entry.is-selected {
        background-color: rgba(204, 228, 247, 0.1); /* 10% of selection color for inactive pane */
    }

    .file-list.is-focused .file-entry.is-selected {
        background-color: var(--color-selection-bg);
    }

    .icon-wrapper {
        position: relative;
        width: 16px;
        height: 16px;
        flex-shrink: 0;
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

    .name {
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .is-directory .name {
        font-weight: 600;
    }

    /* Dark mode: 10% of dark selection color #0a50d0 */
    @media (prefers-color-scheme: dark) {
        .file-entry.is-selected {
            background-color: rgba(10, 80, 208, 0.1);
        }
    }
</style>
