<script lang="ts">
    import { onMount, onDestroy, tick } from 'svelte'
    import { slide } from 'svelte/transition'
    import { homeDir } from '@tauri-apps/api/path'
    import { getVolumeSpace, DEFAULT_VOLUME_ID, type VolumeSpaceInfo } from '$lib/tauri-commands'
    import type { SortColumn, SortOrder, ConflictResolution, TransferOperationType } from '$lib/file-explorer/types'
    import { validateDirectoryPath } from '$lib/utils/filename-validation'
    import { createTransferDestExistsCheck } from './transfer-dest-exists.svelte'
    import CompressLevelControl from './CompressLevelControl.svelte'
    import CompressEstimateLine from './CompressEstimateLine.svelte'
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import Button from '$lib/ui/Button.svelte'
    import Select, { type SelectItem } from '$lib/ui/Select.svelte'
    import RadioGroup, { type RadioItem } from '$lib/ui/RadioGroup.svelte'
    import ToggleGroup, { type ToggleGroupOption } from '$lib/ui/ToggleGroup.svelte'
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import {
        confirmLabelKey,
        generateTitle,
        initialEditedPath,
        shouldShowHardlinkNote,
    } from './transfer-dialog-utils'
    import { getPathValidationError, formatSpaceInfo } from './transfer-dialog-logic'
    import { createTransferScanState } from './transfer-scan-state.svelte'
    import { createTransferConflictCheck } from './transfer-conflict-check.svelte'
    import { getVolumes } from '$lib/stores/volume-store.svelte'
    import { formatNumber } from '$lib/file-explorer/selection/selection-info-utils'
    import Size from '$lib/ui/Size.svelte'
    import { getFileSizeFormat } from '$lib/settings/reactive-settings.svelte'
    import { formatFileSizeWithFormat } from '$lib/settings/format-utils'
    import { getAppLogger } from '$lib/logging/logger'
    import Icon from '$lib/ui/Icon.svelte'
    import Spinner from '$lib/ui/Spinner.svelte'
    import { tooltip } from '$lib/tooltip/tooltip'
    import { useShortenMiddle } from '$lib/utils/shorten-middle-action'
    import Trans from '$lib/intl/Trans.svelte'
    import { t, tString } from '$lib/intl/messages.svelte'

    const log = getAppLogger('transferDialog')

    interface Props {
        operationType: TransferOperationType
        sourcePaths: string[]
        destinationPath: string
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
        /** MCP round-trip id, present only for an auto-confirmed MCP op. Only used
         *  by the compress auto-confirm abort (target archive exists → dialog stays
         *  open): the FE then acks the round-trip WITHOUT an operationId, since no
         *  op spawned. The normal spawn reply happens in the progress state. */
        mcpRequestId?: string
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
        mcpRequestId,
        onConfirm,
        onCancel,
    }: Props = $props()

    let activeOperationType = $state<TransferOperationType>(initialOperationType)

    /**
     * How long the compress-only block takes to slide open or shut. Zero when the
     * OS asks for reduced motion, which is the JS twin of the
     * `@media (prefers-reduced-motion: reduce)` rules elsewhere in the app; the
     * `window` guard keeps the module safe under the static adapter's SSR pass.
     */
    const revealMs =
        typeof window !== 'undefined' && window.matchMedia('(prefers-reduced-motion: reduce)').matches ? 0 : 180
    /** False until the first paint, so the block doesn't slide in when the dialog opens in Compress mode. */
    let isOpen = $state(false)

    // The segmented action toggle. Labels resolve reactively so a locale switch
    // re-renders them; the group drives the whole dialog's mode, hence `tabs`
    // semantics rather than `toggles`.
    const operationOptions = $derived<ToggleGroupOption[]>([
        { value: 'copy', label: tString('fileOperations.transferDialog.toggleCopy') },
        { value: 'move', label: tString('fileOperations.transferDialog.toggleMove') },
        { value: 'compress', label: tString('fileOperations.transferDialog.toggleCompress') },
    ])

    // The file-conflict policy radios, `{#each}`-rendered; `as const` keeps the values and label keys literal.
    const conflictPolicies = [
        { value: 'skip', labelKey: 'fileOperations.transferDialog.policySkip' },
        { value: 'overwrite', labelKey: 'fileOperations.transferDialog.policyOverwrite' },
        { value: 'overwrite_smaller', labelKey: 'fileOperations.transferDialog.policyOverwriteSmaller' },
        { value: 'overwrite_older', labelKey: 'fileOperations.transferDialog.policyOverwriteOlder' },
        { value: 'stop', labelKey: 'fileOperations.transferDialog.policyStop' },
    ] as const

    // Compute initial volume-relative path. Can't use $derived selectedVolume here (not yet available),
    // so look up the volume path directly from the props.
    const initialVolumePath = volumes.find((v) => v.id === currentVolumeId)?.path ?? '/'
    let editedPath = $state(
        initialEditedPath(initialOperationType, destinationPath, initialVolumePath, sourcePaths, sourceFolderPath),
    )
    log.debug(
        'Initial path resolution: destinationPath={destinationPath}, currentVolumeId={currentVolumeId}, initialVolumePath={initialVolumePath}, editedPath={editedPath}',
        {
            destinationPath,
            currentVolumeId,
            initialVolumePath,
            editedPath,
        },
    )
    let selectedVolumeId = $state(currentVolumeId)
    let pathInputRef: HTMLInputElement | undefined = $state()

    // The user's home dir as an absolute path (no trailing slash), resolved on
    // mount. Used to show home as its long form instead of a bare `~`.
    let userHomePath = $state('')

    // Volume space info
    let volumeSpace = $state<VolumeSpaceInfo | null>(null)

    // Whether the user confirmed (so we don't cancel the scan on destroy)
    let confirmed = false
    let destroyed = false

    // Map MCP onConflict string to ConflictResolution, or default to "ask for each"
    const autoConfirmConflictMap: Record<string, ConflictResolution> = {
        skip_all: 'skip',
        overwrite_all: 'overwrite',
        rename_all: 'rename',
        overwrite_all_smaller: 'overwrite_smaller',
        overwrite_all_older: 'overwrite_older',
    }
    let conflictPolicy = $state<ConflictResolution>(
        autoConfirm && autoConfirmOnConflict ? (autoConfirmConflictMap[autoConfirmOnConflict] ?? 'skip') : 'stop',
    ) // Default to "ask for each" unless auto-confirming

    // Filter to only actual volumes (not favorites)
    const actualVolumes = $derived(volumes.filter((v) => v.category !== 'favorite' && v.category !== 'network'))

    const volumeItems = $derived<SelectItem[]>(actualVolumes.map((v) => ({ value: v.id, label: v.name })))

    // Get selected volume info
    const selectedVolume = $derived(actualVolumes.find((v) => v.id === selectedVolumeId))

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
        activeOperationType === 'move' && sourceVolumeId !== DEFAULT_VOLUME_ID && sourceVolumeId === selectedVolumeId,
    )

    // Deep scan-preview orchestration (Size bar + file/dir tallies). The factory
    // owns the scan listeners, the start/cancel lifecycle, and the Copy/Move
    // toggle effect that (re)starts or cancels the preview around a same-volume
    // move. Created synchronously here (component init) so its internal `$effect`
    // lands in the effect-tracking context (L3).
    const scan = createTransferScanState({
        getSourcePaths: () => sourcePaths,
        getSortColumn: () => sortColumn,
        getSortOrder: () => sortOrder,
        getSourceVolumeId: () => sourceVolumeId,
        getIsSameVolumeMove: () => isSameVolumeMove,
        getConfirmed: () => confirmed,
        getDestroyed: () => destroyed,
        getSampleForEstimate: () => activeOperationType === 'compress',
    })

    // Cheap top-level conflict check (one dest listing). Runs in parallel with the
    // deep scan and stays decoupled from it, so a same-volume move can cancel the
    // deep preview while still surfacing merges + the file-policy radios.
    const conflicts = createTransferConflictCheck({
        getSelectedVolumeId: () => selectedVolumeId,
        getSourcePaths: () => sourcePaths,
        getEditedPath: () => editedPath,
        getSourceVolumeId: () => sourceVolumeId,
        getDestroyed: () => destroyed,
        log,
    })

    // Local aliases over the factory getters so the markup reads the same names
    // it always has. Each tracks the factory's reactive `$state`, so the template
    // updates exactly as before.
    const bytesFound = $derived(scan.bytesFound)
    const dedupBytesFound = $derived(scan.dedupBytesFound)
    const filesFound = $derived(scan.filesFound)
    const dirsFound = $derived(scan.dirsFound)
    const isScanning = $derived(scan.isScanning)
    const scanComplete = $derived(scan.scanComplete)
    const totalConflictCount = $derived(conflicts.totalConflictCount)
    const mergeFolderCount = $derived(conflicts.mergeFolderCount)
    const hasTypeMismatchConflict = $derived(conflicts.hasTypeMismatchConflict)
    const isCheckingConflicts = $derived(conflicts.isCheckingConflicts)

    // File-conflict policy options for `RadioGroup`. The label pluralizes on the
    // live conflict count ("Skip" vs "Skip all"), so the items rebuild reactively.
    const conflictPolicyItems = $derived<RadioItem[]>(
        conflictPolicies.map((policy) => ({
            value: policy.value,
            label: tString(policy.labelKey, { count: totalConflictCount }),
        })),
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

    const confirmLabel = $derived(tString(confirmLabelKey(activeOperationType)))

    /** Counting state for the tallies element, exposed as `data-scan-state` so
     *  E2E tests can wait race-free for the scan to settle before asserting the
     *  counter line (no new wire event — this is the existing `scanComplete` /
     *  `isSameVolumeMove` state surfaced to the DOM):
     *   - `done`     → the deep scan finished; the tallies are final.
     *   - `skipped`  → no deep scan runs (a same-volume move renames server-side,
     *                  zero bytes), so the tallies legitimately stay at 0 — there's
     *                  nothing to count.
     *   - `counting` → a scan is in flight (or about to start on mount).
     *  `done` wins over `skipped`: a same-volume COPY still scans and completes. */
    const scanState = $derived<'counting' | 'done' | 'skipped'>(
        scanComplete ? 'done' : isSameVolumeMove ? 'skipped' : 'counting',
    )

    const pathError = $derived.by(() => {
        const structural = validateDirectoryPath(editedPath)
        if (structural.severity === 'error') return structural.message
        return getPathValidationError(sourcePaths, editedPath, activeOperationType)
    })

    // Destination-existence check (debounced, async) behind the yellow "will be
    // created" warning. Created synchronously here (component init) so its internal
    // `$effect` lands in the effect-tracking context, matching the scan/conflict
    // factories above.
    const destExists = createTransferDestExistsCheck({
        getEditedPath: () => editedPath,
        getSelectedVolumeId: () => selectedVolumeId,
        getDestroyed: () => destroyed,
        log,
    })

    // Inline path warning beneath the box (red path error always wins). Copy/move:
    // "folder will be created" (backend auto-creates via `create_directory_all`).
    // Compress targets a new zip FILE, so the inverse — replacing an existing file.
    const targetWarning = $derived.by<string | null>(() => {
        if (pathError) return null
        if (activeOperationType === 'compress')
            return destExists.targetExists ? tString('fileOperations.transferDialog.targetWillBeOverwritten') : null
        if (!destExists.targetMissing) return null
        return tString(
            activeOperationType === 'copy'
                ? 'fileOperations.transferDialog.targetWillBeCreatedCopy'
                : 'fileOperations.transferDialog.targetWillBeCreatedMove',
        )
    })

    // Free-space text is intentionally uncolored: red GB would falsely signal "low space".
    const spaceInfoText = $derived(
        formatSpaceInfo(volumeSpace, (bytes) => formatFileSizeWithFormat(bytes, getFileSizeFormat())),
    )

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

    onMount(async () => {
        // Opening straight into Compress shouldn't animate: the slide is feedback
        // for a mode SWITCH, not a reveal the user is waiting through on open.
        isOpen = true

        // Resolve the user's home dir, then show a destination that's exactly home
        // as its long absolute form (`/Users/me`) rather than a bare `~`, which
        // reads as a glitch. A `~/sub` path keeps its short form; the backend
        // expands `~` on execution. Done before the scan/conflict check so they
        // run against the absolute path.
        try {
            const home = await homeDir()
            userHomePath = home.endsWith('/') ? home.slice(0, -1) : home
        } catch (err) {
            log.warn('Could not resolve home dir for the destination box: {error}', { error: err })
        }
        if (editedPath === '~' && userHomePath) {
            editedPath = userHomePath
        }

        // Focus and select the path input
        await tick()
        pathInputRef?.focus()
        pathInputRef?.select()

        // Volume space is loaded by the $effect watching selectedVolumeId

        // Start the deep scan preview immediately — UNLESS this is a same-volume
        // move, where the backend does a server-side rename (zero bytes) and the
        // recursive byte scan is pure waste (the 30–40 s "Verifying before move…"
        // this fast path eliminates). The scan factory tracks the promise so
        // handleConfirm can await it: this ensures previewId is set before
        // onConfirm fires.
        scan.start()

        // Run the cheap top-level conflict check in parallel with the scan preview
        // (one dest listing, not the recursive byte scan). Assigned BEFORE the
        // auto-confirm branch so the fast path's `handleConfirm` await guard sees a
        // real promise. Compress makes ONE new file, so multi-file dest conflicts
        // are meaningless — it skips the check and uses the dest-exists affordance.
        conflictCheckPromise = activeOperationType === 'compress' ? null : conflicts.check()

        // Auto-confirm if MCP requested it (after a tick so the dialog is fully initialized)
        if (autoConfirm) {
            await tick()
            await handleConfirm(true)
        }
    })

    onDestroy(() => {
        destroyed = true
        destExists.cancel()
        // Free the scan preview unless the user confirmed (then the
        // TransferProgressDialog / the started op takes over the same scan and
        // consumes the cached result). We call this regardless of `isScanning`:
        // `cancelScanPreview` also evicts the cached `CachedScanResult`, so a
        // dialog dismissed AFTER the scan completed doesn't leak the cache until
        // quit.
        if (!confirmed) {
            scan.freeAndCleanup()
        } else {
            // Confirmed: the progress dialog consumes the scan, so only drop our
            // listeners without cancelling the (still-needed) preview.
            scan.cleanup()
        }
    })

    async function handleConfirm(isAuto = false) {
        if (pathError || confirmed) return
        confirmed = true
        // Compress auto-confirm must not silently overwrite an existing archive:
        // proceed unattended only when the target doesn't exist; else stay open.
        if (activeOperationType === 'compress' && isAuto && (await destExists.probeExists())) {
            confirmed = false
            // Ack the MCP round-trip WITHOUT an operationId: no op spawned, the
            // dialog stays open for the user to confirm the overwrite.
            if (mcpRequestId) {
                const { emit } = await import('@tauri-apps/api/event')
                void emit('mcp-response', { requestId: mcpRequestId, ok: true })
            }
            return
        }
        // Same-volume move: dispatch IMMEDIATELY. No deep scan ever ran (the
        // backend renames server-side, zero bytes), so there's nothing to wait
        // for and no cached preview to consume — pass `previewId = null` and
        // `scanInProgress = false`. Await the conflict check for `conflictNames`.
        if (isSameVolumeMove) {
            scan.cancelPreview()
            await conflictCheckPromise
            onConfirm(
                editedPath,
                selectedVolumeId,
                null,
                conflictPolicy,
                activeOperationType,
                false,
                conflicts.conflictNames,
            )
            return
        }
        // Wait for startScanPreview IPC so previewId is set (a fast confirm — MCP,
        // Playwright, rapid Enter — otherwise strands the progress dialog with a
        // null previewId), then for the conflict scan (`await null` is a no-op, so
        // compress falls straight through) so we never dispatch `conflicts: []`.
        await scan.scanStarted
        await conflictCheckPromise
        onConfirm(
            editedPath,
            selectedVolumeId,
            scan.previewId,
            conflictPolicy,
            activeOperationType,
            scan.isScanning,
            conflicts.conflictNames,
        )
    }

    function handleCancel() {
        // Free the scan preview (cancels an in-flight scan and evicts any cached
        // result). Regardless of `isScanning`, so a dismiss after the scan
        // completed doesn't leak the cache.
        scan.freeAndCleanup()
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
    padded={false}
    growDownward
>
    {#snippet title()}{dialogTitle}{/snippet}

    <div class="dialog-body">
        <!-- Copy / Move / Compress -->
        <div class="operation-toggle">
            <ToggleGroup
                semantics="tabs"
                value={activeOperationType}
                options={operationOptions}
                onChange={(next: string) => (activeOperationType = next as TransferOperationType)}
                ariaLabel={tString('fileOperations.transferDialog.operationAria')}
                fullWidth
            />
        </div>

        <!-- Where the items come from: the full source path, middle-shortened when
             it's too long for the row (the tail carries the meaning). -->
        <div class="source-group">
            <SectionCard label={tString('fileOperations.transferDialog.sourceGroupTitle')}>
                <!-- The action writes the text and carries the full path as its `title`. -->
                <div class="source-path" use:useShortenMiddle={{ text: sourceFolderPath, preferBreakAt: '/' }}></div>
            </SectionCard>
        </div>

        <!-- Where the items go: volume, then the editable path on that volume. -->
        <div class="target-group">
            <SectionCard label={tString('fileOperations.transferDialog.targetGroupTitle')}>
                <div class="target-card-body">
                    <div class="volume-selector">
                        <div class="volume-select">
                            <Select
                                items={volumeItems}
                                value={selectedVolumeId}
                                ariaLabel={tString('fileOperations.transferDialog.destVolumeAria')}
                                onChange={(id: string) => {
                                    selectedVolumeId = id
                                }}
                            />
                        </div>
                        {#if volumeSpace}
                            <span class="space-info">{spaceInfoText}</span>
                        {/if}
                    </div>

                    {#if selectedVolume?.smbConnectionState === 'os_mount'}
                        <p class="smb-native-note">
                            {tString('fileOperations.transferDialog.smbNativeNote')}
                        </p>
                    {/if}

                    <div class="path-input-group">
                        <input
                            bind:this={pathInputRef}
                            bind:value={editedPath}
                            type="text"
                            class="path-input"
                            class:has-error={!!pathError}
                            class:has-warning={targetWarning}
                            aria-label={tString('fileOperations.transferDialog.destPathAria')}
                            aria-describedby={pathError
                                ? 'transfer-path-error'
                                : targetWarning
                                  ? 'transfer-path-warning'
                                  : undefined}
                            aria-invalid={!!pathError}
                            spellcheck="false"
                            autocomplete="off"
                            onkeydown={handleInputKeydown}
                        />
                        {#if pathError}
                            <p id="transfer-path-error" class="path-error" role="alert">{pathError}</p>
                        {:else if targetWarning}
                            <p id="transfer-path-warning" class="path-warning">{targetWarning}</p>
                        {/if}
                    </div>
                </div>
            </SectionCard>
        </div>

        <!-- Scan stats (live counting) -->
        <div class="scan-stats" data-scan-state={scanState}>
            <div class="scan-stat">
                <span class="scan-value"><Size bytes={bytesFound} /></span>
            </div>
            <span class="scan-divider">/</span>
            <div class="scan-stat">
                <span class="scan-value">{formatNumber(filesFound)}</span>
                <span class="scan-label">{t('fileOperations.transferDialog.scanFile', { count: filesFound })}</span>
            </div>
            <span class="scan-divider">/</span>
            <div class="scan-stat">
                <span class="scan-value">{formatNumber(dirsFound)}</span>
                <span class="scan-label">{t('fileOperations.transferDialog.scanDir', { count: dirsFound })}</span>
            </div>
            {#if isScanning}
                <span
                    class="scan-status"
                    role="img"
                    aria-label={tString('fileOperations.shared.scanningTooltip')}
                    use:tooltip={{ text: tString('fileOperations.shared.scanningTooltip') }}
                >
                    <Spinner size="sm" />
                </span>
            {/if}
        </div>

        <!-- Compression level + live estimated size: Compress mode only. The wrapper
             owns the side inset for both children and gives the mode switch one
             element to slide, so the dialog's height change reads as a reveal. -->
        {#if activeOperationType === 'compress'}
            <div class="compress-extras" transition:slide={{ duration: isOpen ? revealMs : 0 }}>
                <CompressEstimateLine estimate={scan.estimatedBytes} {isScanning} sourceIsLocal={sourceVolumeId === DEFAULT_VOLUME_ID} />
                <CompressLevelControl />
            </div>
        {/if}

        <!-- Hardlink note: copy writes every hardlink as a full file, so the bytes
         written exceed the source's on-disk size. Clarify the gap so the
         headline size doesn't look wrong against Finder's number. Copy-only:
         a same-filesystem move renames in place and writes nothing. -->
        {#if showHardlinkNote}
            <p class="hardlink-note">
                <Trans key="fileOperations.transferDialog.hardlinkNote" snippets={{ written, ondisk }} />
            </p>
        {/if}

        <!-- Conflicts section -->
        {#if isCheckingConflicts}
            <div class="conflicts-checking">
                <Spinner size="sm" />
                <span class="conflicts-checking-text">{tString('fileOperations.transferDialog.checkingConflicts')}</span
                >
            </div>
        {:else if totalConflictCount > 0 || mergeFolderCount > 0}
            <div class="conflicts-section">
                <!-- Folder merges are informational, never a question: same-named
                 folders always merge silently. Surfaced so a user who didn't
                 expect a same-named folder at the dest gets a visible cue. -->
                {#if mergeFolderCount > 0}
                    <p class="merge-info">
                        {mergeFolderCount === 1
                            ? tString('fileOperations.transferDialog.mergeInfoSingle')
                            : tString('fileOperations.transferDialog.mergeInfoMany', {
                                  countText: formatNumber(mergeFolderCount),
                              })}
                    </p>
                {/if}
                {#if totalConflictCount > 0}
                    <p class="conflicts-summary">
                        {t('fileOperations.transferDialog.conflictsSummary', {
                            countText: String(totalConflictCount),
                            count: totalConflictCount,
                        })}
                    </p>
                {/if}
                <!-- The file policy radios show whenever there's a file conflict OR
                 a folder merge: a merge can surface file clashes mid-operation
                 the upfront check can't see, and the radios pre-answer them. -->
                <div class="conflict-policy">
                    <RadioGroup
                        items={conflictPolicyItems}
                        value={conflictPolicy}
                        onValueChange={(v) => (conflictPolicy = v as ConflictResolution)}
                        orientation="horizontal"
                    />
                </div>

                <!-- Cross-type guardrail: when a clash mixes a file and a same-named
                 folder, "Overwrite all" replaces items of a different type and
                 deletes folder contents. The per-file dialog already warns on
                 this; the bulk path must not be quieter. -->
                {#if hasTypeMismatchConflict && conflictPolicy === 'overwrite'}
                    <p class="conflict-warning" role="alert">
                        <span class="conflict-warning-icon" aria-hidden="true">
                            <Icon name="triangle-alert" size={16} />
                        </span>
                        <span>
                            {tString('fileOperations.transferDialog.typeMismatchWarning')}
                        </span>
                    </p>
                {/if}
            </div>
        {/if}
    </div>

    {#snippet footer()}
        <Button variant="secondary" onclick={handleCancel}>{tString('fileOperations.button.cancel')}</Button>
        <Button variant="primary" onclick={() => handleConfirm()} disabled={!!pathError}>{confirmLabel}</Button>
    {/snippet}
</ModalDialog>

{#snippet written(children: import('svelte').Snippet)}<Size bytes={bytesFound} />{@render children()}{/snippet}
{#snippet ondisk(children: import('svelte').Snippet)}<Size bytes={dedupBytesFound} />{@render children()}{/snippet}

<style>
    /* Uniform vertical rhythm: every top-level section is a flex-column child, so a
       single `gap` sets equal spacing between all of them. Each section keeps its own
       `--spacing-dialog` side inset (matching the title bar and footer). */
    .dialog-body {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-lg);
    }

    /* "Source" and "Target" are the dialog's two halves: where the items are now and
       where they're going. Both are framed by the house `ui/SectionCard`; these
       wrappers only supply the dialog's side inset, which the card doesn't carry. */
    .source-group,
    .target-group {
        padding: 0 var(--spacing-dialog);
    }

    /* A block, not a flex child: `useShortenMiddle` measures `clientWidth`, so the
       element has to be sized by the layout rather than by its (initially empty) text.
       Same `--font-size-md` as the destination path box, so the two paths read as a
       matched pair. */
    .source-path {
        min-width: 0;
        overflow: hidden;
        white-space: nowrap;
        font-size: var(--font-size-md);
        color: var(--color-text-primary);
    }

    .target-card-body {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-md);
    }

    .volume-selector {
        display: flex;
        align-items: center;
        gap: var(--spacing-md);
    }

    /* Wrapper that bounds the `ui/Select` trigger (its trigger is width: 100%)
       so the dropdown stays content-sized next to the free-space text rather
       than stretching across the whole dialog. */
    .volume-select {
        flex: 0 0 auto;
        min-width: 200px;
    }

    /* Pushed to the far edge of the volume row, so the free-space readout lines up
       with the right edge of the path box below it. */
    .space-info {
        margin-left: auto;
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
    }

    .path-input-group {
        /* Side inset comes from `.target-group`. */
        min-width: 0;
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

    /* Yellow "folder will be created" warning: mirrors the red error treatment
       (outlined input + message line) in the warning color. The error state takes
       precedence, so the two never show at once. `--color-warning-text` is the
       AA-safe text token (the brand `--color-warning` is reserved for the border
       and fills). */
    .path-input.has-warning {
        border-color: var(--color-warning);
    }

    .path-input.has-warning:focus {
        box-shadow: 0 0 0 3px color-mix(in srgb, var(--color-warning), transparent 85%);
    }

    .path-warning {
        margin: var(--spacing-sm) 0 0;
        font-size: var(--font-size-sm);
        color: var(--color-warning-text);
    }

    .smb-native-note {
        margin: 0;
        padding: var(--spacing-xs) var(--spacing-sm);
        font-size: var(--font-size-xs);
        color: var(--color-warning-text);
        background: var(--color-warning-bg);
        border-radius: var(--radius-sm);
    }

    /* Scan stats */
    /* Right-aligned so the tallies sit under the dialog's right edge and don't
       compete with the left-aligned labels above them. */
    .scan-stats {
        display: flex;
        align-items: center;
        justify-content: flex-end;
        gap: var(--spacing-sm);
        padding: 0 var(--spacing-dialog);
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

    .scan-status {
        display: inline-flex;
        align-items: center;
    }

    /* Conflicts checking */
    .conflicts-checking {
        display: flex;
        align-items: center;
        justify-content: flex-start;
        gap: var(--spacing-sm);
        padding: 0 var(--spacing-dialog);
        font-size: var(--font-size-sm);
    }

    .conflicts-checking-text {
        color: var(--color-text-tertiary);
    }

    /* Conflicts section */
    .conflicts-section {
        padding: 0 var(--spacing-dialog) var(--spacing-md);
        border-top: 1px solid var(--color-border-strong);
        margin-top: var(--spacing-xs);
        padding-top: var(--spacing-md);
    }

    .conflicts-summary {
        margin: 0 0 var(--spacing-md);
        font-size: var(--font-size-md);
        color: var(--color-warning);
        font-weight: 500;
    }

    /* Folder-merge info line: neutral, not a warning. Folders always merge, so
       this is a heads-up, not a question. */
    .merge-info {
        margin: 0 0 var(--spacing-md);
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
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
        margin-top: 1px;
    }

    /* Wrapper hook for the policy radios (E2E + component tests target
       `.conflict-policy`); `RadioGroup` owns the option layout. The column flex
       stretches the group to full width so its horizontal options wrap. */
    .conflict-policy {
        display: flex;
        flex-direction: column;
    }

    /* Compress-only block. Carries the `--spacing-dialog` side inset its two children
       don't (they're also used where the parent insets them), and stacks them with
       the same rhythm as the dialog body. */
    .compress-extras {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-lg);
        padding: 0 var(--spacing-dialog);
    }

    /* Copy / Move / Compress segmented control. Only the side inset lives here; the
       control itself is the `ui/ToggleGroup` primitive, full-width so it spans the
       same column as the fields below it. */
    .operation-toggle {
        padding: 0 var(--spacing-dialog);
    }
</style>
