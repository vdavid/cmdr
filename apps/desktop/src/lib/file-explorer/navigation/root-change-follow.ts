/**
 * Where a path on a connected place goes when an edit moves its root or its start
 * folder (`volume-root-changed`): the one rule every pane, tab, and remembered
 * path follows. Applying it is `../pane/volume-root-follow.ts`.
 *
 * ❗ By whole components, ❌ never a string prefix: `/srv/data-1` is a sibling of
 * `/srv/data`. The Rust twin of the containment test is
 * `cmdr_fs::volume::remote_paths::RemoteRoot::to_remote_path`.
 */

import type { VolumeRootChanged } from '$lib/ipc/bindings'

/**
 * The path `path` becomes after `change`.
 *
 * 1. On the old root or the old landing → the new landing. That's where the
 *    place put the person, and the edit says where that is now.
 * 2. Inside the new root → unchanged. The folder is still reachable, and moving
 *    someone who went deeper would lose their place.
 * 3. Anywhere else (beside or above a narrowed root) → the new landing, since
 *    the new root refuses it.
 *
 * Idempotent, so two windows following one edit agree. A server root spelled
 * with a trailing slash (`sftp://ada@nas.local:22/`, as Rust mints a `/` root)
 * matches the same root spelled without one.
 */
export function pathAfterRootChange(path: string, change: VolumeRootChanged): string {
  const here = withoutTrailingSlash(path)
  if (here === withoutTrailingSlash(change.oldRoot) || here === withoutTrailingSlash(change.oldLanding)) {
    return change.newLanding
  }
  if (isAtOrUnder(withoutTrailingSlash(change.newRoot), here)) return path
  return change.newLanding
}

function isAtOrUnder(root: string, path: string): boolean {
  return path === root || path.startsWith(`${root}/`)
}

function withoutTrailingSlash(path: string): string {
  return path.replace(/\/+$/, '')
}
