<script lang="ts">
    import { onMount, onDestroy, tick } from 'svelte'
    import { getCurrentWindow } from '@tauri-apps/api/window'
    import SettingsSidebar from '$lib/settings/components/SettingsSidebar.svelte'
    import SettingsContent from '$lib/settings/components/SettingsContent.svelte'
    import { initializeSettings, forceSave as forceSettingsSave } from '$lib/settings'
    import { initializeShortcuts, flushPendingSave as flushShortcutsSave } from '$lib/shortcuts'
    import { initAccentColor, cleanupAccentColor } from '$lib/accent-color'
    import { getMatchingSections } from '$lib/settings/settings-search'
    import { loadLastSettingsSection, saveLastSettingsSection } from '$lib/app-status-store'
    import {
        syncSettingsState,
        notifySettingsWindowOpen,
        setupMcpEventListeners,
        cleanupMcpEventListeners,
    } from '$lib/settings/mcp-settings-bridge'
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
        // Sync state to MCP backend
        void syncSettingsState(sectionPath)
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

    // Handle setting value changed (for MCP sync)
    function handleSettingChanged() {
        void syncSettingsState(selectedSection)
    }

    // Handle keyboard events
    function handleKeydown(event: KeyboardEvent) {
        if (event.key === 'Escape') {
            event.preventDefault()
            void getCurrentWindow().close()
        }
        // Prevent Space from triggering Quick Look (bound to Space in main window menu)
        // Space should only activate focused buttons/controls, not bubble up
        if (
            event.key === ' ' &&
            !(event.target instanceof HTMLButtonElement || event.target instanceof HTMLInputElement)
        ) {
            event.preventDefault()
        }
    }

    // Prevent body from being focused - redirect focus to search input
    function handleFocusOut() {
        // Check if focus is going to body (or null)
        setTimeout(() => {
            if (document.activeElement === document.body || !document.activeElement) {
                const searchInput = document.querySelector('.search-input')
                if (searchInput instanceof HTMLElement) {
                    searchInput.focus()
                }
            }
        }, 0)
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
            // Initialize settings and shortcuts stores
            log.debug('Calling initializeSettings() and initializeShortcuts()')
            await Promise.all([initializeSettings(), initializeShortcuts()])
            log.info('Settings and shortcuts initialization complete')

            // Read system accent color from macOS and listen for changes
            await initAccentColor()

            // Load last viewed section
            const lastSection = await loadLastSettingsSection()
            selectedSection = lastSection
            log.debug('Restored last settings section: {section}', { section: lastSection.join(' > ') })

            initialized = true

            // Focus will be handled naturally by the browser's tab order
            await tick()

            // Set up MCP event listeners and sync initial state
            await setupMcpEventListeners(handleSectionSelect, handleSettingChanged)
            await notifySettingsWindowOpen(true)
            await syncSettingsState(selectedSection)

            log.debug('Settings page ready')
        } catch (error) {
            log.error('Failed to initialize settings: {error}', { error })
        }
    })

    // Flush any pending saves when the Settings window is closing
    onDestroy(() => {
        log.debug('Settings page destroying, flushing pending saves')
        // Fire and forget - we can't await in onDestroy
        void Promise.all([forceSettingsSave(), flushShortcutsSave()])
        // Clean up MCP event listeners and notify backend
        cleanupMcpEventListeners()
        cleanupAccentColor()
        void notifySettingsWindowOpen(false)
    })

    // Also handle beforeunload for when window is closed directly
    function handleBeforeUnload() {
        log.debug('Window unloading, flushing pending saves')
        // Use sync approach since beforeunload doesn't wait for promises
        void Promise.all([forceSettingsSave(), flushShortcutsSave()])
    }
</script>

<svelte:window on:keydown={handleKeydown} on:focusout={handleFocusOut} on:beforeunload={handleBeforeUnload} />

<!-- Prevent body from being a tab stop by keeping focus within the settings window -->
<div class="settings-window" tabindex="-1">
    {#if initialized}
        <div class="settings-layout">
            <SettingsSidebar
                {searchQuery}
                {matchingSections}
                {selectedSection}
                onSearch={handleSearch}
                onSectionSelect={handleSectionSelect}
            />
            <!-- tabindex="-1" prevents this from being a tab stop while still allowing programmatic scrolling -->
            <div class="settings-content-wrapper" bind:this={contentElement} tabindex="-1">
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
        font-family: var(--font-system) sans-serif;
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
        padding: var(--spacing-lg);
        outline: none;
    }

    .settings-loading {
        display: flex;
        align-items: center;
        justify-content: center;
        height: 100%;
        color: var(--color-text-tertiary);
    }
</style>
