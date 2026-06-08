// Restricted-paths event listener. Typed `on*` wrapper over the `tauri-specta`
// `events.restrictedPathsChanged` helper. Carries the full sorted set of paths
// macOS TCC currently blocks Cmdr from reading.

import { type UnlistenFn } from '@tauri-apps/api/event'
import { events, type RestrictedPathsChangedPayload } from '$lib/ipc/bindings'

/** The TCC-restricted path set changed; `paths` is the full sorted set. */
export function onRestrictedPathsChanged(
  handler: (payload: RestrictedPathsChangedPayload) => void,
): Promise<UnlistenFn> {
  return events.restrictedPathsChanged.listen((event) => {
    handler(event.payload)
  })
}
