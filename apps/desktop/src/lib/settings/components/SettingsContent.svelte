<script lang="ts">
    import AppearanceSection from '$lib/settings/sections/AppearanceSection.svelte'
    import AppearanceZoomSection from '$lib/settings/sections/AppearanceZoomSection.svelte'
    import AppearanceSizesSection from '$lib/settings/sections/AppearanceSizesSection.svelte'
    import ListingSection from '$lib/settings/sections/ListingSection.svelte'
    import NavigationAndFileOpsSection from '$lib/settings/sections/NavigationAndFileOpsSection.svelte'
    import ArchivesSection from '$lib/settings/sections/ArchivesSection.svelte'
    import FileSystemWatchingSection from '$lib/settings/sections/FileSystemWatchingSection.svelte'
    import SearchSection from '$lib/settings/sections/SearchSection.svelte'
    import AiSection from '$lib/settings/sections/AiSection.svelte'
    import AskCmdrSection from '$lib/settings/sections/AskCmdrSection.svelte'
    import ImageSearchSection from '$lib/settings/sections/ImageSearchSection.svelte'
    import NetworkSection from '$lib/settings/sections/NetworkSection.svelte'
    import MtpSection from '$lib/settings/sections/MtpSection.svelte'
    import GitSection from '$lib/settings/sections/GitSection.svelte'
    import ViewerSection from '$lib/settings/sections/ViewerSection.svelte'
    import OperationLogSection from '$lib/settings/sections/OperationLogSection.svelte'
    import KeyboardShortcutsSection from '$lib/settings/sections/KeyboardShortcutsSection.svelte'
    import McpServerSection from '$lib/settings/sections/McpServerSection.svelte'
    import LoggingSection from '$lib/settings/sections/LoggingSection.svelte'
    import UpdatesSection from '$lib/settings/sections/UpdatesSection.svelte'
    import LicenseSection from '$lib/settings/sections/LicenseSection.svelte'
    import AdvancedSection from '$lib/settings/sections/AdvancedSection.svelte'
    import SectionSummary from './SectionSummary.svelte'
    import { getMatchingSettingIdsInSection } from '$lib/settings/settings-search'
    import { searchAllCommands } from '$lib/commands/fuzzy-search'

    interface Props {
        searchQuery: string
        selectedSection: string[]
        onNavigate?: (path: string[]) => void
    }

    const { searchQuery, selectedSection, onNavigate }: Props = $props()

    // True when a top-level section that itself has navigable subsections is selected — those
    // render a summary card grid instead of their settings directly.
    const isTopLevelSection = $derived(!searchQuery.trim() && selectedSection.length === 1)
    const sectionsWithSubsections = ['Appearance', 'Behavior', 'AI', 'File systems', 'Developer']
    const showSummary = $derived(isTopLevelSection && sectionsWithSubsections.includes(selectedSection[0]))

    function handleNavigate(path: string[]) {
        onNavigate?.(path)
    }

    function sectionHasMatchingSettings(sectionPath: string[]): boolean {
        if (!searchQuery.trim()) return true
        const matchingIds = getMatchingSettingIdsInSection(searchQuery, sectionPath)
        return matchingIds.size > 0
    }

    function keyboardShortcutsHasMatches(): boolean {
        if (!searchQuery.trim()) return false
        // Full-registry search: the shortcuts section renders every command, so the gate
        // must match the same set (palette-only search hid "Open command palette" here).
        const results = searchAllCommands(searchQuery)
        return results.length > 0
    }

    function shouldShowSection(sectionPath: string[]): boolean {
        if (searchQuery.trim()) {
            if (sectionPath.length === 1 && sectionPath[0] === 'Keyboard shortcuts') {
                return keyboardShortcutsHasMatches()
            }
            return sectionHasMatchingSettings(sectionPath)
        }
        if (showSummary) return false
        return (
            sectionPath.length === selectedSection.length && sectionPath.every((part, i) => part === selectedSection[i])
        )
    }

    // For top-level sections (no subsections), allow selecting the top level OR matching via search.
    function shouldShowTopLevel(path: string[]): boolean {
        return shouldShowSection(path) || (isTopLevelSection && selectedSection[0] === path[0])
    }
</script>

