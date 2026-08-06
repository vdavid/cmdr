<script lang="ts">
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import Button from '$lib/ui/Button.svelte'
    import TextInput from '$lib/ui/TextInput.svelte'
    import Icon from '$lib/ui/Icon.svelte'
    import Checkbox from '$lib/ui/Checkbox.svelte'
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
    } from './ask-cmdr-trigger.svelte'

    const review = $derived(askCmdrState.renameReview)
    const allowedCount = $derived(review?.rows.filter((row) => row.allowed && !row.blockedReason).length ?? 0)
    const blockedCount = $derived(review?.rows.filter((row) => row.blockedReason).length ?? 0)
    const renameLabel = $derived(tString('askCmdr.renameReview.rename', { count: allowedCount }))

    // ── Per-row thumbnails ────────────────────────────────────────────────────
    // Reviewing 50 rows means scanning for the odd wrong one, so every row shows its own
    // image; the focused row's file opens in the full viewer with Space. A row with no
    // thumbnail (not an image, unreadable, on a drive that isn't mounted here) shows a
    // neutral glyph and stays fully reviewable.

    /** `rowId` → `cmdr-media://` URL, for the rows we could tokenize. */
    let thumbnailUrls = $state<Record<string, string>>({})
    /** Tokens minted for the CURRENT proposal, so we drop exactly them. */
    let mintedTokens: string[] = []
    /** Monotonic id, so a late mint for a closed review can't install or leak tokens. */
    let thumbnailSeq = 0
    /** The row whose preview button holds focus, so the whole row reads as focused. */
    let focusedRowId = $state<string | null>(null)

    async function releaseThumbnails(): Promise<void> {
        if (mintedTokens.length === 0) return
        const toDrop = mintedTokens
        mintedTokens = []
        await mediaIndexDropThumbnailTokens(toDrop).catch(() => {
            // Best-effort: a failed drop only risks a stale map entry, never correctness.
        })
    }

    async function loadThumbnails(rows: Array<{ rowId: string; sourcePath: string }>, seq: number): Promise<void> {
        const minted: string[] = []
        const urls: Record<string, string> = {}
        await Promise.all(
            rows.map(async (row) => {
                try {
                    const token = await mediaIndexThumbnailToken(row.sourcePath)
                    if (token === null) return
                    minted.push(token)
                    urls[row.rowId] = mediaUrl(token)
                } catch {
                    // No token → the row falls back to the neutral glyph.
                }
            }),
        )
        if (seq !== thumbnailSeq) {
            void mediaIndexDropThumbnailTokens(minted).catch(() => {})
            return
        }
        mintedTokens = minted
        thumbnailUrls = urls
    }

    // One mint pass per proposal, dropped when the review closes or another replaces it (the
    // token map has no window-close choke point, so a missed drop leaks path mappings).
    // Depends on the proposal id ALONE: preflight mutates rows in place on every recheck, so
    // reading the rows reactively here would re-mint every token each time.
    $effect(() => {
        const proposalId = askCmdrState.renameReview?.proposalId ?? null
        const seq = ++thumbnailSeq
        thumbnailUrls = {}
        // A draft belongs to the review it was typed in; a new proposal starts from its own names.
        nameDrafts = {}
        void releaseThumbnails()
        if (proposalId === null) return
        const rows = untrack(() =>
            (askCmdrState.renameReview?.rows ?? []).map((row) => ({ rowId: row.rowId, sourcePath: row.sourcePath })),
        )
        void loadThumbnails(rows, seq)
    })

    onDestroy(() => {
        thumbnailSeq += 1
        void releaseThumbnails()
    })

    /** Arrow keys walk the preview buttons, so the preview follows the focused row with no
     *  mouse. Tab still reaches every control; this only makes 50 rows navigable. */
    function movePreviewFocus(from: HTMLElement, delta: number): void {
        const buttons = [...(from.closest('table')?.querySelectorAll<HTMLButtonElement>('.preview-open') ?? [])]
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
        const row = review?.rows.find((candidate) => candidate.rowId === rowId)
        if (row) nameDrafts[rowId] = row.destinationName
    }

    /** Commit what's in the field. Blur and Enter both land here; an unchanged name no-ops. */
    function commitName(rowId: string): void {
        const row = review?.rows.find((candidate) => candidate.rowId === rowId)
        if (!row) return
        void reviseRenameRow(rowId, draftName(row)).then(() => { resetDraft(rowId); })
    }

    function onNameKeydown(event: KeyboardEvent, rowId: string): void {
        if (event.key === 'Enter') {
            event.preventDefault()
            commitName(rowId)
        } else if (event.key === 'Escape') {
            // Abandon this edit rather than closing the whole review over a typo.
            event.stopPropagation()
            resetDraft(rowId)
        }
    }

    onMount(() => {
        const listener = onDirectoryDiff((diff) => {
            void renameReviewListingChanged(diff.changes)
        })
        return () => {
            void listener.then((unlisten) => { unlisten(); }).catch(() => {})
        }
    })
