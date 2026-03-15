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

    interface PhaseRecord {
        phase: 'replaying' | 'scanning' | 'aggregating' | 'reconciling' | 'live' | 'idle'
        startedAt: string
        durationMs: number | null
        trigger: string
        stats: [string, string][]
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
        activityPhase: 'replaying' | 'scanning' | 'aggregating' | 'reconciling' | 'live' | 'idle'
        phaseStartedAt: string
        phaseDurationMs: number
        phaseHistory: PhaseRecord[]
        verifying: boolean
        dbMainSize: number | null
        dbWalSize: number | null
        dbPageCount: number | null
        dbFreelistCount: number | null
    }

    let pageElement: HTMLDivElement | undefined = $state()
    let isDarkMode = $state(true)
    let leftHistory = $state<NavigationHistory | null>(null)
    let rightHistory = $state<NavigationHistory | null>(null)
    let focusedPane = $state<'left' | 'right'>('left')
    let unlisten: (() => void) | undefined

    let toastCounter = $state(0)

    // Drive index state
    let debugStatus = $state<IndexDebugStatus | null>(null)
    let indexMessage = $state('')
    let indexPollInterval: ReturnType<typeof setInterval> | undefined

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

        // Drive index: poll status
        await pollDebugStatus()
        indexPollInterval = setInterval(() => {
            void pollDebugStatus()
        }, 2000)
    })

    onDestroy(() => {
        unlisten?.()
        if (indexPollInterval) clearInterval(indexPollInterval)
        if (phaseTickInterval) clearInterval(phaseTickInterval)
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

    function formatTimestamp(unixStr: string | null): string {
        if (unixStr === null) return 'N/A'
        const unix = parseInt(unixStr, 10)
        if (isNaN(unix)) return unixStr
        const d = new Date(unix * 1000)
        const now = new Date()
        const diffMs = now.getTime() - d.getTime()
        const diffMins = Math.floor(diffMs / 60_000)
        const diffHours = Math.floor(diffMs / 3_600_000)
        const diffDays = Math.floor(diffMs / 86_400_000)

        const time = d.toLocaleTimeString('en-US', { hour: '2-digit', minute: '2-digit', hour12: false })
        const date = d.toLocaleDateString('en-US', { month: 'short', day: 'numeric' })

        let ago: string
        if (diffMins < 1) ago = 'just now'
        else if (diffMins < 60) ago = `${String(diffMins)}m ago`
        else if (diffHours < 24) ago = `${String(diffHours)}h ago`
        else ago = `${String(diffDays)}d ago`

        if (diffDays === 0) return `${time} (${ago})`
        return `${date} ${time} (${ago})`
    }

    // Phase timeline ticking duration
    let phaseDurationTick = $state(0)
    let lastPhaseDurationMs = $state(-1)
    let phaseTickInterval: ReturnType<typeof setInterval> | undefined

    // Start the 1-second tick for current phase duration
    onMount(() => {
        phaseTickInterval = setInterval(() => {
            phaseDurationTick++
        }, 1000)
    })

    // When phaseDurationMs changes from a poll, reset the tick counter
    $effect(() => {
        if (debugStatus?.phaseDurationMs !== undefined && debugStatus.phaseDurationMs !== lastPhaseDurationMs) {
            lastPhaseDurationMs = debugStatus.phaseDurationMs
            phaseDurationTick = 0
        }
    })

    const phaseTooltipMap: Record<string, string> = {
        replaying: 'Processing FSEvents journal from cold start',
        scanning: 'Full volume directory walk',
        aggregating: 'Computing directory sizes',
        reconciling: 'Replaying buffered events from during scan',
        live: 'Processing real-time filesystem events',
        idle: 'Waiting for filesystem changes',
    }

    type PhaseStyle = 'active' | 'ready' | 'neutral'

    function phaseStyle(phase: string): PhaseStyle {
        if (phase === 'live') return 'ready'
        if (phase === 'idle') return 'neutral'
        return 'active'
    }

    function isActivePhase(phase: string): boolean {
        return phase === 'scanning' || phase === 'replaying' || phase === 'aggregating' || phase === 'reconciling'
    }

    function formatPhaseDurationMs(ms: number): string {
        if (ms < 1000) return `${String(ms)}ms`
        if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`
        if (ms < 3_600_000) {
            const mins = Math.floor(ms / 60_000)
            const secs = Math.floor((ms % 60_000) / 1000)
            return `${String(mins)}m ${String(secs).padStart(2, '0')}s`
        }
        const hrs = Math.floor(ms / 3_600_000)
        const mins = Math.floor((ms % 3_600_000) / 60_000)
        return `${String(hrs)}h ${String(mins).padStart(2, '0')}m`
    }

    function formatPhaseStats(stats: [string, string][]): string {
        const map = new Map(stats)
        const raw = map.get('raw_events')
        const unique = map.get('unique_events')
        const dedup = map.get('dedup_pct')
        if (raw && unique && dedup) {
            return `${Number(raw).toLocaleString('en-US')} raw → ${Number(unique).toLocaleString('en-US')} unique (${dedup}% dedup)`
        }
        const entries = map.get('total_entries')
        const dirs = map.get('total_dirs')
        if (entries && dirs) {
            return `${Number(entries).toLocaleString('en-US')} entries, ${Number(dirs).toLocaleString('en-US')} dirs`
        }
        if (stats.length === 0) return ''
        return stats.map(([k, v]) => `${k}: ${v}`).join(', ')
    }

    function currentPhaseLiveStat(status: IndexDebugStatus): string {
        if (status.activityPhase === 'scanning') return `${formatCount(status.entriesScanned)} entries scanned`
        if (status.activityPhase === 'live') return `${formatCount(status.liveEventCount)} live events`
        return ''
    }

    const currentPhaseDurationMs = $derived(debugStatus ? debugStatus.phaseDurationMs + phaseDurationTick * 1000 : 0)

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
                <!-- Current phase card + watcher + verifying -->
                <div class="index-status-row">
                    <div class="index-status">
                        {#if debugStatus === null}
                            <span class="status-badge neutral">Loading...</span>
                        {:else}
                            <span
                                class="status-badge {phaseStyle(debugStatus.activityPhase)}"
                                use:tooltip={{
                                    text: debugStatus.phaseStartedAt
                                        ? `Trigger: ${debugStatus.phaseHistory.length > 0 ? (debugStatus.phaseHistory[debugStatus.phaseHistory.length - 1]?.trigger ?? '') : ''}`
                                        : '',
                                }}
                            >
                                {#if isActivePhase(debugStatus.activityPhase)}
                                    <span class="spinner spinner-sm"></span>
                                {/if}
                                {debugStatus.activityPhase.charAt(0).toUpperCase() + debugStatus.activityPhase.slice(1)}
                                <span
                                    class="phase-duration"
                                    use:tooltip={{ text: `${String(currentPhaseDurationMs)}ms` }}
                                >
                                    {formatPhaseDurationMs(currentPhaseDurationMs)}
                                </span>
                            </span>
                            {@const liveStat = currentPhaseLiveStat(debugStatus)}
                            {#if liveStat}
                                <span class="phase-live-stat">{liveStat}</span>
                            {/if}
                        {/if}
                    </div>
                    <div class="index-status">
                        {#if debugStatus?.watcherActive}
                            <span
                                class="status-badge ready"
                                use:tooltip={{
                                    text: 'FSEvents watcher is active — receiving live filesystem change notifications',
                                }}>Watcher on</span
                            >
                        {:else}
                            <span class="status-badge neutral" use:tooltip={{ text: 'FSEvents watcher is not running' }}
                                >Watcher off</span
                            >
                        {/if}
                    </div>
                    {#if debugStatus?.verifying}
                        <div class="index-status">
                            <span
                                class="status-badge active"
                                use:tooltip={{ text: 'Background post-replay directory verification' }}
                            >
                                <span class="spinner spinner-sm"></span>
                                Verifying
                            </span>
                        </div>
                    {/if}
                </div>

                <!-- Phase timeline -->
                <div class="index-sub-header">Phase timeline</div>
                <div class="phase-timeline">
                    {#if debugStatus === null || debugStatus.phaseHistory.length === 0}
                        <p class="no-history">No phase history</p>
                    {:else}
                        {#each debugStatus.phaseHistory as record, i (record.startedAt + record.phase)}
                            {@const isCurrent = i === debugStatus.phaseHistory.length - 1 && record.durationMs === null}
                            <div class="phase-timeline-row" class:phase-current={isCurrent}>
                                <span class="phase-time">{record.startedAt.substring(0, 8)}</span>
                                <span class="phase-name" use:tooltip={{ text: phaseTooltipMap[record.phase] ?? '' }}
                                    >{record.phase.charAt(0).toUpperCase() + record.phase.slice(1)}</span
                                >
                                <span
                                    class="phase-dur"
                                    use:tooltip={{
                                        text:
                                            record.durationMs !== null
                                                ? `${String(record.durationMs)}ms`
                                                : `${String(currentPhaseDurationMs)}ms`,
                                    }}
                                >
                                    {#if record.durationMs !== null}
                                        {formatPhaseDurationMs(record.durationMs)}
                                    {:else}
                                        {formatPhaseDurationMs(currentPhaseDurationMs)}
                                    {/if}
                                </span>
                                <span class="phase-stats">
                                    {#if isCurrent}
                                        <span class="phase-now-marker">now</span>
                                    {:else}
                                        {formatPhaseStats(record.stats)}
                                    {/if}
                                </span>
                            </div>
                        {/each}
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

                <!-- DB statistics -->
                {#if debugStatus?.initialized}
                    <div class="index-sub-header">Database</div>
                    <div class="index-meta">
                        <div class="index-meta-row">
                            <span class="index-meta-label"
                                >Entries <span
                                    class="info-icon"
                                    use:tooltip={{ text: 'Total files and directories in the index DB' }}>i</span
                                ></span
                            >
                            <span class="index-meta-value">{formatCount(debugStatus.liveEntryCount)}</span>
                        </div>
                        <div class="index-meta-row">
                            <span class="index-meta-label"
                                >Directories <span
                                    class="info-icon"
                                    use:tooltip={{ text: 'Total directories (subset of entries)' }}>i</span
                                ></span
                            >
                            <span class="index-meta-value">{formatCount(debugStatus.liveDirCount)}</span>
                        </div>
                        <div class="index-meta-row">
                            <span class="index-meta-label"
                                >Dirs with stats <span
                                    class="info-icon"
                                    use:tooltip={{
                                        text: 'Directories that have computed recursive size/count aggregates',
                                    }}>i</span
                                ></span
                            >
                            <span class="index-meta-value">{formatCount(debugStatus.dirsWithStats)}</span>
                        </div>
                        {#if debugStatus.liveDirCount !== null && debugStatus.dirsWithStats !== null}
                            <div class="index-meta-row">
                                <span class="index-meta-label"
                                    >Dirs missing stats <span
                                        class="info-icon"
                                        use:tooltip={{
                                            text: 'Directories without aggregates — will show no size in the UI until backfilled',
                                        }}>i</span
                                    ></span
                                >
                                <span class="index-meta-value"
                                    >{formatCount(debugStatus.liveDirCount - debugStatus.dirsWithStats)}</span
                                >
                            </div>
                        {/if}
                        {#if debugStatus.indexStatus?.scanCompletedAt}
                            <div class="index-meta-row">
                                <span class="index-meta-label"
                                    >Last scan <span
                                        class="info-icon"
                                        use:tooltip={{ text: 'When the last full volume scan completed' }}>i</span
                                    ></span
                                >
                                <span class="index-meta-value"
                                    >{formatTimestamp(debugStatus.indexStatus.scanCompletedAt)}</span
                                >
                            </div>
                        {/if}
                        {#if debugStatus.indexStatus?.scanDurationMs}
                            <div class="index-meta-row">
                                <span class="index-meta-label"
                                    >Scan duration <span
                                        class="info-icon"
                                        use:tooltip={{ text: 'How long the last full scan took (wall clock)' }}>i</span
                                    ></span
                                >
                                <span class="index-meta-value"
                                    >{formatDuration(debugStatus.indexStatus.scanDurationMs)}</span
                                >
                            </div>
                        {/if}
                        {#if debugStatus.dbFileSize !== null}
                            <div class="index-meta-row">
                                <span class="index-meta-label"
                                    >DB size <span
                                        class="info-icon"
                                        use:tooltip={{ text: 'Total on-disk size: main DB file + WAL + SHM' }}>i</span
                                    ></span
                                >
                                <span class="index-meta-value">
                                    {formatDbSize(debugStatus.dbFileSize)}
                                    {#if debugStatus.dbMainSize !== null}
                                        <span class="db-breakdown"
                                            >(main: {formatDbSize(
                                                debugStatus.dbMainSize,
                                            )}{#if debugStatus.dbWalSize !== null && debugStatus.dbWalSize > 0}, WAL: {formatDbSize(
                                                    debugStatus.dbWalSize,
                                                )}{/if})</span
                                        >
                                    {/if}
                                </span>
                            </div>
                        {/if}
                        {#if debugStatus.dbPageCount !== null}
                            <div class="index-meta-row">
                                <span class="index-meta-label"
                                    >DB pages <span
                                        class="info-icon"
                                        use:tooltip={{
                                            text: 'SQLite pages: total allocated vs freelist (unused, reclaimable with VACUUM)',
                                        }}>i</span
                                    ></span
                                >
                                <span class="index-meta-value">
                                    {formatCount(debugStatus.dbPageCount)}
                                    {#if debugStatus.dbFreelistCount !== null && debugStatus.dbFreelistCount > 0}
                                        <span class="db-breakdown"
                                            >({formatCount(debugStatus.dbFreelistCount)} free, {(
                                                (debugStatus.dbFreelistCount / debugStatus.dbPageCount) *
                                                100
                                            ).toFixed(1)}%)</span
                                        >
                                    {/if}
                                </span>
                            </div>
                        {/if}
                    </div>
                {/if}

                <!-- Event statistics -->
                {#if debugStatus?.initialized}
                    <div class="index-sub-header">Event statistics</div>
                    <div class="index-meta">
                        <div class="index-meta-row">
                            <span class="index-meta-label"
                                >Live FS events <span
                                    class="info-icon"
                                    use:tooltip={{
                                        text: 'Total FSEvents received since indexing started this session',
                                    }}>i</span
                                ></span
                            >
                            <span class="index-meta-value">{formatCount(debugStatus.liveEventCount)}</span>
                        </div>
                        <div class="index-meta-row">
                            <span class="index-meta-label"
                                >MustScanSubDirs <span
                                    class="info-icon"
                                    use:tooltip={{
                                        text: 'FSEvents with MustScanSubDirs flag — means the OS coalesced events and a full subtree rescan is needed',
                                    }}>i</span
                                ></span
                            >
                            <span class="index-meta-value">{formatCount(debugStatus.mustScanCount)}</span>
                        </div>
                        <div class="index-meta-row">
                            <span class="index-meta-label"
                                >Rescans completed <span
                                    class="info-icon"
                                    use:tooltip={{
                                        text: 'Number of MustScanSubDirs subtree rescans that have finished processing',
                                    }}>i</span
                                ></span
                            >
                            <span class="index-meta-value">{formatCount(debugStatus.mustScanRescansCompleted)}</span>
                        </div>
                    </div>
                {/if}
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

    .phase-duration {
        font-weight: 400;
        margin-left: 4px;
        font-family: var(--font-mono);
    }

    .phase-live-stat {
        font-size: var(--font-size-xs);
        color: var(--color-text-secondary);
        margin-left: 8px;
    }

    /* Phase timeline */
    .phase-timeline {
        max-height: 160px;
        overflow-y: auto;
        background: var(--color-bg-primary);
        border-radius: var(--radius-sm);
        padding: 6px;
        font-size: var(--font-size-xs);
        font-family: var(--font-mono);
    }

    .phase-timeline-row {
        display: flex;
        gap: 10px;
        padding: 2px 0;
        line-height: 1.4;
        color: var(--color-text-tertiary);
    }

    .phase-timeline-row.phase-current {
        color: var(--color-text-primary);
        font-weight: 600;
    }

    .phase-time {
        flex-shrink: 0;
        width: 60px;
    }

    .phase-name {
        flex-shrink: 0;
        width: 85px;
    }

    .phase-dur {
        flex-shrink: 0;
        width: 70px;
        text-align: right;
    }

    .phase-stats {
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        color: var(--color-text-secondary);
    }

    .phase-current .phase-stats {
        color: var(--color-text-primary);
    }

    .phase-now-marker {
        color: var(--color-accent);
        font-weight: 600;
    }

    .phase-now-marker::before {
        content: '\2190 ';
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

    .info-icon {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        width: 14px;
        height: 14px;
        font-size: 10px;
        font-weight: 600;
        font-style: italic;
        font-family: var(--font-system), sans-serif;
        border-radius: 50%;
        background: var(--color-bg-tertiary);
        color: var(--color-text-tertiary);
        cursor: help;
        vertical-align: middle;
        margin-left: 2px;
    }

    .info-icon:hover {
        background: var(--color-bg-primary);
        color: var(--color-text-secondary);
    }

    .db-breakdown {
        color: var(--color-text-tertiary);
        font-size: var(--font-size-xs);
        margin-left: 4px;
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
