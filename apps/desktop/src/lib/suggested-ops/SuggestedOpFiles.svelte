<script lang="ts">
    // One group's file list: a `ColumnList` over a windowed source, drawn right away.
    //
    // The list starts at `INITIAL_ROWS` and grows on "Show more" / "Show all"; past
    // `MAX_VISIBLE_ROWS` it stops growing and scrolls, still fetching only the rows it draws,
    // because a group of 60,000 ops is legitimate. Size and Last modified are what the disk says
    // NOW; a value that moved since the suggestion shows both, a file that's gone says so, and
    // one Cmdr couldn't check falls back to the creation snapshot, labelled as that.
    import type { Snippet } from 'svelte'
    import ColumnList from '#lib/ui/ColumnList.svelte'
    import Checkbox from '#lib/ui/Checkbox.svelte'
    import Icon from '#lib/ui/Icon.svelte'
    import StatusGlyph from '#lib/ui/StatusGlyph.svelte'
    import DateLabel from '#lib/ui/DateLabel.svelte'
    import LinkButton from '#lib/ui/LinkButton.svelte'
    import Button from '#lib/ui/Button.svelte'
    import {
        columnListProps,
        type ColumnListCellContext,
        type ColumnListColumn,
        type ColumnListWindowedSource,
    } from '#lib/ui/column-list-types.js'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { formatInteger } from '#lib/intl/number-format.js'
    import {
        formatDateTime,
        getFileSizeFormat,
        getFileSizeUnit,
        getUseAppIconsForDocuments,
    } from '#lib/settings/reactive-settings.svelte.js'
    import { formatSizeForDisplay, formatSizeText } from '#lib/file-explorer/selection/selection-info-utils.js'
    import { getCachedIcon, iconCacheVersion, prefetchIcons } from '#lib/icon-cache.js'
    import { useShortenMiddle } from '#lib/utils/shorten-middle-action.js'
    import { dependOn } from '#lib/utils/reactivity.js'
    import type { SuggestedGroupView, SuggestedOpView } from '#lib/tauri-commands/index.js'
    import {
        SHOW_MORE_STEP,
        ensureOpWindow,
        isOpSelected,
        opAt,
        reviewOf,
        showAllRows,
        showMoreRows,
        selectionState,
        showRange,
        shownRows,
        toggleAllOps,
        toggleOp,
        totalRows,
    } from './suggested-ops-trigger.svelte'
    import { fileIconId, fileName, parentFolder, rowFacts } from './suggested-ops-copy'

    /** The list grows with "Show more" up to this many rows, then scrolls. */
    const MAX_VISIBLE_ROWS = 20

    interface Props {
        group: SuggestedGroupView
        /** The group's decision buttons, right-aligned on the row under the list, opposite
         *  "Show more" / "Show all", so a short group doesn't spend two rows on its footer. */
        actions: Snippet
    }

    const { group, actions }: Props = $props()

    const review = $derived(reviewOf(group.groupId))
    const shown = $derived(shownRows(group))
    const total = $derived(totalRows(group))
    const remaining = $derived(Math.max(0, total - shown))

    /** The ColumnList row and header heights, in its own terms: one line of the dialog type plus
     *  the row's (or the header's) vertical padding. The header adds its 1px rule, the frame 2px. */
    const ROW_HEIGHT = '(var(--font-size-md) * var(--font-line-height-normal) + 2 * var(--spacing-xxs))'
    const HEADER_HEIGHT = '(var(--font-size-md) * var(--font-line-height-normal) + 2 * var(--spacing-xs) + 1px)'
    /** Exactly as tall as the rows shown, up to `MAX_VISIBLE_ROWS`; past that the list scrolls. */
    const frameHeight = $derived(
        `calc(${HEADER_HEIGHT} + ${String(Math.max(1, Math.min(shown, MAX_VISIBLE_ROWS)))} * ${ROW_HEIGHT} + 2px)`,
    )
    /** Every op shares one folder, named once in the group's facts, so rows show the name alone. */
    const sharedFolder = $derived(group.sourceFolder !== null)

    const source: ColumnListWindowedSource<SuggestedOpView> = {
        get count() {
            return shownRows(group)
        },
        getRow: (index) => opAt(group.groupId, index) ?? undefined,
        onRangeChange: ({ start, end }) => {
            void showRange(group.groupId, start, end)
        },
    }

    // The icons the held page needs, fetched once per page through the shared cache.
    $effect(() => {
        const ops = review?.window?.ops
        if (!ops || ops.length === 0) return
        void prefetchIcons(
            ops.map((op) => fileIconId(op.sourcePath)),
            getUseAppIconsForDocuments(),
        )
    })

    function iconUrl(path: string): string | undefined {
        dependOn($iconCacheVersion)
        return getCachedIcon(fileIconId(path))
    }

    const columns = $derived<ColumnListColumn<SuggestedOpView>[]>([
        {
            id: 'include',
            label: tString('suggestedOps.columnInclude'),
            header: includeHeader,
            width: { kind: 'fixed', px: 16 },
            clip: false,
            cell: includeCell,
        },
        { id: 'icon', label: '', width: { kind: 'fixed', px: 16 }, clip: false, cell: iconCell },
        {
            id: 'name',
            label: tString('suggestedOps.columnName'),
            width: { kind: 'share', minPx: 160 },
            emphasis: true,
            cell: nameCell,
        },
        ...(sharedFolder
            ? []
            : [
                  {
                      id: 'folder',
                      label: tString('suggestedOps.columnFolder'),
                      width: { kind: 'share', minPx: 120 },
                      tone: 'tertiary',
                      cell: folderCell,
                  } satisfies ColumnListColumn<SuggestedOpView>,
              ]),
        {
            id: 'size',
            label: tString('suggestedOps.columnSize'),
            width: { kind: 'fixed', ch: 14 },
            align: 'end',
            tone: 'secondary',
            clip: false,
            cell: sizeCell,
        },
        {
            id: 'modified',
            label: tString('suggestedOps.columnModified'),
            width: { kind: 'fixed', ch: 19 },
            align: 'end',
            tone: 'tertiary',
            clip: false,
            cell: modifiedCell,
        },
    ])

    function factsOf(op: SuggestedOpView) {
        return rowFacts(op, review?.liveFacts.get(op.opId))
    }

    /** The size column's own options, so a size here reads exactly as the file panes show it:
     *  the same unit choice (`listing.sizeUnit`) and the same 1000/1024 base. */
    const sizeOptions = $derived({ unit: getFileSizeUnit(), format: getFileSizeFormat() })

    function sizeText(bytes: number | null): string {
        return bytes === null ? tString('suggestedOps.factUnknown') : formatSizeText(bytes, sizeOptions)
    }

    function dateText(seconds: number | null): string {
        return seconds === null ? tString('suggestedOps.factUnknown') : formatDateTime(seconds)
    }
