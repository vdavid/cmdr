<script lang="ts">
    import { onMount } from 'svelte'
    import FilePane from './FilePane.svelte'
    import { loadAppStatus, saveAppStatus } from '$lib/app-status-store'
    import { pathExists } from '$lib/tauri-commands'

    let leftPath = $state('~')
    let rightPath = $state('~')
    let focusedPane = $state<'left' | 'right'>('left')
    let initialized = $state(false)

    let containerElement: HTMLDivElement | undefined = $state()
    let leftPaneRef: FilePane | undefined = $state()
    let rightPaneRef: FilePane | undefined = $state()

    function handleLeftPathChange(path: string) {
        leftPath = path
        void saveAppStatus({ leftPath: path })
    }

    function handleRightPathChange(path: string) {
        rightPath = path
        void saveAppStatus({ rightPath: path })
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

    onMount(() => {
        // Load persisted state
        void loadAppStatus(pathExists).then((status) => {
            leftPath = status.leftPath
            rightPath = status.rightPath
            focusedPane = status.focusedPane
            initialized = true
        })

        // Add global keydown handler for Tab
        containerElement?.focus()
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
            onPathChange={handleLeftPathChange}
            onRequestFocus={handleLeftFocus}
        />
        <FilePane
            bind:this={rightPaneRef}
            initialPath={rightPath}
            isFocused={focusedPane === 'right'}
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
