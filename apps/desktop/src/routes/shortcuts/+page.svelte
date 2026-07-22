<script lang="ts">
    import { onMount, onDestroy } from 'svelte'
    import { getCurrentWindow } from '@tauri-apps/api/window'
    import { listen, type UnlistenFn } from '@tauri-apps/api/event'
    import { initializeSettings } from '$lib/settings'
    import { initializeShortcuts } from '$lib/shortcuts'
    import { initAccentColor, cleanupAccentColor } from '$lib/accent-color'
    import { initReduceTransparency, cleanupReduceTransparency } from '$lib/reduce-transparency'
    import { initTextSize, cleanupTextSize } from '$lib/text-size.svelte'
    import { requestOpenSettings } from '$lib/tauri-commands'
    import { trackOwnRect } from '$lib/window-positioning'
    import { getAppLogger } from '$lib/logging/logger'
    import Checkbox from '$lib/ui/Checkbox.svelte'
    import LinkButton from '$lib/ui/LinkButton.svelte'
    import ShortcutsList from '$lib/shortcuts/ShortcutsList.svelte'

    const log = getAppLogger('shortcuts')

    let initialized = $state(false)
    let hideEmpty = $state(false)
    let unlistenFocusSelf: UnlistenFn | undefined
    let unlistenRectTracking: (() => void) | undefined

    function editShortcuts() {
        // Deep-link to the editable list. This window stays read-only and lacks
        // window-creation capability, so it asks the main window to open Settings
        // (which owns `openSettingsWindow`) over the shared `open-settings` channel.
        void requestOpenSettings('Keyboard shortcuts')
    }

    function handleKeydown(event: KeyboardEvent) {
        if (event.key === 'Escape') {
            event.preventDefault()
            // Defer the close past the current event-loop tick so any in-flight IPC
            // ack settles before the webview is destroyed. Mirrors the Settings /
            // Viewer windows. `setTimeout(0)`, not rAF (throttled when unfocused).
            const win = getCurrentWindow()
            setTimeout(() => {
                void win.close()
            }, 0)
        }
    }

    onMount(async () => {
        const loadingScreen = document.getElementById('loading-screen')
        if (loadingScreen) loadingScreen.style.display = 'none'

        try {
            // Settings must load before text-size/theme reads; shortcuts before the list.
            await Promise.all([initializeSettings(), initializeShortcuts()])
            await initAccentColor()
            await initReduceTransparency()
            await initTextSize()
            initialized = true

            // Already-open window self-focuses on a re-open (cross-window setFocus()
            // doesn't reliably raise a window on macOS).
            unlistenFocusSelf = await listen('focus-self', () => {
                setTimeout(() => {
                    void getCurrentWindow().setFocus()
                }, 0)
            })

            // Remember position/size within the session so a reopen lands in place.
            unlistenRectTracking = await trackOwnRect('shortcuts')
        } catch (error) {
            log.error('Failed to initialize keyboard shortcuts window: {error}', { error })
        }
    })

    onDestroy(() => {
        unlistenFocusSelf?.()
        unlistenRectTracking?.()
        cleanupAccentColor()
        cleanupReduceTransparency()
        cleanupTextSize()
    })
</script>

<svelte:window on:keydown={handleKeydown} />

<main class="shortcuts-window" tabindex="-1">
    <h1 class="sr-only">Keyboard shortcuts</h1>
    <!-- Drag strip under the overlay traffic lights, like Settings/Viewer. -->
    <div class="window-drag-region" data-tauri-drag-region aria-hidden="true"></div>

    <header class="shortcuts-header">
        <div class="title-row">
            <span class="title">Keyboard shortcuts</span>
            <LinkButton onclick={editShortcuts}>Edit shortcuts</LinkButton>
        </div>
        <div class="hide-empty">
            <Checkbox bind:checked={hideEmpty}>Hide features with no shortcut</Checkbox>
        </div>
    </header>

    {#if initialized}
        <div class="shortcuts-scroll" tabindex="-1">
            <ShortcutsList {hideEmpty} />
            <footer class="shortcuts-footer">
                <LinkButton onclick={editShortcuts}>Edit shortcuts in Settings</LinkButton>
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
        z-index: var(--z-dropdown);
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
