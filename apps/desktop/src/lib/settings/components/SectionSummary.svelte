<script lang="ts">
    import { buildSectionTree, type SettingsSection } from '$lib/settings'
    import { tString } from '$lib/intl/messages.svelte'
    import { sectionTitle } from '$lib/settings/section-i18n'
    import type { MessageKey } from '$lib/intl/keys.gen'

    interface Props {
        sectionName: string
        onNavigate: (path: string[]) => void
    }

    const { sectionName, onNavigate }: Props = $props()

    const sectionTree = buildSectionTree()

    // Find the section and its subsections
    const section = $derived.by(() => {
        return sectionTree.find((s) => s.name === sectionName)
    })

    // Subsection name (English structural identity) → its summary-blurb catalog
    // key. A name without a blurb falls back to the generic "Configure …" line.
    const SUMMARY_KEY: Partial<Record<string, MessageKey>> = {
        'Colors and formats': 'settings.summary.colorsAndFormats',
        'Zoom and density': 'settings.summary.zoomAndDensity',
        'File and folder sizes': 'settings.summary.fileAndFolderSizes',
        Listing: 'settings.summary.listing',
        'Navigation & file ops': 'settings.summary.navigationAndFileOps',
        Archives: 'settings.summary.archives',
        'File system watching': 'settings.summary.fileSystemWatching',
        Search: 'settings.summary.search',
        Provider: 'settings.summary.aiProvider',
        'Ask Cmdr': 'settings.summary.askCmdr',
        'SMB/Network shares': 'settings.summary.smbNetworkShares',
        'MTP (Android/Kindle/cameras)': 'settings.summary.mtp',
        Git: 'settings.summary.git',
        'MCP server': 'settings.summary.mcpServer',
        Logging: 'settings.summary.logging',
    }

    function getSubsectionDescription(subsection: SettingsSection): string {
        const key = SUMMARY_KEY[subsection.name]
        return key ? tString(key) : `Configure ${subsection.name.toLowerCase()} settings.`
    }
</script>

<div class="section-summary">
    <h2 class="summary-title">{sectionTitle(sectionName)}</h2>

    {#if section && section.subsections.length > 0}
        <div class="subsection-grid">
            {#each section.subsections as subsection (subsection.name)}
                <button
                    class="subsection-card"
                    onclick={() => {
                        onNavigate(subsection.path)
                    }}
                >
                    <h3 class="subsection-name">{sectionTitle(subsection.name)}</h3>
                    <p class="subsection-description">{getSubsectionDescription(subsection)}</p>
                </button>
            {/each}
        </div>
    {:else}
        <p class="no-subsections">{tString('settings.summary.noSubsections')}</p>
    {/if}
</div>

<style>
    .section-summary {
        padding: var(--spacing-lg);
    }

    .summary-title {
        font-size: var(--font-size-xl);
        font-weight: 600;
        color: var(--color-text-primary);
        margin: 0 0 var(--spacing-lg);
    }

    .subsection-grid {
        display: grid;
        grid-template-columns: repeat(auto-fill, minmax(250px, 1fr));
        gap: var(--spacing-lg);
    }

    .subsection-card {
        display: flex;
        flex-direction: column;
        align-items: flex-start;
        padding: var(--spacing-lg);
        background: var(--color-bg-secondary);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-lg);
        cursor: default;
        text-align: left;
        transition:
            background-color var(--transition-base),
            border-color var(--transition-base);
    }

    .subsection-card:focus-visible {
        outline: 2px solid var(--color-accent);
        outline-offset: 2px;
    }

    .subsection-name {
        font-size: var(--font-size-md);
        font-weight: 600;
        color: var(--color-text-primary);
        margin: 0 0 var(--spacing-xs);
    }

    .subsection-description {
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
        margin: 0;
        line-height: 1.4;
    }

    .no-subsections {
        color: var(--color-text-tertiary);
        font-size: var(--font-size-sm);
    }
</style>
