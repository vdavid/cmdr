<script lang="ts">
    import { onMount } from 'svelte'
    import DualPaneExplorer from '$lib/file-explorer/DualPaneExplorer.svelte'
    import { showMainWindow } from '$lib/tauri-commands'

    interface StartupTimes {
        htmlParse: number
        loadingScreenRendered: number
        svelteMount?: number
    }

    onMount(() => {
        // T3: Svelte app mounted
        const times = (window as unknown as { __STARTUP_TIMES__?: StartupTimes }).__STARTUP_TIMES__
        if (times) {
            times.svelteMount = performance.now()

            // Log startup benchmark results
            console.log('[STARTUP BENCHMARK]')
            console.log(
                `  T1 → T2: ${(times.loadingScreenRendered - times.htmlParse).toFixed(1)}ms (HTML parse → loading screen)`,
            )
            console.log(
                `  T2 → T3: ${(times.svelteMount - times.loadingScreenRendered).toFixed(1)}ms (loading screen → Svelte mount)`,
            )
            console.log(`  Total:   ${times.svelteMount.toFixed(1)}ms (navigation start → Svelte mount)`)
        }

        // Hide loading screen
        const loadingScreen = document.getElementById('loading-screen')
        if (loadingScreen) {
            loadingScreen.style.display = 'none'
        }

        // Show window when ready
        void showMainWindow()

        // Suppress Cmd+A (select all) - always
        const handleKeyDown = (e: KeyboardEvent) => {
            if (e.metaKey && e.key === 'a') {
                e.preventDefault()
            }
            // Suppress Cmd+Opt+I (devtools) in production only
            if (!import.meta.env.DEV && e.metaKey && e.altKey && e.key === 'i') {
                e.preventDefault()
            }
        }

        // Suppress right-click context menu
        const handleContextMenu = (e: MouseEvent) => {
            e.preventDefault()
        }

        document.addEventListener('keydown', handleKeyDown)
        document.addEventListener('contextmenu', handleContextMenu)

        return () => {
            document.removeEventListener('keydown', handleKeyDown)
            document.removeEventListener('contextmenu', handleContextMenu)
        }
    })
</script>

<DualPaneExplorer />
