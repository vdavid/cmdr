<script lang="ts">
    import { onMount, onDestroy } from 'svelte'
    import FilePane from './FilePane.svelte'
    import LoadingIcon from '../LoadingIcon.svelte'
    import {
        loadAppStatus,
        saveAppStatus,
        getLastUsedPathForVolume,
        saveLastUsedPathForVolume,
        type ViewMode,
    } from '$lib/app-status-store'
    import { loadSettings, saveSettings, subscribeToSettingsChanges } from '$lib/settings-store'
    import {
        pathExists,
        listen,
        listVolumes,
        getDefaultVolumeId,
        findContainingVolume,
        DEFAULT_VOLUME_ID,
        type UnlistenFn,
    } from '$lib/tauri-commands'
    import type { VolumeInfo } from './types'
    import { ensureFontMetricsLoaded } from '$lib/font-metrics'

    let leftPath = $state('~')
    let rightPath = $state('~')
    let focusedPane = $state<'left' | 'right'>('left')
    let showHiddenFiles = $state(true)
    let leftViewMode = $state<ViewMode>('brief')
    let rightViewMode = $state<ViewMode>('brief')
    let leftVolumeId = $state(DEFAULT_VOLUME_ID)
    let rightVolumeId = $state(DEFAULT_VOLUME_ID)
    let volumes = $state<VolumeInfo[]>([])
    let initialized = $state(false)

    let containerElement: HTMLDivElement | undefined = $state()
    let leftPaneRef: FilePane | undefined = $state()
    let rightPaneRef: FilePane | undefined = $state()
    let unlistenSettings: UnlistenFn | undefined
    let unlistenViewMode: UnlistenFn | undefined
    let unlistenVolumeUnmount: UnlistenFn | undefined

    // Derived volume paths
    const leftVolumePath = $derived(volumes.find((v) => v.id === leftVolumeId)?.path ?? '/')
    const rightVolumePath = $derived(volumes.find((v) => v.id === rightVolumeId)?.path ?? '/')

    function handleLeftPathChange(path: string) {
        leftPath = path
        void saveAppStatus({ leftPath: path })
        void saveLastUsedPathForVolume(leftVolumeId, path)
        // Re-focus to maintain keyboard handling after navigation
        containerElement?.focus()
    }

    function handleRightPathChange(path: string) {
        rightPath = path
        void saveAppStatus({ rightPath: path })
        void saveLastUsedPathForVolume(rightVolumeId, path)
        // Re-focus to maintain keyboard handling after navigation
        containerElement?.focus()
    }

    async function handleLeftVolumeChange(volumeId: string, volumePath: string, targetPath: string) {
        // Save the current path for the old volume before switching
        void saveLastUsedPathForVolume(leftVolumeId, leftPath)

        // Pass the right pane's state so we can copy its path if it's on the same volume
        const pathToNavigate = await determineNavigationPath(volumeId, volumePath, targetPath, {
            otherPaneVolumeId: rightVolumeId,
            otherPanePath: rightPath,
        })

        leftVolumeId = volumeId
        leftPath = pathToNavigate
        void saveAppStatus({ leftVolumeId: volumeId, leftPath: pathToNavigate })
    }

    async function handleRightVolumeChange(volumeId: string, volumePath: string, targetPath: string) {
        // Save the current path for the old volume before switching
        void saveLastUsedPathForVolume(rightVolumeId, rightPath)

        // Pass the left pane's state so we can copy its path if it's on the same volume
        const pathToNavigate = await determineNavigationPath(volumeId, volumePath, targetPath, {
            otherPaneVolumeId: leftVolumeId,
            otherPanePath: leftPath,
        })

        rightVolumeId = volumeId
        rightPath = pathToNavigate
        void saveAppStatus({ rightVolumeId: volumeId, rightPath: pathToNavigate })
    }

    interface OtherPaneState {
        otherPaneVolumeId: string
        otherPanePath: string
    }

    /**
     * Determines which path to navigate to when switching volumes.
     * Priority order:
     * 1. Favorite path (if targetPath !== volumePath)
     * 2. Other pane's path (if the other pane is on the same volume)
     * 3. Stored lastUsedPath for this volume
     * 4. Default: ~ for main volume, volume root for others
     */
    async function determineNavigationPath(
        volumeId: string,
        volumePath: string,
        targetPath: string,
        otherPane: OtherPaneState,
    ): Promise<string> {
        // User selected a favorite - go to the favorite's path directly
        if (targetPath !== volumePath) {
            return targetPath
        }

        // If the other pane is on the same volume, use its path (allows copying paths between panes)
        if (otherPane.otherPaneVolumeId === volumeId && (await pathExists(otherPane.otherPanePath))) {
            return otherPane.otherPanePath
        }

        // Look up the last used path for this volume
        const lastUsedPath = await getLastUsedPathForVolume(volumeId)
        if (lastUsedPath && (await pathExists(lastUsedPath))) {
            return lastUsedPath
        }

        // Default: ~ for main volume (root), volume path for others
        if (volumeId === DEFAULT_VOLUME_ID) {
            return '~'
        }
        return volumePath
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

        // F1 or ⌥F1 - Open left pane volume chooser
        if (e.key === 'F1') {
            e.preventDefault()
            // eslint-disable-next-line @typescript-eslint/no-unsafe-call
            leftPaneRef?.toggleVolumeChooser()
            return
        }

        // F2 or ⌥F2 - Open right pane volume chooser
        if (e.key === 'F2') {
            e.preventDefault()
            // eslint-disable-next-line @typescript-eslint/no-unsafe-call
            rightPaneRef?.toggleVolumeChooser()
            return
        }

        // Forward arrow keys and Enter to the focused pane
        // eslint-disable-next-line @typescript-eslint/no-unnecessary-type-assertion -- TypeScript thinks FilePane.handleKeyDown is unused without this
        const activePaneRef = (focusedPane === 'left' ? leftPaneRef : rightPaneRef) as FilePane | undefined
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        activePaneRef?.handleKeyDown(e)
    }

    onMount(async () => {
        // Start font metrics measurement in background (non-blocking)
        void ensureFontMetricsLoaded()

        // Load volumes first
        volumes = await listVolumes()

        // Load persisted state and settings in parallel
        const [status, settings] = await Promise.all([loadAppStatus(pathExists), loadSettings()])

        leftPath = status.leftPath
        rightPath = status.rightPath
        focusedPane = status.focusedPane
        showHiddenFiles = settings.showHiddenFiles
        leftViewMode = status.leftViewMode
        rightViewMode = status.rightViewMode

        // Determine the correct volume IDs by finding which volume contains each path
        // This is more reliable than trusting the stored volumeId, which may be stale
        const defaultId = await getDefaultVolumeId()
        const [leftContaining, rightContaining] = await Promise.all([
            findContainingVolume(status.leftPath),
            findContainingVolume(status.rightPath),
        ])
        leftVolumeId = leftContaining?.id ?? defaultId
        rightVolumeId = rightContaining?.id ?? defaultId

        initialized = true

        // Subscribe to settings changes from the backend menu
        unlistenSettings = await subscribeToSettingsChanges((newSettings) => {
            if (newSettings.showHiddenFiles !== undefined) {
                showHiddenFiles = newSettings.showHiddenFiles
                // Persist to settings store
                void saveSettings({ showHiddenFiles: newSettings.showHiddenFiles })
            }
        })

        // Subscribe to view mode changes from the backend menu
        unlistenViewMode = await listen<{ mode: ViewMode }>('view-mode-changed', (event) => {
            const newMode = event.payload.mode
            // Apply to the focused pane
            if (focusedPane === 'left') {
                leftViewMode = newMode
                void saveAppStatus({ leftViewMode: newMode })
            } else {
                rightViewMode = newMode
                void saveAppStatus({ rightViewMode: newMode })
            }
        })

        // Subscribe to volume unmount events
        unlistenVolumeUnmount = await listen<{ volumeId: string }>('volume-unmounted', (event) => {
            void handleVolumeUnmount(event.payload.volumeId)
        })
    })

    async function handleVolumeUnmount(unmountedId: string) {
        const defaultVolumeId = await getDefaultVolumeId()
        const defaultVolume = volumes.find((v) => v.id === defaultVolumeId)
        const defaultPath = defaultVolume?.path ?? '/'

        // Switch affected panes to default volume
        if (leftVolumeId === unmountedId) {
            leftVolumeId = defaultVolumeId
            leftPath = defaultPath
            void saveAppStatus({ leftVolumeId: defaultVolumeId, leftPath: defaultPath })
        }
        if (rightVolumeId === unmountedId) {
            rightVolumeId = defaultVolumeId
            rightPath = defaultPath
            void saveAppStatus({ rightVolumeId: defaultVolumeId, rightPath: defaultPath })
        }

        // Refresh volume list
        volumes = await listVolumes()
    }

    onDestroy(() => {
        unlistenSettings?.()
        unlistenViewMode?.()
        unlistenVolumeUnmount?.()
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
            volumeId={leftVolumeId}
            volumePath={leftVolumePath}
            isFocused={focusedPane === 'left'}
            {showHiddenFiles}
            viewMode={leftViewMode}
            onPathChange={handleLeftPathChange}
            onVolumeChange={handleLeftVolumeChange}
            onRequestFocus={handleLeftFocus}
        />
        <FilePane
            bind:this={rightPaneRef}
            initialPath={rightPath}
            volumeId={rightVolumeId}
            volumePath={rightVolumePath}
            isFocused={focusedPane === 'right'}
            {showHiddenFiles}
            viewMode={rightViewMode}
            onPathChange={handleRightPathChange}
            onVolumeChange={handleRightVolumeChange}
            onRequestFocus={handleRightFocus}
        />
    {:else}
        <LoadingIcon />
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
</style>
