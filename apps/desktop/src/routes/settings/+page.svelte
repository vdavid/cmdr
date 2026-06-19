<script lang="ts">
    import { onMount, onDestroy, tick } from 'svelte'
    import { getCurrentWindow, LogicalSize } from '@tauri-apps/api/window'
    import { listen, type UnlistenFn } from '@tauri-apps/api/event'
    import { onMcpSettingsClose, activateWindowMenu } from '$lib/tauri-commands'
    import SettingsSidebar from '$lib/settings/components/SettingsSidebar.svelte'
    import SettingsContent from '$lib/settings/components/SettingsContent.svelte'
    import { initializeSettings, forceSave as forceSettingsSave, getSetting, onSpecificSettingChange } from '$lib/settings'
    import { setLocale } from '$lib/intl/messages.svelte'
    import { initializeShortcuts, flushPendingSave as flushShortcutsSave } from '$lib/shortcuts'
    import { initAccentColor, cleanupAccentColor } from '$lib/accent-color'
    import { initReduceTransparency, cleanupReduceTransparency } from '$lib/reduce-transparency'
    import { initTextSize, cleanupTextSize, getEffectiveScale } from '$lib/text-size.svelte'
    import { initSystemStrings } from '$lib/system-strings.svelte'
    import {
        SETTINGS_BASE_MIN_HEIGHT,
        settingsMaxWidth,
        settingsMinWidth,
        commandIdFromShortcutAnchor,
    } from '$lib/settings/settings-window'
    import {
        resetShortcutFilters,
        setPendingShortcutHighlight,
    } from '$lib/settings/pending-shortcut-highlight.svelte'
    import { getMatchingSections } from '$lib/settings/settings-search'
    import { loadLastSettingsSection, saveLastSettingsSection } from '$lib/app-status-store'
    import { getAppLogger } from '$lib/logging/logger'
    import { trackOwnRect } from '$lib/window-positioning'

    const log = getAppLogger('settings')

    let searchQuery = $state('')
    let matchingSections = $state<Set<string>>(new Set())
    let selectedSection = $state<string[]>(['Appearance', 'Colors and formats'])
    let initialized = $state(false)
    let contentElement: HTMLElement | null = $state(null)
    let contentScrollTop = $state(0)
    /** Mask string for the scroll wrapper. Top-fade band height tracks
        `scrollTop` 0..70 px and caps at 70. Within the band the top 20 %
        is fully transparent (= hides scrolled-up content); the remaining
        80 % linearly fades to fully visible. At `scrollTop = 0` the band
        is zero-wide, so the gradient effectively collapses to "all black
        = no fade." Stylelint's allowed-values list rejects mask-image
        with a `calc(var(...) / 5)` so we compute the whole string in JS
        and apply it via `style:mask-image` (inline runtime style, not
        scoped CSS that gets linted). */
    const contentMaskImage = $derived.by(() => {
        const band = Math.min(Math.max(contentScrollTop, 0), 70)
        const opaque = band / 5
        return `linear-gradient(to bottom, transparent 0px, transparent ${String(opaque)}px, black ${String(band)}px)`
    })
    function handleContentScroll(): void {
        if (contentElement) {
            contentScrollTop = contentElement.scrollTop
        }
    }
    let unlistenFocusSelf: UnlistenFn | undefined
    let unlistenNavigate: UnlistenFn | undefined
    let unlistenMcpClose: UnlistenFn | undefined
    let unlistenWindowFocus: UnlistenFn | undefined
    let unlistenRectTracking: (() => void) | undefined
    let unsubscribeLanguage: (() => void) | undefined

    /**
     * Keeps THIS window's UI language in sync. The settings window is its own
     * webview with its own i18n runtime instance, so the main window's applier
     * doesn't reach it: apply the persisted language at open, and re-apply on any
     * `appearance.language` change (including the user's own pick in this window,
     * which round-trips through the store), so the picker re-localizes the whole
     * settings UI live. `'system'` maps to the OS locale (`setLocale(null)`).
     */
    function initLanguageSync(): void {
        const applyLanguage = (value: string) => { setLocale(value === 'system' ? null : value); }
        applyLanguage(getSetting('appearance.language'))
        unsubscribeLanguage = onSpecificSettingChange('appearance.language', (_id, value) => {
            applyLanguage(value)
        })
    }

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

    /** `'auto'` when the user prefers reduced motion, else `'smooth'`. */
    function scrollBehavior(): ScrollBehavior {
        return window.matchMedia('(prefers-reduced-motion: reduce)').matches ? 'auto' : 'smooth'
    }

    /**
     * Scroll a deep-link anchor into view. Called after `selectedSection` has
     * committed and the section's `onMount` has run.
     *
     * Two paths:
     *
     * - **Shortcut-row anchors** (`shortcut-<commandId>`) live inside the
     *   Keyboard-shortcuts section's nested `.commands-list` scroller, which the
     *   outer `contentElement.scrollTo` can't reach. They also may be hidden by a
     *   leftover filter. So: clear the section's filters, wait for the target row
     *   to mount, then scroll the inner list and flash the row.
     * - **Everything else** (the `settings-downloads-notifications` sub-group, …)
     *   keeps the deliberate `contentElement.scrollTo` path — `handleSectionSelect`
     *   carries the same comment about avoiding `scrollIntoView` so the outer
     *   settings layout / drag region doesn't shift.
     *
     * `setTimeout(0)` (not `requestAnimationFrame`) defers past the current handler
     * because the settings window can open without focus in E2E, where macOS
     * WKWebView throttles rAF — see `docs/testing.md` § "rAF in unfocused windows".
     * If the anchor never appears the scroll silently no-ops; the deep-link still
     * lands on the right section.
     */
    function scrollAnchorIntoView(anchorId: string) {
        const shortcutCommandId = commandIdFromShortcutAnchor(anchorId)
        if (shortcutCommandId !== null) {
            void scrollShortcutRowIntoView(anchorId, shortcutCommandId)
            return
        }
        setTimeout(() => {
            const target = document.getElementById(anchorId)
            if (target && contentElement) {
                contentElement.scrollTo({
                    top: target.offsetTop - 16,
                    behavior: scrollBehavior(),
                })
            }
        }, 0)
    }

    /**
     * Deep-link arrival into a Keyboard-shortcuts row. The sequence is
     * load-bearing (see the plan's § Deep link timing):
     *
     *   1. Clear the section's filters synchronously — a leftover `Modified`
     *      filter or search query may keep the target row out of the DOM.
     *   2. `await tick()` — clearing filters mutates `$derived` state
     *      (`filteredCommands` → `groupedCommands`); the row does NOT exist until
     *      Svelte flushes.
     *   3. `setTimeout(0)` — defer past the current handler (and stay off rAF for
     *      the unfocused-window throttle).
     *   4. Scroll the nested `.commands-list` to the row and set the flash state.
     */
    async function scrollShortcutRowIntoView(anchorId: string, commandId: string) {
        resetShortcutFilters()
        await tick()
        setTimeout(() => {
            const target = document.getElementById(anchorId)
            const list = target?.closest('.commands-list')
            if (target instanceof HTMLElement && list instanceof HTMLElement) {
                // Scroll the INNER scroller only, so the outer settings layout /
                // drag region stays put (the outer `contentElement.scrollTo` can't
                // reach a row inside this overflow container anyway). Compute the
                // target via the live rect delta rather than `offsetTop`, which is
                // relative to each element's own `offsetParent` and can differ
                // between the row and the list. 16 px of breathing room above.
                const targetTop = list.scrollTop + (target.getBoundingClientRect().top - list.getBoundingClientRect().top)
                list.scrollTo({
                    top: Math.max(targetTop - 16, 0),
                    behavior: scrollBehavior(),
                })
            }
            setPendingShortcutHighlight(commandId)
        }, 0)
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
            // Defer the close() past the current event-loop iteration so the
            // keydown handler can settle (including any in-flight IPC ack to
            // the Tauri runtime) before webkit2gtk begins destroying this
            // webview. Without this, the synchronous close() runs inside the
            // same GTK main-loop tick that handled the keydown, and the
            // destruction can stall queued IPC calls from other webviews —
            // the root cause of the Linux E2E flake on this binding. Mirrors
            // the pattern in `routes/viewer/+page.svelte`'s `closeWindow()`.
            //
            // Uses `setTimeout(0)` instead of nested `requestAnimationFrame`s
            // because macOS WKWebView throttles rAF for windows that opened
            // without focus (E2E case: `openSettingsWindow` passes `focus: false`
            // under `CMDR_E2E_MODE`). Throttled rAF can push the deferred
            // close past the test's 3 s close-confirmation budget. setTimeout
            // isn't subject to the same throttling and still defers to the
            // next event-loop tick, which is all the Linux fix needs.
            const win = getCurrentWindow()
            setTimeout(() => {
                void win.close()
            }, 0)
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

            // Apply + live-sync the UI language for this window (own i18n runtime).
            initLanguageSync()

            // Read system accent color from macOS and listen for changes
            await initAccentColor()

            await initReduceTransparency()

            // Apply compounded text size (system Accessibility × user setting)
            await initTextSize()

            // Hydrate localized macOS pane labels for the Network and Appearance sections.
            await initSystemStrings()

            // Load last viewed section, but a `?section=...` URL param wins so callers (like
            // the volume picker's "Network (disabled)" entry) can deep-link. The param is a
            // JSON-encoded string array so section names with `/` (like "SMB/Network shares")
            // round-trip safely.
            const params = new URLSearchParams(window.location.search)
            const urlSection = params.get('section')
            const parsed = urlSection ? safeParseSectionParam(urlSection) : null
            const urlAnchor = params.get('anchor')
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

            // Scroll the deep-link anchor into view, if any. Runs after `selectedSection`
            // commits and the section's `onMount` runs so the target element exists.
            if (urlAnchor) {
                scrollAnchorIntoView(urlAnchor)
            }

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
            unlistenNavigate = await listen<{ section: string[]; anchor?: string }>(
                'navigate-to-section',
                (event) => {
                    handleSectionSelect(event.payload.section)
                    if (event.payload.anchor) {
                        scrollAnchorIntoView(event.payload.anchor)
                    }
                },
            )

            // MCP can request that this window close (used by `dialog close settings`).
            // Mirror the Escape-key handler: defer past the current event-loop iteration
            // so the in-flight IPC ack can settle before webkit2gtk begins destroying
            // the webview. See `handleKeydown` for why `setTimeout(0)` is used instead
            // of nested rAFs.
            unlistenMcpClose = await onMcpSettingsClose(() => {
                log.debug('Received mcp-settings-close, closing window')
                const win = getCurrentWindow()
                setTimeout(() => {
                    void win.close()
                }, 0)
            })

            // On macOS the app-level menu bar is shared, so this window swaps in the
            // main menu with file-scoped items disabled whenever it gains focus.
            unlistenWindowFocus = await getCurrentWindow().onFocusChanged(
                ({ payload: focused }: { payload: boolean }) => {
                    if (focused) {
                        void activateWindowMenu('other')
                    }
                },
            )

            // Persist position/size while this window is open so reopening
            // within the session lands in the same spot. The cache is in-memory
            // on the Rust side and reset on app start.
            unlistenRectTracking = await trackOwnRect('settings')

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
        unlistenMcpClose?.()
        unlistenWindowFocus?.()
        unlistenRectTracking?.()
        unsubscribeLanguage?.()
        cleanupAccentColor()
        cleanupReduceTransparency()
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
    <!-- Drag region for moving the window. Spans the top strip of the window
         (40 px) so the user can grab anywhere up there — including over the
         traffic-light row's free space — to drag. The traffic-light buttons
         themselves are NSWindow chrome and stay clickable on top of this
         (`pointer-events: none` would also work; the OS chrome paints over
         the webview either way). The `data-tauri-drag-region` attribute is
         what Tauri's overlay-titlebar implementation looks for. -->
    <div class="window-drag-region" data-tauri-drag-region aria-hidden="true"></div>
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
            <div
                class="settings-content-wrapper"
                bind:this={contentElement}
                onscroll={handleContentScroll}
                style:mask-image={contentMaskImage}
                style:-webkit-mask-image={contentMaskImage}
                tabindex="-1"
            >
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
        /* `--color-bg-settings-primary` is a settings-only translucent token
           (defined in `app.css`). Kept separate from `--color-bg-primary` so
           the settings window can run more glass-y without dragging the main
           window's file-list alpha down (capped by the a11y row-state matrix
           at 0.85 / 0.93). The macOS `NSVisualEffectView` (Sidebar material)
           sits behind the webview thanks to `transparent: true` +
           `backgroundColor: [0,0,0,0]` + the explicit `setEffects()` call in
           `settings-window.ts`. */
        background: var(--color-bg-settings-primary);
        color: var(--color-text-primary);
        font-family: var(--font-system) sans-serif;
        font-size: var(--font-size-sm);
        overflow: hidden;
        display: flex;
        flex-direction: column;
        /* Anchor for the absolutely-positioned `.window-drag-region` strip. */
        position: relative;
        /* Match the OS window corner radius set via
           `windowEffects.radius` in `settings-window.ts`. Without this the
           content's edges would square-clip against the rounded NSWindow
           corners and leak vibrancy through the gap. */
        border-radius: var(--radius-xxl);
    }

    .settings-layout {
        display: flex;
        flex: 1;
        overflow: hidden;
        /* 8 px of breathing room (`--spacing-sm`) on all four sides — the
           sidebar's own `border-radius: var(--radius-xl)` makes it float
           visibly inside this padded frame. Vibrancy peeks through the gap.
           Padding lives here (not on `.settings-window`) so the drag-region
           strip can extend edge-to-edge over the window's top, including the
           area above the padding. */
        padding: var(--spacing-sm);
    }

    .settings-content-wrapper {
        flex: 1;
        overflow-y: auto;
        padding: var(--spacing-lg);
        outline: none;
        /* `mask-image` is set inline via `style:mask-image={contentMaskImage}`
           — the gradient depends on scrollTop, and stylelint's allowed-
           values list rejects calc-with-custom-property forms for mask-image.
           Inline runtime styles bypass the scoped-CSS lint. See the
           `contentMaskImage` derived value in the script block for the
           gradient math: top 20% of the band fully transparent (hides
           content), bottom 80% fades to fully visible. At scrollTop=0 the
           band is zero-wide so the mask collapses to "no fade". */
    }

    .settings-loading {
        display: flex;
        align-items: center;
        justify-content: center;
        height: 100%;
        color: var(--color-text-tertiary);
    }

    /* Invisible drag handle covering the top strip of the window. Lets the
       user grab and move the window from anywhere above the visible chrome
       — the traffic lights sit on top as NSWindow buttons, so they keep
       working. Positioned absolutely so it doesn't push other layout. */
    .window-drag-region {
        position: absolute;
        top: 0;
        left: 0;
        right: 0;
        height: 50px;
        z-index: var(--z-dropdown);
    }
</style>
