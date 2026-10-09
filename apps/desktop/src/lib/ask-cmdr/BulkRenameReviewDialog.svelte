<script lang="ts">
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import Button from '$lib/ui/Button.svelte'
    import TextInput from '$lib/ui/TextInput.svelte'
    import Icon from '$lib/ui/Icon.svelte'
    import Checkbox from '$lib/ui/Checkbox.svelte'
    import ColumnList from '$lib/ui/ColumnList.svelte'
    import type { ColumnDemandArgs, ColumnListCellContext, ColumnListColumn } from '$lib/ui/column-list-types'
    import { tString } from '$lib/intl/messages.svelte'
    import { formatInteger } from '$lib/intl/number-format'
    import { mediaIndexDropThumbnailTokens, mediaIndexThumbnailToken, onDirectoryDiff } from '$lib/tauri-commands'
    import { onDestroy, onMount, untrack } from 'svelte'
    import { tooltip } from '$lib/tooltip/tooltip'
    import { useShortenMiddle } from '$lib/utils/shorten-middle-action'
    import { openFileViewer } from '$lib/file-viewer/open-viewer'
    // The `cmdr-media://` URL is built ONLY via the viewer's `mediaUrl` (single source; see
    // `routes/viewer/CLAUDE.md`), so a row's thumbnail reuses the exact preview origin.
    import { mediaUrl } from '../../routes/viewer/media-view'
    import { evidenceSourceLabel } from './ask-cmdr-labels'
    import { parentOf, toCanonical } from '$lib/path/canonical'
    import { coverageStrength } from './rename-evidence-coverage'
    import { nameProvenance } from './rename-name-provenance'
    import {
        applyRenameReview,
        allowAllRenameRows,
        askCmdrState,
        cancelRenameReview,
        denyAllRenameRows,
        renameReviewListingChanged,
        reviseRenameRow,
        setRenameRowAllowed,
        type BulkRenameReviewProposal,
        type BulkRenameReviewRow,
    } from './ask-cmdr-trigger.svelte'

    // ── One review, however many batches ──────────────────────────────────────
    // A big rename arrives as a run of staged plans (about 101 rows per model reply), and the
    // user answers all of them at once. Each batch stays its own PROPOSAL, because preflight,
    // apply, and cancel are per proposal on the backend; the dialog is what makes them read as
    // one list.

    const review = $derived(askCmdrState.renameReview)
    const proposals = $derived(review?.proposals ?? [])
    /** The batches still answerable: an expired one has nothing left to decide. */
    const liveProposals = $derived(proposals.filter((proposal) => !proposal.expired))
    const allRows = $derived(liveProposals.flatMap((proposal) => proposal.rows))
    const allowedCount = $derived(allRows.filter((row) => row.allowed && !row.blockedReason).length)
    const blockedCount = $derived(allRows.filter((row) => row.blockedReason).length)
    const preflighting = $derived(proposals.some((proposal) => proposal.preflighting))
    const allExpired = $derived(proposals.length > 0 && liveProposals.length === 0)
    const renameLabel = $derived(tString('askCmdr.renameReview.rename', { count: allowedCount }))

    /** The folder a batch renames inside. A rename group binds one parent, so the first row
     *  answers for the batch. */
    function folderOf(proposal: BulkRenameReviewProposal): string {
        if (proposal.rows.length === 0) return ''
        try {
            return parentOf(toCanonical(proposal.rows[0].sourcePath, ''))
        } catch {
            // A path shape the brand won't take is display-only here: no heading beats a wrong one.
            return ''
        }
    }

    const folders = $derived(proposals.map(folderOf))
    /** Which folder a row belongs to only earns a heading once a job spans more than one. */
    const showFolders = $derived(folders.some((folder) => folder !== folders[0]))

    // ── The list ──────────────────────────────────────────────────────────────
    // The batches flatten into one `ColumnList` table: a folder heading where the folder
    // changes, an expired batch's notice in place of its rows, and the rows themselves. Rows
    // are sized by their content (badges, a wrapped quote, the name field), which the list's
    // small size allows: about 101 rows per batch.

    type ListRow =
        | { kind: 'folder'; key: string; folder: string }
        | { kind: 'expired'; key: string }
        | { kind: 'file'; key: string; proposalId: string; row: BulkRenameReviewRow }

    const listRows = $derived.by<ListRow[]>(() => {
        const flat: ListRow[] = []
        proposals.forEach((proposal, batchIndex) => {
            // A run of batches inside one folder gets a single heading, and a single-folder job
            // gets none at all.
            if (showFolders && (batchIndex === 0 || folders[batchIndex] !== folders[batchIndex - 1])) {
                flat.push({ kind: 'folder', key: `folder:${proposal.proposalId}`, folder: folders[batchIndex] })
            }
            if (proposal.expired) {
                flat.push({ kind: 'expired', key: `expired:${proposal.proposalId}` })
                return
            }
            for (const row of proposal.rows) {
                flat.push({ kind: 'file', key: `row:${row.rowId}`, proposalId: proposal.proposalId, row })
            }
        })
        return flat
    })

    /** The row whose preview holds focus is the list's cursor: it's the row Space opens. */
    const cursorIndex = $derived(listRows.findIndex((item) => item.kind === 'file' && item.row.rowId === focusedRowId))

    /** The file a data row stands for. Headings never reach a cell snippet, so this never
     *  answers `null` there; the guard is for the type. */
    function fileOf(item: ListRow): { proposalId: string; row: BulkRenameReviewRow } | null {
        return item.kind === 'file' ? item : null
    }

    const columns = $derived<ColumnListColumn<ListRow>[]>([
        {
            id: 'allow',
            label: tString('askCmdr.renameReview.allow'),
            // As wide as the header or the 16 px checkbox, whichever is wider.
            width: { kind: 'fit', fallback: { kind: 'fixed', ch: 6 } },
            demand: () => 16,
            clip: false,
            cell: allowCell,
        },
        {
            id: 'preview',
            // The thumbnail is narrower than any label, so the header is for screen readers.
            label: tString('askCmdr.renameReview.preview'),
            labelHidden: true,
            width: { kind: 'fixed', px: 36 },
            clip: false,
            cell: previewCell,
        },
        {
            id: 'original',
            label: tString('askCmdr.renameReview.originalName'),
            width: { kind: 'share', minPx: 120 },
            demand: ({ row, measure }: ColumnDemandArgs<ListRow>) =>
                row.kind === 'file' ? measure.text(row.row.sourceName) : 0,
            cell: originalCell,
        },
        {
            id: 'arrow',
            label: '',
            width: { kind: 'fixed', px: 14 },
            tone: 'tertiary',
            cell: arrowCell,
        },
        {
            id: 'new',
            label: tString('askCmdr.renameReview.newName'),
            width: { kind: 'share', minPx: 160 },
            // The STORED name, not the draft, so typing doesn't move the columns. The field's
            // transparent 1 px border adds 2 px.
            demand: ({ row, measure }: ColumnDemandArgs<ListRow>) =>
                row.kind === 'file' ? measure.text(row.row.destinationName) + 2 : 0,
            clip: false,
            cell: newNameCell,
        },
        {
            // No demand: evidence always wants more, so it's the column the spare width lands in.
            id: 'why',
            label: tString('askCmdr.renameReview.whyThisName'),
            width: { kind: 'share', minPx: 200 },
            clip: false,
            cell: whyCell,
        },
    ])

    // ── Per-row thumbnails ────────────────────────────────────────────────────
    // Reviewing 50 rows means scanning for the odd wrong one, so every row shows its own
    // image; the focused row's file opens in the full viewer with Space. A row with no
    // thumbnail (not an image, unreadable, on a drive that isn't mounted here) shows a
    // neutral glyph and stays fully reviewable.

    /** `rowId` → `cmdr-media://` URL, for the rows we could tokenize. */
    let thumbnailUrls = $state<Record<string, string>>({})
    /** `rowId` → the token minted for it, so exactly the rows that leave get theirs dropped. */
    let mintedTokens: Record<string, string> = {}
    /** Monotonic id, so a late mint for a closed review can't install or leak tokens. */
    let thumbnailSeq = 0
    /** The row whose preview button holds focus, so the whole row reads as focused. */
    let focusedRowId = $state<string | null>(null)

    async function dropTokens(tokens: string[]): Promise<void> {
        if (tokens.length === 0) return
        await mediaIndexDropThumbnailTokens(tokens).catch(() => {
            // Best-effort: a failed drop only risks a stale map entry, never correctness.
        })
    }

    /** Everything minted so far, for a closing review. */
    async function releaseThumbnails(): Promise<void> {
        const toDrop = Object.values(mintedTokens)
        mintedTokens = {}
        await dropTokens(toDrop)
    }

    /**
     * Mint what the review gained and drop what it lost.
     *
     * Incremental rather than a fresh pass per batch: a job's later batches must not re-mint
     * the rows the user is already looking at, and a token has no window-close choke point, so
     * a row that leaves owes its token back right then.
     */
    async function syncThumbnails(rows: Array<{ rowId: string; sourcePath: string }>, seq: number): Promise<void> {
        const present: Record<string, true> = {}
        for (const row of rows) present[row.rowId] = true
        const gone = Object.keys(mintedTokens).filter((rowId) => !(rowId in present))
        if (gone.length > 0) {
            const toDrop = gone.map((rowId) => mintedTokens[rowId])
            mintedTokens = withoutKeys(mintedTokens, gone)
            thumbnailUrls = withoutKeys(thumbnailUrls, gone)
            // A draft belongs to the row it was typed on, and that row is gone.
            nameDrafts = withoutKeys(nameDrafts, gone)
            void dropTokens(toDrop)
        }
        const minted: Record<string, string> = {}
        const urls: Record<string, string> = {}
        await Promise.all(
            rows
                .filter((row) => !(row.rowId in mintedTokens))
                .map(async (row) => {
                    try {
                        const token = await mediaIndexThumbnailToken(row.sourcePath)
                        if (token === null) return
                        minted[row.rowId] = token
                        urls[row.rowId] = mediaUrl(token)
                    } catch {
                        // No token → the row falls back to the neutral glyph.
                    }
                }),
        )
        if (seq !== thumbnailSeq) {
            void dropTokens(Object.values(minted))
            return
        }
        mintedTokens = { ...mintedTokens, ...minted }
        thumbnailUrls = { ...thumbnailUrls, ...urls }
    }

    function withoutKeys(source: Record<string, string>, keys: string[]): Record<string, string> {
        return Object.fromEntries(Object.entries(source).filter(([key]) => !keys.includes(key)))
    }

    // One pass per change to the SET of batches on show (the token map has no window-close
    // choke point, so a missed drop leaks path mappings). Depends on the proposal ids ALONE:
    // preflight mutates rows in place on every recheck, so reading the rows reactively here
    // would re-mint every token each time.
    $effect(() => {
        const key = proposals.map((proposal) => proposal.proposalId).join('\n')
        const seq = ++thumbnailSeq
        if (key === '') {
            nameDrafts = {}
            void releaseThumbnails()
            return
        }
        const rows = untrack(() =>
            (askCmdrState.renameReview?.proposals ?? []).flatMap((proposal) =>
                proposal.rows.map((row) => ({ rowId: row.rowId, sourcePath: row.sourcePath })),
            ),
        )
        void syncThumbnails(rows, seq)
    })

    onDestroy(() => {
        thumbnailSeq += 1
        void releaseThumbnails()
    })

    /** Arrow keys walk the preview buttons, so the preview follows the focused row with no
     *  mouse. Tab still reaches every control; this only makes 50 rows navigable. */
    function movePreviewFocus(from: HTMLElement, delta: number): void {
        const buttons = [...(from.closest('.rows')?.querySelectorAll<HTMLButtonElement>('.preview-open') ?? [])]
        buttons[buttons.indexOf(from as HTMLButtonElement) + delta]?.focus()
    }

    function onPreviewKeydown(event: KeyboardEvent): void {
        const delta = event.key === 'ArrowDown' ? 1 : event.key === 'ArrowUp' ? -1 : 0
        if (delta === 0 || !(event.currentTarget instanceof HTMLElement)) return
        event.preventDefault()
        movePreviewFocus(event.currentTarget, delta)
    }

    // ── Editing a proposed name ───────────────────────────────────────────────
    // Allow-or-deny left the user with the model's name or the old one, which is the pressure
    // that produces "approved because it looked plausible". The field is the third option.
    // The BACKEND owns the result: it validates the name, drops the row's evidence (the quote
    // described the model's name), and invalidates the accepted preflight, so the edited name is
    // rechecked before it can be applied. A name it won't take leaves the row as it was.

    /** What's typed in each row's field, by row id. Absent means "showing the stored name", so
     *  no seeding pass is needed and a fresh proposal starts clean. */
    let nameDrafts = $state<Record<string, string>>({})

    function draftName(row: { rowId: string; destinationName: string }): string {
        return nameDrafts[row.rowId] ?? row.destinationName
    }

    function onNameInput(event: Event, rowId: string): void {
        if (event.currentTarget instanceof HTMLInputElement) nameDrafts[rowId] = event.currentTarget.value
    }

    /** Put the field back on the row's STORED name: what a refused edit reverts to, and what an
     *  accepted one already shows. */
    function resetDraft(rowId: string): void {
        const row = proposals.flatMap((proposal) => proposal.rows).find((candidate) => candidate.rowId === rowId)
        if (row) nameDrafts[rowId] = row.destinationName
    }

    /** Commit what's in the field. Blur and Enter both land here; an unchanged name no-ops.
     *  The batch travels with the row: a revise is scoped to the proposal that staged it. */
    function commitName(proposalId: string, rowId: string): void {
        const row = proposals
            .find((candidate) => candidate.proposalId === proposalId)
            ?.rows.find((candidate) => candidate.rowId === rowId)
        if (!row) return
        void reviseRenameRow(proposalId, rowId, draftName(row)).then(() => {
            resetDraft(rowId)
        })
    }

    function onNameKeydown(event: KeyboardEvent, proposalId: string, rowId: string): void {
        if (event.key === 'Enter') {
            event.preventDefault()
            commitName(proposalId, rowId)
        } else if (event.key === 'Escape') {
            // Abandon this edit rather than closing the whole review over a typo.
            event.stopPropagation()
            resetDraft(rowId)
        }
    }

    onMount(() => {
        const listener = onDirectoryDiff((diff) => {
            for (const batch of diff.batches) void renameReviewListingChanged(batch.changes)
        })
        return () => {
            void listener
                .then((unlisten) => {
                    unlisten()
                })
                .catch(() => {})
        }
    })
