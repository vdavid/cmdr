<script lang="ts">
    import { onMount, onDestroy, tick } from 'svelte'
    import { closeSelfWindow } from '#lib/child-window-close.js'
    import { showSelfWindow } from '#lib/child-window-show.js'
    import { initWindowSettings, initWindowLanguageSync } from '#lib/settings/window-settings.js'
    import { initializeShortcuts } from '#lib/shortcuts/index.js'
    import { initAccentColor, cleanupAccentColor } from '#lib/accent-color.js'
    import { initGlassMaterial, cleanupGlassMaterial } from '#lib/glass-material.js'
    import { initDisplayAccessibility, cleanupDisplayAccessibility } from '#lib/display-accessibility.js'
    import { initTextSize, cleanupTextSize } from '#lib/text-size.svelte.js'
    import { requestOpenSettings } from '#lib/tauri-commands/index.js'
    import { trackOwnRect } from '#lib/window-positioning.js'
    import { getAppLogger } from '#lib/logging/logger.js'
    import { tString } from '#lib/intl/messages.svelte.js'
    import Checkbox from '#lib/ui/Checkbox.svelte'
    import LinkButton from '#lib/ui/LinkButton.svelte'
    import ShortcutsList from '#lib/shortcuts/ShortcutsList.svelte'

    const log = getAppLogger('shortcuts')

    let initialized = $state(false)
    let hideEmpty = $state(false)
    let unlistenRectTracking: (() => void) | undefined
    let unsubscribeLanguage: (() => void) | undefined

    function editShortcuts() {
        // Deep-link to the editable list. This window stays read-only and lacks
        // window-creation capability, so it asks the main window to open Settings
        // (which owns `openSettingsWindow`) over the shared `open-settings` channel.
        void requestOpenSettings('Keyboard shortcuts')
    }

    function handleKeydown(event: KeyboardEvent) {
        if (event.key === 'Escape') {
            event.preventDefault()
            void closeSelfWindow()
        }
    }

    onMount(async () => {
        const loadingScreen = document.getElementById('loading-screen')
        if (loadingScreen) loadingScreen.style.display = 'none'

        try {
            // Settings must load before text-size/theme reads; shortcuts before the list.
            await Promise.all([initWindowSettings(), initializeShortcuts()])
            // Own webview, own i18n runtime: without this the window renders its
            // chrome in English under a Hungarian UI and never follows a switch.
            unsubscribeLanguage = initWindowLanguageSync({ title: () => tString('shortcuts.window.title') })
            await initAccentColor()
            await initGlassMaterial()
            await initDisplayAccessibility()
            await initTextSize()
            initialized = true

            // The opener creates this window hidden; it looks right now, so show it
            // (`#lib/child-window-show`). A no-op on an E2E window, which is created visible.
            await tick()
            showSelfWindow()

            // Remember position/size within the session so a reopen lands in place.
            unlistenRectTracking = await trackOwnRect('shortcuts')
        } catch (error) {
            log.error('Failed to initialize keyboard shortcuts window: {error}', { error })
        }
    })

    onDestroy(() => {
        unsubscribeLanguage?.()
        unlistenRectTracking?.()
        cleanupAccentColor()
        cleanupGlassMaterial()
        cleanupDisplayAccessibility()
        cleanupTextSize()
    })
</script>

<svelte:window on:keydown={handleKeydown} />

<main class="shortcuts-window" tabindex="-1">
    <h1 class="sr-only">{tString('shortcuts.window.title')}</h1>
    <!-- Drag strip under the overlay traffic lights, like Settings/Viewer. -->
    <div class="window-drag-region" data-tauri-drag-region aria-hidden="true"></div>

    <header class="shortcuts-header">
        <div class="title-row">
            <span class="title">{tString('shortcuts.window.title')}</span>
            <LinkButton onclick={editShortcuts}>{tString('shortcuts.window.edit')}</LinkButton>
        </div>
        <div class="hide-empty">
            <Checkbox bind:checked={hideEmpty}>{tString('shortcuts.window.hideEmpty')}</Checkbox>
        </div>
    </header>

    {#if initialized}
        <div class="shortcuts-scroll" tabindex="-1">
            <ShortcutsList {hideEmpty} />
            <footer class="shortcuts-footer">
                <LinkButton onclick={editShortcuts}>{tString('shortcuts.window.editInSettings')}</LinkButton>
            </footer>
        </div>
    {/if}
</main>

<style>
    .shortcuts-window {
        width: 100%;
        height: 100vh;
        background: var(--color-bg-primary);
        color: var(--color-text-primary);
        font-family: var(--font-system) sans-serif;
        font-size: var(--font-size-sm);
        overflow: hidden;
        display: flex;
        flex-direction: column;
        position: relative;
    }

    /* Drag strip over the overlay title-bar row (where the traffic lights sit).
       Kept to the title-bar height so it doesn't float over the header controls
       below it (it paints on top, so anything under it wouldn't be clickable). */
    .window-drag-region {
        position: absolute;
        top: 0;
        left: 0;
        right: 0;
        height: var(--titlebar-height);
        z-index: var(--z-sticky);
    }

    .shortcuts-header {
        /* Start below the overlay title-bar so the title and link clear the
           traffic lights and the drag strip above. */
        padding: var(--spacing-sm) var(--spacing-lg) var(--spacing-sm);
        padding-top: calc(var(--titlebar-height) + var(--spacing-sm));
        display: flex;
        flex-direction: column;
        gap: var(--spacing-sm);
        border-bottom: 1px solid var(--color-border-subtle);
    }

    .title-row {
        display: flex;
        align-items: baseline;
        justify-content: space-between;
        gap: var(--spacing-md);
    }

    .title {
        font-size: var(--font-size-lg);
        font-weight: 600;
    }

    .hide-empty :global(.checkbox-label) {
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }

    .shortcuts-scroll {
        flex: 1;
        overflow-y: auto;
        padding: var(--spacing-lg);
        outline: none;
        scrollbar-gutter: stable;
    }

    .shortcuts-footer {
        margin-top: var(--spacing-lg);
        padding-top: var(--spacing-md);
        border-top: 1px solid var(--color-border-subtle);
        text-align: center;
    }
</style>
