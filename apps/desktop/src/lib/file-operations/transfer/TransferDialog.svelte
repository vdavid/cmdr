<script lang="ts">
    import { onMount, onDestroy, tick } from 'svelte'
    import {
        getVolumeSpace,
        formatBytes,
        startScanPreview,
        cancelScanPreview,
        onScanPreviewProgress,
        onScanPreviewComplete,
        onScanPreviewError,
        onScanPreviewCancelled,
        scanVolumeForConflicts,
        type VolumeSpaceInfo,
        type VolumeConflictInfo,
        type SourceItemInput,
        type UnlistenFn,
    } from '$lib/tauri-commands'
    import type {
        VolumeInfo,
        SortColumn,
        SortOrder,
        ConflictResolution,
        TransferOperationType,
    } from '$lib/file-explorer/types'
    import { getSetting } from '$lib/settings'
    import DirectionIndicator from './DirectionIndicator.svelte'
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import { generateTitle } from './transfer-dialog-utils'
    import { getAppLogger } from '$lib/logger'

    const log = getAppLogger('transferDialog')

    interface Props {
        operationType: TransferOperationType
        sourcePaths: string[]
        destinationPath: string
        direction: 'left' | 'right'
        volumes: VolumeInfo[]
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
        onConfirm: (
            destination: string,
            volumeId: string,
            previewId: string | null,
            conflictResolution: ConflictResolution,
        ) => void
        onCancel: () => void
    }

    const {
        operationType,
        sourcePaths,
        destinationPath,
        direction,
        volumes,
        currentVolumeId,
        fileCount,
        folderCount,
        sourceFolderPath,
        sortColumn,
        sortOrder,
        // eslint-disable-next-line @typescript-eslint/no-unused-vars -- Passed through for consistency; conflict check uses destVolumeId
        sourceVolumeId: _sourceVolumeId,
        destVolumeId,
        onConfirm,
        onCancel,
    }: Props = $props()

    let editedPath = $state(destinationPath)
    let selectedVolumeId = $state(currentVolumeId)
    let pathInputRef: HTMLInputElement | undefined = $state()

    // Volume space info
    let volumeSpace = $state<VolumeSpaceInfo | null>(null)

    // Scan preview state
    let previewId = $state<string | null>(null)
    let filesFound = $state(0)
    let dirsFound = $state(0)
    let bytesFound = $state(0)
    let isScanning = $state(false)
    let scanComplete = $state(false)
    let unlisteners: UnlistenFn[] = []

    // Conflict detection state
    let conflicts = $state<VolumeConflictInfo[]>([])
    let isCheckingConflicts = $state(false)
    let conflictCheckComplete = $state(false)
    let conflictPolicy = $state<ConflictResolution>('stop') // Default to "ask for each"

    // Filter to only actual volumes (not favorites)
    const actualVolumes = $derived(volumes.filter((v) => v.category !== 'favorite' && v.category !== 'network'))

    // Get selected volume info
    const selectedVolume = $derived(actualVolumes.find((v) => v.id === selectedVolumeId))

    const dialogTitle = $derived(generateTitle(operationType, fileCount, folderCount))

    const confirmLabel = $derived(operationType === 'copy' ? 'Copy' : 'Move')

    // Format space info for display
    function formatSpaceInfo(space: VolumeSpaceInfo | null): string {
        if (!space) return ''
        return `${formatBytes(space.availableBytes)} free of ${formatBytes(space.totalBytes)}`
    }

    // Load volume space when volume changes
    async function loadVolumeSpace() {
        const volume = selectedVolume
        if (volume) {
            volumeSpace = await getVolumeSpace(volume.path)
        }
    }

    // Update path when volume changes
    function handleVolumeChange() {
        const volume = selectedVolume
        if (volume && !editedPath.startsWith(volume.path)) {
            // Update path to be relative to new volume
            editedPath = volume.path === '/' ? editedPath : volume.path
        }
        void loadVolumeSpace()
    }

    $effect(() => {
        // Watch for volume changes - read the reactive value to track it
        void selectedVolumeId
        handleVolumeChange()
    })

    /** Cleans up event listeners for scan preview. */
    function cleanup() {
        for (const unlisten of unlisteners) {
            unlisten()
        }
        unlisteners = []
    }

    /** Checks for conflicts at the destination. */
    async function checkConflicts() {
        if (isCheckingConflicts || conflictCheckComplete) return

        isCheckingConflicts = true
        try {
            // Build source item info from the source paths
            // For conflict detection, we need the name, size, and modified time of each source item
            // We extract the filename from each path
            const sourceItems: SourceItemInput[] = sourcePaths.map((path) => {
                const name = path.split('/').pop() || path
                return {
                    name,
                    size: 0, // Size not known at this point, but name matching is enough for conflict detection
                    modified: null,
                }
            })

            const maxConflicts = getSetting('fileOperations.maxConflictsToShow')
            const foundConflicts = await scanVolumeForConflicts(destVolumeId, sourceItems, editedPath)

            // Limit the conflicts shown
            conflicts = foundConflicts.slice(0, maxConflicts)
            conflictCheckComplete = true

            if (conflicts.length > 0) {
                log.info('Found {count} conflicts at destination', { count: conflicts.length })
            }
        } catch (err) {
            log.error('Failed to check for conflicts: {error}', { error: err })
            // Don't block the operation on conflict check failure
            conflictCheckComplete = true
        } finally {
            isCheckingConflicts = false
        }
    }

    /** Starts the scan preview to count files/dirs/bytes. */
    async function startScan() {
        // Subscribe to events BEFORE starting scan (avoid race condition)
        unlisteners.push(
            await onScanPreviewProgress((event) => {
                if (event.previewId !== previewId) return
                filesFound = event.filesFound
                dirsFound = event.dirsFound
                bytesFound = event.bytesFound
            }),
        )
        unlisteners.push(
            await onScanPreviewComplete((event) => {
                if (event.previewId !== previewId) return
                filesFound = event.filesTotal
                dirsFound = event.dirsTotal
                bytesFound = event.bytesTotal
                isScanning = false
                scanComplete = true
                // After source scan completes, check for conflicts
                void checkConflicts()
            }),
        )
        unlisteners.push(
            await onScanPreviewError((event) => {
                if (event.previewId !== previewId) return
                isScanning = false
                // Keep showing whatever stats we have
            }),
        )
        unlisteners.push(
            await onScanPreviewCancelled((event) => {
                if (event.previewId !== previewId) return
                isScanning = false
            }),
        )

        // Start the scan
        isScanning = true
        const progressIntervalMs = getSetting('fileOperations.progressUpdateInterval')
        const result = await startScanPreview(sourcePaths, sortColumn, sortOrder, progressIntervalMs)
        previewId = result.previewId
    }

    onMount(async () => {
        // Focus and select the path input
        await tick()
        pathInputRef?.focus()
        pathInputRef?.select()

        // Load initial volume space
        await loadVolumeSpace()

        // Start scanning files immediately
        void startScan()
    })

    onDestroy(() => {
        // Cancel scan preview if still running
        if (previewId && isScanning) {
            void cancelScanPreview(previewId)
        }
        cleanup()
    })

    function handleConfirm() {
        // Pass the previewId and conflict policy so the operation can reuse scan results
        onConfirm(editedPath, selectedVolumeId, previewId, conflictPolicy)
    }

    function handleCancel() {
        // Cancel scan preview if still running
        if (previewId && isScanning) {
            void cancelScanPreview(previewId)
        }
        cleanup()
        onCancel()
    }

    function handleKeydown(event: KeyboardEvent) {
        if (event.key === 'Enter') {
            handleConfirm()
        }
    }

    function handleInputKeydown(event: KeyboardEvent) {
        if (event.key === 'Enter') {
            event.preventDefault()
            event.stopPropagation()
            handleConfirm()
        }
    }
