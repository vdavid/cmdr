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
    let debounceTimer: ReturnType<typeof setTimeout> | null = null
    const sectionTree = buildSectionTree()

    // Special sections that have dedicated UI (not from registry)
    // Note: Themes is in the registry, so we don't add it here
    const specialSections = [
        { name: 'Keyboard shortcuts', path: ['Keyboard shortcuts'] },
        { name: 'Advanced', path: ['Advanced'] },
    ]

    // Build flat list of all navigable sections for keyboard navigation
    const allSections = $derived.by(() => {
        const sections: { name: string; path: string[]; isSubsection: boolean }[] = []

        // Add sections from tree (including subsections)
        for (const section of sectionTree) {
            if (!shouldShowSection(section)) continue
            sections.push({ name: section.name, path: section.path, isSubsection: false })
            for (const subsection of section.subsections) {
                if (!shouldShowSection(subsection)) continue
                sections.push({ name: subsection.name, path: subsection.path, isSubsection: true })
            }
        }

        // Add special sections
        for (const special of specialSections) {
            if (!shouldShowSpecialSection(special.path)) continue
            sections.push({ name: special.name, path: special.path, isSubsection: false })
        }

        return sections
    })

    // Find the index of the currently selected section
    function findSelectedIndex(): number {
        return allSections.findIndex(
            (s) => s.path.length === selectedSection.length && s.path.every((part, i) => part === selectedSection[i]),
        )
    }

    function handleSearchInput(event: Event) {
        const target = event.target as HTMLInputElement
        const value = target.value

        // Clear any pending debounce timer
        if (debounceTimer) {
            clearTimeout(debounceTimer)
        }

        // Debounce search by 200ms
        debounceTimer = setTimeout(() => {
            onSearch(value)
            debounceTimer = null
        }, 200)
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
        if (path[0] === 'Advanced') {
            return sectionHasMatches(path, matchingSections)
        }
        // Keyboard shortcuts: show if any command name matches the search query
        if (path[0] === 'Keyboard shortcuts') {
            return matchingSections.has('Keyboard shortcuts')
        }
        return false
    }

    // Shared navigation logic for Up/Down arrows
    function navigateSections(direction: 'up' | 'down') {
        const totalSections = allSections.length
        if (totalSections === 0) return

        const currentIndex = findSelectedIndex()

        if (direction === 'down') {
            const nextIndex = currentIndex < 0 ? 0 : Math.min(totalSections - 1, currentIndex + 1)
            onSectionSelect(allSections[nextIndex].path)
        } else {
            const prevIndex = currentIndex < 0 ? 0 : Math.max(0, currentIndex - 1)
            onSectionSelect(allSections[prevIndex].path)
        }
    }

    // Keyboard navigation directly changes selection (no separate focus state)
    function handleNavKeydown(event: KeyboardEvent) {
        if (event.key === 'ArrowDown') {
            event.preventDefault()
            navigateSections('down')
        } else if (event.key === 'ArrowUp') {
            event.preventDefault()
            navigateSections('up')
        }
    }

    // Handle keyboard in search box - Up/Down move section selector
    function handleSearchKeydown(event: KeyboardEvent) {
        if (event.key === 'ArrowDown') {
            event.preventDefault()
            navigateSections('down')
        } else if (event.key === 'ArrowUp') {
            event.preventDefault()
            navigateSections('up')
        }
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
            onkeydown={handleSearchKeydown}
            autocomplete="off"
            autocapitalize="off"
            spellcheck="false"
        />
        {#if searchQuery}
            <button class="search-clear" onclick={clearSearch} aria-label="Clear search"> × </button>
        {/if}
    </div>

    <div class="section-tree" tabindex="0" onkeydown={handleNavKeydown} role="listbox" aria-label="Settings sections">
        {#each sectionTree as section (section.name)}
            {#if shouldShowSection(section)}
                <div class="section-group">
                    <button
                        class="section-item"
                        class:selected={isSelected(section.path)}
                        onclick={() => {
                            onSectionSelect(section.path)
                        }}
                        role="option"
                        aria-selected={isSelected(section.path)}
                        tabindex="-1"
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
                                        role="option"
                                        aria-selected={isSelected(subsection.path)}
                                        tabindex="-1"
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
                        role="option"
                        aria-selected={isSelected(special.path)}
                        tabindex="-1"
                    >
                        {special.name}
                    </button>
                </div>
            {/if}
        {/each}
    </div>
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
        border-radius: var(--radius-sm);
        background: var(--color-bg-primary);
        color: var(--color-text-primary);
        font-size: var(--font-size-sm);
        outline: none;
    }

    .search-input:focus {
        border-color: var(--color-accent);
        box-shadow: var(--shadow-focus);
    }

    .search-input::placeholder {
        color: var(--color-text-tertiary);
    }

    .search-clear {
        position: absolute;
        right: 12px;
        top: 50%;
        transform: translateY(-50%);
        background: none;
        border: none;
        color: var(--color-text-tertiary);
        cursor: default;
        font-size: var(--font-size-lg);
        padding: 2px var(--spacing-xs);
        line-height: 1;
    }

    .section-tree {
        flex: 1;
        overflow-y: auto;
        padding: var(--spacing-xs) 0;
        outline: none;
        border-radius: var(--radius-sm);
        margin: 0 var(--spacing-xs);
    }

    .section-tree:focus {
        outline: 2px solid var(--color-accent);
        outline-offset: -2px;
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
        cursor: default;
        border-radius: 0;
        transition: background-color var(--transition-fast);
    }

    .section-item.selected {
        background: var(--color-accent);
        color: var(--color-accent-fg);
    }

    .section-item.selected:hover {
        background: var(--color-accent-hover);
    }

    .section-item.subsection {
        padding-left: calc(var(--spacing-sm) + var(--spacing-lg));
        color: var(--color-text-secondary);
    }

    .section-item.subsection.selected {
        color: var(--color-accent-fg);
    }
</style>
