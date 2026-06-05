<script lang="ts">
    import { onMount, onDestroy, tick } from 'svelte'
    import {
        getVolumeSpace,
        startScanPreview,
        cancelScanPreview,
        checkScanPreviewStatus,
        onScanPreviewProgress,
        onScanPreviewComplete,
        onScanPreviewError,
        onScanPreviewCancelled,
        scanVolumeForConflicts,
        DEFAULT_VOLUME_ID,
        type VolumeSpaceInfo,
        type SourceItemInput,
        type UnlistenFn,
    } from '$lib/tauri-commands'
    import type {
        SortColumn,
        SortOrder,
        ConflictResolution,
        TransferOperationType,
    } from '$lib/file-explorer/types'
    import { getSetting } from '$lib/settings'
    import { validateDirectoryPath } from '$lib/utils/filename-validation'
    import { pluralize } from '$lib/utils/pluralize'
    import DirectionIndicator from './DirectionIndicator.svelte'
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import Button from '$lib/ui/Button.svelte'
    import {
        deriveTransferLabel,
        generateTitle,
        shouldShowHardlinkNote,
        toVolumeRelativePath,
    } from './transfer-dialog-utils'
    import { getVolumes } from '$lib/stores/volume-store.svelte'
    import { formatNumber } from '$lib/file-explorer/selection/selection-info-utils'
    import Size from '$lib/ui/Size.svelte'
    import { getFileSizeFormat } from '$lib/settings/reactive-settings.svelte'
    import { formatFileSizeWithFormat } from '$lib/settings/format-utils'
    import { getAppLogger } from '$lib/logging/logger'
    import IconTriangleAlert from '~icons/lucide/triangle-alert'

    const log = getAppLogger('transferDialog')

    interface Props {
        operationType: TransferOperationType
        sourcePaths: string[]
        destinationPath: string
        direction: 'left' | 'right'
        currentVolumeId: string
        fileCount: number
        folderCount: number
        sourceFolderPath: string
        /** Current sort column on source pane (for scan preview ordering) */
        sortColumn: SortColumn
        /** Current sort order on source pane */
        sortOrder: SortOrder
        /** Source volume ID (like "root", "mtp-336592896:65537") */
        sourceVolumeId: string
        /** Destination volume ID */
        destVolumeId: string
        /** When true, dialog auto-confirms without user interaction (MCP). */
        autoConfirm?: boolean
        /** Conflict resolution policy for auto-confirm (MCP). */
        autoConfirmOnConflict?: string
        onConfirm: (
            destination: string,
            volumeId: string,
            previewId: string | null,
            conflictResolution: ConflictResolution,
            operationType: TransferOperationType,
            scanInProgress: boolean,
            /** Source filenames known to conflict at dest, for the BE to bulk-skip
             *  under `Skip all`. Empty when no conflicts were found or the pre-flight
             *  scan failed. */
            preKnownConflicts: string[],
        ) => void
        onCancel: () => void
    }

    const volumes = $derived(getVolumes())

    const {
        operationType: initialOperationType,
        sourcePaths,
        destinationPath,
        direction,
        currentVolumeId,
        fileCount,
        folderCount,
        sourceFolderPath,
        sortColumn,
        sortOrder,
        sourceVolumeId,
        // eslint-disable-next-line @typescript-eslint/no-unused-vars -- Part of Props interface, used by parent
        destVolumeId,
        autoConfirm = false,
        autoConfirmOnConflict,
        onConfirm,
        onCancel,
    }: Props = $props()

    let activeOperationType = $state<TransferOperationType>(initialOperationType)

    // Compute initial volume-relative path. Can't use $derived selectedVolume here (not yet available),
    // so look up the volume path directly from the props.
    const initialVolumePath = volumes.find((v) => v.id === currentVolumeId)?.path ?? '/'
    let editedPath = $state(toVolumeRelativePath(destinationPath, initialVolumePath))
    log.debug('Initial path resolution: destinationPath={destinationPath}, currentVolumeId={currentVolumeId}, initialVolumePath={initialVolumePath}, editedPath={editedPath}', {
        destinationPath, currentVolumeId, initialVolumePath, editedPath,
    })
    let selectedVolumeId = $state(currentVolumeId)
    let pathInputRef: HTMLInputElement | undefined = $state()

    // Volume space info
    let volumeSpace = $state<VolumeSpaceInfo | null>(null)

    // Scan preview state
    let previewId = $state<string | null>(null)
    let filesFound = $state(0)
    let dirsFound = $state(0)
    // `bytesFound` is the write footprint (what the copy writes). `dedupBytesFound`
    // is the `du`-equivalent source size; the two differ only when the source has
    // hardlinks (cargo `target/`, Time Machine, deduped backups), in which case we
    // show a one-line note clarifying the gap.
    let bytesFound = $state(0)
    let dedupBytesFound = $state(0)
    let isScanning = $state(false)
    let scanComplete = $state(false)
    let unlisteners: UnlistenFn[] = []
    // Promise that resolves once startScanPreview IPC has returned and previewId is set.
    // handleConfirm awaits this to guarantee previewId is non-null when passed to
    // TransferProgressDialog, otherwise a fast confirm races with IPC and leaves the
    // progress dialog stuck in "Scanning 0 files" forever.
    let scanStarted: Promise<void> = Promise.resolve()

    // Whether the user confirmed (so we don't cancel the scan on destroy)
    let confirmed = false
    let destroyed = false

    // Conflict detection state. `totalConflictCount` is the unbounded count of
    // real conflicts (file clashes + cross-type clashes) for the summary text —
    // must NOT be derived from a capped slice, or the summary misleads the user
    // about how many files will actually be skipped. Dir-vs-dir collisions are
    // NOT conflicts: they always merge silently, so they're surfaced as a
    // separate informational count (`mergeFolderCount`) and never counted here.
    // The conflict names (file + cross-type only, never dir-dir) are forwarded
    // to the backend on confirm so it can bulk-skip them upfront under
    // `Skip all`. We never render per-conflict rows in this dialog, so we don't
    // need to keep the full `VolumeConflictInfo[]` array around.
    let totalConflictCount = $state(0)
    // Count of source folders that will merge into an existing same-named dest
    // folder. Informational only — never a conflict, never a radio count.
    let mergeFolderCount = $state(0)
    // `true` when any real conflict is a cross-type clash (file-vs-folder either
    // direction). Drives the upfront "Overwrite all" red warning, mirroring the
    // per-file dialog's file→folder warning.
    let hasTypeMismatchConflict = $state(false)
    let conflictNames = $state<string[]>([])
    let isCheckingConflicts = $state(false)
    let conflictCheckComplete = $state(false)
    // Map MCP onConflict string to ConflictResolution, or default to "ask for each"
    const autoConfirmConflictMap: Record<string, ConflictResolution> = {
        skip_all: 'skip',
        overwrite_all: 'overwrite',
        rename_all: 'rename',
        overwrite_all_smaller: 'overwrite_smaller',
        overwrite_all_older: 'overwrite_older',
    }
    let conflictPolicy = $state<ConflictResolution>(
        autoConfirm && autoConfirmOnConflict
            ? autoConfirmConflictMap[autoConfirmOnConflict] ?? 'skip'
            : 'stop',
    ) // Default to "ask for each" unless auto-confirming

    // Filter to only actual volumes (not favorites)
    const actualVolumes = $derived(volumes.filter((v) => v.category !== 'favorite' && v.category !== 'network'))

    // Get selected volume info
    const selectedVolume = $derived(actualVolumes.find((v) => v.id === selectedVolumeId))

    // Source/destination labels for the direction header. At a volume root the
    // path basename isn't a user-meaningful name — for an MTP storage root it's
    // the raw storage id (like "65538"). `deriveTransferLabel` falls back to the
    // volume's display name in that case (like "Virtual Pixel 9 - SD Card").
    const sourceVolume = $derived(volumes.find((v) => v.id === sourceVolumeId))
    const sourceLabel = $derived(
        deriveTransferLabel(sourceFolderPath, sourceVolume?.path ?? '/', sourceVolume?.name ?? ''),
    )
    const destinationLabel = $derived(
        deriveTransferLabel(destinationPath, selectedVolume?.path ?? '/', selectedVolume?.name ?? ''),
    )

    /** A same-volume move: the source and destination are the SAME NON-DEFAULT
     *  volume (one smb2 share / one MTP device) and the active operation is Move.
     *  The backend handles this as a server-side rename (instant, zero bytes), so
     *  the deep recursive scan preview — which exists only to feed a Size bar — is
     *  pure waste here and used to cost 30–40 s of "Verifying before move…" on a
     *  NAS. For this case we dispatch immediately and skip the deep scan; the
     *  cheap top-level conflict check still runs.
     *
     *  The DEFAULT_VOLUME_ID exclusion is load-bearing: a local→local move (root →
     *  root) is NOT a server-side rename. The backend's local move path CONSUMES
     *  the preview cache via `config.preview_id`, and the dialog's own tallies come
     *  from the preview — cancelling it both zeroes the counters and forces a BE
     *  re-scan. So local→local must keep the deep preview running, matching the
     *  same guard in `TransferProgressDialog`'s `isSameVolumeMove`. Derived from
     *  what the dialog already knows (no extra prop). */
    const isSameVolumeMove = $derived(
        activeOperationType === 'move' &&
            sourceVolumeId !== DEFAULT_VOLUME_ID &&
            sourceVolumeId === selectedVolumeId,
    )

    const dialogTitle = $derived(generateTitle(activeOperationType, fileCount, folderCount))
    const showHardlinkNote = $derived(
        shouldShowHardlinkNote({
            operationType: activeOperationType,
            scanComplete,
            writeBytes: bytesFound,
            dedupBytes: dedupBytesFound,
        }),
    )

    const confirmLabel = $derived(activeOperationType === 'copy' ? 'Copy' : 'Move')

    /** Checks whether the destination path is invalid relative to the source paths. */
    function getPathValidationError(sources: string[], destination: string): string | null {
        const normDest = destination.replace(/\/+$/, '')
        const verb = activeOperationType === 'copy' ? 'copy' : 'move'

        for (const source of sources) {
            const normSource = source.replace(/\/+$/, '')
            if (normDest === normSource || normDest.startsWith(normSource + '/')) {
                const folderName = normSource.split('/').pop() ?? normSource
                return `Can't ${verb} "${folderName}" into its own subfolder`
            }
        }

        for (const source of sources) {
            const normSource = source.replace(/\/+$/, '')
            const sourceParent = normSource.substring(0, normSource.lastIndexOf('/'))
            if (normDest === sourceParent) {
                const fileName = normSource.split('/').pop() ?? normSource
                return `"${fileName}" is already in this location`
            }
        }

        return null
    }

    const pathError = $derived.by(() => {
        const structural = validateDirectoryPath(editedPath)
        if (structural.severity === 'error') return structural.message
        return getPathValidationError(sourcePaths, editedPath)
    })

    // Free-space text is intentionally uncolored: red GB would falsely signal "low space".
    function formatSpaceInfo(space: VolumeSpaceInfo | null): string {
        if (!space) return ''
        const format = getFileSizeFormat()
        const free = formatFileSizeWithFormat(space.availableBytes, format)
        const total = formatFileSizeWithFormat(space.totalBytes, format)
        return `${free} free of ${total}`
    }

    // Load volume space when volume changes
    async function loadVolumeSpace() {
        const volume = selectedVolume
        if (volume) {
            volumeSpace = (await getVolumeSpace(volume.path)).data
        }
    }

    // Reset to volume root when volume changes: the current path is meaningless on a different volume
    function handleVolumeChange() {
        editedPath = '/'
        void loadVolumeSpace()
    }

    let isInitialVolumeEffect = true
    $effect(() => {
        // Watch for volume changes - read the reactive value to track it
        void selectedVolumeId
        if (isInitialVolumeEffect) {
            // Skip the first run: editedPath is already initialized with the correct volume-relative path.
            // Only load volume space on init.
            isInitialVolumeEffect = false
            void loadVolumeSpace()
        } else {
            handleVolumeChange()
        }
    })

    /** Cleans up event listeners for scan preview. */
    function cleanup() {
        for (const unlisten of unlisteners) {
            unlisten()
        }
        unlisteners = []
    }

    /**
     * Pending conflict check, captured so `handleConfirm` can await it. Without this,
     * a fast confirm (Enter pressed before the check finishes) sends the operation
     * with `conflicts: []` even when conflicts exist. The FE never displays the count
     * + radio policy section, and the backend can't help if the user picked
     * `overwrite_all` blindly. We resolve this by gating Confirm on the check
     * completing. See `handleConfirm`.
     *
     * The check runs on mount, in parallel with the (potentially slow) scan
     * preview — it's just one cheap dest listing and doesn't need the recursive
     * byte scan. It's assigned synchronously in `onMount` BEFORE the auto-confirm
     * branch, so the MCP fast path's `handleConfirm` await guard sees a real
     * promise (not `undefined`) and dispatches with `conflictNames` populated.
     */
    let conflictCheckPromise: Promise<void> | null = $state(null)

    /** Checks for conflicts at the destination. */
    async function checkConflicts() {
        if (destroyed || isCheckingConflicts || conflictCheckComplete) return

        isCheckingConflicts = true
        try {
            // Build source item info from the source paths. We extract the
            // filename from each path for name matching. The real per-item
            // `is_directory` and size come from the backend, which resolves
            // them authoritatively from the source volume (one batched stat)
            // when we pass `sourceVolumeId` + `sourcePaths`. We still send
            // placeholders here so name matching works even if that resolution
            // is unavailable (e.g. the source volume vanished).
            const sourceItems: SourceItemInput[] = sourcePaths.map((path) => {
                const name = path.split('/').pop() || path
                return {
                    name,
                    size: 0,
                    modified: null,
                    isDirectory: false,
                }
            })

            const foundConflicts = await scanVolumeForConflicts(
                selectedVolumeId,
                sourceItems,
                editedPath,
                sourceVolumeId,
                sourcePaths,
            )

            // Classify each collision:
            //  - dir + dir  → a silent merge, not a conflict (informational).
            //  - everything else (file+file, file+dir, dir+file) → a real
            //    conflict the file policy governs.
            // Only real conflicts count toward `totalConflictCount` and feed
            // the bulk-skip name list; dir-dir merges must never enter the file
            // bulk-skip set ("Skip all" must not skip folders wholesale).
            const realConflicts = foundConflicts.filter((c) => !(c.sourceIsDirectory && c.destIsDirectory))
            mergeFolderCount = foundConflicts.length - realConflicts.length
            totalConflictCount = realConflicts.length
            hasTypeMismatchConflict = realConflicts.some((c) => c.sourceIsDirectory !== c.destIsDirectory)
            conflictNames = realConflicts.map((c) => c.sourcePath)
            conflictCheckComplete = true

            if (totalConflictCount > 0 || mergeFolderCount > 0) {
                log.info('Found {count} {conflictsNoun} and {merges} folder merges at destination', {
                    count: totalConflictCount,
                    conflictsNoun: pluralize(totalConflictCount, 'conflict'),
                    merges: mergeFolderCount,
                })
            }
        } catch (err) {
            log.error('Failed to check for conflicts: {error}', { error: err })
            // Don't block the operation on conflict check failure
            conflictCheckComplete = true
        } finally {
            isCheckingConflicts = false
        }
    }

    /** Accepts the event if it belongs to our scan, filtering stale events from previous scans. */
    function isOurScanEvent(eventPreviewId: string): boolean {
        // Don't accept events until we know our previewId from the IPC return.
        // This prevents adopting stale events from previous orphaned scans.
        if (!previewId) return false
        return eventPreviewId === previewId
    }

    /** Starts the scan preview to count files/dirs/bytes. */
    async function startScan() {
        // Subscribe to events BEFORE starting scan (avoid missing fast completions)
        unlisteners.push(
            await onScanPreviewProgress((event) => {
                if (!isOurScanEvent(event.previewId)) return
                filesFound = event.filesFound
                dirsFound = event.dirsFound
                bytesFound = event.bytesFound
            }),
        )
        unlisteners.push(
            await onScanPreviewComplete((event) => {
                if (!isOurScanEvent(event.previewId)) return
                filesFound = event.filesTotal
                dirsFound = event.dirsTotal
                bytesFound = event.bytesTotal
                dedupBytesFound = event.dedupBytesTotal
                isScanning = false
                scanComplete = true
            }),
        )
        unlisteners.push(
            await onScanPreviewError((event) => {
                if (!isOurScanEvent(event.previewId)) return
                isScanning = false
                // Keep showing whatever stats we have
            }),
        )
        unlisteners.push(
            await onScanPreviewCancelled((event) => {
                if (!isOurScanEvent(event.previewId)) return
                isScanning = false
            }),
        )

        // Start the scan
        isScanning = true
        const progressIntervalMs = getSetting('fileOperations.progressUpdateInterval')
        const result = await startScanPreview(sourcePaths, sortColumn, sortOrder, progressIntervalMs, sourceVolumeId)
        previewId = result.previewId

        // Check if the scan already completed while we were awaiting the IPC return.
        // Events that arrived before previewId was set were dropped (isOurScanEvent returned false),
        // so we need to read the backend's cached totals and hydrate the dialog from them.
        // Without this, M2a's watcher-backed oracle (a ~5 ms scan) lands its events
        // before we register listeners and the dialog shows "✓ 0 files" forever.
        // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- may have changed during await
        if (isScanning) {
            const totals = await checkScanPreviewStatus(previewId)
            if (totals) {
                filesFound = totals.filesTotal
                dirsFound = totals.dirsTotal
                bytesFound = totals.bytesTotal
                dedupBytesFound = totals.dedupBytesTotal
                isScanning = false
                scanComplete = true
            }
        }
    }

    /** Cancels the in-flight deep scan preview and resets its state, without
     *  touching the (independent) conflict check. Used when the user flips to a
     *  same-volume Move, where the deep byte scan is waste — the move is a
     *  rename. Idempotent: a no-op when no preview is running. */
    function cancelPreview() {
        if (previewId) {
            void cancelScanPreview(previewId)
        }
        cleanup() // drop the scan-preview listeners
        previewId = null
        isScanning = false
        scanComplete = false
        filesFound = 0
        dirsFound = 0
        bytesFound = 0
        dedupBytesFound = 0
        scanStarted = Promise.resolve()
    }

    // Copy/Move toggle gating for same-volume moves. `startScan()` runs once in
    // `onMount` for the initial operation; this effect handles LATER toggles:
    //  - flip to a same-volume Move → cancel the deep recursive preview (a
    //    rename moves zero bytes, so there's nothing for the Size bar to show).
    //  - flip away (to Copy, or to a cross-volume Move) → (re)start the preview,
    //    because Copy genuinely needs byte totals for its Size bar.
    // The conflict check is independent (runs in `onMount`), so it's unaffected.
    let toggleEffectInitialized = false
    $effect(() => {
        // Track the reactive inputs.
        const sameVolumeMove = isSameVolumeMove
        if (!toggleEffectInitialized) {
            // Skip the first run: `onMount` owns the initial scan/skip decision.
            toggleEffectInitialized = true
            return
        }
        if (sameVolumeMove) {
            cancelPreview()
        } else if (!previewId && !confirmed && !destroyed) {
            // No preview running and we're back on a path that needs one: start it.
            scanStarted = startScan()
        }
    })

    onMount(async () => {
        // Focus and select the path input
        await tick()
        pathInputRef?.focus()
        pathInputRef?.select()

        // Volume space is loaded by the $effect watching selectedVolumeId

        // Start the deep scan preview immediately — UNLESS this is a same-volume
        // move, where the backend does a server-side rename (zero bytes) and the
        // recursive byte scan is pure waste (the 30–40 s "Verifying before move…"
        // this fast path eliminates). Track the promise so handleConfirm can
        // await it: this ensures previewId is set before onConfirm fires.
        if (!isSameVolumeMove) {
            scanStarted = startScan()
        }

        // Run the cheap top-level conflict check in parallel with the scan
        // preview (it's one dest listing, not the recursive byte scan). MUST be
        // assigned BEFORE the auto-confirm branch so the fast path's
        // `handleConfirm` await guard sees a real promise and dispatches with
        // `conflictNames` populated. Mirrors how `scanStarted` is assigned above.
        conflictCheckPromise = checkConflicts()

        // Auto-confirm if MCP requested it (after a tick so the dialog is fully initialized)
        if (autoConfirm) {
            await tick()
            await handleConfirm()
        }
    })

    onDestroy(() => {
        destroyed = true
        // Free the scan preview unless the user confirmed (then the
        // TransferProgressDialog / the started op takes over the same scan and
        // consumes the cached result). We call this regardless of `isScanning`:
        // `cancelScanPreview` also evicts the cached `CachedScanResult`, so a
        // dialog dismissed AFTER the scan completed doesn't leak the cache until
        // quit.
        if (previewId && !confirmed) {
            void cancelScanPreview(previewId)
        }
        cleanup()
    })

    async function handleConfirm() {
        if (pathError || confirmed) return
        confirmed = true
        // Same-volume move: dispatch IMMEDIATELY. No deep scan ever ran (the
        // backend renames server-side, zero bytes), so there's nothing to wait
        // for and no cached preview to consume — pass `previewId = null` and
        // `scanInProgress = false` so the progress dialog dispatches without
        // gating on a scan. The conflict check still runs, so we await it for
        // `conflictNames`. This is the FE half of the perf fix.
        if (isSameVolumeMove) {
            cancelPreview()
            if (conflictCheckPromise) {
                await conflictCheckPromise
            }
            onConfirm(editedPath, selectedVolumeId, null, conflictPolicy, activeOperationType, false, conflictNames)
            return
        }
        // Wait for startScanPreview IPC to return so previewId is set. Without this,
        // a fast confirm (auto-confirm, Playwright test, rapid Enter keypress) races
        // with the IPC and leaves the progress dialog with a null previewId that it
        // cannot recover from once scan events have already been emitted.
        await scanStarted
        // Also wait for the conflict scan if it's still running. Without this, a fast
        // confirm sends `conflicts: []` to the backend even when conflicts exist,
        // the user never sees the radio policy section, and the operation runs with
        // whatever default `conflictPolicy` was set ("stop" by default, so it'd still
        // prompt per-file via the backend, but only because of the default).
        if (conflictCheckPromise) {
            await conflictCheckPromise
        }
        onConfirm(
            editedPath,
            selectedVolumeId,
            previewId,
            conflictPolicy,
            activeOperationType,
            isScanning,
            conflictNames,
        )
    }

    function handleCancel() {
        // Free the scan preview (cancels an in-flight scan and evicts any cached
        // result). Regardless of `isScanning`, so a dismiss after the scan
        // completed doesn't leak the cache.
        if (previewId) {
            void cancelScanPreview(previewId)
        }
        cleanup()
        onCancel()
    }

    function handleKeydown(event: KeyboardEvent) {
        if (event.key === 'Enter') {
            void handleConfirm()
        }
    }

    function handleInputKeydown(event: KeyboardEvent) {
        if (event.key === 'Enter') {
            event.preventDefault()
            event.stopPropagation()
            void handleConfirm()
        }
    }
