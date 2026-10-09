<script lang="ts">
    import { SvelteMap, SvelteSet } from 'svelte/reactivity'
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import ColumnList from '$lib/ui/ColumnList.svelte'
    import Size from '$lib/ui/Size.svelte'
    import type { ColumnListColumn, ColumnListWindowedSource } from '$lib/ui/column-list-types'

    /**
     * Two lists: a short measured listbox (hover moves the cursor; click the list, then the
     * arrow keys move it too, the way a dialog owns its keys), and a 100,000-row windowed
     * table that "pages" rows in after a delay, with a group heading every 50 rows.
     */
    interface FileRow {
        name: string
        kind: string
        size: number
    }

    const files: FileRow[] = [
        { name: 'quarterly-report-2026-final-really-final.pdf', kind: 'PDF', size: 1_843_200 },
        { name: 'IMG_4021.HEIC', kind: 'Image', size: 3_406_118 },
        { name: 'notes.md', kind: 'Text', size: 2_048 },
        { name: 'holiday-video-from-the-lake.mov', kind: 'Movie', size: 734_003_200 },
        { name: 'budget.numbers', kind: 'Spreadsheet', size: 412_160 },
        { name: 'a', kind: 'File', size: 1 },
    ]
    let cursor = $state(0)
    const fileColumns: ColumnListColumn<FileRow>[] = [
        {
            id: 'name',
            label: 'Name',
            width: { kind: 'share', minPx: 80 },
            demand: ({ row, measure }) => measure.text(row.name),
            emphasis: true,
            cell: nameCell,
        },
        {
            id: 'kind',
            label: 'Kind',
            width: { kind: 'share', minPx: 60 },
            demand: ({ row, measure }) => measure.text(row.kind),
            tone: 'tertiary',
            cell: kindCell,
        },
        { id: 'size', label: 'Size', width: { kind: 'fixed', ch: 10 }, align: 'end', tone: 'secondary', cell: sizeCell },
    ]

    function onListKeydown(event: KeyboardEvent): void {
        if (event.key !== 'ArrowDown' && event.key !== 'ArrowUp') return
        event.preventDefault()
        const step = event.key === 'ArrowDown' ? 1 : -1
        cursor = (cursor + step + files.length) % files.length
        list?.scrollIndexIntoView(cursor)
    }
    let list: ReturnType<typeof ColumnList<FileRow>> | undefined = $state()

    interface RenameRow {
        heading: boolean
        oldName: string
        newName: string
    }

    const TOTAL = 100_000
    const PAGE = 200
    const loaded = new SvelteMap<number, RenameRow>()
    const asked = new SvelteSet<number>()

    function makeRow(index: number): RenameRow {
        if (index % 50 === 0) return { heading: true, oldName: `~/Photos/${String(index / 50)}`, newName: '' }
        return { heading: false, oldName: `IMG_${String(index)}.jpg`, newName: `Holiday ${String(index)}.jpg` }
    }

    const renames: ColumnListWindowedSource<RenameRow> = {
        count: TOTAL,
        getRow: (index) => loaded.get(index),
        onRangeChange: ({ start, end }) => {
            for (let page = Math.floor(start / PAGE); page * PAGE < end; page++) {
                if (asked.has(page)) continue
                asked.add(page)
                setTimeout(() => {
                    for (let i = page * PAGE; i < Math.min(TOTAL, (page + 1) * PAGE); i++) loaded.set(i, makeRow(i))
                }, 300)
            }
        },
    }
    const renameColumns: ColumnListColumn<RenameRow>[] = [
        { id: 'old', label: 'Old name', width: { kind: 'share', minPx: 80 }, cell: oldCell },
        { id: 'new', label: 'New name', width: { kind: 'share', minPx: 80 }, emphasis: true, cell: newCell },
    ]
</script>

{#snippet nameCell({ row }: { row: FileRow })}{row.name}{/snippet}
{#snippet kindCell({ row }: { row: FileRow })}{row.kind}{/snippet}
{#snippet sizeCell({ row }: { row: FileRow })}<Size bytes={row.size} />{/snippet}
{#snippet oldCell({ row }: { row: RenameRow })}{row.oldName}{/snippet}
{#snippet newCell({ row }: { row: RenameRow })}{row.newName}{/snippet}
{#snippet folderHeading({ row }: { row: RenameRow })}{row.oldName}{/snippet}

<SectionCard id="components-column-list" label="Column list">
    <p class="hint">
        Measured listbox: Name and Kind share the width by max-min fairness, Size is fixed. Hover moves the cursor;
        click the list and the arrow keys move it too.
    </p>
    <!-- The demo owns the keys the way a dialog would; the list itself takes no focus. -->
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <div class="frame short" tabindex="0" role="group" aria-label="Measured list demo" onkeydown={onListKeydown}>
        <ColumnList
            bind:this={list}
            columns={fileColumns}
            rows={files}
            rowKey={(row) => row.name}
            ariaLabel="Files"
            cursorIndex={cursor}
            onHover={(index) => (cursor = index)}
        />
    </div>
    <p class="hint">Windowed table: 100,000 rows paged in after 300 ms, a folder heading every 50 rows.</p>
    <div class="frame tall">
        <ColumnList
            columns={renameColumns}
            rows={renames}
            semantics="table"
            ariaLabel="Rename preview"
            isGroupHeading={(row) => row.heading}
            groupHeading={folderHeading}
        />
    </div>
</SectionCard>

<style>
    .hint {
        margin: 0 0 var(--spacing-md);
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
    }

    .frame {
        display: flex;
        flex-direction: column;
        margin-bottom: var(--spacing-lg);
        border: 1px solid var(--color-border-subtle);
        border-radius: var(--radius-md);
        overflow: hidden;
        resize: horizontal;
    }

    .frame.short {
        height: 200px;
    }

    .frame.tall {
        height: 320px;
    }
</style>
