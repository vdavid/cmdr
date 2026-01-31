<script lang="ts">
    import { onMount, tick } from 'svelte'
    import { getCurrentWindow } from '@tauri-apps/api/window'
    import SettingsSidebar from '$lib/settings/components/SettingsSidebar.svelte'
    import SettingsContent from '$lib/settings/components/SettingsContent.svelte'
    import { initializeSettings } from '$lib/settings'
    import { getMatchingSections } from '$lib/settings/settings-search'
    import { loadLastSettingsSection, saveLastSettingsSection } from '$lib/app-status-store'
    import { getAppLogger } from '$lib/logger'

    const log = getAppLogger('settings')

    let searchQuery = $state('')
    let matchingSections = $state<Set<string>>(new Set())
    let selectedSection = $state<string[]>(['General', 'Appearance'])
    let initialized = $state(false)
    let contentElement: HTMLElement | null = $state(null)

    // Log page script initialization
    log.debug('Settings page script loaded')

    // Handle search input
    function handleSearch(query: string) {
        log.debug('Search query changed: {query}', { query })
        searchQuery = query
        if (query.trim()) {
            matchingSections = getMatchingSections(query)
        } else {
            matchingSections = new Set()
        }
    }

    // Handle section selection from sidebar
    function handleSectionSelect(sectionPath: string[]) {
        log.debug('Section selected: {sectionPath}', { sectionPath: sectionPath.join(' > ') })
        selectedSection = sectionPath
        // Save last section to app status store
        void saveLastSettingsSection(sectionPath)
        // Scroll to the section in content area
        if (contentElement) {
            const sectionId = sectionPath
                .join('-')
                .toLowerCase()
                .replace(/[^a-z0-9-]/g, '-')
            const element = contentElement.querySelector(`[data-section-id="${sectionId}"]`)
            if (element instanceof HTMLElement) {
                // Scroll the content wrapper directly instead of using scrollIntoView
                // to avoid scrolling the entire window
                contentElement.scrollTo({
                    top: element.offsetTop - 16, // 16px padding from top
                    behavior: 'smooth',
                })
            }
        }
    }

    // Handle keyboard events
    function handleKeydown(event: KeyboardEvent) {
        if (event.key === 'Escape') {
            log.debug('Escape pressed, closing settings window')
            event.preventDefault()
            void getCurrentWindow().close()
        }
    }

    onMount(async () => {
        log.info('Settings page mounted, starting initialization')

        // Hide loading screen (from app.html) - must do this first!
        const loadingScreen = document.getElementById('loading-screen')
        if (loadingScreen) {
            log.debug('Hiding loading screen')
            loadingScreen.style.display = 'none'
        }

        try {
            // Initialize settings store
            log.debug('Calling initializeSettings()')
            await initializeSettings()
            log.info('Settings initialization complete')

            // Load last viewed section
            const lastSection = await loadLastSettingsSection()
            selectedSection = lastSection
            log.debug('Restored last settings section: {section}', { section: lastSection.join(' > ') })

            initialized = true

            // Focus the window
            await tick()
            document.body.focus()
            log.debug('Settings page ready and focused')
        } catch (error) {
            log.error('Failed to initialize settings: {error}', { error })
        }
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
                <SettingsContent {searchQuery} {selectedSection} onNavigate={handleSectionSelect} />
            </div>
        </div>
    {:else}
        <div class="settings-loading">Loading settings...</div>
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
