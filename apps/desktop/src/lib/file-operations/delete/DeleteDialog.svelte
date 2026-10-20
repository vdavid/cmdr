<script lang="ts">
    import { onMount, onDestroy, tick } from 'svelte'
    import ModalDialog from '#lib/ui/ModalDialog.svelte'
    import Button from '#lib/ui/Button.svelte'
    import Switch from '#lib/ui/Switch.svelte'
    import {
        generateDeleteTitle,
        abbreviatePath,
        getSymlinkNotice,
        MAX_VISIBLE_ITEMS,
        type CloudOnlineOnlyExtent,
        type DeleteSourceItem,
    } from './delete-dialog-utils'
    import { formatNumber } from '#lib/file-explorer/selection/selection-info-utils.js'
    import { getDirSizeDisplayState, LOWER_BOUND_GLYPH } from '#lib/file-explorer/views/full-list-utils.js'
    import Size from '#lib/ui/Size.svelte'
    import Icon from '#lib/ui/Icon.svelte'
    import Spinner from '#lib/ui/Spinner.svelte'
    import { tooltip } from '#lib/tooltip/tooltip.js'
    import { getAppLogger } from '#lib/logging/logger.js'
    import S3CostLine from '../S3CostLine.svelte'
    import { costRequestFor } from '../s3-cost-line'
    import { useShortenMiddle } from '#lib/utils/shorten-middle-action.js'
    import Trans from '#lib/intl/Trans.svelte'
    import { t, tString } from '#lib/intl/messages.svelte.js'
    import type { ConfirmOptions, ConfirmOrigin, DeleteConfirmer } from '#lib/file-explorer/pane/dialog-props.js'
    import StartInBackgroundButton from '../StartInBackgroundButton.svelte'
    import { createDeleteScanState } from './delete-scan-state.svelte'
    import { isStartInBackgroundKey, swallowHeldKeyRepeat } from '../start-in-background-key'

    const log = getAppLogger('deleteDialog')

    interface Props {
        sourceItems: DeleteSourceItem[]
        sourcePaths: string[]
        sourceFolderPath: string
        isPermanent: boolean
        supportsTrash: boolean
        /** Source is inside a zip: deletes are permanent (no Trash inside an archive). */
        isArchive?: boolean
        /** A selected item in a cloud-storage folder is online-only, so the trash was
         *  routed here as a permanent delete rather than downloading it. Swaps the
         *  no-trash banner for one that says so, worded for how much of the selection
         *  is online-only. `null` when none of it is. */
        cloudOnlineOnly?: CloudOnlineOnlyExtent | null
        /** A selected FOLDER sits in a cloud drive, so the scan walk below may still
         *  turn up an online-only file and flip this dialog. Confirm waits for that
         *  answer; see `onlineOnlyAnswer`. */
        cloudFolderMayHoldOnlineOnly?: boolean
        isFromCursor: boolean
        /** Source volume ID. Routes the scan preview through the Volume trait
         *  (`run_volume_scan_preview`) for non-local volumes like MTP, so the
         *  confirmation dialog gets a live climbing tally instead of a silently
         *  failed local-FS walk. */
        sourceVolumeId: string
        /** When true, dialog auto-confirms without user interaction (MCP). */
        autoConfirm?: boolean
        /** The auto-confirming agent's round-trip id, which the start answers with
         *  the operation's id. A `dialog confirm` brings its own in `ConfirmOptions`. */
        mcpRequestId?: string
        /** `origin` names the MCP `dialog confirm` that pressed it, whose round-trip
         *  the start answers with the operation's id. */
        onConfirm: (previewId: string | null, isPermanent: boolean, origin?: ConfirmOrigin) => void
        /** F2 or the Background button: TRASH with no progress dialog. No mode
         *  rides along, so this can't start a permanent delete; the dialog offers
         *  it only while it would trash. Absent, the dialog offers no background
         *  start at all. */
        onConfirmInBackground?: (previewId: string | null, origin?: ConfirmOrigin) => void
        /** Takes this dialog's own confirm for as long as it's mounted, so an MCP
         *  `dialog confirm` presses the same button a person does. Returns the
         *  unregister. */
        registerConfirmer?: (confirm: DeleteConfirmer) => () => void
        onCancel: () => void
    }

    const {
        sourceItems,
        sourcePaths,
        sourceFolderPath,
        isPermanent: initialIsPermanent,
        supportsTrash,
        isArchive = false,
        cloudOnlineOnly = null,
        cloudFolderMayHoldOnlineOnly = false,
        isFromCursor,
        sourceVolumeId,
        autoConfirm = false,
        mcpRequestId: autoConfirmRequestId,
        onConfirm,
        onConfirmInBackground,
        registerConfirmer,
        onCancel,
    }: Props = $props()

    /** Set first thing in `onDestroy`. A plain `let`, read by the scan start, which can outlive the dialog. */
    let destroyed = false

    // The walk behind the tallies, and the online-only answer a cloud folder's
    // walk gives. Freeing the preview stays here: only the dialog knows `confirmed`.
    const scan = createDeleteScanState({
        getSourcePaths: () => sourcePaths,
        getSourceVolumeId: () => sourceVolumeId,
        getDestroyed: () => destroyed,
    })

    /** How much of the selection is online-only, however we learned it. */
    const onlineOnlyExtent = $derived(cloudOnlineOnly ?? scan.onlineOnlyFoundByWalk)
    /** Online-only content is in play, however we learned it. */
    const routedForOnlineOnly = $derived(onlineOnlyExtent !== null)
    /** The trash is only on the table while nothing evicted is in the selection. */
    const trashAvailable = $derived(supportsTrash && !routedForOnlineOnly)

    // The switch's own position. Forced to permanent on volumes that don't support trash.
    let switchIsPermanent = $state(initialIsPermanent || !supportsTrash)
    /** True while Shift is down. Held on the window in the CAPTURE phase, so it also catches a
     *  release outside the dialog. See `SHIFT_LISTENER_PHASE`. */
    let shiftHeld = $state(false)
    /** Shift-hold only upgrades a dialog that opened as a trash: on a Shift+F8 dialog the user is
     *  still holding the key that opened it, so honouring the release would demote a delete they
     *  deliberately asked for. Snapshot at open; neither input changes for the dialog's lifetime. */
    const shiftUpgradesToPermanent = !initialIsPermanent && supportsTrash
    // Shift only ever upgrades: a hold can't demote a permanent delete back to a trash.
    const isPermanent = $derived(switchIsPermanent || routedForOnlineOnly || (shiftUpgradesToPermanent && shiftHeld))
    /** A permanent delete never starts out of sight: it's the one operation nothing
     *  undoes. So the background start follows the FINAL mode, whatever flipped it
     *  (the switch, a held Shift, online-only content, an archive, a volume with no
     *  trash), and goes the moment it reads "Delete permanently". */
    const canTrashInBackground = $derived(!isPermanent && onConfirmInBackground !== undefined)

    const dialogTitle = $derived(generateDeleteTitle(sourceItems, isFromCursor))
    const abbreviatedPath = $derived(abbreviatePath(sourceFolderPath))
    const symlinkNotice = $derived(getSymlinkNotice(sourceItems))

    const visibleItems = $derived(sourceItems.slice(0, MAX_VISIBLE_ITEMS))
    const overflowCount = $derived(Math.max(0, sourceItems.length - MAX_VISIBLE_ITEMS))

    const confirmLabel = $derived(
        isPermanent
            ? tString('fileOperations.delete.confirmDelete')
            : tString('fileOperations.delete.confirmMoveToTrash'),
    )
    const confirmVariant = $derived<'primary' | 'danger'>(isPermanent ? 'danger' : 'primary')
    const dialogRole = $derived<'dialog' | 'alertdialog'>(isPermanent ? 'alertdialog' : 'dialog')
    // `delete-warning-text` only exists while a banner renders, so point
    // `aria-describedby` at the banner's condition, not at `isPermanent`.
    const hasWarningBanner = $derived(isArchive || routedForOnlineOnly || !supportsTrash)

    // True once the user confirms. On confirm the delete/trash op (or the
    // progress dialog) takes over the same scan and consumes the cached result,
    // so teardown must NOT free it then.
    let confirmed = false
    /** True while a confirm is waiting on the online-only answer below. Shows the
     *  spinner in the confirm button and swallows a second press. */
    let confirming = $state(false)
    /** The walk answered while a press was in flight and turned this dialog from a
     *  trash into a permanent delete, so the press was handed back. Without the
     *  line this renders, the dialog just sits there and nothing says the press
     *  didn't take. Cleared by the next press. */
    let handedBackForOnlineOnly = $state(false)
    // Priced as a permanent delete only: a trash is unpriced, so it gets no line.
    const costRequest = $derived(
        costRequestFor({
            operation: isPermanent ? 'delete' : 'trash',
            scanComplete: scan.scanComplete,
            previewId: scan.previewId,
            sourceVolumeId,
            destinationVolumeId: null,
        }),
    )

    /** Any key event re-reads the modifier state, so a keyup we never saw (window switch,
     *  a native menu eating it) self-heals on the next keystroke instead of leaving the
     *  dialog stuck on "Delete permanently". */
    function syncShiftState(event: KeyboardEvent) {
        shiftHeld = event.shiftKey
    }

    /** Focus left the window: whatever Shift does now, we won't see it come back up. */
    function releaseShift() {
        shiftHeld = false
    }

    /** Capture, NOT bubble: `ModalDialog`'s overlay calls `stopPropagation()` on every keydown to
     *  shield the file explorer, and focus lives inside that overlay while we're open. A
     *  bubble-phase window listener sits downstream of it, so it would never see the Shift keydown
     *  at all. The capture phase runs on `window` first, before anything can stop the event. */
    const SHIFT_LISTENER_PHASE = true

    function watchShift() {
        window.addEventListener('keydown', syncShiftState, SHIFT_LISTENER_PHASE)
        window.addEventListener('keyup', syncShiftState, SHIFT_LISTENER_PHASE)
        window.addEventListener('blur', releaseShift)
    }

    function unwatchShift() {
        window.removeEventListener('keydown', syncShiftState, SHIFT_LISTENER_PHASE)
        window.removeEventListener('keyup', syncShiftState, SHIFT_LISTENER_PHASE)
        window.removeEventListener('blur', releaseShift)
    }

    function cleanup() {
        scan.cleanup()
        unwatchShift()
    }

    onMount(async () => {
        if (shiftUpgradesToPermanent) watchShift()
        scan.start()

        // Auto-confirm if MCP requested it (after a tick so the dialog is fully initialized)
        if (autoConfirm) {
            await tick()
            void handleConfirm()
        }
    })

    onDestroy(() => {
        destroyed = true
        unregisterConfirmer?.()
        // Nothing may await an answer that can no longer arrive.
        scan.settleOnlineOnlyAnswer()
        // Free the scan preview unless the user confirmed (the op then consumes
        // the cached result). Regardless of `isScanning`: freeing also evicts the
        // cached `CachedScanResult`, so a dismiss AFTER the scan completed
        // doesn't leak the cache until quit.
        if (!confirmed) scan.freePreview()
        cleanup()
    })

    /** The agent waiting on this press for its operation id: a `dialog confirm`'s
     *  own `mcpRequestId`, or the auto-confirm's. */
    function agentWaitingOnPress(mcpRequestId: string | undefined): string | undefined {
        return mcpRequestId ?? autoConfirmRequestId
    }

    /** Resolves `true` once the press reached `onConfirm` / `onConfirmInBackground`,
     *  `false` when a guard stopped it (an MCP `dialog confirm` answers the agent
     *  from that). */
    async function handleConfirm({ startInBackground = false, mcpRequestId }: ConfirmOptions = {}): Promise<boolean> {
        if (confirming) return false
        if (startInBackground && !canTrashInBackground) return false
        const origin: ConfirmOrigin | undefined = mcpRequestId !== undefined ? { mcpRequestId } : undefined
        confirming = true
        handedBackForOnlineOnly = false
        log.info('Delete confirmed: isPermanent={isPermanent}, items={count}, inBackground={inBackground}', {
            isPermanent,
            count: sourceItems.length,
            inBackground: startInBackground,
        })
        // The scan-preview IPC only mints an id and spawns the walk, so it
        // answers promptly even on a wedged share. See `scan.scanStarted`.
        await scan.scanStarted

        if (cloudFolderMayHoldOnlineOnly && !routedForOnlineOnly) {
            // A selected folder in a cloud drive: the walk decides whether this is
            // a trash or a delete, and pressing before it answers must not settle
            // it by luck. Everywhere else this block never runs.
            await scan.awaitOnlineOnlyAnswer(agentWaitingOnPress(mcpRequestId))
            if (destroyed) return false
            if (scan.onlineOnlyFoundByWalk !== null && !autoConfirm) {
                // The answer just turned this button from "Move to trash" into a
                // permanent delete, and the banner beside it changed too. Hand the
                // dialog back rather than run the one operation nothing can undo
                // off a press that meant something else, and say so: the dialog
                // stays open either way, so silence reads as a dead button. An MCP
                // auto-confirm asked for the delete outright, so it goes through.
                confirming = false
                handedBackForOnlineOnly = true
                return false
            }
        }

        if (startInBackground) {
            // Asked again after the awaits: Shift could have gone down meanwhile.
            // A press that meant "trash, out of sight" never becomes a delete.
            if (!onConfirmInBackground || isPermanent) {
                confirming = false
                return false
            }
            confirmed = true
            onConfirmInBackground(scan.previewId, origin)
            return true
        }

        confirmed = true
        onConfirm(scan.previewId, isPermanent, origin)
        return true
    }

    // An MCP `dialog confirm` is the Confirm button: same preview, same mode.
    // Registered during init, so a confirm that lands while the scan is still
    // starting finds it and waits for the id like a fast Enter does. Asking for
    // the background presses the Background button, under the same trash-only rule.
    // The agent's round-trip id rides along, so the start answers it with the
    // operation's id; a press a guard stopped says so here.
    const unregisterConfirmer = registerConfirmer?.(async (options) => {
        if (options?.startInBackground && !canTrashInBackground) return 'refusedPermanentDelete'
        return (await handleConfirm(options)) ? 'pressed' : 'notStarted'
    })

    function handleCancel() {
        // Free the scan preview (cancels an in-flight scan and evicts any cached
        // result). Regardless of `isScanning`.
        scan.freePreview()
        cleanup()
        onCancel()
    }

    function handleKeydown(event: KeyboardEvent) {
        if (event.key === 'Enter') {
            void handleConfirm()
            return
        }
        // Dialog-scoped F2, like the progress dialog's: `ModalDialog`'s overlay
        // stops every keydown, so F2 never reaches the global `file.rename` while
        // this is open, and is that again once it closes. No binding to leak.
        if (isStartInBackgroundKey(event)) {
            event.preventDefault()
            swallowHeldKeyRepeat(event.key)
            void handleConfirm({ startInBackground: true })
        }
    }

    /** Formats item size for display. Folders show recursive info when available.
     *  Always uses logical (content) sizes (not worth plumbing the display mode setting
     *  through the delete dialog infrastructure for a transient confirmation dialog). */
    function itemSizeBytes(item: DeleteSourceItem): number | null {
        // Group A wire-format: IPC sends `null` for absent fields, not `undefined`.
        if (!item.isDirectory) return item.size ?? null
        // A folder's size is the drive index's, so it reads the way the listing
        // reads it: nothing counted yet is no size at all, and a partial count is
        // a `≥` floor (`itemSizeIsLowerBound`), ❌ never a bare number someone
        // decides a delete on.
        const state = getDirSizeDisplayState(item.recursiveSize, item.recursiveSizeComplete)
        return state === 'dir' ? null : (item.recursiveSize ?? null)
    }

    function itemSizeIsLowerBound(item: DeleteSourceItem): boolean {
        return item.isDirectory && getDirSizeDisplayState(item.recursiveSize, item.recursiveSizeComplete) === 'lower-bound'
    }

    function itemFileCountLabel(item: DeleteSourceItem): string {
        if (!item.isDirectory) return ''
        const fileCount = item.recursiveFileCount
        if (fileCount == null) return ''
        return `${formatNumber(fileCount)} ${tString('fileOperations.delete.scanFile', { count: fileCount })}`
    }
