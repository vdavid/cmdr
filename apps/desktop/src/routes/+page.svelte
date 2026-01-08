<script lang="ts">
    import { onMount, onDestroy } from 'svelte'
    import DualPaneExplorer from '$lib/file-explorer/DualPaneExplorer.svelte'
    import FullDiskAccessPrompt from '$lib/onboarding/FullDiskAccessPrompt.svelte'
    import { showMainWindow, checkFullDiskAccess } from '$lib/tauri-commands'
    import { loadSettings, saveSettings } from '$lib/settings-store'

    let showFdaPrompt = $state(false)
    let fdaWasRevoked = $state(false)
    let showApp = $state(false)

    // Event handlers stored for cleanup
    let handleKeyDown: ((e: KeyboardEvent) => void) | undefined
    let handleContextMenu: ((e: MouseEvent) => void) | undefined

    onMount(async () => {
        // Hide loading screen
        const loadingScreen = document.getElementById('loading-screen')
        if (loadingScreen) {
            loadingScreen.style.display = 'none'
        }

        // Check FDA status
        const settings = await loadSettings()
        const hasFda = await checkFullDiskAccess()

        if (hasFda) {
            // Already have FDA - ensure setting reflects this
            if (settings.fullDiskAccessChoice !== 'allow') {
                await saveSettings({ fullDiskAccessChoice: 'allow' })
            }
            showApp = true
        } else if (settings.fullDiskAccessChoice === 'notAskedYet') {
            // First time - show onboarding
            showFdaPrompt = true
        } else if (settings.fullDiskAccessChoice === 'allow') {
            // User previously allowed but FDA was revoked - show prompt with different text
            showFdaPrompt = true
            fdaWasRevoked = true
        } else {
            // User explicitly denied - proceed without prompting
            showApp = true
        }

        // Show window when ready
        void showMainWindow()

        // Suppress Cmd+A (select all) - always
        handleKeyDown = (e: KeyboardEvent) => {
            if (e.metaKey && e.key === 'a') {
                e.preventDefault()
            }
            // Suppress Cmd+Opt+I (devtools) in production only
            if (!import.meta.env.DEV && e.metaKey && e.altKey && e.key === 'i') {
                e.preventDefault()
            }
        }

        // Suppress right-click context menu
        handleContextMenu = (e: MouseEvent) => {
            e.preventDefault()
        }

        document.addEventListener('keydown', handleKeyDown)
        document.addEventListener('contextmenu', handleContextMenu)
    })

    onDestroy(() => {
        if (handleKeyDown) {
            document.removeEventListener('keydown', handleKeyDown)
        }
        if (handleContextMenu) {
            document.removeEventListener('contextmenu', handleContextMenu)
        }
    })

    function handleFdaComplete() {
        showFdaPrompt = false
        showApp = true
    }
</script>

{#if showFdaPrompt}
    <FullDiskAccessPrompt onComplete={handleFdaComplete} wasRevoked={fdaWasRevoked} />
{:else if showApp}
    <DualPaneExplorer />
{/if}