</script>

<ModalDialog
    titleId="dialog-title"
    onkeydown={handleKeydown}
    dialogId="transfer-confirmation"
    onclose={handleCancel}
    containerStyle="width: 500px"
>
    {#snippet title()}{dialogTitle}{/snippet}

    <!-- Copy/Move toggle -->
    <div class="operation-toggle">
        <button
            class="toggle-option"
            class:active={activeOperationType === 'copy'}
            onclick={() => (activeOperationType = 'copy')}>Copy</button
        >
        <button
            class="toggle-option"
            class:active={activeOperationType === 'move'}
            onclick={() => (activeOperationType = 'move')}>Move</button
        >
    </div>

    <!-- Direction indicator -->
    <DirectionIndicator
        sourcePath={sourceFolderPath}
        {destinationPath}
        {direction}
        {sourceLabel}
        {destinationLabel}
    />

    <!-- Volume selector -->
    <div class="volume-selector">
        <select bind:value={selectedVolumeId} class="volume-select" aria-label="Destination volume">
            {#each actualVolumes as volume (volume.id)}
                <option value={volume.id}>{volume.name}</option>
            {/each}
        </select>
        {#if volumeSpace}
            <span class="space-info">{formatSpaceInfo(volumeSpace)}</span>
        {/if}
    </div>

    {#if selectedVolume?.smbConnectionState === 'os_mount'}
        <p class="smb-native-note">
            This share uses the system connection. Cancellation may be delayed.
            Use "Connect directly" in the volume picker for faster transfers and reliable cancel.
        </p>
    {/if}

    <!-- Path input -->
    <div class="path-input-group">
        <input
            bind:this={pathInputRef}
            bind:value={editedPath}
            type="text"
            class="path-input"
            class:has-error={!!pathError}
            aria-label="Destination path"
            aria-describedby={pathError ? 'transfer-path-error' : undefined}
            aria-invalid={!!pathError}
            spellcheck="false"
            autocomplete="off"
            onkeydown={handleInputKeydown}
        />
        {#if pathError}
            <p id="transfer-path-error" class="path-error" role="alert">{pathError}</p>
        {/if}
    </div>

    <!-- Scan stats (live counting) -->
    <div class="scan-stats">
        <div class="scan-stat">
            <span class="scan-value"><Size bytes={bytesFound} /></span>
        </div>
        <span class="scan-divider">/</span>
        <div class="scan-stat">
            <span class="scan-value">{formatNumber(filesFound)}</span>
            <span class="scan-label">{filesFound === 1 ? 'file' : 'files'}</span>
        </div>
        <span class="scan-divider">/</span>
        <div class="scan-stat">
            <span class="scan-value">{formatNumber(dirsFound)}</span>
            <span class="scan-label">{dirsFound === 1 ? 'dir' : 'dirs'}</span>
        </div>
        {#if isScanning}
            <span class="scan-spinner"></span>
        {:else if scanComplete}
            <span class="scan-checkmark">✓</span>
        {/if}
    </div>

    <!-- Hardlink note: copy writes every hardlink as a full file, so the bytes
         written exceed the source's on-disk size. Clarify the gap so the
         headline size doesn't look wrong against Finder's number. Copy-only:
         a same-filesystem move renames in place and writes nothing. -->
    {#if showHardlinkNote}
        <p class="hardlink-note">
            <Size bytes={bytesFound} /> will be written. The source is
            <Size bytes={dedupBytesFound} /> on disk &ndash; the extra is hardlinked files, which can't
            stay linked across drives.
        </p>
    {/if}

    <!-- Conflicts section -->
    {#if isCheckingConflicts}
        <div class="conflicts-checking">
            <span class="scan-spinner"></span>
            <span class="conflicts-checking-text">Checking for conflicts...</span>
        </div>
    {:else if totalConflictCount > 0 || mergeFolderCount > 0}
        <div class="conflicts-section">
            <!-- Folder merges are informational, never a question: same-named
                 folders always merge silently. Surfaced so a user who didn't
                 expect a same-named folder at the dest gets a visible cue. -->
            {#if mergeFolderCount > 0}
                <p class="merge-info">
                    {mergeFolderCount === 1
                        ? '1 folder will merge with an existing folder'
                        : `${formatNumber(mergeFolderCount)} folders will merge with existing folders`}
                </p>
            {/if}
            {#if totalConflictCount > 0}
                <p class="conflicts-summary">
                    {totalConflictCount}
                    {totalConflictCount === 1 ? 'file already exists' : 'files already exist'}
                </p>
            {/if}
            <!-- The file policy radios show whenever there's a file conflict OR
                 a folder merge: a merge can surface file clashes mid-operation
                 the upfront check can't see, and the radios pre-answer them. -->
            <div class="conflict-policy">
                <label class="policy-option">
                    <input type="radio" bind:group={conflictPolicy} value="skip" />
                    <span>{totalConflictCount === 1 ? 'Skip' : 'Skip all'}</span>
                </label>
                <label class="policy-option">
                    <input type="radio" bind:group={conflictPolicy} value="overwrite" />
                    <span>{totalConflictCount === 1 ? 'Overwrite' : 'Overwrite all'}</span>
                </label>
                <label class="policy-option">
                    <input type="radio" bind:group={conflictPolicy} value="overwrite_smaller" />
                    <span>{totalConflictCount === 1 ? 'Overwrite if smaller' : 'Overwrite all smaller'}</span>
                </label>
                <label class="policy-option">
                    <input type="radio" bind:group={conflictPolicy} value="overwrite_older" />
                    <span>{totalConflictCount === 1 ? 'Overwrite if older' : 'Overwrite all older'}</span>
                </label>
                <label class="policy-option">
                    <input type="radio" bind:group={conflictPolicy} value="stop" />
                    <span>{totalConflictCount === 1 ? 'Ask later' : 'Ask for each'}</span>
                </label>
            </div>

            <!-- Cross-type guardrail: when a clash mixes a file and a same-named
                 folder, "Overwrite all" replaces items of a different type and
                 deletes folder contents. The per-file dialog already warns on
                 this; the bulk path must not be quieter. -->
            {#if hasTypeMismatchConflict && conflictPolicy === 'overwrite'}
                <p class="conflict-warning" role="alert">
                    <span class="conflict-warning-icon" aria-hidden="true">
                        <IconTriangleAlert width="16" height="16" />
                    </span>
                    <span>
                        Some clashes mix a file and a folder by the same name. Overwriting will replace items of a
                        different type, including the entire contents of a folder.
                    </span>
                </p>
            {/if}
        </div>
    {/if}

    <!-- Buttons (centered) -->
    <div class="button-row">
        <Button variant="secondary" onclick={handleCancel}>Cancel</Button>
        <Button variant="primary" onclick={handleConfirm} disabled={!!pathError}>{confirmLabel}</Button>
    </div>
</ModalDialog>

<style>
    .volume-selector {
        display: flex;
        align-items: center;
        gap: var(--spacing-md);
        padding: 0 var(--spacing-xl);
        margin-bottom: var(--spacing-md);
    }

    .volume-select {
        flex: 0 0 auto;
        padding: var(--spacing-sm) var(--spacing-md);
        font-size: var(--font-size-md);
        font-family: var(--font-system) sans-serif;
        background: var(--color-bg-primary);
        border: 1px solid var(--color-border-strong);
        border-radius: var(--radius-md);
        color: var(--color-text-primary);
    }

    .volume-select:focus {
        outline: none;
        border-color: var(--color-accent);
    }

    .space-info {
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
    }

    .path-input-group {
        padding: 0 var(--spacing-xl);
        margin-bottom: var(--spacing-lg);
    }

    .path-input {
        width: 100%;
        padding: var(--spacing-md) var(--spacing-md);
        font-size: var(--font-size-md);
        font-family: var(--font-system) sans-serif;
        background: var(--color-bg-primary);
        border: 2px solid var(--color-accent);
        border-radius: var(--radius-md);
        color: var(--color-text-primary);
        box-sizing: border-box;
    }

    .path-input::placeholder {
        color: var(--color-text-tertiary);
    }

    .path-input:focus {
        outline: none;
        box-shadow: var(--shadow-focus);
    }

    .path-input.has-error {
        border-color: var(--color-error);
    }

    .path-input.has-error:focus {
        box-shadow: 0 0 0 3px color-mix(in srgb, var(--color-error), transparent 85%);
    }

    .path-error {
        margin: var(--spacing-sm) 0 0;
        font-size: var(--font-size-sm);
        color: var(--color-error);
    }

    .smb-native-note {
        margin: 0;
        padding: var(--spacing-xs) var(--spacing-sm);
        font-size: var(--font-size-xs);
        color: var(--color-warning-text);
        background: var(--color-warning-bg);
        border-radius: var(--radius-sm);
    }

    .button-row {
        display: flex;
        gap: var(--spacing-md);
        justify-content: center;
        padding: 0 var(--spacing-xl) var(--spacing-xl);
    }

    /* Scan stats */
    .scan-stats {
        display: flex;
        align-items: center;
        justify-content: center;
        gap: var(--spacing-sm);
        padding: 0 var(--spacing-xl) var(--spacing-lg);
        font-size: var(--font-size-sm);
    }

    .scan-stat {
        display: flex;
        align-items: baseline;
        gap: var(--spacing-xs);
    }

    .scan-value {
        color: var(--color-text-primary);
        font-variant-numeric: tabular-nums;
        font-weight: 500;
    }

    .scan-label {
        color: var(--color-text-tertiary);
    }

    .hardlink-note {
        margin: var(--spacing-xs) 0 0;
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
        line-height: 1.4;
    }

    .scan-divider {
        color: var(--color-text-tertiary);
    }

    .scan-spinner {
        width: 12px;
        height: 12px;
        border: 2px solid var(--color-accent);
        border-top-color: transparent;
        border-radius: var(--radius-full);
        animation: spin 0.8s linear infinite;
        margin-left: var(--spacing-xs);
    }

    .scan-checkmark {
        color: var(--color-allow);
        font-size: var(--font-size-md);
        font-weight: 600;
        margin-left: var(--spacing-xs);
    }

    /* Conflicts checking */
    .conflicts-checking {
        display: flex;
        align-items: center;
        justify-content: center;
        gap: var(--spacing-sm);
        padding: 0 var(--spacing-xl) var(--spacing-md);
        font-size: var(--font-size-sm);
    }

    .conflicts-checking-text {
        color: var(--color-text-tertiary);
    }

    /* Conflicts section */
    .conflicts-section {
        padding: 0 var(--spacing-xl) var(--spacing-md);
        border-top: 1px solid var(--color-border-strong);
        margin-top: var(--spacing-xs);
        padding-top: var(--spacing-md);
    }

    .conflicts-summary {
        margin: 0 0 var(--spacing-md);
        font-size: var(--font-size-md);
        color: var(--color-warning);
        text-align: center;
        font-weight: 500;
    }

    /* Folder-merge info line: neutral, not a warning. Folders always merge, so
       this is a heads-up, not a question. */
    .merge-info {
        margin: 0 0 var(--spacing-md);
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
        text-align: center;
    }

    /* Cross-type "Overwrite all" guardrail. Mirrors the per-file dialog's red
       warning (icon + sentence in a tinted block) to flag the destructive swap
       before the user confirms a bulk overwrite across mixed types. */
    .conflict-warning {
        display: flex;
        align-items: flex-start;
        gap: var(--spacing-sm);
        margin: var(--spacing-md) 0 0;
        padding: var(--spacing-sm) var(--spacing-md);
        background: var(--color-error-bg);
        color: var(--color-error-text);
        border: 1px solid var(--color-error-border);
        border-radius: var(--radius-md);
        font-size: var(--font-size-sm);
        line-height: 1.4;
    }

    .conflict-warning-icon {
        flex-shrink: 0;
        display: inline-flex;
        align-items: center;
        color: var(--color-error-text);
        /* stylelint-disable-next-line declaration-property-value-disallowed-list -- align icon with first line of text */
        margin-top: 1px;
    }

    .conflict-policy {
        display: flex;
        flex-wrap: wrap;
        justify-content: center;
        column-gap: var(--spacing-lg);
        row-gap: var(--spacing-sm);
    }

    .policy-option {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }

    .policy-option input[type='radio'] {
        margin: 0;
    }

    .policy-option:hover {
        color: var(--color-text-primary);
    }

    /* Copy/Move segmented control */
    .operation-toggle {
        display: flex;
        justify-content: center;
        gap: 0;
        padding: 0 var(--spacing-xl) var(--spacing-md);
    }

    .toggle-option {
        padding: var(--spacing-xs) var(--spacing-lg);
        font-size: var(--font-size-sm);
        font-weight: 500;
        border: 1px solid var(--color-border-strong);
        background: transparent;
        color: var(--color-text-secondary);
        transition: all var(--transition-base);
        min-width: 60px;
    }

    .toggle-option:first-child {
        border-radius: var(--radius-md) 0 0 var(--radius-md);
        border-right: none;
    }

    .toggle-option:last-child {
        border-radius: 0 var(--radius-md) var(--radius-md) 0;
    }

    .toggle-option.active {
        background: var(--color-accent);
        border-color: var(--color-accent);
        color: var(--color-accent-fg);
    }

    .toggle-option:not(.active):hover {
        background: var(--color-bg-tertiary);
        color: var(--color-text-primary);
    }
</style>
