# Editable favorites plan

User-editable favorites, replacing the hardcoded four. From GitHub issue #27 (Sebastian Gross): "Searched everywhere but
I can't figure out how to change the favorite list to include my actual favorite folders."

## Decisions (settled with David, don't relitigate)

1. **One editable "Favorites" section** in the volume switcher (not two). The section is fully user-owned: add, remove,
   rename, reorder. No separate "Bookmarks" group.
2. **Seeded once with the current four** (`/Applications`, `~/Desktop`, `~/Documents`, `~/Downloads`) as the initial
   state. After that the store is the source of truth.
3. **Clearable.** The user can remove every favorite, including the defaults. When the list is empty, the switcher shows
   a single **disabled placeholder item**: "(Your favorites will show here)".
4. **Seed-once via file presence.** `favorites.json` existing = already initialized. First launch (file absent) writes
   the four defaults. Every launch after uses the file verbatim and NEVER re-injects defaults. An emptied list stays
   empty. Existing beta users (data dir but no `favorites.json`) get the four seeded on first launch post-update, no
   regression.
5. **Storage: a dedicated `favorites.json`** in the data dir via its own backend module (mirrors the `go_to_path/`
   recents store), not `settings.json`. Ordered, growing, unit-testable in isolation.
6. **Local filesystem paths only** for now. No network/MTP favorites yet (mount-state complexity, deferred).
7. **Reorder via drag** inside the switcher's Favorites section (in scope, not deferred).
8. **"Add to favorites" surfaces:** a command (palette + menu + shortcut) acting on the focused pane's current dir, AND
   a context-menu item on folder rows and on the `..` parent row (favorites the parent dir).

## Naming

UI term is "Favorites" (already in use). Internals use `favorites` / `favorite` to match (AGENTS.md principle 9).
Per-item user action ids follow the existing `file.*` / command-registry vocabulary, e.g. `favorites.add`.

## Current state (what exists today)

- `get_favorites()` (`src-tauri/src/volumes/mod.rs`, and the Linux twin in `volumes_linux/mod.rs`) returns a hardcoded
  `Vec<LocationInfo>` of the four, `category: LocationCategory::Favorite`, with FDA-pending skip logic for the protected
  paths.
- `list_locations()` merges favorites + main volume + attached volumes + cloud + MTP, pushed via the `volumes-changed`
  event, the `list_volumes` IPC, and the MCP `cmdr://state` resource (all three go through the same enrichment path).
- Frontend `stores/volume-store.svelte.ts` holds the list; `file-explorer/navigation/volume-grouping.ts` groups by
  `categoryOrder` (favorite, main_volume, network, ...). The switcher is `VolumeBreadcrumb.svelte`.
- `DualPaneExplorer.selectVolumeByIndex` navigates a favorite: resolve its containing volume, then
  `navigateIntent({ to: { volumeId, path }, source: 'user' })`. There is NO write path anywhere today.

## Phase 1 - Backend (establishes the IPC contract)

New module `src-tauri/src/favorites/`:

- Store: load/save `favorites.json` (respect `CMDR_DATA_DIR`, same resolution as the secret store / install ids). Shape:
  an ordered list of `{ id, path, name }` where `name` is the display label (defaults to the path's file name; user can
  override later via rename). `id` is a stable random id minted on add (don't derive from path: paths can repeat across
  rename). Seed-once: if the file is absent, write the four defaults (compute `~/Desktop` etc. from `dirs::home_dir()`);
  if present, read verbatim.
- Ops (pure-ish core, testable without Tauri): `list`, `add(path, name?)`, `remove(id)`, `rename(id, name)`,
  `reorder(ordered_ids)`. `add` dedups by normalized path (adding an existing path is a no-op or moves-to-existing, pick
  one and test it). Each mutation persists.
- Rewire `get_favorites()` to read the store and map to `LocationInfo`. Keep the FDA-pending protected-path skip for the
  default protected paths; for user-added paths, follow the same safe pattern (don't stat protected paths while FDA
  pending). Add a way for the FE to know a favorite is user-removable - simplest is that ALL favorites are now user
  favorites (since defaults are seeded into the same store), so a boolean isn't strictly needed, but confirm the FE can
  tell favorites apart from volumes (it already does via `category`).
