<script lang="ts">
    import { onMount, onDestroy, tick } from 'svelte'

    interface HistoryEntry {
        volumeId: string
        path: string
        networkHost?: { name: string; hostname: string }
    }

    interface NavigationHistory {
        stack: HistoryEntry[]
        currentIndex: number
    }

    interface HistoryPayload {
        left: NavigationHistory
        right: NavigationHistory
        focusedPane: 'left' | 'right'
    }

    let pageElement: HTMLDivElement | undefined = $state()
    let isDarkMode = $state(true)
    let leftHistory = $state<NavigationHistory | null>(null)
    let rightHistory = $state<NavigationHistory | null>(null)
    let focusedPane = $state<'left' | 'right'>('left')
    let unlisten: (() => void) | undefined

    onMount(async () => {
        // Hide the loading screen
        const loadingScreen = document.getElementById('loading-screen')
        if (loadingScreen) {
            loadingScreen.style.display = 'none'
        }

        // Focus the page so keyboard events work immediately
        void tick().then(() => {
            pageElement?.focus()
        })

        // Detect current system preference
        if (typeof window !== 'undefined') {
            isDarkMode = window.matchMedia('(prefers-color-scheme: dark)').matches
        }

        // Try to get current app theme setting
        try {
            const { getCurrentWindow } = await import('@tauri-apps/api/window')
            const theme = await getCurrentWindow().theme()
            if (theme) {
                isDarkMode = theme === 'dark'
            }
        } catch {
            // Not in Tauri environment or theme not set
        }

        // Listen for history updates from main window
        try {
            const { listen } = await import('@tauri-apps/api/event')
            unlisten = await listen<HistoryPayload>('debug-history', (event) => {
                leftHistory = event.payload.left
                rightHistory = event.payload.right
                focusedPane = event.payload.focusedPane
            })
        } catch {
            // Not in Tauri environment
        }
    })

    onDestroy(() => {
        unlisten?.()
    })

    async function handleThemeToggle() {
        isDarkMode = !isDarkMode
        try {
            const { setTheme } = await import('@tauri-apps/api/app')
            await setTheme(isDarkMode ? 'dark' : 'light')
        } catch (error) {
            // eslint-disable-next-line no-console -- Debug window is dev-only
            console.error('Failed to set theme:', error)
        }
    }

    function handleKeydown(event: KeyboardEvent) {
        if (event.key === 'Escape') {
            void closeWindow()
        }
    }

    async function closeWindow() {
        try {
            const { getCurrentWindow } = await import('@tauri-apps/api/window')
            await getCurrentWindow().close()
        } catch {
            // Not in Tauri environment
        }
    }

    /** Format a history entry for display */
    function formatEntry(entry: HistoryEntry): string {
        if (entry.networkHost) {
            return `${entry.networkHost.name} (${entry.networkHost.hostname})`
        }
        // Show just the last part of the path for readability
        const parts = entry.path.split('/')
        const lastPart = parts[parts.length - 1] || parts[parts.length - 2] || entry.path
        if (entry.volumeId !== 'root' && entry.volumeId !== 'network') {
            const volumeName = entry.volumeId.split('/').pop() ?? entry.volumeId
            return `[${volumeName}] ${lastPart}`
        }
        return lastPart || entry.path
    }
</script>

<div
    bind:this={pageElement}
    class="debug-container"
    role="dialog"
    aria-label="Debug window"
    tabindex="-1"
    onkeydown={handleKeydown}
