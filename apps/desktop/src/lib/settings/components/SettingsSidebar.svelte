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

    // Special sections have dedicated UI (not driven by the registry).
    const specialSections: Record<string, { name: string; path: string[] }> = {
        'Keyboard shortcuts': { name: 'Keyboard shortcuts', path: ['Keyboard shortcuts'] },
        License: { name: 'License', path: ['License'] },
        Advanced: { name: 'Advanced', path: ['Advanced'] },
    }

    // Explicit top-to-bottom sidebar order. Registry-driven sections are looked up by name in
    // `sectionTree`; special sections come from `specialSections`. Keep this in sync with the
    // E2E test in `settings.spec.ts` (§ "lists top-level sections in the expected order").
    const TOP_LEVEL_ORDER = [
        'Appearance',
        'Behavior',
        'AI',
        'File systems',
        'Viewer',
        'Keyboard shortcuts',
        'Developer',
        'Updates',
        'License',
        'Advanced',
    ] as const

    type TopLevelName = (typeof TOP_LEVEL_ORDER)[number]

    type SidebarEntry =
        | { kind: 'tree'; node: SettingsSection }
        | { kind: 'special'; name: string; path: string[] }

    // `Partial<...>` because only a few entries in `TOP_LEVEL_ORDER` are special sections.
    // Without it the index returns the value type unconditionally, and the `if (special)`
    // check below trips `no-unnecessary-condition`.
    const specialByName: Partial<Record<TopLevelName, { name: string; path: string[] }>> = specialSections

    const orderedEntries = $derived.by((): SidebarEntry[] => {
        const treeByName = new Map(sectionTree.map((s) => [s.name, s]))
        const entries: SidebarEntry[] = []
        for (const name of TOP_LEVEL_ORDER) {
            const node = treeByName.get(name)
            if (node) {
                entries.push({ kind: 'tree', node })
                continue
            }
            const special = specialByName[name]
            if (special) {
                entries.push({ kind: 'special', name: special.name, path: [...special.path] })
            }
        }
        return entries
    })

    // Flat list of all visible (top-level + subsection) entries, for keyboard nav.
    const allSections = $derived.by(() => {
        const sections: { name: string; path: string[]; isSubsection: boolean }[] = []
        for (const entry of orderedEntries) {
            if (entry.kind === 'tree') {
                const section = entry.node
                if (!shouldShowSection(section)) continue
                sections.push({ name: section.name, path: section.path, isSubsection: false })
                for (const subsection of section.subsections) {
                    if (!shouldShowSection(subsection)) continue
                    sections.push({ name: subsection.name, path: subsection.path, isSubsection: true })
                }
            } else {
                if (!shouldShowSpecialSection(entry.path)) continue
                sections.push({ name: entry.name, path: entry.path, isSubsection: false })
            }
        }
        return sections
    })

    function findSelectedIndex(): number {
        return allSections.findIndex(
            (s) => s.path.length === selectedSection.length && s.path.every((part, i) => part === selectedSection[i]),
        )
    }

    function handleSearchInput(event: Event) {
        const target = event.target as HTMLInputElement
        const value = target.value

        if (debounceTimer) {
            clearTimeout(debounceTimer)
        }

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
        if (path[0] === 'Advanced') return sectionHasMatches(path, matchingSections)
        if (path[0] === 'Keyboard shortcuts') return matchingSections.has('Keyboard shortcuts')
        if (path[0] === 'License') return matchingSections.has('License')
        return false
    }

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

    function handleNavKeydown(event: KeyboardEvent) {
        if (event.key === 'ArrowDown') {
            event.preventDefault()
            navigateSections('down')
        } else if (event.key === 'ArrowUp') {
            event.preventDefault()
            navigateSections('up')
        }
    }

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
        {#each orderedEntries as entry (entry.kind === 'tree' ? entry.node.name : entry.name)}
            {#if entry.kind === 'tree'}
                {#if shouldShowSection(entry.node)}
                    <div class="section-group">
                        <button
                            class="section-item"
                            class:selected={isSelected(entry.node.path)}
                            onclick={() => {
                                onSectionSelect(entry.node.path)
                            }}
                            role="option"
                            aria-selected={isSelected(entry.node.path)}
                            tabindex="-1"
                        >
                            {entry.node.name}
                        </button>
                        {#if entry.node.subsections.length > 0}
                            <div class="subsections">
                                {#each entry.node.subsections as subsection (subsection.name)}
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
            {:else if entry.kind === 'special' && shouldShowSpecialSection(entry.path)}
                <div class="section-group">
                    <button
                        class="section-item"
                        class:selected={isSelected(entry.path)}
                        onclick={() => {
                            onSectionSelect(entry.path)
                        }}
                        role="option"
                        aria-selected={isSelected(entry.path)}
                        tabindex="-1"
                    >
                        {entry.name}
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
        padding-right: var(--spacing-2xl);
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
        padding: var(--spacing-xxs) var(--spacing-xs);
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
