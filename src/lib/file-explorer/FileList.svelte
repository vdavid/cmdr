<script lang="ts">
    import type { FileEntry } from './types'

    interface Props {
        files: FileEntry[]
        selectedIndex: number
        onSelect: (index: number) => void
        onNavigate: (entry: FileEntry) => void
    }

    const { files, selectedIndex, onSelect, onNavigate }: Props = $props()

    let listElement: HTMLUListElement | undefined = $state()

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
            {#if file.isDirectory}
                <span class="icon">📁</span>
            {:else}
                <span class="icon">📄</span>
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
        background-color: var(--color-selection-bg);
    }

    .icon {
        font-size: var(--font-size-sm);
        flex-shrink: 0;
    }

    .name {
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .is-directory .name {
        font-weight: 600;
    }
</style>
