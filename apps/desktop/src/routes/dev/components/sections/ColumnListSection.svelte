<script lang="ts">
    import { SvelteMap, SvelteSet } from 'svelte/reactivity'
    import SectionCard from '#lib/ui/SectionCard.svelte'
    import Button from '#lib/ui/Button.svelte'
    import Checkbox from '#lib/ui/Checkbox.svelte'
    import ColumnList from '#lib/ui/ColumnList.svelte'
    import Size from '#lib/ui/Size.svelte'
    import { claimKey } from '#lib/shortcuts/claim-key.js'
    import { nextTreeCursor, treeCursorKeyOf } from '#lib/ui/tree-cursor.js'
    import {
        columnListProps,
        type ColumnListApi,
        type ColumnListColumn,
        type ColumnListRowAria,
        type ColumnListWindowedSource,
    } from '#lib/ui/column-list-types.js'

    /**
     * Three lists: a short measured listbox (hover moves the cursor; click the list, then the
     * cursor keys move it too, the way a dialog owns its keys), a 100,000-row windowed grid
     * that "pages" rows in after a delay, with a group heading every 50 rows: a Tab stop whose
     * cursor its own keys and clicks move, and hover doesn't; and a treegrid of up to
     * 2,000,020 rows with a checkbox per row, for the tree ARIA, ← / →, and the scaled scroll.
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
        const next = list?.cursorTarget(event, cursor, { wrap: true })
        if (next === undefined || next === null) return
        claimKey(event)
        if (next < 0) return
        cursor = next
        list?.scrollIndexIntoView(cursor)
    }
    let list: ColumnListApi | undefined = $state()

    interface RenameRow {
        heading: boolean
        oldName: string
        newName: string
    }

    let renameCursor = $state(1)
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

    /**
     * The treegrid: 20 folders of 100,000 files each, so opening them all makes 2,000,020
     * rows, past WebKit's layout-height cap (the scaled scroll). The flattening is index math
     * over which folders are open, standing in for a backend that answers windows; the real
     * consumers keep their tree in Rust.
     */
    interface TreeRow {
        key: string
        name: string
        folder: number
        /** The file's index in its folder, or -1 for the folder row. */
        file: number
        level: number
        expanded?: boolean
        note: string
    }

    const FOLDERS = 20
    const FILES_PER_FOLDER = 100_000
    const openFolders = new SvelteSet<number>([0])
    const picked = new SvelteSet<string>()
    let treeCursor = $state(0)
    let treeList: ColumnListApi | undefined = $state()
    const treeId = $props.id()

    /** Row `index`'s folder and its place in it, by walking the 20 folders. */
    function treeRowAt(index: number): TreeRow | undefined {
        let at = index
        for (let folder = 0; folder < FOLDERS; folder++) {
            const open = openFolders.has(folder)
            if (at === 0) {
                return { key: `f${String(folder)}`, name: `Folder ${String(folder + 1)}`, folder, file: -1, level: 1, expanded: open, note: '' }
            }
            at -= 1
            const size = open ? FILES_PER_FOLDER : 0
            if (at < size) {
                const note = at % 7 === 3 ? 'Skipped: it changed since the copy' : ''
                return { key: `f${String(folder)}/${String(at)}`, name: `photo-${String(at)}.jpg`, folder, file: at, level: 2, note }
            }
            at -= size
        }
        return undefined
    }

    /** Where folder `folder`'s own row sits right now. */
    function folderIndex(folder: number): number {
        let index = 0
        for (let f = 0; f < folder; f++) index += 1 + (openFolders.has(f) ? FILES_PER_FOLDER : 0)
        return index
    }

    const treeRows: ColumnListWindowedSource<TreeRow> = {
        get count() {
            return FOLDERS + openFolders.size * FILES_PER_FOLDER
        },
        getRow: treeRowAt,
    }

    function treeRowAria(row: TreeRow): ColumnListRowAria {
        return {
            level: row.level,
            expanded: row.expanded,
            setSize: row.level === 1 ? FOLDERS : FILES_PER_FOLDER,
            posInSet: (row.level === 1 ? row.folder : row.file) + 1,
            describedBy: row.note ? `${treeId}-note-${row.key}` : undefined,
        }
    }

    function toggleFolder(folder: number): void {
        if (openFolders.has(folder)) openFolders.delete(folder)
        else openFolders.add(folder)
    }

    function togglePicked(key: string): void {
        if (picked.has(key)) picked.delete(key)
        else picked.add(key)
    }

    /** ← and → through `tree-cursor.ts`; the grid itself already answered ↑ ↓ and the rest. */
    function onTreeKeydown(event: KeyboardEvent): void {
        const key = treeCursorKeyOf(event)
        if (key === null) return
        const row = treeRowAt(treeCursor)
        const action = nextTreeCursor(key, {
            from: treeCursor,
            row,
            count: treeRows.count,
            parentIndex: row && row.level > 1 ? folderIndex(row.folder) : null,
        })
        claimKey(event)
        // Nothing sits before this demo tree, so ← on a top-level row has nowhere to go.
        if (!action || !row || action.kind === 'leave') return
        if (action.kind === 'move') {
            treeCursor = action.index
            treeList?.scrollIndexIntoView(action.index)
        } else {
            toggleFolder(row.folder)
        }
    }

    const treeColumns: ColumnListColumn<TreeRow>[] = [
        { id: 'name', label: 'Name', width: { kind: 'share', minPx: 120 }, emphasis: true, clip: false, cell: treeNameCell },
        { id: 'note', label: 'Status', width: { kind: 'share', minPx: 80 }, tone: 'secondary', cell: treeNoteCell },
    ]
