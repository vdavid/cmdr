<script lang="ts">
    import Icon from '$lib/ui/Icon.svelte'
    import { buildSectionTree, type SettingsSection } from '$lib/settings'
    import { sectionHasMatches } from '$lib/settings/settings-search'
    import { tString } from '$lib/intl/messages.svelte'
    import { sectionTitle } from '$lib/settings/section-i18n'

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

    // Special sections have dedicated UI (not driven by the registry). Advanced is
    // NOT special: it's a normal registry section (every entry is `section: ['Advanced']`)
    // that auto-renders, so it comes from `sectionTree` via `TOP_LEVEL_ORDER`.
    const specialSections: Record<string, { name: string; path: string[] }> = {
        'Keyboard shortcuts': { name: 'Keyboard shortcuts', path: ['Keyboard shortcuts'] },
        License: { name: 'License', path: ['License'] },
    }

    // Explicit top-to-bottom sidebar order. Registry-driven sections are looked up by name in
    // `sectionTree`; special sections come from `specialSections`. Keep this in sync with the
    // E2E test in `settings.spec.ts` (§ "lists top-level sections in the expected order").
    const TOP_LEVEL_ORDER = [
        'Appearance',
        'Behavior',
        'Indexing',
        'AI',
        'File systems',
        'Viewer',
        'Keyboard shortcuts',
        'Updates & privacy',
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
        } else if (event.key === 'a' && (event.metaKey || event.ctrlKey)) {
            // Cmd/Ctrl+A doesn't reach the input by default because the
            // settings window's Edit menu doesn't bind a Select-All item to
            // it (the app uses a custom menu, not the macOS-standard Edit
            // menu). Select the input's text manually so the user gets the
            // expected behavior.
            event.preventDefault()
            searchInput?.select()
        }
    }
</script>

<aside class="settings-sidebar">
    <div class="search-container">
        <span class="search-icon" aria-hidden="true"><Icon name="search" size={16} aria-hidden="true" /></span>
        <input
            bind:this={searchInput}
            type="text"
            class="search-input"
            placeholder={tString('settings.sidebar.searchPlaceholder')}
            value={searchQuery}
            oninput={handleSearchInput}
            onkeydown={handleSearchKeydown}
            autocomplete="off"
            autocapitalize="off"
            spellcheck="false"
        />
        {#if searchQuery}
            <button class="search-clear" onclick={clearSearch} aria-label={tString('settings.sidebar.clearSearch')}>
                ×
            </button>
        {/if}
    </div>

    <div
        class="section-tree"
        tabindex="0"
        onkeydown={handleNavKeydown}
        role="listbox"
        aria-label={tString('settings.sidebar.sections')}
    >
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
                            {sectionTitle(entry.node.name)}
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
                                            {sectionTitle(subsection.name)}
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
                        {sectionTitle(entry.name)}
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
        display: flex;
        flex-direction: column;
        /* Subtle angled gradient: darker top-left → ~10% brighter bottom-right.
           In light mode both stops are near-white (sidebar lighter than
           `--color-bg-primary`); in dark mode both stops are darker. See
           `app.css` for the tokens. */
        background: linear-gradient(135deg, var(--color-bg-sidebar-from), var(--color-bg-sidebar-to));
        /* Sidebar floats as a "card" inside the window's 8 px padding (see
           `routes/settings/+page.svelte`). Matching radius + thin border +
           subtle right-leaning shadow define its edge against the vibrant
           frame around it. The shadow token is `none` in dark mode where
           the bg contrast alone separates the card. */
        border-radius: var(--radius-xl);
        border: 1px solid var(--color-sidebar-border);
        box-shadow: var(--shadow-sidebar);
        /* Top padding leaves the traffic-light row clear of the search
           field. The sidebar's top edge sits at the window's 8 px padding;
           the lights land at the sidebar's local y ≈ 22 px. */
        padding-top: calc(var(--spacing-xl) + var(--spacing-md));
    }

    .search-container {
        padding: var(--spacing-sm) var(--spacing-sm);
        position: relative;
    }

    .search-icon {
        position: absolute;
        /* Sits roughly centered in the input's left padded zone (input-x ≈ 20,
           where container-x = 8 input-left + 12 = 20). The icon glyph is
           16 px wide so its right edge lands at input-x ≈ 28, with the
           text starting at input-x ≈ 36 (= 32 + 4 from the input's
           padding-left). */
        left: calc(var(--spacing-sm) + var(--spacing-md));
        top: 50%;
        transform: translateY(-50%);
        display: flex;
        align-items: center;
        color: var(--color-text-tertiary);
        pointer-events: none;
        font-size: var(--font-size-sm);
    }

    .search-input {
        width: 100%;
        /* ~Double the previous padding for a chunkier, System-Settings-style
           pill. Vertical: xs → sm (4 → 8). Horizontal: 20 / 24 → 36 / 32. */
        padding: var(--spacing-sm) var(--spacing-2xl) var(--spacing-sm) calc(var(--spacing-2xl) + var(--spacing-xs));
        border: 1px solid transparent;
        /* Full pill — System Settings-style. */
        border-radius: var(--radius-full);
        background: var(--color-bg-secondary);
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
        right: calc(var(--spacing-sm) + var(--spacing-xs));
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
