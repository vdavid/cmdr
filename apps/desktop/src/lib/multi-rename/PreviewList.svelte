<script lang="ts">
    /**
     * The Multi-Rename sheet's preview: the house `ColumnList` over the state's windowed
     * source, one row per file (its icon, old name → new name, and a status glyph on a
     * problem). A name the user typed in Results carries a pencil where the arrow is. The
     * rows come a page at a time, and it draws only the ones in view.
     *
     * A `grid`: a Tab stop with one cursor, which ↑/↓, PageUp/PageDown, Home/End, and a click
     * move (hover doesn't: the cursor drives the range examples, which mustn't jump while the
     * pointer passes over). The state owns the index; a move it makes is scrolled into view.
     */
    import StatusGlyph from '#lib/ui/StatusGlyph.svelte'
    import ColumnList from '#lib/ui/ColumnList.svelte'
    import Icon from '#lib/ui/Icon.svelte'
    import {
        columnListProps,
        type ColumnListApi,
        type ColumnListCellContext,
        type ColumnListColumn,
        type ColumnListWindowedSource,
    } from '#lib/ui/column-list-types.js'
    import { getCachedIcon, iconCacheVersion } from '#lib/icon-cache.js'
    import { useShortenMiddle } from '#lib/utils/shorten-middle-action.js'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { tooltip } from '#lib/tooltip/tooltip.js'
    import type { PreviewRow } from '#lib/tauri-commands/index.js'
    import { rowStatusView, type StatusMessage } from './row-status'

    interface Props {
        rows: ColumnListWindowedSource<PreviewRow>
        /** The rows are the last good preview under a spec error: dimmed, as the house dims what's unavailable. */
        stale?: boolean
        /** The cursor row, or -1. */
        cursorIndex: number
        onCursorChange: (index: number) => void
    }

    const { rows, stale = false, cursorIndex, onCursorChange }: Props = $props()

    let list = $state<ColumnListApi>()

    // A cursor the state moved (back to every row, a moved row) comes into view; the list's
    // own keys scroll it already.
    $effect(() => {
        const index = cursorIndex
        if (index >= 0) list?.scrollIndexIntoView(index)
    })

    /** The icon track, the same as Search's results (`query-ui/result-column-widths.ts`). */
    const ICON_TRACK_PX = 24
    /** The arrow and status glyph tracks: one small glyph each. */
    const GLYPH_TRACK_PX = 16
    /** Floor on each name track: a narrow sheet still shows a few characters of both. */
    const NAME_MIN_PX = 80

    // Read so a late icon re-renders its rows.
    const iconVersion = $derived($iconCacheVersion)

    // Every track is fixed or shared: a windowed source can't be measured, so the two names
    // split what the glyphs leave.
    const columns = $derived<ColumnListColumn<PreviewRow>[]>([
        { id: 'icon', label: '', width: { kind: 'fixed', px: ICON_TRACK_PX }, clip: false, cell: iconCell },
        {
            id: 'old-name',
            label: tString('multiRename.oldName'),
            width: { kind: 'share', minPx: NAME_MIN_PX },
            class: 'preview-old-name',
            cell: oldNameCell,
        },
        { id: 'arrow', label: '', width: { kind: 'fixed', px: GLYPH_TRACK_PX }, tone: 'tertiary', cell: arrowCell },
        {
            id: 'new-name',
            label: tString('multiRename.newName'),
            width: { kind: 'share', minPx: NAME_MIN_PX },
            emphasis: true,
            class: 'preview-new-name',
            cell: newNameCell,
        },
        {
            id: 'status',
            label: tString('multiRename.statusColumn'),
            labelHidden: true,
            width: { kind: 'fixed', px: GLYPH_TRACK_PX },
            clip: false,
            class: 'preview-status',
            cell: statusCell,
        },
    ])

    function words(message: StatusMessage): string {
        return tString(message.key, message.params)
    }

    function iconUrl(iconId: string | null): string | undefined {
        // eslint-disable-next-line @typescript-eslint/no-unused-expressions -- reactive read: a late icon re-renders the row.
        iconVersion
        return iconId === null ? undefined : getCachedIcon(iconId)
    }
</script>