<div>
    {#if showSummary}
        <SectionSummary sectionName={selectedSection[0]} onNavigate={handleNavigate} />
    {:else}
        <!-- Appearance -->
        {#if shouldShowSection(['Appearance', 'Colors and formats'])}
            <section data-section-id="appearance-colors-and-formats">
                <AppearanceSection {searchQuery} />
            </section>
        {/if}
        {#if shouldShowSection(['Appearance', 'Zoom and density'])}
            <section data-section-id="appearance-zoom-and-density">
                <AppearanceZoomSection {searchQuery} />
            </section>
        {/if}
        {#if shouldShowSection(['Appearance', 'File and folder sizes'])}
            <section data-section-id="appearance-file-and-folder-sizes">
                <AppearanceSizesSection {searchQuery} />
            </section>
        {/if}
        {#if shouldShowSection(['Appearance', 'Listing'])}
            <section data-section-id="appearance-listing">
                <ListingSection {searchQuery} />
            </section>
        {/if}

        <!-- Behavior -->
        {#if shouldShowSection(['Behavior', 'Navigation & file ops'])}
            <section data-section-id="behavior-navigation-and-file-ops">
                <NavigationAndFileOpsSection {searchQuery} />
            </section>
        {/if}
        {#if shouldShowSection(['Behavior', 'Archives'])}
            <section data-section-id="behavior-archives">
                <ArchivesSection {searchQuery} />
            </section>
        {/if}
        {#if shouldShowSection(['Behavior', 'File system watching'])}
            <section data-section-id="behavior-file-system-watching">
                <FileSystemWatchingSection {searchQuery} />
            </section>
        {/if}
        {#if shouldShowSection(['Behavior', 'Search'])}
            <section data-section-id="behavior-search">
                <SearchSection {searchQuery} />
            </section>
        {/if}

        <!-- AI -->
        {#if shouldShowSection(['AI', 'Provider'])}
            <section data-section-id="ai-provider">
                <AiSection {searchQuery} />
            </section>
        {/if}
        {#if shouldShowSection(['AI', 'Ask Cmdr'])}
            <section data-section-id="ai-ask-cmdr">
                <AskCmdrSection {searchQuery} />
            </section>
        {/if}
        {#if shouldShowSection(['AI', 'Image search'])}
            <section data-section-id="ai-image-search">
                <ImageSearchSection {searchQuery} />
            </section>
        {/if}

        <!-- File systems -->
        {#if shouldShowSection(['File systems', 'SMB/Network shares'])}
            <section data-section-id="file-systems-smb-network-shares">
                <NetworkSection {searchQuery} />
            </section>
        {/if}
        {#if shouldShowSection(['File systems', 'MTP (Android/Kindle/cameras)'])}
            <section data-section-id="file-systems-mtp-android-kindle-cameras">
                <MtpSection {searchQuery} />
            </section>
        {/if}
        {#if shouldShowSection(['File systems', 'Git'])}
            <section data-section-id="file-systems-git">
                <GitSection {searchQuery} />
            </section>
        {/if}

        <!-- Viewer (top-level, no subsections) -->
        {#if shouldShowTopLevel(['Viewer'])}
            <section data-section-id="viewer">
                <ViewerSection {searchQuery} />
            </section>
        {/if}

        <!-- Operation log (top-level, no subsections) -->
        {#if shouldShowTopLevel(['Operation log'])}
            <section data-section-id="operation-log">
                <OperationLogSection {searchQuery} />
            </section>
        {/if}

        <!-- Keyboard shortcuts (special) -->
        {#if shouldShowTopLevel(['Keyboard shortcuts'])}
            <section data-section-id="keyboard-shortcuts">
                <KeyboardShortcutsSection {searchQuery} />
            </section>
        {/if}

        <!-- Developer -->
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

        <!-- Updates & privacy (top-level, no subsections) -->
        {#if shouldShowTopLevel(['Updates & privacy'])}
            <section data-section-id="updates">
                <UpdatesSection {searchQuery} />
            </section>
        {/if}

        <!-- License (special) -->
        {#if shouldShowTopLevel(['License'])}
            <section data-section-id="license">
                <LicenseSection />
            </section>
        {/if}

        <!-- Advanced (top-level registry section, no subsections; auto-renders) -->
        {#if shouldShowTopLevel(['Advanced'])}
            <section data-section-id="advanced">
                <AdvancedSection {searchQuery} />
            </section>
        {/if}
    {/if}
</div>

<style>
    /* No content max-width: settings rows fill the wrapper, which is window
     * width minus the fixed sidebar (220 px) and content padding (32 px).
     * Window min/max in `lib/settings/settings-window.ts` make the available
     * content area scale proportionally with text size, so the right edge of
     * the content always snaps to the right edge of the window. */

    section {
        margin-bottom: var(--spacing-lg);
    }

    section:last-child {
        margin-bottom: 0;
    }
</style>
