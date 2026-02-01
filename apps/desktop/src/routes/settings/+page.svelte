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
        // Prevent Space from triggering Quick Look (bound to Space in main window menu)
        // Space should only activate focused buttons/controls, not bubble up
        if (
            event.key === ' ' &&
            !(event.target instanceof HTMLButtonElement || event.target instanceof HTMLInputElement)
        ) {
            event.preventDefault()
        }
        // Debug: On Tab, log the active element after a small delay
        if (event.key === 'Tab') {
            setTimeout(() => {
                debugActiveElement()
            }, 50)
        }
    }

    // Debug: Log focus changes to find mysterious tab stop
    function handleFocusIn(event: FocusEvent) {
        const target = event.target as HTMLElement
        const tagName = target.tagName
        const className = target.className
        const id = target.id
        const text = target.textContent.slice(0, 30)
        const tabIndex = target.tabIndex
        const parent = target.parentElement
        const parentTag = parent ? parent.tagName : ''
        const parentClass = parent ? parent.className : ''
        log.debug(
            'Focus: {tagName} class="{className}" id="{id}" tabIndex={tabIndex} parent={parentTag}.{parentClass} text="{text}"',
            {
                tagName,
                className,
                id,
                tabIndex,
                parentTag,
                parentClass,
                text,
            },
        )
    }

    // Also try to catch focus on document.activeElement periodically
    function debugActiveElement() {
        const el = document.activeElement as HTMLElement | null
        if (el) {
            log.debug('activeElement: {tagName} class="{className}" id="{id}" tabIndex={tabIndex}', {
                tagName: el.tagName,
                className: el.className,
                id: el.id,
                tabIndex: el.tabIndex,
            })
        } else {
            log.debug('activeElement: null')
        }
    }

    // Prevent body from being focused - redirect focus to search input
    function handleFocusOut() {
        // Check if focus is going to body (or null)
        setTimeout(() => {
            if (document.activeElement === document.body || !document.activeElement) {
                log.debug('Focus went to body, redirecting to search')
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
            // Initialize settings store
            log.debug('Calling initializeSettings()')
            await initializeSettings()
            log.info('Settings initialization complete')

            // Load last viewed section
            const lastSection = await loadLastSettingsSection()
            selectedSection = lastSection
            log.debug('Restored last settings section: {section}', { section: lastSection.join(' > ') })

            initialized = true

            // Focus will be handled naturally by the browser's tab order
            await tick()
            log.debug('Settings page ready')
        } catch (error) {
            log.error('Failed to initialize settings: {error}', { error })
        }
    })
</script>

<svelte:window on:keydown={handleKeydown} on:focusin={handleFocusIn} on:focusout={handleFocusOut} />

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
        outline: none;
    }

    .settings-loading {
        display: flex;
        align-items: center;
        justify-content: center;
        height: 100%;
        color: var(--color-text-muted);
    }
</style>
