<script lang="ts">
    import { onMount, onDestroy, tick } from 'svelte'
    import { getCurrentWindow, LogicalSize } from '@tauri-apps/api/window'
    import { listen, type UnlistenFn } from '@tauri-apps/api/event'
    import SettingsSidebar from '$lib/settings/components/SettingsSidebar.svelte'
    import SettingsContent from '$lib/settings/components/SettingsContent.svelte'
    import { initializeSettings, forceSave as forceSettingsSave } from '$lib/settings'
    import { initializeShortcuts, flushPendingSave as flushShortcutsSave } from '$lib/shortcuts'
    import { initAccentColor, cleanupAccentColor } from '$lib/accent-color'
    import { initTextSize, cleanupTextSize, getEffectiveScale } from '$lib/text-size.svelte'
    import { SETTINGS_BASE_MIN_HEIGHT, settingsMaxWidth, settingsMinWidth } from '$lib/settings/settings-window'
    import { getMatchingSections } from '$lib/settings/settings-search'
    import { loadLastSettingsSection, saveLastSettingsSection } from '$lib/app-status-store'
    import { getAppLogger } from '$lib/logging/logger'

    const log = getAppLogger('settings')

    let searchQuery = $state('')
    let matchingSections = $state<Set<string>>(new Set())
    let selectedSection = $state<string[]>(['Appearance', 'Colors and formats'])
    let initialized = $state(false)
    let contentElement: HTMLElement | null = $state(null)
    let unlistenFocusSelf: UnlistenFn | undefined
    let unlistenNavigate: UnlistenFn | undefined

    function safeParseSectionParam(raw: string): string[] | null {
        try {
            const parsed = JSON.parse(raw) as unknown
            if (Array.isArray(parsed) && parsed.every((s) => typeof s === 'string')) {
                return parsed
            }
        } catch {
            // ignore: treat as no deep-link
        }
        return null
    }

    // Log page script initialization
    log.debug('Settings page script loaded')

    /**
     * Settings-window dimensions track the effective text scale: at 100% the
     * base values match the historical layout; at other scales the min/max
     * grow proportionally so all rows stay visible. Tauri has no "no max
     * height" knob. We set a very large value (50_000 logical px) which is
     * effectively unbounded for practical use.
     *
     * Standard NSWindow clamping behavior: when the new constraints leave the
     * current frame out of bounds, macOS clamps it to fit. Otherwise the
     * frame stays where the user put it. The `appearance.textSize` slider
     * itself debounces re-measurement, so the window doesn't thrash.
     *
     * Reading `getEffectiveScale()` inside `$effect` makes this re-run on
     * every scale change (system Accessibility settle or user slider move).
     */
    $effect(() => {
        const scale = getEffectiveScale()
        const win = getCurrentWindow()
        const minSize = new LogicalSize(settingsMinWidth(scale), SETTINGS_BASE_MIN_HEIGHT * scale)
        const maxSize = new LogicalSize(settingsMaxWidth(scale), 50_000)
        // Awaited rather than fire-and-forget so a missing capability surfaces
        // as a warn log instead of silently swallowing the rejection. Tauri
        // rejects without these perms in `capabilities/settings.json`:
        // `core:window:allow-set-min-size`, `core:window:allow-set-max-size`.
        void (async () => {
            try {
                await win.setMinSize(minSize)
                await win.setMaxSize(maxSize)
            } catch (e) {
                log.warn('Settings window setMinSize/setMaxSize failed: {error}', { error: String(e) })
            }
        })()
    })

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
            event.preventDefault()
            // Defer the close() by two animation frames so the keydown event
            // loop iteration can settle (including any in-flight IPC ack to
            // the Tauri runtime) before webkit2gtk begins destroying this
            // webview. Without this, the synchronous close() runs inside the
            // same GTK main-loop tick that handled the keydown, and the
            // destruction can stall queued IPC calls from other webviews —
            // the root cause of the Linux E2E flake on this binding. Mirrors
            // the pattern in `routes/viewer/+page.svelte`'s `closeWindow()`.
            // The +16 ms is invisible to the user.
            const win = getCurrentWindow()
            requestAnimationFrame(() => {
                requestAnimationFrame(() => {
                    void win.close()
                })
            })
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
        log.debug('Settings page mounted, starting initialization')

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
            log.debug('Settings and shortcuts initialization complete')

            // Read system accent color from macOS and listen for changes
            await initAccentColor()

            // Apply compounded text size (system Accessibility × user setting)
            await initTextSize()

            // Load last viewed section, but a `?section=...` URL param wins so callers (like
            // the volume picker's "Network (disabled)" entry) can deep-link. The param is a
            // JSON-encoded string array so section names with `/` (like "SMB/Network shares")
            // round-trip safely.
            const urlSection = new URLSearchParams(window.location.search).get('section')
            const parsed = urlSection ? safeParseSectionParam(urlSection) : null
            if (parsed) {
                selectedSection = parsed
                log.debug('Opened settings to URL section: {section}', { section: parsed.join(' > ') })
            } else {
                const lastSection = await loadLastSettingsSection()
                selectedSection = lastSection
                log.debug('Restored last settings section: {section}', { section: lastSection.join(' > ') })
            }

            initialized = true

            await tick()

            // Focus the search input on open so users can start typing immediately.
            const searchInput = document.querySelector('.search-input')
            if (searchInput instanceof HTMLElement) {
                searchInput.focus()
            }

            // Listen for focus-self events (from ⌘, when window is already open).
            // Self-focusing is needed because cross-window setFocus() doesn't reliably
            // bring a window to front on macOS.
            unlistenFocusSelf = await listen('focus-self', () => {
                // setTimeout(0) defers past the originating keydown handler;
                // without it, macOS restores focus to the main window.
                setTimeout(() => {
                    void getCurrentWindow().setFocus()
                    const input = document.querySelector('.search-input')
                    if (input instanceof HTMLElement) input.focus()
                }, 0)
            })

            // Cross-window deep-link: when the volume picker's "Network (disabled)" entry
            // (or anything else) opens an already-running settings window with a target
            // section, navigate there.
            unlistenNavigate = await listen<{ section: string[] }>('navigate-to-section', (event) => {
                handleSectionSelect(event.payload.section)
            })

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
        // Clean up event listeners
        unlistenFocusSelf?.()
        unlistenNavigate?.()
        cleanupAccentColor()
        cleanupTextSize()
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
<main class="settings-window" tabindex="-1">
    <h1 class="sr-only">Settings</h1>
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
</main>

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
