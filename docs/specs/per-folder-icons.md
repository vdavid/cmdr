# Per-folder icons

**Status:** proposed · **Scope:** macOS first (Linux falls back to the existing XDG theme path) · **Owner:** TBD

Show real, distinctive folder icons in the file list — special system folders (Downloads, Applications, …), app/package
bundles (`.app`, `.bundle`), mounted volumes, and folders the user gave a custom icon in Finder — instead of the single
generic folder glyph every directory shows today. Additive polish in service of the top-5 "OS-custom everything /
delightful UX" principle. It must never block, slow, or destabilise the listing.

## Current state (what exists)

- **Every directory renders one shared `dir` icon.** `file_system/listing/metadata.rs::get_icon_id` assigns `dir` /
  `symlink-dir` to directories and `ext:{x}` / `file` / `symlink*` to files. The registry pattern keeps icon traffic
  bounded: a file entry carries only an `iconId`, and `icons::get_icons` fetches each unique id once from a **sample**
  path (home dir for `dir`, `/etc/hosts` for `file`, a temp file per extension) — never a real user file.
- **FDA gating already exists.** `commands/icons.rs::get_icons` and `refresh_directory_icons` both early-return an empty
  map while `crate::fda_gate::is_fda_pending_runtime()` is true, behind a 2 s `blocking_with_timeout`. Icon fetching
  touches NSWorkspace / LaunchServices, which trigger TCC popups — these must never stack on the onboarding FDA modal.
- **An unwired per-path path exists.** `icons::refresh_icons_for_directory` → `fetch_icon_for_path` (NSWorkspace
  `get_file_icon`) keyed `path:{dir}`, plus a frontend `icon-cache.ts::refreshDirectoryIcons`. **No component calls
  it.**
- **Caches are in-memory only**: Rust `ICON_CACHE` (`RwLock<HashMap<id, dataUrl>>`) + FE `memoryCache` + localStorage.
  No on-disk backend cache, no invalidation beyond a wholesale clear on theme/accent change.

### Three real flaws in the unwired code (do not ship as-is)

1. **Fetches a per-path icon for _every_ folder** — wasteful, and the source of the unbounded `path:` cache, even though
   ~99% of folders just want the generic `dir` icon they already have.
2. **Uses `rayon` (`par_iter`) for NSWorkspace calls.** The codebase explicitly forbids this (`sync_status.rs`,
   `open_with.rs` use dedicated 8 MB-stack threads) because icon/NSWorkspace calls descend into `fileproviderd` XPC for
   cloud folders and overflow rayon's 2 MB worker stack. The registry path is safe (sample paths don't descend); the
   per-path path fetches **real user folders** (which can be iCloud/Dropbox) → **latent stack-overflow crash on cloud
   folders, invisible to CI.**
3. **No invalidation/persistence** — folder icons rarely change (so they _should_ persist), but only with a staleness
   token, else a user re-iconing a folder never sees the update.

## Design

Key reframe: **don't fetch a per-folder icon for all folders — only for the few that actually deviate, and route the
finite special cases through _bounded shared keys_.** Three tiers by icon identity:

### Tier A — plain folders (the ~99%)

Keep the shared `dir` / `symlink-dir` icon. No new work; this is the fallback everything degrades to.

### Tier B — finite special _system_ folders

Downloads, Desktop, Documents, Applications, Movies, Music, Pictures, Public, the home folder, and the Trash. Detected
**by well-known path** in `get_icon_id` (cheap, no NSWorkspace, no TCC) and assigned a **bounded shared key**
`special:{name}` (e.g. `special:downloads`). `get_icons` fetches each `special:*` once from the folder's real path
(FDA-gated, on the 8 MB thread) and caches it. The set is finite and stable, so these keys may persist to localStorage.

### Tier C — genuinely per-path icons

App/package bundles (`.app`, `.bundle`, `.framework`, …), mounted volumes, and user-custom-icon folders. These are
unbounded by nature, so the expensive NSWorkspace fetch is **gated to folders that actually deviate**, detected cheaply
during listing:

- **packages** → by extension (trivial; a directory whose name ends in a known package extension);
- **volumes** → already known to the volumes layer (it uses `icons::get_icon_for_path`);
- **custom icons** → the `kHasCustomIcon` flag in the folder's `com.apple.FinderInfo` xattr (a cheap `getxattr`, no
  NSWorkspace, no TCC popup) — so the expensive fetch happens only for the rare folder that truly has a custom icon.

Tier-C entries use a `path:{dir}` key (or `pkg:{dir}` — same lifecycle) and are bounded by an LRU cap + visible-range
fetching (below), with the custom-icon flag / package extension keeping the candidate set tiny in practice.

## Mechanics (apply to Tiers B + C)

- **FDA-gated** — keep routing through `commands/icons.rs`'s `is_fda_pending_runtime()` gate. The FE re-requests after
  the gate clears (existing behaviour). Never call NSWorkspace while FDA is pending.
