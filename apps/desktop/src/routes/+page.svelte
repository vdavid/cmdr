<script lang="ts">
    import { onMount, onDestroy } from 'svelte'
    import DualPaneExplorer from '$lib/file-explorer/DualPaneExplorer.svelte'
    import FullDiskAccessPrompt from '$lib/onboarding/FullDiskAccessPrompt.svelte'
    import ExpirationModal from '$lib/licensing/ExpirationModal.svelte'
    import AboutWindow from '$lib/licensing/AboutWindow.svelte'
    import { showMainWindow, checkFullDiskAccess, listen, type UnlistenFn } from '$lib/tauri-commands'
    import { loadSettings, saveSettings } from '$lib/settings-store'
    import { hideExpirationModal } from '$lib/licensing-store.svelte'

    let showFdaPrompt = $state(false)
    let fdaWasRevoked = $state(false)
    let showApp = $state(false)
    let showExpiredModal = $state(false)
    let expiredOrgName = $state<string | null>(null)
    let expiredAt = $state<string>('')
    let showAboutWindow = $state(false)

    // Event handlers stored for cleanup
    let handleKeyDown: ((e: KeyboardEvent) => void) | undefined
    let handleContextMenu: ((e: MouseEvent) => void) | undefined
    let unlistenShowAbout: UnlistenFn | undefined

    onMount(async () => {
        // Hide loading screen
        const loadingScreen = document.getElementById('loading-screen')
        if (loadingScreen) {
            loadingScreen.style.display = 'none'
        }

        // Load license status first (non-blocking - don't prevent app load on failure)
        try {
            const { loadLicenseStatus, triggerValidationIfNeeded } = await import('$lib/licensing-store.svelte')
            let licenseStatus = await loadLicenseStatus()

            // Trigger background validation if needed
            const validatedStatus = await triggerValidationIfNeeded()
            if (validatedStatus) {
                licenseStatus = validatedStatus
            }

            // Check if we need to show expiration modal
            if (licenseStatus.type === 'expired' && licenseStatus.showModal) {
                showExpiredModal = true
                expiredOrgName = licenseStatus.organizationName
                expiredAt = licenseStatus.expiredAt
            }
        } catch {
            // License check failed (expected in E2E tests without Tauri backend)
            // App continues without license features
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

        // Listen for show-about event from menu
        try {
            unlistenShowAbout = await listen('show-about', () => {
                showAboutWindow = true
            })
        } catch {
            // Not in Tauri environment
        }

        // Global keyboard shortcuts
        handleKeyDown = (e: KeyboardEvent) => {
            // Suppress Cmd+A (select all) - always
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
        if (unlistenShowAbout) {
            unlistenShowAbout()
        }
    })

    function handleFdaComplete() {
        showFdaPrompt = false
        showApp = true
    }

    function handleExpirationModalClose() {
        showExpiredModal = false
        hideExpirationModal()
    }

    function handleAboutClose() {
        showAboutWindow = false
    }
</script>

{#if showAboutWindow}
    <AboutWindow onClose={handleAboutClose} />
{/if}

{#if showExpiredModal}
    <ExpirationModal organizationName={expiredOrgName} {expiredAt} onClose={handleExpirationModalClose} />
{/if}

{#if showFdaPrompt}
    <FullDiskAccessPrompt onComplete={handleFdaComplete} wasRevoked={fdaWasRevoked} />
{:else if showApp}
    <DualPaneExplorer />
{/if}
