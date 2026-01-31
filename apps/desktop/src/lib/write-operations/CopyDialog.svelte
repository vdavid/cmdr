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
        type VolumeSpaceInfo,
        type UnlistenFn,
    } from '$lib/tauri-commands'
    import type { VolumeInfo, SortColumn, SortOrder } from '$lib/file-explorer/types'
    import { getSetting } from '$lib/settings'
    import DirectionIndicator from './DirectionIndicator.svelte'
    import { generateTitle } from './copy-dialog-utils'

    interface Props {
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
        onConfirm: (destination: string, volumeId: string, previewId: string | null) => void
        onCancel: () => void
    }

    const {
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
        onConfirm,
        onCancel,
    }: Props = $props()

    let editedPath = $state(destinationPath)
    let selectedVolumeId = $state(currentVolumeId)
    let overlayElement: HTMLDivElement | undefined = $state()
    let pathInputRef: HTMLInputElement | undefined = $state()

    // Dragging state
    let dialogPosition = $state({ x: 0, y: 0 })
    let isDragging = $state(false)

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

    // Filter to only actual volumes (not favorites)
    const actualVolumes = $derived(volumes.filter((v) => v.category !== 'favorite' && v.category !== 'network'))

    // Get selected volume info
    const selectedVolume = $derived(actualVolumes.find((v) => v.id === selectedVolumeId))

    // Generate dynamic title with proper pluralization
    const title = $derived(generateTitle(fileCount, folderCount))

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
        // Focus overlay for keyboard events
        await tick()
        overlayElement?.focus()

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
        // Pass the previewId so copy operation can reuse scan results
        onConfirm(editedPath, selectedVolumeId, previewId)
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
        event.stopPropagation()
        if (event.key === 'Escape') {
            handleCancel()
        } else if (event.key === 'Enter') {
            handleConfirm()
        }
    }

    function handleInputKeydown(event: KeyboardEvent) {
        event.stopPropagation()
        if (event.key === 'Escape') {
            handleCancel()
        } else if (event.key === 'Enter') {
            event.preventDefault()
            handleConfirm()
        }
    }

    // Drag handling for movable dialog
    function handleTitleMouseDown(event: MouseEvent) {
        if ((event.target as HTMLElement).tagName === 'BUTTON') return // Don't drag when clicking buttons

        event.preventDefault()
        isDragging = true

        const startX = event.clientX - dialogPosition.x
        const startY = event.clientY - dialogPosition.y

        const handleMouseMove = (e: MouseEvent) => {
            dialogPosition = {
                x: e.clientX - startX,
                y: e.clientY - startY,
            }
        }

        const handleMouseUp = () => {
            isDragging = false
            document.removeEventListener('mousemove', handleMouseMove)
            document.removeEventListener('mouseup', handleMouseUp)
            document.body.style.cursor = ''
        }

        document.addEventListener('mousemove', handleMouseMove)
        document.addEventListener('mouseup', handleMouseUp)
        document.body.style.cursor = 'move'
    }
</script>

<div
    bind:this={overlayElement}
    class="modal-overlay"
    role="dialog"
    aria-modal="true"
    aria-labelledby="dialog-title"
    tabindex="-1"
    onkeydown={handleKeydown}
>
    <div
        class="copy-dialog"
        class:dragging={isDragging}
        style="transform: translate({dialogPosition.x}px, {dialogPosition.y}px)"
    >
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <!-- Draggable title bar -->
        <div class="dialog-title-bar" onmousedown={handleTitleMouseDown}>
            <h2 id="dialog-title">{title}</h2>
        </div>

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

        <!-- Buttons (centered) -->
        <div class="button-row">
            <button class="secondary" onclick={handleCancel}>Cancel</button>
            <button class="primary" onclick={handleConfirm}>Copy</button>
        </div>
    </div>
</div>

<style>
    .modal-overlay {
        position: fixed;
        inset: 0;
        background: rgba(0, 0, 0, 0.4);
        display: flex;
        align-items: center;
        justify-content: center;
        z-index: 9999;
        /* No backdrop-filter blur - user needs to see content behind */
    }

    .copy-dialog {
        background: var(--color-bg-secondary);
        border: 1px solid var(--color-border-primary);
        border-radius: 12px;
        min-width: 420px;
        max-width: 500px;
        box-shadow: 0 16px 48px rgba(0, 0, 0, 0.4);
        position: relative;
    }

    .copy-dialog.dragging {
        cursor: move;
    }

    .dialog-title-bar {
        padding: 16px 24px 8px;
        cursor: move;
        user-select: none;
    }

    h2 {
        margin: 0;
        font-size: 16px;
        font-weight: 600;
        color: var(--color-text-primary);
        text-align: center;
    }

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
        font-family: var(--font-system);
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
        font-family: var(--font-system);
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
</style>
