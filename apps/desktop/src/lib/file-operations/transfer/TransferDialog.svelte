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
    import { validateDirectoryPath } from '$lib/utils/filename-validation'
    import DirectionIndicator from './DirectionIndicator.svelte'
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import Button from '$lib/ui/Button.svelte'
    import { generateTitle, toVolumeRelativePath } from './transfer-dialog-utils'
    import { getAppLogger } from '$lib/logging/logger'

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
        /** When true, shows a copy/move segmented control (for drag-and-drop). */
        allowOperationToggle?: boolean
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
        ) => void
        onCancel: () => void
    }

    const {
        operationType: initialOperationType,
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
        sourceVolumeId,
        // eslint-disable-next-line @typescript-eslint/no-unused-vars -- Part of Props interface, used by parent
        destVolumeId,
        allowOperationToggle = false,
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

    // Whether the user confirmed (so we don't cancel the scan on destroy)
    let confirmed = false

    // Conflict detection state
    let conflicts = $state<VolumeConflictInfo[]>([])
    let isCheckingConflicts = $state(false)
    let conflictCheckComplete = $state(false)
    // Map MCP onConflict string to ConflictResolution, or default to "ask for each"
    const autoConfirmConflictMap: Record<string, ConflictResolution> = {
        skip_all: 'skip',
        overwrite_all: 'overwrite',
        rename_all: 'rename',
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

    const dialogTitle = $derived(generateTitle(activeOperationType, fileCount, folderCount))

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

    // Format space info for display
    function formatSpaceInfo(space: VolumeSpaceInfo | null): string {
        if (!space) return ''
        return `${formatBytes(space.availableBytes)} free of ${formatBytes(space.totalBytes)}`
    }

    // Load volume space when volume changes
    async function loadVolumeSpace() {
        const volume = selectedVolume
        if (volume) {
            volumeSpace = (await getVolumeSpace(volume.path)).data
        }
    }

    // Reset to volume root when volume changes — the current path is meaningless on a different volume
    function handleVolumeChange() {
        editedPath = '/'
        void loadVolumeSpace()
    }

    let isInitialVolumeEffect = true
    $effect(() => {
        // Watch for volume changes - read the reactive value to track it
        void selectedVolumeId
        if (isInitialVolumeEffect) {
            // Skip the first run — editedPath is already initialized with the correct volume-relative path.
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
            const foundConflicts = await scanVolumeForConflicts(selectedVolumeId, sourceItems, editedPath)

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

    /** Accepts the event if it belongs to our scan, filtering stale events from previous scans. */
    function isOurScanEvent(eventPreviewId: string): boolean {
        // previewId may still be null if the scan completes before startScanPreview returns.
        // In that case, adopt the first event's previewId (it's from the scan we just started).
        if (!previewId) previewId = eventPreviewId
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
                isScanning = false
                scanComplete = true
                // After source scan completes, check for conflicts
                void checkConflicts()
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
    }

    onMount(async () => {
        // Focus and select the path input
        await tick()
        pathInputRef?.focus()
        pathInputRef?.select()

        // Volume space is loaded by the $effect watching selectedVolumeId

        // Start scanning files immediately
        void startScan()

        // Auto-confirm if MCP requested it (after a tick so the dialog is fully initialized)
        if (autoConfirm) {
            await tick()
            handleConfirm()
        }
    })

    onDestroy(() => {
        // Cancel scan preview if still running — but only if the user cancelled, not confirmed.
        // On confirm, the TransferProgressDialog takes over listening to the same scan.
        if (previewId && isScanning && !confirmed) {
            void cancelScanPreview(previewId)
        }
        cleanup()
    })

    function handleConfirm() {
        if (pathError) return
        confirmed = true
        // Pass the previewId, conflict policy, (possibly toggled) operation type, and whether scan is still running
        onConfirm(editedPath, selectedVolumeId, previewId, conflictPolicy, activeOperationType, isScanning)
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

    <!-- Copy/Move toggle (shown for drag-and-drop, where the user hasn't chosen yet) -->
    {#if allowOperationToggle}
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
    {/if}

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

    .conflict-policy {
        display: flex;
        justify-content: center;
        gap: var(--spacing-lg);
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
