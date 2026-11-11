<script lang="ts">
    import { onMount, onDestroy, tick } from 'svelte'
    import { getCurrentWindow, LogicalSize } from '@tauri-apps/api/window'
    import { listen, type UnlistenFn } from '@tauri-apps/api/event'
    import { onMcpSettingsClose, activateWindowMenu, isE2eMode } from '#lib/tauri-commands/index.js'
    import SettingsSidebar from '#lib/settings/components/SettingsSidebar.svelte'
    import SettingsContent from '#lib/settings/components/SettingsContent.svelte'
    import { flushSettingsChanges, forceSave as forceSettingsSave } from '#lib/settings/index.js'
    import { initWindowSettings, initWindowLanguageSync } from '#lib/settings/window-settings.js'
    import { initAppMode } from '#lib/app-mode.js'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { initializeShortcuts, flushPendingSave as flushShortcutsSave } from '#lib/shortcuts/index.js'
    import { initAccentColor, cleanupAccentColor } from '#lib/accent-color.js'
    import { initGlassMaterial, cleanupGlassMaterial } from '#lib/glass-material.js'
    import { initDisplayAccessibility, cleanupDisplayAccessibility } from '#lib/display-accessibility.js'
    import { initTextSize, cleanupTextSize, getEffectiveScale } from '#lib/text-size.svelte.js'
    import { initSystemStrings } from '#lib/system-strings.svelte.js'
    import {
        SETTINGS_BASE_MIN_HEIGHT,
        settingsMaxWidth,
        settingsMinWidth,
        commandIdFromShortcutAnchor,
    } from '#lib/settings/settings-window.js'
    import {
        resetShortcutFilters,
        setPendingShortcutHighlight,
    } from '#lib/settings/pending-shortcut-highlight.svelte.js'
    import { getMatchingSections } from '#lib/settings/settings-search.js'
    import {
        CONTENT_FADE_BAND_PX,
        contentScrollTopFor,
        findSectionElements,
        flashElements,
    } from '#lib/settings/settings-navigation.js'
    import { loadLastSettingsSection, saveLastSettingsSection } from '#lib/app-status-store.js'
    import { getAppLogger } from '#lib/logging/logger.js'
    import { trackOwnRect } from '#lib/window-positioning.js'
    import { closeSelfWindow } from '#lib/child-window-close.js'
    import { showSelfWindow } from '#lib/child-window-show.js'

    const log = getAppLogger('settings')

    let searchQuery = $state('')
    // Derived, not set per keystroke: the search reads the UI language reactively, so a language
    // switch mid-search re-lights the sidebar for the new language.
    const matchingSections = $derived(searchQuery.trim() ? getMatchingSections(searchQuery) : new Set<string>())
    let selectedSection = $state<string[]>(['Appearance', 'Colors and formats'])
    let initialized = $state(false)
    let contentElement: HTMLElement | null = $state(null)
    let sidebar: { resetSearch: () => void } | undefined = $state()
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
        const band = Math.min(Math.max(contentScrollTop, 0), CONTENT_FADE_BAND_PX)
        const opaque = band / 5
        return `linear-gradient(to bottom, transparent 0px, transparent ${String(opaque)}px, black ${String(band)}px)`
    })
    function handleContentScroll(): void {
        if (contentElement) {
            contentScrollTop = contentElement.scrollTop
        }
    }
    let unlistenFocusSearch: UnlistenFn | undefined
    let unlistenNavigate: UnlistenFn | undefined
    let unlistenMcpClose: UnlistenFn | undefined
    let unlistenWindowFocus: UnlistenFn | undefined
    let unlistenRectTracking: (() => void) | undefined
    let unsubscribeLanguage: (() => void) | undefined

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
                log.warn('Settings window setMinSize/setMaxSize failed: {error}', { error: e })
            }
        })()
    })

    // Handle search input
    function handleSearch(query: string) {
        log.debug('Search query changed: {query}', { query })
        searchQuery = query
    }

    /** `'auto'` when the user prefers reduced motion, else `'smooth'`. */
    function scrollBehavior(): ScrollBehavior {
        return window.matchMedia('(prefers-reduced-motion: reduce)').matches ? 'auto' : 'smooth'
    }

    /** Bumped by each jump, so one still waiting on a render gives way to a newer one. */
    let latestJump = 0

    /** Scroll the content wrapper directly: `scrollIntoView` would drag the window's layout along. */
    function jumpTo(scroller: HTMLElement, targets: HTMLElement[]) {
        scroller.scrollTo({ top: contentScrollTopFor(scroller, targets[0]), behavior: scrollBehavior() })
        flashElements(targets)
    }

    /**
     * A section picked in the sidebar or a summary card. During a search every result is already
     * on the page, so this jumps to the pick and flashes it, which also covers results too short
     * to scroll. Otherwise the pick replaces the page, which starts at the top with no flash.
     */
    async function showSelectedSection(sectionPath: string[]) {
        const jump = ++latestJump
        await tick()
        if (jump !== latestJump || !contentElement) return
        if (!searchQuery.trim()) {
            contentElement.scrollTo({ top: 0 })
            return
        }
        const targets = findSectionElements(contentElement, sectionPath)
        if (targets.length > 0) jumpTo(contentElement, targets)
    }

    /**
     * Land an in-app link, after `selectedSection` has committed: scroll to the most specific
     * target (the anchor, else the section) and flash it. An anchor that never appears still lands
     * on the right section.
     *
     * A shortcut-row anchor (`shortcut-<commandId>`) takes its own path: the row sits inside the
     * section's nested `.commands-list` scroller, and a leftover filter may hide it, so the filters
     * clear before the wait. The row re-keys on every shortcut change, so its flash is state-driven
     * (`pending-shortcut-highlight`) where the others set a class.
     *
     * `tick()` lets the section render, and `setTimeout(0)` (not `requestAnimationFrame`) lets its
     * `onMount` finish: the window can open without focus in E2E, where macOS WKWebView throttles
     * rAF. `docs/testing.md` § "rAF in unfocused windows".
     */
    async function landDeepLink(section: string[], anchorId: string | null) {
        const jump = ++latestJump
        const shortcutCommandId = anchorId === null ? null : commandIdFromShortcutAnchor(anchorId)
        if (shortcutCommandId !== null) resetShortcutFilters()
        await tick()
        await new Promise((resolve) => setTimeout(resolve, 0))
        if (jump !== latestJump || !contentElement) return

        const anchor = anchorId === null ? null : document.getElementById(anchorId)
        if (shortcutCommandId !== null) {
            scrollShortcutRowIntoView(anchor)
            setPendingShortcutHighlight(shortcutCommandId)
            return
        }
        const targets = anchor ? [anchor] : findSectionElements(contentElement, section)
        if (targets.length > 0) {
            jumpTo(contentElement, targets)
        } else {
            contentElement.scrollTo({ top: 0 })
        }
    }

    /**
     * Scroll the INNER `.commands-list` only: the outer `contentElement.scrollTo` can't reach a row
     * in this overflow container. 16 px of breathing room above; no fade up here.
     */
    function scrollShortcutRowIntoView(target: HTMLElement | null) {
        const list = target?.closest('.commands-list')
        if (!target || !(list instanceof HTMLElement)) return
        const targetTop = list.scrollTop + (target.getBoundingClientRect().top - list.getBoundingClientRect().top)
        list.scrollTo({ top: Math.max(targetTop - 16, 0), behavior: scrollBehavior() })
    }

    // Handle section selection from sidebar
    function handleSectionSelect(sectionPath: string[]) {
        log.debug('Section selected: {sectionPath}', { sectionPath: sectionPath.join(' > ') })
        selectedSection = sectionPath
        // Save last section to app status store
        void saveLastSettingsSection(sectionPath)
        void showSelectedSection(sectionPath)
    }

    // Handle keyboard events
    function handleKeydown(event: KeyboardEvent) {
        if (event.key === 'Escape') {
            event.preventDefault()
            // ❌ Never `getCurrentWindow().close()` here: destroying this webview from inside the
            // handler that asked for it stalls queued IPC on webkit2gtk and can segfault the whole
            // app on macOS WebKit. The backend hides it and destroys it a moment later.
            // See `#lib/child-window-close`.
            void closeSelfWindow()
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
                const searchInput = document.querySelector('.search-container input.text-field-control')
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
            // Initialize settings (store + reactive layer) and the shortcuts store
            // The app mode settles here too: the sidebar and search read it before they render
            // (`imageIndexingAvailable()`: the Linux E2E build's fake image engine keeps the
            // Image indexing page there).
            log.debug('Calling initWindowSettings() and initializeShortcuts()')
            // An unknown mode reads as a plain run, so it mustn't hold the window back.
            const appModeSettled = initAppMode().catch((error: unknown) => {
                log.warn('App mode unknown, reading it as a plain run: {error}', { error })
            })
            await Promise.all([initWindowSettings(), initializeShortcuts(), appModeSettled])
            log.debug('Settings and shortcuts initialization complete')

            await installE2eFlushHook()

            // Apply + live-sync the UI language for this window (own i18n runtime).
            unsubscribeLanguage = initWindowLanguageSync({ title: () => tString('settings.window.title') })

            // Read system accent color from macOS and listen for changes
            await initAccentColor()

            await initGlassMaterial()
            await initDisplayAccessibility()

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

            // The opener creates this window hidden; it looks right now, so show it
            // (`#lib/child-window-show`). A no-op on an E2E window, which is created visible.
            showSelfWindow()

            if (parsed) {
                void landDeepLink(parsed, urlAnchor)
            }

            // Focus the search input on open so users can start typing immediately.
            const searchInput = document.querySelector('.search-container input.text-field-control')
            if (searchInput instanceof HTMLElement) {
                searchInput.focus()
            }

            // ⌘, on an already-open window: the opener has the backend raise it, then
            // asks for the search field so typing lands there again.
            unlistenFocusSearch = await listen('focus-search', () => {
                const input = document.querySelector('.search-container input.text-field-control')
                if (input instanceof HTMLElement) input.focus()
            })

            // Cross-window deep-link: when the volume picker's "Network (disabled)" entry
            // (or anything else) opens an already-running settings window with a target
            // section, navigate there. A search left running could filter the target away, so
            // it ends here, along with any typing still waiting to become one.
            unlistenNavigate = await listen<{ section: string[]; anchor?: string }>(
                'navigate-to-section',
                (event) => {
                    sidebar?.resetSearch()
                    searchQuery = ''
                    selectedSection = event.payload.section
                    void saveLastSettingsSection(event.payload.section)
                    void landDeepLink(event.payload.section, event.payload.anchor ?? null)
                },
            )

            // MCP can request that this window close (used by `dialog close settings`).
            // Mirror the Escape-key handler: defer the close so the in-flight IPC ack can
            // settle before webkit2gtk begins destroying the webview, and so an in-flight
            // layer-tree commit can drain before macOS WebKit tears the view down. See
            // `handleKeydown` and `#lib/window-close-defer`.
            unlistenMcpClose = await onMcpSettingsClose(() => {
                log.debug('Received mcp-settings-close, closing window')
                void closeSelfWindow()
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

    /**
     * E2E-only: lets a spec force the pending settings/shortcuts write to disk instead of
     * waiting out the store's 500 ms save debounce. The same write the window runs on
     * close (`flushSettingsChanges` is `forceSave` when this window changed something), so
     * tests exercise the production flush path rather than a test-only one; it just isn't
     * reachable from an injected script otherwise. Fire-and-forget on
     * purpose — the caller polls the file, which is the assertion that matters.
     *
     * Deliberately NOT a shorter debounce under test: `saveToStore()` has no re-entrancy
     * guard and does one IPC round-trip per changed key, so shrinking the window would
     * make overlapping saves routine and would stop any test covering the real 500 ms
     * window.
     */
    async function installE2eFlushHook(): Promise<void> {
        if (!(await isE2eMode())) return
        ;(window as unknown as { __cmdrFlushSettings?: () => void }).__cmdrFlushSettings = () => {
            void Promise.all([forceSettingsSave(), flushShortcutsSave()])
        }
    }

    // Flush this window's pending saves when the Settings window is closing. Only its OWN
    // changes: a window that changed nothing writes nothing (`flushSettingsChanges`).
    onDestroy(() => {
        log.debug('Settings page destroying, flushing pending saves')
        // Fire and forget - we can't await in onDestroy
        void Promise.all([flushSettingsChanges(), flushShortcutsSave()])
        // Clean up event listeners
        unlistenFocusSearch?.()
        unlistenNavigate?.()
        unlistenMcpClose?.()
        unlistenWindowFocus?.()
        unlistenRectTracking?.()
        unsubscribeLanguage?.()
        cleanupAccentColor()
        cleanupGlassMaterial()
        cleanupDisplayAccessibility()
        cleanupTextSize()
    })

    // Also handle beforeunload for when window is closed directly
    function handleBeforeUnload() {
        log.debug('Window unloading, flushing pending saves')
        // Use sync approach since beforeunload doesn't wait for promises
        void Promise.all([flushSettingsChanges(), flushShortcutsSave()])
    }
</script>

<svelte:window on:keydown={handleKeydown} on:focusout={handleFocusOut} on:beforeunload={handleBeforeUnload} />

<!-- Prevent body from being a tab stop by keeping focus within the settings window -->
<main class="settings-window" tabindex="-1">
    <h1 class="sr-only">{tString('settings.window.title')}</h1>
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
                bind:this={sidebar}
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
        <div class="settings-loading">{tString('settings.window.loading')}</div>
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
        /* `clip`, not `hidden`: `hidden` makes this a scroll CONTAINER, so a
           stray absolutely-positioned descendant that overflows the fold (e.g.
           an Ark hidden input, though those are now contained at the control)
           can be programmatically scrolled into view, dragging the whole window
           under the traffic lights. `clip` creates no scroll container, so the
           shell can never scroll. Belt to the per-control fix in `lib/ui`. WebKit before
           Safari 16 (Catalina) drops `clip`: see the `@supports not` block below. */
        overflow: clip;
        display: flex;
        flex-direction: column;
        /* Anchor for the absolutely-positioned `.window-drag-region` strip. */
        position: relative;
        /* Match the OS window corner radius set via
           `windowEffects.radius` in `settings-window.ts`. Without this the
           content's edges would square-clip against the rounded NSWindow
           corners and leak vibrancy through the gap. */
        border-radius: var(--radius-window);
    }

    /* Falling back to `visible` would let the page itself scroll under the traffic lights, so
       old WebKit gets `hidden`. A block, not a second declaration: the minifier keeps only the
       last of two `overflow` lines. */
    @supports not (overflow: clip) {
        .settings-window {
            overflow: hidden;
        }
    }

    .settings-layout {
        display: flex;
        flex: 1;
        overflow: hidden;
        /* 8 px of breathing room (`--spacing-sm`) on all four sides — the
           sidebar's own radius (the window's minus this padding, so the
           gap keeps one thickness around the corner) makes it float
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
       working. Positioned absolutely so it doesn't push other layout.
       ❗ `--z-sticky`, NOT `--z-dropdown`: this strip paints over whatever it
       covers, so at the dropdown rung it swallowed the top rows of any open
       menu that reached up here (the AI provider pop-up's first two options).
       Being positioned is already enough to sit over the in-flow layout; a
       rung below `--z-dropdown` keeps window chrome under real menus. */
    .window-drag-region {
        position: absolute;
        top: 0;
        left: 0;
        right: 0;
        height: 50px;
        z-index: var(--z-sticky);
    }
</style>
