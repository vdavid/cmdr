<script lang="ts">
    import { buildSectionTree, type SettingsSection } from '$lib/settings'
    import { sectionHasMatches } from '$lib/settings/settings-search'

    interface Props {
        searchQuery: string
        matchingSections: Set<string>
        selectedSection: string[]
        onSearch: (query: string) => void
        onSectionSelect: (sectionPath: string[]) => void
    }

    const { searchQuery, matchingSections, selectedSection, onSearch, onSectionSelect }: Props = $props()

    let searchInput: HTMLInputElement | null = $state(null)
    const sectionTree = buildSectionTree()

    // Special sections that have dedicated UI (not from registry)
    // Note: Themes is in the registry, so we don't add it here
    const specialSections = [
        { name: 'Keyboard shortcuts', path: ['Keyboard shortcuts'] },
        { name: 'Advanced', path: ['Advanced'] },
    ]

    function handleSearchInput(event: Event) {
        const target = event.target as HTMLInputElement
        onSearch(target.value)
    }

    function clearSearch() {
        onSearch('')
        searchInput?.focus()
    }

    function isSelected(sectionPath: string[]): boolean {
        if (sectionPath.length !== selectedSection.length) return false
        return sectionPath.every((part, i) => part === selectedSection[i])
    }

    function shouldShowSection(section: SettingsSection): boolean {
        if (!searchQuery.trim()) return true
        return sectionHasMatches(section.path, matchingSections)
    }

    function shouldShowSpecialSection(path: string[]): boolean {
        if (!searchQuery.trim()) return true
        // For special sections, show if any Advanced setting matches (for Advanced section)
        // or always show them since they have their own search
        if (path[0] === 'Advanced') {
            return sectionHasMatches(path, matchingSections)
        }
        // Keyboard shortcuts and Themes are always visible (they have their own search)
        return true
    }
</script>

<aside class="settings-sidebar">
    <div class="search-container">
        <input
            bind:this={searchInput}
            type="text"
            class="search-input"
            placeholder="Search settings..."
            value={searchQuery}
            oninput={handleSearchInput}
        />
        {#if searchQuery}
            <button class="search-clear" onclick={clearSearch} aria-label="Clear search"> × </button>
        {/if}
    </div>

    <nav class="section-tree">
        {#each sectionTree as section (section.name)}
            {#if shouldShowSection(section)}
                <div class="section-group">
                    <button
                        class="section-item"
                        class:selected={isSelected(section.path)}
                        onclick={() => {
                            onSectionSelect(section.path)
                        }}
                    >
                        {section.name}
                    </button>
                    {#if section.subsections.length > 0}
                        <div class="subsections">
                            {#each section.subsections as subsection (subsection.name)}
                                {#if shouldShowSection(subsection)}
                                    <button
                                        class="section-item subsection"
                                        class:selected={isSelected(subsection.path)}
                                        onclick={() => {
                                            onSectionSelect(subsection.path)
                                        }}
                                    >
                                        {subsection.name}
                                    </button>
                                {/if}
                            {/each}
                        </div>
                    {/if}
                </div>
            {/if}
        {/each}

        <!-- Special sections -->
        {#each specialSections as special (special.name)}
            {#if shouldShowSpecialSection(special.path)}
                <div class="section-group">
                    <button
                        class="section-item"
                        class:selected={isSelected(special.path)}
                        onclick={() => {
                            onSectionSelect(special.path)
                        }}
                    >
                        {special.name}
                    </button>
                </div>
            {/if}
        {/each}
    </nav>
</aside>

<style>
    .settings-sidebar {
        width: 220px;
        min-width: 220px;
        border-right: 1px solid var(--color-border);
        display: flex;
        flex-direction: column;
        background: var(--color-bg-secondary);
    }

    .search-container {
        padding: var(--spacing-sm);
        position: relative;
    }

    .search-input {
        width: 100%;
        padding: var(--spacing-xs) var(--spacing-sm);
        padding-right: 28px;
        border: 1px solid var(--color-border);
        border-radius: 4px;
        background: var(--color-bg-primary);
        color: var(--color-text-primary);
        font-size: var(--font-size-sm);
        outline: none;
    }

    .search-input:focus {
        border-color: var(--color-accent);
        box-shadow: 0 0 0 2px rgba(77, 163, 255, 0.2);
    }

    .search-input::placeholder {
        color: var(--color-text-muted);
    }

    .search-clear {
        position: absolute;
        right: 12px;
        top: 50%;
        transform: translateY(-50%);
        background: none;
        border: none;
        color: var(--color-text-muted);
        cursor: pointer;
        font-size: 16px;
        padding: 2px 6px;
        line-height: 1;
    }

    .search-clear:hover {
        color: var(--color-text-primary);
    }

    .section-tree {
        flex: 1;
        overflow-y: auto;
        padding: var(--spacing-xs) 0;
    }

    .section-group {
        margin-bottom: var(--spacing-xxs);
    }

    .subsections {
        display: flex;
        flex-direction: column;
    }

    .section-item {
        display: block;
        width: 100%;
        padding: var(--spacing-xs) var(--spacing-sm);
        background: none;
        border: none;
        text-align: left;
        color: var(--color-text-primary);
        font-size: var(--font-size-sm);
        cursor: pointer;
        border-radius: 0;
        transition: background-color 0.1s;
    }

    .section-item:hover {
        background: var(--color-bg-hover);
    }

    .section-item.selected {
        background: var(--color-accent);
        color: white;
    }

    .section-item.subsection {
        padding-left: calc(var(--spacing-sm) + var(--spacing-md));
        color: var(--color-text-secondary);
    }

    .section-item.subsection.selected {
        color: white;
    }
</style>