</script>

<ModalDialog
    titleId="dialog-title"
    onkeydown={handleKeydown}
    dialogId="transfer-confirmation"
    onclose={handleCancel}
    containerStyle="min-width: 420px; max-width: 500px"
>
    {#snippet title()}{dialogTitle}{/snippet}

    <!-- Direction indicator -->
    <DirectionIndicator sourcePath={sourceFolderPath} {destinationPath} {direction} />

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

    <!-- Path input -->
    <div class="path-input-group">
        <input
            bind:this={pathInputRef}
            bind:value={editedPath}
            type="text"
            class="path-input"
            aria-label="Destination path"
            spellcheck="false"
            autocomplete="off"
            onkeydown={handleInputKeydown}
        />
    </div>

    <!-- Scan stats (live counting) -->
    <div class="scan-stats">
        <div class="scan-stat">
            <span class="scan-value">{formatBytes(bytesFound)}</span>
        </div>
        <span class="scan-divider">/</span>
        <div class="scan-stat">
            <span class="scan-value">{filesFound}</span>
            <span class="scan-label">{filesFound === 1 ? 'file' : 'files'}</span>
        </div>
        <span class="scan-divider">/</span>
        <div class="scan-stat">
            <span class="scan-value">{dirsFound}</span>
            <span class="scan-label">{dirsFound === 1 ? 'dir' : 'dirs'}</span>
        </div>
        {#if isScanning}
            <span class="scan-spinner"></span>
        {:else if scanComplete}
            <span class="scan-checkmark">✓</span>
        {/if}
    </div>

    <!-- Conflicts section -->
    {#if isCheckingConflicts}
        <div class="conflicts-checking">
            <span class="scan-spinner"></span>
            <span class="conflicts-checking-text">Checking for conflicts...</span>
        </div>
    {:else if conflicts.length > 0}
        <div class="conflicts-section">
            <p class="conflicts-summary">
                {conflicts.length}
                {conflicts.length === 1 ? 'file already exists' : 'files already exist'}
            </p>
            <div class="conflict-policy">
                <label class="policy-option">
                    <input type="radio" bind:group={conflictPolicy} value="skip" />
                    <span>Skip all</span>
                </label>
                <label class="policy-option">
                    <input type="radio" bind:group={conflictPolicy} value="overwrite" />
                    <span>Overwrite all</span>
                </label>
                <label class="policy-option">
                    <input type="radio" bind:group={conflictPolicy} value="stop" />
                    <span>Ask for each</span>
                </label>
            </div>
        </div>
    {/if}

    <!-- Buttons (centered) -->
    <div class="button-row">
        <button class="secondary" onclick={handleCancel}>Cancel</button>
        <button class="primary" onclick={handleConfirm}>{confirmLabel}</button>
    </div>
</ModalDialog>

<style>
    .volume-selector {
        display: flex;
        align-items: center;
        gap: 12px;
        padding: 0 24px;
        margin-bottom: 12px;
    }

    .volume-select {
        flex: 0 0 auto;
        padding: 8px 12px;
        font-size: 13px;
        font-family: var(--font-system) sans-serif;
        background: var(--color-bg-primary);
        border: 1px solid var(--color-border-primary);
        border-radius: 6px;
        color: var(--color-text-primary);
        cursor: pointer;
    }

    .volume-select:focus {
        outline: none;
        border-color: var(--color-accent);
    }

    .space-info {
        font-size: 12px;
        color: var(--color-text-muted);
    }

    .path-input-group {
        padding: 0 24px;
        margin-bottom: 16px;
    }

    .path-input {
        width: 100%;
        padding: 10px 12px;
        font-size: 13px;
        font-family: var(--font-system) sans-serif;
        background: var(--color-bg-primary);
        border: 2px solid var(--color-accent);
        border-radius: 6px;
        color: var(--color-text-primary);
        box-sizing: border-box;
    }

    .path-input::placeholder {
        color: var(--color-text-muted);
    }

    .path-input:focus {
        outline: none;
        box-shadow: 0 0 0 2px rgba(77, 163, 255, 0.2);
    }

    .button-row {
        display: flex;
        gap: 12px;
        justify-content: center;
        padding: 0 24px 20px;
    }

    button {
        padding: 8px 20px;
        border-radius: 6px;
        font-size: 13px;
        font-weight: 500;
        cursor: pointer;
        transition: all 0.15s ease;
        min-width: 80px;
    }

    button:disabled {
        opacity: 0.5;
        cursor: not-allowed;
    }

    .primary {
        background: var(--color-accent);
        color: white;
        border: none;
    }

    .primary:hover:not(:disabled) {
        filter: brightness(1.1);
    }

    .secondary {
        background: transparent;
        color: var(--color-text-secondary);
        border: 1px solid var(--color-border-primary);
    }

    .secondary:hover:not(:disabled) {
        background: var(--color-bg-tertiary);
        color: var(--color-text-primary);
    }

    /* Scan stats */
    .scan-stats {
        display: flex;
        align-items: center;
        justify-content: center;
        gap: 8px;
        padding: 0 24px 16px;
        font-size: 12px;
    }

    .scan-stat {
        display: flex;
        align-items: baseline;
        gap: 4px;
    }

    .scan-value {
        color: var(--color-text-primary);
        font-variant-numeric: tabular-nums;
        font-weight: 500;
    }

    .scan-label {
        color: var(--color-text-muted);
    }

    .scan-divider {
        color: var(--color-text-muted);
    }

    .scan-spinner {
        width: 12px;
        height: 12px;
        border: 2px solid var(--color-accent);
        border-top-color: transparent;
        border-radius: 50%;
        animation: spin 0.8s linear infinite;
        margin-left: 4px;
    }

    @keyframes spin {
        to {
            transform: rotate(360deg);
        }
    }

    .scan-checkmark {
        color: var(--color-allow);
        font-size: 14px;
        font-weight: bold;
        margin-left: 4px;
    }

    /* Conflicts checking */
    .conflicts-checking {
        display: flex;
        align-items: center;
        justify-content: center;
        gap: 8px;
        padding: 0 24px 12px;
        font-size: 12px;
    }

    .conflicts-checking-text {
        color: var(--color-text-muted);
    }

    /* Conflicts section */
    .conflicts-section {
        padding: 0 24px 12px;
        border-top: 1px solid var(--color-border-primary);
        margin-top: 4px;
        padding-top: 12px;
    }

    .conflicts-summary {
        margin: 0 0 12px;
        font-size: 13px;
        color: var(--color-warning);
        text-align: center;
        font-weight: 500;
    }

    .conflict-policy {
        display: flex;
        justify-content: center;
        gap: 16px;
    }

    .policy-option {
        display: flex;
        align-items: center;
        gap: 6px;
        font-size: 12px;
        color: var(--color-text-secondary);
        cursor: pointer;
    }

    .policy-option input[type='radio'] {
        margin: 0;
        cursor: pointer;
    }

    .policy-option:hover {
        color: var(--color-text-primary);
    }
</style>
