# What Finder tags still owe

Reading, showing, and assigning macOS Finder tags all shipped: colored dots in both list modes, a seven-circle context
menu group, seven keyboard-assignable toggle commands, an MCP `tag` tool, and a Finder-compatible write path. Every
design decision lives beside the code: `apps/desktop/src-tauri/src/file_system/listing/DETAILS.md` § "Finder tags" (the
parse, the deferred visible-range-first pass with its 15 µs/file anchor, carry-forward, and the write path),
`apps/desktop/src/lib/file-explorer/views/DETAILS.md` (the dot cluster and the column-width settle),
`apps/desktop/src-tauri/src/menu/DETAILS.md` (the circle bitmaps and the composited checkmark), and
`apps/desktop/src-tauri/src/file_system/DETAILS.md` (the MCP consumer and the analytics event).

Two things are open. Neither blocks anything shipped, and both are judgment calls rather than unsolved problems.

❌ Nothing here restates a mechanism. Every item points at the doc or the code that owns it.

## 1. The seven color circles show on volumes that can't hold a tag

**The gap**: `menu_structure.rs::append_tag_color_group` appends the circles for any file or folder on macOS, with no
check on which backend the path lives on. Right-click a file on an MTP device or a directly-attached SMB share and the
menu offers to tag it; the write reaches `xattr::set` on a path that isn't a real filesystem entry, fails, and is logged
under the `tags` target. Nothing breaks, but the menu promised something it can't do.

**Why it isn't obviously wrong**: an OS-mounted SMB share is a real path, and tagging one genuinely works. So the honest
predicate isn't "local volume", it's "this path reaches a filesystem that stores xattrs", which is only knowable by
trying. The read side settled the same question the other way and is documented at `enrich_tags`
(`commands/file_system/listing.rs`): it runs on any volume because an empty read is harmless, and leans on a 2 s timeout
rather than a backend gate.

**Cost**: small either way. Gating on `supports_local_fs_access()` at menu-build time is a few lines; so is leaving it
and letting the failure stay quiet.

**Trigger**: David seeing it and deciding the menu shouldn't offer it. This is a taste call on his own QA pass, not a
defect report.

## 2. A tag assigned from search results doesn't light up until you navigate

**The gap**: `SearchResultsView.svelte` calls `showFileContextMenu` without a `listingId`, so the context-menu tag
toggle writes to disk and then hands an empty id to `apply_tags_to_listing`, which finds no listing and refreshes
nothing. The dots appear on the next navigation into the containing directory. `toggle_tags`'s doc comment in
`commands/file_system/listing.rs` states the contract; the pane path (`pane-pointer.ts`) passes its `listingId` and
refreshes in place.

**Why it wasn't done with the rest**: a search-results pane is not a cached directory listing, so there is no
`listing_id` to pass. Making the write visible there means either giving search results their own cache identity or
giving the tag refresh a second path that patches the results view directly.

**Cost**: the larger of the two, and the size depends on which of those two shapes wins. The second is smaller and the
first is the one that would also serve sort-by-tag and filter-by-tag if those ever land.

**Trigger**: someone tagging from search results and reporting that nothing happened.

## Settled while re-deriving this, so nobody re-opens it

- **The quiet-backfill worry is closed.** The background sweep (`pane/tag-sweep.ts`) reuses the diff-emitting path, but
  `caching::apply_tags_to_listing` now emits a diff only for rows whose tags genuinely changed, so an off-screen chunk
  over untagged rows is silent. Only rows that really gained or lost a tag cost a coalesced diff, which is the behavior
  the original concern asked for.
- **Translations are covered by the project-wide rule**, not by a tags-specific task. Human review of translated strings
  is deliberately deferred for the whole app (`AGENTS.md` § Principles, point 4); the tag strings carry per-language
  glossary entries under `docs/i18n/` like every other key.
