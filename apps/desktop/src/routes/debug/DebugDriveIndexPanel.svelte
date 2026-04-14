<script lang="ts">
    import { onMount, onDestroy } from 'svelte'
    import { tooltip } from '$lib/tooltip/tooltip'

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

    let debugStatus = $state<IndexDebugStatus | null>(null)
    let indexMessage = $state('')
    let indexPollInterval: ReturnType<typeof setInterval> | undefined

    // Phase timeline ticking duration
    let phaseDurationTick = $state(0)
    let lastPhaseDurationMs = $state(-1)
    let phaseTickInterval: ReturnType<typeof setInterval> | undefined

    onMount(async () => {
        await pollDebugStatus()
        indexPollInterval = setInterval(() => {
            void pollDebugStatus()
        }, 2000)

        phaseTickInterval = setInterval(() => {
            phaseDurationTick++
        }, 1000)
    })

    onDestroy(() => {
        if (indexPollInterval) clearInterval(indexPollInterval)
        if (phaseTickInterval) clearInterval(phaseTickInterval)
    })

    // When phaseDurationMs changes from a poll, reset the tick counter
    $effect(() => {
        if (debugStatus?.phaseDurationMs !== undefined && debugStatus.phaseDurationMs !== lastPhaseDurationMs) {
            lastPhaseDurationMs = debugStatus.phaseDurationMs
            phaseDurationTick = 0
        }
    })

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
</script>

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