</script>

<ModalDialog
    titleId="delete-dialog-title"
    onkeydown={handleKeydown}
    dialogId="delete-confirmation"
    role={dialogRole}
    onclose={handleCancel}
    ariaDescribedby={hasWarningBanner ? 'delete-warning-text' : undefined}
    containerStyle="width: 500px"
    resizable="horizontal"
>
    {#snippet title()}{dialogTitle}{/snippet}

    <div class="dialog-body">
        <!-- Warning banner: archive deletes are permanent (no Trash inside a zip);
             a selection holding online-only cloud content says so in its own words,
             since the person pressed Trash and got a delete; other no-trash volumes
             get the generic banner. -->
        {#if isArchive}
            <div class="warning-banner" role="alert">
                <span class="warning-icon" aria-hidden="true">
                    <Icon name="triangle-alert" size={18} />
                </span>
                <p id="delete-warning-text">
                    <strong>{tString('fileOperations.delete.archiveWarningStrong')}</strong>
                    {tString('fileOperations.delete.archiveWarningRest')}
                </p>
            </div>
        {:else if onlineOnlyExtent !== null}
            <div class="warning-banner" role="alert">
                <span class="warning-icon" aria-hidden="true">
                    <Icon name="triangle-alert" size={18} />
                </span>
                <p id="delete-warning-text">
                    <!-- Two whole sentences, not a shared stem plus a fork: they differ
                         mid-paragraph and in which remedies they can offer, and only one
                         of them can say "deselect the online-only files". -->
                    {#if onlineOnlyExtent === 'all'}
                        <Trans key="fileOperations.delete.cloudOnlineOnlyAllWarning" snippets={{ strong }} />
                    {:else}
                        <Trans key="fileOperations.delete.cloudOnlineOnlyMixedWarning" snippets={{ strong }} />
                    {/if}
                </p>
            </div>
        {:else if !supportsTrash}
            <div class="warning-banner" role="alert">
                <span class="warning-icon" aria-hidden="true">
                    <Icon name="triangle-alert" size={18} />
                </span>
                <p id="delete-warning-text">
                    <strong>{tString('fileOperations.delete.noTrashWarningStrong')}</strong>
                    {tString('fileOperations.delete.noTrashWarningRest')}
                </p>
            </div>
        {/if}

        <!-- Source path. The tooltip is unconditional whenever `abbreviatePath` swapped a
             home directory for `~`, since then the line is short AND incomplete; otherwise
             it only steps in once the line runs out of room. -->
        <div class="source-path" use:tooltip={{ text: sourceFolderPath, overflowOnly: abbreviatedPath === sourceFolderPath }}>
            {tString('fileOperations.delete.fromPath', { path: abbreviatedPath })}
        </div>

        <!-- Scrollable file list -->
        <div class="file-list-container">
            <div class="file-list" role="list">
                <!-- Keyed by full path: a search-results delete spans folders, so two rows can
                     share a name (every folder's `.DS_Store`), and a name key throws. -->
                {#each visibleItems as item, index (sourcePaths[index] ?? index)}
                    <div class="file-list-item" role="listitem">
                        <span class="item-icon" aria-hidden="true">
                            <Icon name={item.isDirectory ? 'folder' : 'file'} size={14} />
                        </span>
                        <!-- `sourcePaths` is index-aligned with `sourceItems` at both call
                             sites, so the row's own full path is what hovering reveals. -->
                        <span
                            class="item-name"
                            use:tooltip={{ text: sourcePaths[index] ?? item.name, overflowOnly: true }}>{item.name}</span
                        >
                        <span class="item-size">
                            {#if itemSizeBytes(item) != null}{#if itemSizeIsLowerBound(item)}<span
                                        class="size-lower-bound-prefix">{LOWER_BOUND_GLYPH}</span
                                    >{/if}<Size bytes={itemSizeBytes(item)} />{/if}
                            {#if itemFileCountLabel(item)}{#if itemSizeBytes(item) != null}&nbsp;&nbsp;&nbsp;{/if}{itemFileCountLabel(
                                    item,
                                )}{/if}
                        </span>
                    </div>
                {/each}
                {#if overflowCount > 0}
                    <div class="file-list-overflow" role="listitem">
                        {t('fileOperations.delete.overflowMore', {
                            countText: formatNumber(overflowCount),
                            count: overflowCount,
                        })}
                    </div>
                {/if}
            </div>
        </div>

        <!-- Symlink notice -->
        {#if symlinkNotice}
            <div class="symlink-notice">
                <span class="symlink-icon" aria-hidden="true">
                    <Icon name="triangle-alert" size={14} />
                </span>
                <span>{symlinkNotice}</span>
            </div>
        {/if}

        <!-- Scan stats (live counting). `data-scan-state` is the race-free
             "counting done" marker for E2E; there's no visual completion badge. -->
        <div class="scan-stats" data-scan-state={scan.scanComplete ? 'done' : 'counting'}>
            <div class="scan-stat">
                <span class="scan-value"><Size bytes={scan.bytesFound} /></span>
            </div>
            <span class="scan-divider">/</span>
            <div class="scan-stat">
                <span class="scan-value">{formatNumber(scan.filesFound)}</span>
                <span class="scan-label">{t('fileOperations.delete.scanFile', { count: scan.filesFound })}</span>
            </div>
            <span class="scan-divider">/</span>
            <div class="scan-stat">
                <span class="scan-value">{formatNumber(scan.dirsFound)}</span>
                <span class="scan-label">{t('fileOperations.delete.scanDir', { count: scan.dirsFound })}</span>
            </div>
            {#if scan.isScanning}
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

        <!-- Throughput -->
        {#if scan.isScanning && scan.scanRate !== null}
            <div class="scan-throughput">
                <span class="scan-throughput-value"
                    >{tString('fileOperations.shared.fileRate', {
                        count: scan.scanRate.value,
                        rateText: scan.scanRate.text,
                    })}</span
                >
                {#if scan.bytesPerSec !== null && scan.bytesPerSec > 0}
                    <span class="scan-throughput-sep" aria-hidden="true">·</span>
                    <span class="scan-throughput-value"
                        ><Trans key="fileOperations.shared.byteRate" snippets={{ size }} /></span
                    >
                {/if}
            </div>
        {/if}

        <!-- Current directory being scanned -->
        {#if scan.isScanning && scan.currentDir}
            <div class="scan-current-dir" use:useShortenMiddle={{ text: scan.currentDir, preferBreakAt: '/' }}></div>
        {/if}

        <!-- The press that didn't take. Sits last in the body so it's the line
             directly above the button whose meaning just changed under the
             person's finger; `role="status"` announces it without stealing focus. -->
        {#if handedBackForOnlineOnly}
            <p class="handed-back" role="status" data-test="delete-handed-back">
                {tString('fileOperations.delete.cloudOnlineOnlyHandedBack')}
            </p>
        {/if}
    </div>

    <!-- Trash (on, the safe default) vs. permanent delete (off). Rides the footer
         row so it reads as a modifier on the confirm button beside it. Holding Shift
         flips it too, for as long as the key is down. -->
    {#snippet footerLeading()}
        {#if trashAvailable}
            <Switch checked={!isPermanent} onCheckedChange={(toTrash) => (switchIsPermanent = !toTrash)}
                >{tString('fileOperations.delete.trashSwitch')}</Switch
            >
        {/if}
    {/snippet}

    {#snippet footer()}
        <Button variant="secondary" onclick={handleCancel}>{tString('fileOperations.button.cancel')}</Button>
        {#if canTrashInBackground}
            <StartInBackgroundButton
                onclick={() => void handleConfirm({ startInBackground: true })}
                disabled={confirming}
            />
        {/if}
        <Button variant={confirmVariant} onclick={() => void handleConfirm()}>
            {#if confirming}<span class="confirm-spinner"><Spinner size="sm" /></span>{/if}{confirmLabel}
        </Button>
    {/snippet}
</ModalDialog>

{#snippet size(children: import('svelte').Snippet)}<Size bytes={scan.bytesPerSec ?? 0} />{@render children()}{/snippet}
{#snippet strong(children: import('svelte').Snippet)}<strong>{@render children()}</strong>{/snippet}

<style>
    /* Uniform vertical rhythm: every section is a flex-column child, so a single
       `gap` sets equal spacing between all of them. The side inset is `ModalDialog`'s. */
    .dialog-body {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-md);
    }

    .source-path {
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
    }

    /* No-trash warning banner */
    .warning-banner {
        display: flex;
        align-items: flex-start;
        gap: var(--spacing-sm);
        padding: var(--spacing-sm) var(--spacing-md);
        background: var(--color-warning-bg);
        border: 1px solid var(--color-warning);
        border-radius: var(--radius-md);
    }

    .warning-icon {
        flex-shrink: 0;
        color: var(--color-warning);
        margin-top: 1px;
    }

    .warning-banner p {
        margin: 0;
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }

    /* Scrollable file list */
    .file-list-container {
        border: 1px solid var(--color-border-strong);
        border-radius: var(--radius-md);
        overflow: hidden;
    }

    .file-list {
        max-height: 250px;
        overflow-y: auto;
    }

    .file-list-item {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
        padding: var(--spacing-xs) var(--spacing-md);
        font-size: var(--font-size-sm);
        border-bottom: 1px solid var(--color-border);
    }

    .file-list-item:last-child {
        border-bottom: none;
    }

    .item-icon {
        flex-shrink: 0;
        display: inline-flex;
        align-items: center;
        justify-content: center;
        width: 16px;
        color: var(--color-text-tertiary);
    }

    .item-name {
        flex: 1;
        color: var(--color-text-primary);
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .item-size {
        flex-shrink: 0;
        color: var(--color-text-tertiary);
        font-variant-numeric: tabular-nums;
        white-space: nowrap;
    }

    /* The listing's `≥` lower-bound prefix (`FullList.svelte`): a qualifier on the
       number, not a digit. */
    .size-lower-bound-prefix {
        color: var(--color-text-secondary);
        margin-right: 1px;
    }

    .file-list-overflow {
        padding: var(--spacing-xs) var(--spacing-md);
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
        font-style: italic;
    }

    /* Symlink notice */
    .symlink-notice {
        display: flex;
        align-items: flex-start;
        gap: var(--spacing-sm);
        font-size: var(--font-size-sm);
        color: var(--color-warning);
    }

    .symlink-icon {
        flex-shrink: 0;
        margin-top: 1px;
    }

    /* Scan stats. Right-aligned so the tallies sit under the dialog's right edge
       and don't compete with the left-aligned labels above them. */
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

    .scan-divider {
        color: var(--color-text-tertiary);
    }

    .scan-status {
        display: inline-flex;
        align-items: center;
    }

    /* Says a press was handed back, right above the button that changed meaning.
       Warning-colored, since it's the same news the banner above carries. */
    .handed-back {
        margin: 0;
        font-size: var(--font-size-sm);
        color: var(--color-warning);
    }

    /* Rides inside the confirm button while a cloud folder's walk is still
       deciding between a trash and a delete. */
    .confirm-spinner {
        display: inline-flex;
        align-items: center;
        margin-right: var(--spacing-xs);
        vertical-align: text-bottom;
    }

    .scan-throughput {
        display: flex;
        justify-content: flex-end;
        gap: var(--spacing-xs);
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
    }

    .scan-throughput-value {
        font-variant-numeric: tabular-nums;
    }

    .scan-throughput-sep {
        opacity: 0.6;
    }

    .scan-current-dir {
        padding: var(--spacing-xs) var(--spacing-md);
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
        overflow: hidden;
        white-space: nowrap;
        background: var(--color-bg-tertiary);
        border-radius: var(--radius-sm);
    }

</style>
