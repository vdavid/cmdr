<script lang="ts">
    /**
     * SearchResultsView renders a search-results snapshot inside a regular `FilePane`,
     * sibling to `NetworkMountView` for the network virtual volume.
     *
     * The pane's `path` is `search-results://<snapshotId>`; we strip the prefix and read
     * the snapshot from `lib/search/snapshot-store.svelte.ts`. The snapshot's entries
     * are adapted into `FileEntry` shape and handed to `FullList` via its
     * `staticEntries` + `showPathColumn` props, so the same virtual-scroll / column-width
     * pipeline as a normal listing applies.
     *
     * Click on a row opens the file or navigates into the directory via the host pane's
     * `onNavigate` callback (which routes through `FilePane.handleNavigate` and pushes a
     * regular history entry for the underlying real path). The snapshot pane has no
     * separate Path column; the Name column shows the full path, and the row-level
     * "Reveal in Finder" / "Open" context menu items cover the ancestor-jump cases.
     * The breadcrumb still shows the snapshot's label.
     *
     * Missing snapshot — shouldn't happen in practice (the dialog pins via the
     * last-attempt slot and the tab-state manager holds refs for history entries), but
     * defensively we render a small "snapshot not available" pane rather than throwing.
     */

    import type { FileEntry, SortColumn, SortOrder } from '../types'
    import FullList from '../views/FullList.svelte'
    import { getSnapshot, getMutationTick, type SearchSnapshot } from '$lib/search/snapshot-store.svelte'
    import { searchResultsVolumeCapabilities } from '$lib/search/capabilities'
    import { showFileContextMenu } from '$lib/tauri-commands'
    import type { SearchResultEntry } from '$lib/ipc/bindings'
    import type { ListViewAPI } from './types'

    interface Props {
        /** Full pane path, expected shape `search-results://<id>`. */
        path: string
        cursorIndex: number
        isFocused?: boolean
        sortBy: SortColumn
        sortOrder: SortOrder
        /**
         * Selected indices within the snapshot's entries. The snapshot pane shares
         * `FilePane.selection` state with normal panes; indices are 0-based (no `..`
         * row). M8d: drives source-side copy/move/cut behaviour.
         */
        selectedIndices?: Set<number>
        /** Called when the user activates a row (Enter / double-click). */
        onNavigate: (entry: FileEntry) => void
        /**
         * Called when the user clicks / shift-clicks / cmd-clicks a row. Mirrors
         * `FullList`'s signature so the host pane can route to selection state.
         */
        onSelect: (index: number, shiftKey?: boolean, metaKey?: boolean) => void
        /** Called by FullList when the visible window changes (passed through). */
        onVisibleRangeChange?: (start: number, end: number) => void
    }

    const {
        path,
        cursorIndex,
        isFocused = false,
        sortBy,
        sortOrder,
        selectedIndices = new Set<number>(),
        onNavigate,
        onSelect,
        onVisibleRangeChange,
    }: Props = $props()

    /** Pull the snapshot id out of `search-results://<id>`. Returns `null` for any other shape. */
    const SEARCH_RESULTS_PREFIX = 'search-results://'
    const snapshotId = $derived(path.startsWith(SEARCH_RESULTS_PREFIX) ? path.slice(SEARCH_RESULTS_PREFIX.length) : null)

    /**
     * Live snapshot lookup. Re-derives if the id changes (which happens on pane
     * history navigation) AND whenever the snapshot store's mutation tick bumps
     * (cross-snapshot delete sync, see `snapshot-store::removeEntryFromAllSnapshots`).
     * Reading `getMutationTick()` registers a Svelte dependency on the underlying
     * `$state` so the entries column re-derives after a delete.
     */
    const snapshot = $derived<SearchSnapshot | undefined>(
        snapshotId ? (void getMutationTick(), getSnapshot(snapshotId)) : undefined,
    )

    /** Capability flags driving the row context menu (M8c). */
    const caps = searchResultsVolumeCapabilities()

    /**
     * Adapt `SearchResultEntry` (the wire-typed search result) into `FileEntry` (the
     * shape FullList expects). The Name column shows the FULL PATH for each entry
     * (`~/Library/Dropbox/test.md`), so we synthesize the FileEntry with
     * `name = friendly full path`. FullList's `col-name` mid-
     * truncates via `useShortenMiddle` (snapping to `/` when the name contains one)
     * and the tooltip surfaces the full string on hover. Recursive fields stay
     * absent: snapshots only carry per-file basics from the search engine. The Size
     * column renders `<dir>` for directory results because `recursiveSize` is
     * absent, which matches the user's expectation for a result set (we didn't
     * recurse into them).
     *
     * The displayed name passes through `prettyPath` to replace the user's home
     * prefix with `~` for compactness. The underlying `path` field (used for ops:
     * Open, Reveal, Copy path, Move, Delete) stays absolute.
     */
    function adaptEntry(e: SearchResultEntry): FileEntry {
        return {
            name: prettyPath(e.path),
            path: e.path,
            isDirectory: e.isDirectory,
            isSymlink: false,
            size: e.size ?? undefined,
            modifiedAt: e.modifiedAt ?? undefined,
            permissions: 0o644,
            owner: '',
            group: '',
            iconId: e.iconId,
            extendedMetadataLoaded: true,
            parentPath: e.parentPath,
        }
    }

    /**
     * Replaces the user's home prefix with `~` for compactness. Best-effort
     * heuristic: the first `/Users/<name>/` (macOS) or `/home/<name>/` (Linux)
     * segment becomes `~/`. Anything that doesn't match passes through unchanged.
     */
    function prettyPath(absolute: string): string {
        const macMatch = /^\/Users\/[^/]+/.exec(absolute)
        if (macMatch) return '~' + absolute.slice(macMatch[0].length)
        const linuxMatch = /^\/home\/[^/]+/.exec(absolute)
        if (linuxMatch) return '~' + absolute.slice(linuxMatch[0].length)
        return absolute
    }

    /**
     * Adapted FileEntry array for FullList. Derived from `snapshot.entries`; changes
     * only when the snapshot id changes (the snapshot itself is immutable once stored).
     */
    const entries = $derived<FileEntry[]>(snapshot ? snapshot.entries.map(adaptEntry) : [])

    let fullListRef: ListViewAPI | undefined = $state()

    // ── Exported API mirrors NetworkMountView so FilePane's branches call uniformly. ──

    /** Move cursor to a specific index in the pane's result list. */
    export function setCursorIndex(index: number) {
        fullListRef?.scrollToIndex(index)
    }

    /**
     * Find an entry by basename; returns its global index or -1. The entry's
     * `name` field is a full path (snapshot-pane display convention), so we
     * compare against the basename derived from `path` instead. Callers (mostly
     * type-to-jump and MCP) still pass plain filenames.
     */
    export function findItemIndex(name: string): number {
        return entries.findIndex((e) => basename(e.path) === name)
    }

    function basename(path: string): string {
        const idx = path.lastIndexOf('/')
        return idx >= 0 ? path.slice(idx + 1) : path
    }

    /**
     * Activate the cursor's row, identical to pressing Enter or double-clicking.
     * The cursor index is clamped by the caller (FilePane's keyboard handler) so
     * we can assume `entries[cursorIndex]` is valid when this fires.
     */
    export function openCursorItem(): void {
        const entry = entries[cursorIndex] as FileEntry | undefined
        if (entry) onNavigate(entry)
    }

    /**
     * Returns `true` when this view rendered with a missing snapshot. Used by FilePane
     * to suppress the per-row IPC traffic (selection sync, listing stats) that would
     * otherwise fire on an empty pane.
     */
    export function isMissing(): boolean {
        return snapshot === undefined
    }