</script>

{#snippet nameCell({ row }: { row: FileRow })}{row.name}{/snippet}
{#snippet kindCell({ row }: { row: FileRow })}{row.kind}{/snippet}
{#snippet sizeCell({ row }: { row: FileRow })}<Size bytes={row.size} />{/snippet}
{#snippet oldCell({ row }: { row: RenameRow })}{row.oldName}{/snippet}
{#snippet newCell({ row }: { row: RenameRow })}{row.newName}{/snippet}
{#snippet folderHeading({ row }: { row: RenameRow })}{row.oldName}{/snippet}
{#snippet treeNameCell({ row }: { row: TreeRow })}
    <span class="tree-name" style:padding-left="calc({row.level - 1} * var(--spacing-lg))">
        {#if row.expanded === undefined}
            <span class="disclosure"></span>
        {:else}
            <!-- A pointer target only: the row (the grid's active descendant) takes → and ←. -->
            <span
                class="disclosure is-folder"
                class:is-open={row.expanded}
                aria-hidden="true"
                onclick={(event) => {
                    event.stopPropagation()
                    toggleFolder(row.folder)
                }}
            ></span>
        {/if}
        <!-- Out of the Tab order: the grid keeps focus, and a press still checks it. -->
        <Checkbox
            checked={picked.has(row.key)}
            tabindex={-1}
            ariaLabel="Include {row.name}"
            onCheckedChange={() => {
                togglePicked(row.key)
            }}
        />
        <span class="tree-label">{row.name}</span>
    </span>
{/snippet}
{#snippet treeNoteCell({ row }: { row: TreeRow })}
    {#if row.note}<span id="{treeId}-note-{row.key}">{row.note}</span>{/if}
{/snippet}

<SectionCard id="components-column-list" label="Column list">
    <p class="hint">
        Measured listbox: Name and Kind share the width by max-min fairness, Size is fixed. Hover moves the cursor;
        click the list and the cursor keys move it too.
    </p>
    <!-- The demo owns the keys the way a dialog would; the list itself takes no focus. -->
    <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
    <div class="frame short" tabindex="0" role="group" aria-label="Measured list demo" onkeydown={onListKeydown}>
        <ColumnList
            bind:this={list}
            {...columnListProps({
                columns: fileColumns,
                rows: files,
                rowKey: (row) => row.name,
                ariaLabel: 'Files',
                cursorIndex: cursor,
                onCursorChange: (index) => (cursor = index),
                hoverMovesCursor: true,
            })}
        />
    </div>
    <p class="hint">
        Windowed grid: 100,000 rows paged in after 300 ms, a folder heading every 50 rows. Tab to it: the cursor keys
        and clicks move its cursor, hover doesn't.
    </p>
    <div class="frame tall">
        <ColumnList
            {...columnListProps({
                columns: renameColumns,
                rows: renames,
                semantics: 'grid',
                ariaLabel: 'Rename preview',
                cursorIndex: renameCursor,
                onCursorChange: (index) => (renameCursor = index),
                isGroupHeading: (row) => row.heading,
                groupHeading: folderHeading,
            })}
        />
    </div>
    <p class="hint">
        Treegrid: 20 folders of 100,000 files; open them all for 2,000,020 rows, past the browser's height cap (End
        still reaches the last row). → opens or steps in, ← shuts or steps out, and a checkbox press keeps focus on the
        grid. Every seventh file is described by its status.
    </p>
    <div class="frame tall">
        <ColumnList
            bind:this={treeList}
            {...columnListProps({
                columns: treeColumns,
                rows: treeRows,
                semantics: 'treegrid',
                rowAria: treeRowAria,
                ariaLabel: 'Copied items',
                cursorIndex: treeCursor,
                onCursorChange: (index) => (treeCursor = index),
                onKeydown: onTreeKeydown,
            })}
        />
    </div>
    <Button
        variant="secondary"
        onclick={() => {
            for (let folder = 0; folder < FOLDERS; folder++) openFolders.add(folder)
        }}>Open every folder</Button
    >
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

    .tree-name {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
        min-width: 0;
    }

    /* A CSS triangle: the Components catalog keeps `Icon` to the Graphics one. */
    .disclosure {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        flex-shrink: 0;
        width: 12px;
        height: 12px;
        color: var(--color-text-tertiary);
    }

    .disclosure.is-folder::before {
        content: '';
        border-left: 5px solid currentColor;
        border-block: 4px solid transparent;
        transition: transform var(--transition-base);
    }

    .disclosure.is-open::before {
        transform: rotate(90deg);
    }

    @media (prefers-reduced-motion: reduce) {
        .disclosure.is-folder::before {
            transition: none;
        }
    }

    .tree-label {
        min-width: 0;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }
</style>
