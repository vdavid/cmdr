<script lang="ts">
    import AppearanceSection from '$lib/settings/sections/AppearanceSection.svelte'
    import FileOperationsSection from '$lib/settings/sections/FileOperationsSection.svelte'
    import ListingSection from '$lib/settings/sections/ListingSection.svelte'
    import DriveIndexingSection from '$lib/settings/sections/DriveIndexingSection.svelte'
    import UpdatesSection from '$lib/settings/sections/UpdatesSection.svelte'
    import NetworkSection from '$lib/settings/sections/NetworkSection.svelte'
    import KeyboardShortcutsSection from '$lib/settings/sections/KeyboardShortcutsSection.svelte'
    import ThemesSection from '$lib/settings/sections/ThemesSection.svelte'
    import McpServerSection from '$lib/settings/sections/McpServerSection.svelte'
    import LoggingSection from '$lib/settings/sections/LoggingSection.svelte'
    import AdvancedSection from '$lib/settings/sections/AdvancedSection.svelte'
    import SectionSummary from './SectionSummary.svelte'
    import { getMatchingSettingIdsInSection } from '$lib/settings/settings-search'
    import { searchCommands } from '$lib/commands/fuzzy-search'

    interface Props {
        searchQuery: string
        selectedSection: string[]
        onNavigate?: (path: string[]) => void
    }

    const { searchQuery, selectedSection, onNavigate }: Props = $props()

    // Check if we're showing a top-level section (summary view) or a specific subsection
    const isTopLevelSection = $derived(!searchQuery.trim() && selectedSection.length === 1)

    // Sections that have subsections and should show summary pages
    const sectionsWithSubsections = ['General', 'Network', 'Developer']

    // Check if current selection should show summary
    const showSummary = $derived(isTopLevelSection && sectionsWithSubsections.includes(selectedSection[0]))

    // Handle navigation from summary cards
    function handleNavigate(path: string[]) {
        onNavigate?.(path)
    }

    // Check if a section has any matching settings during search
    function sectionHasMatchingSettings(sectionPath: string[]): boolean {
        if (!searchQuery.trim()) return true
        const matchingIds = getMatchingSettingIdsInSection(searchQuery, sectionPath)
        return matchingIds.size > 0
    }

    // Check if keyboard shortcuts section has matching commands
    function keyboardShortcutsHasMatches(): boolean {
        if (!searchQuery.trim()) return false
        const results = searchCommands(searchQuery)
        return results.length > 0
    }

    // Determine which sections to show based on selection and search
    function shouldShowSection(sectionPath: string[]): boolean {
        // If searching, only show sections that have matching settings
        if (searchQuery.trim()) {
            // Special handling for keyboard shortcuts which has commands, not settings
            if (sectionPath.length === 1 && sectionPath[0] === 'Keyboard shortcuts') {
                return keyboardShortcutsHasMatches()
            }
            return sectionHasMatchingSettings(sectionPath)
        }
        // If showing summary, don't show any section content
        if (showSummary) {
            return false
        }
        // Show only the exact selected section (not all under the same root)
        return (
            sectionPath.length === selectedSection.length && sectionPath.every((part, i) => part === selectedSection[i])
        )
    }
</script>

<div class="settings-content">
    <!-- Summary pages for top-level sections -->
    {#if showSummary}
        <SectionSummary sectionName={selectedSection[0]} onNavigate={handleNavigate} />
    {:else}
        <!-- General sections -->
        {#if shouldShowSection(['General', 'Appearance'])}
            <section data-section-id="general-appearance">
                <AppearanceSection {searchQuery} />
            </section>
        {/if}

        {#if shouldShowSection(['General', 'Listing'])}
            <section data-section-id="general-listing">
                <ListingSection {searchQuery} />
            </section>
        {/if}

        {#if shouldShowSection(['General', 'File operations'])}
            <section data-section-id="general-file-operations">
                <FileOperationsSection {searchQuery} />
            </section>
        {/if}

        {#if shouldShowSection(['General', 'Drive indexing'])}
            <section data-section-id="general-drive-indexing">
                <DriveIndexingSection {searchQuery} />
            </section>
        {/if}

        {#if shouldShowSection(['General', 'Updates'])}
            <section data-section-id="general-updates">
                <UpdatesSection {searchQuery} />
            </section>
        {/if}

        <!-- Network sections -->
        {#if shouldShowSection(['Network', 'SMB/Network shares'])}
            <section data-section-id="network-smb-network-shares">
                <NetworkSection {searchQuery} />
            </section>
        {/if}

        <!-- Special sections (no subsections, show directly) -->
        {#if shouldShowSection( ['Keyboard shortcuts'], ) || (isTopLevelSection && selectedSection[0] === 'Keyboard shortcuts')}
            <section data-section-id="keyboard-shortcuts">
                <KeyboardShortcutsSection {searchQuery} />
            </section>
        {/if}

        {#if shouldShowSection(['Themes']) || (isTopLevelSection && selectedSection[0] === 'Themes')}
            <section data-section-id="themes">
                <ThemesSection {searchQuery} />
            </section>
        {/if}

        <!-- Developer sections -->
        {#if shouldShowSection(['Developer', 'MCP server'])}
            <section data-section-id="developer-mcp-server">
                <McpServerSection {searchQuery} />
            </section>
        {/if}

        {#if shouldShowSection(['Developer', 'Logging'])}
            <section data-section-id="developer-logging">
                <LoggingSection {searchQuery} />
            </section>
        {/if}

        <!-- Advanced section (no subsections, show directly) -->
        {#if shouldShowSection(['Advanced']) || (isTopLevelSection && selectedSection[0] === 'Advanced')}
            <section data-section-id="advanced">
                <AdvancedSection {searchQuery} />
            </section>
        {/if}
    {/if}
</div>

<style>
    .settings-content {
        max-width: 600px;
    }

    section {
        margin-bottom: var(--spacing-lg);
    }

    section:last-child {
        margin-bottom: 0;
    }
</style>
