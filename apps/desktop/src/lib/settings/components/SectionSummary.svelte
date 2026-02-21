<script lang="ts">
    import { buildSectionTree, type SettingsSection } from '$lib/settings'

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

    // Get description for each subsection
    function getSubsectionDescription(subsection: SettingsSection): string {
        // Return a brief description based on the subsection name
        const descriptions: Record<string, string> = {
            Appearance: 'Customize fonts, density, and visual display options.',
            'File operations': 'Configure copy, move, delete, and progress display settings.',
            'Drive indexing': 'Background indexing for directory sizes. Manage index storage.',
            Updates: 'Manage automatic update checks and notifications.',
            'SMB/Network shares': 'Configure network timeouts and connection settings.',
            'MCP server': 'Configure the Model Context Protocol server for AI integrations.',
            Logging: 'Debug logging and diagnostic settings.',
        }
        return descriptions[subsection.name] ?? `Configure ${subsection.name.toLowerCase()} settings.`
    }
</script>

<div class="section-summary">
    <h2 class="summary-title">{sectionName}</h2>

    {#if section && section.subsections.length > 0}
        <div class="subsection-grid">
            {#each section.subsections as subsection (subsection.name)}
                <button
                    class="subsection-card"
                    onclick={() => {
                        onNavigate(subsection.path)
                    }}
                >
                    <h3 class="subsection-name">{subsection.name}</h3>
                    <p class="subsection-description">{getSubsectionDescription(subsection)}</p>
                </button>
            {/each}
        </div>
    {:else}
        <p class="no-subsections">This section has no subsections.</p>
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
