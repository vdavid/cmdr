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

    interface IndexStatusMeta {
        schemaVersion: string | null
        volumePath: string | null
        scanCompletedAt: string | null
        scanDurationMs: string | null
        totalEntries: string | null
        lastEventId: string | null
    }

    interface IndexDebugStatus {
        initialized: boolean
        scanning: boolean
        entriesScanned: number
        dirsFound: number
        indexStatus: IndexStatusMeta | null
        dbFileSize: number | null
        watcherActive: boolean
        liveEventCount: number
        mustScanCount: number
        mustScanRescansCompleted: number
        liveEntryCount: number | null
        liveDirCount: number | null
        dirsWithStats: number | null
        recentMustScanPaths: [string, string][]
    }

    interface IndexLogEntry {
        time: string
        event: string
        detail: string
    }

    /** Rolling event rate buckets: events per second for the last 60 seconds */
    const EVENT_RATE_BUCKETS = 60

    let pageElement: HTMLDivElement | undefined = $state()
    let isDarkMode = $state(true)
    let leftHistory = $state<NavigationHistory | null>(null)
    let rightHistory = $state<NavigationHistory | null>(null)
    let focusedPane = $state<'left' | 'right'>('left')
    let unlisten: (() => void) | undefined

    let toastCounter = $state(0)

    // Drive index state
    let debugStatus = $state<IndexDebugStatus | null>(null)
    let indexLog = $state<IndexLogEntry[]>([])
    let indexMessage = $state('')
    let indexPollInterval: ReturnType<typeof setInterval> | undefined
    let indexUnlisteners: (() => void)[] = []

    // Event rate tracking (frontend-side, from events we observe)
    let eventRateBuckets = $state<number[]>(new Array<number>(EVENT_RATE_BUCKETS).fill(0))
    let currentBucketEvents = 0
    let rateInterval: ReturnType<typeof setInterval> | undefined

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
        await pollDebugStatus()
        indexPollInterval = setInterval(() => {
            void pollDebugStatus()
        }, 2000)

        // Event rate bucket rotation: shift every second
        rateInterval = setInterval(() => {
            eventRateBuckets = [...eventRateBuckets.slice(1), currentBucketEvents]
            currentBucketEvents = 0
        }, 1000)

        try {
            const { listen: listenEvent } = await import('@tauri-apps/api/event')
            const eventNames = [
                'index-scan-started',
                'index-scan-progress',
                'index-scan-complete',
                'index-dir-updated',
                'index-replay-progress',
                'index-aggregation-progress',
                'index-aggregation-complete',
                'index-rescan-notification',
            ]
            for (const name of eventNames) {
                const unsub = await listenEvent(name, (event: { payload: unknown }) => {
                    appendIndexLog(name, JSON.stringify(event.payload))
                    currentBucketEvents++
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
        if (rateInterval) clearInterval(rateInterval)
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

    async function pollDebugStatus() {
        try {
            const { invoke } = await import('@tauri-apps/api/core')
            debugStatus = await invoke<IndexDebugStatus>('get_index_debug_status')
        } catch {
            // Indexing not available
        }
    }

    function appendIndexLog(event: string, detail: string) {
        const now = new Date()
        const time = `${String(now.getHours()).padStart(2, '0')}:${String(now.getMinutes()).padStart(2, '0')}:${String(now.getSeconds()).padStart(2, '0')}.${String(now.getMilliseconds()).padStart(3, '0')}`
        indexLog = [...indexLog.slice(-99), { time, event, detail }]
    }

    async function handleStartScan() {
        try {
            const { invoke } = await import('@tauri-apps/api/core')
            await invoke('start_drive_index', { volumeId: 'root' })
            indexMessage = 'Scan started'
        } catch (error) {
            indexMessage = `Couldn't start: ${String(error)}`
        }
    }

    async function handleClearIndex() {
        try {
            const { invoke } = await import('@tauri-apps/api/core')
            await invoke('clear_drive_index')
            indexMessage = 'Index cleared'
            await pollDebugStatus()
        } catch (error) {
            indexMessage = `Couldn't clear: ${String(error)}`
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

    function formatCount(n: number | null | undefined): string {
        if (n === null || n === undefined) return 'N/A'
        return n.toLocaleString('en-US')
    }

    /** Format a history entry for display */
    function formatEntry(entry: HistoryEntry): string {
        if (entry.networkHost) {
            return `${entry.networkHost.name} (${entry.networkHost.hostname})`
        }
        const parts = entry.path.split('/')
        const lastPart = parts[parts.length - 1] || parts[parts.length - 2] || entry.path
        if (entry.volumeId !== 'root' && entry.volumeId !== 'network') {
            const volumeName = entry.volumeId.split('/').pop() ?? entry.volumeId
            return `[${volumeName}] ${lastPart}`
        }
        return lastPart || entry.path
    }

    /** Build sparkline SVG path from event rate buckets */
    function sparklinePath(buckets: number[]): string {
        const max = Math.max(...buckets, 1)
        const w = 100
        const h = 100
        const step = w / (buckets.length - 1)
        const points = buckets.map((v, i) => `${(i * step).toFixed(1)},${(h - (v / max) * h).toFixed(1)}`)
        return `M${points.join(' L')}`
    }

    /** Build sparkline area fill path */
    function sparklineArea(buckets: number[]): string {
        const max = Math.max(...buckets, 1)
        const w = 100
        const h = 100
        const step = w / (buckets.length - 1)
        const points = buckets.map((v, i) => `${(i * step).toFixed(1)},${(h - (v / max) * h).toFixed(1)}`)
        return `M0,${String(h)} L${points.join(' L')} L${String(w)},${String(h)} Z`
    }

    const maxEventRate = $derived(Math.max(...eventRateBuckets, 1))
    const totalRecentEvents = $derived(eventRateBuckets.reduce((a: number, b: number) => a + b, 0))
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
        <button class="close-button" onclick={closeWindow} aria-label="Close">&times;</button>
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
                <!-- Scan + watcher status row -->
                <div class="index-status-row">
                    <div class="index-status">
                        {#if debugStatus === null}
                            <span class="status-badge neutral">Loading...</span>
                        {:else if debugStatus.scanning}
                            <span class="status-badge active">
                                <span class="spinner spinner-sm"></span>
                                Scanning {formatCount(debugStatus.entriesScanned)} entries
                            </span>
                        {:else if debugStatus.initialized}
                            <span class="status-badge ready">Idle</span>
                        {:else}
                            <span class="status-badge neutral">Not initialized</span>
                        {/if}
                    </div>
                    <div class="index-status">
                        {#if debugStatus?.watcherActive}
                            <span class="status-badge ready">Watcher on</span>
                        {:else}
                            <span class="status-badge neutral">Watcher off</span>
                        {/if}
                    </div>
                </div>

                <!-- Action buttons -->
                <div class="index-actions">
                    <button class="index-button" onclick={handleStartScan}>Start scan</button>
                    <button class="index-button" onclick={handleClearIndex}>Clear index</button>
                    {#if indexMessage}
                        <span class="index-message">{indexMessage}</span>
                    {/if}
                </div>

                <!-- DB statistics -->
                {#if debugStatus?.initialized}
                    <div class="index-sub-header">Database</div>
                    <div class="index-meta">
                        <div class="index-meta-row">
                            <span class="index-meta-label">Entries</span>
                            <span class="index-meta-value">{formatCount(debugStatus.liveEntryCount)}</span>
                        </div>
                        <div class="index-meta-row">
                            <span class="index-meta-label">Directories</span>
                            <span class="index-meta-value">{formatCount(debugStatus.liveDirCount)}</span>
                        </div>
                        <div class="index-meta-row">
                            <span class="index-meta-label">Dirs with stats</span>
                            <span class="index-meta-value">{formatCount(debugStatus.dirsWithStats)}</span>
                        </div>
                        {#if debugStatus.liveDirCount !== null && debugStatus.dirsWithStats !== null}
                            <div class="index-meta-row">
                                <span class="index-meta-label">Dirs missing stats</span>
                                <span class="index-meta-value"
                                    >{formatCount(debugStatus.liveDirCount - debugStatus.dirsWithStats)}</span
                                >
                            </div>
                        {/if}
                        {#if debugStatus.indexStatus?.scanCompletedAt}
                            <div class="index-meta-row">
                                <span class="index-meta-label">Last scan</span>
                                <span class="index-meta-value">{debugStatus.indexStatus.scanCompletedAt}</span>
                            </div>
                        {/if}
                        {#if debugStatus.indexStatus?.scanDurationMs}
                            <div class="index-meta-row">
                                <span class="index-meta-label">Scan duration</span>
                                <span class="index-meta-value"
                                    >{formatDuration(debugStatus.indexStatus.scanDurationMs)}</span
                                >
                            </div>
                        {/if}
                        {#if debugStatus.dbFileSize !== null}
                            <div class="index-meta-row">
                                <span class="index-meta-label">Database size</span>
                                <span class="index-meta-value">{formatDbSize(debugStatus.dbFileSize)}</span>
                            </div>
                        {/if}
                    </div>
                {/if}

                <!-- Event statistics -->
                {#if debugStatus?.initialized}
                    <div class="index-sub-header">Event statistics</div>
                    <div class="index-meta">
                        <div class="index-meta-row">
                            <span class="index-meta-label">Live FS events</span>
                            <span class="index-meta-value">{formatCount(debugStatus.liveEventCount)}</span>
                        </div>
                        <div class="index-meta-row">
                            <span class="index-meta-label">MustScanSubDirs</span>
                            <span class="index-meta-value">{formatCount(debugStatus.mustScanCount)}</span>
                        </div>
                        <div class="index-meta-row">
                            <span class="index-meta-label">Rescans completed</span>
                            <span class="index-meta-value">{formatCount(debugStatus.mustScanRescansCompleted)}</span>
                        </div>
                    </div>
                {/if}

                <!-- Event rate sparkline -->
                <div class="index-sub-header">
                    Event rate
                    <span class="index-sub-header-detail">
                        (last 60s: {totalRecentEvents} events, peak {maxEventRate}/s)
                    </span>
                </div>
                <div class="sparkline-container">
                    <svg viewBox="0 0 100 100" preserveAspectRatio="none" class="sparkline-svg">
                        <path d={sparklineArea(eventRateBuckets)} class="sparkline-area" />
                        <path d={sparklinePath(eventRateBuckets)} class="sparkline-line" />
                    </svg>
                    <div class="sparkline-labels">
                        <span>0</span>
                        <span>{maxEventRate}/s</span>
                    </div>
                </div>

                <!-- MustScanSubDirs tracking -->
                {#if debugStatus && debugStatus.recentMustScanPaths.length > 0}
                    <div class="index-sub-header">Recent MustScanSubDirs paths</div>
                    <div class="must-scan-log">
                        {#each debugStatus.recentMustScanPaths as [time, path] (`${time}-${path}`)}
                            <div class="must-scan-entry">
                                <span class="index-log-time">{time}</span>
                                <span class="must-scan-path" use:tooltip={{ text: path, overflowOnly: true }}
                                    >{path}</span
                                >
                            </div>
                        {/each}
                    </div>
                {/if}

                <!-- Live event log -->
                <div class="index-sub-header">Event log</div>
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
                            addToast(`Success toast #${String(toastCounter)}`, { level: 'success' })
                        }}>Success</button
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
                            addToast(`Persistent success #${String(toastCounter)}`, {
                                dismissal: 'persistent',
                                level: 'success',
                            })
                        }}>Success</button
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

    .index-status-row {
        display: flex;
        gap: 8px;
        align-items: center;
    }

    .index-status {
        font-size: var(--font-size-sm);
    }

    .status-badge {
        display: inline-flex;
        align-items: center;
        gap: 5px;
        padding: 2px 8px;
        border-radius: var(--radius-sm);
        font-size: var(--font-size-xs);
        font-weight: 600;
    }

    .status-badge.active {
        background: color-mix(in srgb, var(--color-accent) 20%, transparent);
        color: var(--color-accent);
    }

    .status-badge.ready {
        background: color-mix(in srgb, #4caf50 20%, transparent);
        color: #4caf50;
    }

    .status-badge.neutral {
        background: var(--color-bg-tertiary);
        color: var(--color-text-tertiary);
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

    .index-sub-header {
        font-size: var(--font-size-xs);
        font-weight: 600;
        text-transform: uppercase;
        letter-spacing: 0.5px;
        color: var(--color-text-tertiary);
        margin-top: 4px;
    }

    .index-sub-header-detail {
        font-weight: 400;
        text-transform: none;
        letter-spacing: normal;
    }

    .index-meta {
        display: flex;
        flex-direction: column;
        gap: 3px;
        font-size: var(--font-size-sm);
    }

    .index-meta-row {
        display: flex;
        gap: 8px;
    }

    .index-meta-label {
        color: var(--color-text-tertiary);
        min-width: 120px;
    }

    .index-meta-value {
        color: var(--color-text-primary);
        font-family: var(--font-mono);
    }

    /* Sparkline chart */
    .sparkline-container {
        position: relative;
        height: 48px;
        background: var(--color-bg-primary);
        border-radius: var(--radius-sm);
        overflow: hidden;
    }

    .sparkline-svg {
        width: 100%;
        height: 100%;
    }

    .sparkline-line {
        fill: none;
        stroke: var(--color-accent);
        stroke-width: 1.5;
        vector-effect: non-scaling-stroke;
    }

    .sparkline-area {
        fill: color-mix(in srgb, var(--color-accent) 15%, transparent);
    }

    .sparkline-labels {
        position: absolute;
        top: 0;
        right: 4px;
        bottom: 0;
        display: flex;
        flex-direction: column-reverse;
        justify-content: space-between;
        font-size: 9px;
        font-family: var(--font-mono);
        color: var(--color-text-tertiary);
        pointer-events: none;
        padding: 2px 0;
    }

    /* MustScanSubDirs log */
    .must-scan-log {
        max-height: 120px;
        overflow-y: auto;
        background: var(--color-bg-primary);
        border-radius: var(--radius-sm);
        padding: 6px;
        font-size: var(--font-size-xs);
        font-family: var(--font-mono);
    }

    .must-scan-entry {
        display: flex;
        gap: 6px;
        padding: 1px 0;
        line-height: 1.4;
    }

    .must-scan-path {
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        color: var(--color-accent);
    }

    /* Event log */
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