- IPC commands in `src-tauri/src/commands/` (thin pass-throughs, async + `blocking_with_timeout` since add/validate
  touches the FS): `add_favorite`, `remove_favorite`, `rename_favorite`, `reorder_favorites`. After any mutation,
  re-emit `volumes-changed` (reuse the existing broadcast path in `volume_broadcast.rs`) so both panes' switchers update
  live (subscribe-don't-poll). `list` can ride the existing `list_volumes`.
- Linux twin: update `volumes_linux/mod.rs::get_favorites` the same way (share the store module; only the default seed
  paths and any platform specifics differ).
- Regenerate bindings: `pnpm bindings:regen` so `lib/ipc/bindings.ts` carries the new commands/types.
- Tests: store CRUD, seed-once-on-absence, no-reseed-when-present, empty-stays-empty, dedup, reorder, rename,
  persistence round-trip. Follow existing module test patterns (e.g. `go_to_path/`).
- Update colocated docs: a new `src-tauri/src/favorites/CLAUDE.md` (+ `DETAILS.md` if depth warrants) and the
  architecture map line.

Deliverable: backend compiles, `pnpm check rust` green, bindings regenerated and committed.

## Phase 2 - Frontend (consumes the contract from Phase 1)

- Typed wrappers: call the new `commands.*` (regenerated). No raw `invoke` outside `lib/ipc/`.
- "Add to favorites" command: add the id to `COMMAND_IDS`, an entry in `command-registry.ts`, a handler in
  `routes/(main)/command-handlers/`, and a default shortcut. It favorites the focused pane's current directory. Palette
  - native menu entry.
- Context menu: add "Add to favorites" to the folder-row context menu and the `..` row (favorites the parent dir). Find
  the existing context-menu infra and follow it.
- Switcher (`VolumeBreadcrumb` + `volume-grouping`): within the Favorites section, add per-item "Remove" and "Rename"
  affordances (context menu on the item), and drag-to-reorder (calls `reorder_favorites`). When the favorites list is
  empty, render a single disabled placeholder row "(Your favorites will show here)".
- Live update: the switcher already re-renders from `volume-store` on `volumes-changed`; confirm mutations reflect
  without a manual refresh.
- Tests: command registration + handler, context-menu wiring, the empty-state placeholder, reorder calling the right IPC
  with the right order, rename. Follow existing patterns. a11y test for any new interactive UI.
- Update colocated docs (`file-explorer/navigation/`, `stores/`, etc.).

Deliverable: full `pnpm check` green from the worktree root; ideally a live MCP smoke (add a folder, see it appear,
reorder, remove, empty placeholder).

## Guardrails

- Style guide: sentence case, active voice, no em-dashes, CSS design tokens, Oxford comma. Placeholder copy exactly:
  "(Your favorites will show here)" unless David revises.
- `blocking_with_timeout` for any FS-touching command; capability files updated if a new Tauri API is used.
- Don't break the MCP `cmdr://state` favorites shape or the `list_volumes` consumers.
- Run `pnpm check` at the right cadence; read full output (no truncation).
- No push. Land via FF-merge to local main (David's flow).

## Phase 1 result - IPC contract

Phase 1 (backend) shipped. Phase 2 (frontend) builds against this. Backend module: `src-tauri/src/favorites/` (see its
`CLAUDE.md` / `DETAILS.md`).

### Typed commands (call via the generated `commands.*` wrappers, regenerated into `lib/ipc/bindings.ts`)

All four are async, run the store write under a 5s `blocking_result_with_timeout`, then re-emit `volumes-changed`. They
return `Result<(), IpcError>` (TS: `Promise<Result<null, IpcError>>`):

- `commands.addFavorite(path: string, name: string | null)` -> backend `add_favorite(path, name?)`. `name` null/omitted
  defaults the label to the path's file name. Dedups by normalized path: a re-add moves the existing entry to the end
  and keeps its id (applying the name override if given).
- `commands.removeFavorite(id: string)` -> `remove_favorite(id)`. No-op if the id is gone.
- `commands.renameFavorite(id: string, name: string)` -> `rename_favorite(id, name)`. No-op if gone.
- `commands.reorderFavorites(orderedIds: string[])` -> `reorder_favorites(ordered_ids)`. Unknown ids ignored; favorites
  missing from the list are appended in current order (never drops an entry).

There is NO `list_favorites` command. Favorites ride the existing `list_volumes` IPC and the `volumes-changed` event,
surfaced as `LocationInfo` entries with `category: "favorite"` and `id: "fav-<favoriteId>"`. So the FE reads favorites
from `volume-store` like before; only the mutations are new.

### `favorites.json` shape (data dir, alongside `settings.json`)

```json
{
  "_schemaVersion": 1,
  "favorites": [
    { "id": "9f1c4e2a-…", "path": "/Applications", "name": "Applications" },
    { "id": "a83e1b77-…", "path": "/Users/me/Desktop", "name": "Desktop" }
  ]
}
```

- `id`: random UUID minted on add, stable across renames (NOT derived from path). The switcher's `LocationInfo.id` is
  `"fav-" + id`, so a Phase 2 "remove"/"rename" action strips the `fav-` prefix to recover the favorite id.
- Array order is the display order.
- Seed-once via file presence: absent file seeds the platform defaults (macOS: `/Applications`, `~/Desktop`,
  `~/Documents`, `~/Downloads`); a present file (even empty) is read verbatim and never re-seeded. So an empty Favorites
  section is a real state the FE must render (the "(Your favorites will show here)" placeholder).
