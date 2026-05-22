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
     * regular history entry for the underlying real path). Click on a path-pill segment
     * navigates the active pane to that ancestor and leaves the snapshot view; the host
     * pane's `onNavigateToAncestor` callback owns the volume / path switch.
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
        /** Called when the user clicks a path-pill ancestor (leaves the snapshot view). */
        onNavigateToAncestor: (ancestorPath: string) => void
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
        onNavigateToAncestor,
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
     * shape FullList expects). We carry `parentPath` through so the path-pills column
     * has data, and we synthesize the metadata fields FullList reads (permissions /
     * owner / group / extendedMetadataLoaded) with safe defaults. Recursive fields stay
     * absent: snapshots only carry per-file basics from the search engine. The Size
     * column will render `<dir>` for directory results because `recursiveSize` is
     * absent, which matches the user's expectation for a result set (we didn't recurse
     * into them).
     */
    function adaptEntry(e: SearchResultEntry): FileEntry {
        return {
            name: e.name,
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

    /** Find an entry by name; returns its global index or -1. */
    export function findItemIndex(name: string): number {
        return entries.findIndex((e) => e.name === name)
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
        showPathColumn={true}
        staticEntries={entries}
        onPathPillPick={onNavigateToAncestor}
        onContextMenu={(entry: FileEntry) => {
            // Route through the standard native context menu but ask Rust to
            // suppress Rename and New folder for this virtual pane: the
            // underlying paths are real, so Open / Copy / Move / Delete /
            // Show in Finder all still make sense, but the snapshot view
            // isn't a destination for "rename inside this folder" or
            // "make a new folder here". `caps` is the capability flag set
            // (`searchResultsVolumeCapabilities`) that gates this. The flag
            // is intentionally read at call time — referencing it here keeps
            // the wiring discoverable if the per-pane capabilities ever
            // grow a runtime branch (right now the flags are static for
            // search-results panes).
            const restrict = !caps.canRename
            void showFileContextMenu(entry.path, entry.name, entry.isDirectory, [entry.path], restrict)
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