</script>

{#snippet groupHeading({ row: item }: ColumnListCellContext<ListRow>)}
    {#if item.kind === 'folder'}
        <!-- A job can span folders, so the list says which one a row is in. -->
        <span class="folder-heading" use:useShortenMiddle={{ text: item.folder, preferBreakAt: '/', startRatio: 0.3 }}
        ></span>
    {:else if item.kind === 'expired'}
        <span class="batch-expired" role="status">{tString('askCmdr.renameReview.expired')}</span>
    {/if}
{/snippet}

{#snippet allowCell({ row: item }: ColumnListCellContext<ListRow>)}
    {@const file = fileOf(item)}
    {#if file}
        {@const row = file.row}
        <span class="allow">
            <Checkbox
                checked={row.allowed}
                disabled={Boolean(row.blockedReason) || preflighting}
                ariaLabel={row.allowed
                    ? `${tString('askCmdr.renameReview.deny')}: ${row.sourceName}`
                    : `${tString('askCmdr.renameReview.allow')}: ${row.sourceName}`}
                onCheckedChange={(checked: boolean) => {
                    setRenameRowAllowed(file.proposalId, row.rowId, checked)
                }}
            />
        </span>
    {/if}
{/snippet}

<!-- Seeing the file is the whole point: a plausible wrong name only looks wrong next to the
     picture. -->
{#snippet previewCell({ row: item }: ColumnListCellContext<ListRow>)}
    {@const file = fileOf(item)}
    {#if file}
        {@const row = file.row}
        <button
            type="button"
            class="preview-open"
            data-row-id={row.rowId}
            aria-label={tString('askCmdr.renameReview.openPreview', { name: row.sourceName })}
            use:tooltip={tString('askCmdr.renameReview.openPreviewTooltip')}
            onclick={() => {
                void openFileViewer(row.sourcePath, row.volumeId)
            }}
            onkeydown={onPreviewKeydown}
            onfocus={() => {
                focusedRowId = row.rowId
            }}
            onblur={() => {
                if (focusedRowId === row.rowId) focusedRowId = null
            }}
        >
            {#if thumbnailUrls[row.rowId]}
                <!-- The button carries the accessible name, so the image is presentational
                     (axe image-redundant-alt). -->
                <img src={thumbnailUrls[row.rowId]} alt="" loading="lazy" draggable="false" />
            {:else}
                <span class="preview-fallback" data-preview="none">
                    <Icon name="file" size={18} aria-hidden="true" />
                </span>
            {/if}
        </button>
    {/if}
{/snippet}

{#snippet originalCell({ row: item }: ColumnListCellContext<ListRow>)}
    {@const file = fileOf(item)}
    {#if file}
        <span
            class="fname"
            class:is-blocked={file.row.blockedReason}
            use:useShortenMiddle={{ text: file.row.sourceName, preferBreakAt: '.', startRatio: 0.7 }}
        ></span>
    {/if}
{/snippet}

{#snippet arrowCell()}
    <span class="arrow"><Icon name="arrow-right" size={14} aria-hidden="true" /></span>
{/snippet}

{#snippet newNameCell({ row: item }: ColumnListCellContext<ListRow>)}
    {@const file = fileOf(item)}
    {#if file}
        {@const row = file.row}
        {@const hasBadges =
            row.warnings.includes('extensionChanged') ||
            row.warnings.includes('cycle') ||
            row.blockedReason === 'targetExists' ||
            row.blockedReason === 'sourceMissing'}
        {@const provenance = nameProvenance(row)}
        {@const keptName = provenance === 'nameKept'}
        {@const provenanceLabel = keptName
            ? tString('askCmdr.renameReview.nameKeptTooltip')
            : tString('askCmdr.renameReview.nothingReadTooltip')}
        <span class="name" class:is-blocked={row.blockedReason}>
            <!-- Editable, so a wrong name can be corrected in place instead of abandoned. The
                 value is one-way from the server: the field is the edit buffer, and a commit
                 puts back whatever the backend accepted. -->
            <TextInput
                variant="chromeless"
                radius="sm"
                spellcheck="false"
                autocomplete="off"
                data-row-id={row.rowId}
                value={draftName(row)}
                invalid={row.nameRejected}
                ariaLabel={tString('askCmdr.renameReview.editName', { name: row.sourceName })}
                oninput={(event: Event) => {
                    onNameInput(event, row.rowId)
                }}
                onkeydown={(event: KeyboardEvent) => {
                    onNameKeydown(event, file.proposalId, row.rowId)
                }}
                onblur={() => {
                    commitName(file.proposalId, row.rowId)
                }}
            />
            {#if row.nameRejected}
                <small class="rejected" role="status">{tString('askCmdr.renameReview.nameRejected')}</small>
            {/if}
            {#if provenance === 'nothingRead' || provenance === 'nameKept'}
                <!-- Scannable per row, not only inferable from the evidence column: this is the
                     state M4's "keep a neutral name" path lands in, and it must keep saying
                     nothing inside the file was read. -->
                <span class="badges">
                    <span
                        class="quiet-badge"
                        data-name-provenance={provenance}
                        tabindex="0"
                        aria-label={provenanceLabel}
                        use:tooltip={provenanceLabel}
                        >{keptName
                            ? tString('askCmdr.renameReview.nameKeptBadge')
                            : tString('askCmdr.renameReview.nothingReadBadge')}</span
                    >
                </span>
            {/if}
            {#if hasBadges}
                <span class="badges">
                    {#if row.warnings.includes('extensionChanged')}
                        <span
                            class="warning-badge"
                            data-rename-warning="extensionChanged"
                            tabindex="0"
                            aria-label={tString('askCmdr.renameReview.extensionTooltip')}
                            use:tooltip={tString('askCmdr.renameReview.extensionTooltip')}
                            >{tString('askCmdr.renameReview.extensionBadge')}</span
                        >
                    {/if}
                    {#if row.warnings.includes('cycle')}
                        <span
                            class="warning-badge"
                            data-rename-warning="cycle"
                            tabindex="0"
                            aria-label={tString('askCmdr.renameReview.cycleTooltip')}
                            use:tooltip={tString('askCmdr.renameReview.cycleTooltip')}
                            >{tString('askCmdr.renameReview.cycleBadge')}</span
                        >
                    {/if}
                    {#if row.blockedReason === 'targetExists'}
                        <span
                            class="danger-badge"
                            data-warning="overwrite"
                            tabindex="0"
                            aria-label={tString('askCmdr.renameReview.overwriteTooltip')}
                            use:tooltip={tString('askCmdr.renameReview.overwriteTooltip')}
                            >{tString('askCmdr.renameReview.overwriteBadge')}</span
                        >
                    {/if}
                    {#if row.blockedReason === 'sourceMissing'}
                        <span
                            class="danger-badge"
                            data-warning="source-missing"
                            tabindex="0"
                            aria-label={tString('askCmdr.renameReview.sourceMissingTooltip')}
                            use:tooltip={tString('askCmdr.renameReview.sourceMissingTooltip')}
                            >{tString('askCmdr.renameReview.sourceMissingBadge')}</span
                        >
                    {/if}
                </span>
            {/if}
            {#if row.blockedReason}
                <small>{tString('askCmdr.renameReview.blocked')}</small>
            {/if}
        </span>
    {/if}
{/snippet}

{#snippet whyCell({ row: item }: ColumnListCellContext<ListRow>)}
    {@const file = fileOf(item)}
    {#if file}
        {@const row = file.row}
        <!-- Evidence and the text Cmdr read in the image are both untrusted text, so they render
             as plain text (Svelte escapes it), never `{@html}`. -->
        <span class="why" class:is-blocked={row.blockedReason} data-evidence-source={row.evidence.source}>
            <span class="evidence-source">{evidenceSourceLabel(row.evidence.source)}</span>
            {#if row.coverage}
                {@const coverage = row.coverage}
                {@const strength = coverageStrength(coverage)}
                <!-- The quote inside the line it came from: a bare quote made a sliver of a page
                     of OCR look as strong as a decisive match. -->
                <span class="evidence-detail"
                    >{#if coverage.trimmedBefore}…{/if}{coverage.contextBefore}<mark>{coverage.matchedText}</mark
                    >{coverage.contextAfter}{#if coverage.trimmedAfter}…{/if}</span
                >
                <span class="coverage" data-coverage={strength}>
                    {#if strength === 'thin'}
                        <!-- `role="img"`: the marker's meaning IS the icon, so its label can't
                             come from text content the way a badge's does. -->
                        <span
                            class="coverage-warning"
                            data-coverage-warning="thin"
                            role="img"
                            tabindex="0"
                            aria-label={tString('askCmdr.renameReview.coverageThin')}
                            use:tooltip={tString('askCmdr.renameReview.coverageThin')}
                            ><Icon name="triangle-alert" size={12} aria-hidden="true" /></span
                        >
                    {/if}
                    {tString('askCmdr.renameReview.coverage', {
                        matchedText: formatInteger(coverage.matchedChars),
                        matched: coverage.matchedChars,
                        totalText: formatInteger(coverage.deliveredChars),
                        total: coverage.deliveredChars,
                    })}
                </span>
            {:else if row.evidence.detail}
                <!-- A user-typed name carries no detail at all: the label above IS the whole
                     answer. -->
                <span class="evidence-detail">{row.evidence.detail}</span>
            {/if}
        </span>
    {/if}
{/snippet}

{#if review}
    <ModalDialog
        titleId="bulk-rename-review-title"
        dialogId="bulk-rename-review"
        resizable
        containerStyle="width: min(1040px, 90vw)"
        onclose={cancelRenameReview}
    >
        {#snippet title()}{tString('askCmdr.renameReview.title')}{/snippet}

        <div class="dialog-body">
            <p class="description">{tString('askCmdr.renameReview.description')}</p>
            {#if allExpired}
                <p class="notice" role="status">{tString('askCmdr.renameReview.expired')}</p>
            {:else}
                <div class="bulk-actions">
                    <Button size="mini" onclick={allowAllRenameRows} disabled={preflighting}>
                        {tString('askCmdr.renameReview.allowAll')}
                    </Button>
                    <Button size="mini" onclick={denyAllRenameRows} disabled={preflighting}>
                        {tString('askCmdr.renameReview.denyAll')}
                    </Button>
                    <span class="summary" role="status" aria-live="polite">
                        {tString('askCmdr.renameReview.status', { allowed: allowedCount, blocked: blockedCount })}
                    </span>
                </div>
                <div class="rows" aria-busy={preflighting}>
                    <ColumnList
                        {columns}
                        rows={listRows}
                        rowKey={(item: ListRow) => item.key}
                        semantics="table"
                        virtualized={false}
                        ariaLabel={tString('askCmdr.renameReview.title')}
                        {cursorIndex}
                        isGroupHeading={(item: ListRow) => item.kind !== 'file'}
                        {groupHeading}
                        rowClass="review-row"
                    />
                </div>
            {/if}
        </div>

        {#snippet footer()}
            <Button onclick={cancelRenameReview}>{tString('askCmdr.renameReview.cancel')}</Button>
            <Button
                variant="primary"
                onclick={applyRenameReview}
                disabled={preflighting || allowedCount === 0}
                aria-label={renameLabel}>{renameLabel}</Button
            >
        {/snippet}
    </ModalDialog>
{/if}

<style>
    /* Fills the resizable modal body so the list, not the whole dialog, scrolls. */
    .dialog-body {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-md);
        height: 100%;
        min-height: 0;
        font-size: var(--font-size-md);
    }

    .description,
    .notice {
        margin: 0;
        color: var(--color-text-secondary);
    }

    .notice {
        padding: var(--spacing-sm);
        background: var(--color-bg-tertiary);
        border-radius: var(--radius-sm);
    }

    .bulk-actions {
        display: flex;
        align-items: center;
        flex-wrap: wrap;
        gap: var(--spacing-xs);
    }

    .summary {
        margin-left: auto;
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }

    /* The `ColumnList` fills what's left, and its own viewport scrolls. */
    .rows {
        display: flex;
        flex-direction: column;
        flex: 1 1 auto;
        min-height: 0;
    }

    /* `ColumnList` cells don't wrap; the multi-line cells opt back in on their own content. */
    .allow,
    .arrow,
    .preview-fallback {
        display: flex;
    }

    /* A fixed square, so 50 rows keep one rhythm whatever shape the images are. */
    .preview-open {
        display: flex;
        align-items: center;
        justify-content: center;
        width: 36px;
        height: 36px;
        margin-block: var(--spacing-xxs);
        padding: 0;
        overflow: hidden;
        border: 1px solid var(--color-border-subtle);
        border-radius: var(--radius-sm);
        background: var(--color-bg-tertiary);
        cursor: default;
    }

    .preview-open:hover {
        border-color: var(--color-accent);
    }

    .preview-open:focus-visible {
        outline: 2px solid var(--color-accent);
        outline-offset: 2px;
    }

    .preview-open img {
        width: 100%;
        height: 100%;
        object-fit: cover;
    }

    /* No thumbnail (not an image, unreadable, or a drive that isn't mounted here) degrades to
       a neutral glyph: never a broken image, never an empty cell, and the row stays
       reviewable. */
    .preview-fallback {
        color: var(--color-text-tertiary);
    }

    /* Which folder the rows under it are in, once a job spans more than one. */
    .folder-heading {
        display: block;
        padding-top: var(--spacing-sm);
        color: var(--color-text-primary);
        font-weight: 600;
    }

    /* A notice, so it wraps rather than cutting off like a heading. */
    .batch-expired {
        display: block;
        padding-block: var(--spacing-xs);
        font-weight: normal;
        white-space: normal;
    }

    .fname {
        display: block;
    }

    /* The proposed name is a field, not a label: correcting a wrong name in place is the point.
       `chromeless` keeps 50 rows from reading as a form; the primitive owns the rest. */
    .name,
    .why {
        display: block;
        padding-block: var(--spacing-xs);
        white-space: normal;
    }

    .name :global(.text-field) {
        width: 100%;
    }

    .rejected {
        color: var(--color-error-text);
    }

    .name .badges {
        display: flex;
        flex-wrap: wrap;
        gap: var(--spacing-xs);
        margin-top: var(--spacing-xs);
    }

    .is-blocked {
        color: var(--color-text-secondary);
    }

    /* Why this name: a quiet caption naming the source, then the quote or note under it.
       The caption is what keeps a metadata-only name from reading as content-derived. */
    .evidence-source {
        display: block;
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }

    /* A long quote wraps rather than stretching the column, and an unbroken string can't
       push out of the list. The backend caps the detail, so the four-line clamp is a floor
       against a squeezed column, not a routine truncation. */
    .evidence-detail {
        display: -webkit-box;
        margin-top: var(--spacing-xxs);
        overflow: hidden;
        overflow-wrap: anywhere;
        -webkit-box-orient: vertical;
        -webkit-line-clamp: 4;
        line-clamp: 4;
    }

    /* The matched span, so the eye lands on the quote and reads the surrounding line as
       context rather than as part of it. */
    .evidence-detail mark {
        background: var(--color-accent-subtle);
        color: var(--color-text-primary);
        border-radius: var(--radius-sm);
        padding: 0 var(--spacing-xxs);
    }

    /* How much of the image's text the quote covers. A thin match takes the warning tone AND
       a marker, so it doesn't rely on color alone. */
    .coverage {
        display: flex;
        align-items: center;
        gap: var(--spacing-xxs);
        margin-top: var(--spacing-xxs);
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
    }

    .coverage[data-coverage='thin'] {
        color: var(--color-warning-text);
    }

    .coverage-warning {
        display: inline-flex;
    }

    .coverage-warning:focus-visible {
        outline: 2px solid var(--color-accent);
        outline-offset: 2px;
        border-radius: var(--radius-sm);
    }

    small {
        display: block;
        margin-top: var(--spacing-xxs);
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
    }

    .warning-badge,
    .danger-badge,
    .quiet-badge {
        display: inline-flex;
        width: fit-content;
        padding: 0 var(--spacing-xs);
        border-radius: var(--radius-sm);
        font-size: var(--font-size-sm);
        white-space: nowrap;
    }

    /* "Nothing was read inside this file" is a limit to notice, not a problem to fix, so it
       takes the quiet tone rather than the warning one. It still says so on every such row. */
    .quiet-badge {
        color: var(--color-text-secondary);
        background: var(--color-bg-tertiary);
    }

    .quiet-badge:focus-visible {
        outline: 2px solid var(--color-accent);
        outline-offset: 2px;
    }

    .warning-badge {
        color: var(--color-warning-text);
        background: var(--color-warning-bg);
    }

    .danger-badge {
        color: var(--color-error-text);
        background: var(--color-error-bg);
    }
</style>
