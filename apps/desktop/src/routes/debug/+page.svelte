<script lang="ts">
    import { onMount, onDestroy, tick } from 'svelte'
    import { tooltip } from '$lib/tooltip/tooltip'
    import { addToast, dismissTransientToasts, clearAllToasts, getToasts } from '$lib/ui/toast'
    import ToastContainer from '$lib/ui/toast/ToastContainer.svelte'

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

    interface IndexStatus {
        initialized: boolean
        scanning: boolean
        entriesScanned: number
        dirsFound: number
        indexStatus: {
            schemaVersion: string | null
            volumePath: string | null
            scanCompletedAt: string | null
            scanDurationMs: string | null
            totalEntries: string | null
            lastEventId: string | null
        } | null
        dbFileSize: number | null
    }

    interface IndexLogEntry {
        time: string
        event: string
        detail: string
    }

    let pageElement: HTMLDivElement | undefined = $state()
    let isDarkMode = $state(true)
    let leftHistory = $state<NavigationHistory | null>(null)
    let rightHistory = $state<NavigationHistory | null>(null)
    let focusedPane = $state<'left' | 'right'>('left')
    let unlisten: (() => void) | undefined

    let toastCounter = $state(0)

    // Drive index state
    let indexStatus = $state<IndexStatus | null>(null)
    let indexLog = $state<IndexLogEntry[]>([])
    let indexMessage = $state('')
    let indexPollInterval: ReturnType<typeof setInterval> | undefined
    let indexUnlisteners: (() => void)[] = []

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

        // Drive index: poll status and set up event listeners
        await pollIndexStatus()
        indexPollInterval = setInterval(() => {
            void pollIndexStatus()
        }, 2000)

        try {
            const { listen: listenEvent } = await import('@tauri-apps/api/event')
            const eventNames = [
                'index-scan-started',
                'index-scan-progress',
                'index-scan-complete',
                'index-dir-updated',
                'index-replay-progress',
            ]
            for (const name of eventNames) {
                const unsub = await listenEvent(name, (event: { payload: unknown }) => {
                    appendIndexLog(name, JSON.stringify(event.payload))
                })
                indexUnlisteners.push(unsub)
            }
        } catch {
            // Not in Tauri environment
        }
    })

    onDestroy(() => {
        unlisten?.()
        if (indexPollInterval) clearInterval(indexPollInterval)
        for (const unsub of indexUnlisteners) unsub()
        indexUnlisteners = []
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

    // ==== Drive index helpers ====

    async function pollIndexStatus() {
        try {
            const { invoke } = await import('@tauri-apps/api/core')
            indexStatus = await invoke<IndexStatus>('get_index_status')
        } catch {
            // Indexing not available
        }
    }

    function appendIndexLog(event: string, detail: string) {
        const now = new Date()
        const time = `${String(now.getHours()).padStart(2, '0')}:${String(now.getMinutes()).padStart(2, '0')}:${String(now.getSeconds()).padStart(2, '0')}.${String(now.getMilliseconds()).padStart(3, '0')}`
        indexLog = [...indexLog.slice(-49), { time, event, detail }]
    }

    async function handleStartScan() {
        try {
            const { invoke } = await import('@tauri-apps/api/core')
            await invoke('start_drive_index', { volumeId: 'root' })
            indexMessage = 'Scan started'
        } catch (error) {
            indexMessage = `Error: ${String(error)}`
        }
    }

    async function handleClearIndex() {
        try {
            const { invoke } = await import('@tauri-apps/api/core')
            await invoke('clear_drive_index')
            indexMessage = 'Index cleared'
            await pollIndexStatus()
        } catch (error) {
            indexMessage = `Error: ${String(error)}`
        }
    }

    function formatDbSize(bytes: number | null): string {
        if (bytes === null) return 'N/A'
        if (bytes < 1024) return `${String(bytes)} B`
        if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
        return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
    }

    function formatDuration(ms: string | null): string {
        if (ms === null) return 'N/A'
        const millis = parseInt(ms, 10)
        if (isNaN(millis)) return ms
        if (millis < 1000) return `${String(millis)} ms`
        return `${(millis / 1000).toFixed(1)} s`
    }

    function formatEntryCount(n: number): string {
        return n.toLocaleString('en-US')
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
    <ToastContainer />
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
            <h2>Drive index</h2>
            <div class="index-panel">
                <!-- Status display -->
                <div class="index-status">
                    {#if indexStatus === null}
                        <span class="index-status-text">Loading...</span>
                    {:else if indexStatus.scanning}
                        <span class="index-status-text scanning">
                            <span class="spinner spinner-sm"></span>
                            Scanning... {formatEntryCount(indexStatus.entriesScanned)} / ? entries
                        </span>
                    {:else if indexStatus.initialized}
                        <span class="index-status-text ready">
                            Ready: {formatEntryCount(indexStatus.entriesScanned)} entries, {formatEntryCount(
                                indexStatus.dirsFound,
                            )} folders
                        </span>
                    {:else}
                        <span class="index-status-text">Not initialized</span>
                    {/if}
                </div>

                <!-- Action buttons -->
                <div class="index-actions">
                    <button class="index-button" onclick={handleStartScan}>Start scan</button>
                    <button class="index-button" onclick={handleClearIndex}>Clear index</button>
                    {#if indexMessage}
                        <span class="index-message">{indexMessage}</span>
                    {/if}
                </div>

                <!-- Last scan info -->
                {#if indexStatus?.indexStatus}
                    <div class="index-meta">
                        {#if indexStatus.indexStatus.scanCompletedAt}
                            <div class="index-meta-row">
                                <span class="index-meta-label">Last scan</span>
                                <span class="index-meta-value">{indexStatus.indexStatus.scanCompletedAt}</span>
                            </div>
                        {/if}
                        {#if indexStatus.indexStatus.scanDurationMs}
                            <div class="index-meta-row">
                                <span class="index-meta-label">Duration</span>
                                <span class="index-meta-value"
                                    >{formatDuration(indexStatus.indexStatus.scanDurationMs)}</span
                                >
                            </div>
                        {/if}
                        {#if indexStatus.indexStatus.totalEntries}
                            <div class="index-meta-row">
                                <span class="index-meta-label">Total entries</span>
                                <span class="index-meta-value"
                                    >{formatEntryCount(parseInt(indexStatus.indexStatus.totalEntries, 10))}</span
                                >
                            </div>
                        {/if}
                        {#if indexStatus.dbFileSize !== null}
                            <div class="index-meta-row">
                                <span class="index-meta-label">Database size</span>
                                <span class="index-meta-value">{formatDbSize(indexStatus.dbFileSize)}</span>
                            </div>
                        {/if}
                    </div>
                {/if}

                <!-- Live event log -->
                <div class="index-log-header">Events</div>
                <div class="index-log">
                    {#if indexLog.length === 0}
                        <p class="no-history">No events yet</p>
                    {:else}
                        {#each indexLog as entry (entry.time + entry.event)}
                            <div class="index-log-entry">
                                <span class="index-log-time">{entry.time}</span>
                                <span class="index-log-event">{entry.event}</span>
                                <span class="index-log-detail" use:tooltip={{ text: entry.detail, overflowOnly: true }}
                                    >{entry.detail}</span
                                >
                            </div>
                        {/each}
                    {/if}
                </div>
            </div>
        </section>

        <section class="debug-section">
            <h2>Toast notifications</h2>
            <div class="toast-debug-panel">
                <div class="toast-debug-row">
                    <span class="toast-debug-label">Transient</span>
                    <button
                        class="index-button"
                        onclick={() => {
                            toastCounter++
                            addToast(`Info toast #${String(toastCounter)}`)
                        }}>Info</button
                    >
                    <button
                        class="index-button"
                        onclick={() => {
                            toastCounter++
                            addToast(`Warning toast #${String(toastCounter)}`, { level: 'warn' })
                        }}>Warn</button
                    >
                    <button
                        class="index-button"
                        onclick={() => {
                            toastCounter++
                            addToast(`Error toast #${String(toastCounter)}`, { level: 'error' })
                        }}>Error</button
                    >
                </div>
                <div class="toast-debug-row">
                    <span class="toast-debug-label">Persistent</span>
                    <button
                        class="index-button"
                        onclick={() => {
                            toastCounter++
                            addToast(`Persistent info #${String(toastCounter)}`, { dismissal: 'persistent' })
                        }}>Info</button
                    >
                    <button
                        class="index-button"
                        onclick={() => {
                            toastCounter++
                            addToast(`Persistent warning #${String(toastCounter)}`, {
                                dismissal: 'persistent',
                                level: 'warn',
                            })
                        }}>Warn</button
                    >
                    <button
                        class="index-button"
                        onclick={() => {
                            toastCounter++
                            addToast(`Persistent error #${String(toastCounter)}`, {
                                dismissal: 'persistent',
                                level: 'error',
                            })
                        }}>Error</button
                    >
                </div>
                <div class="toast-debug-row">
                    <span class="toast-debug-label">Dedup</span>
                    <button
                        class="index-button"
                        onclick={() => {
                            toastCounter++
                            addToast(`Dedup toast (always replaces) #${String(toastCounter)}`, { id: 'dedup-test' })
                        }}>Replace (same ID)</button
                    >
                </div>
                <div class="toast-debug-row">
                    <span class="toast-debug-label">Custom timeout</span>
                    <button
                        class="index-button"
                        onclick={() => {
                            toastCounter++
                            addToast(`10s timeout #${String(toastCounter)}`, { timeoutMs: 10000 })
                        }}>10 seconds</button
                    >
                </div>
                <div class="toast-debug-row">
                    <span class="toast-debug-label">Bulk actions</span>
                    <button class="index-button" onclick={dismissTransientToasts}>Dismiss transient</button>
                    <button class="index-button" onclick={clearAllToasts}>Clear all</button>
                </div>
                <div class="toast-debug-row">
                    <span class="toast-debug-label">Active</span>
                    <span class="toast-debug-count">{getToasts().length} toasts</span>
                </div>
            </div>
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
                                    <span class="history-path" use:tooltip={{ text: entry.path, overflowOnly: true }}
                                        >{formatEntry(entry)}</span
                                    >
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
                                    <span class="history-path" use:tooltip={{ text: entry.path, overflowOnly: true }}
                                        >{formatEntry(entry)}</span
                                    >
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
        border-bottom: 1px solid var(--color-border-strong);
        /* Allow dragging the window from header */
        -webkit-app-region: drag;
    }

    .debug-header h1 {
        margin: 0;
        font-size: var(--font-size-md);
        font-weight: 600;
        color: var(--color-text-primary);
    }

    .close-button {
        -webkit-app-region: no-drag;
        background: none;
        border: none;
        color: var(--color-text-secondary);
        font-size: var(--font-size-xl);
        padding: 2px 8px;
        line-height: 1;
        border-radius: var(--radius-sm);
    }

    .close-button:hover {
        background: var(--color-bg-tertiary);
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
        margin: 0 0 var(--spacing-md);
        font-size: var(--font-size-sm);
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
        border-radius: var(--radius-md);
    }

    .toggle-row:hover {
        background: var(--color-bg-tertiary);
    }

    .toggle-row span {
        font-size: var(--font-size-md);
        color: var(--color-text-primary);
    }

    .toggle-checkbox {
        width: 18px;
        height: 18px;
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
        border-radius: var(--radius-md);
        padding: 8px;
        min-width: 0;
    }

    .history-pane.focused {
        outline: 2px solid var(--color-accent);
    }

    .history-pane h3 {
        margin: 0 0 var(--spacing-sm);
        font-size: var(--font-size-sm);
        font-weight: 600;
        color: var(--color-text-secondary);
        text-transform: uppercase;
    }

    .history-list {
        list-style: none;
        margin: 0;
        padding: 0;
        font-size: var(--font-size-sm);
        font-family: var(--font-mono);
    }

    .history-list li {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
        padding: 3px 4px;
        border-radius: var(--radius-sm);
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
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
        font-style: italic;
    }

    /* Drive index styles */
    .index-panel {
        background: var(--color-bg-secondary);
        border-radius: var(--radius-md);
        padding: 12px;
        display: flex;
        flex-direction: column;
        gap: 10px;
    }

    .index-status {
        font-size: var(--font-size-sm);
    }

    .index-status-text {
        color: var(--color-text-secondary);
    }

    .index-status-text.scanning {
        display: inline-flex;
        align-items: center;
        gap: 6px;
        color: var(--color-accent);
    }

    .index-status-text.ready {
        color: var(--color-text-primary);
    }

    .index-actions {
        display: flex;
        align-items: center;
        gap: 8px;
    }

    .index-button {
        padding: 4px 12px;
        font-size: var(--font-size-sm);
        font-family: var(--font-system), sans-serif;
        background: var(--color-bg-tertiary);
        color: var(--color-text-primary);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
    }

    .index-button:hover {
        background: var(--color-bg-primary);
    }

    .index-message {
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
    }

    .index-meta {
        display: flex;
        flex-direction: column;
        gap: 4px;
        font-size: var(--font-size-sm);
    }

    .index-meta-row {
        display: flex;
        gap: 8px;
    }

    .index-meta-label {
        color: var(--color-text-tertiary);
        min-width: 100px;
    }

    .index-meta-value {
        color: var(--color-text-primary);
        font-family: var(--font-mono);
    }

    .index-log-header {
        font-size: var(--font-size-xs);
        font-weight: 600;
        text-transform: uppercase;
        letter-spacing: 0.5px;
        color: var(--color-text-tertiary);
    }

    .index-log {
        max-height: 200px;
        overflow-y: auto;
        background: var(--color-bg-primary);
        border-radius: var(--radius-sm);
        padding: 6px;
        font-size: var(--font-size-xs);
        font-family: var(--font-mono);
    }

    .index-log-entry {
        display: flex;
        gap: 6px;
        padding: 2px 0;
        line-height: 1.4;
    }

    .index-log-time {
        flex-shrink: 0;
        color: var(--color-text-tertiary);
    }

    .index-log-event {
        flex-shrink: 0;
        color: var(--color-accent);
    }

    .index-log-detail {
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        color: var(--color-text-secondary);
    }

    /* Toast debug styles */
    .toast-debug-panel {
        background: var(--color-bg-secondary);
        border-radius: var(--radius-md);
        padding: 12px;
        display: flex;
        flex-direction: column;
        gap: 8px;
    }

    .toast-debug-row {
        display: flex;
        align-items: center;
        gap: 8px;
    }

    .toast-debug-label {
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
        min-width: 110px;
    }

    .toast-debug-count {
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
        font-family: var(--font-mono);
    }
</style>