</script>

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
            {#if review.expired}
                <p class="notice" role="status">{tString('askCmdr.renameReview.expired')}</p>
            {:else}
                <div class="bulk-actions">
                    <Button size="mini" onclick={allowAllRenameRows} disabled={review.preflighting}>
                        {tString('askCmdr.renameReview.allowAll')}
                    </Button>
                    <Button size="mini" onclick={denyAllRenameRows} disabled={review.preflighting}>
                        {tString('askCmdr.renameReview.denyAll')}
                    </Button>
                    <span class="summary" role="status" aria-live="polite">
                        {tString('askCmdr.renameReview.status', { allowed: allowedCount, blocked: blockedCount })}
                    </span>
                </div>
                <div class="rows" aria-busy={review.preflighting}>
                    <table>
                        <thead>
                            <tr>
                                <th scope="col" class="allow-col">{tString('askCmdr.renameReview.allow')}</th>
                                <!-- The column is 44 px wide, so its heading is for screen
                                     readers only rather than a truncated visible label. -->
                                <th scope="col" class="preview-col">
                                    <span class="sr-only">{tString('askCmdr.renameReview.preview')}</span>
                                </th>
                                <th scope="col" class="name-col">{tString('askCmdr.renameReview.originalName')}</th>
                                <th scope="col" class="arrow-col" aria-hidden="true"></th>
                                <th scope="col" class="name-col">{tString('askCmdr.renameReview.newName')}</th>
                                <th scope="col" class="why-col">{tString('askCmdr.renameReview.whyThisName')}</th>
                            </tr>
                        </thead>
                        <tbody>
                            {#each review.rows as row (row.rowId)}
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
                                <tr class:blocked={row.blockedReason} class:focused={row.rowId === focusedRowId}>
                                    <td class="allow-cell">
                                        <Checkbox
                                            checked={row.allowed}
                                            disabled={Boolean(row.blockedReason) || review.preflighting}
                                            ariaLabel={row.allowed
                                                ? `${tString('askCmdr.renameReview.deny')}: ${row.sourceName}`
                                                : `${tString('askCmdr.renameReview.allow')}: ${row.sourceName}`}
                                            onCheckedChange={(checked: boolean) => { setRenameRowAllowed(row.rowId, checked); }}
                                        />
                                    </td>
                                    <!-- Seeing the file is the whole point: a plausible wrong
                                         name only looks wrong next to the picture. -->
                                    <td class="preview-cell">
                                        <button
                                            type="button"
                                            class="preview-open"
                                            data-row-id={row.rowId}
                                            aria-label={tString('askCmdr.renameReview.openPreview', { name: row.sourceName })}
                                            use:tooltip={tString('askCmdr.renameReview.openPreviewTooltip')}
                                            onclick={() => { void openFileViewer(row.sourcePath, row.volumeId); }}
                                            onkeydown={onPreviewKeydown}
                                            onfocus={() => { focusedRowId = row.rowId; }}
                                            onblur={() => { if (focusedRowId === row.rowId) focusedRowId = null; }}
                                        >
                                            {#if thumbnailUrls[row.rowId]}
                                                <!-- The button carries the accessible name, so the
                                                     image is presentational (axe image-redundant-alt). -->
                                                <img src={thumbnailUrls[row.rowId]} alt="" loading="lazy" draggable="false" />
                                            {:else}
                                                <span class="preview-fallback" data-preview="none">
                                                    <Icon name="file" size={18} aria-hidden="true" />
                                                </span>
                                            {/if}
                                        </button>
                                    </td>
                                    <td class="name">
                                        <span class="fname" use:useShortenMiddle={{ text: row.sourceName, preferBreakAt: '.', startRatio: 0.7 }}></span>
                                    </td>
                                    <td class="arrow"><Icon name="arrow-right" size={14} aria-hidden="true" /></td>
                                    <td class="name">
                                        <!-- Editable, so a wrong name can be corrected in place
                                             instead of abandoned. The value is one-way from the
                                             server: the field is the edit buffer, and a commit
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
                                            oninput={(event: Event) => { onNameInput(event, row.rowId); }}
                                            onkeydown={(event: KeyboardEvent) => { onNameKeydown(event, row.rowId); }}
                                            onblur={() => { commitName(row.rowId); }}
                                        />
                                        {#if row.nameRejected}
                                            <small class="rejected" role="status">{tString('askCmdr.renameReview.nameRejected')}</small>
                                        {/if}
                                        {#if provenance === 'nothingRead' || provenance === 'nameKept'}
                                            <!-- Scannable per row, not only inferable from the
                                                 evidence column: this is the state M4's "keep a
                                                 neutral name" path lands in, and it must keep
                                                 saying nothing inside the file was read. -->
                                            <span class="badges">
                                                <span
                                                    class="quiet-badge"
                                                    data-name-provenance={provenance}
                                                    tabindex="0"
                                                    aria-label={provenanceLabel}
                                                    use:tooltip={provenanceLabel}
                                                >{keptName
                                                    ? tString('askCmdr.renameReview.nameKeptBadge')
                                                    : tString('askCmdr.renameReview.nothingReadBadge')}</span>
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
                                                    >{tString('askCmdr.renameReview.extensionBadge')}</span>
                                                {/if}
                                                {#if row.warnings.includes('cycle')}
                                                    <span
                                                        class="warning-badge"
                                                        data-rename-warning="cycle"
                                                        tabindex="0"
                                                        aria-label={tString('askCmdr.renameReview.cycleTooltip')}
                                                        use:tooltip={tString('askCmdr.renameReview.cycleTooltip')}
                                                    >{tString('askCmdr.renameReview.cycleBadge')}</span>
                                                {/if}
                                                {#if row.blockedReason === 'targetExists'}
                                                    <span
                                                        class="danger-badge"
                                                        data-warning="overwrite"
                                                        tabindex="0"
                                                        aria-label={tString('askCmdr.renameReview.overwriteTooltip')}
                                                        use:tooltip={tString('askCmdr.renameReview.overwriteTooltip')}
                                                    >{tString('askCmdr.renameReview.overwriteBadge')}</span>
                                                {/if}
                                                {#if row.blockedReason === 'sourceMissing'}
                                                    <span
                                                        class="danger-badge"
                                                        data-warning="source-missing"
                                                        tabindex="0"
                                                        aria-label={tString('askCmdr.renameReview.sourceMissingTooltip')}
                                                        use:tooltip={tString('askCmdr.renameReview.sourceMissingTooltip')}
                                                    >{tString('askCmdr.renameReview.sourceMissingBadge')}</span>
                                                {/if}
                                            </span>
                                        {/if}
                                        {#if row.blockedReason}
                                            <small>{tString('askCmdr.renameReview.blocked')}</small>
                                        {/if}
                                    </td>
                                    <!-- Evidence and the text Cmdr read in the image are both
                                         untrusted text, so they render as plain text (Svelte
                                         escapes it), never `{@html}`. -->
                                    <td class="why" data-evidence-source={row.evidence.source}>
                                        <span class="evidence-source">{evidenceSourceLabel(row.evidence.source)}</span>
                                        {#if row.coverage}
                                            {@const coverage = row.coverage}
                                            {@const strength = coverageStrength(coverage)}
                                            <!-- The quote inside the line it came from: a
                                                 bare quote made a sliver of a page of OCR
                                                 look as strong as a decisive match. -->
                                            <span class="evidence-detail"
                                                >{#if coverage.trimmedBefore}…{/if}{coverage.contextBefore}<mark
                                                    >{coverage.matchedText}</mark
                                                >{coverage.contextAfter}{#if coverage.trimmedAfter}…{/if}</span
                                            >
                                            <span class="coverage" data-coverage={strength}>
                                                {#if strength === 'thin'}
                                                    <!-- `role="img"`: the marker's meaning IS
                                                         the icon, so its label can't come from
                                                         text content the way a badge's does. -->
                                                    <span
                                                        class="coverage-warning"
                                                        data-coverage-warning="thin"
                                                        role="img"
                                                        tabindex="0"
                                                        aria-label={tString('askCmdr.renameReview.coverageThin')}
                                                        use:tooltip={tString('askCmdr.renameReview.coverageThin')}
                                                    ><Icon name="triangle-alert" size={12} aria-hidden="true" /></span>
                                                {/if}
                                                {tString('askCmdr.renameReview.coverage', {
                                                    matchedText: formatInteger(coverage.matchedChars),
                                                    totalText: formatInteger(coverage.deliveredChars),
                                                })}
                                            </span>
                                        {:else if row.evidence.detail}
                                            <!-- A user-typed name carries no detail at all: the
                                                 label above IS the whole answer. -->
                                            <span class="evidence-detail">{row.evidence.detail}</span>
                                        {/if}
                                    </td>
                                </tr>
                            {/each}
                        </tbody>
                    </table>
                </div>
            {/if}
        </div>

        {#snippet footer()}
            <Button onclick={cancelRenameReview}>{tString('askCmdr.renameReview.cancel')}</Button>
            <Button
                variant="primary"
                onclick={applyRenameReview}
                disabled={review.preflighting || review.expired || allowedCount === 0}
                aria-label={renameLabel}
            >{renameLabel}</Button>
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

    /* The list scrolls; row dividers carry the structure, no surrounding border. */
    .rows {
        flex: 1 1 auto;
        min-height: 0;
        overflow: auto;
    }

    table {
        width: 100%;
        border-collapse: collapse;
        table-layout: fixed;
    }

    /* Column headers are quiet chrome, not the browser's bold black default. */
    th {
        padding: var(--spacing-sm) var(--spacing-md);
        font-size: var(--font-size-sm);
        font-weight: 500;
        color: var(--color-text-secondary);
        text-align: left;
        border-bottom: 1px solid var(--color-border-subtle);
        background: var(--color-bg-secondary);
        position: sticky;
        top: 0;
        z-index: var(--z-sticky);
    }

    td {
        padding: var(--spacing-sm) var(--spacing-md);
        vertical-align: middle;
        border-bottom: 1px solid var(--color-border-subtle);
    }

    tbody tr:last-child td {
        border-bottom: none;
    }

    /* Three fixed-width columns take their pixels first; the three text columns share what's
       left in these proportions, so evidence gets the most room and both names still fit at
       the dialog's default width. */
    .allow-col,
    .allow-cell {
        width: 56px;
        text-align: center;
    }

    .allow-cell {
        line-height: 0;
    }

    .preview-col,
    .preview-cell {
        width: 44px;
        padding-right: 0;
        line-height: 0;
    }

    /* A fixed square, so 50 rows keep one rhythm whatever shape the images are. */
    .preview-open {
        display: flex;
        align-items: center;
        justify-content: center;
        width: 36px;
        height: 36px;
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
        display: flex;
        color: var(--color-text-tertiary);
    }

    /* The focused row is highlighted, so "the preview follows the focus" is visible while
       arrowing down the list. */
    tbody tr.focused {
        background: var(--color-accent-subtle);
    }

    .arrow-col,
    .arrow {
        width: 32px;
        text-align: center;
        color: var(--color-text-tertiary);
    }

    .name-col {
        width: 25%;
    }

    .why-col {
        width: 42%;
    }

    .arrow :global(svg) {
        vertical-align: middle;
    }

    .name .fname {
        display: block;
    }

    /* The proposed name is a field, not a label: correcting a wrong name in place is the point.
       `chromeless` keeps 50 rows from reading as a form; the primitive owns the rest. */
    .name :global(.text-field) {
        width: 100%;
    }

    .rejected {
        color: var(--color-error-text);
    }

    .name .badges {
        display: inline-flex;
        flex-wrap: wrap;
        gap: var(--spacing-xs);
        margin-top: var(--spacing-xs);
    }

    tr.blocked .name {
        color: var(--color-text-secondary);
    }

    /* Why this name: a quiet caption naming the source, then the quote or note under it.
       The caption is what keeps a metadata-only name from reading as content-derived. */
    .why {
        vertical-align: top;
    }

    .evidence-source {
        display: block;
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }

    /* A long quote wraps rather than stretching the column, and an unbroken string can't
       push out of the table. The backend caps the detail, so the four-line clamp is a floor
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

    tr.blocked .why {
        color: var(--color-text-secondary);
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
