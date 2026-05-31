# Concurrent git subscribe leaks watcher subscriptions over a long session

**Severity:** high **Lens:** G — Resource hygiene **Confidence:** medium

## Location

`apps/desktop/src/lib/file-explorer/git/git-store.svelte.ts:53-67`
`apps/desktop/src/lib/file-explorer/pane/FilePane.svelte:350-383` (caller)
`apps/desktop/src/lib/file-explorer/pane/FilePane.svelte:2389-2394` (fire-and-forget `$effect`)

## What

`subscribeToRepo(repoRoot)` is `async` with two `await`s (`ensureListener()`, then `commands.subscribeGitState(...)`)
BEFORE it writes the new entry into the `repos` map. It's invoked from a fire-and-forget `void syncGitState(path)`
inside a `$effect` that runs on every `currentPath` change, with no generation guard and no serialization. Two
invocations against the same `repoRoot` that interleave across those `await`s both see
`repos.get(repoRoot) === undefined`, both call `commands.subscribeGitState` (backend `refcount` bumped to 2), and both
run `repos.set(..., { refcount: 1 })` — so the FE map ends at refcount 1 while the backend `GitWatcherRegistry` is at 2.

## Why it matters

`unsubscribeFromRepo` only calls `commands.unsubscribeGitState` when the FE `refcount` hits 0, and the backend
`GitWatcherRegistry::unsubscribe` only drops the `notify-debouncer-full` debouncer (and the cached gix `RepoHandle` +
the full-repo `list_status` snapshot) when its own `refcount` hits 0. With the backend stuck one ahead, the debouncer
never tears down: an OS file watcher on `<repo>/.git/refs/` (recursive), `index`, `HEAD`, `logs/HEAD`, plus the pinned
`RepoCache` handle and the full-repo status snapshot, all leak for the rest of the session. A user who navigates quickly
in and out of git repos all day (holding Backspace to walk up, fast back/forward, switching between many project
folders) accumulates one orphaned `.git` watcher + repo handle per race, never reclaimed until app quit. On a multi-day
session across dozens of repos this is unbounded growth of FSEvents registrations and pinned git state.

## Evidence

```ts
// git-store.svelte.ts
export async function subscribeToRepo(repoRoot: string): Promise<RepoInfo> {
  await ensureListener() // <-- await #1

  const existing = repos.get(repoRoot)
  if (existing) {
    existing.refcount += 1
    return existing.info
  }

  const res = await commands.subscribeGitState(repoRoot) // <-- await #2; backend refcount++ here
  if (res.status === 'error') throwIpcError(res.error)
  const info = res.data
  repos.set(repoRoot, { refcount: 1, info }) // <-- two racers both land here with refcount: 1
  return info
}
```

```ts
// FilePane.svelte — fire-and-forget, no await, no generation guard
$effect(() => {
  const path = currentPath
  void showRepoChip
  void showGitStatusColumn
  void syncGitState(path)
})
```

```rust
// file_system/git/watcher.rs — backend bumps refcount unconditionally on every subscribe
sub.refcount = sub.refcount.saturating_add(1);   // existing repo path
// ... first-subscriber path inserts with refcount: 1 and the live debouncer
if sub.refcount == 0 { inner.remove(&canonical); }  // only torn down at exactly 0
```

## Suggested fix

Make `subscribeToRepo` reserve the map slot synchronously before the first `await`, so concurrent callers coalesce onto
one in-flight subscription. Store a `Promise<RepoInfo>` in the map entry the moment a new subscribe starts: the second
caller finds the pending entry, bumps `refcount`, and awaits the same promise instead of issuing a second
`commands.subscribeGitState`. (Pattern: an `inflight: Map<string, Promise<RepoInfo>>` keyed by repoRoot, or a
`refcount` + `pending` field on the existing entry.) Alternatively/additionally, guard `syncGitState` with a per-pane
generation counter so a superseded navigation's subscribe is discarded — but the store-level coalescing is the durable
fix because two different panes can also race the same repoRoot.

## Notes

The single-pane single-nav happy path is correct; this only bites under overlapping async navigations, which is exactly
what a long, fast-moving session produces. `git-store.test.ts` covers refcounted subscribe/unsubscribe but (per the test
list in the git CLAUDE.md) does not appear to cover the concurrent-subscribe interleaving, so the gap is untested. The
backend side is symmetric and correct on its own; the imbalance originates entirely on the FE.
