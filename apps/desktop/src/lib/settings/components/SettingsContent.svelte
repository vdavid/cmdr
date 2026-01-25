<script lang="ts">
    import AppearanceSection from '$lib/settings/sections/AppearanceSection.svelte'
    import FileOperationsSection from '$lib/settings/sections/FileOperationsSection.svelte'
    import UpdatesSection from '$lib/settings/sections/UpdatesSection.svelte'
    import NetworkSection from '$lib/settings/sections/NetworkSection.svelte'
    import KeyboardShortcutsSection from '$lib/settings/sections/KeyboardShortcutsSection.svelte'
    import ThemesSection from '$lib/settings/sections/ThemesSection.svelte'
    import McpServerSection from '$lib/settings/sections/McpServerSection.svelte'
    import LoggingSection from '$lib/settings/sections/LoggingSection.svelte'
    import AdvancedSection from '$lib/settings/sections/AdvancedSection.svelte'

    interface Props {
        searchQuery: string
        selectedSection: string[]
    }

    const { searchQuery, selectedSection }: Props = $props()

    // Determine which sections to show based on selection and search
    function shouldShowSection(sectionPath: string[]): boolean {
        // If searching, show all sections that have matches (handled by components)
        if (searchQuery.trim()) {
            return true
        }
        // If not searching, show the selected section and related
        const selectedRoot = selectedSection[0]
        const sectionRoot = sectionPath[0]
        return selectedRoot === sectionRoot
    }
</script>

<div class="settings-content">
    <!-- General sections -->
    {#if shouldShowSection(['General', 'Appearance'])}
        <section data-section-id="general-appearance">
            <AppearanceSection {searchQuery} />
        </section>
    {/if}

    {#if shouldShowSection(['General', 'File operations'])}
        <section data-section-id="general-file-operations">
            <FileOperationsSection {searchQuery} />
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

    <!-- Special sections -->
    {#if shouldShowSection(['Keyboard shortcuts'])}
        <section data-section-id="keyboard-shortcuts">
            <KeyboardShortcutsSection {searchQuery} />
        </section>
    {/if}

    {#if shouldShowSection(['Themes'])}
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

    <!-- Advanced section -->
    {#if shouldShowSection(['Advanced'])}
        <section data-section-id="advanced">
            <AdvancedSection {searchQuery} />
        </section>
    {/if}
</div>

<style>
    .settings-content {
        max-width: 600px;
    }

    section {
        margin-bottom: var(--spacing-md);
    }

    section:last-child {
        margin-bottom: 0;
    }
</style>