>
    <div class="debug-header">
        <h1>Debug</h1>
        <button class="close-button" onclick={closeWindow} aria-label="Close">×</button>
    </div>

    <div class="debug-content">
        <section class="debug-section">
            <h2>Appearance</h2>
            <label class="toggle-row">
                <span>Dark mode</span>
                <input type="checkbox" checked={isDarkMode} onchange={handleThemeToggle} class="toggle-checkbox" />
            </label>
        </section>

        <section class="debug-section">
            <h2>Navigation history</h2>
            <div class="history-panes">
                <div class="history-pane" class:focused={focusedPane === 'left'}>
                    <h3>Left pane</h3>
                    {#if leftHistory}
                        <ul class="history-list">
                            {#each leftHistory.stack as entry, i (i)}
                                {@const isCurrent = i === leftHistory.currentIndex}
                                {@const isFuture = i > leftHistory.currentIndex}
                                {@const arrow = isCurrent ? '→' : i < leftHistory.currentIndex ? '←' : '↓'}
                                <li class:current={isCurrent} class:future={isFuture}>
                                    <span class="history-index">{arrow}</span>
                                    <span class="history-path" title={entry.path}>{formatEntry(entry)}</span>
                                </li>
                            {/each}
                        </ul>
                    {:else}
                        <p class="no-history">No history yet</p>
                    {/if}
                </div>
                <div class="history-pane" class:focused={focusedPane === 'right'}>
                    <h3>Right pane</h3>
                    {#if rightHistory}
                        <ul class="history-list">
                            {#each rightHistory.stack as entry, i (i)}
                                {@const isCurrent = i === rightHistory.currentIndex}
                                {@const isFuture = i > rightHistory.currentIndex}
                                {@const arrow = isCurrent ? '→' : i < rightHistory.currentIndex ? '←' : '↓'}
                                <li class:current={isCurrent} class:future={isFuture}>
                                    <span class="history-index">{arrow}</span>
                                    <span class="history-path" title={entry.path}>{formatEntry(entry)}</span>
                                </li>
                            {/each}
                        </ul>
                    {:else}
                        <p class="no-history">No history yet</p>
                    {/if}
                </div>
            </div>
        </section>
    </div>
</div>

<style>
    .debug-container {
        display: flex;
        flex-direction: column;
        height: 100vh;
        background: var(--color-bg-primary);
        color: var(--color-text-primary);
        font-family: var(--font-system), sans-serif;
        outline: none;
    }

    .debug-header {
        display: flex;
        align-items: center;
        justify-content: space-between;
        padding: 12px 16px;
        background: var(--color-bg-secondary);
        border-bottom: 1px solid var(--color-border-primary);
        /* Allow dragging the window from header */
        -webkit-app-region: drag;
    }

    .debug-header h1 {
        margin: 0;
        font-size: 14px;
        font-weight: 600;
        color: var(--color-text-primary);
    }

    .close-button {
        -webkit-app-region: no-drag;
        background: none;
        border: none;
        color: var(--color-text-secondary);
        font-size: 20px;
        cursor: pointer;
        padding: 2px 8px;
        line-height: 1;
        border-radius: 4px;
    }

    .close-button:hover {
        background: var(--color-button-hover);
        color: var(--color-text-primary);
    }

    .debug-content {
        flex: 1;
        padding: 16px;
        overflow-y: auto;
    }

    .debug-section {
        margin-bottom: 24px;
    }

    .debug-section h2 {
        margin: 0 0 12px;
        font-size: 12px;
        font-weight: 600;
        text-transform: uppercase;
        letter-spacing: 0.5px;
        color: var(--color-text-secondary);
    }

    .toggle-row {
        display: flex;
        align-items: center;
        justify-content: space-between;
        padding: 8px 12px;
        background: var(--color-bg-secondary);
        border-radius: 6px;
        cursor: pointer;
    }

    .toggle-row:hover {
        background: var(--color-bg-tertiary);
    }

    .toggle-row span {
        font-size: 13px;
        color: var(--color-text-primary);
    }

    .toggle-checkbox {
        width: 18px;
        height: 18px;
        cursor: pointer;
        accent-color: var(--color-accent);
    }

    /* History styles */
    .history-panes {
        display: flex;
        gap: 12px;
    }

    .history-pane {
        flex: 1;
        background: var(--color-bg-secondary);
        border-radius: 6px;
        padding: 8px;
        min-width: 0;
    }

    .history-pane.focused {
        outline: 2px solid var(--color-accent);
    }

    .history-pane h3 {
        margin: 0 0 8px;
        font-size: 11px;
        font-weight: 600;
        color: var(--color-text-secondary);
        text-transform: uppercase;
    }

    .history-list {
        list-style: none;
        margin: 0;
        padding: 0;
        font-size: 11px;
        font-family: var(--font-mono);
    }

    .history-list li {
        display: flex;
        align-items: center;
        gap: 6px;
        padding: 3px 4px;
        border-radius: 3px;
        color: var(--color-text-secondary);
    }

    .history-list li.current {
        background: var(--color-bg-tertiary);
        color: var(--color-text-primary);
        font-weight: 600;
    }

    .history-list li.future {
        opacity: 0.5;
    }

    .history-index {
        flex-shrink: 0;
        width: 12px;
        text-align: center;
    }

    .history-path {
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .no-history {
        margin: 0;
        font-size: 11px;
        color: var(--color-text-muted);
        font-style: italic;
    }
</style>
