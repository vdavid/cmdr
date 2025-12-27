<script lang="ts">
    import type { FileEntry } from './types'
    import { getCachedIcon, prefetchIcons } from '$lib/icon-cache'

    interface Props {
        files: FileEntry[]
        selectedIndex: number
        isFocused?: boolean
        onSelect: (index: number) => void
        onNavigate: (entry: FileEntry) => void
    }

    const { files, selectedIndex, isFocused = true, onSelect, onNavigate }: Props = $props()

    let listElement: HTMLUListElement | undefined = $state()

    // Track which icons we've prefetched to avoid redundant calls (module-level, non-reactive)
    // Using a plain Set outside the reactive system since we only add to it
    // eslint-disable-next-line svelte/prefer-svelte-reactivity
    const prefetchedSet: Set<string> = new Set()

    // Prefetch icons when files change
    $effect(() => {
        const newIconIds = files.map((f) => f.iconId).filter((id) => id && !prefetchedSet.has(id))
        if (newIconIds.length > 0) {
            // Add to set first to avoid re-fetching during async
            newIconIds.forEach((id) => prefetchedSet.add(id))
            void prefetchIcons(newIconIds)
        }
    })

    function getIconUrl(file: FileEntry): string | undefined {
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

    function handleClick(index: number) {
        onSelect(index)
    }

    function handleDoubleClick(index: number) {
        onNavigate(files[index])
    }

    // Exported for parent to call when arrow keys change selection
    export function scrollToIndex(index: number) {
        if (!listElement) return
        const items = listElement.querySelectorAll('.file-entry')
        const item = items[index] as HTMLElement | undefined
        if (item) {
            item.scrollIntoView({ block: 'nearest' })
        }
    }
</script>

<ul
    class="file-list"
    class:is-focused={isFocused}
    bind:this={listElement}
    tabindex="-1"
    role="listbox"
    aria-activedescendant={files[selectedIndex] ? `file-${String(selectedIndex)}` : undefined}
>
    {#each files as file, index (file.path)}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <li
            id={`file-${String(index)}`}
            class="file-entry"
            class:is-directory={file.isDirectory}
            class:is-selected={index === selectedIndex}
            onclick={() => {
                handleClick(index)
            }}
            ondblclick={() => {
                handleDoubleClick(index)
            }}
            role="option"
            aria-selected={index === selectedIndex}
        >
            {#if getIconUrl(file)}
                <img class="icon" src={getIconUrl(file)} alt="" width="16" height="16" />
            {:else}
                <span class="icon-emoji">{getFallbackEmoji(file)}</span>
            {/if}
            <span class="name">{formatName(file)}</span>
        </li>
    {/each}
</ul>

<style>
    .file-list {
        list-style: none;
        margin: 0;
        padding: 0;
        overflow-y: auto;
        font-family: var(--font-system) sans-serif;
        font-size: var(--font-size-sm);
        flex: 1;
        outline: none;
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

    .icon {
        width: 16px;
        height: 16px;
        flex-shrink: 0;
        object-fit: contain;
    }

    .icon-emoji {
        font-size: var(--font-size-sm);
        flex-shrink: 0;
        width: 16px;
        text-align: center;
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
