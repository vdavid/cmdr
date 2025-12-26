<script lang="ts">
    import { onMount, onDestroy } from 'svelte'
    import FilePane from './FilePane.svelte'
    import { loadAppStatus, saveAppStatus } from '$lib/app-status-store'
    import { loadSettings, saveSettings, subscribeToSettingsChanges } from '$lib/settings-store'
    import { pathExists } from '$lib/tauri-commands'
    import type { UnlistenFn } from '@tauri-apps/api/event'

    let leftPath = $state('~')
    let rightPath = $state('~')
    let focusedPane = $state<'left' | 'right'>('left')
    let showHiddenFiles = $state(true)
    let initialized = $state(false)

    let containerElement: HTMLDivElement | undefined = $state()
    let leftPaneRef: FilePane | undefined = $state()
    let rightPaneRef: FilePane | undefined = $state()
    let unlistenSettings: UnlistenFn | undefined

    function handleLeftPathChange(path: string) {
        leftPath = path
        void saveAppStatus({ leftPath: path })
        // Re-focus to maintain keyboard handling after navigation
        containerElement?.focus()
    }

    function handleRightPathChange(path: string) {
        rightPath = path
        void saveAppStatus({ rightPath: path })
        // Re-focus to maintain keyboard handling after navigation
        containerElement?.focus()
    }

    function handleLeftFocus() {
        if (focusedPane !== 'left') {
            focusedPane = 'left'
            void saveAppStatus({ focusedPane: 'left' })
        }
    }

    function handleRightFocus() {
        if (focusedPane !== 'right') {
            focusedPane = 'right'
            void saveAppStatus({ focusedPane: 'right' })
        }
    }

    function handleKeyDown(e: KeyboardEvent) {
        if (e.key === 'Tab') {
            e.preventDefault()
            const newFocus = focusedPane === 'left' ? 'right' : 'left'
            focusedPane = newFocus
            void saveAppStatus({ focusedPane: newFocus })
            return
        }

        // Forward arrow keys and Enter to the focused pane
        const activePaneRef = focusedPane === 'left' ? leftPaneRef : rightPaneRef
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        activePaneRef?.handleKeyDown(e)
    }

    onMount(async () => {
        // Load persisted state and settings in parallel
        const [status, settings] = await Promise.all([loadAppStatus(pathExists), loadSettings()])

        leftPath = status.leftPath
        rightPath = status.rightPath
        focusedPane = status.focusedPane
        showHiddenFiles = settings.showHiddenFiles
        initialized = true

        // Subscribe to settings changes from the backend menu
        unlistenSettings = await subscribeToSettingsChanges((newSettings) => {
            if (newSettings.showHiddenFiles !== undefined) {
                showHiddenFiles = newSettings.showHiddenFiles
                // Persist to settings store
                void saveSettings({ showHiddenFiles: newSettings.showHiddenFiles })
            }
        })
    })

    onDestroy(() => {
        unlistenSettings?.()
    })

    // Focus the container after initialization so keyboard events work
    $effect(() => {
        if (initialized) {
            containerElement?.focus()
        }
    })
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex,a11y_no_noninteractive_element_interactions -->
<div
    class="dual-pane-explorer"
    bind:this={containerElement}
    onkeydown={handleKeyDown}
    tabindex="0"
    role="application"
    aria-label="File explorer"
>
    {#if initialized}
        <FilePane
            bind:this={leftPaneRef}
            initialPath={leftPath}
            isFocused={focusedPane === 'left'}
            {showHiddenFiles}
            onPathChange={handleLeftPathChange}
            onRequestFocus={handleLeftFocus}
        />
        <FilePane
            bind:this={rightPaneRef}
            initialPath={rightPath}
            isFocused={focusedPane === 'right'}
            {showHiddenFiles}
            onPathChange={handleRightPathChange}
            onRequestFocus={handleRightFocus}
        />
    {:else}
        <div class="loading">Loading...</div>
    {/if}
</div>

<style>
    .dual-pane-explorer {
        display: flex;
        width: 100%;
        height: 100%;
        gap: 0;
        outline: none;
    }

    .loading {
        display: flex;
        align-items: center;
        justify-content: center;
        width: 100%;
        height: 100%;
        color: var(--color-text-secondary);
    }
</style>