</script>

<!-- Ticks or unticks every op, mixed when the rows disagree. Unticking all is an empty
     "only these" set, so it costs the wire nothing at any size. -->
{#snippet includeHeader()}
    {@const state = selectionState(group.groupId)}
    <span class="include">
        <Checkbox
            checked={state === 'all'}
            indeterminate={state === 'some'}
            onCheckedChange={() => {
                toggleAllOps(group.groupId)
            }}
            ariaLabel={tString('suggestedOps.includeAll')}
        />
    </span>
{/snippet}

{#snippet includeCell({ row }: ColumnListCellContext<SuggestedOpView>)}
    <span class="include">
        <Checkbox
            checked={isOpSelected(group.groupId, row)}
            onCheckedChange={() => {
                toggleOp(group.groupId, row)
            }}
            ariaLabel={tString('suggestedOps.includeFile', { name: fileName(row.sourcePath) })}
        />
    </span>
{/snippet}

{#snippet iconCell({ row }: ColumnListCellContext<SuggestedOpView>)}
    <span class="icon-box">
        {#if iconUrl(row.sourcePath)}
            <img class="icon-img" src={iconUrl(row.sourcePath)} alt="" width="16" height="16" />
        {:else}
            <span class="icon-fallback"><Icon name="file" size={16} aria-hidden="true" /></span>
        {/if}
    </span>
{/snippet}

<!-- Mid-truncating, so the extension stays visible; the full name rides the tooltip. A new name,
     whatever the verb, follows an arrow in the same cell. -->
{#snippet nameCell({ row }: ColumnListCellContext<SuggestedOpView>)}
    <span class="name-cell">
        <span
            class="shortened"
            use:useShortenMiddle={{
                text: fileName(row.sourcePath),
                preferBreakAt: '.',
                startRatio: 0.7,
                tooltipWhenTruncated: true,
            }}
        ></span>
        {#if row.newName}
            <span class="arrow" aria-hidden="true"><Icon name="arrow-right" size={12} /></span>
            <span class="sr-only">{tString('suggestedOps.columnNewName')}</span>
            <span
                class="shortened new-name"
                use:useShortenMiddle={{ text: row.newName, preferBreakAt: '.', startRatio: 0.7, tooltipWhenTruncated: true }}
            ></span>
        {/if}
    </span>
{/snippet}

{#snippet folderCell({ row }: ColumnListCellContext<SuggestedOpView>)}
    <span
        class="shortened"
        use:useShortenMiddle={{
            text: parentFolder(row.sourcePath),
            preferBreakAt: '/',
            startRatio: 0.3,
            tooltipWhenTruncated: true,
        }}
    ></span>
{/snippet}

{#snippet sizeCell({ row }: ColumnListCellContext<SuggestedOpView>)}
    {@const facts = factsOf(row)}
    <span class="fact">
        {#if facts.kind === 'live'}
            {#if facts.sizeChanged}
                <span class="changed-glyph">
                    <StatusGlyph
                        name="triangle-alert"
                        label={tString('suggestedOps.factChanged')}
                        tooltip={tString('suggestedOps.factChangedTooltip', {
                            then: sizeText(facts.snapshotSize),
                            now: sizeText(facts.size),
                        })}
                    />
                </span>
            {/if}
            {#if facts.size !== null}
                <span class="size"
                    >{#each formatSizeForDisplay(facts.size, sizeOptions) as part, i (i)}<span class={part.tierClass}
                            >{part.value}</span
                        >{/each}</span
                >
            {/if}
        {:else if facts.kind === 'missing'}
            <span class="missing">{tString('suggestedOps.fileMissing')}</span>
        {:else}
            <StatusGlyph
                name="history"
                label={tString('suggestedOps.snapshotOnly')}
                tooltip={tString('suggestedOps.snapshotOnlyTooltip')}
            />
            <span class="snapshot">{sizeText(facts.size)}</span>
        {/if}
    </span>
{/snippet}

{#snippet modifiedCell({ row }: ColumnListCellContext<SuggestedOpView>)}
    {@const facts = factsOf(row)}
    <span class="fact">
        {#if facts.kind === 'live'}
            {#if facts.modifiedChanged}
                <span class="changed-glyph">
                    <StatusGlyph
                        name="triangle-alert"
                        label={tString('suggestedOps.factChanged')}
                        tooltip={tString('suggestedOps.factChangedTooltip', {
                            then: dateText(facts.snapshotModified),
                            now: dateText(facts.modified),
                        })}
                    />
                </span>
            {/if}
            <DateLabel modifiedAt={facts.modified} />
        {:else if facts.kind === 'snapshot'}
            <StatusGlyph
                name="history"
                label={tString('suggestedOps.snapshotOnly')}
                tooltip={tString('suggestedOps.snapshotOnlyTooltip')}
            />
            <span class="snapshot">{dateText(facts.modified)}</span>
        {/if}
    </span>
{/snippet}

{#if review?.windowError}
    <!-- The list only asks for rows again when its range moves, so a read that didn't come
         back needs its own way to ask. -->
    <div class="files-problem" role="alert">
        <span>{tString('suggestedOps.filesLoadFailed')}</span>
        <Button size="mini" onclick={() => void ensureOpWindow(group.groupId, review.range.start, review.range.end)}>
            {tString('suggestedOps.retryFiles')}
        </Button>
    </div>
{/if}

{#if review?.sourceUnreachable}
    <p class="source-note" role="status">
        {tString(
            review.sourceUnreachable === 'notConnected' ? 'suggestedOps.sourceNotConnected' : 'suggestedOps.sourceGone',
        )}
    </p>
{/if}

<div class="files" style:height={frameHeight}>
    <ColumnList
        {...columnListProps({
            columns,
            rows: source,
            semantics: 'table',
            ariaLabel: tString('suggestedOps.filesLabel', { name: group.displayName }),
            rowClass: 'op-row',
        })}
    />
</div>

<div class="list-footer">
    <div class="more">
        {#if remaining > 0}
        <LinkButton onclick={() => { showMoreRows(group.groupId) }}>
            {tString('suggestedOps.showMore', {
                countText: formatInteger(Math.min(SHOW_MORE_STEP, remaining)),
                count: Math.min(SHOW_MORE_STEP, remaining),
            })}
        </LinkButton>
        {#if remaining > SHOW_MORE_STEP}
            <LinkButton onclick={() => { showAllRows(group.groupId) }}>
                {tString('suggestedOps.showAll', { countText: formatInteger(total), count: total })}
            </LinkButton>
        {/if}
        {/if}
    </div>
    {@render actions()}
</div>

<style>
    /* Its height is `frameHeight`, inline: it depends on how many rows show. */
    .files {
        display: flex;
        flex-direction: column;
        overflow: hidden;
        border: 1px solid var(--color-border-subtle);
        border-radius: var(--radius-md);
    }

    .source-note {
        margin: 0 0 var(--spacing-xs);
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
    }

    .list-footer {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: var(--spacing-md);
        margin-top: var(--spacing-sm);
    }

    .more {
        display: flex;
        gap: var(--spacing-md);
        font-size: var(--font-size-sm);
    }

    .files-problem {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
        margin-bottom: var(--spacing-xs);
        color: var(--color-error-text);
        font-size: var(--font-size-sm);
    }

    .include,
    .icon-box {
        display: flex;
        align-items: center;
    }

    .icon-img {
        width: 16px;
        height: 16px;
        object-fit: contain;
    }

    .icon-fallback {
        display: inline-flex;
        color: var(--color-text-secondary);
    }

    /* A block, so `useShortenMiddle` reads the track's width rather than its own text's. */
    .shortened {
        display: block;
        overflow: hidden;
        white-space: nowrap;
    }

    /* The old and new names split the column, each shortening in the middle on its own. */
    .name-cell {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
        min-width: 0;
    }

    .name-cell .shortened {
        flex: 1 1 0;
        min-width: 0;
    }

    .arrow {
        display: inline-flex;
        flex-shrink: 0;
        color: var(--color-text-tertiary);
    }

    .new-name {
        font-weight: 400;
    }

    .fact {
        display: inline-flex;
        align-items: center;
        justify-content: flex-end;
        gap: var(--spacing-xxs);
        width: 100%;
        white-space: nowrap;
    }

    /* The glyph takes the warning color: a value that moved is the thing to look at. */
    .changed-glyph {
        display: inline-flex;
        color: var(--color-warning-text);
    }

    .missing {
        color: var(--color-warning-text);
    }

    .snapshot {
        color: var(--color-text-tertiary);
    }
</style>