- **Threading** — move the per-path / per-special NSWorkspace fetch **off rayon onto the dedicated 8 MB-stack OS-thread
  pattern** (mirror `file_system/open_with.rs` / `sync_status.rs`). Correctness, not hygiene: real folders can be cloud
  folders that descend into `fileproviderd`. Linux keeps using the XDG theme lookup (no NSWorkspace).
- **Bounded + persistent cache**
  - Rust: keep `ICON_CACHE` hot, but bound the `path:` / `pkg:` keys with an **LRU cap** (a few hundred) as a backstop.
    Add an optional **on-disk cache** under the resolved data dir keyed by `path + icon-resource-mtime` (or the folder's
    own mtime) so icons survive restarts _and_ invalidate when the user re-icons a folder. `special:*` and `dir`/`ext:*`
    keys stay uncapped (inherently bounded).
  - FE (`icon-cache.ts`): LRU-cap the `path:`/`pkg:` keys in `memoryCache`; **do not** persist `path:`/`pkg:` keys to
    localStorage (only the bounded `dir` / `ext:` / `special:` keys persist). Evict `path:` entries on
    `list_directory_end` for paths no longer visible (the listing cache already has this lifecycle).
- **Triggered lazily from the virtual-scroll visible range** — for visible directory rows whose `iconId` is a `special:`
  / `path:` / `pkg:` key not yet cached, batch-request through the existing `get_icons` path, debounced.
- **Fallback / correctness** — while a richer icon is pending, FDA-gated, timed-out, or unavailable, render the generic
  `dir` icon. The feature is purely additive; the listing must never block or degrade on it.

## Data flow (target)

```
listing assigns a richer iconId
   dir / symlink-dir                      (Tier A, plain)
   special:{name}                         (Tier B, by well-known path)
   pkg:{dir} / path:{dir}                 (Tier C, by package-ext / kHasCustomIcon xattr / volume)
        ↓
FE prefetches the unique non-cached iconIds for VISIBLE rows (debounced, batched)
        ↓
backend get_icons: FDA-gated → 8 MB dedicated thread → bounded LRU + on-disk persistent cache (mtime-keyed)
        ↓
FE renders; falls back to `dir` while pending / gated / failed
```

## Phasing

- **Phase 0 — foundation / de-risk (ships independently; also closes the round-4 audit finding).** Bound the existing
  `path:` cache (Rust LRU + FE `memoryCache` LRU, stop persisting `path:` to localStorage) and move the per-path fetch
  off rayon onto the 8 MB-thread pattern — **without** wiring the feature. Removes the latent cloud-folder crash and the
  unbounded-growth finding. Safe to ship.
- **Phase 1 — Tier B (special system folders).** `get_icon_id` special-path detection + `special:*` fetch in
  `get_icons`. Most of the visible delight, bounded cache.
- **Phase 2 — Tier C (packages, volumes, custom-icon folders + persistence + FE visible-range trigger).** The largest:
  custom-icon xattr detection during listing, package-extension detection, the visible-range fetch wiring, the
  persistent on-disk cache + mtime invalidation, LRU eviction.

## Testing

**Unit-testable (TDD these):**

- `get_icon_id` tier routing: plain dir → `dir`; well-known path → `special:{name}`; package extension → `pkg:`;
  custom-icon xattr present → `path:` (use a tmp dir with the flag set/unset).
- The custom-icon-flag check (`kHasCustomIcon` in `com.apple.FinderInfo`) as a pure helper over a path.
- LRU cap + eviction (Rust and FE): insert N+k, assert oldest evicted, assert size bound; `list_directory_end` eviction.
- On-disk cache round-trip + mtime invalidation: write, read-back, bump mtime token → miss.
- FE: `path:`/`pkg:` keys not written to localStorage; visible-range prefetch requests only uncached ids.

**NOT unit-testable — needs manual verification on a real Mac (flag explicitly in each phase's report):**

- The actual NSWorkspace fetch result and visuals (retina sizing, light/dark, which folders look right).
- TCC popup behaviour and the FDA-gate interaction with real special / custom / cloud folders.
- The `fileproviderd` cloud-folder threading (the stack-overflow only reproduces on real iCloud/Dropbox folders).

## Non-goals

- Per-_file_ custom icons (only directories here; files keep the `ext:` registry).
- Animated / live folder previews.
- Changing the registry pattern for the bounded `dir` / `ext:` / `file` keys.

## Open questions (decide during review)

- On-disk cache: a small sidecar DB vs a flat keyed-file dir under the data dir? (Lean: simplest that invalidates by
  mtime.)
- `pkg:` vs `path:` key namespace for packages — merge or keep distinct for eviction tuning?
- Should Tier B include the Trash and network mounts, or start with the local special set?
