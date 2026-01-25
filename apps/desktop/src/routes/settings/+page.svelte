<script lang="ts">
    import { onMount, onDestroy, tick } from 'svelte'
    import { getCurrentWindow } from '@tauri-apps/api/window'
    import SettingsSidebar from '$lib/settings/components/SettingsSidebar.svelte'
    import SettingsContent from '$lib/settings/components/SettingsContent.svelte'
    import { initializeSettings } from '$lib/settings'
    import { searchSettings, getMatchingSections } from '$lib/settings/settings-search'

    let searchQuery = $state('')
    let matchingSections = $state<Set<string>>(new Set())
    let selectedSection = $state<string[]>(['General', 'Appearance'])
    let initialized = $state(false)
    let contentElement: HTMLElement | null = $state(null)

    // Handle search input
    function handleSearch(query: string) {
        searchQuery = query
        if (query.trim()) {
            matchingSections = getMatchingSections(query)
        } else {
            matchingSections = new Set()
        }
    }

    // Handle section selection from sidebar
    function handleSectionSelect(sectionPath: string[]) {
        selectedSection = sectionPath
        // Scroll to the section in content area
        if (contentElement) {
            const sectionId = sectionPath.join('-').toLowerCase().replace(/[^a-z0-9-]/g, '-')
            const element = contentElement.querySelector(`[data-section-id="${sectionId}"]`)
            if (element) {
                element.scrollIntoView({ behavior: 'smooth', block: 'start' })
            }
        }
    }

    // Handle keyboard events
    function handleKeydown(event: KeyboardEvent) {
        if (event.key === 'Escape') {
            event.preventDefault()
            getCurrentWindow().close()
        }
    }

    onMount(async () => {
        // Initialize settings store
        await initializeSettings()
        initialized = true

        // Focus the window
        await tick()
        document.body.focus()
    })
</script>

<svelte:window on:keydown={handleKeydown} />

<div class="settings-window">
    {#if initialized}
        <div class="settings-layout">
            <SettingsSidebar
                {searchQuery}
                {matchingSections}
                {selectedSection}
                onSearch={handleSearch}
                onSectionSelect={handleSectionSelect}
            />
            <div class="settings-content-wrapper" bind:this={contentElement}>
                <SettingsContent
                    {searchQuery}
                    {selectedSection}
                />
            </div>
        </div>
    {:else}
        <div class="settings-loading">
            Loading settings...
        </div>
    {/if}
</div>

<style>
    .settings-window {
        width: 100%;
        height: 100vh;
        background: var(--color-bg-primary);
        color: var(--color-text-primary);
        font-family: var(--font-system);
        font-size: var(--font-size-sm);
        overflow: hidden;
        display: flex;
        flex-direction: column;
    }

    .settings-layout {
        display: flex;
        flex: 1;
        overflow: hidden;
    }

    .settings-content-wrapper {
        flex: 1;
        overflow-y: auto;
        padding: var(--spacing-md);
    }

    .settings-loading {
        display: flex;
        align-items: center;
        justify-content: center;
        height: 100%;
        color: var(--color-text-muted);
    }
</style>
