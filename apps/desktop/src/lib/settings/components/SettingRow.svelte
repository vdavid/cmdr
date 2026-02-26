<script lang="ts">
    import type { Snippet } from 'svelte'
    import { isModified, resetSetting, onSpecificSettingChange, type SettingId } from '$lib/settings'
    import { getMatchIndicesForLabel, highlightMatches } from '$lib/settings/settings-search'
    import { tooltip } from '$lib/tooltip/tooltip'
    import { onMount } from 'svelte'

    interface Props {
        id: SettingId
        label: string
        description: string
        disabled?: boolean
        disabledReason?: string
        requiresRestart?: boolean
        searchQuery?: string
        children: Snippet
        descriptionContent?: Snippet
    }

    const {
        id,
        label,
        description,
        disabled = false,
        disabledReason,
        requiresRestart = false,
        searchQuery = '',
        children,
        descriptionContent,
    }: Props = $props()

    // Get highlighted label segments based on search query
    const labelSegments = $derived.by(() => {
        if (!searchQuery.trim()) {
            return [{ text: label, matched: false }]
        }
        const matchIndices = getMatchIndicesForLabel(searchQuery, id)
        return highlightMatches(label, matchIndices)
    })

    // Track modified state reactively by subscribing to changes
    let modified = $state(isModified(id))

    // Subscribe to setting changes to update modified state
    onMount(() => {
        return onSpecificSettingChange(id, () => {
            modified = isModified(id)
        })
    })

    function handleReset() {
        resetSetting(id)
    }
</script>

<div class="setting-row" class:disabled>
    <div class="setting-header">
        <div class="setting-label-wrapper">
            {#if modified}
                <span class="modified-indicator" use:tooltip={'Modified from default'}>●</span>
            {/if}
            <label class="setting-label" for={id}
                >{#each labelSegments as segment, i (i)}{#if segment.matched}<mark class="search-highlight"
                            >{segment.text}</mark
                        >{:else}{segment.text}{/if}{/each}</label
            >
            {#if disabled && disabledReason}
                <span class="disabled-badge">{disabledReason}</span>
            {/if}
            {#if requiresRestart}
                <span class="restart-badge">Restart required</span>
            {/if}
        </div>
        <div class="setting-control">
            {@render children()}
        </div>
    </div>
    {#if descriptionContent}
        <p class="setting-description">{@render descriptionContent()}</p>
    {:else}
        <p class="setting-description">{description}</p>
    {/if}
    <button class="reset-link" class:hidden={!modified} onclick={handleReset} aria-hidden={!modified}>
        Reset to default
    </button>
</div>

<style>
    .setting-row {
        padding: var(--spacing-sm) 0;
        border-bottom: 1px solid var(--color-border-subtle);
    }

    .setting-row:last-child {
        border-bottom: none;
    }

    .setting-row.disabled {
        opacity: 0.6;
    }

    .setting-header {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: var(--spacing-md);
    }

    .setting-label-wrapper {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
    }

    .modified-indicator {
        color: var(--color-accent);
        font-size: var(--font-size-xs);
    }

    .setting-label {
        font-weight: 500;
        color: var(--color-text-primary);
    }

    .disabled-badge,
    .restart-badge {
        font-size: var(--font-size-xs);
        padding: 2px var(--spacing-xs);
        border-radius: var(--radius-sm);
        font-weight: 500;
    }

    .disabled-badge {
        background: var(--color-bg-tertiary);
        color: var(--color-text-tertiary);
    }

    .restart-badge {
        background: var(--color-accent);
        color: var(--color-accent-fg);
        margin-left: var(--spacing-xs);
    }

    .setting-control {
        flex-shrink: 0;
    }

    .setting-description {
        margin: var(--spacing-xs) 0 0;
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
        line-height: 1.4;
    }

    .reset-link {
        margin-top: var(--spacing-xs);
        padding: 0;
        background: none;
        border: none;
        color: var(--color-accent);
        font-size: var(--font-size-sm);
        cursor: default;
        text-decoration: underline;
    }

    .reset-link.hidden {
        visibility: hidden;
    }

    .search-highlight {
        background-color: var(--color-highlight);
        color: inherit;
        padding: 0 2px;
        border-radius: 2px;
    }
</style>
