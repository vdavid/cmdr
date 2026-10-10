<script lang="ts">
    import { onMount, onDestroy, tick } from 'svelte'
    import { slide } from 'svelte/transition'
    import { homeDir } from '@tauri-apps/api/path'
    import { getVolumeSpace, DEFAULT_VOLUME_ID, type SpaceInfo } from '$lib/tauri-commands'
    import type { SortColumn, SortOrder, ConflictResolution, TransferOperationType } from '$lib/file-explorer/types'
    import type { ConfirmOptions, TransferConfirmPayload, TransferConfirmer } from '$lib/file-explorer/pane/dialog-props'
    import StartInBackgroundButton from '../StartInBackgroundButton.svelte'
    import { isStartInBackgroundKey } from '../start-in-background-key'
    import {
        validateDirectoryPath,
        validateDisallowedChars,
        validateNameLength,
        validateNotEmpty,
    } from '$lib/utils/filename-validation'
    import { createTransferDestExistsCheck } from './transfer-dest-exists.svelte'
    import { conflictPolicyFromMcpName } from './conflict-policy'
    import CompressLevelControl from './CompressLevelControl.svelte'
    import CompressEstimateLine from './CompressEstimateLine.svelte'
    import S3CostLine from '../S3CostLine.svelte'
    import { costRequestFor } from '../s3-cost-line'
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import TextInput from '$lib/ui/TextInput.svelte'
    import Button from '$lib/ui/Button.svelte'
    import Select, { type SelectItem } from '$lib/ui/Select.svelte'
    import RadioGroup, { type RadioItem } from '$lib/ui/RadioGroup.svelte'
    import ToggleGroup, { type ToggleGroupOption } from '$lib/ui/ToggleGroup.svelte'
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import {
        confirmLabelKey,
        generateTitle,
        getFolderName,
        initialEditedPath,
        editedPathAfterOperationChange,
        joinPathLeaf,
        shouldShowHardlinkNote,
        splitPathLeaf,
        toVolumeRelativePath,
    } from './transfer-dialog-utils'
    import { getPathValidationError, formatSpaceInfo } from './transfer-dialog-logic'
    import { createTransferScanState } from './transfer-scan-state.svelte'
    import { createTransferConflictCheck } from './transfer-conflict-check.svelte'
    import { getVolumes } from '$lib/stores/volume-store.svelte'
    import { capabilitiesFor } from '$lib/file-explorer/pane/volume-capabilities'
    import { formatNumber } from '$lib/file-explorer/selection/selection-info-utils'
    import Size from '$lib/ui/Size.svelte'
    import { formatByteSize } from '$lib/units'
    import { getAppLogger } from '$lib/logging/logger'
    import Icon from '$lib/ui/Icon.svelte'
    import Spinner from '$lib/ui/Spinner.svelte'
    import { tooltip } from '$lib/tooltip/tooltip'
    import { useShortenMiddle } from '$lib/utils/shorten-middle-action'
    import Trans from '$lib/intl/Trans.svelte'
    import { t, tString } from '$lib/intl/messages.svelte'
    import { isRenameInPlace, resolveTransferTarget } from './transfer-target'
    import { dependOn } from '$lib/utils/reactivity'

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
        /**
         * Rename mode: the ONE source moves to this name in place (F2 on a big S3
         * folder, `pane/rename-as-move.ts`). The path box holds folder + name, the
         * Copy/Move/Compress toggle is gone, and confirm splits the box back.
         */
        newName?: string
        onConfirm: (payload: TransferConfirmPayload) => void
        /** Takes this dialog's own confirm for as long as it's mounted, so an MCP
         *  `dialog confirm` presses the same button a person does. Returns the
         *  unregister. */
        registerConfirmer?: (confirm: TransferConfirmer) => () => void
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
        newName,
        onConfirm,
        registerConfirmer,
        onCancel,
    }: Props = $props()

    const isRenameMode = newName !== undefined

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
    // re-renders them. `toggles` semantics, not `tabs`: there are no tab panels here,
    // so "toggle button, Compress, pressed" is the honest announcement — `tabs` would
    // promise a "tab 1 of 3" structure that doesn't exist.
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
    const initialFolderOrTarget = initialEditedPath(
        initialOperationType,
        destinationPath,
        initialVolumePath,
        sourcePaths,
        sourceFolderPath,
    )
    let editedPath = $state(
        newName === undefined ? initialFolderOrTarget : joinPathLeaf(toVolumeRelativePath(destinationPath, initialVolumePath), newName),
    )
    /** Rename mode's path box, split back into the folder it lands in and the new name. */
    const renameTarget = $derived(splitPathLeaf(editedPath))
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
    let volumeSpace = $state<SpaceInfo | null>(null)

    // Whether the user confirmed (so we don't cancel the scan on destroy)
    let confirmed = false
    let destroyed = false
    // Whether a confirm is mid-flight, still awaiting something before it can
    // dispatch. Reactive (unlike `confirmed`) because the confirm button reads it
    // to disable itself and show a spinner: a button that looks live while the
    // handler silently awaits is how a click reads as "nothing happened".
    let confirmPending = $state(false)

    // "Ask for each" unless an auto-confirming caller named a policy. The map is
    // shared with the `dialog confirm` path (`conflict-policy.ts`); the two used
    // to keep private copies that had drifted on the conditional names.
    let conflictPolicy = $state<ConflictResolution>(
        (autoConfirm ? conflictPolicyFromMcpName(autoConfirmOnConflict) : undefined) ?? 'stop',
    )

    // Filter to only actual volumes (not favorites)
    const actualVolumes = $derived(volumes.filter((v) => v.category !== 'favorite' && v.category !== 'network'))

    const volumeItems = $derived<SelectItem[]>(actualVolumes.map((v) => ({ value: v.id, label: v.name })))

    const singleTransfer = $derived(!isRenameMode && activeOperationType !== 'compress' && sourcePaths.length === 1)
    const namedTarget = $derived(singleTransfer ? resolveTransferTarget({
        enteredPath: editedPath, sourceFolderPath, sourcePath: sourcePaths[0], sourceVolumeId, selectedVolumeId, volumes, homePath: userHomePath,
    }) : null)
    const targetVolumeId = $derived(namedTarget?.volumeId ?? selectedVolumeId)
    const selectedVolume = $derived(actualVolumes.find((v) => v.id === targetVolumeId))
    const targetPath = $derived(isRenameMode ? renameTarget.folder : (namedTarget?.path ?? editedPath))

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
    //  Rename mode is never this fast path: its move copies every object (that's
    //  why the editor sent it here), and the backend consumes the preview the
    //  dialog's counts come from. Nor is a volume whose renames copy (S3,
    //  `renamesCanCopy`): its move is billed server-side copying, so it scans for
    //  the counts and the cost line, and hands that preview to the backend the
    //  same way rename mode does.
    const isSameVolumeMove = $derived(
        !isRenameMode &&
            activeOperationType === 'move' &&
            sourceVolumeId !== DEFAULT_VOLUME_ID &&
            sourceVolumeId === targetVolumeId &&
            !capabilitiesFor(sourceVolumeId).renamesCanCopy,
    )

    function changeOperation(next: string) {
        const nextOperationType = next as TransferOperationType
        const volumeId = targetVolumeId
        const nextPath = editedPathAfterOperationChange({
            operationType: activeOperationType, nextOperationType, editedPath, targetParent: namedTarget?.parent,
            volumePath: volumes.find((v) => v.id === volumeId)?.path ?? '/', sourcePaths, sourceFolderPath,
        })
        selectedVolumeId = volumeId
        activeOperationType = nextOperationType
        editedPath = nextPath
    }

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
        getDestinationName: () => namedTarget?.name,
        getSourceIsDirectory: () => folderCount === 1 && fileCount === 0,
        getSelectedVolumeId: () => targetVolumeId,
        getSourcePaths: () => sourcePaths,
        getEditedPath: () => targetPath,
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
    // A single item about to take an existing folder's name almost always means
    // a pasted folder path meant "into it". Its own name there is a plain merge.
    const sourceName = $derived(getFolderName(sourcePaths[0] ?? ''))
    const showNamedFolderHint = $derived(
        conflicts.namedTargetIsFolder && namedTarget !== null && namedTarget.name !== sourceName,
    )
    const hasTypeMismatchConflict = $derived(conflicts.hasTypeMismatchConflict)
    const isCheckingConflicts = $derived(conflicts.isCheckingConflicts)

    let showConflictSpinner = $state(false)
    $effect(() => {
        showConflictSpinner = false
        if (!isCheckingConflicts) return
        const timer = setTimeout(() => { showConflictSpinner = true; }, 100)
        return () => { clearTimeout(timer); }
    })

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
    // A same-volume move that renames in one call never scans, so it never asks.
    // The overwrites the policy makes are priced from the conflict check's
    // clashes, once it has answered (rename mode's new name is never taken).
    const costClashes = $derived(
        conflicts.conflictCheckComplete && !isRenameMode
            ? { resolution: conflictPolicy, clashes: conflicts.fileClashes }
            : null,
    )
    const costRequest = $derived(
        costRequestFor({
            operation: activeOperationType,
            scanComplete,
            previewId: scan.previewId,
            sourceVolumeId,
            destinationVolumeId: targetVolumeId,
            clashes: costClashes,
        }),
    )

    /** Counting state for the tallies element, exposed as `data-scan-state` so
     *  E2E tests can wait race-free for the scan to settle before asserting the
     *  counter line (no new wire event — this is the existing `scanComplete` /
     *  `isSameVolumeMove` state surfaced to the DOM):
     *   - `done`     → the deep scan finished; the tallies are final.
     *   - `skipped`  → no deep scan runs (a same-volume move renames server-side,
     *                  zero bytes), so the tallies legitimately stay at 0 — there's
     *                  nothing to count.
     *   - `counting` → a scan is in flight (or about to start on mount).
     *   - `unavailable` → the scan stopped without an answer; the tallies are a
     *                  floor, and the notice under them says so. Also a preview
     *                  that refused to start (the source isn't connected).
     *  `done` wins over `skipped`: a same-volume COPY still scans and completes. */
    const scanState = $derived<'counting' | 'done' | 'skipped' | 'unavailable'>(
        scanComplete ? 'done' : scan.scanFailure || scan.sourceRefusal ? 'unavailable' : isSameVolumeMove ? 'skipped' : 'counting',
    )

    /** Settle state of the top-level conflict check, exposed as `data-conflict-state`
     *  on the dialog body. The conflicts section renders NOTHING when the check comes
     *  back clean, so "did the check finish?" has no element to poll for; a test that
     *  waits for `.conflict-policy` can only ever observe the conflict outcome and
     *  would have to guess at the clean one. This marker makes both observable:
     *   - `checking` → the dest listing is in flight (or about to start on mount).
     *   - `done`     → the check settled; the conflicts section below is final.
     *   - `skipped`  → compress makes ONE new file, so the multi-file check never
     *                  runs and the dest-exists affordance answers instead.
     *   - `unknown`  → the check couldn't run, so nothing is known about the
     *                  destination. ⚠️ Never folded into `done`: an empty conflict
     *                  list from a check that never ran reads as "all clear".
     *  The body carries it because it's the only element present in every state. */
    const conflictState = $derived<'checking' | 'done' | 'skipped' | 'unknown'>(
        conflicts.conflictCheckComplete
            ? 'done'
            : conflicts.conflictCheckUnknown
              ? 'unknown'
              : activeOperationType === 'compress' || isRenameMode
                ? 'skipped'
                : 'checking',
    )

    const pathError = $derived.by(() => {
        if (isRenameMode) return renamePathError()
        if (singleTransfer && !namedTarget) return validateNotEmpty('').message
        const structural = validateDirectoryPath(namedTarget ? `${namedTarget.parent}/${namedTarget.name}` : editedPath)
        if (structural.severity === 'error') return structural.message
        // `fullPath` is already rooted; `anchorOnVolume` passes a path on the volume through.
        return getPathValidationError(
            sourcePaths,
            namedTarget?.fullPath ?? targetPath,
            activeOperationType,
            {
                sourceVolumePath: volumes.find((v) => v.id === sourceVolumeId)?.path ?? '/',
                destinationVolumePath: selectedVolume?.path ?? '/',
            },
            !!namedTarget,
        )
    })

    /** Rename mode validates the folder's shape and the NEW NAME as a name.
     *  `getPathValidationError` would call the source's own folder "already there". */
    function renamePathError(): string | null {
        const structural = validateDirectoryPath(renameTarget.folder)
        if (structural.severity === 'error') return structural.message
        const isDir = folderCount > 0
        for (const check of [validateNotEmpty, validateDisallowedChars, validateNameLength]) {
            const result = check(renameTarget.leaf, isDir)
            if (result.severity === 'error') return result.message
        }
        return null
    }

    // Destination-existence check (debounced, async) behind the yellow "will be
    // created" warning. Created synchronously here (component init) so its internal
    // `$effect` lands in the effect-tracking context, matching the scan/conflict
    // factories above.
    // Rename mode asks about the FOLDER the source lands in, not the new name.
    const destExists = createTransferDestExistsCheck({
        getEditedPath: () => targetPath,
        getSelectedVolumeId: () => targetVolumeId,
        getDestroyed: () => destroyed,
        log,
    })

    // The destination folder takes no writes, said before confirm (red, beneath the
    // path box, after a structural error). Confirm is disabled while it shows, the
    // same way a path error disables it, with the notice as the reason: the transfer
    // asks the same question before it writes and refuses anyway, so an enabled button
    // only led to that refusal in an extra dialog. A notice gone stale (the folder
    // became writable while the dialog was open) clears on the next edit of the path.
    // MCP auto-confirm skips the button and meets the backend's typed refusal instead.
    const REFUSAL_KEY = {
        readOnlyFilesystem: 'fileOperations.transferDialog.destinationReadOnly',
        noPermission: 'fileOperations.transferDialog.destinationNoPermission',
        unexplained: 'fileOperations.transferDialog.destinationNotWritable',
    } as const
    const targetRefusal = $derived(pathError || !destExists.refusal ? null : tString(REFUSAL_KEY[destExists.refusal]))

    // Inline path warning beneath the box (red path error always wins). Copy/move:
    // "folder will be created" (backend auto-creates via `create_directory_all`),
    // unless the folder takes no writes, where that would be a promise the transfer
    // can't keep. Compress targets a new zip FILE, so the inverse — replacing an
    // existing file.
    const targetWarning = $derived.by<string | null>(() => {
        if (pathError || targetRefusal) return null
        if (activeOperationType === 'compress')
            return destExists.targetExists ? tString('fileOperations.transferDialog.targetWillBeOverwritten') : null
        if (!destExists.targetMissing) return null
        return tString(
            activeOperationType === 'copy'
                ? 'fileOperations.transferDialog.targetWillBeCreatedCopy'
                : 'fileOperations.transferDialog.targetWillBeCreatedMove',
        )
    })

    // The path starts with the place's own root folder (`/srv/data/photos` on a
    // place rooted at `/srv/data`), so it can be read two ways (#164). The transfer
    // goes where the box says; this names that and offers the other reading, and
    // nothing rewrites the box unless the button is pressed. Outranks "will be
    // created", which is usually true of the doubled folder too.
    const rootEcho = $derived(pathError || targetRefusal ? null : destExists.rootEcho)

    function useStrippedPath() {
        if (rootEcho) editedPath = namedTarget ? joinPathLeaf(rootEcho.stripped, namedTarget.name) : rootEcho.stripped
        pathInputRef?.focus()
    }

    // Free-space text is intentionally uncolored: red GB would falsely signal "low space".
    const spaceInfoText = $derived(
        formatSpaceInfo(volumeSpace, formatByteSize),
    )

    // Load volume space when volume changes
    async function loadVolumeSpace() {
        const volume = actualVolumes.find((v) => v.id === targetVolumeId)
        if (volume) {
            volumeSpace = (await getVolumeSpace(volume.path)).data
        }
    }

    $effect(() => {
        dependOn(targetVolumeId)
        void loadVolumeSpace()
    })

    /**
     * Pending conflict check, captured so `handleConfirm` can await it under the
     * one policy that needs its result (see `needsConflictNames`).
     *
     * The check runs on mount, in parallel with the (potentially slow) scan
     * preview — it's just one cheap dest listing and doesn't need the recursive
     * byte scan. It's assigned synchronously in `onMount` BEFORE the auto-confirm
     * branch, so the MCP fast path's `handleConfirm` await sees a real promise
     * (not `undefined`) and dispatches with `conflictNames` populated.
     */
    let conflictCheckPromise: Promise<void> | null = $state(null)

    let checkedTarget = ''
    $effect(() => {
        const key = JSON.stringify([targetVolumeId, targetPath, namedTarget?.name, activeOperationType])
        if (!isOpen || key === checkedTarget) return
        checkedTarget = key
        conflicts.reset()
        conflictCheckPromise = null
        if (activeOperationType === 'compress' || isRenameMode || pathError) return
        const timer = setTimeout(() => {
            conflictCheckPromise ??= conflicts.check()
        }, 300)
        return () => { clearTimeout(timer); }
    })

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
        // Closed while resolving the home dir (an MCP `dialog confirm`, or a quick
        // Escape): the props are already gone, so start nothing. The scan start's
        // own reads would throw. `../DETAILS.md` § "A dialog's async start can
        // outlive it".
        if (destroyed) return
        pathInputRef?.focus()
        pathInputRef?.select()

        // Volume space is loaded by the $effect watching targetVolumeId

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
        if (activeOperationType !== 'compress' && !isRenameMode && !pathError) {
            conflictCheckPromise ??= conflicts.check()
        }

        // Auto-confirm if MCP requested it (after a tick so the dialog is fully initialized)
        if (autoConfirm) {
            await tick()
            await handleConfirm(true)
        }
    })

    onDestroy(() => {
        destroyed = true
        unregisterConfirmer?.()
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

    /**
     * Whether this confirm needs the upfront conflict NAMES before it dispatches.
     *
     * Only `skip` does: `pre_known_conflicts` is a bulk-skip perf optimization the
     * backend reads under that one resolution and ignores under every other
     * (`build_pre_skip_set` in `transfer_driver/mod.rs` returns an empty set unless
     * `config_resolution == Skip`). Under `stop` the backend prompts per clash at
     * runtime, so dispatching with `conflicts: []` costs information, never safety.
     *
     * Only the auto-confirm waits for them: it fires on mount, before the check
     * can have answered, and nobody is watching the button. A person can't reach
     * `skip` while the check is running (the policy radios only render once it's
     * done), and an MCP `dialog confirm` answers within its ack budget, so both
     * dispatch with whatever names the check has by then. A dest listing can take
     * minutes on a big remote folder.
     */
    function needsConflictNames(isAuto: boolean): boolean {
        return isAuto && conflictPolicy === 'skip'
    }

    async function handleConfirm(isAuto = false, { startInBackground = false }: ConfirmOptions = {}) {
        // A refused source can't be read, so there's nothing to confirm. A confirm that
        // beats the refusal (MCP auto-confirm) reaches the backend, which refuses it typed.
        if (pathError || scan.sourceRefusal || confirmed) return
        confirmed = true
        confirmPending = true
        // Compress auto-confirm must not silently overwrite an existing archive:
        // proceed unattended only when the target doesn't exist; else stay open.
        if (activeOperationType === 'compress' && isAuto && (await destExists.probeExists())) {
            confirmed = false
            confirmPending = false
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
        // for and no cached preview to consume — pass `previewId = null`, which
        // the backend reads as "no preview" rather than a miss. The conflict
        // check only gates `skip`.
        if (isSameVolumeMove) {
            scan.cancelPreview()
            await confirmTransfer({ previewId: null, isAuto, startInBackground })
            return
        }
        // Wait for `startScanPreview` so `previewId` is non-null on a fast
        // confirm (MCP, Playwright, rapid Enter). Dropping this is a three-part
        // failure, not a missing id: the operation would dispatch with no
        // preview to claim, fall into the backend's miss case, and re-walk the
        // tree CONCURRENTLY with the preview this dialog already started — the
        // exact contention the wait exists to prevent, and worst on MTP and SMB.
        // The orphan would also have no owner and nothing to cancel it, because
        // the `confirmed` guard keeps `handleCancel` away from `freeAndCleanup`,
        // so its result would sit until a TTL sweep. The IPC only mints an id
        // and spawns the walk, so it returns promptly even on a wedged share.
        // The conflict check does NOT gate this path: it's a dest listing that
        // can take minutes on a big remote dir, and only `skip` consumes its
        // names.
        await scan.scanStarted
        await confirmTransfer({ previewId: scan.previewId, isAuto, startInBackground })
    }

    async function confirmTransfer({
        previewId,
        isAuto,
        startInBackground,
    }: {
        previewId: string | null
        isAuto: boolean
        startInBackground: boolean
    }) {
        if (needsConflictNames(isAuto) && !isRenameMode) await (conflictCheckPromise ??= conflicts.check())
        onConfirm({
            destination: targetPath,
            destinationName: namedTarget?.name,
            volumeId: targetVolumeId,
            previewId,
            conflictResolution: conflictPolicy,
            operationType: activeOperationType,
            preKnownConflicts: conflicts.conflictNames,
            ...(isRenameMode ? { newName: renameTarget.leaf } : {}),
            ...(activeOperationType === 'move' &&
            isRenameInPlace({ target: namedTarget, sourcePath: sourcePaths[0], sourceVolumeId })
                ? { renameInPlace: true }
                : {}),
            // F2 / Background: the one difference from Enter.
            ...(startInBackground ? { startInBackground: true } : {}),
        })
    }

    // An MCP `dialog confirm` is the Confirm button under a policy the agent named:
    // same path, same box contents, same preview. Registered during init, so a
    // confirm that lands while the mount is still resolving the home dir finds it.
    // Asking for the background presses the Background button instead.
    const unregisterConfirmer = registerConfirmer?.((policy, options) => {
        conflictPolicy = policy
        void handleConfirm(false, options)
    })

    function handleCancel() {
        // A confirm already committed and is only waiting to dispatch: the pending
        // `onConfirm` owns the preview now. Freeing it here (Cancel, Escape, or the
        // dialog's own close path) would cancel the scan out from under that
        // dispatch, and the progress dialog would open onto a dead preview.
        if (confirmed) return
        // Free the scan preview (cancels an in-flight scan and evicts any cached
        // result). Regardless of `isScanning`, so a dismiss after the scan
        // completed doesn't leak the cache.
        scan.freeAndCleanup()
        onCancel()
    }

    /**
     * A person's confirm: the button or Enter. Refused while the destination
     * refuses writes, where the button reads disabled. ❌ Not in `handleConfirm`:
     * an MCP confirm goes through that, and it meets the backend's typed refusal.
     */
    function confirmFromUser(options: ConfirmOptions = {}) {
        if (targetRefusal) return
        void handleConfirm(false, options)
    }

    function handleKeydown(event: KeyboardEvent) {
        if (event.key === 'Enter') {
            confirmFromUser()
            return
        }
        // Dialog-scoped F2 = confirm and background, Total Commander's "F2 Queue".
        // `ModalDialog`'s overlay stops every keydown, so F2 never reaches the
        // global `file.rename` while this is open, and is that again once it
        // closes. No binding to leak.
        if (isStartInBackgroundKey(event)) {
            event.preventDefault()
            confirmFromUser({ startInBackground: true })
        }
    }

    function handleInputKeydown(event: KeyboardEvent) {
        if (event.key === 'Enter') {
            event.preventDefault()
            event.stopPropagation()
            confirmFromUser()
        }
    }
</script>

<ModalDialog
    titleId="dialog-title"
    onkeydown={handleKeydown}
    dialogId="transfer-confirmation"
    onclose={handleCancel}
    containerStyle="width: 500px"
    growDownward
    resizable="horizontal"
>
    {#snippet title()}{dialogTitle}{/snippet}

    <div class="dialog-body" data-conflict-state={conflictState}>
        <!-- Copy / Move / Compress. `fullWidth` so the segmented control spans the
             same column as the fields below it. Rename mode is a move and nothing
             else, so it shows why instead. -->
        {#if isRenameMode}
            <p class="rename-hint">{tString('fileOperations.transferDialog.renameByMoveHint')}</p>
        {:else}
            <ToggleGroup
                semantics="toggles"
                value={activeOperationType}
                options={operationOptions}
                onChange={changeOperation}
                ariaLabel={tString('fileOperations.transferDialog.operationAria')}
                fullWidth
            />
        {/if}

        <!-- Where the items come from: the full source path, middle-shortened when
             it's too long for the row (the tail carries the meaning). -->
        <SectionCard label={tString('fileOperations.transferDialog.sourceGroupTitle')}>
            <!-- The action writes the text and carries the full path as its `title`. -->
            <div class="source-path" use:useShortenMiddle={{ text: sourceFolderPath, preferBreakAt: '/' }}></div>
        </SectionCard>

        <!-- Where the items go: volume, then the editable path on that volume. -->
        <SectionCard label={tString('fileOperations.transferDialog.targetGroupTitle')}>
            <div class="target-card-body">
                <div class="volume-selector">
                    <div class="volume-select">
                        <Select
                            items={volumeItems}
                            value={targetVolumeId}
                            ariaLabel={tString('fileOperations.transferDialog.destVolumeAria')}
                            disabled={isRenameMode}
                            onChange={(id: string) => {
                                selectedVolumeId = id
                                const root = volumes.find((v) => v.id === id)?.path ?? '/'
                                editedPath = initialEditedPath(activeOperationType, root, root, sourcePaths, sourceFolderPath)
                            }}
                        />
                    </div>
                    {#if volumeSpace}
                        <span class="space-info">{spaceInfoText}</span>
                    {/if}
                </div>

                {#if selectedVolume?.connectionState === 'os_mount'}
                    <p class="smb-native-note">
                        {tString('fileOperations.transferDialog.smbNativeNote')}
                    </p>
                {/if}

                <div class="path-input-group">
                    <TextInput
                        bind:inputElement={pathInputRef}
                        bind:value={editedPath}
                        invalid={!!pathError || !!targetRefusal}
                        warning={!!rootEcho || !!targetWarning}
                        ariaLabel={tString('fileOperations.transferDialog.destPathAria')}
                        aria-describedby={pathError
                            ? 'transfer-path-error'
                            : targetRefusal
                              ? 'transfer-path-refusal'
                              : rootEcho
                                ? 'transfer-path-root-echo'
                                : targetWarning
                                  ? 'transfer-path-warning'
                                  : undefined}
                        spellcheck={false}
                        autocomplete="off"
                        onkeydown={handleInputKeydown}
                    />
                    {#if pathError}
                        <p id="transfer-path-error" class="path-error" role="alert">{pathError}</p>
                    {:else if targetRefusal}
                        <p id="transfer-path-refusal" class="path-error" role="alert">{targetRefusal}</p>
                    {:else if rootEcho}
                        <div class="path-warning root-echo">
                            <p id="transfer-path-root-echo">
                                {tString('fileOperations.transferDialog.rootEchoWarning', {
                                    rootFolder: rootEcho.rootFolder,
                                    resolvedPath: rootEcho.resolved,
                                    strippedPath: rootEcho.stripped,
                                })}
                            </p>
                            <Button variant="secondary" size="mini" onclick={useStrippedPath}>
                                {tString('fileOperations.transferDialog.rootEchoUseStripped')}
                            </Button>
                        </div>
                    {:else if targetWarning}
                        <p id="transfer-path-warning" class="path-warning">{targetWarning}</p>
                    {/if}
                </div>
            </div>
        </SectionCard>

        <!-- Scan stats (live counting) -->
        <div class="scan-stats" data-scan-state={scanState}>
            <div class="scan-stat">
                <span class="scan-value"><Size bytes={bytesFound} /></span>
            </div>
            <span class="scan-divider">/</span>
            <div class="scan-stat">
                <span class="scan-value">{formatNumber(filesFound)}</span>
                <span class="scan-label">{t('fileOperations.transferDialog.scanFile', { count: filesFound })}</span>
                <span class="conflict-check-status">
                    {#if isCheckingConflicts && showConflictSpinner}
                        <Spinner size="sm" label={tString('fileOperations.transferDialog.checkingConflicts')} />
                    {/if}
                </span>
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
        <S3CostLine request={costRequest} />

        <!-- The size scan couldn't finish. Said plainly, because the tallies
             above are now a floor rather than a total, and a dialog that goes
             quiet here is one the user reads as "nothing to copy". The transfer
             can still start: the operation counts as it goes, and the scan
             preview only feeds this Size line and a cache it can rebuild. -->
        {#if scan.sourceRefusal}
            <!-- The preview refused before walking anything: no volume answers
                 for the source (a phone unplugged under a search-results pane).
                 No Retry and no Confirm, since neither can work until it's back. -->
            <p class="scan-unavailable" role="status">
                <span class="scan-unavailable-icon" aria-hidden="true">
                    <Icon name="triangle-alert" size={16} />
                </span>
                <span>{tString('fileOperations.transferDialog.sourceNoLongerConnected')}</span>
            </p>
        {:else if scan.scanFailure}
            <p class="scan-unavailable" role="status">
                <span class="scan-unavailable-icon" aria-hidden="true">
                    <Icon name="triangle-alert" size={16} />
                </span>
                <span>
                    {scan.scanFailure.timedOut
                        ? tString('fileOperations.transferDialog.scanUnresponsive')
                        : tString('fileOperations.transferDialog.scanStopped')}
                </span>
                <Button variant="secondary" size="mini" onclick={() => { scan.retry(); }}>
                    {tString('fileOperations.transferDialog.scanRetry')}
                </Button>
            </p>
        {/if}

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
        {#if conflicts.conflictCheckUnknown}
            <!-- The check couldn't run. Rendering nothing here would show exactly
                 what a clean destination shows, and the user is about to make a
                 decision about their own files on the strength of it. -->
            <p class="conflicts-unknown" role="status">
                <span class="conflicts-unknown-icon" aria-hidden="true">
                    <Icon name="triangle-alert" size={16} />
                </span>
                <span>{tString('fileOperations.transferDialog.conflictsUnknown')}</span>
            </p>
        {:else if totalConflictCount > 0 || mergeFolderCount > 0}
            <!-- A warning-toned card, not a full-bleed band: it's one more block in the
             dialog's column, so it obeys the same inset as the fields above it. -->
            <SectionCard tone="warning">
                {#if showNamedFolderHint && namedTarget}
                    <p class="named-folder-hint">
                        {tString('fileOperations.transferDialog.namedFolderHint', {
                            target: namedTarget.name,
                            name: sourceName,
                        })}
                    </p>
                {/if}
                <!-- Folder merges are informational, never a question: same-named
                 folders always merge silently. Surfaced so a user who didn't
                 expect a same-named folder at the dest gets a visible cue. -->
                {#if mergeFolderCount > 0}
                    <p class="merge-info">
                        {mergeFolderCount === 1
                            ? tString('fileOperations.transferDialog.mergeInfoSingle')
                            : tString('fileOperations.transferDialog.mergeInfoMany', {
                                  countText: formatNumber(mergeFolderCount),
                                  count: mergeFolderCount,
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
                        columns={3}
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
            </SectionCard>
        {/if}
    </div>

    {#snippet footer()}
        <!-- Cancel goes inert for the same window: the confirm has committed, so a
             press could only look like it did something (`handleCancel` refuses to
             free a preview the pending dispatch is about to consume). -->
        <Button variant="secondary" onclick={handleCancel} disabled={confirmPending}
            >{tString('fileOperations.button.cancel')}</Button
        >
        <!-- A pending confirm disables and grows a spinner beside the SAME label: the
             button has to look busy rather than inviting a second click. The spinner
             is decorative (no `label`, so `aria-hidden`), which keeps the button's
             accessible name exactly `confirmLabel` and needs no new catalog string. -->
        <StartInBackgroundButton
            onclick={() => { confirmFromUser({ startInBackground: true }); }}
            disabled={!!pathError || !!targetRefusal || !!scan.sourceRefusal || confirmPending}
        />
        <Button variant="primary" onclick={() => { confirmFromUser(); }} disabled={!!pathError || !!targetRefusal || !!scan.sourceRefusal || confirmPending}>
            <span class="confirm-content">
                {#if confirmPending}
                    <Spinner size="sm" />
                {/if}
                {confirmLabel}
            </span>
        </Button>
    {/snippet}
</ModalDialog>

{#snippet written(children: import('svelte').Snippet)}<Size bytes={bytesFound} />{@render children()}{/snippet}
{#snippet ondisk(children: import('svelte').Snippet)}<Size bytes={dedupBytesFound} />{@render children()}{/snippet}

<style>
    /* Uniform vertical rhythm: every top-level section is a flex-column child, so a
       single `gap` sets equal spacing between all of them. The side inset is
       `ModalDialog`'s. */
    .dialog-body {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-lg);
    }

    /* `SectionCard` carries its own bottom margin for stacked use (Settings); here the
       column's `gap` owns every gap, so the cards drop it and can't double up. */
    .dialog-body > :global(.section-card-wrap) {
        margin-bottom: 0;
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

    .path-error {
        margin: var(--spacing-sm) 0 0;
        font-size: var(--font-size-sm);
        color: var(--color-error);
    }

    /* Yellow "folder will be created" warning: the field's own `warning` state
       (see `TextInput`) plus this message line. The error state takes precedence,
       so the two never show at once. `--color-warning-text` is the AA-safe text
       token (the brand `--color-warning` is reserved for borders and fills). */
    .path-warning {
        margin: var(--spacing-sm) 0 0;
        font-size: var(--font-size-sm);
        color: var(--color-warning-text);
    }

    /* The two-readings warning: the sentence, then its one button under it. */
    .root-echo {
        display: flex;
        flex-direction: column;
        align-items: flex-start;
        gap: var(--spacing-xs);
    }

    .root-echo p {
        margin: 0;
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

    .rename-hint {
        margin: 0;
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }

    .hardlink-note {
        margin: var(--spacing-xs) 0 0;
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }

    .scan-divider {
        color: var(--color-text-tertiary);
    }

    .scan-status {
        display: inline-flex;
        align-items: center;
    }

    /* Both "couldn't find out" notices: one row, warning-toned text on the
       dialog's own background (they're a caveat about a number above them, not a
       card of their own). */
    .scan-unavailable,
    .conflicts-unknown {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
        margin: 0;
        font-size: var(--font-size-sm);
        color: var(--color-warning-text);
    }

    .scan-unavailable-icon,
    .conflicts-unknown-icon {
        display: inline-flex;
        flex: 0 0 auto;
        color: var(--color-warning-text);
    }

    /* Reserve the indicator's width even when a check finishes before the delay. */
    .conflict-check-status {
        display: inline-flex;
        align-self: center;
        width: 12px;
        height: 12px;
        flex: 0 0 auto;
    }

    /* Keeps the pending spinner on the label's baseline row inside the confirm
       button. `inline-flex` so the button still sizes to its content. */
    .confirm-content {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-sm);
    }

    /* The question the card asks. Plain text color: the card's warning tint already
       carries the "pay attention" signal, and coloring the sentence too made it read
       as an error rather than a choice to make. */
    .conflicts-summary {
        margin: 0 0 var(--spacing-md);
        font-size: var(--font-size-md);
        color: var(--color-text-primary);
        font-weight: 500;
    }

    /* Folder-merge info line: neutral, not a warning. Folders always merge, so
       this is a heads-up, not a question. */
    .named-folder-hint,
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
    }

    .conflict-warning-icon {
        flex-shrink: 0;
        display: inline-flex;
        align-items: center;
        color: var(--color-error-text);
        margin-top: 1px;
    }

    /* Wrapper hook for the policy radios (E2E + component tests target
       `.conflict-policy`); `RadioGroup` owns the option layout (`columns={3}`, so the
       five options fill the card as 3 + 2). The column flex stretches the group to
       full width, which is what gives those three columns something to divide. */
    .conflict-policy {
        display: flex;
        flex-direction: column;
    }

    /* Compress-only block: stacks its two children with the dialog body's rhythm,
       and gives the mode switch one element to slide. */
    .compress-extras {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-lg);
    }
</style>