</script>

{#if snapshot}
    <FullList
        bind:this={fullListRef}
        listingId=""
        totalCount={entries.length}
        includeHidden={true}
        {cursorIndex}
        {isFocused}
        {selectedIndices}
        hasParent={false}
        parentPath=""
        currentPath={path}
        {sortBy}
        {sortOrder}
        {onSelect}
        {onNavigate}
        {onVisibleRangeChange}
        staticEntries={entries}
        onContextMenu={(entry: FileEntry) => {
            // Route through the standard native context menu but ask Rust to
            // suppress Rename and New folder for this virtual pane: the
            // underlying paths are real, so Open / Copy / Move / Delete /
            // Show in Finder all still make sense, but the snapshot view
            // isn't a destination for "rename inside this folder" or
            // "make a new folder here". `caps` is the capability flag set
            // (`searchResultsVolumeCapabilities`) that gates this.
            //
            // Round 2 P10: the menu's `Copy {filename}` label uses the
            // `filename` arg as-is. Our adapted `entry.name` is the friendly
            // full path (`~/Library/.../test.md`) so the menu would otherwise
            // read `Copy ~/Library/.../test.md`. Hand the Rust side the
            // basename instead so it reads `Copy test.md` and the underlying
            // command-dispatch (which uses `entryUnderCursor.name`, also a
            // basename) copies the same string.
            const restrict = !caps.canRename
            void showFileContextMenu(entry.path, basename(entry.path), entry.isDirectory, [entry.path], restrict)
        }}
    />
{:else}
    <!-- Defensive empty state. Reaching this means the snapshot was evicted out from
         under a pane history entry, which the refcount design rules out by construction.
         Render a friendly message rather than throwing so the user can navigate away. -->
    <div class="snapshot-missing">
        <div class="title">Search results no longer available</div>
        <div class="body">The result set for this search was cleared. Open a new search to start again.</div>
    </div>
{/if}

<style>
    .snapshot-missing {
        display: flex;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        height: 100%;
        gap: var(--spacing-sm);
        padding: var(--spacing-xl);
        color: var(--color-text-secondary);
        text-align: center;
    }

    .snapshot-missing .title {
        font-size: var(--font-size-lg);
        color: var(--color-text-primary);
    }

    .snapshot-missing .body {
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
        max-width: 420px;
    }
</style>