<div class="preview" class:is-unavailable={stale}>
    <ColumnList
        bind:this={list}
        {...columnListProps({
            columns,
            rows,
            semantics: 'grid',
            ariaLabel: tString('multiRename.preview'),
            cursorIndex,
            onCursorChange,
            headerClass: 'preview-header',
            rowClass: 'preview-row',
        })}
    />
</div>

{#snippet iconCell({ row }: ColumnListCellContext<PreviewRow>)}
    <span class="icon-box">
        {#if iconUrl(row.iconId)}
            <img class="icon-img" src={iconUrl(row.iconId)} alt="" width="16" height="16" />
        {:else}
            <Icon name={row.isDirectory ? 'folder' : 'file'} size={16} aria-hidden="true" />
        {/if}
    </span>
{/snippet}

<!-- Mid-truncating names, as in Search's results: the extension stays in view, and the full
     name is on hover when it was cut. -->
{#snippet oldNameCell({ row }: ColumnListCellContext<PreviewRow>)}
    <span
        class="name-text"
        use:useShortenMiddle={{ text: row.oldName, preferBreakAt: '.', startRatio: 0.7, tooltipWhenTruncated: true }}
    ></span>
{/snippet}

{#snippet arrowCell({ row }: ColumnListCellContext<PreviewRow>)}
    {#if row.edited}
        <span class="arrow edited-mark" use:tooltip={{ text: tString('multiRename.editedName') }}>
            <Icon name="pencil" size={12} aria-hidden="true" />
            <span class="sr-only">{tString('multiRename.editedName')}</span>
        </span>
    {:else}
        <span class="arrow" aria-hidden="true"><Icon name="arrow-right" size={12} /></span>
    {/if}
{/snippet}

{#snippet newNameCell({ row }: ColumnListCellContext<PreviewRow>)}
    <span
        class="name-text"
        class:unchanged={row.status.type === 'unchanged'}
        use:useShortenMiddle={{ text: row.newName, preferBreakAt: '.', startRatio: 0.7, tooltipWhenTruncated: true }}
    ></span>
{/snippet}

<!-- A problem shows its glyph, named by its label and explained by its tooltip; a ready or
     unchanged row stays quiet and says what it is to screen readers alone. -->
{#snippet statusCell({ row }: ColumnListCellContext<PreviewRow>)}
    {@const view = rowStatusView(row.status)}
    {#if view.glyph}
        <span class="problem-glyph">
            <StatusGlyph
                name={view.glyph}
                label={words(view.label)}
                tooltip={view.reason ? words(view.reason) : undefined}
            />
        </span>
    {:else}
        <span class="sr-only">{words(view.label)}</span>
    {/if}
{/snippet}

<style>
    /* The well around the list: one element owns the border and the rounded corners, and
       hands the list the sheet's spare height. */
    .preview {
        display: flex;
        flex-direction: column;
        flex: 1 1 auto;
        min-height: 0;
        overflow: hidden;
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
    }

    /* A stale preview reads as unavailable: the house disabled opacity, faded in unless the
       user asked for less motion. `is-unavailable` names it as inactive content (it can't be
       applied), which is also the shape the contrast checker exempts. */
    .is-unavailable {
        opacity: 0.5;
    }

    @media (prefers-reduced-motion: no-preference) {
        .preview {
            transition: opacity var(--transition-base);
        }
    }

    /* Cell contents. The cells themselves (font, tone, the track widths) are `ColumnList`'s. */
    .icon-box {
        display: flex;
        align-items: center;
        justify-content: center;
        width: 16px;
        color: var(--color-text-secondary);
    }

    .icon-img {
        width: 16px;
        height: 16px;
        object-fit: contain;
    }

    /* A block, so `useShortenMiddle` reads the track's width rather than its own text's. */
    .name-text {
        display: block;
        overflow: hidden;
        white-space: nowrap;
    }

    .name-text.unchanged {
        color: var(--color-text-quiet);
        font-weight: normal;
    }

    .arrow {
        display: flex;
        align-items: center;
    }

    /* The typed name's mark stands out from the quiet arrows it replaces, but stays a hint. */
    .edited-mark {
        color: var(--color-accent-text);
    }

    .problem-glyph {
        display: flex;
        align-items: center;
        color: var(--color-error-text);
    }
</style>
